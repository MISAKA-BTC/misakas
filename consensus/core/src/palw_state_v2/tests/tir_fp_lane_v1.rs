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
    /// The producer's ML-DSA-87 signing key (T-N1 signs the producer's preparation and answers with it).
    keypair: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    /// The producer's run as committed (honest, or forged at one leaf).
    execution: crate::palw_tir_court_v1::test_support::TinyExecution,
}

fn at() -> PalwBlockContextV2 {
    ctx(3, AT_DAA, 3)
}

/// The world: the derived-work chain (its floor, the compute era's basis), the tiny IR class registered with its published layout,
/// one family certified for the attempt lane covering every primitive its program reaches, and a producer bond with a real ML-DSA-87
/// key. The commitment, its frame and the paired extras, built the way the producer builds them.
fn lane(certify: bool) -> Lane {
    lane_with(certify, None, None)
}

/// [`lane`] whose producer commits a run forged at `forge` (`(leaf, lane)`: one lane of one committed leaf moved — a commitment
/// the program did not compute), its frame priced and clocked by `cost` (`None`: the staged `network_v1` template with the TEST-ONLY
/// clock, as before).
fn lane_with(
    certify: bool,
    forge: Option<(usize, usize)>,
    cost: Option<crate::palw_tir_frame_cost_v1::PalwTirFrameCostV1>,
) -> Lane {
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
    let x = crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(|_, _, _| context.clone(), forge, &select);
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
    let upper = crate::palw_tir_frame_cost_v1::palw_tir_frame_custodian_upper_v1(AT_DAA, &p, &extras).expect("the frame's upper");
    extras.tir_public_allocation_rules_v1 = Some(PalwTirPublicAllocationRulesV1 {
        maximum_leaves: 1 << 40,
        maximum_custodians: upper,
        court: crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 40, 45, 2).unwrap(),
        dissection: true,
        prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        config: cost.map_or(crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(), |c| c.config),
        clock: cost.map_or(crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1().clock, |c| c.clock),
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
    Lane { p, state, class_id, claim, object, envelope, extras, leaves: binding.step_leaf_count, keypair: kp, execution: x }
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

/// **RFC-0002 02-A: past `palw_tir_container_identity_v1` the lane takes only a class bound to its `PALWART3` container.** The framed
/// original of an unbound class is refused at G-03's door (`TIR_CLASS_CONTAINER_UNBOUND`); the registrant's binding (tag 175) folds,
/// folds again as nothing, and a binding to another container is refused (immutable); the same original is then admitted. Below the
/// fence the binding kind is refused by the fold (it rides unjudged at acceptance) and the original is taken as before.
#[test]
fn tir_fp_lane_past_the_container_fence_takes_only_a_class_bound_to_its_palwart3_container() {
    use crate::palw_tir_container_v1::{PALW_TIR_UNBOUND_CONTAINER_CODE_V1, palw_tir_bound_container_v1, test_binding_v1};
    let mut l = lane(false);
    let mut class = l.state.class(&l.class_id).unwrap().clone();
    class.registrant_bond = Some(bond_key(PRODUCER));
    l.state.set_class_for_tests(l.class_id, class);
    let record = l.state.tir_class_v1(&l.class_id).unwrap().clone();
    let artifact_root = l.state.class(&l.class_id).unwrap().artifact_root;
    let bind = |manifest: &[u8]| PalwConsensusObjectV2::TirClassContainerBoundV1 {
        binding: Box::new(test_binding_v1(&record, artifact_root, manifest)),
        signature: Vec::new(),
    };
    let bound = bind(b"{\"model\":\"tiny\"}");
    // Below the fence: the binding does not fold, and the unbound original is taken exactly as before.
    assert!(apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&bound), &l.extras).is_err());
    assert!(apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &l.extras).is_ok());
    // Past it: the unbound class's original is refused at the door.
    let mut past = l.extras.clone();
    past.tir_container_identity_active = true;
    assert!(matches!(
        apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &past).unwrap_err(),
        PalwStateV2Error::ClassNotRewardable { code, .. } if code == PALW_TIR_UNBOUND_CONTAINER_CODE_V1
    ));
    // The registrant binds the class; again is nothing; another container is refused.
    let (bound_state, bound_delta) = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&bound), &past).expect("the binding folds");
    // Restart / IBD: the stored delta replayed on the parent rebuilds the same row and root; reorg: reverting it removes the row.
    let replayed = apply_delta_v2(&l.state, &bound_delta, &l.p).expect("the stored delta replays");
    assert_eq!(replayed.state_root(), bound_state.state_root(), "replaying the stored delta restores the bound container");
    assert_eq!(palw_tir_bound_container_v1(&replayed, l.class_id), palw_tir_bound_container_v1(&bound_state, l.class_id));
    let reverted = revert_delta_v2(&bound_state, &bound_delta, &l.p).expect("the binding reverts");
    assert_eq!(reverted.state_root(), l.state.state_root(), "a reorg past the binding removes it exactly");
    assert!(palw_tir_bound_container_v1(&reverted, l.class_id).is_none());
    let PalwConsensusObjectV2::TirClassContainerBoundV1 { binding, .. } = &bound else { unreachable!() };
    assert_eq!(palw_tir_bound_container_v1(&bound_state, l.class_id).map(|row| row.container_root), Some(binding.container_root));
    let next = ctx(4, AT_DAA + 1, 4);
    let (again, _) = apply_derived(&bound_state, &l.p, &next, std::slice::from_ref(&bound), &past).expect("the same binding again");
    assert_eq!(
        palw_tir_bound_container_v1(&again, l.class_id),
        palw_tir_bound_container_v1(&bound_state, l.class_id),
        "the same container again folds to nothing (the row, its bound DAA included, is unchanged)"
    );
    assert_eq!(
        apply_derived(&bound_state, &l.p, &next, std::slice::from_ref(&bind(b"{}")), &past).unwrap_err(),
        PalwStateV2Error::ImmutableModelUpdate("a TIR class's bound container is immutable")
    );
    // A binding whose inner root is another inventory is refused by the chain's own check.
    let mut stray = test_binding_v1(&record, Hash64::from_u64_word(0xBAD), b"{}");
    stray.class_id = l.class_id;
    assert_eq!(
        apply_derived(
            &l.state,
            &l.p,
            &at(),
            &[PalwConsensusObjectV2::TirClassContainerBoundV1 { binding: Box::new(stray), signature: Vec::new() }],
            &past
        )
        .unwrap_err(),
        PalwStateV2Error::ImmutableModelUpdate("the bound container's inner inventory root is not the class's registered artifact root")
    );
    // Bound (the binding and the original in one block): the same original is admitted.
    let (admitted, _) =
        apply_derived(&l.state, &l.p, &at(), &[bound.clone(), l.object.clone()], &past).expect("the bound class's original");
    assert!(admitted.claim(&l.claim).is_some());
}

