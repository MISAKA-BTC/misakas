//! **ADR-0152 v3.1 M5 (§7.1, §8.3 item 1): the reorg half — T07's F17 twin, T75's reorg twin and
//! V3S-03 on a real DA default, on testnet-12's own fold.**
//!
//! Every block is folded through `apply_palw_transition_v7` with the extras the processor resolves
//! (`palw_offence_attribution` armed as on testnet-12) and recorded on a [`Tape`]: its delta
//! re-applies and reverts, its carriage reloads under its root, and the whole run then reverts block
//! by block to its base and re-applies (every tip on the way a reorg target), and a fork of it is
//! reorged to and back.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_m5_reorg

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
use kaspa_consensus_core::palw_state_v2::{PalwPayoutV2, PalwStateV2Error, palw_reporter_reward_amount_v1};
use kaspa_consensus_core::palw_vesting_v1::{
    PalwVestingNoteV1, PalwVestingSourceV1, palw_reporter_payout_key_v1, palw_vesting_notes_of_delta_v1,
};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

/// Bond `n`'s payout payload (`bond_obj`'s).
fn payload(n: u64) -> Hash64 {
    h(0x9A00 + n)
}

/// The award for `key` wherever the chain holds it after the block: in `reporter_rewards` from step
/// 2's sweep until step 3d moves it, then on its A-KEY key in `pending_payouts`.
fn awarded(s: &PalwChainStateV2, key: &Hash64) -> Option<PalwPayoutV2> {
    s.reporter_reward(key).copied().or_else(|| s.pending_payout(&palw_reporter_payout_key_v1(key)).copied())
}

