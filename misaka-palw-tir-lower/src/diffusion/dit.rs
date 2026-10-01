//! **The denoiser stage** (RFC-0003 §6, activation step 6): `SD3Transformer2DModel` plus the flow-matching Euler update
//! as ONE `TirProgramV2` run by the pipeline's `JobSteps` stage — one step = one position.
//!
//! * **`pre`** — the latent at this position (the stage's `Random { IMAGE_INIT_NOISE_V1, Normal }` input at position 0,
//!   else the latent `Fixed` state), the patch embedding, the timestep sinusoid row by `base[steps_index] + pos`, the two
//!   conditioning embedders and their sum, `SiLU` of the conditioning, the text rows through the context embedder.
//!   Its carry is `(h_img, h_txt, cond_a)`.
//! * **layers** — one program block per transformer block ([`super::block`]); the last is `context_pre_only`.
//! * **`post`** — the output `AdaLayerNormContinuous`, `proj_out`, unpatchify to `[C, H, W]`, and the Euler update
//!   `x' = StateWrite(x + Δσ_i · v)`; the stage's `Final` output is the new latent.
//!
//! The external inputs are the text stage's rows `[L, joint]` and the pooled vector `[1, pooled]` (`i32` at the
//! encoder's own unit, narrowed here to `i16` codes at the calibrated site scales this module reports), the job's step count (a rank-0 `idx` in the offered range) and the noise.
//! Params are named by the checkpoint's own module names; the lowering is a function of `(weights, calibration,
//! configuration)` and nothing else.

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, RandomDist, TirProgramV2};
use misaka_palw_tir::{DType, Ref};

use super::act::QAct;
use super::ada::{AdaNormConsts, lower_ada_layer_norm};
use super::block::{NORM_EPS, QBlock, lower_block, lower_to_stream_linear, qlin};
use super::calib::Calib;
use super::conv::ConvSpec;
use super::embed::{
    QEmbedder, QPatchEmbed, TIMESTEP_CODE_SCALE, TimestepTable, cropped_pos_embed, lower_cond_sum, lower_embedder, lower_patch_embed,
    lower_step_row_index, lower_timestep_row,
};
use super::float::Dit;
use super::linear::lower_linear_codes;
use super::sampler::{SamplerTables, declare_unpatchify, lower_euler_step, lower_initial_latent, lower_unpatchify};
use super::sink::ParamSink;
use super::stream::lower_to_codes;
use crate::quant::mul_shift;

/// RFC-0003 §I.1.4's `IMAGE_INIT_NOISE_V1` domain id.
pub const IMAGE_INIT_NOISE_DOMAIN: u16 = 1;

/// What the stage is lowered for.
#[derive(Clone, Debug)]
pub struct DitStageSpec {
    /// The text rows the stage reads (`L`: the class's padded prompt length).
    pub n_txt: usize,
    /// The latent's fixed point, `2^-q_lat` per code.
    pub q_lat: u32,
    /// The offered step counts, ascending.
    pub counts: Vec<u32>,
    /// The scheduler's `shift` and `num_train_timesteps`.
    pub shift: f64,
    pub n_train: f64,
    /// The float value of one code of the text stage's rows and of its pooled vector (`i32` inputs: an encoder's
    /// embedding is `i32` at a power-of-two unit); the stage narrows them to its own `i16` site scales.
    pub text_unit: f64,
    pub pooled_unit: f64,
}

/// The stage's input indices (`Ref::Input(2 + k)`; the pipeline's bindings are positional in this order).
pub const INPUT_TEXT: u16 = 0;
pub const INPUT_POOLED: u16 = 1;
pub const INPUT_STEPS: u16 = 2;
pub const INPUT_NOISE: u16 = 3;

/// The scales the stage's neighbours must meet: the text stage's rows and the pooled vector are `i16` codes at these.
#[derive(Clone, Copy, Debug)]
pub struct DitScales {
    pub txt_in: f64,
    pub pooled_in: f64,
    /// The latent state's code (`2^-q_lat`) and the velocity's.
    pub latent: f64,
    pub velocity: f64,
    pub stream_img: f64,
    pub stream_txt: f64,
}

pub struct DitStage {
    pub program: TirProgramV2,
    pub sink: ParamSink,
    pub scales: DitScales,
    pub tables: SamplerTables,
    pub timesteps: TimestepTable,
    pub spec: DitStageSpec,
}

