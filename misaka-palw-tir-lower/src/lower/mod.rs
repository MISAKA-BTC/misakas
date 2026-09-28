//! **HL → PALW-TIR expansion (RFC-0002 Gate 2a).**
//!
//! [`lower`] turns an [`HlProgram`] into a [`TirProgramV1`] plus one *fill* per TIR param: the
//! program is the structure (blocks, schedule, states, commit points) and depends on the
//! architecture only; the fills compute the integer params (weight codes, `(m, s, z)` narrowings,
//! activation tables, RoPE tables) for each layer from the float checkpoint and the calibration
//! statistics ([`fill::materialise`]). A program therefore never changes with a recalibration —
//! only its artifact does.
//!
//! # The number formats (A16, W8)
//!
//! | value | dtype | meaning |
//! | --- | --- | --- |
//! | residual stream (the carry) | `i32` | one scale `S_r` for the whole program, calibrated with `headroom_resid` |
//! | activations at matmul inputs and op boundaries | `i16` codes `±32767` | a static scale per site and layer |
//! | weights | `i8` codes `±127` | one scale per output channel (row) |
//! | norm unit rows, attention logits and probabilities | `i32` Q24 | exact fixed point, `ONE = 2^24` |
//! | logits | `i32` | a static scale for the post occurrence |
//!
//! Every change of scale is the A16 narrowing `N(x; m, s, z)` of the library
//! (`narrow_a16`: `clamp(sat64(HAFZ(x·m / 2^s)) + z)`), with `m` (`i64`), `s` (`i8`) and `z`
//! (`i64`) as three separately typed per-channel params named `<site>.m`, `<site>.s`, `<site>.z`.
//!
//! # Lowering, op by op
//!
//! * **Embedding**: `Gather` of the `i8` row, and a per-TOKEN lift `N(row; m[t], s[t])` gathered
//!   by the same token (a per-row weight scale is a per-token activation scale).
//! * **Linear**: `MatMul(W:i8[out,in], x:i16[in,1]) → i64`, then `N` per output channel; a bias is
//!   the narrowing's `z` at the output scale.
//! * **Add**: the residual add is `Clamp_i32(Add_i64(x, y))` at `S_r`; the planning pass below makes
//!   the producing projection narrow straight to `S_r`, so no extra rounding sits on the stream.
//! * **Scale by a constant** (Gemma's `√D`, Granite's multipliers, Cohere's logit scale): no node —
//!   the constant is folded into the value's scale and so into the next narrowing.
//! * **RMSNorm / LayerNorm**: the wide RMS template (`rms_norm_wide_q36`, exponent taken out before
//!   `IntRsqrt`); LayerNorm first centres exactly (`n·x − Σx`, corpus §6.2.7). The gain (and
//!   `1 + w`) and the bias are the per-channel `(m, z)` of the narrowing out of Q24.
//! * **RoPE**: angles from two pinned tables by angle addition (spec 04b §11.3) — or, for the
//!   position-dependent rope types (dynamic NTK, LongRoPE), one row per position; half-split pairs
//!   rotate by slices, interleaved pairs by `rope_pairs`. The attention factor is folded into the
//!   value scale (the tables stay in `[−ONE, ONE]`).
//! * **Attention**: `HistAppend` of the K and V code rows (committed), scores
//!   `[kv, G, d] × [kv, d, H] → i64` narrowed straight to Q24 logits, the two-pass softmax
//!   (`softmax_shifted`), `P·V` against the Q24 probabilities, then `N` to codes.
//! * **Activations**: `Table(x; T)` — a 65,536-entry `i16` table per site and layer, gathered by
//!   `code + 32768` (corpus §2.2: a transcendental at registration is data). Every activation the
//!   corpus names (SiLU, GELU erf/tanh, quick-GELU, ReLU², sigmoid, tanh, softplus) is one table;
//!   the table IS the exactly rounded float function on the code grid.
//! * **Elementwise product** (the GLU): `Mul → i32`, then `N`.
//!
//! Commit points: every carry-out, the logits, the K/V rows (normal form), and every `i16` code
//! row at a matmul boundary (norm outputs, projection outputs, the attention context, the GLU
//! hidden) plus the mid-layer residual — the pattern of corpus §2.4. Court cone sizing is
//! `tir_admit_v1`'s job and is not attempted here.

pub mod fill;

use crate::error::{LowerError, Result};
use crate::hl::{self, BlockRole, HlProgram, Op, StateKind};
use crate::rope::RopeStyle;
use crate::spec::{Act, Gain, NormKind};
use misaka_palw_tir as tir;
use std::collections::BTreeMap;
use std::sync::Arc;
use tir::builder::{BlockBuilder, ProgramBuilder};
use tir::program::{INPUT_POS, INPUT_TOKEN};
use tir::{DType, Dim, Rounding, TensorType, TirProgramV1};

pub use fill::{FillCtx, IntData, IntParams, IntTensor, Materialised, materialise};

/// Lowering options.
#[derive(Clone, Debug)]
pub struct LowerOpts {
    /// `2^18` (the default) or `2^21`.
    pub history_bound: u32,
}

impl Default for LowerOpts {
    fn default() -> Self {
        Self { history_bound: tir::program::HISTORY_BOUND_V1_SMALL }
    }
}

/// What a scale is made of; resolved per occurrence at materialisation ([`FillCtx::scale`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    /// The residual stream: ONE `i32` scale for every block and layer.
    Resid,
    /// A site's calibrated scale in this occurrence: the max absmax over `names`, for `i16` codes
    /// (`wide = false`) or `i32` values (`wide = true`). With `split = k > 0` the scale is PER
    /// CHANNEL: the `k` channels with the largest calibrated absmax (the outliers, lowest index on
    /// ties) each get their own scale, every other channel shares the scale of the largest of them
    /// ([`FillCtx::scale_vec`], [`FillCtx::outliers`]). Only a value consumed by projections alone
    /// is split: the projection routes the outlier channels through their own high-precision
    /// columns (see `lower_linear`).
    Site { names: Vec<String>, wide: bool, split: usize },
    /// Q24 fixed point (`2^−24`).
    Q24,
    /// A scale known when lowering (a value with a proven range, e.g. `clamp(up, −l, l) + 1`).
    Fixed(f64),
}

/// The float value of one integer unit: `resolve(base) · factor`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleKey {
    pub base: Base,
    pub factor: f64,
}

impl ScaleKey {
    pub fn resid() -> Self {
        Self { base: Base::Resid, factor: 1.0 }
    }
    pub fn q24() -> Self {
        Self { base: Base::Q24, factor: 1.0 }
    }
    pub fn site(names: Vec<String>, wide: bool) -> Self {
        Self { base: Base::Site { names, wide, split: 0 }, factor: 1.0 }
    }
    /// `i16` codes with `split` outlier channels scaled on their own.
    pub fn site_split(names: Vec<String>, split: usize) -> Self {
        Self { base: Base::Site { names, wide: false, split }, factor: 1.0 }
    }
    /// The number of channels with their own scale (0: one scale for the tensor).
    pub fn split(&self) -> usize {
        match &self.base {
            Base::Site { split, .. } => *split,
            _ => 0,
        }
    }
    pub fn times(&self, c: f64) -> Self {
        Self { base: self.base.clone(), factor: self.factor * c }
    }
    fn same(&self, o: &ScaleKey) -> bool {
        self.base == o.base && (self.factor - o.factor).abs() <= 1e-12 * self.factor.abs().max(o.factor.abs())
    }
}

/// Computes one TIR param for one occurrence.
pub type FillFn = Arc<dyn Fn(&FillCtx<'_>) -> Result<IntTensor> + Send + Sync>;

/// A lowered program: the structure, and how to fill each of its params.
pub struct Lowered {
    pub program: TirProgramV1,
    /// One per `program.params`, same order.
    pub fills: Vec<FillFn>,
    /// Statistics keys (with occurrence prefix) of every value carried at the residual scale, and
    /// the factor its scale carries: `S_r` is sized on them.
    pub resid_sites: Vec<(String, f64)>,
    /// The scale of the logits node, resolved in the `post` occurrence.
    pub logits_key: ScaleKey,
    /// TIR block index of each HL block.
    pub block_map: Vec<u8>,
    /// `(TIR block, node)` → the HL site whose float value the node holds, its scale and width:
    /// what a per-site comparison against the float reference decodes committed values with.
    pub site_nodes: BTreeMap<(u8, u16), (String, ScaleKey, usize)>,
}

impl Lowered {
    /// A one-screen description of the program.
    pub fn summary(&self) -> String {
        program_summary(&self.program)
    }
}

/// Blocks, schedule, params, states and commit points of a TIR program.
pub fn program_summary(p: &TirProgramV1) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let bytes = p.encode().len();
    let nodes: usize = p.blocks.iter().map(|b| b.nodes.len()).sum();
    let _ = writeln!(
        s,
        "TIR program v{}: {} blocks, {} layers, {} nodes, {} params, {} consts ({} B), {} states, {} bytes encoded",
        p.version,
        p.blocks.len(),
        p.schedule.layers.len(),
        nodes,
        p.params.len(),
        p.consts.len(),
        p.consts.iter().map(|c| c.data.len()).sum::<usize>(),
        p.states.len(),
        bytes
    );
    let _ = writeln!(s, "  token_bound {}, history_bound {}", p.token_bound, p.history_bound);
    for (i, b) in p.blocks.iter().enumerate() {
        let commits = b.nodes.iter().filter(|n| n.commit).count();
        let role = if i as u8 == p.schedule.pre {
            "pre".to_string()
        } else if i as u8 == p.schedule.post {
            "post".to_string()
        } else {
            format!("layer ×{}", p.schedule.layers.iter().filter(|k| **k == i as u8).count())
        };
        let mut prims: BTreeMap<&str, usize> = BTreeMap::new();
        for n in &b.nodes {
            *prims.entry(n.prim.name()).or_default() += 1;
        }
        let top: Vec<String> = prims.iter().map(|(k, v)| format!("{k}×{v}")).collect();
        let _ = writeln!(
            s,
            "  block {i} `{}` ({role}): {} nodes, {commits} commit points, carries {}→{}; {}",
            b.name,
            b.nodes.len(),
            b.carry_in.len(),
            b.carry_out.len(),
            top.join(" ")
        );
    }
    let per_layer = p.params.iter().filter(|d| d.per_layer).count();
    let _ = writeln!(s, "  params: {} ({} per-layer)", p.params.len(), per_layer);
    for st in &p.states {
        let _ = writeln!(
            s,
            "    state `{}` {:?} {} {:?}{}",
            st.name,
            st.kind,
            st.dtype.name(),
            st.shape,
            if st.per_layer { " per-layer" } else { "" }
        );
    }
    s
}

/// Lower an HL program.
pub fn lower(hl: &HlProgram, opts: &LowerOpts) -> Result<Lowered> {
    let hb = opts.history_bound;
    if hb != tir::program::HISTORY_BOUND_V1_SMALL && hb != tir::program::HISTORY_BOUND_V1_HELD {
        return Err(LowerError::bad(format!("history_bound {hb} is neither 2^18 nor 2^21")));
    }
    if hl.carries.len() != 1 {
        return Err(LowerError::not_lowerable("a program with more than one carry"));
    }
    let token_bound = u32::try_from(hl.vocab).map_err(|_| LowerError::not_lowerable("vocabulary beyond u32"))?;
    let mut pb = ProgramBuilder::new(token_bound, hb);
    // A param read twice in one occurrence (a tied embedding: the pre gather and the post head)
    // must keep the plain codes its other reader shares. A per-layer param read by two layer KINDS
    // is never read twice in one occurrence.
    let mut uses: BTreeMap<u32, usize> = BTreeMap::new();
    let mut shared: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for blk in &hl.blocks {
        let mut here: BTreeMap<u32, usize> = BTreeMap::new();
        for n in &blk.nodes {
            for r in &n.inputs {
                if let hl::Ref::Param(p) = r {
                    *uses.entry(*p).or_default() += 1;
                    *here.entry(*p).or_default() += 1;
                }
            }
        }
        shared.extend(here.into_iter().filter(|(_, n)| *n > 1).map(|(p, _)| p));
    }
    shared.extend(uses.into_iter().filter(|(p, n)| *n > 1 && !hl.params[*p as usize].per_layer).map(|(p, _)| p));
    let mut cx = Cx {
        hl,
        fills: Vec::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared,
        split_max_readers: usize::MAX,
    };
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    // HL order is pre, layer kinds, post; TIR keeps it.
    let mut order: Vec<usize> = vec![hl.pre];
    for k in &hl.schedule {
        if !order.contains(&(*k as usize)) {
            order.push(*k as usize);
        }
    }
    order.push(hl.post);
    let mut logits = None;
    for &hbk in &order {
        // The builder panics on a block past NF-12's node cap. The lowering then gives up the
        // outlier splits of the most-read values first and tries again (the program stays a
        // function of the architecture); a block that does not fit even so is a refusal.
        let mut result = None;
        for readers in [usize::MAX, 3, 1, 0] {
            let snap = (
                pb.params.len(),
                pb.consts.len(),
                pb.states.len(),
                pb.blocks.len(),
                cx.fills.len(),
                cx.tstate.clone(),
                cx.resid_sites.clone(),
                cx.site_nodes.clone(),
                cx.logits_key.clone(),
            );
            cx.split_max_readers = readers;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lower_block(&mut pb, &mut cx, hbk)));
            match r {
                Ok(v) => {
                    result = Some(v?);
                    break;
                }
                Err(p) => {
                    let msg = p
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default();
                    pb.params.truncate(snap.0);
                    pb.consts.truncate(snap.1);
                    pb.states.truncate(snap.2);
                    pb.blocks.truncate(snap.3);
                    cx.fills.truncate(snap.4);
                    cx.tstate = snap.5;
                    cx.resid_sites = snap.6;
                    cx.site_nodes = snap.7;
                    cx.logits_key = snap.8;
                    if !msg.contains("exceeds") || readers == 0 {
                        return Err(LowerError::not_lowerable(format!("block `{}`: {msg}", hl.blocks[hbk].name)));
                    }
                }
            }
        }
        let (tb, lg) = result.expect("a lowering attempt returns or refuses");
        block_map[hbk] = tb;
        if lg.is_some() {
            logits = lg;
        }
    }
    let logits = logits.ok_or_else(|| LowerError::eval("internal: post block produced no logits"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], logits);
    tir::validate::validate(&program)
        .map_err(|e| LowerError::eval(format!("internal: lowered program is not in normal form: {e}")))?;
    let fills: Vec<FillFn> = cx.fills;
    if fills.len() != program.params.len() {
        return Err(LowerError::eval("internal: fills do not match params"));
    }
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no logits scale"))?;
    Ok(Lowered { program, fills, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes })
}

