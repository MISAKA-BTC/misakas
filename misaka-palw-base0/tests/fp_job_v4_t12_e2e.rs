//! **FP Job V4 end to end on a copy of testnet-12 with the decode-rules fence ARMED** (RFC-0001 §A;
//! ADR-0082 D10/D11; the audit D M-1 refusal lifted by what this build carries).
//!
//! testnet-12's shipped ruleset, with `palw_fp_decode_rules` armed at a low height through the
//! flag-day entry the integration will add to the flag day's list
//! (`PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1`), assembles (`validate_palw_v2`), and a V4 job on
//! the floor class goes the whole way:
//!
//! 1. **produce** — the floor's real engine runs the job (sampled, penalized, a stop sequence that
//!    ends the answer early) and the commitment is built from the run;
//! 2. **carry** — the payload passes the extraction walk at a height past the fence (and is skipped
//!    below it, while a V3 payload is skipped past it);
//! 3. **price** — the real fold opens the claim, credited its DECODE work only (D10): strictly less
//!    than the same commitment earns on the unarmed ruleset;
//! 4. **verify** — the floor's seats replay every interval of the claim under the claim's rule and
//!    check its stop, all `Valid`;
//! 5. **license** — a panel binds and a `Valid` quorum licenses the claim through the real fold.

mod common;
#[path = "../../consensus/core/tests/dos_l5_common.rs"]
mod l5;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1, Params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_decode_pipeline_v4::{DecodeConfigV4, decode_answer_stop_v4};
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
use kaspa_consensus_core::palw_fp_objects_v3::{
    PalwFpClassCapsV1, PalwFpDerivedWorkCapV1, palw_fp_objects_from_accepted_txs_by_class_v1,
};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3, PalwFpDecodeRulesV1,
    PalwFpStopReasonV3, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2, PalwClaimPhaseV2,
    PalwConsensusObjectV2, PalwPanelSeatV2, PalwStateCarriageV2, PalwStateV2Error, apply_palw_transition_v7,
};
use l5::*;

/// The fence's height on this copy — past genesis, below every post-launch flag day.
const FENCE: u64 = 110;

/// testnet-12's shipped ruleset with the decode rules armed at [`FENCE`] through the flag-day entry.
fn t12_with_v4() -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut p, Some(ForkActivation::new(FENCE)));
    p.validate_palw_v2().expect("testnet-12 with the decode rules armed assembles");
    assert!(p.palw_fp_decode_rules_active_at(FENCE) && !p.palw_fp_decode_rules_active_at(FENCE - 1));
    p
}

/// One block through the real fold, with the processor's t12 free-prompt extras.
fn fold_at(
    p: &Params,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let sp = bundle(p).state;
    let f = flags(p, ctx.daa_score);
    let mut e = extras(p, ctx.daa_score);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(ctx.daa_score));
    apply_palw_transition_v7(
        parent,
        &sp,
        None,
        ctx,
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .map(|(s, _, _)| s)
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// The floor's two on-chain prerequisites the live network writes permissionlessly (a
/// `FamilyCertified` drill and a registry boundary) — `dos_repro_1`'s, restated: a real state
/// object each, not a reimplementation.
fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let family = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0's fp family is pinned");
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    let work = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut c = PalwStateCarriageV2::from_state(s);
    c.fp_certified_families.insert(family.digest(), PalwCertifiedFamilyStateV2 { family, certified_daa: 0 });
    c.model_lifecycles.insert(floor, row);
    c.into_state_v3(&bundle(p).state, None, false, p.palw_canonical_work_daa()).expect("carriage rebuilds")
}

/// The chain up to the commitment: an executor bond, the floor's prerequisites, its graph published.
fn chain(p: &Params) -> (PalwChainStateV2, Hash64) {
    let floor = genesis_classes(p)[0].0;
    let g = genesis_state(p);
    let s = fold_at(p, &g, &ctx(1, 100, 1, 0), &[bond_obj(1, at_least_the_floor(p, 100_000_000_000))]).expect("the executor bonds");
    let s = seed_prereqs(p, &s);
    let publish =
        PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(p, &s, &ctx(2, 101, 2, 0), &[publish]).expect("the floor publishes its graph");
    (s, floor)
}

