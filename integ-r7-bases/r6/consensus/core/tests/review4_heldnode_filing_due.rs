//! **Regression (feat/t12-aheld-node fourth review, MEDIUM on the node): a seat's court filing is dated
//! before the claim's earliest `Final`, not only before its receipt deadline.**
//!
//! kaspad dated a seat's named-leaf pursuit and its `ShardCourtAccused` at `palw_seat_court_filing_due_v1`
//! = the seat duty's receipt deadline − 60. It assumed the receipt deadline was never later than the
//! claim's earliest `Final`. Past §4-quater that is false:
//!
//! * the receipt deadline is `bound + max(window_receipt, D(c))`;
//! * the licensed `Final` floor is `max(L + window_challenge_at(L), bound + D(c) + 1)`.
//!
//! For a class whose `D(c)` is shorter than `window_receipt` (testnet-12's 8k row: `D = 15`), a licence
//! at `bound + 1` puts `Final` at `bound + 121`, and the filing was dated `bound + 540`. The node now
//! dates the pursuit `min(duty deadline, earliest Final) − 60` (or now, once that has passed). The
//! one-move accusation is due now. The earliest `Final` it reads is `min over L > bound of L + wc(L)`
//! (`palw_seat_claim_earliest_final_v1`).
//!
//! This pins the chain's side of that date on testnet-12's own params, for every genesis held class:
//! the earliest-`Final` bound is never after DL-1's floor at any licence the claim can take while its
//! receipts count, so a filing dated a margin before it lands before every `Final`. It also records
//! why the duty's deadline alone was wrong: some genesis held class's receipt deadline − 60 falls after
//! that `Final`. kaspad's `the_seats_court_filings_are_dated_in_the_priority_lane` checks the node's own
//! function against the same params.

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::config::params::palw_t12_genesis_held_class_ids_v1;
use kaspa_consensus_core::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1;

/// The node's landing margin (`PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1`).
const MARGIN: u64 = 60;

#[test]
fn review4_a_seat_filing_dated_before_the_earliest_final_lands_before_every_final_on_testnet_12() {
    let p = t12();
    let sp = bundle(&p).state.clone();
    let s = genesis_state(&p);
    let bound = 1_000u64;
    assert!(sp.class_verify_deadline_active_at(bound), "testnet-12 arms §4-quater from genesis");
    // The node's bound: a licence at `bound + 1`, or at the short window's fence, whichever ends first.
    let first = bound + 1;
    let mut earliest = first + sp.window_challenge_at(first);
    if let Some(from) = sp.short_challenge_window_from_daa()
        && from > first
    {
        earliest = earliest.min(from + sp.window_challenge_at(from));
    }
    let mut duty_alone_late = 0;
    for class in palw_t12_genesis_held_class_ids_v1() {
        let d = sp.claim_verify_daa_v1(&s, &class, PalwClaimVerifyShapeV1::Attempt, bound);
        let w_r = sp.receipt_window_for_claim_v1(&s, &class, PalwClaimVerifyShapeV1::Attempt, bound);
        let receipt_deadline = bound + w_r;
        // DL-1's licensed floor, `max(L + wc(L), H)` with `H = bound + D + 1` past the fence, at every
        // licence the claim can take while its receipts count.
        let floor = |licensed: u64| (licensed + sp.window_challenge_at(licensed)).max(bound + d + 1);
        let min_final = (first..=receipt_deadline).map(floor).min().expect("a licence window");
        assert!(earliest <= min_final, "class {class}: the node's earliest Final {earliest} is never after DL-1's {min_final}");
        let due = receipt_deadline.min(earliest).saturating_sub(MARGIN).max(bound);
        assert!(due + MARGIN <= min_final, "class {class}: due {due} lands a margin before every Final ({min_final})");
        println!(
            "class {class}: D(c) {d} W_r(c) {w_r} | receipt deadline {receipt_deadline}, earliest Final {min_final} → due {due} \
             (the duty's deadline alone: {})",
            receipt_deadline - MARGIN
        );
        if receipt_deadline - MARGIN > min_final {
            duty_alone_late += 1;
        }
    }
    assert!(duty_alone_late > 0, "the duty's deadline alone dates some genesis held class's filing after its Final");
}
