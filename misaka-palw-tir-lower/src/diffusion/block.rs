//! **One joint transformer block** (`JointTransformerBlock`, RFC-0003 §6) as TWO program blocks of the layer schedule —
//! its attention half and its MLP half — assembled from the lowerers of this module tree.
//!
//! The carry between every layer block is `(h_img, h_txt, cond_a)`: the two token streams as `i32` at their stream
//! scales and the SiLU of the conditioning vector as `i16` codes (computed once by `pre`; every modulation reads it).
//!
//! * **attention half**: the stream's attention modulation (`Linear(cond_a)` to shift/scale/gate at `2^-e`; the last
//!   block's text stream's continuous norm has `scale, shift` only), the stream narrowed to codes and
//!   `LayerNorm · (1 + scale) + shift` ([`super::ada`]), [`super::attn`]'s joint attention over both streams, the gated
//!   residual into each stream. Commit points: the modulations and the two modulated streams (so a cone reads their
//!   leaves, not the modulation weights and the LayerNorm's rows), the attention's own, the carry-out.
//! * **MLP half**: the MLP modulation (shift, scale, gate), the stream narrowed again and modulated, `Linear → GELU →
//!   Linear` (the GELU composed, no table), the gated residual. Commit points: the modulations, the modulated streams,
//!   the carry-out.
//!
//! Splitting is what keeps a program block inside the 512 nodes of normal form and a cone inside one carrier: the
//! modulation weights of a block are 24 KB a stream, a LayerNorm is ~40 nodes, an activation ~40. The last block is
//! `context_pre_only`: its text stream feeds the attention and nothing after it (no output projection, no MLP), and its
//! text carry passes through unchanged. A block is built from the float model's tensors by their diffusers names and from
//! the calibration's site scales, so the same names bind a real checkpoint.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Ref};

use super::act::{Act, lower_act_codes};
use super::ada::{AdaNormConsts, lower_ada_layer_norm, lower_gated_residual};
use super::attn::{JointAttnRefs, JointAttnSpec, attn_narrowings, lower_joint_attention};
use super::calib::Calib;
use super::float::Dit;
use super::linear::{QLinear, QLinearRefs, lower_linear, lower_linear_codes};
use super::sink::ParamSink;
use super::stream::lower_to_codes;
use crate::quant::mul_shift;

/// diffusers' LayerNorm `ε` in the transformer's norms.
pub const NORM_EPS: f64 = 1e-6;

/// A quantised linear map from the checkpoint's `{name}.weight` (`[out, in]`) and optional `{name}.bias`.
pub fn qlin(dit: &Dit, name: &str, x_scale: f64, y_scale: f64) -> QLinear {
    let shape = &dit.t(&format!("{name}.weight")).0;
    qlin_rows(dit, name, 0, shape[0], x_scale, y_scale)
}

/// [`qlin`] of the output channels `[start, start + rows)` only (a modulation's attention chunks, or its MLP chunks).
pub fn qlin_rows(dit: &Dit, name: &str, start: usize, rows: usize, x_scale: f64, y_scale: f64) -> QLinear {
    let inn = dit.t(&format!("{name}.weight")).0[1];
    let w = dit.f32s(&format!("{name}.weight"));
    let bias = dit.has(&format!("{name}.bias")).then(|| dit.f32s(&format!("{name}.bias")));
    QLinear::new(&w[start * inn..(start + rows) * inn], rows, inn, bias.as_ref().map(|b| &b[start..start + rows]), x_scale, y_scale)
}

/// A stream's MLP half.
#[derive(Clone, Debug)]
pub struct QMlpHalf {
    /// `cond_a → (shift_mlp, scale_mlp, gate_mlp)`, codes at `2^-e`.
    pub modulation: QLinear,
    pub e: u32,
    pub s_ln2: f64,
    pub s_x2: f64,
    pub l1: QLinear,
    pub l2: QLinear,
    pub s_ff1: f64,
    pub s_ffa: f64,
    pub s_ff2: f64,
}

/// One stream's half-blocks and scales.
#[derive(Clone, Debug)]
pub struct QStream {
    /// `cond_a → (shift_msa, scale_msa, gate_msa)` (`(scale, shift)` for the last block's text stream), codes at `2^-e`.
    pub mod_a: QLinear,
    pub e_a: u32,
    pub s_ln1: f64,
    pub s_x1: f64,
    pub s_attn_out: f64,
    /// `None` for the last block's text stream.
    pub mlp: Option<QMlpHalf>,
    pub stream_scale: f64,
}

