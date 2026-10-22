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
    // audit A-5: the table also holds each row's horizon index (prefix 0x03), so count the held rows (prefix 0x01) and the index.
    let count = |prefix: u8| {
        b.state
            .kernel_route
            .as_ref()
            .unwrap()
            .aux
            .keys()
            .filter(|(table, key)| *table == PALW_NATIVE_LIABILITY_HOLD_TABLE_V1 && key.first() == Some(&prefix))
            .count()
    };
    assert_eq!(count(crate::palw_native_liability_hold_v1::PALW_NATIVE_HELD_FEE_PREFIX_V1), PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1);
    assert_eq!(count(crate::palw_native_liability_hold_v1::PALW_NATIVE_HELD_FEE_HORIZON_PREFIX_V1), PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1);
}

/// **audit A-5: the sweep is indexed by horizon and respects the pending-payout ceiling.** Rows not yet due are never decoded past
/// the first future horizon; with the payout queue one short of its ceiling only one due fee is released this block, the other waits
/// (held, its index intact) and is released by a later block once there is room. Release deletes the row and its index together.
#[test]
fn audit_a5_release_walks_the_horizon_index_and_respects_the_payout_ceiling() {
    use crate::palw_native_liability_hold_v1::{PALW_NATIVE_HELD_FEE_HORIZON_PREFIX_V1, palw_native_held_fee_horizon_key_v1};
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let item = |n: u64, horizon: u64| {
        let mut r = PalwNativeHeldFeeRowV1 { horizon_daa: horizon, ..row(horizon) };
        r.job[..8].copy_from_slice(&n.to_be_bytes());
        r.income_root = r.job;
        r.queue = h64(5_000 + n).as_bytes();
        r
    };
    let (early_a, early_b, late) = (item(1, 300), item(2, 300), item(3, 900));
    for r in [&late, &early_a, &early_b] {
        hold_native_fee_v1(&mut b, r).unwrap();
    }
    let route = |b: &TransitionBuilder<'_>| b.state.kernel_route.clone().unwrap();
    assert!(route(&b).aux.contains_key(&(
        PALW_NATIVE_LIABILITY_HOLD_TABLE_V1,
        palw_native_held_fee_horizon_key_v1(late.horizon_daa, late.family, &late.job)
    )));
    // Fill the payout queue to one below its ceiling.
    for i in 0..(PALW_V2_MAX_PENDING_PAYOUTS - 1) as u64 {
        b.write_payout(h64(900_000 + i), Some(PalwPayoutV2 { payload: h64(7), amount: 1 })).unwrap();
    }
    release_held_native_fees_v1(&mut b, 301, |_, _| false).unwrap();
    let released = [early_a, early_b].iter().filter(|r| held_native_fee_v1(&b.state, r.family, &r.job).is_none()).count();
    assert_eq!(released, 1, "one payout row of room: one release this block");
    assert_eq!(b.state.pending_payouts.len(), PALW_V2_MAX_PENDING_PAYOUTS, "never past the ceiling");
    assert!(held_native_fee_v1(&b.state, late.family, &late.job).is_some(), "a future horizon is not due");
    // Room again: the waiting fee is released, the future one still is not; each release took its index with it.
    for i in 0..(PALW_V2_MAX_PENDING_PAYOUTS - 1) as u64 {
        b.write_payout(h64(900_000 + i), None).unwrap();
    }
    release_held_native_fees_v1(&mut b, 302, |_, _| false).unwrap();
    assert!([early_a, early_b].iter().all(|r| held_native_fee_v1(&b.state, r.family, &r.job).is_none()));
    assert!(held_native_fee_v1(&b.state, late.family, &late.job).is_some());
    let index: Vec<_> = route(&b)
        .aux
        .keys()
        .filter(|(t, k)| *t == PALW_NATIVE_LIABILITY_HOLD_TABLE_V1 && k.first() == Some(&PALW_NATIVE_HELD_FEE_HORIZON_PREFIX_V1))
        .cloned()
        .collect();
    assert_eq!(index, vec![(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, palw_native_held_fee_horizon_key_v1(900, late.family, &late.job))]);
    release_held_native_fees_v1(&mut b, 901, |_, _| false).unwrap();
    assert!(held_native_fee_v1(&b.state, late.family, &late.job).is_none());
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

#[test]
fn release_and_refund_remove_only_the_settled_producer_index_entry() {
    for refund in [false, true] {
        let p = armed();
        let extras = PalwTransitionExtrasV1::default();
        let mut parent = with_route(PalwChainStateV2::genesis());
        let index = borsh::to_vec(&(39u8, 22u8, row(500).producer)).unwrap();
        let other = [0x7C; 64];
        parent.kernel_route.as_mut().unwrap().aux.insert((112, index.clone()), borsh::to_vec(&vec![JOB, other]).unwrap());
        use misaka_palw_kernel::flat_quota11::{QuotaQuantitiesV11, QuotaSubjectSummaryV11, SubjectBindingV11, FUTURE_ACTION_KINDS_V11};
        let subject = QuotaSubjectSummaryV11 {
            binding: SubjectBindingV11 {
                version: 11, policy_root: [1;64], class_id: [2;64], class_scope9_root: [3;64],
                program_root: [4;64], parameter_root: [5;64], job_id: JOB, job_input_root: [6;64],
                resource10_root: [7;64], quota_plan_root: [8;64], actual_positions: 1,
                requester: [9;64], producer: [10;64], admitted_interval: 0, deadline_daa: 500,
            },
            committed: QuotaQuantitiesV11::default(), consumed: QuotaQuantitiesV11::default(),
            guarantee_remaining: QuotaQuantitiesV11::default(), future_attempts_used: [1; FUTURE_ACTION_KINDS_V11],
            closed: false, retained_protected_seals: 2,
        };
        let subject_key = borsh::to_vec(&(2u8, JOB)).unwrap();
        parent.kernel_route.as_mut().unwrap().aux.insert((92, subject_key.clone()),
            borsh::to_vec(&(subject.row_root(), &subject)).unwrap());
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &row(500)).unwrap();
        if refund {
            refund_native_fee_v1(&mut b, PALW_NATIVE_FAMILY_JOB_V39, &JOB, 500).unwrap();
        } else {
            release_held_native_fees_v1(&mut b, 501, |_, _| false).unwrap();
        }
        let remaining: Vec<[u8; 64]> = b.state.kernel_route.as_ref().unwrap().aux_row(112, &index).unwrap();
        assert_eq!(remaining, vec![other], "other jobs keep their collateral index slot");
        let (root, closed): ([u8;64], QuotaSubjectSummaryV11) =
            b.state.kernel_route.as_ref().unwrap().aux_row(92, &subject_key).unwrap();
        assert_eq!(root, closed.row_root());
        assert_eq!(closed, QuotaSubjectSummaryV11 { closed: true, ..subject },
            "actual release/refund closes retained capacity without refunding attempts or guarantees");
    }
}

