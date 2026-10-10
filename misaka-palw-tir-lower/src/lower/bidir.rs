//! **Bidirectional encoders** (BERT, RoBERTa, XLM-R) as ONE position over a padded token axis
//! (RFC-0003 Part II.3: "a bidirectional encoder is a single-position program over a padded token
//! axis").
//!
//! The program reads two inputs, declared here as global params and lifted by
//! [`crate::encoder::bidir_v2`]:
//! * `input.ids`, an `idx [L]` (the job's template, padded: `JobTokens`);
//! * `input.count`, an `idx []` (the unpadded length: `JobTokenCount`).
//!
//! Every value is a `[L, …]` tensor: the rows of all tokens at once. Attention is full over the
//! token axis, a fixed axis rather than `H`, with the keys at or past `count` masked
//! (`Compare(Iota < count)` → `Select` to `i32::MIN`, which the softmax's `IntExp` maps to exactly
//! 0). Pad rows are computed like any other and never reach a real row or the pooling.
//!
//! The per-position HL program of the same spec supplies the params, their binding, the
//! occurrences and the fills' float weights. Its blocks are not lowered: the blocks below are.
//! Scales are the calibration's, per site and occurrence, from [`float_forward`]'s statistics,
//! whose site names match the ones here.

use super::*;
use crate::float_ref::{ParamStore, SiteStat};
use crate::spec::{ArchSpec, Ffn, Mixer, NormSpec, Position, Residual};

/// How the class pools the token rows into one vector (sentence-transformers' pooling modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pooling {
    /// Row 0 (`[CLS]` / `<s>`).
    Cls,
    /// The mean over the `count` real rows.
    Mean,
}

/// The class's choices around the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BidirCfg {
    /// The padded token axis `L` (the template's `pad.to_len`).
    pub lmax: u32,
    pub pooling: Pooling,
    /// L2-normalise the pooled vector (sentence-transformers' `Normalize`).
    pub normalize: bool,
}

/// **Class choices that are not the model's, the pooling's or the normalisation's** (HFX 2026-10-10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BidirExtras {
    /// **`ENC_PAIR_SEGMENTS_V1`**: the id of the separator that closes the first segment of a pair input (`question ‖ sep ‖ context`).
    /// A model with a token-type table of two or more rows then adds type row 1 to every position after the first separator and row 0
    /// to the others — computed IN the program from the job's ids (an equality against this constant, a strictly-lower-triangular
    /// count of earlier separators, a clamp to `{0, 1}` and a two-row gather), so the job supplies ids and a count and nothing else
    /// (K2-TIR-v5's binding). `None`: one segment (type row 0 everywhere), the lowering before this feature. Ignored by a model
    /// with fewer than two type rows (RoBERTa / XLM-R, DistilBERT: no segment ids exist).
    pub pair_sep: Option<u32>,
}

/// The input params' names (lifted into inputs in this order).
pub const IDS_PARAM: &str = "input.ids";
pub const COUNT_PARAM: &str = "input.count";

/// A norm of the encoder: its kind, epsilon and whether it has a bias.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NormCfg {
    kind: NormKind,
    eps: f64,
    bias: bool,
}

/// Where a layer's norms sit.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    /// BERT: `x = n1(x + mixer(x))`, `x = n2(x + ffn(x))`.
    Post { mix: NormCfg, ffn: NormCfg },
    /// ModernBERT, EuroBERT: `x += mixer(n1(x))`, `x += ffn(n2(x))`, a norm absent where the checkpoint has none.
    Pre { mix: Option<NormCfg>, ffn: Option<NormCfg> },
}

/// What one layer kind needs from the spec (`ENC_BIDIR_V1`'s rows lowering).
#[derive(Clone, Debug)]
struct LayerArch {
    heads: u32,
    head_dim: u32,
    scale: f64,
    /// q, k, v, o biases.
    bias: [bool; 4],
    /// `ENC_ROPE_V1`: rotate_half over the whole head, the table taken per position.
    rope: Option<crate::rope::RopeSpec>,
    /// `ENC_BAND_WINDOW_V1`: a key is visible iff `|i − j| < w` (the symmetric band of a bidirectional sliding window).
    band: Option<usize>,
    gated: bool,
    up_bias: bool,
    down_bias: bool,
    act: crate::spec::Act,
    place: Place,
}

/// What a bidirectional lowering needs from the spec.
struct Arch {
    layers: Vec<LayerArch>,
    embed_norm: Option<NormCfg>,
    final_norm: Option<NormCfg>,
    has_positions: bool,
    pos_offset: usize,
    /// MPNet's bias over bucketed relative positions.
    rel: Option<crate::spec::RelBiasSpec>,
    /// DeBERTa's disentangled attention (`ATTN_DISENTANGLED_V1`).
    dis: Option<crate::spec::DisentangledSpec>,
    /// ALBERT: the embedding is at the table's width and a projection (bias or not) lifts the normed row to the hidden width.
    proj_in: Option<bool>,
    /// `OUTPUT_CLASSIFY_V1`: the pooled row `[CLS]` goes through an optional dense layer + activation and a linear layer to the labels.
    classify: Option<(bool, Option<crate::spec::ClassifyPre>)>,
    /// `OUTPUT_TOKEN_LOGITS_V1`: every row goes through a linear layer (its bias) to the labels; no pooling.
    token_logits: Option<bool>,
    /// `ENC_PAIR_SEGMENTS_V1`: the first segment's closing separator (set only for a model with a type table of two rows or more).
    pair_sep: Option<u32>,
    /// `OutputSpec::MaskedLm`: every row through the head's transform (dense, activation, LayerNorm) and the vocabulary projection.
    mlm: Option<MlmArch>,
}

/// A masked-LM head's transform and projection (`HEAD_TRANSFORM_V1`; the projection is the head param `head.w`, bound to the word
/// embeddings when the head is tied, with the head's bias `head.b`).
#[derive(Clone, Copy, Debug)]
struct MlmArch {
    dense_bias: bool,
    act: crate::spec::Act,
    norm: NormCfg,
    head_bias: bool,
}

impl Arch {
    /// The layer kind of the HL block `hbk` (its first scheduled layer).
    fn of_block(&self, hl: &HlProgram, hbk: usize) -> &LayerArch {
        let li = hl.schedule.iter().position(|k| *k as usize == hbk).unwrap_or(0);
        &self.layers[li.min(self.layers.len() - 1)]
    }
}

fn norm_cfg(n: &NormSpec) -> NormCfg {
    NormCfg { kind: n.kind, eps: n.eps, bias: n.bias }
}

fn arch_of(spec: &ArchSpec) -> Result<Arch> {
    let bad = |m: &str| Err(LowerError::not_lowerable(format!("{}: {m}", spec.architecture)));
    if spec.layers.is_empty() {
        return bad("no layers");
    }
    if spec.hyper.is_some() {
        return bad("hyper-connection residual streams");
    }
    let mut layers = Vec::new();
    for first in &spec.layers {
        let Mixer::Attention(at) = &first.mixer else { return bad("a mixer other than attention") };
        if at.kv_heads != at.heads || at.head_dim != at.v_head_dim || at.qk_norm.is_some() || at.sinks {
            return bad("attention other than plain multi-head");
        }
        // FR-26: a field this lowering never reads must be at the value it assumes — otherwise an adapter that sets it
        // lowers to a WRONG program with no error.
        let rope = match &at.position {
            Position::None => None,
            Position::Rope(r) => {
                if r.style != crate::rope::RopeStyle::Half || r.rotary_dim != at.head_dim || r.offset != 0 {
                    return bad("a rotary position other than rotate_half over the whole head (partial or interleaved rope in a bidirectional encoder is not lowered)");
                }
                Some(r.clone())
            }
            Position::Alibi(_) => return bad("ALiBi in a bidirectional encoder"),
        };
        if at.softcap.is_some()
            || at.clip_qkv.is_some()
            || at.chunk.is_some()
            || at.q_temperature.is_some()
            || at.v_norm.is_some()
            || at.v_from_k
            || at.output_gate
            || at.gate.is_some()
            || at.v_scale != 1.0
            || at.differential.is_some()
            || at.moa.is_some()
            || at.qk_norm_after_rope
            || at.kv_share.is_some()
            || at.sparse.is_some()
            || at.param_prefix.is_some()
        {
            return bad("an attention feature (soft-cap, q/k/v clipping, chunks, temperature, v-norm, gate, KV sharing, sparse blocks) this lowering does not read");
        }
        let Ffn::Mlp(mlp) = &first.ffn else { return bad("an FFN other than a dense MLP") };
        let place = match &first.residual {
            Residual::PostNorm { mixer_norm, ffn_norm } => Place::Post { mix: norm_cfg(mixer_norm), ffn: norm_cfg(ffn_norm) },
            Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } => {
                if post_mixer.is_some() || post_ffn.is_some() || *multiplier != 1.0 {
                    return bad("a sandwich or scaled residual in a bidirectional encoder");
                }
                Place::Pre { mix: pre_mixer.as_ref().map(norm_cfg), ffn: pre_ffn.as_ref().map(norm_cfg) }
            }
            _ => return bad("a residual other than post-LN or pre-norm"),
        };
        for n in match place {
            Place::Post { mix, ffn } => vec![Some(mix), Some(ffn)],
            Place::Pre { mix, ffn } => vec![mix, ffn],
        }
        .into_iter()
        .flatten()
        {
            if n.kind == NormKind::Rms && n.bias {
                return bad("an RMSNorm with a bias");
            }
        }
        if first.post_scale != 1.0 {
            return bad("a layer output scale");
        }
        layers.push(LayerArch {
            heads: at.heads as u32,
            head_dim: at.head_dim as u32,
            scale: at.scale,
            bias: [at.q_bias, at.k_bias, at.v_bias, at.o_bias],
            rope,
            band: at.window,
            gated: mlp.gated,
            up_bias: mlp.up_bias,
            down_bias: mlp.down_bias,
            act: mlp.act,
            place,
        });
    }
    let e = &spec.embedding;
    // A factorised embedding is read only as ALBERT's: projection after the norm.
    let factorised = e.proj_in && e.proj_after_norm && e.dim != spec.hidden_size && e.norm.is_some();
    if (e.proj_in != factorised) || (e.dim != spec.hidden_size && !factorised) || e.scale != 1.0 {
        return bad("an embedding of another width than the hidden size without a projection after its norm (OPT's project_in first), or an embedding scale: this lowering reads the plain BERT embedding and ALBERT's factorised one");
    }
    if e.positions.is_none() && e.disentangled.is_none() && layers.iter().all(|l| l.rope.is_none()) {
        return bad("no positions: neither a learned position table, a rotary position nor relative-position embeddings");
    }
    if e.positions.is_some() && layers.iter().any(|l| l.rope.is_some()) {
        return bad("learned positions together with a rotary position");
    }
    if let Some(dis) = &e.disentangled {
        if e.rel_bias.is_some() {
            return bad("a relative-position bias together with disentangled attention");
        }
        if !(dis.c2p || dis.p2c) || dis.span < 2 || dis.max_position < 2 {
            return bad("disentangled attention with no position term or a degenerate bucket span");
        }
        if dis.norm.is_some_and(|n| n.kind != NormKind::Layer) {
            return bad("a relative-embedding norm other than LayerNorm");
        }
    }
    let embed_norm = e.norm.as_ref().map(norm_cfg);
    if embed_norm.is_some_and(|n| n.kind == NormKind::Rms && n.bias) {
        return bad("an RMSNorm with a bias");
    }
    Ok(Arch {
        layers,
        embed_norm,
        final_norm: spec.final_norm.as_ref().map(norm_cfg),
        has_positions: e.positions.is_some(),
        pos_offset: e.positions.as_ref().map_or(0, |p| p.offset),
        rel: e.rel_bias,
        proj_in: factorised.then_some(e.proj_in_bias),
        dis: e.disentangled,
        classify: match &spec.output {
            crate::spec::OutputSpec::Classify { bias, pre, .. } => Some((*bias, *pre)),
            _ => None,
        },
        token_logits: match &spec.output {
            crate::spec::OutputSpec::TokenLogits { bias, .. } => Some(*bias),
            _ => None,
        },
        pair_sep: None,
        mlm: match (&spec.output, &spec.head.transform) {
            (crate::spec::OutputSpec::MaskedLm, Some(t)) => {
                if spec.head.tied && spec.head.proj_out {
                    return bad("a masked-LM head with a projection out of the hidden width");
                }
                Some(MlmArch { dense_bias: t.bias, act: t.act, norm: norm_cfg(&t.norm), head_bias: spec.head.bias })
            }
            (crate::spec::OutputSpec::MaskedLm, None) => return bad("a masked-LM output without HEAD_TRANSFORM_V1"),
            _ => None,
        },
    })
}

