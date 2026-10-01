//! **RFC-0003 §6 (activation step 6): the reduced SD3 / MMDiT fixture, lowered to a pipeline and held to the float run.**
//!
//! The fixture is generated once by `tools/gen_diffusers_sd3_fixture.py` (diffusers 0.40, tiny seeded random configs,
//! `HF_HUB_OFFLINE=1`; see that file's header) into `tests/fixtures/hf-diff/sd3_tiny/`. A checkout without it skips
//! these tests with a line saying so — the fixture is a generated artifact, like the other `hf-*` ones. The lowering
//! itself is `diffusion::fixture::build_sd3_tiny` (shared with the SDK's class and court tests).
//!
//! What runs here, on the evaluation cases (prompts and seeds the calibration did not see):
//!
//! 1. **the float reference** — the Rust `f64` denoiser and VAE run on the fixture's own weights and, where the python
//!    reference outputs exist (`reference/case_*.json`), are compared with diffusers' velocity per step and image (the
//!    float reference is held to diffusers before anything is held to it);
//! 2. **fidelity** — the integer pipeline against the Rust float pipeline, step by step, on the SAME noise, and
//!    against diffusers when its reference is present. Thresholds are measured and printed here and frozen by the
//!    record, not written down before the first run.
//!
//! To produce the diffusers reference for the evaluation cases: `cargo test --test diffusers_sd3 -- --ignored write_requests`
//! writes `requests/case_*.json` (the integer run's noise and sigma table); then for each, from this crate directory,
//! `python tools/gen_diffusers_sd3_fixture.py reference requests/case_K.json reference/case_K.json`.

use misaka_palw_tir::pipeline::{PipelineJob, RandomSource, run_pipeline};
use misaka_palw_tir::program_v2::RandomDist;
use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_lower::diffusion::calib::Calib;
use misaka_palw_tir_lower::diffusion::fixture::*;
use misaka_palw_tir_lower::diffusion::sampler::SamplerTables;
use misaka_palw_tir_lower::diffusion::vae_float::Fm;

struct Noise(Vec<i128>);

impl RandomSource for Noise {
    fn random(&self, _domain: u16, dist: RandomDist, _step: u32, shape: &[u32]) -> Option<Tensor> {
        assert_eq!(dist, RandomDist::Normal);
        let n: usize = shape.iter().map(|d| *d as usize).product();
        assert_eq!(n, self.0.len());
        Tensor::new(DType::I32, shape.iter().map(|d| *d as usize).collect(), self.0.clone()).ok()
    }
}