/// The payloads of every step-3d `Moved` note a run of blocks journaled for the reporter award `key`.
fn moved_notes(blocks: &[TapeBlock], key: &Hash64) -> Vec<Hash64> {
    blocks
        .iter()
        .flat_map(|b| palw_vesting_notes_of_delta_v1(&b.delta).cloned().collect::<Vec<_>>())
        .filter_map(|note| match note {
            PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Reporter { offence_id }, legs } if offence_id == *key => {
                Some(legs.iter().map(|leg| leg.payload).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

/// Every `ReporterAwarded` note a run of blocks journaled for `key`: `(reporter, payload, sompi)`.
fn award_notes(blocks: &[TapeBlock], key: &Hash64) -> Vec<(PalwBondKeyV2, Hash64, u64)> {
    blocks
        .iter()
        .flat_map(|b| palw_vesting_notes_of_delta_v1(&b.delta).cloned().collect::<Vec<_>>())
        .filter_map(|note| match note {
            PalwVestingNoteV1::ReporterAwarded { offence_id, reporter, payload, sompi } if offence_id == *key => {
                Some((reporter, payload, sompi))
            }
            _ => None,
        })
        .collect()
}

/// Does `payload` appear in any reward place of `s` (the rewards table or the payout queue)?
fn pays(s: &PalwChainStateV2, payload: Hash64) -> bool {
    s.reporter_rewards_iter().any(|(_, p)| p.payload == payload) || s.pending_payouts_iter().any(|(_, p)| p.payload == payload)
}

/// A testnet-12 chain with `palw_offence_attribution` armed as the processor resolves it, on a tape.
fn armed_tape() -> Tape {
    let mut c = Chain::new(t12());
    c.attribution = true;
    Tape::new(c)
}

/// **T07's F17 twin: two copies of one conviction carrying different reporters on two branches.**
///
/// A genesis seat equivocates; the same `ExecutorEquivocation` object (one evidence, one ledger key)
/// is folded on two branches of one chain. Two shapes:
///
/// 1. forked BEFORE the commitments: branch A carries reporter 21's commitment, the conviction, its
///    reveal and the sweep; branch B the same with reporter 22;
/// 2. forked AFTER both commitments and the conviction: both branches reveal 22 (the later
///    commitment) first; branch A then reveals 21, whose earlier commitment takes the reward over, and
///    branch B does not.
///
/// On each branch's tip the award for the key is exactly one payout — its own reporter's payload at
/// `⌊r × collected⌋`, `awarded_sompi` grown by it once — with exactly one 3d `Moved` note for the
/// award on the branch, and the other reporter's payload is nowhere in the rewards table or the
/// payout queue.
/// A reorg from A to B (A's blocks reverted, B's applied, every state the recorded one) moves the
/// reward with the branch, and back.
#[test]
fn t07_f17_two_copies_of_one_conviction_pay_each_branchs_own_reporter() {
    let mut t = armed_tape();
    t.step(vec![bond_obj(21, 50_000 * MSK), bond_obj(22, 50_000 * MSK)]);
    let (r21, r22) = (bond_key(21), bond_key(22));
    let accused = t.c.floor_seats()[1].0;
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    let (eq, evidence) = equivocation_of(accused, floor, 0xF17);
    let key = equivocation_key(accused, evidence);

    // The branch tail both shapes share: the reveals, then the sweep past the window.
    let finish = |x: &mut Tape, reveals: &[PalwBondKeyV2]| -> u64 {
        let pending = *x.c.s.reward_pending(&key).expect("the conviction opened a commit–reveal reward");
        assert!(pending.accepts_reveals() && pending.evidence_id == evidence, "a kind-0 reward takes reveals on its evidence");
        for r in reveals {
            x.step(vec![reporter_reveal(key, *r)]);
        }
        x.at(pending.reveal_until + 1, vec![]);
        pending.amount
    };
    let check = |x: &Tape, winner: u64, loser: u64, amount: u64, what: &str| {
        let tip = &x.c.s;
        assert!(tip.reward_pending(&key).is_none(), "{what}: the window closed");
        assert_eq!(awarded(tip, &key), Some(PalwPayoutV2 { payload: payload(winner), amount }), "{what}: its own reporter, once");
        assert!(!pays(tip, payload(loser)), "{what}: the other branch's reporter is paid nothing here");
        assert_eq!(tip.reporter_counters().awarded_sompi, u128::from(amount), "{what}: awarded once, never twice");
        assert_eq!(moved_notes(&x.blocks, &key), vec![payload(winner)], "{what}: one 3d move of the award, to its own reporter");
        x.revert_to_base_and_reapply();
    };

    // Shape 1: forked before the commitments.
    let fork = t.len();
    let branch = |reporter: PalwBondKeyV2| -> (Tape, u64) {
        let mut x = t.fork(fork);
        x.step(vec![reporter_commit(key, evidence, reporter)]);
        let before = x.c.s.bond(&accused).unwrap().collateral;
        x.step(vec![eq.clone()]);
        let record = x.c.s.consumed_offence(&key).expect("the kind-0 record").clone();
        assert_eq!(
            u64::try_from(before - x.c.s.bond(&accused).unwrap().collateral).unwrap(),
            record.collected,
            "collected is the debit"
        );
        let amount = finish(&mut x, &[reporter]);
        assert_eq!(amount, palw_reporter_reward_amount_v1(record.collected, 0), "R-1: ⌊r × collected⌋");
        (x, amount)
    };
    let (a, amount_a) = branch(r21);
    let (b, amount_b) = branch(r22);
    assert_eq!(amount_a, amount_b, "one conviction, one reward");
    check(&a, 21, 22, amount_a, "shape 1, branch A");
    check(&b, 22, 21, amount_b, "shape 1, branch B");
    assert_ne!(a.c.s.state_root(), b.c.s.state_root(), "two copies, two winners");
    a.reorg_to(0, &b);
    b.reorg_to(0, &a);

    // Shape 2: both commitments and the conviction on the trunk; the branches differ in their reveals.
    t.step(vec![reporter_commit(key, evidence, r21)]);
    t.step(vec![reporter_commit(key, evidence, r22)]);
    t.step(vec![eq.clone()]);
    let fork = t.len();
    let mut a = t.fork(fork);
    let amount = finish(&mut a, &[r22, r21]);
    let mut b = t.fork(fork);
    assert_eq!(finish(&mut b, &[r22]), amount);
    check(&a, 21, 22, amount, "shape 2, branch A (the earlier commitment revealed second)");
    check(&b, 22, 21, amount, "shape 2, branch B (only the later one revealed)");
    a.reorg_to(0, &b);
    b.reorg_to(0, &a);
    t.revert_to_base_and_reapply();
}

/// Floor claims by the genesis producer, each bound to the floor seats in its own block.
fn bound_claims(t: &mut Tape, seeds: &[u64]) -> Vec<Hash64> {
    seeds
        .iter()
        .map(|seed| {
            let id = t.attempt(None, *seed);
            t.bind(id);
            id
        })
        .collect()
}

/// **T75's reorg twin on the real funnel: step 2's sweep → `reporter_rewards` → step 3d, with
/// sweep-time convictions entering `reward_pending` at step 2.**
///
/// Wave 1: nine bound floor claims are accused in one block by a seat of their panel (a seat's
/// session pauses the claim, DA-5); the sessions run out together, and in the next block's sweep
/// (step 2) each DA default — a conviction made BY the sweep — opens its named reward (the accuser,
/// no reveal) in `reward_pending`, never straight into `reporter_rewards`. Wave 2: five more claims,
/// bound and accused so that their sessions run out in the very block that closes wave 1's reveal
/// window. In that block step 2 moves wave 1's nine awards into `reporter_rewards` and opens wave 2's
/// five in `reward_pending`; step 3d then moves what its budget of new queue keys allows (8), and one
/// wave-1 award waits in `reporter_rewards` for the next block's 3d. Every award is `⌊r ×
/// collected⌋` of its producer's debit and pays the accuser once. The reorg twins: the whole run
/// reverts block by block to its base and re-applies, and a sibling branch forked just before the
/// default block, and one forked just before the sweep block, are each reorged to and back.
#[test]
fn t75_reorg_twin_sweep_to_reporter_rewards_to_3d_with_sweep_time_convictions() {
    let mut t = armed_tape();
    let accuser = t.c.floor_seats()[0].0;
    let accuser_payload = t.c.s.bond(&accuser).unwrap().payout_payload;
    let (producer, _, _) = floor_producer(&t.c.p);
    let key = |id: &Hash64| palw_da_offence_id_v1(&producer.0, id);
    let wave1 = bound_claims(&mut t, &(0x7501..0x750A).collect::<Vec<_>>());
    t.step(wave1.iter().map(|id| da_accuse(*id, accuser, 0)).collect());
    let opened1 = t.c.daa;
    let deadline1 = t.c.s.da_session(&wave1[0], &accuser).expect("a session").deadline_daa;
    assert!(wave1.iter().all(|id| t.c.s.da_session(id, &accuser).map(|s| s.deadline_daa) == Some(deadline1)), "one deadline");
    assert!(wave1.iter().all(|id| t.c.s.deadline_of(id).is_none()), "a seat's session pauses each claim (DA-5, DL-1)");
    let session_len = deadline1 - opened1;
    // The default block: wave 1 runs out at deadline + 1. Learn the reveal window's end from a probe.
    let x1 = deadline1 + 1;
    let probe = t.probe(x1, &[], None, 0).expect("the default's block folds").0;
    let r1 = probe.reward_pending(&key(&wave1[0])).expect("the sweep-time conviction's reward").reveal_until;
    let x2 = r1 + 1;
    // Wave 2: bound just before it is accused, so every claim is live when its session opens.
    let open2 = x2 - 1 - session_len;
    assert!(open2 > t.c.daa + 12 && open2 < x1, "the premise: wave 2 opens between wave 1's accusation and its default");
    t.at(open2 - 11, vec![]);
    let wave2 = bound_claims(&mut t, &(0x7511..0x7516).collect::<Vec<_>>());
    t.at(open2, wave2.iter().map(|id| da_accuse(*id, accuser, 0)).collect());
    assert!(wave2.iter().all(|id| t.c.s.da_session(id, &accuser).map(|s| s.deadline_daa + 1) == Some(x2)), "wave 2 runs out at x2");
    let collateral_before = t.c.s.bond(&producer).unwrap().collateral;
    t.at(x1, vec![]);
    let default_block = t.len();
    for id in &wave1 {
        let pending = *t.c.s.reward_pending(&key(id)).expect("each sweep-time conviction enters reward_pending at step 2");
        let record = t.c.s.consumed_offence(&key(id)).expect("the DaDefault record").clone();
        assert_eq!(pending.amount, palw_reporter_reward_amount_v1(record.collected, 0), "⌊r × collected⌋ on the producer's debit");
        assert!(!pending.accepts_reveals() && pending.best.map(|w| w.reporter) == Some(accuser), "named: the accuser");
        assert!(awarded(&t.c.s, &key(id)).is_none(), "not awarded before its window closes");
    }
    assert!(collateral_before > t.c.s.bond(&producer).unwrap().collateral, "the producer paid");
    // The window's last block: nothing moves.
    t.at(r1, vec![]);
    assert!(wave1.iter().all(|id| t.c.s.reward_pending(&key(id)).is_some()), "the window is inclusive");
    // x2: step 2 sweeps wave 1 into reporter_rewards and opens wave 2 in reward_pending; 3d moves 8.
    let payout = |id: &Hash64, s: &PalwChainStateV2| s.pending_payout(&palw_reporter_payout_key_v1(&key(id))).copied();
    t.at(x2, vec![]);
    let sweep_block = t.len();
    assert!(wave1.iter().all(|id| t.c.s.reward_pending(&key(id)).is_none()), "wave 1's windows closed at step 2");
    let in_table: Vec<Hash64> = wave1.iter().copied().filter(|id| t.c.s.reporter_reward(&key(id)).is_some()).collect();
    let moved: Vec<Hash64> = wave1.iter().copied().filter(|id| payout(id, &t.c.s).is_some()).collect();
    assert_eq!((in_table.len(), moved.len()), (1, 8), "3d moves 8 new keys; one award waits in reporter_rewards");
    for id in &wave2 {
        let pending = t.c.s.reward_pending(&key(id)).expect("wave 2's sweep-time convictions enter reward_pending at step 2");
        assert!(pending.reveal_until > x2 && awarded(&t.c.s, &key(id)).is_none(), "in their own window, not awarded");
    }
    let waiting = in_table[0];
    let amount = t.c.s.reporter_reward(&key(&waiting)).unwrap().amount;
    t.step(vec![]);
    assert_eq!(payout(&waiting, &t.c.s), Some(PalwPayoutV2 { payload: accuser_payload, amount }), "the next block's 3d moves it");
    assert!(t.c.s.reporter_rewards_iter().next().is_none(), "the table drained");
    let total: u64 =
        wave1.iter().map(|id| t.c.s.consumed_offence(&key(id)).map(|r| palw_reporter_reward_amount_v1(r.collected, 0)).unwrap()).sum();
    assert_eq!(t.c.s.reporter_counters().awarded_sompi, u128::from(total), "Σ awards, each once");
    for id in &wave1 {
        assert_eq!(moved_notes(&t.blocks, &key(id)), vec![accuser_payload], "one 3d move per award, to the accuser");
    }
    println!(
        "T75 twin: session {session_len} DAA, reveal window to {r1}; the sweep at {x2} moved {} and left {}; awarded {:.4} MSK",
        moved.len(),
        in_table.len(),
        total as f64 / MSK as f64
    );
    // The reorg twins.
    t.revert_to_base_and_reapply();
    for j in [default_block - 1, sweep_block - 1] {
        let mut sibling = t.fork(j);
        bound_claims(&mut sibling, &[0x75FF]);
        t.reorg_to(j, &sibling);
    }
}

/// **V3S-03 on the real DA court: a speculative commitment to a claim's DA key at admission is
/// refused and cannot win the DA default.** A speculator commits to `palw_da_offence_id_v1(producer,
/// claim)` — public from the claim's admission — in the claim's first block after acceptance, with
/// the zero evidence and with a guessed one; a seat accuses, the session runs out, the DA default
/// opens its named reward (the accuser, no reveal). Every reveal by the speculator is refused by
/// name, the award goes to the accuser, and the speculator's payload is paid nothing. The run's reorg
/// twin: reverted to its base and re-applied, and a sibling where the speculator never committed
/// reaches the same award.
#[test]
fn v3s03_a_speculative_commitment_to_a_da_key_cannot_win_the_da_default() {
    let mut t = armed_tape();
    let (accuser, speculator) = (t.c.floor_seats()[2].0, bond_key(42));
    let accuser_payload = t.c.s.bond(&accuser).unwrap().payout_payload;
    t.step(vec![bond_obj(42, 50_000 * MSK)]);
    let (producer, _, _) = floor_producer(&t.c.p);
    let fork = t.len();
    let run = |t: &mut Tape, speculate: bool| -> Hash64 {
        let id = t.attempt(None, 0x3503);
        let key = palw_da_offence_id_v1(&producer.0, &id);
        if speculate {
            t.step(vec![reporter_commit(key, Hash64::default(), speculator), reporter_commit(key, h(0xE71D), speculator)]);
            assert!(
                t.c.s.reporter_open_commitments(&speculator) == 2,
                "the commitments themselves are admitted (they hide their key)"
            );
        }
        t.bind(id);
        t.step(vec![da_accuse(id, accuser, 0)]);
        let deadline = t.c.s.da_session(&id, &accuser).unwrap().deadline_daa;
        t.at(deadline + 1, vec![]);
        let pending = *t.c.s.reward_pending(&key).expect("the DA default's reward");
        assert!(!pending.accepts_reveals(), "a DA default takes no reveal");
        assert_eq!(pending.best.map(|w| w.reporter), Some(accuser), "the accuser of the earliest defaulted session, named");
        if speculate {
            let reveal = reporter_reveal(key, speculator);
            match t.probe(t.c.daa + 1, std::slice::from_ref(&reveal), None, 0) {
                Err(PalwStateV2Error::ReporterRevealRefused { why, .. }) => assert!(why.contains("takes no reveal"), "{why}"),
                other => panic!("the speculator's reveal must be refused: {:?}", other.map(|_| ())),
            }
        }
        t.at(pending.reveal_until + 1, vec![]);
        assert_eq!(
            awarded(&t.c.s, &key),
            Some(PalwPayoutV2 { payload: accuser_payload, amount: pending.amount }),
            "the accuser is paid"
        );
        assert!(!pays(&t.c.s, payload(42)), "the speculator is paid nothing");
        key
    };
    let mut spec = t.fork(fork);
    let key = run(&mut spec, true);
    let mut clean = t.fork(fork);
    assert_eq!(run(&mut clean, false), key);
    assert_eq!(awarded(&spec.c.s, &key), awarded(&clean.c.s, &key), "the speculation changed nothing about the award");
    spec.revert_to_base_and_reapply();
    spec.reorg_to(0, &clean);
}

/// **BUG (reproduction): the award is journaled with no `ReporterAwarded` note.** `PalwVestingNoteV1`
/// declares `ReporterAwarded { offence_id, reporter, payload, sompi }` (tag 4, phase2-plan §2.5), and
/// the vesting read (`PalwReporterRewardReadV1::reporter`: "`None` once awarded … the
/// `ReporterAwarded` note names it") and the leg builder (`palw_vesting_v1.rs`: "the bond is named by
/// S-7's `ReporterAwarded` note") rely on it — but `sweep_reward_reveals` (`palw_state_v2.rs`, the
/// `write_reporter_reward` call) writes the award without it. Once the award leaves `reward_pending`
/// (whose `best` held the bond), the winning reporter BOND is recorded nowhere: `reporter_rewards` and
/// 3d's `Moved` note carry only the payload. No root moves (notes are journal-only), but every Phase 2
/// consumer of the notes (the vesting notification, the per-payee index, the economics ledger) is blind
/// to which bond was paid. One commit–reveal award on testnet-12's fold: the sweep block's delta holds
/// no `ReporterAwarded` for the key.
#[test]
#[ignore = "BUG: sweep_reward_reveals awards without journaling PalwVestingNoteV1::ReporterAwarded (the reporter bond is lost)"]
fn bug_the_reporter_award_journals_a_reporter_awarded_note() {
    let mut t = armed_tape();
    t.step(vec![bond_obj(21, 50_000 * MSK)]);
    let accused = t.c.floor_seats()[1].0;
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    let (eq, evidence) = equivocation_of(accused, floor, 0xB06);
    let key = equivocation_key(accused, evidence);
    t.step(vec![reporter_commit(key, evidence, bond_key(21))]);
    t.step(vec![eq]);
    let pending = *t.c.s.reward_pending(&key).expect("a reward");
    t.step(vec![reporter_reveal(key, bond_key(21))]);
    t.at(pending.reveal_until + 1, vec![]);
    assert_eq!(awarded(&t.c.s, &key).map(|p| p.payload), Some(payload(21)), "the award itself is right");
    assert_eq!(
        award_notes(&t.blocks, &key),
        vec![(bond_key(21), payload(21), pending.amount)],
        "the sweep that awards must journal ReporterAwarded naming the reporter bond"
    );
}

// ---------------------------------------------------------------------------------------------
// T07: the reorg fuzz.
// ---------------------------------------------------------------------------------------------

/// What one Final claim's vesting row was when the chain wrote it — the oracle's own copy, kept after
/// the row moves or burns, so a row that leaves early is still counted against its producer.
#[derive(Clone, Debug)]
struct RowSeen {
    producer: PalwBondKeyV2,
    g_res: u128,
    total: u128,
    expiry_daa: u64,
    settled_at_final: u64,
    basis_k: u8,
}

/// The T07 oracle, independent of the fold's own latch: which Final claims are still inside their
/// conviction window, and what each bond could extract and the chain could recover from it there.
#[derive(Default)]
struct T07Oracle {
    rows: std::collections::BTreeMap<Hash64, RowSeen>,
    /// Claims whose row a conviction burned: the recovery happened, nothing is extractable any more.
    convicted: std::collections::BTreeSet<Hash64>,
    /// Claims whose window has closed — at a tip where the oracle read it closed, or where the fold
    /// latched the row. Monotone: a latched row is mature for good (X29), a halt after it reopens
    /// nothing.
    closed: std::collections::BTreeSet<Hash64>,
    worst_margin: Option<(i128, String)>,
    checks: usize,
}

/// Coverage counters over one seed: what the run actually drove.
#[derive(Default, Debug, Clone, Copy)]
struct T07Coverage {
    blocks: usize,
    refused: usize,
    finals: usize,
    da_defaults: usize,
    equivocations: usize,
    court_defaults: usize,
    receipt_timeouts: usize,
    commits: usize,
    reveals: usize,
    courts: usize,
    awards_moved: usize,
    forgone: bool,
    latched: usize,
    rows_moved: usize,
    rows_burned: usize,
    licences_released: usize,
    licences_held: usize,
    sibling_reorgs: usize,
    restarts: usize,
}

impl T07Oracle {
    /// Is the row's conviction window still open at `now` (ADR-0152 V-4 read independently of the
    /// row's latch): its DAA clock has not run, or the second clock still holds it at the escaped
    /// depth, or the chain is in a licence halt.
    fn window_open(c: &Chain, s: &PalwChainStateV2, row: &RowSeen, now: u64) -> bool {
        let wc = c.sp.window_court();
        let raw = c.extras_at(now).settled_anchor_depth;
        let escaped = kaspa_consensus_core::palw_state_v2::palw_second_clock_depth_v1(raw, s.recent_anchor_daas(), now, wc);
        let halted = raw.is_some() && escaped.is_none();
        now < row.expiry_daa
            || halted
            || kaspa_consensus_core::palw_panel_var_v1::palw_second_clock_holds_v1(
                escaped,
                s.settled_attempt_finals(),
                row.settled_at_final,
                row.expiry_daa,
                now,
                wc,
            )
    }

    /// One committed block: the structural checks on its transition, then the invariant at its tip.
    fn observe(&mut self, c: &Chain, parent: &PalwChainStateV2, b: &TapeBlock, cov: &mut T07Coverage) {
        use kaspa_consensus_core::palw_state_v2::{palw_claim_g_v1, palw_reporter_reward_extracted_v1};
        let s = &b.state;
        let now = b.daa;
        kaspa_consensus_core::palw_vesting_v1::palw_vesting_consistency_v1(s).unwrap_or_else(|e| panic!("DAA {now}: V-3: {e}"));
        let notes: Vec<PalwVestingNoteV1> = palw_vesting_notes_of_delta_v1(&b.delta).cloned().collect();
        let latched_here: std::collections::BTreeSet<Hash64> = notes
            .iter()
            .filter_map(|n| if let PalwVestingNoteV1::Latched { claim_id, .. } = n { Some(*claim_id) } else { None })
            .collect();
        // V-2: every claim that went Final in this block has its row (or a conviction burned it here).
        for (id, claim) in s.claims_iter() {
            let was_final = parent.claim(id).is_some_and(|p| matches!(p.phase, PalwClaimPhaseV2::Final { .. }));
            if matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) && !was_final {
                cov.finals += 1;
                let row = s.vesting_row(id).unwrap_or_else(|| panic!("DAA {now}: claim {id} went Final with no vesting row (V-2)"));
                let g = palw_claim_g_v1(s, id).expect("a Final claim's G");
                assert_eq!(g.g_res, claim.rcore.g_res_sompi, "DAA {now}: the row's G_res is the one the licence froze");
                self.rows.insert(
                    *id,
                    RowSeen {
                        producer: row.producer_bond,
                        g_res: g.g_res,
                        total: row.total_sompi_u128(),
                        expiry_daa: row.expiry_daa,
                        settled_at_final: row.settled_at_final,
                        basis_k: row.basis_k,
                    },
                );
            }
        }
        for note in &notes {
            match note {
                PalwVestingNoteV1::Latched { claim_id, .. } => {
                    cov.latched += 1;
                    let seen = self.rows.get(claim_id).expect("a latched row was seen at its Final").clone();
                    assert!(!Self::window_open(c, s, &seen, now), "DAA {now}: row {claim_id} latched inside its conviction window");
                    self.closed.insert(*claim_id);
                }
                PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { claim_id }, .. } => {
                    cov.rows_moved += 1;
                    let latched =
                        parent.vesting_row(claim_id).is_some_and(|r| r.matured_at.is_some()) || latched_here.contains(claim_id);
                    assert!(latched, "DAA {now}: row {claim_id} moved before it was latched");
                }
                PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Reporter { .. }, .. } => cov.awards_moved += 1,
                PalwVestingNoteV1::Burned { claim_id, .. } => {
                    cov.rows_burned += 1;
                    self.convicted.insert(*claim_id);
                }
                _ => {}
            }
        }
        // Every row that left in this block left by a note: a move of a latched row, or a burn.
        for row in parent.vesting_iter_by_expiry() {
            if s.vesting_row(&row.claim_id).is_none() {
                let noted = notes.iter().any(|n| match n {
                    PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { claim_id }, .. } => *claim_id == row.claim_id,
                    PalwVestingNoteV1::Burned { claim_id, .. } => *claim_id == row.claim_id,
                    _ => false,
                });
                assert!(noted, "DAA {now}: row {} vanished without a Moved or Burned note", row.claim_id);
            }
        }
        // The invariant, per bond: Σ recoverable (net of R) ≥ Σ extractable over the bond's Final
        // claims whose conviction window is open.
        let mut extractable: std::collections::BTreeMap<PalwBondKeyV2, u128> = Default::default();
        let mut recoverable: std::collections::BTreeMap<PalwBondKeyV2, u128> = Default::default();
        let wc = c.sp.window_court();
        let raw = c.extras_at(now).settled_anchor_depth;
        let escaped = kaspa_consensus_core::palw_state_v2::palw_second_clock_depth_v1(raw, s.recent_anchor_daas(), now, wc);
        let rows: Vec<(Hash64, RowSeen)> = self.rows.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (id, seen) in &rows {
            if self.convicted.contains(id) || self.closed.contains(id) {
                continue;
            }
            if !Self::window_open(c, s, seen, now) {
                self.closed.insert(*id);
                continue;
            }
            // Extractable: G_res from the Final on; the escrow too if the row has already left.
            let row = s.vesting_row(id);
            let ext = seen.g_res + if row.is_none() { seen.total } else { 0 };
            // Recoverable: the row (a burn takes it whole, never rewarded) and every live lock on the
            // claim, each capped by its seat's collateral, less R-1's reward on the lock's debit.
            let mut rec = row.map(|r| r.total_sompi_u128()).unwrap_or(0);
            if let Some(liability) = s.panel_liability(id) {
                for (outpoint, _) in &liability.valid_signers {
                    let seat = PalwBondKeyV2(*outpoint);
                    let Some(lock) = s.slashable_lock(seat, *id) else { continue };
                    if !lock.is_live_v3(now, s.settled_attempt_finals(), escaped, wc) {
                        continue;
                    }
                    let collected = lock.amount.min(u128::from(s.bond(&seat).map(|b| b.collateral).unwrap_or(0)));
                    let x = palw_reporter_reward_extracted_v1(lock.amount, seen.g_res, seen.basis_k);
                    let r = palw_reporter_reward_amount_v1(u64::try_from(collected).unwrap_or(u64::MAX), x);
                    rec += collected - u128::from(r).min(collected);
                }
            }
            *extractable.entry(seen.producer).or_default() += ext;
            *recoverable.entry(seen.producer).or_default() += rec;
        }
        for (bond, ext) in &extractable {
            let rec = recoverable.get(bond).copied().unwrap_or(0);
            let margin = rec as i128 - *ext as i128;
            if self.worst_margin.as_ref().is_none_or(|(m, _)| margin < *m) {
                self.worst_margin =
                    Some((margin, format!("DAA {now}, bond {:?}: recoverable {rec} vs extractable {ext}", bond.0.transaction_id)));
            }
            assert!(rec >= *ext, "DAA {now}: bond {bond:?} Σ recoverable (net of R) {rec} < Σ extractable {ext}");
            self.checks += 1;
        }
    }
}

