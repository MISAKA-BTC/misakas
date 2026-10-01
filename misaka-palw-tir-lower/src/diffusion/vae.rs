//! **The VAE decoder as a chain of single-position stages** (`GEN_STAGE_VAE_V1`, RFC-0003 §6): the latent to the
//! `ImageRgb8` output, one op group per stage so that a cone stays inside one resnet, one attention or one head.
//!
//! Every stage is a `TirProgramV2` with one `External` input (the previous stage's `Final`: the first reads the
//! denoiser's `i32` latent, the others `i16` codes `[C, H, W]`), a `pre` that commits the input as a leaf, no layers
//! and a `post` that computes the group and ends in a committed `Final` output. The groups are the checkpoint's:
//!
//! | stage | group | lowerers |
//! | --- | --- | --- |
//! | `vae.in` | `z/scaling + shift`, `post_quant_conv`, `conv_in` | narrowing with a bias, conv |
//! | `vae.mid.r0`, `vae.mid.r1`, `vae.up{i}.r{j}` | `ResnetBlock2D` | group norm, SiLU table, conv ×2, shortcut, add |
//! | `vae.mid.at` | the single-head attention over the pixels | group norm, linear ×4, committed-statistics softmax |
//! | `vae.up{i}.us` | nearest ×2 then conv | gather by a pinned table, conv |
//! | `vae.out` | `conv_norm_out`, SiLU, `conv_out`, the image | group norm, table, conv, `Clamp(HAFZ((y + 1)·255/2))` |
//!
//! Scales come from the calibration under the names [`super::vae_float`] notes; a stage's input scale is its
//! predecessor's output scale by construction.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::{DType, Ref, Rounding, TensorType};

use super::act::{QAct, lower_act};
use super::attn::{JointAttnSpec, LOGIT_Q, attn_narrowings, softmax_committed};
use super::calib::Calib;
use super::conv::{ConvSpec, QConv, QConvRefs, lower_conv};
use super::linear::{QLinear, QLinearRefs, lower_linear_codes};
use super::norm::{GroupNormSpec, QGroupNorm, QGroupNormRefs, lower_group_norm};
use super::sink::ParamSink;
use super::stream::{lower_requant, lower_to_codes};
use super::vae_float::{VAE_EPS, Vae};
use crate::quant::mul_shift;

/// One stage of the decoder.
pub struct VaeStage {
    pub name: String,
    pub program: TirProgramV2,
    pub sink: ParamSink,
    /// The input's shape and the scale of its codes (`i32` latent codes for the first stage).
    pub in_shape: Vec<usize>,
    pub in_scale: f64,
    /// The output's shape and the scale of its codes (1.0 for the image).
    pub out_shape: Vec<usize>,
    pub out_scale: f64,
    /// The float site whose tensor this stage's output approximates, when there is one.
    pub site: Option<String>,
}

/// The lowered decoder.
pub struct VaeChain {
    pub stages: Vec<VaeStage>,
}

fn conv_spec(w_shape: &[usize], h: usize, w: usize, pad: usize) -> ConvSpec {
    ConvSpec { cin: w_shape[1], cout: w_shape[0], k: w_shape[2], stride: 1, pad, h, w }
}

fn qconv(vae: &Vae, name: &str, h: usize, w: usize, pad: usize, x_scale: f64, y_scale: f64) -> QConv {
    let spec = conv_spec(&vae.t(&format!("{name}.weight")).0, h, w, pad);
    QConv::new(spec, &vae.f32s(&format!("{name}.weight")), Some(&vae.f32s(&format!("{name}.bias"))), x_scale, y_scale)
}

fn qgn(vae: &Vae, name: &str, c: usize, h: usize, w: usize, sx: f64, sy: f64) -> QGroupNorm {
    let spec = GroupNormSpec { c, groups: vae.cfg.groups, h, w };
    QGroupNorm::new(spec, &vae.f32s(&format!("{name}.weight")), &vae.f32s(&format!("{name}.bias")), VAE_EPS, sx, sy)
}

