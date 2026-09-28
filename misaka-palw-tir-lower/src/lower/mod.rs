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
//! hidden) plus the mid-layer residual — the pattern of corpus §2.4. With these commit points
//! every cone of every lowered program fits the legacy court's terminal ceiling as `tir_admit_v1`
//! measures it (`tests/admission.rs`; the worst, Falcon-40B's, is 2.6 Mi MACs of 16 Mi), so no
//! commit point is added for cone size.

pub mod bidir;
pub mod encdec;
pub mod vision;
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
    /// A multimodal LM: image rows placed at the prompt's placeholder ids by a cursor
    /// ([`ImageRows`], RFC-0003 §II.2.1).
    pub image_rows: Option<ImageRows>,
    /// The longest history window any block keeps (`None`: the history bound, or the NF-8 cap).
    /// A class whose layout bounds its jobs below the history bound can keep a shorter window —
    /// the attention is the model's up to it — and its per-position cost scales with it.
    pub max_window: Option<u32>,
}

impl Default for LowerOpts {
    fn default() -> Self {
        Self { history_bound: tir::program::HISTORY_BOUND_V1_SMALL, max_window: None, image_rows: None }
    }
}

/// Image rows an LM reads in place of its token embedding (RFC-0003 §II.2.1's placement): at each
/// position whose token is `placeholder`, the next of `rows` rows of `width` (the LM's hidden size),
/// counted by a `Fixed` cursor over the stream (no job field). The rows arrive as the input
/// `input.image_rows` (`i32 [rows, width]`, bound to the vision stage's `Final` output), in that
/// stage's fixed point `unit` (`2^−q`). Once every row is placed, a placeholder id is an ordinary
/// token again (a generated one, as HF embeds it).
///
/// `mrope` (Qwen2-VL, Qwen2.5-VL): the image's merged grid `(h, w)`. The LM's M-RoPE positions then
/// follow `get_rope_index` for one image: an image token `i` sits at `(s, s + i / w, s + i % w)`
/// with `s` its first row's stream position, and every later token at `p − rows + max(h, w)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRows {
    pub rows: usize,
    pub width: usize,
    pub unit: f64,
    pub placeholder: u32,
    pub mrope: Option<(u32, u32)>,
}

/// The input param of [`ImageRows`], and its cursor state.
pub const IMAGE_ROWS_PARAM: &str = "input.image_rows";
pub const IMAGE_CURSOR_STATE: &str = "image.cursor";
/// The cursor's per-layer copy (M-RoPE only): every layer advances its own by the same rule.
pub const IMAGE_CURSOR_LAYER_STATE: &str = "image.cursor.layer";

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
    /// A power-of-two unit `2^−q` for an output in the class's fixed point (RFC-0003 §I.3.3): the
    /// smallest power of two at least the site's `i32` scale, `q` clamped to `[0, 31]`.
    Pow2Site { names: Vec<String> },
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
    /// HL blocks that did not fit NF-12's 512 nodes with every outlier split, and the reader limit
    /// they were lowered at instead (a split is kept only for values at most this many projections
    /// read) — empty when every block kept every split.
    pub budget_fallbacks: Vec<(String, usize)>,
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
    // A gather's table is its SECOND input (`[Token | Pos, table]`).
    let tables: std::collections::BTreeSet<u32> = hl
        .blocks
        .iter()
        .flat_map(|b| b.nodes.iter())
        .filter(|n| matches!(n.op, Op::Embedding | Op::PosEmbedding { .. }))
        .filter_map(|n| match n.inputs.get(1) {
            Some(hl::Ref::Param(p)) => Some(*p),
            _ => None,
        })
        .collect();
    let mut cx = Cx {
        hl,
        fills: Vec::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window: opts.max_window.unwrap_or(u32::MAX).max(1),
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared,
        tables,
        image_rows: opts.image_rows,
        image_cursor: None,
        image_cursor_layer: None,
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
    let mut budget_fallbacks = Vec::new();
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
            quiet_budget_hook();
            QUIET_BUDGET.with(|q| q.set(true));
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lower_block(&mut pb, &mut cx, hbk)));
            QUIET_BUDGET.with(|q| q.set(false));
            match r {
                Ok(v) => {
                    result = Some(v?);
                    if readers != usize::MAX {
                        budget_fallbacks.push((hl.blocks[hbk].name.clone(), readers));
                    }
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
    // `prim_set_id` is the builder's default, `misaka_palw_tir::prim::PRIM_SET_ID_V1` (NF-1).
    tir::validate::validate(&program)
        .map_err(|e| LowerError::eval(format!("internal: lowered program is not in normal form: {e}")))?;
    let fills: Vec<FillFn> = cx.fills;
    if fills.len() != program.params.len() {
        return Err(LowerError::eval("internal: fills do not match params"));
    }
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no logits scale"))?;
    Ok(Lowered { program, fills, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks })
}

thread_local! {
    /// Set while this thread tries a block against NF-12's node cap (the builder panics past it,
    /// and the lowering catches that to retry with fewer splits).
    static QUIET_BUDGET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A panic hook, installed once, that keeps the builder's "exceeds 512 nodes" panic quiet while
/// the lowering is catching it on this thread — a fallback is reported in
/// [`Lowered::budget_fallbacks`], not as a panic message on stderr. Every other panic, and that one
/// anywhere else, goes to the hook that was installed before.
fn quiet_budget_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let p = info.payload();
            let msg = p.downcast_ref::<String>().map(String::as_str).or_else(|| p.downcast_ref::<&str>().copied()).unwrap_or("");
            if QUIET_BUDGET.with(|q| q.get()) && msg.contains("exceeds") && msg.contains("nodes") {
                return;
            }
            prev(info)
        }));
    });
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
    /// [`LowerOpts::max_window`].
    max_window: u32,
    logits_key: Option<ScaleKey>,
    site_nodes: BTreeMap<(u8, u16), (String, ScaleKey, usize)>,
    /// HL params read by more than one node (a tied embedding): a projection over them keeps the
    /// plain per-row codes the other reader shares.
    shared: std::collections::BTreeSet<u32>,
    /// HL params gathered by row (embeddings, learned positions): stored as per-row `i16` codes,
    /// and so is a head that reads the same tensor.
    tables: std::collections::BTreeSet<u32>,
    /// [`LowerOpts::image_rows`].
    image_rows: Option<ImageRows>,
    /// The image cursor state, once the pre block declared it.
    image_cursor: Option<u16>,
    /// Its per-layer copy, which a layer block reads for M-RoPE (a layer block reads only per-layer
    /// states, NF-15), declared by the first block that needs it.
    image_cursor_layer: Option<u16>,
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

/// The window `W` a `Hist` state of HL block `hbk` is lowered with: the model's own (a sliding
/// window) or the history bound — capped so that every history the block appends to fits NF-8's
/// `2^28` elements at the worst case (`[W] ++ row`: a row of 1,024 lanes admits `2^18`, one of
/// 4,096 lanes `2^16`). Every `Hist` state a block appends to shares this window (spec 04b §2.2),
/// and the cap is a power of two so a canonical `H` chunk divides it. Up to `W` positions the
/// program is the model; a class whose layout's `max_context` stays within `W` never sees the cap.
pub fn hist_window_cap(row_lanes: usize) -> u32 {
    let most = (tir::types::MAX_ELEMENTS / row_lanes.max(1) as u64).max(1);
    1u32 << (63 - most.leading_zeros()).min(31)
}

fn hist_window(cx: &Cx<'_>, hbk: usize, window: Option<usize>) -> u32 {
    let hl = cx.hl;
    let widest = hl.blocks[hbk]
        .nodes
        .iter()
        .filter(|n| matches!(n.op, Op::HistAppend))
        .filter_map(|n| match n.inputs.get(1) {
            Some(hl::Ref::State(s)) => Some(hl.states[*s as usize].shape.iter().product::<usize>()),
            _ => None,
        })
        .max()
        .unwrap_or(1);
    window.map(|w| w as u32).unwrap_or(cx.history_bound).min(cx.history_bound).min(hist_window_cap(widest)).min(cx.max_window)
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
    /// For each scan node (Mamba, Mamba2): where its step size comes from.
    ssm: BTreeMap<usize, SsmDt>,
    /// Products carried on the `i32` rail into their projection ([`wide_products`]).
    wide: Vec<bool>,
    /// Projections read at per-row `i16` ([`scan_param_linears`]).
    w16: Vec<bool>,
    /// Set by the `Linear` dispatch for the node being lowered: its weights are per-row `i16`.
    w16_now: bool,
    /// The M-RoPE positions of this block, once computed (its per-layer cursor is written once).
    mrope_pos: Option<[tir::Ref; 3]>,
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
        ssm: BTreeMap::new(),
        wide: wide_products(blk),
        w16: scan_param_linears(blk),
        w16_now: false,
        mrope_pos: None,
        suffix,
    };
    gdn_patterns(hl, blk, &mut lb)?;
    ssm_patterns(blk, &mut lb)?;
    concat_wants(blk, &mut lb);
    wide_wants(blk, &mut lb);
    w16_inputs_unsplit(blk, &mut lb);
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
            let key = output_key(hl, &v.site);
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
        Want { dt: DType::I32, key: output_key(hl, &site) }
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

