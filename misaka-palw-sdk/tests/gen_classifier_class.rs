//! **A sequence classifier / reranker / reward model as a registered class** (`OUTPUT_CLASSIFY_V1`): the frontend lowers
//! `…ForSequenceClassification` to a program whose `Final` output is one row of logits, `[1, labels]` — the shape an
//! `EmbeddingI32` header declares. This file takes the fixtures `misaka-palw-tir-lower/tests/seqcls.rs` holds to Hugging Face's
//! logits and declares each as an **Embedding-profile** generative class, then runs the gate a node runs on a registration
//! offline (`gen_class_admission_offline_v1`).
//!
//! What this establishes and what it does not:
//!
//! * It establishes that the existing Embedding profile can *carry* a classifier — the row's shape and dtype, the pipeline's
//!   normal form, the layouts and the output header all pass the chain's registration gate with no kernel or profile code change.
//! * It does **not** decide that the profile *means* a classifier. An `EmbeddingI32` row of unnormalised logits is a vector
//!   to a verifier; whether a dedicated job profile (`labels`, a label map) is wanted is the Lead's decision, and the lifecycle
//!   records the question as `KERNEL_EXTENSION_REQUIRED` with the reason "task profile" (`census::onboarding::TASK_PROFILE_REASON`).
//!
//! No tokenizer ships with a fixture, so the class commits to a placeholder tokenizer id (it is a hash the chain does not read here).

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_hashes::Hash64;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_sdk::gen_class::*;
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::embedding::embedding_row_v2;
use misaka_palw_tir_lower::encoder::{self, EncoderOutput};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::hf_schema::{read_model, ReadOptions};
use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Padded, Pooling};
use misaka_palw_tir_lower::lower::{materialise, IntParams, LowerOpts};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf-cls").join(name)
}

struct Weights(Vec<IntParams>);
impl tir::pipeline::PipelineParams for Weights {
    fn params(&self, program: u16) -> &dyn tir::ParamSource {
        &self.0[program as usize]
    }
}

/// A classifier's pieces: its row program, one-stage pipeline, weights, `max_prompt_tokens` and label count.
struct Pieces {
    program: tir::program_v2::TirProgramV2,
    pipeline: tir::pipeline::TirPipelineV1,
    params: IntParams,
    labels: u32,
    max_prompt_tokens: u32,
    pooling: u8,
}

fn read(name: &str) -> (misaka_palw_tir_lower::spec::ModelSpec, PathBuf) {
    let dir = fixture_dir(name);
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).expect("config")).expect("json");
    (read_model(&cfg, None, &ReadOptions::default()).expect("the adapter reads the configuration").spec, dir)
}

/// An encoder classifier (BERT lineage): the same lowering `tests/seqcls.rs` holds to HF's logits.
fn encoder_pieces(name: &str, pad: u32) -> Pieces {
    let (spec, dir) = read(name);
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let lmax = 12u32;
    let cfg = BidirCfg { lmax, pooling: Pooling::Cls, normalize: false };
    let (cls, sep) = (2usize, 3usize);
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate() {
        let n = 1 + (i % (lmax as usize - 2));
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad as usize);
        bidir::float_forward(&hl, &spec, &cfg, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir(&hl, &spec, &cfg).expect("lower");
    let loader = Resident(Arc::new(params_f));
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &|_: usize, _: usize| {}).expect("materialise");
    let program = embedding_row_v2(encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).expect("v2")).expect("a [1, labels] row");
    let params = encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let pipeline = encoder::bidir_pipeline(vec![cls as u32], vec![sep as u32], pad, lmax);
    tir::pipeline::validate_pipeline(&pipeline, std::slice::from_ref(&program)).expect("pipeline normal form");
    let labels = match spec.output {
        misaka_palw_tir_lower::spec::OutputSpec::Classify { labels, .. } => labels as u32,
        ref o => panic!("{name}: not a classifier: {o:?}"),
    };
    Pieces { program, pipeline, params, labels, max_prompt_tokens: lmax - 2, pooling: PALW_GEN_POOLING_CLS_V1 }
}

