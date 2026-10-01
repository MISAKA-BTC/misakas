//! **One joint transformer block** (`JointTransformerBlock`, RFC-0003 §6): the assembly of the lowerers above into the
//! program block that the denoiser's layer schedule runs once per transformer block.
//!
//! The carry is `(h_img, h_txt, cond_a)`: the two token streams as `i32` at their stream scales and the SiLU of the
//! conditioning vector as `i16` codes (computed once by `pre`; every modulation reads it). Per block, per stream:
//!
//! 1. the modulation `Linear(cond_a)` to `6d` codes at `2^-e` (`2d` for the continuous norm of the last block's text
//!    stream), sliced into shift/scale/gate chunks;
//! 2. the stream narrowed to `i16` codes, `LayerNorm · (1 + scale) + shift` ([`super::ada`]);
//! 3. [`super::attn`]'s joint attention over both streams, the gated residual into each stream;
//! 4. the stream narrowed again, `LayerNorm · (1 + scale_mlp) + shift_mlp`, `Linear → GELU → Linear`, the gated
//!    residual.
//!
//! The last block is `context_pre_only`: its text stream feeds the attention and nothing after it (no output
//! projection, no MLP), and its text carry passes through unchanged. A block is built from the float model's
//! tensors by their diffusers names and from the calibration's site scales, so the same names bind a real checkpoint.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::{DType, Ref};

use super::act::QAct;
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
    let (out, inn) = (shape[0], shape[1]);
    let w = dit.f32s(&format!("{name}.weight"));
    let bias = dit.has(&format!("{name}.bias")).then(|| dit.f32s(&format!("{name}.bias")));
    QLinear::new(&w, out, inn, bias.as_deref(), x_scale, y_scale)
}

/// One stream's half of a block: its modulation, its feed-forward and its scales.
#[derive(Clone, Debug)]
pub struct QStream {
    /// `cond_a → 6d` (or `2d`), codes at `2^-e`.
    pub modulation: QLinear,
    pub e: u32,
    /// The LayerNorm input codes' scales (before attention / before the MLP), the modulated outputs', the
    /// branch outputs' (attention out, MLP out).
    pub s_ln1: f64,
    pub s_x1: f64,
    pub s_ln2: f64,
    pub s_x2: f64,
    pub s_attn_out: f64,
    pub s_ff2: f64,
    /// `None` for the last block's text stream.
    pub ff: Option<QFeedForward>,
    pub stream_scale: f64,
}

#[derive(Clone, Debug)]
pub struct QFeedForward {
    pub l1: QLinear,
    pub act: QAct,
    pub l2: QLinear,
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

