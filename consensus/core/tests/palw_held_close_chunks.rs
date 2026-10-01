//! **RFC-0003 §I.4.7, decision 22: the held leaf challenge** (`HeldLeafChallengeDeclared`, tag 90; spec 04b
//! §15.15.6) — through the real fold on testnet-12's own state with `palw_gen_v1`, `palw_fp_job_v5` and
//! `palw_held_close_chunks_v1` armed and the held regime in force, over a REAL execution of the golden toy image
//! pipeline (its weights, its worker, its step tree, its output digest) — not a stand-in:
//!
//! * a challenge opens a session at `Terminal` on the named leaf and writes the challenger-side close group the
//!   court's declaration would have written (declarer the accuser, the pinned digests, the deposit, `4 · count`
//!   DAA); the claim is not touched;
//! * the close rides the court's own chunks, in any order; the completing chunk applies the assembled
//!   `CourtClosed` through the one arm: the lying claim voids as `CourtFraud` and its executor is charged, the
//!   accuser's deposit is refunded by never being taken; the acceptance layer's adjudication of the assembled
//!   proof at the session's leaf is the court's own function (and a close of another leaf is refused by it);
//! * a close that never comes convicts its DECLARER at the assembly deadline: the accuser is charged and the
//!   claim is left alone;
//! * an executor's own acquitting close may stand beside the challenger's group, and whichever completes first
//!   ends the session;
//! * the refusals by name: below the fence, off the held regime, another claim's roots, a producer challenging
//!   its own claim, a stranger, a leaf outside the claim's step space, a count outside 1..=32, one digest per
//!   chunk, a second challenge by the same accuser, a further challenge from a bond that is no seat.
//!
//! Every step is the harness's: the delta re-applies and reverts, and the carriage reloads to the same state.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
};
use kaspa_consensus_core::palw_gen_admission_v1::*;
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_claim_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::{
    PalwGenLeafCoordV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenExecutionV1, palw_gen_execute_tensor_v1};
