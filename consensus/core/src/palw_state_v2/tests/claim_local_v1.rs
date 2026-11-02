//! Original context through the production fold and actual cold carriage codec.
//! This tests ledger context retention, not the remaining node G14 acceptance drill.
#![allow(warnings)]
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
    // First scoped admission readers operate without deserializing any F1.
    let original = s2.claim(&id).unwrap();
    let source = super::super::palw_claim_local_source_admission_v8::for_stored_claim_v8(&restored,id,original).unwrap().unwrap();
    assert_eq!(source.accepting_point(),point);
    assert_eq!(source.original_claim(),admission.original_claim());
    assert_eq!(source.original_params_bytes(),borsh::to_vec(&p).unwrap().as_slice());
    super::super::palw_claim_local_source_admission_v8::assert_stored_birth_alias_rejected_v8(
        &restored,&p,&extra(),id,original);

    assert!(source.inputs_known().unwrap());
    let mut scoped_only=restored.clone();
    scoped_only.kernel_route.as_mut().unwrap().aux.retain(|(table,key),_| *table!=PALW_CLAIM_LOCAL_TABLE_V1 || key.first()!=Some(&b'F'));
    assert_eq!(super::super::palw_claim_local_source_admission_v8::known_inputs_v8(&scoped_only,id,original).unwrap(),Some(true));
    let cursor=super::super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&scoped_only,&p,id,original).unwrap().unwrap();
    assert_eq!(cursor.accepting_point(),point);assert_eq!(cursor.original_claim(),admission.original_claim());assert!(cursor.prefix_use().unwrap().is_none());
    let exact = super::super::palw_claim_local_source_admission_v8::for_birth_point_v8(
        &scoped_only, id, point.block, point.daa_score, point.blue_score).unwrap().unwrap();
    assert_eq!(exact.accepting_point(), point, "actual subsidy is returned from the archive, never guessed");
    assert_eq!(exact.original_claim(), admission.original_claim());
    for (block, daa, blue) in [(h64(900_001), point.daa_score, point.blue_score),
        (point.block, point.daa_score + 1, point.blue_score),
        (point.block, point.daa_score, point.blue_score + 1)] {
        assert!(super::super::palw_claim_local_source_admission_v8::for_birth_point_v8(
            &scoped_only, id, block, daa, blue).unwrap().is_none());
    }
    let wrong_subsidy = PalwBlockContextV2 { subsidy: point.subsidy.checked_add(1).unwrap(), ..point };
    assert!(super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(
        &scoped_only, id, wrong_subsidy).is_err());

    assert!(super::super::palw_claim_local_rewards_v1::dependency_inputs_v1(&scoped_only,&p,id,original,None).unwrap().is_empty());
    // This checks the migrated readers only. Remaining readers still need F1;
    // scoped_only is never asserted to be a complete replacement global state.

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
    let mut header = crate::header::Header::new_finalized(
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
    header.hash = point.block;
    f.origin_headers.insert(point.block, borsh::to_vec(&header).unwrap());
    let key = palw_claim_liability_frame_key_v1(point.block);
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&f).unwrap());

    let admission = state.claim_local_admission_context_v1(&p, &point, id).unwrap().unwrap();
    let recovered = admission.native_material_binding_v3().unwrap().unwrap();
    assert_eq!(recovered.claim, id);
    assert_eq!(recovered.execution_key, Hash64::default());
    assert_eq!(recovered.manifest, manifest);

    let mut substituted = header;
    substituted.hash = h64(710);
    f.origin_headers.insert(point.block, borsh::to_vec(&substituted).unwrap());
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()), borsh::to_vec(&f).unwrap());
    let admission = state.claim_local_admission_context_v1(&p, &point, id).unwrap().unwrap();
    assert!(admission.native_material_binding_v3().is_err(), "a different origin header cannot supply this claim's manifest");
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
    // Direct retention boundary: supply the actual same executing point that
    // the ordinary full-fold entry seals before dispatching native owners.
    let point=ctx(2,100,2);builder.claim_local_lock_point_v6=Some(point);
    let claim = palw_claim_template_v1(h64(1), bond_key(1), 100, 2, 1000);
    builder.write_claim(h64(500), Some(claim)).unwrap();
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
    builder.native_reservation = crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
    let root = builder.state.state_root();
    let result = palw_claim_local_fold_v1::retain_changed_claim_context_v1(
        &mut builder,
        &original,
        &point,
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))), "actual local zero-capacity refusal: {result:?}");
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
            attestor_liability_from_daa: None,
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
    let p = rules().with_claim_retirement_daa(1).unwrap();
    let s1 = parent(&p);
    let env = attempt(40, 1);
    let id = attempt_id_v2(&env.attempt);
    let carrier = h64(0xB1);
    let accepting = ctx(2, 100, 2);
    let mut extras = extra();
    extras.native_fact_ruleset=Some(crate::Hash::from_bytes([25;32]));
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
    let exact = super::super::palw_claim_local_source_admission_v8::for_birth_point_v8(
        &s2, id, accepting.block, accepting.daa_score, accepting.blue_score).unwrap().unwrap();
    assert_eq!(exact.accepting_point(), accepting);
    assert_eq!(exact.original_claim().accepted_block, carrier);
    assert!(super::super::palw_claim_local_source_admission_v8::for_birth_point_v8(
        &s2, id, carrier, accepting.daa_score, accepting.blue_score).unwrap().is_none(),
        "the carrying block cannot stand in for the actual accepting block");

    // Continue this real merged source through ordinary binding/licensing and
    // finality, then use the native retirement owner. Its accepting block is
    // still distinct from the carrier at the actual deletion observation.
    let bind=PalwConsensusObjectV2::PanelBound{claim:id,anchor:h64(77),seats:vec![PalwPanelSeatV2{bond:bond_key(1),operator_id:op_id(21)}]};
    let(s3,_)=apply_palw_transition_v2_with_extras(&s2,&p,&ctx(3,101,3),&[bind],None,false,false,false,false,&extras).unwrap();
    let license=PalwConsensusObjectV2::ReceiptLicensed{claim:id,receipts:seat_says(true)};
    let(s4,_)=apply_palw_transition_v2_with_extras(&s3,&p,&ctx(4,102,4),&[license],None,false,false,false,false,&extras).unwrap();
    let(s5,_)=step(&s4,&p,&ctx(5,124,5),None,&extras).unwrap();let terminal=s5.claim(&id).unwrap().clone();
    assert!(matches!(terminal.phase,PalwClaimPhaseV2::Final{..}));assert_eq!(terminal.accepted_block,carrier);
    let at=ctx(6,1_000_000,6);let mut b=TransitionBuilder::new(&s5,&p,false,false,false,false,&extras);
    b.claim_local_lock_point_v6=Some(at);b.claim_local_gc_defer_v3=true;b.claim_local_capture_retired_exec_events_v24=true;
    b.retire_claim_at_v18(&at,id,&terminal).unwrap();
    let ordinal=b.entries.iter().position(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old:Some(c),new:None}if *key==id&&c==&terminal)).unwrap();
    let event=b.claim_local_retired_claim_events_v25.get(&(id,carrier,accepting.daa_score,accepting.blue_score)).unwrap();
    assert!(event.covers_claim_deletion_v25(&at,ordinal,id,&terminal).unwrap(),"actual accepting point must not be confused with its carrying block");
    assert!(!b.claim_local_retired_claim_events_v25.contains_key(&(id,accepting.block,accepting.daa_score,accepting.blue_score)),"claim identity index uses actual carried claim identity");

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

