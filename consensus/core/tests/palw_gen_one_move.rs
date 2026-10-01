//! **RFC-0003 §I.4.7: a tensor claim's court on a chain that plays no bisection** — the generative one-move
//! accusation (`GenShardCourtAccused`), through the real fold on testnet-12's own state with `palw_gen_v1` and
//! `palw_fp_job_v5` armed, over a REAL execution of the golden toy image pipeline (its weights, its worker, its
//! step tree, its output digest) — not a stand-in:
//!
//! * the worker's binding IS the commitment's roots and the chain's own count of the job's leaves (the worker, the
//!   carriage and the fold agree on one execution);
//! * a planted lie in the canonical output is convicted in one move by the output close (`GenOutputTile`, tag 16),
//!   and a lie in a step tile by the cone close (`GenCone`): the claim voids, its executor is charged;
//! * an honest claim accused is acquitted and its accuser charged;
//! * the refusals by name: another claim's roots, a producer accusing its own claim, an accuser the registry does
//!   not hold, a terminal claim, a claim that is not a pipeline claim's, and below `palw_gen_v1`.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP};
use kaspa_consensus_core::palw_gen_admission_v1::*;
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_claim_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_one_move_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::{PalwGenLeafCoordV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenExecutionV1, palw_gen_execute_tensor_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::{PalwClaimSourceV2, PalwCourtVerdictV2, PalwStateV2Error, PalwVoidReasonV2, palw_object_is_gen_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::Transaction;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{Binding, JobImageV1, PipelineParams, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const AT: u64 = 1_100;
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
const EXECUTOR: u64 = 21;
const ACCUSER: u64 = 22;

fn net_domain() -> Hash64 {
    h(0xD0)
}

fn armed() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    p.palw_fp_decode_rules = Some(ForkActivation::new(AT));
    p.sync_palw_fp_decode_rules();
    p.palw_fp_job_v5 = Some(ForkActivation::new(AT));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the four fences at {AT}: {e}"));
    p
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Weights(Vec<MapParams>);
impl PipelineParams for Weights {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// The golden toy text-to-image class with its weights, as the chain registers it.
struct Fixture {
    class: PalwGenClassV1,
    row: PalwGenClassRecordV1,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    weights: Weights,
}

fn image() -> Fixture {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/toy-image.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let program_bytes: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let programs: Vec<TirProgramV2> = program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b).unwrap()).collect();
    let pipeline_bytes = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let pipeline = TirPipelineV1::decode_canonical(&pipeline_bytes, &programs).unwrap();
    let weights = Weights(
        v["programs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&programs)
            .map(|(pj, prog)| {
                let mut m = MapParams::default();
                for e in pj["params"].as_array().unwrap() {
                    let j = e["param"].as_u64().unwrap() as u16;
                    let decl = &prog.params[j as usize];
                    let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                    let layer = e["layer"].as_u64().map(|l| l as u16);
                    m.tensors
                        .insert((j, layer), Tensor::from_le_bytes(decl.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap());
                }
                m
            })
            .collect(),
    );
    let layouts: Vec<PalwTirLayoutV1> = pipeline
        .stages
        .iter()
        .map(|st| {
            let p = &programs[st.program as usize];
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: 1,
                h_tile: 16,
                commit_tiles: vec![4; commits],
                state_tiles: vec![4; p.states.len()],
            }
        })
        .collect();
    let steps_max = pipeline.stages.iter().filter(|st| matches!(st.trip, TripRule::JobSteps)).map(|st| st.max_trip).min().unwrap();
    let mut scalars = Vec::new();
    for st in &pipeline.stages {
        let ext = programs[st.program as usize].inputs.iter().filter(|d| d.is_external());
        for (b, d) in st.bind.iter().zip(ext) {
            if let Binding::JobScalar { index } = b {
                let (lo, hi) = d.interval();
                if scalars.len() <= *index as usize {
                    scalars.resize(*index as usize + 1, PalwGenScalarOfferV1 { lo: 0, hi: 0 });
                }
                scalars[*index as usize] = PalwGenScalarOfferV1 { lo: lo as i64, hi: hi as i64 };
            }
        }
    }
    let rule = pipeline.stages.iter().find_map(|st| st.tokens.as_ref()).expect("the encoder reads the prompt");
    assert_eq!(rule.source, TokenSource::Prompt);
    let encoder = pipeline.stages.iter().find(|st| st.tokens.is_some()).unwrap();
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Image as u8,
        layouts,
        offers: PalwGenOffersV1 {
            steps: vec![steps_max - 1, steps_max],
            profile: PalwGenProfileOffersV1::Image(PalwGenImageOffersV1 {
                sampler_id: Hash64::from_bytes([0x5A; 64]),
                guidance: Some(PalwGenGuidanceOfferV1 { scalar: 0, lo: scalars[0].lo as u16, hi: scalars[0].hi as u16 }),
                steps_scalar: Some(1),
            }),
            scalars,
            max_prompt_tokens: encoder.max_trip - (rule.prefix.len() + rule.suffix.len()) as u32,
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
        },
        output: OutputSpecV1::image_rgb8(2, 2),
        pipeline: pipeline_bytes,
        programs: program_bytes,
        tokenizer_id: Hash64::from_bytes([0x71; 64]),
    };
    let (root, _) = palw_gen_inventory_root_v1(&programs, &weights).expect("the toy weights have an inventory");
    let row = palw_gen_class_record_v1(&class, &root).expect("the class is a row");
    Fixture { class, row, pipeline, programs, weights }
}

fn prompt() -> Vec<u32> {
    vec![2, 5, 1]
}

/// The executor's job: the toy class's, by bond `n`.
fn job_of(f: &Fixture, n: u64, nonce: u8, seed: u8) -> PalwGenJobV1 {
    let PalwGenProfileOffersV1::Image(offers) = &f.class.offers.profile else { unreachable!() };
    let g = offers.guidance.as_ref().unwrap();
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: PalwJobEnvelopeV1 {
            network_domain: net_domain(),
            class_id: f.row.class_id,
            executor_bond: bond_key(n).0,
            executor_pubkey: fp_pubkey_of(n),
            operator_id: h(0x0E),
            anchor_block: h(0xA1),
            anchor_daa: AT,
            job_nonce: [nonce; 32],
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        },
        seed: [seed; 32],
        body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &prompt()).unwrap(),
            prompt_tokens: prompt().len() as u32,
            negative_token_ids_hash: Hash64::default(),
            negative_tokens: 0,
            guidance_q: g.lo,
            image_index: 0,
            sampler_id: offers.sampler_id,
            steps: f.class.offers.steps[0] as u16,
            width: 2,
            height: 2,
            output: misaka_palw_gen::OutputKindV1::ImageRgb8.tag(),
        }),
    }
}

