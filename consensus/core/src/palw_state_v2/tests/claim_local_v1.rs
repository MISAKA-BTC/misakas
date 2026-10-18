//! Original context through the production fold and actual cold carriage codec.
//! This tests ledger context retention, not the remaining node G14 acceptance drill.
use super::*;
use crate::palw_claim_local_v1::*;

fn rules() -> PalwStateParamsV2 {
    params().with_claim_local_settlement_from_daa(Some(100))
}
fn extra() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        claim_local_rule_context_v1: Some(([11; 32], [12; 32])),
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: h64(10),
            ruleset_digest: h64(11),
            chain_genesis: h64(12),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn parent(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    apply(&PalwChainStateV2::genesis(), p, &ctx(1, 99, 1), &register_class_and_bond(), None).0
}
fn step(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    point: &PalwBlockContextV2,
    envelope: Option<&PalwAttemptEnvelopeV2>,
    extras: &PalwTransitionExtrasV1,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    apply_palw_transition_v2_with_extras(s, p, point, &[], envelope, false, false, false, false, extras)
}
fn frame(
    s: &PalwChainStateV2,
    point: PalwBlockContextV2,
) -> crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLiabilityFrameV1> {
    let key = palw_claim_liability_frame_key_v1(point.block);
    let raw = &s.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())];
    palw_decode_claim_liability_frame_v1(raw).unwrap()
}

#[test]
fn rfc23s2_origin_real_fold_cold_restore_and_reorg_preserve_original_context() {
    let p = rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let point = ctx(2, 100, 2);
    let (s2, delta) = step(&s1, &p, &point, Some(&env), &extra()).unwrap();
    let f = frame(&s2, point);
    assert_eq!(f.claims, BTreeSet::from([id]));
    assert_eq!(
        f.inputs,
        vec![PalwClaimLocalInputV1::Attempt { block: point.block, execution_key: Hash64::default(), envelope: Box::new(env.clone()) }]
    );
    let admission = s2.claim_local_admission_context_v1(&p, &point, id).unwrap().unwrap();
    assert!(admission.native_material_binding_v3().unwrap().is_none(), "PAV2 history never invents a native manifest");
    f.validate_context_v1().unwrap();
    let prior: PalwStateCarriageV2 = borsh::from_slice(&f.prior_state).unwrap();
    let current: PalwStateCarriageV2 = borsh::from_slice(&f.current_state).unwrap();
    assert_eq!(prior.classes, s1.classes);
    assert_eq!(prior.bonds, s1.bonds);
    assert!(!prior.claims.contains_key(&id));
    assert_eq!(current.claims.get(&id), s2.claim(&id));
    assert_eq!(current.claims[&id].reserved, s2.claim(&id).unwrap().reserved);
    s2.assert_internal_consistency(&p).unwrap();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s2)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s2.state_root()), false, None).unwrap();
    assert_eq!(restored.state_root(), s2.state_root());
    assert_eq!(*frame(&restored, point), *f);
    let replayed = apply_delta_v2(&s1, &delta, &p).unwrap();
    assert_eq!(replayed.state_root(), s2.state_root());
    let reverted = revert_delta_v2(&s2, &delta, &p).unwrap();
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(&reverted)).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(&s1)).unwrap()
    );
}