/// **audit(verify-3) C-01**: past `palw_tir_container_identity_v1`, a TIR class with no signing registrant (a sentinel / genesis
/// registration: `registrant_bond == None`) can never be bound — the fold refuses every binding — so every producer's framed original
/// on it is refused `TIR_CLASS_CONTAINER_UNBOUND` forever. Expected: some binding route (or an exemption) so the honest original folds.
#[test]
fn audit_v3_c01_a_tir_class_without_a_registrant_is_unrewardable_forever_past_the_container_fence() {
    use crate::palw_tir_container_v1::{PALW_TIR_UNBOUND_CONTAINER_CODE_V1, test_binding_v1};
    let l = lane(false);
    assert!(l.state.class(&l.class_id).unwrap().registrant_bond.is_none(), "the lane's class is a sentinel registration");
    let record = l.state.tir_class_v1(&l.class_id).unwrap().clone();
    let artifact_root = l.state.class(&l.class_id).unwrap().artifact_root;
    let bound = PalwConsensusObjectV2::TirClassContainerBoundV1 {
        binding: Box::new(test_binding_v1(&record, artifact_root, b"{\"model\":\"tiny\"}")),
        signature: Vec::new(),
    };
    let mut past = l.extras.clone();
    past.tir_container_identity_active = true;
    // Below the fence the honest original is taken.
    assert!(apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &l.extras).is_ok());
    let binding = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&bound), &past).map(|_| ()).map_err(|e| format!("{e:?}"));
    let together =
        apply_derived(&l.state, &l.p, &at(), &[bound.clone(), l.object.clone()], &past).map(|_| ()).map_err(|e| format!("{e:?}"));
    let alone = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &past);
    let refused_unbound =
        matches!(&alone, Err(PalwStateV2Error::ClassNotRewardable { code, .. }) if *code == PALW_TIR_UNBOUND_CONTAINER_CODE_V1);
    assert!(
        together.is_ok() || binding.is_ok() || !refused_unbound,
        "C-01: past the container fence the sentinel TIR class can never be bound (binding: {binding:?}; binding+original: \
         {together:?}) and its honest original is refused {PALW_TIR_UNBOUND_CONTAINER_CODE_V1}"
    );
}