/// The accepted-transaction walk exactly as the processor runs it, at `daa` under `rules`.
fn walk(
    p: &Params,
    s: &PalwChainStateV2,
    payload: &PalwFpCommitmentTxPayloadV3,
    rules: PalwFpDecodeRulesV1,
) -> Vec<PalwConsensusObjectV2> {
    walk_with(p, s, payload, rules, false)
}

fn walk_with(
    p: &Params,
    s: &PalwChainStateV2,
    payload: &PalwFpCommitmentTxPayloadV3,
    rules: PalwFpDecodeRulesV1,
    prefix_state_armed: bool,
) -> Vec<PalwConsensusObjectV2> {
    walk_all(p, s, payload, rules, prefix_state_armed, false, false)
}

fn walk_all(
    p: &Params,
    s: &PalwChainStateV2,
    payload: &PalwFpCommitmentTxPayloadV3,
    rules: PalwFpDecodeRulesV1,
    prefix_state_armed: bool,
    constraint_armed: bool,
    constraint_v2_armed: bool,
) -> Vec<PalwConsensusObjectV2> {
    let tx = kaspa_consensus_core::tx::Transaction::new(
        0,
        vec![],
        vec![],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
        0,
        borsh::to_vec(payload).unwrap(),
    );
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let ladder = bundle(p).court.max_step_leaf_count();
    palw_fp_objects_from_accepted_txs_by_class_v1(
        &[tx],
        domain,
        &bundle(p).freeprompt,
        kaspa_consensus_core::BlockHash::default(),
        false,
        |class_id| PalwFpClassCapsV1 {
            step_ladder: s.class_step_ladder_v1(class_id, ladder),
            held: s.class_is_held_v1(class_id),
            derived_work: match s.fp_work_profile_of(class_id) {
                Some(profile) => PalwFpDerivedWorkCapV1::Derived(profile),
                None => PalwFpDerivedWorkCapV1::Unpublished,
            },
            logits_q24: s.class_commits_q24_logits_v1(class_id),
            prefix_state_armed,
            constraint_armed,
            constraint_v2_armed,
            tokenizer: kaspa_consensus_core::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::Dormant,
        },
        true,
        false,
        PalwPromptIdsFormV1::MerkleV1,
        rules,
        // The executor's key here is a fixture's; the signature is the door's other test.
        |_, _, _, _| true,
    )
    .objects
    .into_iter()
    .map(|carried| carried.object)
    .collect()
}

