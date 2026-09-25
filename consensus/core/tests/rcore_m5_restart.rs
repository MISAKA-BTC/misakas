//! **ADR-0152 v3.1 M5 (§7.1, §8.3 item 1): the restart half — T01's revert and IBD twins, T40 with
//! DL-1's restarts (IMPL-2), and T83, on testnet-12's own fold.**
//!
//! Every run is recorded on a [`Tape`] (`rcore_common`): each block's delta re-applies and reverts
//! and its carriage reloads under its root as it is written; the run then reverts block by block to
//! its base — each reverted tip LOADED from its carriage bytes under its root, the ledger, R-core+'s
//! load invariants and DL-1's deadlines re-derived (T40's load after a revert) — and re-applies; it
//! is folded again from a genesis rebuilt from scratch (the IBD twin); and at the tips named below
//! the carriage is encoded, decoded and loaded exactly as the store loads it (`into_state_v3` under
//! the committed root, the processor's flags) and the rest of the run is folded on the LOADED state,
//! every later child, delta and root equal to the uninterrupted run's (the restart twin). A
//! `CarriageInconsistent` anywhere fails the test.
//!
//! **Not here:** DL-1's `ReceiptLicensed` row for `basis_k < 2` under Q-5's gate (`max(licensed +
//! window_challenge_at, bound + window_receipt + 1, last)`), an S2 upgrade in a block `U ≥` that
//! floor through both supplementary doors, the first-panel redraw, the second panel's
//! `NotReplayBacked`, a court clearing on an S2 licence, and T01's twins of those lifecycles —
//! F4 part 2's rows, in `rcore_m5_q5_gate.rs` on the same [`Tape`]. The S2 licence and its SR-1b
//! upgrade through the V2 door also ride T01's lifecycle below, where every tip restarts.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_m5_restart

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2;

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

/// The V1 door's receipts: `Valid` from `valid_seats`, `Unavailable` from `unavailable_seats`.
fn v1(
    id: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    valid_seats: &[usize],
    unavailable_seats: &[usize],
    signed: u64,
) -> PalwConsensusObjectV2 {
    let mut receipts: Vec<_> = valid_seats.iter().map(|i| valid(id, seats[*i].0, signed)).collect();
    receipts.extend(unavailable_seats.iter().map(|i| unavailable(id, seats[*i].0, signed)));
    PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }
}

/// Restart at every tip of `t` and at its tip: every restart loads and replays equal.
fn restart_everywhere(t: &Tape) {
    for j in 0..=t.len() {
        t.restart_at(j);
    }
}

