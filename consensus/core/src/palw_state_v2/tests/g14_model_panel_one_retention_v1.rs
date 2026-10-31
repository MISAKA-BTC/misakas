//! Delayed signed Attempts through the actual state transition. Execution roots
//! are fixture inputs; signatures, receipt authentication and folds are real.
use super::*;
use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_MLDSA87_CONTEXT, attempt_trace_manifest_root_v1, class_ticket_v3};
use crate::palw_panel_v2::{PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwPanelParamsV2, PalwReceiptQuorumV2, PalwSeatReceiptV3};
use crate::palw_verification_v2::PalwSegmentMaskV2;

fn policy(fence: Option<u64>) -> PalwStateParamsV2 {
    params()
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_model_panel_one_from_daa(Some(101))
        .with_model_panel_one_retention_v1(fence)
        .with_model_tier_v1(Some(
            crate::palw_model_tier_v1::PalwModelTierFenceV1::at(crate::config::params::ForkActivation::new(0)).mirror(),
        ))
        .with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 {
            from_daa: 0,
            policy: crate::palw_court_capacity_v1::palw_court_capacity_placeholder_policy_v1(),
        }))
        .with_g14_route_required_v1(Some(crate::palw_g14_gates_v1::PalwG14RouteMirrorV1 {
            from_daa: 101,
            legacy_filer_from: Some(0),
            legacy_held_da_from: Some(0),
            court_capacity_from: Some(0),
            kernel_panel_one_from: Some(0),
            gate_c_from: Some(0),
            ..Default::default()
        }))
}

fn lane() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        round_lane: round_extras(1, Vec::new()).round_lane,
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(999),
            ruleset_digest: h64(998),
            ..Default::default()
        }),
        panel_economy_active: true,
        ..door_extras(true)
    }
}

fn keys(seed: u8) -> libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32])
}

fn setup(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    if let PalwConsensusObjectV2::BondRegistered { pubkey, collateral, .. } = &mut objects[1] {
        *pubkey = keys(41).verification_key.as_ref().to_vec();
        *collateral = 5_000_000_000;
    }
    let mut verifier = seat_bond_reg(2, 10_000);
    if let PalwConsensusObjectV2::BondRegistered { pubkey, .. } = &mut verifier {
        *pubkey = keys(42).verification_key.as_ref().to_vec();
    }
    objects.push(verifier);
    let s = apply_round(&PalwChainStateV2::genesis(), p, &ctx(1, 97, 1), &objects, &lane()).unwrap();
    // A later registration carries no FamilyCertified/ClassLaneCertified or
    // ModelReview outcome. The transition fixture supplies a fused descriptor;
    // it does not substitute a receipt, a court result or retention calculation.
    let mut entrant = registration(h64(2), 0, Some(bond_key(1)));
    if let PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, .. } = &mut entrant {
        *slash_value_per_pwu = 5;
    }
    let mut s = apply_round(&s, p, &ctx(2, 98, 2), &[entrant], &lane()).unwrap();
    s.classes.get_mut(&h64(2)).unwrap().fused_attention = true;
    assert_eq!(
        crate::palw_model_tier_v1::palw_model_tier_at_v1(&s, p, &h64(2), 101),
        crate::palw_model_tier_v1::PalwModelTierV1::Registered
    );
    assert_eq!(
        crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(
            p.model_tier_v1(),
            crate::palw_model_tier_v1::PalwModelTierV1::Registered,
            101
        ),
        300
    );
    s
}

