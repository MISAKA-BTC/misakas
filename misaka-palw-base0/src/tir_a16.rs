//! **The A16 engine as a PALW-TIR program, and the conversion of an A16 artifact into its TIR
//! artifact** (RFC-0002 Phase F: step F3's converter, and the program drill D-F1 runs).
//!
//! [`a16_mirror_program`] writes, for one `Base0ShapeV1`, the `TirProgramV1` that computes what
//! [`crate::engine_a16::A16Engine::forward_token`] computes — the same narrowings in the same
//! order, from `misaka-palw-tir`'s templates (`narrow_a16` is `a16_scale_round` + saturating zero +
//! clamp, `rms_norm_a16` is `a16_rms_norm`, `rope_pairs` is `a16_rope`, `silu` is op 6), so every
//! logit code is the engine's. Two things the engine does by branching are data here:
//!
//! * **the first-position lane** (ADR-0050's `.sink0` overrides at seven seams) is a `Select` on
//!   `pos = 0` between the two parameter sets;
//! * **the softmax's widening byte** varies by layer, so the program shifts by a gathered `2^up`
//!   instead of the template's constant — the same steps (`max`, the difference floored at
//!   `i32::MIN >> up`, `<< up`, `IntExp`, the sum, `IntRecip`, `>> 24`).
//!
//! Beyond the rotary table's last row the engine refuses the position; the program holds the last
//! row (a class's layout bounds its jobs below `max_position`, so no job reaches it).
//!
//! [`a16_tir_tensor_bytes`] converts an artifact's tensors to the program's params: int8 slabs
//! verbatim, rotary tables verbatim, and every 17-byte `(multiplier, shift, zero)` wire triple of
//! the A16 store split into three typed tensors — `m` (`i64`), `s` (`i8`, the shift is ≤ 62) and
//! `z` (`i64`) — named by the store's template with `{layer}` dropped (`blk.attn_q.weight.a16.m`).
//! [`convert_a16_to_tir`] writes them as a `PALWTIR1` container.

use crate::artifact::{Base0ArtifactV1, Base0ShapeV1};
use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::{INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{Cmp, DType, Dim, Ref, Rounding, TensorType, TirProgramV1};
use std::collections::BTreeMap;
use std::path::Path;

/// The per-layer A16 triple tables, `(store template, width)`: `d`, `kv`, `ff` or 1 (a tiled scalar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum W {
    D,
    Kv,
    Ff,
    One,
}

const LAYER_TRIPLES: [(&str, W); 25] = [
    ("blk.{layer}.attn_norm.a16", W::D),
    ("blk.{layer}.attn_q.weight.a16", W::D),
    ("blk.{layer}.attn_k.weight.a16", W::Kv),
    ("blk.{layer}.attn_v.weight.a16", W::Kv),
    ("blk.{layer}.attn_logits.a16", W::One),
    ("blk.{layer}.attn_probs.a16", W::One),
    ("blk.{layer}.attn_values.a16", W::One),
    ("blk.{layer}.attn_output.weight.a16", W::D),
    ("blk.{layer}.attn_output.weight.a16.sink0", W::D),
    ("blk.{layer}.attn_align.a16", W::One),
    ("blk.{layer}.attn_align.a16.sink0", W::One),
    ("blk.{layer}.attn_residual.a16", W::One),
    ("blk.{layer}.ffn_norm.a16", W::D),
    ("blk.{layer}.ffn_gate.weight.a16", W::Ff),
    ("blk.{layer}.ffn_silu.a16", W::One),
    ("blk.{layer}.ffn_silu.a16.sink0", W::One),
    ("blk.{layer}.ffn_up.weight.a16", W::Ff),
    ("blk.{layer}.ffn_up.weight.a16.sink0", W::Ff),
    ("blk.{layer}.ffn_gated.a16", W::One),
    ("blk.{layer}.ffn_gated.a16.sink0", W::One),
    ("blk.{layer}.ffn_down.weight.a16", W::D),
    ("blk.{layer}.ffn_down.weight.a16.sink0", W::D),
    ("blk.{layer}.ffn_align.a16", W::One),
    ("blk.{layer}.ffn_align.a16.sink0", W::One),
    ("blk.{layer}.ffn_residual.a16", W::One),
];