#[test]
fn a_v4_job_goes_produce_verify_license_on_testnet_12_with_the_decode_rules_armed() {
    let p = t12_with_v4();
    let (s, floor) = chain(&p);
    let backend = common::floor_backend(PalwPromptIdsFormV1::MerkleV1);
    assert_eq!(backend.profile().shape_profile_id(), floor, "the floor the chain registers is the engine's class");

    // ---- 1. produce --------------------------------------------------------------------------
    let prompt: Vec<usize> = vec![17, 3, 911];
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let limit = (backend.profile().n_ctx as usize - prompt.len()) as u32;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let v3 = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: domain,
        class_id: floor,
        executor_bond: bond_key(1).0,
        executor_pubkey: pubkey_of(1),
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(1)),
        anchor_block: h(0xA0),
        anchor_daa: 100,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            PalwPromptIdsFormV1::MerkleV1,
            &ids,
        )
        .expect("the ids commit"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: limit,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: [0x3C; 32],
        temperature_q: 1 << 24,
        decode: None,
        tail: None,
    };
    let controls =
        DecodeConfigV4 { repeat_penalty_q: 131_072, penalty_window: 8, presence_penalty_q: 1 << 23, ..DecodeConfigV4::NOOP };
    // A stop sequence the sampled, penalized answer itself completes early.
    let probe = backend.execute_free_prompt(&v3.clone().into_v4(controls.clone()), &prompt).expect("the probe run");
    assert!(probe.output_token_ids.len() >= 3, "the fixture decodes a few tokens");
    let stop = probe.output_token_ids[1..3].to_vec();
    let job = v3.clone().into_v4(DecodeConfigV4 { stop_sequences: vec![stop.clone()], ..controls });
    let run = backend.execute_free_prompt(&job, &prompt).expect("the V4 run");
    let decode = job.decode.as_ref().unwrap();
    let stopped =
        decode_answer_stop_v4(decode, limit, 1_024, &run.output_token_ids).expect("the answer ends where its stop rule ends it");
    assert!(stopped.executed < limit, "the stop ended the run early");
    assert_eq!(run.facts.stop_reason, PalwFpStopReasonV3::EndOfGeneration);
    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: floor,
        cu_ruleset_id: Hash64::default(),
    };
    let commitment = palw_fp_commitment_v3(&job, &class, &run, b"misaka-palw-rc", 9_999_999).expect("the run commits");
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment,
        prompt_token_ids: ids.clone(),
        signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let claim_id = payload.claim_id();

    // ---- 2. carry: one job version per side of the fence ---------------------------------------
    assert!(walk(&p, &s, &payload, PalwFpDecodeRulesV1::at(Some(FENCE), FENCE - 1)).is_empty(), "no V4 claim below the fence");
    let v3_payload = {
        let v3_run = backend
            .execute_free_prompt(&PalwFreePromptJobV3 { sampling_seed: [0; 32], temperature_q: 0, ..v3.clone() }, &prompt)
            .unwrap();
        let v3_job = PalwFreePromptJobV3 { sampling_seed: [0; 32], temperature_q: 0, ..v3.clone() };
        PalwFpCommitmentTxPayloadV3 {
            commitment: palw_fp_commitment_v3(&v3_job, &class, &v3_run, b"misaka-palw-rc", 9_999_999).unwrap(),
            ..payload.clone()
        }
    };
    assert!(walk(&p, &s, &v3_payload, PalwFpDecodeRulesV1::at(Some(FENCE), FENCE)).is_empty(), "no NEW V3 claim past the fence");
    let objects = walk(&p, &s, &payload, PalwFpDecodeRulesV1::at(Some(FENCE), 112));
    assert_eq!(objects.len(), 1, "the V4 carrier opens a claim past the fence");

    // ---- 3. price: D10 credits the decode work only --------------------------------------------
    let s = fold_at(&p, &s, &ctx(3, 112, 3, 0), &objects).expect("the real fold opens the V4 claim");
    let claim = s.claim(&claim_id).expect("the claim exists").clone();
    let unarmed = palw_t12_shipped_params();
    let (s_unarmed, _) = chain(&unarmed);
    let s_unarmed = fold_at(&unarmed, &s_unarmed, &ctx(3, 112, 3, 0), &objects).expect("the unarmed fold prices it too");
    let whole = s_unarmed.claim(&claim_id).expect("priced unarmed").pwu;
    assert!(claim.pwu < whole, "D10: the prefill is priced at zero ({} < {whole})", claim.pwu);

    // ---- 4. verify: every interval under the claim's rule, and the stop -------------------------
    let roots = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = backend.fp_interval_count_for(job.prompt_tokens, run.facts.decode_tokens_executed).expect("intervals");
    let mut verdicts = Vec::new();
    for index in 0..count {
        let opening = backend.open_fp_interval(&run.outcome.material, index, &ids).expect("the executor opens it");
        backend.fp_forget_seat_state_v1();
        if let Some((_, covered, _)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
            let ctx = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).unwrap().binding.job_context;
            backend.checkpoint_root_for_context_v1(&ctx, &ids, &run.output_token_ids, covered).expect("the seat recomputes");
        }
        verdicts.push(backend.verify_fp_interval_opening_under_job_v1(
            &opening,
            roots,
            index,
            &ids,
            run.facts.step_leaf_count,
            &job,
            &run.output_token_ids,
        ));
    }
    assert!(verdicts.iter().all(|v| *v == PalwFpIntervalVerdictV1::Valid), "{verdicts:?}");

    // ---- 5. license: a panel binds and a Valid quorum licenses ---------------------------------
    let bonds = genesis_bonds(&p);
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s = fold_at(&p, &s, &ctx(4, 113, 4, 0), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0xA1), seats }])
        .expect("the panel binds");
    let receipts: Vec<PalwSeatReceiptV2> = bonds[0..3]
        .iter()
        .map(|(k, _, _)| PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: *k,
            signed_daa: 114,
            signature: Vec::new(),
        })
        .collect();
    let s = fold_at(&p, &s, &ctx(5, 114, 5, 0), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }])
        .expect("the Valid quorum licenses");
    let phase = &s.claim(&claim_id).expect("still live").phase;
    assert!(matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed: {phase:?}");
}

