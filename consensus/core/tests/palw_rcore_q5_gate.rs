//! **ADR-0152 v3.1 M4 part 2 through the public fold: Q-5's gate, DL-1's `basis_k < 2` row, the
//! upgrade's re-arm, and the collector's selection** (F4 part 2; T72, T72b, T40, T74's V3 half).
//!
//! * **DL-1's Q-5 row** — an S2 licence (the full seat and one rider, `basis_k` 1) is due at
//!   `max(L + wc(L), bound + window_receipt + 1)`, never before both supplementary doors have shut,
//!   and the carriage rebuilds the same deadline mid-gate (T40).
//! * **The first panel redraws** (V3S-01): `Provisional`, `rebound_daa`, the `Default` record, every
//!   lock and the duty row with its credit gone, the producer's commitment unmoved.
//! * **The second panel voids `NotReplayBacked`**, charged exactly as the second `ReceiptTimeout`
//!   (S0′; the S-4 funnel's arm, not a restatement) — measured against that twin.
//! * **The upgrade re-arms** — through SR-10's V3 door and through the V2 door (any seat's full-replay
//!   V2 `Valid`, V3S-01) — to `max(L + wc(L), U)`, so the claim finalizes and is never redrawn; a set
//!   that does not reach 2 leaves the gate where it was.
//! * **Q-7's duties** follow an S2 licence while it awaits its upgrade, to the seats it did not count.
//! * **The collector offers exactly what the door credits** (`palw_select_supplementary_v3_v1` over
//!   the validator and the fold's own `palw_v2_supplementary_effect_v1`).
//! * Every one with its fence-off twin, and every write round-trips through its delta.

use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_full_seat_bond_v2;
use kaspa_consensus_core::palw_panel_v2::{
    PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3, palw_select_supplementary_v3_v1, validate_supplementary_receipts_v3,
};
use kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimRcoreV1, PalwConsensusObjectV2 as Obj,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_claim_receipt_deadline_v1,
    palw_operator_id_v2, palw_rcore_licence_awaits_replay_v1, palw_seat_uncounted_on_licence_v1, palw_v2_supplementary_effect_v1,
    revert_delta_v2,
};
use kaspa_consensus_core::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

/// The first panel binds at 102 and licenses at `L`; `window_bind` 10, `window_receipt` 600,
/// `window_challenge` 120 (so SR-1b's window closes at `L + 60`, the gate at `102 + 600 + 1`).
const BOUND: u64 = 102;
const L: u64 = 103;
const GATE: u64 = BOUND + 600 + 1;
const FIRST_ANCHOR: u64 = 77;
const SECOND_ANCHOR: u64 = 78;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

fn ctx(block_word: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Hash64::from_u64_word(block_word), daa_score: daa, blue_score: daa, subsidy: 0 }
}

fn params(rcore: bool) -> PalwStateParamsV2 {
    let p =
        PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap();
    if rcore { p.with_rcore_plus_mirrors(Some(0), 0, Vec::new()) } else { p }
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        objective_offence_daa: Some(0),
        audit_2026_09_23_active: true,
        verification_v2_active: true,
        verification_s2_active: true,
        panel_economy_active: true,
        ..Default::default()
    }
}

/// One block, every consistency check the load path runs — DL-1's index equality included.
fn apply(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let applied = apply_palw_transition_v2_with_extras(parent, p, c, objects, att, false, false, false, false, &extras())?;
    applied.0.assert_internal_consistency(p).expect("internal consistency");
    applied.0.assert_deadline_consistency(p).expect("deadline consistency");
    Ok(applied)
}

fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    let network_domain = h64(999);
    let bond = bond_key(1).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, nonce, h64(1), &bond),
            class_id: h64(1),
            executor_bond: bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&op_key(21)),
            artifact_root: h64(11),
            trace_root: h64(31),
            output_root: h64(32),
            pwu,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h64(41),
        },
        signature: vec![0; 8],
    }
}

