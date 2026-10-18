//! RFC08 08-B (ADR-0184 §1) in the fold (`palw_native_liability_hold_fold_v1`; dormant fence `palw_native_liability_horizon_v1`,
//! kernel route aux 144): a held native fee is queued only past its horizon and only with no open dispute, is held once, and a
//! post-Final conviction inside the horizon refunds it to the requester; outside the horizon nothing can be refunded. Below the
//! fence the sweep reads nothing.
use super::super::palw_native_liability_hold_fold_v1::*;
use super::*;
use crate::palw_native_liability_hold_v1::*;

const JOB: [u8; 64] = [0x7B; 64];

fn with_route(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state
}

fn armed() -> PalwStateParamsV2 {
    params().with_settlement_once_from(Some(0)).with_native_liability_horizon_from(Some(100))
}

fn row(horizon: u64) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family: PALW_NATIVE_FAMILY_JOB_V39,
        job: JOB,
        claim: [0xC1; 64],
        producer: bond_key(5),
        queue: h64(0xA1).as_bytes(),
        payload: h64(0xB1).as_bytes(),
        amount: 7_000,
        income_root: JOB,
        income_kind: 11,
        budget: h64(0xD1).as_bytes(),
        refund_payload: h64(0xB2).as_bytes(),
        final_daa: 200,
        horizon_daa: horizon,
    }
}

fn queued(state: &PalwChainStateV2, key: [u8; 64]) -> Option<(Hash64, u64)> {
    state.pending_payouts.get(&Hash64::from_bytes(key)).map(|p| (p.payload, p.amount))
}

#[test]
fn the_hold_applies_only_to_claims_committed_past_the_fence() {
    let p = armed();
    assert!(!native_hold_applies_v1(&p, 99), "a claim committed below the fence keeps its old ruleset");
    assert!(native_hold_applies_v1(&p, 100));
    assert!(!native_hold_applies_v1(&params(), u64::MAX), "dormant: never");
    assert_eq!(native_horizon_v1(&p, 1_000), 1_000 + p.window_court(), "the signed deadline + window_court (RFC08 08-A's bound)");
}

#[test]
fn a_final_after_the_signed_horizon_needs_no_new_hold_or_extended_deadline() {
    let p = armed();
    let deadline = 1_000;
    let horizon = native_horizon_v1(&p, deadline);
    assert!(native_fee_needs_hold_v1(&p, 100, horizon - 1, deadline));
    assert!(native_fee_needs_hold_v1(&p, 100, horizon, deadline));
    assert!(!native_fee_needs_hold_v1(&p, 100, horizon + 1, deadline));
    assert!(!native_fee_needs_hold_v1(&p, 99, horizon, deadline));
    assert!(!native_fee_needs_hold_v1(&params(), 100, horizon, deadline));
}

#[test]
fn a_held_fee_is_queued_only_past_its_horizon_and_never_while_a_dispute_is_open() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &row(500)).unwrap();
    assert!(hold_native_fee_v1(&mut b, &row(500)).is_err(), "one hold per job");
    assert!(queued(&b.state, row(500).queue).is_none(), "the Final queued nothing");
    // At the horizon the claim is still accusable: nothing is released.
    release_held_native_fees_v1(&mut b, 500, |_, _| false).unwrap();
    assert!(held_native_fee_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB).is_some());
    assert!(native_accusable_after_final_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 500).is_some());
    // Past it, an open dispute holds the release.
    release_held_native_fees_v1(&mut b, 501, |_, _| true).unwrap();
    assert!(held_native_fee_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB).is_some(), "the verdict is awaited");
    // Past it with no dispute: released once, to the producer's payee, exactly the deferred amount.
    release_held_native_fees_v1(&mut b, 501, |_, _| false).unwrap();
    assert!(held_native_fee_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB).is_none());
    assert_eq!(queued(&b.state, row(500).queue), Some((h64(0xB1), 7_000)));
    release_held_native_fees_v1(&mut b, 502, |_, _| false).unwrap();
    assert_eq!(queued(&b.state, row(500).queue), Some((h64(0xB1), 7_000)), "released once");
    assert!(refund_native_fee_v1(&mut b, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 502).is_err(), "a released fee cannot be refunded");
}