/// The key of `post`'s output node: the logits at their site's `i32` scale, or an encoder's
/// embedding in fixed point — `Q30` when L2-normalised (every lane is in `[−1, 1]`), else a
/// power-of-two unit sized on the calibration ([`Base::Pow2Site`]).
fn output_key(hl: &HlProgram, site: &str) -> ScaleKey {
    match hl.output {
        hl::HlOutput::Logits => ScaleKey::site(vec![site.to_string()], true),
        hl::HlOutput::Embedding { normalized: true } => ScaleKey { base: Base::Fixed(1.0 / (1u64 << 30) as f64), factor: 1.0 },
        hl::HlOutput::Embedding { normalized: false } => {
            ScaleKey { base: Base::Pow2Site { names: vec![site.to_string()] }, factor: 1.0 }
        }
    }
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
            let capable = matches!(
                blk.nodes[i].op,
                Op::Norm { .. }
                    | Op::Mul
                    | Op::Attention { .. }
                    | Op::Linear { .. }
                    | Op::GatedRmsNorm { .. }
                    | Op::MlaAttention { .. }
                    | Op::Lerp
                    | Op::Wkv4
            );
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

/// Headroom of a recurrence's output path on the `i32` rail, over the `i32` policy headroom. A
/// scan's output grows with the context it has integrated. Calibrated on shorter contexts, one
/// static 16-bit scale saturates. Mamba-370m's layer-29 gated product reached 4.5x its calibrated
/// absmax at position 3,569 of a 4,096-token document, drifting x5.6. The `i32` rail has the bits:
/// at x256 its unit is still 2^14 finer than the 16-bit code it replaces.
const WIDE_RECURRENT_HEADROOM: f64 = 256.0;

/// The products carried on the `i32` rail into their projection. A recurrence's output (a
/// selective scan) gated by an elementwise product, `y · silu(z)`, feeding projections only. The
/// product of the scan's `i32` value and the gate's code is exact in `i64`, is narrowed ONCE to
/// `i32`, and the projection reads it wide (`i8 × i32` into `i64`).
fn wide_products(blk: &hl::Block) -> Vec<bool> {
    let n = blk.nodes.len();
    let mut consumers: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (i, node) in blk.nodes.iter().enumerate() {
        for (k, r) in node.inputs.iter().enumerate() {
            if let hl::Ref::Node(j, _) = r {
                consumers[*j as usize].push((i, k));
            }
        }
    }
    let outputs: std::collections::BTreeSet<usize> =
        blk.outputs.iter().filter_map(|o| if let hl::Ref::Node(j, _) = o { Some(*j as usize) } else { None }).collect();
    (0..n)
        .map(|i| {
            let node = &blk.nodes[i];
            let from_scan = matches!(node.inputs.first(), Some(hl::Ref::Node(j, 0))
                if matches!(blk.nodes[*j as usize].op, Op::SelectiveScan { .. }) && consumers[*j as usize].len() == 1);
            matches!(node.op, Op::Mul)
                && from_scan
                && !outputs.contains(&i)
                && !consumers[i].is_empty()
                && consumers[i].iter().all(|(c, k)| *k == 0 && matches!(blk.nodes[*c].op, Op::Linear { .. }))
        })
        .collect()
}

/// The projections that set a selective scan's step size and `B`/`C` (Mamba's `x_proj` parts and
/// `dt_proj`, through an optional norm). Their weights are read at per-row `i16`. At `i8` their
/// rounding is input-correlated: it biases the step size, so each channel's decay rate is slightly
/// off, and that compounds over a long context. In float, Mamba-370m's `x_proj` alone at per-row
/// `i8` drifts x5.8 over 4,096 positions; `x_proj` and `dt_proj` at 16 bits give x1.10. The cost is
/// small: these matrices are thin (15.7 MB at 16 bits on Mamba-370m).
fn scan_param_linears(blk: &hl::Block) -> Vec<bool> {
    let mut out = vec![false; blk.nodes.len()];
    let node_of = |r: hl::Ref| if let hl::Ref::Node(j, 0) = r { Some(j as usize) } else { None };
    let linear_through_norm = |j: usize| -> Option<usize> {
        match blk.nodes[j].op {
            Op::Linear { .. } => Some(j),
            Op::Norm { .. } => node_of(blk.nodes[j].inputs[0]).filter(|k| matches!(blk.nodes[*k].op, Op::Linear { .. })),
            _ => None,
        }
    };
    for n in &blk.nodes {
        if !matches!(n.op, Op::SelectiveScan { .. }) {
            continue;
        }
        // The step size: [Clamp] → Softplus → [+ bias] → dt_proj → [norm] → x_proj's dt rows.
        let mut at = node_of(n.inputs[1]);
        while let Some(j) = at {
            match blk.nodes[j].op {
                Op::Clamp { .. } | Op::Act(Act::Softplus) | Op::Add => at = node_of(blk.nodes[j].inputs[0]),
                Op::Linear { .. } => {
                    out[j] = true;
                    if let Some(k) = node_of(blk.nodes[j].inputs[0]).and_then(linear_through_norm) {
                        out[k] = true;
                    }
                    break;
                }
                _ => break,
            }
        }
        for r in [n.inputs[2], n.inputs[3]] {
            if let Some(k) = node_of(r).and_then(linear_through_norm) {
                out[k] = true;
            }
        }
    }
    out
}

/// An `i16`-weight projection reads its input unsplit: the outlier split's high-precision columns
/// exist for `i8` weights (a value read only by such projections keeps one scale).
fn w16_inputs_unsplit(blk: &hl::Block, lb: &mut Lb) {
    let mut readers: Vec<Vec<usize>> = vec![Vec::new(); blk.nodes.len()];
    for (i, n) in blk.nodes.iter().enumerate() {
        for r in &n.inputs {
            if let hl::Ref::Node(j, _) = r {
                readers[*j as usize].push(i);
            }
        }
    }
    for (j, rd) in readers.iter().enumerate() {
        if lb.split[j] > 0 && !rd.is_empty() && rd.iter().all(|c| lb.w16[*c]) {
            lb.split[j] = 0;
        }
    }
}

/// The wide rail's key for site `site`.
fn wide_key(site: &str) -> ScaleKey {
    ScaleKey::site(vec![site.to_string()], true).times(WIDE_RECURRENT_HEADROOM)
}

/// A wide product's scan delivers its output on the `i32` rail too.
fn wide_wants(blk: &hl::Block, lb: &mut Lb) {
    for i in 0..blk.nodes.len() {
        if !lb.wide[i] {
            continue;
        }
        lb.split[i] = 0;
        if let Some(hl::Ref::Node(j, 0)) = blk.nodes[i].inputs.first() {
            let j = *j as usize;
            let site = blk.nodes[j].site.clone().unwrap_or_default();
            lb.wants[j] = Some(Want { dt: DType::I32, key: wide_key(&site) });
        }
    }
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

/// The narrowing `N(x; m, 2^s, z)` into `(dt, lo..hi)` — the library's `narrow` (three nodes past
/// the `Pow2` gather when there is no zero term).
fn narrow(b: &mut BlockBuilder<'_>, x: tir::Ref, m: tir::Ref, s: tir::Ref, z: Option<tir::Ref>, dt: DType) -> tir::Ref {
    let (lo, hi) = code_bounds(dt);
    b.narrow(x, &tir::library::Narrowing::new(m, s, z), lo, hi, dt)
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
            match cx.image_rows {
                Some(img) => one(inject_image_rows(b, cx, lb, v, img)?),
                None => one(v),
            }
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
        Op::Linear { bias, lora } => {
            let x = operand(lb, node.inputs[0])?;
            let wide_in = matches!(node.inputs[0], hl::Ref::Node(j, 0) if lb.wide[j as usize]);
            let x = if wide_in { x } else { codes(b, cx, lb, &x)? };
            let w = pidx(node.inputs[1])?;
            let bp = if *bias { Some(pidx(node.inputs[2])?) } else { None };
            lb.w16_now = lb.w16[i];
            let v = lower_linear(b, cx, lb, &x, w, bp, &site, &want);
            lb.w16_now = false;
            let v = v?;
            match lora {
                Some(l) => {
                    let at = if *bias { 3 } else { 2 };
                    let (ap, bpp) = (pidx(node.inputs[at])?, pidx(node.inputs[at + 1])?);
                    one(lower_lora(b, cx, lb, &x, v, *l, ap, bpp, &site, &want)?)
                }
                None => one(v),
            }
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
        Op::Mul if lb.wide[i] => {
            // `y · g`: the scan's `i32` value times the gate's code, exact in `i64`, narrowed once.
            let a = operand(lb, node.inputs[0])?;
            let c = operand(lb, node.inputs[1])?;
            let c = codes(b, cx, lb, &c)?;
            if a.dt != DType::I32 {
                return Err(LowerError::eval(format!("internal: the wide product `{site}` reads a {:?} scan", a.dt)));
            }
            let p = b.mul(a.r, c.r, DType::I64);
            let key = wide_key(&site);
            let (ka, kc, ko) = (a.key.clone(), c.key.clone(), key.clone());
            let (m, s) = decl_ms(b, cx, lb, &site, 1, Arc::new(move |f| Ok(vec![f.scale(&ka)? * f.scale(&kc)? / f.scale(&ko)?])))?;
            let r = narrow(b, p, m, s, None, DType::I32);
            b.commit(r);
            one(Val { r, dt: DType::I32, key, len: out_len, site })
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
            let sel_bias = if node.inputs.len() > 1 { Some(pidx(node.inputs[1])?) } else { None };
            let (idx, w) = lower_route(b, cx, lb, &l, sel_bias, router, *experts, *top_k, &site)?;
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
            let w = hist_window(cx, lb.hb, window);
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
                StateKind::Hist { window } => hist_window(cx, lb.hb, window),
                StateKind::Fixed => return Err(LowerError::eval("internal: attention over a Fixed state")),
            };
            let shape = AttnDims { heads: *heads, kv: *kv_heads, d: *head_dim, dv: *v_head_dim, window };
            let extra = AttnExtras { scale: *scale, softcap: *softcap, alibi: alibi.clone(), sinks: sink_param, rel_bias: None };
            one(lower_attention(b, cx, lb, &q, (kw, kk), (vw, vk), shape, &extra, &site, &want)?)
        }
        Op::MlaAttention { heads, nope, rope, v_dim, kv_lora, scale } => {
            let q = operand(lb, node.inputs[0])?;
            let q = codes(b, cx, lb, &q)?;
            let (hl::Ref::State(ls), hl::Ref::State(rs)) = (node.inputs[1], node.inputs[2]) else {
                return Err(LowerError::eval("internal: MLA without states"));
            };
            let lat = lb.windows.get(&ls).cloned().ok_or_else(|| LowerError::eval("internal: latent read before its append"))?;
            let kr = lb.windows.get(&rs).cloned().ok_or_else(|| LowerError::eval("internal: rope key read before its append"))?;
            let kvb = pidx(node.inputs[3])?;
            let dims = MlaDims { heads: *heads, nope: *nope, rope: *rope, vd: *v_dim, r: *kv_lora };
            let want =
                if want.dt == DType::I16 { want } else { Want { dt: DType::I16, key: ScaleKey::site(vec![site.clone()], false) } };
            one(lower_mla(b, cx, lb, &q, lat, kr, kvb, dims, *scale, &site, &want)?)
        }
        Op::TokenShift => {
            // The previous position's row (zeros at position 0, as the state starts), and this
            // row becomes the state: a `Fixed` code row read before it is written.
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: token shift without a state")) };
            let sd = &hl.states[st as usize];
            let ts = match cx.tstate.get(&st) {
                Some(t) => *t,
                None => {
                    let t = b.pb.fixed_state(&sd.name, DType::I16, &u32s(&sd.shape), -32767, 32767, true);
                    cx.tstate.insert(st, t);
                    t
                }
            };
            let row = ensure_node(b, &x);
            let row = Val { r: b.commit(row.r), ..row };
            b.state_write(ts, row.r);
            let prev = b.clamp(tir::Ref::State(ts), -32767, 32767, DType::I16);
            one(Val { r: prev, ..x })
        }
        Op::Lerp => {
            // `a + (b − a)·t` = `a·(1 − t) + b·t`, `t` a per-channel param in Q24.
            let a = operand(lb, node.inputs[0])?;
            let c = operand(lb, node.inputs[1])?;
            let a = codes(b, cx, lb, &a)?;
            let c = coerce(b, cx, lb, &c, DType::I16, &a.key)?;
            let tp = pidx(node.inputs[2])?;
            let n = out_len;
            let t = decl(
                b,
                cx,
                lb,
                &format!("{site}.t"),
                DType::I32,
                &[n],
                per_layer(lb),
                Arc::new(move |f| Ok(IntTensor::i32(vec![n], f.f(tp)?.data.iter().map(|v| q24_wide(*v as f64)).collect()))),
            )?;
            let one_ = b.c(DType::I64, 1 << 24);
            let omt = b.sub(one_, t, DType::I64);
            let pa = b.mul(a.r, omt, DType::I64);
            let pc = b.mul(c.r, t, DType::I64);
            let sum = b.add(pa, pc, DType::I64);
            let (ka, ko) = (a.key.clone(), want.key.clone());
            let per = if ko.split() > 0 { n } else { 1 };
            let (m, s) = decl_ms(
                b,
                cx,
                lb,
                &site,
                per,
                Arc::new(move |f| {
                    let sa = f.scale(&ka)? / (1u64 << 24) as f64;
                    Ok(f.scale_vec(&ko, per)?.iter().map(|so| sa / so).collect())
                }),
            )?;
            let r = narrow(b, sum, m, s, None, want.dt);
            b.commit(r);
            one(Val { r, dt: want.dt, key: want.key.clone(), len: n, site })
        }
        Op::Wkv4 => {
            let k = operand(lb, node.inputs[0])?;
            let k = codes(b, cx, lb, &k)?;
            let v = operand(lb, node.inputs[1])?;
            let v = codes(b, cx, lb, &v)?;
            let (wp, up) = (pidx(node.inputs[2])?, pidx(node.inputs[3])?);
            let sts: Vec<u32> = node.inputs[4..7]
                .iter()
                .map(|r| if let hl::Ref::State(s) = r { Ok(*s) } else { Err(LowerError::eval("internal: WKV without states")) })
                .collect::<Result<_>>()?;
            one(lower_wkv4(b, cx, lb, &k, &v, wp, up, &sts, &site, &want)?)
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
            let x = operand(lb, node.inputs[0])?;
            let z = operand(lb, node.inputs[1])?;
            let z = codes(b, cx, lb, &z)?;
            let gain = pidx(node.inputs[2])?;
            let v = if *gate_first {
                let x = codes(b, cx, lb, &x)?;
                lower_gated_norm_first(b, cx, lb, &x, &z, gain, *eps, *groups, &site, &want)?
            } else {
                lower_gated_norm(b, cx, lb, &x, &z, gain, *eps, *groups, &site, &want)?
            };
            note_resid(cx, lb, &v);
            one(v)
        }
        Op::SelectiveScan { inner, state } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let bb = operand(lb, node.inputs[2])?;
            let bb = codes(b, cx, lb, &bb)?;
            let cc = operand(lb, node.inputs[3])?;
            let cc = codes(b, cx, lb, &cc)?;
            let (ap, dp) = (pidx(node.inputs[4])?, pidx(node.inputs[5])?);
            let hl::Ref::State(st) = node.inputs[6] else { return Err(LowerError::eval("internal: scan without a state")) };
            let dt = lb.ssm.get(&i).cloned().ok_or_else(|| LowerError::eval("internal: scan step size not matched"))?;
            let dims = SsmDims { heads: *inner, p: 1, groups: 1, n: *state, per_state_decay: true };
            one(lower_ssm(b, cx, lb, &x, &bb, &cc, ap, dp, st, &dt, dims, &site, &want)?)
        }
        Op::Ssd { heads, head_dim, groups, state } => {
            let x = operand(lb, node.inputs[0])?;
            let x = codes(b, cx, lb, &x)?;
            let bb = operand(lb, node.inputs[2])?;
            let bb = codes(b, cx, lb, &bb)?;
            let cc = operand(lb, node.inputs[3])?;
            let cc = codes(b, cx, lb, &cc)?;
            let (ap, dp) = (pidx(node.inputs[4])?, pidx(node.inputs[5])?);
            let hl::Ref::State(st) = node.inputs[6] else { return Err(LowerError::eval("internal: scan without a state")) };
            let dt = lb.ssm.get(&i).cloned().ok_or_else(|| LowerError::eval("internal: scan step size not matched"))?;
            let dims = SsmDims { heads: *heads, p: *head_dim, groups: *groups, n: *state, per_state_decay: false };
            one(lower_ssm(b, cx, lb, &x, &bb, &cc, ap, dp, st, &dt, dims, &site, &want)?)
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
    let table = decl(b, cx, lb, &d.name, DType::I16, &[rows, cols], d.per_layer, table_codes(tp))?;
    let key = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        rows,
        Arc::new(move |c| {
            let rc = c.rows16(tp)?;
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

/// RFC-0003 §II.2.1's placement ([`ImageRows`]): at a placeholder position, while rows remain, the
/// LM reads row `cursor` — narrowed from the vision stage's fixed point to the residual scale — in
/// place of its token embedding, and the cursor advances. The embedding must be the value the pre
/// block carries out (residual scale).
fn inject_image_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: Val, img: ImageRows) -> Result<Val> {
    if !v.key.same(&ScaleKey::resid()) || v.dt != DType::I32 || v.len != img.width {
        return Err(LowerError::not_lowerable("image rows into a pre block that does more than look up the token embedding"));
    }
    let (n, w) = (img.rows, img.width);
    let rows = decl(b, cx, lb, IMAGE_ROWS_PARAM, DType::I32, &[n, w], false, bidir::input_fill(DType::I32, vec![n, w]))?;
    let cursor = b.pb.fixed_state(IMAGE_CURSOR_STATE, DType::I32, &[1], 0, n as i64, false);
    cx.image_cursor = Some(cursor);
    let c = tir::Ref::State(cursor);
    let is_img = image_token(b, c, img);
    let at = b.clamp(c, 0, n as i64 - 1, DType::Idx);
    let row = b.gather(rows, at, 0, 0);
    let row = b.reshape_fixed(row, &[w as u32]);
    let unit = img.unit;
    let (m, s) = decl_ms(b, cx, lb, "image_rows", 1, Arc::new(move |c| Ok(vec![unit / c.scale(&ScaleKey::resid())?])))?;
    let row = narrow(b, row, m, s, None, DType::I32);
    let r = b.select(is_img, row, v.r, DType::I32);
    b.commit(r);
    let step = b.cast(is_img, DType::I32);
    let next = b.add(c, step, DType::I32);
    let next = b.clamp(next, 0, n as i64, DType::I32);
    b.state_write(cursor, next);
    Ok(Val { r, ..v })
}

/// `1` when this position's token is an image placeholder with rows left (`[1]`, `i8`).
fn image_token(b: &mut BlockBuilder<'_>, cursor: tir::Ref, img: ImageRows) -> tir::Ref {
    let ph = b.c(DType::Idx, img.placeholder as i128);
    let is_ph = b.compare(tir::Ref::Input(INPUT_TOKEN), ph, tir::Cmp::Eq);
    let nn = b.c(DType::I32, img.rows as i128);
    let room = b.compare(cursor, nn, tir::Cmp::Lt);
    let zero = b.c(DType::I8, 0);
    b.select(is_ph, room, zero, DType::I8)
}

/// Qwen2-VL's M-RoPE positions `(t, h, w)` at this position, from the stream position and the image
/// cursor as this step starts (the image rows placed before this position). A layer block reads its
/// own per-layer copy of the cursor and advances it by the pre block's rule, so every layer's copy
/// equals the pre block's. Each position is an `idx [1]` for the rotary tables.
fn mrope_positions(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, img: ImageRows) -> Result<[tir::Ref; 3]> {
    if let Some(p) = lb.mrope_pos {
        return Ok(p);
    }
    let (gh, gw) = img.mrope.ok_or_else(|| LowerError::eval("internal: M-RoPE without a grid"))?;
    if cx.image_cursor.is_none() {
        return Err(LowerError::eval("internal: M-RoPE before the pre block placed the image"));
    }
    let n = img.rows as i64;
    let cursor = match (lb.role, cx.image_cursor_layer) {
        (BlockRole::Layer, Some(k)) => k,
        (BlockRole::Layer, None) => {
            let k = b.pb.fixed_state(IMAGE_CURSOR_LAYER_STATE, DType::I32, &[1], 0, n, true);
            cx.image_cursor_layer = Some(k);
            k
        }
        _ => return Err(LowerError::not_lowerable("M-RoPE outside a layer block")),
    };
    let c = tir::Ref::State(cursor);
    let is_img = image_token(b, c, img);
    let step = b.cast(is_img, DType::I32);
    let next = b.add(c, step, DType::I32);
    let next = b.clamp(next, 0, n, DType::I32);
    b.state_write(cursor, next);
    let p = b.cast(tir::Ref::Input(INPUT_POS), DType::I64);
    // An image token `i = cursor`: `s = p − i`, rows `i / w`, columns `i % w`.
    let s = b.sub(p, c, DType::I64);
    let wc = b.c(DType::I64, gw as i128);
    let row = b.div(c, wc, Rounding::Floor, DType::I64);
    let rw = b.mul(row, wc, DType::I64);
    let col = b.sub(c, rw, DType::I64);
    let ih = b.add(s, row, DType::I64);
    let iw = b.add(s, col, DType::I64);
    // Any other token: `p`, less `rows − max(h, w)` once the image is complete.
    let nc = b.c(DType::I32, n as i128);
    let done = b.compare(c, nc, tir::Cmp::Ge);
    let shift = b.c(DType::I64, n as i128 - gh.max(gw) as i128);
    let zero = b.c(DType::I64, 0);
    let off = b.select(done, shift, zero, DType::I64);
    let tp = b.sub(p, off, DType::I64);
    let hb = cx.history_bound as i64 - 1;
    let mut out = [tir::Ref::Input(INPUT_POS); 3];
    for (k, img_pos) in [s, ih, iw].into_iter().enumerate() {
        let v = b.select(is_img, img_pos, tp, DType::I64);
        out[k] = b.clamp(v, 0, hb, DType::Idx);
    }
    lb.mrope_pos = Some(out);
    Ok(out)
}

/// The names of a LoRA path's params all carry this marker ([`adapter_params_last`] moves them
/// behind the parent's).
pub const LORA_MARK: &str = ".lora_";

/// **An unmerged LoRA path** added to a projection's lowered output `base` (RFC-0004):
/// `y = base + N(⌊(B·N(A·x))·num / den⌉)`, where `num/den` is `alpha/r` exactly.
/// * `A·x`: `x`'s codes carry per-channel scales when `x` is split, so `A` is stored with those
///   scales folded into its columns, `A'[j,c] = A[j,c]·s_x[c]/s_x`, at per-row `i32` codes. That is
///   one exact `i32 × i16` product over the same codes the parent projection reads. It is narrowed
///   to `i16` codes at its calibrated site `{site}.lora_a`.
/// * `B·a`: per-row `i16` codes, exact in `i64`, then the rational scale as an integer `Mul` and a
///   rounded `Div`, then one narrowing into the projection's own output scale.
///
/// The parent's params are declared by `lower_linear` exactly as without the adapter. The adapter
/// adds params of its own, and no change of scale that would renumber a parent param.
#[allow(clippy::too_many_arguments)]
fn lower_lora(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    base: Val,
    l: hl::LoraOp,
    ap: u32,
    bp: u32,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let (r, inp) = (hl.params[ap as usize].shape[0], hl.params[ap as usize].shape[1]);
    let out = hl.params[bp as usize].shape[0];
    if r != l.rank || hl.params[bp as usize].shape[1] != r || (x.len != 0 && x.len != inp) || base.len != out {
        return Err(LowerError::eval(format!("internal: LoRA `{site}`: A {:?}, B {:?}, x {}", hl.params[ap as usize].shape, hl.params[bp as usize].shape, x.len)));
    }
    let pl = per_layer(lb);
    let kx = x.key.clone();
    // A' (the per-channel activation scales folded in), per-row i32 codes and row scales.
    let a_prime = move |c: &FillCtx<'_>, kx: &ScaleKey| -> Result<(Vec<i32>, Vec<f64>)> {
        let a = c.f(ap)?;
        let sv = c.scale_vec(kx, inp)?;
        let s0 = c.scale(kx)?;
        let mut codes = Vec::with_capacity(r * inp);
        let mut scales = Vec::with_capacity(r);
        for j in 0..r {
            let row: Vec<f64> = (0..inp).map(|ci| a.data[j * inp + ci] as f64 * sv[ci] / s0).collect();
            let mx = row.iter().fold(0f64, |m, v| m.max(v.abs()));
            let s = if mx > 0.0 { mx / i32::MAX as f64 } else { 1.0 };
            scales.push(s);
            codes.extend(row.iter().map(|v| (v / s).round().clamp(-(i32::MAX as f64), i32::MAX as f64) as i32));
        }
        Ok((codes, scales))
    };
    let a_prime = Arc::new(a_prime);
    let (fa, fm) = (a_prime.clone(), a_prime);
    let (k1, k2) = (kx.clone(), kx.clone());
    let aw = decl(
        b,
        cx,
        lb,
        &format!("{site}.lora_a.w"),
        DType::I32,
        &[r, inp],
        pl,
        Arc::new(move |c| Ok(IntTensor::i32(vec![r, inp], fa(c, &k1)?.0))),
    )?;
    let ka = ScaleKey::site(vec![format!("{site}.lora_a")], false);
    let ka1 = ka.clone();
    let (ma, sa) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.lora_a"),
        r,
        Arc::new(move |c| {
            let (_, rs) = fm(c, &k2)?;
            let (s0, s_a) = (c.scale(&k2)?, c.scale(&ka1)?);
            Ok(rs.iter().map(|rsj| rsj * s0 / s_a).collect())
        }),
    )?;
    let xc = b.reshape_fixed(x.r, &[inp as u32, 1]);
    let acc = b.matmul(aw, xc, DType::I64);
    let acc = b.reshape_fixed(acc, &[r as u32]);
    let a = narrow(b, acc, ma, sa, None, DType::I16);
    let a = b.commit(a);
    cx.site_nodes.entry((b.pb.blocks.len() as u8, match a {
        tir::Ref::Node(n) => n,
        _ => 0,
    }))
    .or_insert((format!("{site}.lora_a"), ka.clone(), r));
    // B·a, per-row i16, then the exact rational scale.
    let bw = decl(b, cx, lb, &format!("{site}.lora_b.w"), DType::I16, &[out, r], pl, table_codes(bp))?;
    let ac = b.reshape_fixed(a, &[r as u32, 1]);
    let bacc = b.matmul(bw, ac, DType::I64);
    let bacc = b.reshape_fixed(bacc, &[out as u32]);
    let num = b.c(DType::I64, l.num as i128);
    let scaled = b.mul(bacc, num, DType::I128);
    let den = b.c(DType::I64, l.den as i128);
    let scaled = b.div(scaled, den, Rounding::HalfAwayFromZero, DType::I64);
    let (ka2, ky) = (ka, want.key.clone());
    let (mb, sb) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.lora_b"),
        out,
        Arc::new(move |c| {
            let sbw = c.rows16(bp)?.scales.clone();
            let (s_a, sy) = (c.scale(&ka2)?, c.scale_vec(&ky, out)?);
            Ok((0..out).map(|o| sbw[o] * s_a / sy[o]).collect())
        }),
    )?;
    let delta = narrow(b, scaled, mb, sb, None, want.dt);
    let sum = b.add(base.r, delta, DType::I64);
    let (lo, hi) = code_bounds(want.dt);
    let r = b.clamp(sum, lo, hi, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, ..base })
}

