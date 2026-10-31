//! Actual transition, replay and restore checks for the all-model migration.
use super::*;
use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_panel_v2::{PalwPanelParamsV2, PalwPanelV2Error, PalwReceiptQuorumV2, PalwSeatReceiptV3};
use crate::palw_verification_v2::PalwSegmentMaskV2;

fn policy() -> PalwStateParamsV2 {
    params()
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_model_panel_one_from_daa(Some(101))
        // Small transition fixtures advertise the route dependencies; full Params
        // synchronization is checked separately against the shipped testnet preset.
        .with_g14_route_required_v1(Some(crate::palw_g14_gates_v1::PalwG14RouteMirrorV1 {
            from_daa: 101, legacy_filer_from: Some(0), legacy_held_da_from: Some(0),
            court_capacity_from: Some(0), kernel_panel_one_from: Some(0), gate_c_from: Some(0),
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
fn setup(p: &PalwStateParamsV2, with_verifier: bool) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    if let PalwConsensusObjectV2::BondRegistered { collateral, .. } = objects.last_mut().unwrap() {
        *collateral = 10_000;
    }
    if with_verifier {
        objects.push(seat_bond_reg(2, 10_000));
    }
    apply_round(&PalwChainStateV2::genesis(), p, &ctx(1, 100, 1), &objects, &lane()).unwrap()
}
fn bound(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64) {
    let s = setup(p, true);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, p, &ctx(2, 101, 2), &[], Some((&env, h64(0x101))), &lane()).unwrap();
    assert!(s.panel(&id).is_none(), "the commitment's own seed is not a draw");
    let next = attempt(1, 2);
    let (s, _) = fold_round(&s, p, &ctx(3, 102, 3), &[], Some((&next, h64(0x102))), &lane()).unwrap();
    assert_eq!(s.panel(&id).unwrap().seats, vec![seat_n(2)], "one independent bond is enough");
    assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { bound_daa: 102 }));
    (s, id)
}
#[test]
fn post_commit_assignment_and_full_receipt_survive_restore_and_reorg() {
    let p = policy();
    let (s, id) = bound(&p);
    let duties = crate::palw_producer_v2::palw_seat_duties_v2(&s, &p, &[bond_key(2)]);
    assert!(duties.iter().any(|d| d.claim_id == id && d.model_panel_one && d.panel_seat_count == 1));
    let receipt = receipt_at(id, bond_key(2), true, 103);
    let params = PalwPanelParamsV2::new(5, 3, 2).unwrap();
    let check = crate::palw_panel_v2::validate_receipt_quorum_v2_with_economy(
        &s,
        &params,
        &p,
        &ctx(4, 103, 4),
        h64(999),
        &id,
        &[receipt.clone()],
        |_, _, _, _| true,
        true,
        true,
        None,
    )
    .unwrap();
    assert!(matches!(check, PalwReceiptQuorumV2::Licensed { valid: 1 }));
    let expected = palw_rcore_seat_lock_v1(&s, &p, &lane(), &id, s.claim(&id).unwrap(), 1);
    let s = apply_round(
        &s,
        &p,
        &ctx(4, 103, 4),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: vec![receipt] }],
        &lane(),
    )
    .unwrap();
    let claim = s.claim(&id).unwrap();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::G14PanelOne));
    assert_eq!((claim.rcore.basis_k, claim.rcore.served_mask), (1, 1));
    assert!(!claim.rcore.escrow_released);
    assert!(palw_rcore_counts_licensed_v1(claim));
    assert!(s.slashable_locks.get(&(bond_key(2), id)).is_some(), "the actual signer remains liable");
    assert!(expected > 0);
    let finalized = apply_round(&s, &p, &ctx(5, 124, 5), &[], &lane()).unwrap();
    assert!(matches!(finalized.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
}
#[test]
fn full_scope_signature_and_unique_seat_are_mandatory() {
    let p = policy();
    let (s, id) = bound(&p);
    let pp = PalwPanelParamsV2::new(5, 3, 2).unwrap();
    let r = receipt_at(id, bond_key(2), true, 103);
    let check = |rs: &[PalwSeatReceiptV3], signature_ok: bool| {
        crate::palw_panel_v2::validate_receipt_coverage_v2(
            &s,
            &pp,
            &p,
            &ctx(4, 103, 4),
            h64(999),
            &id,
            rs,
            |_, _, _, _| signature_ok,
            true,
            None,
        )
    };
    let full = PalwSeatReceiptV3 { receipt: r.clone(), segments: PalwSegmentMaskV2::full(1) };
    assert!(matches!(check(&[full.clone()], true), Ok(PalwReceiptQuorumV2::Licensed { valid: 1 })));
    assert!(matches!(check(&[full.clone()], false), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));
    assert!(matches!(check(&[full.clone(), full], true), Err(PalwPanelV2Error::DuplicateSeat(_))));
    assert!(matches!(
        check(&[PalwSeatReceiptV3 { receipt: r, segments: PalwSegmentMaskV2(0) }], true),
        Err(PalwPanelV2Error::MaskNotAssigned { .. })
    ));
    let old_bind = PalwConsensusObjectV2::PanelBound { claim: id, anchor: h64(77), seats: vec![seat_n(2)] };
    assert!(apply_round(&s, &p, &ctx(4, 103, 4), &[old_bind], &lane()).is_err());
}
#[test]
fn no_independent_verifier_is_a_nonfraud_outcome_without_legacy_retry() {
    let p = policy();
    let s = setup(&p, false);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(2, 101, 2), &[], Some((&env, h64(0x101))), &lane()).unwrap();
    let next = attempt(1, 2);
    let (s, _) = fold_round(&s, &p, &ctx(3, 102, 3), &[], Some((&next, h64(0x102))), &lane()).unwrap();
    assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::NoCapablePanel, .. }));
    assert!(s.panel(&id).is_none());
    assert_eq!(s.bond(&bond_key(1)).unwrap().slashed, 0);
}