/// A quantised joint block.
#[derive(Clone, Debug)]
pub struct QBlock {
    pub index: usize,
    pub pre_only: bool,
    pub spec: JointAttnSpec,
    pub img: QStream,
    pub txt: QStream,
    pub q: QLinear,
    pub k: QLinear,
    pub v: QLinear,
    pub add_q: QLinear,
    pub add_k: QLinear,
    pub add_v: QLinear,
    pub o: QLinear,
    pub add_o: Option<QLinear>,
    pub score: (i64, i8),
    pub ctx: (i64, i8),
}

impl QBlock {
    /// Block `i` of `dit`, with the calibration's scales (`b{i}.*` sites; `stream_img`, `stream_txt`, `cond_a`).
    pub fn new(i: usize, dit: &Dit, cal: &Calib, n_txt: usize) -> Self {
        let cfg = &dit.cfg;
        let pre_only = i + 1 == cfg.num_layers;
        let (d, n) = (cfg.width(), cfg.grid() * cfg.grid());
        let b = format!("b{i}");
        let pf = format!("transformer_blocks.{i}");
        let (s_ca, s_si, s_st) = (cal.scale16("cond_a"), cal.scale32("stream_img"), cal.scale32("stream_txt"));
        let (s_q, s_k, s_v, s_ctx) = (
            cal.scale16(&format!("{b}.q")),
            cal.scale16(&format!("{b}.k")),
            cal.scale16(&format!("{b}.v")),
            cal.scale16(&format!("{b}.ctx")),
        );
        let spec = JointAttnSpec { heads: cfg.heads, head_dim: cfg.head_dim, n_img: n, n_txt };
        let (s_ao, s_aot) = (cal.scale16(&format!("{b}.ao")), if pre_only { 0.0 } else { cal.scale16(&format!("{b}.aot")) });
        let (score, ctx) = attn_narrowings(&spec, s_q, s_k, s_v, s_ctx);
        let pow2 = |e: u32| 1.0 / (1u64 << e) as f64;

        let stream = |side: &str, norm: &str, stream_scale: f64, s_attn_out: f64, with_mlp: bool, ff_key: &str, ff_pf: &str| {
            let name = format!("{pf}.{norm}.linear");
            let e_a = cal.pow2_exp(&format!("{b}.mod_{side}.a"));
            // The attention chunks are the first three (`shift, scale, gate`); the last block's text norm has two.
            let rows_a = if with_mlp { 3 * d } else { 2 * d };
            let mod_a = qlin_rows(dit, &name, 0, rows_a, s_ca, pow2(e_a));
            let (s_ln1, s_x1) = (cal.scale16(&format!("{b}.ln1_in_{side}")), cal.scale16(&format!("{b}.x1_{side}")));
            let mlp = with_mlp.then(|| {
                let e = cal.pow2_exp(&format!("{b}.mod_{side}.m"));
                let (s_ln2, s_x2) = (cal.scale16(&format!("{b}.ln2_in_{side}")), cal.scale16(&format!("{b}.x2_{side}")));
                let (s_ff1, s_ffa, s_ff2) = (
                    cal.scale16(&format!("{b}.{ff_key}.ff1")),
                    cal.scale16(&format!("{b}.{ff_key}.ffa")),
                    cal.scale16(&format!("{b}.{ff_key}.ff2")),
                );
                QMlpHalf {
                    modulation: qlin_rows(dit, &name, 3 * d, 3 * d, s_ca, pow2(e)),
                    e,
                    s_ln2,
                    s_x2,
                    l1: qlin(dit, &format!("{pf}.{ff_pf}.net.0.proj"), s_x2, s_ff1),
                    l2: qlin(dit, &format!("{pf}.{ff_pf}.net.2"), s_ffa, s_ff2),
                    s_ff1,
                    s_ffa,
                    s_ff2,
                }
            });
            QStream { mod_a, e_a, s_ln1, s_x1, s_attn_out, mlp, stream_scale }
        };
        let img = stream("img", "norm1", s_si, s_ao, true, "img", "ff");
        let txt = stream("txt", "norm1_context", s_st, s_aot, !pre_only, "txt", "ff_context");
        let at = |name: &str, x: f64, y: f64| qlin(dit, &format!("{pf}.attn.{name}"), x, y);
        Self {
            index: i,
            pre_only,
            spec,
            q: at("to_q", img.s_x1, s_q),
            k: at("to_k", img.s_x1, s_k),
            v: at("to_v", img.s_x1, s_v),
            add_q: at("add_q_proj", txt.s_x1, s_q),
            add_k: at("add_k_proj", txt.s_x1, s_k),
            add_v: at("add_v_proj", txt.s_x1, s_v),
            o: at("to_out.0", s_ctx, s_ao),
            add_o: (!pre_only).then(|| at("to_add_out", s_ctx, s_aot)),
            img,
            txt,
            score,
            ctx,
        }
    }