#[test]
fn rfc23s2_chain_checkpoint_scoped_red_material_without_f_cold_and_inverse() {
    let p=rules();let s1=parent(&p);let env=attempt(40,1);let id=attempt_id_v2(&env.attempt);
    let carrier=h64(0xB912);let point=ctx(2,100,2);let mut extras=extra();extras.merged_reds.insert(carrier);
    let merged=[PalwMergedWorkV1{carrying_block:carrier,work:PalwBlockWorkV3::Attempt(&env),execution_key:h64(0xE912),subsidy:0,escrow_carve:None,bits:0,job_anchor:Hash64::default()}];
    let admission=crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
    let (state,delta,skips)=apply_palw_transition_v7(&s1,&p,Some(&admission),&point,&[],PalwBlockWorkV3::None,&merged,Hash64::default(),false,false,false,false,&extras).unwrap();
    assert!(skips.is_empty());assert_eq!(state.claim(&id).unwrap().accepted_block,carrier);
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
    let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(state.state_root()),false,None).unwrap();
    let expected=super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&cold,&p,&point).unwrap().unwrap();
    let mut scoped=cold.clone();
    scoped.kernel_route.as_mut().unwrap().aux.retain(|(table,key),_|*table!=PALW_CLAIM_LOCAL_TABLE_V1||key.first()!=Some(&b'F'));
    // This removes only the historical full F in a diagnostic clone. It does
    // not assert that all production consumers/writers can yet collect F1.
    let projected=super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&scoped,&p,&point).unwrap().unwrap();
    assert_eq!(*expected,*projected,"A8 supplies exact local material independent of a whole F shadow");
    let record=&projected.records[&PalwClaimLocalWorkRefV1::ChainClaim{claim:id}];
    assert_eq!(record.accepting_block,point.block);assert_ne!(record.accepting_block,carrier);
    let (_,birth_frames):(Vec<(Hash64,PalwClaimStateV2,Vec<(Hash64,PalwCourtSessionStateV2)>)>,Vec<Hash64>)=borsh::from_slice(&projected.liability_state).unwrap();
    assert_eq!(birth_frames,vec![point.block]);
    let mut literal_ids=BTreeSet::new();
    let actual=super::super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(&scoped,id,scoped.claim(&id).unwrap(),|raw|{
        let mid=palw_liability_material_id_v1(raw);assert!(record.material.contains(&mid));assert_eq!(projected.material[&mid],raw);literal_ids.insert(mid);Ok(())
    }).unwrap();assert_eq!(actual,Some(point));assert!(literal_ids.len()>=6);
    let key=palw_claim_liability_frame_key_v1(point.block);let raw=&cold.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())];
    assert!(!record.material.contains(&palw_liability_material_id_v1(raw)),"complete F is not retained as Chain A8 material");
    let reverted=revert_delta_v2(&state,&delta,&p).unwrap();assert_eq!(reverted.state_root(),s1.state_root());
    assert_eq!(super::super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(&reverted,id,state.claim(&id).unwrap(),|_|panic!("reverted source must be absent")).unwrap(),None);
}

