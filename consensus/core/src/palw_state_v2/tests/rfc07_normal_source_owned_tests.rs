//! Existing real kernel claim fixture +
//! actual native Fund / StageMessage / FundRound folds. No source independence,
//! selected approval or complete nonempty-source campaign is inferred.
use super::*;
use crate::config::params::ForkActivation;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeSnapshotResourceLimitsV1,
    NativeDemandLeaseV1, arm_native_logical_limits_v1, reserve_native_requests_v1};
use crate::rfc07_normal_source_owned_draft::with_normal_source_before_row_v1;
use misaka_palw_kernel::{hash::Digest, beacon_source_fee::BeaconSourceFeeTermsV1};

fn apply(b: &mut TransitionBuilder<'_>, daa: u64, signer: &PalwBondKeyV2, object: &PaidClaimObjectV2) {
    super::super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(b, &ctx(daa, daa, daa), signer, object).unwrap();
}
fn native_funded(staged: bool) -> (PalwChainStateV2, Digest, PalwPaidClaimAuthorityV2) {
    // The kernel FundBeaconSourceFee gate needs OPV in force (`fee_gate`,
    // "source fee fence not reached"; the inner refusal is a silent no-op so
    // the outer strict debit check fired). Existing interim fence terms only.
    let opv = crate::palw_panel_free_v1::PalwPanelFreeFenceV1::at(ForkActivation::always()).opv_policy();
    let (state, p, mut extras, a) = setup_mode_contract_opv(staged, staged, Some(opv));
    let route_extras = extras.kernel_route.as_mut().unwrap();
    route_extras.beacon_source = Some(crate::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1 {
        activation: ForkActivation::always(), policy: a.source_policy,
    });
    let mut fee = crate::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::always());
    // Private arithmetic controls only, not measurements or approved prices.
    fee.policy.cost_bounds.as_mut().unwrap().maximum_source_deposit_sompi = u64::MAX;
    fee.policy.cost_bounds.as_mut().unwrap().maximum_liability_daa = u64::MAX;
    route_extras.beacon_source_fee = Some(fee);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let enrollment = a.enrollment.id();
    let (context, target, at) = if staged {
        apply(&mut b, 5, &a.producer, &PaidClaimObjectV2::StageMessage {
            enrollment, round: 0, prior_transcript: [0;64], message_root: [23;64],
        });
        let round = b.state.kernel_route().unwrap().paid_claim_round_v3(&enrollment, 0).unwrap();
        (round.context.id(), round.target, 5)
    } else { (a.context.id(), a.source_target.clone(), 4) };
    let terms = BeaconSourceFeeTermsV1 { source: target.terms, sponsor: palw_kernel_bond_id_v1(&a.producer),
        fee_per_source: fee.policy.minimum_fee_for(&target).unwrap(), max_sources: 1 };
    let before = b.state.bonds[&a.producer].collateral;
    let object = if staged { PaidClaimObjectV2::FundRound { enrollment, round: 0, terms } }
        else { PaidClaimObjectV2::Fund { enrollment, terms } };
    apply(&mut b, at, &a.producer, &object);
    let ledger = b.state.kernel_route().unwrap().ledger_checked_v1().unwrap();
    let funded = ledger.beacon_source_fees.get(&context).expect("actual native Fund wrote authenticated funding");
    assert_eq!(funded.terms, terms);
    assert_eq!(funded.held, terms.fee_per_source * u64::from(terms.max_sources));
    // The native exact funding branch itself verifies its strict same-R debit;
    // this outer control also verifies native state was affected.
    assert!(b.state.bonds[&a.producer].collateral <= before);
    drop(ledger);
    (b.state, context, a)
}

fn compare_normal(state: &PalwChainStateV2, context: &Digest) {
    let route = state.kernel_route().unwrap();
    let before = state.state_root();
    let old = route.paid_beacon_material_v5(context).unwrap().unwrap();
    let new = route.rfc07_normal_paid_material_owned_v5(context).unwrap().unwrap();
    assert_eq!(*new, old, "same contract, complete source inventory and promises");
    assert_eq!(*route.rfc07_normal_opt_in_sources_owned_v4(context).unwrap(), route.beacon_opt_in_sources_v4(context).unwrap());
    assert_eq!(*route.rfc07_normal_finals_owned_v1().unwrap(), route.finals_read_v1().unwrap());
    let old = route.paid_claim_source_authority_v3(context).unwrap();
    let new = route.rfc07_normal_source_authority_checked_v3(context).unwrap();
    assert_eq!(new.context(), &old.context);
    assert_eq!(new.target(), &old.source_target);
    assert_eq!(new.authority().source_policy, old.source_policy);
    assert_eq!(new.authorized_daa(), old.authorized_daa);
    assert_eq!(new.sponsor(), old.sponsor);
    assert_eq!(new.authority().enrollment.base_claim, old.base_claim);
    assert_eq!(state.state_root(), before);
}
#[test]
fn rfc07_normal_owned_initial_native_funding_matches_legacy_inventory() {
    let (state, context, _) = native_funded(false);
    compare_normal(&state, &context);
}
#[test]
fn rfc07_normal_owned_staged_native_message_and_funding_match_legacy() {
    let (state, context, _) = native_funded(true);
    compare_normal(&state, &context);
}
#[test]
fn rfc07_normal_owned_primary_absence_is_the_only_staged_fallback() {
    let (mut state, context, _) = native_funded(true);
    assert!(!state.kernel_route().unwrap().aux.contains_key(&(PAID_CLAIM_ENROLLMENTS_V2, context.to_vec())));
    assert!(state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context).is_ok());
    state.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_ENROLLMENTS_V2, context.to_vec()), vec![255]);
    assert!(matches!(state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context), Err(E::Decode(_))),
        "present corrupt primary cannot fall back to the valid staged row");
    state.kernel_route.as_mut().unwrap().aux.remove(&(PAID_CLAIM_ENROLLMENTS_V2, context.to_vec()));
    assert!(state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context).is_ok());
    let missing = [0x99;64];
    assert!(matches!(state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&missing), Err(E::Decode(_))));
}
#[test]
fn rfc07_normal_owned_staged_index_and_round_corruption_are_decode() {
    let (state, context, _) = native_funded(true);
    let index_key = paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, &context);
    let index: PalwPaidClaimIndexV3 = borsh::from_slice(&state.kernel_route().unwrap().aux[&(PAID_CLAIM_ROUNDS_V2, index_key.clone())]).unwrap();
    for key in [index_key, index.target.to_vec()] {
        let mut bad = state.clone();
        bad.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_ROUNDS_V2, key.clone()), vec![255]);
        assert!(matches!(bad.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context), Err(E::Decode(_))));
        bad.kernel_route.as_mut().unwrap().aux.remove(&(PAID_CLAIM_ROUNDS_V2, key));
        assert!(matches!(bad.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context), Err(E::Decode(_))));
    }
}
#[test]
fn rfc07_normal_owned_frozen_targets_and_costs_refuse_corrupt_or_missing_rows() {
    use crate::palw_beacon_source_fee_v1::{PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5};
    use crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4;
    let (state, context, _) = native_funded(false);
    for table in [PALW_BEACON_SOURCE_TABLE_TARGETS_V4, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5] {
        for missing in [false, true] {
            let mut bad = state.clone();
            let r = bad.kernel_route.as_mut().unwrap();
            if missing { r.aux.remove(&(table, context.to_vec())); } else { r.aux.insert((table, context.to_vec()), vec![255]); }
            let before = bad.state_root();
            assert!(matches!(bad.kernel_route().unwrap().rfc07_normal_paid_material_owned_v5(&context), Err(E::Decode(_))));
            assert_eq!(bad.state_root(), before);
        }
    }
}