/// DeBERTa's `build_relative_position` + `make_log_bucket_position` for one `(query i, key j)` pair — float32 exactly as
/// transformers computes it — then the clamp into the table's `2·span` rows (`c2p_pos = clamp(rel + span, 0, 2·span − 1)`).
/// The bucket is odd in the relative position, so the position-to-content term reads the same index.
pub fn deberta_index(i: usize, j: usize, span: usize, max_position: usize) -> usize {
    let rel = i as i64 - j as i64;
    let mid = (span / 2) as i64;
    let sign = rel.signum();
    let abs_pos = if rel < mid && rel > -mid { mid - 1 } else { rel.abs() };
    let bucket = if abs_pos <= mid {
        rel
    } else {
        let a = (abs_pos as f32 / mid as f32).ln();
        let b = (((max_position as f64 - 1.0) / mid as f64) as f32).ln();
        ((a / b * (mid - 1) as f32).ceil() as i64 + mid) * sign
    };
    (bucket + span as i64).clamp(0, 2 * span as i64 - 1) as usize
}

/// **`ENC_PAIR_SEGMENTS_V1`'s segment ids** of a padded id row: position `i` is in segment 1 iff a separator sits at some `j < i`
/// (the first separator closes segment 0 and is itself in it; every later position — a second separator and the pad included —
/// is in segment 1). All zeros without a separator id. The program computes exactly this from the ids.
pub fn pair_segment_types(ids: &[usize], sep: Option<u32>) -> Vec<usize> {
    let mut seen = false;
    ids.iter()
        .map(|id| {
            let t = usize::from(seen);
            seen |= sep.is_some_and(|s| *id == s as usize);
            t
        })
        .collect()
}

/// One row through a LayerNorm or RMSNorm, in f64.
fn norm_row(kind: NormKind, eps: f64, x: &[f64], g: &[f64], bias: Option<&[f64]>) -> Vec<f64> {
    let n = x.len() as f64;
    match kind {
        NormKind::Layer => {
            let mu = x.iter().sum::<f64>() / n;
            let var = x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
            let inv = 1.0 / (var + eps).sqrt();
            x.iter().enumerate().map(|(i, v)| (v - mu) * inv * g[i] + bias.map_or(0.0, |b| b[i])).collect()
        }
        NormKind::Rms => {
            let inv = 1.0 / (x.iter().map(|v| v * v).sum::<f64>() / n + eps).sqrt();
            x.iter().enumerate().map(|(i, v)| v * inv * g[i]).collect()
        }
    }
}

pub(super) fn hl_param(hl: &HlProgram, name: &str) -> Result<u32> {
    hl.params
        .iter()
        .position(|p| p.name == name)
        .map(|i| i as u32)
        .ok_or_else(|| LowerError::eval(format!("internal: the HL program has no param `{name}`")))
}

/// Lower a bidirectional encoder: pre (embeddings), one block per layer kind, post (pooling). The
/// program's "logits" node is the pooled vector `[1, d]` in the class's fixed point (`Q30` when
/// normalised, a power-of-two unit otherwise).
pub fn lower_bidir(hl: &HlProgram, spec: &ArchSpec, cfg: &BidirCfg) -> Result<Lowered> {
    lower_bidir_with(hl, spec, cfg, &BidirExtras::default())
}

