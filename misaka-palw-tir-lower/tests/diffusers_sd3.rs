//! **RFC-0003 §6 (activation step 6): the reduced SD3 / MMDiT fixture, lowered to a pipeline and held to the float run.**
//!
//! The fixture is generated once by `tools/gen_diffusers_sd3_fixture.py` (diffusers 0.40, tiny seeded random configs,
//! `HF_HUB_OFFLINE=1`; see that file's header) into `tests/fixtures/hf-diff/sd3_tiny/`. A checkout without it skips
//! these tests with a line saying so — the fixture is a generated artifact, like the other `hf-*` ones.
//!
//! What runs here:
//!
//! 1. **the float reference** — the Rust `f64` denoiser (`diffusion::float`) and VAE (`diffusion::vae_float`) run on the
//!    fixture's own weights, and, where the python reference outputs exist (`reference/case_*.json`), are compared
//!    with diffusers' velocity per step and image (the float reference is held to diffusers before anything is held
//!    to it);
//! 2. **calibration** — the float Euler loop over a calibration set of prompts and seeds, the integer text stages' rows
//!    and pooled vectors as the denoiser's inputs;
//! 3. **the lowering** — text rows and pooled stages (the HF frontend), the denoise stage, the VAE chain, the pipeline
//!    (`validate_pipeline`: every edge's shape, dtype and interval proved), admission;
//! 4. **fidelity** — the integer pipeline against the Rust float pipeline, step by step, on the SAME noise, and against
//!    diffusers when its reference is present. Thresholds are measured and printed here and frozen by the record,
//!    not written down before the first run.
//!
//! To produce the diffusers reference for the evaluation cases: `cargo test --test diffusers_sd3 -- --ignored write_requests`
//! writes `requests/case_*.json` (the integer run's noise and sigma table); then for each, from this crate directory,
//! `python tools/gen_diffusers_sd3_fixture.py reference requests/case_K.json reference/case_K.json`.

use std::path::{Path, PathBuf};

use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
use misaka_palw_tir::pipeline::{PipelineJob, RandomSource, run_pipeline};
use misaka_palw_tir::program_v2::RandomDist;
use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_lower::diffusion::calib::Calib;
use misaka_palw_tir_lower::diffusion::dit::{DitStageSpec, lower_dit_stage};
use misaka_palw_tir_lower::diffusion::float::{Dit, DitInputs, Sd3Config};
use misaka_palw_tir_lower::diffusion::pipeline::{TextStagesV1, assemble_sd3_pipeline};
use misaka_palw_tir_lower::diffusion::sampler::SamplerTables;
use misaka_palw_tir_lower::diffusion::text::{ClipStage, lower_clip_stage};
use misaka_palw_tir_lower::diffusion::vae::lower_vae;
use misaka_palw_tir_lower::diffusion::vae_float::{Fm, Vae, VaeConfig};
use misaka_palw_tir_lower::encoder::EncoderOutput;
use misaka_palw_tir_lower::weights::Checkpoint;

const BOS: u32 = 62;
const EOS: u32 = 63;
const PAD: u32 = 63;
/// The class's padded template length `L` (the denoiser's text rows).
const L: usize = 8;
const COUNTS: [u32; 2] = [2, 4];
const Q_LAT: u32 = 20;

fn fixture() -> Option<PathBuf> {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-diff/sd3_tiny");
    if d.join("transformer/config.json").exists() && d.join("text/model.safetensors").exists() && d.join("vae/config.json").exists() {
        Some(d)
    } else {
        eprintln!("skipped: {} holds no generated fixture (run tools/gen_diffusers_sd3_fixture.py)", d.display());
        None
    }
}

fn json(p: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("json")
}

/// `bos ‖ prompt ‖ eos ‖ pad…` to `L`.
fn padded(prompt: &[u32]) -> Vec<u32> {
    let mut t = vec![BOS];
    t.extend_from_slice(prompt);
    t.push(EOS);
    while t.len() < L {
        t.push(PAD);
    }
    t
}

