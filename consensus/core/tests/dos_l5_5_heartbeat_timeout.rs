//! **L5 stress test 5 — the heartbeat-only timeout attack, with the two-clock fix ARMED.**
//!
//! Real fold, t12 genesis state, t12 params (`palw_settled_anchor_depth = 30`, the 09-23 fence at
//! DAA 0). A prefix of honest traffic walks `N_FINALS` floor claims to `Final` (attempt ->
//! `PanelBound` -> `ReceiptLicensed` -> challenge-window sweep), so the second clock stands at
//! `N_FINALS >= depth`. Then the chain is driven with claimless blocks only — the heartbeat lane,
//! which carries ordinary transactions (so a bond registration rides it: palw_heartbeat_v1.rs:7-8)
//! but no attempt.
//!
//! What the fix promises (processor.rs `palw_bond_maturity_window_at`, palw_panel_v2.rs
//! `palw_settled_anchor_floor_daa_v1`): a bond may judge only once `depth` anchors have settled
//! since it registered, and a heartbeat history settles none.
//!
//! What the audit measured (2026-09-24, before `6bb8c844`), per rule the fix names:
//!   * slash-liability expiry            — rooted counter `settled_attempt_finals`  -> HELD
//!   * retiring-bond withdrawal          — rooted counter                           -> HELD
//!   * ADR-0065 D1 seat maturity         — the floor WALKED the live claim table, and `Final`
//!                                          claims RETIRE after `claim_retirement_daa` -> BROKE
//!   * what still expired on DAA alone   — licensed claims finalize at `window_challenge`, and
//!                                          every such `Final` ticked the second clock
//!
//! What this file now asserts, against the rework (fixes #2, #3, #13a) and exactly as the processor
//! reads it — the escaped depth (`palw_second_clock_depth_v1`) first, then the ring-backed floor:
//!   * the backlog's `Final`s on heartbeats tick nothing (anchors are licences past the fence);
//!   * until `E = last licence + 2 × window_court` no D1 maturity is granted to a bond registered
//!     after the last anchor, no slash lock expires and the retiring bond stays locked (`v4`);
//!   * from `E` the second clock is waived (the liveness escape): the DAA clock alone decides, so
//!     the sybil matures on D1's window, the lock (DAA-expired long before) is released, and the
//!     retiring bond waits only for its DAA delay.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l5_5_heartbeat_timeout -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::PALW_T12_BOND_MATURITY_WINDOW_DAA;
use kaspa_consensus_core::palw_panel_v2::{palw_bond_maturity_window_v2, palw_settled_anchor_floor_daa_v1};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, palw_bond_collateral_is_locked_v4,
    palw_second_clock_depth_v1,
};
use kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2;

const N_FINALS: u64 = 31;

struct Chain {
    p: kaspa_consensus_core::config::params::Params,
    sp: kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    blue: u64,
    block: u64,
}

