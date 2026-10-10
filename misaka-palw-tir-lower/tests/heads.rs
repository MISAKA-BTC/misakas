//! **NLU task heads over a bidirectional encoder** (`OUTPUT_TOKEN_LOGITS_V1`, HFX 2026-10-08) against their Hugging Face fixtures
//! (`tools/gen_hf_heads_fixtures.py`, `tests/fixtures/hf-heads/`):
//!
//! * token classification (`…ForTokenClassification`: `classifier` on every row) and extractive question answering
//!   (`…ForQuestionAnswering`: `qa_outputs` on every row, start then end) for BERT, RoBERTa / XLM-R and DistilBERT;
//! * the adapter `<family>-tokcls` / `<family>-qa` reads the configuration; every checkpoint tensor is bound and read; the float
//!   reference equals transformers' logits on the real rows; the integer program (calibrated, materialised, run as its version-2
//!   program and through its one-stage pipeline — equal byte strings) is held to transformers' logits; the version-1 view passes
//!   the three implementations and the court; the program and the pipeline are admitted.
//!
//! What this does NOT establish: that a class over these programs is a registrable TASK. The output is a `[L, labels]` tensor of
//! unnormalised logits; the task profile that gives it a canonical input, a label map and a decode rule is the Head profile's
//! (`docs/design/palw/tir/task-heads-profile-v1.md`), behind its own fence. A BERT-type pair input (QA's question ‖ context) is
//! computed with token-type row 0 everywhere — the fixture's `logits` are transformers' with `token_type_ids` all zero, and the
//! segment-aware logits (`logits_segments`) are recorded beside them for the pair-segment rule this lowering does not have yet.

mod common;

use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder;
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, BidirExtras, Padded, Pooling};
use misaka_palw_tir_lower::lower::materialise;
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::OutputSpec;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-heads").join(name)
}

/// (fixture, adapter, logits per token, pad id)
const TOKEN_HEADS: &[(&str, &str, usize, u32)] = &[
    ("bert_tokcls", "bert-tokcls", 5, 0),
    ("roberta_tokcls", "roberta-tokcls", 4, 1),
    ("distilbert_tokcls", "distilbert-tokcls", 3, 0),
    ("bert_qa", "bert-qa", 2, 0),
    ("roberta_qa", "roberta-qa", 2, 1),
    ("distilbert_qa", "distilbert-qa", 2, 0),
    // HFX 2026-10-10: ALBERT and DeBERTa-v2.
    ("albert_tokcls", "albert-tokcls", 4, 0),
    ("albert_qa", "albert-qa", 2, 0),
    ("deberta_v2_tokcls", "deberta-v2-tokcls", 4, 0),
    ("deberta_v2_qa", "deberta-v2-qa", 2, 0),
];

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

#[test]
fn every_token_head_is_read_by_its_adapter_and_binds_every_tensor() {
    for (name, adapter, labels, _) in TOKEN_HEADS {
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
            matches!(r.spec.output, OutputSpec::TokenLogits { labels: l, bias: true } if l == *labels),
            "{name}: {:?}",
            r.spec.output
        );
        let ids: Vec<&str> = r.spec.features().iter().map(|f| f.id.0).collect();
        assert!(ids.contains(&"OUTPUT_TOKEN_LOGITS_V1") && ids.contains(&"ENC_BIDIR_V1"), "{name}: {ids:?}");
        let hl = misaka_palw_tir_lower::hl::build_program(&r.spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = misaka_palw_tir_lower::hf_weights::bind(&r.spec, &hl).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ck = Checkpoint::open(&dir).unwrap();
        let rep = misaka_palw_tir_lower::weights::check_weights(&hl, &binding, &ck);
        assert!(rep.errors.is_empty(), "{name}: {:?}", rep.errors);
        assert!(rep.unused.is_empty(), "{name}: unread tensors {:?}", rep.unused);
        // The scope names the task the head computes.
        let scope = misaka_palw_tir_lower::model::scope::scope_of(&cfg, Some(&r.spec), None, &[]);
        let want = if name.ends_with("_qa") { "question-answering" } else { "token-classification" };
        assert_eq!(scope.task, want, "{name}");
    }
}

