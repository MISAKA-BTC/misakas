//! **ADR-0152 v3.1 M5 (§7.1, §8.3 item 1) on F4 part 2: T40's DL-1 rows for Q-5's gate and T01's
//! twins of its lifecycles, on testnet-12's own fold.**
//!
//! Every run is recorded on M5's [`Tape`] (`rcore_common`): each block's delta re-applies and
//! reverts and its carriage reloads under its root as it is written; then the run restarts at EVERY
//! tip (the carriage encoded, decoded and loaded exactly as the store loads it, `into_state_v3` under
//! the committed root with the processor's flags, and the rest of the run folded on the LOADED
//! state, every later child, delta and root equal to the uninterrupted run's), reverts block by block
//! to its base with every reverted tip loading, re-applies, is folded again from a genesis rebuilt
//! from scratch (the IBD twin), and reorgs to a sibling fork and back. A `CarriageInconsistent`
//! anywhere fails the test.
//!
//! At every tip each claim's deadline on the LOADED state is checked against [`dl1`], DL-1's table
//! (ADR §3.13) spelled here from the rooted facts and the tip's DAA — never read off the fold's
//! index or `palw_rcore_deadline_v1`. The rows these runs reach:
//!
//! * **(a) an S2 licence (`basis_k` 1):** `max(L + wc(L), bound + window_receipt + 1, last)` —
//!   restarted mid-gate, past `L + wc(L)` where a replay-backed licence would already be `Final`;
//! * **(b) the upgrade in a block `U ≥ L + wc(L)`** through SR-10's V3 door and through the V2 door
//!   (V3S-01: a counted partial seat's whole-job V2 `Valid` after a pay set credited it), each in a
//!   block `U > L + wc(L)` so `U` and the floor differ: re-derived to `max(L + wc(L), U) = U`,
//!   restarted just before and just after `U`, `Final` at `U + 1`; and the V3 door's boundary `U =
//!   L + wc(L)` on a sibling;
//! * **(c) the gate on the first panel redraws** (`redraw_unreplayed_licence_v1`): the locks, the duty
//!   row and its credit released, `rebound_daa` the sweep's DAA, the commitment unmoved, a bystander's
//!   open DA session untouched; restarted mid-redraw;
//! * **(d) the gate on the second panel voids `NotReplayBacked`** through `void_and_slash`, S-4's S0′
//!   arm (`w + E + rr`, no strike, no action), charged exactly as its sibling's second
//!   `ReceiptTimeout`; restarted, reverted and reorged across it — on a floor attempt (`E > 0`,
//!   `rr = 0`) and on a compute-priced free-prompt claim (`rr > 0`, `E = 0`), so each term of the
//!   charge is one some run depends on;
//! * **(e) a court cleared on an S2 licence** (`ChallengerDefeated`) re-arms through DL-1
//!   (`rearm_claim_after_court_close`): the gate row, not the plain `max(L + wc(L), clearing)`.
//!
//! **T01's twins**: the escrow held on `basis_k < 2`, released at the upgrade (SR-1b, both doors,
//! inside `L + wc(L) / 2` and on its last DAA), held to `Final` past that window (one DAA past it,
//! and at `L + wc(L)`), and forfeited at `NotReplayBacked` — the same roots under revert, IBD and
//! restart.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_m5_q5_gate

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_state_v2::{
    PalwClaimSourceV2, PalwVoidReasonV2, palw_claim_receipt_deadline_v1, palw_rcore_licence_awaits_replay_v1,
    palw_rcore_release_window_closes_v1, palw_rcore_v2_door_takes_seat_v1,
};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

/// A testnet-12 chain with `palw_offence_attribution` armed as the processor resolves it, on a tape.
fn armed_tape() -> Tape {
    let mut c = Chain::new(t12());
    c.attribution = true;
    Tape::new(c)
}

/// testnet-12's genesis rebuilt from scratch (the IBD twin's base).
fn scratch_genesis() -> PalwChainStateV2 {
    Chain::new(t12()).s
}

/// **DL-1's deadline for `id` on `s` at a tip whose last point is `last`** — ADR §3.13's table for
/// the rows these runs reach, from the rooted record alone:
///
/// | claim | deadline |
/// |---|---|
/// | non-terminal, a seat DA session open | none |
/// | `Provisional` (first panel or after a redraw) | `bind_base + window_bind` |
/// | `PanelBound` | `bound + window_receipt` |
/// | `ReceiptLicensed`, a court open | none |
/// | `ReceiptLicensed`, `basis_k ≥ 2` (or no door recorded) | `max(L + wc(L), last)` |
/// | `ReceiptLicensed`, a door recorded and `basis_k < 2` | `max(L + wc(L), bound + window_receipt + 1, last)` |
/// | terminal, any DA session open | none |
/// | terminal | `max(terminal + claim_retirement_daa, last_closed + 1)` |
///
/// `None` also for a claim the state no longer holds (retired).
fn dl1(sp: &PalwStateParamsV2, s: &PalwChainStateV2, id: &Hash64, last: u64) -> Option<u64> {
    let claim = s.claim(id)?;
    let da = s.da_claim(id);
    let seat_open = da.is_some_and(|record| record.open_seat_sessions > 0);
    let any_open = da.is_some_and(|record| record.open_seat_sessions + record.open_other_sessions > 0);
    if !claim.phase.is_terminal() && seat_open {
        return None;
    }
    let terminal = |at: u64| -> Option<u64> {
        if any_open {
            return None;
        }
        let retire = at + sp.claim_retirement_daa();
        Some(match da.and_then(|record| record.last_closed_daa) {
            Some(closed) => retire.max(closed + 1),
            None => retire,
        })
    };
    match claim.phase {
        PalwClaimPhaseV2::Provisional => Some(claim.rebound_daa.unwrap_or(claim.accepted_daa) + sp.window_bind()),
        PalwClaimPhaseV2::PanelBound { bound_daa } => Some(bound_daa + sp.window_receipt()),
        PalwClaimPhaseV2::ReceiptLicensed { .. } if s.court_sessions_iter().any(|(_, court)| court.claim == *id) => None,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
            let floor = licensed_daa + sp.window_challenge_at(licensed_daa);
            if claim.rcore.licence_door.is_some() && claim.rcore.basis_k < 2 {
                let bound = s.panel(id).expect("a licensed claim's panel").bound_daa;
                Some(floor.max(bound + sp.window_receipt() + 1).max(last))
            } else {
                Some(floor.max(last))
            }
        }
        PalwClaimPhaseV2::Final { final_daa } => terminal(final_daa),
        PalwClaimPhaseV2::Voided { voided_daa, .. } => terminal(voided_daa),
        ref other => panic!("{id}: a phase these runs never reach: {other:?}"),
    }
}

/// **Every tip of `t` restarts equal to the uninterrupted run, and on each LOADED state every claim of
/// `claims` has [`dl1`]'s deadline.** Returns how many (tip, claim) rows were checked.
fn restart_everywhere_against_dl1(t: &Tape, claims: &[Hash64]) -> usize {
    let mut rows = 0;
    for j in 0..=t.len() {
        let loaded = t.restart_at(j);
        let last = t.daa_at(j);
        for id in claims {
            assert_eq!(
                loaded.deadline_of(id),
                dl1(&t.c.sp, &loaded, id, last),
                "tip {j} (DAA {last}): {id}'s DL-1 row, re-derived at load: {:?}",
                loaded.claim(id).map(|claim| (&claim.phase, claim.rcore.basis_k))
            );
            rows += 1;
        }
    }
    rows
}