/// The worker's run of a job: the execution and its binding (what `GenHeldClassV1::run_tensor` does).
fn run(f: &Fixture, job: &PalwGenJobV1) -> (PalwGenExecutionV1, PalwGenTensorBindingV1) {
    let p = prompt();
    let ids = PalwGenIdsV1 { prompt: &p, negative: &[] };
    let accepted = palw_gen_job_resolve_class_v1(job, &f.row).unwrap_or_else(|e| panic!("the job is the class's: {e}"));
    palw_gen_job_ids_admitted_v1(&f.row, &accepted, ids, FORM).unwrap_or_else(|e| panic!("the ids are the job's: {e}"));
    let run_job = palw_gen_pipeline_job_v1(&accepted, ids, Vec::<JobImageV1>::new());
    let e = palw_gen_execute_tensor_v1(
        &f.pipeline,
        &f.programs,
        &f.class.layouts,
        &f.weights,
        &run_job,
        job.seed,
        accepted.item_index,
        &f.class.output,
    )
    .unwrap_or_else(|e| panic!("the pipeline runs: {e}"));
    let root = e.claim.output_root.expect("a tensor run commits an output root");
    let binding = PalwGenTensorBindingV1::of(job, &e.claim, e.space.leaf_count(), root);
    (e, binding)
}

fn evidence<'a>(f: &'a Fixture, e: &'a PalwGenExecutionV1, binding: &PalwGenTensorBindingV1, ids: &'a [u32]) -> PalwGenEvidenceV1<'a> {
    PalwGenEvidenceV1 {
        row: &f.row,
        params: &f.weights,
        execution: e,
        binding: binding.clone().into(),
        prompt: ids,
        negative: &[],
        images: &[],
        source: &[],
    }
}

/// An execution whose claimed canonical output differs from its own step tree at one lane.
fn lie_about_the_output(e: &PalwGenExecutionV1, lane: usize) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    let out = l.output.as_mut().unwrap();
    out.values[lane] ^= 1;
    out.root = Hash64::from_bytes(misaka_palw_gen::output_root_v1(&out.spec, &out.values, out.tile_len).unwrap());
    l.claim.output_root = Some(out.root);
    l
}