use kaspa_consensus_core::palw_held_close_v1::*;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwCourtSideV1, PalwCourtVerdictV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwVoidReasonV2,
    apply_delta_v2, palw_close_assembly_daa_v1, palw_close_assembly_deposit_v1, palw_court_close_chunk_digest_v1, revert_delta_v2,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::Transaction;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{Binding, JobImageV1, PipelineParams, TirPipelineV1, TokenSource, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const AT: u64 = 1_100;
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const EXECUTOR: u64 = 21;
const ACCUSER: u64 = 22;
const OTHER: u64 = 23;
/// The held regime's ladder the fold is run under (the extras' `held_context_ladder`).
const HELD: u64 = 1 << 20;

fn net_domain() -> Hash64 {
    h(0xD0)
}

fn armed() -> Params {
    armed_with_chunks_at(AT)
}

/// The five fences, the held leaf challenge's at `chunks_at`.
fn armed_with_chunks_at(chunks_at: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    p.palw_fp_decode_rules = Some(ForkActivation::new(AT));
    p.sync_palw_fp_decode_rules();
    p.palw_fp_job_v5 = Some(ForkActivation::new(AT));
    p.sync_palw_gen_v1();
    // Decision 22: the held leaf challenge, over the held regime the release carries from genesis.
    p.palw_held_close_chunks_v1 = Some(ForkActivation::new(chunks_at));
    p.sync_palw_held_close_chunks_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the five fences at {AT} and {chunks_at}: {e}"));
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

/// **The chain**: fences armed, the toy class registered at [`AT`] under its REAL weights' root, two rich bonds.
struct Env {
    chain: Chain,
    f: Fixture,
}

fn env() -> Env {
    env_with(armed())
}

fn env_with(p: Params) -> Env {
    let f = image();
    let mut chain = Chain::new(p);
    chain.room = true;
    let (registrant, _, _) = floor_producer(&chain.p);
    let (floor, _, target, slash) = genesis_classes(&chain.p)[0];
    let target = chain.s.class_target(&floor).map(|t| t.target).unwrap_or(target);
    let object =
        palw_gen_post_genesis_registration_v1(f.class.clone(), f.row.artifact_root, 0, target, slash, AT, registrant, vec![9; 16])
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
fn commit(
    job: &PalwGenJobV1,
    e: &PalwGenExecutionV1,
    binding: &PalwGenTensorBindingV1,
) -> (PalwConsensusObjectV2, kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3) {
    let payload = palw_gen_payload_v1(
        job,
        e.space.leaf_count(),
        e.claim.step_root,
        e.claim.output_root.unwrap(),
        prompt(),
        vec![7u8; MLDSA87_SIGNATURE_LEN],
    );
    assert_eq!(
        payload.commitment.execution_root, binding.committed_execution_root,
        "the worker's binding is the commitment's execution root"
    );
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(&payload).unwrap());
    let out = palw_fp_gen_objects_from_accepted_txs_v1(
        &[tx],
        net_domain(),
        true,
        |_| PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
        FORM,
        |_, _, _, _| true,
    );
    let [one] = &out.objects[..] else { panic!("the walk makes one object: {:?}", out.skipped) };
    // Spec 17 section 17.0: tag 87, declared explicitly — no declaration order moves it.
    assert_eq!(borsh::to_vec(&one.object).expect("serializes")[0], 87, "GenTensorCommitted rides under tag 87");
    (one.object.clone(), payload)
}

fn claim_id_of(object: &PalwConsensusObjectV2) -> Hash64 {
    let PalwConsensusObjectV2::GenTensorCommitted { claim, .. } = object else { panic!("a tensor claim's object") };
    *claim
}

// ---------------------------------------------------------------------------------------------
// The held regime's steps: the harness's, with `held_context_ladder` set
// ---------------------------------------------------------------------------------------------

type Folded = Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error>;

fn fold_held(env: &Env, parent: &PalwChainStateV2, at: u64, objects: &[PalwConsensusObjectV2], held: Option<u64>) -> Folded {
    let c = ctx(0xCA_0000 + at, at, at, 0);
    let mut e = env.chain.extras_at(at);
    e.held_context_ladder = held;
    fold_with(&env.chain.p, &env.chain.sp, parent, &c, objects, PalwBlockWorkV3::None, Hash64::default(), &e)
}

/// One block at `daa` under the held regime: folded, re-applied, reverted, reloaded (the harness's `step_at`).
fn step_held(env: &mut Env, daa: u64, objects: &[PalwConsensusObjectV2]) {
    assert!(daa > env.chain.daa, "DAA moves forward");
    let parent = env.chain.s.clone();
    let (child, delta, skips) =
        fold_held(env, &parent, daa, objects, Some(HELD)).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
    assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
    let sp: &PalwStateParamsV2 = &env.chain.sp;
    assert_eq!(apply_delta_v2(&parent, &delta, sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child)
        .into_state(sp, Some(child.state_root()))
        .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    env.chain.daa = daa;
    env.chain.s = child;
}

fn next(env: &Env) -> u64 {
    env.chain.daa + 1
}

fn collateral(env: &Env, n: u64) -> u64 {
    env.chain.s.bond(&bond_key(n)).expect("the bond").collateral
}

// ---------------------------------------------------------------------------------------------
// The claims and the closes
// ---------------------------------------------------------------------------------------------

/// A tensor claim on the chain: the execution (lied about or not), the binding it commits, the claim's id.
struct Claim {
    execution: PalwGenExecutionV1,
    binding: PalwGenTensorBindingV1,
    id: Hash64,
}

fn put_claim(env: &mut Env, nonce: u8, lie: Option<usize>) -> Claim {
    let job = job_of(&env.f, EXECUTOR, nonce, 0x33);
    let (honest, _) = run(&env.f, &job);
    let execution = match lie {
        Some(lane) => lie_about_the_output(&honest, lane),
        None => honest,
    };
    let binding =
        PalwGenTensorBindingV1::of(&job, &execution.claim, execution.space.leaf_count(), execution.claim.output_root.unwrap());
    let (object, _) = commit(&job, &execution, &binding);
    let id = claim_id_of(&object);
    let at = next(env);
    step_held(env, at, &[object]);
    Claim { execution, binding, id }
}

/// The output close of `tile` of `claim`, and the global leaf the close opens (the leaf a challenge names).
fn output_close(env: &Env, claim: &Claim, tile: u64) -> (PalwCourtVerdictProofV2, u64) {
    let ids = prompt();
    let ev = evidence(&env.f, &claim.execution, &claim.binding, &ids);
    let close = ev.output_close(tile).expect("the close builds");
    let leaf = claim.execution.space.global_index(&close.step_tile.coord).expect("the step tile is a leaf of the claim");
    (PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) }, leaf)
}

/// A challenge, and what rides behind it.
struct Challenge {
    object: PalwConsensusObjectV2,
    challenge: PalwHeldLeafChallengeV1,
    session: Hash64,
    proof: PalwCourtVerdictProofV2,
    chunks: Vec<Vec<u8>>,
}

fn split(bytes: &[u8], count: u8) -> Vec<Vec<u8>> {
    assert!(bytes.len() >= count as usize, "a close of {} bytes cannot ride in {count} chunks", bytes.len());
    let per = bytes.len().div_ceil(count as usize);
    let mut chunks: Vec<Vec<u8>> = bytes.chunks(per).map(<[u8]>::to_vec).collect();
    // `div_ceil` can leave fewer chunks than asked for; carve the last until there are enough.
    while chunks.len() < count as usize {
        let last = chunks.pop().expect("a chunk");
        let (a, b) = last.split_at(last.len() / 2);
        chunks.push(a.to_vec());
        chunks.push(b.to_vec());
    }
    chunks
}

/// The challenge of `claim_id` by bond `accuser` at `leaf` pinning `proof`'s close in `count` chunks.
fn challenge(env: &Env, claim_id: Hash64, accuser: u64, leaf: u64, proof: PalwCourtVerdictProofV2, count: u8) -> Challenge {
    let claim = env.chain.claim(&claim_id);
    let ladder = env.chain.s.class_step_ladder_v1(&claim.class_id, HELD);
    let mut c = PalwHeldLeafChallengeV1 {
        version: PALW_HELD_LEAF_CHALLENGE_VERSION_V1,
        claim: claim_id,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.bond,
        accuser_bond: bond_key(accuser),
        leaf_index: leaf,
        count,
        chunk_digests: Vec::new(),
        close_digest: Hash64::default(),
        signature: vec![1; 8],
    };
    let session = palw_held_leaf_challenge_session_id_v1(&c, ladder);
    let close =
        PalwConsensusObjectV2::CourtClosed { session_id: session, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof: proof.clone() };
    let bytes = borsh::to_vec(&close).expect("a close serializes");
    let chunks = split(&bytes, count);
    c.chunk_digests = chunks.iter().map(|b| palw_court_close_chunk_digest_v1(b)).collect();
    c.close_digest = palw_court_close_chunk_digest_v1(&bytes);
    let object = PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge: Box::new(c.clone()) };
    // Spec 17 section 17.0: tag 90, declared explicitly (89 is RFC-0004's `CourtEvalRootClaimed`); and it reads back.
    let wire = borsh::to_vec(&object).expect("serializes");
    assert_eq!(wire[0], 90, "HeldLeafChallengeDeclared rides under tag 90");
    assert_eq!(borsh::from_slice::<PalwConsensusObjectV2>(&wire).expect("reads back"), object, "the challenge round-trips");
    Challenge { object, challenge: c, session, proof, chunks }
}