/// The full seat and the four partial seats (panel indices) of `id`'s current panel.
fn geometry(t: &Tape, id: &Hash64) -> (usize, [usize; 4]) {
    let full = palw_segment_assignment_v2(t.c.anchor(id), *id, 5).full_seat as usize;
    (full, [(full + 1) % 5, (full + 2) % 5, (full + 3) % 5, (full + 4) % 5])
}

/// `id`'s current panel's bound DAA.
fn bound_of(t: &Tape, id: &Hash64) -> u64 {
    t.c.s.panel(id).expect("a bound panel").bound_daa
}

/// Whether the chain credited `seat` on `id`'s duty row.
fn credited(s: &PalwChainStateV2, id: Hash64, seat: PalwBondKeyV2) -> bool {
    s.panel_duties_of(&id).and_then(|row| row.get(&seat)).is_some_and(|at| *at != 0)
}

/// The producer's reserved ledger.
fn ledger(t: &Tape, producer: &PalwBondKeyV2) -> u128 {
    t.c.s.reserved_exposure(producer)
}

/// **An S2 licence of `id` in the next block**: the full seat's and the first partial seat's V3
/// `Valid`s (`OptimisticLicensed`, as the licence-stall fix builds it). `basis_k` 1, the door recorded
/// Optimistic, the escrow held (SR-1 cond. 1: the producer's ledger unmoved), the anchor not settled
/// (V-8), and DL-1's Q-5 row at the licence. Returns `L`.
fn s2_licence(t: &mut Tape, id: Hash64) -> u64 {
    let seats = t.c.floor_seats();
    let bound = bound_of(t, &id);
    let anchor = t.c.anchor(&id);
    let (full, partial) = geometry(t, &id);
    let producer = t.c.s.claim(&id).unwrap().bond;
    let (held, settled) = (ledger(t, &producer), t.c.s.settled_attempt_finals());
    t.step(vec![PalwConsensusObjectV2::OptimisticLicensed {
        claim: id,
        receipts: covered(id, anchor, &seats, &[full, partial[0]], bound),
    }]);
    let l = t.c.daa;
    let claim = t.c.s.claim(&id).unwrap().clone();
    assert_eq!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: l }, "S2 licenses");
    assert_eq!(
        (claim.rcore.licence_door, claim.rcore.basis_k, claim.rcore.escrow_released),
        (Some(PalwLicenceDoorTagV1::Optimistic), 1, false),
        "S2: the full seat and one rider recount to 1, the escrow held"
    );
    assert!(palw_rcore_licence_awaits_replay_v1(&claim));
    assert_eq!(ledger(t, &producer), held, "SR-1 cond. 1: an S2 licence holds E on the producer's ledger");
    assert_eq!(t.c.s.settled_attempt_finals(), settled, "V-8: an S2 licence does not settle the anchor");
    let gate = bound + t.c.sp.window_receipt() + 1;
    assert_eq!(
        palw_claim_receipt_deadline_v1(&t.c.s, &t.c.sp, &id, &claim),
        Ok(Some(gate - 1)),
        "the floor keeps the network's window_receipt: both doors shut at bound + window_receipt"
    );
    assert!(l + t.c.sp.window_challenge_at(l) < gate, "the premise: the gate term dominates L + wc(L) on the floor");
    assert_eq!(t.c.s.deadline_of(&id), Some(gate), "DL-1's Q-5 row at the licence: bound + window_receipt + 1");
    l
}

