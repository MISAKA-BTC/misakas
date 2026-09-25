//! **Regression (feat/t12-aheld-node third review, c290ce8a): a record-holder's demand that PREDATES
//! its record pauses the claim too.**
//!
//! The record-holder's pause was granted only when the challenger's DA session OPENED after the
//! record existed (`held_forfeit_pauses_v1`, from `open_da_session_rcore_v1`). A non-seat challenger's
//! node files its step-6 demand while the held session is still open (at its `Terminal` duty); a forger
//! that filed its acquitting close AFTER that demand landed got a record whose challenger's demand was
//! already open and admitted non-pausing, and one session per accuser left no second demand to open.
//! On testnet-12 (`window_challenge_at` = 120, `W_disclose` = 1,200) `Final` = close + 120 came long
//! before the demand's deadline: the forger answered after `Final` (or never), `CheckpointAccused` was
//! refused, the record was dropped at `Final`, and the non-seat was never refunded.
//!
//! Now the record's write converts that open demand into the record's pause — counted with the seats'
//! sessions (`opened_by_seat` too), the claim paused from the close (`paused_since`, its deadline
//! disarmed), the record marked paused — and the state the probe built (an unspent record beside its
//! challenger's non-pausing demand) is one no fold leaves, which the loader refuses.
//!
//! The floor fixture carries no binding a held close could name, so the record is written through the
//! carriage, the way the fold leaves the state; kaspad's `held_court_e2e` plays the ordering through
//! the node and the fold (the close a block after the demand, and in the same block after it).

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_state_v2::{PalwHeldForfeitV1, PalwStateV2Error, palw_da_event_index_v1};

const MSK: u64 = 100_000_000;

fn accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: palw_da_event_index_v1(row, 0), accuser, signature: vec![] }
}

fn record(challenger: PalwBondKeyV2, amount: u64, closed_daa: u64, paused: bool) -> PalwHeldForfeitV1 {
    PalwHeldForfeitV1 {
        challenger,
        amount,
        anchor_leaf_hash: Hash64::from_u64_word(0xA7),
        anchor_checkpoint: 9,
        narrowed_leaf: 4_750,
        chunks: vec![3],
        closed_daa,
        paused,
    }
}

/// The licensed floor claim, and the non-seat's demand on it opened before any record exists.
fn demanded_before_the_acquittal() -> (Chain, Hash64, PalwBondKeyV2) {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let id = c.floor_claim(0x71);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    let non_seat = bond_key(1);
    c.step(&[accuse(id, non_seat, 0)]);
    assert!(!c.s.da_session(&id, &non_seat).expect("the demand's session").accuser_is_seat, "no record yet: a non-seat's");
    (c, id, non_seat)
}

/// The state the probe built — an unspent record beside its challenger's non-pausing open demand — is
/// one no fold leaves (the record's write converts the demand), and the loader refuses it.
#[test]
fn review3_an_unspent_record_beside_a_non_pausing_demand_is_refused_at_load() {
    let (c, id, non_seat) = demanded_before_the_acquittal();
    let amount = 1_000 * MSK;
    let mut carriage = PalwStateCarriageV2::from_state(&c.s);
    let bond = carriage.bonds.get_mut(&non_seat).expect("the non-seat's bond");
    bond.collateral -= amount;
    bond.slashed += amount;
    carriage.held_forfeits.insert((id, Hash64::from_u64_word(0x5E55)), record(non_seat, amount, c.daa, false));
    let refused = carriage.into_state(&c.sp, None);
    assert!(
        matches!(&refused, Err(PalwStateV2Error::CarriageInconsistent(why)) if why.contains("does not pause")),
        "{:?}",
        refused.map(|_| ())
    );
}

/// The state the record's write leaves when the demand predates it: the demand counted with the seats'
/// sessions, the claim paused from the close, the record's pause spent. `Final` no longer comes at
/// close + `window_challenge` while the demand is open.
#[test]
fn review3_a_demand_that_predates_its_record_pauses_the_claim() {
    let (mut c, id, non_seat) = demanded_before_the_acquittal();
    let final_at = c.s.deadline_of(&id).expect("the claim owes its Final before the close");
    let session = c.s.da_session(&id, &non_seat).expect("the demand").clone();
    assert!(session.deadline_daa > final_at, "the demand's deadline is past the unpaused Final");
    let amount = 1_000 * MSK;
    let close = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        let bond = carriage.bonds.get_mut(&non_seat).expect("the non-seat's bond");
        bond.collateral -= amount;
        bond.slashed += amount;
        carriage.held_forfeits.insert((id, Hash64::from_u64_word(0x5E55)), record(non_seat, amount, close, true));
        carriage.da_sessions.get_mut(&(id, non_seat)).expect("the demand").accuser_is_seat = true;
        let da = carriage.da_claims.get_mut(&id).expect("the claim's DA record");
        da.open_other_sessions -= 1;
        da.open_seat_sessions += 1;
        *da.opened_by_seat.entry(non_seat).or_insert(0) += 1;
        da.paused_since = Some(close);
    });
    assert_eq!(c.s.deadline_of(&id), None, "the claim is paused while the demand is open");
    c.step_at(final_at + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final while the demand is open");
    assert!(c.s.da_session(&id, &non_seat).is_some(), "the demand still open: a CheckpointAccused on its answer lands");
    assert_eq!(c.s.held_forfeits_of_claim(&id).count(), 1, "and the record, with its refund door, is still there");
}