fn token_head_end_to_end(name: &str, pad: u32, float_tol: f64, int_tol: f64) {
    token_head_end_to_end_with(name, pad, float_tol, int_tol, false)
}

/// `segments`: the pair separator is the fixture's (`ENC_PAIR_SEGMENTS_V1`) and the reference is HF's `logits_segments` (explicit
/// `token_type_ids`: 1 after the first separator) wherever the fixture records them.
fn token_head_end_to_end_with(name: &str, pad: u32, float_tol: f64, int_tol: f64, segments: bool) {
    let dir = fixture_dir(name);
    let cfg_json: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let spec =
        misaka_palw_tir_lower::hf_schema::read_model(&cfg_json, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default())
            .expect("read")
            .spec;
    let labels = match spec.output {
        OutputSpec::TokenLogits { labels, .. } => labels,
        ref o => panic!("{name}: {o:?}"),
    };
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let lmax = 12u32;
    let cfg = BidirCfg { lmax, pooling: Pooling::Cls, normalize: false };
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).unwrap()).unwrap();
    let seqs: Vec<(Padded, Vec<f64>)> = v["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let ids = s["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
            let count = s["count"].as_u64().unwrap() as usize;
            let rows: Vec<f64> = s[if segments && s.get("logits_segments").is_some() { "logits_segments" } else { "logits" }]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()))
                .collect();
            assert_eq!(rows.len(), count * labels, "{name}: the fixture holds one row per real token");
            (Padded { ids, count }, rows)
        })
        .collect();
    // The separator of a pair input is the fixture's closing id (every sequence ends with it).
    let extras = BidirExtras { pair_sep: segments.then(|| seqs[0].0.ids[seqs[0].0.count - 1] as u32) };
    for (p, want) in &seqs {
        let got = bidir::float_forward_with(&hl, &spec, &cfg, &extras, &params_f, p, None).expect("float");
        assert_eq!(got.len(), want.len(), "{name}: [count, labels]");
        let scale = want.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(0.05);
        let w = got.iter().zip(want).map(|(a, b)| (a - b).abs() / scale).fold(0.0, f64::max);
        eprintln!("{name}: float reference vs HF logits: worst {w:.2e} of the largest logit");
        assert!(w < float_tol, "{name}: float vs HF {w}");
    }
    // Calibration on random templated sequences: the template's first and last ids around random bodies.
    let (cls, sep) = (seqs[0].0.ids[0], seqs[0].0.ids[seqs[0].0.count - 1]);
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate() {
        let n = 2 + (i % (lmax as usize - 2)) + 1;
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n - 2)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad as usize);
        bidir::float_forward_with(&hl, &spec, &cfg, &extras, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir_with(&hl, &spec, &cfg, &extras).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).expect("v2");
    // The output is the whole padded token axis: `[L, labels]`.
    let tir::program_v2::OutputDecl::Final { node } = p2.output else { panic!("{name}: a Final output") };
    let out_ty = &p2.blocks[p2.schedule.post as usize].nodes[node as usize].out;
    assert_eq!(out_ty.shape, vec![tir::Dim::Fixed(lmax), tir::Dim::Fixed(labels as u32)], "{name}: [L, labels]");
    let params2 = encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let pipe = encoder::bidir_pipeline(vec![cls as u32], vec![sep as u32], pad, lmax);
    tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    let spread = seqs.iter().flat_map(|(_, w)| w.iter()).fold(0.0f64, |m, x| m.max(x.abs()));
    for (p, want) in &seqs {
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs
            .constant
            .insert(0, tir::Tensor::new(tir::DType::Idx, vec![lmax as usize], p.ids.iter().map(|t| *t as i128).collect()).unwrap());
        inputs.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, p.count as i128).unwrap());
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let out = &run[0].output;
        let job =
            tir::pipeline::PipelineJob { prompt: p.ids[1..p.count - 1].iter().map(|t| *t as u32).collect(), ..Default::default() };
        let pr =
            tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &OneProgram(&params2), &NoRandom, &job).expect("pipeline");
        assert_eq!(pr.output.data, out.data, "{name}: the pipeline differs from the program");
        assert_eq!(out.data.len(), lmax as usize * labels, "{name}: [L, labels]");
        let got: Vec<f64> = out.data[..p.count * labels].iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let abs = got.iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        eprintln!(
            "{name}: integer vs HF logits ({} real of {lmax} rows): worst absolute error {abs:.4} (unit {:.2e})",
            p.count, mat.logits_scale
        );
        assert!(abs < int_tol * spread.max(0.5), "{name}: integer vs HF: {abs}");
        // The decode a task profile pins is a function of these rows; the arg-max per real token agrees with transformers'.
        for r in 0..p.count {
            let am = |v: &[f64]| (0..labels).fold(0usize, |b, j| if v[j] > v[b] { j } else { b });
            let (g, w) = (&got[r * labels..(r + 1) * labels], &want[r * labels..(r + 1) * labels]);
            let gap = {
                let mut s = w.to_vec();
                s.sort_by(|a, b| b.partial_cmp(a).unwrap());
                if labels > 1 { s[0] - s[1] } else { f64::INFINITY }
            };
            // A near tie in the reference is not a disagreement the integer program can be held to.
            if gap > 2.0 * abs {
                assert_eq!(am(g), am(w), "{name}: token {r}: the arg-max differs (reference gap {gap:.4})");
            }
        }
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
    tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &inputs, &tir::admit_v2::TirJobCeilingsV1::open_v1())
        .expect("tir_admit_pipeline_v1");
}