fn with(c: &Challenge, change: impl FnOnce(&mut PalwHeldLeafChallengeV1)) -> PalwConsensusObjectV2 {
    let mut x = c.challenge.clone();
    change(&mut x);
    PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge: Box::new(x) }
}

fn chunk_object(c: &Challenge, side: PalwCourtSideV1, index: usize) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::CourtCloseChunk { session_id: c.session, side, index: index as u8, bytes: c.chunks[index].clone() }
}

/// **The acceptance layer's adjudication of the assembled proof** at the session's leaf — the court's own function,
/// at the state the session stands in (what refuses a completing chunk whose close does not adjudicate).
fn adjudicates(env: &Env, c: &Challenge) -> Result<PalwCourtVerdictV2, String> {
    let court = bundle(&env.chain.p).court;
    adjudicate_court_close_v3(&env.chain.s, &c.session, &c.proof, &court, court.max_step_leaf_count(), FORM, true, false, None)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------------

#[test]
fn a_challenge_opens_the_session_at_the_leaf_and_writes_the_challengers_group() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, Some(0));
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof, 3);
    assert_eq!(palw_held_leaf_challenge_shape_v1(&ch.challenge), Ok(()));
    let accuser_before = collateral(&env, ACCUSER);
    let at = next(&env);
    step_held(&mut env, at, &[ch.object.clone()]);
    let s = &env.chain.s;
    let session = s.court_session(&ch.session).expect("the session opened under the id the challenge names");
    assert_eq!(session.challenger_bond, bond_key(ACCUSER));
    assert_eq!(session.claim, claim.id);
    assert_eq!(session.ladder.terminal_index(), Some(leaf), "the ladder is narrowed to the named leaf");
    assert_eq!(session.ladder.turn(), PalwBisectTurnV1::Terminal);
    let group = s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).expect("the challenger-side group");
    assert_eq!(
        (group.declarer, group.count, group.verdict, group.declared_daa),
        (bond_key(ACCUSER), 3, PalwCourtVerdictV2::ExecutorGuilty, at)
    );
    assert_eq!(group.assembly_deadline_daa, at + palw_close_assembly_daa_v1(3), "4 DAA a chunk from the block that carried it");
    assert_eq!(group.deposit, palw_close_assembly_deposit_v1(3), "the deposit is the declaration's");
    assert_eq!(group.chunk_digests, ch.challenge.chunk_digests);
    assert_eq!(group.close_digest, ch.challenge.close_digest);
    assert!(group.chunks.is_empty() && !group.has(0), "nothing has arrived");
    assert!(s.court_close_group(&ch.session, PalwCourtSideV1::Executor).is_none(), "the executor declared nothing");
    assert!(!env.chain.claim(&claim.id).phase.is_terminal(), "a challenge does not touch the claim");
    assert_eq!(collateral(&env, ACCUSER), accuser_before, "the deposit is a charge, collected only if the group ends undelivered");
    // Past `palw_rcore_plus` the stake is the session itself (nothing is written to `reserved_exposure`): the
    // accuser ledger the gate reads counts the claim's `reserved` through the challenger index.
    assert!(
        kaspa_consensus_core::palw_state_v2::palw_accuser_exposure_v1(&env.chain.s, &bond_key(ACCUSER)) > 0,
        "the accuser stakes what a losing challenger pays, through the session"
    );
}