// ───────────────────────────── lowering state ─────────────────────────────

struct Cx<'h> {
    hl: &'h HlProgram,
    fills: Vec<FillFn>,
    /// (stats key, factor bits) → factor.
    resid_sites: BTreeMap<(String, u64), f64>,
    /// HL state → TIR state.
    tstate: BTreeMap<u32, u16>,
    history_bound: u32,
    logits_key: Option<ScaleKey>,
    site_nodes: BTreeMap<(u8, u16), (String, ScaleKey, usize)>,
    /// HL params read by more than one node (a tied embedding): a projection over them keeps the
    /// plain per-row codes the other reader shares.
    shared: std::collections::BTreeSet<u32>,
    /// Split a value's outlier channels only when at most this many projections read it: each
    /// split projection costs 9 nodes more, and a block has 512 (NF-12).
    split_max_readers: usize,
}

/// A lowered value: a TIR operand, its dtype, its scale, and the HL site whose statistics
/// describe it (for a later change of scale).
#[derive(Clone, Debug)]
struct Val {
    r: tir::Ref,
    dt: DType,
    key: ScaleKey,
    len: usize,
    site: String,
}

/// What a consumer asks a producer for.
#[derive(Clone, Debug)]
struct Want {
    dt: DType,
    key: ScaleKey,
}

/// Attention logits are `i32` in Q`LOGIT_Q` fixed point.
pub const LOGIT_Q: u32 = 14;

fn code_bounds(dt: DType) -> (i64, i64) {
    match dt {
        DType::I16 => (-32767, 32767),
        DType::I32 => (i32::MIN as i64, i32::MAX as i64),
        other => panic!("no code bounds for {}", other.name()),
    }
}

fn u32s(shape: &[usize]) -> Vec<u32> {
    shape.iter().map(|d| *d as u32).collect()
}

/// Per-block lowering state.
struct Lb {
    hb: usize,
    role: BlockRole,
    /// Statistics prefixes of the occurrences this block runs as.
    prefixes: Vec<String>,
    vals: Vec<Vec<Option<Val>>>,
    wants: Vec<Option<Want>>,
    rope_sites: Vec<Vec<String>>,
    /// Outlier channels a node's codes split off (0: one scale), see [`Base::Site`].
    split: Vec<usize>,
    windows: BTreeMap<u32, (tir::Ref, ScaleKey)>,
    angles: BTreeMap<u32, (tir::Ref, tir::Ref)>,
    requants: usize,
    /// Nodes a composite reads through a pattern and that produce no TIR node of their own.
    absorbed: Vec<bool>,
    /// For each GatedDelta node: where its decay and beta come from.
    gdn: BTreeMap<usize, GdnInputs>,
    /// Name suffix for per-layer params whose base name another block already declared.
    suffix: String,
}

fn lower_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let blk = &hl.blocks[hbk];
    let d = hl.hidden;
    let carry_sig = vec![TensorType::fixed(DType::I32, &[d as u32])];
    let prefixes: Vec<String> = match blk.role {
        BlockRole::Pre => vec!["pre.".into()],
        BlockRole::Post => vec!["post.".into()],
        BlockRole::Layer => {
            hl.schedule.iter().enumerate().filter(|(_, k)| **k as usize == hbk).map(|(l, _)| format!("L{l}.")).collect()
        }
    };
    let suffix = if blk.role == BlockRole::Layer && hl.schedule.first().map(|k| *k as usize) != Some(hbk) {
        format!("#{hbk}")
    } else {
        String::new()
    };
    let mut lb = Lb {
        hb: hbk,
        role: blk.role,
        prefixes,
        vals: Vec::with_capacity(blk.nodes.len()),
        wants: plan(hl, hbk),
        rope_sites: rope_consumer_sites(blk),
        split: split_plan(hl, blk, &cx.shared, cx.split_max_readers),
        windows: BTreeMap::new(),
        angles: BTreeMap::new(),
        requants: 0,
        absorbed: vec![false; blk.nodes.len()],
        gdn: BTreeMap::new(),
        suffix,
    };
    gdn_patterns(hl, blk, &mut lb)?;
    concat_wants(blk, &mut lb);
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&blk.name, if blk.role == BlockRole::Pre { vec![] } else { carry_sig });
    for (i, node) in blk.nodes.iter().enumerate() {
        let out = lower_node(&mut b, cx, &mut lb, i, node).map_err(|e| match e {
            LowerError::NotLowerable(m) => {
                LowerError::NotLowerable(format!("block `{}` node {i} ({}): {m}", blk.name, node.op.name()))
            }
            other => other,
        })?;
        if std::env::var_os("PALW_TIR_DEBUG").is_some() {
            let last = out.iter().flatten().filter_map(|v| if let tir::Ref::Node(n) = v.r { Some(n) } else { None }).max();
            eprintln!("  [{}] {i:>3} {:<14} {:<22} → node {last:?}", blk.name, node.op.name(), node.site.as_deref().unwrap_or(""));
        }
        if let (Some(site), Some(Some(v))) = (&node.site, out.first())
            && let tir::Ref::Node(n) = v.r
            && v.dt != DType::Idx
        {
            cx.site_nodes.entry((tb, n)).or_insert((site.clone(), v.key.clone(), v.len));
        }
        lb.vals.push(out);
    }
    match blk.role {
        BlockRole::Post => {
            let v = operand(&lb, blk.outputs[0])?;
            let key = ScaleKey::site(vec![v.site.clone()], true);
            let v = coerce(&mut b, cx, &mut lb, &v, DType::I32, &key)?;
            let v = ensure_node(&mut b, &v);
            let tir::Ref::Node(li) = v.r else { unreachable!("ensure_node") };
            b.commit(v.r);
            cx.logits_key = Some(v.key.clone());
            Ok((b.finish(&[]), Some(li)))
        }
        _ => {
            let v = operand(&lb, blk.outputs[0])?;
            let v = coerce(&mut b, cx, &mut lb, &v, DType::I32, &ScaleKey::resid())?;
            let v = ensure_node(&mut b, &v);
            note_resid(cx, &lb, &v);
            Ok((b.finish(&[v.r]), None))
        }
    }
}

/// A carry-out must be a node; a value that is still the carry-in gets an identity clamp.
fn ensure_node(b: &mut BlockBuilder<'_>, v: &Val) -> Val {
    if matches!(v.r, tir::Ref::Node(_)) {
        return v.clone();
    }
    let (lo, hi) = code_bounds(v.dt);
    Val { r: b.clamp(v.r, lo, hi, v.dt), ..v.clone() }
}

/// Record a value carried at the residual scale, so `S_r` covers it.
fn note_resid(cx: &mut Cx<'_>, lb: &Lb, v: &Val) {
    if v.key.base != Base::Resid {
        return;
    }
    for p in &lb.prefixes {
        let k = format!("{p}{}", v.site);
        cx.resid_sites.insert((k, v.key.factor.to_bits()), v.key.factor);
    }
}

/// The planning pass: which node outputs are produced directly in the representation a
/// consumer needs. Block outputs want the residual `i32` scale (post: the wide logits); an `Add`
/// passes its want to both operands, a `Scale{c}` to its input divided by `c`. Everything else
/// produces its default (`i16` codes at its own site) and consumers convert.
fn plan(hl: &HlProgram, hbk: usize) -> Vec<Option<Want>> {
    let blk = &hl.blocks[hbk];
    let mut want: Vec<Option<Want>> = vec![None; blk.nodes.len()];
    let out_want = if blk.role == BlockRole::Post {
        let site = match blk.outputs[0] {
            hl::Ref::Node(i, _) => blk.nodes[i as usize].site.clone().unwrap_or_else(|| "logits".into()),
            _ => "logits".into(),
        };
        Want { dt: DType::I32, key: ScaleKey::site(vec![site], true) }
    } else {
        Want { dt: DType::I32, key: ScaleKey::resid() }
    };
    for o in &blk.outputs {
        if let hl::Ref::Node(i, 0) = o {
            want[*i as usize] = Some(out_want.clone());
        }
    }
    for i in (0..blk.nodes.len()).rev() {
        let Some(w) = want[i].clone() else { continue };
        let n = &blk.nodes[i];
        match &n.op {
            Op::Add => {
                for r in &n.inputs {
                    if let hl::Ref::Node(j, 0) = r
                        && want[*j as usize].is_none()
                    {
                        want[*j as usize] = Some(w.clone());
                    }
                }
            }
            Op::Scale { c } => {
                if let hl::Ref::Node(j, 0) = n.inputs[0]
                    && want[j as usize].is_none()
                {
                    want[j as usize] = Some(Want { dt: w.dt, key: w.key.times(1.0 / c) });
                }
            }
            _ => {}
        }
    }
    // A router's logits come straight out of their projection in the attention logits' Q14.
    for n in &blk.nodes {
        if let Op::Route { .. } = n.op
            && let hl::Ref::Node(j, 0) = n.inputs[0]
            && want[j as usize].is_none()
        {
            want[j as usize] = Some(Want { dt: DType::I32, key: q14() });
        }
    }
    want
}

/// Q14 fixed point (`2^−14`), the logits' format.
fn q14() -> ScaleKey {
    ScaleKey::q24().times((1u64 << (24 - LOGIT_Q)) as f64)
}

/// For each node, the sites of the RoPE nodes that consume it: its codes must also hold the
/// rotated values, which share its scale.
fn rope_consumer_sites(blk: &hl::Block) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new(); blk.nodes.len()];
    for n in &blk.nodes {
        if let Op::Rope { .. } = n.op
            && let hl::Ref::Node(j, 0) = n.inputs[0]
            && let Some(s) = &n.site
        {
            out[j as usize].push(s.clone());
        }
    }
    out
}

/// Outlier channels to split off, per node: only for a value whose every consumer is the input of
/// a projection over an unshared weight, and whose producer narrows per channel (a norm, a
/// projection, an elementwise product, an attention context).
fn split_plan(hl: &HlProgram, blk: &hl::Block, shared: &std::collections::BTreeSet<u32>, max_readers: usize) -> Vec<usize> {
    let n = blk.nodes.len();
    let mut consumers: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (i, node) in blk.nodes.iter().enumerate() {
        for (k, r) in node.inputs.iter().enumerate() {
            if let hl::Ref::Node(j, _) = r {
                consumers[*j as usize].push((i, k));
            }
        }
    }
    for o in &blk.outputs {
        if let hl::Ref::Node(j, _) = o {
            consumers[*j as usize].push((usize::MAX, 0));
        }
    }
    (0..n)
        .map(|i| {
            let capable =
                matches!(blk.nodes[i].op, Op::Norm { .. } | Op::Mul | Op::Attention { .. } | Op::Linear { .. } | Op::GatedRmsNorm { .. });
            let all_linear = !consumers[i].is_empty()
                && consumers[i].iter().all(|(c, k)| {
                    *c != usize::MAX
                        && *k == 0
                        && matches!(blk.nodes[*c].op, Op::Linear { .. })
                        && matches!(blk.nodes[*c].inputs[1], hl::Ref::Param(p) if !shared.contains(&p) && hl.params[p as usize].shape.len() == 2)
                });
            let width: usize = blk.nodes[i].outs.first().map(|s| s.iter().product()).unwrap_or(0);
            if capable && all_linear && consumers[i].len() <= max_readers { split_size(width) } else { 0 }
        })
        .collect()
}