#[test]
fn model_ids_do_not_select_a_different_panel_route() {
    let p = policy();
    let mut objects = register_class_and_bond();
    if let PalwConsensusObjectV2::BondRegistered { collateral, .. } = &mut objects[1] {
        *collateral = 5_000_000_000;
    }
    objects.push(seat_bond_reg(2, 10_000));
    let s = apply_round(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, &lane()).unwrap();
    // A model registered after migration uses the same route immediately.
    let mut entrant = registration(h64(2), 500, Some(bond_key(1)));
    if let PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, .. } = &mut entrant {
        *slash_value_per_pwu = 5;
    }
    let s = apply_round(&s, &p, &ctx(2, 101, 2), &[entrant], &lane()).unwrap();
    let other = attempt_for_class(5, 1, h64(2), bond_key(1), vec![7; 4], op_id(21), h64(0xA1));
    let other_id = attempt_id_v2(&other.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(3, 102, 3), &[], Some((&other, h64(0x102))), &lane()).unwrap();
    let base = attempt(1, 2);
    let base_id = attempt_id_v2(&base.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(4, 103, 4), &[], Some((&base, h64(0x103))), &lane()).unwrap();
    assert_eq!(s.panel(&other_id).unwrap().seats, vec![seat_n(2)]);
    let next = attempt(1, 3);
    let (s, _) = fold_round(&s, &p, &ctx(5, 104, 5), &[], Some((&next, h64(0x104))), &lane()).unwrap();
    assert_eq!(s.panel(&base_id).unwrap().seats, vec![seat_n(2)]);
}

#[test]
fn single_seat_final_does_not_replace_gate_c() {
    use crate::palw_rfc0018_v1::{PalwGateCRuleV1, PalwRfc0018MirrorV1, palw_gate_c_weight_v1};
    let p = policy().with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
        gate_c_from_daa: Some(101),
        gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
        certification: crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1::approved_v1(100),
        ..Default::default()
    }));
    let (s, id) = bound(&p);
    let receipt = receipt_at(id, bond_key(2), true, 103);
    let s = apply_round(
        &s,
        &p,
        &ctx(4, 103, 4),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: vec![receipt] }],
        &lane(),
    )
    .unwrap();
    let s = apply_round(&s, &p, &ctx(5, 124, 5), &[], &lane()).unwrap();
    let claim = s.claim(&id).unwrap();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Final { .. }));
    assert_eq!(claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::G14PanelOne));
    assert_eq!(palw_gate_c_weight_v1(&p, &s, &id, claim, 10_000), 0);
    assert_eq!(s.safe_weight(), 0);
    assert!(!crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(&p, &s, &id, claim));
}

#[test]
fn a_bond_registered_after_the_commit_cannot_grind_into_its_draw() {
    let p = policy();
    let s = setup(&p, false);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(2, 101, 2), &[], Some((&env, h64(0x101))), &lane()).unwrap();
    let next = attempt(1, 2);
    let (s, _) = fold_round(&s, &p, &ctx(3, 102, 3), &[seat_bond_reg(2, 10_000)], Some((&next, h64(0x102))), &lane()).unwrap();
    assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::NoCapablePanel, .. }));
    assert!(s.panel(&id).is_none());
}