#[test]
fn an_encoder_token_classifier_and_a_span_qa_head_match_their_hf_logits_on_the_integer_program_and_the_pipeline() {
    for (name, _, _, pad) in TOKEN_HEADS {
        token_head_end_to_end(name, *pad, 1e-5, 0.08);
    }
}

/// **`ENC_PAIR_SEGMENTS_V1`**: BERT's span head over a pair input (`question ‖ sep ‖ context`), the segment ids computed IN the program
/// from the ids, matches transformers' logits run with explicit `token_type_ids` — the float reference, the calibrated integer program, the
/// one-stage pipeline (equal byte strings), the three implementations, the court and both admissions. The single-segment lowering
/// (`logits`, type row 0 everywhere) differs on the pair sequence: the test asserts the two references are not the same function.
#[test]
fn a_bert_span_head_with_pair_segments_matches_hf_logits_run_with_token_types() {
    token_head_end_to_end_with("bert_qa", 0, 1e-5, 0.08, true);
    // The two functions differ on the pair sequence (the fixture records both): segments are not a no-op.
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture_dir("bert_qa").join("outputs.json")).unwrap()).unwrap();
    let pair = v["sequences"].as_array().unwrap().iter().find(|s| s.get("logits_segments").is_some()).expect("a pair sequence");
    let flat = |k: &str| -> Vec<f64> { pair[k].as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap())).collect() };
    let worst = flat("logits").iter().zip(flat("logits_segments")).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    assert!(worst > 0.1, "the segment-aware logits differ from the single-segment ones by {worst}");
}

/// `pair_segment_types`: the first separator closes segment 0 and is in it; everything after is segment 1; no separator, no segment 1.
#[test]
fn pair_segment_types_start_after_the_first_separator() {
    assert_eq!(bidir::pair_segment_types(&[2, 40, 9, 3, 33, 21, 3, 0], Some(3)), vec![0, 0, 0, 0, 1, 1, 1, 1]);
    assert_eq!(bidir::pair_segment_types(&[2, 40, 9, 3, 0], None), vec![0; 5]);
    assert_eq!(bidir::pair_segment_types(&[2, 40, 9], Some(3)), vec![0; 3]);
}

// ───────────────────────────── masked language models (`OutputSpec::MaskedLm`) ─────────────────────────────

/// (fixture, built-in MLM adapter, pad id)
const MLM_HEADS: &[(&str, &str, u32)] = &[("bert_mlm", "bert-mlm", 0), ("roberta_mlm", "roberta-mlm", 1), ("distilbert_mlm", "distilbert-mlm", 0)];