/// A decoder classifier (the last token's hidden state through `score`).
fn decoder_pieces(name: &str) -> Pieces {
    let (spec, dir) = read(name);
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("hl");
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("bind");
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &LowerOpts { max_window: Some(64), ..LowerOpts::default() }).expect("lower");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &binding, &ck).expect("params");
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&hl, &loader, &fidelity::random_sequences(hl.vocab, 6, 12, 7), &quiet).expect("calibrate");
    let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let program = embedding_row_v2(encoder::causal_v2(&lw, EncoderOutput::Final).expect("v2")).expect("a [1, labels] row");
    let pipeline = encoder::causal_pipeline(vec![], vec![], None, 64);
    tir::pipeline::validate_pipeline(&pipeline, std::slice::from_ref(&program)).expect("pipeline normal form");
    let labels = match spec.output {
        misaka_palw_tir_lower::spec::OutputSpec::Classify { labels, .. } => labels as u32,
        ref o => panic!("{name}: not a classifier: {o:?}"),
    };
    Pieces { program, pipeline, params: mat.params, labels, max_prompt_tokens: 64, pooling: PALW_GEN_POOLING_LAST_V1 }
}

/// Declare `p` as an Embedding-profile class and return the registration gate's verdict.
fn declare(p: &Pieces) -> Result<GenDeclaredV1, String> {
    let spec = GenClassSpecV1 {
        profile: PalwGenProfileV1::Embedding,
        pipeline: p.pipeline.clone(),
        programs: vec![p.program.clone()],
        tokenizer_id: Hash64::from_bytes([0xC1; 64]),
        // Unnormalised logits: q = 0 (a calibrated unit, provenance only), `normalised` false.
        output: OutputSpecV1::embedding_i32(1, p.labels, 0, false),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: p.max_prompt_tokens,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 { pooling: p.pooling, dims: vec![p.labels] }),
        },
    };
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    gen_declare_layout_v1(
        &params,
        bundle,
        &spec,
        &Weights(vec![p.params.clone()]),
        &GenLayoutChoiceV1 { tile_len: 16, output_tile: None, h_chunk: 16, checkpoint_interval: 1 },
    )
}

fn assert_gate_passes(name: &str, p: &Pieces) {
    let d = declare(p).unwrap_or_else(|e| panic!("{name}: declare: {e}"));
    let admitted = d.admission.as_ref().unwrap_or_else(|why| panic!("{name}: the registration gate refuses the class: {why}"));
    assert_eq!(admitted.report.profile, PalwGenProfileV1::Embedding, "{name}");
    assert_eq!(d.class.output, OutputSpecV1::embedding_i32(1, p.labels, 0, false), "{name}: the header is the program's row");
    assert!(!admitted.report.draws_randomness, "{name}: a classifier draws none");
    assert!(d.row.is_some(), "{name}: the class derives a registry row");
    // The kernel route of the pipeline (K2-TIR-v3's pipeline family): shipped inactive, and — armed hypothetically — a bounded
    // check, not a kernel extension. The same statement `census` makes of a vision-chat pipeline.
    let k = misaka_palw_sdk::preflight::kernel::pipeline_route_of(&p.pipeline, std::slice::from_ref(&p.program), 0);
    assert_eq!((k.shipped.as_str(), k.hypothetical.as_str()), ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT"), "{name}: {k:?}");
    eprintln!(
        "{name}: classifier class {} ({} label(s), pooling tag {}): canonical job {} step leaves, widest {}; kernel {} {}",
        d.class_id, p.labels, p.pooling, admitted.entry.canonical_step_leaf_count, admitted.entry.max_step_leaf_count, k.kernel, k.hypothetical
    );
}

#[test]
fn an_encoder_classifier_and_a_cross_encoder_reranker_pass_the_embedding_registration_gate() {
    assert_gate_passes("bert_cls", &encoder_pieces("bert_cls", 0));
    assert_gate_passes("bert_rerank", &encoder_pieces("bert_rerank", 0));
}

#[test]
fn a_decoder_classifier_a_reranker_and_a_reward_model_pass_the_embedding_registration_gate() {
    assert_gate_passes("llama_cls", &decoder_pieces("llama_cls"));
    assert_gate_passes("llama_reward", &decoder_pieces("llama_reward"));
    assert_gate_passes("qwen3_rerank", &decoder_pieces("qwen3_rerank"));
}