/// **RFC-0001 §2.6 stage 2 end to end on a copy of testnet-12 with `palw_fp_prefix_state` ARMED** (a copy: no preset
/// carries it). A prefix-state claim (FP job version 11) produced by the floor's real engine is carried only past the
/// fence, is credited less than the same run as a V4 claim (the cached prefix is not paid), is replayed by the floor's seats
/// under V4 rules, and is licensed by a `Valid` quorum through the real fold.
#[test]
fn a_prefix_state_claim_is_carried_credited_less_replayed_and_licensed_with_its_fence_armed() {
    use kaspa_consensus_core::palw_fp_prefix_v1::{PALW_DRILL_FP_PREFIX_STATE_ENTRY, PALW_FP_PREFIX_VERSION, palw_fp_prefix_state_root_v1};
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpJobTailV1, PalwFpPrefixStateV1};

    const PREFIX_FENCE: u64 = 111;
    let mut p = t12_with_v4();
    (PALW_DRILL_FP_PREFIX_STATE_ENTRY.set)(&mut p, Some(ForkActivation::new(PREFIX_FENCE)));
    p.validate_palw_v2().expect("testnet-12 with the decode rules and the prefix state armed assembles");
    let (s, floor) = chain(&p);
    let backend = common::floor_backend(PalwPromptIdsFormV1::MerkleV1);
    let prompt: Vec<usize> = vec![17, 3, 911, 44, 5];
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let limit = 6u32;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let state = PalwFpPrefixStateV1 {
        state_root: palw_fp_prefix_state_root_v1(&floor, 3, &Hash64::from_u64_word(0xCAFE)),
        prefix_tokens: 3,
        class_id: floor,
    };
    let v4 = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: domain,
        class_id: floor,
        executor_bond: bond_key(1).0,
        executor_pubkey: pubkey_of(1),
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(1)),
        anchor_block: h(0xA0),
        anchor_daa: 100,
        job_nonce: [0x6B; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &ids)
            .expect("the ids commit"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: limit,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: [0x3C; 32],
        temperature_q: 1 << 24,
        decode: None,
        tail: None,
    }
    .into_v4(DecodeConfigV4::NOOP);
    let mut job = v4.clone();
    job.version = PALW_FP_PREFIX_VERSION;
    job.tail = Some(PalwFpJobTailV1::Prefix(state));
    assert!(job.is_prefix_state() && job.decodes_under_v4_rules());
    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: floor,
        cu_ruleset_id: Hash64::default(),
    };
    let sig = vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let mk = |j: &PalwFreePromptJobV3| {
        let run = backend.execute_free_prompt(j, &prompt).expect("the floor's real engine runs the job");
        let commitment = palw_fp_commitment_v3(j, &class, &run, b"misaka-palw-rc", 9_999_999).expect("the run commits");
        (PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids: ids.clone(), signature: sig.clone() }, run)
    };
    let (payload, run) = mk(&job);
    let (v4_payload, _) = mk(&v4);
    // The V11 job id is its own domain: the two claims are distinct objects.
    assert_ne!(payload.claim_id(), v4_payload.claim_id());
    assert_eq!(payload.consumed_prefix_state_v1(), state);
    let claim_id = payload.claim_id();

    // carry: not below the prefix fence (V4 still is), and not with the fence unarmed in the caps.
    let rules = |at| PalwFpDecodeRulesV1::at(Some(FENCE), at);
    assert!(walk_with(&p, &s, &payload, rules(PREFIX_FENCE - 1), false).is_empty(), "no prefix-state claim below the fence");
    assert!(walk_with(&p, &s, &payload, rules(PREFIX_FENCE), false).is_empty(), "and none where the ruleset does not arm it");
    assert_eq!(walk_with(&p, &s, &v4_payload, rules(PREFIX_FENCE - 1), false).len(), 1, "the V4 twin is carried either way");
    let objects = walk_with(&p, &s, &payload, rules(PREFIX_FENCE + 1), true);
    assert_eq!(objects.len(), 1, "armed, the prefix-state claim opens");

    // price: the cached prefix is not paid.
    let s2 = fold_at(&p, &s, &ctx(3, 112, 3, 0), &objects).expect("the real fold opens the prefix-state claim");
    let credited = s2.claim(&claim_id).expect("the claim exists").pwu;
    let twin_objects = walk_with(&p, &s, &v4_payload, rules(112), false);
    let s_twin = fold_at(&p, &s, &ctx(3, 112, 3, 0), &twin_objects).expect("the V4 twin opens");
    let decode_only = s_twin.claim(&v4_payload.claim_id()).expect("priced").pwu;
    // FINDING (stage 2 is credit-side only): with the decode rules armed D10 already credits the DECODE work only, so the
    // named prefix state changes no price on top of it — the claim is credited exactly the V4 twin, never more.
    assert_eq!(credited, decode_only, "armed D10 already prices the prefill at zero; stage 2 adds no credit and takes none");
    let unarmed = palw_t12_shipped_params();
    let (s_unarmed, _) = chain(&unarmed);
    let s_unarmed = fold_at(&unarmed, &s_unarmed, &ctx(3, 112, 3, 0), &twin_objects).expect("the unarmed fold prices the twin too");
    let whole = s_unarmed.claim(&v4_payload.claim_id()).expect("priced unarmed").pwu;
    assert!(credited < whole, "the cached prefix is not paid: {credited} < {whole} (prefill + decode, unarmed)");

    // verify: the seats replay every interval under V4 rules (the job's decode rules, version 11 notwithstanding).
    let roots = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = backend.fp_interval_count_for(job.prompt_tokens, run.facts.decode_tokens_executed).expect("intervals");
    for index in 0..count {
        let opening = backend.open_fp_interval(&run.outcome.material, index, &ids).expect("the executor opens it");
        backend.fp_forget_seat_state_v1();
        if let Some((_, covered, _)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
            let c = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).unwrap().binding.job_context;
            backend.checkpoint_root_for_context_v1(&c, &ids, &run.output_token_ids, covered).expect("the seat recomputes");
        }
        let v = backend.verify_fp_interval_opening_under_job_v1(&opening, roots, index, &ids, run.facts.step_leaf_count, &job, &run.output_token_ids);
        assert_eq!(v, PalwFpIntervalVerdictV1::Valid, "interval {index}");
    }

    // license.
    let bonds = genesis_bonds(&p);
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s3 = fold_at(&p, &s2, &ctx(4, 113, 4, 0), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0xA1), seats }])
        .expect("the panel binds");
    let receipts: Vec<PalwSeatReceiptV2> = bonds[0..3]
        .iter()
        .map(|(k, _, _)| PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 114, signature: Vec::new() })
        .collect();
    let s4 = fold_at(&p, &s3, &ctx(5, 114, 5, 0), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }])
        .expect("the Valid quorum licenses");
    assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
}