fn psnr(a: &[i128], b: &[f64]) -> f64 {
    let mse = a.iter().zip(b).map(|(x, y)| (*x as f64 - y).powi(2)).sum::<f64>() / a.len() as f64;
    if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

/// `(prompt ids, noise seed, steps)`.
fn evaluation_cases() -> Vec<(Vec<u32>, u64, u32)> {
    vec![(vec![5, 9, 13], 1, 4), (vec![2, 7], 2, 2), (vec![20, 21, 22, 23], 3, 4), (vec![11], 4, 2)]
}

#[test]
fn the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline() {
    let Some(dir) = fixture() else { return };
    let fx = build_sd3_tiny(&dir).expect("the fixture lowers");
    let (c, side) = (fx.cfg.in_channels, fx.cfg.sample_size);
    let len = c * side * side;
    let pipe = &fx.pipeline;
    eprintln!(
        "pipeline: {} stages, {} programs, params {:?} (elements) per program",
        pipe.pipeline.stages.len(),
        pipe.programs.len(),
        pipe.params.iter().map(|p| p.tensors.values().map(|t| t.data.len()).sum::<usize>()).collect::<Vec<_>>()
    );

    // The programs: blocks, nodes (the largest block is held to 512 by normal form), commit points, params.
    for (k, prog) in pipe.programs.iter().enumerate() {
        let nodes: usize = prog.blocks.iter().map(|b| b.nodes.len()).sum();
        let widest = prog.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        let commits: usize = prog.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum();
        eprintln!(
            "  program {k:>2} {:<12} {:>2} blocks, {nodes:>5} nodes (widest block {widest:>3}), {commits:>4} commit points, {:>7} param elements",
            pipe.pipeline.stages[k].name,
            prog.blocks.len(),
            pipe.params[k].tensors.values().map(|t| t.data.len()).sum::<usize>()
        );
    }

    let mut worst_lat = 0f64;
    for (k, (prompt, seed, steps)) in evaluation_cases().into_iter().enumerate() {
        let si = COUNTS.iter().position(|c| *c == steps).unwrap();
        let words = noise_words(seed, len);
        let noise_f: Vec<f64> = words.iter().map(|w| *w as f64 / (1u64 << 24) as f64).collect();
        // The text the integer pipeline hands the denoiser, as floats (the float run is fed the same rows).
        let (rows, pooled) = integer_text_floats(&fx, &prompt);
        let (lats_f, vels_f) = float_loop(&fx.dit, &fx.tables, si, &noise_f, &rows, &pooled, &mut Calib::new());
        let img_f = fx.vae.decode(&Fm { c, h: side, w: side, d: lats_f.last().unwrap().clone() }, &mut Calib::new());

        // The steps scalar is the step count's position among the class's offered counts.
        let job = PipelineJob { prompt: prompt.clone(), steps, scalars: vec![si as i64], ..Default::default() };
        let run = run_pipeline(&pipe.pipeline, &pipe.programs, pipe, &Noise(words), &job).expect("the integer pipeline runs");
        let denoise = &run.stages[2];
        for (i, st) in denoise.steps.iter().enumerate() {
            let got: Vec<f64> = st.output.data.iter().map(|v| *v as f64 * fx.stage.scales.latent).collect();
            let amax = lats_f[i].iter().fold(0f64, |m, v| m.max(v.abs()));
            let worst = got.iter().zip(&lats_f[i]).fold(0f64, |m, (g, w)| m.max((g - w).abs()));
            eprintln!(
                "case {k} (seed {seed}, {steps} steps) step {i}: latent worst |int - float| {worst:.4} ({:.2}% of amax {amax:.3})",
                100.0 * worst / amax
            );
            worst_lat = worst_lat.max(worst / amax);
        }
        // The image: u8 HWC against the float image's `round(clamp(y/2 + 1/2) · 255)`.
        let hw = img_f.h;
        let want: Vec<f64> = (0..hw * hw * 3)
            .map(|q| {
                let (h, w, ch) = (q / (hw * 3), (q / 3) % hw, q % 3);
                ((img_f.d[(ch * hw + h) * hw + w] / 2.0 + 0.5).clamp(0.0, 1.0) * 255.0).round()
            })
            .collect();
        let max_err = run.output.data.iter().zip(&want).fold(0f64, |m, (g, w)| m.max((*g as f64 - w).abs()));
        eprintln!("case {k}: image PSNR {:.2} dB, max error {max_err} of 255", psnr(&run.output.data, &want));

        // Against diffusers itself, when its reference for this case exists.
        let refp = dir.join(format!("reference/case_{k}.json"));
        if let Ok(bytes) = std::fs::read(&refp) {
            let r: serde_json::Value = serde_json::from_slice(&bytes).expect("reference json");
            let detail = r["steps_detail"].as_array().expect("steps_detail");
            let flat = |key: &str| -> Vec<f64> { r[key].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() };
            // The float denoiser is held to diffusers on diffusers' OWN text tensors (the integer text stages' rows differ
            // from the float CLIP's by their own quantisation, which is the pipeline's fidelity, not the denoiser's).
            let (pe_ref, pooled_ref) = (flat("prompt_embeds"), flat("pooled"));
            let (_, vels_own) = float_loop(&fx.dit, &fx.tables, si, &noise_f, &pe_ref, &pooled_ref, &mut Calib::new());
            for (i, d) in detail.iter().enumerate() {
                let f = |key: &str| -> Vec<f64> { d[key].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() };
                let (v_ref, l_ref) = (f("velocity"), f("latent"));
                let amax_v = v_ref.iter().fold(0f64, |m, v| m.max(v.abs()));
                let dv = vels_own[i].iter().zip(&v_ref).fold(0f64, |m, (a, b)| m.max((a - b).abs()));
                let got: Vec<f64> = denoise.steps[i].output.data.iter().map(|v| *v as f64 * fx.stage.scales.latent).collect();
                let amax_l = l_ref.iter().fold(0f64, |m, v| m.max(v.abs()));
                let dl = got.iter().zip(&l_ref).fold(0f64, |m, (a, b)| m.max((a - b).abs()));
                eprintln!(
                    "  diffusers step {i}: Rust float velocity (same text) worst {dv:.2e} ({:.2e} of amax); integer pipeline latent worst {dl:.4} ({:.2}% of amax)",
                    dv / amax_v,
                    100.0 * dl / amax_l
                );
                assert!(dv / amax_v < 1e-4, "the Rust float denoiser is diffusers' (case {k} step {i}): relative {}", dv / amax_v);
            }
            let img_ref: Vec<f64> = r["image_hwc_u8"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            let worst = run.output.data.iter().zip(&img_ref).fold(0f64, |m, (g, w)| m.max((*g as f64 - w).abs()));
            eprintln!("  diffusers image: integer PSNR {:.2} dB, max error {worst} of 255", psnr(&run.output.data, &img_ref));
        }
    }
    // A bring-up bound only: the fidelity thresholds are measured and frozen by the record.
    assert!(worst_lat < 0.35, "the integer latent trajectory tracks the float one (bring-up), worst {worst_lat}");
}

/// The text rows and pooled vector the integer pipeline's stages 0 and 1 hand the denoiser, as floats.
fn integer_text_floats(fx: &Sd3Fixture, prompt: &[u32]) -> (Vec<f64>, Vec<f64>) {
    use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
    let pipe = &fx.pipeline;
    let run = |program: usize, toks: Vec<u32>| {
        let interp = InterpreterV2::new(&pipe.programs[program]).unwrap();
        interp.run(&pipe.params[program], &MapInputs::default(), &toks).unwrap()
    };
    let rows: Vec<f64> = run(0, padded(prompt)).iter().flat_map(|s| s.output.data.iter().map(|v| *v as f64 * fx.rows_unit)).collect();
    let mut unpadded = vec![BOS];
    unpadded.extend_from_slice(prompt);
    unpadded.push(EOS);
    let pooled: Vec<f64> = run(1, unpadded).last().unwrap().output.data.iter().map(|v| *v as f64 * fx.pooled_unit).collect();
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
    for (k, (prompt, seed, steps)) in evaluation_cases().into_iter().enumerate() {
        let si = COUNTS.iter().position(|c| *c == steps).unwrap();
        let noise: Vec<f64> = noise_words(seed, 4 * 8 * 8).iter().map(|w| *w as f64 / (1u64 << 24) as f64).collect();
        let req = serde_json::json!({"ids": padded(&prompt), "steps": steps, "noise": noise, "sigmas": tables.sigmas[si]});
        std::fs::write(dir.join(format!("requests/case_{k}.json")), req.to_string()).unwrap();
    }
}