/// How many outlier channels a projection input of `n` channels splits off.
fn split_size(n: usize) -> usize {
    if n >= 16 { (n / 4).min(16) } else { 0 }
}

fn operand(lb: &Lb, r: hl::Ref) -> Result<Val> {
    match r {
        hl::Ref::Node(i, o) => lb
            .vals
            .get(i as usize)
            .and_then(|v| v.get(o as usize))
            .and_then(|v| v.clone())
            .ok_or_else(|| LowerError::eval(format!("internal: node {i}.{o} has no lowered value"))),
        hl::Ref::Carry(0) => {
            Ok(Val { r: tir::Ref::CarryIn(0), dt: DType::I32, key: ScaleKey::resid(), len: 0, site: "carry0".into() })
        }
        other => Err(LowerError::eval(format!("internal: operand {other:?} is not a value"))),
    }
}

// ───────────────────────────── params ─────────────────────────────

/// Declare (or reuse) a TIR param with its fill. Global params are shared by name across blocks;
/// a per-layer param another block already declared gets this block's suffix, because its fill
/// captured that block's scales.
#[allow(clippy::too_many_arguments)]
fn decl(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &Lb,
    name: &str,
    dt: DType,
    shape: &[usize],
    per_layer: bool,
    fill: FillFn,
) -> Result<tir::Ref> {
    let mut name = name.to_string();
    if let Some(i) = b.pb.params.iter().position(|p| p.name == name) {
        let p = &b.pb.params[i];
        if !per_layer && !p.per_layer && p.dtype == dt && p.shape == u32s(shape) {
            return Ok(tir::Ref::Param(i as u16));
        }
        name = format!("{name}{}", if lb.suffix.is_empty() { format!("#{}", lb.hb) } else { lb.suffix.clone() });
        if let Some(i) = b.pb.params.iter().position(|p| p.name == name) {
            return Ok(tir::Ref::Param(i as u16));
        }
    }
    if name.len() > tir::program::MAX_NAME_BYTES {
        return Err(LowerError::not_lowerable(format!("param name `{name}` exceeds 128 bytes")));
    }
    if shape.iter().any(|d| *d == 0 || *d > 1 << 24) {
        return Err(LowerError::not_lowerable(format!("param `{name}` {shape:?}: a dimension outside [1, 2^24]")));
    }
    let r = b.pb.param(&name, dt, &u32s(shape), per_layer);
    cx.fills.push(fill);
    Ok(r)
}

fn per_layer(lb: &Lb) -> bool {
    lb.role == BlockRole::Layer
}

/// `(m, s)` params for a per-channel ratio; `ratio(ctx)` gives `n` floats.
fn decl_ms(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &Lb,
    name: &str,
    n: usize,
    ratio: Arc<dyn Fn(&FillCtx<'_>) -> Result<Vec<f64>> + Send + Sync>,
) -> Result<(tir::Ref, tir::Ref)> {
    let pl = per_layer(lb);
    let r1 = ratio.clone();
    let m = decl(
        b,
        cx,
        lb,
        &format!("{name}.m"),
        DType::I64,
        &[n],
        pl,
        Arc::new(move |c| {
            let v = r1(c)?;
            Ok(IntTensor::i64(vec![v.len()], v.iter().map(|r| crate::quant::mul_shift(*r).0).collect()))
        }),
    )?;
    let s = decl(
        b,
        cx,
        lb,
        &format!("{name}.s"),
        DType::I8,
        &[n],
        pl,
        Arc::new(move |c| {
            let v = ratio(c)?;
            Ok(IntTensor::i8(vec![v.len()], v.iter().map(|r| crate::quant::mul_shift(*r).1).collect()))
        }),
    )?;
    Ok((m, s))
}

/// The narrowing `N(x; m, 2^s, z)` into `(dt, lo..hi)`.
fn narrow(b: &mut BlockBuilder<'_>, x: tir::Ref, m: tir::Ref, s: tir::Ref, z: Option<tir::Ref>, dt: DType) -> tir::Ref {
    let (lo, hi) = code_bounds(dt);
    let p2 = b.pow2_of(s);
    match z {
        Some(z) => b.narrow_a16(x, m, p2, z, lo, hi, dt),
        // Without a zero term the template's `Clamp_i64 → Add 0 → Clamp[lo,hi]` is exactly
        // `Clamp[lo,hi]` (`[lo, hi] ⊆ i64`): two nodes fewer, every value identical.
        None => {
            let p = b.mul(x, m, DType::I128);
            let q = b.div(p, p2, Rounding::HalfAwayFromZero, DType::I128);
            b.clamp(q, lo, hi, dt)
        }
    }
}

/// Re-express `v` in `(dt, key)`: one uniform narrowing (the identity when it already is).
fn coerce(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: &Val, dt: DType, key: &ScaleKey) -> Result<Val> {
    if v.dt == dt && v.key.same(key) {
        return Ok(v.clone());
    }
    lb.requants += 1;
    let name = format!("{}.rq{}", v.site, lb.requants);
    let (from, to) = (v.key.clone(), key.clone());
    let per_channel = from.split() > 0 || to.split() > 0;
    let n = if per_channel { v.len } else { 1 };
    if per_channel && n == 0 {
        return Err(LowerError::eval("internal: a per-channel change of scale of a value of unknown width"));
    }
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &name,
        n,
        Arc::new(move |c| {
            let (f, t) = (c.scale_vec(&from, n)?, c.scale_vec(&to, n)?);
            Ok(f.iter().zip(&t).map(|(a, b)| a / b).collect())
        }),
    )?;
    let r = narrow(b, v.r, m, s, None, dt);
    if dt == DType::I16 {
        b.commit(r);
    }
    let out = Val { r, dt, key: key.clone(), len: v.len, site: v.site.clone() };
    note_resid(cx, lb, &out);
    Ok(out)
}

/// `v` as `i16` codes: kept if it already is, else narrowed to its own site's code scale.
fn codes(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: &Val) -> Result<Val> {
    if v.dt == DType::I16 {
        return Ok(v.clone());
    }
    let key = ScaleKey::site(vec![v.site.clone()], false);
    coerce(b, cx, lb, v, DType::I16, &key)
}

/// What a node with site `site` produces when nobody asked: `i16` codes at its own scale (and
/// that of any RoPE consuming it).
fn default_want(lb: &Lb, i: usize, site: &str) -> Want {
    let mut names = vec![site.to_string()];
    names.extend(lb.rope_sites[i].iter().cloned());
    let key = if lb.split[i] > 0 { ScaleKey::site_split(names, lb.split[i]) } else { ScaleKey::site(names, false) };
    Want { dt: DType::I16, key }
}

// ───────────────────────────── node lowering ─────────────────────────────

fn lower_node(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, i: usize, node: &hl::Node) -> Result<Vec<Option<Val>>> {
    let hl = cx.hl;
    let site = node.site.clone().unwrap_or_default();
    let want = lb.wants[i].clone().unwrap_or_else(|| default_want(lb, i, &site));
    let one = |v: Val| Ok(vec![Some(v)]);
    let pidx = |r: hl::Ref| -> Result<u32> {
        if let hl::Ref::Param(p) = r { Ok(p) } else { Err(LowerError::eval(format!("internal: expected a param, got {r:?}"))) }
    };
    let out_len = node.outs.first().map(|s| s.iter().product::<usize>()).unwrap_or(0);
    if lb.absorbed[i] {
        return Ok(vec![None; node.outs.len()]);
    }
    match &node.op {
        Op::Embedding => {
            let tp = pidx(node.inputs[1])?;
            let v = lower_row_lookup(b, cx, lb, tp, tir::Ref::Input(INPUT_TOKEN), &site, &want)?;
            one(v)
        }
        Op::PosEmbedding { offset } => {
            let tp = pidx(node.inputs[1])?;
            let rows = hl.params[tp as usize].shape[0];
            let off = b.c(DType::I64, *offset as i128);
            let at = b.add(tir::Ref::Input(INPUT_POS), off, DType::I64);
            // Past the learned table the model is undefined (HF indexes out of range); the program
            // stays total by holding the last row, which also states the range to the analysis.
            let at = b.clamp(at, 0, rows as i64 - 1, DType::Idx);
            let v = lower_row_lookup(b, cx, lb, tp, at, &site, &want)?;
            one(v)
        }
        Op::Linear { bias } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let w = pidx(node.inputs[1])?;
            let bp = if *bias { Some(pidx(node.inputs[2])?) } else { None };
            one(lower_linear(b, cx, lb, &x, w, bp, &site, &want)?)
        }
        Op::Add | Op::Sub => {
            let a = operand(lb, node.inputs[0])?;
            let c = operand(lb, node.inputs[1])?;
            let target = if lb.wants[i].is_some() || a.key.base == Base::Resid || c.key.base == Base::Resid {
                if lb.wants[i].is_some() { want.clone() } else { Want { dt: DType::I32, key: ScaleKey::resid() } }
            } else {
                want.clone()
            };
            let a = coerce(b, cx, lb, &a, target.dt, &target.key)?;
            let c = coerce(b, cx, lb, &c, target.dt, &target.key)?;
            let s = if matches!(node.op, Op::Add) { b.add(a.r, c.r, DType::I64) } else { b.sub(a.r, c.r, DType::I64) };
            let (lo, hi) = code_bounds(target.dt);
            let r = b.clamp(s, lo, hi, target.dt);
            b.commit(r);
            let v = Val { r, dt: target.dt, key: target.key, len: out_len, site };
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::Scale { c } => {
            let x = operand(lb, node.inputs[0])?;
            let v = Val { key: x.key.times(*c), site, ..x };
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::Norm { spec, groups } => {
            let x = operand(lb, node.inputs[0])?;
            let gain = if spec.gain != Gain::None { Some(pidx(node.inputs[1])?) } else { None };
            let bias = if spec.bias { Some(pidx(node.inputs[if gain.is_some() { 2 } else { 1 }])?) } else { None };
            let v = lower_norm(b, cx, lb, &x, spec.kind, spec.eps, spec.gain, gain, bias, *groups, &site, &want)?;
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::Act(a) => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            one(lower_table(b, cx, lb, i, &x, TableFn::Act(*a), &site)?)
        }
        Op::Softcap { cap } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            one(lower_table(b, cx, lb, i, &x, TableFn::Softcap(*cap), &site)?)
        }
        Op::Clamp { lo, hi } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            one(lower_table(b, cx, lb, i, &x, TableFn::Clamp(*lo, *hi), &site)?)
        }
        Op::Mul => {
            let a = operand(lb, node.inputs[0])?;
            let c = operand(lb, node.inputs[1])?;
            let a = codes(b, cx, lb, &a)?;
            let c = codes(b, cx, lb, &c)?;
            let p = b.mul(a.r, c.r, DType::I32);
            let key = want.key.clone();
            let (ka, kc, ko) = (a.key.clone(), c.key.clone(), key.clone());
            let per_channel = ka.split() > 0 || kc.split() > 0 || ko.split() > 0;
            let n = if per_channel { out_len } else { 1 };
            let (la, lc) = (a.len, c.len);
            let (m, s) = decl_ms(
                b,
                cx,
                lb,
                &site,
                n,
                Arc::new(move |f| {
                    let at = |k: &ScaleKey, len: usize| -> Result<Vec<f64>> {
                        if n == 1 || len == 1 { Ok(vec![f.scale(k)?; n]) } else { f.scale_vec(k, n) }
                    };
                    let (va, vc, vo) = (at(&ka, la)?, at(&kc, lc)?, f.scale_vec(&ko, n)?);
                    Ok((0..n).map(|i| va[i] * vc[i] / vo[i]).collect())
                }),
            )?;
            let r = narrow(b, p, m, s, None, want.dt);
            b.commit(r);
            let v = Val { r, dt: want.dt, key, len: out_len, site };
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::Route { router, experts, top_k } => {
            let l = operand(lb, node.inputs[0])?;
            let l = coerce(b, cx, lb, &l, DType::I32, &q14())?;
            let (idx, w) = lower_route(b, lb, &l, router, *experts, *top_k)?;
            b.commit(w);
            let wkey = ScaleKey::q24().times(router.scale);
            Ok(vec![
                Some(Val { r: idx, dt: DType::Idx, key: ScaleKey::q24(), len: *top_k, site: format!("{site}.idx") }),
                Some(Val { r: w, dt: DType::I32, key: wkey, len: *top_k, site: format!("{site}.w") }),
            ])
        }
        Op::MoeExperts { top_k, act, glu, bias } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let idx = operand(lb, node.inputs[1])?;
            let w = operand(lb, node.inputs[2])?;
            let ps: Vec<u32> = (3..if *bias { 9 } else { 6 }).map(|k| pidx(node.inputs[k])).collect::<Result<_>>()?;
            let v = lower_moe(b, cx, lb, &x, &idx, &w, &ps, *top_k, *act, *glu, &site, &want)?;
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::Rope { heads, head_dim, rotary_dim, offset, style, table } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            one(lower_rope(b, cx, lb, &x, *heads, *head_dim, *rotary_dim, *offset, *style, *table)?)
        }
        Op::HistAppend => {
            let hl::Ref::State(s) = node.inputs[1] else { return Err(LowerError::eval("internal: HistAppend without a state")) };
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let sd = &hl.states[s as usize];
            let StateKind::Hist { window } = sd.kind else { return Err(LowerError::eval("internal: HistAppend on a Fixed state")) };
            let w = window.map(|w| w as u32).unwrap_or(cx.history_bound).min(cx.history_bound);
            let ts = match cx.tstate.get(&s) {
                Some(t) => *t,
                None => {
                    let t = b.pb.hist_state(&sd.name, DType::I16, &u32s(&sd.shape), w, true);
                    cx.tstate.insert(s, t);
                    t
                }
            };
            let row = ensure_node(b, &x);
            let win = b.hist_append(ts, row.r);
            lb.windows.insert(s, (win, x.key.clone()));
            Ok(vec![None])
        }
        Op::Attention { heads, kv_heads, head_dim, v_head_dim, scale, softcap, window: _, alibi, sinks } => {
            let sink_param = if *sinks { Some(pidx(node.inputs[3])?) } else { None };
            let q = operand(lb, node.inputs[0])?;
            let q = codes(b, cx, lb, &q)?;
            let (hl::Ref::State(ks), hl::Ref::State(vs)) = (node.inputs[1], node.inputs[2]) else {
                return Err(LowerError::eval("internal: attention without states"));
            };
            let (kw, kk) = lb.windows.get(&ks).cloned().ok_or_else(|| LowerError::eval("internal: K read before its append"))?;
            let (vw, vk) = lb.windows.get(&vs).cloned().ok_or_else(|| LowerError::eval("internal: V read before its append"))?;
            let want =
                if want.dt == DType::I16 { want } else { Want { dt: DType::I16, key: ScaleKey::site(vec![site.clone()], false) } };
            let window = match hl.states[ks as usize].kind {
                StateKind::Hist { window } => window.map(|w| w as u32).unwrap_or(cx.history_bound).min(cx.history_bound),
                StateKind::Fixed => return Err(LowerError::eval("internal: attention over a Fixed state")),
            };
            let shape = AttnDims { heads: *heads, kv: *kv_heads, d: *head_dim, dv: *v_head_dim, window };
            let extra = AttnExtras { scale: *scale, softcap: *softcap, alibi: alibi.clone(), sinks: sink_param };
            one(lower_attention(b, cx, lb, &q, (kw, kk), (vw, vk), shape, &extra, &site, &want)?)
        }
        Op::Slice { start, len } => {
            let x = operand(lb, node.inputs[0])?;
            let r = b.slice(x.r, 0, *start as u32, *len as u32);
            one(Val { r, len: *len, ..x })
        }
        Op::Concat => {
            let parts: Vec<Val> = node.inputs.iter().map(|r| operand(lb, *r)).collect::<Result<_>>()?;
            let first = codes(b, cx, lb, &parts[0])?;
            let mut refs = vec![first.r];
            for p in &parts[1..] {
                let p = coerce(b, cx, lb, p, first.dt, &first.key)?;
                refs.push(p.r);
            }
            if refs.len() > 8 {
                return Err(LowerError::not_lowerable("a concat of more than 8 values"));
            }
            let r = b.concat(&refs, 0);
            let site = if site.is_empty() { first.site.clone() } else { site };
            one(Val { r, len: out_len, site, ..first })
        }
        Op::CausalConv1d { channels, kernel, bias, act } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: conv without a state")) };
            let w = pidx(node.inputs[2])?;
            let bp = if *bias { Some(pidx(node.inputs[3])?) } else { None };
            one(lower_conv(b, cx, lb, i, &x, st, w, bp, *channels, *kernel, *act, &site)?)
        }
        Op::L2Norm { groups, eps: _ } => {
            // `x·rsqrt(Σx² + eps)`: the library's Q15 L2 norm (a zero row stays zero; eps ≈ 1e-6
            // is below code resolution for every other row).
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let g = out_len / groups;
            let xr = b.reshape_fixed(x.r, &[*groups as u32, g as u32]);
            let y = l2_unit_q15(b, xr);
            let r = b.reshape_fixed(y, &[out_len as u32]);
            b.commit(r);
            one(Val { r, dt: DType::I16, key: ScaleKey { base: Base::Fixed(1.0 / 32768.0), factor: 1.0 }, len: out_len, site })
        }
        Op::GatedDelta { k_heads, v_heads, dk, dv, head_map, q_scale } => {
            let q = operand(lb, node.inputs[0])?;
            let k = operand(lb, node.inputs[1])?;
            let v = operand(lb, node.inputs[2])?;
            let v = codes(b, cx, lb, &v)?;
            let hl::Ref::State(st) = node.inputs[5] else { return Err(LowerError::eval("internal: GDN without a state")) };
            let gi = lb.gdn.get(&i).cloned().ok_or_else(|| LowerError::eval("internal: GDN inputs not matched"))?;
            let a = lb.vals[gi.a as usize][0].clone().ok_or_else(|| LowerError::eval("internal: GDN `a` not lowered"))?;
            let bb = lb.vals[gi.b as usize][0].clone().ok_or_else(|| LowerError::eval("internal: GDN `b` not lowered"))?;
            let dims = GdnDims { nk: *k_heads, nv: *v_heads, dk: *dk, dv: *dv, map: *head_map, q_scale: *q_scale };
            one(lower_gdn(b, cx, lb, &q, &k, &v, &a, &bb, &gi, st, dims, &site, &want)?)
        }
        Op::GatedRmsNorm { eps, groups, gate_first } => {
            if *gate_first {
                return Err(LowerError::not_lowerable("the gate-first gated RMSNorm (Mamba2) is not in Gate 2a"));
            }
            let x = operand(lb, node.inputs[0])?;
            let z = operand(lb, node.inputs[1])?;
            let z = codes(b, cx, lb, &z)?;
            let gain = pidx(node.inputs[2])?;
            let v = lower_gated_norm(b, cx, lb, &x, &z, gain, *eps, *groups, &site, &want)?;
            note_resid(cx, lb, &v);
            one(v)
        }
        other => Err(LowerError::not_lowerable(format!("op {} is not in Gate 2a (dense decoders only)", other.name()))),
    }
}

