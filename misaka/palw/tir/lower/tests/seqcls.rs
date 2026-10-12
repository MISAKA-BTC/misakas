//! **Sequence classifiers, rerankers and reward models** (`OUTPUT_CLASSIFY_V1`, `…ForSequenceClassification`) against their
//! Hugging Face fixtures (`tools/gen_hf_seqcls_fixtures.py`, `tests/fixtures/hf-cls/`).
//!
//! * the decoder families (Llama, Qwen2/3, Mistral, Gemma/2, Phi-3, Mixtral, Qwen3-MoE, OLMo-2, GPT-2, OPT): the adapter `<family>-seqcls`
//!   reads the configuration into the base model's layers plus a classification head; every checkpoint tensor is bound and read
//!   (the head is `score`); the float reference equals transformers' logits; the integer program is held to the float reference.
//! * `num_labels` is the length of `id2label`, and a reward model (one label) is the same class with one logit.

mod common;

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
            matches!(r.spec.output, OutputSpec::Classify { labels: l, bias: false, pre: None, .. } if l == *labels),
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

// ───────────────────────────── encoders: BERT, RoBERTa / XLM-R, DistilBERT ─────────────────────────────

const ENCODERS: &[(&str, &str, usize, bool, u32)] = &[
    // (fixture, adapter, labels, has a dense + activation before the classifier, pad id)
    ("bert_cls", "bert-seqcls", 3, true, 0),
    ("bert_rerank", "bert-seqcls", 1, true, 0),
    ("roberta_cls", "roberta-seqcls", 2, true, 1),
    ("xlmr_rerank", "roberta-seqcls", 1, true, 1),
    ("distilbert_cls", "distilbert-seqcls", 2, true, 0),
    // HFX 2026-10-10: ALBERT's pooler, DeBERTa-v2's ContextPooler, CamemBERT under RoBERTa's head.
    ("albert_cls", "albert-seqcls", 3, true, 0),
    ("deberta_v2_cls", "deberta-v2-seqcls", 3, true, 0),
    ("camembert_cls", "roberta-seqcls", 2, true, 1),
    // ModernBERT's head (dense, activation, norm) and its two poolings.
    ("modernbert_cls", "modernbert-seqcls", 3, true, 0),
    ("modernbert_cls_mean", "modernbert-seqcls", 2, true, 0),
    ("modernbert_cls_nobias", "modernbert-seqcls", 2, true, 0),
    ("modernbert_cls_mean_bias", "modernbert-seqcls", 3, true, 0),
];

#[test]
fn every_encoder_sequence_classifier_is_read_by_its_adapter_and_binds_every_tensor() {
    for (name, adapter, labels, pre, _) in ENCODERS {
        let dir = fixture_dir(name);
        let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
        let r = misaka_palw_tir_lower::hf_schema::read_model(&cfg, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default())
            .unwrap_or_else(|f| panic!("{name}: {}", f.error));
        assert!(
            matches!(&r.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id, .. } if id == adapter),
            "{name}: read by {:?}, not {adapter}",
            r.adapter
        );
        assert!(
            matches!(r.spec.output, OutputSpec::Classify { labels: l, pre: Some(_), .. } if l == *labels && *pre),
            "{name}: {:?}",
            r.spec.output
        );
        let ids: Vec<&str> = r.spec.features().iter().map(|f| f.id.0).collect();
        assert!(ids.contains(&"OUTPUT_CLASSIFY_V1") && ids.contains(&"ENC_BIDIR_V1"), "{name}: {ids:?}");
        let hl = misaka_palw_tir_lower::hl::build_program(&r.spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = misaka_palw_tir_lower::hf_weights::bind(&r.spec, &hl).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ck = Checkpoint::open(&dir).unwrap();
        let rep = misaka_palw_tir_lower::weights::check_weights(&hl, &binding, &ck);
        assert!(rep.errors.is_empty(), "{name}: {:?}", rep.errors);
        assert!(rep.unused.is_empty(), "{name}: unread tensors {:?}", rep.unused);
    }
}

