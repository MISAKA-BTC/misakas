//! **RFC-0003 activation step 7: the embedding profile as the smoke-test class, end to end** — a reduced
//! BERT / MiniLM-style encoder (tir-lower's tiny Hugging Face fixture: 2 layers, hidden 32, mean pooling,
//! L2-normalised) becomes a registered Embedding class, runs a job to a canonical `EmbeddingI32`, is
//! replayed by a seat, and is convicted at the court when its executor lies.
//!
//! ```text
//! HF fixture ──lower──▶ program + pipeline + integer weights      (misaka-palw-tir-lower: embedding)
//!            ──declare──▶ layouts, output header, offers, class id  (misaka-palw-sdk: gen_class)
//!            ──gate────▶ ClassRegisteredGenV1 admission, offline    (the registration gate a node runs)
//!            ──write───▶ a PALWTIR2 container; a worker holds it    (misaka-palw-base0: gen_worker)
//!            ──job─────▶ PalwGenJobV1 → the worker → output_root    (misaka-palw-base0: gen_tensor_worker)
//!            ──seat────▶ replay: the claim's execution root
//!            ──court───▶ a planted computation fault, a planted digest fault, a lane outside the proven
//!                        interval: each convicted through the close the consensus court runs
//! ```
//!
//! The fidelity of the integer class to the float model is held to the fixture's HF output (cosine); that
//! is a statement about the lowering, not about validity — the chain never reads it.

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PROMPT_MODE_USER};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::gen_tensor_worker::*;
use misaka_palw_base0::gen_worker::{
    GenCourtMoveV1, GenHeldClassV1, GenSeatJudgmentV1, gen_court_object_v1, gen_execution_first_divergence_v1,
};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_sdk::gen_class::*;
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::embedding::{EmbeddingLowerOptsV1, LoweredEmbeddingV1, lower_bidir_embedding_v1};
use misaka_palw_tir_lower::lower::IntParams;
use misaka_palw_tir_lower::lower::bidir::Pooling;
use std::path::{Path, PathBuf};

const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const LIMITS: tir::demand::DemandLimits = tir::demand::DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf-enc/bert")
}

/// The lowered program's weights as the pipeline's (one program).
struct Weights(Vec<IntParams>);
impl tir::pipeline::PipelineParams for Weights {
    fn params(&self, program: u16) -> &dyn tir::ParamSource {
        &self.0[program as usize]
    }
}

struct NoRandom;
impl tir::pipeline::RandomSource for NoRandom {
    fn random(&self, _: u16, _: tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<tir::Tensor> {
        None
    }
}

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    d / (na * nb)
}

/// The fixture's tokenizer ids (`[CLS]`, `[SEP]`, `[PAD]`) and its HF reference outputs: `(padded ids,
/// real count, mean-pooled normalised embedding)`.
fn hf_reference() -> (u32, u32, u32, Vec<(Vec<u32>, usize, Vec<f64>)>) {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture_dir().join("outputs.json")).expect("outputs")).unwrap();
    let seqs: Vec<(Vec<u32>, usize, Vec<f64>)> = v["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let ids = s["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect();
            let count = s["count"].as_u64().unwrap() as usize;
            let want = s["mean_normalized"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            (ids, count, want)
        })
        .collect();
    let (cls, sep) = (seqs[0].0[0], seqs[0].0[seqs[0].1 - 1]);
    (cls, sep, 0, seqs)
}

/// Everything the class is: the lowering, its declared class and its registered row.
struct Embedding {
    lowered: LoweredEmbeddingV1,
    weights: Weights,
    declared: GenDeclaredV1,
    row: PalwGenClassRecordV1,
    hf: Vec<(Vec<u32>, usize, Vec<f64>)>,
}