impl Chain {
    fn new() -> Self {
        let p = t12();
        let sp = bundle(&p).state.clone();
        let s = genesis_state(&p);
        Chain { p, sp, s, daa: 0, blue: 0, block: 0x100 }
    }
    fn step(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, subsidy: u64) {
        assert!(daa >= self.daa);
        self.daa = daa;
        self.blue += 1;
        self.block += 1;
        let (next, _, _) = fold(&self.p, &self.sp, &self.s, &ctx(self.block, daa, self.blue, subsidy), objects, work, key)
            .unwrap_or_else(|e| panic!("block at DAA {daa} folds: {e:?}"));
        self.s = next;
    }
    /// A heartbeat: claimless, subsidy 0 (palw_heartbeat_v1.rs: "a heartbeat block's declared subsidy is zero").
    fn beat(&mut self, daa: u64, objects: &[PalwConsensusObjectV2]) {
        self.step(daa, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    }
}

/// Walk one floor claim of `executor` to `ReceiptLicensed` on the seats given. Returns its id.
fn license_one(c: &mut Chain, seed: u64, executor: (PalwBondKeyV2, Vec<u8>, Vec<u8>), seats: &[(PalwBondKeyV2, Hash64)]) -> Hash64 {
    let (floor, leaves, target, _) = genesis_classes(&c.p)[0];
    let (env, key, id) = junk_attempt(floor, executor.0, executor.1.clone(), &executor.2, palw_pwu_v1(target, leaves), seed, 0x5EED_0000 + seed);
    let d = c.daa + 1;
    c.step(d, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    let bound = PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA0C0 + seed), seats: seats_of(seats) };
    c.beat(c.daa + 1, &[bound]);
    assert!(matches!(c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "claim {seed} bound");
    let receipts = valid_receipts(id, &seats.iter().map(|s| s.0).collect::<Vec<_>>());
    c.beat(c.daa + 1, &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    assert!(matches!(c.s.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "claim {seed} licensed");
    id
}

fn executor_and_seats(p: &kaspa_consensus_core::config::params::Params) -> ((PalwBondKeyV2, Vec<u8>, Vec<u8>), Vec<(PalwBondKeyV2, Hash64)>) {
    let b = bundle(p);
    let mut regs = Vec::new();
    for o in b.genesis_objects.iter() {
        if let PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } = o {
            regs.push((*bond, pubkey.clone(), operator_pubkey.clone()));
        }
    }
    let exec = regs[0].clone();
    let seats: Vec<(PalwBondKeyV2, Hash64)> = regs[1..6]
        .iter()
        .map(|(b, _, op)| (*b, kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(op)))
        .collect();
    (exec, seats)
}

#[test]
fn dos_l5_5_heartbeat_only_history_with_the_two_clock_fix_armed() {
    let mut c = Chain::new();
    let depth = c.p.palw_settled_anchor_depth.expect("t12 arms the second clock");
    let window_challenge = c.sp.window_challenge_at(1_000);
    let retirement = c.sp.claim_retirement_daa();
    let window_court = c.sp.window_court();
    let d1_window = PALW_T12_BOND_MATURITY_WINDOW_DAA;
    let (exec, seats) = executor_and_seats(&c.p);

    // ---- phase 1: honest traffic settles N_FINALS anchors ------------------------------------
    c.beat(1_000, &[]);
    let mut finals = Vec::new();
    for i in 0..N_FINALS {
        finals.push(license_one(&mut c, i + 1, exec.clone(), &seats));
    }
    // One more licensed claim that is still in its challenge window when the chain goes quiet.
    let backlog: Vec<Hash64> = (0..5).map(|i| license_one(&mut c, 1_000 + i, exec.clone(), &seats)).collect();
    // The challenge window of the first N_FINALS closes (on blocks that carry work: the honest era).
    let last_licensed_daa = c.daa;
    let finalize_at = last_licensed_daa - 5 * 3 + window_challenge + 1; // strictly past the N_FINALS' deadlines, before the backlog's
    c.beat(finalize_at, &[]);
    let finals_now = finals.iter().filter(|id| matches!(c.s.claim(id).map(|x| &x.phase), Some(PalwClaimPhaseV2::Final { .. }))).count();
    let settled_honest = c.s.settled_attempt_finals();
    let last_final_daa = finalize_at;
    // A seat that signed Valid holds a slashable lock; pick one to watch.
    let (watched_seat, _) = seats[0];
    let watched_claim = finals[N_FINALS as usize - 1];
    let watched_lock = *c.s.slashable_lock(watched_seat, watched_claim).expect("a Valid seat holds a lock");

    // A retiring bond: seat #5 has locks, so retire the one bond that holds none — a newcomer.
    let retiree = bond_key(77);
    c.beat(c.daa + 1, &[bond_obj(77, at_least_the_floor(&c.p, 1_000_000_000_000))]);
    c.beat(c.daa + 1, &[PalwConsensusObjectV2::BondRetireRequested { bond: retiree, signature: vec![1] }]);

    println!("=== phase 1: the honest era ===");
    println!("challenge window (window_challenge_at) = {window_challenge} DAA");
    println!("finals settled (rooted counter)      = {settled_honest}   (depth = {depth})");
    println!("claims in Final                      = {finals_now}/{N_FINALS}");
    println!("backlog still ReceiptLicensed        = {}", backlog.len());
    println!("last Final at DAA                    = {last_final_daa}");
    println!("watched lock: amount={} expiry_daa={} settled_at_final={}", watched_lock.amount, watched_lock.expiry_daa, watched_lock.settled_at_final);
    assert_eq!(finals_now as u64, N_FINALS, "the prefix settled its anchors");
    assert!(settled_honest >= depth, "the second clock is past depth: the bootstrap waiver is over");

    // ---- phase 2: the chain goes heartbeat-only; a sybil bond registers on a heartbeat -------
    let sybil_reg_daa = c.daa + 1;
    c.beat(sybil_reg_daa, &[bond_obj(66, at_least_the_floor(&c.p, 1_000_000_000_000))]);
    // The last licence: the liveness escape is measured from it.
    let last_licence = *c.s.recent_anchor_daas().last().expect("past the fence every licence joins the ring");
    let escape_at = last_licence + 2 * window_court;
    // The second clock as the processor reads it at `at`: `palw_second_clock_depth_at`.
    let clock = |s: &PalwChainStateV2, at: u64| palw_second_clock_depth_v1(Some(depth), s.recent_anchor_daas(), at, window_court);
    let anchor_probe = |s: &PalwChainStateV2, anchor: u64| -> (Option<u64>, u64, bool) {
        // `palw_bond_maturity_window_at`: the escaped depth, then the ring-backed floor.
        let floor = clock(s, anchor).and_then(|depth| palw_settled_anchor_floor_daa_v1(s, anchor, depth));
        let window = palw_bond_maturity_window_v2(anchor, d1_window, floor);
        let registered_by = anchor.saturating_sub(window);
        let sybil = s.bond(&bond_key(66)).expect("sybil registered");
        (floor, window, sybil.registered_daa <= registered_by)
    };
    let (floor_at_reg, _, _) = anchor_probe(&c.s, sybil_reg_daa + d1_window);
    println!();
    println!("=== phase 2: heartbeat-only history (no attempt, no Final) ===");
    println!("sybil bond registered at DAA         = {sybil_reg_daa}");
    println!("settled-anchor floor at that time    = {floor_at_reg:?}");

    let mut table = Vec::new();
    let mut beats = 0u64;
    // Past the audit's horizon (`claim_retirement + window`) AND past the escape.
    let stop = (last_final_daa + retirement + d1_window + 200).max(escape_at + 500);
    let mut next = c.daa + 1;
    let settled_before_backlog = c.s.settled_attempt_finals();
    while next <= stop {
        c.beat(next, &[]);
        beats += 1;
        let (floor, window, eligible) = anchor_probe(&c.s, next);
        let lock_live = c
            .s
            .slashable_lock(watched_seat, watched_claim)
            .map(|l| l.is_live_v2(next, c.s.settled_attempt_finals(), clock(&c.s, next)))
            .unwrap_or(false);
        // The withdrawal gate as `palw_v2_locked_bond_outpoints` reads it: v4, duty gate on.
        let withdraw_locked = palw_bond_collateral_is_locked_v4(
            &c.s,
            &retiree,
            c.s.bond(&retiree).unwrap(),
            next,
            bundle(&c.p).bond.withdrawal_delay_daa(),
            clock(&c.s, next),
            true,
        );
        let live_finals = c.s.claims_iter().filter(|(_, x)| matches!(x.phase, PalwClaimPhaseV2::Final { .. })).count();
        table.push((next, c.s.settled_attempt_finals(), live_finals, floor, window, eligible, lock_live, withdraw_locked));
        next += 50;
    }
    let settled_after = c.s.settled_attempt_finals();
    let backlog_final = backlog.iter().filter(|id| matches!(c.s.claim(id).map(|x| &x.phase), Some(PalwClaimPhaseV2::Final { .. })) || c.s.claim(id).is_none()).count();

    println!("{:>7} {:>8} {:>11} {:>14} {:>8} {:>15} {:>9} {:>15}", "DAA", "settled", "live Finals", "anchor floor", "D1 win", "sybil eligible", "lock live", "retiree locked");
    let mut first_eligible = None;
    for (i, row) in table.iter().enumerate() {
        if row.5 && first_eligible.is_none() {
            first_eligible = Some(row.0);
        }
        if i % 12 == 0 || (row.5 && first_eligible == Some(row.0)) {
            println!(
                "{:>7} {:>8} {:>11} {:>14} {:>8} {:>15} {:>9} {:>15}",
                row.0,
                row.1,
                row.2,
                format!("{:?}", row.3),
                row.4,
                row.5,
                row.6,
                row.7
            );
        }
    }
    let bound: Vec<_> = table.iter().filter(|r| r.0 < escape_at).collect();
    let escaped: Vec<_> = table.iter().filter(|r| r.0 >= escape_at).collect();
    let all_locks_live = bound.iter().all(|r| r.6);
    let all_withdraw_locked = bound.iter().all(|r| r.7);
    let heartbeat_hashes: u128 = (beats as u128) * 50 * (1u128 << PALW_HEARTBEAT_WORK_LOG2);
    println!();
    println!("--- verdicts ---");
    println!("heartbeat blocks folded (1 per 50 DAA stand-in) = {beats}; DAA advanced = {}", stop - sybil_reg_daa);
    println!("attacker cost to drive it: {heartbeat_hashes} hashes (= DAA x 2^{PALW_HEARTBEAT_WORK_LOG2}), 0 bond, 0 inference");
    println!("last licence at DAA {last_licence}; the liveness escape E = {escape_at}");
    println!("slash liability live until E         = {all_locks_live}");
    println!("retiring bond locked until E         = {all_withdraw_locked}");
    println!("backlog licensed claims -> Final     = {backlog_final}/{} on DAA alone; settled counter {settled_before_backlog} -> {settled_after}", backlog.len());
    match first_eligible {
        Some(d) => println!(
            "D1: sybil registered at {sybil_reg_daa} with ZERO anchors since, eligible to judge at anchor DAA {d} \
             (E = last licence {last_licence} + 2 x window_court {window_court}): the liveness escape, after which D1's \
             DAA window alone decides (the audit measured last Final {last_final_daa} + claim_retirement {retirement} + ...)"
        ),
        None => println!("D1: sybil never became eligible on this history"),
    }

    // ---- the assertions: the fix's promise, stated as the bound -------------------------------
    let (retiree_since, _) = match c.s.bond(&retiree).unwrap().status {
        kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { since_daa, settled_at_since } => (since_daa, settled_at_since),
        _ => panic!("the retiree is retiring"),
    };
    let delay = bundle(&c.p).bond.withdrawal_delay_daa();
    assert!(!bound.is_empty() && !escaped.is_empty(), "the table spans the escape");
    assert_eq!(backlog_final, backlog.len(), "the backlog reached Final on DAA alone");
    assert_eq!(settled_after, settled_before_backlog, "…and those Finals, swept on heartbeats, settled NO anchor");
    assert_eq!(settled_honest, N_FINALS + backlog.len() as u64, "the counter counts licences: one per licensed claim");
    assert!(all_locks_live, "no slash liability may expire on heartbeat-only history before the escape");
    assert!(all_withdraw_locked, "no retiring bond may withdraw on heartbeat-only history before the escape");
    assert!(
        bound.iter().all(|r| !r.5),
        "NO D1 maturity may be granted on heartbeat-only history before the escape: a bond registered after the last \
         settled anchor must wait for `depth` = {depth} further anchors. Measured: eligible at DAA {first_eligible:?}."
    );
    // The escape: from E the DAA clock alone decides, as below the fence.
    assert_eq!(first_eligible, escaped.first().map(|r| r.0), "the sybil matures on D1's DAA window at the escape, not before");
    assert!(escaped.iter().all(|r| r.5 && !r.6), "past E: mature on the DAA window, and the DAA-expired lock is released");
    assert!(
        escaped.iter().all(|r| r.7 == (r.0 < retiree_since + delay)),
        "past E the retiring bond waits only for its DAA delay ({retiree_since} + {delay})"
    );
}