/// A row of a table param selected by an index (`Embedding`, `PosEmbedding`), lifted to `want`:
/// the `i8` row times its own per-row scale — a per-token (per-position) `(m, s)` gathered by the
/// same index.
fn lower_row_lookup(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    tp: u32,
    at: tir::Ref,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let d = &hl.params[tp as usize];
    let (rows, cols) = (d.shape[0], d.shape[1]);
    let table = decl(b, cx, lb, &d.name, DType::I8, &[rows, cols], d.per_layer, weight_codes(tp))?;
    let key = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        rows,
        Arc::new(move |c| {
            let rc = c.rows(tp)?;
            let to = c.scale(&key)?;
            Ok(rc.scales.iter().map(|sw| sw / to).collect())
        }),
    )?;
    let row = b.gather(table, at, 0, 0);
    let mt = b.gather(m, at, 0, 0);
    let st = b.gather(s, at, 0, 0);
    let r = narrow(b, row, mt, st, None, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = Val { r, dt: want.dt, key: want.key.clone(), len: cols, site: site.to_string() };
    note_resid(cx, lb, &v);
    Ok(v)
}

/// Weight codes of an HL `[out, in]` param (per-row scales).
fn weight_codes(p: u32) -> FillFn {
    Arc::new(move |c| {
        let rc = c.rows(p)?;
        Ok(IntTensor::i8(c.f(p)?.shape.clone(), rc.codes.clone()))
    })
}

/// `W·x (+ b)`. With a split input (`x.key.split() = k`) the `k` outlier channels — each at its
/// own activation scale — go through their own columns in `i32` fixed point, the rest through
/// the `i8` main codes (outlier columns zeroed), and the two accumulators meet exactly:
///
/// ```text
/// acc  = MatMul(W8:i8[out,in], x:i16[in,1])            -- main (outlier columns are 0)
/// acco = MatMul(Wo:i32[out,k], Gather(x, oidx):[k,1])   -- outliers, in main units · 2^f[o]
/// y    = N(acc · 2^f[o] + acco; m, s, z)
/// ```
///
/// This is the static form of the int8 "outlier decomposition": a massive activation in a few
/// fixed channels (Qwen2.5's first-token MLP channels reach 1000× the others) costs those
/// channels nothing and the rest keep their precision.
#[allow(clippy::too_many_arguments)]
fn lower_linear(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    w: u32,
    bias: Option<u32>,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let d = &hl.params[w as usize];
    let (out, inp) = (d.shape[0], d.shape[1]);
    if x.len != 0 && x.len != inp {
        return Err(LowerError::eval(format!("internal: linear `{}` reads {} values, weight has {inp} columns", d.name, x.len)));
    }
    let pl = per_layer(lb);
    let k = x.key.split();
    let (kx, ky) = (x.key.clone(), want.key.clone());
    let z = match bias {
        Some(bp) => {
            let ky = want.key.clone();
            Some(decl(
                b,
                cx,
                lb,
                &format!("{site}.z"),
                DType::I64,
                &[out],
                pl,
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let sy = c.scale_vec(&ky, bv.data.len())?;
                    Ok(IntTensor::i64(
                        vec![bv.data.len()],
                        bv.data.iter().zip(&sy).map(|(v, s)| (*v as f64 / s).round() as i64).collect(),
                    ))
                }),
            )?)
        }
        None => None,
    };
    let xc = b.reshape_fixed(x.r, &[inp as u32, 1]);
    let r = if k == 0 {
        let wt = decl(b, cx, lb, &d.name, DType::I8, &[out, inp], d.per_layer, weight_codes(w))?;
        let (m, s) = decl_ms(
            b,
            cx,
            lb,
            site,
            out,
            Arc::new(move |c| {
                let rc = c.rows(w)?;
                let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
                Ok(rc.scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
            }),
        )?;
        let acc = b.matmul(wt, xc, DType::I64);
        let acc = b.reshape_fixed(acc, &[out as u32]);
        narrow(b, acc, m, s, z, want.dt)
    } else {
        // `acc · 2^f` stays inside i64 for any i8 × i16 accumulator of `inp` terms.
        let acc_bits = 22 + (inp as f64).log2().ceil() as i32;
        let f_max = (62 - acc_bits).clamp(0, 24);
        let (k1, k2, k3, k4) = (kx.clone(), kx.clone(), kx.clone(), kx.clone());
        let wt = decl(
            b,
            cx,
            lb,
            &d.name,
            DType::I8,
            &[out, inp],
            d.per_layer,
            Arc::new(move |c| {
                let sc = c.split_rows(w, &k1, f_max)?;
                Ok(IntTensor::i8(vec![sc.main.rows, sc.main.cols], sc.main.codes.clone()))
            }),
        )?;
        let oidx = decl(
            b,
            cx,
            lb,
            &format!("{site}.oidx"),
            DType::Idx,
            &[k],
            pl,
            Arc::new(move |c| Ok(IntTensor::idx(vec![k], c.outliers(&k2)?.iter().map(|c| *c as u32).collect()))),
        )?;
        let wo = decl(
            b,
            cx,
            lb,
            &format!("{site}.wo"),
            DType::I32,
            &[out, k],
            pl,
            Arc::new(move |c| {
                let sc = c.split_rows(w, &k3, f_max)?;
                Ok(IntTensor::i32(vec![out, k], sc.wo.clone()))
            }),
        )?;
        let fo = decl(
            b,
            cx,
            lb,
            &format!("{site}.of"),
            DType::I8,
            &[out],
            pl,
            Arc::new(move |c| Ok(IntTensor::i8(vec![out], c.split_rows(w, &k4, f_max)?.f.clone()))),
        )?;
        let (m, s) = decl_ms(
            b,
            cx,
            lb,
            site,
            out,
            Arc::new(move |c| {
                let sc = c.split_rows(w, &kx, f_max)?;
                let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
                Ok((0..out).map(|o| sc.main.scales[o] * sx / (2f64.powi(sc.f[o] as i32) * sy[o])).collect())
            }),
        )?;
        let acc = b.matmul(wt, xc, DType::I64);
        let acc = b.reshape_fixed(acc, &[out as u32]);
        // The outlier indices are a param: state their range for the analysis (never fires).
        let oi = b.clamp(oidx, 0, inp as i64 - 1, DType::Idx);
        let xo = b.gather(x.r, oi, 0, 0);
        let xo = b.reshape_fixed(xo, &[k as u32, 1]);
        let acco = b.matmul(wo, xo, DType::I64);
        let acco = b.reshape_fixed(acco, &[out as u32]);
        let p2f = b.pow2_128_of(fo, f_max as u32);
        let shifted = b.mul(acc, p2f, DType::I64);
        let sum = b.add(shifted, acco, DType::I64);
        narrow(b, sum, m, s, z, want.dt)
    };
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = Val { r, dt: want.dt, key: want.key.clone(), len: out, site: site.to_string() };
    note_resid(cx, lb, &v);
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
fn lower_norm(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    kind: NormKind,
    eps: f64,
    gain_kind: Gain,
    gain: Option<u32>,
    bias: Option<u32>,
    groups: usize,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let n = match b.shape(x.r).as_slice() {
        [Dim::Fixed(n)] => *n as usize,
        s => return Err(LowerError::eval(format!("internal: norm input of shape {s:?}"))),
    };
    let groups = groups.max(1);
    let g = n / groups;
    let pl = per_layer(lb);
    let xr = if groups > 1 { b.reshape_fixed(x.r, &[groups as u32, g as u32]) } else { x.r };
    // LayerNorm: exact centring `c = g·x − Σx` (no division), brought into i32 by 2^k.
    let in_bits: i32 = if x.dt == DType::I16 { 16 } else { 32 };
    let k = match kind {
        NormKind::Rms => 0,
        NormKind::Layer => (in_bits + (g as f64).log2().ceil() as i32 - 31).max(0) as u32,
    };
    let kx = x.key.clone();
    let eps_q = Arc::new(move |c: &FillCtx<'_>| -> Result<f64> {
        let sx = c.scale(&kx)?;
        let base = eps * (1u64 << 24) as f64 / (sx * sx);
        Ok(match kind {
            NormKind::Rms => base,
            NormKind::Layer => base * (g * g) as f64 / 4f64.powi(k as i32),
        })
    });
    let eps_p = decl_eps(b, cx, lb, site, eps_q)?;
    let u = match kind {
        NormKind::Rms => rms_unit(b, xr, eps_p),
        NormKind::Layer => {
            let axis = if groups > 1 { 1 } else { 0 };
            let gg = b.c(DType::I64, g as i128);
            let nx = b.mul(xr, gg, DType::I64);
            let sum = b.reduce_sum(xr, axis, DType::I64);
            let c = b.sub(nx, sum, DType::I64);
            let c = if k > 0 { b.shr(c, k, Rounding::HalfAwayFromZero, DType::I64) } else { c };
            let c = b.clamp(c, i32::MIN as i64, i32::MAX as i64, DType::I32);
            rms_unit(b, c, eps_p)
        }
    };
    let u = if groups > 1 { b.reshape_fixed(u, &[n as u32]) } else { u };
    let ky = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let sy = c.scale_vec(&ky, n)?;
            let unit = 1.0 / (1u64 << 24) as f64;
            let gv: Option<Vec<f32>> = match gain {
                Some(p) => Some(c.f(p)?.data.clone()),
                None => None,
            };
            Ok((0..n)
                .map(|i| {
                    let gi = gv.as_ref().map(|v| if v.len() == n { v[i] } else { v[i % v.len()] });
                    let g = match (gain_kind, gi) {
                        (Gain::OnePlusW, Some(w)) => 1.0 + w as f64,
                        (_, Some(w)) => w as f64,
                        (_, None) => 1.0,
                    };
                    g * unit / sy[i]
                })
                .collect())
        }),
    )?;
    let z = match bias {
        Some(bp) => {
            let ky = want.key.clone();
            Some(decl(
                b,
                cx,
                lb,
                &format!("{site}.z"),
                DType::I64,
                &[n],
                pl,
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let sy = c.scale_vec(&ky, n)?;
                    Ok(IntTensor::i64(
                        vec![n],
                        (0..n)
                            .map(|i| (bv.data[if bv.data.len() == n { i } else { i % bv.data.len() }] as f64 / sy[i]).round() as i64)
                            .collect(),
                    ))
                }),
            )?)
        }
        None => None,
    };
    let r = narrow(b, u, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
}