fn build() -> Embedding {
    let (cls, sep, pad, hf) = hf_reference();
    let lowered = lower_bidir_embedding_v1(
        &fixture_dir(),
        &EmbeddingLowerOptsV1 {
            pooling: Pooling::Mean,
            normalize: true,
            lmax: 12,
            pad,
            cls,
            sep,
            calibration_sequences: 8,
            calibration_seed: 11,
        },
    )
    .unwrap_or_else(|e| panic!("the lowering: {e}"));
    let weights = Weights(vec![lowered.params.clone()]);
    let q = lowered.q.expect("a normalised embedding is a power-of-two unit (Q30)");
    assert_eq!(q, 30, "a normalised embedding is Q30");
    let spec = GenClassSpecV1 {
        profile: PalwGenProfileV1::Embedding,
        pipeline: lowered.pipeline.clone(),
        programs: vec![lowered.program.clone()],
        tokenizer_id: Hash64::from_bytes([0xE5; 64]),
        output: OutputSpecV1::embedding_i32(1, lowered.dims, q, lowered.normalised),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: lowered.max_prompt_tokens,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 {
                pooling: PALW_GEN_POOLING_MEAN_V1,
                dims: vec![lowered.dims],
            }),
        },
    };
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let declared = gen_declare_layout_v1(
        &params,
        bundle,
        &spec,
        &weights,
        &GenLayoutChoiceV1 { tile_len: 16, output_tile: None, h_chunk: 16, checkpoint_interval: 1 },
    )
    .unwrap_or_else(|e| panic!("declare: {e}"));
    let row = declared.row.clone().expect("the class derives a registry row");
    Embedding { lowered, weights, declared, row, hf }
}

// ---------------------------------------------------------------------------------------------
// The class
// ---------------------------------------------------------------------------------------------

#[test]
fn the_lowered_encoder_is_the_float_model_to_a_cosine_and_the_class_passes_the_registration_gate() {
    let e = build();
    // 1. Fidelity to the HF output (the lowering's statement, not the chain's).
    let program = &e.lowered.program;
    for (ids, count, want) in &e.hf {
        let job = tir::pipeline::PipelineJob { prompt: ids[1..count - 1].to_vec(), ..Default::default() };
        let run = tir::pipeline::run_pipeline(&e.lowered.pipeline, std::slice::from_ref(program), &e.weights, &NoRandom, &job)
            .expect("the pipeline runs");
        assert_eq!(run.output.shape, vec![1, e.lowered.dims as usize], "an EmbeddingI32 row is [1, d]");
        let got: Vec<f64> = run.output.data.iter().map(|c| *c as f64 * e.lowered.logits_scale).collect();
        let c = cosine(&got, want);
        assert!(c > 0.999, "integer vs HF cosine {c}");
    }
    // 2. The declared class: its header is the program's output, its offers the profile's.
    let class = &e.declared.class;
    assert_eq!(class.profile, PalwGenProfileV1::Embedding as u8);
    assert_eq!(class.output, OutputSpecV1::embedding_i32(1, e.lowered.dims, 30, true));
    // 3. The gate: the pipeline admission a node runs on a registration, offline.
    let admitted = e.declared.admission.as_ref().unwrap_or_else(|why| panic!("the registration gate refuses the class: {why}"));
    assert_eq!(admitted.entry.class_id, e.declared.class_id);
    assert_eq!(admitted.record, e.row, "the one derivation of the row");
    assert_eq!(admitted.report.profile, PalwGenProfileV1::Embedding);
    assert!(admitted.report.output_tile_len.is_some_and(|t| t >= 4), "the output node has its own tile");
    assert!(!admitted.report.draws_randomness, "a bidirectional encoder draws none");
    // 4. The registration object a registrant signs: built under the chain's terms, gated again, its message
    // the one the bond signs (`palw_gen_class_registration_message_v1` over every field it carries).
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let gate = GenOfflineGateV1::of(&params);
    let terms = kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2 {
        min_grantable_share_permille: 1,
        slash_value_per_pwu: 1,
        initial_target: 1 << 100,
        registered_class_ids: vec![],
        registered_artifact_roots: vec![],
        chain_certified_families: vec![],
    };
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([5; 64]), 1));
    let object = build_gen_registration_v1(
        &gate.params,
        bundle,
        &e.declared.class,
        e.declared.artifact_root,
        &terms,
        gate.daa,
        bond,
        Vec::new(),
        gate.daa,
    )
    .unwrap_or_else(|why| panic!("the registration: {why}"));
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, share_permille, .. } = &object else {
        panic!("a generative registration")
    };
    assert_eq!((*class_id, *share_permille), (e.declared.class_id, 0), "weightless: a generative class earns none");
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_gen_v1(&object), "dropped by name below palw_gen_v1");
    let message = gen_registration_message_v1(Hash64::from_bytes([0xD0; 64]), &object).expect("a message to sign");
    assert_ne!(message, Hash64::default());
    eprintln!(
        "embedding class {}: {} dims, Q30; layouts at {} (checkpoint {}), canonical job {} step leaves, widest {}; gate {}",
        e.declared.class_id,
        e.lowered.dims,
        class.layouts[0].commit_tiles[0],
        class.layouts[0].checkpoint_interval,
        admitted.entry.canonical_step_leaf_count,
        admitted.entry.max_step_leaf_count,
        e.declared.admission_at
    );
}