#[test]
fn the_only_false_valid_signer_pays_its_full_lock_before_and_after_final() {
    use crate::palw_offence_v1::{PalwOffenceKindV1, PalwPanelContradictionV1};
    let p = policy();
    let (mut binding, _, _) = crate::palw_step_refute::tests::base0_honest_execution();
    binding.step_leaf_count += 1;
    crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
    let contradiction = PalwPanelContradictionV1::StepStructural(crate::palw_step_leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: crate::palw_step_leg::PalwStepEvidenceV1::Shape,
    });
    let s = setup(&p, true);
    let mut env = attempt(40, 1);
    env.attempt.execution_root = binding.committed_execution_root;
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(2, 101, 2), &[], Some((&env, h64(0x101))), &lane()).unwrap();
    let next = attempt(1, 2);
    let (s, _) = fold_round(&s, &p, &ctx(3, 102, 3), &[], Some((&next, h64(0x102))), &lane()).unwrap();
    let s = apply_round(
        &s,
        &p,
        &ctx(4, 103, 4),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: vec![receipt_at(id, bond_key(2), true, 103)] }],
        &lane(),
    )
    .unwrap();
    let mut extras = lane();
    extras.offence_attribution_active = true;
    for after_final in [false, true] {
        let source = if after_final { apply_round(&s, &p, &ctx(5, 124, 5), &[], &extras).unwrap() } else { s.clone() };
        let before = source.bond(&bond_key(2)).unwrap().collateral;
        let lock = source.slashable_lock(bond_key(2), id).unwrap().amount;
        let evidence = borsh::to_vec(&f2_payload(id, 2, contradiction.clone())).unwrap();
        let now = if after_final { 125 } else { 104 };
        let convicted =
            apply_round(&source, &p, &ctx(6, now, 6), &[f2_offence(PalwOffenceKindV1::PanelFalseValidV2, 2, evidence)], &extras)
                .unwrap();
        assert!(matches!(convicted.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
        // Existing S4 adds min(25% of collateral, 3 * G) to the lock.
        // This claim has G=200, so the additional action is 600.
        assert_eq!(u128::from(before - convicted.bond(&bond_key(2)).unwrap().collateral), lock + 600);
        assert!(convicted.slashable_lock(bond_key(2), id).is_none());
    }
}

#[test]
fn the_single_scope_approval_authenticates_the_real_seat_signature_and_context() {
    use crate::palw_panel_v2::{PALW_RECEIPT_V3_MLDSA87_CONTEXT, palw_receipt_message_v3};
    let p = policy();
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([42; 32]);
    let mut objects = register_class_and_bond();
    if let PalwConsensusObjectV2::BondRegistered { collateral, .. } = &mut objects[1] {
        *collateral = 10_000;
    }
    let mut verifier = seat_bond_reg(2, 10_000);
    if let PalwConsensusObjectV2::BondRegistered { pubkey, .. } = &mut verifier {
        *pubkey = keys.verification_key.as_ref().to_vec();
    }
    objects.push(verifier);
    let s = apply_round(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, &lane()).unwrap();
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let (s, _) = fold_round(&s, &p, &ctx(2, 101, 2), &[], Some((&env, h64(0x101))), &lane()).unwrap();
    let next = attempt(1, 2);
    let (s, _) = fold_round(&s, &p, &ctx(3, 102, 3), &[], Some((&next, h64(0x102))), &lane()).unwrap();
    let mut receipt = PalwSeatReceiptV3 { receipt: receipt_at(id, bond_key(2), true, 103), segments: PalwSegmentMaskV2::full(1) };
    let message = palw_receipt_message_v3(h64(999), id, receipt.receipt.verdict, 103, receipt.segments);
    receipt.receipt.signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT, [43; 32])
            .unwrap()
            .as_ref()
            .to_vec();
    let check = |r: &PalwSeatReceiptV3| {
        crate::palw_panel_v2::validate_receipt_coverage_v2(
            &s,
            &PalwPanelParamsV2::new(5, 3, 2).unwrap(),
            &p,
            &ctx(4, 103, 4),
            h64(999),
            &id,
            &[r.clone()],
            crate::palw_attempt_material_v3::verify_ready_signature_v3,
            true,
            None,
        )
    };
    assert!(matches!(check(&receipt), Ok(PalwReceiptQuorumV2::Licensed { valid: 1 })));
    receipt.receipt.signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, message.as_byte_slice(), b"another-context", [44; 32])
            .unwrap()
            .as_ref()
            .to_vec();
    assert!(matches!(check(&receipt), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));
}