/// Actual core accepted work and journal drive this data factory. This is not
/// signed node ingress, a paid-plan installation, or public activation evidence.
#[test]
fn rfc23s2_cohort_v15_postblock_red_peers_cold_material_and_native_inverse(){
    use super::super::palw_claim_local_execution_cohort_archive_v15 as cohorts;
    use crate::palw_native_resource_v1::{native_value_requests_v1,reserve_native_requests_v1};
    let p=rules();let before=parent(&p);let point=ctx(2,100,2);let mut extras=extra();
    assert!(cohorts::quote_current_catalogue_scoped_geometry_v15(&before,h64(41)).unwrap().is_none(),"empty native catalogue is genuinely absent");
    let first=attempt(40,1501);let second=attempt(40,1502);
    // A genuinely admitted unrelated root makes ROOTS a multi-root directory.
    // All fields are set BEFORE native admission; no stored Claim is injected.
    let mut third=attempt(40,1503);third.attempt.execution_root=h64(0xE1503);
    let first_id=attempt_id_v2(&first.attempt);let second_id=attempt_id_v2(&second.attempt);let third_id=attempt_id_v2(&third.attempt);
    assert_eq!(first.attempt.execution_root,second.attempt.execution_root);
    let carriers=[h64(0xB1501),h64(0xB1502),h64(0xB1503)];extras.merged_reds.extend(carriers);
    let merged=[PalwMergedWorkV1{carrying_block:carriers[0],work:PalwBlockWorkV3::Attempt(&first),execution_key:h64(0xE1501),subsidy:0,escrow_carve:None,bits:0,job_anchor:Hash64::default()},
        PalwMergedWorkV1{carrying_block:carriers[1],work:PalwBlockWorkV3::Attempt(&second),execution_key:h64(0xE1502),subsidy:0,escrow_carve:None,bits:0,job_anchor:Hash64::default()},
        PalwMergedWorkV1{carrying_block:carriers[2],work:PalwBlockWorkV3::Attempt(&third),execution_key:h64(0xE1503),subsidy:0,escrow_carve:None,bits:0,job_anchor:Hash64::default()}];
    let admission=crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
    let (accepted,delta,skips)=apply_palw_transition_v7(&before,&p,Some(&admission),&point,&[],PalwBlockWorkV3::None,&merged,Hash64::default(),false,false,false,false,&extras).unwrap();
    assert!(skips.is_empty(),"all three actual merged claims admitted: {skips:?}");
    // The normal core tail already observed this complete native batch in the
    // UNHASHED census. Reuse its actual delta as evidence, never fabricate rows.
    let _journal=reserve_native_requests_v1(native_value_requests_v1(&delta).unwrap()).unwrap();
    let mut builder=TransitionBuilder::new(&accepted,&p,false,false,false,false,&extras);builder.entries=delta.entries.clone();
    let report=cohorts::capture_postblock_cohorts_v15(&mut builder,point,&[first_id,second_id,third_id]).unwrap();
    assert_eq!(report.complete_snapshots,3);assert_eq!(report.missing_legacy_census,0);
    for id in [first_id,second_id]{let original=builder.state.claim(&id).unwrap();let mut members=BTreeSet::new();
        assert!(cohorts::visit_snapshot_members_v15(&builder.state,id,original,|member,_|{members.insert(member);Ok(())}).unwrap());
        assert_eq!(members,BTreeSet::from([first_id,second_id]),"earlier same-block target includes later accepted peer");
        assert_eq!(cohorts::snapshot_point_v15(&builder.state,id,original).unwrap(),Some(point));assert_ne!(original.accepted_block,point.block);
    }
    let unrelated_geometry=cohorts::quote_current_catalogue_scoped_geometry_v15(&builder.state,third.attempt.execution_root).unwrap().unwrap();
    assert_eq!(unrelated_geometry.member_body_occurrences,1);assert_eq!(unrelated_geometry.actual_member_nodes,1,"single actual member under the other root");
    let cost=cohorts::quote_cohort_cost_v15(&builder.state,first.attempt.execution_root,point).unwrap();assert_eq!(cost.native_members,2);assert!(cost.publication_wire_max>0&&cost.undo_wire_max>0);assert!(cost.missing.current_catalogue_release_authority,"data geometry is not a funded finite-lifecycle claim");
    super::super::palw_claim_local_checkpoint_fold_v1::write_claim_local_checkpoint_v1(&mut builder,&before,&point,&[],PalwBlockWorkV3::None,&merged).unwrap();
    super::super::palw_claim_local_context_gc_v3::finish_fold_v3(&mut builder).unwrap();
    cohorts::validate_cohort_archives_v15(&builder.state).unwrap();
    cohorts::assert_snapshot_integrity_negatives_v15(&builder.state,&p,&extras,first_id,second_id);
    cohorts::assert_catalogue_physical_geometry_v15(&builder.state,&p,&extras,first_id,second_id);
    let _combined=reserve_native_requests_v1(native_value_requests_v1(&builder.entries).unwrap()).unwrap();
    let combined=PalwStateDeltaV2{point,entries:builder.entries.clone()};
    let root=builder.state.state_root();let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&builder.state)).unwrap();
    let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(root),false,None).unwrap();
    assert_eq!(cold.state_root(),root);cohorts::validate_cohort_archives_v15(&cold).unwrap();
    cohorts::assert_catalogue_physical_geometry_v15(&cold,&p,&extras,first_id,second_id);
    let expected=super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&cold,&p,&point).unwrap().unwrap();
    let mut no_f=cold.clone();no_f.kernel_route.as_mut().unwrap().aux.retain(|(table,key),_|*table!=PALW_CLAIM_LOCAL_TABLE_V1||key.first()!=Some(&b'F'));
    // Scoped data/coordinate projection only; other production consumers still
    // retain F and the writer is deliberately unchanged by this candidate.
    let scoped=super::super::palw_claim_local_checkpoint_fold_v1::expected_claim_local_checkpoint_v1(&no_f,&p,&point).unwrap().unwrap();assert_eq!(*expected,*scoped);
    assert_eq!(apply_delta_v2(&before,&combined,&p).unwrap().state_root(),root);
    assert_eq!(revert_delta_v2(&cold,&combined,&p).unwrap().state_root(),before.state_root());
}
#[test]
fn rfc23s2_cohort_v15_known_exact_a5_denial_survives_raw_reuse_and_f0(){
    use super::super::palw_claim_local_rewards_v1 as rewards;
    let p=rules().with_claim_retirement_daa(30).unwrap();let before=parent(&p);let mut extras=extra();extras.native_fact_ruleset=Some(kaspa_hashes::Hash::from_bytes([15;32]));
    let id=h64(0xFC);let birth=ctx(2,100,2);let old_object=fp_commit(0xFC,60,3);
    let apply=|s:&PalwChainStateV2,point:&PalwBlockContextV2,objects:&[PalwConsensusObjectV2]|apply_palw_transition_v2_with_extras(s,&p,point,objects,None,false,false,false,false,&extras).unwrap();
    let (old,_)=apply(&before,&birth,&[old_object.clone()]);let old_claim=old.claim(&id).unwrap().clone();
    let (voided,_)=apply(&old,&ctx(3,111,3),&[]);assert!(matches!(voided.claim(&id).unwrap().phase,PalwClaimPhaseV2::Voided{..}));
    let rebirth=ctx(4,142,4);let (new,_)=apply(&voided,&rebirth,&[old_object,fp_commit(0xFD,60,3)]);
    assert_eq!(new.claim(&id).unwrap().accepted_daa,142);
    let closed=super::super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(&new,id,&old_claim).unwrap().expect("actual timeout/retirement minted exact old source");assert!(!closed.permitted);assert_eq!(closed.birth,birth);
    // This native known-fact join is needed even without a Complete snapshot.
    assert!(super::super::palw_claim_local_execution_cohort_archive_v15::snapshot_point_v15(&new,id,new.claim(&id).unwrap()).unwrap().is_none());
    {
        use super::super::{palw_claim_local_compact_sources_v1 as compact,palw_claim_local_execution_cohort_census_v15 as census};
        let unrelated_id=h64(0xFB);let mut unrelated=fp_commit(0xFB,60,3);
        if let PalwConsensusObjectV2::FreePromptCommitted{execution_root,..}=&mut unrelated{*execution_root=h64(0x151515)}else{unreachable!()}
        let (two_live,_)=apply(&before,&birth,&[fp_commit(0xFC,60,3),unrelated]);
        let (two_voided,_)=apply(&two_live,&ctx(3,111,3),&[]);
        let (native_closed,_)=apply(&two_voided,&ctx(4,142,4),&[]);
        let original=compact::read(&native_closed,id).unwrap().expect("actual retirement without re-admission owns native S1 row");
        let target=&original.original;
        let unrelated_closed=compact::read(&native_closed,unrelated_id).unwrap().expect("other actual root also closed by native timeout/retirement");
        assert_ne!(unrelated_closed.original.execution_root,target.execution_root);
        {
            let mut diagnostic=native_closed.clone();let mut unrelated_key=[0;65];unrelated_key[0]=b'S';unrelated_key[1..].copy_from_slice(unrelated_id.as_byte_slice());
            // Only diagnostic native corruption: status/body came from the real
            // close above. A per-root reader must never hydrate this other root.
            diagnostic.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,unrelated_key.to_vec()),vec![0]);
            assert!(compact::shared_root_legacy_v15(&diagnostic,target).is_err(),"legacy global iterator reaches earlier unrelated corrupted S1");
            assert_eq!(census::compact_shared_root_v15(&diagnostic,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            let mut matching_key=[0;65];matching_key[0]=b'S';matching_key[1..].copy_from_slice(id.as_byte_slice());
            diagnostic.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,matching_key.to_vec()),vec![0]);
            assert!(census::compact_shared_root_v15(&diagnostic,target).is_err(),"matching native hash cannot be replaced by cache status");
        }
        assert_eq!(census::compact_shared_root_v15(&native_closed,target).unwrap(),Some(compact::shared_root_legacy_v15(&native_closed,target).unwrap()));
        let mut key=[0;65];key[0]=b'S';key[1..].copy_from_slice(id.as_byte_slice());
        {
            use crate::palw_native_resource_v1::{NativeDemandCheckerV1,NativeSnapshotResourceLimitsV1,NativeTransitionReservationV1};
            let off=p.clone().with_claim_local_settlement_from_daa(None);
            let mut inactive=TransitionBuilder::new(&native_closed,&off,false,false,false,false,&extras);
            let checker=NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1{memory_bytes:0,database_bytes:0});
            inactive.native_reservation=NativeTransitionReservationV1::for_checker_v1(&checker);
            // A dormant-path metadata observer neither decodes this deliberately
            // invalid prospective body nor admits any scalable index growth.
            let plan=census::prepare_compact_writer_v15(&mut inactive,PALW_CLAIM_LOCAL_TABLE_V1,&key,Some(&[0])).unwrap().unwrap();
            census::commit_compact_change_v15(&mut inactive.state,&plan);
            assert!(census::compact_shared_root_v15(&inactive.state,target).unwrap().is_none());
        }
        {
            use super::super::palw_claim_local_carrier_lineage_v9::atomic_source_metadata_v9 as atomic;
            let mut b=TransitionBuilder::new(&native_closed,&p,false,false,false,false,&extras);b.claim_local_lock_point_v6=Some(rebirth);
            let root=b.state.state_root();let entries=b.entries.len();let obs=super::super::palw_claim_local_object_spans_v2::checkpoint(&b);
            let failed:Result<(),PalwStateV2Error>=atomic(&mut b,|b|{
                b.write_kernel_row_encoded_v1::<compact::PalwClosedClaimSourceV1>(PALW_CLAIM_LOCAL_TABLE_V1,&key,None)?;
                assert_eq!(census::compact_shared_root_v15(&b.state,target)?,Some(rewards::RewardReadinessV1::Ready));
                b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key,Some(&*original))?;
                assert_eq!(census::compact_shared_root_v15(&b.state,target)?,Some(rewards::RewardReadinessV1::Denied));
                Err(PalwStateV2Error::DeltaMismatch("genuine metadata transaction refusal after S1 writes"))
            });assert!(failed.is_err());assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),entries);
            assert_eq!(b.compact_root_undo_v15.len(),obs.compact_undo_v15);assert_eq!(b.compact_root_inactive_undo_v15,obs.compact_inactive_v15);
            assert_eq!(census::compact_shared_root_v15(&b.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied),"rollback retains complete exact native index without rebuild");
            // Real local quota refusal from the actual writer's preparation API,
            // in the real carrier metadata wrapper, before native mutation.
            let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:0,database_bytes:0});
            let mut no_capacity=TransitionBuilder::new(&native_closed,&p,false,false,false,false,&extras);no_capacity.claim_local_lock_point_v6=Some(rebirth);
            no_capacity.native_reservation=crate::palw_native_resource_v1::NativeTransitionReservationV1::for_checker_v1(&checker);
            let refused:Result<(),PalwStateV2Error>=atomic(&mut no_capacity,|b|{let _=census::prepare_compact_writer_v15(b,PALW_CLAIM_LOCAL_TABLE_V1,&key,None)?;Ok(())});
            assert!(matches!(refused,Err(PalwStateV2Error::NativeResource(_))));assert_eq!(no_capacity.state.state_root(),root);assert!(no_capacity.entries.is_empty());assert!(no_capacity.compact_root_undo_v15.is_empty());
            assert_eq!(census::compact_shared_root_v15(&no_capacity.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            atomic(&mut b,|b|b.write_kernel_row_encoded_v1::<compact::PalwClosedClaimSourceV1>(PALW_CLAIM_LOCAL_TABLE_V1,&key,None)).unwrap();
            assert_eq!(census::compact_shared_root_v15(&b.state,target).unwrap(),Some(rewards::RewardReadinessV1::Ready));
            let checkpoint=b.try_checkpoint().unwrap();let saved_log=b.compact_root_undo_v15.len();
            b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key,Some(&*original)).unwrap();
            assert_eq!(census::compact_shared_root_v15(&b.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            b.restore(checkpoint);assert_eq!(b.compact_root_undo_v15.len(),saved_log);
            assert_eq!(census::compact_shared_root_v15(&b.state,target).unwrap(),Some(rewards::RewardReadinessV1::Ready),"private tuple4 checkpoint retains earlier genuine deletion/tombstone observation");
            atomic(&mut b,|b|b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key,Some(&*original))).unwrap();
            assert_eq!(census::compact_shared_root_v15(&b.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            census::finish_compact_observations_v15(&mut b);assert!(b.compact_root_undo_v15.is_empty());
            let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&b.state)).unwrap();
            let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(root),false,None).unwrap();
            assert_eq!(cold.state_root(),root);assert_eq!(census::compact_shared_root_v15(&cold,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            let delta=PalwStateDeltaV2{point:rebirth,entries:b.entries.clone()};let hot=apply_delta_v2(&native_closed,&delta,&p).unwrap();let inverse=revert_delta_v2(&hot,&delta,&p).unwrap();
            assert_eq!(hot.state_root(),root);assert_eq!(inverse.state_root(),root);assert_eq!(census::compact_shared_root_v15(&inverse,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
            // An actual dormant native edit restores its fixed complete flag on
            // failure without allocating any pointer-undo vector.
            let off=p.clone().with_claim_local_settlement_from_daa(None);let mut inactive=TransitionBuilder::new(&native_closed,&off,false,false,false,false,&extras);
            let failed:Result<(),PalwStateV2Error>=atomic(&mut inactive,|b|{b.write_kernel_row_encoded_v1::<compact::PalwClosedClaimSourceV1>(PALW_CLAIM_LOCAL_TABLE_V1,&key,None)?;assert!(census::compact_shared_root_v15(&b.state,target)?.is_none());Err(PalwStateV2Error::DeltaMismatch("dormant scoped refusal"))});
            assert!(failed.is_err());assert!(inactive.compact_root_undo_v15.is_empty());assert!(inactive.compact_root_inactive_undo_v15.is_none());assert_eq!(inactive.state.state_root(),root);
            assert_eq!(census::compact_shared_root_v15(&inactive.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
        }
        let mut builder=TransitionBuilder::new(&native_closed,&p,false,false,false,false,&extras);builder.claim_local_lock_point_v6=Some(rebirth);
        builder.write_kernel_row_encoded_v1::<compact::PalwClosedClaimSourceV1>(PALW_CLAIM_LOCAL_TABLE_V1,&key,None).unwrap();
        assert_eq!(census::compact_shared_root_v15(&builder.state,target).unwrap(),Some(rewards::RewardReadinessV1::Ready),"same-block native deletion removes old pointer");
        builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key,Some(&*original)).unwrap();
        assert_eq!(census::compact_shared_root_v15(&builder.state,target).unwrap(),Some(rewards::RewardReadinessV1::Denied),"same-block native restore returns exact original pointer");
        let delta=PalwStateDeltaV2{point:rebirth,entries:builder.entries.clone()};
        let replay=apply_delta_v2(&native_closed,&delta,&p).unwrap();let inverse=revert_delta_v2(&replay,&delta,&p).unwrap();
        assert_eq!(census::compact_shared_root_v15(&inverse,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
        let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&builder.state)).unwrap();
        let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(builder.state.state_root()),false,None).unwrap();
        assert_eq!(census::compact_shared_root_v15(&cold,target).unwrap(),Some(rewards::RewardReadinessV1::Denied));
        census::invalidate_compact_row_v15(&mut builder.state,PALW_CLAIM_LOCAL_TABLE_V1,&key);
        assert!(census::compact_shared_root_v15(&builder.state,target).unwrap().is_none());
        assert_eq!(compact::shared_root(&builder.state,target).unwrap(),compact::shared_root_legacy_v15(&builder.state,target).unwrap());
    }
    let later=super::super::palw_claim_local_source_admission_v8::manifest_reference_v9(&new,h64(0xFD),rebirth).unwrap().unwrap();
    let mut no_f=new.clone();no_f.kernel_route.as_mut().unwrap().aux.retain(|(table,key),_|*table!=PALW_CLAIM_LOCAL_TABLE_V1||key.first()!=Some(&b'F'));
    // A later unrelated source body is deliberately unavailable. Known exact
    // denial must stop before it is hydrated, rather than become local replay
    // success or disappear behind the newer raw-ID S1/source slot.
    let mut key=[0;65];key[0]=b'B';key[1..].copy_from_slice(later.data.as_byte_slice());no_f.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec()));
    assert_eq!(rewards::palw_claim_local_recorded_settlement_v1(&no_f,&p,id,new.claim(&id).unwrap(),142).unwrap(),PalwClaimLocalSettlementV1::Denied);
}