/// A seeded stand-in for `R`'s `Normal` draw over the init-noise domain: Q24 Gaussian words in `PALW_GAUSS_Q24_V1`'s
/// range (Box–Muller over a 64-bit LCG). The integer run and the float runs are fed the SAME words.
fn noise_words(seed: u64, n: usize) -> Vec<i128> {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((s >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            let g = (-2.0 * next().ln()).sqrt() * (2.0 * std::f64::consts::PI * next()).cos();
            ((g * (1u64 << 24) as f64).round() as i128).clamp(-72_560_101, 72_560_101)
        })
        .collect()
}

struct Noise(Vec<i128>);

impl RandomSource for Noise {
    fn random(&self, _domain: u16, dist: RandomDist, _step: u32, shape: &[u32]) -> Option<Tensor> {
        assert_eq!(dist, RandomDist::Normal);
        let n: usize = shape.iter().map(|d| *d as usize).product();
        assert_eq!(n, self.0.len());
        Tensor::new(DType::I32, shape.iter().map(|d| *d as usize).collect(), self.0.clone()).ok()
    }
}

/// The text stages' float outputs for a template: rows `[L, d]` and the pooled vector.
fn text_floats(rows: &ClipStage, pool: &ClipStage, prompt: &[u32]) -> (Vec<f64>, Vec<f64>) {
    let run = |st: &ClipStage, toks: &[u32]| {
        let interp = InterpreterV2::new(&st.program).expect("the text program validates");
        interp.run(&st.params, &MapInputs::default(), toks).expect("the text program runs")
    };
    let r = run(rows, &padded(prompt));
    let rows_f: Vec<f64> = r.iter().flat_map(|s| s.output.data.iter().map(|v| *v as f64 * rows.unit)).collect();
    let mut unpadded = vec![BOS];
    unpadded.extend_from_slice(prompt);
    unpadded.push(EOS);
    let p = run(pool, &unpadded);
    let pooled_f: Vec<f64> = p.last().unwrap().output.data.iter().map(|v| *v as f64 * pool.unit).collect();
    (rows_f, pooled_f)
}

/// The float Euler loop over the Rust denoiser: the latent after each step, and the velocity.
fn float_loop(
    dit: &Dit,
    tables: &SamplerTables,
    si: usize,
    noise: &[f64],
    rows: &[f64],
    pooled: &[f64],
    cal: &mut Calib,
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut lat = noise.to_vec();
    let (mut lats, mut vels) = (Vec::new(), Vec::new());
    for i in 0..tables.timesteps[si].len() {
        let v = dit.forward(&DitInputs { latent: &lat, timestep: tables.timesteps[si][i], text: rows, n_txt: L, pooled }, cal);
        let ds = tables.sigmas[si][i + 1] - tables.sigmas[si][i];
        for (x, vv) in lat.iter_mut().zip(&v) {
            *x += ds * vv;
        }
        lats.push(lat.clone());
        vels.push(v);
    }
    (lats, vels)
}