// ---- G14 lane rfc08-hold: text29 and funded v11 held fees (same row, same sweep, family-dispatched side effects) ----

fn family_row(family: u8, horizon: u64) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 { family, ..row(horizon) }
}

/// Honest text29 / funded v11 Finals: held at Final, paid exactly once just past H (never at or before H), to the producer's payee.
#[test]
fn rfc08_hold_text29_and_funded11_are_paid_exactly_past_the_horizon() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let p = armed();
        let extras = PalwTransitionExtrasV1::default();
        let parent = with_route(PalwChainStateV2::genesis());
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        let r = family_row(family, 500);
        hold_native_fee_v1(&mut b, &r).unwrap();
        assert!(queued(&b.state, r.queue).is_none(), "the Final queued nothing");
        for now in [201, 400, 500] {
            release_held_native_fees_v1(&mut b, now, |_, _| false).unwrap();
            assert!(held_native_fee_v1(&b.state, family, &JOB).is_some(), "not before H (family {family}, now {now})");
            assert!(native_accusable_after_final_v1(&b.state, family, &JOB, now).is_some(), "accusable through H");
        }
        release_held_native_fees_v1(&mut b, 501, |_, _| false).unwrap();
        assert!(held_native_fee_v1(&b.state, family, &JOB).is_none());
        assert_eq!(queued(&b.state, r.queue), Some((h64(0xB1), 7_000)), "paid at H+1, the deferred amount");
        assert!(refund_native_fee_v1(&mut b, family, &JOB, 501).is_err(), "a released fee cannot be refunded");
    }
}