        let stream = |side: &str, norm: &str, stream_scale: f64, s_attn_out: f64, with_ff: bool, ff_key: &str, ff_pf: &str| {
            let e = cal.pow2_exp(&format!("{b}.mod_{side}"));
            let modulation = qlin(dit, &format!("{pf}.{norm}.linear"), s_ca, 1.0 / (1u64 << e) as f64);
            let (s_ln1, s_x1) = (cal.scale16(&format!("{b}.ln1_in_{side}")), cal.scale16(&format!("{b}.x1_{side}")));
            if !with_ff {
                return QStream { modulation, e, s_ln1, s_x1, s_ln2: 0.0, s_x2: 0.0, s_attn_out, s_ff2: 0.0, ff: None, stream_scale };
            }
            let (s_ln2, s_x2) = (cal.scale16(&format!("{b}.ln2_in_{side}")), cal.scale16(&format!("{b}.x2_{side}")));
            let (s_ff1, s_ffa, s_ff2) = (
                cal.scale16(&format!("{b}.{ff_key}.ff1")),
                cal.scale16(&format!("{b}.{ff_key}.ffa")),
                cal.scale16(&format!("{b}.{ff_key}.ff2")),
            );
            let ff = QFeedForward {
                l1: qlin(dit, &format!("{pf}.{ff_pf}.net.0.proj"), s_x2, s_ff1),
                act: QAct::gelu_tanh(s_ff1, s_ffa),
                l2: qlin(dit, &format!("{pf}.{ff_pf}.net.2"), s_ffa, s_ff2),
            };
            QStream { modulation, e, s_ln1, s_x1, s_ln2, s_x2, s_attn_out, s_ff2, ff: Some(ff), stream_scale }
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

    /// Declare every param of the block (named by the checkpoint's own module names) on `pb`.
    pub fn declare(&self, pb: &mut ProgramBuilder, sink: &mut ParamSink) -> QBlockRefs {
        let pf = format!("transformer_blocks.{}", self.index);
        let stream = |pb: &mut ProgramBuilder, sink: &mut ParamSink, s: &QStream, norm: &str, ff_pf: &str| QStreamRefs {
            modulation: s.modulation.declare(pb, sink, &format!("{pf}.{norm}.linear")),
            ff: s.ff.as_ref().map(|f| QFeedForwardRefs {
                l1: f.l1.declare(pb, sink, &format!("{pf}.{ff_pf}.net.0.proj")),
                act: f.act.declare(pb, sink, &format!("{pf}.{ff_pf}.act")),
                l2: f.l2.declare(pb, sink, &format!("{pf}.{ff_pf}.net.2")),
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
pub struct QFeedForwardRefs {
    pub l1: QLinearRefs,
    pub act: Ref,
    pub l2: QLinearRefs,
}

#[derive(Clone, Copy, Debug)]
pub struct QStreamRefs {
    pub modulation: QLinearRefs,
    pub ff: Option<QFeedForwardRefs>,
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

/// **The block's program**: reads the carry `(h_img, h_txt, cond_a)` and returns the carry-out nodes (the caller
/// `finish`es the block with them).
pub fn lower_block(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs) -> [Ref; 3] {
    lower_block_over(b, blk, r, [Ref::CarryIn(0), Ref::CarryIn(1), Ref::CarryIn(2)])
}

/// [`lower_block`] over any three operands standing in for the carry (the tests read them from params).
pub fn lower_block_over(b: &mut BlockBuilder<'_>, blk: &QBlock, r: &QBlockRefs, carry: [Ref; 3]) -> [Ref; 3] {
    let d = blk.width();
    let [h_img, h_txt, cond_a] = carry;
    let nt = blk.spec.n_txt as u32;
    let rng = (-32_767, 32_767, DType::I16);

    // 1. Modulations.
    let m_img = lower_linear_codes(b, cond_a, &r.img.modulation); // [1, 6d]
    let m_txt = lower_linear_codes(b, cond_a, &r.txt.modulation); // [1, 6d] or [1, 2d]

    // 2. Normalise and modulate both streams.
    let norm = |b: &mut BlockBuilder<'_>, h: Ref, s: &QStream, s_ln: f64, s_out: f64, scale: Ref, shift: Ref| {
        let x = lower_to_codes(b, h, mul_shift(s.stream_scale / s_ln));
        let k = AdaNormConsts::new(d, s_ln, NORM_EPS, s.e, s_out);
        lower_ada_layer_norm(b, x, scale, shift, &k)
    };
    let (shift_a, scale_a, gate_a) = (chunk(b, m_img, 0, d), chunk(b, m_img, 1, d), chunk(b, m_img, 2, d));
    let x1_img = norm(b, h_img, &blk.img, blk.img.s_ln1, blk.img.s_x1, scale_a, shift_a);
    let (t_shift, t_scale) = if blk.pre_only {
        // AdaLayerNormContinuous: scale first, then shift; no gate.
        (chunk(b, m_txt, 1, d), chunk(b, m_txt, 0, d))
    } else {
        (chunk(b, m_txt, 0, d), chunk(b, m_txt, 1, d))
    };
    let x1_txt = norm(b, h_txt, &blk.txt, blk.txt.s_ln1, blk.txt.s_x1, t_scale, t_shift);

    // 3. Joint attention and the gated residuals.
    let (out_img, out_txt) = lower_joint_attention(b, x1_img, x1_txt, &r.attn, &blk.spec, blk.score, blk.ctx, rng);
    let h_img1 = lower_gated_residual(b, h_img, out_img, gate_a, gate_ratio(blk.img.e, blk.img.s_attn_out, blk.img.stream_scale));

    // 4. The image stream's MLP.
    let ff = blk.img.ff.as_ref().expect("the image stream always has a feed-forward");
    let ffr = r.img.ff.as_ref().expect("declared with the stream");
    let (shift_m, scale_m, gate_m) = (chunk(b, m_img, 3, d), chunk(b, m_img, 4, d), chunk(b, m_img, 5, d));
    let x2 = norm(b, h_img1, &blk.img, blk.img.s_ln2, blk.img.s_x2, scale_m, shift_m);
    let f = feed_forward(b, x2, ffr, ff);
    let h_img2 = lower_gated_residual(b, h_img1, f, gate_m, gate_ratio(blk.img.e, blk.img.s_ff2, blk.img.stream_scale));

    // 5. The text stream's second half, unless this is the last block.
    let h_txt2 = match (out_txt, &blk.txt.ff, &r.txt.ff) {
        (Some(out_txt), Some(ff), Some(ffr)) => {
            let gate_a = chunk(b, m_txt, 2, d);
            let h1 = lower_gated_residual(b, h_txt, out_txt, gate_a, gate_ratio(blk.txt.e, blk.txt.s_attn_out, blk.txt.stream_scale));
            let (shift_m, scale_m, gate_m) = (chunk(b, m_txt, 3, d), chunk(b, m_txt, 4, d), chunk(b, m_txt, 5, d));
            let x2 = norm(b, h1, &blk.txt, blk.txt.s_ln2, blk.txt.s_x2, scale_m, shift_m);
            let f = feed_forward(b, x2, ffr, ff);
            lower_gated_residual(b, h1, f, gate_m, gate_ratio(blk.txt.e, blk.txt.s_ff2, blk.txt.stream_scale))
        }
        // The last block: the text stream is read by the attention and goes no further; carry it through.
        _ => b.reshape_fixed(h_txt, &[nt, d as u32]),
    };
    let cond_out = b.reshape_fixed(cond_a, &[1, d as u32]);
    [h_img2, h_txt2, cond_out]
}

/// `Linear → GELU table → Linear`, all `i16` codes.
fn feed_forward(b: &mut BlockBuilder<'_>, x: Ref, r: &QFeedForwardRefs, _q: &QFeedForward) -> Ref {
    let h = lower_linear_codes(b, x, &r.l1);
    let a = b.act_table(h, r.act);
    lower_linear_codes(b, a, &r.l2)
}

/// A projection to the stream's `i32` rail (the context embedder, the patch embedding's cousin).
pub fn lower_to_stream_linear(b: &mut BlockBuilder<'_>, x: Ref, l: &QLinearRefs) -> Ref {
    lower_linear(b, x, l, i32::MIN as i64, i32::MAX as i64, DType::I32)
}

#[cfg(test)]
mod tests {
    use super::super::float::testkit::{inputs, random_source, tiny_config};
    use super::super::float::{Dit, DitInputs};
    use super::super::testkit::run_multi;
    use super::*;

    /// Calibrate the tiny random model on a few runs; the first run's block tensors are kept for the checks.
    fn calibrated(seed: u64, n_txt: usize) -> (Dit, Calib) {
        let cfg = tiny_config();
        let dit = Dit::load(cfg.clone(), &random_source(&cfg, seed)).unwrap();
        let keep: Vec<String> =
            ["cond_a", "b0.h_in_img", "b0.h_in_txt", "b0.h_out_img", "b0.h_out_txt", "b1.h_in_img", "b1.h_in_txt", "b1.h_out_img"]
                .iter()
                .map(|s| s.to_string())
                .collect();
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

    /// Run block `i` as the one layer of a one-position program, its carry read from params quantised from the float
    /// run's own tensors; return the integer outputs as floats `(h_img, h_txt)`.
    fn run_block(dit: &Dit, cal: &Calib, i: usize, n_txt: usize) -> (Vec<f64>, Vec<f64>) {
        let blk = QBlock::new(i, dit, cal, n_txt);
        let cfg = &dit.cfg;
        let (d, n) = (cfg.width(), cfg.grid() * cfg.grid());
        let (s_si, s_st, s_ca) = (cal.scale32("stream_img"), cal.scale32("stream_txt"), cal.scale16("cond_a"));
        let q = |v: &[f64], s: f64| -> Vec<i128> { v.iter().map(|x| (x / s).round() as i128).collect() };
        let h_img = q(&cal.kept[&format!("b{i}.h_in_img")], s_si);
        let h_txt = q(&cal.kept[&format!("b{i}.h_in_txt")], s_st);
        let cond = q(&cal.kept["cond_a"], s_ca);
        let outs = run_multi(
            |pb, sink| {
                let refs = blk.declare(pb, sink);
                (
                    sink.put(pb, "carry.h_img", DType::I32, &[n as u32, d as u32], h_img),
                    sink.put(pb, "carry.h_txt", DType::I32, &[n_txt as u32, d as u32], h_txt),
                    sink.put(pb, "carry.cond", DType::I16, &[1, d as u32], cond),
                    refs,
                )
            },
            // All three carry-out nodes are observed: a node nothing reads is refused by normal form.
            |b, (ph, pt, pc, refs)| lower_block_over(b, &blk, &refs, [ph, pt, pc]).to_vec(),
            1,
        );
        let o = &outs[0];
        (o[0].data.iter().map(|v| *v as f64 * s_si).collect(), o[1].data.iter().map(|v| *v as f64 * s_st).collect())
    }

    #[test]
    fn a_joint_block_is_the_float_block_to_its_lossy_sites() {
        let n_txt = 5;
        let (dit, cal) = calibrated(7, n_txt);
        for i in 0..2 {
            let (img, txt) = run_block(&dit, &cal, i, n_txt);
            let want = &cal.kept[&format!("b{i}.h_out_img")];
            let amax = want.iter().fold(0f64, |m, v| m.max(v.abs()));
            let worst = img.iter().zip(want).fold(0f64, |m, (g, w)| m.max((g - w).abs()));
            // The sites' own roundings: a few percent of the stream's range at most.
            assert!(
                worst <= 0.05 * amax + 4.0 * cal.scale32("stream_img"),
                "block {i}: image stream worst error {worst} against amax {amax}"
            );
            if let Some(want) = cal.kept.get(&format!("b{i}.h_out_txt")) {
                let amax = want.iter().fold(0f64, |m, v| m.max(v.abs()));
                let worst = txt.iter().zip(want).fold(0f64, |m, (g, w)| m.max((g - w).abs()));
                assert!(
                    worst <= 0.05 * amax + 4.0 * cal.scale32("stream_txt"),
                    "block {i}: text stream worst error {worst} against {amax}"
                );
            } else {
                // The last block carries its text stream through unchanged.
                let before = &cal.kept[&format!("b{i}.h_in_txt")];
                let worst = txt.iter().zip(before).fold(0f64, |m, (g, w)| m.max((g - w).abs()));
                assert!(worst <= 2.0 * cal.scale32("stream_txt"), "block {i}: the text carry passes through");
            }
        }
    }
}