#[test]
fn rfc23s2_admission_context_recovers_the_exact_pav3_manifest_from_its_origin_header() {
    use crate::palw_attempt_material_v3::{
        PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3, PalwAttemptMaterialEnvelopeV3, PalwNativeMaterialBindingV3, PalwNativeMaterialReadyV3,
    };
    use crate::palw_material_capacity_v1::{PalwMaterialCodecV1, PalwMaterialManifestV1, PalwMaterialScopeV1};

    let p = rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let point = ctx(2, 100, 2);
    let (mut state, _) = step(&s1, &p, &point, Some(&env), &extra()).unwrap();
    let mut f = (*frame(&state, point)).clone();

    let manifest = PalwMaterialManifestV1 {
        root: h64(701),
        scopes: vec![PalwMaterialScopeV1 {
            claim: id,
            commitment: env.attempt.execution_root,
            codec: PalwMaterialCodecV1::NativeHeldComplete,
            bytes: 128,
            units: 1,
            content: h64(702),
        }],
    };
    let signature = vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let ready = PalwNativeMaterialReadyV3 {
        version: 3,
        session: manifest.root,
        consent: h64(703),
        binding: PalwNativeMaterialBindingV3 { claim: id, execution_key: Hash64::default(), manifest: manifest.clone() },
        providers: vec![
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h64(704), 0)),
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h64(705), 0)),
        ],
    };
    let envelope =
        PalwAttemptMaterialEnvelopeV3::assemble(env.clone(), ready, vec![signature.clone(); 2], |_, _| Ok(signature.clone())).unwrap();
    assert!(envelope.encode_wire().starts_with(&PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3));
    let header = crate::header::Header::new_finalized(
        crate::constants::BLOCK_VERSION,
        vec![vec![point.block]].try_into().unwrap(),
        h64(706),
        h64(707),
        h64(708),
        1_700_000_000_000,
        0x1e7f_ffff,
        9,
        crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
        point.daa_score,
        10u64.into(),
        point.blue_score,
        h64(709),
    )
    .with_palw_commitment(envelope.encode_wire());
    f.origin_headers.insert(point.block, borsh::to_vec(&header).unwrap());
    let key = palw_claim_liability_frame_key_v1(point.block);
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&f).unwrap());

    let admission = state.claim_local_admission_context_v1(&p, &point, id).unwrap().unwrap();
    let recovered = admission.native_material_binding_v3().unwrap().unwrap();
    assert_eq!(recovered.claim, id);
    assert_eq!(recovered.execution_key, Hash64::default());
    assert_eq!(recovered.manifest, manifest);
}

#[test]
fn rfc23s2_origin_unrelated_changes_do_not_rewrite_a_or_recursively_copy_frames() {
    let p = rules();
    let s1 = parent(&p);
    let env_a = attempt(40, 1);
    let a = attempt_id_v2(&env_a.attempt);
    let a_point = ctx(2, 100, 2);
    let (s2, _) = step(&s1, &p, &a_point, Some(&env_a), &extra()).unwrap();
    let original = borsh::to_vec(&*frame(&s2, a_point)).unwrap();
    let mut env_b = attempt(40, 2);
    env_b.attempt.execution_root = h64(42);
    let b = attempt_id_v2(&env_b.attempt);
    let b_point = ctx(3, 101, 3);
    let (s3, _) = step(&s2, &p, &b_point, Some(&env_b), &extra()).unwrap();
    assert_eq!(borsh::to_vec(&*frame(&s3, a_point)).unwrap(), original);
    let b_frame = frame(&s3, b_point);
    assert_eq!(b_frame.claims, BTreeSet::from([b]));
    assert!(!b_frame.claims.contains(&a), "sharing a model/DAG does not record a dependency");
    for bytes in [&b_frame.prior_state, &b_frame.current_state] {
        let c: PalwStateCarriageV2 = borsh::from_slice(bytes).unwrap();
        assert!(c.kernel_route.as_ref().unwrap().aux.keys().all(|(table, _)| *table != PALW_CLAIM_LOCAL_TABLE_V1));
    }
    s3.assert_internal_consistency(&p).unwrap();
}

#[test]
fn rfc23s2_origin_prefence_state_and_delta_bytes_match_the_released_fold() {
    let old = params();
    let future = old.clone().with_claim_local_settlement_from_daa(Some(100));
    let s1 = apply(&PalwChainStateV2::genesis(), &old, &ctx(1, 1, 1), &register_class_and_bond(), None).0;
    let env = attempt(40, 1);
    let point = ctx(2, 99, 2);
    let (released, old_delta) = step(&s1, &old, &point, Some(&env), &extra()).unwrap();
    let (scheduled, new_delta) = step(&s1, &future, &point, Some(&env), &extra()).unwrap();
    assert_eq!(borsh::to_vec(&old_delta).unwrap(), borsh::to_vec(&new_delta).unwrap());
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(&released)).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(&scheduled)).unwrap()
    );
    assert!(scheduled.kernel_route.as_ref().is_none_or(|route| route.aux.keys().all(|(t, _)| *t != PALW_CLAIM_LOCAL_TABLE_V1)));
}

