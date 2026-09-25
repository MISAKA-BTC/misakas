//! **Regression (feat/t12-aheld-node second review, c68479db): the two ways a forger kept an honest
//! challenger's held forfeit from ever being refunded — both closed, on testnet-12's own fold.**
//!
//! 1. **Withhold.** The only refund door was `CheckpointAccused → ExecutorGuilty`. A forger that did
//!    not answer the step-6 `StateChunk` demand let DA-7 void the claim `ProducerWithholding` (S1, a
//!    first strike carries no action tier, cheaper than S2 at 8k), `write_claim` dropped the record,
//!    and nothing restored it — so a rational forger always withheld. Now DA-7 restores it too, when
//!    the defaulted session is the record's challenger's and its named unit is the step-6 demand of
//!    the record's anchor (`PalwHeldForfeitV1::is_demanded_by_v1`). Any other default refunds nothing.
//! 2. **Disclose late, against a non-seat.** A non-seat's DA session did not pause the claim (DA-5),
//!    so the claim reached `Final` at `L + W` while the session opened at `L + 1` ran to `L + 1 + W`:
//!    an answer in that tail landed after `Final`, where `CheckpointAccused` is refused, and the
//!    record was gone. Now the first session a live record's challenger opens on the claim pauses it
//!    as a seat's does (once a record); a non-seat without a record still pauses nothing.
//!
//! A floor claim stands in for the held one: the record's rules (live claim, drop at terminal, the
//! refund doors) and DA-5/DA-7 are class-agnostic. The record and the step-6 session are written
//! through the carriage the way the fold leaves them (the floor fixture carries no binding a
//! `DefaultAccusedHeld` could name); kaspad's `held_court_e2e` plays both escapes through the node.

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_da_rcore_v1::{
    PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1, PalwDaClaimV1, PalwDaSessionV1, PalwDaStageV1, PalwDaUnitV1,
};
use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwHeldForfeitV1, PalwVoidReasonV2, palw_da_disclose_window_daa_v1, palw_da_event_index_v1,
};

const MSK: u64 = 100_000_000;
const ANCHOR_CHECKPOINT: u32 = 9;
const CHUNK: u32 = 3;

fn accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: palw_da_event_index_v1(row, 0), accuser, signature: vec![] }
}

fn covered_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    (id, seats)
}

/// testnet-12's chain with `palw_offence_attribution` armed as the processor resolves it (the fence
/// every held-forfeit rule sits behind).
fn t12_chain() -> Chain {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    c
}

/// The forfeit a proven acquittal kept for `challenger` on `claim` — written through the carriage the
/// way `record_held_forfeit_v1` leaves the state: the debit out of `collateral` into `slashed`, and the
/// record beside it.
fn with_forfeit(c: &mut Chain, claim: Hash64, challenger: PalwBondKeyV2, amount: u64) {
    c.s = edited(&c.sp, &c.s, |carriage| {
        let bond = carriage.bonds.get_mut(&challenger).expect("the challenger's bond");
        bond.collateral -= amount;
        bond.slashed += amount;
        carriage.held_forfeits.insert(
            (claim, Hash64::from_u64_word(0x5E55)),
            PalwHeldForfeitV1 {
                challenger,
                amount,
                anchor_leaf_hash: Hash64::from_u64_word(0xA7),
                anchor_checkpoint: ANCHOR_CHECKPOINT,
                narrowed_leaf: 4_750,
                chunks: vec![CHUNK],
                closed_daa: c.daa,
                paused: false,
            },
        );
    });
    assert_eq!(c.s.held_forfeits_of_claim(&claim).count(), 1);
}

/// The seat's step-6 demand of `unit`, open from this block — the session `open_da_session_rcore_v1`
/// writes for a seat's `DefaultAccusedHeld` (the claim paused, DA-5), written through the carriage.
fn with_seat_demand(c: &mut Chain, claim: Hash64, seat: PalwBondKeyV2, unit: PalwDaUnitV1) -> u64 {
    let window = palw_da_disclose_window_daa_v1(&c.sp);
    let opened = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_sessions.insert(
            (claim, seat),
            PalwDaSessionV1 {
                opened_daa: opened,
                deadline_daa: opened + window,
                accuser_is_seat: true,
                exposure: 1_000,
                units: vec![unit],
                stage: PalwDaStageV1::Licensed,
            },
        );
        carriage.da_claims.insert(
            claim,
            PalwDaClaimV1 {
                open_seat_sessions: 1,
                opened_by_seat: [(seat, 1)].into_iter().collect(),
                paused_since: Some(opened),
                ..Default::default()
            },
        );
    });
    opened + window
}

/// The DA-7 sweep's block past `deadline`, folded with the processor's extras.
fn default_after(c: &Chain, deadline: u64) -> PalwChainStateV2 {
    let mut c2 = Chain { p: c.p.clone(), sp: c.sp.clone(), s: c.s.clone(), daa: c.daa, room: c.room, attribution: c.attribution };
    c2.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let daa = deadline + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let mut e = c2.extras_at(daa);
    e.seat_da_answer_landed = PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1;
    let (child, _, _) =
        fold_with(&c2.p, &c2.sp, &c2.s, &x, &[], PalwBlockWorkV3::None, Hash64::default(), &e).expect("the default folds");
    child
}

