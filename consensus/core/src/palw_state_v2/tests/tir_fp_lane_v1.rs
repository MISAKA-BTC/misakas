//! **RFC-0002 Phase H: the IR class Free Prompt lane, through the production fold** (`apply_palw_transition_v2_with_extras`, the
//! transition the processor folds a chain block with — no `cfg(test)` seam on the acceptance path).
//!
//! The commitment is a REAL run of the tiny IR program under its own free-prompt context, assembled by the producer's
//! `palw_fp_commitment_from_context_v3`; its `TFP1` frame carries the producer's ML-DSA-87 source pledge (verified by the fold with the
//! production verifier) over the plan the producer's pre-signing quote (`palw_tir_public_original_quote_v1`) returned. Every fence
//! the lane reads is armed at DAA 0 (derived work, canonical work, the TIR public frame). Not covered here: the carrier
//! transaction's fee and burn, the mempool, the template and the processor's extraction walk (the node E2E's).
use super::*;
use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3, palw_fp_job_pin_v1};
use crate::palw_freeprompt_v3::{PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3};
use crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1;

const NET: &[u8] = b"testnet-12";
const PRODUCER: u64 = 0x1F0;
const AT_DAA: u64 = 102;

struct Lane {
    p: PalwStateParamsV2,
    state: PalwChainStateV2,
    class_id: Hash64,
    claim: Hash64,
    object: PalwConsensusObjectV2,
    envelope: PalwTirPublicFpEnvelopeV1,
    /// Past the fence, with the TFP1 pairing.
    extras: PalwTransitionExtrasV1,
    leaves: u64,
}

fn at() -> PalwBlockContextV2 {
    ctx(3, AT_DAA, 3)
}