/// One step of the fuzz: an action drawn from the state (see [`t07_run`]).
#[allow(clippy::too_many_arguments)]
fn t07_act(
    t: &mut Tape,
    rng: &mut Rng,
    seq: &mut u64,
    eqs: &mut Vec<(PalwBondKeyV2, u64, Hash64, Vec<PalwBondKeyV2>, bool)>,
    cov: &mut T07Coverage,
) -> bool {
    use kaspa_consensus_core::palw_state_v2::palw_da_event_index_v1;
    let now = t.c.daa;
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    let seats = t.c.floor_seats();
    let reporters = [bond_key(13), bond_key(14)];
    let producers = [None, Some(11u64), Some(12u64)];
    let claims: Vec<(Hash64, PalwClaimStateV2)> = t.c.s.claims_iter().map(|(id, c)| (*id, c.clone())).collect();
    let pick = |rng: &mut Rng, v: &[Hash64]| -> Option<Hash64> { (!v.is_empty()).then(|| v[rng.below(v.len() as u64) as usize]) };
    let of_phase = |f: &dyn Fn(&PalwClaimPhaseV2) -> bool| -> Vec<Hash64> {
        claims.iter().filter(|(_, c)| f(&c.phase)).map(|(id, _)| *id).collect()
    };
    let mut roll = rng.below(100);
    let revealable = eqs.iter().any(|e| e.4 && t.c.s.reward_pending(&e.2).is_some_and(|p| p.accepts_reveals()));
    if revealable && rng.below(2) == 0 {
        roll = 70;
    }
    *seq += 1;
    let (daa, objects, attempt): (u64, Vec<PalwConsensusObjectV2>, Option<TapeAttempt>) = if roll < 16 {
        let n = producers[rng.below(3) as usize];
        let seed = 0x7070_0000 + *seq;
        let (env, key, bond) = match n {
            Some(n) => {
                let (env, key, _) = floor_attempt_of(&t.c, n, seed);
                (env, key, bond_key(n))
            }
            None => {
                let (bond, pubkey, operator) = floor_producer(&t.c.p);
                let (env, key, _) = junk_attempt(floor, bond, pubkey, &operator, t.c.floor_pwu(now + 1), seed, 0x10C0 + seed);
                (env, key, bond)
            }
        };
        (now + 1, vec![], Some((env, key, floor_job_anchor(&t.c.p, bond, 0x10C0 + seed))))
    } else if roll < 30 {
        let unbound = of_phase(&|p| matches!(p, PalwClaimPhaseV2::Provisional));
        let Some(id) = pick(rng, &unbound) else { return false };
        (now + 1, vec![PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + now), seats: seats_of(&seats) }], None)
    } else if roll < 44 {
        let bound = of_phase(&|p| matches!(p, PalwClaimPhaseV2::PanelBound { .. }));
        let Some(id) = pick(rng, &bound) else { return false };
        let PalwClaimPhaseV2::PanelBound { bound_daa, .. } = t.c.s.claim(&id).unwrap().phase else { unreachable!() };
        let object = match rng.below(4) {
            0 => PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats.iter().map(|(k, _)| valid(id, *k, bound_daa)).collect(),
            },
            1 => {
                let mut receipts: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound_daa)).collect();
                receipts.extend(seats[3..].iter().map(|(k, _)| unavailable(id, *k, bound_daa)));
                PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }
            }
            2 => PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats[..3].iter().map(|(k, _)| valid(id, *k, bound_daa)).collect(),
            },
            _ => PalwConsensusObjectV2::ReceiptLicensedV2 {
                claim: id,
                receipts: covered(id, t.c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound_daa),
            },
        };
        (now + 1, vec![object], None)
    } else if roll < 54 {
        // A DA accusation: by a seat of the panel or a bystander, on a live, licensed or Final claim.
        let accusable = of_phase(&|p| {
            matches!(
                p,
                PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. } | PalwClaimPhaseV2::Final { .. }
            )
        });
        let Some(id) = pick(rng, &accusable) else { return false };
        let accuser = if rng.below(2) == 0 { seats[rng.below(5) as usize].0 } else { reporters[rng.below(2) as usize] };
        let object = PalwConsensusObjectV2::DefaultAccused {
            claim: id,
            missing_event_index: palw_da_event_index_v1(0, 0),
            accuser,
            signature: vec![],
        };
        (now + 1, vec![object], None)
    } else if roll < 58 {
        // A court opened by a bystander on a licensed claim; its responder never moves.
        let licensed = of_phase(&|p| matches!(p, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        let Some(id) = pick(rng, &licensed) else { return false };
        let challenger = reporters[rng.below(2) as usize];
        let record = t.c.s.claim(&id).unwrap().clone();
        const SPACE: kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1 =
            kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves;
        let object = PalwConsensusObjectV2::CourtOpened {
            session_id: kaspa_consensus_core::palw_court_v2::court_session_id_v2(
                &id,
                &record.trace_root,
                &record.bond,
                &challenger,
                SPACE,
                16,
            ),
            claim: id,
            challenger_bond: challenger,
            space: SPACE,
            space_size: 16,
            signature: Vec::new(),
        };
        (now + 1, vec![object], None)
    } else if roll < 64 {
        // A planned equivocation: one or two reporters commit to it before it is filed.
        let accused = if rng.below(3) == 0 { bond_key(11 + rng.below(2)) } else { seats[rng.below(5) as usize].0 };
        let (_, evidence) = equivocation_of(accused, floor, *seq);
        let key = equivocation_key(accused, evidence);
        let mut committed = vec![reporters[rng.below(2) as usize]];
        if rng.below(2) == 0 {
            committed.push(if committed[0] == reporters[0] { reporters[1] } else { reporters[0] });
        }
        let objects = committed.iter().map(|r| reporter_commit(key, evidence, *r)).collect();
        eqs.push((accused, *seq, key, committed, false));
        (now + 1, objects, None)
    } else if roll < 69 {
        let waiting: Vec<usize> = (0..eqs.len()).filter(|i| !eqs[*i].4).collect();
        if waiting.is_empty() {
            return false;
        }
        let i = waiting[rng.below(waiting.len() as u64) as usize];
        let (accused, nonce, _, _, _) = eqs[i].clone();
        let (object, _) = equivocation_of(accused, floor, nonce);
        eqs[i].4 = true;
        (now + 1, vec![object], None)
    } else if roll < 75 {
        let open: Vec<(Hash64, PalwBondKeyV2)> = eqs
            .iter()
            .filter(|e| e.4 && t.c.s.reward_pending(&e.2).is_some_and(|p| p.accepts_reveals()))
            .flat_map(|e| e.3.iter().map(move |r| (e.2, *r)))
            .collect();
        if open.is_empty() {
            return false;
        }
        let (key, reporter) = open[rng.below(open.len() as u64) as usize];
        // A reporter reveals once: whatever the block says, it is not asked again.
        if let Some(e) = eqs.iter_mut().find(|e| e.2 == key) {
            e.3.retain(|r| *r != reporter);
        }
        (now + 1, vec![reporter_reveal(key, reporter)], None)
    } else if roll < 77 {
        // A speculative commitment to a live claim's public DA key (V3S-03): it can never win.
        let live = of_phase(&|p| !matches!(p, PalwClaimPhaseV2::Voided { .. }));
        let Some(id) = pick(rng, &live) else { return false };
        let producer = t.c.s.claim(&id).unwrap().bond;
        let key = palw_da_offence_id_v1(&producer.0, &id);
        (now + 1, vec![reporter_commit(key, h(*seq), reporters[rng.below(2) as usize])], None)
    } else {
        // Time: to the next deadline-sized step.
        let jump = [1u64, 7, 61, 601, 1_201, 3_001][rng.below(6) as usize];
        (now + jump, vec![], None)
    };
    let kind_commit = objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterCommitted { .. }));
    let kind_reveal = objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterRevealed { .. }));
    let kind_court = objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::CourtOpened { .. }));
    if t.probe(daa, &objects, attempt.as_ref(), if attempt.is_some() { T12_BLOCK_SUBSIDY_SOMPI } else { 0 }).is_err() {
        cov.refused += 1;
        // A refused equivocation stays unfiled; a refused commitment plan is dropped.
        if kind_commit {
            eqs.pop();
        }
        if let Some(e) = eqs.iter_mut().find(|e| {
            objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::ObjectiveOffence { accused, .. } if *accused == e.0)) && e.4
        }) {
            e.4 = false;
        }
        return false;
    }
    cov.commits += usize::from(kind_commit);
    cov.reveals += usize::from(kind_reveal);
    cov.courts += usize::from(kind_court);
    let subsidy = if attempt.is_some() { T12_BLOCK_SUBSIDY_SOMPI } else { 0 };
    t.block(daa, objects, attempt, subsidy).expect("a probed block folds");
    true
}