/// `a + c` for two code tensors at their own scales, into `i16` codes at `s_out`: one narrowing each, an exact add.
pub fn lower_add_codes(b: &mut BlockBuilder<'_>, a: Ref, ra: (i64, i8), c: Ref, rc: (i64, i8)) -> Ref {
    let ta = lower_requant(b, a, ra, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let tc = lower_requant(b, c, rc, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let sum = b.add(ta, tc, DType::I64);
    b.clamp(sum, -32_767, 32_767, DType::I16)
}

/// Build one stage: the input param, `pre` committing it, `post` running `build` over the carry and committing the
/// output. `in_iv` is the input's declared interval.
#[allow(clippy::type_complexity)]
fn make_stage<D>(
    in_dtype: DType,
    in_shape: &[u32],
    in_iv: (i64, i64),
    declare: impl FnOnce(&mut ProgramBuilder, &mut ParamSink) -> D,
    build: impl FnOnce(&mut BlockBuilder<'_>, D, Ref) -> Ref,
) -> Result<(TirProgramV2, ParamSink), String> {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let mut sink = ParamSink::new();
    let input = pb.param("in.x", in_dtype, in_shape, false);
    let d = declare(&mut pb, &mut sink);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let r = b.reshape_fixed(input, in_shape);
        b.finish(&[r])
    };
    let (post, out) = {
        let mut b = pb.block("post", vec![TensorType::fixed(in_dtype, in_shape)]);
        let o = build(&mut b, d, Ref::CarryIn(0));
        b.commit(o);
        let Ref::Node(i) = o else { unreachable!("a stage ends in a node") };
        (b.finish(&[]), i)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let program = TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: in_iv.0, hi: in_iv.1 })],
        OutputDecl::Final { node: out },
    )
    .map_err(|e| format!("a VAE stage does not lift to a version-2 program: {e}"))?;
    Ok((program, sink))
}

/// The nearest-neighbour ×2 table: for each element `(c, 2h, 2w)` the flat index of `(c, h, w)`.
pub fn upsample_index(c: usize, h: usize, w: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(c * h * w * 4);
    for ch in 0..c {
        for hh in 0..2 * h {
            for ww in 0..2 * w {
                out.push(((ch * h + hh / 2) * w + ww / 2) as u32);
            }
        }
    }
    out
}

/// A resnet's quantised parts.
struct QResnet {
    n1: QGroupNorm,
    a1: QAct,
    c1: QConv,
    n2: QGroupNorm,
    a2: QAct,
    c2: QConv,
    sc: Option<QConv>,
    /// `x → s_o` and `c2 → s_o`.
    r_x: (i64, i8),
    r_c2: (i64, i8),
}

struct QResnetRefs {
    n1: QGroupNormRefs,
    a1: Ref,
    c1: QConvRefs,
    n2: QGroupNormRefs,
    a2: Ref,
    c2: QConvRefs,
    sc: Option<QConvRefs>,
}

