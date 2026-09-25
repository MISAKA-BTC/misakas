//! **ADR-0152 v3.1 T28 (§8.1, S): a conviction at F+3,001…F+9,000 under a held second clock, after
//! retirement, burns the row; `basis_k` is read from the row.** A child of T46's suite, so the claim,
//! its licence, its `Final` and the conviction are the real ones, through the gate, the acceptance
//! walk and the fold.
//!
//! The window: a claim retires at `F + claim_retirement_daa` (3,000 on testnet-12) and its vesting
//! row's DAA clock is met at `F + window_court` (3,000), but past `palw_rcore_plus` the row matures on
//! the SECOND clock too — `palw_settled_anchor_depth` anchors settled since `F`, or the per-obligation
//! bound `F + 3 · window_court` (9,000) — so on a chain where nothing else licenses after `F` the row
//! (and the locks that follow it, L-1) outlives the claim record by up to 6,000 DAA. A conviction in
//! that window must still find the row and burn it (S3 through the funnel's burn hook), with the
//! claim record gone: every term the funnel needs — `G` and `basis_k` — comes from the rows the claim
//! left (`palw_claim_g_v1` reads the liability record; the vesting row carries its copies, N8).
//!
//! The edges are elsewhere: a row maturing at the bound under a trickle is T43 (`vesting_fold_v1`'s
//! `t43_the_trickle_regime_matures_at_the_bound`), a halt that holds rows past it is T37, and a
//! conviction after retirement with no row left (the claim's row already moved) is T46f.

use super::*;