fn seats() -> Vec<PalwPanelSeatV2> {
    (2..=6).map(|n| PalwPanelSeatV2 { bond: bond_key(n), operator_id: palw_operator_id_v2(&op_key(20 + n)) }).collect()
}

fn bonds() -> Vec<PalwBondKeyV2> {
    seats().iter().map(|seat| seat.bond).collect()
}

/// A 160-pwu floor claim bound at `BOUND` to five seats (the producer posts 1,000, each seat 10⁹).
fn bound(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64) {
    let mut objects = vec![
        Obj::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        Obj::BondRegistered {
            bond: bond_key(1),
            pubkey: vec![7; 4],
            operator_pubkey: op_key(21),
            collateral: 1_000,
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ];
    objects.extend((2..=6).map(|n| Obj::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![0x40 + n as u8; 4],
        operator_pubkey: op_key(20 + n),
        collateral: 1_000_000_000,
        payout_payload: h64(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }));
    let (s0, _) = apply(&PalwChainStateV2::genesis(), p, &ctx(1, 100), &objects, None).expect("registry");
    let env = attempt(160, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s1, _) = apply(&s0, p, &ctx(2, 101), &[], Some(&env)).expect("claim");
    let (s2, _) = apply(&s1, p, &ctx(3, BOUND), &[bind(claim_id, FIRST_ANCHOR)], None).expect("bind");
    (s2, claim_id)
}

fn bind(claim_id: Hash64, anchor: u64) -> Obj {
    Obj::PanelBound { claim: claim_id, anchor: h64(anchor), seats: seats() }
}

/// The full seat and the four partial seats of the panel drawn from `anchor`.
fn geometry(claim_id: Hash64, anchor: u64) -> (PalwBondKeyV2, Vec<PalwBondKeyV2>) {
    let assignment = palw_segment_assignment_v2(h64(anchor), claim_id, 5);
    let full = palw_optimistic_full_seat_bond_v2(&assignment, &bonds()).expect("a full seat");
    (full, bonds().into_iter().filter(|b| *b != full).collect())
}

fn mask_of(claim_id: Hash64, anchor: u64, bond: PalwBondKeyV2) -> PalwSegmentMaskV2 {
    let index = bonds().iter().position(|b| *b == bond).unwrap() as u16;
    palw_segment_assignment_v2(h64(anchor), claim_id, 5).mask_of(index)
}

fn v2(claim_id: Hash64, seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> PalwSeatReceiptV2 {
    PalwSeatReceiptV2 { claim: claim_id, verdict, seat_bond: seat, signed_daa, signature: Vec::new() }
}

fn v3(claim_id: Hash64, anchor: u64, seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> PalwSeatReceiptV3 {
    PalwSeatReceiptV3 { receipt: v2(claim_id, seat, verdict, signed_daa), segments: mask_of(claim_id, anchor, seat) }
}

/// An S2 licence of `anchor`'s full seat and its first partial, at `at`.
fn s2(claim_id: Hash64, anchor: u64, at: u64) -> Obj {
    let (full, partial) = geometry(claim_id, anchor);
    let receipts = [full, partial[0]].iter().map(|seat| v3(claim_id, anchor, *seat, PalwReceiptVerdictV2::Valid, at)).collect();
    Obj::OptimisticLicensed { claim: claim_id, receipts }
}

/// The first panel, S2-licensed at `L`: `(state, claim, full seat, partial seats)`.
fn s2_licensed(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64, PalwBondKeyV2, Vec<PalwBondKeyV2>) {
    let (s2_state, claim_id) = bound(p);
    let (full, partial) = geometry(claim_id, FIRST_ANCHOR);
    let (s3, _) = apply(&s2_state, p, &ctx(4, L), &[s2(claim_id, FIRST_ANCHOR, L)], None).expect("S2 licenses");
    assert!(matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: L }));
    (s3, claim_id, full, partial)
}

fn door(claim_id: Hash64, receipts: Vec<PalwSeatReceiptV3>) -> Obj {
    Obj::ReceiptLicensedV2 { claim: claim_id, receipts }
}

fn credited(state: &PalwChainStateV2, claim_id: Hash64, seat: PalwBondKeyV2) -> bool {
    state.panel_duties_of(&claim_id).and_then(|row| row.get(&seat)).is_some_and(|at| *at != 0)
}

fn round_trips(before: &PalwChainStateV2, after: &PalwChainStateV2, delta: &PalwStateDeltaV2, p: &PalwStateParamsV2) {
    assert_eq!(revert_delta_v2(after, delta, p).unwrap().state_root(), before.state_root());
    assert_eq!(apply_delta_v2(before, delta, p).unwrap().state_root(), after.state_root());
}

/// T40: the carriage a pruned node imports rebuilds the index with DL-1 and asserts it — the same
/// deadline for `claim_id`, and no `CarriageInconsistent`.
fn carriage_rebuilds(state: &PalwChainStateV2, p: &PalwStateParamsV2, claim_id: Hash64) {
    let rebuilt = PalwStateCarriageV2::from_state(state).into_state(p, Some(state.state_root())).expect("the carriage rebuilds");
    assert_eq!(rebuilt.deadline_of(&claim_id), state.deadline_of(&claim_id), "DL-1 rebuilds the stored deadline");
}

/// **DL-1's Q-5 row: an S2 licence waits out both supplementary doors.** Its recount is 1, so it
/// awaits its replay, and it is due at `max(L + 120, bound + 600 + 1)` = the gate — not at `L + 120`,
/// where a replay-backed licence would finalize. The carriage rebuilds that deadline (T40), a block
/// at the gate itself leaves the claim licensed (the sweep takes deadlines strictly below the block),
/// and the doors' own receipt deadline is one DAA before it. On the fence-off twin nothing is gated:
/// the S2 licence records no door, and finalizes at `L + 120` as before.
#[test]
fn dl1_gates_an_s2_licence_past_both_doors_and_the_twin_does_not() {
    let p = params(true);
    let (s3, claim_id, _, _) = s2_licensed(&p);
    let claim = s3.claim(&claim_id).unwrap().clone();
    assert_eq!(claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::Optimistic));
    assert_eq!(claim.rcore.basis_k, 1, "the full seat and one rider");
    assert!(palw_rcore_licence_awaits_replay_v1(&claim));
    assert_eq!(palw_claim_receipt_deadline_v1(&s3, &p, &claim_id, &claim), Ok(Some(GATE - 1)), "the doors shut at bound + 600");
    assert_eq!(s3.deadline_of(&claim_id), Some(GATE), "gated past both doors, not L + 120");
    carriage_rebuilds(&s3, &p, claim_id);
    let (at_gate, _) = apply(&s3, &p, &ctx(5, GATE), &[], None).expect("a block at the gate");
    assert!(matches!(at_gate.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not yet swept");
    carriage_rebuilds(&at_gate, &p, claim_id);

    let off = params(false);
    let (t3, twin_id, _, _) = s2_licensed(&off);
    let twin = t3.claim(&twin_id).unwrap();
    assert_eq!(twin.rcore, PalwClaimRcoreV1::default(), "below the fence no record is staged");
    assert!(!palw_rcore_licence_awaits_replay_v1(twin));
    assert_eq!(t3.deadline_of(&twin_id), Some(L + 120), "the plain licence row");
    let (t4, _) = apply(&t3, &off, &ctx(5, L + 121), &[], None).expect("the sweep");
    assert!(matches!(t4.claim(&twin_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "finalized unbacked, as before M4");
}

/// **The first panel redraws (V3S-01)** — at the first block past the gate, exactly as a first
/// `ReceiptTimeout` would: `Provisional` with `rebound_daa` the sweep's DAA and the `Default` record;
/// the S2 signers' locks gone and the duty row with its credit gone, every seat's exposure back; the
/// producer's commitment unmoved; no anchor tick; the bind deadline from the rebound base, which the
/// carriage rebuilds. The block round-trips through its delta.
#[test]
fn the_first_panel_redraws_once() {
    let p = params(true);
    let (s3, claim_id, full, partial) = s2_licensed(&p);
    assert!(s3.slashable_lock(full, claim_id).is_some() && s3.slashable_lock(partial[0], claim_id).is_some());
    assert!(credited(&s3, claim_id, full) && credited(&s3, claim_id, partial[0]));
    let producer_before = s3.reserved_exposure(&bond_key(1));

    let (s4, d4) = apply(&s3, &p, &ctx(5, GATE + 1), &[], None).expect("the gate");
    let claim = s4.claim(&claim_id).unwrap().clone();
    assert_eq!(claim.phase, PalwClaimPhaseV2::Provisional, "redrawn, not voided");
    assert_eq!(claim.rebound_daa, Some(GATE + 1), "the one redraw is spent, anchored on the sweep");
    assert_eq!(claim.rcore, PalwClaimRcoreV1::default(), "a Provisional claim carries the Default record");
    for seat in [full, partial[0]] {
        assert!(s4.slashable_lock(seat, claim_id).is_none(), "the S2 signer's lock is released");
    }
    assert!(s4.panel_duties_of(&claim_id).is_none(), "the duty row and its credit are gone");
    for seat in bonds() {
        // Each seat of the fixture sits on this one claim, so its duty was its whole exposure.
        assert_eq!(s4.reserved_exposure(&seat), 0, "every seat's duty exposure is back");
    }
    assert_eq!(s4.reserved_exposure(&bond_key(1)), producer_before, "w + esc + rr throughout: SR-4 is not engaged");
    assert_eq!(s4.bond(&bond_key(1)).unwrap().slashed, s3.bond(&bond_key(1)).unwrap().slashed, "T72b: the producer forfeits nothing");
    assert_eq!(s4.settled_attempt_finals(), s3.settled_attempt_finals(), "a redraw ticks nothing");
    assert_eq!(s4.deadline_of(&claim_id), Some(GATE + 1 + 10), "DL-1: the bind window from the rebound base");
    carriage_rebuilds(&s4, &p, claim_id);
    round_trips(&s3, &s4, &d4, &p);
    assert!(palw_seat_duties_v2(&s4, &p, &bonds()).is_empty(), "nobody owes a redrawn claim a receipt");
}

/// The second panel: the redrawn claim re-bound from `SECOND_ANCHOR` at `GATE + 2` and S2-licensed at
/// `GATE + 3`. Returns the state and the second panel's receipt deadline.
fn second_s2(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64, u64) {
    let (s3, claim_id, _, _) = s2_licensed(p);
    let (s4, _) = apply(&s3, p, &ctx(5, GATE + 1), &[], None).expect("the redraw");
    let (s5, _) = apply(&s4, p, &ctx(6, GATE + 2), &[bind(claim_id, SECOND_ANCHOR)], None).expect("the second panel");
    let (s6, _) = apply(&s5, p, &ctx(7, GATE + 3), &[s2(claim_id, SECOND_ANCHOR, GATE + 3)], None).expect("the second S2");
    let claim = s6.claim(&claim_id).unwrap().clone();
    assert!(palw_rcore_licence_awaits_replay_v1(&claim) && claim.rebound_daa.is_some());
    let receipt_deadline = palw_claim_receipt_deadline_v1(&s6, p, &claim_id, &claim).unwrap().unwrap();
    assert_eq!(receipt_deadline, GATE + 2 + 600);
    assert_eq!(s6.deadline_of(&claim_id), Some(receipt_deadline + 1), "gated again");
    (s6, claim_id, receipt_deadline)
}

/// **The second panel voids `NotReplayBacked`, charged as the second `ReceiptTimeout`** (SR-5: S0′ at
/// launch). The twin is the same claim whose second panel never licenses at all: the same producer
/// debit, both past the audit fence's escrow-inclusive forfeit — so the gate reaches S-4's S0′ arm and
/// restates nothing. No strike (S1's alone), no anchor tick. The void round-trips.
#[test]
fn the_second_panel_voids_not_replay_backed_as_rt2_charges() {
    let p = params(true);
    let (s6, claim_id, receipt_deadline) = second_s2(&p);
    let slashed = |state: &PalwChainStateV2| state.bond(&bond_key(1)).unwrap().slashed;
    let (s7, d7) = apply(&s6, &p, &ctx(8, receipt_deadline + 2), &[], None).expect("the second gate");
    let voided = s7.claim(&claim_id).unwrap().clone();
    assert!(
        matches!(voided.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::NotReplayBacked, voided_daa } if voided_daa == receipt_deadline + 2),
        "{:?}",
        voided.phase
    );
    assert!(s7.withholding_strikes(&bond_key(1)).is_none(), "S0′ writes no strike");
    assert_eq!(s7.settled_attempt_finals(), s6.settled_attempt_finals());
    round_trips(&s6, &s7, &d7, &p);

    // The twin: the second panel bound and never licensed — the second `ReceiptTimeout`.
    let (s3, twin_id, _, _) = s2_licensed(&p);
    let (t4, _) = apply(&s3, &p, &ctx(5, GATE + 1), &[], None).expect("the redraw");
    let (t5, _) = apply(&t4, &p, &ctx(6, GATE + 2), &[bind(twin_id, SECOND_ANCHOR)], None).expect("the second panel");
    let (t6, _) = apply(&t5, &p, &ctx(8, receipt_deadline + 2), &[], None).expect("RT#2");
    assert!(matches!(t6.claim(&twin_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }));
    assert!(slashed(&s7) > slashed(&s6), "S0′ forfeits");
    assert_eq!(slashed(&s7) - slashed(&s6), slashed(&t6) - slashed(&t5), "NotReplayBacked is charged as RT#2");
}

/// **The upgrade lifts the gate and re-arms the deadline** (Q-5, T72): through SR-10's V3 door
/// (the three other partial seats' `Valid`s) and through the V2 door (one partial seat's full-replay
/// V2 `Valid`, V3S-01: any seat's covers every segment), the recount reaches 2 in block U, and the
/// deadline is re-derived as `max(L + 120, U)` — the carriage rebuilds it — so the claim finalizes at
/// `L + 121` and is never redrawn. A V3 set that stops at 1 leaves the gate where it was.
#[test]
fn an_upgrade_lifts_the_gate_and_rearms_the_deadline() {
    let p = params(true);
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let upgrades = [
        (
            "V3 door",
            door(
                claim_id,
                partial[1..].iter().map(|seat| v3(claim_id, FIRST_ANCHOR, *seat, PalwReceiptVerdictV2::Valid, L + 1)).collect(),
            ),
        ),
        (
            "V2 door",
            Obj::ReceiptLicensed { claim: claim_id, receipts: vec![v2(claim_id, partial[1], PalwReceiptVerdictV2::Valid, L + 1)] },
        ),
    ];
    for (route, set) in upgrades {
        let (s4, d4) = apply(&s3, &p, &ctx(5, L + 1), &[set], None).unwrap_or_else(|e| panic!("{route}: {e}"));
        let claim = s4.claim(&claim_id).unwrap().clone();
        assert_eq!(claim.rcore.basis_k, 2, "{route}: two replays of every segment");
        assert_eq!(claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::Coverage), "{route}: a counted mask is partial");
        assert!(!palw_rcore_licence_awaits_replay_v1(&claim), "{route}");
        assert_eq!(s4.deadline_of(&claim_id), Some(L + 120), "{route}: max(L + wc(L), U)");
        carriage_rebuilds(&s4, &p, claim_id);
        round_trips(&s3, &s4, &d4, &p);
        let (s5, _) = apply(&s4, &p, &ctx(6, L + 121), &[], None).unwrap_or_else(|e| panic!("{route}: {e}"));
        assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{route}: finalized, never redrawn");
    }

    let short = door(claim_id, vec![v3(claim_id, FIRST_ANCHOR, partial[1], PalwReceiptVerdictV2::Valid, L + 1)]);
    let (s4, _) = apply(&s3, &p, &ctx(5, L + 1), &[short], None).expect("one more partial");
    assert_eq!(s4.claim(&claim_id).unwrap().rcore.basis_k, 1, "two segments still once");
    assert_eq!(s4.deadline_of(&claim_id), Some(GATE), "the gate stands");
    let (s5, _) = apply(&s4, &p, &ctx(6, L + 121), &[], None).expect("past L + 120");
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not finalized unbacked");
}

/// **Q-7's duties follow an S2 licence while it awaits its upgrade**: every seat is on duty while the
/// panel is bound; once S2 licenses, exactly the seats it did not count (on duty, not credited, no
/// lock); a seat a supplementary set credits (a `Valid`, or a `Sampled` for pay) leaves; after the
/// upgrade nobody. The fence-off twin: nobody once the claim leaves `PanelBound`, as before M4.
#[test]
fn seat_duties_follow_an_s2_licence_until_it_upgrades() {
    let seats_of = |state: &PalwChainStateV2| -> Vec<PalwBondKeyV2> {
        let mut out: Vec<PalwBondKeyV2> = palw_seat_duties_v2(state, &params(true), &bonds()).iter().map(|d| d.seat_bond).collect();
        out.sort();
        out
    };
    let p = params(true);
    let (s2_state, _) = bound(&p);
    let mut all = bonds();
    all.sort();
    assert_eq!(seats_of(&s2_state), all, "a bound panel: every seat");
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let mut rest = partial[1..].to_vec();
    rest.sort();
    assert_eq!(seats_of(&s3), rest, "an S2 licence: the seats it did not count");
    for seat in &rest {
        assert!(palw_seat_uncounted_on_licence_v1(&s3, &claim_id, seat));
    }
    let duty = palw_seat_duties_v2(&s3, &p, &[partial[1]]).pop().expect("a duty");
    assert_eq!((duty.bound_daa, duty.claim_id), (BOUND, claim_id), "the bound panel's own facts");

    let sampled = door(claim_id, vec![v3(claim_id, FIRST_ANCHOR, partial[1], PalwReceiptVerdictV2::Sampled, L + 1)]);
    let (s4, _) = apply(&s3, &p, &ctx(5, L + 1), &[sampled], None).expect("a Sampled rides");
    let mut left = partial[2..].to_vec();
    left.sort();
    assert_eq!(seats_of(&s4), left, "a credited Sampled owes nothing more");
    let upgrade =
        Obj::ReceiptLicensed { claim: claim_id, receipts: vec![v2(claim_id, partial[2], PalwReceiptVerdictV2::Valid, L + 2)] };
    let (s5, _) = apply(&s4, &p, &ctx(6, L + 2), &[upgrade], None).expect("the V2 door upgrades");
    assert!(!palw_rcore_licence_awaits_replay_v1(s5.claim(&claim_id).unwrap()));
    assert!(seats_of(&s5).is_empty(), "a replay-backed licence owes no seat an answer");

    let off = params(false);
    let (t3, _, _, _) = s2_licensed(&off);
    assert!(palw_seat_duties_v2(&t3, &off, &bonds()).is_empty(), "below the fence a licence ends every duty");
}

/// **The collector offers exactly what the door credits** (Q-7, SR-10). The pool: the three other
/// partial seats' `Valid`s — one of them first as a junk copy the validator refuses — plus a seat's
/// `Sampled` beside its `Valid`, an abstention, the licence's own rider (already counted), and a
/// receipt of another claim. The selection keeps one receipt a seat, `Valid` first, drops everything
/// the door would not credit, and the fold credits exactly the set: basis 1 → 2 (Q-5's upgrade) and
/// SR-1b's release inside `L + 60`. A pool of a `Sampled` alone is offered for pay: credited, no
/// upgrade, no release. Below the fence the fold's answer is `None`.
#[test]
fn the_collector_offers_exactly_what_the_door_credits() {
    let p = params(true);
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let at = ctx(5, L + 1);
    let valid = |seat: PalwBondKeyV2| v3(claim_id, FIRST_ANCHOR, seat, PalwReceiptVerdictV2::Valid, L + 1);
    let mut junk = valid(partial[2]);
    junk.receipt.signature = vec![0xBA];
    let mut elsewhere = valid(partial[1]);
    elsewhere.receipt.claim = h64(0xE15E);
    let pool = vec![
        v3(claim_id, FIRST_ANCHOR, partial[3], PalwReceiptVerdictV2::Sampled, L + 1),
        junk,
        v3(claim_id, FIRST_ANCHOR, partial[1], PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: BOUND }, L + 1),
        valid(partial[0]),
        elsewhere,
        valid(partial[1]),
        valid(partial[2]),
        valid(partial[3]),
    ];
    let verify = |_: &[u8], _: &[u8], signature: &[u8], _: &[u8]| signature != [0xBA];
    let select = |pool: &[PalwSeatReceiptV3], p: &PalwStateParamsV2| {
        palw_select_supplementary_v3_v1(
            &s3,
            &claim_id,
            pool,
            |set| validate_supplementary_receipts_v3(&s3, p, &at, h64(999), &claim_id, set, verify),
            |set| palw_v2_supplementary_effect_v1(&s3, p, &at, &door(claim_id, set.to_vec()), false, false, false, false, &extras()),
        )
    };
    let (set, effect) = select(&pool, &p).expect("a set");
    assert_eq!(set, vec![valid(partial[1]), valid(partial[2]), valid(partial[3])], "one Valid a seat, the junk and the rest dropped");
    let mut credited_seats = partial[1..].to_vec();
    credited_seats.sort_by_key(|seat| bonds().iter().position(|b| b == seat));
    assert_eq!(effect.credited, credited_seats, "in panel order");
    assert_eq!((effect.basis_k_before, effect.basis_k_after, effect.upgrades), (1, 2, true));
    assert!(effect.releases_escrow, "every seat served, inside SR-1b's window");
    assert_eq!(effect.lands_by_daa, L + 60, "the release's clock is the one to race");
    let (s4, _) = apply(&s3, &p, &at, &[door(claim_id, set)], None).expect("the door takes the offer");
    for seat in &partial[1..] {
        assert!(credited(&s4, claim_id, *seat) && s4.slashable_lock(*seat, claim_id).is_some());
    }
    assert!(s4.claim(&claim_id).unwrap().rcore.escrow_released);

    let (set, effect) = select(&[v3(claim_id, FIRST_ANCHOR, partial[3], PalwReceiptVerdictV2::Sampled, L + 1)], &p).expect("pay");
    assert_eq!(set.len(), 1);
    assert_eq!(effect.credited, vec![partial[3]]);
    assert!(!effect.upgrades && !effect.releases_escrow);
    assert_eq!(effect.lands_by_daa, GATE - 1, "the receipt deadline");
    assert!(
        select(&[v3(claim_id, FIRST_ANCHOR, partial[3], PalwReceiptVerdictV2::Incapable, L + 1)], &p).is_none(),
        "nothing to credit"
    );

    let off = params(false);
    let (t3, twin_id, _, twin_partial) = s2_licensed(&off);
    let set = door(twin_id, vec![v3(twin_id, FIRST_ANCHOR, twin_partial[1], PalwReceiptVerdictV2::Valid, L + 1)]);
    assert_eq!(palw_v2_supplementary_effect_v1(&t3, &off, &at, &set, false, false, false, false, &extras()), None);
}
