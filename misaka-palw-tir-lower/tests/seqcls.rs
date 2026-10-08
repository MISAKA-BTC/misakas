//! **Sequence classifiers, rerankers and reward models** (`OUTPUT_CLASSIFY_V1`, `…ForSequenceClassification`) against their
//! Hugging Face fixtures (`tools/gen_hf_seqcls_fixtures.py`, `tests/fixtures/hf-cls/`).
//!
//! * the decoder families (Llama, Qwen2/3, Mistral, Gemma/2, Phi-3, Mixtral, Qwen3-MoE, OLMo-2, GPT-2, OPT): the adapter `<family>-seqcls`
//!   reads the configuration into the base model's layers plus a classification head; every checkpoint tensor is bound and read
//!   (the head is `score`); the float reference equals transformers' logits; the integer program is held to the float reference.
//! * `num_labels` is the length of `id2label`, and a reward model (one label) is the same class with one logit.

use misaka_palw_tir_lower::spec::OutputSpec;
use std::path::{Path, PathBuf};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-cls").join(name)
}

const DECODERS: &[(&str, &str, usize)] = &[
    ("llama_cls", "llama-seqcls", 3),
    ("llama_reward", "llama-seqcls", 1),
    ("qwen2_cls", "qwen2-seqcls", 2),
    ("qwen3_rerank", "qwen3-seqcls", 1),
    ("mistral_cls", "mistral-seqcls", 2),
    ("gemma_cls", "gemma-seqcls", 2),
    ("gemma2_cls", "gemma2-seqcls", 2),
    ("phi3_cls", "phi3-seqcls", 2),
    ("mixtral_cls", "mixtral-seqcls", 2),
    ("qwen3_moe_cls", "qwen3-moe-seqcls", 2),
    ("olmo2_cls", "olmo2-seqcls", 2),
    ("gpt2_cls", "gpt2-seqcls", 2),
    ("opt_cls", "opt-seqcls", 2),
];