const GLOBAL_TRIPLES: [(&str, W); 3] = [("embed_lift.a16", W::One), ("final_norm.a16", W::D), ("token_embd.weight.a16", W::One)];

/// The per-layer int8 weights: `(TIR name, rows, cols)` as `W::*` of the shape.
const LAYER_WEIGHTS: [(&str, W, W); 7] = [
    ("blk.attn_q.weight", W::D, W::D),
    ("blk.attn_k.weight", W::Kv, W::D),
    ("blk.attn_v.weight", W::Kv, W::D),
    ("blk.attn_output.weight", W::D, W::D),
    ("blk.ffn_gate.weight", W::Ff, W::D),
    ("blk.ffn_up.weight", W::Ff, W::D),
    ("blk.ffn_down.weight", W::D, W::Ff),
];

/// The TIR param name of a store template: `{layer}` and the dot after it dropped.
fn tir_base(template: &str) -> String {
    template.replace("{layer}.", "")
}

fn width(shape: &Base0ShapeV1, w: W) -> usize {
    match w {
        W::D => shape.d_model(),
        W::Kv => shape.kv_dim(),
        W::Ff => shape.d_ff,
        W::One => 1,
    }
}

struct Triple {
    m: Ref,
    s: Ref,
    z: Ref,
}

fn declare_triple(pb: &mut ProgramBuilder, base: &str, n: usize, per_layer: bool) -> Triple {
    let n = n as u32;
    Triple {
        m: pb.param(&format!("{base}.m"), DType::I64, &[n], per_layer),
        s: pb.param(&format!("{base}.s"), DType::I8, &[n], per_layer),
        z: pb.param(&format!("{base}.z"), DType::I64, &[n], per_layer),
    }
}

/// `narrow_a16` with the shift gathered from `2^0 … 2^62` (the engine's `shift.min(62)`).
fn narrow(b: &mut BlockBuilder<'_>, x: Ref, t: (Ref, Ref, Ref), lo: i64, hi: i64, dt: DType) -> Ref {
    let p2 = b.pow2_of(t.1);
    b.narrow_a16(x, t.0, p2, t.2, lo, hi, dt)
}

fn codes(b: &mut BlockBuilder<'_>, x: Ref, t: (Ref, Ref, Ref)) -> Ref {
    narrow(b, x, t, -32767, 32767, DType::I16)
}

/// The generic set, or the first-position set at `pos = 0`.
fn lane(b: &mut BlockBuilder<'_>, pos0: Ref, sink: &Triple, generic: &Triple) -> (Ref, Ref, Ref) {
    (
        b.select(pos0, sink.m, generic.m, DType::I64),
        b.select(pos0, sink.s, generic.s, DType::I8),
        b.select(pos0, sink.z, generic.z, DType::I64),
    )
}

fn t3(t: &Triple) -> (Ref, Ref, Ref) {
    (t.m, t.s, t.z)
}

/// `W·x` for an int8 weight `[rows, cols]` and a code row `[cols]`: the `i64` accumulator `[rows]`.
fn matvec(b: &mut BlockBuilder<'_>, w: Ref, x: Ref, rows: usize, cols: usize) -> Ref {
    let xc = b.reshape_fixed(x, &[cols as u32, 1]);
    let acc = b.matmul(w, xc, DType::I64);
    b.reshape_fixed(acc, &[rows as u32])
}

/// `palw_base0_ops::softmax_shifted` along the last axis with the widening `up` a runtime value
/// (`[1]` `i8`, clamped to `[0, 62]` as the engine does): the template's steps, the constants
/// `2^up` and `i32::MIN >> up` gathered/divided instead of folded.
fn softmax_up(b: &mut BlockBuilder<'_>, x: Ref, up: Ref) -> Ref {
    let axis = b.shape(x).len() - 1;
    let p2 = b.pow2_of(up);
    let mn = b.c(DType::I64, i32::MIN as i128);
    // `i32::MIN >> up` is the floor division by `2^up` (arithmetic shift of a negative value).
    let floor = b.div(mn, p2, Rounding::Floor, DType::I64);
    let max = b.reduce_max(x, axis);
    let diff = b.sub(x, max, DType::I64);
    let below = b.compare(diff, floor, Cmp::Lt);
    let d = b.select(below, floor, diff, DType::I64);
    // `d · 2^up` is within `[i32::MIN, 0]` for every `up` (the floor makes it so); the range
    // analysis cannot see the correlation, so the product is taken in i128 and then clamped.
    let w = b.mul(d, p2, DType::I128);
    let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(arg);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let recip = b.int_recip(sum);
    let p = b.mul(e, recip, DType::I128);
    let q = b.shr(p, 24, Rounding::Floor, DType::I64);
    b.clamp(q, 0, 1 << 25, DType::I32)
}