fn encoder_end_to_end(name: &str, pad: u32, float_tol: f64, int_tol: f64) {
    use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Padded, Pooling};
    let dir = fixture_dir(name);
    let cfg_json: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let spec = misaka_palw_tir_lower::hf_schema::read_model(&cfg_json, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default()).expect("read").spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let lmax = 12u32;
    // The pooled row the head names: `[CLS]`, or the mean of the real rows (ModernBERT's `classifier_pooling = "mean"`).
    let pooling = if matches!(spec.output, OutputSpec::Classify { mean: true, .. }) { Pooling::Mean } else { Pooling::Cls };
    let cfg = BidirCfg { lmax, pooling, normalize: false };
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).unwrap()).unwrap();
    let seqs: Vec<(Padded, Vec<f64>)> = v["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let ids = s["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
            let count = s["count"].as_u64().unwrap() as usize;
            (Padded { ids, count }, s["logits"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect())
        })
        .collect();
    for (p, want) in &seqs {
        let got = bidir::float_forward(&hl, &spec, &cfg, &params_f, p, None).expect("float");
        let w = worst(&got, want);
        eprintln!("{name}: float reference vs HF logits: worst {w:.2e} of the largest logit");
        assert!(w < float_tol, "{name}: float vs HF {w}");
    }
    let (cls, sep) = (seqs[0].0.ids[0], seqs[0].0.ids[seqs[0].0.count - 1]);
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate() {
        let n = 2 + (i % (lmax as usize - 2)) + 1;
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n - 2)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad as usize);
        bidir::float_forward(&hl, &spec, &cfg, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir(&hl, &spec, &cfg).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).expect("v2");
    let params2 = encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let pipe = encoder::bidir_pipeline(vec![cls as u32], vec![sep as u32], pad, lmax);
    tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    let spread = seqs.iter().flat_map(|(_, w)| w.iter()).fold(0.0f64, |m, x| m.max(x.abs()));
    for (p, want) in &seqs {
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs.constant.insert(0, tir::Tensor::new(tir::DType::Idx, vec![lmax as usize], p.ids.iter().map(|t| *t as i128).collect()).unwrap());
        inputs.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, p.count as i128).unwrap());
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let out = &run[0].output;
        let job = tir::pipeline::PipelineJob { prompt: p.ids[1..p.count - 1].iter().map(|t| *t as u32).collect(), ..Default::default() };
        let pr = tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &OneProgram(&params2), &NoRandom, &job).expect("pipeline");
        assert_eq!(pr.output.data, out.data, "{name}: the pipeline differs from the program");
        let got: Vec<f64> = out.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
        assert_eq!(got.len(), want.len(), "{name}: one logit per label");
        // Absolute error, in logits: a reranker's one logit can sit near zero, where a relative error means nothing. The bound is a
        // fraction of the logit spread of the fixture (at least 0.5): the BERT lineage's post-LN rows quantise to ~1-3 % of their range.
        let abs = got.iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        eprintln!("{name}: integer vs HF logits ({} real of {lmax}): worst absolute error {abs:.4} (unit {:.2e})", p.count, mat.logits_scale);
        assert!(abs < int_tol * spread.max(0.5), "{name}: integer vs HF: {abs}");
    }
    // The three implementations and the court on the encoder's version-1 view.
    {
        use misaka_palw_tir_lower::lower::IntTensor;
        let (p, _) = &seqs[0];
        let ids = IntTensor::idx(vec![lmax as usize], p.ids.iter().map(|t| *t as u32).collect());
        let count = IntTensor::idx(vec![], vec![p.count as u32]);
        let p6 = common::with_inputs(&lw.program, &mat.params, &[(bidir::IDS_PARAM, ids), (bidir::COUNT_PARAM, count)]);
        common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("{name}: three implementations: {e}"));
        let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name}: court: {e}"));
        assert!(c.commits > 0);
    }
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    tir::admit_v2::tir_admit_program_v2(&p2, &inputs).expect("tir_admit_v2");
    tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &inputs, &tir::admit_v2::TirJobCeilingsV1::open_v1()).expect("tir_admit_pipeline_v1");
}

#[test]
fn an_encoder_sequence_classifier_matches_its_hf_logits_on_three_implementations_and_the_court() {
    for (name, _, _, _, pad) in ENCODERS {
        encoder_end_to_end(name, *pad, 1e-5, 0.08);
    }
}