#[test]
fn rfc23s2_origin_rejects_missing_rule_context_and_nested_or_changed_frame_points() {
    let p = rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let point = ctx(2, 100, 2);
    let mut missing = extra();
    missing.claim_local_rule_context_v1 = None;
    assert!(step(&s1, &p, &point, Some(&env), &missing).is_err());
    let (s2, _) = step(&s1, &p, &point, Some(&env), &extra()).unwrap();
    let original = frame(&s2, point);
    let mut bad = (*original).clone();
    bad.point.blue_score += 1;
    assert!(bad.validate_context_v1().is_err());
    let mut nested = (*original).clone();
    nested.current_state = borsh::to_vec(&PalwStateCarriageV2::from_state(&s2)).unwrap();
    assert!(palw_decode_claim_liability_frame_v1(&borsh::to_vec(&nested).unwrap()).is_err(), "reject recursion before allocation");
    // Refusing nested data must release the scanner's recursion guard.
    palw_decode_claim_liability_frame_v1(&borsh::to_vec(&*original).unwrap()).unwrap();
    let mut forged = s2.clone();
    forged.set_kernel_aux_row_for_tests(
        PALW_CLAIM_LOCAL_TABLE_V1,
        palw_claim_liability_frame_key_v1(point.block).to_vec(),
        Some(borsh::to_vec(&bad).unwrap()),
    );
    assert!(forged.assert_internal_consistency(&p).is_err());
}

#[test]
fn rfc23s2_origin_local_capacity_refusal_does_not_publish_a_partial_frame() {
    use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
    let p = rules();
    let original = parent(&p);
    let extras = extra();
    let mut builder = TransitionBuilder::new(&original, &p, false, false, false, false, &extras);
    let claim = palw_claim_template_v1(h64(1), bond_key(1), 100, 2, 1000);
    builder.write_claim(h64(500), Some(claim)).unwrap();
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
    builder.native_reservation = crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
    let root = builder.state.state_root();
    let result = palw_claim_local_fold_v1::retain_changed_claim_context_v1(
        &mut builder,
        &original,
        &ctx(2, 100, 2),
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(builder.state.state_root(), root);
    assert!(builder.state.kernel_route.is_none());
    assert_eq!(checker.outstanding(), (0, 0));
}

fn policy_rules() -> PalwStateParamsV2 {
    use crate::config::params::ForkActivation;
    use crate::palw_detection_policy_v1::*;
    use crate::palw_rfc0018_v1::{PalwGateCRuleV1, PalwRfc0018MirrorV1};
    rules()
        .with_detection_policy_v1(Some(PalwDetectionPolicyFenceV1 {
            activation: ForkActivation::new(150),
            policies: vec![palw_detection_genesis_design_v1(
                h64(1),
                PalwDetectionRouteV1::Base0A16,
                PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval: 16 },
            )],
        }))
        .with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
            sibling_attribution_from_daa: None,
            panel_anchor_permissionless_from_daa: None,
            rule_e_participation_v2_from_daa: None,
            gate_c_from_daa: Some(150),
            gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
            certification: crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1::design_unapproved_v1(720),
            fp_receipt_tail_daa: 0,
        }))
        .with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 {
            from_daa: 150,
            policy: crate::palw_court_capacity_v1::palw_court_capacity_placeholder_policy_v1(),
        }))
}

#[test]
fn rfc23s2_policy_fold_retains_actual_values_through_cold_restore() {
    let p = policy_rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let (s2, _) = step(&s1, &p, &ctx(2, 100, 2), Some(&env), &extra()).unwrap();
    let first = frame(&s2, ctx(2, 100, 2));
    assert!(first.policy.detection.is_empty());
    assert!(first.policy.certification.is_none());
    assert!(first.policy.court_capacity.is_none());
    // An actual timeout mutation at the later point creates another frame.
    // These are fixture policies, never a declaration that arming is permitted.
    let point = ctx(3, 150, 3);
    let (s3, _) = step(&s2, &p, &point, None, &extra()).unwrap();
    let f = frame(&s3, point);
    assert_eq!(f.policy.detection, p.detection_policy_v1().unwrap().policies);
    assert_eq!(f.policy.certification, Some(p.rfc0018_v1().unwrap().certification));
    assert_eq!(f.policy.court_capacity, p.court_capacity_at_v1(150));
    assert_eq!(f.policy.clocks.challenge, p.window_challenge_at(150));
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s3)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s3.state_root()), false, None).unwrap();
    assert_eq!(frame(&restored, point).policy, f.policy);
    assert_eq!(frame(&restored, ctx(2, 100, 2)).policy, first.policy);
    // A forged, enormous policy-vector count is refused before Borsh allocation.
    let mut bad = borsh::to_vec(&*f).unwrap();
    let at = borsh::to_vec(&(f.point, f.network_identity, f.fired_forks, &f.policy.clocks)).unwrap().len();
    bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(palw_decode_claim_liability_frame_v1(&bad).is_err());
}