/// **Where each legacy node row lives in the mirror program**: for the shape profile's `pre_nodes`,
/// `attn_nodes` (graph v5/v7 numbering, twenty-four a layer) and `post_nodes`, the node of the
/// program's `pre`, layer and `post` block that computes that row. Every one of them is a commit
/// point, so the IR class commits every row the legacy class commits (the fused attention site is
/// one row, as in v5/v7; the program also commits the site's logits and probability codes, which
/// the legacy row keeps inside its kernel).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct A16MirrorRowsV1 {
    pub pre: Vec<u16>,
    pub layer: Vec<u16>,
    pub post: Vec<u16>,
}

/// The `pre`, layer and `post` blocks of the mirror program.
pub const A16_MIRROR_PRE_BLOCK: u8 = 0;
pub const A16_MIRROR_LAYER_BLOCK: u8 = 1;
pub const A16_MIRROR_POST_BLOCK: u8 = 2;

fn node_of(r: Ref) -> u16 {
    match r {
        Ref::Node(j) => j,
        other => panic!("a legacy row is a node of the mirror, not {other:?}"),
    }
}

/// **The A16 engine as a TIR program** for one shape. `history_bound` is `2^18` or `2^21`.
pub fn a16_mirror_program(shape: &Base0ShapeV1, history_bound: u32) -> Result<TirProgramV1, String> {
    a16_mirror_program_with_rows(shape, history_bound).map(|(p, _)| p)
}