/// Move every adapter param ([`LORA_MARK`]) behind the parent's, keeping both orders: the
/// candidate's params `0 .. P` are then the parent program's, and `P ..` the adapter's section.
/// Returns `P`.
pub fn adapter_params_last(lw: &mut Lowered) -> Result<usize> {
    let n = lw.program.params.len();
    let is_ad: Vec<bool> = lw.program.params.iter().map(|p| p.name.contains(LORA_MARK)).collect();
    let order: Vec<usize> = (0..n).filter(|i| !is_ad[*i]).chain((0..n).filter(|i| is_ad[*i])).collect();
    let mut new_of = vec![0u16; n];
    for (new, old) in order.iter().enumerate() {
        new_of[*old] = new as u16;
    }
    let params = order.iter().map(|i| lw.program.params[*i].clone()).collect();
    lw.program.params = params;
    for blk in &mut lw.program.blocks {
        for node in &mut blk.nodes {
            for r in &mut node.inputs {
                if let tir::Ref::Param(j) = r {
                    *j = new_of[*j as usize];
                }
            }
        }
    }
    let mut fills: Vec<Option<FillFn>> = lw.fills.drain(..).map(Some).collect();
    lw.fills = order.iter().map(|i| fills[*i].take().expect("each fill moves once")).collect();
    tir::validate::validate(&lw.program).map_err(|e| LowerError::eval(format!("internal: reordered program: {e}")))?;
    Ok(is_ad.iter().filter(|a| !**a).count())
}