#[test]
fn the_chunks_deliver_the_close_in_any_order_and_the_lying_claim_is_convicted() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, Some(0));
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof, 3);
    let at = next(&env);
    step_held(&mut env, at, &[ch.object.clone()]);
    // What the acceptance layer would adjudicate the assembled proof to, at the session's leaf.
    assert_eq!(adjudicates(&env, &ch), Ok(PalwCourtVerdictV2::ExecutorGuilty), "the close convicts the lie at the named leaf");
    let (executor_before, accuser_before) = (collateral(&env, EXECUTOR), collateral(&env, ACCUSER));
    // Chunks arrive out of order; the group is whole only at the last.
    for (k, index) in [2usize, 0, 1].into_iter().enumerate() {
        let at = next(&env);
        step_held(&mut env, at, &[chunk_object(&ch, PalwCourtSideV1::Challenger, index)]);
        let standing = env.chain.s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).is_some();
        assert_eq!(standing, k < 2, "the group stands until the completing chunk assembles it (chunk {k})");
    }
    let s = &env.chain.s;
    assert!(s.court_session(&ch.session).is_none(), "the close ended the session");
    assert!(s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).is_none(), "and its group with it");
    let claim = env.chain.claim(&claim.id);
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{:?}", claim.phase);
    assert!(collateral(&env, EXECUTOR) < executor_before, "the executor is charged");
    assert_eq!(
        collateral(&env, ACCUSER),
        accuser_before,
        "the accuser's deposit is refunded by never being taken: the group delivered"
    );
}