/// **ADR-0096 Decisions 6–8 end to end on a copy of testnet-12 with `palw_fp_decode_constraint` (and the second form's
/// `palw_fp_constraint_v2`) ARMED**: a JSON-Schema constraint compiled by `misaka-palw-constraint`, the job at version 6, the
/// floor's real engine masked by the class's token table, carried only past the fence, replayed by the floor's seats through
/// the same mask, licensed by a `Valid` quorum — and a seat without the table, a wrong table, and a lying answer refused.
#[test]
fn a_constrained_claim_is_masked_carried_replayed_and_licensed_with_its_fences_armed() {
    use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_TEMPERATURE_GREEDY};
    use kaspa_consensus_core::palw_fp_constraint_job_v1::{
        PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY, PALW_FP_CONSTRAINT_VERSION, PalwConstraintMaskV1, PalwTokenTableV1,
        palw_fp_with_constraint_scope_v1,
    };
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpJobTailV1, fp_job_id_v3};
    use std::sync::Arc;

    const CONSTRAINT_FENCE: u64 = 111;
    const SECOND_FORM_FENCE: u64 = 113;
    let mut p = t12_with_v4();
    (PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY.set)(&mut p, Some(ForkActivation::new(CONSTRAINT_FENCE)));
    (kaspa_consensus_core::palw_fp_constraint_v2::PALW_DRILL_FP_CONSTRAINT_V2_ENTRY.set)(&mut p, Some(ForkActivation::new(SECOND_FORM_FENCE)));
    p.validate_palw_v2().expect("testnet-12 with the constraint fences armed assembles (the refusal is lifted)");
    let (s, floor) = chain(&p);
    let backend = common::floor_backend(PalwPromptIdsFormV1::MerkleV1);
    let vocab = backend.profile().vocab_size;

    // The class's token table: every id renders one printable byte; the top id is end-of-generation.
    let mut table = PalwTokenTableV1 {
        entries: (0..vocab).map(|i| Some(vec![0x20 + (i % 95) as u8])).collect(),
        eog_token_ids: vec![vocab - 1],
        tokenizer_id: Hash64::default(),
    };
    // A table with no tokenizer file behind it names its own digest as the tokenizer it was derived under.
    table.tokenizer_id = table.digest();
    let table = Arc::new(table);
    // The constraint: a JSON string that is "abc" or "cab".
    let schema = misaka_palw_constraint::schema::parse(&serde_json::json!({ "type": "string", "enum": ["abc", "cab"] })).expect("in the subset");
    let automaton = misaka_palw_constraint::compile::compile_v1(&schema).expect("compiles");
    let bytes = automaton.to_bytes();

    let prompt: Vec<usize> = vec![17, 3, 91, 4];
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let limit = 8u32;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_CONSTRAINT_VERSION,
        network_domain: domain,
        class_id: floor,
        executor_bond: bond_key(1).0,
        executor_pubkey: pubkey_of(1),
        operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(1)),
        anchor_block: h(0xA0),
        anchor_daa: 100,
        job_nonce: [0x7C; 32],
        tokenizer_id: table.tokenizer_id,
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &ids)
            .expect("the ids commit"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: limit,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: PALW_DECODE_SEED_GREEDY,
        temperature_q: PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: Some(PalwFpJobTailV1::Constraint(bytes.clone())),
    };
    assert!(job.is_constraint() && !job.decodes_under_v4_rules());
    let mask = PalwConstraintMaskV1::for_job(&job, table.clone()).expect("the host's table is the job's");
    let wrong_table = Arc::new(PalwTokenTableV1 {
        entries: table.entries.iter().rev().cloned().collect(),
        eog_token_ids: vec![vocab - 1],
        tokenizer_id: Hash64::from_u64_word(0xBAD),
    });
    assert!(PalwConstraintMaskV1::for_job(&job, wrong_table).is_err(), "a table that is not the job's tokenizer_id is refused");

    // ---- produce: only with the table ---------------------------------------------------------
    let refused = match backend.execute_free_prompt(&job, &prompt) {
        Err(why) => why,
        Ok(_) => panic!("a host without the table cannot run a constrained job"),
    };
    assert!(refused.contains("token table"), "{refused}");
    let run = palw_fp_with_constraint_scope_v1(Some(mask.clone()), || backend.execute_free_prompt(&job, &prompt)).expect("the masked run");
    let rendered: Vec<u8> =
        run.output_token_ids.iter().take_while(|id| **id != vocab - 1).map(|id| table.entries[*id as usize].clone().unwrap()[0]).collect();
    assert!(rendered == b"\"abc\"" || rendered == b"\"cab\"", "the committed answer is in the schema: {:?}", String::from_utf8_lossy(&rendered));
    assert_eq!(*run.output_token_ids.last().unwrap(), vocab - 1, "the stop rule commits the lowest end-of-generation id");
    assert_eq!(run.output_token_ids.len(), rendered.len() + 1);
    assert!(run.facts.decode_tokens_executed < limit && run.facts.stop_reason == PalwFpStopReasonV3::EndOfGeneration);
    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: floor,
        cu_ruleset_id: Hash64::default(),
    };
    let commitment = palw_fp_commitment_v3(&job, &class, &run, b"misaka-palw-rc", 9_999_999).expect("the run commits");
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment,
        prompt_token_ids: ids.clone(),
        signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let claim_id = payload.claim_id();
    assert_eq!(borsh::from_slice::<PalwFpCommitmentTxPayloadV3>(&borsh::to_vec(&payload).unwrap()).unwrap(), payload, "the wire round-trips");
    // The job id is its own domain: the same fields as a V3 job are another claim.
    let plain = PalwFreePromptJobV3 { version: PALW_FP_V3_VERSION, tail: None, ..job.clone() };
    assert_ne!(fp_job_id_v3(&job), fp_job_id_v3(&plain));

    // ---- carry: not below the fence, not unarmed, and the second form only past its own ------------
    let rules = |at| PalwFpDecodeRulesV1::at(Some(FENCE), at);
    assert!(walk_all(&p, &s, &payload, rules(CONSTRAINT_FENCE - 1), false, false, false).is_empty(), "no constrained claim below the fence");
    let objects = walk_all(&p, &s, &payload, rules(CONSTRAINT_FENCE + 1), false, true, false);
    assert_eq!(objects.len(), 1, "armed, the constrained claim opens (a first-form constraint needs no second fence)");
    // A second-form constraint (a union with a `$ref`) is carried only where `palw_fp_constraint_v2` is.
    let second = {
        let schema = misaka_palw_constraint::schema::parse_v2(&serde_json::json!({
            "$defs": { "p": { "const": "p" } },
            "anyOf": [
                { "type": "object", "properties": { "kind": { "$ref": "#/$defs/p" } }, "required": ["kind"], "additionalProperties": false },
                { "type": "object", "properties": { "kind": { "const": "q" } }, "required": ["kind"], "additionalProperties": false }
            ]
        }))
        .expect("the second subset");
        misaka_palw_constraint::compile_v2::compile_v2(&schema).expect("compiles").to_bytes()
    };
    let second_payload = {
        let mut j = job.clone();
        j.tail = Some(PalwFpJobTailV1::Constraint(second));
        let mut pl = payload.clone();
        pl.commitment.job = j;
        pl
    };
    assert!(walk_all(&p, &s, &second_payload, rules(SECOND_FORM_FENCE - 1), false, true, false).is_empty(), "the second form below its fence");
    assert_eq!(walk_all(&p, &s, &second_payload, rules(SECOND_FORM_FENCE), false, true, true).len(), 1, "and from it");

    // ---- price and fold -----------------------------------------------------------------------
    let s2 = fold_at(&p, &s, &ctx(3, 112, 3, 0), &objects).expect("the real fold opens the constrained claim");
    assert!(s2.claim(&claim_id).is_some(), "the claim exists");

    // ---- verify: every interval through the same mask, and a lying answer is a Fault -----------------
    let roots = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: fp_job_id_v3(&job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = backend.fp_interval_count_for(job.prompt_tokens, run.facts.decode_tokens_executed).expect("intervals");
    let verify = |scope: Option<Arc<PalwConstraintMaskV1>>, answer: &[u32], index: u32| {
        let opening = backend.open_fp_interval(&run.outcome.material, index, &ids).expect("the executor opens it");
        backend.fp_forget_seat_state_v1();
        if let Some((_, covered, _)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
            let c = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).unwrap().binding.job_context;
            backend.checkpoint_root_for_context_v1(&c, &ids, &run.output_token_ids, covered).expect("the seat recomputes");
        }
        palw_fp_with_constraint_scope_v1(scope, || {
            backend.verify_fp_interval_opening_under_job_v1(&opening, roots, index, &ids, run.facts.step_leaf_count, &job, answer)
        })
    };
    for index in 0..count {
        assert_eq!(verify(Some(mask.clone()), &run.output_token_ids, index), PalwFpIntervalVerdictV1::Valid, "interval {index}");
    }

    // A committed answer that is not what the mask selects does not replay (the first id swapped for one the schema forbids).
    let mut lying = run.output_token_ids.clone();
    lying[1] = (lying[1] + 1) % (vocab - 1);
    let verdicts: Vec<_> = (0..count).map(|i| verify(Some(mask.clone()), &lying, i)).collect();
    assert!(verdicts.iter().any(|v| *v != PalwFpIntervalVerdictV1::Valid), "a lying answer replays as Valid everywhere: {verdicts:?}");

    // ---- license ------------------------------------------------------------------------------
    let bonds = genesis_bonds(&p);
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s3 = fold_at(&p, &s2, &ctx(4, 113, 4, 0), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0xA1), seats }])
        .expect("the panel binds");
    let receipts: Vec<PalwSeatReceiptV2> = bonds[0..3]
        .iter()
        .map(|(k, _, _)| PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 114, signature: Vec::new() })
        .collect();
    let s4 = fold_at(&p, &s3, &ctx(5, 114, 5, 0), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }])
        .expect("the Valid quorum licenses");
    assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
}