/// Coverage read off one committed block's transition.
fn t07_count(parent: &PalwChainStateV2, b: &TapeBlock, cov: &mut T07Coverage) {
    use kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1;
    use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2;
    let s = &b.state;
    for (id, claim) in s.claims_iter() {
        let before = parent.claim(id).map(|c| c.phase.clone());
        if before.as_ref() == Some(&claim.phase) {
            continue;
        }
        match &claim.phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtDefault | PalwVoidReasonV2::CourtFraud, .. } => {
                cov.court_defaults += 1
            }
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. } => cov.receipt_timeouts += 1,
            PalwClaimPhaseV2::ReceiptLicensed { .. } if !matches!(before, Some(PalwClaimPhaseV2::ReceiptLicensed { .. })) => {
                if claim.rcore.escrow_released { cov.licences_released += 1 } else { cov.licences_held += 1 }
            }
            _ => {}
        }
    }
    for entry in &b.delta.entries {
        if let kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::ConsumedOffence { old: None, new: Some(record), .. } = entry {
            match record.kind {
                PalwOffenceKindV1::DaDefault => cov.da_defaults += 1,
                PalwOffenceKindV1::ExecutorEquivocation => cov.equivocations += 1,
                _ => {}
            }
        }
    }
    cov.forgone |= s.reporter_counters().forgone_sompi > parent.reporter_counters().forgone_sompi;
}