/// Per-row `i16` codes of a gathered table (and of a head tied to it).
fn table_codes(p: u32) -> FillFn {
    Arc::new(move |c| {
        let rc = c.rows16(p)?;
        Ok(IntTensor::i16(c.f(p)?.shape.clone(), rc.codes.clone()))
    })
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
    let table = cx.tables.contains(&w);
    if table && k != 0 {
        return Err(LowerError::eval(format!("internal: a split input reads the table `{}`", d.name)));
    }
    // A projection reading the `i32` rail (a scan's gated output) has its weights at per-row `i16`
    // too: the wide input gives up the outlier split, whose high-precision weight columns carried
    // most of such an input's energy (Mamba-370m: KL 0.0012 → 0.0041 at `i8`). `i16 × i32` over
    // `inp ≤ 2^16` terms stays inside `i64`.
    let wide16 = x.dt == DType::I32 || lb.w16_now;
    let r = if k == 0 {
        // A head tied to an embedding reads the table's `i16` codes (the same param).
        let rows16 = table || wide16;
        let (dt, fill) = if rows16 { (DType::I16, table_codes(w)) } else { (DType::I8, weight_codes(w)) };
        let wt = decl(b, cx, lb, &d.name, dt, &[out, inp], d.per_layer, fill)?;
        let (m, s) = decl_ms(
            b,
            cx,
            lb,
            site,
            out,
            Arc::new(move |c| {
                let scales = if rows16 { c.rows16(w)?.scales.clone() } else { c.rows(w)?.scales.clone() };
                let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
                Ok(scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
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

/// The unit row `x / √(mean(x²) + eps)` in Q24 along the last axis — the library's `rms_unit_q24`
/// (21 nodes, one `i64` eps; the value of `rms_norm_wide_q36`).
fn rms_unit(b: &mut BlockBuilder<'_>, x: tir::Ref, eps: tir::Ref) -> tir::Ref {
    b.rms_unit_q24(x, eps)
}

/// `x / ‖x‖` in Q15 codes along the last axis — the library's `l2_unit_q15` (17 nodes; the value
/// of `l2_norm_q15`, a zero row stays zero).
fn l2_unit_q15(b: &mut BlockBuilder<'_>, x: tir::Ref) -> tir::Ref {
    b.l2_unit_q15(x)
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
    // M-RoPE with an image: three positions, each rotating its own frequencies.
    if let (Some(mr), Some(img @ ImageRows { mrope: Some(_), .. })) = (f.mrope, cx.image_rows) {
        if f.dynamic.is_some() || f.longrope.is_some() {
            return Err(LowerError::not_lowerable("M-RoPE with position-dependent frequencies"));
        }
        let half = f.inv_freq.len();
        let hb = cx.history_bound as usize;
        let lo_bits = hb.trailing_zeros().div_ceil(2);
        let (lo_rows, hi_rows) = (1usize << lo_bits, hb >> lo_bits);
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
        let pos = mrope_positions(b, cx, lb, img)?;
        let mut cs = Vec::with_capacity(3);
        for p in pos {
            let p = b.reshape_fixed(p, &[]);
            cs.push(b.rope_angles_two_level(p, ch, sh, cl, sl, lo_bits));
        }
        // Each frequency from its component: `h` where the mask says so, else `w`, else `t`.
        let mask = |k: usize| -> Vec<i128> { (0..half).map(|j| i128::from(mr.component(j) == k)).collect() };
        let (mh, mw) = (b.pb.konst(DType::I8, &[half as u32], &mask(1)), b.pb.konst(DType::I8, &[half as u32], &mask(2)));
        let c = b.select(mw, cs[2].0, cs[0].0, DType::I32);
        let c = b.select(mh, cs[1].0, c, DType::I32);
        let s = b.select(mw, cs[2].1, cs[0].1, DType::I32);
        let s = b.select(mh, cs[1].1, s, DType::I32);
        let out = (c, s);
        lb.angles.insert(t, out);
        return Ok(out);
    }
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
    /// T5's bias over bucketed relative positions ([`encdec`]'s decoder).
    rel_bias: Option<RelBias>,
}

/// T5's relative-position bias on the causal scores: `table[bucket(pos − j)]` per head, in
/// Q`LOGIT_Q` logit units. The distance is clamped at `max_d` (the bucket saturates there).
#[derive(Clone, Copy)]
struct RelBias {
    /// `i32 [buckets, heads]`.
    table: tir::Ref,
    /// `idx [max_d + 1]`: the bucket of each distance.
    buckets: tir::Ref,
    max_d: u32,
    n_buckets: u32,
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
    if ex.softcap.is_none() && ex.alibi.is_none() && ex.rel_bias.is_none() && dv == d {
        return lower_attention_library(b, cx, lb, q, k, v, dims, ex, site, want);
    }
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
            // Falcon rounds `slope` and `slope · j` to bfloat16: one row of pinned values per head,
            // one value per position the program serves — `max_window` of them when the lowering
            // was given one (a class is declared at no longer a context), else every position of
            // the history bound (a table no court close can carry: lower Falcon-ALiBi with
            // `--max-window`).
            let rows = (cx.max_window.min(hb)) as usize;
            let tab = decl(
                b,
                cx,
                lb,
                &format!("{site}.alibi"),
                DType::I64,
                &[heads, rows],
                false,
                Arc::new(move |_c| {
                    let mut v = Vec::with_capacity(hs * rows);
                    for h in 0..hs {
                        for j in 0..rows {
                            let bj = crate::rope::bf16_round(crate::rope::bf16_round(al2.slopes[h] as f32) * j as f32) as f64;
                            let bj = if al2.scaled_by_softmax_scale { bj * scale } else { bj };
                            v.push((bj * (1u64 << LOGIT_Q) as f64).round() as i64);
                        }
                    }
                    Ok(IntTensor::i64(vec![hs, rows], v))
                }),
            )?;
            let base = b.sub(pos, hm1, DType::I64);
            let j = b.add(base, t, DType::I64);
            let j = b.clamp(j, 0, rows as i64 - 1, DType::Idx);
            let bj = b.gather(tab, j, 1, 0);
            let bj = b.clamp(bj, -(1i64 << 40), 1 << 40, DType::I64);
            let pc = b.clamp(pos, 0, rows as i64 - 1, DType::Idx);
            let bp = b.gather(tab, pc, 1, 0);
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
    if let Some(rb) = ex.rel_bias {
        // Keys are `j = pos − (H − 1) + t`: the distance `pos − j = (H − 1) − t`.
        let pos = tir::Ref::Input(INPUT_POS);
        let hm1 = b.clamp(pos, 0, window as i64 - 1, DType::I64);
        let t = b.iota(DType::I64, &[Dim::H], 0, 0, 1);
        let dist = b.sub(hm1, t, DType::I64);
        let dist = b.clamp(dist, 0, rb.max_d as i64, DType::Idx);
        let bk = b.gather(rb.buckets, dist, 0, 0);
        let bk = b.clamp(bk, 0, rb.n_buckets as i64 - 1, DType::Idx);
        let bias = b.gather(rb.table, bk, 0, 0);
        let bias = b.transpose(bias, &[1, 0]);
        let bias = b.reshape(bias, &[Dim::Fixed(kv32), Dim::Fixed(g32), Dim::H]);
        let sum = b.add(logits, bias, DType::I64);
        logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
    }
    let probs = match ex.sinks {
        None => b.softmax_shifted(logits, 24 - LOGIT_Q),
        Some(sp) => {
            let sink = decl_sinks(b, cx, lb, site, heads, sp)?;
            let sink = b.reshape_fixed(sink, &[kv32, g32, 1]);
            b.softmax_with_sink(logits, sink, 24 - LOGIT_Q)
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

/// The attention sink logits `[heads]`, Q`LOGIT_Q` (gpt-oss).
fn decl_sinks(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, site: &str, heads: usize, sp: u32) -> Result<tir::Ref> {
    decl(
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
    )
}

/// **Attention through the library's `attention` template** (grouped-query, sliding window, sinks):
/// the score narrowing to Q`LOGIT_Q` logits, the softmax lifted by `2^(24 − LOGIT_Q)`, the value
/// narrowing back to codes (per channel when the output is split). The lowering's own form is kept
/// only where the function differs: soft-capping (Q24 then Q14, no division), ALiBi (Falcon's
/// bfloat16 table, the distance form) and a value head width unlike the query's.
#[allow(clippy::too_many_arguments)]
fn lower_attention_library(
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
    let AttnDims { heads, kv, d, .. } = dims;
    let g = heads / kv;
    let (kq, kk) = (q.key.clone(), k.1.clone());
    let scale = ex.scale;
    let (ms, ss) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.scores"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64])),
    )?;
    let sink = match ex.sinks {
        Some(sp) => Some(decl_sinks(b, cx, lb, site, heads, sp)?),
        None => None,
    };
    let out_key = want.key.clone();
    let (kv_, ko) = (v.1.clone(), out_key.clone());
    let n = if ko.split() > 0 { heads * d } else { 1 };
    let (mv, sv) = decl_ms(
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
    let (mv, sv) = if n > 1 {
        let shape = [kv as u32, g as u32, d as u32];
        (b.reshape_fixed(mv, &shape), b.reshape_fixed(sv, &shape))
    } else {
        (mv, sv)
    };
    let cfg = tir::library::attn::AttnCfg {
        heads: heads as u32,
        kv_heads: kv as u32,
        head_dim: d as u32,
        score: tir::library::Narrowing::new(ms, ss, None),
        softcap: None,
        alibi: None,
        sink,
        up_bits: 24 - LOGIT_Q,
        value: tir::library::Narrowing::new(mv, sv, None),
    };
    let r = b.attention(q.r, k.0, v.0, &cfg);
    let r = b.commit(r);
    Ok(Val { r, dt: DType::I16, key: out_key, len: heads * d, site: site.to_string() })
}

/// Expert selection from Q14 router logits (library `router_topk_q36` pattern): the softmax over
/// all experts then `TopK` (committed; lowest index on ties, index order), `TopK` on the logits then
/// the softmax over the kept ones, or DeepSeek-V3's sigmoid scores with a selection-only bias;
/// group-limited routing keeps the `topk_group` best groups (their max, or the sum of their top two)
/// and masks the rest (`ReduceMax(Compare(Iota, kept, Eq))`, corpus §6.1 — no scatter). Weights are
/// Q24, renormalised through `IntRecip` when the config asks; `routed_scaling_factor` lives in their
/// scale.
fn lower_route(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    l: &Val,
    sel_bias: Option<u32>,
    r: &crate::spec::RouterSpec,
    experts: usize,
    k: usize,
    site: &str,
) -> Result<(tir::Ref, tir::Ref)> {
    use crate::spec::{GroupScore, Scoring};
    if r.normalize && r.norm_eps > 1e-9 {
        return Err(LowerError::not_lowerable(format!("router renormalisation epsilon {} is not in Gate 2a", r.norm_eps)));
    }
    let up = 24 - LOGIT_Q;
    // (scores the weights are read from, choice scores the selection ranks, the value a masked
    // expert takes)
    let (scores, choice, fill): (tir::Ref, tir::Ref, i64) = match r.scoring {
        Scoring::Softmax => {
            let probs = b.softmax_shifted(l.r, up);
            (probs, probs, 0)
        }
        Scoring::TopKThenSoftmax => (l.r, l.r, i32::MIN as i64),
        Scoring::Sigmoid => {
            let c = b.c(DType::I64, 1i128 << up);
            let y = b.mul(l.r, c, DType::I64);
            let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let sig = b.int_sigmoid(y);
            let choice = match sel_bias {
                Some(bp) => {
                    let bias = decl(
                        b,
                        cx,
                        lb,
                        &format!("{site}.sel_bias"),
                        DType::I32,
                        &[experts],
                        per_layer(lb),
                        Arc::new(move |c| {
                            Ok(IntTensor::i32(vec![experts], c.f(bp)?.data.iter().map(|v| q24_wide(*v as f64)).collect()))
                        }),
                    )?;
                    b.selection_bias(sig, bias)
                }
                None => sig,
            };
            (sig, choice, i32::MIN as i64)
        }
    };
    // Group-limited routing is the library's `grouped_topk` (its top-two group score needs groups
    // of two or more experts; a group of one scores by its one expert, which is `Max`).
    let idx = match &r.groups {
        None => b.topk(choice, 0, k as u32),
        Some(g) => {
            let per = experts / g.n_group;
            let rule = match g.score {
                GroupScore::Top2Sum if per >= 2 => tir::library::moe::GroupScore::Top2Sum,
                _ => tir::library::moe::GroupScore::Max,
            };
            b.grouped_topk(choice, g.n_group as u32, g.topk_group as u32, k as u32, rule, fill)
        }
    };
    let kept = b.gather(scores, idx, 0, 0);
    let kept = if r.scoring == Scoring::TopKThenSoftmax { b.softmax_shifted(kept, up) } else { kept };
    let w = if r.normalize { b.renormalize_recip(kept) } else { kept };
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
    let row = ensure_node(b, x).r;
    let row = b.commit(row);
    // The taps are stored window-major (`[kernel, ch]`, oldest first), the library's layout: the
    // checkpoint's per-channel codes, transposed at fill time.
    let pd = &hl.params[w as usize];
    let taps = decl(
        b,
        cx,
        lb,
        &pd.name,
        DType::I8,
        &[kernel, ch],
        pd.per_layer,
        Arc::new(move |c| {
            let rc = c.rows(w)?;
            let mut t = vec![0i8; kernel * ch];
            for (c_, row) in rc.codes.chunks(kernel).enumerate() {
                for (k_, v) in row.iter().enumerate() {
                    t[k_ * ch + c_] = *v;
                }
            }
            Ok(IntTensor::i8(vec![kernel, ch], t))
        }),
    )?;
    let acc = b.causal_conv(ts, row, taps);
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
    // `v` enters the step `GDN_V_FRAC_BITS` finer than its code: the read `w = S·k`, `v − w` and
    // the β product's rounding then sit on that grid, not on `v`'s 16-bit one. On `v`'s grid those
    // roundings are written back into the state at every step and pile up: Qwen3.5-0.8B's late
    // layers drift x1.3–x3.0 over 4,096 positions in a float replay of the step, flat 8 bits finer.
    let vh = b.reshape_fixed(v.r, &[nv32, dv32]);
    let vf = b.c(DType::I32, 1i128 << GDN_V_FRAC_BITS);
    let vh = b.mul(vh, vf, DType::I32);
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
        let sv = c.scale(&kv)? / (1u64 << GDN_V_FRAC_BITS) as f64;
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

/// Fractional bits `v` gains on its way into the gated-delta step (see `lower_gdn`).
const GDN_V_FRAC_BITS: u32 = 8;

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

/// Where a scan's step size comes from: `dt = clamp(softplus(proj (+ dt_bias)), lo, hi)`, the
/// softplus/add/clamp nodes absorbed and the projection narrowed to Q24.
#[derive(Clone, Debug)]
struct SsmDt {
    proj: u32,
    bias: Option<u32>,
    clamp: Option<(f64, f64)>,
}

fn ssm_patterns(blk: &hl::Block, lb: &mut Lb) -> Result<()> {
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
        if !matches!(n.op, Op::SelectiveScan { .. } | Op::Ssd { .. }) {
            continue;
        }
        let bad = || LowerError::not_lowerable("a scan whose step size is not softplus(projection (+ bias)), optionally clamped");
        let mut at = node_of(n.inputs[1]).ok_or_else(bad)?;
        let mut absorbed = Vec::new();
        let mut clamp = None;
        if let Op::Clamp { lo, hi } = blk.nodes[at].op {
            clamp = Some((lo, hi));
            absorbed.push(at);
            at = node_of(blk.nodes[at].inputs[0]).ok_or_else(bad)?;
        }
        let Op::Act(Act::Softplus) = blk.nodes[at].op else { return Err(bad()) };
        absorbed.push(at);
        at = node_of(blk.nodes[at].inputs[0]).ok_or_else(bad)?;
        let mut bias = None;
        if let (Op::Add, [a, hl::Ref::Param(p)]) = (&blk.nodes[at].op, blk.nodes[at].inputs.as_slice()) {
            bias = Some(*p);
            absorbed.push(at);
            at = node_of(*a).ok_or_else(bad)?;
        }
        if !matches!(blk.nodes[at].op, Op::Linear { .. }) || consumers[at] != 1 {
            return Err(bad());
        }
        for j in absorbed {
            if consumers[j] != 1 {
                return Err(bad());
            }
            lb.absorbed[j] = true;
        }
        lb.wants[at] = Some(Want { dt: DType::I32, key: ScaleKey::q24() });
        lb.ssm.insert(i, SsmDt { proj: at as u32, bias, clamp });
    }
    Ok(())
}

struct SsmDims {
    /// Mamba: the inner channels (one "head" each); Mamba2: the heads.
    heads: usize,
    /// Channels per head (Mamba: 1).
    p: usize,
    /// B/C groups (Mamba: 1, shared by every channel).
    groups: usize,
    n: usize,
    /// Mamba's `A` is `[inner, N]` (a decay per state); Mamba2's is `[heads]`.
    per_state_decay: bool,
}

/// One step of a selective state-space scan (Mamba's `selective_scan`, Mamba2's SSD at one
/// position), state `h [heads, p, N]` in `i32` at a calibrated scale:
///
/// ```text
/// Δ      = clamp(softplus(dt_proj (+ dt_bias)))                     Q24  (library softplus_q36)
/// decay  = exp(A·Δ)                        Q24  (the decay_q36 steps after its softplus)
/// h'     = (decay · h) >> 24  +  N(Δ · x · B)                      the input term narrowed to h's scale
/// y      = N(Σ_n C_n · h'_n) + N(D · x)                            two narrowings to y's codes
/// ```
#[allow(clippy::too_many_arguments)]
fn lower_ssm(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    bb: &Val,
    cc: &Val,
    ap: u32,
    dp: u32,
    st: u32,
    dt: &SsmDt,
    dims: SsmDims,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let SsmDims { heads, p, groups, n, per_state_decay } = dims;
    let (h32, p32, n32, g32) = (heads as u32, p as u32, n as u32, groups as u32);
    let pl = per_layer(lb);
    let dv = lb.vals[dt.proj as usize][0].clone().ok_or_else(|| LowerError::eval("internal: scan step projection not lowered"))?;
    let mut d = dv.r;
    if let Some(bp) = dt.bias {
        let bias = decl(
            b,
            cx,
            lb,
            &format!("{site}.dt_bias"),
            DType::I32,
            &[heads],
            pl,
            Arc::new(move |c| Ok(IntTensor::i32(vec![heads], c.f(bp)?.data.iter().map(|v| q24_wide(*v as f64)).collect()))),
        )?;
        let s = b.add(d, bias, DType::I64);
        d = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
    }
    let sp = b.softplus_q36(d);
    let sp = match dt.clamp {
        Some((lo, hi)) => {
            let lo = (lo * (1u64 << 24) as f64).round().clamp(0.0, i32::MAX as f64) as i64;
            let hi =
                if hi.is_finite() { (hi * (1u64 << 24) as f64).round().clamp(0.0, i32::MAX as f64) as i64 } else { i32::MAX as i64 };
            b.clamp(sp, lo, hi.max(lo), DType::I32)
        }
        None => b.clamp(sp, 0, i32::MAX as i64, DType::I32),
    };
    let sp = b.commit(sp);
    // decay = exp(−c·Δ), c = −A ≥ 0 in Q24: Mamba `[heads, 1, N]`, Mamba2 `[heads, 1, 1]`.
    let a_shape: Vec<usize> = if per_state_decay { vec![heads, n] } else { vec![heads] };
    let an = a_shape.iter().product::<usize>();
    let cpar = decl(
        b,
        cx,
        lb,
        &format!("{site}.decay_c"),
        DType::I64,
        &[an],
        pl,
        Arc::new(move |c| {
            Ok(IntTensor::i64(
                vec![an],
                c.f(ap)?.data.iter().map(|v| (-(*v as f64) * (1u64 << 24) as f64).round().max(0.0) as i64).collect(),
            ))
        }),
    )?;
    let cr = if per_state_decay { b.reshape_fixed(cpar, &[h32, 1, n32]) } else { b.reshape_fixed(cpar, &[h32, 1, 1]) };
    let spr = b.reshape_fixed(sp, &[h32, 1, 1]);
    let prod = b.mul(cr, spr, DType::I128);
    let arg = b.shr(prod, 24, Rounding::Floor, DType::I128);
    let arg = b.clamp(arg, 0, 1i64 << 31, DType::I64);
    let zero = b.c(DType::I32, 0);
    let neg = b.sub(zero, arg, DType::I64);
    let neg = b.clamp(neg, i32::MIN as i64, 0, DType::I32);
    let decay = b.exp_refined_q36(neg);
    // The state.
    let sd = &hl.states[st as usize];
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I32, &[h32, p32, n32], -(i32::MAX as i64), i32::MAX as i64, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    let kept = b.mul(tir::Ref::State(ts), decay, DType::I64);
    let kept = b.shr(kept, 24, Rounding::HalfAwayFromZero, DType::I64);
    // Input term Δ·x·B → h's scale.
    let xr = b.reshape_fixed(x.r, &[h32, p32, 1]);
    let map_groups = |b: &mut BlockBuilder<'_>, v: tir::Ref| -> tir::Ref {
        if groups == 1 {
            b.reshape_fixed(v, &[1, 1, n32])
        } else {
            let r = h32 / g32;
            let y = b.reshape_fixed(v, &[g32, 1, n32]);
            let y = b.broadcast(y, &[Dim::Fixed(g32), Dim::Fixed(r), Dim::Fixed(n32)]);
            b.reshape_fixed(y, &[h32, 1, n32])
        }
    };
    let br = map_groups(b, bb.r);
    let dx = b.mul(spr, xr, DType::I64);
    let u = b.mul(dx, br, DType::I64);
    let hkey = ScaleKey::site(vec![format!("{site}.state")], true);
    let (kx, kb, kh) = (x.key.clone(), bb.key.clone(), hkey.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.input"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kx)? * c.scale(&kb)? / (1u64 << 24) as f64 / c.scale(&kh)?])),
    )?;
    let un = narrow(b, u, m, s, None, DType::I32);
    let hn = b.add(kept, un, DType::I64);
    let hn = b.state_write(ts, hn);
    // y = Σ_n C_n h'_n + D·x.
    let cr = map_groups(b, cc.r);
    let ct = b.transpose(cr, &[0, 2, 1]);
    let acc = b.matmul(hn, ct, DType::I64);
    let acc = b.reshape_fixed(acc, &[(heads * p) as u32]);
    let yk = want.key.clone();
    let yn = heads * p;
    let (kc, kh2, ky) = (cc.key.clone(), hkey.clone(), yk.clone());
    let (m, s) = decl_ms(b, cx, lb, site, 1, Arc::new(move |c| Ok(vec![c.scale(&kc)? * c.scale(&kh2)? / c.scale(&ky)?])))?;
    let y1 = narrow(b, acc, m, s, None, DType::I32);
    let (kx2, ky2) = (x.key.clone(), yk.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.skip"),
        heads,
        Arc::new(move |c| {
            let dvv = c.f(dp)?.data.clone();
            let (sx, sy) = (c.scale(&kx2)?, c.scale(&ky2)?);
            Ok(dvv.iter().map(|d| *d as f64 * sx / sy).collect())
        }),
    )?;
    let m = b.reshape_fixed(m, &[h32, 1]);
    let s = b.reshape_fixed(s, &[h32, 1]);
    let x2 = b.reshape_fixed(x.r, &[h32, p32]);
    let y2 = narrow(b, x2, m, s, None, DType::I32);
    let y2 = b.reshape_fixed(y2, &[yn as u32]);
    let y = b.add(y1, y2, DType::I64);
    let (lo, hi) = code_bounds(want.dt);
    let r = b.clamp(y, lo, hi, want.dt);
    b.commit(r);
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: yn, site: site.to_string() })
}

/// Mamba2's `RMSNorm(y · silu(z)) · w` per group: the product of two code rows is exact in `i32`
/// and the norm is scale-free, so the gated row goes straight into the unit-row composite.
#[allow(clippy::too_many_arguments)]
fn lower_gated_norm_first(
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
    let gate = lower_table_named(b, cx, lb, z, TableFn::Act(Act::Silu), &format!("{site}.gate"))?;
    let p = b.mul(x.r, gate.r, DType::I32);
    let pr = b.reshape_fixed(p, &[groups as u32, g as u32]);
    let (kx, kg) = (x.key.clone(), gate.key.clone());
    let eps_q = Arc::new(move |c: &FillCtx<'_>| -> Result<f64> {
        let s = c.scale(&kx)? * c.scale(&kg)?;
        Ok(eps * (1u64 << 24) as f64 / (s * s))
    });
    let eps_p = decl_eps(b, cx, lb, site, eps_q)?;
    let u = rms_unit(b, pr, eps_p);
    let u = b.reshape_fixed(u, &[n as u32]);
    let ky = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let w = c.f(gain)?.data.clone();
            let sy = c.scale_vec(&ky, n)?;
            Ok((0..n).map(|i| w[if w.len() == n { i } else { i % w.len() }] as f64 / (1u64 << 24) as f64 / sy[i]).collect())
        }),
    )?;
    let r = narrow(b, u, m, s, None, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
}

struct MlaDims {
    heads: usize,
    nope: usize,
    rope: usize,
    vd: usize,
    r: usize,
}

/// Multi-head latent attention in the absorbed form (DeepSeek-V2/V3): the latent rows and the shared
/// rotary key are the histories; per head `q̃ = W_kᵀ q_nope` (`kv_b`'s key half quantised per
/// (head, latent column), since the contraction runs over its rows), logits
/// `q̃·latent + q_rope·k_rope` in Q14, the softmax, the latent context `P·latent`, and
/// `W_v · ctx` (the value half per (head, row)).
#[allow(clippy::too_many_arguments)]
fn lower_mla(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    q: &Val,
    lat: (tir::Ref, ScaleKey),
    kr: (tir::Ref, ScaleKey),
    kvb: u32,
    dims: MlaDims,
    scale: f64,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let MlaDims { heads, nope, rope, vd, r } = dims;
    let (h32, n32, ro32, v32, r32) = (heads as u32, nope as u32, rope as u32, vd as u32, r as u32);
    let pl = per_layer(lb);
    let pd = &cx.hl.params[kvb as usize];
    let base = pd.name.trim_end_matches(".w").to_string();
    // kv_b rows: head h's key half then its value half.
    let half = move |c: &FillCtx<'_>, value: bool| -> Result<(Vec<f32>, usize)> {
        let t = c.f(kvb)?;
        let (rows_per, off, len) = (nope + vd, if value { nope } else { 0 }, if value { vd } else { nope });
        let mut out = Vec::with_capacity(heads * len * r);
        for hh in 0..heads {
            let start = (hh * rows_per + off) * r;
            out.extend_from_slice(&t.data[start..start + len * r]);
        }
        Ok((out, len))
    };
    // Key half: codes per (head, column).
    let kcodes = move |c: &FillCtx<'_>| -> Result<(Vec<i8>, Vec<f64>)> {
        let (w, len) = half(c, false)?;
        let mut codes = vec![0i8; w.len()];
        let mut scales = vec![1.0f64; heads * r];
        for hh in 0..heads {
            for col in 0..r {
                let amax = (0..len).fold(0f64, |m, i| m.max((w[(hh * len + i) * r + col] as f64).abs()));
                let sc = if amax > 0.0 { amax / 127.0 } else { 1.0 };
                scales[hh * r + col] = sc;
                for i in 0..len {
                    let k = (hh * len + i) * r + col;
                    codes[k] = (w[k] as f64 / sc).round().clamp(-127.0, 127.0) as i8;
                }
            }
        }
        Ok((codes, scales))
    };
    let kc1 = kcodes;
    let wk = decl(
        b,
        cx,
        lb,
        &format!("{base}.k"),
        DType::I8,
        &[heads, nope, r],
        pl,
        Arc::new(move |c| Ok(IntTensor::i8(vec![heads, nope, r], kc1(c)?.0))),
    )?;
    let vrows = move |c: &FillCtx<'_>| -> Result<crate::quant::RowCodes> {
        let (w, len) = half(c, true)?;
        Ok(crate::quant::quantize_rows(&w, heads * len, r, None))
    };
    let vr1 = vrows;
    let wv = decl(
        b,
        cx,
        lb,
        &format!("{base}.v"),
        DType::I8,
        &[heads, vd, r],
        pl,
        Arc::new(move |c| Ok(IntTensor::i8(vec![heads, vd, r], vr1(c)?.codes))),
    )?;
    let q3 = b.reshape_fixed(q.r, &[h32, n32 + ro32]);
    let qn = b.slice(q3, 1, 0, n32);
    let qn = b.reshape_fixed(qn, &[h32, 1, n32]);
    let qr = b.slice(q3, 1, n32, ro32);
    let qr = b.reshape_fixed(qr, &[h32, 1, ro32]);
    // q̃ per (head, latent column).
    let qt_key = ScaleKey::site(vec![format!("{site}.qt")], false);
    let acc = b.matmul(qn, wk, DType::I64);
    let (kq, kt) = (q.key.clone(), qt_key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.qt"),
        heads * r,
        Arc::new(move |c| {
            let (_, sc) = kcodes(c)?;
            let (sq, st) = (c.scale(&kq)?, c.scale(&kt)?);
            Ok(sc.iter().map(|v| v * sq / st).collect())
        }),
    )?;
    let m = b.reshape_fixed(m, &[h32, 1, r32]);
    let s = b.reshape_fixed(s, &[h32, 1, r32]);
    let qt = narrow(b, acc, m, s, None, DType::I16);
    let qt = b.commit(qt);
    // Logits over the latent history and the rotary keys, in Q14.
    let lw = b.reshape(lat.0, &[Dim::H, Dim::Fixed(r32)]);
    let lt = b.transpose(lw, &[1, 0]);
    let s1 = b.matmul(qt, lt, DType::I64);
    let kw = b.reshape(kr.0, &[Dim::H, Dim::Fixed(ro32)]);
    let kt2 = b.transpose(kw, &[1, 0]);
    let s2 = b.matmul(qr, kt2, DType::I64);
    let (kt1, kl) = (qt_key.clone(), lat.1.clone());
    let (m1, sh1) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.scores"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kt1)? * c.scale(&kl)? * scale * (1u64 << LOGIT_Q) as f64])),
    )?;
    let (kq2, kk2) = (q.key.clone(), kr.1.clone());
    let (m2, sh2) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.scores_rope"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kq2)? * c.scale(&kk2)? * scale * (1u64 << LOGIT_Q) as f64])),
    )?;
    let l1 = narrow(b, s1, m1, sh1, None, DType::I32);
    let l2 = narrow(b, s2, m2, sh2, None, DType::I32);
    let ls = b.add(l1, l2, DType::I64);
    let logits = b.clamp(ls, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let probs = b.softmax_shifted(logits, 24 - LOGIT_Q);
    // Latent context, then the value half.
    let ctx = b.matmul(probs, lw, DType::I64);
    let ck = ScaleKey::site(vec![format!("{site}.latent_ctx")], false);
    let (kl2, kc) = (lat.1.clone(), ck.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.latent_ctx"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kl2)? / (1u64 << 24) as f64 / c.scale(&kc)?])),
    )?;
    let ctx = narrow(b, ctx, m, s, None, DType::I16);
    let ctx = b.commit(ctx);
    let ctxt = b.transpose(ctx, &[0, 2, 1]);
    let acc = b.matmul(wv, ctxt, DType::I64);
    let acc = b.reshape_fixed(acc, &[(heads * vd) as u32]);
    let (kc2, ko) = (ck.clone(), want.key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        heads * vd,
        Arc::new(move |c| {
            let rc = vrows(c)?;
            let sc = c.scale(&kc2)?;
            let so = c.scale_vec(&ko, heads * vd)?;
            Ok(rc.scales.iter().zip(&so).map(|(sw, so)| sw * sc / so).collect())
        }),
    )?;
    let o = narrow(b, acc, m, s, None, DType::I16);
    let o = b.commit(o);
    let _ = v32;
    Ok(Val { r: o, dt: DType::I16, key: want.key.clone(), len: heads * vd, site: site.to_string() })
}