fn resnet_stage(
    vae: &Vae,
    cal: &Calib,
    prefix: &str,
    s: &str,
    c_in: usize,
    h: usize,
    w: usize,
    s_in: f64,
) -> Result<VaeStage, String> {
    let c_out = vae.t(&format!("{prefix}.conv1.weight")).0[0];
    let sc = |k: &str| cal.scale16(&format!("{s}.{k}"));
    let (sn1, sa1, sc1, sn2, sa2, sc2, so) = (sc("n1"), sc("a1"), sc("c1"), sc("n2"), sc("a2"), sc("c2"), sc("o"));
    let has_sc = vae.has(&format!("{prefix}.conv_shortcut.weight"));
    let s_short = if has_sc { sc("sc") } else { s_in };
    let q = QResnet {
        n1: qgn(vae, &format!("{prefix}.norm1"), c_in, h, w, s_in, sn1),
        a1: QAct::silu(sn1, sa1),
        c1: qconv(vae, &format!("{prefix}.conv1"), h, w, 1, sa1, sc1),
        n2: qgn(vae, &format!("{prefix}.norm2"), c_out, h, w, sc1, sn2),
        a2: QAct::silu(sn2, sa2),
        c2: qconv(vae, &format!("{prefix}.conv2"), h, w, 1, sa2, sc2),
        sc: has_sc.then(|| qconv(vae, &format!("{prefix}.conv_shortcut"), h, w, 0, s_in, s_short)),
        r_x: mul_shift(s_short / so),
        r_c2: mul_shift(sc2 / so),
    };
    let (c_in32, h32, w32) = (c_in as u32, h as u32, w as u32);
    let (spec1, spec2) = (q.c1.spec, q.c2.spec);
    let spec_sc = q.sc.as_ref().map(|c| c.spec);
    let (program, sink) = make_stage(
        DType::I16,
        &[c_in32, h32, w32],
        (-32_767, 32_767),
        |pb, sink| QResnetRefs {
            n1: q.n1.declare(pb, sink, &format!("{prefix}.norm1")),
            a1: q.a1.declare(pb, sink, &format!("{prefix}.act1")),
            c1: q.c1.declare(pb, sink, &format!("{prefix}.conv1")),
            n2: q.n2.declare(pb, sink, &format!("{prefix}.norm2")),
            a2: q.a2.declare(pb, sink, &format!("{prefix}.act2")),
            c2: q.c2.declare(pb, sink, &format!("{prefix}.conv2")),
            sc: q.sc.as_ref().map(|c| c.declare(pb, sink, &format!("{prefix}.conv_shortcut"))),
        },
        |b, r, x| {
            let n1 = lower_group_norm(b, x, &q.n1, &r.n1);
            let a1 = lower_act(b, n1, r.a1);
            let c1 = lower_conv(b, a1, &r.c1, &spec1);
            let n2 = lower_group_norm(b, c1, &q.n2, &r.n2);
            let a2 = lower_act(b, n2, r.a2);
            let c2 = lower_conv(b, a2, &r.c2, &spec2);
            let short = match (&r.sc, spec_sc) {
                (Some(rc), Some(spec)) => lower_conv(b, x, rc, &spec),
                _ => x,
            };
            lower_add_codes(b, short, q.r_x, c2, q.r_c2)
        },
    )?;
    Ok(VaeStage {
        name: s.to_string(),
        program,
        sink,
        in_shape: vec![c_in, h, w],
        in_scale: s_in,
        out_shape: vec![c_out, h, w],
        out_scale: so,
        site: Some(format!("{s}.o")),
    })
}