fn signed_attempt(s: &PalwChainStateV2, p: &PalwStateParamsV2, header_daa: u64) -> PalwAttemptEnvelopeV2 {
    let mut env = attempt_for_class(5, 71, h64(2), bond_key(1), keys(41).verification_key.as_ref().to_vec(), op_id(21), h64(0xA1));
    env.attempt.trace_chunk_count = crate::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS;
    env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(env.attempt.trace_root, env.attempt.trace_chunk_count);
    env.attempt.trace_retention_daa =
        header_daa + crate::palw_court_capacity_v1::palw_attempt_trace_retention_daa_v1(s, p, &h64(2), header_daa);
    env.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keys(41).signing_key,
        attempt_id_v2(&env.attempt).as_byte_slice(),
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
        [43; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    env
}

#[test]
fn delayed_signed_attempt_extends_only_new_acceptance_and_survives_revert_and_cold_load() {
    let p = policy(Some(104));
    let s = setup(&p);
    let env = signed_attempt(&s, &p, 100);
    let bytes = borsh::to_vec(&env).unwrap();
    let id = attempt_id_v2(&env.attempt);
    let ticket = class_ticket_v3(&env.attempt, h64(0x100));
    crate::palw_admission_v2::check_palw_attempt_da_pins_v1(&p, &env.attempt, 100).unwrap();
    assert!(crate::palw_attempt_material_v3::verify_ready_signature_v3(
        &env.attempt.executor_pubkey,
        id.as_byte_slice(),
        &env.signature,
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT
    ));
    let old = fold_round(&s, &p, &ctx(3, 103, 3), &[], Some((&env, h64(0x100))), &lane()).unwrap_err();
    assert!(old.to_string().contains("COURT_RETENTION_BELOW_GUARANTEE"), "{old}");
    let (accepted, delta) = fold_round(&s, &p, &ctx(3, 104, 3), &[], Some((&env, h64(0x100))), &lane()).unwrap();
    let claim = accepted.claim(&id).unwrap();
    let span = crate::palw_court_capacity_v1::palw_attempt_trace_retention_daa_v1(&s, &p, &h64(2), 104);
    assert_eq!(claim.trace_retention_daa, 104 + span);
    assert_eq!(claim.pwu, env.attempt.pwu);
    assert_eq!(claim.execution_root, env.attempt.execution_root);
    assert_eq!(bytes, borsh::to_vec(&env).unwrap());
    assert_eq!(ticket, class_ticket_v3(&env.attempt, h64(0x100)));
    assert_eq!(id, attempt_id_v2(&env.attempt));
    assert_eq!(revert_delta_v2(&accepted, &delta, &p).unwrap().state_root(), s.state_root());
    let carriage = borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
    let cold = borsh::from_slice::<PalwStateCarriageV2>(&carriage).unwrap().into_state(&p, Some(accepted.state_root())).unwrap();
    assert_eq!(cold.claim(&id).unwrap().trace_retention_daa, 104 + span);
    let between = env.attempt.trace_retention_daa + 1;
    assert!(between < claim.trace_retention_daa);
    assert!(crate::palw_da_rcore_v1::palw_da_material_owed_v1(claim, between));
    assert!(!crate::palw_da_rcore_v1::palw_da_material_owed_v1(claim, claim.trace_retention_daa + 1));
}

#[test]
fn delayed_registered_attempt_gets_one_real_signature_approval_and_final() {
    let p = policy(Some(104));
    let s = setup(&p);
    let env = signed_attempt(&s, &p, 100);
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(3, 104, 3), &[], Some((&env, h64(0x100))), &lane()).unwrap();
    let anchor = attempt(1, 72);
    let (s, _) = fold_round(&s, &p, &ctx(4, 105, 4), &[], Some((&anchor, h64(0x105))), &lane()).unwrap();
    assert_eq!(s.panel(&id).unwrap().seats, vec![seat_n(2)]);
    let mut approval = PalwSeatReceiptV3 { receipt: receipt_at(id, bond_key(2), true, 106), segments: PalwSegmentMaskV2::full(1) };
    let message = crate::palw_panel_v2::palw_receipt_message_v3(h64(999), id, approval.receipt.verdict, 106, approval.segments);
    approval.receipt.signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&keys(42).signing_key, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT, [44; 32])
            .unwrap()
            .as_ref()
            .to_vec();
    assert!(matches!(
        crate::palw_panel_v2::validate_receipt_coverage_v2(
            &s,
            &PalwPanelParamsV2::new(5, 3, 2).unwrap(),
            &p,
            &ctx(5, 106, 5),
            h64(999),
            &id,
            &[approval.clone()],
            crate::palw_attempt_material_v3::verify_ready_signature_v3,
            true,
            None
        ),
        Ok(PalwReceiptQuorumV2::Licensed { valid: 1 })
    ));
    let s = apply_round(
        &s,
        &p,
        &ctx(5, 106, 5),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: vec![approval.receipt] }],
        &lane(),
    )
    .unwrap();
    let disclose = palw_da_disclose_window_daa_v1(&p);
    let beyond_raw = env.attempt.trace_retention_daa - disclose + 1;
    assert!(
        palw_da_accusation_admissible_v2(&s, &p, &lane(), &id, &bond_key(2), beyond_raw).is_ok(),
        "the accepted deadline, not the raw header pin, controls the DA admission reader"
    );
    let past_effective = s.claim(&id).unwrap().trace_retention_daa - disclose + 1;
    assert!(matches!(
        palw_da_accusation_admissible_v2(&s, &p, &lane(), &id, &bond_key(2), past_effective),
        Err(PalwStateV2Error::DaOutsideRetention { .. })
    ));
    let final_state = apply_round(&s, &p, &ctx(6, 127, 6), &[], &lane()).unwrap();
    assert!(matches!(final_state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
}

#[test]
fn effective_retention_never_reduces_raw_promise_or_rewrites_old_claims() {
    let p = policy(Some(104));
    let s = setup(&p);
    let effective = crate::palw_model_panel_one_v1::model_panel_one_effective_attempt_retention_v1;
    let span = crate::palw_court_capacity_v1::palw_attempt_trace_retention_daa_v1(&s, &p, &h64(2), 104);
    assert_eq!(effective(&s, &p, &h64(2), 103, 500), 500);
    assert_eq!(effective(&s, &p, &h64(2), 104, 500), (104 + span).max(500));
    assert_eq!(effective(&s, &p, &h64(2), 104, u64::MAX - 1), u64::MAX - 1);
    let model_one_off = p.clone().with_model_panel_one_from_daa(None);
    assert_eq!(effective(&s, &model_one_off, &h64(2), 104, 500), 500);
}

#[test]
fn full_admission_class_pin_prevents_a_second_fused_retention_ticket() {
    use crate::palw_admission_v2::{check_palw_attempt_da_pins_v1, check_palw_attempt_model_one_retention_pin_v1};
    let p = policy(Some(104));
    let s = setup(&p);
    let env = signed_attempt(&s, &p, 100);
    let original = borsh::to_vec(&env).unwrap();
    let original_id = attempt_id_v2(&env.attempt);
    let ticket = class_ticket_v3(&env.attempt, h64(0x100));
    check_palw_attempt_da_pins_v1(&p, &env.attempt, 100).unwrap();
    check_palw_attempt_model_one_retention_pin_v1(&s, &p, &env.attempt, 100, 104).unwrap();
    let mut short = env.attempt.clone();
    short.trace_retention_daa = 100
        + crate::palw_producer_v2::palw_min_trace_retention_daa_at_v2(&p, 100)
        + crate::palw_rfc0018_v1::palw_gate_c_retention_extension_daa_v1(&p, 100);
    assert_ne!(short.trace_retention_daa, env.attempt.trace_retention_daa);
    // Shape alone historically admits both canonical forms; the class-aware
    // full-admission seam must keep exactly one for this fused class.
    check_palw_attempt_da_pins_v1(&p, &short, 100).unwrap();
    check_palw_attempt_model_one_retention_pin_v1(&s, &p, &short, 100, 103).unwrap();
    assert!(matches!(
        check_palw_attempt_model_one_retention_pin_v1(&s, &p, &short, 100, 104),
        Err(crate::palw_admission_v2::PalwAdmissionV2Error::TraceRetentionNotDerived { .. })
    ));
    assert_eq!(original, borsh::to_vec(&env).unwrap());
    assert_eq!(original_id, attempt_id_v2(&env.attempt));
    assert_eq!(ticket, class_ticket_v3(&env.attempt, h64(0x100)));
    assert!(crate::palw_attempt_material_v3::verify_ready_signature_v3(
        &env.attempt.executor_pubkey,
        original_id.as_byte_slice(),
        &env.signature,
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT
    ));
}

#[test]
fn rider_signs_exact_accepted_lead_obligation_and_gets_its_own_full_guarantee() {
    let p = policy(Some(104)).with_worker_carve_permille(1000).unwrap().with_capacity_s567_mirror(None, Some(0), None);
    let s = setup(&p);
    for accepted_daa in [103, 104] {
        let mut lead = attempt(40, accepted_daa);
        lead.attempt.executor_pubkey = keys(41).verification_key.as_ref().to_vec();
        lead.attempt.trace_chunk_count = crate::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS;
        lead.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(lead.attempt.trace_root, lead.attempt.trace_chunk_count);
        lead.attempt.trace_retention_daa =
            100 + crate::palw_court_capacity_v1::palw_attempt_trace_retention_daa_v1(&s, &p, &h64(1), 100);
        let lead_id = attempt_id_v2(&lead.attempt);
        let mut at = ctx(3, accepted_daa, 3);
        at.subsidy = 1_000;
        let (accepted, _) = fold_round(&s, &p, &at, &[], Some((&lead, h64(0xA0))), &lane()).unwrap();
        let inherited = accepted.claim(&lead_id).unwrap().trace_retention_daa;
        let mut rider = lead.clone();
        rider.attempt.pwu = 5;
        rider.attempt.challenge = crate::palw_capacity_s567_v1::palw_rider_challenge_v1(&lead_id, 0);
        rider.attempt.trace_retention_daa = inherited;
        rider.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys(41).signing_key,
            attempt_id_v2(&rider.attempt).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [45; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        let extras = lane();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(1_000).unwrap();
        let mut builder = TransitionBuilder::new(&accepted, &p, false, false, false, false, &extras);
        let before = accepted.claim(&lead_id).unwrap().escrowed_reward;
        attach_riders_v1(&mut builder, &ctx(4, 105, 4), &p, Some(&admission), &lead_id, &[rider.clone()]).unwrap();
        let child = builder.state.claim(&attempt_id_v2(&rider.attempt)).unwrap();
        let span = crate::palw_court_capacity_v1::palw_attempt_trace_retention_daa_v1(&accepted, &p, &h64(1), 105);
        assert_eq!(child.trace_retention_daa, inherited.max(105 + span));
        assert_eq!(before, child.escrowed_reward + builder.state.claim(&lead_id).unwrap().escrowed_reward);
        if accepted_daa == 104 {
            let mut raw_cached = rider.clone();
            raw_cached.attempt.trace_retention_daa = lead.attempt.trace_retention_daa;
            let mut refused = TransitionBuilder::new(&accepted, &p, false, false, false, false, &extras);
            assert!(
                attach_riders_v1(&mut refused, &ctx(4, 105, 4), &p, Some(&admission), &lead_id, &[raw_cached])
                    .unwrap_err()
                    .to_string()
                    .contains("data-availability pins")
            );
        }
    }
}
