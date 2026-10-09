//! **Lane PL part C × G14: a lane-A claim's uncharged expiry waits for a pending accusation** (agent SHARD; the Lead's ruling of
//! 2026-10-09 under PRINCIPLES §3/§6.7 — nothing escapes to an uncharged void while a valid accusation is open).
//!
//! Past `palw_panel_unavailable_expiry` a lane-A claim whose SECOND Panel says nothing expires `PanelUnavailable`, uncharged —
//! and an uncharged void closes every session on the claim neutrally. A non-seat DA session pauses nothing in V2 (V3S-08), so before
//! this rule a producer whose second Panel stayed silent walked away from an open accusation for free. Now the expiry waits on
//! `PalwChainStateV2::palw_accusation_pending_v1`, exactly as the permissionless Panel's ends do:
//!
//! * **the twin with nothing pending** expires `PanelUnavailable` at the second receipt timeout, uncharged (part C, unchanged);
//! * **a non-seat DA session** opened on the second Panel holds the expiry (no deadline while it is open) and the default wins
//!   (`ProducerWithholding`);
//! * **a court** opened on the second Panel holds it too, and once it is cleared — nothing pending — the expiry already due lands
//!   at the next sweep.
//!
//! Testnet-12's own fold with the part-C mirror armed on a copy of the params (the fence is armed on no shipped preset, so this
//! bypasses `validate_palw_v2`, and says so). Every block is delta-checked and carriage-reloaded (`Chain::step_at`).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_panel_part_c_g14_hold`

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2;

const MSK: u64 = 100_000_000;
/// A bystander that accuses (never a seat of the floor's panels), and a second one that opens a court.
const ACCUSER: u64 = 1;
const CHALLENGER: u64 = 2;

/// Testnet-12 with lane PL part C in force from genesis, two funded bystanders, and a floor claim on its SECOND Panel: bound,
/// silent through the first receipt window (the first timeout redraws), bound again. Returns the claim and its second receipt
/// deadline.
fn on_its_second_panel(seed: u64) -> (Chain, Hash64, u64) {
    let mut c = Chain::new(t12());
    c.sp = c.sp.clone().with_panel_unavailable_expiry_mirror(Some(0));
    c.step(&[bond_obj(ACCUSER, 20_000 * MSK), bond_obj(CHALLENGER, 20_000 * MSK)]);
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    c.bind(id, &seats);
    let first = c.s.deadline_of(&id).expect("the first receipt deadline");
    c.step_at(first + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let claim = c.claim(&id);
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional) && claim.rebound_daa.is_some(), "the first timeout redraws");
    c.bind(id, &seats);
    let second = c.s.deadline_of(&id).expect("the second receipt deadline, nothing pending");
    (c, id, second)
}

#[test]
fn with_nothing_pending_the_second_panels_silence_expires_panel_unavailable_uncharged() {
    let (mut c, id, second) = on_its_second_panel(0xC0);
    let (producer, _, _) = floor_producer(&c.p);
    let slashed = c.s.bond(&producer).unwrap().slashed;
    c.step_at(second + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PanelUnavailable, .. }),
        "{:?}",
        c.claim(&id).phase
    );
    assert_eq!(c.s.bond(&producer).unwrap().slashed, slashed, "part C: uncharged");
}

#[test]
fn a_non_seat_da_session_holds_the_second_panels_expiry_and_the_default_wins() {
    let (mut c, id, second) = on_its_second_panel(0xC1);
    let accuser = bond_key(ACCUSER);
    assert!(!c.s.panel(&id).unwrap().seats.iter().any(|seat| seat.bond == accuser), "a non-seat accuser");
    let (producer, _, _) = floor_producer(&c.p);
    let collateral = c.s.bond(&producer).unwrap().collateral;
    c.step_at(second, &[da_accuse(id, accuser, 3)], PalwBlockWorkV3::None, Hash64::default(), 0);
    let session = c.s.da_session(&id, &accuser).expect("the session is open").clone();
    assert!(!session.accuser_is_seat, "V3S-08: a non-seat session pauses nothing in V2 — the hold is what keeps it");
    assert!(c.s.palw_v2_expiry_held_v1(&c.sp, &id, &c.claim(&id)), "the hold reaches the claim");
    assert_eq!(c.s.deadline_of(&id), None, "no deadline while the accusation is pending");
    c.step_at(second + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }),
        "past the second timeout, still bound: {:?}",
        c.claim(&id).phase
    );
    c.step_at(session.deadline_daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }));
    c.step_at(session.deadline_daa + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "the default wins, never a neutral close: {:?}",
        c.claim(&id).phase
    );
    assert!(c.s.bond(&producer).unwrap().collateral < collateral, "the producer is charged");
}

#[test]
fn a_court_holds_the_second_panels_expiry_and_the_void_lands_once_it_is_cleared() {
    let (mut c, id, second) = on_its_second_panel(0xC2);
    assert!(c.sp.turn_deadline_daa() >= 2, "the premise: the court's first rung outlives the next two blocks");
    let challenger = bond_key(CHALLENGER);
    let session = court_session_of(&c.s, id, challenger);
    c.step_at(second, &[court_opened(&c.s, id, challenger)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.palw_accusation_pending_v1(&id));
    assert_eq!(c.s.deadline_of(&id), None, "the court holds the expiry");
    c.step_at(second + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }), "held: {:?}", c.claim(&id).phase);
    assert!(c.s.court_session(&session).is_some());
    // The court clears (the challenger defeated): nothing is pending; the deadline DL-1 derives is already past, so the expiry
    // lands at the next block's sweep.
    c.step_at(second + 2, &[court_cleared(session)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(!c.s.palw_accusation_pending_v1(&id));
    assert_eq!(c.s.deadline_of(&id), Some(second), "re-derived once nothing is pending");
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }));
    c.step_at(second + 3, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PanelUnavailable, .. }),
        "the deferred expiry, once nothing is pending: {:?}",
        c.claim(&id).phase
    );
}

#[test]
fn below_the_fence_nothing_holds() {
    // The same chain without part C: the hold reaches no claim (the second timeout is the charged `ReceiptTimeout` path, unchanged).
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(ACCUSER, 20_000 * MSK)]);
    let id = c.floor_claim(0xC3);
    let seats = c.floor_seats();
    c.bind(id, &seats);
    let first = c.s.deadline_of(&id).unwrap();
    c.step_at(first + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    c.bind(id, &seats);
    let second = c.s.deadline_of(&id).unwrap();
    c.step_at(second, &[da_accuse(id, bond_key(ACCUSER), 3)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(!c.s.palw_v2_expiry_hold_applies_v1(&c.sp, &id, &c.claim(&id)));
    assert_eq!(c.s.deadline_of(&id), Some(second), "V3S-08 unchanged below the fence");
}