/// The mid block's attention over the pixels.
#[allow(clippy::too_many_arguments)]
fn attention_stage(
    vae: &Vae,
    cal: &Calib,
    prefix: &str,
    s: &str,
    c: usize,
    h: usize,
    w: usize,
    s_in: f64,
) -> Result<VaeStage, String> {
    let sc = |k: &str| cal.scale16(&format!("{s}.{k}"));
    let (sn, sq, sk, sv, sctx, sp, so) = (sc("n"), sc("q"), sc("k"), sc("v"), sc("ctx"), sc("p"), sc("o"));
    let t = h * w;
    let gn = qgn(vae, &format!("{prefix}.group_norm"), c, h, w, s_in, sn);
    let lin = |name: &str, x: f64, y: f64| {
        let (wt, bs) = (vae.f32s(&format!("{prefix}.{name}.weight")), vae.f32s(&format!("{prefix}.{name}.bias")));
        QLinear::new(&wt, c, c, Some(&bs), x, y)
    };
    let (lq, lk, lv, lo) = (lin("to_q", sn, sq), lin("to_k", sn, sk), lin("to_v", sn, sv), lin("to_out.0", sctx, sp));
    let spec = JointAttnSpec { heads: 1, head_dim: c, n_img: t, n_txt: 0 };
    let (score, ctx) = attn_narrowings(&spec, sq, sk, sv, sctx);
    let (r_x, r_p) = (mul_shift(s_in / so), mul_shift(sp / so));
    let (c32, h32, w32) = (c as u32, h as u32, w as u32);
    let (program, sink) = make_stage(
        DType::I16,
        &[c32, h32, w32],
        (-32_767, 32_767),
        |pb, sink| {
            (
                gn.declare(pb, sink, &format!("{prefix}.group_norm")),
                lq.declare(pb, sink, &format!("{prefix}.to_q")),
                lk.declare(pb, sink, &format!("{prefix}.to_k")),
                lv.declare(pb, sink, &format!("{prefix}.to_v")),
                lo.declare(pb, sink, &format!("{prefix}.to_out.0")),
            )
        },
        |b, (rgn, rq, rk, rv, ro): (QGroupNormRefs, QLinearRefs, QLinearRefs, QLinearRefs, QLinearRefs), x| {
            let n = lower_group_norm(b, x, &gn, &rgn);
            let flat = b.reshape_fixed(n, &[c32, t as u32]);
            let rows = b.transpose(flat, &[1, 0]); // [T, C]
            let q = lower_linear_codes(b, rows, &rq);
            let k = lower_linear_codes(b, rows, &rk);
            let v = lower_linear_codes(b, rows, &rv);
            b.commit(q);
            b.commit(k);
            b.commit(v);
            let kt = b.transpose(k, &[1, 0]);
            let scores = b.matmul(q, kt, DType::I64); // [T, T]
            let m = b.c(DType::I64, score.0 as i128);
            let sh = b.c(DType::I8, score.1 as i128);
            let logits = b.narrow(scores, &Narrowing::new(m, sh, None), i32::MIN as i64, i32::MAX as i64, DType::I32);
            let p = softmax_committed(b, logits, 24 - LOGIT_Q);
            let o = b.matmul(p, v, DType::I64); // [T, C]
            let cm = b.c(DType::I64, ctx.0 as i128);
            let cs = b.c(DType::I8, ctx.1 as i128);
            let codes = b.narrow(o, &Narrowing::new(cm, cs, None), -32_767, 32_767, DType::I16);
            let codes = b.commit(codes);
            let proj = lower_linear_codes(b, codes, &ro); // [T, C]
            let back = b.transpose(proj, &[1, 0]);
            let back = b.reshape_fixed(back, &[c32, h32, w32]);
            lower_add_codes(b, x, r_x, back, r_p)
        },
    )?;
    Ok(VaeStage {
        name: s.to_string(),
        program,
        sink,
        in_shape: vec![c, h, w],
        in_scale: s_in,
        out_shape: vec![c, h, w],
        out_scale: so,
        site: Some(format!("{s}.o")),
    })
}

/// Nearest ×2 then the upsampler's 3×3 convolution.
fn upsample_stage(vae: &Vae, cal: &Calib, prefix: &str, s: &str, c: usize, h: usize, w: usize, s_in: f64) -> Result<VaeStage, String> {
    let s_uc = cal.scale16(&format!("{s}.uc"));
    let q = qconv(vae, &format!("{prefix}.conv"), 2 * h, 2 * w, 1, s_in, s_uc);
    let spec = q.spec;
    let idx = upsample_index(c, h, w);
    let n = (c * h * w) as u32;
    let (c32, h32, w32) = (c as u32, h as u32, w as u32);
    let (program, sink) = make_stage(
        DType::I16,
        &[c32, h32, w32],
        (-32_767, 32_767),
        |pb, sink| {
            (
                sink.put(
                    pb,
                    &format!("{prefix}.nearest_idx"),
                    DType::Idx,
                    &[c32, 2 * h32, 2 * w32],
                    idx.iter().map(|v| *v as i128).collect(),
                ),
                q.declare(pb, sink, &format!("{prefix}.conv")),
            )
        },
        |b, (idx, rc): (Ref, QConvRefs), x| {
            let flat = b.reshape_fixed(x, &[n]);
            let at = b.clamp(idx, 0, n as i64 - 1, DType::Idx);
            let up = b.gather(flat, at, 0, 0); // [C, 2H, 2W]
            lower_conv(b, up, &rc, &spec)
        },
    )?;
    Ok(VaeStage {
        name: s.to_string(),
        program,
        sink,
        in_shape: vec![c, h, w],
        in_scale: s_in,
        out_shape: vec![c, 2 * h, 2 * w],
        out_scale: s_uc,
        site: Some(format!("{s}.uc")),
    })
}