#[test]
fn rfc23s2_policy_future_settings_cannot_change_earlier_frame_or_delta_bytes() {
    let p1 = policy_rules();
    let mut detection = p1.detection_policy_v1().unwrap().clone();
    detection.policies[0].resources.max_claim_fetch_bytes += 17;
    let p2 = p1.clone().with_detection_policy_v1(Some(detection));
    let s1 = parent(&p1);
    let env = attempt(40, 1);
    let point = ctx(2, 100, 2);
    let (a, da) = step(&s1, &p1, &point, Some(&env), &extra()).unwrap();
    let (b, db) = step(&s1, &p2, &point, Some(&env), &extra()).unwrap();
    assert_eq!(borsh::to_vec(&da).unwrap(), borsh::to_vec(&db).unwrap());
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(&a)).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(&b)).unwrap()
    );
}

#[test]
fn rfc23s2_policy_codec_preserves_all_discriminants_and_values() {
    use crate::palw_detection_policy_v1::*;
    for route in
        [PalwDetectionRouteV1::Base0A16, PalwDetectionRouteV1::TirV1, PalwDetectionRouteV1::PalwQ361, PalwDetectionRouteV1::Kernel]
    {
        for rule in [
            PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval: 16 },
            PalwIntervalCountRuleV1::HeldPositions { width: 7 },
            PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval: 1234 },
        ] {
            for method in [PalwInspectionMethodV1::IntervalReplay, PalwInspectionMethodV1::FullReexecution] {
                for localisation in [PalwLocalisationV1::IntervalLeafCourt, PalwLocalisationV1::RootDescentElementCourt] {
                    for gain in [None, Some(987654321)] {
                        let mut p = palw_detection_genesis_design_v1(h64(14), route, rule);
                        p.method = method;
                        p.localisation = localisation;
                        p.economics.fixed_gain_sompi = gain;
                        let bytes = borsh::to_vec(&p).unwrap();
                        crate::palw_native_resource_v1::preflight_native_value_v1::<PalwDetectionPolicyV1>(
                            bytes.as_slice(),
                            bytes.len() as u64,
                        )
                        .unwrap();
                        let restored: PalwDetectionPolicyV1 = borsh::from_slice(&bytes).unwrap();
                        assert_eq!(restored, p);
                        assert_eq!(restored.id(), p.id());
                    }
                }
            }
        }
    }
}

#[test]
fn rfc23s2_admission_reader_uses_the_real_birth_not_a_later_policy_frame() {
    let p = policy_rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let born = ctx(2, 100, 2);
    let (s2, _) = step(&s1, &p, &born, Some(&env), &extra()).unwrap();
    let later = ctx(3, 150, 3);
    let (s3, _) = step(&s2, &p, &later, None, &extra()).unwrap();
    assert!(matches!(s3.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { .. }));
    let fact = s3.claim_local_admission_context_v1(&p, &born, id).unwrap().unwrap();
    assert_eq!(fact.claim_id(), id);
    assert_eq!(fact.accepting_point(), born);
    assert!(matches!(fact.original_claim().phase, PalwClaimPhaseV2::Provisional));
    assert!(fact.frame().policy.detection.is_empty(), "the birth preceded the detection fence");
    assert!(
        s3.claim_local_admission_context_v1(&p, &later, id).unwrap().is_none(),
        "a timeout mutation is not a new birth under its later policy"
    );
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s3)).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s3.state_root()), false, None).unwrap();
    let cold = restored.claim_local_admission_context_v1(&p, &born, id).unwrap().unwrap();
    assert_eq!(cold.original_claim(), fact.original_claim());
    assert_eq!(cold.frame(), fact.frame());
}