/// **T01's revert and IBD twins: the staged lifecycle.** On one chain, floor claims licensed every
/// way `rcore/int-3` licenses them, each checked at its licence against SR-1 and at its Final:
///
/// * A: the V1 quorum door, five `Valid`s — `E` released at the licence (the producer's ledger falls by
///   `E`, `escrow_released`, `basis_k` 3);
/// * B: the coverage door, the full seat and every partial — released (`basis_k` 2);
/// * C: three `Valid`s and two `Unavailable`s — held to Final (`unserved_seen`);
/// * D: three `Valid`s and two missing — held to Final;
/// * E: redrawn (its first panel silent past `window_receipt`), rebound and licensed by five `Valid`s —
///   held (SR-1: never released after a redraw);
/// * F: S2 (the full seat and one partial, `basis_k` 1) — held; then the other three seats' `Valid`s
///   through the V2 door at `L + 60` (SR-1b) — released in that block;
/// * G1, G2: three `Valid`s and two missing — held; then SR-10's V3 supplementary door with the two
///   missing seats: G2 two `Valid`s — released in that block (SR-1b through the V3 door); G1 a `Valid`
///   and a `Sampled` — the `Sampled` credited, no lock, no served bit, `unserved_seen` latched: held
///   to Final (Q-1, IA-5).
///
/// Final releases `w` for each, and the producer's ledger ends where it began. The run then reverts
/// block by block to testnet-12's genesis (each reverted tip loading under its root) and re-applies,
/// is folded again from a genesis rebuilt from scratch with the same deltas and roots, and restarts
/// at every tip with no `CarriageInconsistent`.
#[test]
fn t01_revert_and_ibd_twins_of_the_staged_lifecycle() {
    let mut t = armed_tape();
    let (producer, _, _) = floor_producer(&t.c.p);
    let seats = t.c.floor_seats();
    let base = t.c.s.reserved_exposure(&producer);
    // E first: its first panel runs out.
    let e_id = t.attempt(None, 0x01E0);
    let e_bound = t.bind(e_id);
    t.at(e_bound + t.c.sp.window_receipt() + 1, vec![]);
    assert!(matches!(t.c.s.claim(&e_id).unwrap().phase, PalwClaimPhaseV2::Provisional), "RT#1 redraws E");
    let e_rebound = t.bind(e_id);
    let mut expect: Vec<(Hash64, bool, u8)> = Vec::new();
    let mut licence = |t: &mut Tape, id: Hash64, object: PalwConsensusObjectV2, released: bool, basis: u8, what: &str| {
        let claim = t.c.s.claim(&id).unwrap().clone();
        let (w, e) = (claim.reserved, escrow(&t.c.sp, &claim));
        let before = t.c.s.reserved_exposure(&producer);
        t.step(vec![object]);
        let licensed = t.c.s.claim(&id).unwrap().clone();
        assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{what}: licensed");
        assert_eq!(
            (licensed.rcore.escrow_released, licensed.rcore.basis_k),
            (released, basis),
            "{what}: SR-1's release and Q-3's recount"
        );
        assert_eq!(licensed.rcore.g_res_sompi > 0, true, "{what}: the licence froze G_res");
        let after = t.c.s.reserved_exposure(&producer);
        assert_eq!(before - after, if released { e } else { 0 }, "{what}: E leaves at the licence iff released");
        assert!(w > 0 && e > 0);
        expect.push((id, released, basis));
    };
    licence(&mut t, e_id, v1(e_id, &seats, &[0, 1, 2, 3, 4], &[], e_rebound), false, 3, "E (redrawn)");
    let a = t.attempt(None, 0x01A0);
    let bound = t.bind(a);
    licence(&mut t, a, v1(a, &seats, &[0, 1, 2, 3, 4], &[], bound), true, 3, "A (V1 quorum, full service)");
    let b = t.attempt(None, 0x01B0);
    let bound = t.bind(b);
    let anchor = t.c.anchor(&b);
    licence(
        &mut t,
        b,
        PalwConsensusObjectV2::ReceiptLicensedV2 { claim: b, receipts: covered(b, anchor, &seats, &[0, 1, 2, 3, 4], bound) },
        true,
        2,
        "B (coverage)",
    );
    let cc = t.attempt(None, 0x01C0);
    let bound = t.bind(cc);
    licence(&mut t, cc, v1(cc, &seats, &[0, 1, 2], &[3, 4], bound), false, 3, "C (two Unavailable)");
    assert!(t.c.s.claim(&cc).unwrap().rcore.unserved_seen, "C latched unserved_seen");
    let d = t.attempt(None, 0x01D0);
    let bound = t.bind(d);
    licence(&mut t, d, v1(d, &seats, &[0, 1, 2], &[], bound), false, 3, "D (two missing)");
    let f = t.attempt(None, 0x01F0);
    let f_bound = t.bind(f);
    let anchor = t.c.anchor(&f);
    let assignment = palw_segment_assignment_v2(anchor, f, 5);
    let full = assignment.full_seat as usize;
    let partial = (full + 1) % 5;
    licence(
        &mut t,
        f,
        PalwConsensusObjectV2::OptimisticLicensed { claim: f, receipts: covered(f, anchor, &seats, &[full, partial], f_bound) },
        false,
        1,
        "F (S2)",
    );
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = t.c.s.claim(&f).unwrap().phase else { unreachable!() };
    let e_f = escrow(&t.c.sp, &t.c.s.claim(&f).unwrap());
    let before = t.c.s.reserved_exposure(&producer);
    let rest: Vec<_> = (0..5).filter(|i| *i != full && *i != partial).map(|i| valid(f, seats[i].0, f_bound)).collect();
    t.at(licensed_daa + 60, vec![PalwConsensusObjectV2::ReceiptLicensed { claim: f, receipts: rest }]);
    let up = t.c.s.claim(&f).unwrap().clone();
    assert!(up.rcore.escrow_released && up.rcore.basis_k >= 2, "F: SR-1b's upgrade releases E: {:?}", up.rcore);
    assert_eq!(before - t.c.s.reserved_exposure(&producer), e_f, "F: E leaves in the upgrade's block");
    // G1 / G2: three `Valid`s and two missing (held), then SR-10's V3 supplementary door inside
    // SR-1b's window with the two missing seats — G2 both `Valid` (served: the flip releases E in that
    // block), G1 one `Valid` and one `Sampled` (Q-1: credited for pay, no lock, no served bit, and it
    // latches `unserved_seen`, so it never flips: held to Final).
    let mut supplementary = |t: &mut Tape, sampled: bool, seed: u64, what: &str| -> Hash64 {
        let id = t.attempt(None, seed);
        let bound = t.bind(id);
        licence(t, id, v1(id, &seats, &[0, 1, 2], &[], bound), false, 3, what);
        let anchor = t.c.anchor(&id);
        let mut receipts = covered(id, anchor, &seats, &[3, 4], bound);
        if sampled {
            receipts[1].receipt.verdict = PalwReceiptVerdictV2::Sampled;
        }
        let claim = t.c.s.claim(&id).unwrap().clone();
        let e = escrow(&t.c.sp, &claim);
        let before = t.c.s.reserved_exposure(&producer);
        t.step(vec![PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
        let after = t.c.s.claim(&id).unwrap().clone();
        let credited = |k: &PalwBondKeyV2| t.c.s.panel_duties_of(&id).and_then(|row| row.get(k)).is_some_and(|at| *at != 0);
        assert!(credited(&seats[3].0) && credited(&seats[4].0), "{what}: both supplementary seats credited for pay");
        assert!(t.c.s.slashable_lock(seats[3].0, id).is_some(), "{what}: the Valid locks");
        if sampled {
            assert!(t.c.s.slashable_lock(seats[4].0, id).is_none(), "{what}: a Sampled takes no lock (Q-1)");
            assert_eq!(after.rcore.served_mask & (1 << 4), 0, "{what}: a Sampled sets no served bit");
            assert!(after.rcore.unserved_seen && !after.rcore.escrow_released, "{what}: Sampled latches unserved_seen: held");
            assert_eq!(t.c.s.reserved_exposure(&producer), before, "{what}: E stays on the ledger");
        } else {
            assert!(after.rcore.escrow_released && !after.rcore.unserved_seen, "{what}: served by every seat: SR-1b flips");
            assert_eq!(before - t.c.s.reserved_exposure(&producer), e, "{what}: E leaves in the V3 door's block");
        }
        id
    };
    let g1 = supplementary(&mut t, true, 0x0161, "G1 (V3 door: a Valid and a Sampled)");
    let g2 = supplementary(&mut t, false, 0x0162, "G2 (V3 door: two Valids)");
    // Final releases w for every claim; the ledger ends where it began.
    let last = [a, b, cc, d, e_id, f, g1, g2]
        .iter()
        .map(|id| match t.c.s.claim(id).unwrap().phase {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa + t.c.sp.window_challenge_at(licensed_daa),
            ref other => panic!("{id} licensed: {other:?}"),
        })
        .max()
        .unwrap();
    t.at(last + 1, vec![]);
    for (id, _, _) in &expect {
        assert!(matches!(t.c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id} is Final");
        assert!(t.c.s.vesting_row(id).is_some(), "{id}: its row");
    }
    assert_eq!(t.c.s.reserved_exposure(&producer), base, "Final releases the rest: the producer's ledger is back where it began");
    // The twins.
    t.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    restart_everywhere(&t);
    println!("T01 twins: {} blocks reverted to genesis, replayed from scratch, restarted at every tip", t.len());
}

/// **T01's twins on the model rows: 8k released at licence and held on `Incapable`, 2M (C7) held to
/// Final.** Each class on its own tape standing on [`model_chain`]'s state (the class `Active`, the
/// genesis bonds proved ready at its first DAA — a tape takes no carriage edits, so every claim is
/// accepted, bound and licensed inside the readiness rows' eight one-DAA spans, and nothing after the
/// licence reads readiness):
///
/// * 8k: X, five `Valid`s — `E` released at the licence (U1: only C7 is held); Y, three `Valid`s and
///   two `Incapable` (refused on the floor, admitted on a model row) — held, `unserved_seen`;
/// * 2M: Z, five `Valid`s through the quorum door, every seat served — still held (SR-1 cond. 4, C7).
///
/// Final releases `w` (and a held `E`) for each, the producer's ledger back where it began and each row
/// written. Each tape then reverts block by block to its base (each reverted tip loading under its
/// root) and re-applies, is folded again from its base (the IBD twin: a model row's readiness is a
/// carriage edit here, so the scratch genesis is the tape's base), and restarts at every tip.
#[test]
fn t01_model_class_twins_8k_released_and_held_and_2m_held_to_final() {
    let p = t12();
    let (short, id2m) = model_classes(&p);
    for (class, what) in [(short, "8k"), (id2m, "2M")] {
        let mut c = model_chain(p.clone(), class, 1);
        c.attribution = true;
        let readied_at = c.daa - 1;
        let mut t = Tape::new(c);
        let producer = bond_key(1);
        let base = t.c.s.reserved_exposure(&producer);
        let seats = honest_seats(&t.c.p, 5);
        let mut claims: Vec<(Hash64, bool)> = Vec::new();
        let plans: Vec<(u64, bool)> = if class == short { vec![(0x01A8, false), (0x01B8, true)] } else { vec![(0x01C2, false)] };
        for (seed, incapable) in plans {
            let id = t.model_attempt(class, 1, seed);
            let bound = t.bind_to(id, &seats);
            let receipts: Vec<_> =
                seats
                    .iter()
                    .enumerate()
                    .map(|(i, (k, _))| {
                        if incapable && i >= 3 {
                            receipt(id, *k, PalwReceiptVerdictV2::Incapable, bound)
                        } else {
                            valid(id, *k, bound)
                        }
                    })
                    .collect();
            let claim = t.c.s.claim(&id).unwrap().clone();
            let e = escrow(&t.c.sp, &claim);
            let before = t.c.s.reserved_exposure(&producer);
            t.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
            assert!(t.c.daa < readied_at + 8, "{what}: licensed inside the readiness rows' eight spans");
            let licensed = t.c.s.claim(&id).unwrap().clone();
            assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{what}: licensed");
            let released = class == short && !incapable;
            assert_eq!(
                (licensed.rcore.escrow_released, licensed.rcore.unserved_seen, licensed.rcore.basis_k),
                (released, incapable, 3),
                "{what} {seed:#x}: SR-1 (U1: 8k released at licence, C7 held; an Incapable holds)"
            );
            assert!(licensed.rcore.g_res_sompi > 0, "{what}: the licence froze G_res");
            assert_eq!(before - t.c.s.reserved_exposure(&producer), if released { e } else { 0 }, "{what}: E leaves iff released");
            claims.push((id, released));
        }
        let last = claims
            .iter()
            .map(|(id, _)| match t.c.s.claim(id).unwrap().phase {
                PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa + t.c.sp.window_challenge_at(licensed_daa),
                ref other => panic!("{what}: licensed: {other:?}"),
            })
            .max()
            .unwrap();
        t.at(last + 1, vec![]);
        for (id, _) in &claims {
            assert!(matches!(t.c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{what}: {id} Final");
            assert!(t.c.s.vesting_row(id).is_some(), "{what}: {id}'s row");
        }
        assert_eq!(t.c.s.reserved_exposure(&producer), base, "{what}: Final releases the rest");
        t.revert_to_base_and_reapply();
        t.ibd_from(t.base.clone());
        restart_everywhere(&t);
        println!("T01 {what} twin: {} claims, {} blocks reverted, replayed and restarted at every tip", claims.len(), t.len());
    }
}

/// DL-1's deadline for `id` on `s`, read off the loaded state, against its row of the table.
fn deadline(s: &PalwChainStateV2, id: &Hash64) -> Option<u64> {
    s.deadline_of(id)
}

/// **T40 and DL-1 (IMPL-2): restart mid-session and mid-gate on `PanelBound`, `ReceiptLicensed`
/// (`basis_k` 3 and 2) and `Final` claims, and on redrawn and voided ones; and the load-time
/// re-derivation after a revert with released and held licensed claims and open DA sessions.**
///
/// One chain passes through, each named tip checked on the state LOADED from its carriage against its
/// DL-1 row:
///
/// * P `PanelBound` mid its receipt gate: `bound + window_receipt`;
/// * Q `PanelBound` with a seat's DA session open: no deadline (DA-5);
/// * R `ReceiptLicensed` (V1, `basis_k` 3, released) and S (coverage, `basis_k` 2, released) mid their
///   challenge gates: `max(licensed + window_challenge_at, last)`; H (three `Valid`s, two `Unavailable`,
///   held) with its unserved seat's session open: no deadline;
/// * V licensed with a bystander's session open (which does not pause it) reaches `Final` with the
///   session open (`FinalRow`): no deadline (retirement deferred) and its row re-keyed behind it;
/// * W `ReceiptLicensed` with a bystander's court open (its responder never moves): no deadline; then
///   voided `CourtDefault`: `voided + retirement`;
/// * P redrawn (`Provisional` mid its rebind window): `rebound + window_bind`, the redraw block's DAA
///   the rebound; then voided `BindTimeout`: `voided + retirement`;
/// * Q and H voided by their DA defaults, and V reversed by its `FinalRow` default:
///   `max(voided + retirement, last_closed + 1)`;
/// * R and S `Final` mid their retirement: `final + claim_retirement_daa`.
///
/// Every expected deadline is computed from DL-1's table and the run's own DAAs, never read off the
/// fold's index. **Not here:** DL-1's re-arm from the SHIFTED anchor after a seat's session is
/// ANSWERED (DA-5's pause credit) — an answer needs a real claim's material, which this harness's
/// junk attempts do not carry; the processor's real-claim harness runs it
/// (`t67_r_an_answer_at_the_deadline_credits_exactly_the_pause`, `t46_false_valid_real_claim.rs`).
///
/// Every tip restarts equal to the uninterrupted run; the run reverts to its base with every reverted
/// tip loading (released and held licences and open sessions among them), re-applies, and replays by
/// IBD from a genesis rebuilt from scratch.
#[test]
fn t40_dl1_restart_mid_session_and_mid_gate_on_every_phase_equals_the_uninterrupted_run() {
    let mut t = armed_tape();
    let bystander = bond_key(61);
    t.step(vec![bond_obj(61, 400_000 * MSK)]);
    let seats = t.c.floor_seats();
    let wr = t.c.sp.window_receipt();
    let mut marks: Vec<(usize, &'static str, Hash64, Option<u64>)> = Vec::new();
    // P, mid its receipt gate.
    let p_id = t.attempt(None, 0x4001);
    let p_bound = t.bind(p_id);
    // Q, a seat's session open on the bound claim.
    let q = t.attempt(None, 0x4002);
    t.bind(q);
    t.step(vec![da_accuse(q, seats[0].0, 0)]);
    marks.push((t.len(), "Q PanelBound, a seat's session open", q, None));
    marks.push((t.len(), "P PanelBound mid its receipt gate", p_id, Some(p_bound + wr)));
    // R (V1, basis 3), S (coverage, basis 2), H (held, its unserved seat accusing), V (bystander).
    let r = t.attempt(None, 0x4003);
    let bound = t.bind(r);
    t.step(vec![v1(r, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
    let r_licensed = t.c.daa;
    let s_id = t.attempt(None, 0x4004);
    let bound = t.bind(s_id);
    let anchor = t.c.anchor(&s_id);
    t.step(vec![PalwConsensusObjectV2::ReceiptLicensedV2 {
        claim: s_id,
        receipts: covered(s_id, anchor, &seats, &[0, 1, 2, 3, 4], bound),
    }]);
    let s_licensed = t.c.daa;
    assert_eq!(t.c.s.claim(&s_id).unwrap().rcore.basis_k, 2, "S: coverage recounts to 2");
    let hh = t.attempt(None, 0x4005);
    let bound = t.bind(hh);
    t.step(vec![v1(hh, &seats, &[0, 1, 2], &[3, 4], bound)]);
    t.step(vec![da_accuse(hh, seats[3].0, 0)]);
    let v = t.attempt(None, 0x4006);
    let bound = t.bind(v);
    t.step(vec![v1(v, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
    let v_licensed = t.c.daa;
    t.step(vec![da_accuse(v, bystander, 0)]);
    // W: licensed (V1, basis 3), then a court opened on it by the bystander, whose responder never
    // moves — DL-1's "ReceiptLicensed, open court: none".
    let w = t.attempt(None, 0x4007);
    let bound = t.bind(w);
    t.step(vec![v1(w, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
    t.step(vec![court_opened(&t.c.s, w, bystander)]);
    let court_deadline = t.c.s.court_sessions_iter().find(|(_, c)| c.claim == w).expect("the court is open").1.deadline_daa;
    marks.push((t.len(), "W ReceiptLicensed with a court open", w, None));
    let wc_r = t.c.sp.window_challenge_at(r_licensed);
    let wc_s = t.c.sp.window_challenge_at(s_licensed);
    marks.push((t.len(), "R ReceiptLicensed basis 3 mid its challenge gate", r, Some((r_licensed + wc_r).max(t.c.daa))));
    marks.push((t.len(), "S ReceiptLicensed basis 2 mid its challenge gate", s_id, Some((s_licensed + wc_s).max(t.c.daa))));
    marks.push((t.len(), "H held licence, its unserved seat's session open", hh, None));
    assert!(t.c.s.deadline_of(&v).is_some(), "a bystander's session does not pause V (V3S-08)");
    // R, S and V go Final (the short challenge window); V's bystander session outlives it (FinalRow).
    let finals = (r_licensed + wc_r).max(s_licensed + wc_s).max(v_licensed + t.c.sp.window_challenge_at(v_licensed)) + 1;
    assert!(finals < p_bound + wr, "the premise: the Finals land inside P's receipt gate");
    t.at(finals, vec![]);
    for id in [r, s_id, v] {
        assert!(matches!(t.c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id} Final");
    }
    let retire = |t: &Tape, id: &Hash64| match t.c.s.claim(id).unwrap().phase {
        PalwClaimPhaseV2::Final { final_daa } => final_daa + t.c.sp.claim_retirement_daa(),
        ref other => panic!("Final: {other:?}"),
    };
    marks.push((t.len(), "R Final mid its retirement", r, Some(retire(&t, &r))));
    marks.push((t.len(), "S Final mid its retirement", s_id, Some(retire(&t, &s_id))));
    assert!(t.c.s.da_session(&v, &bystander).is_some(), "V's session is open at FinalRow");
    marks.push((t.len(), "V Final with a session open (FinalRow): retirement deferred", v, None));
    assert!(t.c.s.vesting_row(&v).unwrap().expiry_daa > t.c.s.da_session(&v, &bystander).unwrap().deadline_daa, "V's row re-keyed");
    marks.push((t.len(), "P PanelBound mid its receipt gate, later", p_id, Some(p_bound + wr)));
    // P's receipt gate runs out: redrawn, mid its rebind window. DL-1: "Provisional after a redraw:
    // as today, from rebound_daa" — the redraw block's DAA plus `window_bind` (no DA session on P).
    let redraw = p_bound + wr + 1;
    t.at(redraw, vec![]);
    assert!(matches!(t.c.s.claim(&p_id).unwrap().phase, PalwClaimPhaseV2::Provisional), "RT#1 redraws P");
    marks.push((t.len(), "P redrawn, mid its rebind window", p_id, Some(redraw + t.c.sp.window_bind())));
    // Q and H default (each its own block); P's rebind window runs out between them or after.
    let q_default = t.c.s.da_session(&q, &seats[0].0).unwrap().deadline_daa + 1;
    let h_default = t.c.s.da_session(&hh, &seats[3].0).unwrap().deadline_daa + 1;
    for (id, at) in [(q, q_default), (hh, h_default)] {
        if at > t.c.daa {
            t.at(at, vec![]);
        }
        let claim = t.c.s.claim(&id).unwrap().clone();
        let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::ProducerWithholding } = claim.phase else {
            panic!("{id}: the DA default voids it: {:?}", claim.phase)
        };
        let closed = t.c.s.da_claim(&id).and_then(|record| record.last_closed_daa).unwrap_or(0);
        let want = (voided_daa + t.c.sp.claim_retirement_daa()).max(closed + 1);
        marks.push((t.len(), "a DA-defaulted claim mid its retirement", id, Some(want)));
    }
    // P's rebind window ran out on the way: voided BindTimeout (an attempt: no abandon hold), retiring.
    let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::BindTimeout } = t.c.s.claim(&p_id).unwrap().phase else {
        panic!("P voids BindTimeout: {:?}", t.c.s.claim(&p_id).unwrap().phase)
    };
    marks.push((t.len(), "P voided BindTimeout, mid its retirement", p_id, Some(voided_daa + t.c.sp.claim_retirement_daa())));
    let producer = floor_producer(&t.c.p).0;
    assert!(t.c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &q)).is_some(), "Q's DaDefault");
    // V's FinalRow session runs out: S3, the row burned, the Final reversed — voided and closed in
    // the default's block, so DL-1's terminal row is `max(v_default + retirement, v_default + 1)`.
    let v_default = t.c.s.da_session(&v, &bystander).unwrap().deadline_daa + 1;
    t.at(v_default, vec![]);
    assert!(t.c.s.vesting_row(&v).is_none(), "V's row burned (S3)");
    assert_eq!(t.c.s.da_claim(&v).and_then(|record| record.last_closed_daa), Some(v_default), "the session closed at the default");
    marks.push((
        t.len(),
        "V reversed by its FinalRow default, mid its retirement",
        v,
        Some((v_default + t.c.sp.claim_retirement_daa()).max(v_default + 1)),
    ));
    // W's court runs out on its responder's silence: `CourtDefault`, terminal — no DA record on W, so
    // DL-1's terminal row is `voided + retirement`.
    if court_deadline + 1 > t.c.daa {
        t.at(court_deadline + 1, vec![]);
    }
    let w_voided = (0..=t.len())
        .find_map(|j| match t.state_at(j).claim(&w).map(|c| c.phase.clone()) {
            Some(PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtDefault }) => Some((j, voided_daa)),
            _ => None,
        })
        .expect("W's court defaults on its responder's silence");
    assert!(t.c.s.da_claim(&w).is_none(), "no DA record on W");
    marks.push((w_voided.0, "W voided CourtDefault, mid its retirement", w, Some(w_voided.1 + t.c.sp.claim_retirement_daa())));
    // Every named tip: the loaded state answers DL-1's row.
    for (j, what, id, want) in &marks {
        let loaded = t.restart_at(*j);
        assert_eq!(deadline(&loaded, id), *want, "tip {j} ({what}): DL-1's deadline re-derived at load");
    }
    // Every tip restarts; the run reverts (each reverted tip loading), re-applies and replays by IBD.
    restart_everywhere(&t);
    t.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    println!("T40: {} blocks, {} named tips checked against DL-1, every tip restarted", t.len(), marks.len());
}

/// **T40 / DL-1: restart mid-abandon-hold.** A free-prompt commitment on the floor (the lane made
/// ready as `dos_repro_2` documents, before the run's base) is never bound: past `window_bind` it
/// voids `BindTimeout` and starts its abandon hold (SR-1: the reservation kept), DL-1's row
/// `voided + fp_abandon_hold_daa`; past the hold the reservation returns, then the claim retires.
/// A restart at every tip — mid-bind, mid-hold, after the release — loads and replays equal; the run
/// reverts to its base (each reverted tip loading) and re-applies, and replays from its base.
#[test]
fn t40_dl1_restart_mid_abandon_hold() {
    let mut c = Chain::new(t12());
    c.attribution = true;
    let collateral = at_least_the_floor(&c.p, 100_000 * MSK);
    let (mut t, leaves) = fp_floor_ready(c, 51, collateral);
    let bond = bond_key(51);
    let before = t.c.s.reserved_exposure(&bond);
    let (commit, id) = fp_commit_of(&t.c, 51, leaves, 0x40FF);
    t.step(vec![commit]);
    let reserved = t.c.s.reserved_exposure(&bond) - before;
    assert!(reserved > 0, "the commitment reserves");
    let accepted = t.c.daa;
    t.at(accepted + t.c.sp.window_bind() / 2, vec![]);
    let mid_bind = t.len();
    t.at(accepted + t.c.sp.window_bind() + 1, vec![]);
    let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::BindTimeout } = t.c.s.claim(&id).unwrap().phase else {
        panic!("the unbound commitment voids BindTimeout: {:?}", t.c.s.claim(&id).unwrap().phase)
    };
    assert_eq!(t.c.s.reserved_exposure(&bond) - before, reserved, "SR-1: the abandon hold keeps the reservation");
    let hold = voided_daa + t.c.sp.fp_abandon_hold_daa();
    t.at(voided_daa + t.c.sp.fp_abandon_hold_daa() / 2, vec![]);
    let mid_hold = t.len();
    let loaded = t.restart_at(mid_hold);
    assert_eq!(loaded.deadline_of(&id), Some(hold), "DL-1: the abandon hold's row, re-derived at load");
    assert_eq!(loaded.reserved_exposure(&bond) - before, reserved, "the held reservation re-derived at load");
    t.at(hold + 1, vec![]);
    assert_eq!(t.c.s.reserved_exposure(&bond), before, "past the hold the reservation returns");
    let retire = t.c.s.deadline_of(&id).expect("the retirement");
    t.at(retire + 1, vec![]);
    assert!(t.c.s.claim(&id).is_none(), "the claim retires");
    let _ = mid_bind;
    restart_everywhere(&t);
    t.revert_to_base_and_reapply();
    t.ibd_from(t.base.clone());
    println!("T40 abandon hold: void at {voided_daa}, hold to {hold}, {} blocks restarted everywhere", t.len());
}

/// **T83: restart mid-DA-session, mid-reveal window and mid-backlog — the roots equal an
/// uninterrupted replay.** One chain passes through:
///
/// * a seat's DA session open on a bound claim (mid-session);
/// * a reporter's commitment landed on an equivocation, the conviction consumed and its reward
///   pending, the reveal not yet made (mid-reveal window), and then the reveal made with the window
///   still open;
/// * nine DA defaults whose awards close together: step 3d moves eight and one waits in
///   `reporter_rewards` (a reward queue longer than one block's budget);
/// * two batches of thirty-one floor claims Final one after the other, so the first batch's rows
///   latch together past their DAA clock and the second clock's depth: a maturity backlog 3d drains
///   by its budget of eight new queue keys a block (a row's producer key and its five seats' keys the
///   first time, the producer key alone for a row whose seats' keys the block already created).
///
/// At each named tip — every backlog tip among them — the carriage restarts under its root and the
/// rest of the run folded on the loaded state equals the uninterrupted run, block by block; the run
/// also reverts to its base and replays by IBD.
#[test]
fn t83_restart_mid_session_mid_reveal_window_and_mid_backlog() {
    let mut t = armed_tape();
    let reporter = bond_key(71);
    t.step(vec![bond_obj(71, 400_000 * MSK)]);
    let seats = t.c.floor_seats();
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    let (producer, _, _) = floor_producer(&t.c.p);
    let mut marks: Vec<(usize, &'static str)> = Vec::new();
    // Mid-session.
    let a = t.attempt(None, 0x8301);
    t.bind(a);
    t.step(vec![da_accuse(a, seats[0].0, 0)]);
    assert!(t.c.s.da_session(&a, &seats[0].0).is_some());
    marks.push((t.len(), "mid DA session"));
    // Mid-reveal window: the commitment landed, the conviction consumed, the reveal not yet.
    let accused = seats[2].0;
    let (eq, evidence) = equivocation_of(accused, floor, 0x8302);
    let key = equivocation_key(accused, evidence);
    t.step(vec![reporter_commit(key, evidence, reporter)]);
    t.step(vec![eq]);
    let pending = *t.c.s.reward_pending(&key).expect("the reward is pending");
    assert!(pending.best.is_none() && t.c.s.reporter_open_commitments(&reporter) == 1, "committed, not revealed");
    marks.push((t.len(), "mid reveal window: committed, not yet revealed"));
    t.step(vec![reporter_reveal(key, reporter)]);
    assert!(t.c.s.reward_pending(&key).unwrap().best.is_some() && t.c.daa <= pending.reveal_until);
    marks.push((t.len(), "mid reveal window: revealed, window open"));
    // The reward backlog: nine seat-accused claims default together; their awards close together.
    let batch: Vec<Hash64> = (0..9u64)
        .map(|k| {
            let id = t.attempt(None, 0x8310 + k);
            t.bind(id);
            id
        })
        .collect();
    t.step(batch.iter().map(|id| da_accuse(*id, seats[1].0, 0)).collect());
    let deadline = t.c.s.da_session(&batch[0], &seats[1].0).unwrap().deadline_daa;
    t.at(deadline + 1, vec![]);
    marks.push((t.len(), "nine sweep-time rewards pending"));
    let reveal_until = t.c.s.reward_pending(&palw_da_offence_id_v1(&producer.0, &batch[0])).unwrap().reveal_until;
    t.at(reveal_until + 1, vec![]);
    // Ten awards close here (the nine, and `a`'s, whose session ran out in the same default block):
    // 3d's budget of eight new keys moves eight.
    assert_eq!(t.c.s.reporter_rewards_iter().count(), 2, "awards wait past 3d's budget");
    marks.push((t.len(), "mid reward backlog: an award waiting in reporter_rewards"));
    t.step(vec![]);
    assert_eq!(t.c.s.reporter_rewards_iter().count(), 0, "drained");
    // The maturity backlog.
    let mut batch_final = Vec::new();
    for b in 0..2u64 {
        for k in 0..31u64 {
            let id = t.attempt(None, 0x8400 + (b << 6) + k);
            let bound = t.bind(id);
            t.step(vec![v1(id, &seats, &[0, 1, 2, 3, 4], &[], bound)]);
        }
        t.at(t.c.daa + t.c.sp.window_challenge() + 1, vec![]);
        batch_final.push(t.c.daa);
    }
    t.at(batch_final[0] + t.c.sp.window_court() + 1, vec![]);
    let latched_unmoved = |s: &PalwChainStateV2| s.vesting_iter_by_expiry().filter(|row| row.matured_at.is_some()).count();
    assert!(latched_unmoved(&t.c.s) >= 20, "the first batch latched together and waits: {}", latched_unmoved(&t.c.s));
    marks.push((t.len(), "mid maturity backlog: latched rows waiting"));
    let mut backlog_tips = 0;
    let mut drain = vec![latched_unmoved(&t.c.s)];
    for _ in 0..6 {
        t.step(vec![]);
        let waiting = latched_unmoved(&t.c.s);
        assert!(waiting > 0 && waiting < *drain.last().unwrap(), "3d moves what its budget allows, and a backlog remains");
        drain.push(waiting);
        marks.push((t.len(), "mid maturity backlog: draining"));
        backlog_tips += 1;
    }
    assert!(drain.last() < drain.first(), "the backlog drains: {drain:?}");
    println!("T83: latched rows waiting, block by block: {drain:?}");
    for (j, what) in &marks {
        t.restart_at(*j);
        println!("T83: restarted at tip {j} ({what})");
    }
    t.revert_to_base_and_reapply();
    t.ibd_from(scratch_genesis());
    println!("T83: {} blocks; {} named tips ({backlog_tips} inside the maturity backlog) restart equal", t.len(), marks.len());
}