/// The first stage: `z/scaling + shift` as one narrowing with a bias, `post_quant_conv`, `conv_in`.
fn first_stage(vae: &Vae, cal: &Calib, c: usize, h: usize, w: usize, s_x: f64) -> Result<VaeStage, String> {
    let cfg = &vae.cfg;
    let s_z = cal.scale16("vae.z");
    let ratio = mul_shift(s_x / cfg.scaling_factor / s_z);
    let zc = (cfg.shift_factor / s_z).round() as i128;
    let (pq, s_pq) = if cfg.post_quant_conv {
        let sp = cal.scale16("vae.pq");
        (Some(qconv(vae, "post_quant_conv", h, w, 0, s_z, sp)), sp)
    } else {
        (None, s_z)
    };
    let s_ci = cal.scale16("vae.ci");
    let ci = qconv(vae, "decoder.conv_in", h, w, 1, s_pq, s_ci);
    let (spec_pq, spec_ci) = (pq.as_ref().map(|q| q.spec), ci.spec);
    let (c32, h32, w32) = (c as u32, h as u32, w as u32);
    let (program, sink) = make_stage(
        DType::I32,
        &[c32, h32, w32],
        (i32::MIN as i64, i32::MAX as i64),
        |pb, sink| (pq.as_ref().map(|q| q.declare(pb, sink, "post_quant_conv")), ci.declare(pb, sink, "decoder.conv_in")),
        |b, (rpq, rci): (Option<QConvRefs>, QConvRefs), x| {
            let m = b.c(DType::I64, ratio.0 as i128);
            let s = b.c(DType::I8, ratio.1 as i128);
            let z = b.c(DType::I64, zc);
            let codes = b.narrow(x, &Narrowing::new(m, s, Some(z)), -32_767, 32_767, DType::I16);
            let codes = match (rpq, spec_pq) {
                (Some(r), Some(spec)) => lower_conv(b, codes, &r, &spec),
                _ => codes,
            };
            lower_conv(b, codes, &rci, &spec_ci)
        },
    )?;
    Ok(VaeStage {
        name: "vae.in".to_string(),
        program,
        sink,
        in_shape: vec![c, h, w],
        in_scale: s_x,
        out_shape: vec![cfg.block_out_channels[cfg.block_out_channels.len() - 1], h, w],
        out_scale: s_ci,
        site: Some("vae.ci".to_string()),
    })
}