/// [`lower_bidir`] with the class's [`BidirExtras`].
pub fn lower_bidir_with(hl: &HlProgram, spec: &ArchSpec, cfg: &BidirCfg, extras: &BidirExtras) -> Result<Lowered> {
    let mut a = arch_of(spec)?;
    a.pair_sep = extras.pair_sep.filter(|_| spec.embedding.type_rows.is_some_and(|r| r > 1));
    let l = cfg.lmax;
    let max_rows = spec.embedding.positions.as_ref().map_or(0, |p| p.rows);
    if l == 0 || (a.has_positions && a.pos_offset + l as usize > max_rows) {
        return Err(LowerError::not_lowerable(format!(
            "{}: a padded length of {l} needs positions {}..{} of {max_rows}",
            spec.architecture,
            a.pos_offset,
            a.pos_offset + l as usize
        )));
    }
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let token_bound = u32::try_from(hl.vocab).map_err(|_| LowerError::not_lowerable("vocabulary beyond u32"))?;
    let mut pb = ProgramBuilder::new(token_bound, hb);
    let mut cx = Cx {
        hl,
        fills: Vec::new(),
        row_params: BTreeMap::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window: hb,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared: Default::default(),
        tables: Default::default(),
        image_rows: None,
        image_cursor: None,
        image_cursor_layer: None,
        split_max_readers: 0,
        quant: BTreeMap::new(),
        table_shift: 0,
        carry_keys: BTreeMap::new(),
        table_chunk: 1 << 24,
        conv_weight_bits: 8,
        out_major_rows: false,
    };
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut order: Vec<usize> = vec![hl.pre];
    for k in &hl.schedule {
        if !order.contains(&(*k as usize)) {
            order.push(*k as usize);
        }
    }
    order.push(hl.post);
    let mut out_node = None;
    for &hbk in &order {
        let (tb, o) = bidir_block(&mut pb, &mut cx, hbk, &a, cfg)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    let out_node = out_node.ok_or_else(|| LowerError::eval("internal: no output node"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], out_node);
    tir::validate::validate(&program)
        .map_err(|e| LowerError::eval(format!("internal: the encoder program is not in normal form: {e}")))?;
    let fills = cx.fills;
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no output scale"))?;
    Ok(Lowered { program, fills, row_params: cx.row_params, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks: vec![] })
}

/// A value of this lowering: `[L, n]` (or `[1, n]`) rows.
pub(super) fn rows_val(r: tir::Ref, dt: DType, key: ScaleKey, n: usize, site: &str) -> Val {
    Val { r, dt, key, len: n, site: site.to_string() }
}

pub(super) fn site_key(site: &str) -> ScaleKey {
    ScaleKey::site(vec![site.to_string()], false)
}

/// Record a committed node's site for the per-site diagnosis.
pub(super) fn note_site(cx: &mut Cx<'_>, tb: u8, v: &Val) {
    if let tir::Ref::Node(n) = v.r {
        cx.site_nodes.entry((tb, n)).or_insert((v.site.clone(), v.key.clone(), v.len));
    }
}

fn bidir_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, a: &Arch, cfg: &BidirCfg) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let blk = &hl.blocks[hbk];
    let d = hl.hidden;
    let l = cfg.lmax;
    let carry_sig = vec![TensorType::fixed(DType::I32, &[l, d as u32])];
    let prefixes: Vec<String> = match blk.role {
        BlockRole::Pre => vec!["pre.".into()],
        BlockRole::Post => vec!["post.".into()],
        BlockRole::Layer => {
            hl.schedule.iter().enumerate().filter(|(_, k)| **k as usize == hbk).map(|(li, _)| format!("L{li}.")).collect()
        }
    };
    let suffix = if blk.role == BlockRole::Layer && hl.schedule.first().map(|k| *k as usize) != Some(hbk) {
        format!("#{hbk}")
    } else {
        String::new()
    };
    let n = blk.nodes.len();
    let mut lb = Lb {
        hb: hbk,
        role: blk.role,
        prefixes,
        vals: Vec::new(),
        wants: vec![None; n],
        rope_sites: vec![Vec::new(); n],
        split: vec![0; n],
        windows: BTreeMap::new(),
        angles: BTreeMap::new(),
        requants: 0,
        absorbed: vec![false; n],
        gdn: BTreeMap::new(),
        ssm: BTreeMap::new(),
        wide: vec![false; n],
        w16: vec![false; n],
        w16_now: false,
        mrope_pos: None,
        suffix,
        appended: BTreeMap::new(),
        carry_in: Vec::new(),
        gx: Default::default(),
    };
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&blk.name, if blk.role == BlockRole::Pre { vec![] } else { carry_sig });
    let resid = ScaleKey::resid();
    match blk.role {
        BlockRole::Pre => {
            // The inputs, declared first so that lifting them leaves every weight's index alone.
            let ids = decl(&mut b, cx, &lb, IDS_PARAM, DType::Idx, &[l as usize], false, input_fill(DType::Idx, vec![l as usize]))?;
            let count = decl(&mut b, cx, &lb, COUNT_PARAM, DType::Idx, &[], false, input_fill(DType::Idx, vec![]))?;
            let _ = count;
            // Word rows, each at its table row's scale, narrowed to the residual scale.
            let tp = hl_param(hl, "embed.table")?;
            let (rows, cols) = (hl.params[tp as usize].shape[0], hl.params[tp as usize].shape[1]);
            let table = decl_rows(&mut b, cx, &lb, "embed.table", &[rows, cols], false, RowKind::T16, tp)?;
            let key = resid.clone();
            let (m, s) = decl_ms(
                &mut b,
                cx,
                &lb,
                "embed",
                rows,
                Arc::new(move |c| {
                    let scales = c.row_scales(tp, true)?;
                    let to = c.scale(&key)?;
                    Ok(scales.iter().map(|sw| sw / to).collect())
                }),
            )?;
            let row = b.gather(table, ids, 0, 0);
            let mt = b.gather(m, ids, 0, 0);
            let st = b.gather(s, ids, 0, 0);
            let mt = b.reshape_fixed(mt, &[l, 1]);
            let st = b.reshape_fixed(st, &[l, 1]);
            let word = narrow(&mut b, row, mt, st, None, DType::I32);
            // Position rows `offset + i` plus the token-type row 0, one param at the residual scale (a rotary encoder
            // has no position table: its rows are the token-type row alone, or nothing).
            let (pp, tp2) = (if a.has_positions { Some(hl_param(hl, "embed.pos_table")?) } else { None }, hl_param(hl, "embed.type_table").ok());
            let off = a.pos_offset;
            let lr = l as usize;
            let segments = a.pair_sep.is_some();
            let sum = if pp.is_none() && tp2.is_none() {
                word
            } else {
                let pos = decl(
                    &mut b,
                    cx,
                    &lb,
                    "embed.pos_rows",
                    DType::I32,
                    &[lr, cols],
                    false,
                    Arc::new(move |c| {
                        let sr = c.scale(&ScaleKey::resid())?;
                        let p = match pp {
                            Some(p) => Some(c.f(p)?),
                            None => None,
                        };
                        // With segments the type rows are a separate two-row table (below), not folded into the positions.
                        let t = match tp2 {
                            Some(t) if !segments => Some(c.f(t)?),
                            _ => None,
                        };
                        let mut v = Vec::with_capacity(lr * cols);
                        for i in 0..lr {
                            for j in 0..cols {
                                let x = p.as_ref().map_or(0.0, |p| p.data[(off + i) * cols + j] as f64) + t.as_ref().map_or(0.0, |t| t.data[j] as f64);
                                v.push((x / sr).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32);
                            }
                        }
                        Ok(IntTensor::i32(vec![lr, cols], v))
                    }),
                )?;
                let mut sum = b.add(word, pos, DType::I64);
                if let (Some(sep), Some(tt)) = (a.pair_sep, tp2) {
                    // **`ENC_PAIR_SEGMENTS_V1`**: the segment id of position `i` is 1 iff a separator sits at some `j < i`, computed
                    // from the job's ids: `ids == sep`, masked by the strictly-lower triangle (`j < i`), summed over `j`, clamped to
                    // {0, 1}, and used to gather from the two-row type table (rows 0 and 1, at the residual scale).
                    let ty = decl(
                        &mut b,
                        cx,
                        &lb,
                        "embed.type_rows",
                        DType::I32,
                        &[2, cols],
                        false,
                        Arc::new(move |c| {
                            let sr = c.scale(&ScaleKey::resid())?;
                            let t = c.f(tt)?;
                            let v: Vec<i32> = (0..2 * cols)
                                .map(|k| (t.data[k] as f64 / sr).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32)
                                .collect();
                            Ok(IntTensor::i32(vec![2, cols], v))
                        }),
                    )?;
                    let sepc = b.c(DType::Idx, sep as i128);
                    let is_sep = b.compare(ids, sepc, tir::Cmp::Eq);
                    let one = b.c(DType::I32, 1);
                    let zero = b.c(DType::I32, 0);
                    let m = b.select(is_sep, one, zero, DType::I32);
                    let m_row = b.reshape_fixed(m, &[1, l]);
                    let ii = b.iota(DType::Idx, &[Dim::Fixed(l), Dim::Fixed(1)], 0, 0, 1);
                    let jj = b.iota(DType::Idx, &[Dim::Fixed(1), Dim::Fixed(l)], 1, 0, 1);
                    let before = b.compare(jj, ii, tir::Cmp::Lt);
                    let hit = b.select(before, m_row, zero, DType::I32);
                    let cnt = b.reduce_sum(hit, 1, DType::I32);
                    let cnt = b.reshape_fixed(cnt, &[l]);
                    let seg = b.clamp(cnt, 0, 1, DType::I32);
                    let seg = b.cast(seg, DType::Idx);
                    let trow = b.gather(ty, seg, 0, 0);
                    sum = b.add(sum, trow, DType::I64);
                }
                b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32)
            };
            let sum = rows_val(sum, DType::I32, resid.clone(), cols, "embed.sum");
            note_resid(cx, &lb, &sum);
            let normed = match (a.embed_norm, a.proj_in) {
                (Some(n), Some(pb)) => {
                    // ALBERT: the normed row at the table's width, as codes, lifted by the projection.
                    let c = norm_rows_kind(&mut b, cx, &mut lb, &sum, n.kind, n.eps, "embed.norm", n.bias, &Want { dt: DType::I16, key: site_key("embed.norm") })?;
                    note_site(cx, tb, &c);
                    linear_rows(&mut b, cx, &mut lb, &c, "embed.proj_in.w", pb.then_some("embed.proj_in.b"), "embed.proj_in", &Want { dt: DType::I32, key: resid.clone() })?
                }
                (Some(n), None) => norm_rows_kind(&mut b, cx, &mut lb, &sum, n.kind, n.eps, "embed.norm", n.bias, &Want { dt: DType::I32, key: resid.clone() })?,
                (None, _) => sum,
            };
            b.commit(normed.r);
            note_resid(cx, &lb, &normed);
            note_site(cx, tb, &normed);
            Ok((b.finish(&[normed.r]), None))
        }
        BlockRole::Layer => {
            let la = a.of_block(hl, hbk).clone();
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let codes_want = |site: &str| Want { dt: DType::I16, key: site_key(site) };
            let (h, dh) = (la.heads, la.head_dim);
            // The attention's input codes: the pre-norm's output, or the residual stream's own codes (post-LN).
            let xc = match la.place {
                Place::Pre { mix: Some(n), .. } => norm_rows_kind(&mut b, cx, &mut lb, &x, n.kind, n.eps, "norm.mix", n.bias, &codes_want("norm.mix"))?,
                _ => codes_rows(&mut b, cx, &mut lb, &x)?,
            };
            // The rotary tables: cos and sin per position, Q24, `[1, L, dh]` (the half repeated), one pair per layer kind.
            let rope_tabs = match &la.rope {
                Some(rs) => {
                    let (lu, dhu) = (l as usize, dh as usize);
                    let freqs = rs.freqs.clone();
                    let tab = move |sin: bool| -> FillFn {
                        let freqs = freqs.clone();
                        Arc::new(move |_| {
                            let mut v = Vec::with_capacity(lu * dhu);
                            for pos in 0..lu {
                                let (c, s) = freqs.cos_sin(pos);
                                let t = if sin { &s } else { &c };
                                for j in 0..dhu {
                                    v.push((t[j % (dhu / 2)] as f64 * (1u64 << 24) as f64).round() as i32);
                                }
                            }
                            Ok(IntTensor::i32(vec![1, lu, dhu], v))
                        })
                    };
                    let cos = decl(&mut b, cx, &lb, "rope.cos", DType::I32, &[1, lu, dhu], false, tab(false))?;
                    let sin = decl(&mut b, cx, &lb, "rope.sin", DType::I32, &[1, lu, dhu], false, tab(true))?;
                    Some((cos, sin))
                }
                None => None,
            };
            // q, k, v: codes [L, d], committed head-major [h, L, dh] (one head's K/V is whole leaves), q and k rotated.
            let heads_of = |b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, name: &str, bias: bool, rotate: bool, slot: &mut Option<tir::Ref>| -> Result<Val> {
                let bn = format!("{name}.b");
                let v = linear_rows_shared(b, cx, lb, &xc, &format!("{name}.w"), if bias { Some(bn.as_str()) } else { None }, name, &Want { dt: DType::I16, key: site_key(name) }, slot)?;
                let r = b.reshape_fixed(v.r, &[l, h, dh]);
                let mut r = b.transpose(r, &[1, 0, 2]);
                if let (true, Some((cos, sin))) = (rotate, rope_tabs) {
                    // x·cos + rotate_half(x)·sin, rotate_half = [−x2, x1].
                    let x1 = b.slice(r, 2, 0, dh / 2);
                    let x2 = b.slice(r, 2, dh / 2, dh / 2);
                    let zero = b.c(DType::I16, 0);
                    let nx2 = b.sub(zero, x2, DType::I16);
                    let rh = b.concat(&[nx2, x1], 2);
                    let ac = b.mul(r, cos, DType::I64);
                    let bs = b.mul(rh, sin, DType::I64);
                    let sum = b.add(ac, bs, DType::I64);
                    let rot = b.shr(sum, 24, tir::Rounding::HalfAwayFromZero, DType::I64);
                    r = b.clamp(rot, -32767, 32767, DType::I16);
                }
                b.commit(r);
                let hv = Val { r, ..v };
                note_site(cx, tb, &hv);
                Ok(hv)
            };
            let (mut qslot, mut kslot, mut vslot) = (None, None, None);
            let q = heads_of(&mut b, cx, &mut lb, "attn.q", la.bias[0], true, &mut qslot)?;
            let k = heads_of(&mut b, cx, &mut lb, "attn.k", la.bias[1], true, &mut kslot)?;
            let v = heads_of(&mut b, cx, &mut lb, "attn.v", la.bias[2], false, &mut vslot)?;
            // Scores: q·kᵀ (exact i64) to Q14 logits, the 1/√d scale and both code scales in m.
            let kt = b.transpose(k.r, &[0, 2, 1]);
            let s = b.matmul(q.r, kt, DType::I64);
            let (kq, kk, sc) = (q.key.clone(), k.key.clone(), la.scale);
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "attn.score",
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * sc * (1u64 << LOGIT_Q) as f64])),
            )?;
            let mut logits = narrow(&mut b, s, m, sh, None, DType::I32);
            // DeBERTa: the relative-position table, normed, goes through this layer's own key and query weights, and its
            // products with q (content to position) and k (position to content) are read at the bucket of (i − j).
            if let Some(dis) = a.dis {
                let (span2, lu) = (2 * dis.span, l as usize);
                let (tp, gp, bp) = (hl_param(hl, "rel.table")?, dis.norm.map(|_| hl_param(hl, "rel.norm.gain")).transpose()?, if dis.norm.is_some_and(|n| n.bias) { Some(hl_param(hl, "rel.norm.bias")?) } else { None });
                let rel_key = site_key("rel.rows");
                let rk = rel_key.clone();
                let rel_rows = decl(
                    &mut b,
                    cx,
                    &lb,
                    "rel.rows",
                    DType::I16,
                    &[span2, d],
                    false,
                    Arc::new(move |c| {
                        let t = c.f(tp)?;
                        let sc = c.scale(&rk)?;
                        let g = match gp {
                            Some(g) => Some(c.f(g)?.data.iter().map(|x| *x as f64).collect::<Vec<_>>()),
                            None => None,
                        };
                        let bi = match bp {
                            Some(b) => Some(c.f(b)?.data.iter().map(|x| *x as f64).collect::<Vec<_>>()),
                            None => None,
                        };
                        let mut v = Vec::with_capacity(span2 * d);
                        for r in 0..span2 {
                            let row: Vec<f64> = t.data[r * d..(r + 1) * d].iter().map(|x| *x as f64).collect();
                            let row = match (&g, dis.norm) {
                                (Some(g), Some(n)) => norm_row(n.kind, n.eps, &row, g, bi.as_deref()),
                                _ => row,
                            };
                            v.extend(row.iter().map(|x| (x / sc).round().clamp(-32767.0, 32767.0) as i16));
                        }
                        Ok(IntTensor::i16(vec![span2, d], v))
                    }),
                )?;
                let relv = rows_val(rel_rows, DType::I16, rel_key, d, "rel.rows");
                let ids: Vec<u32> = (0..lu).flat_map(|i| (0..lu).map(move |j| deberta_index(i, j, dis.span, dis.max_position) as u32)).collect();
                let idx = decl(&mut b, cx, &lb, "attn.rel_index", DType::Idx, &[lu, lu], false, Arc::new(move |_| Ok(IntTensor::idx(vec![lu, lu], ids.clone()))))?;
                let idx = b.clamp(idx, 0, span2 as i64 - 1, DType::Idx); // admission proves the gather's range from the clamp
                let mut terms: Vec<tir::Ref> = Vec::new();
                for (c2p, name, slot, bias, side) in [(true, "attn.pos_k", &mut kslot, la.bias[1], &q), (false, "attn.pos_q", &mut qslot, la.bias[0], &k)] {
                    if (c2p && !dis.c2p) || (!c2p && !dis.p2c) {
                        continue;
                    }
                    let (wname, bname) = if c2p { ("attn.k.w", "attn.k.b") } else { ("attn.q.w", "attn.q.b") };
                    let pos = linear_rows_shared(&mut b, cx, &mut lb, &relv, wname, bias.then_some(bname), name, &Want { dt: DType::I16, key: site_key(name) }, slot)?;
                    let pr = b.reshape_fixed(pos.r, &[span2 as u32, h, dh]);
                    let pt = b.transpose(pr, &[1, 2, 0]); // [h, dh, 2s]
                    // c2p: q [h, L, dh] · pos_k; p2c: k [h, L, dh] · pos_q — both [h, L, 2s].
                    let operand = if c2p { q.r } else { k.r };
                    let acc = b.matmul(operand, pt, DType::I64);
                    let (ko, kp, scl) = (side.key.clone(), pos.key.clone(), la.scale);
                    let site = if c2p { "attn.c2p" } else { "attn.p2c" };
                    let (m, sh) = decl_ms(&mut b, cx, &lb, site, 1, Arc::new(move |cc| Ok(vec![cc.scale(&ko)? * cc.scale(&kp)? * scl * (1u64 << LOGIT_Q) as f64])))?;
                    let prod = narrow(&mut b, acc, m, sh, None, DType::I32);
                    b.commit(prod);
                    let by_row = b.transpose(prod, &[1, 0, 2]); // [L, h, 2s], the rows (i for c2p, j for p2c) the gather is batched over
                    let term = if c2p {
                        let g = b.gather(by_row, idx, 2, 1); // [L(i), h, L(j)]
                        b.transpose(g, &[1, 0, 2])
                    } else {
                        let idx_t = b.transpose(idx, &[1, 0]);
                        let g = b.gather(by_row, idx_t, 2, 1); // [L(j), h, L(i)]
                        b.transpose(g, &[1, 2, 0])
                    };
                    terms.push(term);
                }
                for t in terms {
                    let sum = b.add(logits, t, DType::I64);
                    logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
                }
            }
            // MPNet: `table[bucket(j − i), head]` in Q`LOGIT_Q` on the scaled scores, one table and
            // one pinned `[L, L]` bucket map for every layer.
            if let Some(rb) = a.rel {
                let tp = hl_param(hl, "attn.rel_bias")?;
                let (nb, hh) = (rb.buckets, h as usize);
                let table = decl(
                    &mut b,
                    cx,
                    &lb,
                    "attn.rel_bias.q",
                    DType::I32,
                    &[nb, hh],
                    false,
                    Arc::new(move |c| {
                        let t = c.f(tp)?;
                        Ok(IntTensor::i32(
                            vec![nb, hh],
                            t.data.iter().map(|v| (*v as f64 * (1u64 << LOGIT_Q) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).collect(),
                        ))
                    }),
                )?;
                let lu = l as usize;
                let ids: Vec<u32> = (0..lu)
                    .flat_map(|i| (0..lu).map(move |j| super::encdec::t5_bucket(j as i64 - i as i64, true, rb.buckets, rb.max_distance) as u32))
                    .collect();
                let bk = decl(&mut b, cx, &lb, "attn.buckets", DType::Idx, &[lu, lu], false, Arc::new(move |_| Ok(IntTensor::idx(vec![lu, lu], ids.clone()))))?;
                let bk = b.clamp(bk, 0, nb as i64 - 1, DType::Idx);
                let bias = b.gather(table, bk, 0, 0); // [L, L, h]
                let bias = b.transpose(bias, &[2, 0, 1]);
                let sum = b.add(logits, bias, DType::I64);
                logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
            }
            // The mask: keys at or past `count` score i32::MIN, which IntExp maps to exactly 0.
            let count = b.pb.params.iter().position(|p| p.name == COUNT_PARAM);
            let count = match count {
                Some(i) => tir::Ref::Param(i as u16),
                None => return Err(LowerError::eval("internal: no count input")),
            };
            let iota = b.iota(DType::Idx, &[Dim::Fixed(l)], 0, 0, 1);
            let keep = b.compare(iota, count, tir::Cmp::Lt);
            let neg = b.c(DType::I32, i32::MIN as i128);
            let mut masked = b.select(keep, logits, neg, DType::I32);
            // A sliding window of a bidirectional encoder: the band `|i − j| < w` (`i < j + w` and `j < i + w`).
            if let Some(w) = la.band {
                let w = w as i64;
                let (ri, rj) = (b.iota(DType::Idx, &[Dim::Fixed(l), Dim::Fixed(1)], 0, 0, 1), b.iota(DType::Idx, &[Dim::Fixed(1), Dim::Fixed(l)], 1, 0, 1));
                let (riw, rjw) = (b.iota(DType::Idx, &[Dim::Fixed(l), Dim::Fixed(1)], 0, w, 1), b.iota(DType::Idx, &[Dim::Fixed(1), Dim::Fixed(l)], 1, w, 1));
                let near_hi = b.compare(ri, rjw, tir::Cmp::Lt);
                let near_lo = b.compare(rj, riw, tir::Cmp::Lt);
                masked = b.select(near_hi, masked, neg, DType::I32);
                masked = b.select(near_lo, masked, neg, DType::I32);
            }
            let p = softmax_rows(&mut b, masked, h, l, dh);
            // Context: P·V (Q24 × codes, exact i64) back to codes, [h, L, dh] → [L, h·dh].
            let o = b.matmul(p, v.r, DType::I64);
            let ctx_key = site_key("attn.ctx");
            let (kv, kc) = (v.key.clone(), ctx_key.clone());
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "attn.ctx",
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kv)? / (1u64 << 24) as f64 / c.scale(&kc)?])),
            )?;
            let ctx = narrow(&mut b, o, m, sh, None, DType::I16);
            b.commit(ctx);
            let ctxv = rows_val(ctx, DType::I16, ctx_key, (h * dh) as usize, "attn.ctx");
            note_site(cx, tb, &ctxv);
            let ctx = b.transpose(ctx, &[1, 0, 2]);
            let ctx = b.reshape_fixed(ctx, &[l, h * dh]);
            let ctxv = Val { r: ctx, ..ctxv };
            let ob = la.bias[3].then_some("attn.o.b");
            let att = linear_rows(&mut b, cx, &mut lb, &ctxv, "attn.o.w", ob, "attn.o", &Want { dt: DType::I32, key: resid.clone() })?;
            let r1 = add_rows(&mut b, cx, &lb, &x, &att, "resid.mix")?;
            let r1_committed = resid_commit_needed(l, d, (h * dh) as usize);
            if r1_committed {
                b.commit(r1.r);
            }
            // Post-LN normalises the sum; pre-norm keeps the stream and normalises the FFN's input.
            let (x1, n2) = match la.place {
                Place::Post { mix, .. } => {
                    let x1 = norm_rows_kind(&mut b, cx, &mut lb, &r1, mix.kind, mix.eps, "norm.mix", mix.bias, &Want { dt: DType::I32, key: resid.clone() })?;
                    b.commit(x1.r);
                    note_resid(cx, &lb, &x1);
                    note_site(cx, tb, &x1);
                    let c = codes_rows(&mut b, cx, &mut lb, &x1)?;
                    (x1, c)
                }
                Place::Pre { ffn, .. } => {
                    if !r1_committed {
                        b.commit(r1.r);
                    }
                    let c = match ffn {
                        Some(n) => norm_rows_kind(&mut b, cx, &mut lb, &r1, n.kind, n.eps, "norm.ffn", n.bias, &codes_want("norm.ffn"))?,
                        None => codes_rows(&mut b, cx, &mut lb, &r1)?,
                    };
                    (r1.clone(), c)
                }
            };
            let ub = la.up_bias.then_some("mlp.up.b");
            let inter = hl.params[hl_param(hl, "mlp.down.w")? as usize].shape[1];
            let hidden = if la.gated {
                let gate = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.gate.w", ub.map(|_| "mlp.gate.b"), "mlp.gate", &codes_want("mlp.gate"))?;
                let ga = lower_table_named(&mut b, cx, &mut lb, &gate, TableFn::Act(la.act), "mlp.gate_act")?;
                let up = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.up.w", ub, "mlp.up", &codes_want("mlp.up"))?;
                let prod = b.mul(ga.r, up.r, DType::I32);
                let pk = site_key("mlp.prod");
                let (ka, ku, kp) = (ga.key.clone(), up.key.clone(), pk.clone());
                let (m, sh) = decl_ms(&mut b, cx, &lb, "mlp.prod", 1, Arc::new(move |c| Ok(vec![c.scale(&ka)? * c.scale(&ku)? / c.scale(&kp)?])))?;
                let r = narrow(&mut b, prod, m, sh, None, DType::I16);
                b.commit(r);
                let pv = rows_val(r, DType::I16, pk, inter, "mlp.prod");
                note_site(cx, tb, &pv);
                pv
            } else {
                let up = linear_rows(&mut b, cx, &mut lb, &n2, "mlp.up.w", ub, "mlp.up", &codes_want("mlp.up"))?;
                let act = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(la.act), "mlp.act")?;
                note_site(cx, tb, &act);
                act
            };
            let db = la.down_bias.then_some("mlp.down.b");
            let down = linear_rows(&mut b, cx, &mut lb, &hidden, "mlp.down.w", db, "mlp.down", &Want { dt: DType::I32, key: resid.clone() })?;
            let r2 = add_rows(&mut b, cx, &lb, &x1, &down, "resid.ffn")?;
            if resid_commit_needed(l, d, inter) {
                b.commit(r2.r);
            }
            let out = match la.place {
                Place::Post { ffn, .. } => norm_rows_kind(&mut b, cx, &mut lb, &r2, ffn.kind, ffn.eps, "norm.ffn", ffn.bias, &Want { dt: DType::I32, key: resid.clone() })?,
                Place::Pre { .. } => r2,
            };
            note_resid(cx, &lb, &out);
            note_site(cx, tb, &out);
            Ok((b.finish(&[out.r]), None))
        }
        BlockRole::Post => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let x = match a.final_norm {
                Some(n) => {
                    let v = norm_rows_kind(&mut b, cx, &mut lb, &x, n.kind, n.eps, "final_norm", n.bias, &Want { dt: DType::I32, key: resid.clone() })?;
                    b.commit(v.r);
                    note_resid(cx, &lb, &v);
                    note_site(cx, tb, &v);
                    v
                }
                None => x,
            };
            // **A masked-LM head** (`OutputSpec::MaskedLm`): every row through the head's transform and the vocabulary projection,
            // `[L, vocab]` in one power-of-two unit. The program reads no mask position; a pad row is computed like any other.
            if let Some(m) = a.mlm {
                if cfg.normalize {
                    return Err(LowerError::not_lowerable("a masked-LM head reads the encoder's rows, un-normalised"));
                }
                let codes_want = |site: &str| Want { dt: DType::I16, key: site_key(site) };
                let h0 = codes_rows(&mut b, cx, &mut lb, &x)?;
                let up = linear_rows(
                    &mut b,
                    cx,
                    &mut lb,
                    &h0,
                    "head.transform.dense.w",
                    m.dense_bias.then_some("head.transform.dense.b"),
                    "mlm.dense",
                    &codes_want("mlm.dense"),
                )?;
                note_site(cx, tb, &up);
                let act = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(m.act), "mlm.act")?;
                note_site(cx, tb, &act);
                let nrm = norm_rows_kind(
                    &mut b,
                    cx,
                    &mut lb,
                    &act,
                    m.norm.kind,
                    m.norm.eps,
                    "head.transform.norm",
                    m.norm.bias,
                    &codes_want("mlm.norm"),
                )?;
                note_site(cx, tb, &nrm);
                let key = ScaleKey { base: Base::Pow2Site { names: vec!["mlm.out".into()] }, factor: 1.0 };
                let out = linear_rows(
                    &mut b,
                    cx,
                    &mut lb,
                    &nrm,
                    "head.w",
                    m.head_bias.then_some("head.b"),
                    "mlm.out",
                    &Want { dt: DType::I32, key },
                )?;
                let out = ensure_node(&mut b, &out);
                let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
                b.commit(out.r);
                note_site(cx, tb, &out);
                cx.logits_key = Some(out.key.clone());
                return Ok((b.finish(&[]), Some(oi)));
            }
            // **Per-token logits** (`OUTPUT_TOKEN_LOGITS_V1`): every row through the classification layer, `[L, labels]` in one
            // power-of-two unit; no pooling. A pad row is computed like any other.
            if let Some(tbias) = a.token_logits {
                if cfg.normalize {
                    return Err(LowerError::not_lowerable("per-token logits read the encoder's rows, un-normalised"));
                }
                let h = codes_rows(&mut b, cx, &mut lb, &x)?;
                let key = ScaleKey { base: Base::Pow2Site { names: vec!["tok.out".into()] }, factor: 1.0 };
                let out = linear_rows(
                    &mut b,
                    cx,
                    &mut lb,
                    &h,
                    "classifier.out.w",
                    tbias.then_some("classifier.out.b"),
                    "tok.out",
                    &Want { dt: DType::I32, key },
                )?;
                let out = ensure_node(&mut b, &out);
                let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
                b.commit(out.r);
                note_site(cx, tb, &out);
                cx.logits_key = Some(out.key.clone());
                return Ok((b.finish(&[]), Some(oi)));
            }
            let pooled = match cfg.pooling {
                Pooling::Cls => b.slice(x.r, 0, 0, 1),
                Pooling::Mean => {
                    let count = match b.pb.params.iter().position(|p| p.name == COUNT_PARAM) {
                        Some(i) => tir::Ref::Param(i as u16),
                        None => return Err(LowerError::eval("internal: no count input")),
                    };
                    let iota = b.iota(DType::Idx, &[Dim::Fixed(l), Dim::Fixed(1)], 0, 0, 1);
                    let keep = b.compare(iota, count, tir::Cmp::Lt);
                    let zero = b.c(DType::I32, 0);
                    let kept = b.select(keep, x.r, zero, DType::I32);
                    let sum = b.reduce_sum(kept, 0, DType::I64);
                    // `count ≥ 1` for every template (it holds the template's own ids); the clamp
                    // keeps the division total.
                    let n = b.clamp(count, 1, l as i64, DType::I64);
                    let mean = b.div(sum, n, tir::Rounding::HalfAwayFromZero, DType::I64);
                    b.clamp(mean, i32::MIN as i64, i32::MAX as i64, DType::I32)
                }
            };
            b.commit(pooled);
            let pv = rows_val(pooled, DType::I32, resid.clone(), d, "pool");
            note_resid(cx, &lb, &pv);
            note_site(cx, tb, &pv);
            // **A classification head** (`OUTPUT_CLASSIFY_V1`): the `[CLS]` row through the optional dense + activation (BERT's pooler,
            // RoBERTa's `dense` + tanh, DistilBERT's `pre_classifier` + ReLU) and the linear layer to the labels; the output is the
            // labels' logits in one power-of-two unit.
            if let Some((cbias, pre)) = a.classify {
                if cfg.pooling != Pooling::Cls || cfg.normalize {
                    return Err(LowerError::not_lowerable("a classification head reads the [CLS] row, un-normalised"));
                }
                let codes_want = |site: &str| Want { dt: DType::I16, key: site_key(site) };
                let mut h = codes_rows(&mut b, cx, &mut lb, &pv)?;
                if let Some(p) = pre {
                    let up = linear_rows(&mut b, cx, &mut lb, &h, "classifier.pre.w", p.bias.then_some("classifier.pre.b"), "cls.pre", &codes_want("cls.pre"))?;
                    note_site(cx, tb, &up);
                    h = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(p.act), "cls.act")?;
                    note_site(cx, tb, &h);
                }
                let key = ScaleKey { base: Base::Pow2Site { names: vec!["cls.out".into()] }, factor: 1.0 };
                let out = linear_rows(&mut b, cx, &mut lb, &h, "classifier.out.w", cbias.then_some("classifier.out.b"), "cls.out", &Want { dt: DType::I32, key })?;
                let out = ensure_node(&mut b, &out);
                let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
                b.commit(out.r);
                note_site(cx, tb, &out);
                cx.logits_key = Some(out.key.clone());
                return Ok((b.finish(&[]), Some(oi)));
            }
            let out = if cfg.normalize {
                let pc = codes_rows(&mut b, cx, &mut lb, &pv)?;
                let u = b.l2_unit_q15(pc.r);
                let key = ScaleKey { base: Base::Fixed(1.0 / (1u64 << 30) as f64), factor: 1.0 };
                let f = b.c(DType::I32, 1 << 15);
                let r = b.mul(u, f, DType::I32);
                rows_val(r, DType::I32, key, d, "embed.normed")
            } else {
                let key = ScaleKey { base: Base::Pow2Site { names: vec!["pool".into()] }, factor: 1.0 };
                coerce(&mut b, cx, &mut lb, &pv, DType::I32, &key)?
            };
            let out = ensure_node(&mut b, &out);
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            note_site(cx, tb, &out);
            cx.logits_key = Some(out.key.clone());
            Ok((b.finish(&[]), Some(oi)))
        }
    }
}

