//! **Regression (feat/t12-aheld-node fourth review, HIGH: a chain halt): a non-seat's held-forfeit
//! pause never counts on a seat's DA-8 budget.**
//!
//! Both pause paths (`held_forfeit_pauses_v1` when a session opens, c290ce8a, and
//! `pause_open_demand_of_held_forfeit_v1` when the record is written, f156e283) did
//! `opened_by_seat[challenger] += 1` for a challenger that is no seat of the claim's panel. DA-8's
//! per-seat cap (`PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1 = 4`) is checked at admission only for a real
//! seat, but the loader (`assert_da_consistency_v1`) refuses any `opened_by_seat` count above 4. So a
//! non-seat could run this loop five times on one claim, for about five forfeits:
//!
//! 1. open a held dissection;
//! 2. lose it by its own `ChallengerDefeated` close, which writes a record;
//! 3. file the demand, which pauses the claim;
//! 4. let the honest producer answer.
//!
//! The fold took the fifth demand, and then every node's tip loader (`load_tip_cached` →
//! `materialize_tip` → `into_state_v3`) refused the fold's own output. The chain halted.
//!
//! Now a record-holder's pause is counted only where DA-5 and DL-1 read it: `open_seat_sessions` and
//! `paused_since`, with the session flagged `accuser_is_seat`. The loader splits the open counts by that
//! flag, so they still agree. The session was admitted on the non-seat budget, so it is counted there
//! (`opened_non_seat_total`, sixteen over the claim's life). `opened_by_seat` stays a seat's alone. No
//! layout changes.
//!
//! The floor fixture carries no binding a held close could name. So the records are written through the
//! carriage, the way the fold leaves them. The fifth demand is folded through `Chain::step`, which checks
//! each block: the delta re-applies and reverts, and the child's carriage reloads under its root, as the
//! node's tip store does. kaspad's `held_court_e2e` plays both pause paths through the node and the
//! fold, reloading after every block.

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaClaimV1;
use kaspa_consensus_core::palw_state_v2::{PalwHeldForfeitV1, palw_da_event_index_v1};

const MSK: u64 = 100_000_000;

fn accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: palw_da_event_index_v1(row, 0), accuser, signature: vec![] }
}

/// The licensed floor claim, a non-seat that lost five held dissections on it (five records, their
/// forfeits slashed), four of them spent on a demand the producer answered (their sessions closed),
/// and the claim's DA record as those four left it: `opened_non_seat_total` counted, and
/// `opened_by_seat` holding `by_seat` for the non-seat (`None` is the fixed fold's).
fn four_pauses_spent(by_seat: Option<u8>) -> (Chain, Hash64, PalwBondKeyV2) {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let id = c.floor_claim(0x74);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    let non_seat = bond_key(1);
    assert!(!seats.iter().any(|(k, _)| *k == non_seat), "the premise: no seat");
    let amount = 500 * MSK;
    let closed = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        let bond = carriage.bonds.get_mut(&non_seat).expect("the non-seat's bond");
        bond.collateral -= 5 * amount;
        bond.slashed += 5 * amount;
        for n in 0..5u64 {
            carriage.held_forfeits.insert(
                (id, Hash64::from_u64_word(0x5E50 + n)),
                PalwHeldForfeitV1 {
                    challenger: non_seat,
                    amount,
                    anchor_leaf_hash: Hash64::from_u64_word(0xA0 + n),
                    anchor_checkpoint: 9,
                    narrowed_leaf: 4_750 + n,
                    chunks: vec![3],
                    closed_daa: closed,
                    paused: n < 4,
                },
            );
        }
        carriage.da_claims.insert(
            id,
            PalwDaClaimV1 {
                opened_non_seat_total: 4,
                opened_by_seat: by_seat.map(|n| (non_seat, n)).into_iter().collect(),
                last_closed_daa: Some(closed),
                ..Default::default()
            },
        );
    });
    (c, id, non_seat)
}

/// **The fifth pause leaves a tip the loader takes.** The fifth demand is folded through the checked
/// step, and the claim pauses from it: the session is counted with the seats' OPEN sessions, the
/// non-seat's lifetime count rises to five, and `opened_by_seat` is untouched. The probe's pre-state
/// (`opened_by_seat[non_seat] = 4`, the count the pre-fix fold wrote for four pauses) passes too: the
/// fifth pause no longer pushes it to five.
#[test]
fn review4_a_non_seats_fifth_record_pause_leaves_a_tip_the_loader_takes() {
    for (label, by_seat) in [("the fixed fold's four pauses", None), ("the probe's pre-state", Some(4u8))] {
        let (mut c, id, non_seat) = four_pauses_spent(by_seat);
        // `step` folds, re-applies and reverts the delta, and reloads the child's carriage under its
        // root; the pre-fix fold panicked there ("exceeds DA-8's caps").
        c.step(&[accuse(id, non_seat, 0)]);
        let record = c.s.da_claim(&id).expect("the DA record").clone();
        assert_eq!(record.opened_by_seat.get(&non_seat).copied(), by_seat, "{label}: never counted on a seat's budget");
        assert_eq!(record.opened_non_seat_total, 5, "{label}: counted on the non-seat budget it was admitted on");
        assert_eq!((record.open_seat_sessions, record.open_other_sessions), (1, 0), "{label}: with the seats' open sessions");
        assert_eq!(record.paused_since, Some(c.daa), "{label}: the claim paused from the demand");
        assert!(c.s.da_session(&id, &non_seat).expect("the demand").accuser_is_seat, "{label}: a pausing session");
        assert_eq!(c.s.deadline_of(&id), None, "{label}: no Final while the demand is open");
        assert!(c.s.held_forfeits_of_claim(&id).all(|(_, r)| r.paused), "{label}: the fifth record's pause spent");
    }
}

/// **The loader still refuses a count past four**: DA-8's per-seat cap is a load rule. The fold no
/// longer writes such a count, and the loader keeps refusing it.
#[test]
fn review4_the_loader_still_refuses_a_seat_count_past_four() {
    let (c, id, non_seat) = four_pauses_spent(None);
    let mut carriage = PalwStateCarriageV2::from_state(&c.s);
    carriage.da_claims.get_mut(&id).expect("the DA record").opened_by_seat.insert(non_seat, 5);
    let refused = carriage.into_state(&c.sp, None);
    assert!(
        matches!(&refused, Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::CarriageInconsistent(why)) if why.contains("DA-8")),
        "{:?}",
        refused.map(|_| ())
    );
}
