//! Actual atomic Lock regression; source-row controls are private projected
//! monetary inputs, not a paid source campaign or evidence of source independence.
use super::*;
use crate::palw_beacon_source_fee_v1::{
    PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5 as COST, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5 as FEE, PalwBeaconSourceCostBoundsV1,
};
use crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4 as TARGET;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
use misaka_palw_kernel::{
    beacon_source_fee::*,
    rows::TABLE_VERIFIER_PAY_V1,
    verifier_pay::{VERIFIER_PAY_SOURCE_FEE_FUNDING_V1, VerifierPayRowV1},
};

fn funded(state: &mut PalwChainStateV2, a: &mut PalwPaidClaimAuthorityV2) {
    a.sponsor = Some(a.producer);
    let context = a.context.id();
    let sponsor = palw_kernel_bond_id_v1(&a.producer);
    let fee = BeaconSourceFeeTargetV1 { target: a.source_target.clone(), sponsor, minimum_fee: 6, max_sources: 1 };
    let cost = PalwBeaconSourceCostBoundsV1 {
        execution_sompi: 1,
        carrier_sompi: 1,
        material_sompi: 1,
        public_court_sompi: 1,
        locked_capital_sompi: 1,
        minimum_net_receipt_sompi: 1,
        maximum_source_deposit_sompi: u64::MAX,
        maximum_liability_daa: u64::MAX,
        report_root: [31; 64],
    };
    let funding = BeaconSourceFeeFundingV1 {
        target: a.source_target.clone(),
        terms: BeaconSourceFeeTermsV1 { source: a.source_target.terms, sponsor, fee_per_source: 6, max_sources: 1 },
        funded_daa: 4,
        held: 6,
        allocated: 0,
        paid: 0,
        refunded: 0,
        unused_refunded: false,
    };
    let route = state.kernel_route.as_mut().unwrap();
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, context.to_vec()), borsh::to_vec(a).unwrap());
    route.aux.insert((TARGET, context.to_vec()), borsh::to_vec(&a.source_target).unwrap());
    route.aux.insert((FEE, context.to_vec()), borsh::to_vec(&fee).unwrap());
    route.aux.insert((COST, context.to_vec()), borsh::to_vec(&cost).unwrap());
    route.rows.insert(
        (TABLE_VERIFIER_PAY_V1, borsh::to_vec(&(VERIFIER_PAY_SOURCE_FEE_FUNDING_V1, context)).unwrap()),
        borsh::to_vec(&VerifierPayRowV1::SourceFeeFunding { funding }).unwrap(),
    );
    route.ledger_cache.clear();
}
fn run_lock(b: &mut TransitionBuilder<'_>, a: &PalwPaidClaimAuthorityV2) -> Result<(), PalwStateV2Error> {
    super::super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(
        b,
        &ctx(5, 5, 5),
        &a.producer,
        &PaidClaimObjectV2::Lock { enrollment: a.enrollment.id() },
    )
}
fn unchanged_refusal(state: PalwChainStateV2, p: &PalwStateParamsV2, extras: &PalwTransitionExtrasV1, a: &PalwPaidClaimAuthorityV2) {
    let before = state.state_root();
    let mut b = TransitionBuilder::new(&state, p, false, false, false, false, extras);
    assert!(matches!(run_lock(&mut b, a), Err(PalwStateV2Error::KernelRouteRefused(_))));
    assert_eq!(b.state.state_root(), before);
    assert!(!b.state.kernel_route().unwrap().aux.contains_key(&(PAID_CLAIM_LOCKS_V2, a.enrollment.id().to_vec())));
}
#[test]
fn genuine_unfunded_still_bootstraps_but_partial_and_malformed_source_rows_refuse() {
    let (state, p, extras, a) = setup();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    run_lock(&mut b, &a).unwrap();
    assert_eq!(
        b.state.kernel_route().unwrap().paid_claim_effective_lock_v6(&a).unwrap().unwrap().anchor,
        paid_claim_bootstrap_anchor_v2()
    );
    for table in [TARGET, FEE, COST] {
        let mut malformed = state.clone();
        malformed.kernel_route.as_mut().unwrap().aux.insert((table, a.context.id().to_vec()), vec![255]);
        unchanged_refusal(malformed, &p, &extras, &a);
    }
    let mut state = state;
    let mut a = a;
    funded(&mut state, &mut a);
    let route = state.kernel_route().unwrap();
    let ledger = route.ledger_checked_v1().unwrap();
    let checked = route.paid_claim_bootstrap_material_checked_v6(&a, &ledger).unwrap().unwrap();
    assert_eq!(*checked, route.paid_beacon_material_v5(&a.context.id()).unwrap().unwrap());
    drop(checked);
    drop(ledger);
    // A valid funded-but-still-empty source window keeps the existing private bootstrap semantics.
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    run_lock(&mut b, &a).unwrap();
    for table in [TARGET, FEE, COST] {
        let mut absent = state.clone();
        absent.kernel_route.as_mut().unwrap().aux.remove(&(table, a.context.id().to_vec()));
        unchanged_refusal(absent, &p, &extras, &a);
        let mut malformed = state.clone();
        malformed.kernel_route.as_mut().unwrap().aux.insert((table, a.context.id().to_vec()), vec![255]);
        unchanged_refusal(malformed, &p, &extras, &a);
    }
    let mut orphan = state.clone();
    let mut unsigned = a.clone();
    unsigned.sponsor = None;
    orphan
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec()), borsh::to_vec(&unsigned).unwrap());
    unchanged_refusal(orphan, &p, &extras, &unsigned);
    let mut missing_funding = state.clone();
    missing_funding
        .kernel_route
        .as_mut()
        .unwrap()
        .rows
        .remove(&(TABLE_VERIFIER_PAY_V1, borsh::to_vec(&(VERIFIER_PAY_SOURCE_FEE_FUNDING_V1, a.context.id())).unwrap()));
    unchanged_refusal(missing_funding, &p, &extras, &a);
    let mut wrong = state.clone();
    let route = wrong.kernel_route.as_mut().unwrap();
    let mut fee: BeaconSourceFeeTargetV1 = borsh::from_slice(route.aux.get(&(FEE, a.context.id().to_vec())).unwrap()).unwrap();
    fee.sponsor = [77; 64];
    route.aux.insert((FEE, a.context.id().to_vec()), borsh::to_vec(&fee).unwrap());
    unchanged_refusal(wrong, &p, &extras, &a);
}
#[test]
fn malformed_final_never_reopens_native_bootstrap_and_capacity_retry_keeps_roots() {
    let (mut state, p, extras, mut a) = setup();
    let mut bad = state.clone();
    bad.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_FINALS_V2, [90; 64].to_vec()), vec![255]);
    assert!(bad.kernel_route().unwrap().paid_claim_approved_final_exists_v2(), "legacy latch fails closed");
    unchanged_refusal(bad, &p, &extras, &a);
    funded(&mut state, &mut a);
    let before = state.state_root();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
    let result = crate::palw_claim_challenge_v2::with_bootstrap_checker_v6(checker.clone(), || run_lock(&mut b, &a));
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(b.state.state_root(), before);
    assert_eq!(checker.outstanding(), (0, 0));
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 20, database_bytes: 0 });
    crate::palw_claim_challenge_v2::with_bootstrap_checker_v6(checker.clone(), || run_lock(&mut b, &a)).unwrap();
    assert_eq!(checker.outstanding(), (0, 0));
    assert!(b.state.kernel_route().unwrap().paid_claim_effective_lock_v6(&a).unwrap().is_some());
}