/// [`a16_mirror_program`] and the node of every legacy row ([`A16MirrorRowsV1`]).
pub fn a16_mirror_program_with_rows(shape: &Base0ShapeV1, history_bound: u32) -> Result<(TirProgramV1, A16MirrorRowsV1), String> {
    let (d, kv, ff, v) = (shape.d_model(), shape.kv_dim(), shape.d_ff, shape.vocab);
    let (h, kvh, dh) = (shape.n_heads, shape.n_kv_heads, shape.d_head);
    if h == 0 || kvh == 0 || !h.is_multiple_of(kvh) || dh % 2 != 0 || shape.n_layers == 0 || shape.max_position == 0 {
        return Err(format!("not an A16 shape the mirror can write: {shape:?}"));
    }
    let g = h / kvh;
    let pairs = (dh / 2) as u32;
    let mut pb = ProgramBuilder::new(v as u32, history_bound);
    // Params, in the order the store and the slabs name them.
    let embed = pb.param("token_embd.weight", DType::I8, &[v as u32, d as u32], false);
    let unembed = pb.param(crate::plan::BASE0_ENGINE_HEAD_TENSOR, DType::I8, &[v as u32, d as u32], false);
    let rope_cos = pb.param("rope.cos_q", DType::I32, &[shape.max_position as u32, pairs], false);
    let rope_sin = pb.param("rope.sin_q", DType::I32, &[shape.max_position as u32, pairs], false);
    let globals: Vec<Triple> = GLOBAL_TRIPLES.iter().map(|(t, w)| declare_triple(&mut pb, t, width(shape, *w), false)).collect();
    let weights: Vec<Ref> = LAYER_WEIGHTS
        .iter()
        .map(|(n, r, c)| pb.param(n, DType::I8, &[width(shape, *r) as u32, width(shape, *c) as u32], true))
        .collect();
    let lt: Vec<Triple> = LAYER_TRIPLES.iter().map(|(t, w)| declare_triple(&mut pb, &tir_base(t), width(shape, *w), true)).collect();
    let up_bits = pb.param("blk.attn_softmax_up", DType::I8, &[1], true);
    let k_hist = pb.hist_state("blk.attn_k.cache", DType::I16, &[kv as u32], history_bound, true);
    let v_hist = pb.hist_state("blk.attn_v.cache", DType::I16, &[kv as u32], history_bound, true);
    let carry = vec![TensorType::fixed(DType::I16, &[d as u32])];
    let ix = |name: &str| LAYER_TRIPLES.iter().position(|(t, _)| tir_base(t) == name).expect("a known table");

    let mut rows = A16MirrorRowsV1 { pre: Vec::new(), layer: Vec::new(), post: Vec::new() };
    // Commit a node that is a legacy row, and record it.
    let row_of = |b: &mut BlockBuilder<'_>, table: &mut Vec<u16>, r: Ref| -> Ref {
        let r = b.commit(r);
        table.push(node_of(r));
        r
    };

    // pre: the int8 row (graph row 0), lifted onto the A16 stream (row 1).
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(embed, Ref::Input(INPUT_TOKEN), 0, 0);
        row_of(&mut b, &mut rows.pre, row);
        let x = codes(&mut b, row, t3(&globals[0]));
        row_of(&mut b, &mut rows.pre, x);
        b.finish(&[x])
    };

    let layer = {
        let mut b = pb.block("a16", carry.clone());
        let x = Ref::CarryIn(0);
        let zero = b.c(DType::Idx, 0);
        let pos0 = b.compare(Ref::Input(INPUT_POS), zero, Cmp::Eq);
        let t = |name: &str| t3(&lt[ix(name)]);
        let r = &mut rows.layer;
        // Attention (graph rows 0–7).
        let unit = b.rms_norm_a16(x, shape.eps_q);
        row_of(&mut b, r, unit);
        let normed = codes(&mut b, unit, t("blk.attn_norm.a16"));
        let normed = row_of(&mut b, r, normed);
        let q = matvec(&mut b, weights[0], normed, d, d);
        let q = codes(&mut b, q, t("blk.attn_q.weight.a16"));
        row_of(&mut b, r, q);
        let k = matvec(&mut b, weights[1], normed, kv, d);
        let k = codes(&mut b, k, t("blk.attn_k.weight.a16"));
        row_of(&mut b, r, k);
        let vv = matvec(&mut b, weights[2], normed, kv, d);
        let vv = codes(&mut b, vv, t("blk.attn_v.weight.a16"));
        row_of(&mut b, r, vv);
        let at = b.clamp(Ref::Input(INPUT_POS), 0, shape.max_position as i64 - 1, DType::Idx);
        let cos = b.gather(rope_cos, at, 0, 0);
        let sin = b.gather(rope_sin, at, 0, 0);
        let qh = b.reshape_fixed(q, &[h as u32, dh as u32]);
        let q_rot = b.rope_pairs(qh, cos, sin, -32767, 32767, DType::I16);
        row_of(&mut b, r, q_rot);
        let kh = b.reshape_fixed(k, &[kvh as u32, dh as u32]);
        let k_rot = b.rope_pairs(kh, cos, sin, -32767, 32767, DType::I16);
        let k_row = b.reshape_fixed(k_rot, &[kv as u32]);
        row_of(&mut b, r, k_row);
        let keys = b.hist_append(k_hist, k_row);
        let vals = b.hist_append(v_hist, vv);
        let qg = b.reshape_fixed(q_rot, &[kvh as u32, g as u32, dh as u32]);
        let kw = b.reshape(keys, &[Dim::H, Dim::Fixed(kvh as u32), Dim::Fixed(dh as u32)]);
        let kt = b.transpose(kw, &[1, 2, 0]);
        let scores = b.matmul(qg, kt, DType::I64);
        let logits = codes(&mut b, scores, t("blk.attn_logits.a16"));
        let logits = b.commit(logits);
        let probs = softmax_up(&mut b, logits, up_bits);
        let p15 = codes(&mut b, probs, t("blk.attn_probs.a16"));
        let p15 = b.commit(p15);
        let vw = b.reshape(vals, &[Dim::H, Dim::Fixed(kvh as u32), Dim::Fixed(dh as u32)]);
        let vt = b.transpose(vw, &[1, 0, 2]);
        let ctx = b.matmul(p15, vt, DType::I64);
        let ctx = codes(&mut b, ctx, t("blk.attn_values.a16"));
        let ctx = b.reshape_fixed(ctx, &[d as u32]);
        // The fused attention site (graph row 7), then the output projection and the residual
        // (rows 8–11).
        let ctx = row_of(&mut b, r, ctx);
        let delta = matvec(&mut b, weights[3], ctx, d, d);
        let wo = lane(&mut b, pos0, &lt[ix("blk.attn_output.weight.a16.sink0")], &lt[ix("blk.attn_output.weight.a16")]);
        let delta = codes(&mut b, delta, wo);
        row_of(&mut b, r, delta);
        let al = lane(&mut b, pos0, &lt[ix("blk.attn_align.a16.sink0")], &lt[ix("blk.attn_align.a16")]);
        let aligned = codes(&mut b, x, al);
        row_of(&mut b, r, aligned);
        let sum = b.add(aligned, delta, DType::I32);
        row_of(&mut b, r, sum);
        let x1 = codes(&mut b, sum, t("blk.attn_residual.a16"));
        let x1 = row_of(&mut b, r, x1);
        // SwiGLU (rows 12–23).
        let unit = b.rms_norm_a16(x1, shape.eps_q);
        row_of(&mut b, r, unit);
        let normed = codes(&mut b, unit, t("blk.ffn_norm.a16"));
        let normed = row_of(&mut b, r, normed);
        let gate = matvec(&mut b, weights[4], normed, ff, d);
        let gate = narrow(&mut b, gate, t("blk.ffn_gate.weight.a16"), i32::MIN as i64, i32::MAX as i64, DType::I32);
        let gate = row_of(&mut b, r, gate);
        let up = matvec(&mut b, weights[5], normed, ff, d);
        let upl = lane(&mut b, pos0, &lt[ix("blk.ffn_up.weight.a16.sink0")], &lt[ix("blk.ffn_up.weight.a16")]);
        let up = codes(&mut b, up, upl);
        let up = row_of(&mut b, r, up);
        let silu = b.silu(gate);
        row_of(&mut b, r, silu);
        let sl = lane(&mut b, pos0, &lt[ix("blk.ffn_silu.a16.sink0")], &lt[ix("blk.ffn_silu.a16")]);
        let s16 = codes(&mut b, silu, sl);
        row_of(&mut b, r, s16);
        let prod = b.mul(s16, up, DType::I32);
        row_of(&mut b, r, prod);
        let gl = lane(&mut b, pos0, &lt[ix("blk.ffn_gated.a16.sink0")], &lt[ix("blk.ffn_gated.a16")]);
        let gated = codes(&mut b, prod, gl);
        let gated = row_of(&mut b, r, gated);
        let down = matvec(&mut b, weights[6], gated, d, ff);
        let dl = lane(&mut b, pos0, &lt[ix("blk.ffn_down.weight.a16.sink0")], &lt[ix("blk.ffn_down.weight.a16")]);
        let down = codes(&mut b, down, dl);
        row_of(&mut b, r, down);
        let fl = lane(&mut b, pos0, &lt[ix("blk.ffn_align.a16.sink0")], &lt[ix("blk.ffn_align.a16")]);
        let aligned = codes(&mut b, x1, fl);
        row_of(&mut b, r, aligned);
        let sum = b.add(aligned, down, DType::I32);
        row_of(&mut b, r, sum);
        let x2 = codes(&mut b, sum, t("blk.ffn_residual.a16"));
        row_of(&mut b, r, x2);
        b.finish(&[x2])
    };

    let post = {
        let mut b = pb.block("post", carry);
        let unit = b.rms_norm_a16(Ref::CarryIn(0), shape.eps_q);
        row_of(&mut b, &mut rows.post, unit);
        let fin = codes(&mut b, unit, t3(&globals[1]));
        let fin = row_of(&mut b, &mut rows.post, fin);
        let acc = matvec(&mut b, unembed, fin, v, d);
        let logits = codes(&mut b, acc, t3(&globals[2]));
        row_of(&mut b, &mut rows.post, logits);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    debug_assert_eq!((pre, layer, post), (A16_MIRROR_PRE_BLOCK, A16_MIRROR_LAYER_BLOCK, A16_MIRROR_POST_BLOCK));
    debug_assert_eq!((rows.pre.len(), rows.layer.len(), rows.post.len()), (2, 24, 3));
    let program = pb.finish(pre, vec![layer; shape.n_layers], post, logits);
    misaka_palw_tir::validate::validate(&program).map_err(|e| format!("the mirror program is not in normal form: {e}"))?;
    Ok((program, rows))
}