/// **(a) and (b): DL-1's Q-5 row for an S2 licence, and its re-arm by an upgrade in a block `U ≥
/// L + wc(L)` through both supplementary doors.** One chain carries three S2 licences:
///
/// * G is never upgraded. Its deadline is the gate row `max(L + wc(L), bound + window_receipt + 1,
///   last)` at every tip; a block past `L + wc(L)` (where a replay-backed licence would be `Final`)
///   and a block AT the gate itself leave it licensed.
/// * V3 upgrades through SR-10's V3 door (the three other partial seats' `Valid`s) in block
///   `U3 = L + wc(L) + 5` — past the floor and before the doors shut at `bound + window_receipt`, so
///   a re-arm that kept the floor (or any `U` but this block's) is a different deadline: `basis_k` 2,
///   recorded Coverage, the anchor settled (V-8), the escrow still held (U3 is past SR-1b's window),
///   and the deadline re-derived to `max(L + wc(L), U3) = U3`; `Final` at `U3 + 1`, not at the
///   floor's `L + wc(L) + 1`. A sibling lands the same set ON the floor (`U = L + wc(L)`, the `≥`'s
///   boundary): the deadline `L + wc(L)`, `Final` at `L + wc(L) + 1`.
/// * V2: a third party's pay set credits two partial seats' V3 `Valid`s first (no upgrade: the silent
///   seat's segment has one replay; the gate stands); then, in `U2 > L + wc(L)`, one of those seats'
///   whole-job V2 `Valid` through the V2 door (V3S-01: `palw_rcore_v2_door_takes_seat_v1`) widens its
///   lock to the whole cut (amount, expiry and second clock unmoved) and upgrades: the deadline
///   `U2`, `Final` at `U2 + 1`.
///
/// Restarts at every tip — just before and just after each `U` among them — load DL-1's row; a
/// sibling fork on which the V3 upgrade does not land keeps V3 gated past `U3 + 1`, and the reorg to
/// it and back reverts the re-arm; the boundary sibling restarts everywhere and reverts too; the run
/// reverts to its base and replays by IBD.
#[test]
fn t40_dl1_the_q5_gate_row_and_the_upgrade_in_block_u_through_both_doors() {
    let mut t = armed_tape();
    let sp = t.c.sp.clone();
    let wr = sp.window_receipt();
    let seats = t.c.floor_seats();
    let (producer, _, _) = floor_producer(&t.c.p);
    // G: gated, never upgraded.
    let g = t.attempt(None, 0x5A01);
    let g_bound = t.bind(g);
    let g_l = s2_licence(&mut t, g);
    let g_gate = g_bound + wr + 1;
    // V3: gated, then upgraded through the V3 door at U3 = L + wc(L).
    let v3 = t.attempt(None, 0x5A03);
    let v3_bound = t.bind(v3);
    let v3_l = s2_licence(&mut t, v3);
    // V2: gated; a pay set first; the V2 door's widening upgrade at U2 > L + wc(L).
    let v2 = t.attempt(None, 0x5A02);
    let v2_bound = t.bind(v2);
    let v2_l = s2_licence(&mut t, v2);
    let (_, v2_partial) = geometry(&t, &v2);
    let v2_anchor = t.c.anchor(&v2);
    t.step(vec![PalwConsensusObjectV2::ReceiptLicensedV2 {
        claim: v2,
        receipts: covered(v2, v2_anchor, &seats, &[v2_partial[1], v2_partial[2]], v2_bound),
    }]);
    let paid = t.c.s.claim(&v2).unwrap().clone();
    assert_eq!(paid.rcore.basis_k, 1, "V2's pay set: the silent seat's segment still has one replay");
    assert!(credited(&t.c.s, v2, seats[v2_partial[1]].0) && credited(&t.c.s, v2, seats[v2_partial[2]].0), "the pay set credits both");
    assert!(
        palw_rcore_v2_door_takes_seat_v1(&t.c.s, &v2, &paid, &seats[v2_partial[1]].0),
        "V3S-01: the V2 door still takes the credited seat's whole-job replay"
    );
    assert_eq!(t.c.s.deadline_of(&v2), Some(v2_bound + wr + 1), "a pay set leaves the gate where it was");
    // Mid-gate: past G's L + wc(L), where a replay-backed licence would already be Final.
    let mid = g_l + sp.window_challenge_at(g_l) + 1;
    t.at(mid, vec![]);
    let mid_tip = t.len();
    for id in [g, v3, v2] {
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{id}: gated, not finalized");
    }
    assert_eq!(t.c.s.deadline_of(&g), Some(g_gate), "G mid-gate");
    // U3 = L + wc(L) + 5: the V3 door with the three other partial seats, past the floor.
    let v3_floor = v3_l + sp.window_challenge_at(v3_l);
    let u3 = v3_floor + 5;
    assert!(v3_floor > mid, "the premise: V3's floor lands after the mid-gate tip");
    assert!(u3 != v3_floor && u3 <= v3_bound + wr, "the premise: U3 is past the floor and both doors still take the set");
    let (_, v3_partial) = geometry(&t, &v3);
    let v3_anchor = t.c.anchor(&v3);
    let settled = t.c.s.settled_attempt_finals();
    let held = ledger(&t, &producer);
    let before_u3 = t.len();
    let v3_upgrade =
        PalwConsensusObjectV2::ReceiptLicensedV2 { claim: v3, receipts: covered(v3, v3_anchor, &seats, &v3_partial[1..], v3_bound) };
    t.at(u3, vec![v3_upgrade.clone()]);
    let after_u3 = t.len();
    let up = t.c.s.claim(&v3).unwrap().clone();
    assert_eq!((up.rcore.basis_k, up.rcore.licence_door), (2, Some(PalwLicenceDoorTagV1::Coverage)), "V3: the V3 door upgrades");
    assert!(!palw_rcore_licence_awaits_replay_v1(&up));
    assert!(!up.rcore.escrow_released, "U3 is past SR-1b's window: the escrow stays held to Final");
    assert_eq!(ledger(&t, &producer), held, "V3's upgrade moves no commitment past the window");
    assert_eq!(t.c.s.settled_attempt_finals(), settled + 1, "V-8: the upgrade settles the anchor");
    assert_eq!(t.c.s.deadline_of(&v3), Some(u3), "Q-5: max(L + wc(L), U3) = U3, not the floor {v3_floor}");
    // U3 + 1: V3 finalizes, nobody else moves.
    t.at(u3 + 1, vec![]);
    assert!(matches!(t.c.s.claim(&v3).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "V3: Final at U3 + 1");
    // U2 = L + wc(L) + 7: V2's whole-job V2 Valid from the seat its pay set credited.
    let u2 = v2_l + sp.window_challenge_at(v2_l) + 7;
    assert!(u2 > u3 + 1);
    let widen = seats[v2_partial[1]].0;
    let lock_before = *t.c.s.slashable_lock(widen, v2).expect("the pay set locked the seat's V3 Valid");
    assert!(!lock_before.attested.is_full(4), "a partial mask before");
    let (settled, held) = (t.c.s.settled_attempt_finals(), ledger(&t, &producer));
    let before_u2 = t.len();
    t.at(u2, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: v2, receipts: vec![valid(v2, widen, v2_bound)] }]);
    let after_u2 = t.len();
    let up = t.c.s.claim(&v2).unwrap().clone();
    assert_eq!((up.rcore.basis_k, up.rcore.licence_door), (2, Some(PalwLicenceDoorTagV1::Coverage)), "V2: the V2 door upgrades");
    assert!(!palw_rcore_licence_awaits_replay_v1(&up) && !up.rcore.escrow_released);
    assert_eq!(ledger(&t, &producer), held, "V2's upgrade past the window moves no commitment");
    assert_eq!(t.c.s.settled_attempt_finals(), settled + 1, "V-8: the V2 door's upgrade settles the anchor too");
    let widened = *t.c.s.slashable_lock(widen, v2).unwrap();
    assert!(widened.attested.is_full(4), "V3S-01: the lock is widened to the whole cut");
    assert_eq!(
        (widened.amount, widened.expiry_daa, widened.settled_at_final),
        (lock_before.amount, lock_before.expiry_daa, lock_before.settled_at_final),
        "never repriced, never re-dated (Q-4)"
    );
    assert_eq!(t.c.s.deadline_of(&v2), Some(u2), "Q-5: max(L + wc(L), U2) = U2");
    t.at(u2 + 1, vec![]);
    assert!(matches!(t.c.s.claim(&v2).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "V2: Final at U2 + 1");
    // A block AT G's gate leaves it licensed: the sweep takes deadlines strictly below its block.
    t.at(g_gate, vec![]);
    assert!(matches!(t.c.s.claim(&g).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "G at its gate: not yet swept");
    assert_eq!(t.c.s.deadline_of(&g), Some(g_gate));

    // The named restarts: mid-gate, just before and just after each U.
    let gate_row = |s: &PalwChainStateV2, id: &Hash64, last: u64| {
        let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = s.claim(id).unwrap().phase else { panic!("{id} licensed") };
        (licensed_daa + sp.window_challenge_at(licensed_daa)).max(s.panel(id).unwrap().bound_daa + wr + 1).max(last)
    };
    let loaded = t.restart_at(mid_tip);
    assert_eq!(loaded.deadline_of(&g), Some(gate_row(&loaded, &g, mid)), "(a) G restarted mid-gate");
    assert_eq!(loaded.deadline_of(&g), Some(g_gate));
    for (before, after, id, u, bound) in [(before_u3, after_u3, v3, u3, v3_bound), (before_u2, after_u2, v2, u2, v2_bound)] {
        let loaded = t.restart_at(before);
        assert_eq!(loaded.deadline_of(&id), Some(bound + wr + 1), "(b) {id} just before U: the gate row");
        let loaded = t.restart_at(after);
        assert_eq!(loaded.deadline_of(&id), Some(u), "(b) {id} just after U: max(L + wc(L), U)");
    }
    // A sibling on which the V3 upgrade does not land: V3 stays gated past U3 + 1.
    let mut sibling = t.fork(before_u3);
    sibling.at(u3, vec![]);
    sibling.at(u3 + 1, vec![]);
    assert!(
        matches!(sibling.c.s.claim(&v3).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "without the upgrade V3 is not finalized unbacked"
    );
    assert_eq!(sibling.c.s.deadline_of(&v3), Some(v3_bound + wr + 1));
    // The boundary sibling: the same V3 set ON the floor, U = L + wc(L).
    let mut on_floor = t.fork(before_u3);
    on_floor.at(v3_floor, vec![v3_upgrade]);
    let on_floor_tip = on_floor.len();
    assert_eq!(on_floor.c.s.claim(&v3).unwrap().rcore.basis_k, 2, "the boundary: upgraded on the floor");
    assert_eq!(on_floor.c.s.deadline_of(&v3), Some(v3_floor), "Q-5 at U = L + wc(L): max(L + wc(L), U) = L + wc(L)");
    on_floor.at(v3_floor + 1, vec![]);
    assert!(matches!(on_floor.c.s.claim(&v3).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the boundary: Final at L + wc(L) + 1");
    assert_eq!(on_floor.restart_at(on_floor_tip).deadline_of(&v3), Some(v3_floor), "(b) restarted just after the boundary U");
    t.reorg_to(before_u3, &sibling);
    t.reorg_to(before_u3, &on_floor);
    let rows = restart_everywhere_against_dl1(&t, &[g, v3, v2])
        + restart_everywhere_against_dl1(&sibling, &[g, v3, v2])
        + restart_everywhere_against_dl1(&on_floor, &[g, v3, v2]);
    t.revert_to_base_and_reapply();
    sibling.revert_to_base_and_reapply();
    on_floor.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    println!(
        "(a)+(b): {} blocks, {rows} (tip, claim) DL-1 rows checked at load; V3's floor {v3_floor}, U3 {u3}, U2 {u2}, G's gate {g_gate}",
        t.len()
    );
}

/// **(c): the gate on the first panel redraws (`redraw_unreplayed_licence_v1`), with a bystander's DA
/// session open.** R is S2-licensed; a bystander accuses it (a non-seat session: it pauses nothing,
/// V3S-08) one block before the gate; a block at the gate leaves R licensed; the first block past it
/// redraws, exactly as a first `ReceiptTimeout` would:
///
/// * `Provisional`, `rebound_daa` the sweep's DAA, the `Default` record;
/// * the S2 signers' locks released, the duty row and its credit gone, every seat's exposure back to
///   what it was before R bound;
/// * the commitment unmoved — the claim's `reserved`, `escrowed_reward` and `rights_reserved`, and the
///   producer's ledger (`w + esc + rr` throughout: SR-4 not engaged), nothing slashed, no strike, no
///   anchor tick;
/// * the bystander's session and the claim's DA record untouched (DA-5: a redraw is not an answer);
/// * DL-1: `rebound + window_bind`.
///
/// The claim then waits mid its rebind window and the second panel binds. Restarts at every tip —
/// the redraw's among them — load DL-1's row and the open session; a sibling on which the bystander
/// never accused redraws the same way; the run reverts and replays by IBD.
#[test]
fn t40_dl1_q5_the_first_panel_redraws_with_a_bystanders_session_open() {
    let mut t = armed_tape();
    let sp = t.c.sp.clone();
    let bystander = bond_key(61);
    t.step(vec![bond_obj(61, 400_000 * MSK)]);
    let seats = t.c.floor_seats();
    let (producer, _, _) = floor_producer(&t.c.p);
    let exposure_before: Vec<u128> = seats.iter().map(|(k, _)| t.c.s.reserved_exposure(k)).collect();
    let r = t.attempt(None, 0x5C01);
    let bound = t.bind(r);
    s2_licence(&mut t, r);
    let (full, partial) = geometry(&t, &r);
    let gate = bound + sp.window_receipt() + 1;
    let fork_at = t.len();
    t.at(gate - 1, vec![da_accuse(r, bystander, 0)]);
    let session = t.c.s.da_session(&r, &bystander).cloned().expect("the bystander's session is open");
    let record = t.c.s.da_claim(&r).cloned().expect("the claim's DA record");
    assert!(!session.accuser_is_seat && session.deadline_daa > gate + 1, "the premise: a non-seat session outliving the redraw");
    assert_eq!(t.c.s.deadline_of(&r), Some(gate), "a bystander's session does not pause the gate (V3S-08)");
    t.at(gate, vec![]);
    let licensed = t.c.s.claim(&r).unwrap().clone();
    assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "at the gate: not yet swept");
    for i in [full, partial[0]] {
        assert!(t.c.s.slashable_lock(seats[i].0, r).is_some() && credited(&t.c.s, r, seats[i].0), "the S2 signer {i} is locked");
    }
    let before = t.c.s.clone();
    let redraw = gate + 1;
    t.at(redraw, vec![]);
    let redraw_tip = t.len();
    let redrawn = t.c.s.claim(&r).unwrap().clone();
    assert_eq!(redrawn.phase, PalwClaimPhaseV2::Provisional, "the first panel redraws, it does not void");
    assert_eq!(redrawn.rebound_daa, Some(redraw), "the one redraw is spent, anchored on the sweep");
    assert_eq!(redrawn.rcore, PalwClaimRcoreV1::default(), "a Provisional claim carries the Default record");
    assert_eq!(
        (redrawn.reserved, redrawn.escrowed_reward, redrawn.rights_reserved, redrawn.bond),
        (licensed.reserved, licensed.escrowed_reward, licensed.rights_reserved, licensed.bond),
        "the claim's commitment is unchanged"
    );
    for i in [full, partial[0]] {
        assert!(t.c.s.slashable_lock(seats[i].0, r).is_none(), "the S2 signer {i}'s lock is released");
    }
    assert!(t.c.s.panel_duties_of(&r).is_none(), "the duty row and its credit are gone");
    for (i, (k, _)) in seats.iter().enumerate() {
        assert_eq!(t.c.s.reserved_exposure(k), exposure_before[i], "seat {i}: its duty exposure is back");
    }
    assert_eq!(ledger(&t, &producer), before.reserved_exposure(&producer), "w + esc + rr throughout: SR-4 not engaged");
    assert_eq!(t.c.s.bond(&producer).unwrap().slashed, before.bond(&producer).unwrap().slashed, "the producer forfeits nothing");
    assert!(t.c.s.withholding_strikes(&producer).is_none(), "no strike");
    assert_eq!(t.c.s.settled_attempt_finals(), before.settled_attempt_finals(), "a redraw ticks nothing");
    assert_eq!(t.c.s.da_session(&r, &bystander), Some(&session), "DA-5: the bystander's session is untouched");
    assert_eq!(t.c.s.da_claim(&r), Some(&record), "and the claim's DA record");
    assert_eq!(t.c.s.deadline_of(&r), Some(redraw + sp.window_bind()), "DL-1: the bind window from the rebound base");
    // Mid the rebind window; then the second panel.
    t.at(redraw + 3, vec![]);
    let bound2 = t.bind(r);
    assert_eq!(t.c.s.deadline_of(&r), Some(bound2 + sp.window_receipt()), "the second panel's receipt row");
    // Restart mid-redraw: the loaded state is the redrawn one, its session open, DL-1's row.
    let loaded = t.restart_at(redraw_tip);
    assert_eq!(loaded.deadline_of(&r), Some(redraw + sp.window_bind()));
    assert_eq!(loaded.da_session(&r, &bystander), Some(&session));
    // A sibling on which nobody accused: the same redraw, no session.
    let mut sibling = t.fork(fork_at);
    sibling.at(gate, vec![]);
    sibling.at(redraw, vec![]);
    let quiet = sibling.c.s.claim(&r).unwrap();
    assert_eq!((quiet.phase.clone(), quiet.rebound_daa), (PalwClaimPhaseV2::Provisional, Some(redraw)));
    assert!(sibling.c.s.da_claim(&r).is_none() && sibling.c.s.slashable_lock(seats[full].0, r).is_none());
    t.reorg_to(fork_at, &sibling);
    let rows = restart_everywhere_against_dl1(&t, &[r]) + restart_everywhere_against_dl1(&sibling, &[r]);
    t.revert_to_base_and_reapply();
    sibling.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    println!("(c): redrawn at {redraw} (gate {gate}), rebound at {bound2}; {} blocks, {rows} DL-1 rows at load", t.len());
}

/// **(d): the gate on the second panel voids `NotReplayBacked` through `void_and_slash`, S-4's S0′
/// arm.** N — a floor ATTEMPT, so `E > 0` and `rr = 0` (an attempt reserves no receipt rights; the
/// `rr > 0` twin is [`t40_dl1_q5_a_compute_priced_fp_licence_voids_not_replay_backed_forfeiting_its_receipt_rights`])
/// — is S2-licensed, redrawn at its first gate (the commitment unmoved, the first panel's
/// locks released), re-bound, S2-licensed again (gated from the second panel's bound), and at the
/// second gate voided `NotReplayBacked` at the sweep's DAA: the producer is charged `w + E + rr` —
/// `slashed` and `collateral` move by exactly that, and the reservation leaves its ledger — with no
/// strike, no action tier and no anchor tick. Its sibling, forked after the second bind, never
/// licenses the second panel and voids `ReceiptTimeout` (RT#2) at `bound₂ + window_receipt + 1`:
/// charged the same. Both tapes restart at every tip (DL-1's terminal row at load), revert to the
/// base and re-apply; the run replays by IBD and reorgs to the sibling and back across the void.
#[test]
fn t40_dl1_q5_the_second_panel_voids_not_replay_backed_charged_as_rt2() {
    let mut t = armed_tape();
    let sp = t.c.sp.clone();
    let wr = sp.window_receipt();
    let seats = t.c.floor_seats();
    let (producer, _, _) = floor_producer(&t.c.p);
    let n = t.attempt(None, 0x5D01);
    let bound1 = t.bind(n);
    s2_licence(&mut t, n);
    let (full1, partial1) = geometry(&t, &n);
    let committed = ledger(&t, &producer);
    let gate1 = bound1 + wr + 1;
    t.at(gate1 + 1, vec![]);
    let redrawn = t.c.s.claim(&n).unwrap().clone();
    assert_eq!(
        (redrawn.phase.clone(), redrawn.rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(gate1 + 1)),
        "the first gate redraws"
    );
    assert!(t.c.s.slashable_lock(seats[full1].0, n).is_none() && t.c.s.slashable_lock(seats[partial1[0]].0, n).is_none());
    assert_eq!(ledger(&t, &producer), committed, "the redraw moves no commitment");
    let bound2 = t.bind(n);
    let fork_at = t.len();
    s2_licence(&mut t, n);
    let gate2 = bound2 + wr + 1;
    assert_eq!(t.c.s.deadline_of(&n), Some(gate2), "gated again, from the second panel's bound");
    t.at(gate2, vec![]);
    let before = t.c.s.clone();
    let claim = before.claim(&n).unwrap().clone();
    let commitment = u64::try_from(claim.reserved + escrow(&sp, &claim) + claim.rights_reserved).unwrap();
    assert!(commitment > 0 && escrow(&sp, &claim) > 0, "the premise: an attempt's charge carries E");
    assert_eq!(claim.rights_reserved, 0, "the premise: an attempt reserves no receipt rights (rr = 0 on this run)");
    let void_at = gate2 + 1;
    t.at(void_at, vec![]);
    let void_tip = t.len();
    assert_eq!(
        t.c.s.claim(&n).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: void_at, reason: PalwVoidReasonV2::NotReplayBacked },
        "the second panel's gate voids NotReplayBacked"
    );
    let bond_before = before.bond(&producer).unwrap().clone();
    let bond_after = t.c.s.bond(&producer).unwrap().clone();
    let charged = bond_after.slashed - bond_before.slashed;
    assert_eq!(charged, commitment, "S0′: w + E + rr, and no action tier");
    assert_eq!(bond_before.collateral - bond_after.collateral, charged, "burned from the collateral");
    assert_eq!(
        before.reserved_exposure(&producer) - ledger(&t, &producer),
        u128::from(commitment),
        "the reservation leaves the producer's ledger with the void"
    );
    assert!(t.c.s.withholding_strikes(&producer).is_none(), "S0′ writes no strike");
    assert_eq!(t.c.s.settled_attempt_finals(), before.settled_attempt_finals(), "no anchor tick");
    assert_eq!(t.c.s.deadline_of(&n), Some(void_at + sp.claim_retirement_daa()), "DL-1's terminal row");
    // The sibling: the second panel never licenses — RT#2.
    let mut sibling = t.fork(fork_at);
    let rt2 = bound2 + wr + 1;
    sibling.at(rt2, vec![]);
    assert_eq!(
        sibling.c.s.claim(&n).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: rt2, reason: PalwVoidReasonV2::ReceiptTimeout },
        "the sibling's second panel times out"
    );
    let rt2_charged = sibling.c.s.bond(&producer).unwrap().slashed - sibling.base.bond(&producer).unwrap().slashed;
    assert_eq!(charged, rt2_charged, "NotReplayBacked is charged exactly as the second ReceiptTimeout");
    // Restart just before and just after the void, then everywhere; revert, IBD, reorg across it.
    assert_eq!(t.restart_at(void_tip - 1).deadline_of(&n), Some(gate2));
    assert_eq!(t.restart_at(void_tip).deadline_of(&n), Some(void_at + sp.claim_retirement_daa()));
    let rows = restart_everywhere_against_dl1(&t, &[n]) + restart_everywhere_against_dl1(&sibling, &[n]);
    t.revert_to_base_and_reapply();
    sibling.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    t.reorg_to(fork_at, &sibling);
    println!(
        "(d): redrawn at {}, rebound at {bound2}, voided NotReplayBacked at {void_at}, charged {charged} = RT#2's; {rows} rows",
        gate1 + 1
    );
}

/// The free-prompt executor bond of the `rr > 0` run.
const FP_BOND: u64 = 51;

/// **A floor with its free-prompt lane ready and bond [`FP_BOND`] registered to use it** (M5's
/// [`fp_floor_ready`]), folded with `work_target_active` as the processor resolves it on testnet-12
/// (`room_extras`; armed at DAA 0) — the one extra that `fp_receipt_rights_inputs_v1` needs besides
/// the 2026-09-23 audit fence, so a compute-priced commitment in a block with a subsidy reserves its
/// receipt rights. Deterministic: called twice it stands on the same root (the IBD twin's base).
fn fp_rights_tape() -> (Tape, u64) {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.room = true;
    let collateral = at_least_the_floor(&c.p, 100_000 * MSK);
    fp_floor_ready(c, FP_BOND, collateral)
}

/// **(d) with `rr > 0`: a compute-priced free-prompt claim through Q-5's gate on both panels — the
/// S0′ charge's receipt-rights term.** `OptimisticLicensed` does not restrict the claim's source, so a
/// free-prompt claim reaches the gate exactly as an attempt does, and on testnet-12 its receipt
/// rights (`rr`, fixed at acceptance: `compute × worker_carve / W₀`) are the charge's largest term.
/// F is committed in a block with the network's subsidy (`w > 0`, `rr > 0`, `E = 0`: the FP lane
/// escrows nothing; the ledger takes `w + rr`), bound, S2-licensed (`basis_k` 1, gated), redrawn at
/// its first gate — the claim keeps `w` and `rr`, the ledger keeps `w + rr`, the first panel's locks
/// released — re-bound, S2-licensed again, and at the second gate voided `NotReplayBacked`: charged
/// exactly `w + E + rr` (`slashed` and `collateral` move by it, the reservation leaves the ledger
/// whole — no abandon hold, which is `BindTimeout`'s), no strike, DL-1's terminal row. Its sibling,
/// forked after the second bind, times out (RT#2) and is charged the same. Both tapes restart at
/// every tip against DL-1 and revert to their base; the run replays by IBD from a base rebuilt from
/// scratch and reorgs to the sibling and back across the void.
#[test]
fn t40_dl1_q5_a_compute_priced_fp_licence_voids_not_replay_backed_forfeiting_its_receipt_rights() {
    let (mut t, leaves) = fp_rights_tape();
    let sp = t.c.sp.clone();
    let wr = sp.window_receipt();
    let seats = t.c.floor_seats();
    let bond = bond_key(FP_BOND);
    let before = ledger(&t, &bond);
    let (commit, f) = fp_commit_of(&t.c, FP_BOND, leaves, 0x5D0F);
    let skips = t.block(t.c.daa + 1, vec![commit], None, T12_BLOCK_SUBSIDY_SOMPI).expect("the commitment's block folds");
    assert!(skips.is_empty(), "the commitment is accepted: {skips:?}");
    let accepted = t.c.s.claim(&f).expect("the commitment is a claim").clone();
    assert!(matches!(accepted.source, PalwClaimSourceV2::FreePrompt { .. }));
    let (w, e, rr) = (accepted.reserved, escrow(&sp, &accepted), accepted.rights_reserved);
    assert!(w > 0 && rr > 0, "the premise: compute-priced, the receipt rights reserved beside the weight (w {w}, rr {rr})");
    assert_eq!(e, 0, "the FP lane escrows nothing");
    assert_eq!(ledger(&t, &bond) - before, w + rr, "acceptance commits w + E + rr");
    // The first panel: S2, the gate, the redraw.
    let bound1 = t.bind(f);
    s2_licence(&mut t, f);
    let (full1, partial1) = geometry(&t, &f);
    assert!(
        t.c.s.slashable_lock(seats[full1].0, f).is_some() && t.c.s.slashable_lock(seats[partial1[0]].0, f).is_some(),
        "the S2 signers lock"
    );
    let gate1 = bound1 + wr + 1;
    t.at(gate1 + 1, vec![]);
    let redraw_tip = t.len();
    let redrawn = t.c.s.claim(&f).unwrap().clone();
    assert_eq!(
        (redrawn.phase.clone(), redrawn.rebound_daa),
        (PalwClaimPhaseV2::Provisional, Some(gate1 + 1)),
        "the first gate redraws the free-prompt claim"
    );
    assert_eq!((redrawn.reserved, redrawn.rights_reserved), (w, rr), "the redraw keeps the weight and the receipt rights");
    assert_eq!(ledger(&t, &bond) - before, w + rr, "and the ledger keeps w + E + rr");
    assert!(t.c.s.slashable_lock(seats[full1].0, f).is_none() && t.c.s.slashable_lock(seats[partial1[0]].0, f).is_none());
    // The second panel: S2 again, the gate, NotReplayBacked.
    let bound2 = t.bind(f);
    let fork_at = t.len();
    s2_licence(&mut t, f);
    let gate2 = bound2 + wr + 1;
    t.at(gate2, vec![]);
    let pre = t.c.s.clone();
    let claim = pre.claim(&f).unwrap().clone();
    assert_eq!((claim.reserved, claim.rights_reserved, escrow(&sp, &claim)), (w, rr, 0), "the second licence holds the same terms");
    let charge = w + e + rr;
    let void_at = gate2 + 1;
    t.at(void_at, vec![]);
    let void_tip = t.len();
    assert_eq!(
        t.c.s.claim(&f).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: void_at, reason: PalwVoidReasonV2::NotReplayBacked },
        "the second panel's gate voids the free-prompt claim NotReplayBacked"
    );
    let (bond_before, bond_after) = (pre.bond(&bond).unwrap().clone(), t.c.s.bond(&bond).unwrap().clone());
    let charged = u128::from(bond_after.slashed - bond_before.slashed);
    assert_eq!(charged, charge, "S0′: w + E + rr with rr = {rr} > 0, and no action tier");
    assert!(charged > w, "the receipt rights are charged, not only the weight");
    assert_eq!(u128::from(bond_before.collateral - bond_after.collateral), charged, "burned from the collateral");
    assert_eq!(pre.reserved_exposure(&bond) - ledger(&t, &bond), w + rr, "the whole reservation leaves the ledger with the void");
    assert_eq!(ledger(&t, &bond), before, "no abandon hold on a NotReplayBacked void");
    assert!(t.c.s.withholding_strikes(&bond).is_none(), "S0′ writes no strike");
    assert_eq!(t.c.s.deadline_of(&f), Some(void_at + sp.claim_retirement_daa()), "DL-1's terminal row: the retirement");
    // The sibling: the second panel never licenses — RT#2, charged the same.
    let mut sibling = t.fork(fork_at);
    let rt2 = bound2 + wr + 1;
    sibling.at(rt2, vec![]);
    assert_eq!(
        sibling.c.s.claim(&f).unwrap().phase,
        PalwClaimPhaseV2::Voided { voided_daa: rt2, reason: PalwVoidReasonV2::ReceiptTimeout },
        "the sibling's second panel times out"
    );
    let rt2_charged = sibling.c.s.bond(&bond).unwrap().slashed - sibling.base.bond(&bond).unwrap().slashed;
    assert_eq!(charged, u128::from(rt2_charged), "NotReplayBacked is charged exactly as the second ReceiptTimeout, rr included");
    // Restart mid-redraw, just before and just after the void, then everywhere; revert, IBD, reorg.
    let loaded = t.restart_at(redraw_tip);
    assert_eq!(loaded.reserved_exposure(&bond) - before, w + rr, "the held w + rr re-derived at load mid-redraw");
    assert_eq!(t.restart_at(void_tip - 1).deadline_of(&f), Some(gate2));
    assert_eq!(t.restart_at(void_tip).reserved_exposure(&bond), before, "the void's release re-derived at load");
    let rows = restart_everywhere_against_dl1(&t, &[f]) + restart_everywhere_against_dl1(&sibling, &[f]);
    t.revert_to_base_and_reapply();
    sibling.revert_to_base_and_reapply();
    t.ibd_from(fp_rights_tape().0.base);
    t.reorg_to(fork_at, &sibling);
    println!(
        "(d) rr > 0: w {w}, rr {rr}; redrawn at {}, rebound at {bound2}, voided NotReplayBacked at {void_at}, charged {charged} = RT#2's; {rows} rows",
        gate1 + 1
    );
}