/// **T28.** A floor claim with a real step fault, licensed through Verification V2 (a coverage
/// licence, `basis_k` 2) and swept to `Final`; the claim retires at `F + 3,001`, its vesting row held
/// by the second clock; at `F + 4,000` — past the row's DAA clock, before the per-obligation bound,
/// with the last anchor under `2 · window_court` old (no halt) — the full seat is convicted by kind 3:
///
/// * the row is burned (deleted, counted burned by its whole amount) and the producer pays S3,
///   `min(25% · C₀, 3 G)`, `G` the liability record's `g_res + escrowed_reward`;
/// * the seat pays S4 on the lock that followed the row;
/// * the kind-3 record carries both legs, and the reporter reward is opened on `collected − X` with
///   `X = min(lock, g_res / basis_k)` at the ROW's `basis_k` (the claim record being gone);
/// * the block reverts to its parent exactly, and the state reloads.
#[tokio::test]
async fn t28_a_conviction_after_retirement_under_a_held_second_clock_burns_the_row() {
    use kaspa_consensus_core::palw_state_v2::{
        palw_claim_g_v1, palw_rcore_s3s4_action_v1, palw_reporter_reward_amount_v1, palw_reporter_reward_extracted_v1,
    };
    let h = harness(true);
    assert!(kaspa_consensus_core::palw_state_v2::PALW_RCORE_VESTING_ROWS_LANDED_V1, "the rows landed (IA-7)");
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    h.sweep_to_final(&mut walk, id);
    let PalwClaimPhaseV2::Final { final_daa } = walk.state.claim(&id).unwrap().phase else { unreachable!("Final") };
    let wc = h.sp().window_court();
    let retirement = h.sp().claim_retirement_daa();
    let vesting = walk.state.vesting_row(&id).expect("the Final wrote its vesting row").clone();
    assert_eq!(vesting.expiry_daa, final_daa + wc, "the row's DAA clock is F + window_court");
    assert!(
        retirement > 0 && final_daa + retirement < final_daa + 3 * wc,
        "the claim retires inside the second clock's reach (retirement {retirement}, window_court {wc})"
    );

    // The claim retires; the row does not.
    let point = walk.at(final_daa + retirement + 1);
    let retired = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
    walk.advance(&point, retired);
    assert!(walk.state.claim(&id).is_none(), "the claim record retired at F + {}", retirement + 1);
    let row = walk.state.vesting_row(&id).expect("the second clock holds the row past its DAA clock").clone();
    assert_eq!(row.matured_at, None, "not latched: no anchor has settled since F");

    // Up to the block before the conviction, still held — past the DAA clock, before the bound, no halt.
    let conviction_daa = final_daa + 4_000;
    assert!(conviction_daa > vesting.expiry_daa && conviction_daa < final_daa + 3 * wc, "inside F+3,001…F+9,000");
    let point = walk.at(conviction_daa - 1);
    let held = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
    walk.advance(&point, held);
    assert!(
        walk.state.vesting_row(&id).is_some_and(|r| r.matured_at.is_none()),
        "the row is still held at F + 3,999 (the second clock, not the DAA clock)"
    );
    assert!(walk.state.claim(&id).is_none());
    let liability = walk.state.panel_liability(&id).expect("the liability record outlives the claim").clone();
    assert_eq!(liability.basis_k, row.basis_k, "N8: the vesting row copies the licence's basis_k");
    assert!(liability.basis_k >= 2, "a replay-backed licence: basis_k {}", liability.basis_k);
    let gains = palw_claim_g_v1(&walk.state, &id).expect("the gain terms outlive the claim");
    assert_eq!(
        (gains.g_res, gains.escrowed_reward, gains.basis_k),
        (liability.g_res_sompi, liability.escrowed_reward, liability.basis_k),
        "with the claim record gone, G and basis_k are the liability record's"
    );
    let g = gains.g();

    // The conviction.
    let full = licence.full_card();
    let seat = h.cards[full];
    let executor = h.cards[EXECUTOR];
    let lock = *walk.state.slashable_lock(seat, id).expect("the full seat's lock follows the row");
    let before = walk.state.clone();
    let (seat_nominal, seat_debit) = s4_charge(&h, &before, seat, lock.amount, g, conviction_daa);
    let c0 = before.bond(&executor).unwrap().collateral;
    let s3 = palw_rcore_s3s4_action_v1(c0, g);
    assert!(s3 > 0, "S3 has something to charge");
    let (parent, delta) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
    assert_eq!(walk.daa, conviction_daa);
    let s = &walk.state;
    assert!(s.vesting_row(&id).is_none(), "the conviction burned the row");
    assert_eq!(
        s.vesting_counters().burned - before.vesting_counters().burned,
        row.total_sompi_u128(),
        "burned by its whole amount, once"
    );
    assert_eq!(u128::from(c0 - s.bond(&executor).unwrap().collateral), s3, "S3: the producer pays min(25% · C₀, 3 G), once");
    assert_eq!(
        u128::from(before.bond(&seat).unwrap().collateral - s.bond(&seat).unwrap().collateral),
        seat_debit,
        "S4 on the seat's live lock"
    );
    assert!(s.slashable_lock(seat, id).is_none(), "the lock is taken");
    let key = palw_false_valid_offence_id_v2(&seat.0, &id);
    let record = s.consumed_offence(&key).expect("one kind-3 record");
    assert_eq!(
        (record.kind, u128::from(record.amount), u128::from(record.collected), record.claim_id),
        (PalwOffenceKindV1::PanelFalseValidV2, seat_nominal + s3, seat_debit + s3, id),
        "the record carries the seat's S4 and the producer's S3"
    );
    // basis_k from the row: R-1's X is `min(lock, g_res / basis_k)`.
    let extracted = palw_reporter_reward_extracted_v1(lock.amount, liability.g_res_sompi, liability.basis_k);
    let at_k1 = palw_reporter_reward_extracted_v1(lock.amount, liability.g_res_sompi, 1);
    let reward = s.reward_pending(&key).expect("a proven conviction opens a reward");
    assert_eq!(reward.amount, palw_reporter_reward_amount_v1(record.collected, extracted), "X at the row's basis_k");
    println!(
        "[t28] F {final_daa}; retired {}; convicted {conviction_daa}; row {} sompi (basis_k {}); S3 {s3}; S4 {seat_debit}; \
         X {extracted} (at basis_k 1: {at_k1}); reward {}",
        final_daa + retirement + 1,
        row.total_sompi_u128(),
        row.basis_k,
        reward.amount
    );
    assert!(s.palw_execution_root_is_forfeited_v1(&claim.envelope.attempt.execution_root), "the proven-false root is forfeit");
    h.reloads(s);
    assert_eq!(revert_delta_v2(s, &delta, h.sp()).expect("reverts").state_root(), parent.state_root(), "the block reverts exactly");
}