/// Actual Attempt admission/license/final/deadline retirement. No Claim,
/// source-origin or terminal history is inserted by the fixture.
#[test]
fn rfc23s2_owner18_actual_current_and_held_retirement_survive_cold_and_undo(){
    use super::super::palw_claim_local_legacy_terminal_v18 as owner;
    let p=rules().with_claim_retirement_daa(1).unwrap();let mut x=extra();x.native_fact_ruleset=Some(crate::Hash::from_bytes([18;32]));
    let s1=parent(&p);let env=attempt(40,1);let id=attempt_id_v2(&env.attempt);let born=ctx(2,100,2);
    let(s2,_)=step(&s1,&p,&born,Some(&env),&x).unwrap();
    assert!(owner::claim_checkpoint_overlay_ready_v18(&s2,&p,id,s2.claim(&id).unwrap()).unwrap());
    let bind=PalwConsensusObjectV2::PanelBound{claim:id,anchor:h64(77),seats:vec![PalwPanelSeatV2{bond:bond_key(1),operator_id:op_id(21)}]};
    let(s3,_)=apply_palw_transition_v2_with_extras(&s2,&p,&ctx(3,101,3),&[bind],None,false,false,false,false,&x).unwrap();
    let license=PalwConsensusObjectV2::ReceiptLicensed{claim:id,receipts:seat_says(true)};
    let(s4,_)=apply_palw_transition_v2_with_extras(&s3,&p,&ctx(4,102,4),&[license],None,false,false,false,false,&x).unwrap();
    let(s5,_)=step(&s4,&p,&ctx(5,124,5),None,&x).unwrap();let terminal=s5.claim(&id).unwrap().clone();assert!(matches!(terminal.phase,PalwClaimPhaseV2::Final{..}));
    let mut own_material=Vec::new();assert!(owner::visit_current_checkpoint_material_v18(&s5,&p,id,&terminal,|raw|{own_material.push(raw.to_vec());Ok(())}).unwrap());assert!(own_material.len()>=3);
    // Keep the genuine birth F1, remove only later full snapshots. The current
    // native receipt supplies the actual replacement, not an invented F1 codec.
    let mut partial=s5.clone();let birth_key=palw_claim_liability_frame_key_v1(born.block);partial.kernel_route.as_mut().unwrap().aux.retain(|(t,k),_|*t!=PALW_CLAIM_LOCAL_TABLE_V1||k.first()!=Some(&b'F')||k.as_slice()==birth_key);
    owner::validate_pending_v18(&partial,&p).unwrap();assert!(owner::claim_checkpoint_overlay_ready_v18(&partial,&p,id,&terminal).unwrap());
    let raw=borsh::to_vec(&PalwStateCarriageV2::from_state(&partial)).unwrap();let cold=borsh::from_slice::<PalwStateCarriageV2>(&raw).unwrap().into_state_v3(&p,Some(partial.state_root()),false,None).unwrap();assert_eq!(cold.state_root(),partial.state_root());
    // Existing native trace/terminal deadlines select the actual removal. GateC
    // is not active in this legacy fixture: missing approval remains Held.
    let at=ctx(6,1_000_000,6);let(retired,delta)=step(&s5,&p,&at,None,&x).unwrap();assert!(retired.claim(&id).is_none());
    // Exercise the production retirement boundary from the genuine folded
    // terminal history, with transient observation requested before removal.
    // This marker proves an event only; the missing GateC approval stays Held.
    {
        let mut observed=TransitionBuilder::new(&s5,&p,false,false,false,false,&x);
        observed.claim_local_lock_point_v6=Some(at);observed.claim_local_gc_defer_v3=true;
        observed.claim_local_capture_retired_exec_events_v24=true;
        let checkpoint=observed.checkpoint();let before=observed.state.state_root();
        observed.retire_claim_at_v18(&at,id,&terminal).unwrap();
        let ordinal=observed.entries.iter().position(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old:Some(c),new:None}if *key==id&&c==&terminal)).unwrap();
        let key=(id,terminal.accepted_block,terminal.accepted_daa,terminal.accepted_blue_score);
        let event=observed.claim_local_retired_claim_events_v25.get(&key).unwrap();
        assert!(event.covers_claim_deletion_v25(&at,ordinal,id,&terminal).unwrap());
        assert!(!event.covers_claim_deletion_v25(&at,ordinal+1,id,&terminal).unwrap());
        assert!(!event.covers_claim_deletion_v25(&ctx(7,at.daa_score+1,7),ordinal,id,&terminal).unwrap());
        let mut foreign=terminal.clone();foreign.trace_retention_daa+=1;
        assert!(!event.covers_claim_deletion_v25(&at,ordinal,id,&foreign).unwrap(),"same birth cannot lend a different full old terminal body");
        assert!(observed.claim_local_retired_exec_events_v24.is_empty(),"actual no-EXEC retirement creates no heavy decoded token");
        observed.restore(checkpoint);assert!(observed.claim_local_retired_claim_events_v25.is_empty());
        assert_eq!(observed.state.state_root(),before);assert!(observed.entries.is_empty());
    }

    assert!(owner::removed_checkpoint_overlay_ready_v18(&retired,&p,id,&terminal,&at).unwrap(),"actual Held retirement retains its complete native projection without economic Ready");
    assert!(!owner::removed_checkpoint_overlay_ready_v18(&retired,&p,id,&terminal,&ctx(7,at.daa_score+1,7)).unwrap(),"another native removal point cannot borrow it");
    let later=ctx(7,at.daa_score+1,7);let mut b=TransitionBuilder::new(&retired,&p,false,false,false,false,&x);b.claim_local_lock_point_v6=Some(later);
    assert!(!owner::chain_subject_checkpoint_overlay_ready_in_fold_v21(&b,&later,id).unwrap(),"a retained Pending alone supplies no selected-instance event seal");
    assert!(!owner::removed_exec_event_covered_v18(&retired,&p,&at,&terminal,id,PALW_EXEC_V2_TABLE_ROOTS_V1,id.as_byte_slice(),&[0]).unwrap(),"actual no-EXEC retirement cannot lend a root deletion body");
    assert!(owner::has_pending_v18(&retired,id,&terminal).unwrap());assert!(matches!(owner::readiness_for_source_v18(&retired,&p,at.daa_score,id,&terminal,id,&terminal).unwrap(),Some(super::super::palw_claim_local_rewards_v1::RewardReadinessV1::Hold(_))));
    owner::validate_pending_v18(&retired,&p).unwrap();let raw=borsh::to_vec(&PalwStateCarriageV2::from_state(&retired)).unwrap();let restored=borsh::from_slice::<PalwStateCarriageV2>(&raw).unwrap().into_state_v3(&p,Some(retired.state_root()),false,None).unwrap();assert_eq!(restored.state_root(),retired.state_root());assert!(owner::has_pending_v18(&restored,id,&terminal).unwrap());
    assert_eq!(revert_delta_v2(&retired,&delta,&p).unwrap().state_root(),s5.state_root());
}