/// **audit fix-cd C-01 boundary: who must sign a container binding.** An Active registrant keeps priority (its signature is
/// required, so a third party's binding is verified against the registrant's key and refused); a registrant that is no longer
/// Active, or a sentinel registration, leaves the binding open (no signer, empty signature) — and the fold then binds it with the
/// same strict identity check (a stray inventory is still refused).
#[test]
fn audit_fixcd_c01_an_active_registrant_keeps_binding_priority_and_an_inactive_one_opens_it() {
    use crate::palw_tir_container_v1::{palw_tir_container_binding_signer_v1, test_binding_v1};
    let mut l = lane(false);
    let record = l.state.tir_class_v1(&l.class_id).unwrap().clone();
    let artifact_root = l.state.class(&l.class_id).unwrap().artifact_root;
    let binding = test_binding_v1(&record, artifact_root, b"{\"model\":\"tiny\"}");
    assert!(palw_tir_container_binding_signer_v1(&l.state, &binding).unwrap().is_none(), "sentinel: open");
    const REGISTRANT: u64 = 0x4C0;
    tn1_bond(&mut l.state, REGISTRANT, vec![0x4C; 4]);
    let mut class = l.state.class(&l.class_id).unwrap().clone();
    class.registrant_bond = Some(bond_key(REGISTRANT));
    l.state.set_class_for_tests(l.class_id, class);
    assert_eq!(
        palw_tir_container_binding_signer_v1(&l.state, &binding).unwrap().map(|(key, _)| *key),
        Some(bond_key(REGISTRANT)),
        "an Active registrant must sign (priority)"
    );
    l.state.bonds.get_mut(&bond_key(REGISTRANT)).unwrap().status = PalwBondStatusV2::Retiring { since_daa: 1, settled_at_since: 0 };
    assert!(palw_tir_container_binding_signer_v1(&l.state, &binding).unwrap().is_none(), "a retiring registrant: open");
    l.state.bonds.remove(&bond_key(REGISTRANT));
    assert!(palw_tir_container_binding_signer_v1(&l.state, &binding).unwrap().is_none(), "a gone registrant: open");
    let mut past = l.extras.clone();
    past.tir_container_identity_active = true;
    let open = PalwConsensusObjectV2::TirClassContainerBoundV1 { binding: Box::new(binding), signature: Vec::new() };
    let (bound, _) = apply_derived(&l.state, &l.p, &at(), &[open, l.object.clone()], &past).expect("an open binding folds");
    assert!(bound.claim(&l.claim).is_some(), "and the class's original is admitted");
    let mut stray = test_binding_v1(&record, Hash64::from_u64_word(0xBAD), b"{}");
    stray.class_id = l.class_id;
    assert!(
        apply_derived(
            &l.state,
            &l.p,
            &at(),
            &[PalwConsensusObjectV2::TirClassContainerBoundV1 { binding: Box::new(stray), signature: Vec::new() }],
            &past
        )
        .is_err(),
        "an open binding keeps the strict identity check"
    );
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

// ---------------------------------------------------------------------------------------------------------------------------------
// **T-N1 (G14 audit §10): an honest IR free-prompt claim reaches Final and payment; a forged one is convicted by ONE outside bond.**
//
// Every step is a block through the production transition (`apply_palw_transition_v2_with_extras`, and its V7 form for the receipt
// spend), every control is signed with a real ML-DSA-87 key and verified by the fold. The frame is priced and clocked by the TEST-ONLY
// frame cost (`palw_tir_frame_test_only_cost_v1`: small windows, NOT a measurement). Not covered here: the carrier transaction's fee
// and burn, the PQ mempool, the template and the node service loops (the kaspad service E2E's; see the audit's §10).
// ---------------------------------------------------------------------------------------------------------------------------------

const SEAT: u64 = 0x2A0;
const OUTSIDER: u64 = 0x3A0;

fn tn1_ctx(daa: u64) -> PalwBlockContextV2 {
    ctx(daa, daa, daa)
}

fn tn1_bond(state: &mut PalwChainStateV2, n: u64, pubkey: Vec<u8>) {
    state.bonds.insert(
        bond_key(n),
        PalwBondStateV2 {
            pubkey,
            operator_id: op_id(n),
            collateral: 1_000_000_000_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(0x9A00 + n),
            capable_classes: Default::default(),
        },
    );
}

/// One signed frame control as a block's consensus object.
fn tn1_control(
    keypair: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    domain: Hash64,
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    signer: PalwBondKeyV2,
    target: crate::palw_tir_public_carriage_v1::PalwTirPublicControlTargetV1,
    action: crate::palw_tir_public_carriage_v1::PalwTirPublicControlActionV1,
    rent: u64,
    turn: u64,
) -> PalwConsensusObjectV2 {
    let plan = frame.plan_v1();
    let mut control = crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1 {
        version: 1,
        claim: plan.original_claim,
        allocation_id: plan.allocation_id,
        source_id: plan.source_id,
        target,
        expected_turn: turn,
        declared_rent_sompi: rent,
        signer,
        action,
        signature: Vec::new(),
    };
    tn1_sign(keypair, domain, &mut control);
    PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) }
}