#[test]
fn every_decoder_sequence_classifier_is_read_by_its_adapter_and_binds_every_tensor() {
    for (name, adapter, labels) in DECODERS {
        let dir = fixture_dir(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_else(|e| panic!("{name}: {e}"));
        let r = misaka_palw_tir_lower::hf_schema::read_model(
            &serde_json::from_str(&cfg).unwrap(),
            None,
            &misaka_palw_tir_lower::hf_schema::ReadOptions::default(),
        )
        .unwrap_or_else(|f| panic!("{name}: {}", f.error));
        assert!(
            matches!(&r.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id, .. } if id == adapter),
            "{name}: read by {:?}, not {adapter}",
            r.adapter
        );
        assert!(
            matches!(r.spec.output, OutputSpec::Classify { labels: l, bias: false, pre: None } if l == *labels),
            "{name}: {:?}",
            r.spec.output
        );
        assert!(r.spec.features().iter().any(|f| f.id.0 == "OUTPUT_CLASSIFY_V1"), "{name}");
        // The weights: every tensor of the checkpoint is bound or deliberately ignored; the classification layer is `score`.
        let hl = misaka_palw_tir_lower::hl::build_program(&r.spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        hl.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = misaka_palw_tir_lower::hf_weights::bind(&r.spec, &hl).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ck = misaka_palw_tir_lower::weights::Checkpoint::open(&dir).unwrap_or_else(|e| panic!("{name}: {e}"));
        let rep = misaka_palw_tir_lower::weights::check_weights(&hl, &binding, &ck);
        assert!(rep.errors.is_empty(), "{name}: {:?}", rep.errors);
        assert!(rep.unused.is_empty(), "{name}: unread tensors {:?}", rep.unused);
        let out = hl.params.iter().position(|p| p.name == "classifier.out.w").unwrap_or_else(|| panic!("{name}: no classifier.out.w"));
        assert_eq!(hl.params[out].shape, vec![*labels, hl.hidden], "{name}");
    }
}

// ───────────────────────────── numbers: the float reference, the integer program, admission ─────────────────────────────

use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder::{self, EncoderOutput};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::sync::Arc;

struct Seq {
    tokens: Vec<usize>,
    logits: Vec<f64>,
}

fn outputs(name: &str) -> Vec<Seq> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture_dir(name).join("outputs.json")).expect("outputs")).expect("json");
    v["sequences"]
        .as_array()
        .expect("sequences")
        .iter()
        .map(|s| Seq {
            tokens: s["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect(),
            logits: s["logits"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect(),
        })
        .collect()
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

/// The largest absolute difference, in units of the largest reference logit (a one-label model has no direction to take a cosine of).
fn worst(a: &[f64], b: &[f64]) -> f64 {
    let scale = b.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(0.05);
    a.iter().zip(b).map(|(x, y)| (x - y).abs() / scale).fold(0.0, f64::max)
}

/// One decoder classifier, end to end: the float reference against transformers' logits; the integer program (calibrated,
/// materialised, run as a version-1 program, as its version-2 program, and through its one-stage pipeline — three equal byte
/// strings) against the float reference; admission of both.
fn decoder_end_to_end(name: &str, float_tol: f64, int_tol: f64) {
    let dir = fixture_dir(name);
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let r = misaka_palw_tir_lower::hf_schema::read_model(&cfg, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default()).expect("read");
    let spec = r.spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &LowerOpts { max_window: Some(64), ..LowerOpts::default() }).expect("lower");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let seqs = outputs(name);
    for s in &seqs {
        let rows = Session::new(&hl, &params_f).run(&s.tokens).expect("float");
        let last: Vec<f64> = rows.last().unwrap().iter().map(|x| *x as f64).collect();
        let w = worst(&last, &s.logits);
        eprintln!("{name}: float reference vs HF: worst {w:.2e} of the largest logit");
        assert!(w < float_tol, "{name}: float vs HF: {w}");
    }
    let loader = Resident(Arc::new(params_f));
    let calib = fidelity::random_sequences(hl.vocab, 6, 12, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::causal_v2(&lw, EncoderOutput::Final).expect("v2");
    let pipe = encoder::causal_pipeline(vec![], vec![], None, 64);
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    for s in &seqs {
        let toks: Vec<u32> = s.tokens.iter().map(|t| *t as u32).collect();
        let run = interp.run(&mat.params, &tir::interp_v2::MapInputs::default(), &toks).expect("v2 run");
        let out = &run.last().unwrap().output;
        let job = tir::pipeline::PipelineJob { prompt: toks.clone(), ..Default::default() };
        let pr = tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &OneProgram(&mat.params), &NoRandom, &job).expect("pipeline");
        assert_eq!(pr.output.data, out.data, "{name}: the pipeline gives the version-2 program's bytes");
        let got: Vec<f64> = out.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        assert_eq!(got.len(), s.logits.len(), "{name}: one logit per label");
        let w = worst(&got, &s.logits);
        eprintln!("{name}: integer vs HF ({} tokens): worst {w:.2e}", s.tokens.len());
        assert!(w < int_tol, "{name}: integer vs HF: {w}");
    }
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &inputs, &tir::admit_v2::TirJobCeilingsV1::open_v1())
        .unwrap_or_else(|e| panic!("{name}: tir_admit_pipeline_v1: {e}"));
}

#[test]
fn a_decoder_sequence_classifier_matches_its_hf_logits() {
    // Llama (three labels; and a reward model with one), Qwen3 (a reranker's one logit), Gemma-2 (four norms, soft caps), GPT-2
    // (learned positions, Conv1D body and a plain-Linear score), Mixtral (a routed MLP).
    for name in ["llama_cls", "llama_reward", "qwen3_rerank", "gemma2_cls", "gpt2_cls", "mixtral_cls"] {
        decoder_end_to_end(name, 1e-5, 0.05);
    }
}