#[test]
fn rfc23_native_cohort_baseline_real_genesis_cold_and_undo_not_first_ordinary_fold() {
    use super::super::palw_claim_local_native_cohort_baseline_v22 as baseline;
    let p=params().with_claim_local_settlement_from_daa(Some(0));
    let mut e=extra();e.native_fact_ruleset=Some(crate::Hash::from_bytes([23;32]));
    let empty=PalwChainStateV2::genesis();let root=h64(993);
    // An empty native parent is not enough: an ordinary first block cannot
    // manufacture the genesis provenance used for a previously unseen root.
    let (ordinary,_)=step(&empty,&p,&ctx(13,100,3),None,&e).unwrap();
    assert_eq!(baseline::expected_native_root_births_v22(&ordinary,root,ctx(14,101,4)).unwrap(),None);
    // The same actual full fold at its configured native genesis boundary is
    // the sole initializer. No Source8/Claim/source point is fabricated here.
    let g=ctx(12,0,0);
    let (born,delta)=apply_palw_transition_v2_with_extras(&empty,&p,&g,&register_class_and_bond(),None,false,false,false,false,&e).unwrap();
    assert_eq!(baseline::expected_native_root_births_v22(&born,root,ctx(14,1,1)).unwrap(),Some(0));
    let raw=borsh::to_vec(&PalwStateCarriageV2::from_state(&born)).unwrap();
    let restored=borsh::from_slice::<PalwStateCarriageV2>(&raw).unwrap().into_state_v3(&p,Some(born.state_root()),false,None).unwrap();
    assert_eq!(baseline::expected_native_root_births_v22(&restored,root,ctx(14,1,1)).unwrap(),Some(0));
    let undone=revert_delta_v2(&born,&delta,&p).unwrap();
    assert_eq!(undone.state_root(),empty.state_root());
    assert_eq!(baseline::expected_native_root_births_v22(&undone,root,ctx(14,1,1)).unwrap(),None);
}