#[test]
fn a_post_final_conviction_inside_the_horizon_refunds_the_requester_and_never_after() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &row(500)).unwrap();
    let refunded = refund_native_fee_v1(&mut b, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 500).unwrap();
    assert_eq!(refunded, row(500));
    assert!(held_native_fee_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB).is_none());
    assert!(queued(&b.state, row(500).queue).is_none(), "the producer is paid nothing");
    assert_eq!(queued(&b.state, native_refund_queue_v1(&refunded).as_bytes()), Some((h64(0xB2), 7_000)), "the requester's refund");
    release_held_native_fees_v1(&mut b, 10_000, |_, _| false).unwrap();
    assert!(queued(&b.state, row(500).queue).is_none(), "nothing is released after a refund");

    // Past the horizon the claim is no longer accusable: a refund is refused and the fee is released.
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    hold_native_fee_v1(&mut b, &row(500)).unwrap();
    assert!(native_accusable_after_final_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 501).is_none());
    assert!(refund_native_fee_v1(&mut b, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 501).is_err());
    // A horizon before the Final is refused at the hold.
    assert!(hold_native_fee_v1(&mut b, &PalwNativeHeldFeeRowV1 { job: [0x7C; 64], horizon_daa: 199, ..row(500) }).is_err());
}

#[test]
fn below_the_fence_the_sweep_reads_nothing() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.kernel_route.as_mut().unwrap().aux.insert(
        (PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_JOB_V39, &JOB)),
        borsh::to_vec(&row(10)).unwrap(),
    );
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    release_held_native_fees_v1(&mut b, 1_000, |_, _| false).unwrap();
    assert!(held_native_fee_v1(&b.state, PALW_NATIVE_FAMILY_JOB_V39, &JOB).is_some(), "dormant: untouched");
}

#[test]
fn open_disputes_do_not_starve_a_later_payable_fee() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let mut payable = row(500);
    for i in 0..=PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1 {
        let mut item = row(500);
        item.job[..8].copy_from_slice(&(i as u64).to_be_bytes());
        item.income_root = item.job;
        item.queue = h64(1000 + i as u64).as_bytes();
        hold_native_fee_v1(&mut b, &item).unwrap();
        payable = item;
    }
    release_held_native_fees_v1(&mut b, 501, |_, item| item.job != payable.job).unwrap();
    assert_eq!(queued(&b.state, payable.queue), Some((h64(0xB1), 7_000)));
    assert!(held_native_fee_v1(&b.state, payable.family, &payable.job).is_none());
    let still_held =
        b.state.kernel_route.as_ref().unwrap().aux.keys().filter(|(table, _)| *table == PALW_NATIVE_LIABILITY_HOLD_TABLE_V1).count();
    assert_eq!(still_held, PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1);
}

#[test]
fn an_unreadable_job_cannot_release_a_held_fee() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let item = row(500);
    hold_native_fee_v1(&mut b, &item).unwrap();
    release_held_native_fees_v1(&mut b, 501, |builder, item| {
        super::super::palw_native_job_fold_v39::native_dispute_open_v39(builder, item)
    })
    .unwrap();
    assert!(held_native_fee_v1(&b.state, item.family, &item.job).is_some());
    assert!(queued(&b.state, item.queue).is_none());
}

#[test]
fn payable_backlog_still_releases_at_most_the_per_block_bound() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    for i in 0..=PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1 {
        let mut item = row(500);
        item.job[..8].copy_from_slice(&(i as u64).to_be_bytes());
        item.income_root = item.job;
        item.queue = h64(1000 + i as u64).as_bytes();
        hold_native_fee_v1(&mut b, &item).unwrap();
    }
    release_held_native_fees_v1(&mut b, 501, |_, _| false).unwrap();
    assert_eq!(b.state.pending_payouts.len(), PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1);
    release_held_native_fees_v1(&mut b, 502, |_, _| false).unwrap();
    assert_eq!(b.state.pending_payouts.len(), PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1 + 1);
    release_held_native_fees_v1(&mut b, 503, |_, _| false).unwrap();
    assert_eq!(b.state.pending_payouts.len(), PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1 + 1);
}