/// RWKV-4's WKV with its (num, den, max) state, in the log domain HF keeps it in:
///
/// ```text
/// ww = u + k;  p = max(m, ww);  e1 = IntExp(m − p);  e2 = IntExp(ww − p)                 Q24
/// out = (e1·num + e2·v) · 2^16 / (e1·den + e2·2^16)                                    v's code scale
/// ww' = m + w; p' = max(ww', k); num ← (e1'·num + e2'·v) >> 24; den ← (e1'·den + e2'·2^16) >> 24; m ← p'
/// ```
///
/// `num` is `i32` at v's code scale, `den` Q16 (so it can count 32,768 undecayed steps), `m` Q24.
/// The TIR state starts at zeros where HF starts `m` at −∞: with `num = den = 0` the true values
/// `num·e^m`, `den·e^m` are zero either way, and every later step is invariant to the common
/// factor, so the outputs are the same function.
#[allow(clippy::too_many_arguments)]
fn lower_wkv4(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    k: &Val,
    v: &Val,
    wp: u32,
    up: u32,
    sts: &[u32],
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let n = k.len;
    let pl = per_layer(lb);
    let mut ts = Vec::new();
    for s in sts {
        let sd = &hl.states[*s as usize];
        let t = match cx.tstate.get(s) {
            Some(t) => *t,
            None => {
                let t = b.pb.fixed_state(&sd.name, DType::I32, &[n as u32], i32::MIN as i64 + 1, i32::MAX as i64, true);
                cx.tstate.insert(*s, t);
                t
            }
        };
        ts.push(t);
    }
    let (num, den, mx) = (tir::Ref::State(ts[0]), tir::Ref::State(ts[1]), tir::Ref::State(ts[2]));
    let q24p = |b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, name: &str, p: u32| -> Result<tir::Ref> {
        decl(
            b,
            cx,
            lb,
            name,
            DType::I32,
            &[n],
            pl,
            Arc::new(move |c| Ok(IntTensor::i32(vec![n], c.f(p)?.data.iter().map(|v| q24_wide(*v as f64)).collect()))),
        )
    };
    let w = q24p(b, cx, lb, &format!("{site}.w"), wp)?;
    let u = q24p(b, cx, lb, &format!("{site}.u"), up)?;
    // k in Q24 (a logit-like exponent).
    let kk = k.key.clone();
    let (m, s) = decl_ms(b, cx, lb, &format!("{site}.k"), 1, Arc::new(move |c| Ok(vec![c.scale(&kk)? * (1u64 << 24) as f64])))?;
    let kq = narrow(b, k.r, m, s, None, DType::I32);
    let kq = b.commit(kq);
    let exp_diff = |b: &mut BlockBuilder<'_>, a: tir::Ref, p: tir::Ref| -> tir::Ref {
        let d = b.sub(a, p, DType::I64);
        let d = b.clamp(d, i32::MIN as i64, 0, DType::I32);
        b.int_exp(d)
    };
    let max2 = |b: &mut BlockBuilder<'_>, a: tir::Ref, c: tir::Ref| -> tir::Ref {
        let gt = b.compare(a, c, tir::Cmp::Gt);
        b.select(gt, a, c, DType::I32)
    };
    let s16 = b.c(DType::I64, 1 << 16);
    // Output.
    let ww = b.add(u, kq, DType::I64);
    let ww = b.clamp(ww, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let p = max2(b, mx, ww);
    let e1 = exp_diff(b, mx, p);
    let e2 = exp_diff(b, ww, p);
    let a1 = b.mul(e1, num, DType::I64);
    let a2 = b.mul(e2, v.r, DType::I64);
    let numer = b.add(a1, a2, DType::I64);
    let d1 = b.mul(e1, den, DType::I64);
    let d2 = b.mul(e2, s16, DType::I64);
    let denom = b.add(d1, d2, DType::I64);
    let denom = b.clamp(denom, 1, i64::MAX, DType::I64);
    let nu = b.mul(numer, s16, DType::I128);
    // A weighted mean of v codes: the clamp states the range the analysis cannot see.
    let o = b.div(nu, denom, Rounding::HalfAwayFromZero, DType::I128);
    let o = b.clamp(o, i32::MIN as i64, i32::MAX as i64, DType::I64);
    let (kv_, ko) = (v.key.clone(), want.key.clone());
    let per = if ko.split() > 0 { n } else { 1 };
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        per,
        Arc::new(move |c| {
            let sv = c.scale(&kv_)?;
            Ok(c.scale_vec(&ko, per)?.iter().map(|so| sv / so).collect())
        }),
    )?;
    let out = narrow(b, o, m, s, None, want.dt);
    b.commit(out);
    // State update.
    let ww2 = b.add(mx, w, DType::I64);
    let ww2 = b.clamp(ww2, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let p2 = max2(b, ww2, kq);
    let f1 = exp_diff(b, ww2, p2);
    let f2 = exp_diff(b, kq, p2);
    let b1 = b.mul(f1, num, DType::I64);
    let b2 = b.mul(f2, v.r, DType::I64);
    let nn = b.add(b1, b2, DType::I64);
    let nn = b.shr(nn, 24, Rounding::HalfAwayFromZero, DType::I64);
    b.state_write(ts[0], nn);
    let c1 = b.mul(f1, den, DType::I64);
    let c2 = b.mul(f2, s16, DType::I64);
    let dn = b.add(c1, c2, DType::I64);
    let dn = b.shr(dn, 24, Rounding::HalfAwayFromZero, DType::I64);
    b.state_write(ts[1], dn);
    b.state_write(ts[2], p2);
    Ok(Val { r: out, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
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