fn tn1_sign(
    keypair: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    domain: Hash64,
    control: &mut crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1,
) {
    let message = control.signing_message_v1(domain).expect("the control's message");
    control.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keypair.signing_key,
        message.as_byte_slice(),
        crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1,
        [0x2B; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    control.check_v1().expect("a well-formed control");
}

/// The world past the claim's acceptance: the framed claim folded at `AT_DAA`, then the producer's preparation (ONE WorkDebit for the
/// pass — the batched debit of audit §10 — and its Ready), then a Panel of one seat bound and the claim licensed. Returns the lane,
/// the state, the frame and the next free DAA.
fn tn1_licensed(forge: Option<(usize, usize)>) -> (Lane, PalwChainStateV2, crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1, u64) {
    use crate::palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T};
    use crate::palw_tir_public_owned_v1::{PalwTirPublicOwnedActionV1 as Owned, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1};
    let cost = crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1();
    let mut l = lane_with(false, forge, Some(cost));
    // An accuser's room is a share of its collateral (the fixture params carry none): half, as the conviction fold tests use.
    // R-core+ in force from genesis, as on testnet-12 (`palw_t12_arm_every_rule_from_genesis`): the conviction tiers, the
    // `CourtConviction` record and the exit-lock lattice a collectible debit needs.
    l.p = l
        .p
        .clone()
        .with_fp_exposure_ceiling(500)
        .expect("a representable exposure ratio")
        .with_escrow_backed_exposure_from_daa(Some(0))
        .with_rcore_plus_mirrors(Some(0), 12_900, Vec::new());
    // ADR-0152 v2 F2 as testnet-12 arms it at genesis: the claim records its job identity (the free-prompt job pin), the identity a
    // public conviction is judged against (`original_job_matches_v1`); without it no arithmetic refutation can name the claim.
    l.extras.offence_attribution_active = true;
    // ADR-0124's panel economy, as on testnet-12: the bound seats go on duty (the duty row a seat's atomic Valid + grant credits).
    l.extras.panel_economy_active = true;
    for n in 3..5 {
        tn1_bond(&mut l.state, SEAT + n, vec![7; 4]);
    }
    let domain = l.extras.tir_public_network_domain_v1.unwrap();
    let (s, _) = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&l.object), &l.extras).expect("the framed claim folds");
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&s, l.claim).expect("the frame is installed").clone();
    let plan = frame.plan_v1().clone();
    let terms = plan.terms;
    eprintln!(
        "[T-N1] test-only clock: accepted {} preparation_until +{} open_until +{} stream_until +{} collect_until +{} material +{}",
        terms.accepted_daa,
        plan.preparation_until_daa - terms.accepted_daa,
        terms.open_until_daa - terms.accepted_daa,
        terms.stream_until_daa - terms.accepted_daa,
        terms.collect_until_daa - terms.accepted_daa,
        terms.material_until_daa - terms.accepted_daa,
    );
    assert!(
        terms.collect_until_daa - terms.accepted_daa <= cost.clock.maximum_door_daa,
        "an accepted pursuit ends inside the record's longest door"
    );
    let producer = bond_key(PRODUCER);
    let maximum = frame.descriptor_v1().maximum_leaves;
    let key = crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirPreparation(plan.original_claim, plan.allocation_id);

    // ---- the Panel binds (five seats; three will sign Valid), and the producer files its ONE debit for the pass ----
    let seat_kps: Vec<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair> =
        (0..3u8).map(|n| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x50 + n; 32])).collect();
    let mut s = s;
    for (n, kp) in seat_kps.iter().enumerate() {
        tn1_bond(&mut s, SEAT + n as u64, kp.verification_key.as_ref().to_vec());
    }
    let seats: Vec<PalwPanelSeatV2> = (0..5).map(|n| PalwPanelSeatV2 { bond: bond_key(SEAT + n), operator_id: op_id(SEAT + n) }).collect();
    let preparation = T::Preparation { original_accepted_daa: terms.accepted_daa };
    let debit_of = |s: &PalwChainStateV2, actor: PalwBondKeyV2| {
        let mut book = crate::palw_tir_public_state_v1::palw_tir_public_work_book_v1(s, key, actor, maximum).expect("a duty");
        let snapshot = crate::palw_tir_public_state_v1::palw_tir_public_preparation_snapshot_v1(s, key, maximum).unwrap().unwrap();
        let lane = &snapshot.row.preparation.work_v1().allocation;
        assert_eq!(snapshot.row.preparation.contract_v1().plan.maximum_slices_per_actor, lane.maximum_restarts + 1, "one debit a pass");
        let work = crate::palw_tir_frame_cost_v1::tir_public_pass_budget_v1(lane.per_actor, lane.maximum_restarts + 1).unwrap();
        let scope = PalwTirPublicWorkScopeV1 {
            claim: plan.original_claim,
            owner: producer,
            reserved_daa: terms.accepted_daa,
            bundle_id: plan.allocation_id,
            source_id: plan.source_id,
            turn: 0,
            request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 },
            actor,
            phase: PalwTirPublicWorkPhaseV1::Capture,
            input_head: plan.source_id,
            input_offset: 0,
            slice: 0,
            cursor_id: h64(0xC0),
        };
        (book.prepare_v1(s, scope, work, None, maximum).expect("the pass's one debit prepares"), book)
    };
    let mut daa = AT_DAA + 1;
    let bind = PalwConsensusObjectV2::PanelBound { claim: l.claim, anchor: h64(77), seats };
    let (s, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &[bind], &l.extras).expect("the Panel binds");
    let (debit, mut producer_book) = debit_of(&s, producer);
    let object = tn1_control(&l.keypair, domain, &frame, producer, preparation, A::Preparation(debit), 0, 0);
    daa += 1;
    let (s, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &[object], &l.extras).expect("the pass's debit folds");
    assert!(producer_book.confirm_v1(&s, maximum).expect("confirms").is_some(), "the accepted debit mints the pass's permit");
    daa += 1;

    // ---- the producer's Ready, and the three Valid seats' own pass debits (each prepares its own capture) ----
    let header = frame.header_v1();
    let original = frame.original_v1();
    let descriptor = crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
        version: 1,
        source_id: plan.source_id,
        class_id: frame.descriptor_v1().class_id,
        artifact_root: header.artifact_root,
        execution_root: original.execution_root,
        trace_root: original.trace_root,
        step_root: header.step_merkle_root,
        step_leaf_count: header.step_leaf_count,
        decode_count: header.job_context.exact_decode_tokens,
        vocab: plan.acquisition.envelope.vocab,
        material_until_daa: terms.material_until_daa,
        manifest_digest: h64(0xD1),
        retained_bytes: 1,
    };
    let mut objects = vec![tn1_control(&l.keypair, domain, &frame, producer, preparation, A::Preparation(Owned::PreparationReady { descriptor }), 0, 0)];
    for (n, kp) in seat_kps.iter().enumerate() {
        let seat = bond_key(SEAT + n as u64);
        let (debit, _) = debit_of(&s, seat);
        objects.push(tn1_control(kp, domain, &frame, seat, preparation, A::Preparation(debit), 0, 0));
    }
    let (s, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &objects, &l.extras).expect("Ready and the seats' debits fold");
    daa += 1;

    // ---- each Valid seat's atomic Valid + raw custody grant + Ready (it becomes liable on the claim) ----
    let mut receipts = Vec::new();
    let mut grants = Vec::new();
    for (n, kp) in seat_kps.iter().enumerate() {
        let seat = bond_key(SEAT + n as u64);
        let sign = |message: &[u8], context: &[u8]| {
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message, context, [0x5E; 32]).unwrap().as_ref().to_vec()
        };
        let message = crate::palw_panel_v2::palw_receipt_message_v2(domain, l.claim, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid, daa);
        let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim: l.claim,
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            seat_bond: seat,
            signed_daa: daa,
            signature: sign(message.as_byte_slice(), crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        };
        let mut grant = crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1 {
            version: 1,
            claim: l.claim,
            seat,
            signed_daa: daa,
            pledge_id: frame.custody_v1().pledge_v1().id_v1(),
            valid_receipt_digest: crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
            signature: Vec::new(),
        };
        grant.signature = sign(grant.id_v1().as_byte_slice(), crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_GRANT_CONTEXT_V1);
        let mut movement = crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1 {
            version: 1,
            claim: l.claim,
            allocation_id: plan.allocation_id,
            source_id: plan.source_id,
            signer: seat,
            receipt: receipt.clone(),
            grant,
            descriptor,
            signature: Vec::new(),
        };
        let digest = movement.signing_message_v1(domain).unwrap();
        movement.signature = sign(digest.as_byte_slice(), crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1);
        movement.check_v1().unwrap();
        grants.push(PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant: Box::new(movement) });
        receipts.push(receipt);
    }
    let (s, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &grants, &l.extras).expect("the seats' atomic grants fold");
    assert!(daa < plan.preparation_until_daa, "every Ready landed inside the preparation door");
    daa += 1;

    // ---- the claim is licensed on those exact credited receipts (the Panel signs whatever the producer committed) ----
    let licence = PalwConsensusObjectV2::ReceiptLicensed { claim: l.claim, receipts };
    let (s, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &[licence], &l.extras).expect("the claim is licensed");
    assert!(
        matches!(s.claim(&l.claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "licensed: {:?}",
        s.claim(&l.claim).unwrap().phase
    );
    daa += 1;
    (l, s, frame, daa)
}

fn tn1_spend(l: &Lane, s: &PalwChainStateV2, daa: u64) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let bond = bond_key(PRODUCER).0;
    let spend = PalwReceiptSpendUnsignedV3 {
        version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain: h64(999),
        challenge: crate::palw_freeprompt_v3::spend_challenge_v3(h64(999), h64(0xB0), 1_700, 7, l.claim, 0, &bond),
        claim_id: l.claim,
        quantum_index: 0,
        beacon_block: h64(0xBEAC),
        producer_bond: bond,
        producer_pubkey: l.keypair.verification_key.as_ref().to_vec(),
    };
    apply_palw_transition_v7(
        s,
        &l.p,
        None,
        &tn1_ctx(daa),
        &[],
        PalwBlockWorkV3::ReceiptSpend(&spend),
        &[],
        Hash64::default(),
        false,
        false,
        true,
        false,
        &l.extras,
    )
    .map(|(state, delta, _)| (state, delta))
}