/// The norm's epsilon at the input's code scale, `eps · 2^24 / S_x²`, as one `i64` param.
fn decl_eps(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    site: &str,
    eps_q: Arc<dyn Fn(&FillCtx<'_>) -> Result<f64> + Send + Sync>,
) -> Result<tir::Ref> {
    decl(
        b,
        cx,
        lb,
        &format!("{site}.eps"),
        DType::I64,
        &[1],
        per_layer(lb),
        Arc::new(move |c| Ok(IntTensor::i64(vec![1], vec![eps_q(c)?.round().clamp(0.0, i64::MAX as f64) as i64]))),
    )
}

/// The unit row `x / √(mean(x²) + eps)` in Q24 along the last axis, for rows of any width up to
/// `i32` — the value of the library's `rms_norm_wide_q36` in 21 nodes instead of 39.
///
/// The mean's exponent is taken out only when it is positive (`h = max(0, ⌊(log2 mean − 24)/2⌋)`):
/// for a smaller mean `IntRsqrt` normalises internally and returns `y · 2^−e` exactly as the
/// template's left-shift branch reassembles it, so both give the same integers; the eps is one
/// `i64` param instead of a mantissa and a shift. (Proposed for `tir_library_v1`: the GDN + MoE
/// hybrid layers of Qwen3-Next/3.5 do not fit NF-12's 512 nodes with the 39-node form.)
fn rms_unit(b: &mut BlockBuilder<'_>, x: tir::Ref, eps: tir::Ref) -> tir::Ref {
    let shape = b.shape(x);
    let axis = shape.len() - 1;
    let Dim::Fixed(n) = shape[axis] else { panic!("norm over H") };
    let sq = b.mul(x, x, DType::I64);
    let sum = b.reduce_sum(sq, axis, DType::I128);
    let one = b.c(DType::I64, 1 << 24);
    let scaled = b.mul(sum, one, DType::I128);
    let nn = b.c(DType::I64, n as i128);
    let mean0 = b.div(scaled, nn, Rounding::Floor, DType::I128);
    let e = b.clamp(eps, 0, i64::MAX, DType::I64);
    let mean = b.add(mean0, e, DType::I128);
    let bit = b.log2_floor(mean, DType::I32);
    let k = b.c(DType::I32, 24);
    let t = b.sub(bit, k, DType::I32);
    let two = b.c(DType::I32, 2);
    let h = b.div(t, two, Rounding::Floor, DType::I32);
    let h = b.clamp(h, 0, 51, DType::I32);
    let h2 = b.mul(h, two, DType::I32);
    let p2 = b.pow2_128_of(h2, 102);
    let m = b.div(mean, p2, Rounding::Floor, DType::I128);
    let m = b.clamp(m, 0, i64::MAX, DType::I64);
    let r = b.int_rsqrt(m);
    let prod = b.mul(x, r, DType::I128);
    let p1 = b.pow2_128_of(h, 51);
    let y = b.div(prod, p1, Rounding::Floor, DType::I128);
    b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

/// `x / ‖x‖` in Q15 codes along the last axis (the library's `l2_norm_q15`, a zero row stays
/// zero) in 17 nodes: `IntRsqrt(Σx² / 2^2h) · x / 2^(h + 21)`, `h = max(0, ⌊(log2 Σx² − 24)/2⌋)`.
fn l2_unit_q15(b: &mut BlockBuilder<'_>, x: tir::Ref) -> tir::Ref {
    let axis = b.shape(x).len() - 1;
    let sq = b.mul(x, x, DType::I64);
    let sum = b.reduce_sum(sq, axis, DType::I64);
    let bit = b.log2_floor(sum, DType::I32);
    let k = b.c(DType::I32, 24);
    let t = b.sub(bit, k, DType::I32);
    let two = b.c(DType::I32, 2);
    let h = b.div(t, two, Rounding::Floor, DType::I32);
    let h = b.clamp(h, 0, 20, DType::I32);
    let h2 = b.mul(h, two, DType::I32);
    let p2 = b.pow2_128_of(h2, 40);
    let m = b.div(sum, p2, Rounding::Floor, DType::I64);
    let r = b.int_rsqrt(m);
    let prod = b.mul(x, r, DType::I64);
    let off = b.c(DType::I32, 21);
    let sh = b.add(h, off, DType::I32);
    let p1 = b.pow2_128_of(sh, 41);
    let y = b.div(prod, p1, Rounding::Floor, DType::I64);
    b.clamp(y, -32767, 32767, DType::I16)
}

/// The float function an activation table tabulates.
#[derive(Clone, Copy, Debug)]
enum TableFn {
    Act(Act),
    Softcap(f64),
    Clamp(f64, f64),
    /// gpt-oss's gate half: `g·σ(α·g)` with `g = min(gate, limit)`.
    ClampedGlu {
        alpha: f64,
        limit: f64,
    },
    /// gpt-oss's up half: `clamp(up, −limit, limit) + 1`.
    ClampedUp {
        limit: f64,
    },
}

impl TableFn {
    fn eval(self, x: f64) -> f64 {
        match self {
            TableFn::Act(a) => crate::float_ref::act(a, x as f32) as f64,
            TableFn::Softcap(c) => (x / c).tanh() * c,
            TableFn::Clamp(lo, hi) => x.clamp(lo, hi),
            TableFn::ClampedGlu { alpha, limit } => {
                let g = x.min(limit);
                g / (1.0 + (-(g * alpha)).exp())
            }
            TableFn::ClampedUp { limit } => x.clamp(-limit, limit) + 1.0,
        }
    }
}

/// `Table(x; T)`: `Gather(T, Cast_idx(x + 32768))`, `T` the float function on the code grid,
/// rounded to the output site's codes.
fn lower_table(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, i: usize, x: &Val, f: TableFn, site: &str) -> Result<Val> {
    // A table has one output scale (its own site's, and any RoPE's that rotates it).
    let mut names = vec![site.to_string()];
    names.extend(lb.rope_sites[i].iter().cloned());
    let out_key = ScaleKey::site(names, false);
    let (kx, ko) = (x.key.clone(), out_key.clone());
    let t = decl(
        b,
        cx,
        lb,
        &format!("{site}.table"),
        DType::I16,
        &[65536],
        per_layer(lb),
        Arc::new(move |c| {
            let (sx, so) = (c.scale(&kx)?, c.scale(&ko)?);
            Ok(IntTensor::i16(
                vec![65536],
                (0..65536i64).map(|i| (f.eval((i - 32768) as f64 * sx) / so).round().clamp(-32767.0, 32767.0) as i16).collect(),
            ))
        }),
    )?;
    // `code + 32768` straight into `idx` (every i16 code lands in [0, 65535]).
    let off = b.c(DType::I32, 32768);
    let at = b.add(x.r, off, DType::Idx);
    let r = b.gather(t, at, 0, 0);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key: out_key, len: x.len, site: site.to_string() })
}

/// [`lower_table`] for a value that is not an HL node's output (a sub-site of a composite).
fn lower_table_named(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, f: TableFn, site: &str) -> Result<Val> {
    lower_table_keyed(b, cx, lb, x, f, site, ScaleKey::site(vec![site.to_string()], false))
}

/// A table whose output scale is given (not a calibrated site).
fn lower_table_keyed(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    f: TableFn,
    site: &str,
    out_key: ScaleKey,
) -> Result<Val> {
    let (kx, ko) = (x.key.clone(), out_key.clone());
    let t = decl(
        b,
        cx,
        lb,
        &format!("{site}.table"),
        DType::I16,
        &[65536],
        per_layer(lb),
        Arc::new(move |c| {
            let (sx, so) = (c.scale(&kx)?, c.scale(&ko)?);
            Ok(IntTensor::i16(
                vec![65536],
                (0..65536i64).map(|i| (f.eval((i - 32768) as f64 * sx) / so).round().clamp(-32767.0, 32767.0) as i16).collect(),
            ))
        }),
    )?;
    // `code + 32768` straight into `idx` (every i16 code lands in [0, 65535]).
    let off = b.c(DType::I32, 32768);
    let at = b.add(x.r, off, DType::Idx);
    let r = b.gather(t, at, 0, 0);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key: out_key, len: x.len, site: site.to_string() })
}