/// Escape 1, closed: the forger withholds the demanded chunk; DA-7 voids the claim
/// `ProducerWithholding` — and restores the seat's forfeit where it went (`slashed` down, collateral
/// up by the record's amount), the record gone with the void.
#[test]
fn review2_withholding_the_step6_chunk_refunds_the_forfeit() {
    let mut c = t12_chain();
    let (id, seats) = covered_floor_claim(&mut c, 0x6E);
    let challenger = seats[1].0;
    with_forfeit(&mut c, id, challenger, 3_744 * MSK);
    let deadline = with_seat_demand(
        &mut c,
        id,
        challenger,
        PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: ANCHOR_CHECKPOINT, chunk: CHUNK }),
    );
    let before = c.s.bond(&challenger).unwrap().clone();
    let child = default_after(&c, deadline);
    let phase = child.claim(&id).unwrap().phase.clone();
    assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }), "{phase:?}");
    assert_eq!(child.held_forfeits_of_claim(&id).count(), 0, "the record went with the void");
    let after = child.bond(&challenger).unwrap();
    assert_eq!(after.slashed, before.slashed - 3_744 * MSK, "the forfeit is no longer owed to the burn");
    assert_eq!(after.collateral, before.collateral + 3_744 * MSK, "and is the seat's collateral again");
}

/// …and only that default: a default of the same seat's demand of another unit (not the step-6
/// demand of the record's anchor) refunds nothing — the record goes with the void, the forfeit stays.
#[test]
fn review2_a_default_of_another_unit_refunds_nothing() {
    for unit in [
        PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: ANCHOR_CHECKPOINT + 1, chunk: CHUNK }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: ANCHOR_CHECKPOINT, chunk: CHUNK + 1 }),
        PalwDaUnitV1::Event { row: 0, tile: 0 },
    ] {
        let mut c = t12_chain();
        let (id, seats) = covered_floor_claim(&mut c, 0x6E);
        let challenger = seats[1].0;
        with_forfeit(&mut c, id, challenger, 3_744 * MSK);
        let deadline = with_seat_demand(&mut c, id, challenger, unit);
        let before = c.s.bond(&challenger).unwrap().clone();
        let child = default_after(&c, deadline);
        assert!(matches!(
            child.claim(&id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }
        ));
        assert_eq!(child.held_forfeits_of_claim(&id).count(), 0, "{unit:?}: the record went with the void");
        let after = child.bond(&challenger).unwrap();
        assert_eq!((after.slashed, after.collateral), (before.slashed, before.collateral), "{unit:?}: no refund");
    }
}

/// Escape 2, closed: a non-seat that holds a live forfeit on the claim opens its demand the block
/// after the acquittal, and the session pauses the claim as a seat's does — no deadline while it is
/// open, so `Final` cannot outrun the forger's answer. Once a record: the record is marked.
#[test]
fn review2_a_forfeit_holding_non_seats_demand_pauses_the_claim() {
    let mut c = t12_chain();
    let (id, _) = covered_floor_claim(&mut c, 0x6F);
    let final_at = c.s.deadline_of(&id).expect("a licensed claim's deadline");
    let non_seat = bond_key(1);
    with_forfeit(&mut c, id, non_seat, 1_000 * MSK);
    c.step(&[accuse(id, non_seat, 0)]);
    let session = c.s.da_session(&id, &non_seat).expect("the non-seat's session").clone();
    assert!(session.accuser_is_seat, "counted with the seats' sessions (DL-1)");
    assert_eq!(c.s.deadline_of(&id), None, "the claim is paused while the demand is open");
    assert!(c.s.held_forfeits_of_claim(&id).all(|(_, record)| record.paused), "the record's one pause is spent");
    c.step_at(final_at + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final while the demand is unanswered");
}

/// The control: a non-seat WITHOUT a forfeit on the claim still pauses nothing (DA-5, V3S-08) — the
/// claim finalizes at `L + W` with its session still open.
#[test]
fn review2_a_non_seat_without_a_forfeit_still_pauses_nothing() {
    let mut c = t12_chain();
    let (id, _) = covered_floor_claim(&mut c, 0x6F);
    let final_at = c.s.deadline_of(&id).expect("a licensed claim's deadline");
    let non_seat = bond_key(1);
    c.step(&[accuse(id, non_seat, 0)]);
    let da_deadline = c.s.da_session(&id, &non_seat).expect("the non-seat's session").deadline_daa;
    assert!(!c.s.da_session(&id, &non_seat).unwrap().accuser_is_seat);
    assert_eq!(c.s.deadline_of(&id), Some(final_at), "a non-seat session pauses nothing");
    assert!(da_deadline > final_at);
    c.step_at(final_at + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { .. }), "Final while the demand is unanswered");
}