/// A job of the class over `ids`.
fn job(e: &Embedding, ids: &[u32]) -> PalwGenJobV1 {
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: PalwJobEnvelopeV1 {
            network_domain: Hash64::from_bytes([0xD0; 64]),
            class_id: e.row.class_id,
            executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 0),
            executor_pubkey: vec![1; 8],
            operator_id: Hash64::from_bytes([2; 64]),
            anchor_block: Hash64::from_bytes([3; 64]),
            anchor_daa: 100,
            job_nonce: [4; 32],
            privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        },
        seed: [0; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Text {
                token_ids_hash: prompt_token_ids_commitment_v1(FORM, ids).unwrap(),
                tokens: ids.len() as u32,
            },
            pooling: PALW_GEN_POOLING_MEAN_V1,
            dims: e.lowered.dims,
            output: misaka_palw_gen::OutputKindV1::EmbeddingI32.tag(),
        }),
    }
}

/// A container path of its own per call (the tests run in parallel and each writes, holds and removes one).
fn container_path(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("misaka-gen-e2e-{}-{tag}-{n}.palwtir2", std::process::id()))
}

/// The container file a worker holds: read by path on demand, so it lives as long as the worker does.
struct TempContainer(PathBuf);
impl Drop for TempContainer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A worker that holds the class from its container file (the path a node takes), and the file's guard.
fn held(e: &Embedding) -> (GenHeldClassV1<misaka_palw_tir_artifact::PalwTirContainerV2>, TempContainer) {
    let path = container_path("held");
    let guard = TempContainer(path.clone());
    gen_write_declared_container_v1(&path, &e.declared, &e.weights, "{\"model_id\":\"tiny-bert\"}".into()).expect("the container");
    let held = GenHeldClassV1::hold_container(e.row.clone(), &path).expect("the worker holds the class");
    (held, guard)
}

#[test]
fn a_job_runs_to_a_canonical_embedding_a_seat_replays_and_the_container_is_the_class() {
    let e = build();
    let (held, _file) = held(&e);
    let ids: Vec<u32> = vec![11, 25, 7];
    let job = job(&e, &ids);
    // The job is the class's, by name.
    let accepted = palw_gen_job_resolve_class_v1(&job, &e.row).expect("the class accepts the job");
    assert_eq!(accepted.profile, PalwGenProfileV1::Embedding);
    // The worker's run: the canonical output is the program's own run, as EmbeddingI32.
    let work = held.run_tensor(&job, &ids, &[], &[], FORM).expect("the worker runs the job");
    let direct = {
        let pj = tir::pipeline::PipelineJob { prompt: ids.clone(), ..Default::default() };
        tir::pipeline::run_pipeline(&e.lowered.pipeline, std::slice::from_ref(&e.lowered.program), &e.weights, &NoRandom, &pj).unwrap()
    };
    let out = work.output();
    assert_eq!(out.values, direct.output.data.iter().map(|v| *v as i64).collect::<Vec<_>>(), "the output is the program's row");
    let bytes = e.declared.class.output.canonical_bytes(&out.values).expect("the canonical bytes");
    assert_eq!(bytes.len(), 4 * e.lowered.dims as usize);
    assert_eq!(bytes[..4], (out.values[0] as i32).to_le_bytes(), "little-endian i32 lanes");
    assert_eq!(
        out.root.as_byte_slice(),
        &misaka_palw_gen::output_root_v1(&e.declared.class.output, &out.values, out.tile_len).unwrap()[..]
    );
    // A normalised embedding: the L2 norm is 1 in Q30, to the integer lowering's precision.
    let norm: f64 = out.values.iter().map(|v| (*v as f64 / (1u64 << 30) as f64).powi(2)).sum::<f64>().sqrt();
    assert!((norm - 1.0).abs() < 0.01, "‖v‖ = {norm}");
    // The wire: one request frame in, the claim's binding out.
    let request = PalwGenTensorRequestV1 { job: job.clone(), prompt_ids: ids.clone(), negative_ids: vec![], images: vec![] };
    let (answer, again) = gen_tensor_answer_v1(&held, &borsh::to_vec(&request).unwrap(), FORM);
    assert_eq!(answer, PalwGenTensorAnswerV1::Result { binding: work.binding.clone() }, "the same job, the same claim");
    assert_eq!(again.unwrap().execution_root(), work.execution_root());
    let (refused, none) = gen_tensor_answer_v1(&held, b"not a request", FORM);
    assert!(matches!(refused, PalwGenTensorAnswerV1::Refused { .. }) && none.is_none());
    // A request the class does not accept is refused by name, and the held class stays held.
    let mut other = request.clone();
    other.prompt_ids = vec![11, 25, 8];
    let (refused, _) = gen_tensor_answer_v1(&held, &borsh::to_vec(&other).unwrap(), FORM);
    assert!(matches!(&refused, PalwGenTensorAnswerV1::Refused { why } if why.contains("hash")), "{refused:?}");
    // The seat replays the job from the material the panel received.
    let root = work.execution_root();
    assert_eq!(gen_tensor_seat_judge_v1(&held, &root, &job, &ids, &[], &[], FORM), GenSeatJudgmentV1::Valid);
    assert!(matches!(
        gen_tensor_seat_judge_v1(&held, &Hash64::from_bytes([9; 64]), &job, &ids, &[], &[], FORM),
        GenSeatJudgmentV1::Differs(_)
    ));
    assert!(matches!(gen_tensor_seat_judge_v1(&held, &root, &job, &[11, 25, 8], &[], &[], FORM), GenSeatJudgmentV1::Unjudgeable(_)));
    // Another prompt is another claim; the same ids in another job are another claim.
    let other_ids = vec![5, 6, 7, 8];
    let other_work = held.run_tensor(&self::job(&e, &other_ids), &other_ids, &[], &[], FORM).unwrap();
    assert_ne!(other_work.output().values, out.values);
    let mut renonced = job.clone();
    renonced.envelope.job_nonce[0] ^= 1;
    assert_ne!(held.run_tensor(&renonced, &ids, &[], &[], FORM).unwrap().execution_root(), root, "the job id is committed");
}