#[test]
fn rfc23s2_admission_reader_distinguishes_merged_carrier_and_shared_acceptor() {
    let p = rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let carrier = h64(0xB1);
    let accepting = ctx(2, 100, 2);
    let mut extras = extra();
    extras.merged_reds.insert(carrier);
    let merged = [PalwMergedWorkV1 {
        carrying_block: carrier,
        work: PalwBlockWorkV3::Attempt(&env),
        execution_key: h64(0xE1),
        subsidy: 0,
        escrow_carve: None,
        bits: 0,
        job_anchor: Hash64::default(),
    }];
    let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
    let (s2, _, skips) = apply_palw_transition_v7(
        &s1,
        &p,
        Some(&admission),
        &accepting,
        &[],
        PalwBlockWorkV3::None,
        &merged,
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .unwrap();
    assert!(skips.is_empty(), "the actual merged attempt must be admitted: {skips:?}");
    let fact = s2.claim_local_admission_context_v1(&p, &accepting, id).unwrap().unwrap();
    assert_eq!(fact.original_claim().accepted_block, carrier);
    assert_eq!(fact.accepting_point().block, accepting.block);
    assert_ne!(carrier, accepting.block);
}

#[test]
fn rfc23s2_admission_reader_refuses_lost_or_mismatched_presented_inputs() {
    let p = rules();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let point = ctx(2, 100, 2);
    let (s2, _) = step(&s1, &p, &point, Some(&env), &extra()).unwrap();
    for kind in 0..2 {
        let mut altered = (*frame(&s2, point)).clone();
        if kind == 0 {
            altered.inputs.clear();
        } else if let PalwClaimLocalInputV1::Attempt { block, .. } = &mut altered.inputs[0] {
            *block = h64(999);
        }
        let mut bad = s2.clone();
        bad.set_kernel_aux_row_for_tests(
            PALW_CLAIM_LOCAL_TABLE_V1,
            palw_claim_liability_frame_key_v1(point.block).to_vec(),
            Some(borsh::to_vec(&altered).unwrap()),
        );
        assert!(bad.claim_local_admission_context_v1(&p, &point, id).is_err());
    }
    let mut unknown = s1.clone();
    let before = ctx(1, 99, 1);
    unknown.set_kernel_aux_row_for_tests(
        PALW_CLAIM_LOCAL_TABLE_V1,
        palw_claim_liability_frame_key_v1(before.block).to_vec(),
        Some(vec![255]),
    );
    assert!(
        unknown.claim_local_admission_context_v1(&p, &before, id).unwrap().is_none(),
        "do not inspect new frame bytes before their owner fence"
    );
}

#[test]
fn rfc23s2_admission_reader_scopes_reused_fp_identity_by_its_real_acceptance() {
    let p = rules().with_claim_retirement_daa(30).unwrap();
    let s1 = parent(&p);
    let id = h64(0xFC);
    let birth = ctx(2, 100, 2);
    let extras = extra();
    let wire_only = fp_commit(0xFC, 60, 3);
    let mut presented = wire_only.clone();
    if let PalwConsensusObjectV2::FreePromptCommitted { job_pin, public_job, .. } = &mut presented {
        *job_pin = h64(0xCAFE);
        *public_job = Some(Box::new(public_fp_job_fixture()));
    }
    // The released object wire intentionally omits these native-only fields.
    assert_eq!(borsh::to_vec(&wire_only).unwrap(), borsh::to_vec(&presented).unwrap());
    let wire = borsh::to_vec(&presented).unwrap();
    crate::palw_native_resource_v1::preflight_native_value_v1::<PalwConsensusObjectV2>(wire.as_slice(), wire.len() as u64).unwrap();
    let fold = |s: &PalwChainStateV2, point: &PalwBlockContextV2, objects: &[PalwConsensusObjectV2]| {
        apply_palw_transition_v2_with_extras(s, &p, point, objects, None, false, false, false, false, &extras).unwrap().0
    };
    let s2 = fold(&s1, &birth, &[presented.clone()]);
    let s3 = fold(&s2, &ctx(3, 111, 3), &[]);
    assert!(matches!(s3.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { .. }));
    // Step 0 retires the old record; step 3 admits a new record of the same
    // identity. The before/after carriages therefore BOTH contain this key.
    let rebirth = ctx(4, 142, 4);
    let s4 = fold(&s3, &rebirth, &[presented.clone()]);
    assert_eq!(s4.claim(&id).unwrap().accepted_daa, 142);
    let old = s4.claim_local_admission_context_v1(&p, &birth, id).unwrap().unwrap();
    let new = s4.claim_local_admission_context_v1(&p, &rebirth, id).unwrap().unwrap();
    assert_eq!(old.original_claim().accepted_daa, 100);
    assert_eq!(new.original_claim().accepted_daa, 142);
    assert_ne!(old.accepting_point(), new.accepting_point());
    assert!(new.frame().inputs.iter().any(|input| matches!(input,
        PalwClaimLocalInputV1::FreePromptNative { claim, job_pin, public_job: Some(job), eval: None }
            if *claim == id && *job_pin == h64(0xCAFE) && **job == public_fp_job_fixture())));
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s4)).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(s4.state_root()), false, None).unwrap();
    assert_eq!(restored.claim_local_admission_context_v1(&p, &rebirth, id).unwrap().unwrap().frame(), new.frame());
    let mut missing = new.frame().clone();
    missing.inputs.retain(|input| !matches!(input, PalwClaimLocalInputV1::FreePromptNative { .. }));
    let mut bad = s4.clone();
    bad.set_kernel_aux_row_for_tests(
        PALW_CLAIM_LOCAL_TABLE_V1,
        palw_claim_liability_frame_key_v1(rebirth.block).to_vec(),
        Some(borsh::to_vec(&missing).unwrap()),
    );
    assert!(bad.claim_local_admission_context_v1(&p, &rebirth, id).is_err());
}