/// A zero-filled input param (the program's inputs are lifted before it runs; a version-1 run of
/// the lowered program gets its inputs by overwriting these).
pub(super) fn input_fill(dt: DType, shape: Vec<usize>) -> FillFn {
    Arc::new(move |_c| {
        let n: usize = shape.iter().product();
        Ok(match dt {
            DType::Idx => IntTensor { dtype: DType::Idx, shape: shape.clone(), data: crate::lower::IntData::Idx(vec![0; n]) },
            DType::I16 => IntTensor::i16(shape.clone(), vec![0; n]),
            _ => IntTensor::i32(shape.clone(), vec![0; n]),
        })
    })
}

/// `i16` codes of a row value at its own site's scale (the rows' version of `codes`).
pub(super) fn codes_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: &Val) -> Result<Val> {
    if v.dt == DType::I16 {
        return Ok(v.clone());
    }
    let key = site_key(&v.site);
    let (from, to) = (v.key.clone(), key.clone());
    let (m, s) = decl_ms(b, cx, lb, &format!("{}.rq", v.site), 1, Arc::new(move |c| Ok(vec![c.scale(&from)? / c.scale(&to)?])))?;
    let r = narrow(b, v.r, m, s, None, DType::I16);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key, len: v.len, site: v.site.clone() })
}