fn mlm_read(name: &str, adapter: &str) -> (std::path::PathBuf, serde_json::Value, misaka_palw_tir_lower::hf_schema::ModelRead) {
    let dir = fixture_dir(name);
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    // The task picks the head's reading; the architecture alone never does.
    assert_eq!(misaka_palw_tir_lower::hf_schema::masked_lm_adapter_for(&cfg), Some(adapter), "{name}");
    let r = misaka_palw_tir_lower::hf_schema::read_model(
        &cfg,
        None,
        &misaka_palw_tir_lower::hf_schema::ReadOptions {
            adapter: misaka_palw_tir_lower::hf_schema::AdapterChoice::BuiltIn(adapter.to_string()),
        },
    )
    .unwrap_or_else(|f| panic!("{name}: {}", f.error));
    (dir, cfg, r)
}

#[test]
fn every_masked_lm_head_is_read_by_the_task_chosen_adapter_and_binds_every_tensor() {
    for (name, adapter, _) in MLM_HEADS {
        let (dir, cfg, r) = mlm_read(name, adapter);
        assert!(matches!(r.spec.output, OutputSpec::MaskedLm), "{name}: {:?}", r.spec.output);
        assert!(r.spec.head.transform.is_some() && r.spec.head.bias && !r.spec.head.tied, "{name}");
        let ids: Vec<&str> = r.spec.features().iter().map(|f| f.id.0).collect();
        assert!(ids.contains(&"ENC_BIDIR_V1") && ids.contains(&"HEAD_TRANSFORM_V1"), "{name}: {ids:?}");
        let hl = misaka_palw_tir_lower::hl::build_program(&r.spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = misaka_palw_tir_lower::hf_weights::bind(&r.spec, &hl).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ck = Checkpoint::open(&dir).unwrap();
        let rep = misaka_palw_tir_lower::weights::check_weights(&hl, &binding, &ck);
        assert!(rep.errors.is_empty(), "{name}: {:?}", rep.errors);
        assert!(rep.unused.is_empty(), "{name}: unread tensors {:?}", rep.unused);
        let scope = misaka_palw_tir_lower::model::scope::scope_of(&cfg, Some(&r.spec), None, &[]);
        assert_eq!(scope.task, "fill-mask", "{name}");
        // …and by architecture alone the same configuration is NOT the masked-LM reading (an encoder for embedding, or no adapter).
        let auto = misaka_palw_tir_lower::hf_schema::read_model(&cfg, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default());
        if let Ok(a) = auto {
            assert!(!matches!(a.spec.output, OutputSpec::MaskedLm), "{name}: Auto reads the head");
        }
    }
}

fn masked_lm_end_to_end(name: &str, adapter: &str, pad: u32, float_tol: f64, int_tol: f64) {
    let (dir, _cfg, r) = mlm_read(name, adapter);
    let spec = r.spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let lmax = 12u32;
    let vocab = spec.vocab_size;
    let cfg = BidirCfg { lmax, pooling: Pooling::Cls, normalize: false };
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).unwrap()).unwrap();
    let seqs: Vec<(Padded, usize, Vec<f64>)> = v["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let ids = s["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
            let count = s["count"].as_u64().unwrap() as usize;
            let pos = s["mask_pos"].as_u64().unwrap() as usize;
            let row: Vec<f64> = s["logits"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            assert_eq!(row.len(), vocab, "{name}: the fixture holds the masked row's vocabulary logits");
            (Padded { ids, count }, pos, row)
        })
        .collect();
    let extras = BidirExtras::default();
    for (p, pos, want) in &seqs {
        let got = bidir::float_forward_with(&hl, &spec, &cfg, &extras, &params_f, p, None).expect("float");
        assert_eq!(got.len(), p.count * vocab, "{name}: [count, vocab]");
        let row = &got[pos * vocab..(pos + 1) * vocab];
        let scale = want.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(0.05);
        let w = row.iter().zip(want).map(|(a, b)| (a - b).abs() / scale).fold(0.0, f64::max);
        eprintln!("{name}: float reference vs HF masked-row logits: worst {w:.2e} of the largest logit");
        assert!(w < float_tol, "{name}: float vs HF {w}");
    }
    let (cls, sep) = (seqs[0].0.ids[0], seqs[0].0.ids[seqs[0].0.count - 1]);
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in fidelity::random_sequences(vocab, 8, lmax as usize - 2, 11).into_iter().enumerate() {
        let n = 2 + (i % (lmax as usize - 2)) + 1;
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n - 2)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad as usize);
        bidir::float_forward_with(&hl, &spec, &cfg, &extras, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir_with(&hl, &spec, &cfg, &extras).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let p2 = encoder::bidir_v2(&lw, vocab as u32, lmax).expect("v2");
    let tir::program_v2::OutputDecl::Final { node } = p2.output else { panic!("{name}: a Final output") };
    let out_ty = &p2.blocks[p2.schedule.post as usize].nodes[node as usize].out;
    assert_eq!(out_ty.shape, vec![tir::Dim::Fixed(lmax), tir::Dim::Fixed(vocab as u32)], "{name}: [L, vocab]");
    let params2 = encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let pipe = encoder::bidir_pipeline(vec![cls as u32], vec![sep as u32], pad, lmax);
    tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).expect("pipeline normal form");
    let interp = tir::interp_v2::InterpreterV2::new(&p2).expect("interpreter v2");
    let spread = seqs.iter().flat_map(|(_, _, w)| w.iter()).fold(0.0f64, |m, x| m.max(x.abs()));
    for (p, pos, want) in &seqs {
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs
            .constant
            .insert(0, tir::Tensor::new(tir::DType::Idx, vec![lmax as usize], p.ids.iter().map(|t| *t as i128).collect()).unwrap());
        inputs.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, p.count as i128).unwrap());
        let run = interp.run_positions(&params2, &inputs, 1).expect("v2 run");
        let out = &run[0].output;
        let job =
            tir::pipeline::PipelineJob { prompt: p.ids[1..p.count - 1].iter().map(|t| *t as u32).collect(), ..Default::default() };
        let pr =
            tir::pipeline::run_pipeline(&pipe, std::slice::from_ref(&p2), &OneProgram(&params2), &NoRandom, &job).expect("pipeline");
        assert_eq!(pr.output.data, out.data, "{name}: the pipeline differs from the program");
        assert_eq!(out.data.len(), lmax as usize * vocab, "{name}: [L, vocab]");
        let got: Vec<f64> = out.data[pos * vocab..(pos + 1) * vocab].iter().map(|c| *c as f64 * mat.logits_scale).collect();
        let abs = got.iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        eprintln!("{name}: integer vs HF masked-row logits: worst absolute error {abs:.4} (unit {:.2e})", mat.logits_scale);
        assert!(abs < int_tol * spread.max(0.5), "{name}: integer vs HF: {abs}");
        // The decode a fill-mask user reads: the arg-max token of the masked row agrees unless the reference nearly ties.
        let am = |v: &[f64]| (0..vocab).fold(0usize, |b, j| if v[j] > v[b] { j } else { b });
        let gap = {
            let mut s = want.to_vec();
            s.sort_by(|a, b| b.partial_cmp(a).unwrap());
            s[0] - s[1]
        };
        if gap > 2.0 * abs {
            assert_eq!(am(&got), am(want), "{name}: the masked row's arg-max differs (reference gap {gap:.4})");
        }
    }
    {
        use misaka_palw_tir_lower::lower::IntTensor;
        let (p, _, _) = &seqs[0];
        let ids = IntTensor::idx(vec![lmax as usize], p.ids.iter().map(|t| *t as u32).collect());
        let count = IntTensor::idx(vec![], vec![p.count as u32]);
        let p6 = common::with_inputs(&lw.program, &mat.params, &[(bidir::IDS_PARAM, ids), (bidir::COUNT_PARAM, count)]);
        common::three_ways(&lw.program, &p6, &[vec![0]]).unwrap_or_else(|e| panic!("{name}: three implementations: {e}"));
        let c = common::court_coverage(&lw.program, &p6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("{name}: court: {e}"));
        assert!(c.commits > 0);
    }
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    tir::admit_v2::tir_admit_program_v2(&p2, &inputs).expect("tir_admit_v2");
    tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &inputs, &tir::admit_v2::TirJobCeilingsV1::open_v1())
        .expect("tir_admit_pipeline_v1");
}

#[test]
fn an_encoder_masked_lm_head_matches_its_hf_logits_on_the_integer_program_and_the_pipeline() {
    for (name, adapter, pad) in MLM_HEADS {
        masked_lm_end_to_end(name, adapter, *pad, 1e-5, 0.08);
    }
}