/// The world: the derived-work chain (its floor, the compute era's basis), the tiny IR class registered with its published layout,
/// one family certified for the attempt lane covering every primitive its program reaches, and a producer bond with a real ML-DSA-87
/// key. The commitment, its frame and the paired extras, built the way the producer builds them.
fn lane(certify: bool) -> Lane {
    use crate::palw_e2e_adjudicability::{PalwE2eCoveringV1, PalwE2eFamilyV1, palw_e2e_family_id_v1};
    use crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1;
    let (p, base, _, _) = derived_work_chain();
    let mut state = base;
    // The floor's registry row (the network quantum is one `quanta_per_canonical_job`-th of its draw, ADR-0148). Inserted directly,
    // as the compute-pricing tests do; its draw is chosen so the tiny program's decode positions earn whole quanta (the network's
    // floor is a fixture parameter here, not a measured value).
    state.model_lifecycles.insert(
        h64(1),
        crate::palw_model_registry_v1::PalwModelLifecycleRowV1 {
            state: crate::palw_model_registry_v1::PalwModelLifecycleV1::Active,
            work: crate::palw_model_registry_v1::PalwModelWorkV1 {
                verification_ccu: 8,
                economic_ccu_per_claim: 8,
                ops_supported: true,
                ..Default::default()
            },
            profile: Default::default(),
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
        },
    );
    let tiny = crate::palw_tir_court_v1::test_support::tiny_execution(None);
    let class_id = tiny.binding.class.class_id(&tiny.binding.artifact_root);
    let (record, _) =
        crate::palw_tir_admission_v1::palw_tir_class_record_v1(&tiny.binding.class, &tiny.binding.artifact_root).expect("the record");
    state.set_class_for_tests(
        class_id,
        PalwClassStateV2 {
            artifact_root: tiny.binding.artifact_root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(8_000),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    state.tir_classes.insert(class_id, record.clone());
    state.staged_courts.insert(
        crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirLayout(class_id),
        crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirLayout(Box::new(tiny.binding.class.layout.clone())),
    );
    if certify {
        let program = tiny.binding.class.decode_program().expect("canonical");
        let family = PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("TEST-TIR-ATTEMPT"),
            drilled_class_id: class_id,
            kernel_ids: crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program),
            covering: PalwE2eCoveringV1 { convicted_leaves: 6, ..Default::default() },
        };
        let delta = PalwStateDeltaV2 {
            point: ctx(2, 101, 2),
            entries: vec![PalwDeltaEntryV2::CertifiedFamily {
                lane: PalwCertifiedLaneV1::Attempt,
                key: family.digest(),
                old: None,
                new: Some(PalwCertifiedFamilyStateV2 { family, certified_daa: 101 }),
            }],
        };
        state = apply_delta_v2(&state, &delta, &p).expect("the family delta applies");
    }
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x1F; 32]);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let producer = bond_key(PRODUCER);
    state.bonds.insert(
        producer,
        PalwBondStateV2 {
            pubkey: pubkey.clone(),
            operator_id: op_id(0x1F0),
            collateral: 1_000_000_000_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(0x9A1F),
            capable_classes: Default::default(),
        },
    );
    let sign = |message: Hash64, context: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), context, [0x1F; 32]).unwrap().as_ref().to_vec()
    };

    // ---- the producer: the job, its FP context, the run IN that context, the commitment assembled from the run ----
    let (_, _, mut job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
    job.version = PALW_FP_V3_VERSION;
    job.decode = None;
    job.tail = None;
    job.class_id = class_id;
    job.executor_bond = producer.0;
    job.executor_pubkey = pubkey.clone();
    job.tokenizer_id = tiny.binding.class.tokenizer_id;
    job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&tiny.prompt);
    job.prompt_tokens = tiny.prompt.len() as u32;
    job.decode_token_limit = tiny.generated.len() as u32;
    job.max_context_tokens = 64;
    let class_facts = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: class_id,
        cu_ruleset_id: Hash64::default(),
    };
    let context = palw_fp_job_context_v3(
        &job,
        &class_facts,
        &crate::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job, job.decode_token_limit),
        NET,
    )
    .unwrap();
    let rule = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&job).expect("a V3 job has a selection rule");
    let select = |_: u32, row: &[i32], before: &[u32]| rule.select_v1(before, row, None).expect("a lane is admissible");
    let x = crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(|_, _, _| context.clone(), None, &select);
    let binding = x.binding.clone();
    let run = crate::palw_backend::PalwFpRunV1 {
        outcome: crate::palw_backend::PalwExecutionOutcomeV1 {
            trace_root: binding.full_logits_trace_root,
            output_root: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &x.generated),
            execution_root: binding.committed_execution_root,
            trace_manifest_root: h64(0x7A),
            trace_chunk_count: 1,
            material: Vec::new(),
        },
        facts: PalwFpRunFactsV3 {
            full_logits_trace_root: binding.full_logits_trace_root,
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: binding.step_merkle_root,
            step_leaf_count: binding.step_leaf_count,
            ..crate::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job, x.generated.len() as u32)
        },
        output_token_ids: x.generated.clone(),
    };
    let original = crate::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(&job, &binding.job_context, &run, 999_999)
        .expect("the producer's assembly reproduces the IR root");
    let mut payload = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment: original.clone(),
        prompt_token_ids: x.prompt.clone(),
        signature: Vec::new(),
    };
    payload.signature = sign(payload.claim_id(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT);
    let claim = payload.claim_id();
    let object = PalwConsensusObjectV2::FreePromptCommitted {
        claim,
        class_id,
        bond: producer,
        executor_pubkey: pubkey,
        work_leaves: original.work_leaves,
        prompt_token_ids_hash: original.job.prompt_token_ids_hash,
        prompt_tokens: original.job.prompt_tokens,
        prompt_token_ids: x.prompt.clone(),
        decode_tokens_executed: original.decode_tokens_executed,
        trace_root: original.trace_root,
        output_root: original.output_root,
        execution_root: original.execution_root,
        trace_chunk_count: original.trace_chunk_count,
        trace_retention_daa: original.trace_retention_daa,
        consumed_prefix_state: payload.consumed_prefix_state_v1(),
        job_pin: palw_fp_job_pin_v1(&original),
        public_job: Some(Box::new(original.clone())),
        eval: None,
    };

    // ---- the frame: quoted on the snapshot, pledged and signed by the producer, paired in its own transaction ----
    let header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&binding);
    let mut extras = PalwTransitionExtrasV1 {
        fp_derived_work_daa: Some(0),
        canonical_work_daa: Some(0),
        tir_public_conviction_active: true,
        tir_public_network_domain_v1: Some(h64(0x4E7)),
        // The registry's seat count bounds the frame's custodians (a Panel and the producer), as on every network that runs one;
        // inside its activation grace, so no lifecycle row is stepped by this fold.
        model_registry: Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals: crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1,
            span_daa: 100,
            genesis_works: Default::default(),
            grace_until_daa: u64::MAX,
            admission_audit_period_daa: None,
            readiness_v2_active: false,
            bond_maturity: None,
        }),
        ..Default::default()
    };
    let upper = state.staged_maximum_custodians_v1(class_id, AT_DAA, &p, &extras).expect("the native custodian upper");
    extras.tir_public_allocation_rules_v1 = Some(PalwTirPublicAllocationRulesV1 {
        maximum_leaves: 1 << 40,
        maximum_custodians: upper,
        court: crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 40, 45, 2).unwrap(),
        dissection: true,
        prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
        clock: crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1().clock,
    });
    let plan = palw_tir_public_original_quote_v1(&state, &p, &extras, AT_DAA, &original, &header)
        .expect("the quote folds")
        .expect("the frame route admits a tiled greedy original");
    let mut pledge =
        crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1::for_plan_v1(&original, &plan, AT_DAA).expect("the pledge");
    pledge.signature = sign(pledge.id_v1(), crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1);
    let envelope =
        PalwTirPublicFpEnvelopeV1::new_v1(borsh::to_vec(&payload).unwrap(), header, pledge, plan.terms.plan.rent_sompi).expect("TFP1");
    envelope.matches_object_v1(&object).expect("the envelope is the object's own");
    extras.tir_public_originals.insert(claim, envelope.clone());
    Lane { p, state, class_id, claim, object, envelope, extras, leaves: binding.step_leaf_count }
}