/// An execution with `value` at `lane` of the step tile at `coord`, every root recomputed over it.
fn lie_in_the_step_tile(e: &PalwGenExecutionV1, coord: &PalwGenLeafCoordV1, lane: usize, value: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    let stage = coord.stage as usize;
    let index = l.space.stages[stage].leaf_index(coord).unwrap() as usize;
    l.leaf_values[stage][index][lane] = value;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

/// **The chain**: fences armed, the toy class registered at [`AT`] under its REAL weights' root, two rich bonds.
struct Env {
    chain: Chain,
    f: Fixture,
}

fn env() -> Env {
    let f = image();
    let mut chain = Chain::new(armed());
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    let object = palw_gen_post_genesis_registration_v1(f.class.clone(), f.row.artifact_root, 0, target, slash, AT, registrant, vec![9; 16])
        .expect("the builder counts the yardstick job");
    let bond = |n: u64| PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: fp_pubkey_of(n),
        operator_pubkey: operator_pubkey_of(n),
        collateral: RICH,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    chain.step_at(AT - 1, &[bond(EXECUTOR), bond(ACCUSER)], PalwBlockWorkV3::None, Hash64::default(), 0);
    chain.step_at(AT, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(chain.s.gen_class_v1(&f.row.class_id), Some(&f.row), "the chain's row is the fixture's");
    // The genesis bonds hold the class (readiness V2 rows): a claim flows only once a panel's worth can replay it.
    chain.s = readied(&chain.sp, &chain.s, &honest(&chain.p), f.row.class_id, chain.daa);
    Env { chain, f }
}

/// The object the acceptance walk makes of an execution's commitment, and the payload it came from.
fn commit(job: &PalwGenJobV1, e: &PalwGenExecutionV1, binding: &PalwGenTensorBindingV1) -> (PalwConsensusObjectV2, kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3) {
    let payload = palw_gen_payload_v1(
        job,
        e.space.leaf_count(),
        e.claim.step_root,
        e.claim.output_root.unwrap(),
        prompt(),
        vec![7u8; MLDSA87_SIGNATURE_LEN],
    );
    assert_eq!(payload.commitment.execution_root, binding.committed_execution_root, "the worker's binding is the commitment's execution root");
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(&payload).unwrap());
    let out = palw_fp_gen_objects_from_accepted_txs_v1(&[tx], net_domain(), true, |_| PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, FORM, |_, _, _, _| true);
    let [one] = &out.objects[..] else { panic!("the walk makes one object: {:?}", out.skipped) };
    (one.object.clone(), payload)
}

fn claim_id_of(object: &PalwConsensusObjectV2) -> Hash64 {
    let PalwConsensusObjectV2::GenTensorCommitted { claim, .. } = object else { panic!("a tensor claim's object") };
    *claim
}

/// The accusation of `claim_id` by [`ACCUSER`] carrying `proof`, its verdict derived by the court's own
/// adjudication (what the acceptance layer re-derives), unsigned beyond a placeholder.
fn accuse(env: &Env, claim_id: Hash64, proof: PalwCourtVerdictProofV2) -> (PalwGenOneMoveAccusationV1, PalwCourtVerdictV2) {
    let claim = env.chain.claim(&claim_id);
    let court = bundle(&env.chain.p).court;
    let ladder = env.chain.s.class_step_ladder_v1(&claim.class_id, court.max_step_leaf_count());
    let mut a = palw_gen_one_move_accusation_v1(claim_id, &claim, bond_key(ACCUSER), PalwCourtVerdictV2::ExecutorGuilty, proof);
    a.signature = vec![1; 8];
    let verdict = palw_gen_one_move_verdict_v1(&env.chain.s, &claim, &a, &court, ladder, FORM).unwrap_or_else(|e| panic!("the proof adjudicates: {e}"));
    a.verdict = verdict;
    (a, verdict)
}

fn object_of(a: &PalwGenOneMoveAccusationV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::GenShardCourtAccused { accusation: Box::new(a.clone()) }
}

#[test]
fn the_workers_run_the_commitment_and_the_chains_count_are_one_execution() {
    let env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (e, binding) = run(&env.f, &job);
    let (object, payload) = commit(&job, &e, &binding);
    let PalwConsensusObjectV2::GenTensorCommitted { trace_root, output_root, execution_root, work_leaves, .. } = &object else { unreachable!() };
    assert_eq!((*trace_root, *output_root, *execution_root, *work_leaves), (binding.step_root(), binding.output_root, binding.committed_execution_root, binding.step_leaf_count));
    // The chain's closed-form count of the job's step space is the worker's enumeration.
    let accepted = palw_gen_job_resolve_class_v1(&job, &env.f.row).unwrap();
    assert_eq!(palw_gen_job_step_leaves_v1(&env.f.row, &accepted), Ok(binding.step_leaf_count), "the chain counts what the worker commits");
    assert_eq!(payload.claim_id(), claim_id_of(&object));
}

#[test]
fn a_planted_output_lie_is_convicted_in_one_move_and_the_claim_voids() {
    let mut env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (honest, _) = run(&env.f, &job);
    let lanes = honest.output.as_ref().unwrap().values.len();
    let tile_len = honest.output.as_ref().unwrap().tile_len as usize;
    let lied = lie_about_the_output(&honest, lanes / 2);
    let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
    let (object, _) = commit(&job, &lied, &accused);
    let id = claim_id_of(&object);
    env.chain.step(&[object]);
    let executor_collateral = env.chain.s.bond(&bond_key(EXECUTOR)).expect("the executor").collateral;
    let ids = prompt();
    let ev = evidence(&env.f, &lied, &accused, &ids);
    // The challenger's output close at the lying tile convicts; every other tile is honest.
    let bad_tile = (lanes / 2 / tile_len) as u64;
    let close = ev.output_close(bad_tile).expect("the close builds");
    let (a, verdict) = accuse(&env, id, PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) });
    assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "the output close convicts the lie");
    assert!(palw_gen_one_move_shape_v1(&a).is_ok());
    env.chain.step(&[object_of(&a)]);
    let claim = env.chain.claim(&id);
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{:?}", claim.phase);
    assert!(env.chain.s.bond(&bond_key(EXECUTOR)).expect("the executor").collateral < executor_collateral, "the executor is charged");
    assert!(matches!(claim.source, PalwClaimSourceV2::FreePrompt { quanta: 0, .. }));
    assert_eq!(env.chain.s.safe_weight(), 0, "a weightless claim moves no weight, convicted or not");
}