#[test]
fn a_close_of_another_leaf_does_not_adjudicate_at_the_named_one() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, Some(0));
    // The lying tile is tile 0; the challenger pins a close of tile 1 (honest) but names tile 0's leaf, or the reverse.
    let (proof_1, leaf_1) = output_close(&env, &claim, 1);
    let (_, leaf_0) = output_close(&env, &claim, 0);
    assert_ne!(leaf_0, leaf_1);
    let ch = challenge(&env, claim.id, ACCUSER, leaf_0, proof_1, 2);
    let at = next(&env);
    step_held(&mut env, at, &[ch.object.clone()]);
    let refused = adjudicates(&env, &ch);
    assert!(refused.is_err(), "a close is a move IN this session, at the leaf it narrowed to: {refused:?}");
}

#[test]
fn a_close_that_never_comes_convicts_its_declarer_and_leaves_the_claim_alone() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, None);
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof, 2);
    let at = next(&env);
    step_held(&mut env, at, &[ch.object.clone()]);
    let deadline = env.chain.s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).expect("the group").assembly_deadline_daa;
    // One chunk arrives; the other never does.
    let at = next(&env);
    step_held(&mut env, at, &[chunk_object(&ch, PalwCourtSideV1::Challenger, 0)]);
    let (accuser_before, executor_before) = (collateral(&env, ACCUSER), collateral(&env, EXECUTOR));
    // Inside the window the session stands and nobody is charged.
    step_held(&mut env, deadline, &[]);
    assert!(env.chain.s.court_session(&ch.session).is_some(), "the group's own window is still open at its deadline");
    // Past the deadline the sweep convicts the declarer.
    step_held(&mut env, deadline + 1, &[]);
    let s = &env.chain.s;
    assert!(s.court_session(&ch.session).is_none(), "the session ended");
    assert!(s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).is_none(), "and its group");
    assert!(!env.chain.claim(&claim.id).phase.is_terminal(), "the claim is not convicted, voided or slashed");
    assert_eq!(collateral(&env, EXECUTOR), executor_before, "the executor pays nothing");
    let charged = accuser_before - collateral(&env, ACCUSER);
    assert!(
        charged >= palw_close_assembly_deposit_v1(2) as u64,
        "the accuser forfeits its deposit ({charged}) and what a lost held dissection costs"
    );
}

#[test]
fn an_executors_own_acquitting_close_may_stand_beside_the_challengers_group_and_the_first_to_complete_wins() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, None);
    // The challenger (a false accuser) pins a close of the honest tile; the executor, honest, pins its own acquittal.
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof.clone(), 2);
    let at = next(&env);
    step_held(&mut env, at, &[ch.object.clone()]);
    // The executor's acquitting close, declared in its own group (the unfused terminal is the turn it may declare at).
    let acquit = PalwConsensusObjectV2::CourtClosed { session_id: ch.session, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof };
    let bytes = borsh::to_vec(&acquit).unwrap();
    let chunks = split(&bytes, 2);
    let declared = PalwConsensusObjectV2::CourtCloseDeclared {
        session_id: ch.session,
        side: PalwCourtSideV1::Executor,
        count: 2,
        chunk_digests: chunks.iter().map(|b| palw_court_close_chunk_digest_v1(b)).collect(),
        close_digest: palw_court_close_chunk_digest_v1(&bytes),
        verdict: PalwCourtVerdictV2::ChallengerDefeated,
        signature: vec![1; 8],
    };
    let at = next(&env);
    step_held(&mut env, at, &[declared]);
    assert!(env.chain.s.court_close_group(&ch.session, PalwCourtSideV1::Executor).is_some());
    assert!(env.chain.s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).is_some(), "both groups stand");
    let accuser_before = collateral(&env, ACCUSER);
    let executor_before = collateral(&env, EXECUTOR);
    for (index, bytes) in chunks.iter().enumerate() {
        let at = next(&env);
        step_held(
            &mut env,
            at,
            &[PalwConsensusObjectV2::CourtCloseChunk {
                session_id: ch.session,
                side: PalwCourtSideV1::Executor,
                index: index as u8,
                bytes: bytes.clone(),
            }],
        );
    }
    let s = &env.chain.s;
    assert!(s.court_session(&ch.session).is_none(), "the executor's close completed first and ended the session");
    assert!(
        s.court_close_group(&ch.session, PalwCourtSideV1::Challenger).is_none(),
        "the challenger's undelivered group went with it"
    );
    assert!(!env.chain.claim(&claim.id).phase.is_terminal(), "an acquittal leaves the claim standing");
    assert_eq!(collateral(&env, EXECUTOR), executor_before, "the executor's delivered group costs it nothing");
    let charged = accuser_before - collateral(&env, ACCUSER);
    assert!(
        charged >= palw_close_assembly_deposit_v1(2) as u64,
        "the losing challenger forfeits its undelivered group's deposit and is charged ({charged})"
    );
}