/// The last stage: `conv_norm_out`, SiLU, `conv_out`, and the image `HWC` in `[0, 255]`.
fn last_stage(vae: &Vae, cal: &Calib, c: usize, h: usize, w: usize, s_in: f64) -> Result<VaeStage, String> {
    let cfg = &vae.cfg;
    let (s_no, s_ao, s_img) = (cal.scale16("vae.no"), cal.scale16("vae.ao"), cal.scale16("vae.img"));
    let gn = qgn(vae, "decoder.conv_norm_out", c, h, w, s_in, s_no);
    let act = QAct::silu(s_no, s_ao);
    let co = qconv(vae, "decoder.conv_out", h, w, 1, s_ao, s_img);
    let spec = co.spec;
    // `pixel = Clamp(HAFZ((y + 1) · 255 / 2), 0, 255)` on `y` in Q24 (the RFC's `Div_HAFZ((y + ONE)·255, 2·ONE)`).
    let to_q24 = mul_shift(s_img * (1u64 << 24) as f64);
    let out_c = cfg.out_channels as u32;
    let (c32, h32, w32) = (c as u32, h as u32, w as u32);
    let (program, sink) = make_stage(
        DType::I16,
        &[c32, h32, w32],
        (-32_767, 32_767),
        |pb, sink| {
            (
                gn.declare(pb, sink, "decoder.conv_norm_out"),
                act.declare(pb, sink, "decoder.conv_act"),
                co.declare(pb, sink, "decoder.conv_out"),
            )
        },
        |b, (rgn, ra, rco): (QGroupNormRefs, Ref, QConvRefs), x| {
            let n = lower_group_norm(b, x, &gn, &rgn);
            let a = lower_act(b, n, ra);
            let y = lower_conv(b, a, &rco, &spec); // [3, H, W] codes
            let yq = lower_requant(b, y, to_q24, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let one = b.c(DType::I64, 1 << 24);
            let shifted = b.add(yq, one, DType::I64);
            let k255 = b.c(DType::I64, 255);
            let scaled = b.mul(shifted, k255, DType::I64);
            let two_one = b.c(DType::I64, 2 << 24);
            let px = b.div(scaled, two_one, Rounding::HalfAwayFromZero, DType::I64);
            let px = b.clamp(px, 0, 255, DType::I16);
            b.transpose(px, &[1, 2, 0]) // [H, W, 3]
        },
    )?;
    let _ = out_c;
    Ok(VaeStage {
        name: "vae.out".to_string(),
        program,
        sink,
        in_shape: vec![c, h, w],
        in_scale: s_in,
        out_shape: vec![h, w, cfg.out_channels],
        out_scale: 1.0,
        site: None,
    })
}

/// Lower the decoder for a latent of `[C, side, side]` at the denoiser's latent scale `s_x`.
pub fn lower_vae(vae: &Vae, cal: &Calib, side: usize, s_x: f64) -> Result<VaeChain, String> {
    let cfg = &vae.cfg;
    let top = *cfg.block_out_channels.last().ok_or("no block_out_channels")?;
    let mut stages = vec![first_stage(vae, cal, cfg.latent_channels, side, side, s_x)?];
    let (mut c, mut h, mut w) = (top, side, side);
    let mut s_cur = stages[0].out_scale;
    let mut push = |st: VaeStage, stages: &mut Vec<VaeStage>, cur: &mut f64| {
        *cur = st.out_scale;
        stages.push(st);
    };
    let st = resnet_stage(vae, cal, "decoder.mid_block.resnets.0", "vae.mid.r0", c, h, w, s_cur)?;
    push(st, &mut stages, &mut s_cur);
    if cfg.mid_attention {
        let st = attention_stage(vae, cal, "decoder.mid_block.attentions.0", "vae.mid.at", c, h, w, s_cur)?;
        push(st, &mut stages, &mut s_cur);
    }
    let st = resnet_stage(vae, cal, "decoder.mid_block.resnets.1", "vae.mid.r1", c, h, w, s_cur)?;
    push(st, &mut stages, &mut s_cur);
    for (i, (_, c_out, up)) in cfg.up_blocks().into_iter().enumerate() {
        for j in 0..cfg.layers_per_block + 1 {
            let st =
                resnet_stage(vae, cal, &format!("decoder.up_blocks.{i}.resnets.{j}"), &format!("vae.up{i}.r{j}"), c, h, w, s_cur)?;
            c = c_out;
            push(st, &mut stages, &mut s_cur);
        }
        if up {
            let st =
                upsample_stage(vae, cal, &format!("decoder.up_blocks.{i}.upsamplers.0"), &format!("vae.up{i}.us"), c, h, w, s_cur)?;
            h *= 2;
            w *= 2;
            push(st, &mut stages, &mut s_cur);
        }
    }
    stages.push(last_stage(vae, cal, c, h, w, s_cur)?);
    Ok(VaeChain { stages })
}

#[cfg(test)]
mod tests {
    use super::super::vae_float::testkit::{latent, random_source, tiny_config};
    use super::super::vae_float::{Fm, VaeConfig};
    use super::*;
    use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
    use misaka_palw_tir::interval_v2::analyze_ranges_v2;
    use misaka_palw_tir::{RunState, Tensor};

    /// Run one stage on an input tensor.
    pub(crate) fn run_stage(st: &VaeStage, x: Tensor) -> Tensor {
        let interp = InterpreterV2::new(&st.program).unwrap_or_else(|e| panic!("{}: the stage validates: {e}", st.name));
        if let Err(e) = analyze_ranges_v2(&st.program) {
            panic!("{}: the stage is admissible at full param ranges: {e}", st.name);
        }
        let names: Vec<String> = st.program.params.iter().map(|p| p.name.clone()).collect();
        let params = st.sink.bind(names.iter().map(String::as_str));
        let mut inp = MapInputs::default();
        inp.constant.insert(0, x);
        interp.step(&params, &inp, &mut RunState::default(), 0).unwrap_or_else(|e| panic!("{}: {e}", st.name)).output
    }

    fn calibrated(cfg: &VaeConfig, vae: &Vae, side: usize) -> Calib {
        let mut cal = Calib::new();
        for k in 0..3u64 {
            let mut run = Calib::new();
            vae.decode(&latent(cfg, side, 40 + k), &mut run);
            cal.merge(&run);
        }
        cal
    }

    #[test]
    fn the_nearest_table_matches_the_float_upsample() {
        let (c, h, w) = (2usize, 2usize, 3usize);
        let x = Fm { c, h, w, d: (0..c * h * w).map(|i| i as f64).collect() };
        let up = super::super::vae_float::upsample_nearest(&x);
        let idx = upsample_index(c, h, w);
        assert_eq!(idx.len(), up.d.len());
        for (i, j) in idx.iter().enumerate() {
            assert_eq!(up.d[i], x.d[*j as usize]);
        }
    }

    #[test]
    fn every_stage_is_admissible_and_the_chain_follows_the_float_decoder() {
        let cfg = tiny_config();
        let vae = Vae::load(cfg.clone(), &random_source(&cfg, 5)).unwrap();
        let side = 8;
        let cal = calibrated(&cfg, &vae, side);
        let s_x = 1.0 / (1u64 << 20) as f64;
        let chain = lower_vae(&vae, &cal, side, s_x).expect("the decoder lowers");
        assert_eq!(chain.stages.len(), 1 + 3 + 2 + 1 + 2 + 1, "in, mid r0/at/r1, up0 r0/r1/us, up1 r0/r1, out");

        // A latent on the denoiser's grid, quantised at s_x; the float run's tensors at every stage's site.
        let z = latent(&cfg, side, 90);
        let mut kept = Calib::new();
        for st in &chain.stages {
            if let Some(s) = &st.site {
                kept.keep.insert(s.clone());
            }
        }
        let img = vae.decode(&z, &mut kept);
        let mut cur = Tensor::new(DType::I32, vec![z.c, z.h, z.w], z.d.iter().map(|v| (v / s_x).round() as i128).collect()).unwrap();
        for st in &chain.stages {
            cur = run_stage(st, cur);
            assert_eq!(cur.shape, st.out_shape, "{}: output shape", st.name);
            if let Some(site) = &st.site {
                let want = &kept.kept[site];
                let amax = want.iter().fold(0f64, |m, v| m.max(v.abs()));
                let worst = cur.data.iter().zip(want).fold(0f64, |m, (g, w)| m.max((*g as f64 * st.out_scale - w).abs()));
                eprintln!("{}: worst |int - float| {worst:.4} ({:.2}% of site amax {amax:.3})", st.name, 100.0 * worst / amax);
                assert!(worst <= 0.20 * amax + 4.0 * st.out_scale, "{}: worst error {worst} against amax {amax}", st.name);
            }
        }
        // The image: u8 HWC against the float image's `round(clamp(y/2 + 1/2, 0, 1) · 255)`.
        assert_eq!(cur.shape, vec![16, 16, 3]);
        let mut worst = 0i128;
        for hh in 0..16 {
            for ww in 0..16 {
                for ch in 0..3 {
                    let y = img.d[(ch * 16 + hh) * 16 + ww];
                    let want = ((y / 2.0 + 0.5).clamp(0.0, 1.0) * 255.0).round() as i128;
                    worst = worst.max((cur.data[(hh * 16 + ww) * 3 + ch] - want).abs());
                }
            }
        }
        eprintln!("the image: worst |int - float| {worst} of 255");
        assert!(worst <= 40, "the integer image tracks the float one (bring-up bound), worst {worst}");
    }
}