/// **T-N1, honest half: an IR free-prompt claim the producer computed reaches `Final` at the frame's own door and is paid** — its
/// receipt quantum spends (the receipt lane's weight is its payment), and the producer's collateral is untouched.
#[test]
fn t_n1_an_honest_ir_fp_claim_reaches_final_and_payment_inside_the_liability_horizon() {
    let (l, s, frame, daa) = tn1_licensed(None);
    let terms = frame.plan_v1().terms;
    let claim = s.claim(&l.claim).unwrap().clone();
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = claim.phase else { unreachable!() };
    let floor = palw_claim_final_floor_v1(&s, &l.p, &l.claim, &claim, licensed_daa).unwrap();
    assert!(floor >= terms.collect_until_daa, "no Final before the frame's conviction door closes");
    let horizon = licensed_daa + l.p.window_challenge_at(licensed_daa) + l.p.window_court();
    eprintln!("[T-N1] honest: licensed {licensed_daa} floor {floor} collect_until {} horizon {horizon}", terms.collect_until_daa);
    assert!(floor <= horizon, "the frame's door ends inside the liability horizon (no indefinitely delayed Final)");
    let collateral = s.bonds.get(&bond_key(PRODUCER)).unwrap().collateral;
    // Not Final (and not payable) before the floor.
    let (before, _) = apply_derived(&s, &l.p, &tn1_ctx(floor.max(daa + 1) - 1), &[], &l.extras).unwrap();
    assert!(!matches!(before.claim(&l.claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "not Final before its floor");
    assert!(tn1_spend(&l, &before, floor.max(daa + 1)).is_err(), "no receipt spend (payment) before Final");
    let (after, _) = apply_derived(&before, &l.p, &tn1_ctx(floor.max(daa + 1) + 1), &[], &l.extras).unwrap();
    assert!(matches!(after.claim(&l.claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "Final at its floor");
    let weight = after.safe_weight();
    let (paid, _) = tn1_spend(&l, &after, floor.max(daa + 1) + 2).expect("the Final claim's quantum spends");
    assert!(paid.safe_weight() > weight, "the spend adds its quantum's weight: the claim is paid");
    assert_eq!(paid.bonds.get(&bond_key(PRODUCER)).unwrap().collateral, collateral, "the honest producer is not charged");
}

/// **T-N1, forged half: a claim whose committed leaf the program did not compute, licensed by its (colluding) Panel, is convicted by
/// ONE bond outside the Panel** — it Opens its own acquisition after the preparation door, Demands the leaf its own replay disagrees
/// with, the producer answers it (Begin / Part / Finish, authenticated against the claim's step root), and the outsider files the
/// cone refutation built from its own recomputation. The claim is voided (`CourtFraud`), the producer's collateral is debited, it
/// never reaches `Final`, and its receipt quantum never spends (no weight, no reward).
#[test]
fn t_n1_a_forged_ir_fp_claim_is_convicted_by_one_outside_owner_and_never_paid() {
    use crate::palw_tir_public_carriage_v1::{
        PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T, PalwTirPublicControlV1 as Control,
        PalwTirPublicConvictionEvidenceV1 as Evidence,
    };
    use crate::palw_tir_public_owned_v1::{PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as PART, PalwTirPublicOwnedActionV1 as Owned};
    const FORGED: usize = 7;
    let (l, mut s, frame, mut daa) = tn1_licensed(Some((FORGED, 1)));
    let plan = frame.plan_v1().clone();
    let terms = plan.terms;
    let domain = l.extras.tir_public_network_domain_v1.unwrap();
    let producer = bond_key(PRODUCER);
    let outsider_kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x3A; 32]);
    tn1_bond(&mut s, OUTSIDER, outsider_kp.verification_key.as_ref().to_vec());
    let outsider = bond_key(OUTSIDER);
    let before = s.bonds.get(&producer).unwrap().collateral;

    // ---- the outside owner opens its own acquisition once the preparation door has closed ----
    daa = daa.max(plan.preparation_until_daa);
    let rent = plan.owner_terms_v1().unwrap().plan.rent_sompi;
    let open = tn1_control(&outsider_kp, domain, &frame, outsider, T::OwnerOpen { owner: outsider }, A::Open, rent, 0);
    let (opened, _) = apply_derived(&s, &l.p, &tn1_ctx(daa), &[open], &l.extras).expect("an outside bond opens");
    let acquisition = T::Acquisition { owner: outsider, reserved_daa: daa };
    daa += 1;

    // ---- its own replay of the job disagrees at the forged leaf: it demands that leaf ----
    let honest = crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(
        |_, _, _| l.execution.binding.job_context.clone(),
        None,
        &|_, row, before| {
            crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&l.envelope.original_fp_v1().unwrap().commitment.job)
                .unwrap()
                .select_v1(before, row, None)
                .unwrap()
        },
    );
    let differs: Vec<usize> = (0..honest.hashes.len()).filter(|i| honest.hashes[*i] != l.execution.hashes[*i]).collect();
    assert_eq!(differs, vec![FORGED], "the owner's own recomputation localizes exactly the forged leaf");
    let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: FORGED as u64 };
    let demand = tn1_control(&outsider_kp, domain, &frame, outsider, acquisition, A::Acquisition(Owned::Demand { request }), 0, 0);
    let (demanded, _) = apply_derived(&opened, &l.p, &tn1_ctx(daa), &[demand], &l.extras).expect("the owner demands the leaf");
    daa += 1;
    let owner_key = *demanded
        .staged_courts
        .keys()
        .find(|key| {
            matches!(key, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition(c, owner, _)
                if *c == l.claim && *owner == outsider)
        })
        .expect("the owner's acquisition row");
    let pending = |state: &PalwChainStateV2| match state.staged_court_row_v1(&owner_key) {
        Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirAcquisition(row)) => row.owned.active_v1().cloned(),
        _ => None,
    };
    let p = pending(&demanded).expect("an open demand");
    eprintln!("[T-N1] forged: demand opened {} respond_until {} (turn span {})", p.opened_daa, p.respond_until_daa, p.respond_until_daa - p.opened_daa);

    // ---- the producer answers from its committed material (it must, or it is defaulted) ----
    let body = borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
        preimage: l.execution.preimages[FORGED].clone(),
        opening: crate::palw_step_leg::step_opening_v1(&l.execution.hashes, FORGED as u64).unwrap(),
    })
    .unwrap();
    let mut state = demanded;
    let mut moves = vec![Owned::Begin {
        demand_id: p.demand_id,
        answerer: producer,
        total_bytes: body.len() as u64,
        body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),
    }];
    for (i, bytes) in body.chunks(PART).enumerate() {
        moves.push(Owned::Part { demand_id: p.demand_id, answerer: producer, offset: (i * PART) as u64, bytes: bytes.to_vec() });
    }
    moves.push(Owned::Finish { demand_id: p.demand_id, answerer: producer });
    for action in moves {
        let object = tn1_control(&l.keypair, domain, &frame, producer, acquisition, A::Acquisition(action), 0, p.turn);
        state = apply_derived(&state, &l.p, &tn1_ctx(daa), &[object], &l.extras).expect("the answer move folds").0;
        daa += 1;
    }
    assert!(daa <= p.respond_until_daa, "the honest custodian's whole answer fits its turn span");
    assert!(daa < terms.collect_until_daa, "the localization ends before the conviction door closes");

    // ---- the outsider files the cone refutation from its own recomputation; the fold adjudicates it deterministically ----
    let rules = crate::palw_tir_court_v1::PalwTirCourtRulesV1 {
        max_step_leaf_count: 1 << 26,
        prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
    };
    let refutation =
        crate::palw_tir_court_v1::build_tir_cone_refutation_v1(&l.execution.binding, FORGED as u64, &l.execution, &rules).unwrap();
    let mut proof = crate::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) };
    proof.tir_strip_program_v1();
    let second_proof = proof.clone();
    let mut filing = Control::conviction_unsigned_v1(
        plan.original_claim,
        plan.allocation_id,
        plan.source_id,
        outsider,
        Evidence::Arithmetic { proof: Box::new(proof) },
        frame.conviction_rent_v1(),
    )
    .unwrap();
    tn1_sign(&outsider_kp, domain, &mut filing);
    let (convicted, _) = apply_derived(
        &state,
        &l.p,
        &tn1_ctx(daa),
        &[PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(filing) }],
        &l.extras,
    )
    .expect("one outside bond's filing convicts");
    assert!(
        matches!(convicted.claim(&l.claim).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "the forged claim is voided"
    );
    assert!(convicted.bonds.get(&producer).unwrap().collateral < before, "the producer is slashed");
    // The producer's conviction and each credited false-Valid seat's offence, each recorded once.
    assert_eq!(convicted.consumed_offences.len(), state.consumed_offences.len() + 4, "producer + the three Valid seats");
    for n in 0..3 {
        let seat = bond_key(SEAT + n);
        assert!(
            convicted.consumed_offences.contains_key(&crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &l.claim)),
            "the Valid seat {n}'s false-Valid offence is consumed"
        );
        assert!(!convicted.slashable_locks.contains_key(&(seat, l.claim)), "its lock on the claim is taken");
        assert!(convicted.bonds.get(&seat).unwrap().collateral < state.bonds.get(&seat).unwrap().collateral, "seat {n} is slashed");
    }
    for n in 3..5 {
        let seat = bond_key(SEAT + n);
        assert_eq!(convicted.bonds.get(&seat).unwrap().collateral, state.bonds.get(&seat).unwrap().collateral, "a silent seat is not");
    }
    // Once only: the same filing again finds the offence consumed and the claim voided.
    let mut again = Control::conviction_unsigned_v1(
        plan.original_claim,
        plan.allocation_id,
        plan.source_id,
        outsider,
        Evidence::Arithmetic { proof: Box::new(second_proof) },
        frame.conviction_rent_v1(),
    )
    .unwrap();
    tn1_sign(&outsider_kp, domain, &mut again);
    assert!(
        apply_derived(&convicted, &l.p, &tn1_ctx(daa + 1), &[PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(again) }], &l.extras)
            .is_err(),
        "no second conviction of the same offence"
    );
    // Past every floor: never Final, never paid, no weight.
    let (later, _) = apply_derived(&convicted, &l.p, &tn1_ctx(terms.material_until_daa + 1), &[], &l.extras).unwrap();
    assert!(!matches!(later.claim(&l.claim).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::Final { .. })), "never Final");
    assert!(tn1_spend(&l, &later, terms.material_until_daa + 2).is_err(), "its receipt never spends: no reward, no weight");
    assert_eq!(later.safe_weight(), 0, "no consensus weight from the forged claim");
}