/// Lower the denoiser stage.
pub fn lower_dit_stage(dit: &Dit, cal: &Calib, spec: &DitStageSpec) -> Result<DitStage, String> {
    let cfg = &dit.cfg;
    let (d, g, p, c) = (cfg.width(), cfg.grid(), cfg.patch_size, cfg.in_channels);
    let (n, l, side) = (g * g, spec.n_txt, cfg.sample_size);
    if cfg.out_channels != c {
        return Err("a denoiser whose output channels differ from its input's is a later profile (learned sigma)".to_string());
    }
    if spec.counts.is_empty() {
        return Err("no step counts offered".to_string());
    }
    let tables = SamplerTables::new(&spec.counts, spec.shift, spec.n_train);
    let timesteps = TimestepTable::new(&spec.counts, &tables.timesteps, 256, true, 0.0, 10_000.0);

    // ---- scales ----
    let s_x = 1.0 / (1u64 << spec.q_lat) as f64;
    let (s_lat, s_si, s_st) = (cal.scale16("lat_in"), cal.scale32("stream_img"), cal.scale32("stream_txt"));
    let (s_cond, s_ca) = (cal.scale16("cond"), cal.scale16("cond_a"));
    let (s_txt, s_pool) = (cal.scale16("txt_in"), cal.scale16("pe_in"));
    let (s_no_in, s_no_x, s_vel) = (cal.scale16("no_in"), cal.scale16("no_x"), cal.scale16("vel"));
    let e_no = cal.pow2_exp("no_mod");

    // ---- the quantised pieces ----
    let conv = ConvSpec { cin: c, cout: d, k: p, stride: p, pad: 0, h: side, w: side };
    let pos32: Vec<f32> = dit.f32s("pos_embed.pos_embed");
    let pos = cropped_pos_embed(&pos32, cfg.pos_max, d, g, g);
    let patch = QPatchEmbed::new(conv, &dit.f32s("pos_embed.proj.weight"), Some(&dit.f32s("pos_embed.proj.bias")), &pos, s_lat, s_si);
    let embedder = |prefix: &str, inn: usize, x: f64, h: &str, a: &str| {
        QEmbedder::new(
            &dit.f32s(&format!("{prefix}.linear_1.weight")),
            &dit.f32s(&format!("{prefix}.linear_1.bias")),
            &dit.f32s(&format!("{prefix}.linear_2.weight")),
            &dit.f32s(&format!("{prefix}.linear_2.bias")),
            inn,
            d,
            x,
            cal.scale16(h),
            cal.scale16(a),
            s_cond,
        )
    };
    let te = embedder("time_text_embed.timestep_embedder", 256, TIMESTEP_CODE_SCALE, "te_h", "te_a");
    let pe = embedder("time_text_embed.text_embedder", cfg.pooled_dim, s_pool, "pe_h", "pe_a");
    let ctx_embed = qlin(dit, "context_embedder", s_txt, s_st);
    let cond_silu = QAct::silu(s_cond, s_ca);
    let blocks: Vec<QBlock> = (0..cfg.num_layers).map(|i| QBlock::new(i, dit, cal, l)).collect();
    let no_mod = qlin(dit, "norm_out.linear", s_ca, 1.0 / (1u64 << e_no) as f64);
    let proj_out = qlin(dit, "proj_out", s_no_x, s_vel);

    // ---- declare: inputs first, then the state and every param ----
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let mut sink = ParamSink::new();
    let text_in = pb.param("in.text", DType::I32, &[l as u32, cfg.joint_dim as u32], false);
    let pooled_in = pb.param("in.pooled", DType::I32, &[1, cfg.pooled_dim as u32], false);
    let steps_in = pb.param("in.steps", DType::Idx, &[], false);
    let noise_in = pb.param("in.noise", DType::I32, &[c as u32, side as u32, side as u32], false);
    let latent = pb.fixed_state("latent", DType::I32, &[c as u32, side as u32, side as u32], i32::MIN as i64, i32::MAX as i64, false);

    let patch_r = patch.declare(&mut pb, &mut sink, "pos_embed.proj");
    let ts_r = timesteps.declare(&mut pb, &mut sink, "time_text_embed.timestep_table");
    let te_r = te.declare(&mut pb, &mut sink, "time_text_embed.timestep_embedder");
    let pe_r = pe.declare(&mut pb, &mut sink, "time_text_embed.text_embedder");
    let ctx_r = ctx_embed.declare(&mut pb, &mut sink, "context_embedder");
    let silu_r = cond_silu.declare(&mut pb, &mut sink, "time_text_embed.cond_silu");
    let block_r: Vec<_> = blocks.iter().map(|b| b.declare(&mut pb, &mut sink)).collect();
    let no_r = no_mod.declare(&mut pb, &mut sink, "norm_out.linear");
    let proj_r = proj_out.declare(&mut pb, &mut sink, "proj_out");
    let unpatch_r = declare_unpatchify(&mut pb, &mut sink, "unpatchify", c, g, g, p);
    let dsigma_r = tables.declare(&mut pb, &mut sink, "sampler");

    // ---- pre ----
    let carry = vec![
        misaka_palw_tir::TensorType::fixed(DType::I32, &[n as u32, d as u32]),
        misaka_palw_tir::TensorType::fixed(DType::I32, &[l as u32, d as u32]),
        misaka_palw_tir::TensorType::fixed(DType::I16, &[1, d as u32]),
    ];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = lower_initial_latent(&mut b, noise_in, Ref::State(latent), spec.q_lat);
        let xc = lower_to_codes(&mut b, x, mul_shift(s_x / s_lat));
        let h_img = lower_patch_embed(&mut b, xc, &patch_r, &conv);
        let row = lower_timestep_row(&mut b, steps_in, &ts_r, &timesteps);
        let t = lower_embedder(&mut b, row, &te_r);
        let pooled16 = lower_to_codes(&mut b, pooled_in, mul_shift(spec.pooled_unit / s_pool));
        let q = lower_embedder(&mut b, pooled16, &pe_r);
        let cond = lower_cond_sum(&mut b, t, q);
        let cond_a = b.act_table(cond, silu_r);
        let text16 = lower_to_codes(&mut b, text_in, mul_shift(spec.text_unit / s_txt));
        let h_txt = lower_to_stream_linear(&mut b, text16, &ctx_r);
        b.finish(&[h_img, h_txt, cond_a])
    };

    // ---- the transformer blocks ----
    let mut layers = Vec::new();
    for (blk, r) in blocks.iter().zip(&block_r) {
        let mut b = pb.block(&format!("transformer_blocks.{}", blk.index), carry.clone());
        let outs = lower_block(&mut b, blk, r);
        layers.push(b.finish(&outs));
    }

    // ---- post ----
    let (post, out_node) = {
        let mut b = pb.block("post", carry);
        let m = lower_linear_codes(&mut b, Ref::CarryIn(2), &no_r); // [1, 2d]: scale, then shift
        let scale = b.slice(m, 1, 0, d as u32);
        let shift = b.slice(m, 1, d as u32, d as u32);
        let xin = lower_to_codes(&mut b, Ref::CarryIn(0), mul_shift(s_si / s_no_in));
        let k = AdaNormConsts::new(d, s_no_in, NORM_EPS, e_no, s_no_x);
        let x = lower_ada_layer_norm(&mut b, xin, scale, shift, &k);
        let rows = lower_linear_codes(&mut b, x, &proj_r); // [N, p·p·C]
        let v = lower_unpatchify(&mut b, rows, unpatch_r, n * p * p * c);
        let at = lower_step_row_index(&mut b, steps_in, ts_r.base, spec.counts.len() as u32 - 1, timesteps.total_rows() as u32);
        let dsig = b.gather(dsigma_r, at, 0, 0);
        let cur = lower_initial_latent(&mut b, noise_in, Ref::State(latent), spec.q_lat);
        let new = lower_euler_step(&mut b, cur, v, dsig, mul_shift(s_vel / (1u64 << 24) as f64 / s_x), latent);
        // NF-28/29: the output node, and a post StateWrite, are commit points — the latent's next value is a leaf.
        b.commit(new);
        let Ref::Node(out) = new else { unreachable!("a StateWrite is a node") };
        (b.finish(&[]), out)
    };

    let v1 = pb.finish(pre, layers, post, out_node);
    let param_of = |r: Ref| match r {
        Ref::Param(j) => j,
        _ => unreachable!("declared as a param"),
    };
    let inputs = [
        (param_of(text_in), InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 }),
        (param_of(pooled_in), InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 }),
        (param_of(steps_in), InputSource::External { lo: 0, hi: spec.counts.len() as i64 - 1 }),
        (param_of(noise_in), InputSource::Random { domain: IMAGE_INIT_NOISE_DOMAIN, dist: RandomDist::Normal, per_step: false }),
    ];
    let program = TirProgramV2::from_v1_lifting_params(&v1, &inputs, OutputDecl::Final { node: out_node })
        .map_err(|e| format!("the denoiser stage does not lift to a version-2 program: {e}"))?;
    Ok(DitStage {
        program,
        sink,
        scales: DitScales { txt_in: s_txt, pooled_in: s_pool, latent: s_x, velocity: s_vel, stream_img: s_si, stream_txt: s_st },
        tables,
        timesteps,
        spec: spec.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::float::testkit::{inputs, random_source, tiny_config};
    use super::super::float::{Dit, DitInputs};
    use super::super::testkit::Lcg;
    use super::*;
    use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
    use misaka_palw_tir::interval_v2::analyze_ranges_v2;
    use misaka_palw_tir::{RunState, Tensor};

    #[test]
    fn the_denoiser_stage_validates_is_admissible_and_follows_the_float_euler_loop() {
        let cfg = tiny_config();
        let n_txt = 5;
        let dit = Dit::load(cfg.clone(), &random_source(&cfg, 11)).unwrap();
        let spec = DitStageSpec {
            n_txt,
            q_lat: 20,
            counts: vec![2, 4],
            shift: 3.0,
            n_train: 1000.0,
            text_unit: 1.0 / 65_536.0,
            pooled_unit: 1.0 / 65_536.0,
        };
        let tables = SamplerTables::new(&spec.counts, spec.shift, spec.n_train);

        // Calibrate on float runs at the sampler's own timesteps (two seeds of text and latents).
        let mut cal = Calib::new();
        for k in 0..3u64 {
            for (si, steps) in spec.counts.iter().enumerate() {
                for i in 0..*steps as usize {
                    let (latent, _, text, pooled) = inputs(&cfg, 200 + k, n_txt);
                    let mut run = Calib::new();
                    dit.forward(
                        &DitInputs { latent: &latent, timestep: tables.timesteps[si][i], text: &text, n_txt, pooled: &pooled },
                        &mut run,
                    );
                    cal.merge(&run);
                }
            }
        }
        cal.unify(&["cond", "te_out", "pe_out"]);

        let stage = lower_dit_stage(&dit, &cal, &spec).expect("the stage lowers");
        let interp = InterpreterV2::new(&stage.program).expect("the program validates under the version-2 rules");
        analyze_ranges_v2(&stage.program).expect("the stage is admissible at full param ranges");
        let names: Vec<String> = stage.program.params.iter().map(|p| p.name.clone()).collect();
        let params = stage.sink.bind(names.iter().map(String::as_str));

        // One job: 4 steps, seeded noise (Q24 gauss words, |x| <= 3 sigma), the float run's text and pooled.
        let steps = 4usize;
        let si = 1;
        let mut rng = Lcg(77);
        let len = cfg.in_channels * cfg.sample_size * cfg.sample_size;
        let noise: Vec<i128> = (0..len).map(|_| (rng.unit() * 3.0 * (1u64 << 24) as f64) as i128).collect();
        let (_, _, text, pooled) = inputs(&cfg, 301, n_txt);
        let q32 = |v: &[f64], s: f64| -> Vec<i128> { v.iter().map(|x| (x / s).round() as i128).collect() };
        let mut inp = MapInputs::default();
        inp.constant.insert(INPUT_TEXT, Tensor::new(DType::I32, vec![n_txt, cfg.joint_dim], q32(&text, spec.text_unit)).unwrap());
        inp.constant.insert(INPUT_POOLED, Tensor::new(DType::I32, vec![1, cfg.pooled_dim], q32(&pooled, spec.pooled_unit)).unwrap());
        inp.constant.insert(INPUT_STEPS, Tensor::new(DType::Idx, vec![], vec![si as i128]).unwrap());
        inp.constant.insert(
            INPUT_NOISE,
            Tensor::new(DType::I32, vec![cfg.in_channels, cfg.sample_size, cfg.sample_size], noise.clone()).unwrap(),
        );

        let mut st = RunState::default();
        let s_x = stage.scales.latent;
        let mut prev: Vec<f64> = noise.iter().map(|v| *v as f64 / (1u64 << 24) as f64).collect();
        let mut worst_rel = 0f64;
        for i in 0..steps {
            let out = interp.step(&params, &inp, &mut st, 0).unwrap_or_else(|e| panic!("step {i}: {e}"));
            let x_new: Vec<f64> = out.output.data.iter().map(|v| *v as f64 * s_x).collect();
            // The float velocity at the integer run's own latent (so one step's error is measured alone).
            let mut scratch = Calib::new();
            let v_float = dit.forward(
                &DitInputs { latent: &prev, timestep: stage.tables.timesteps[si][i], text: &text, n_txt, pooled: &pooled },
                &mut scratch,
            );
            let dsig = (stage.tables.sigmas[si][i + 1] - stage.tables.sigmas[si][i]) as f64;
            let amax = v_float.iter().fold(0f64, |m, v| m.max(v.abs()));
            let mut worst = 0f64;
            for e in 0..len {
                let v_int = (x_new[e] - prev[e]) / dsig;
                worst = worst.max((v_int - v_float[e]).abs());
            }
            eprintln!("step {i}: velocity amax {amax:.4}, worst |v_int - v_float| {worst:.4} ({:.2}% of amax)", 100.0 * worst / amax);
            worst_rel = worst_rel.max(worst / amax);
            prev = x_new;
        }
        // Bring-up bound only (the fidelity thresholds are measured and frozen by the harness, not written here).
        assert!(worst_rel < 0.30, "the integer denoiser tracks the float one: worst relative velocity error {worst_rel}");
    }
}