/// `x + y` of two residual-scale rows.
pub(super) fn add_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, x: &Val, y: &Val, site: &str) -> Result<Val> {
    if !x.key.same(&ScaleKey::resid()) || !y.key.same(&ScaleKey::resid()) {
        return Err(LowerError::eval(format!("internal: `{site}` adds values off the residual scale")));
    }
    let s = b.add(x.r, y.r, DType::I64);
    let r = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let v = rows_val(r, DType::I32, ScaleKey::resid(), x.len, site);
    note_resid(cx, lb, &v);
    Ok(v)
}

/// `x·Wᵀ + b` over rows: `x:code[L, in]`, `W:i8[out, in]` per-row codes, stored transposed as
/// `[in, out]` so the product is one `MatMul` into `[L, out]`; narrowed per output channel.
#[allow(clippy::too_many_arguments)]
pub(super) fn linear_rows(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    w_name: &str,
    b_name: Option<&str>,
    site: &str,
    want: &Want,
) -> Result<Val> {
    linear_rows_shared(b, cx, lb, x, w_name, b_name, site, want, &mut None)
}

/// [`linear_rows`] reading a weight another product of this block already declared (`wt`, the `[in, out]` codes): the
/// weight is stored once and a second input (DeBERTa's relative-position rows) goes through the same matrix. The slot is
/// filled by the first call.
#[allow(clippy::too_many_arguments)]
pub(super) fn linear_rows_shared(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    w_name: &str,
    b_name: Option<&str>,
    site: &str,
    want: &Want,
    wt_slot: &mut Option<tir::Ref>,
) -> Result<Val> {
    let hl = cx.hl;
    let w = hl_param(hl, w_name)?;
    let (out, inp) = (hl.params[w as usize].shape[0], hl.params[w as usize].shape[1]);
    if x.dt != DType::I16 || x.len != inp {
        return Err(LowerError::eval(format!("internal: `{site}` reads {:?} rows of {}, W has {inp} columns", x.dt, x.len)));
    }
    let pl = per_layer(lb);
    let out_major = cx.out_major_rows;
    let wt = match *wt_slot {
        Some(r) => r,
        None if out_major => {
            // `[out, in]`, the rows' codes as stored: a tile of output channels reads whole contiguous rows.
            let r = decl(
                b,
                cx,
                lb,
                &format!("{w_name}.o"),
                DType::I8,
                &[out, inp],
                pl,
                Arc::new(move |c| {
                    let rc = c.rows(w)?;
                    Ok(IntTensor::i8(vec![out, inp], rc.codes.clone()))
                }),
            )?;
            *wt_slot = Some(r);
            r
        }
        None => {
            let r = decl(
                b,
                cx,
                lb,
                &format!("{w_name}.t"),
                DType::I8,
                &[inp, out],
                pl,
                Arc::new(move |c| {
                    let rc = c.rows(w)?;
                    let mut t = vec![0i8; inp * out];
                    for o in 0..out {
                        for i in 0..inp {
                            t[i * out + o] = rc.codes[o * inp + i];
                        }
                    }
                    Ok(IntTensor::i8(vec![inp, out], t))
                }),
            )?;
            *wt_slot = Some(r);
            r
        }
    };
    let (kx, ky) = (x.key.clone(), want.key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        out,
        Arc::new(move |c| {
            let scales = c.rows(w)?.scales.clone();
            let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
            Ok(scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
        }),
    )?;
    let z = match b_name {
        Some(bn) => {
            let bp = hl_param(hl, bn)?;
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
                    let sy = c.scale_vec(&ky, out)?;
                    Ok(IntTensor::i64(vec![out], bv.data.iter().zip(&sy).map(|(v, s)| (*v as f64 / s).round() as i64).collect()))
                }),
            )?)
        }
        None => None,
    };
    let acc = if out_major {
        // `[1, out, in] · [L, in, 1] → [L, out, 1]` (a batched product over the rows, no transpose): the same sums as `X · Wᵀ`,
        // element for element; output element `(r, o)` reads weight row `o` and input row `r`, both contiguous.
        let rows = match b.shape(x.r).first() {
            Some(tir::Dim::Fixed(n)) => *n,
            _ => return Err(LowerError::eval(format!("internal: `{site}` rows of no fixed count"))),
        };
        let w3 = b.reshape_fixed(wt, &[1, out as u32, inp as u32]);
        let x3 = b.reshape_fixed(x.r, &[rows, inp as u32, 1]);
        let acc3 = b.matmul(w3, x3, DType::I64);
        b.reshape_fixed(acc3, &[rows, out as u32])
    } else {
        b.matmul(x.r, wt, DType::I64)
    };
    let r = narrow(b, acc, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = rows_val(r, want.dt, want.key.clone(), out, site);
    note_resid(cx, lb, &v);
    Ok(v)
}