fn c_window_court(t: &Tape) -> u64 {
    t.c.sp.window_court()
}

const T07_SEEDS: [u64; 4] = [0x7070_0001, 0x7070_0002, 0x7070_0003, 0x7070_0004];
const T07_STEPS: usize = 200;

/// One seed of the fuzz; returns its coverage.
fn t07_run(seed: u64, steps: usize) -> T07Coverage {
    let mut t = armed_tape();
    t.step(vec![bond_obj(11, 60_000 * MSK), bond_obj(12, 60_000 * MSK), bond_obj(13, 400_000 * MSK), bond_obj(14, 400_000 * MSK)]);
    let mut rng = Rng(seed);
    let mut seq = seed << 12;
    let mut eqs = Vec::new();
    let mut cov = T07Coverage::default();
    let mut oracle = T07Oracle::default();
    let mut forks: Vec<usize> = Vec::new();
    let clock = std::time::Instant::now();
    while t.len() < steps {
        let before = t.len();
        if !t07_act(&mut t, &mut rng, &mut seq, &mut eqs, &mut cov) {
            continue;
        }
        assert_eq!(t.len(), before + 1);
        let parent = t.state_at(before).clone();
        let block = t.blocks[before].clone();
        oracle.observe(&t.c, &parent, &block, &mut cov);
        t07_count(&parent, &block, &mut cov);
        if rng.below(23) == 0 {
            forks.push(before);
        }
    }
    // The maturity tail (seeded like the rest): a batch of thirty-one floor claims licensed in a row
    // and Final together, then a second batch Final after it — thirty-one anchors settled past the
    // first batch's Final, the second clock's depth (30) — then time past the first batch's DAA
    // clock: its rows latch together and move a row per block (six new queue keys each against 3d's
    // budget of eight), a backlog the last blocks drain part of.
    let observe = |t: &Tape, oracle: &mut T07Oracle, cov: &mut T07Coverage| {
        let j = t.len() - 1;
        let parent = t.state_at(j).clone();
        let block = t.blocks[j].clone();
        oracle.observe(&t.c, &parent, &block, cov);
        t07_count(&parent, &block, cov);
    };
    let mut batch_final = Vec::new();
    for batch in 0..2u64 {
        for k in 0..31u64 {
            let id = t.attempt(None, (seed << 8) ^ 0x7A11_0000 ^ (batch << 6) ^ k);
            observe(&t, &mut oracle, &mut cov);
            let bound = t.bind(id);
            observe(&t, &mut oracle, &mut cov);
            let seats = t.c.floor_seats();
            t.step(vec![PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
            }]);
            observe(&t, &mut oracle, &mut cov);
        }
        t.at(t.c.daa + 1_201, vec![]);
        observe(&t, &mut oracle, &mut cov);
        batch_final.push(t.c.daa);
    }
    t.at(batch_final[0] + c_window_court(&t) + 1, vec![]);
    observe(&t, &mut oracle, &mut cov);
    for _ in 0..8 {
        t.step(vec![]);
        observe(&t, &mut oracle, &mut cov);
    }
    cov.blocks = t.len();
    let built = clock.elapsed();
    // Every tip is a reorg target: revert block by block to the base, re-apply to the tip.
    t.revert_to_base_and_reapply();
    let reverted = clock.elapsed();
    // The IBD twin: the run folded from a genesis rebuilt from scratch.
    let mut scratch = Chain::new(t12());
    scratch.attribution = true;
    t.ibd_from(scratch.s);
    let ibd = clock.elapsed();
    // Sibling reorgs: at seeded tips a different branch (time moved on differently, an attempt of
    // its own) is reorged to and back.
    for j in forks {
        let mut sibling = t.fork(j);
        let jump = 1 + rng.below(1_500);
        sibling.at(sibling.c.daa + jump, vec![]);
        // An attempt of its own by bond 12, which the run may have drained below the producer floor
        // (U2): then the fold skips it, non-fatally, and the sibling records that block as it is.
        let seed = 0x5B1B_0000 + j as u64;
        let (env, key, _) = floor_attempt_of(&sibling.c, 12, seed);
        let anchor = floor_job_anchor(&sibling.c.p, bond_key(12), 0x10C0 + seed);
        let daa = sibling.c.daa + 1;
        sibling.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the sibling's attempt block folds");
        t.reorg_to(j, &sibling);
        cov.sibling_reorgs += 1;
    }
    let reorged = clock.elapsed();
    // Restarts at seeded tips: the carriage decoded and loaded under its root, the rest folded on.
    for j in (0..t.len()).step_by(29).chain([t.len() - 1]) {
        t.restart_at(j);
        cov.restarts += 1;
    }
    println!(
        "T07 seed {seed:#x} timing: built {built:?} reverted {reverted:?} ibd {ibd:?} reorged {reorged:?} restarted {:?}",
        clock.elapsed()
    );
    println!(
        "T07 seed {seed:#x}: {cov:?}; invariant checked {} times, worst margin {:?}",
        oracle.checks,
        oracle.worst_margin.as_ref().map(|(m, at)| (*m as f64 / MSK as f64, at.clone()))
    );
    cov
}

