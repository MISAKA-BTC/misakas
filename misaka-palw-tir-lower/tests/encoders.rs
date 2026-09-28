//! **RFC-0003 Part II.3: encoders against their Hugging Face fixtures.** Each tiny fixture
//! (`tools/gen_hf_encoder_fixtures.py`) is lowered, checked through the float reference, calibrated,
//! materialised and run as an integer program: as the version-1 program, as its version-2 program on
//! `InterpreterV2`, and through its one-stage pipeline. The version-2 program and the pipeline must
//! give the version-1 program's bytes, and all three are held against the HF output, which is
//! fidelity, not validity. Admission (`tir_admit_v2`, `tir_admit_pipeline_v1`) must accept both.

use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder::{self, EncoderOutput};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::fidelity;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-enc").join(name)
}

struct Seq {
    tokens: Vec<usize>,
    embeds: Vec<f64>,
}

fn outputs(name: &str) -> Vec<Seq> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture_dir(name).join("outputs.json")).expect("outputs")).expect("json");
    v["sequences"]
        .as_array()
        .expect("sequences")
        .iter()
        .map(|s| Seq {
            tokens: s["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect(),
            embeds: s["embeds"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect(),
        })
        .collect()
}

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    d / (na * nb)
}

fn rel(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt();
    d / b.iter().map(|x| x * x).sum::<f64>().sqrt()
}

struct OneProgram<'a>(&'a dyn tir::ParamSource);
impl tir::pipeline::PipelineParams for OneProgram<'_> {
    fn params(&self, _: u16) -> &dyn tir::ParamSource {
        self.0
    }
}
struct NoRandom;
impl tir::pipeline::RandomSource for NoRandom {
    fn random(&self, _: u16, _: tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<tir::Tensor> {
        None
    }
}

/// CLIP's text tower with its projection: a causal scan over `bos ‖ prompt ‖ eos`, `Final`.
#[test]
fn clip_text_encoder_matches_its_hf_fixture() {
    let dir = fixture_dir("clip_text");
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    // An encoder never runs past its learned positions: its window is its context, so admission
    // counts the attention at H = 16, not at the history bound.
    let ctx = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare").spec.max_position_embeddings.unwrap();
    let prep = fidelity::prepare(&cfg, &LowerOpts { max_window: Some(ctx as u32), ..LowerOpts::default() }).expect("prepare");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let seqs = outputs("clip_text");
    let (bos, eos) = (62usize, 63usize);
    // 1. The float reference is the HF model: the last position's row is `text_embeds`.
    for s in &seqs {
        let rows = Session::new(&prep.hl, &params_f).run(&s.tokens).expect("float");
        let last: Vec<f64> = rows.last().unwrap().iter().map(|x| *x as f64).collect();
        let r = rel(&last, &s.embeds);
        eprintln!("clip_text float reference vs HF: rel {r:.2e}");
        assert!(r < 1e-5, "float reference vs HF text_embeds: rel {r}");
    }
    // 2. Calibrate on templated random prompts; materialise.
    let loader = Resident(Arc::new(params_f));
    let calib: Vec<Vec<usize>> = fidelity::random_sequences(prep.hl.vocab - 2, 6, 12, 7)
        .into_iter()
        .map(|p| std::iter::once(bos).chain(p).chain(std::iter::once(eos)).collect())
        .collect();
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let q = (-mat.logits_scale.log2()).round() as i32;
    assert_eq!(2f64.powi(-q), mat.logits_scale, "the output is in a power-of-two fixed point");
    // 3. The version-2 program and its pipeline.
    let p2 = encoder::causal_v2(&prep.lowered, EncoderOutput::Final).expect("v2");
    let max_trip = prep.spec.max_position_embeddings.unwrap() as u32;
    let pipe = encoder::causal_pipeline(vec![bos as u32], vec![eos as u32], None, max_trip);
    tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for s in &seqs {
        // Version 1.
        let v1 = fidelity::int_logits(&prep.lowered.program, &mat.params, &s.tokens, 1.0, &|_| {}).expect("v1");
        let v1_last: Vec<i128> = v1.last().unwrap().iter().map(|x| *x as i128).collect();
        // Version 2: the same bytes.
        let toks: Vec<u32> = s.tokens.iter().map(|t| *t as u32).collect();
        let run = interp.run(&mat.params, &tir::interp_v2::MapInputs::default(), &toks).expect("v2 run");
        assert_eq!(run.last().unwrap().output.data, v1_last, "version 2 differs from version 1");
        // The pipeline over the prompt alone: the same bytes.
        let job = tir::pipeline::PipelineJob { prompt: toks[1..toks.len() - 1].to_vec(), ..Default::default() };
        let pr = tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &OneProgram(&mat.params), &NoRandom, &job)
            .expect("pipeline run");
        assert_eq!(pr.output.data, v1_last, "the pipeline differs from the program");
        // Fidelity against HF.
        let got: Vec<f64> = v1_last.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let (c, r) = (cosine(&got, &s.embeds), rel(&got, &s.embeds));
        eprintln!("clip_text integer vs HF text_embeds ({} tokens): cosine {c:.6}, rel {r:.2e}, q {q}", s.tokens.len());
        assert!(c > 0.999 && r < 0.05, "cosine {c}, rel {r}");
    }
    // 4. Admission: the program and the pipeline, at the legacy court's ceilings.
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    let a = tir::admit_v2::tir_admit_program_v2(&p2, &inputs).expect("tir_admit_v2");
    let pa = tir::admit_v2::tir_admit_pipeline_v1(
        &pipe.encode(),
        &[p2.encode()],
        &inputs,
        &tir::admit_v2::TirJobCeilingsV1::open_v1(),
    )
    .expect("tir_admit_pipeline_v1");
    eprintln!(
        "clip_text admitted: {} cones; pipeline job cost {:?}, {} step leaves, cone work {}, output interval {:?}",
        a.view.cones.len(),
        pa.job_cost,
        pa.job_step_leaves,
        pa.cone_work,
        pa.output_interval
    );
    // The output is an `EmbeddingI32` in Q(q): its proven interval lies inside i32.
    assert!(pa.output_interval.lo >= i32::MIN as i128 && pa.output_interval.hi <= i32::MAX as i128);
}