/// LayerNorm or RMSNorm along the last axis of `[L, n]` rows, gain `{site}.gain` and (when
/// `bias`) bias `{site}.bias`. LayerNorm centres exactly (`c = n·x − Σx`, brought into `i32` by
/// `2^k`); both then take the Q24 unit row and one per-channel narrowing with the gain as `m`.
#[allow(clippy::too_many_arguments)]
pub(super) fn norm_rows_kind(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    kind: NormKind,
    eps: f64,
    site: &str,
    bias: bool,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let n = x.len;
    let gp = hl_param(hl, &format!("{site}.gain"))?;
    let bp = if bias { Some(hl_param(hl, &format!("{site}.bias"))?) } else { None };
    let in_bits: i32 = if x.dt == DType::I16 { 16 } else { 32 };
    let k = match kind {
        NormKind::Layer => (in_bits + (n as f64).log2().ceil() as i32 - 31).max(0) as u32,
        NormKind::Rms => 0,
    };
    let kx = x.key.clone();
    let eps_q = Arc::new(move |c: &FillCtx<'_>| -> Result<f64> {
        let sx = c.scale(&kx)?;
        let base = eps * (1u64 << 24) as f64 / (sx * sx);
        Ok(match kind {
            NormKind::Layer => base * (n * n) as f64 / 4f64.powi(k as i32),
            NormKind::Rms => base,
        })
    });
    let eps_p = decl_eps(b, cx, lb, site, eps_q)?;
    let c = match kind {
        NormKind::Layer => {
            let axis = b.shape(x.r).len() - 1;
            let nn = b.c(DType::I64, n as i128);
            let nx = b.mul(x.r, nn, DType::I64);
            let sum = b.reduce_sum(x.r, axis, DType::I64);
            let c = b.sub(nx, sum, DType::I64);
            let c = if k > 0 { b.shr(c, k, tir::Rounding::HalfAwayFromZero, DType::I64) } else { c };
            b.clamp(c, i32::MIN as i64, i32::MAX as i64, DType::I32)
        }
        NormKind::Rms => x.r,
    };
    let u = rms_unit(b, c, eps_p);
    let ky = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let sy = c.scale_vec(&ky, n)?;
            let g = c.f(gp)?.data.clone();
            Ok((0..n).map(|i| g[i] as f64 / (1u64 << 24) as f64 / sy[i]).collect())
        }),
    )?;
    let ky = want.key.clone();
    let pl = per_layer(lb);
    let z = match bp {
        Some(bp) => Some(decl(
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
                Ok(IntTensor::i64(vec![n], (0..n).map(|i| (bv.data[i] as f64 / sy[i]).round() as i64).collect()))
            }),
        )?),
        None => None,
    };
    let r = narrow(b, u, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
}

// ───────────────────────────── admission at real sizes ─────────────────────────────
//
// Admission charges a commit point's tile by box demand (spec 04b §10.3): an operand of a `MatMul`
// is demanded `d · contraction` elements for `d` demanded outputs, and a reduction's operand `d`
// times the reduced axis. Over a fixed token axis that widens two cones past the legacy court's
// tile ceilings at real sizes (BERT-base at 128 tokens: 75 Mi MACs a tile, against 16 Mi):
// * the softmax's row maximum and row sum, broadcast back over their rows, reach the whole
//   `[h, L, L]` score matrix from any tile downstream of it ([`split_softmax`]);
// * a norm reduces a whole row, so a tile of it reaches `d` columns of every row of the projection
//   its input came from ([`resid_commit_needed`]).
// The fix is commit points, which change no value: the masked logits, the row maximum and the row
// reciprocal, and the residual sum a norm reads.