/// **T07: the reorg fuzz.** Over a fixed set of seeded block sequences on testnet-12's own fold (with
/// `palw_offence_attribution` armed as the processor arms it) — attempts by three producers, binds,
/// licences through the V1 door (all `Valid`; three `Valid` and two `Unavailable`; three `Valid` and
/// two missing) and the coverage door, DA sessions by seats and bystanders and their defaults (S1,
/// S4, S3 with the row burn), courts whose responder never moves (court defaults), equivocations
/// with reporter commitments, reveals and speculative commitments to public DA keys, and time
/// jumps sized to every deadline (receipt timeouts and redraws, Final, the reveal sweep, row latch,
/// move and retirement):
///
/// * after every block, for every producer bond: **Σ recoverable (net of R) ≥ Σ extractable** over its
///   Final claims inside their conviction window, the window read from the row's clocks independently
///   of the fold's latch — extractable `G_res` (the frozen gain), plus the escrow if the row left
///   early; recoverable the row and every live lock on the claim capped by its seat's collateral, less
///   R-1's reward on each lock's debit. Beside it: V-3's counters, every Final writes its row with the
///   licence's frozen `G_res` (V-2), no row latches inside its window, no row moves unlatched, and no
///   row leaves without a `Moved` or `Burned` note;
/// * the run reverts block by block to its base and re-applies (every tip a reorg target: the same
///   state and root), is folded again from a genesis rebuilt from scratch (the IBD twin: the same
///   deltas and roots), a sibling branch at seeded tips is reorged to and back, and the carriage at
///   seeded tips restarts under its root with the rest of the run folded on the loaded state.
///
/// The seeds together must drive every kind of event the property names.
#[test]
fn t07_reorg_fuzz_recoverable_covers_extractable_and_every_tip_reverts_and_replays() {
    t07_fuzz(&T07_SEEDS, T07_STEPS);
}