#[test]
fn a_planted_fault_is_convicted_through_the_court_and_an_honest_claim_is_not() {
    let e = build();
    let (held, _file) = held(&e);
    let ids: Vec<u32> = vec![11, 25, 7];
    let job = job(&e, &ids);
    let honest = held.run_tensor(&job, &ids, &[], &[], FORM).unwrap();
    let court = match &palw_t12_shipped_params().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.court.clone(),
        _ => unreachable!(),
    };
    let session = Hash64::from_bytes([0x5E; 64]);
    let file = |work: &GenTensorWorkV1, narrowed: u64, challenger: bool, mv: GenCourtMoveV1| {
        gen_court_object_v1(&held.row, &work.execution_root(), session, narrowed, 2, challenger, FORM, &court, mv, &|_| None)
    };
    let capture = GenTensorCaptureV1::of(&honest).expect("a capture");
    assert_eq!(capture.rebuild(&held).unwrap().binding, honest.binding, "the capture is the run's commitments");
    let out_stage = held.pipeline.output_stage as usize;
    let space = &honest.execution.space;

    // (a) A planted COMPUTATION fault: one committed lane of the encoder's tree is not what the program
    // computes. The accused commits to its lie; the honest run finds the first divergent leaf; the cone
    // close there convicts, and the accused has no move that wins.
    let victim = space.stages[0].leaves().iter().position(|l| l.value_count > 1).expect("a leaf with lanes");
    let mut lying = capture.clone();
    // Lanes are four bytes (PALW-TIR-5): flip the low bit of the leaf's first value.
    lying.leaves[0][victim][0] ^= 1;
    let accused = lying.rebuild(&held).expect("the lie is a consistent commitment");
    assert_ne!(accused.execution_root(), honest.execution_root());
    let index = gen_execution_first_divergence_v1(&accused.execution, &honest.execution).expect("the trees part");
    assert_eq!(index, victim as u64, "the first divergent leaf is the planted one");
    let challenger_moves = gen_tensor_court_candidates_v1(&held, &accused, index, true, &LIMITS);
    let (name, cone) = &challenger_moves[0];
    assert_eq!(*name, "cone");
    let object =
        file(&accused, index, true, cone.clone().expect("the challenger builds the cone close")).expect("the close is checked");
    let Some(PalwConsensusObjectV2::CourtClosed { verdict, proof, .. }) = object else {
        panic!("the challenger wins at the planted leaf")
    };
    assert_eq!(verdict, kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty);
    assert!(matches!(proof, PalwCourtVerdictProofV2::GenCone { .. }));
    // The accused's own cone close at that leaf convicts it, so it files nothing that wins.
    let own = gen_tensor_court_candidates_v1(&held, &accused, index, false, &LIMITS);
    let (_, own_cone) = &own[0];
    assert_eq!(file(&accused, index, false, own_cone.clone().unwrap()).unwrap(), None, "a lie has no winning defence");

    // (b) A planted DIGEST fault: the step tree is honest and the canonical output is not its own output
    // node's. No leaf diverges; the output audit names the tile; the output close convicts.
    let mut lying = capture.clone();
    lying.output[3] ^= 1;
    let accused = lying.rebuild(&held).unwrap();
    assert_ne!(accused.execution_root(), honest.execution_root(), "the output root is part of the claim");
    assert_eq!(gen_execution_first_divergence_v1(&accused.execution, &honest.execution), None, "the step trees agree");
    let (tile, leaf, fault) = gen_tensor_output_audit_v1(&held, &accused).expect("the audit finds the tile");
    assert_eq!(fault, PalwStepFaultV1::TirOutputDigestMismatch { value_index: 3 });
    let moves = gen_tensor_court_candidates_v1(&held, &accused, leaf, true, &LIMITS);
    let (_, output_close) =
        moves.iter().find(|(n, _)| *n == "output").expect("a challenger at the output node's step tile files the output close");
    let object = file(&accused, leaf, true, output_close.clone().expect("the output close builds")).expect("checked");
    let Some(PalwConsensusObjectV2::CourtClosed { verdict, proof, .. }) = object else { panic!("the output close wins") };
    assert_eq!(verdict, kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty);
    assert!(matches!(proof, PalwCourtVerdictProofV2::GenOutputTile { .. }));
    let _ = tile;

    // (c) A committed lane OUTSIDE the output node's proven interval (PALW-TIR-33): the lane is a lane no
    // honest run produces, whatever the canonical bytes say.
    let interval = tir::interval_v2::output_interval_v2(&held.programs[0]).expect("the output node's proven interval");
    let out_leaf = {
        let sp = &space.stages[out_stage];
        let coord = kaspa_consensus_core::palw_gen_close_v1::palw_gen_output_step_coord_v1(sp, 0, honest.output().tile_len).unwrap();
        sp.leaf_index(&coord).unwrap() as usize
    };
    assert!(interval.hi < i32::MAX as i128, "the normalised embedding's proven interval is inside i32");
    let mut lying = capture.clone();
    lying.leaves[out_stage][out_leaf][4..8].copy_from_slice(&((interval.hi + 1) as i32).to_le_bytes());
    let accused = lying.rebuild(&held).unwrap();
    let (_, leaf, fault) = gen_tensor_output_audit_v1(&held, &accused).expect("the audit finds the lane");
    assert_eq!(fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 1 });
    let moves = gen_tensor_court_candidates_v1(&held, &accused, leaf, true, &LIMITS);
    let (_, output_close) = moves.iter().find(|(n, _)| *n == "output").unwrap();
    assert!(file(&accused, leaf, true, output_close.clone().unwrap()).unwrap().is_some(), "the challenger wins at the lane");

    // (d) An HONEST claim is not convicted: the audit finds nothing, the trees agree, and the responder
    // wins the cone close at any leaf the challenger narrows to.
    assert_eq!(gen_tensor_output_audit_v1(&held, &honest), None);
    for index in [0u64, victim as u64, space.leaf_count() - 1] {
        let (_, cone) = &gen_tensor_court_candidates_v1(&held, &honest, index, false, &LIMITS)[0];
        let object = file(&honest, index, false, cone.clone().expect("the responder builds the cone close")).unwrap();
        let Some(PalwConsensusObjectV2::CourtClosed { verdict, .. }) = object else { panic!("the responder wins at leaf {index}") };
        assert_eq!(verdict, kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ChallengerDefeated);
        // …and the challenger has no winning move there.
        let (_, cone) = &gen_tensor_court_candidates_v1(&held, &honest, index, true, &LIMITS)[0];
        assert_eq!(file(&honest, index, true, cone.clone().unwrap()).unwrap(), None);
    }
}
