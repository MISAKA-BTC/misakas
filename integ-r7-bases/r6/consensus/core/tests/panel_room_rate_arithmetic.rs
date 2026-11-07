//! **The rate rule's pure functions: degenerate inputs refuse, an exact fit keeps its last claim,
//! a product that does not fit refuses, reachable sizes do not saturate, and the 2M figures** —
//! 2026-09-24 audit #4 review items 1, 3, 4 and 6.
//!
//! `palw_panel_demand_term_v1` (one class's owed claims × its claim's replay over its window, in
//! Q32, rounded up) and `palw_panel_capacity_by_rate_v1` (the claims of a class the budget holds
//! beside every other class's term). At e93be0f2 the room was a floored quotient of the budget
//! less a rounded-up demand that included the class's own term, and both functions saturated:
//! * an exact fit lost its last claim (rooms `[3, 1, 0]` where the exact rule gives `[3, 2, 1]`);
//! * a saturated term was divided by the window afterwards (≈ `MAX / w`, an undercount), and a
//!   saturated divisor overstated the room — the opposite of the doc's "saturating refuses";
//! * one step-3 reservation could leave the step-4 attempt no room (item 4).
//! Past the review every product is checked: a term that does not fit is `u128::MAX` and a
//! capacity that does not fit is 0, and room = capacity − owed.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_rate_arithmetic

use kaspa_consensus_core::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
use kaspa_consensus_core::palw_state_v2::palw_inflight_claims_counted_v1;
use kaspa_consensus_core::palw_work_target_v1::{
    PALW_PANEL_DEMAND_SCALE_V1 as Q, palw_panel_capacity_by_rate_v1 as capacity, palw_panel_demand_term_v1 as term,
    palw_panel_room_v1 as room_old,
};

/// A class's room: its capacity beside `others` less what it owes.
fn room(per_span: u128, others: u128, window: u64, cost: u128, owed: u128) -> u128 {
    (capacity(per_span, others, window, cost) as u128).saturating_sub(owed)
}

/// Degenerate inputs: every one refuses.
#[test]
fn degenerate_inputs_refuse() {
    // per_span 0 (ready 0).
    assert_eq!(capacity(0, 0, 3, 1_000), 0);
    // cost 0.
    assert_eq!(capacity(1_000_000, 0, 3, 0), 0);
    // other classes' demand above the budget.
    assert_eq!(capacity(1_000, 1_000 * Q + 1, 3, 1), 0);
    // window 0 read as 1, both sides.
    assert_eq!(term(1, 10, 0), 10 * Q);
    assert_eq!(capacity(10, 0, 0, 10), 1);
}

/// An exact fit keeps every claim: the rate rule's rooms are the common-horizon rule's with the
/// same single window.
#[test]
fn an_exact_fit_keeps_its_last_claim() {
    let (per_span, w, cost) = (1_000u128, 3u64, 1_000u128);
    let mut new = vec![];
    let mut old = vec![];
    for n in 0..3u128 {
        new.push(room(per_span, 0, w, cost, n) as u64);
        old.push(room_old(per_span, w, n * cost, cost));
    }
    println!("exact fit m=3: rate rule {new:?}, common-horizon (same window) {old:?}");
    assert_eq!(old, vec![3, 2, 1]);
    assert_eq!(new, vec![3, 2, 1], "the third claim of an exactly-fitting budget is admitted");
}

/// A product that does not fit refuses (review item 6): the term is `u128::MAX` rather than
/// ≈ `MAX / w`, and a capacity whose Q32 image does not fit is 0.
#[test]
fn a_term_or_a_capacity_that_does_not_fit_refuses() {
    let t = term(u64::MAX as u128, 1u128 << 100, 2);
    println!("the term of 2^64 claims × 2^100 over window 2 = {t:#x}");
    assert_eq!(t, u128::MAX, "a term that does not fit is a demand no budget holds");
    // A class whose per-span capacity's Q32 image is huge still admits nothing against it.
    assert_eq!(capacity(1u128 << 96, t, 2, 1), 0);
    // A claim_replay whose Q32 image does not fit: the true room floor(2^96 × 2 / 2^127) is 0.
    assert_eq!(capacity(1u128 << 96, 0, 2, 1u128 << 127), 0);
    // And a per-span budget whose Q32 image does not fit admits nothing either.
    assert_eq!(capacity(1u128 << 97, 0, 2, 1), 0);
}