    pub fn width(&self) -> usize {
        self.spec.width()
    }

    /// Declare every param of the block (named by the checkpoint's own module names; a modulation's two halves carry
    /// the suffixes `.attn` and `.mlp`) on `pb`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink) -> QBlockRefs {
        let pf = format!("transformer_blocks.{}", self.index);
        let stream = |pb: &mut ProgramBuilder, sink: &mut ParamSink, s: &QStream, norm: &str, ff_pf: &str| QStreamRefs {
            mod_a: s.mod_a.declare(pb, sink, &format!("{pf}.{norm}.linear.attn")),
            mlp: s.mlp.as_ref().map(|m| QMlpRefs {
                modulation: m.modulation.declare(pb, sink, &format!("{pf}.{norm}.linear.mlp")),
                l1: m.l1.declare(pb, sink, &format!("{pf}.{ff_pf}.net.0.proj")),
                l2: m.l2.declare(pb, sink, &format!("{pf}.{ff_pf}.net.2")),
            }),
        };
        let img = stream(pb, sink, &self.img, "norm1", "ff");
        let txt = stream(pb, sink, &self.txt, "norm1_context", "ff_context");
        let mut at = |name: &str, l: &QLinear| l.declare(pb, sink, &format!("{pf}.attn.{name}"));
        let attn = JointAttnRefs {
            q: at("to_q", &self.q),
            k: at("to_k", &self.k),
            v: at("to_v", &self.v),
            add_q: at("add_q_proj", &self.add_q),
            add_k: at("add_k_proj", &self.add_k),
            add_v: at("add_v_proj", &self.add_v),
            o: at("to_out.0", &self.o),
            add_o: self.add_o.as_ref().map(|l| at("to_add_out", l)),
        };
        QBlockRefs { img, txt, attn }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QMlpRefs {
    pub modulation: QLinearRefs,
    pub l1: QLinearRefs,
    pub l2: QLinearRefs,
}

#[derive(Clone, Copy, Debug)]
pub struct QStreamRefs {
    pub mod_a: QLinearRefs,
    pub mlp: Option<QMlpRefs>,
}

#[derive(Clone, Copy, Debug)]
pub struct QBlockRefs {
    pub img: QStreamRefs,
    pub txt: QStreamRefs,
    pub attn: JointAttnRefs,
}

/// `Slice` of a `[1, k·d]` modulation into its `j`-th `[1, d]` chunk.
fn chunk(b: &mut BlockBuilder<'_>, m: Ref, j: usize, d: usize) -> Ref {
    b.slice(m, 1, (j * d) as u32, d as u32)
}

/// The `ratio` of a gated residual: the gate's `2^-e`, the branch's scale, the stream's.
fn gate_ratio(e: u32, s_branch: f64, s_stream: f64) -> (i64, i8) {
    mul_shift(s_branch / (1u64 << e) as f64 / s_stream)
}

/// The stream narrowed to codes, normalised and modulated, committed.
fn modulated(b: &mut BlockBuilder<'_>, h: Ref, d: usize, s: &QStream, s_ln: f64, s_out: f64, e: u32, scale: Ref, shift: Ref) -> Ref {
    let x = lower_to_codes(b, h, mul_shift(s.stream_scale / s_ln));
    let k = AdaNormConsts::new(d, s_ln, NORM_EPS, e, s_out);
    let y = lower_ada_layer_norm(b, x, scale, shift, &k);
    b.commit(y)
}

/// **The attention half** over the carry `(h_img, h_txt, cond_a)`; returns the carry-out nodes (the caller `finish`es
/// the block with them).
pub fn lower_attn_half(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs) -> [Ref; 3] {
    lower_attn_half_over(b, blk, r, [Ref::CarryIn(0), Ref::CarryIn(1), Ref::CarryIn(2)])
}

/// [`lower_attn_half`] over any three operands standing in for the carry (the tests read them from params).
pub fn lower_attn_half_over(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs, carry: [Ref; 3]) -> [Ref; 3] {
    let d = blk.width();
    let [h_img, h_txt, cond_a] = carry;
    let rng = (-32_767, 32_767, DType::I16);

    let m_img = lower_linear_codes(b, cond_a, &r.img.mod_a); // [1, 3d]: shift, scale, gate
    b.commit(m_img);
    let m_txt = lower_linear_codes(b, cond_a, &r.txt.mod_a); // [1, 3d], or [1, 2d]: scale, shift
    b.commit(m_txt);
    let (shift_a, scale_a, gate_a) = (chunk(b, m_img, 0, d), chunk(b, m_img, 1, d), chunk(b, m_img, 2, d));
    let x1_img = modulated(b, h_img, d, &blk.img, blk.img.s_ln1, blk.img.s_x1, blk.img.e_a, scale_a, shift_a);
    let (t_shift, t_scale) = if blk.pre_only {
        // AdaLayerNormContinuous: scale first, then shift; no gate.
        (chunk(b, m_txt, 1, d), chunk(b, m_txt, 0, d))
    } else {
        (chunk(b, m_txt, 0, d), chunk(b, m_txt, 1, d))
    };
    let x1_txt = modulated(b, h_txt, d, &blk.txt, blk.txt.s_ln1, blk.txt.s_x1, blk.txt.e_a, t_scale, t_shift);

    let (out_img, out_txt) = lower_joint_attention(b, x1_img, x1_txt, &r.attn, &blk.spec, blk.score, blk.ctx, rng);
    let h_img1 = lower_gated_residual(b, h_img, out_img, gate_a, gate_ratio(blk.img.e_a, blk.img.s_attn_out, blk.img.stream_scale));
    let h_txt1 = match out_txt {
        Some(out_txt) => {
            let gate = chunk(b, m_txt, 2, d);
            lower_gated_residual(b, h_txt, out_txt, gate, gate_ratio(blk.txt.e_a, blk.txt.s_attn_out, blk.txt.stream_scale))
        }
        // The last block: the text stream is read by the attention and goes no further; carry it through.
        None => b.reshape_fixed(h_txt, &[blk.spec.n_txt as u32, d as u32]),
    };
    let cond_out = b.reshape_fixed(cond_a, &[1, d as u32]);
    [h_img1, h_txt1, cond_out]
}

/// `Linear → GELU → Linear` of one stream's MLP half over its modulated codes.
///
/// The hidden tensor and its activation are commit points: an output tile of the second linear needs ALL of its `4d` inputs, and
/// without them its cone would open every row of the first linear (256 rows ≈ 300 KB on the fixture).
fn feed_forward(b: &mut BlockBuilder<'_>, x: Ref, m: &QMlpHalf, r: &QMlpRefs) -> Ref {
    let h = lower_linear_codes(b, x, &r.l1);
    b.commit(h);
    let a = lower_act_codes(b, h, Act::GeluTanh, m.s_ff1, m.s_ffa);
    b.commit(a);
    lower_linear_codes(b, a, &r.l2)
}

/// **The MLP half** over the carry `(h_img, h_txt, cond_a)`; returns the carry-out nodes.
pub fn lower_mlp_half(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs) -> [Ref; 3] {
    lower_mlp_half_over(b, blk, r, [Ref::CarryIn(0), Ref::CarryIn(1), Ref::CarryIn(2)])
}

/// [`lower_mlp_half`] over any three operands standing in for the carry.
pub fn lower_mlp_half_over(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs, carry: [Ref; 3]) -> [Ref; 3] {
    let d = blk.width();
    let [h_img, h_txt, cond_a] = carry;
    let one_stream = |b: &mut BlockBuilder<'_>, h: Ref, s: &QStream, sr: &QStreamRefs| -> Ref {
        let (m, mr) = (s.mlp.as_ref().expect("a stream with an MLP half"), sr.mlp.as_ref().expect("declared with it"));
        let mm = lower_linear_codes(b, cond_a, &mr.modulation); // [1, 3d]: shift_mlp, scale_mlp, gate_mlp
        b.commit(mm);
        let (shift, scale, gate) = (chunk(b, mm, 0, d), chunk(b, mm, 1, d), chunk(b, mm, 2, d));
        let x2 = modulated(b, h, d, s, m.s_ln2, m.s_x2, m.e, scale, shift);
        let f = feed_forward(b, x2, m, mr);
        lower_gated_residual(b, h, f, gate, gate_ratio(m.e, m.s_ff2, s.stream_scale))
    };
    let h_img2 = one_stream(b, h_img, &blk.img, &r.img);
    let h_txt2 = if blk.txt.mlp.is_some() {
        one_stream(b, h_txt, &blk.txt, &r.txt)
    } else {
        b.reshape_fixed(h_txt, &[blk.spec.n_txt as u32, d as u32])
    };
    let cond_out = b.reshape_fixed(cond_a, &[1, d as u32]);
    [h_img2, h_txt2, cond_out]
}