fn native_exit() -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::KernelRouteV1 {
        bytes: misaka_palw_kernel::route::KernelRouteObjectV1::RequestExit {
            bond: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(1)),
        }
        .encode(),
        signer: bond_key(1),
        signature: Vec::new(),
    }
}

#[test]
fn rfc23s2_native_only_fold_retains_actual_rows_through_restore_and_reorg() {
    let p = rules();
    let before = parent(&p);
    let point = ctx(2, 100, 2);
    let (after, delta) =
        apply_palw_transition_v2_with_extras(&before, &p, &point, &[native_exit()], None, false, false, false, false, &extra())
            .unwrap();
    let f = frame(&after, point);
    assert!(f.claims.is_empty());
    assert!(!f.native_changes.is_empty());
    let journal: Vec<_> = delta
        .entries
        .iter()
        .filter_map(|e| match e {
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } if *table != PALW_CLAIM_LOCAL_TABLE_V1 => {
                Some(PalwClaimLocalNativeChangeV1 {
                    owner: PalwClaimLocalRowOwnerV1::KernelRoute,
                    table: *table,
                    key: key.clone(),
                    old: old.clone(),
                    new: new.clone(),
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(f.native_changes, journal);
    f.validate_context_v1().unwrap();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&after)).unwrap();
    crate::palw_native_resource_v1::preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state_v3(&p, Some(after.state_root()), false, None).unwrap();
    assert_eq!(*frame(&restored, point), *f);
    assert_eq!(apply_delta_v2(&before, &delta, &p).unwrap().state_root(), after.state_root());
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(&revert_delta_v2(&after, &delta, &p).unwrap())).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(&before)).unwrap()
    );
}

#[test]
fn rfc23s2_native_transient_journal_keeps_intermediate_values_and_refuses_discontinuity() {
    let p = rules();
    let before = parent(&p);
    let extras = extra();
    let point = ctx(2, 100, 2);
    let mut builder = TransitionBuilder::new(&before, &p, false, false, false, false, &extras);
    palw_kernel_route_fold_v1::ensure_route_header(&mut builder, &point).unwrap();
    // Real journal writer seam; the row is absent in both boundary carriages.
    for value in [Some(vec![1]), Some(vec![2]), None] {
        builder.write_kernel_row(32, vec![9; 64], value).unwrap();
    }
    palw_claim_local_fold_v1::retain_changed_claim_context_v1(
        &mut builder,
        &before,
        &point,
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    )
    .unwrap();
    let f = frame(&builder.state, point);
    assert_eq!(f.native_changes.len(), 3);
    f.validate_context_v1().unwrap();
    let mut bad = (*f).clone();
    bad.native_changes[1].old = Some(vec![7]);
    assert!(bad.validate_context_v1().is_err());
    let mut bad = (*f).clone();
    bad.native_changes.swap(0, 1);
    assert!(bad.validate_context_v1().is_err());
    let mut bad = (*f).clone();
    bad.native_changes[0].table = PALW_CLAIM_LOCAL_TABLE_V1;
    assert!(bad.validate_context_v1().is_err());
}

#[test]
fn rfc23s2_native_prefence_state_and_delta_remain_identical() {
    let released = params();
    let future = released.clone().with_claim_local_settlement_from_daa(Some(100));
    let before = parent(&released);
    let point = ctx(2, 99, 2);
    let run = |p| {
        apply_palw_transition_v2_with_extras(&before, p, &point, &[native_exit()], None, false, false, false, false, &extra()).unwrap()
    };
    let (a, da) = run(&released);
    let (b, db) = run(&future);
    assert_eq!(borsh::to_vec(&da).unwrap(), borsh::to_vec(&db).unwrap());
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(&a)).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(&b)).unwrap()
    );
}