/// Why an artifact did not convert.
#[derive(Debug, PartialEq, Eq)]
pub enum A16ToTirError {
    Program(String),
    /// The store lacks a table the engine reads (the same refusal `A16Engine::new` gives).
    Missing(String),
    Malformed(String),
}

impl std::fmt::Display for A16ToTirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Program(m) => write!(f, "mirror program: {m}"),
            Self::Missing(m) => write!(f, "the artifact's A16 store has no `{m}`"),
            Self::Malformed(m) => write!(f, "malformed A16 table: {m}"),
        }
    }
}

impl std::error::Error for A16ToTirError {}

fn parse_triples(name: &str, bytes: &[u8], want: usize) -> Result<Vec<A16QuantParams>, A16ToTirError> {
    if bytes.len() != want * A16QuantParams::WIRE_BYTES {
        return Err(A16ToTirError::Malformed(format!("`{name}`: {} bytes, {want} triples expected", bytes.len())));
    }
    bytes
        .chunks_exact(A16QuantParams::WIRE_BYTES)
        .map(|c| A16QuantParams::from_wire(c).map_err(|e| A16ToTirError::Malformed(format!("`{name}`: {e:?}"))))
        .collect()
}

/// **The artifact's tensors as the mirror program's params**, every instance's little-endian
/// bytes keyed by `(param index, layer)`.
pub fn a16_tir_tensor_bytes(
    artifact: &Base0ArtifactV1,
    program: &TirProgramV1,
) -> Result<BTreeMap<(u16, Option<u16>), Vec<u8>>, A16ToTirError> {
    let shape = &artifact.shape;
    let index = |name: &str| program.param_index(name).ok_or_else(|| A16ToTirError::Program(format!("no param `{name}`")));
    let mut out: BTreeMap<(u16, Option<u16>), Vec<u8>> = BTreeMap::new();
    let i8s = |s: &[i8]| s.iter().map(|x| *x as u8).collect::<Vec<u8>>();
    let i32s = |s: &[i32]| s.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    out.insert((index("token_embd.weight")?, None), i8s(&artifact.embed));
    out.insert((index(crate::plan::BASE0_ENGINE_HEAD_TENSOR)?, None), i8s(&artifact.unembed));
    out.insert((index("rope.cos_q")?, None), i32s(&artifact.rope.cos_q));
    out.insert((index("rope.sin_q")?, None), i32s(&artifact.rope.sin_q));
    let mut triple = |template: &str, layer: Option<u16>, n: usize| -> Result<(), A16ToTirError> {
        let bytes = artifact.a16_param(template, layer).ok_or_else(|| A16ToTirError::Missing(template.to_string()))?;
        let rows = parse_triples(template, bytes, n)?;
        let base = tir_base(template);
        out.insert((index(&format!("{base}.m"))?, layer), rows.iter().flat_map(|r| r.multiplier.to_le_bytes()).collect());
        out.insert((index(&format!("{base}.s"))?, layer), rows.iter().map(|r| r.shift).collect());
        out.insert((index(&format!("{base}.z"))?, layer), rows.iter().flat_map(|r| r.zero.to_le_bytes()).collect());
        Ok(())
    };
    for (t, w) in GLOBAL_TRIPLES {
        triple(t, None, width(shape, w))?;
    }
    for li in 0..shape.n_layers {
        let l = Some(li as u16);
        for (t, w) in LAYER_TRIPLES {
            triple(t, l, width(shape, w))?;
        }
    }
    for (li, lw) in artifact.layers.iter().enumerate() {
        let l = Some(li as u16);
        let slabs = [&lw.wq, &lw.wk, &lw.wv, &lw.wo, &lw.w_gate, &lw.w_up, &lw.w_down];
        for ((name, _, _), slab) in LAYER_WEIGHTS.iter().zip(slabs) {
            out.insert((index(name)?, l), i8s(slab));
        }
        let up =
            artifact.a16_param("blk.{layer}.attn_softmax_up", l).ok_or_else(|| A16ToTirError::Missing("attn_softmax_up".into()))?;
        if up.len() != 1 || up[0] > 62 {
            return Err(A16ToTirError::Malformed("attn_softmax_up".into()));
        }
        out.insert((index("blk.attn_softmax_up")?, l), up.to_vec());
    }
    Ok(out)
}