/// Whether a softmax over a fixed axis of `l` keys, `h` heads of `dh`, is split at commit points
/// ([`softmax_committed`]): unsplit, a tile of anything downstream costs `h·L²·dh` MACs and `h·L²`
/// exponentials. Past half the legacy court's tile ceilings it is split.
pub(super) fn split_softmax(h: u32, l: u32, dh: u32) -> bool {
    let c = tir::admit::TirCeilingsV1::legacy_court_v1();
    let (h, l, dh) = (h as u64, l as u64, dh as u64);
    h * l * l * dh > c.max_tile_macs / 2 || h * l * l > c.max_tile_transcendentals / 2
}

/// The library's `softmax_shifted` over the last axis, bit for bit, with its input (the masked
/// logits), the row maximum and the row reciprocal as commit points. A tile of anything downstream
/// then recomputes `L` exponentials a demanded row and opens those rows; a logits tile costs one
/// score's `dh` MACs. The reciprocal is `i32` (`[2^24 / L, 2^24]`: the row maximum contributes
/// `IntExp(0) = 2^24` to the sum), so its clamp never fires.
pub(super) fn softmax_committed(b: &mut BlockBuilder<'_>, x: tir::Ref, up_bits: u32) -> tir::Ref {
    b.commit(x);
    let axis = b.shape(x).len() - 1;
    let up = up_bits.min(62);
    let max = b.reduce_max(x, axis);
    b.commit(max);
    let diff = b.sub(x, max, DType::I64);
    let d = b.clamp(diff, (i32::MIN as i64) >> up, 0, DType::I64);
    let scale = b.c(DType::I64, 1i128 << up);
    let w = b.mul(d, scale, DType::I64);
    let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(arg);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let recip = b.int_recip(sum);
    let recip = b.clamp(recip, 0, i32::MAX as i64, DType::I32);
    b.commit(recip);
    let p = b.mul(e, recip, DType::I128);
    let q = b.shr(p, tir::arith::K, tir::Rounding::Floor, DType::I64);
    b.clamp(q, 0, 1 << 25, DType::I32)
}

/// The softmax of an encoder's attention over its `l` rows: split at commit points when
/// [`split_softmax`] says so, else the library's.
pub(super) fn softmax_rows(b: &mut BlockBuilder<'_>, x: tir::Ref, h: u32, l: u32, dh: u32) -> tir::Ref {
    if split_softmax(h, l, dh) { softmax_committed(b, x, 24 - LOGIT_Q) } else { b.softmax_shifted(x, 24 - LOGIT_Q) }
}

/// Whether the residual sum a norm reads is committed: `x + y` with `y` a projection over a
/// `contraction`-wide input, rows of `n`. Unsplit, a tile of the norm (64 lanes) reaches
/// `min(64, L)·n` outputs of the projection, each `contraction` MACs.
pub(super) fn resid_commit_needed(l: u32, n: usize, contraction: usize) -> bool {
    let c = tir::admit::TirCeilingsV1::legacy_court_v1();
    (l.min(64) as u64) * n as u64 * contraction as u64 > c.max_tile_macs / 2
}

// ───────────────────────────── the float reference ─────────────────────────────

/// One padded sequence: the ids `[L]` and the unpadded length.
#[derive(Clone, Debug)]
pub struct Padded {
    pub ids: Vec<usize>,
    pub count: usize,
}

/// The float encoder over one padded sequence: the pooled output (normalised when the class
/// says), and, when `stats` is given, every site's statistics over the real rows (keys as the
/// lowering's: `pre.embed.sum`, `L3.attn.q`, `post.pool`, …).
pub fn float_forward(
    hl: &HlProgram,
    spec: &ArchSpec,
    cfg: &BidirCfg,
    params: &ParamStore,
    seq: &Padded,
    stats: Option<&mut BTreeMap<String, SiteStat>>,
) -> Result<Vec<f64>> {
    float_forward_with(hl, spec, cfg, &BidirExtras::default(), params, seq, stats)
}