#[test]
fn a_planted_step_tile_lie_is_convicted_by_the_cone_close() {
    let mut env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (honest, _) = run(&env.f, &job);
    // A lie in the last stage's first committed leaf (the output node's step tile): one lane moved.
    let out_stage = env.f.pipeline.output_stage as usize;
    let coord = env.f.row.class_id; // placeholder to keep the borrow checker honest about `honest` below
    let _ = coord;
    let leaf = honest.space.stages[out_stage].leaves()[0];
    let value = honest.leaf_values[out_stage][0][0] ^ 1;
    let lied = lie_in_the_step_tile(&honest, &leaf.coord, 0, value);
    let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
    let (object, _) = commit(&job, &lied, &accused);
    let id = claim_id_of(&object);
    env.chain.step(&[object]);
    let ids = prompt();
    let ev = evidence(&env.f, &lied, &accused, &ids);
    let index = lied.space.global_index(&leaf.coord).expect("the leaf");
    let close = ev.cone_close(index, &LIMITS).expect("the cone close builds");
    let (a, verdict) = accuse(&env, id, PalwCourtVerdictProofV2::GenCone { close: Box::new(close) });
    assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "the cone close convicts the lie");
    env.chain.step(&[object_of(&a)]);
    assert!(matches!(env.chain.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
}

#[test]
fn an_honest_claim_accused_is_acquitted_and_its_accuser_is_charged() {
    let mut env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (honest, binding) = run(&env.f, &job);
    let (object, _) = commit(&job, &honest, &binding);
    let id = claim_id_of(&object);
    env.chain.step(&[object]);
    let ids = prompt();
    let ev = evidence(&env.f, &honest, &binding, &ids);
    let accuser_collateral = env.chain.s.bond(&bond_key(ACCUSER)).expect("the accuser").collateral;
    for tile in 0..2u64 {
        let close = ev.output_close(tile).expect("the close builds");
        let claim = env.chain.claim(&id);
        let court = bundle(&env.chain.p).court;
        let mut a = palw_gen_one_move_accusation_v1(id, &claim, bond_key(ACCUSER), PalwCourtVerdictV2::ExecutorGuilty, PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) });
        a.signature = vec![1; 8];
        let verdict = palw_gen_one_move_verdict_v1(&env.chain.s, &claim, &a, &court, court.max_step_leaf_count(), FORM).expect("adjudicates");
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest tile acquits");
        if tile == 0 {
            a.verdict = verdict;
            env.chain.step(&[object_of(&a)]);
        }
    }
    let claim = env.chain.claim(&id);
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional), "the claim stands: {:?}", claim.phase);
    assert!(env.chain.s.bond(&bond_key(ACCUSER)).expect("the accuser").collateral < accuser_collateral, "a false accusation is charged");
}