/// Production PAV2 admission already has native8, but the old retention path
/// has not made15. A later actual Claim mutation migrates only its genuine F1.
#[test]
fn rfc23s2_source8_without15_migrates_exact_birth_f1_before_retirement_cold_and_undo(){
    use super::super::palw_claim_local_legacy_terminal_v18 as owner;
    use super::super::palw_claim_local_source_admission_v8 as source8;
    use super::super::palw_claim_local_execution_cohort_archive_v15 as source15;
    let p=rules().with_claim_retirement_daa(1).unwrap();
    let mut x=extra();x.native_fact_ruleset=Some(crate::Hash::from_bytes([18;32]));
    let s1=parent(&p);let env=attempt(40,1);let id=attempt_id_v2(&env.attempt);let birth=ctx(2,100,2);
    let(s2,_)=step(&s1,&p,&birth,Some(&env),&x).unwrap();
    let original=s2.claim(&id).unwrap();assert_ne!(original.execution_root,Hash64::default());
    assert!(source8::for_stored_claim_v8(&s2,id,original).unwrap().is_some());
    assert!(source15::alias_original_snapshot_v15(&s2,id,original).unwrap().is_none());
    let key=palw_claim_liability_frame_key_v1(birth.block);
    let raw=s2.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())].clone();
    // An unrelated real tick must retain the live source's actual birth F1.
    let(s3,_)=step(&s2,&p,&ctx(3,101,3),None,&x).unwrap();
    assert_eq!(s3.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())),Some(&raw));
    let bind=PalwConsensusObjectV2::PanelBound{claim:id,anchor:h64(77),seats:vec![PalwPanelSeatV2{bond:bond_key(1),operator_id:op_id(21)}]};
    let at=ctx(4,102,4);
    let(s4,delta4)=apply_palw_transition_v2_with_extras(&s3,&p,&at,&[bind],None,false,false,false,false,&x).unwrap();
    assert!(owner::immutable_frame_pinned_v18(&s4,birth.block).unwrap());
    assert_eq!(s4.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())),Some(&raw));
    assert!(source15::alias_original_snapshot_v15(&s4,id,s4.claim(&id).unwrap()).unwrap().is_none(),"migration does not manufacture a15 birth snapshot");
    let mut b=TransitionBuilder::new(&s4,&p,false,false,false,false,&x);b.claim_local_lock_point_v6=Some(at);
    assert!(owner::claim_native_ledger_ownership_ready_in_fold_v21(&b,&at,id,s4.claim(&id).unwrap()).unwrap());
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&s4)).unwrap();
    let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(s4.state_root()),false,None).unwrap();
    assert_eq!(cold.state_root(),s4.state_root());
    assert_eq!(revert_delta_v2(&s4,&delta4,&p).unwrap().state_root(),s3.state_root());
    let mut missing=s4.clone();missing.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec()));
    assert!(owner::claim_checkpoint_overlay_ready_v18(&missing,&p,id,s4.claim(&id).unwrap()).is_err(),"same native8 does not replace the pinned exact F1");
    let license=PalwConsensusObjectV2::ReceiptLicensed{claim:id,receipts:seat_says(true)};
    let(s5,_)=apply_palw_transition_v2_with_extras(&s4,&p,&ctx(5,103,5),&[license],None,false,false,false,false,&x).unwrap();
    let(s6,_)=step(&s5,&p,&ctx(6,125,6),None,&x).unwrap();let terminal=s6.claim(&id).unwrap().clone();
    assert!(matches!(terminal.phase,PalwClaimPhaseV2::Final{..}));
    let removed=ctx(7,1_000_000,7);let(s7,delta7)=step(&s6,&p,&removed,None,&x).unwrap();
    assert!(s7.claim(&id).is_none());assert!(owner::has_pending_v18(&s7,id,&terminal).unwrap());
    assert!(owner::immutable_frame_pinned_v18(&s7,birth.block).unwrap(),"Pending and its cohort acquire refs before Current releases its ref");
    assert_eq!(s7.kernel_route.as_ref().unwrap().aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())),Some(&raw));
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&s7)).unwrap();
    let cold=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state_v3(&p,Some(s7.state_root()),false,None).unwrap();
    assert_eq!(cold.state_root(),s7.state_root());
    assert_eq!(revert_delta_v2(&s7,&delta7,&p).unwrap().state_root(),s6.state_root());
    assert_eq!(apply_delta_v2(&s6,&delta7,&p).unwrap().state_root(),s7.state_root());
}