/// [`float_forward`] with the class's [`BidirExtras`].
pub fn float_forward_with(
    hl: &HlProgram,
    spec: &ArchSpec,
    cfg: &BidirCfg,
    extras: &BidirExtras,
    params: &ParamStore,
    seq: &Padded,
    mut stats: Option<&mut BTreeMap<String, SiteStat>>,
) -> Result<Vec<f64>> {
    let mut a = arch_of(spec)?;
    a.pair_sep = extras.pair_sep.filter(|_| spec.embedding.type_rows.is_some_and(|r| r > 1));
    let (l, d) = (cfg.lmax as usize, hl.hidden);
    if seq.ids.len() != l || seq.count == 0 || seq.count > l {
        return Err(LowerError::eval(format!("a padded sequence of {} ids with {} real ones for L = {l}", seq.ids.len(), seq.count)));
    }
    let p = |name: &str, layer: Option<usize>| -> Result<Vec<f64>> {
        let i = hl_param(hl, name)?;
        let t = params.get(i, layer)?;
        Ok(t.data.iter().map(|x| *x as f64).collect())
    };
    let n_real = seq.count;
    let mut observe = |key: String, rows: &[Vec<f64>]| {
        if let Some(st) = stats.as_deref_mut() {
            let e = st.entry(key).or_default();
            let width = rows.first().map_or(0, |r| r.len());
            if e.count == 0 {
                e.chan_absmax = vec![0.0; width];
            }
            for (i, r) in rows.iter().enumerate() {
                let mut row = 0f64;
                for (c, x) in r.iter().enumerate() {
                    let ax = x.abs();
                    row = row.max(ax);
                    e.sum_sq += ax * ax;
                    if let Some(ch) = e.chan_absmax.get_mut(c) {
                        *ch = ch.max(ax as f32);
                    }
                }
                e.absmax = e.absmax.max(row);
                if i == 0 {
                    e.pos0_absmax = e.pos0_absmax.max(row);
                } else {
                    e.rest_absmax = e.rest_absmax.max(row);
                }
                e.count += r.len() as u64;
            }
        }
    };
    let norm = |x: &[f64], c: &NormCfg, name: &str, layer: Option<usize>| -> Result<Vec<f64>> {
        let g = p(&format!("{name}.gain"), layer)?;
        let bias = if c.bias { Some(p(&format!("{name}.bias"), layer)?) } else { None };
        Ok(norm_row(c.kind, c.eps, x, &g, bias.as_deref()))
    };
    let lin = |x: &[f64], w: &[f64], bias: Option<&[f64]>, out: usize| -> Vec<f64> {
        let inp = x.len();
        (0..out).map(|o| bias.map_or(0.0, |b| b[o]) + (0..inp).map(|i| w[o * inp + i] * x[i]).sum::<f64>()).collect()
    };
    // Embeddings.
    let word = p("embed.table", None)?;
    let pos = if a.has_positions { Some(p("embed.pos_table", None)?) } else { None };
    let typ = p("embed.type_table", None).ok();
    let ed = hl.params[hl_param(hl, "embed.table")? as usize].shape[1];
    // Segment ids of a pair input: type 1 from the first separator's NEXT position on, else type 0 (`pair_segment_types`).
    let seg = pair_segment_types(&seq.ids, a.pair_sep);
    let mut x: Vec<Vec<f64>> = (0..l)
        .map(|i| {
            (0..ed)
                .map(|j| {
                    word[seq.ids[i] * ed + j]
                        + pos.as_ref().map_or(0.0, |p| p[(a.pos_offset + i) * ed + j])
                        + typ.as_ref().map_or(0.0, |t| t[seg[i] * ed + j])
                })
                .collect()
        })
        .collect();
    observe("pre.embed.sum".into(), &x[..n_real]);
    if let Some(n) = &a.embed_norm {
        x = x.iter().map(|r| norm(r, n, "embed.norm", None)).collect::<Result<_>>()?;
    }
    observe("pre.embed.norm".into(), &x[..n_real]);
    if let Some(pb) = a.proj_in {
        let w = p("embed.proj_in.w", None)?;
        let bv = if pb { Some(p("embed.proj_in.b", None)?) } else { None };
        x = x.iter().map(|r| lin(r, &w, bv.as_deref(), d)).collect();
        observe("pre.embed.proj_in".into(), &x[..n_real]);
    }
    for li in 0..hl.schedule.len() {
        let la = &a.layers[li];
        let (h, dh) = (la.heads as usize, la.head_dim as usize);
        let pre = format!("L{li}.");
        let ly = Some(li);
        observe(format!("{pre}carry0"), &x[..n_real]);
        let proj = |name: &str, x: &[Vec<f64>], out: usize, bias: bool| -> Result<Vec<Vec<f64>>> {
            let w = p(&format!("{name}.w"), ly)?;
            let bv = if bias { Some(p(&format!("{name}.b"), ly)?) } else { None };
            Ok(x.iter().map(|r| lin(r, &w, bv.as_deref(), out)).collect())
        };
        let xin: Vec<Vec<f64>> = match la.place {
            Place::Pre { mix: Some(n), .. } => {
                let v = x.iter().map(|r| norm(r, &n, "norm.mix", ly)).collect::<Result<Vec<_>>>()?;
                observe(format!("{pre}norm.mix"), &v[..n_real]);
                v
            }
            _ => x.clone(),
        };
        let (mut q, mut k, v) = (proj("attn.q", &xin, h * dh, la.bias[0])?, proj("attn.k", &xin, h * dh, la.bias[1])?, proj("attn.v", &xin, h * dh, la.bias[2])?);
        // The site's scale covers the values before and after the rotation.
        observe(format!("{pre}attn.q"), &q[..n_real]);
        observe(format!("{pre}attn.k"), &k[..n_real]);
        if let Some(rs) = &la.rope {
            let half = dh / 2;
            for t in [&mut q, &mut k] {
                for (pos, row) in t.iter_mut().enumerate() {
                    let (c, s) = rs.freqs.cos_sin(pos);
                    for hh in 0..h {
                        for j in 0..half {
                            let (x1, x2) = (row[hh * dh + j], row[hh * dh + half + j]);
                            row[hh * dh + j] = x1 * c[j] as f64 - x2 * s[j] as f64;
                            row[hh * dh + half + j] = x2 * c[j] as f64 + x1 * s[j] as f64;
                        }
                    }
                }
            }
            observe(format!("{pre}attn.q"), &q[..n_real]);
            observe(format!("{pre}attn.k"), &k[..n_real]);
        }
        observe(format!("{pre}attn.v"), &v[..n_real]);
        // DeBERTa: pos_key = K(table), pos_query = Q(table) with this layer's weights.
        let dis_rows: Option<(Vec<Vec<f64>>, Vec<Vec<f64>>)> = match a.dis {
            Some(dis) => {
                let span2 = 2 * dis.span;
                let t = p("rel.table", None)?;
                let g = dis.norm.map(|_| p("rel.norm.gain", None)).transpose()?;
                let bi = if dis.norm.is_some_and(|n| n.bias) { Some(p("rel.norm.bias", None)?) } else { None };
                let rows: Vec<Vec<f64>> = (0..span2)
                    .map(|r| {
                        let row = t[r * d..(r + 1) * d].to_vec();
                        match (&g, dis.norm) {
                            (Some(g), Some(n)) => norm_row(n.kind, n.eps, &row, g, bi.as_deref()),
                            _ => row,
                        }
                    })
                    .collect();
                observe(format!("{pre}rel.rows"), &rows);
                let pk = if dis.c2p { proj("attn.k", &rows, h * dh, la.bias[1])? } else { vec![] };
                let pq = if dis.p2c { proj("attn.q", &rows, h * dh, la.bias[0])? } else { vec![] };
                if dis.c2p {
                    observe(format!("{pre}attn.pos_k"), &pk);
                }
                if dis.p2c {
                    observe(format!("{pre}attn.pos_q"), &pq);
                }
                Some((pk, pq))
            }
            None => None,
        };
        let rel = match a.rel {
            Some(rb) => Some((p("attn.rel_bias", None)?, rb)),
            None => None,
        };
        let mut ctx = vec![vec![0f64; h * dh]; l];
        for hh in 0..h {
            for i in 0..l {
                let keys: Vec<usize> = (0..n_real).filter(|j| la.band.is_none_or(|w| i.abs_diff(*j) < w)).collect();
                if keys.is_empty() {
                    continue; // a pad row far from every real key: never read
                }
                let sc: Vec<f64> = keys
                    .iter()
                    .map(|&j| {
                        let bias = rel.as_ref().map_or(0.0, |(t, rb)| {
                            t[super::encdec::t5_bucket(j as i64 - i as i64, true, rb.buckets, rb.max_distance) * h + hh]
                        });
                        let mut dot: f64 = (0..dh).map(|t| q[i][hh * dh + t] * k[j][hh * dh + t]).sum();
                        if let (Some(dis), Some((pk, pq))) = (a.dis, &dis_rows) {
                            let ix = deberta_index(i, j, dis.span, dis.max_position);
                            if dis.c2p {
                                dot += (0..dh).map(|t| q[i][hh * dh + t] * pk[ix][hh * dh + t]).sum::<f64>();
                            }
                            if dis.p2c {
                                dot += (0..dh).map(|t| k[j][hh * dh + t] * pq[ix][hh * dh + t]).sum::<f64>();
                            }
                        }
                        dot * la.scale + bias
                    })
                    .collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let e: Vec<f64> = sc.iter().map(|s| (s - mx).exp()).collect();
                let z: f64 = e.iter().sum();
                for t in 0..dh {
                    ctx[i][hh * dh + t] = keys.iter().enumerate().map(|(n, &j)| e[n] / z * v[j][hh * dh + t]).sum();
                }
            }
        }
        observe(format!("{pre}attn.ctx"), &ctx[..n_real]);
        let o = proj("attn.o", &ctx, d, la.bias[3])?;
        observe(format!("{pre}attn.o"), &o[..n_real]);
        let r1: Vec<Vec<f64>> = x.iter().zip(&o).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.mix"), &r1[..n_real]);
        let (x1, ffn_in): (Vec<Vec<f64>>, Vec<Vec<f64>>) = match la.place {
            Place::Post { mix, .. } => {
                let x1 = r1.iter().map(|r| norm(r, &mix, "norm.mix", ly)).collect::<Result<Vec<_>>>()?;
                observe(format!("{pre}norm.mix"), &x1[..n_real]);
                (x1.clone(), x1)
            }
            Place::Pre { ffn: Some(n), .. } => {
                let f = r1.iter().map(|r| norm(r, &n, "norm.ffn", ly)).collect::<Result<Vec<_>>>()?;
                observe(format!("{pre}norm.ffn"), &f[..n_real]);
                (r1.clone(), f)
            }
            Place::Pre { ffn: None, .. } => (r1.clone(), r1.clone()),
        };
        let inter = hl.params[hl_param(hl, "mlp.down.w")? as usize].shape[1];
        let hidden: Vec<Vec<f64>> = if la.gated {
            let gate = proj("mlp.gate", &ffn_in, inter, la.up_bias)?;
            observe(format!("{pre}mlp.gate"), &gate[..n_real]);
            let ga: Vec<Vec<f64>> = gate.iter().map(|r| r.iter().map(|v| crate::float_ref::act(la.act, *v as f32) as f64).collect()).collect();
            observe(format!("{pre}mlp.gate_act"), &ga[..n_real]);
            let up = proj("mlp.up", &ffn_in, inter, la.up_bias)?;
            observe(format!("{pre}mlp.up"), &up[..n_real]);
            let prod: Vec<Vec<f64>> = ga.iter().zip(&up).map(|(g, u)| g.iter().zip(u).map(|(g, u)| g * u).collect()).collect();
            observe(format!("{pre}mlp.prod"), &prod[..n_real]);
            prod
        } else {
            let up = proj("mlp.up", &ffn_in, inter, la.up_bias)?;
            observe(format!("{pre}mlp.up"), &up[..n_real]);
            let act: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|v| crate::float_ref::act(la.act, *v as f32) as f64).collect()).collect();
            observe(format!("{pre}mlp.act"), &act[..n_real]);
            act
        };
        let down = proj("mlp.down", &hidden, d, la.down_bias)?;
        observe(format!("{pre}mlp.down"), &down[..n_real]);
        let r2: Vec<Vec<f64>> = x1.iter().zip(&down).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.ffn"), &r2[..n_real]);
        x = match la.place {
            Place::Post { ffn, .. } => {
                let v = r2.iter().map(|r| norm(r, &ffn, "norm.ffn", ly)).collect::<Result<Vec<_>>>()?;
                observe(format!("{pre}norm.ffn"), &v[..n_real]);
                v
            }
            Place::Pre { .. } => r2,
        };
    }
    observe("post.carry0".into(), &x[..n_real]);
    if let Some(n) = &a.final_norm {
        x = x.iter().map(|r| norm(r, n, "final_norm", None)).collect::<Result<_>>()?;
        observe("post.final_norm".into(), &x[..n_real]);
    }
    // A masked-LM head: the real rows' vocabulary logits, row-major `[count, vocab]`.
    if let Some(m) = a.mlm {
        let wd = p("head.transform.dense.w", None)?;
        let bd = if m.dense_bias { Some(p("head.transform.dense.b", None)?) } else { None };
        let mut rows: Vec<Vec<f64>> = x.iter().map(|r| lin(r, &wd, bd.as_deref(), d)).collect();
        observe("post.mlm.dense".into(), &rows[..n_real]);
        rows = rows.iter().map(|r| r.iter().map(|v| crate::float_ref::act(m.act, *v as f32) as f64).collect()).collect();
        observe("post.mlm.act".into(), &rows[..n_real]);
        rows = rows.iter().map(|r| norm(r, &m.norm, "head.transform.norm", None)).collect::<Result<_>>()?;
        observe("post.mlm.norm".into(), &rows[..n_real]);
        let wh = p("head.w", None)?;
        let vocab = hl.params[hl_param(hl, "head.w")? as usize].shape[0];
        let bh = if m.head_bias { Some(p("head.b", None)?) } else { None };
        let out: Vec<Vec<f64>> = rows.iter().map(|r| lin(r, &wh, bh.as_deref(), vocab)).collect();
        observe("post.mlm.out".into(), &out[..n_real]);
        return Ok(out[..n_real].concat());
    }
    // Per-token logits: the real rows' logits, row-major `[count, labels]`.
    if let Some(tbias) = a.token_logits {
        let w = p("classifier.out.w", None)?;
        let labels = hl.params[hl_param(hl, "classifier.out.w")? as usize].shape[0];
        let bv = if tbias { Some(p("classifier.out.b", None)?) } else { None };
        let rows: Vec<Vec<f64>> = x.iter().map(|r| lin(r, &w, bv.as_deref(), labels)).collect();
        observe("post.tok.out".into(), &rows[..n_real]);
        return Ok(rows[..n_real].concat());
    }
    let pooled: Vec<f64> = match cfg.pooling {
        Pooling::Cls => x[0].clone(),
        Pooling::Mean => (0..d).map(|j| (0..n_real).map(|i| x[i][j]).sum::<f64>() / n_real as f64).collect(),
    };
    observe("post.pool".into(), std::slice::from_ref(&pooled));
    if let Some((cbias, pre)) = a.classify {
        if cfg.pooling != Pooling::Cls || cfg.normalize {
            return Err(LowerError::not_lowerable("a classification head reads the [CLS] row, un-normalised"));
        }
        let mut h = pooled.clone();
        if let Some(pr) = pre {
            let w = p("classifier.pre.w", None)?;
            let bv = if pr.bias { Some(p("classifier.pre.b", None)?) } else { None };
            h = lin(&h, &w, bv.as_deref(), d);
            observe("post.cls.pre".into(), std::slice::from_ref(&h));
            h = h.iter().map(|v| crate::float_ref::act(pr.act, *v as f32) as f64).collect();
            observe("post.cls.act".into(), std::slice::from_ref(&h));
        }
        let w = p("classifier.out.w", None)?;
        let labels = hl.params[hl_param(hl, "classifier.out.w")? as usize].shape[0];
        let bv = if cbias { Some(p("classifier.out.b", None)?) } else { None };
        let out = lin(&h, &w, bv.as_deref(), labels);
        observe("post.cls.out".into(), std::slice::from_ref(&out));
        return Ok(out);
    }
    Ok(if cfg.normalize {
        let nrm = pooled.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
        let out: Vec<f64> = pooled.iter().map(|v| v / nrm).collect();
        observe("post.embed.normed".into(), std::slice::from_ref(&out));
        out
    } else {
        pooled
    })
}