/// Reachable magnitudes do not saturate: 2M-scale cost (~1.68e16 × the seat factor), window
/// 2,799, u32::MAX claims, window 1, the whole u32 ready range at 2.4e12 × 0.7.
#[test]
fn reachable_magnitudes_do_not_saturate() {
    let cost_2m = 16_800_000_000_000_000u128;
    assert!(term(u32::MAX as u128 + 2, cost_2m, 1) < u128::MAX);
    assert!(term(u32::MAX as u128 + 2, cost_2m, 2799) < u128::MAX);
    let per_span_max = (u32::MAX as u128) * 2_400_000_000_000 * 700 / 1_000;
    assert!(per_span_max.checked_mul(Q).is_some());
    assert!(capacity(per_span_max, 0, 2_799, cost_2m) > 0);
    println!("per_span_max*Q = 2^{:.1}", ((per_span_max * Q) as f64).log2());
}

/// Courted free-prompt claims pool with the pending ones (review item 3): `palw_panel_owed_v1` now
/// collects `(attempts, claims, quanta)` per class across the pending claims and the licensed ones
/// under a court and counts them once, where e93be0f2 rounded each courted claim up to a whole job
/// on its own. The fold is measured in `panel_room_courted_fp_claims_pool_per_class`.
#[test]
fn courted_fp_claims_count_as_the_one_pooled_job() {
    let per_job = 8u64;
    // Eight one-quantum commitments, pending: one job.
    let before = palw_inflight_claims_counted_v1(0, 8, 8, per_job, true);
    // All eight licensed with a court on each (none pending), pooled: still one job …
    let (pending, courted) = ((0u64, 0u64), (8u64, 8u64));
    let pooled = palw_inflight_claims_counted_v1(0, pending.0 + courted.0, pending.1 + courted.1, per_job, true);
    // … where rounding each courted claim alone charged eight.
    let per_claim: u32 = (0..8).map(|_| palw_inflight_claims_counted_v1(0, 1, 1, per_job, true)).sum();
    println!("pending {before} job(s); courted, pooled {pooled}; each courted claim rounded alone {per_claim}");
    assert_eq!((before, pooled), (1, 1));
    assert_eq!(per_claim, 8);
    // Two four-quanta claims, one pending and one licensed under a court: one job, not two.
    assert_eq!(palw_inflight_claims_counted_v1(0, 1 + 1, 4 + 4, per_job, true), 1);
}

/// The 2M figures the review names: cost 1.68e16 (economic × 5 seats), window 2,799, 700 ‰. The
/// rate alone holds 1/2/2/2/3/4 2M claims at 7/8/9/10/11/15 ready seats; the gate holds the 2M row
/// to its cap of 1 (asserted through the fold in `panel_room_2m_cap_holds_beside_the_short_class`).
#[test]
fn the_2m_capacity_figures() {
    let g = PALW_REGISTRY_GLOBALS_V1;
    let cost = 16_800_000_000_000_000u128;
    let w = 2799u64;
    let mut out = vec![];
    for ready in [7u128, 8, 9, 10, 11, 15] {
        let per_span = ready * g.reference_work_per_span * 700 / 1_000;
        out.push((ready, capacity(per_span, 0, w, cost), room_old(per_span, 3, 0, cost)));
    }
    println!("2M (ready, rate capacity, old room with a window-3 class admitting): {out:?}");
    assert_eq!(out.iter().map(|r| r.1).collect::<Vec<_>>(), vec![1, 2, 2, 2, 3, 4]);
    assert_eq!(out[1].2, 0, "the old rule with any short-window class admitting admitted none");
}

/// The step-3 reservation implies the step-4 room across classes of different windows too (review
/// item 4): class A (cost 1, window 3) with two jobs pending and the own attempt reserved takes a
/// two-job commitment; class B owes one claim of cost 715,827,882 over window 2,147,483,647 (its
/// term 1,431,655,765). per_span 2. At e93be0f2 step 3 read room 2 and step 4 read 0.
#[test]
fn the_step_three_reservation_implies_the_step_four_room() {
    let (c, w) = (1u128, 3u64);
    let other = term(1, 715_827_882, 2_147_483_647);
    assert_eq!(other, 1_431_655_765);
    let per_span = 2u128;
    let step3 = room(per_span, other, w, c, 2 + 1);
    let step4 = room(per_span, other, w, c, 2 + 2);
    println!("rate rule: step-3 room {step3} (needs 2), step-4 room {step4} (needs 1)");
    assert!(step3 >= 2, "the commitment is admitted with the attempt reserved");
    assert!(step4 >= 1, "and the block's own attempt still meets its room at step 4");
}