#[test]
fn rfc23_targeted_native_material_ignores_an_unrelated_birth_body(){
 use super::super::{palw_claim_local_retired_dependency_material_v24 as related,palw_claim_local_source_admission_v8 as source};
 let p=rules();let before=parent(&p);let mut x=extra();x.native_fact_ruleset=Some(crate::Hash::from_bytes([24;32]));
 let a=attempt(40,2401);let aid=attempt_id_v2(&a.attempt);let aborn=ctx(2,100,2);
 let(s2,_)=step(&before,&p,&aborn,Some(&a),&x).unwrap();
 let mut other=attempt(40,2402);other.attempt.execution_root=h64(0xA2402);let bid=attempt_id_v2(&other.attempt);let bborn=ctx(3,101,3);
 let(s3,_)=step(&s2,&p,&bborn,Some(&other),&x).unwrap();assert_ne!(s3.claim(&aid).unwrap().execution_root,s3.claim(&bid).unwrap().execution_root);
 let inspect=|s:&PalwChainStateV2|{let mut ids=BTreeSet::new();let complete=related::visit_consumer_material_v24(s,&p,aid,s.claim(&aid).unwrap(),|raw|{ids.insert(palw_liability_material_id_v1(raw));Ok(())}).unwrap();assert!(complete);ids};
 let expected=inspect(&s3);assert!(!expected.is_empty());
 let prospective=ctx(4,102,4);let root_a=s3.claim(&aid).unwrap().execution_root;
 let token=related::preflight_parent_cohort_material_v26(&s3,&p,&prospective,root_a).unwrap().expect("real existing parent cohort has complete transitive material");
 assert_eq!(token.execution_root_v26(),root_a);assert_eq!(token.parent_point_v26(),s3.last_point);assert_eq!(token.candidate_point_v26(),prospective);drop(token);

 let mut diagnostic=s3.clone();let value=source::manifest_reference_v9(&s3,bid,bborn).unwrap().unwrap();let mut key=[0u8;65];key[0]=b'B';key[1..].copy_from_slice(value.data.as_byte_slice());
 // Diagnostic corruption only after BOTH actual originals were admitted.
 // This is not a replacement positive or a fabricated Claim/source/point.
 diagnostic.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec()),vec![0]);
 assert!(source::for_stored_claim_v8(&diagnostic,bid,diagnostic.claim(&bid).unwrap()).is_err());
 assert_eq!(inspect(&diagnostic),expected,"one consumer never decodes/attaches another execution root's unrelated current source body");
 assert!(related::preflight_parent_cohort_material_v26(&diagnostic,&p,&prospective,root_a).unwrap().is_some(),"unrelated root body cannot block parent OWN preflight");
 let own=source::manifest_reference_v9(&s3,aid,aborn).unwrap().unwrap();let mut own_key=vec![b'B'];own_key.extend_from_slice(own.data.as_byte_slice());
 let mut missing_own=s3.clone();missing_own.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,own_key),vec![0]);
 assert!(!matches!(related::preflight_parent_cohort_material_v26(&missing_own,&p,&prospective,root_a),Ok(Some(_))),"actual related source absence refuses preflight before a new object is accepted");

}