fn psnr(a: &[i128], b: &[f64]) -> f64 {
    let mse = a.iter().zip(b).map(|(x, y)| (*x as f64 - y).powi(2)).sum::<f64>() / a.len() as f64;
    if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

struct Evals {
    cases: Vec<(Vec<u32>, u64, u32)>,
}

fn evaluation_cases() -> Evals {
    Evals { cases: vec![(vec![5, 9, 13], 1, 4), (vec![2, 7], 2, 2), (vec![20, 21, 22, 23], 3, 4), (vec![11], 4, 2)] }
}

#[test]
fn the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline() {
    let Some(dir) = fixture() else { return };
    let cfg = Sd3Config::from_json(&json(&dir.join("transformer/config.json"))).expect("transformer config");
    let vcfg = VaeConfig::from_json(&json(&dir.join("vae/config.json"))).expect("vae config");
    let dit = Dit::load(
        cfg.clone(),
        &Checkpoint::open(&dir.join("transformer/diffusion_pytorch_model.safetensors")).expect("transformer weights"),
    )
    .expect("the denoiser's tensors load");
    let vae = Vae::load(vcfg.clone(), &Checkpoint::open(&dir.join("vae/diffusion_pytorch_model.safetensors")).expect("vae weights"))
        .expect("vae tensors");
    let sch = json(&dir.join("scheduler/scheduler_config.json"));
    let (shift, n_train) = (sch["shift"].as_f64().unwrap(), sch["num_train_timesteps"].as_f64().unwrap());
    let tables = SamplerTables::new(&COUNTS, shift, n_train);
    let (c, side) = (cfg.in_channels, cfg.sample_size);
    let len = c * side * side;

    // ---- the text stages (integer), calibrated on templated random prompts ----
    let calib_prompts: Vec<Vec<u32>> = vec![vec![3, 4], vec![30, 31, 32], vec![17], vec![40, 8, 9, 10], vec![50, 51]];
    let calib_seqs: Vec<Vec<usize>> = calib_prompts.iter().map(|p| padded(p).iter().map(|t| *t as usize).collect()).collect();
    let pool_seqs: Vec<Vec<usize>> = calib_prompts
        .iter()
        .map(|p| {
            let mut t = vec![BOS as usize];
            t.extend(p.iter().map(|x| *x as usize));
            t.push(EOS as usize);
            t
        })
        .collect();
    let rows_stage = lower_clip_stage(&dir.join("text"), false, EncoderOutput::Rows, &calib_seqs).expect("text rows stage");
    let pool_stage = lower_clip_stage(&dir.join("text"), true, EncoderOutput::Final, &pool_seqs).expect("text pooled stage");

    // ---- calibration: the float Euler loop on the calibration set, the integer text outputs as inputs ----
    let (mut cal, mut vcal) = (Calib::new(), Calib::new());
    for (k, p) in calib_prompts.iter().enumerate() {
        let (rows, pooled) = text_floats(&rows_stage, &pool_stage, p);
        for (si, steps) in COUNTS.iter().enumerate() {
            let noise: Vec<f64> =
                noise_words(100 + k as u64 * 7 + *steps as u64, len).iter().map(|w| *w as f64 / (1u64 << 24) as f64).collect();
            let (lats, _) = float_loop(&dit, &tables, si, &noise, &rows, &pooled, &mut cal);
            vae.decode(&Fm { c, h: side, w: side, d: lats.last().unwrap().clone() }, &mut vcal);
        }
    }
    cal.unify(&["cond", "te_out", "pe_out"]);

    // ---- the lowering ----
    let spec = DitStageSpec {
        n_txt: L,
        q_lat: Q_LAT,
        counts: COUNTS.to_vec(),
        shift,
        n_train,
        text_unit: rows_stage.unit,
        pooled_unit: pool_stage.unit,
    };
    let stage = lower_dit_stage(&dit, &cal, &spec).expect("the denoiser stage lowers");
    let chain = lower_vae(&vae, &vcal, side, stage.scales.latent).expect("the VAE lowers");
    let pipe = assemble_sd3_pipeline(
        TextStagesV1 {
            rows: (rows_stage.program, rows_stage.params),
            pool: (pool_stage.program, pool_stage.params),
            bos: BOS,
            eos: EOS,
            pad: PAD,
            to_len: L as u32,
        },
        &stage,
        &chain,
    )
    .expect("the pipeline assembles and validates");
    eprintln!(
        "pipeline: {} stages, {} programs, params {:?} (elements, bytes) per program",
        pipe.pipeline.stages.len(),
        pipe.programs.len(),
        pipe.params.iter().map(|p| p.tensors.values().map(|t| t.data.len()).sum::<usize>()).collect::<Vec<_>>()
    );

    // ---- fidelity on the evaluation cases ----
    let mut worst_lat = 0f64;
    for (prompt, seed, steps) in evaluation_cases().cases {
        let si = COUNTS.iter().position(|c| *c == steps).unwrap();
        let words = noise_words(seed, len);
        let noise_f: Vec<f64> = words.iter().map(|w| *w as f64 / (1u64 << 24) as f64).collect();
        let (rows, pooled) = text_floats_from(&pipe, &prompt, &spec);
        let (lats_f, _vels_f) = float_loop(&dit, &tables, si, &noise_f, &rows, &pooled, &mut Calib::new());
        let img_f = vae.decode(&Fm { c, h: side, w: side, d: lats_f.last().unwrap().clone() }, &mut Calib::new());

        let job = PipelineJob { prompt: prompt.clone(), steps, scalars: vec![steps as i64], ..Default::default() };
        let run = run_pipeline(&pipe.pipeline, &pipe.programs, &pipe, &Noise(words), &job).expect("the integer pipeline runs");
        let denoise = &run.stages[2];
        for (i, st) in denoise.steps.iter().enumerate() {
            let got: Vec<f64> = st.output.data.iter().map(|v| *v as f64 * stage.scales.latent).collect();
            let amax = lats_f[i].iter().fold(0f64, |m, v| m.max(v.abs()));
            let worst = got.iter().zip(&lats_f[i]).fold(0f64, |m, (g, w)| m.max((g - w).abs()));
            eprintln!(
                "case seed {seed} steps {steps} step {i}: latent worst |int - float| {worst:.4} ({:.2}% of amax {amax:.3})",
                100.0 * worst / amax
            );
            worst_lat = worst_lat.max(worst / amax);
        }
        // The image: u8 HWC against the float image's `round(clamp(y/2 + 1/2) · 255)`.
        let hw = img_f.h;
        let want: Vec<f64> = (0..hw * hw * 3)
            .map(|k| {
                let (h, w, ch) = (k / (hw * 3), (k / 3) % hw, k % 3);
                ((img_f.d[(ch * hw + h) * hw + w] / 2.0 + 0.5).clamp(0.0, 1.0) * 255.0).round()
            })
            .collect();
        let max_err = run.output.data.iter().zip(&want).fold(0f64, |m, (g, w)| m.max((*g as f64 - w).abs()));
        eprintln!("case seed {seed} steps {steps}: image PSNR {:.2} dB, max error {max_err} of 255", psnr(&run.output.data, &want));
    }
    // A bring-up bound only: the fidelity thresholds are measured and frozen by the record.
    assert!(worst_lat < 0.35, "the integer latent trajectory tracks the float one (bring-up), worst {worst_lat}");
}

/// The text rows and pooled vector the integer pipeline hands the denoiser, as floats (stages 0 and 1 of a run).
fn text_floats_from(
    pipe: &misaka_palw_tir_lower::diffusion::pipeline::Sd3Pipeline,
    prompt: &[u32],
    spec: &DitStageSpec,
) -> (Vec<f64>, Vec<f64>) {
    let job = PipelineJob { prompt: prompt.to_vec(), steps: 2, scalars: vec![2], ..Default::default() };
    // Run only the two text programs, as the pipeline does.
    let run = |program: usize, toks: Vec<u32>| {
        let interp = InterpreterV2::new(&pipe.programs[program]).unwrap();
        interp.run(&pipe.params[program], &MapInputs::default(), &toks).unwrap()
    };
    let _ = &job;
    let rows: Vec<f64> =
        run(0, padded(prompt)).iter().flat_map(|s| s.output.data.iter().map(|v| *v as f64 * spec.text_unit)).collect();
    let mut unpadded = vec![BOS];
    unpadded.extend_from_slice(prompt);
    unpadded.push(EOS);
    let pooled: Vec<f64> = run(1, unpadded).last().unwrap().output.data.iter().map(|v| *v as f64 * spec.pooled_unit).collect();
    (rows, pooled)
}

/// Write the diffusers reference requests for the evaluation cases (the integer run's noise and sigma table).
#[test]
#[ignore = "writes requests/ for tools/gen_diffusers_sd3_fixture.py reference"]
fn write_requests() {
    let Some(dir) = fixture() else { return };
    let sch = json(&dir.join("scheduler/scheduler_config.json"));
    let (shift, n_train) = (sch["shift"].as_f64().unwrap(), sch["num_train_timesteps"].as_f64().unwrap());
    let tables = SamplerTables::new(&COUNTS, shift, n_train);
    std::fs::create_dir_all(dir.join("requests")).unwrap();
    std::fs::create_dir_all(dir.join("reference")).unwrap();
    for (k, (prompt, seed, steps)) in evaluation_cases().cases.into_iter().enumerate() {
        let si = COUNTS.iter().position(|c| *c == steps).unwrap();
        let noise: Vec<f64> = noise_words(seed, 4 * 8 * 8).iter().map(|w| *w as f64 / (1u64 << 24) as f64).collect();
        let req = serde_json::json!({"ids": padded(&prompt), "steps": steps, "noise": noise, "sigmas": tables.sigmas[si]});
        std::fs::write(dir.join(format!("requests/case_{k}.json")), req.to_string()).unwrap();
    }
}