/// **(e): a court cleared on an S2 licence keeps the gate** (`rearm_claim_after_court_close` through
/// DL-1; ADR §3.13 "a court clearing … just re-evaluates DL-1"). A bystander opens a court on K one
/// block before `L + wc(L)`: the deadline is gone (DL-1: "open court: none"). It is cleared
/// (`ChallengerDefeated`) past `L + wc(L)`, and the deadline is the gate row `bound + window_receipt
/// + 1` — not the plain re-arm `max(L + wc(L), clearing)`, which would finalize K unbacked in the
/// next block. The block after the clearing leaves K licensed, and the first block past the gate
/// redraws it (the gate still fires after a court). A sibling on which the court is never cleared
/// keeps K without a deadline; the reorg between them re-arms and disarms. A second sibling, forked
/// before the court, opens it one block before the gate instead (a court may open on a licence
/// until it is terminal), holds K across the gate, and clears it in block `C > bound +
/// window_receipt + 1`: the gate row's third term, `last`, is the deadline (`C`, both doors long
/// shut), and `C + 1` redraws. Every tip of all three restarts against DL-1; each reverts to its
/// base; the run replays by IBD.
#[test]
fn t40_dl1_q5_a_court_clearing_on_an_s2_licence_keeps_the_gate() {
    let mut t = armed_tape();
    let sp = t.c.sp.clone();
    let bystander = bond_key(61);
    t.step(vec![bond_obj(61, 400_000 * MSK)]);
    let k = t.attempt(None, 0x5E01);
    let bound = t.bind(k);
    let l = s2_licence(&mut t, k);
    let floor = l + sp.window_challenge_at(l);
    let gate = bound + sp.window_receipt() + 1;
    let open = floor - 1;
    t.at(open, vec![court_opened(&t.c.s, k, bystander)]);
    let open_tip = t.len();
    let session = court_session_of(&t.c.s, k, bystander);
    assert!(t.c.s.court_sessions_iter().any(|(id, _)| *id == session), "the court is open");
    assert_eq!(t.c.s.deadline_of(&k), None, "DL-1: a licence under an open court owes no deadline");
    let close = open + sp.turn_deadline_daa().clamp(1, 4);
    assert!(close > floor, "the premise: the clearing lands past L + wc(L) (turn_deadline {})", sp.turn_deadline_daa());
    t.at(close, vec![court_cleared(session)]);
    let close_tip = t.len();
    assert!(!t.c.s.court_sessions_iter().any(|(_, court)| court.claim == k), "the court is closed");
    assert!(matches!(t.c.s.claim(&k).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "cleared, still licensed");
    assert!(t.c.s.bond(&bystander).unwrap().slashed > 0, "the defeated challenger pays");
    assert_eq!(t.c.s.deadline_of(&k), Some(gate), "the gate row, not max(L + wc(L), clearing) = {close}");
    t.at(close + 1, vec![]);
    assert!(
        matches!(t.c.s.claim(&k).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "the plain re-arm would have finalized K here, unbacked"
    );
    t.at(gate + 1, vec![]);
    let redrawn = t.c.s.claim(&k).unwrap().clone();
    assert_eq!((redrawn.phase.clone(), redrawn.rebound_daa), (PalwClaimPhaseV2::Provisional, Some(gate + 1)), "the gate fires");
    assert_eq!(t.restart_at(open_tip).deadline_of(&k), None, "restarted mid-court");
    assert_eq!(t.restart_at(close_tip).deadline_of(&k), Some(gate), "restarted just after the clearing");
    // The sibling: the court is not cleared.
    let mut sibling = t.fork(open_tip);
    sibling.at(close, vec![]);
    assert_eq!(sibling.c.s.deadline_of(&k), None, "still under court");
    // The late sibling: a court opened at gate - 1 holds K across its gate, and clears in C > gate.
    let late = gate + 2;
    assert!(
        gate - 1 + sp.turn_deadline_daa() > late,
        "the premise: the responder's rung is still open at {late} (turn_deadline {})",
        sp.turn_deadline_daa()
    );
    let held_at = open_tip - 1;
    let mut held = t.fork(held_at);
    held.at(gate - 1, vec![court_opened(&held.c.s, k, bystander)]);
    assert_eq!(court_session_of(&held.c.s, k, bystander), session, "the same session id on the sibling");
    held.at(gate + 1, vec![]);
    assert_eq!(held.c.s.deadline_of(&k), None, "a court holds K across its gate");
    assert!(matches!(held.c.s.claim(&k).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not swept under court");
    held.at(late, vec![court_cleared(session)]);
    let late_tip = held.len();
    assert!(matches!(held.c.s.claim(&k).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "cleared past the gate, licensed");
    assert_eq!(held.c.s.deadline_of(&k), Some(late), "the gate row's last term: max(L + wc(L), gate, C) = C");
    held.at(late + 1, vec![]);
    let redrawn = held.c.s.claim(&k).unwrap().clone();
    assert_eq!((redrawn.phase.clone(), redrawn.rebound_daa), (PalwClaimPhaseV2::Provisional, Some(late + 1)), "C + 1 redraws");
    assert_eq!(held.restart_at(late_tip).deadline_of(&k), Some(late), "restarted just after the late clearing");
    t.reorg_to(open_tip, &sibling);
    t.reorg_to(held_at, &held);
    let rows = restart_everywhere_against_dl1(&t, &[k])
        + restart_everywhere_against_dl1(&sibling, &[k])
        + restart_everywhere_against_dl1(&held, &[k]);
    t.revert_to_base_and_reapply();
    sibling.revert_to_base_and_reapply();
    held.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    println!(
        "(e): court {open}..{close} on an S2 licence (L {l}, floor {floor}), gate {gate}, late clearing {late}; {rows} DL-1 rows at load"
    );
}

/// **T01's twins for Q-5's lifecycles: the escrow held on `basis_k < 2`, released at the upgrade
/// (SR-1b, through both doors), held to `Final` past SR-1b's window, forfeited at `NotReplayBacked`.**
/// Five S2 licences on one chain, each holding `E` on the producer's ledger at its licence:
///
/// * A: SR-10's V3 door with the three other partial seats inside `L + wc(L) / 2` — every seat served,
///   `E` leaves the ledger in that block;
/// * B: a third party's pay set of two partial seats' V3 `Valid`s (no upgrade, `E` held), then the V2
///   door inside the window with one of them WIDENED (its whole-job replay, V3S-01) and the silent
///   seat's own V2 `Valid` — upgraded, every seat served, `E` leaves in that block;
/// * D: the V3 door ON the window's last DAA, `min(L + ⌊wc(L) / 2⌋, bound + window_receipt)` —
///   released; its sibling, forked just before, lands the same set one DAA later — upgraded but
///   held, `E` released only with `w` at `Final`;
/// * C: the V3 door at `U = L + wc(L)`, past the window — upgraded but held: `E` stays to `Final`;
/// * N: never upgraded — redrawn (commitment unmoved), re-bound, S2 again, voided `NotReplayBacked`:
///   `w + E + rr` forfeited (an attempt: `rr = 0`; the free-prompt twin with `rr > 0` is (d)'s
///   compute-priced test), the reservation off the ledger.
///
/// `Final` releases the rest for A, B, C and D; the producer's ledger ends where it began and its
/// collateral is down exactly N's `w + E`. The run reverts block by block to testnet-12's genesis
/// (each reverted tip loading under its root) and re-applies, is folded again from a genesis rebuilt
/// from scratch with the same deltas and roots, and restarts at every tip with DL-1's rows; D's
/// sibling reverts to its fork point and restarts at every tip too.
#[test]
fn t01_q5_twins_escrow_held_on_s2_released_at_the_upgrade_and_forfeited_at_not_replay_backed() {
    let mut t = armed_tape();
    let sp = t.c.sp.clone();
    let wr = sp.window_receipt();
    let seats = t.c.floor_seats();
    let (producer, _, _) = floor_producer(&t.c.p);
    let base = ledger(&t, &producer);
    let collateral0 = t.c.s.bond(&producer).unwrap().collateral;
    let s2 = |t: &mut Tape, seed: u64| {
        let id = t.attempt(None, seed);
        let bound = t.bind(id);
        let l = s2_licence(t, id);
        (id, bound, l)
    };
    let (a, a_bound, a_l) = s2(&mut t, 0x5F0A);
    let (b, b_bound, b_l) = s2(&mut t, 0x5F0B);
    let (c, c_bound, c_l) = s2(&mut t, 0x5F0C);
    let (n, n_bound, _) = s2(&mut t, 0x5F0D);
    let (d, d_bound, d_l) = s2(&mut t, 0x5F0E);
    // One supplementary set in its own block: the recount, the release and the producer's ledger.
    let upgrade = |t: &mut Tape, id: Hash64, at: u64, object: PalwConsensusObjectV2, upgrades: bool, released: bool, what: &str| {
        let claim = t.c.s.claim(&id).unwrap().clone();
        let e = escrow(&sp, &claim);
        let held = ledger(t, &producer);
        t.at(at, vec![object]);
        let after = t.c.s.claim(&id).unwrap().clone();
        assert_eq!(!palw_rcore_licence_awaits_replay_v1(&after), upgrades, "{what}: the upgrade");
        assert_eq!(after.rcore.escrow_released, released, "{what}: SR-1b");
        assert_eq!(
            held - ledger(t, &producer),
            if released { e } else { 0 },
            "{what}: E leaves the ledger in this block iff released"
        );
        assert!(e > 0);
    };
    // B's pay set, then its V2 upgrade: the widened seat and the silent one.
    let (_, bp) = geometry(&t, &b);
    let b_anchor = t.c.anchor(&b);
    let pay = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: b, receipts: covered(b, b_anchor, &seats, &[bp[1], bp[2]], b_bound) };
    let at = t.c.daa + 1;
    upgrade(&mut t, b, at, pay, false, false, "B's pay set");
    let at = t.c.daa + 1;
    assert!(at <= b_l + sp.window_challenge_at(b_l) / 2, "the premise: B's V2 set lands inside SR-1b's window");
    let v2_set = PalwConsensusObjectV2::ReceiptLicensed {
        claim: b,
        receipts: vec![valid(b, seats[bp[1]].0, b_bound), valid(b, seats[bp[3]].0, b_bound)],
    };
    upgrade(&mut t, b, at, v2_set, true, true, "B (V2 door: widened + the silent seat)");
    assert!(t.c.s.slashable_lock(seats[bp[1]].0, b).unwrap().attested.is_full(4), "B: the widened lock");
    // A: the V3 door inside the window.
    let (_, ap) = geometry(&t, &a);
    let a_anchor = t.c.anchor(&a);
    let at = t.c.daa + 1;
    assert!(at <= a_l + sp.window_challenge_at(a_l) / 2, "the premise: A's V3 set lands inside SR-1b's window");
    let v3_set = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: a, receipts: covered(a, a_anchor, &seats, &ap[1..], a_bound) };
    upgrade(&mut t, a, at, v3_set, true, true, "A (V3 door inside the window)");
    // D: the V3 door on SR-1b's last DAA — released; one DAA later (the sibling) — held.
    let (_, dp) = geometry(&t, &d);
    let d_anchor = t.c.anchor(&d);
    let d_edge = palw_rcore_release_window_closes_v1(&sp, d_l, d_bound + wr);
    let d_final = d_l + sp.window_challenge_at(d_l) + 1;
    assert!(d_edge > t.c.daa && d_edge == d_l + sp.window_challenge_at(d_l) / 2, "the premise: the window's L + wc(L) / 2 term binds");
    let d_set = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: d, receipts: covered(d, d_anchor, &seats, &dp[1..], d_bound) };
    let mut d_late = t.fork(t.len());
    upgrade(&mut t, d, d_edge, d_set.clone(), true, true, "D (V3 door on the window's last DAA)");
    upgrade(&mut d_late, d, d_edge + 1, d_set, true, false, "D's sibling (one DAA past the window: held)");
    let d_claim = d_late.c.s.claim(&d).unwrap().clone();
    let (d_w, d_e) = (d_claim.reserved, escrow(&sp, &d_claim));
    // C: the V3 door at U = L + wc(L), past the window (A and B finalize in the same block).
    let (_, cp) = geometry(&t, &c);
    let c_anchor = t.c.anchor(&c);
    let c_u = c_l + sp.window_challenge_at(c_l);
    let late = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: c, receipts: covered(c, c_anchor, &seats, &cp[1..], c_bound) };
    let a_final = a_l + sp.window_challenge_at(a_l) + 1;
    let b_final = b_l + sp.window_challenge_at(b_l) + 1;
    assert!(a_final < b_final && b_final < c_u, "the premise: A and B finalize before C's upgrade");
    t.at(b_final, vec![]);
    for id in [a, b] {
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id}: Final");
    }
    // D's sibling: A and B finalize there too, then D with w and its held E.
    assert!(b_final < d_final, "the premise: D finalizes after A and B");
    d_late.at(b_final, vec![]);
    let held = ledger(&d_late, &producer);
    d_late.at(d_final, vec![]);
    assert!(matches!(d_late.c.s.claim(&d).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "D's sibling: Final");
    assert_eq!(held - ledger(&d_late, &producer), d_w + d_e, "D's sibling: Final releases w and the held E");
    upgrade(&mut t, c, c_u, late, true, false, "C (V3 door at L + wc(L): held)");
    let c_claim = t.c.s.claim(&c).unwrap().clone();
    let (c_w, c_e) = (c_claim.reserved, escrow(&sp, &c_claim));
    let held = ledger(&t, &producer);
    t.at(c_u + 1, vec![]);
    assert!(matches!(t.c.s.claim(&c).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "C: Final at U + 1");
    assert_eq!(held - ledger(&t, &producer), c_w + c_e, "C: Final releases w and the held E");
    // D finalizes with only w left to release (its E left at the upgrade).
    assert!(d_final > c_u + 1, "the premise: D finalizes after C");
    let held = ledger(&t, &producer);
    t.at(d_final, vec![]);
    assert!(matches!(t.c.s.claim(&d).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "D: Final");
    assert_eq!(held - ledger(&t, &producer), d_w, "D: Final releases w alone");
    // N: the first gate redraws, the second voids NotReplayBacked.
    let committed = ledger(&t, &producer);
    t.at(n_bound + wr + 2, vec![]);
    assert_eq!(t.c.s.claim(&n).unwrap().phase, PalwClaimPhaseV2::Provisional, "N: redrawn");
    assert_eq!(ledger(&t, &producer), committed, "N: the redraw keeps w + E + rr");
    let n_bound2 = t.bind(n);
    s2_licence(&mut t, n);
    let n_claim = t.c.s.claim(&n).unwrap().clone();
    let forfeit = n_claim.reserved + escrow(&sp, &n_claim) + n_claim.rights_reserved;
    assert_eq!(n_claim.rights_reserved, 0, "the premise: N is an attempt (rr = 0)");
    assert_eq!(ledger(&t, &producer), committed, "N: its second S2 licence holds E too");
    t.at(n_bound2 + wr + 2, vec![]);
    assert!(matches!(t.c.s.claim(&n).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::NotReplayBacked, .. }));
    assert_eq!(committed - ledger(&t, &producer), forfeit, "N: the reservation leaves the ledger");
    assert_eq!(ledger(&t, &producer), base, "the producer's ledger ends where it began");
    assert_eq!(u128::from(collateral0 - t.c.s.bond(&producer).unwrap().collateral), forfeit, "N's w + E + rr forfeited, nothing else");
    // The twins.
    t.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    d_late.revert_to_base_and_reapply();
    let rows = restart_everywhere_against_dl1(&t, &[a, b, c, d, n]) + restart_everywhere_against_dl1(&d_late, &[d]);
    println!(
        "T01 Q-5 twins: {} blocks reverted to genesis, replayed from scratch, restarted at every tip ({rows} DL-1 rows)",
        t.len()
    );
}