/// **Lane rfc02-09 (RFC-0002 Tensor IR identity): a catalog that names a TIR model by its `PALWART3` container root reaches the
/// class only through the chain's recorded binding.** The class registers its inventory root; edition 0 lists the container root
/// (as `palw_genesis8_edition0_v1` does). Unbound — or asked at a DAA before the binding — the class is `Registered` (the inventory
/// root is not the catalog's key and is never guessed into it); once its registrant's binding folds (past
/// `palw_tir_container_identity_v1`) it is `Approved` from the bound DAA on. A catalog listing the INVENTORY root still matches
/// exactly as before (byte-identical for every class that never binds).
#[test]
fn tir_catalog_tier_reaches_a_class_through_its_bound_container_root_only() {
    use crate::palw_annual_catalog_v1::{ApprovedModelV1, PalwAnnualCatalogChainV1};
    use crate::palw_artifact_identity_v1::{palw_catalog_container_root_of_class_v1, palw_registered_inventory_root_v1};
    use crate::palw_model_tier_v1::{PalwModelTierMirrorV1, palw_model_tier_at_v1};
    use crate::palw_tir_container_v1::test_binding_v1;
    let mut l = lane(false);
    let mut class = l.state.class(&l.class_id).unwrap().clone();
    class.registrant_bond = Some(bond_key(PRODUCER));
    l.state.set_class_for_tests(l.class_id, class);
    let record = l.state.tir_class_v1(&l.class_id).unwrap().clone();
    let inventory = palw_registered_inventory_root_v1(&l.state, &l.class_id).expect("a TIR class's root is an inventory root");
    let binding = test_binding_v1(&record, inventory.hash(), b"{\"model\":\"tiny\"}");
    let container = binding.container_root;
    assert_ne!(container, inventory.hash(), "the container root and the inventory root are different objects");
    let mirror_of = |root: Hash64| PalwModelTierMirrorV1 {
        from_daa: 0,
        catalogs: PalwAnnualCatalogChainV1::genesis_only(vec![ApprovedModelV1::of(l.class_id, root)]),
        registered_permille: 300,
        registered_cap_permille: 1000,
    };
    let mut p = l.p.clone();
    p.model_tier_v1 = Some(mirror_of(container));
    // Unbound: the catalog's container root does not reach the class.
    assert!(palw_catalog_container_root_of_class_v1(&l.state, &l.class_id, AT_DAA).is_none());
    assert!(!palw_model_tier_at_v1(&l.state, &p, &l.class_id, AT_DAA).is_approved(), "unbound: registered");
    // The registrant binds (past the fence).
    let mut past = l.extras.clone();
    past.tir_container_identity_active = true;
    let bound = PalwConsensusObjectV2::TirClassContainerBoundV1 { binding: Box::new(binding.clone()), signature: Vec::new() };
    let (bound_state, _) = apply_derived(&l.state, &l.p, &at(), std::slice::from_ref(&bound), &past).expect("the binding folds");
    assert_eq!(palw_catalog_container_root_of_class_v1(&bound_state, &l.class_id, AT_DAA).map(|c| c.hash()), Some(container));
    assert!(palw_model_tier_at_v1(&bound_state, &p, &l.class_id, AT_DAA).is_approved(), "bound: approved through the container");
    assert!(
        !palw_model_tier_at_v1(&bound_state, &p, &l.class_id, AT_DAA - 1).is_approved(),
        "a DAA before the binding does not read it"
    );
    // A catalog naming the inventory root matches as it always did; one naming another root matches nothing.
    p.model_tier_v1 = Some(mirror_of(inventory.hash()));
    assert!(palw_model_tier_at_v1(&l.state, &p, &l.class_id, AT_DAA).is_approved());
    p.model_tier_v1 = Some(mirror_of(Hash64::from_u64_word(0xC0FFEE)));
    assert!(!palw_model_tier_at_v1(&bound_state, &p, &l.class_id, AT_DAA).is_approved());
}