#[test]
fn the_challenge_is_refused_by_name_where_it_is_not_this_courts() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, Some(0));
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof.clone(), 3);
    let at = next(&env);
    let parent = env.chain.s.clone();
    let fold = |objects: &[PalwConsensusObjectV2], held: Option<u64>, at: u64| fold_held(&env, &parent, at, objects, held).map(|_| ());
    assert!(fold(&[ch.object.clone()], Some(HELD), at).is_ok(), "the control folds");
    // Off the held regime the ladder reaches the leaf; below the fence (a ruleset that arms it later) the lock refuses.
    assert!(
        matches!(fold(&[ch.object.clone()], None, at), Err(PalwStateV2Error::HeldLeafChallengeRefused(_))),
        "a chain that plays bisection"
    );
    let mut early = env_with(armed_with_chunks_at(AT + 1_000));
    let early_claim = put_claim(&mut early, 1, Some(0));
    let (early_proof, early_leaf) = output_close(&early, &early_claim, 0);
    let early_ch = challenge(&early, early_claim.id, ACCUSER, early_leaf, early_proof, 2);
    let early_parent = early.chain.s.clone();
    let below = fold_held(&early, &early_parent, next(&early), &[early_ch.object.clone()], Some(HELD)).map(|_| ());
    assert!(matches!(below, Err(PalwStateV2Error::HeldLeafChallengeRefused(_))), "below palw_held_close_chunks_v1: {below:?}");
    // Another claim; the roots and the executor are the claim's; a producer does not challenge its own claim.
    assert!(
        matches!(fold(&[with(&ch, |c| c.claim = h(0xDEAD))], Some(HELD), at), Err(PalwStateV2Error::MissingClaim(c)) if c == h(0xDEAD))
    );
    assert!(matches!(
        fold(&[with(&ch, |c| c.execution_root = h(1))], Some(HELD), at),
        Err(PalwStateV2Error::ShardCourtRootsDiffer(_))
    ));
    assert!(matches!(fold(&[with(&ch, |c| c.trace_root = h(1))], Some(HELD), at), Err(PalwStateV2Error::ShardCourtRootsDiffer(_))));
    assert!(matches!(
        fold(&[with(&ch, |c| c.executor_bond = bond_key(ACCUSER))], Some(HELD), at),
        Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(_))
    ));
    assert!(matches!(
        fold(&[with(&ch, |c| c.accuser_bond = bond_key(EXECUTOR))], Some(HELD), at),
        Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(_))
    ));
    assert!(matches!(fold(&[with(&ch, |c| c.accuser_bond = bond_key(77))], Some(HELD), at), Err(PalwStateV2Error::MissingBond(_))));
    // The leaf is inside the claim's step space: its priced leaf count.
    let work = claim.execution.space.leaf_count();
    assert!(fold(&[with(&ch, |c| c.leaf_index = work - 1)], Some(HELD), at).is_ok(), "the last leaf is a leaf");
    assert!(
        matches!(fold(&[with(&ch, |c| c.leaf_index = work)], Some(HELD), at), Err(PalwStateV2Error::HeldLeafChallengeRefused(_))),
        "one past the last"
    );
    // The declaration: between one chunk and the structural bound, one digest per chunk.
    assert!(matches!(fold(&[with(&ch, |c| c.count = 0)], Some(HELD), at), Err(PalwStateV2Error::CourtCloseCountOutOfRange { .. })));
    assert!(matches!(fold(&[with(&ch, |c| c.count = 33)], Some(HELD), at), Err(PalwStateV2Error::CourtCloseCountOutOfRange { .. })));
    assert!(matches!(
        fold(
            &[with(&ch, |c| {
                c.chunk_digests.pop();
            })],
            Some(HELD),
            at
        ),
        Err(PalwStateV2Error::CourtCloseDigestsIncoherent { .. })
    ));
    // A close that cannot assemble inside the session's backstop is refused where the accuser can still make a smaller one.
    let window = env.chain.sp.window_court();
    let too_many = (window / 4 + 1).min(32) as u8;
    if u64::from(too_many) * 4 > window {
        let digests = vec![h(9); too_many as usize];
        assert!(matches!(
            fold(
                &[with(&ch, |c| {
                    c.count = too_many;
                    c.chunk_digests = digests.clone();
                })],
                Some(HELD),
                at
            ),
            Err(PalwStateV2Error::CourtCloseCannotAssemble { .. })
        ));
    }
    // Once opened: a second challenge by the same accuser is a duplicate session; a bond that is no seat may not add one.
    step_held(&mut env, at, &[ch.object.clone()]);
    let parent = env.chain.s.clone();
    let at = next(&env);
    let again = fold_held(&env, &parent, at, &[ch.object.clone()], Some(HELD)).map(|_| ());
    assert!(
        matches!(
            again,
            Err(PalwStateV2Error::HeldDissectionFurtherSessionRefused { .. }) | Err(PalwStateV2Error::DuplicateSession(_))
        ),
        "{again:?}"
    );
    let other = challenge(&env, claim.id, OTHER, leaf, proof, 2);
    let from_a_non_seat = fold_held(&env, &parent, at, &[other.object.clone()], Some(HELD)).map(|_| ());
    assert!(
        matches!(
            from_a_non_seat,
            Err(PalwStateV2Error::HeldDissectionFurtherSessionRefused { .. }) | Err(PalwStateV2Error::MissingBond(_))
        ),
        "{from_a_non_seat:?}"
    );
    // A terminal claim takes no challenge: convict it (the one-move way would too), then challenge again.
    let mut done = env;
    let (p2, l2) = output_close(&done, &claim, 0);
    let ch2 = challenge(&done, claim.id, ACCUSER, l2, p2, 2);
    for index in 0..ch.chunks.len() {
        let at = next(&done);
        step_held(&mut done, at, &[chunk_object(&ch, PalwCourtSideV1::Challenger, index)]);
    }
    assert!(done.chain.claim(&claim.id).phase.is_terminal(), "the first challenge convicted the claim");
    let parent = done.chain.s.clone();
    let after = fold_held(&done, &parent, next(&done), &[ch2.object.clone()], Some(HELD)).map(|_| ());
    assert!(matches!(after, Err(PalwStateV2Error::WrongPhase { .. }) | Err(PalwStateV2Error::MissingClaim(_))), "{after:?}");
}