/// The `(cos, sin)` Q24 rows for RoPE table `t` at this position, once per block.
fn rope_angles(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, t: u32) -> Result<(tir::Ref, tir::Ref)> {
    if let Some(a) = lb.angles.get(&t) {
        return Ok(*a);
    }
    let hl = cx.hl;
    let f = hl.rope_tables[t as usize].clone();
    let half = f.inv_freq.len();
    let hb = cx.history_bound as usize;
    let per_position = f.dynamic.is_some() || f.longrope.is_some();
    let out = if per_position {
        // The frequencies depend on the position (decode reading, `seq_len = pos + 1`): one row per
        // position of the history bound.
        let mk = |sin: bool| -> FillFn {
            let f = f.clone();
            Arc::new(move |_c| {
                let mut v = Vec::with_capacity(hb * half);
                for pos in 0..hb {
                    for fr in f.inv_freq_at(pos) {
                        let ang = ((pos as f32) * fr) as f64;
                        v.push(q24(if sin { ang.sin() } else { ang.cos() }));
                    }
                }
                Ok(IntTensor::i32(vec![hb, half], v))
            })
        };
        let cos = decl(b, cx, lb, &format!("rope{t}.cos"), DType::I32, &[hb, half], false, mk(false))?;
        let sin = decl(b, cx, lb, &format!("rope{t}.sin"), DType::I32, &[hb, half], false, mk(true))?;
        let pos = tir::Ref::Input(INPUT_POS);
        (b.gather(cos, pos, 0, 0), b.gather(sin, pos, 0, 0))
    } else {
        let lo_bits = hb.trailing_zeros().div_ceil(2);
        let (lo_rows, hi_rows) = (1usize << lo_bits, hb >> lo_bits);
        // cos/sin of `r · 2^step · θ_i`, exact in f64 on HF's float32 θ.
        let mk = |rows: usize, step: u32, sin: bool| -> FillFn {
            let inv = f.inv_freq.clone();
            Arc::new(move |_c| {
                let mut v = Vec::with_capacity(rows * inv.len());
                for r in 0..rows {
                    for th in &inv {
                        let ang = (r as f64) * (1u64 << step) as f64 * (*th as f64);
                        v.push(q24(if sin { ang.sin() } else { ang.cos() }));
                    }
                }
                Ok(IntTensor::i32(vec![rows, inv.len()], v))
            })
        };
        let ch = decl(b, cx, lb, &format!("rope{t}.cos_hi"), DType::I32, &[hi_rows, half], false, mk(hi_rows, lo_bits, false))?;
        let sh = decl(b, cx, lb, &format!("rope{t}.sin_hi"), DType::I32, &[hi_rows, half], false, mk(hi_rows, lo_bits, true))?;
        let cl = decl(b, cx, lb, &format!("rope{t}.cos_lo"), DType::I32, &[lo_rows, half], false, mk(lo_rows, 0, false))?;
        let sl = decl(b, cx, lb, &format!("rope{t}.sin_lo"), DType::I32, &[lo_rows, half], false, mk(lo_rows, 0, true))?;
        b.rope_angles_two_level(tir::Ref::Input(INPUT_POS), ch, sh, cl, sl, lo_bits)
    };
    lb.angles.insert(t, out);
    Ok(out)
}

fn q24(v: f64) -> i32 {
    (v * (1u64 << 24) as f64).round().clamp(-(1i64 << 24) as f64, (1i64 << 24) as f64) as i32
}

#[allow(clippy::too_many_arguments)]
fn lower_rope(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    heads: usize,
    hd: usize,
    rd: usize,
    off: usize,
    style: RopeStyle,
    t: u32,
) -> Result<Val> {
    let f = cx.hl.rope_tables[t as usize].attention_factor;
    if rd / 2 != cx.hl.rope_tables[t as usize].inv_freq.len() {
        return Err(LowerError::eval("internal: rotary dim and frequency count disagree"));
    }
    let (cos, sin) = rope_angles(b, cx, lb, t)?;
    let x2 = b.reshape_fixed(x.r, &[heads as u32, hd as u32]);
    let xr = if off == 0 && rd == hd { x2 } else { b.slice(x2, 1, off as u32, rd as u32) };
    let rot = match style {
        RopeStyle::Interleaved => b.rope_pairs(xr, cos, sin, -32767, 32767, DType::I16),
        RopeStyle::Half => {
            let half = (rd / 2) as u32;
            let a = b.slice(xr, 1, 0, half);
            let c = b.slice(xr, 1, half, half);
            // HF `rotate_half`: first half a·cos − b·sin, second half b·cos + a·sin (floor >> 24).
            let ac = b.mul(a, cos, DType::I64);
            let bs = b.mul(c, sin, DType::I64);
            let bc = b.mul(c, cos, DType::I64);
            let as_ = b.mul(a, sin, DType::I64);
            let re = b.sub(ac, bs, DType::I64);
            let im = b.add(bc, as_, DType::I64);
            let re = b.shr(re, 24, Rounding::Floor, DType::I64);
            let im = b.shr(im, 24, Rounding::Floor, DType::I64);
            let re = b.clamp(re, -32767, 32767, DType::I16);
            let im = b.clamp(im, -32767, 32767, DType::I16);
            b.concat(&[re, im], 1)
        }
    };
    // HF multiplies cos and sin — the ROTATED dims — by the attention factor and leaves the
    // pass-through dims alone. The rotated codes carry the factor in their scale (`x.key · f`), so
    // a pass-through part is narrowed by `1/f` to share that scale.
    let pass = |b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, start: usize, len: usize| -> Result<tir::Ref> {
        let p = b.slice(x2, 1, start as u32, len as u32);
        if (f - 1.0).abs() < 1e-12 {
            return Ok(p);
        }
        let (m, s) = decl_ms(b, cx, lb, &format!("{}.rope_pass{start}", x.site), 1, Arc::new(move |_c| Ok(vec![1.0 / f])))?;
        Ok(narrow(b, p, m, s, None, DType::I16))
    };
    let mut parts = Vec::new();
    if off > 0 {
        parts.push(pass(b, cx, lb, 0, off)?);
    }
    parts.push(rot);
    if off + rd < hd {
        parts.push(pass(b, cx, lb, off + rd, hd - off - rd)?);
    }
    let y = if parts.len() > 1 { b.concat(&parts, 1) } else { rot };
    let r = b.reshape_fixed(y, &[(heads * hd) as u32]);
    b.commit(r);
    // The tables rotate only; HF's attention factor multiplies cos and sin, so it lives in the
    // scale of the rotated codes.
    Ok(Val { r, dt: DType::I16, key: x.key.times(f), len: heads * hd, site: x.site.clone() })
}

struct AttnDims {
    heads: usize,
    kv: usize,
    d: usize,
    dv: usize,
    /// The history window `W` of the block (`H = min(pos + 1, W)`).
    window: u32,
}

struct AttnExtras {
    scale: f64,
    softcap: Option<f64>,
    alibi: Option<crate::rope::AlibiSpec>,
    /// gpt-oss: a learned per-head logit that joins the softmax and is dropped.
    sinks: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
fn lower_attention(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    q: &Val,
    k: (tir::Ref, ScaleKey),
    v: (tir::Ref, ScaleKey),
    dims: AttnDims,
    ex: &AttnExtras,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let AttnDims { heads, kv, d, dv, window } = dims;
    let g = heads / kv;
    let (kv32, g32, d32, dv32) = (kv as u32, g as u32, d as u32, dv as u32);
    let qg = b.reshape_fixed(q.r, &[kv32, g32, d32]);
    let kw = b.reshape(k.0, &[Dim::H, Dim::Fixed(kv32), Dim::Fixed(d32)]);
    let kt = b.transpose(kw, &[1, 2, 0]);
    let scores = b.matmul(qg, kt, DType::I64);
    let (kq, kk) = (q.key.clone(), k.1.clone());
    let scale = ex.scale;
    let mut logits = match ex.softcap {
        // Scores to logits in Q14 (`S_q · S_k · scale · 2^14`): ±131,072 in logit units, because a
        // layer's logits can be huge (Qwen2.5's layer 0 reaches 24,000 through its key biases), at
        // a resolution of 6e-5 — far below anything `IntExp` resolves. The softmax lifts
        // differences to Q24 by `2^(24 − 14)`.
        None => {
            let (m, s) = decl_ms(
                b,
                cx,
                lb,
                &format!("{site}.scores"),
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64])),
            )?;
            narrow(b, scores, m, s, None, DType::I32)
        }
        // Gemma 2: `cap · tanh(score / cap)`, with `tanh(y) = 2σ(2y) − 1` through the library's
        // integer sigmoid on `y = score / cap` in Q24 (the scores' range is ±128·cap there).
        Some(cap) => {
            let (m, s) = decl_ms(
                b,
                cx,
                lb,
                &format!("{site}.scores"),
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << 24) as f64 / cap])),
            )?;
            let y = narrow(b, scores, m, s, None, DType::I32);
            let two = b.c(DType::I64, 2);
            let y2 = b.mul(y, two, DType::I64);
            let y2 = b.clamp(y2, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let sig = b.int_sigmoid(y2);
            let s2 = b.mul(sig, two, DType::I64);
            let one = b.c(DType::I64, 1 << 24);
            let t = b.sub(s2, one, DType::I64);
            let t = b.clamp(t, -(1 << 24), 1 << 24, DType::I32);
            let (m, s) = decl_ms(
                b,
                cx,
                lb,
                &format!("{site}.softcap"),
                1,
                Arc::new(move |_c| Ok(vec![cap * (1u64 << LOGIT_Q) as f64 / (1u64 << 24) as f64])),
            )?;
            narrow(b, t, m, s, None, DType::I32)
        }
    };
    if let Some(al) = &ex.alibi {
        // ALiBi as the key's distance behind the query, `bias_j − bias_pos` (a per-row constant
        // away from HF's `slope · j`): recent keys stay small, and far ones saturate towards
        // `exp = 0` instead of overflowing. Keys are `j = pos − (H − 1) + t`, `H − 1 = min(pos, W − 1)`.
        let hb = cx.history_bound;
        let pos = tir::Ref::Input(INPUT_POS);
        let hm1 = b.clamp(pos, 0, window as i64 - 1, DType::I64);
        let t = b.iota(DType::I64, &[Dim::H], 0, 0, 1);
        let (hs, al2) = (heads, al.clone());
        let bias = if al.bf16_bias {
            // Falcon rounds `slope` and `slope · j` to bfloat16: one row of pinned values per head.
            let tab = decl(
                b,
                cx,
                lb,
                &format!("{site}.alibi"),
                DType::I64,
                &[heads, hb as usize],
                false,
                Arc::new(move |_c| {
                    let mut v = Vec::with_capacity(hs * hb as usize);
                    for h in 0..hs {
                        for j in 0..hb as usize {
                            let bj = crate::rope::bf16_round(crate::rope::bf16_round(al2.slopes[h] as f32) * j as f32) as f64;
                            let bj = if al2.scaled_by_softmax_scale { bj * scale } else { bj };
                            v.push((bj * (1u64 << LOGIT_Q) as f64).round() as i64);
                        }
                    }
                    Ok(IntTensor::i64(vec![hs, hb as usize], v))
                }),
            )?;
            let base = b.sub(pos, hm1, DType::I64);
            let j = b.add(base, t, DType::I64);
            let j = b.clamp(j, 0, hb as i64 - 1, DType::Idx);
            let bj = b.gather(tab, j, 1, 0);
            let bj = b.clamp(bj, -(1i64 << 40), 1 << 40, DType::I64);
            let bp = b.gather(tab, pos, 1, 0);
            let bp = b.clamp(bp, -(1i64 << 40), 1 << 40, DType::I64);
            let bp = b.reshape_fixed(bp, &[heads as u32, 1]);
            b.sub(bj, bp, DType::I64)
        } else {
            let slope = decl(
                b,
                cx,
                lb,
                &format!("{site}.alibi"),
                DType::I64,
                &[heads, 1],
                false,
                Arc::new(move |_c| {
                    Ok(IntTensor::i64(
                        vec![hs, 1],
                        al2.slopes
                            .iter()
                            .map(|sl| {
                                (sl * if al2.scaled_by_softmax_scale { scale } else { 1.0 } * (1u64 << LOGIT_Q) as f64).round() as i64
                            })
                            .collect(),
                    ))
                }),
            )?;
            let slope = b.clamp(slope, -(1i64 << 31), 1 << 31, DType::I64);
            let dist = b.sub(t, hm1, DType::I64);
            b.mul(slope, dist, DType::I64)
        };
        let bias = b.reshape(bias, &[Dim::Fixed(kv32), Dim::Fixed(g32), Dim::H]);
        let sum = b.add(logits, bias, DType::I64);
        logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
    }
    let probs = match ex.sinks {
        None => b.softmax_shifted(logits, 24 - LOGIT_Q),
        Some(sp) => {
            let sink = decl(
                b,
                cx,
                lb,
                &format!("{site}.sinks"),
                DType::I32,
                &[heads],
                per_layer(lb),
                Arc::new(move |c| {
                    let t = c.f(sp)?;
                    Ok(IntTensor::i32(
                        vec![t.data.len()],
                        t.data
                            .iter()
                            .map(|v| (*v as f64 * (1u64 << LOGIT_Q) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32)
                            .collect(),
                    ))
                }),
            )?;
            let sink = b.reshape_fixed(sink, &[kv32, g32, 1]);
            softmax_with_sink(b, logits, sink, 24 - LOGIT_Q)
        }
    };
    let vw = b.reshape(v.0, &[Dim::H, Dim::Fixed(kv32), Dim::Fixed(dv32)]);
    let vt = b.transpose(vw, &[1, 0, 2]);
    let o = b.matmul(probs, vt, DType::I64);
    let o = b.reshape_fixed(o, &[(heads * dv) as u32]);
    let out_key = want.key.clone();
    let (kv_, ko) = (v.1.clone(), out_key.clone());
    let n = if ko.split() > 0 { heads * dv } else { 1 };
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let sv = c.scale(&kv_)? / (1u64 << 24) as f64;
            Ok(c.scale_vec(&ko, n)?.iter().map(|so| sv / so).collect())
        }),
    )?;
    let r = narrow(b, o, m, s, None, DType::I16);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key: out_key, len: heads * dv, site: site.to_string() })
}