/// Prove a historical subset from the complete CURRENT queried-root census,
/// without depending on unrelated-root evolution or projecting old snapshots.
#[test]
fn rfc23_native_cohort_historical_scope_authenticates_all_current_root_members_v25(){
    use super::super::palw_claim_local_native_cohort_baseline_v22 as baseline;
    use super::super::palw_claim_local_execution_cohort_census_v15 as census;
    use super::super::palw_claim_local_source_admission_v8 as source8;
    let p=params().with_claim_local_settlement_from_daa(Some(0));let mut e=extra();e.native_fact_ruleset=Some(crate::Hash::from_bytes([25;32]));
    let empty=PalwChainStateV2::genesis();let genesis=ctx(12,0,0);
    let (s0,_)=apply_palw_transition_v2_with_extras(&empty,&p,&genesis,&register_class_and_bond(),None,false,false,false,false,&e).unwrap();
    let root_a=h64(41);let root_b=h64(42);
    assert_eq!(baseline::expected_native_root_births_v22(&s0,root_a,genesis).unwrap(),Some(0));
    let a=attempt(40,1);let id_a=attempt_id_v2(&a.attempt);let point_a=ctx(31,100,1);assert_eq!(a.attempt.execution_root,root_a);
    let (s1,_)=step(&s0,&p,&point_a,Some(&a),&e).unwrap();assert!(s1.claim(&id_a).is_some());
    let mut b=attempt(40,2);b.attempt.execution_root=root_b;let id_b=attempt_id_v2(&b.attempt);let point_b=ctx(32,101,2);
    let (s2,_)=step(&s1,&p,&point_b,Some(&b),&e).unwrap();assert!(s2.claim(&id_b).is_some());
    // B advances global observed, but never this unrelated root's scalar scope.
    assert_eq!(baseline::expected_native_root_births_v22(&s2,root_a,point_a).unwrap(),Some(1));
    assert!(census::native_root_members_complete_v22(&s2,root_a,point_a).unwrap());
    let c=attempt(40,3);let id_c=attempt_id_v2(&c.attempt);let point_c=ctx(33,102,3);assert_eq!(c.attempt.execution_root,root_a);
    let (s3,delta3)=step(&s2,&p,&point_c,Some(&c),&e).unwrap();assert!(s3.claim(&id_c).is_some());
    // The scalar deliberately remains unavailable after THIS root advances.
    assert_eq!(baseline::expected_native_root_births_v22(&s3,root_a,point_a).unwrap(),None);
    let population=baseline::current_native_root_population_v25(&s3,root_a).unwrap().unwrap();assert_eq!(population.total_v25(),2);assert_eq!(population.observed_v25(),Some(point_c));
    // Stronger proof authenticates A+C against current total2, then the
    // existing historical visitor excludes later C from consumer A's scope.
    assert!(census::native_root_members_complete_v22(&s3,root_a,point_a).unwrap());
    let mut historical=BTreeSet::new();assert!(census::visit_birth_eligible_members_v15(&s3,root_a,point_a,|id,_|{historical.insert(id);Ok(())}).unwrap());assert_eq!(historical,BTreeSet::from([id_a]));
    let mut current=BTreeSet::new();assert!(census::visit_birth_eligible_members_v15(&s3,root_a,point_c,|id,_|{current.insert(id);Ok(())}).unwrap());assert_eq!(current,BTreeSet::from([id_a,id_c]));
    let original_ref=|state:&PalwChainStateV2,id|{let view=source8::for_stored_claim_v8(state,id,state.claim(&id).unwrap()).unwrap().unwrap();view.original_reference_v18()};
    let mut missing_b=s3.clone();let rv=original_ref(&missing_b,id_b);let mut key=vec![b'B'];key.extend_from_slice(rv.data.as_byte_slice());assert!(missing_b.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1,key)).is_some());
    assert!(census::native_root_members_complete_v22(&missing_b,root_a,point_a).unwrap(),"unrelated B body is outside queried A-root availability");
    let mut only_a=BTreeSet::new();assert!(census::visit_birth_eligible_members_v15(&missing_b,root_a,point_a,|id,_|{only_a.insert(id);Ok(())}).unwrap());assert_eq!(only_a,BTreeSet::from([id_a]));
    let mut missing_c=s3.clone();let rv=original_ref(&missing_c,id_c);let mut key=vec![b'B'];key.extend_from_slice(rv.data.as_byte_slice());assert!(missing_c.kernel_route.as_mut().unwrap().aux.remove(&(PALW_CLAIM_LOCAL_TABLE_V1,key)).is_some());
    assert!(!matches!(census::native_root_members_complete_v22(&missing_c,root_a,point_a),Ok(true)),"future-to-consumer C still belongs to the current completeness proof and cannot be omitted");
    let raw=borsh::to_vec(&PalwStateCarriageV2::from_state(&s3)).unwrap();let cold=borsh::from_slice::<PalwStateCarriageV2>(&raw).unwrap().into_state_v3(&p,Some(s3.state_root()),false,None).unwrap();assert!(census::native_root_members_complete_v22(&cold,root_a,point_a).unwrap());
    assert_eq!(revert_delta_v2(&s3,&delta3,&p).unwrap().state_root(),s2.state_root());assert_eq!(apply_delta_v2(&s2,&delta3,&p).unwrap().state_root(),s3.state_root());
}


#[test]
fn rfc23_native_cohort_current_completeness_preserves_actual_red_acceptor_v25(){
    use super::super::palw_claim_local_native_cohort_baseline_v22 as baseline;
    use super::super::palw_claim_local_execution_cohort_census_v15 as census;
    use super::super::palw_claim_local_source_admission_v8 as source8;
    let p=params().with_claim_local_settlement_from_daa(Some(0));let mut e=extra();e.native_fact_ruleset=Some(crate::Hash::from_bytes([25;32]));
    let empty=PalwChainStateV2::genesis();let genesis=ctx(12,0,0);
    let(s0,_)=apply_palw_transition_v2_with_extras(&empty,&p,&genesis,&register_class_and_bond(),None,false,false,false,false,&e).unwrap();
    assert_eq!(baseline::expected_native_root_births_v22(&s0,h64(41),genesis).unwrap(),Some(0));
    let env=attempt(40,31);let id=attempt_id_v2(&env.attempt);let carrier=h64(0xB925);let point=ctx(34,100,1);e.merged_reds.insert(carrier);
    let merged=[PalwMergedWorkV1{carrying_block:carrier,work:PalwBlockWorkV3::Attempt(&env),execution_key:h64(0xE925),subsidy:0,escrow_carve:None,bits:0,job_anchor:Hash64::default()}];
    let admission=crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
    let(state,delta,skips)=apply_palw_transition_v7(&s0,&p,Some(&admission),&point,&[],PalwBlockWorkV3::None,&merged,Hash64::default(),false,false,false,false,&e).unwrap();
    assert!(skips.is_empty(),"actual merged RED must be admitted: {skips:?}");assert_eq!(state.claim(&id).unwrap().accepted_block,carrier);assert_ne!(carrier,point.block);
    let origin=source8::for_stored_claim_v8(&state,id,state.claim(&id).unwrap()).unwrap().unwrap();assert_eq!(origin.accepting_point(),point);assert_eq!(origin.original_claim().accepted_block,carrier);drop(origin);
    let population=baseline::current_native_root_population_v25(&state,h64(41)).unwrap().unwrap();assert_eq!(population.total_v25(),1);assert_eq!(population.observed_v25(),Some(point));
    assert!(census::native_root_members_complete_v22(&state,h64(41),point).unwrap(),"stored carrying block must not be substituted for actual accepting point");
    let mut members=BTreeSet::new();assert!(census::visit_birth_eligible_members_v15(&state,h64(41),point,|actual,row|{assert_eq!(row.accepted_block,carrier);members.insert(actual);Ok(())}).unwrap());assert_eq!(members,BTreeSet::from([id]));
    let raw=borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();let cold=borsh::from_slice::<PalwStateCarriageV2>(&raw).unwrap().into_state_v3(&p,Some(state.state_root()),false,None).unwrap();
    assert!(census::native_root_members_complete_v22(&cold,h64(41),point).unwrap());assert_eq!(source8::for_stored_claim_v8(&cold,id,cold.claim(&id).unwrap()).unwrap().unwrap().accepting_point(),point);
    let wrong=PalwBlockContextV2{block:carrier,..point};assert!(!matches!(source8::for_accepting_point_v8(&cold,id,wrong),Ok(Some(_))),"carrier is not an alternate accepting block");
    assert_eq!(revert_delta_v2(&state,&delta,&p).unwrap().state_root(),s0.state_root());assert_eq!(apply_delta_v2(&s0,&delta,&p).unwrap().state_root(),state.state_root());
}
