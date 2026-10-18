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