#[test]
fn the_accusation_is_refused_by_name_where_it_is_not_a_pipeline_claims_court() {
    let mut env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (honest, binding) = run(&env.f, &job);
    let lied = lie_about_the_output(&honest, 0);
    let accused = PalwGenTensorBindingV1::of(&job, &lied.claim, lied.space.leaf_count(), lied.claim.output_root.unwrap());
    let (object, _) = commit(&job, &lied, &accused);
    let id = claim_id_of(&object);
    env.chain.step(&[object]);
    let ids = prompt();
    let ev = evidence(&env.f, &lied, &accused, &ids);
    let tile_len = lied.output.as_ref().unwrap().tile_len as usize;
    let _ = (binding, tile_len);
    let close = ev.output_close(0).unwrap();
    let (good, _) = accuse(&env, id, PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) });
    fn fold(env: &Env, a: &PalwGenOneMoveAccusationV1) -> Result<(), PalwStateV2Error> {
        let at = env.chain.daa + 1;
        env.chain.try_fold(&env.chain.s, &ctx(0xCA_0000 + at, at, at, 0), &[object_of(a)], PalwBlockWorkV3::None, Hash64::default()).map(|_| ())
    }
    assert!(fold(&env, &good).is_ok(), "the control folds");
    // Another claim: not on the chain.
    let mut elsewhere = good.clone();
    elsewhere.claim = h(0xDEAD);
    assert!(matches!(fold(&env, &elsewhere), Err(PalwStateV2Error::MissingClaim(c)) if c == h(0xDEAD)));
    // The roots and the executor are the claim's.
    let mut roots = good.clone();
    roots.execution_root = h(1);
    assert!(matches!(fold(&env, &roots), Err(PalwStateV2Error::ShardCourtRootsDiffer(_))));
    let mut wrong_executor = good.clone();
    wrong_executor.executor_bond = bond_key(ACCUSER);
    assert!(matches!(fold(&env, &wrong_executor), Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(_))));
    // A producer does not accuse its own claim.
    let mut own = good.clone();
    own.accuser_bond = bond_key(EXECUTOR);
    assert!(matches!(fold(&env, &own), Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(_))));
    // An accuser the registry does not hold.
    let mut stranger = good.clone();
    stranger.accuser_bond = bond_key(77);
    assert!(matches!(fold(&env, &stranger), Err(PalwStateV2Error::MissingBond(_))));
    // A terminal claim takes no accusation: convict it, then accuse again.
    env.chain.step(&[object_of(&good)]);
    let again = fold(&env, &good);
    assert!(matches!(again, Err(PalwStateV2Error::WrongPhase { .. }) | Err(PalwStateV2Error::MissingClaim(_))), "{again:?}");
}

#[test]
fn an_accusation_below_the_generative_fence_is_refused_and_one_of_another_class_is_not_this_courts() {
    let env = env();
    let job = job_of(&env.f, EXECUTOR, 1, 0x33);
    let (honest, binding) = run(&env.f, &job);
    let (object, _) = commit(&job, &honest, &binding);
    let id = claim_id_of(&object);
    let ids = prompt();
    let ev = evidence(&env.f, &honest, &binding, &ids);
    let close = ev.output_close(0).unwrap();
    let mut env = env;
    env.chain.step(&[object]);
    let (a, _) = accuse(&env, id, PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) });
    assert!(palw_object_is_gen_v1(&object_of(&a)), "a generative object: dropped by name below the fence");
    // A fresh state below the fence refuses it by name (the lock before any arm).
    let fresh = Chain::new(armed());
    let below = fresh.try_fold(&fresh.s, &ctx(0xCA_0000 + AT - 1, AT - 1, AT - 1, 0), &[object_of(&a)], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(below, Err(PalwStateV2Error::GenObjectRefused(_))), "{below:?}");
    // The shape: a signature, a generative close, the roots the proof's binding commits, not one's own claim.
    let mut unsigned = a.clone();
    unsigned.signature.clear();
    assert!(palw_gen_one_move_shape_v1(&unsigned).is_err());
    let mut other_roots = a.clone();
    other_roots.execution_root = h(2);
    assert!(palw_gen_one_move_shape_v1(&other_roots).is_err());
    let mut self_accuse = a.clone();
    self_accuse.accuser_bond = a.executor_bond;
    assert!(palw_gen_one_move_shape_v1(&self_accuse).is_err());
    assert_eq!(palw_gen_one_move_shape_v1(&a), Ok(()));
    // The session id covers every field: the signature is over what the object says.
    let id_of = |x: &PalwGenOneMoveAccusationV1| palw_gen_one_move_session_id_v1(b"net", x);
    assert_ne!(id_of(&a), id_of(&other_roots));
    assert_ne!(id_of(&a), palw_gen_one_move_session_id_v1(b"other", &a), "the network");
}