#[test]
fn the_shape_and_the_digest_are_what_the_acceptance_layer_reads() {
    let mut env = env();
    let claim = put_claim(&mut env, 1, Some(0));
    let (proof, leaf) = output_close(&env, &claim, 0);
    let ch = challenge(&env, claim.id, ACCUSER, leaf, proof, 3);
    // The object rides at every height (the shape gate), is a declaration for pricing, and is dropped by name below the fence.
    assert_eq!(kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&ch.object), Ok(()));
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_held_close_chunks_v1(&ch.object));
    assert_eq!(
        kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(&ch.object),
        kaspa_consensus_core::palw_state_v2::palw_court_close_min_fee_v1(3),
        "a challenge buys one grading, as a declaration does"
    );
    let mut unsigned = ch.challenge.clone();
    unsigned.signature.clear();
    assert!(palw_held_leaf_challenge_shape_v1(&unsigned).is_err());
    let one = |c: &PalwHeldLeafChallengeV1| palw_held_leaf_challenge_digest_v1(b"net", c);
    assert_ne!(one(&ch.challenge), palw_held_leaf_challenge_digest_v1(b"other", &ch.challenge), "the network");
    let mut moved = ch.challenge.clone();
    moved.leaf_index += 1;
    assert_ne!(one(&ch.challenge), one(&moved), "the leaf is signed");
}