/// Expert selection from Q14 router logits (library `router_topk_q36` pattern): the softmax over
/// all experts then `TopK` (committed; lowest index on ties, index order), or `TopK` on the
/// logits then the softmax over the kept ones; renormalised through `IntRecip` when asked. The
/// weights are Q24; `routed_scaling_factor` lives in their scale.
fn lower_route(
    b: &mut BlockBuilder<'_>,
    _lb: &mut Lb,
    l: &Val,
    r: &crate::spec::RouterSpec,
    _experts: usize,
    k: usize,
) -> Result<(tir::Ref, tir::Ref)> {
    use crate::spec::Scoring;
    if r.groups.is_some() || r.selection_bias {
        return Err(LowerError::not_lowerable("group-limited or bias-corrected routing is not in Gate 2a"));
    }
    if r.normalize && r.norm_eps > 1e-9 {
        return Err(LowerError::not_lowerable(format!("router renormalisation epsilon {} is not in Gate 2a", r.norm_eps)));
    }
    let up = 24 - LOGIT_Q;
    let (idx, kept) = match r.scoring {
        Scoring::Softmax => {
            let probs = b.softmax_shifted(l.r, up);
            let idx = b.topk(probs, 0, k as u32);
            (idx, b.gather(probs, idx, 0, 0))
        }
        Scoring::TopKThenSoftmax => {
            let idx = b.topk(l.r, 0, k as u32);
            let kl = b.gather(l.r, idx, 0, 0);
            (idx, b.softmax_shifted(kl, up))
        }
        Scoring::Sigmoid => return Err(LowerError::not_lowerable("sigmoid routing is not in Gate 2a")),
    };
    let w = if r.normalize {
        let sum = b.reduce_sum(kept, 0, DType::I64);
        let recip = b.int_recip(sum);
        let p = b.mul(kept, recip, DType::I128);
        let q = b.shr(p, 24, Rounding::Floor, DType::I64);
        b.clamp(q, 0, 1 << 25, DType::I32)
    } else {
        kept
    };
    Ok((idx, w))
}

/// The routed experts, batched over the `k` selected: every expert tensor is a param `[E, …]`
/// gathered by the committed selection, so one `MatMul` with a batch of `k` computes every
/// expert's projection; per-(expert, row) weight scales are gathered the same way. The expert
/// outputs meet in ONE exact accumulator (`moe_combine_q36`: `w[1,k] × y[k,D]`), narrowed once.
#[allow(clippy::too_many_arguments)]
fn lower_moe(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    idx: &Val,
    w: &Val,
    ps: &[u32],
    k: usize,
    act: Act,
    glu: crate::spec::Glu,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let (gp, upp, dp) = (ps[0], ps[1], ps[2]);
    let (e, i, d) = (hl.params[gp as usize].shape[0], hl.params[gp as usize].shape[1], hl.params[gp as usize].shape[2]);
    let pl = per_layer(lb);
    let sub = |s: &str| format!("{site}.{s}");
    let kx = x.key.clone();
    // One projection over the selected experts: `[E, rows, cols]` codes, `(m, s, z)` per
    // (expert, row), gathered by `idx`.
    let proj = |b: &mut BlockBuilder<'_>,
                cx: &mut Cx<'_>,
                lb: &mut Lb,
                p: u32,
                bias: Option<u32>,
                input: tir::Ref,
                in_key: ScaleKey,
                name: &str,
                out: ScaleKey,
                dt: DType|
     -> Result<tir::Ref> {
        let pd = &hl.params[p as usize];
        let (rows, cols) = (pd.shape[1], pd.shape[2]);
        let codes = decl(b, cx, lb, &pd.name, DType::I8, &[e, rows, cols], pd.per_layer, weight_codes(p))?;
        let (ki, ko) = (in_key.clone(), out.clone());
        let (m, s) = decl_ms(
            b,
            cx,
            lb,
            name,
            e * rows,
            Arc::new(move |c| {
                let rc = c.rows(p)?;
                let (si, so) = (c.scale(&ki)?, c.scale(&ko)?);
                Ok(rc.scales.iter().map(|sw| sw * si / so).collect())
            }),
        )?;
        let z = match bias {
            Some(bp) => {
                let ko = out.clone();
                let zp = decl(
                    b,
                    cx,
                    lb,
                    &format!("{name}.z"),
                    DType::I64,
                    &[e * rows],
                    pl,
                    Arc::new(move |c| {
                        let bv = c.f(bp)?;
                        let so = c.scale(&ko)?;
                        Ok(IntTensor::i64(vec![bv.data.len()], bv.data.iter().map(|v| (*v as f64 / so).round() as i64).collect()))
                    }),
                )?;
                let zp = b.reshape_fixed(zp, &[e as u32, rows as u32]);
                Some(b.gather(zp, idx.r, 0, 0))
            }
            None => None,
        };
        let sel = b.gather(codes, idx.r, 0, 0);
        let acc = b.matmul(sel, input, DType::I64);
        let acc = b.reshape_fixed(acc, &[k as u32, rows as u32]);
        let m = b.reshape_fixed(m, &[e as u32, rows as u32]);
        let s = b.reshape_fixed(s, &[e as u32, rows as u32]);
        let mk = b.gather(m, idx.r, 0, 0);
        let sk = b.gather(s, idx.r, 0, 0);
        Ok(narrow(b, acc, mk, sk, z, dt))
    };
    let (gb, ub, db) = if ps.len() > 3 { (Some(ps[3]), Some(ps[4]), Some(ps[5])) } else { (None, None, None) };
    let xb = b.reshape_fixed(x.r, &[d as u32, 1]);
    let (gk, uk) = (ScaleKey::site(vec![sub("gate")], false), ScaleKey::site(vec![sub("up")], false));
    let g16 = proj(b, cx, lb, gp, gb, xb, kx.clone(), &sub("gate"), gk.clone(), DType::I16)?;
    b.commit(g16);
    let u16_ = proj(b, cx, lb, upp, ub, xb, kx.clone(), &sub("up"), uk.clone(), DType::I16)?;
    b.commit(u16_);
    let gv = Val { r: g16, dt: DType::I16, key: gk.clone(), len: k * i, site: sub("gate") };
    let (a16, u16_, uk) = match glu {
        crate::spec::Glu::Standard => (lower_table_named(b, cx, lb, &gv, TableFn::Act(act), &sub("act"))?, u16_, uk),
        // gpt-oss: two single-input tables — `|g·σ(αg)| ≤ |g|` keeps the gate's scale, and the
        // clamped up half lies in `[1 − l, 1 + l]`.
        crate::spec::Glu::ClampedSwiGlu { alpha, limit } => {
            let a = lower_table_keyed(b, cx, lb, &gv, TableFn::ClampedGlu { alpha, limit }, &sub("glu"), gk.clone())?;
            let uv = Val { r: u16_, dt: DType::I16, key: uk.clone(), len: k * i, site: sub("up") };
            let ukey = ScaleKey { base: Base::Fixed((limit + 1.0) * 2.0 / crate::quant::CODE16_MAX), factor: 1.0 };
            let u = lower_table_keyed(b, cx, lb, &uv, TableFn::ClampedUp { limit }, &sub("up1"), ukey.clone())?;
            (a, u.r, ukey)
        }
    };
    let hk = ScaleKey::site(vec![sub("hidden")], false);
    let p = b.mul(a16.r, u16_, DType::I32);
    let (ka, ku, kh) = (a16.key.clone(), uk.clone(), hk.clone());
    let (m, s) = decl_ms(b, cx, lb, &sub("hidden"), 1, Arc::new(move |c| Ok(vec![c.scale(&ka)? * c.scale(&ku)? / c.scale(&kh)?])))?;
    let hid = narrow(b, p, m, s, None, DType::I16);
    b.commit(hid);
    let hb = b.reshape_fixed(hid, &[k as u32, i as u32, 1]);
    let ok = ScaleKey::site(vec![sub("out")], true);
    let y = proj(b, cx, lb, dp, db, hb, hk, &sub("out"), ok.clone(), DType::I32)?;
    b.commit(y);
    // Σ_j w_j · y_j in one accumulator, narrowed once to the wanted scale.
    let (kw, ko2, kt) = (w.key.clone(), ok, want.key.clone());
    let (m, s) = decl_ms(b, cx, lb, site, 1, Arc::new(move |c| Ok(vec![c.scale(&kw)? * c.scale(&ko2)? / c.scale(&kt)?])))?;
    let (lo, hi) = code_bounds(want.dt);
    let p2 = b.pow2_of(s);
    let z = b.c(DType::I64, 0);
    let r = b.moe_combine_q36(y, w.r, m, p2, z, lo, hi, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: d, site: site.to_string() })
}

/// `softmax_shifted` over the last axis with one extra logit per row that joins the maximum and
/// the sum and is then dropped (gpt-oss's sinks) — the template's steps and roundings, one term
/// more: `m = max(max_j x_j, s)`, `e_j = IntExp(clamp(x_j − m) · 2^up)`,
/// `p_j = (e_j · IntRecip(Σe + IntExp(clamp(s − m) · 2^up))) >> 24`.
fn softmax_with_sink(b: &mut BlockBuilder<'_>, x: tir::Ref, sink: tir::Ref, up: u32) -> tir::Ref {
    use tir::Cmp;
    let axis = b.shape(x).len() - 1;
    let mx = b.reduce_max(x, axis);
    let gt = b.compare(mx, sink, Cmp::Gt);
    let m = b.select(gt, mx, sink, DType::I32);
    let floor = (i32::MIN as i64) >> up;
    let scale = b.c(DType::I64, 1i128 << up);
    let arg = |b: &mut BlockBuilder<'_>, v: tir::Ref| {
        let d = b.sub(v, m, DType::I64);
        let d = b.clamp(d, floor, 0, DType::I64);
        let w = b.mul(d, scale, DType::I64);
        b.clamp(w, i32::MIN as i64, 0, DType::I32)
    };
    let ax = arg(b, x);
    let e = b.int_exp(ax);
    let sa = arg(b, sink);
    let es = b.int_exp(sa);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let sum = b.add(sum, es, DType::I64);
    let recip = b.int_recip(sum);
    let p = b.mul(e, recip, DType::I128);
    let q = b.shr(p, 24, Rounding::Floor, DType::I64);
    b.clamp(q, 0, 1 << 25, DType::I32)
}

/// Where a gated-delta node's decay and beta come from: `g = A · softplus(a + dt_bias)` and
/// `β = σ(b)`, matched in the HL graph and computed by the library's `decay_q36` and
/// `int_sigmoid` on the Q24 projections instead of node by node.
#[derive(Clone, Debug)]
struct GdnInputs {
    a: u32,
    dt_bias: u32,
    a_log_neg: u32,
    b: u32,
}

/// Match every GatedDelta's decay/beta chains; the chain nodes are absorbed and the two
/// projections feeding them narrow to Q24.
fn gdn_patterns(hl: &HlProgram, blk: &hl::Block, lb: &mut Lb) -> Result<()> {
    let mut consumers = vec![0usize; blk.nodes.len()];
    for n in &blk.nodes {
        for r in &n.inputs {
            if let hl::Ref::Node(j, _) = r {
                consumers[*j as usize] += 1;
            }
        }
    }
    let node_of = |r: hl::Ref| if let hl::Ref::Node(j, 0) = r { Some(j as usize) } else { None };
    for (i, n) in blk.nodes.iter().enumerate() {
        if !matches!(n.op, Op::GatedDelta { .. }) {
            continue;
        }
        let bad = || LowerError::not_lowerable("a gated delta whose decay is not A·softplus(a + dt_bias) and beta σ(b)");
        let gl = node_of(n.inputs[3]).ok_or_else(bad)?;
        let (Op::Mul, [sp, hl::Ref::Param(a_neg)]) = (&blk.nodes[gl].op, blk.nodes[gl].inputs.as_slice()) else { return Err(bad()) };
        let sp = node_of(*sp).ok_or_else(bad)?;
        let (Op::Act(Act::Softplus), [t]) = (&blk.nodes[sp].op, blk.nodes[sp].inputs.as_slice()) else { return Err(bad()) };
        let t = node_of(*t).ok_or_else(bad)?;
        let (Op::Add, [a, hl::Ref::Param(dtb)]) = (&blk.nodes[t].op, blk.nodes[t].inputs.as_slice()) else { return Err(bad()) };
        let a = node_of(*a).ok_or_else(bad)?;
        let be = node_of(n.inputs[4]).ok_or_else(bad)?;
        let (Op::Act(Act::Sigmoid), [bn]) = (&blk.nodes[be].op, blk.nodes[be].inputs.as_slice()) else { return Err(bad()) };
        let bn = node_of(*bn).ok_or_else(bad)?;
        for j in [gl, sp, t, be] {
            if consumers[j] != 1 {
                return Err(bad());
            }
            lb.absorbed[j] = true;
        }
        for j in [a, bn] {
            if !matches!(blk.nodes[j].op, Op::Linear { .. }) || consumers[j] != 1 {
                return Err(bad());
            }
            lb.wants[j] = Some(Want { dt: DType::I32, key: ScaleKey::q24() });
        }
        let _ = hl;
        lb.gdn.insert(i, GdnInputs { a: a as u32, dt_bias: *dtb, a_log_neg: *a_neg, b: bn as u32 });
    }
    Ok(())
}