/// **Convert an A16 artifact into a `PALWTIR1` TIR artifact** at `out`: the mirror program for its
/// shape, its tokenizer commitment, and every tensor converted. Returns the program and the file
/// digest of what was written.
pub fn convert_a16_to_tir(
    artifact: &Base0ArtifactV1,
    history_bound: u32,
    out: &Path,
    meta: String,
) -> Result<(TirProgramV1, [u8; 64]), Box<dyn std::error::Error>> {
    let program = a16_mirror_program(&artifact.shape, history_bound).map_err(A16ToTirError::Program)?;
    let mut tensors = a16_tir_tensor_bytes(artifact, &program)?;
    let mut tokenizer_id = [0u8; 64];
    tokenizer_id.copy_from_slice(artifact.tokenizer_commitment.as_byte_slice());
    let digest = misaka_palw_tir_artifact::write_container_v1(out, &program, Vec::new(), tokenizer_id, meta, &mut |j, l| {
        tensors.remove(&(j, l)).ok_or_else(|| format!("no converted tensor for param {j} layer {l:?}"))
    })?;
    Ok((program, digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;
    use crate::engine_a16::{A16Cache, A16Engine, derived_a16_store};
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Interpreter, MapParams, Tensor};

    /// A small A16 artifact whose store is NOT the derived one's uniform tables: the first-position
    /// lane differs from the generic one, the projections carry zeros (biases), and the layers
    /// widen their softmax by different amounts — so every seam the mirror must get right moves.
    fn artifact(n_layers: usize) -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 48,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let mut store = derived_a16_store(&shape);
        for (name, bytes) in store.iter_mut() {
            if name.ends_with("attn_softmax_up") {
                bytes[0] = if name.contains(".0.") { 24 } else { 20 };
                continue;
            }
            let rows = bytes.len() / A16QuantParams::WIRE_BYTES;
            let mut out = Vec::with_capacity(bytes.len());
            for (i, c) in bytes.chunks_exact(A16QuantParams::WIRE_BYTES).enumerate() {
                let mut p = A16QuantParams::from_wire(c).expect("derived");
                if name.ends_with(".sink0") {
                    p.multiplier *= 3;
                    p.shift += 1;
                }
                if name.contains("attn_q.weight") || name.contains("attn_k.weight") || name.contains("attn_v.weight") {
                    p.zero = (i as i64 % 7) - 3;
                }
                if rows > 1 && name.ends_with("norm.a16") {
                    p.multiplier = 1 + (i as i64 % 3);
                }
                out.extend_from_slice(&p.to_wire());
            }
            *bytes = out;
        }
        Base0ArtifactV1::derive_deterministic(shape, 0x7A16).expect("a valid shape").with_a16_params(store).expect("sorted and unique")
    }

    fn params(program: &TirProgramV1, tensors: &BTreeMap<(u16, Option<u16>), Vec<u8>>) -> MapParams {
        let mut mp = MapParams::default();
        for ((j, l), bytes) in tensors {
            let d = &program.params[*j as usize];
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            mp.tensors.insert((*j, *l), Tensor::from_le_bytes(d.dtype, &shape, bytes).expect("typed"));
        }
        mp
    }

    #[test]
    fn the_mirror_program_is_the_engine_logit_for_logit() {
        let a = artifact(3);
        let program = a16_mirror_program(&a.shape, HISTORY_BOUND_V1_SMALL).expect("program");
        misaka_palw_tir::interval::analyze_ranges(&program).expect("the range analysis admits it");
        let tensors = a16_tir_tensor_bytes(&a, &program).expect("converted");
        let mp = params(&program, &tensors);
        let engine = A16Engine::new(&a).expect("engine");
        let mut cache = A16Cache::new(a.shape.n_layers);
        let interp = Interpreter::new(&program).expect("valid");
        let mut state = misaka_palw_tir::RunState::default();
        let tokens = [5usize, 17, 3, 3, 60, 1, 42, 9, 0, 33, 12, 7];
        let mut distinct = std::collections::BTreeSet::new();
        for (pos, t) in tokens.iter().enumerate() {
            let want = engine.forward_token(&mut cache, *t, pos).expect("engine step");
            let got = interp.step(&mp, &mut state, *t as u32).expect("TIR step");
            let got: Vec<i32> = got.logits.data.iter().map(|v| *v as i32).collect();
            assert_eq!(got, want, "position {pos}: the TIR logits differ from the engine's");
            distinct.insert(want);
        }
        assert!(distinct.len() > 4, "the fixture must compute something position-dependent");
    }

    #[test]
    fn converted_tensors_are_the_legacy_codes_and_triples() {
        let a = artifact(2);
        let program = a16_mirror_program(&a.shape, HISTORY_BOUND_V1_SMALL).expect("program");
        let tensors = a16_tir_tensor_bytes(&a, &program).expect("converted");
        let get = |name: &str, l: Option<u16>| tensors[&(program.param_index(name).expect("declared"), l)].clone();
        assert_eq!(get("token_embd.weight", None), a.embed.iter().map(|x| *x as u8).collect::<Vec<_>>());
        assert_eq!(get("blk.ffn_down.weight", Some(1)), a.layers[1].w_down.iter().map(|x| *x as u8).collect::<Vec<_>>());
        for (t, _) in LAYER_TRIPLES {
            for l in 0..2u16 {
                let wire = a.a16_param(t, Some(l)).expect("store row");
                let base = tir_base(t);
                let (m, s, z) =
                    (get(&format!("{base}.m"), Some(l)), get(&format!("{base}.s"), Some(l)), get(&format!("{base}.z"), Some(l)));
                for (i, c) in wire.chunks_exact(A16QuantParams::WIRE_BYTES).enumerate() {
                    let p = A16QuantParams::from_wire(c).expect("triple");
                    assert_eq!(i64::from_le_bytes(m[8 * i..8 * i + 8].try_into().expect("8")), p.multiplier, "{t} m[{i}]");
                    assert_eq!(s[i], p.shift, "{t} s[{i}]");
                    assert_eq!(i64::from_le_bytes(z[8 * i..8 * i + 8].try_into().expect("8")), p.zero, "{t} z[{i}]");
                }
            }
        }
        // Every declared instance is converted, and nothing else.
        let want: usize = misaka_palw_tir_artifact::param_instances_v1(&program).iter().map(Vec::len).sum();
        assert_eq!(tensors.len(), want);
    }

    #[test]
    fn a_converted_container_opens_and_runs_like_the_engine() {
        let a = artifact(2);
        let path = std::env::temp_dir().join(format!("a16-to-tir-{}.palwtir", std::process::id()));
        let (program, digest) = convert_a16_to_tir(&a, HISTORY_BOUND_V1_SMALL, &path, "{}".into()).expect("converted");
        assert_eq!(misaka_palw_tir_artifact::file_digest_v1(&path).expect("digest"), digest);
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(&path).expect("opens");
        assert_eq!(c.program, program);
        assert_eq!(&c.header.tokenizer_id[..], a.tokenizer_commitment.as_byte_slice());
        let engine = A16Engine::new(&a).expect("engine");
        let mut cache = A16Cache::new(a.shape.n_layers);
        let interp = Interpreter::new(&program).expect("valid");
        let mut state = misaka_palw_tir::RunState::default();
        for (pos, t) in [7usize, 1, 50].iter().enumerate() {
            let want = engine.forward_token(&mut cache, *t, pos).expect("engine step");
            let got = interp.step(&c, &mut state, *t as u32).expect("TIR step from the file");
            assert_eq!(got.logits.data.iter().map(|v| *v as i32).collect::<Vec<_>>(), want);
        }
        let _ = std::fs::remove_file(&path);
    }
}