// Wide logical ceiling (see paid_check_authority_resource_v2): base reads must
// pass; only the hook's independent owner exhausts the remainder.
const LIMIT: u64 = 1 << 40;
fn free_requests() -> u64 {
    let (mut low, mut high) = (0, LIMIT);
    while low < high {
        let mid = low + (high-low+1)/2;
        if let Ok(owner) = reserve_native_requests_v1(mid) { drop(owner); low=mid; } else { high=mid-1; }
    }
    low
}
fn child(name: &str, flag: &str) -> bool {
    if std::env::var_os(flag).is_some() { return true; }
    let qualified = format!("{}::{name}", module_path!());
    let exact = qualified.split_once("::").unwrap().1;
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact",exact,"--nocapture","--test-threads=1"])
        .env(flag,"1").env("RUST_MIN_STACK","67108864").output().unwrap();
    let stdout=String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");eprint!("{}",String::from_utf8_lossy(&output.stderr));
    assert!(stdout.lines().any(|line| line == "running 1 test"));
    assert!(output.status.success()); false
}
#[test]
fn rfc07_normal_owned_initial_and_staged_resource_preserve_rows_and_retry() {
    const NAME: &str="rfc07_normal_owned_initial_and_staged_resource_preserve_rows_and_retry";
    if !child(NAME,"MISAKA_NORMAL_SOURCE_RESOURCE_CHILD_V1") { return; }
    // Build all native states before globally enabling the isolated quota.
    let states = [native_funded(false), native_funded(true)];
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: LIMIT }));
    for (state,context,_) in states {
        use std::{cell::{Cell,RefCell},rc::Rc};
        let staged = !state.kernel_route().unwrap().aux.contains_key(&(PAID_CLAIM_ENROLLMENTS_V2,context.to_vec()));
        let reached=Rc::new(Cell::new(false)); let held:Rc<RefCell<Option<NativeDemandLeaseV1>>>=Rc::new(RefCell::new(None));
        let (seen,blocker)=(reached.clone(),held.clone()); let before=state.state_root();
        let result=with_normal_source_before_row_v1(move |table,key| {
            let target = if staged { table==PAID_CLAIM_ROUNDS_V2 && key.len()==64 }
                else { table==PAID_CLAIM_ENROLLMENTS_V2 && key==context.as_slice() };
            if target && !seen.replace(true) { *blocker.borrow_mut()=reserve_native_requests_v1(free_requests()).unwrap(); }
        },||state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context));
        assert!(reached.get(),"the precise initial/staged read boundary must be reached");
        assert!(matches!(result,Err(E::Resource(_))));
        assert_eq!(state.state_root(),before);
        drop(held.borrow_mut().take());
        assert!(state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context).is_ok());
        assert_eq!(state.state_root(),before);
    }
}
#[test]
fn rfc07_normal_owned_authority_ticket_survives_to_last_use() {
    const NAME:&str="rfc07_normal_owned_authority_ticket_survives_to_last_use";
    if !child(NAME,"MISAKA_NORMAL_SOURCE_OWNER_CHILD_V1") { return; }
    let states=[native_funded(false),native_funded(true)];
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: LIMIT }));
    for (state,context,_) in states {
        let before=free_requests();
        let owner=state.kernel_route().unwrap().rfc07_normal_source_authority_checked_v3(&context).unwrap();
        let held=free_requests(); assert!(held<before,"actual decoded context/profile ownership occupies quota");
        assert_eq!(owner.context().id(),context);
        assert!(reserve_native_requests_v1(before).is_err(),"last field use still retains the original owner");
        drop(owner);assert_eq!(free_requests(),before,"dropping the authority releases its ticket exactly once");
    }
}