/// A projection to the stream's `i32` rail (the context embedder, the patch embedding's cousin).
pub fn lower_to_stream_linear(b: &mut BlockBuilder<'_>, x: Ref, l: &QLinearRefs) -> Ref {
    lower_linear(b, x, l, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

#[cfg(test)]
mod tests {
    use super::super::float::testkit::{inputs, random_source, tiny_config};
    use super::super::float::{Dit, DitInputs};
    use super::*;

    /// Calibrate the tiny random model on a few runs; the first run's block tensors are kept for the checks.
    fn calibrated(seed: u64, n_txt: usize) -> (Dit, Calib) {
        let cfg = tiny_config();
        let dit = Dit::load(cfg.clone(), &random_source(&cfg, seed)).unwrap();
        let mut keep: Vec<String> = vec!["cond_a".to_string()];
        for i in 0..cfg.num_layers {
            for s in ["h_in_img", "h_in_txt", "h_mid_img", "h_mid_txt", "h_out_img", "h_out_txt"] {
                keep.push(format!("b{i}.{s}"));
            }
        }
        let mut cal = Calib::new();
        for k in 0..3u64 {
            let (latent, t, text, pooled) = inputs(&cfg, 100 + k, n_txt);
            let mut run = Calib::new();
            run.keep = keep.iter().cloned().collect();
            dit.forward(&DitInputs { latent: &latent, timestep: t, text: &text, n_txt, pooled: &pooled }, &mut run);
            cal.merge(&run);
            if k == 0 {
                cal.kept = run.kept;
            }
        }
        cal.unify(&["cond", "te_out", "pe_out"]);
        (dit, cal)
    }

    /// Block `i` as the two layers of a one-position program (`pre` hands it the float run's own input carry, quantised;
    /// `post` commits what comes out): the integer carry after the attention half and after the MLP half, as floats
    /// `((h_img, h_txt), (h_img, h_txt))`.
    #[allow(clippy::type_complexity)]
    fn run_block(dit: &Dit, cal: &Calib, i: usize, n_txt: usize) -> ((Vec<f64>, Vec<f64>), (Vec<f64>, Vec<f64>)) {
        use misaka_palw_tir::interval::analyze_ranges;
        use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
        use misaka_palw_tir::{Interpreter, RunState, TensorType};
        let blk = QBlock::new(i, dit, cal, n_txt);
        let cfg = &dit.cfg;
        let (d, n) = (cfg.width(), cfg.grid() * cfg.grid());
        let (s_si, s_st, s_ca) = (cal.scale32("stream_img"), cal.scale32("stream_txt"), cal.scale16("cond_a"));
        let q = |v: &[f64], s: f64| -> Vec<i128> { v.iter().map(|x| (x / s).round() as i128).collect() };
        let (h_img, h_txt, cond) = (
            q(&cal.kept[&format!("b{i}.h_in_img")], s_si),
            q(&cal.kept[&format!("b{i}.h_in_txt")], s_st),
            q(&cal.kept["cond_a"], s_ca),
        );
        let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
        let mut sink = ParamSink::new();
        let refs = blk.declare(&mut pb, &mut sink);
        let ph = sink.put(&mut pb, "carry.h_img", DType::I32, &[n as u32, d as u32], h_img);
        let pt = sink.put(&mut pb, "carry.h_txt", DType::I32, &[n_txt as u32, d as u32], h_txt);
        let pc = sink.put(&mut pb, "carry.cond", DType::I16, &[1, d as u32], cond);
        let sig = vec![
            TensorType::fixed(DType::I32, &[n as u32, d as u32]),
            TensorType::fixed(DType::I32, &[n_txt as u32, d as u32]),
            TensorType::fixed(DType::I16, &[1, d as u32]),
        ];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let a = b.reshape_fixed(ph, &[n as u32, d as u32]);
            let t = b.reshape_fixed(pt, &[n_txt as u32, d as u32]);
            let c = b.reshape_fixed(pc, &[1, d as u32]);
            b.finish(&[a, t, c])
        };
        let attn = {
            let mut b = pb.block("attn", sig.clone());
            let o = lower_attn_half(&mut b, &blk, &refs);
            b.finish(&o)
        };
        let mlp = {
            let mut b = pb.block("mlp", sig.clone());
            let o = lower_mlp_half(&mut b, &blk, &refs);
            b.finish(&o)
        };
        let post = {
            let mut b = pb.block("post", sig);
            for k in 0..3 {
                let t = b.ty(Ref::CarryIn(k));
                let shape: Vec<u32> = t.shape.iter().map(|x| if let misaka_palw_tir::Dim::Fixed(v) = x { *v } else { 0 }).collect();
                let r = b.reshape_fixed(Ref::CarryIn(k), &shape);
                b.commit(r);
            }
            b.finish(&[])
        };
        let names: Vec<String> = pb.params.iter().map(|p| p.name.clone()).collect();
        let program = pb.finish(pre, vec![attn, mlp], post, 0);
        let interp = Interpreter::new(&program).unwrap_or_else(|e| panic!("the block program validates: {e}"));
        analyze_ranges(&program).unwrap_or_else(|e| panic!("the block is admissible at full param ranges: {e}"));
        let params = sink.bind(names.iter().map(String::as_str));
        let step = interp.step(&params, &mut RunState::default(), 0).unwrap_or_else(|e| panic!("step: {e}"));
        let carry_of = |block: u8| -> (Vec<f64>, Vec<f64>) {
            let outs = &program.blocks[block as usize].carry_out;
            let get = |k: usize, s: f64| -> Vec<f64> {
                let c = step.commits.iter().find(|c| c.block == block && c.node == outs[k]).expect("a carry-out is committed");
                c.value.data.iter().map(|v| *v as f64 * s).collect()
            };
            (get(0, s_si), get(1, s_st))
        };
        (carry_of(attn), carry_of(mlp))
    }

    fn worst(got: &[f64], want: &[f64]) -> (f64, f64) {
        let amax = want.iter().fold(0f64, |m, v| m.max(v.abs()));
        (got.iter().zip(want).fold(0f64, |m, (g, w)| m.max((g - w).abs())), amax)
    }

    #[test]
    fn each_half_of_a_joint_block_is_the_float_half_to_its_lossy_sites() {
        let n_txt = 5;
        let (dit, cal) = calibrated(7, n_txt);
        for i in 0..2 {
            let pre_only = i + 1 == 2;
            let (mid, out) = run_block(&dit, &cal, i, n_txt);
            for (half, (img, txt), site_i, site_t) in
                [("attn", &mid, "h_mid_img", "h_mid_txt"), ("mlp ", &out, "h_out_img", "h_out_txt")]
            {
                let want = &cal.kept[&format!("b{i}.{site_i}")];
                let (w, amax) = worst(img, want);
                eprintln!("block {i} {half}: image stream worst {w:.4} ({:.2}% of amax {amax:.3})", 100.0 * w / amax);
                assert!(
                    w <= 0.05 * amax + 4.0 * cal.scale32("stream_img"),
                    "block {i} {half}: image stream worst error {w} against amax {amax}"
                );
                match (cal.kept.get(&format!("b{i}.{site_t}")), pre_only) {
                    (Some(want), _) => {
                        let (w, amax) = worst(txt, want);
                        eprintln!("block {i} {half}: text stream worst {w:.4} ({:.2}% of amax {amax:.3})", 100.0 * w / amax);
                        assert!(
                            w <= 0.05 * amax + 4.0 * cal.scale32("stream_txt"),
                            "block {i} {half}: text stream worst {w} against {amax}"
                        );
                    }
                    (None, true) => {
                        // The last block carries its text stream through unchanged.
                        let (w, _) = worst(txt, &cal.kept[&format!("b{i}.h_in_txt")]);
                        assert!(w <= 2.0 * cal.scale32("stream_txt"), "block {i} {half}: the text carry passes through");
                    }
                    (None, false) => panic!("a text stream with no float site"),
                }
            }
        }
    }
}