/// Projections concatenated into one row share one scale (the max over their sites), so the
/// concat needs no change of scale.
fn concat_wants(blk: &hl::Block, lb: &mut Lb) {
    for n in &blk.nodes {
        if !matches!(n.op, Op::Concat) {
            continue;
        }
        let parts: Vec<usize> =
            n.inputs.iter().filter_map(|r| if let hl::Ref::Node(j, 0) = r { Some(*j as usize) } else { None }).collect();
        if parts.len() != n.inputs.len()
            || parts.iter().any(|j| !matches!(blk.nodes[*j].op, Op::Linear { .. }) || lb.wants[*j].is_some())
        {
            continue;
        }
        let names: Vec<String> = parts.iter().filter_map(|j| blk.nodes[*j].site.clone()).collect();
        for j in parts {
            lb.wants[j] = Some(Want { dt: DType::I16, key: ScaleKey::site(names.clone(), false) });
        }
    }
}

/// Depthwise causal conv over the last `kernel` rows: `Fixed` state `[kernel − 1, C]` of codes,
/// `Concat(state, row)`, the new state its last `kernel − 1` rows, `Σ_t taps[c,t] · window[t,c]`
/// exactly, one per-channel narrowing to the pre-activation scale, then the activation table.
#[allow(clippy::too_many_arguments)]
fn lower_conv(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    i: usize,
    x: &Val,
    st: u32,
    w: u32,
    bias: Option<u32>,
    ch: usize,
    kernel: usize,
    act: Option<Act>,
    site: &str,
) -> Result<Val> {
    let hl = cx.hl;
    let sd = &hl.states[st as usize];
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I16, &[(kernel - 1) as u32, ch as u32], -32767, 32767, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    let row = b.reshape_fixed(x.r, &[1, ch as u32]);
    let row = b.commit(row);
    let win = b.concat(&[tir::Ref::State(ts), row], 0);
    let keep = b.slice(win, 0, 1, (kernel - 1) as u32);
    b.state_write(ts, keep);
    let pd = &hl.params[w as usize];
    let taps = decl(b, cx, lb, &pd.name, DType::I8, &[ch, kernel], pd.per_layer, weight_codes(w))?;
    let tt = b.transpose(taps, &[1, 0]);
    let prod = b.mul(win, tt, DType::I32);
    let acc = b.reduce_sum(prod, 0, DType::I64);
    let acc = b.reshape_fixed(acc, &[ch as u32]);
    let pre_key = ScaleKey::site(vec![format!("{site}.pre")], false);
    let (kx, kp) = (x.key.clone(), pre_key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.pre"),
        ch,
        Arc::new(move |c| {
            let rc = c.rows(w)?;
            let (sx, sp) = (c.scale(&kx)?, c.scale(&kp)?);
            Ok(rc.scales.iter().map(|sw| sw * sx / sp).collect())
        }),
    )?;
    let z = match bias {
        Some(bp) => {
            let kp = pre_key.clone();
            Some(decl(
                b,
                cx,
                lb,
                &format!("{site}.pre.z"),
                DType::I64,
                &[ch],
                per_layer(lb),
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let sp = c.scale(&kp)?;
                    Ok(IntTensor::i64(vec![ch], bv.data.iter().map(|v| (*v as f64 / sp).round() as i64).collect()))
                }),
            )?)
        }
        None => None,
    };
    let pre = narrow(b, acc, m, s, z, DType::I16);
    let pv = Val { r: pre, dt: DType::I16, key: pre_key, len: ch, site: format!("{site}.pre") };
    match act {
        Some(a) => lower_table(b, cx, lb, i, &pv, TableFn::Act(a), site),
        None => {
            b.commit(pre);
            Ok(Val { site: site.to_string(), ..pv })
        }
    }
}

struct GdnDims {
    nk: usize,
    nv: usize,
    dk: usize,
    dv: usize,
    map: crate::spec::HeadMap,
    q_scale: f64,
}

/// The gated delta rule, one position (library `gdn_step_q36`), from the HL pieces: q and k as
/// Q15 unit codes mapped to the value heads (grouping = `repeat_interleave`, tiling = modulo),
/// v codes, the decay `exp(A·softplus(a + dt_bias))` by `decay_q36`, beta by `int_sigmoid`, and
/// the state `S [heads, d_v, d_k]` in `i32` at a calibrated scale that is a power-of-two multiple
/// of the delta's (`write_shift`).
#[allow(clippy::too_many_arguments)]
fn lower_gdn(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    q: &Val,
    k: &Val,
    v: &Val,
    a: &Val,
    bb: &Val,
    gi: &GdnInputs,
    st: u32,
    dims: GdnDims,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let GdnDims { nk, nv, dk, dv, map, q_scale } = dims;
    let (nk32, nv32, dk32, dv32) = (nk as u32, nv as u32, dk as u32, dv as u32);
    let r = (nv / nk) as u32;
    let unit = ScaleKey { base: Base::Fixed(1.0 / 32768.0), factor: 1.0 };
    for (name, x) in [("q", q), ("k", k)] {
        if x.dt != DType::I16 || !x.key.same(&unit) {
            return Err(LowerError::not_lowerable(format!("gated delta `{name}` is not an L2-normalised row")));
        }
    }
    let to_v_heads = |b: &mut BlockBuilder<'_>, x: tir::Ref| -> tir::Ref {
        match map {
            crate::spec::HeadMap::Group => {
                let y = b.reshape_fixed(x, &[nk32, 1, dk32]);
                let y = b.broadcast(y, &[Dim::Fixed(nk32), Dim::Fixed(r), Dim::Fixed(dk32)]);
                b.reshape_fixed(y, &[nv32, dk32])
            }
            crate::spec::HeadMap::Tile => {
                let y = b.reshape_fixed(x, &[1, nk32, dk32]);
                let y = b.broadcast(y, &[Dim::Fixed(r), Dim::Fixed(nk32), Dim::Fixed(dk32)]);
                b.reshape_fixed(y, &[nv32, dk32])
            }
        }
    };
    let qh = to_v_heads(b, q.r);
    let kh = to_v_heads(b, k.r);
    let vh = b.reshape_fixed(v.r, &[nv32, dv32]);
    let pl = per_layer(lb);
    // decay = exp(−c · softplus(dt)), c = −A ≥ 0, dt = a + dt_bias, all Q24.
    let (dtb, an) = (gi.dt_bias, gi.a_log_neg);
    let dtp = decl(
        b,
        cx,
        lb,
        &format!("{site}.dt_bias"),
        DType::I32,
        &[nv],
        pl,
        Arc::new(move |c| Ok(IntTensor::i32(vec![nv], c.f(dtb)?.data.iter().map(|v| q24_wide(*v as f64)).collect()))),
    )?;
    let cp = decl(
        b,
        cx,
        lb,
        &format!("{site}.decay_c"),
        DType::I64,
        &[nv],
        pl,
        Arc::new(move |c| {
            Ok(IntTensor::i64(
                vec![nv],
                c.f(an)?.data.iter().map(|v| (-(*v as f64) * (1u64 << 24) as f64).round().max(0.0) as i64).collect(),
            ))
        }),
    )?;
    let dt = b.add(a.r, dtp, DType::I64);
    let dt = b.clamp(dt, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let decay = b.decay_q36(dt, cp);
    let decay = b.commit(decay);
    let beta = b.int_sigmoid(bb.r);
    let beta = b.commit(beta);
    let _ = hl;
    // The state and its scales: S at `s_s` (i32), the delta u at `s_u` (±2^24). The rank-one
    // write `u ⊗ k` is at `s_u · 2^−15` and is shifted LEFT by `ws` (right when negative) into the
    // state, so `s_s = s_u · 2^−15 / 2^ws` — `ws` the largest shift that keeps the calibrated state
    // inside i32 with the policy's headroom.
    let tsd = &hl.states[st as usize];
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&tsd.name, DType::I32, &[nv32, dv32, dk32], -(i32::MAX as i64), i32::MAX as i64, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    let (sn, dn) = (format!("{site}.state"), format!("{site}.delta"));
    let kv = v.key.clone();
    let scales = Arc::new(move |c: &FillCtx<'_>| -> Result<(f64, f64, i32, f64)> {
        let sv = c.scale(&kv)?;
        let su = crate::quant::code_scale(c.absmax(&dn)?, ((1u64 << 24) - 1) as f64, c.policy.headroom32);
        let target = crate::quant::code_scale(c.absmax(&sn)?, crate::quant::CODE32_MAX, c.policy.headroom32);
        let ws = (su / 32768.0 / target).log2().floor().clamp(-62.0, 20.0) as i32;
        let ss = su / 32768.0 / 2f64.powi(ws);
        Ok((sv, su, ws, ss))
    });
    let head_ms = |b: &mut BlockBuilder<'_>,
                   cx: &mut Cx<'_>,
                   lb: &mut Lb,
                   name: &str,
                   f: Arc<dyn Fn(&FillCtx<'_>) -> Result<f64> + Send + Sync>|
     -> Result<(tir::Ref, tir::Ref, tir::Ref)> {
        let (m, s) = decl_ms(b, cx, lb, name, nv, Arc::new(move |c| Ok(vec![f(c)?; nv])))?;
        let p2 = b.pow2_of(s);
        let z = b.c(DType::I64, 0);
        let z = b.broadcast(z, &[Dim::Fixed(nv32)]);
        Ok((m, p2, z))
    };
    let s1 = scales.clone();
    let read = head_ms(
        b,
        cx,
        lb,
        &format!("{site}.read"),
        Arc::new(move |c| {
            let (sv, _, _, ss) = s1(c)?;
            Ok(ss / 32768.0 / sv)
        }),
    )?;
    let s2 = scales.clone();
    let delta = head_ms(
        b,
        cx,
        lb,
        &format!("{site}.delta"),
        Arc::new(move |c| {
            let (sv, su, _, _) = s2(c)?;
            Ok(sv / su)
        }),
    )?;
    let (s3, ko) = (scales.clone(), want.key.clone());
    let out = head_ms(
        b,
        cx,
        lb,
        &format!("{site}.out"),
        Arc::new(move |c| {
            let (_, _, _, ss) = s3(c)?;
            Ok(ss / 32768.0 * q_scale / c.scale(&ko)?)
        }),
    )?;
    let s4 = scales.clone();
    let ws = decl(
        b,
        cx,
        lb,
        &format!("{site}.write_shift"),
        DType::I32,
        &[nv],
        pl,
        Arc::new(move |c| Ok(IntTensor::i32(vec![nv], vec![s4(c)?.2; nv]))),
    )?;
    let o = b.gdn_step_q36(ts, kh, vh, qh, decay, beta, read, delta, ws, out);
    let o = b.reshape_fixed(o, &[(nv * dv) as u32]);
    let o = b.commit(o);
    Ok(Val { r: o, dt: DType::I32, key: want.key.clone(), len: nv * dv, site: site.to_string() })
}

fn q24_wide(v: f64) -> i32 {
    (v * (1u64 << 24) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// `RMSNorm(x) · w · silu(z)` per group (Qwen3-Next's output norm): the wide RMS template on the
/// `i32` rows, the gate through a SiLU table, their product narrowed per channel with the gain.
#[allow(clippy::too_many_arguments)]
fn lower_gated_norm(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    z: &Val,
    gain: u32,
    eps: f64,
    groups: usize,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let n = x.len;
    let g = n / groups;
    let pl = per_layer(lb);
    let xr = b.reshape_fixed(x.r, &[groups as u32, g as u32]);
    let kx = x.key.clone();
    let eps_q = Arc::new(move |c: &FillCtx<'_>| -> Result<f64> {
        let sx = c.scale(&kx)?;
        Ok(eps * (1u64 << 24) as f64 / (sx * sx))
    });
    let _ = pl;
    let eps_p = decl_eps(b, cx, lb, site, eps_q)?;
    let u = rms_unit(b, xr, eps_p);
    let u = b.reshape_fixed(u, &[n as u32]);
    let gate = lower_table_named(b, cx, lb, z, TableFn::Act(Act::Silu), &format!("{site}.gate"))?;
    let p = b.mul(u, gate.r, DType::I64);
    let (kg, ky) = (gate.key.clone(), want.key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let w = c.f(gain)?.data.clone();
            let sg = c.scale(&kg)?;
            let sy = c.scale_vec(&ky, n)?;
            Ok((0..n).map(|i| w[if w.len() == n { i } else { i % w.len() }] as f64 * sg / (1u64 << 24) as f64 / sy[i]).collect())
        }),
    )?;
    let r = narrow(b, p, m, s, None, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
}

/// Occurrence prefixes in program order: `pre.`, `L0.` … `L{n−1}.`, `post.`.
pub fn occurrences(hl: &HlProgram) -> Vec<(usize, Option<usize>, String)> {
    let mut v = vec![(hl.pre, None, "pre.".to_string())];
    for (l, k) in hl.schedule.iter().enumerate() {
        v.push((*k as usize, Some(l), format!("L{l}.")));
    }
    v.push((hl.post, None, "post.".to_string()));
    v
}