/// **T07, heavy**: sixteen seeds of six hundred random blocks each (the default run is the four-seed
/// twin above, the same property on fewer blocks).
#[test]
#[ignore = "heavy: 16 seeds x 600 random blocks; the default run is t07_reorg_fuzz_recoverable_covers_extractable_and_every_tip_reverts_and_replays"]
fn t07_reorg_fuzz_heavy() {
    let seeds: Vec<u64> = (0..16u64).map(|i| 0x7070_1000 + i).collect();
    t07_fuzz(&seeds, 600);
}

/// The fuzz over `seeds`, `steps` random blocks each, the seeds folded side by side; the coverage
/// asserted over all of them together.
fn t07_fuzz(seeds: &[u64], steps: usize) {
    let mut total = T07Coverage::default();
    // The seeds are independent chains: folded four side by side, each deterministic on its own.
    let runs: Vec<T07Coverage> = seeds
        .chunks(4)
        .flat_map(|chunk| {
            std::thread::scope(|scope| {
                let handles: Vec<_> = chunk.iter().map(|seed| scope.spawn(move || t07_run(*seed, steps))).collect();
                handles.into_iter().map(|h| h.join().expect("a seed's run")).collect::<Vec<_>>()
            })
        })
        .collect();
    for c in runs {
        total.finals += c.finals;
        total.da_defaults += c.da_defaults;
        total.equivocations += c.equivocations;
        total.court_defaults += c.court_defaults;
        total.receipt_timeouts += c.receipt_timeouts;
        total.reveals += c.reveals;
        total.courts += c.courts;
        total.awards_moved += c.awards_moved;
        total.forgone |= c.forgone;
        total.latched += c.latched;
        total.rows_moved += c.rows_moved;
        total.rows_burned += c.rows_burned;
        total.licences_released += c.licences_released;
        total.licences_held += c.licences_held;
        total.sibling_reorgs += c.sibling_reorgs;
        total.restarts += c.restarts;
    }
    println!("T07 total: {total:?}");
    // The seeds are not vacuous: together they drove every event the property names.
    for (what, n) in [
        ("Finals", total.finals),
        ("released licences (E at licence)", total.licences_released),
        ("held licences", total.licences_held),
        ("receipt timeouts (RT#2)", total.receipt_timeouts),
        ("DA defaults", total.da_defaults),
        ("equivocations", total.equivocations),
        ("court defaults", total.court_defaults),
        ("reveals", total.reveals),
        ("reporter awards moved by 3d", total.awards_moved),
        ("row latches", total.latched),
        ("row moves", total.rows_moved),
        ("row burns", total.rows_burned),
        ("sibling reorgs", total.sibling_reorgs),
        ("restarts", total.restarts),
    ] {
        assert!(n > 0, "the seeds drove no {what}");
    }
    assert!(total.forgone, "the seeds drove no forgone reward");
}