/// A conviction (court or DA default) within H refunds the requester whole; the producer is paid nothing, ever.
#[test]
fn rfc08_hold_text29_and_funded11_convicted_within_the_horizon_refund_the_requester() {
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11] {
        let p = armed();
        let extras = PalwTransitionExtrasV1::default();
        let parent = with_route(PalwChainStateV2::genesis());
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        let r = family_row(family, 500);
        hold_native_fee_v1(&mut b, &r).unwrap();
        let refunded = refund_native_fee_v1(&mut b, family, &JOB, 500).unwrap();
        assert_eq!(refunded, r);
        assert_eq!(
            native_refund_queue_v1(&refunded),
            native_refund_queue_of_v1(family, r.job, r.claim, r.refund_payload),
            "a family's collection re-derives the refund queue from its parts"
        );
        assert_eq!(queued(&b.state, native_refund_queue_v1(&refunded).as_bytes()), Some((h64(0xB2), 7_000)));
        release_held_native_fees_v1(&mut b, 10_000, |_, _| false).unwrap();
        assert!(queued(&b.state, r.queue).is_none(), "the producer is never paid after a refund");
        // Past H the claim is no longer accusable.
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        hold_native_fee_v1(&mut b, &r).unwrap();
        assert!(refund_native_fee_v1(&mut b, family, &JOB, 501).is_err());
    }
}

/// The refund queues of different families never collide for the same job id.
#[test]
fn rfc08_hold_refund_queues_are_per_family() {
    let a = native_refund_queue_of_v1(PALW_NATIVE_FAMILY_TEXT_V29, JOB, [1; 64], [2; 64]);
    let b = native_refund_queue_of_v1(PALW_NATIVE_FAMILY_FUNDED_V11, JOB, [1; 64], [2; 64]);
    let c = native_refund_queue_of_v1(PALW_NATIVE_FAMILY_JOB_V39, JOB, [1; 64], [2; 64]);
    assert!(a != b && b != c && a != c);
}

/// The production dispatcher fails closed: a held text29 / funded v11 / unknown-family row whose job cannot be read is never paid.
#[test]
fn rfc08_hold_the_dispute_dispatcher_fails_closed_on_an_unreadable_job() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11, 77u8] {
        let r = PalwNativeHeldFeeRowV1 { job: [family; 64], ..family_row(family, 500) };
        hold_native_fee_v1(&mut b, &r).unwrap();
        assert!(native_held_dispute_open_v1(&b, &r), "family {family}: no readable Final, no release");
    }
    release_held_native_fees_v1(&mut b, 501, native_held_dispute_open_v1).unwrap();
    for family in [PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_FAMILY_FUNDED_V11, 77u8] {
        assert!(held_native_fee_v1(&b.state, family, &[family; 64]).is_some());
    }
}

/// Below the fence (dormant on every preset, or a claim committed before it) text29 / funded v11 take the old immediate payout:
/// `native_fee_needs_hold_v1` is false, so their `settle` / `credit` arms run the pre-RFC08 writes byte for byte.
#[test]
fn rfc08_hold_below_the_fence_text29_and_funded11_keep_the_immediate_payout() {
    let deadline = 1_000;
    assert!(!native_fee_needs_hold_v1(&params(), 0, 500, deadline), "dormant fence");
    assert!(!native_fee_needs_hold_v1(&params(), u64::MAX, 500, deadline));
    assert!(!native_fee_needs_hold_v1(&armed(), 99, 500, deadline), "committed before the fence");
    assert!(native_fee_needs_hold_v1(&armed(), 100, 500, deadline));
}