/// **Past the fence (every fence the lane reads at DAA 0): an IR class's framed free-prompt commitment is priced from its admitted
/// program and accepted, with its frame installed — through the production fold.**
#[test]
fn tir_fp_lane_prices_and_accepts_a_framed_ir_claim_with_every_fence_at_genesis() {
    let l = lane(false);
    // The price is the fold's own function: compute, from the program, with the derived leaves the capture's own.
    let price = palw_fp_commitment_price_v1(
        &l.state,
        &l.p,
        PalwFpPriceInputsV1 {
            fp_derived_work_daa: Some(0),
            canonical_work_daa: Some(0),
            daa_score: AT_DAA,
            receipt_rights: None,
            tir_free_prompt_active: true,
        },
        &l.claim,
        &l.class_id,
        match &l.object {
            PalwConsensusObjectV2::FreePromptCommitted { prompt_token_ids, .. } => prompt_token_ids,
            _ => unreachable!(),
        },
        l.envelope.original_fp_v1().unwrap().commitment.job.prompt_tokens,
        l.envelope.original_fp_v1().unwrap().commitment.decode_tokens_executed,
        l.leaves,
    )
    .expect("an IR class past the fence has a price");
    assert!(price.priced_in_compute && price.quanta >= 1 && price.pwu > 0, "{price:?}");
    assert_eq!(price.derived_work.map(|w| w.total_leaves), Some(l.leaves), "the derived leaves are the capture's");

    let root = l.state.state_root();
    let (next, delta) = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &l.extras).expect("the framed IR claim folds");
    let claim = next.claim(&l.claim).expect("the claim exists");
    assert!(matches!(claim.source, PalwClaimSourceV2::FreePrompt { quanta, .. } if quanta == price.quanta));
    assert_eq!(claim.pwu, price.pwu, "the fold wrote the price it quotes");
    assert!(
        crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&next, l.claim).is_some(),
        "the ORIGINAL frame is installed with the claim"
    );
    assert!(!next.fp_claimed_prompts_of(&l.class_id, &bond_key(PRODUCER)).is_empty(), "the prompt row is recorded");
    assert_eq!(revert_delta_v2(&next, &delta, &l.p).unwrap().state_root(), root, "and it reverts");
}

/// **Below the fence nothing changes: an IR class has no work profile** (refused by the same name as before), and past it a
/// frameless original is refused by G-03 — neither writes anything.
#[test]
fn tir_fp_lane_is_closed_below_the_fence_and_frameless_past_it() {
    let l = lane(false);
    let mut below = l.extras.clone();
    below.tir_public_conviction_active = false;
    below.tir_public_originals.clear();
    below.tir_public_allocation_rules_v1 = None;
    assert_eq!(
        apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &below).unwrap_err(),
        PalwStateV2Error::FreePromptClassHasNoWorkProfile { claim: l.claim, class: l.class_id },
        "below the fence an IR class has no work profile, exactly as before"
    );
    let mut frameless = l.extras.clone();
    frameless.tir_public_originals.clear();
    assert!(matches!(
        apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &frameless).unwrap_err(),
        PalwStateV2Error::ClassNotRewardable { code, .. }
            if code == PALW_G03_FRAMELESS_TIR_ORIGINAL_CODE_V1
    ));
}

/// **The lane's authentication on a gated network**: an IR class passes the free-prompt certification gate past the fence only
/// when one certified family covers every primitive its program reaches; below the fence the gate is the legacy one.
#[test]
fn tir_fp_lane_is_authenticated_by_one_covering_family_on_a_gated_network() {
    for certify in [false, true] {
        let mut l = lane(certify);
        l.p = l.p.with_fp_certified_classes(Default::default());
        let folded = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &l.extras);
        if certify {
            assert!(folded.is_ok(), "a covered IR class takes its framed claim: {:?}", folded.err());
        } else {
            assert_eq!(folded.unwrap_err(), PalwStateV2Error::FreePromptLaneUncertified(l.class_id));
        }
    }
}
