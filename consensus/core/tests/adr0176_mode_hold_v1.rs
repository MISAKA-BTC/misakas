//! **ADR-0176 — the mode-aware common hold (ユーザー方針 2026-10-15)**, on the budget engine itself (`PalwBondBudgetStateV1`), every
//! block's release run before its admissions as the fold does (`tick_bond_budget_v1`).
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS.** Every policy is a TEST value built from the
//! V3 form (`hold_baseline_unapproved_v1`): the 1-day / 7-day spans are the user's baseline, the per-day rates are fixtures, never
//! production values.
//!
//! RFC-0015 §13.2 cap rows and the user's 2026-10-15 test list covered here:
//!
//! | test | row / guidance item |
//! |---|---|
//! | `per_day_issuance_is_equal_for_both_modes_over_seven_days` | guidance 2 (per-day equalization; no 7× faster recovery) |
//! | `honest_and_fast_forger_reach_the_same_counts` | §13.2 "同額bond … 正直な計算対省略偽造"; guidance 9 (honest vs fast forgery) |
//! | `colluding_immediate_valid_does_not_shorten_panel_one_day` | §13.2 "早期検証/Final … d + W"; guidance 4, 9 |
//! | `panel_zero_then_panel_one_gets_no_fresh_day` / `panel_one_then_panel_zero_gets_no_fresh_week` | guidance 3, 9 (mode switching) |
//! | `mass_parallel_submission_is_cut_by_the_burst_view` | guidance 6, 9 (mass parallel) |
//! | `period_boundary_hammering_never_exceeds_any_rolling_window` | §13.2 "期間境界 … 丸め/burst/繰越"; guidance 9 |
//! | `honest_minimal_miner_sustains_its_daily_cap` | guidance 9 (honest-miner reachability, minimal configuration near cap) |
//! | `concurrent_liability_cap_binds_beside_the_period_caps` | §13.2 "`d + W`到達、未解決court…"; guidance 5 |
//! | `split_capital_never_beats_the_whole_in_any_mode` | §13.2 "bond分割…"; RFC-0015 §8.3.4 |
//! | `rho_moves_claims_only_in_both_modes` | §13.2 "容量×1/×100/×1000" |
//! | `restart_and_reorg_restore_the_same_views` | §13.2 "restart/IBD/reorg"; guidance 9 (restart/reorg) |
//! | `the_v3_policy_pins_the_users_spans` | validation (no hours/days drift) |
//!
//! Run (validation phase): `cargo test -p kaspa-consensus-core --test adr0176_mode_hold_v1`

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bond_budget_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

const KAS: u64 = 100_000_000;
const DAY: u64 = PALW_BOND_HOLD_DAA_PER_DAY_V1;

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes([n; 64]), 0))
}

fn id(bond: u8, n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[0] = bond;
    b[8..16].copy_from_slice(&n.to_le_bytes());
    Hash64::from_bytes(b)
}

/// TEST policy (V3): per 1,000 BILI of capital per DAY — 3 claims, 3 reward blocks, 30 BILI, 3,000 weight; ρ as given.
fn policy(rho: u32) -> PalwBondBudgetPolicyV1 {
    let mut p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    p.capital_unit_sompi = 1_000 * KAS;
    p.rho = rho;
    p.claims_per_unit = 3;
    p.block_units_per_unit = 3 * PALW_BUDGET_BLOCK_UNIT_V1;
    p.reward_per_unit_sompi = 30 * KAS;
    p.final_weight_per_unit = 3_000;
    p.max_open_claims_per_bond = 1_000_000;
    p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 };
    p.validate().expect("a well-formed V3 test policy");
    p
}

/// One claim's ask: one block, 10 BILI, 1,000 weight, `k` of liability collateral.
fn ask(k: u64) -> PalwBudgetAskV1 {
    PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 10 * KAS, final_weight: 1_000, liability_sompi: k as u128 }
}

struct World {
    p: PalwBondBudgetPolicyV1,
    s: PalwBondBudgetStateV1,
    next: u64,
}

impl World {
    fn new(p: PalwBondBudgetPolicyV1) -> Self {
        let s = PalwBondBudgetStateV1::new(0, &p);
        Self { p, s, next: 0 }
    }

    fn tick(&mut self, now: u64) {
        let mut j = Vec::new();
        self.s.release_due(now, &mut j);
    }

    fn try_claim(&mut self, b: u8, capital: u64, now: u64, mode: PalwHoldModeV1, k: u64) -> Result<(), PalwBudgetRefusalV1> {
        self.next += 1;
        let origin = if mode == PalwHoldModeV1::PanelZero { PalwBudgetOriginV1::KernelRoute } else { PalwBudgetOriginV1::Attempt };
        let mut j = Vec::new();
        self.s.reserve_claim_moded_v1(&self.p, capital, id(b, self.next), bond(b), None, now, ask(k), origin, mode, &mut j).map(|_| ())
    }

    /// The colluding immediate Valid / early Final: the claim closes at once (its liability hold starts), with every right consumed.
    fn close_all_open(&mut self, now: u64) {
        let open: Vec<Hash64> = (1..=self.next)
            .flat_map(|n| (0u8..=20).map(move |b| id(b, n)))
            .filter(|c| self.s.claim_row(c).is_some_and(|r| r.open))
            .collect();
        let mut j = Vec::new();
        for c in open {
            self.s.close_at(&c, Some(now + self.p.liability_hold_daa), &mut j);
        }
    }

    /// Accepted claims of `b` (shadow rows excluded) whose acceptance lies in `[from, to)`.
    fn accepted_in(&self, b: u8, from: u64, to: u64) -> usize {
        (1..=self.next).filter(|n| self.s.claim_row(&id(b, *n)).is_some_and(|r| r.accepted_daa >= from && r.accepted_daa < to)).count()
    }
}

#[test]
fn per_day_issuance_is_equal_for_both_modes_over_seven_days() {
    // 1,000 BILI → daily caps (3 claims, 3 blocks, 30 BILI, 3,000 weight); Panel=1 over 1 day, Panel=0 7× that over 7 days.
    let p = policy(1);
    let hold = p.hold.unwrap();
    let capital = 1_000 * KAS;
    let daily = palw_bond_hold_daily_caps_v1(&p, capital);
    assert_eq!(daily.claims, 3);
    assert_eq!(palw_bond_hold_period_caps_v1(&p, &hold, capital, PalwHoldModeV1::PanelOne).claims, 3);
    assert_eq!(palw_bond_hold_period_caps_v1(&p, &hold, capital, PalwHoldModeV1::PanelZero).claims, 21);
    assert_eq!(hold.window_daa(PalwHoldModeV1::PanelOne), DAY);
    assert_eq!(hold.window_daa(PalwHoldModeV1::PanelZero), 7 * DAY);
    assert_eq!(p.window_daa, 7 * DAY, "the long window is Panel=0's span");

    // Each mode, one claim offered every 10 DAA for 7 days: both admit exactly 21 (3 per day for Panel=1; up to the 7-day total for
    // Panel=0, the burst view spreading it), never 21 per day.
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        let mut w = World::new(p.clone());
        let mut admitted = 0;
        for now in (1..7 * DAY).step_by(10) {
            w.tick(now);
            if w.try_claim(1, capital, now, mode, 0).is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 21, "{mode:?}: the same per-day maximum issuance over the 7 days");
        w.s.check_consistency().expect("consistent");
    }
}

#[test]
fn honest_and_fast_forger_reach_the_same_counts() {
    // Same bond capital, same mode, same period: the honest miner submits one claim every 240 DAA (its inference time); the forger,
    // skipping computation, submits every DAA. Both are admitted the same counts — the forger's extra submissions are refused.
    let p = policy(1);
    let capital = 1_000 * KAS;
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        let mut honest = World::new(p.clone());
        let mut forger = World::new(p.clone());
        let (mut h, mut f, mut f_refused) = (0, 0, 0);
        for now in 1..7 * DAY {
            honest.tick(now);
            forger.tick(now);
            if now % 240 == 0 && honest.try_claim(1, capital, now, mode, 0).is_ok() {
                h += 1;
            }
            match forger.try_claim(2, capital, now, mode, 0) {
                Ok(()) => f += 1,
                Err(_) => f_refused += 1,
            }
        }
        assert!(f <= 21 && h <= 21, "{mode:?}: neither passes 7·daily in 7 days");
        assert_eq!(h, 20, "{mode:?}: the honest pace reaches its 20 submissions in 7 days (one per 240 DAA)");
        assert!(f >= h && f <= h + 1, "{mode:?}: the forger gets no more rights than the cap allows (h = {h}, f = {f})");
        assert!(f_refused > 5_000, "{mode:?}: the fast forger's extra submissions are refused");
    }
}

#[test]
fn colluding_immediate_valid_does_not_shorten_panel_one_day() {
    // Panel=1: three claims at DAA 10 fill the day; colluding signers make them Valid/Final at once. Nothing returns before 10 + DAY.
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    w.tick(10);
    for _ in 0..2 {
        w.try_claim(1, capital, 10, PalwHoldModeV1::PanelOne, 0).unwrap();
    }
    // The burst view (2 per 72 DAA here) returns its room at 10 + 72; the third claim of the day lands at 83.
    w.tick(83);
    w.try_claim(1, capital, 83, PalwHoldModeV1::PanelOne, 0).unwrap();
    w.close_all_open(84);
    for now in 85..10 + DAY {
        w.tick(now);
        assert!(w.try_claim(1, capital, now, PalwHoldModeV1::PanelOne, 0).is_err(), "no early return at {now}");
    }
    w.tick(10 + DAY);
    w.try_claim(1, capital, 10 + DAY, PalwHoldModeV1::PanelOne, 0).expect("the day view returns exactly at d + W_day");
    w.s.check_consistency().expect("consistent");
}

#[test]
fn panel_zero_then_panel_one_gets_no_fresh_day() {
    // A bond consumes its 7-day Panel=0 total early on day 1, then switches to Panel=1: no fresh day while the Panel=0 reservations sit
    // in the long view (the Panel=1 claim checks it too), i.e. until they leave at d + 7 days.
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    let mut taken = 0;
    for now in (1..DAY).step_by(1) {
        w.tick(now);
        if w.try_claim(1, capital, now, PalwHoldModeV1::PanelZero, 0).is_ok() {
            taken += 1;
        }
    }
    assert!(taken >= 3, "Panel=0 may front-load beyond one day's rate (bounded by the burst view)");
    let last_p0 = (1..=w.next).filter_map(|n| w.s.claim_row(&id(1, n)).map(|r| r.accepted_daa)).max().unwrap();
    let mut p1_before = 0;
    for now in DAY..7 * DAY {
        w.tick(now);
        if w.try_claim(1, capital, now, PalwHoldModeV1::PanelOne, 0).is_ok() {
            p1_before += 1;
        }
    }
    // Over the first 7 days the bond never passes 7·daily whatever it switched to.
    assert!(taken + p1_before <= 21, "mode switching never re-acquires capacity: {taken} + {p1_before}");
    assert!(last_p0 < DAY);
}

#[test]
fn panel_one_then_panel_zero_gets_no_fresh_week() {
    // Three Panel=1 claims a day for 3 days, then Panel=0: the Panel=1 reservations stay in the long view for 7 days, so the Panel=0
    // claims get only the remainder of 7·daily, never a fresh week.
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    let mut total = 0;
    for now in (1..7 * DAY).step_by(10) {
        w.tick(now);
        let mode = if now < 3 * DAY { PalwHoldModeV1::PanelOne } else { PalwHoldModeV1::PanelZero };
        if w.try_claim(1, capital, now, mode, 0).is_ok() {
            total += 1;
        }
    }
    assert_eq!(total, 21, "Panel=1 then Panel=0: exactly 7·daily in the 7 days, not 9 + 21");
    assert_eq!(w.accepted_in(1, 0, 3 * DAY), 9);
}

#[test]
fn mass_parallel_submission_is_cut_by_the_burst_view() {
    // 30,000 BILI → daily 90 claims; burst window 72 DAA, factor 4 → ⌈90·72·4/720⌉ = 36 claims in any 72 DAA (an ADDITIONAL limit).
    let p = policy(1);
    let hold = p.hold.unwrap();
    let capital = 30_000 * KAS;
    let burst = palw_bond_hold_burst_caps_v1(&p, &hold, capital);
    assert_eq!(burst.claims, 36);
    let mut w = World::new(p);
    w.tick(5);
    let mut ok = 0;
    let mut burst_refusals = 0;
    for _ in 0..1_000 {
        match w.try_claim(1, capital, 5, PalwHoldModeV1::PanelZero, 0) {
            Ok(()) => ok += 1,
            Err(PalwBudgetRefusalV1::Burst { .. }) => burst_refusals += 1,
            Err(other) => panic!("only the burst view refuses here: {other:?}"),
        }
    }
    assert_eq!(ok, 36);
    assert_eq!(burst_refusals, 1_000 - 36);
    // The burst view returns its room at d + W_burst; the long view keeps the 36 for 7 days.
    w.tick(5 + hold.burst_window_daa);
    assert!(w.try_claim(1, capital, 5 + hold.burst_window_daa, PalwHoldModeV1::PanelZero, 0).is_ok());
    assert_eq!(w.s.bond_row(&bond(1)).unwrap().window.claims, 37, "nothing of the long view was returned early");
}

#[test]
fn period_boundary_hammering_never_exceeds_any_rolling_window() {
    // A bond hammers every DAA in both modes alternately, densest right at every window edge. Every rolling 1-day window holds at most
    // the burst-bounded mix and every rolling 7-day window at most 7·daily; the Panel=1 claims of any rolling day at most daily.
    let p = policy(1);
    let capital = 3_000 * KAS; // daily 9
    let mut w = World::new(p);
    let mut log: Vec<(u64, PalwHoldModeV1)> = Vec::new();
    for now in 1..21 * DAY {
        w.tick(now);
        let edge = now % DAY < 3 || now % DAY > DAY - 3;
        let tries = if edge { 20 } else { 1 };
        for t in 0..tries {
            let mode = if (now + t) % 2 == 0 { PalwHoldModeV1::PanelOne } else { PalwHoldModeV1::PanelZero };
            if w.try_claim(1, capital, now, mode, 0).is_ok() {
                log.push((now, mode));
            }
        }
    }
    for start in 0..14 * DAY {
        let week = log.iter().filter(|(d, _)| *d >= start && *d < start + 7 * DAY).count();
        assert!(week <= 63, "rolling 7-day window at {start}: {week} > 63");
        let p1_day = log.iter().filter(|(d, m)| *m == PalwHoldModeV1::PanelOne && *d >= start && *d < start + DAY).count();
        assert!(p1_day <= 9, "rolling day at {start}: {p1_day} Panel=1 claims > 9");
    }
    w.s.check_consistency().expect("consistent");
}

#[test]
fn honest_minimal_miner_sustains_its_daily_cap() {
    // Reachability (guidance 9): a minimal honest miner whose compute + lottery + submission + verification yields one accepted claim
    // per 240 DAA (3 a day) on capital whose daily cap is 3, in Panel=1, never sees a budget refusal over 30 days — REAL does not stop
    // for lack of cap while demand, equipment and verification exist. (The real pipeline is measured by the harness in the plan doc;
    // this pins the engine half: a cap equal to the sustained rate is usable to the last claim.)
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    let mut refused_for_cap = 0;
    let mut accepted = 0;
    for now in 1..30 * DAY {
        w.tick(now);
        if now % 240 == 0 {
            match w.try_claim(1, capital, now, PalwHoldModeV1::PanelOne, 0) {
                Ok(()) => accepted += 1,
                Err(_) => refused_for_cap += 1,
            }
        }
        if now % 50 == 0 {
            w.close_all_open(now);
        }
    }
    assert_eq!(refused_for_cap, 0, "the cap never stops a sustained honest rate equal to it");
    assert_eq!(accepted, (30 * DAY - 1) / 240);
}

#[test]
fn concurrent_liability_cap_binds_beside_the_period_caps() {
    // φ = 500‰ of 10,000 BILI: open liability ≤ 5,000 BILI. Claims of 2,000 BILI liability: two open at once, the third refused for
    // collateral even though the period caps have room; closing one frees collateral but NOT the period capacity.
    let mut p = policy(1);
    p.hold.as_mut().unwrap().concurrent_liability_permille = 500;
    p.validate().unwrap();
    let capital = 10_000 * KAS;
    let mut w = World::new(p);
    w.tick(1);
    w.try_claim(1, capital, 1, PalwHoldModeV1::PanelZero, 2_000 * KAS).unwrap();
    w.tick(100);
    w.try_claim(1, capital, 100, PalwHoldModeV1::PanelZero, 2_000 * KAS).unwrap();
    w.tick(200);
    assert_eq!(w.try_claim(1, capital, 200, PalwHoldModeV1::PanelZero, 2_000 * KAS), Err(PalwBudgetRefusalV1::LiabilityExceedsCapital));
    w.close_all_open(201);
    w.tick(300);
    w.try_claim(1, capital, 300, PalwHoldModeV1::PanelZero, 2_000 * KAS).expect("collateral freed by the terminal claims");
    assert_eq!(w.s.bond_row(&bond(1)).unwrap().window.claims, 3, "closing returned no period capacity");
    assert!(w.s.liability_holds(&bond(1), 301), "the liability hold keeps the capital after close (collateral ≠ capacity)");
}

#[test]
fn split_capital_never_beats_the_whole_in_any_mode() {
    let p = policy(1);
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        let mut whole = World::new(p.clone());
        let mut parts = World::new(p.clone());
        let (mut a, mut b) = (0, 0);
        for now in 1..7 * DAY {
            whole.tick(now);
            parts.tick(now);
            // 10,999 BILI whole (daily 32) against ten bonds of 1,099.9 BILI (daily 3 each): the floors only ever lose to the split.
            if whole.try_claim(1, 10_999 * KAS, now, mode, 0).is_ok() {
                a += 1;
            }
            for k in 0..10u8 {
                if parts.try_claim(10 + k, 1_099 * KAS + 90_000_000, now, mode, 0).is_ok() {
                    b += 1;
                }
            }
        }
        assert!(b <= a, "{mode:?}: ten bonds of a tenth ({b}) never pass the whole ({a}) in the period");
    }
}

#[test]
fn rho_moves_claims_only_in_both_modes() {
    for rho in [1u32, 100, 1_000] {
        let p = policy(rho);
        let hold = p.hold.unwrap();
        for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
            let caps = palw_bond_hold_period_caps_v1(&p, &hold, 1_000 * KAS, mode);
            let days = mode.days(&hold);
            assert_eq!(caps.claims, 3 * rho as u64 * days);
            assert_eq!(caps.block_units, 3 * PALW_BUDGET_BLOCK_UNIT_V1 * days, "B never moves with rho");
            assert_eq!(caps.reward_sompi, 30 * KAS * days, "R never moves with rho");
            assert_eq!(caps.final_weight, 3_000 * days as u128, "F never moves with rho");
        }
    }
}

#[test]
fn restart_and_reorg_restore_the_same_views() {
    use borsh::BorshDeserialize;
    let p = policy(1);
    let capital = 2_000 * KAS;
    let mut w = World::new(p.clone());
    for now in (1..2 * DAY).step_by(37) {
        w.tick(now);
        let _ = w.try_claim(1, capital, now, PalwHoldModeV1::PanelOne, 0);
    }
    // Restart: the carried bytes restore the same state (shadow rows included) and the same root.
    let bytes = borsh::to_vec(&w.s).unwrap();
    let restored = PalwBondBudgetStateV1::try_from_slice(&bytes).unwrap();
    assert_eq!(restored, w.s);
    assert_eq!(restored.root_parts(), w.s.root_parts());
    // Reorg: one block's writes reverted row by row give back the parent state exactly.
    let before = w.s.clone();
    let mut j = Vec::new();
    w.s.release_due(2 * DAY, &mut j);
    w.next += 1;
    let _ = w.s.reserve_claim_moded_v1(
        &p,
        capital,
        id(1, w.next),
        bond(1),
        None,
        2 * DAY,
        ask(0),
        PalwBudgetOriginV1::Attempt,
        PalwHoldModeV1::PanelOne,
        &mut j,
    );
    for write in j.iter().rev() {
        if let PalwBudgetWriteV1::Row { table, key, old, new } = write {
            w.s.apply_row(*table, key, old, new, true).expect("the journal reverts");
        }
    }
    assert_eq!(w.s, before, "the reverted block leaves no shadow and no release behind");
    w.s.check_consistency().expect("consistent");
}

#[test]
fn the_v3_policy_pins_the_users_spans() {
    let good = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    good.validate().expect("the baseline form validates");
    assert_eq!(PALW_BOND_HOLD_DAA_PER_DAY_V1, 720);
    let bad = |f: &dyn Fn(&mut PalwBondBudgetPolicyV1)| {
        let mut p = good.clone();
        f(&mut p);
        p.validate().is_err()
    };
    assert!(bad(&|p| p.hold.as_mut().unwrap().panel_one_days = 2));
    assert!(bad(&|p| p.hold.as_mut().unwrap().panel_zero_days = 1));
    assert!(bad(&|p| p.hold.as_mut().unwrap().daa_per_day = 576));
    assert!(bad(&|p| p.window_daa = 400), "the drill-interim W = 400 is not a V3 window");
    assert!(bad(&|p| p.hold.as_mut().unwrap().burst_factor_permille = 999));
    assert!(bad(&|p| p.hold = None), "V3 needs the hold");
    // V2 bytes are untouched (the hold is serialized for V3 only).
    let v2 = PalwBondBudgetPolicyV1::unapproved_probe_v1();
    let mut v2_with = v2.clone();
    v2_with.hold = Some(PalwBondHoldPolicyV1::baseline_unapproved_v1());
    assert_eq!(borsh::to_vec(&v2).unwrap(), borsh::to_vec(&v2_with).unwrap());
    assert!(v2_with.validate().is_err(), "a V2 policy carries no hold");
    assert_ne!(good.digest(), PalwBondBudgetPolicyV1 { hold: None, ..good.clone() }.digest());
}

#[test]
fn after_a_halt_and_heartbeat_only_spans_the_budget_releases_and_real_resumes() {
    // Guidance item 7/9 (restart/reorg/halt recovery; heartbeat-only → REAL): a bond fills its Panel=1 day, the chain then runs
    // heartbeat-only (no REAL acceptance) and halts; the first block after resumption — whatever DAA jump it carries — releases
    // every reservation whose clock passed BEFORE its admissions, and a REAL claim is admitted again. The floor state machine,
    // Idle through the heartbeat-only span, moves to Normal on the first BLUE REAL acceptance (no cooldown).
    use kaspa_consensus_core::palw_real_share_v1::{PalwFloorModeV1, PalwFloorStateV1, PalwRealAcceptedV1, palw_floor_step_v1};
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    w.tick(10);
    w.try_claim(1, capital, 10, PalwHoldModeV1::PanelOne, 0).unwrap();
    w.try_claim(1, capital, 10, PalwHoldModeV1::PanelOne, 0).unwrap();
    w.tick(90);
    w.try_claim(1, capital, 90, PalwHoldModeV1::PanelOne, 0).unwrap();
    assert!(w.try_claim(1, capital, 91, PalwHoldModeV1::PanelOne, 0).is_err(), "the day is full");
    // The three claims reach their fate before the halt (PESG §6, 16cbef7ae: an OPEN claim keeps its Final-weight reservation in the
    // unsettled weight however old it is — a halt releases clocks, not open claims).
    w.close_all_open(100);
    // Heartbeat-only, then a halt: the next block arrives at DAA 10 + 9 days (a sparse jump).
    let mut floor = PalwFloorStateV1::default();
    for daa in (100..9 * DAY).step_by(50) {
        floor = palw_floor_step_v1(floor, daa, None);
    }
    assert_eq!(floor.mode, PalwFloorModeV1::Idle, "heartbeat-only spans leave the floor Idle");
    let resume = 10 + 9 * DAY;
    w.tick(resume);
    assert_eq!(w.s.bond_row(&bond(1)).map(|r| r.window.claims).unwrap_or(0), 0, "every clock that passed during the halt released");
    w.try_claim(1, capital, resume, PalwHoldModeV1::PanelOne, 0).expect("REAL is admitted again at once");
    floor = palw_floor_step_v1(floor, resume, Some(PalwRealAcceptedV1 { blue: true }));
    assert!(matches!(floor.mode, PalwFloorModeV1::Normal { last_blue } if last_blue == resume), "REAL resumes the Normal state");
    w.s.check_consistency().expect("consistent");
}

#[test]
fn exit_and_re_registration_wait_for_the_long_view_and_the_liability() {
    // RFC-0015 §8.3.4 / ADR-0176 D4: the capital that consumed capacity cannot leave (and come back as a fresh bond with a fresh
    // ledger) while any reservation sits in its long view — 7 days even for a Panel=1-only bond (decision doc C2, lead default) — nor
    // while its liability holds it. A slash lowers the caps of the next claims; the old consumption stays counted beside them.
    let p = policy(1);
    let mut w = World::new(p.clone());
    w.tick(10);
    w.try_claim(1, 2_000 * KAS, 10, PalwHoldModeV1::PanelOne, 100 * KAS).unwrap();
    w.close_all_open(20);
    for now in [21, DAY + 10, 6 * DAY] {
        assert!(w.s.withdrawal_holds(&bond(1), now), "the capital is held at {now}");
    }
    assert!(!w.s.window_holds(&bond(1), 10 + 7 * DAY), "the long view ends at d + 7 days");
    w.tick(10 + 7 * DAY);
    assert!(!w.s.withdrawal_holds(&bond(1), 10 + 7 * DAY), "and the liability (20 + H_L) ended long before");
    // A slash: the same bond at half its capital — its daily caps halve, the consumption still in its views stays counted.
    let mut v = World::new(p);
    v.tick(1);
    for _ in 0..2 {
        v.try_claim(2, 2_000 * KAS, 1, PalwHoldModeV1::PanelOne, 0).unwrap();
    }
    v.tick(100);
    assert!(v.try_claim(2, 1_000 * KAS, 100, PalwHoldModeV1::PanelOne, 0).is_ok(), "3/day after the slash: 2 used + 1");
    assert!(v.try_claim(2, 1_000 * KAS, 101, PalwHoldModeV1::PanelOne, 0).is_err(), "the old consumption counts against the new cap");
}

// ---- C2 (user decision 2026-10-15): the 7-day exit hold; exit, re-registration and mode switches never reset long liabilities or
// ---- capacity. TEST SOURCES ONLY (not run by the lane that wrote them).

#[test]
fn c2_panel_one_only_bond_recovers_issuance_in_a_day_but_exits_after_seven() {
    // A bond that only ever made Panel=1 claims: its day view frees after one day (issuance capacity recovers), but its long view —
    // the exit hold — keeps the capital for 7 days (C2: kept at 7 days; the 1-day recovery is issuance, never release).
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    w.tick(10);
    // The burst view (2 per 72 DAA at this capital) spreads the day's three: two at DAA 10, the third at 83.
    for _ in 0..2 {
        w.try_claim(1, capital, 10, PalwHoldModeV1::PanelOne, 0).unwrap();
    }
    w.tick(83);
    w.try_claim(1, capital, 83, PalwHoldModeV1::PanelOne, 0).unwrap();
    assert!(w.try_claim(1, capital, 83, PalwHoldModeV1::PanelOne, 0).is_err(), "the day's three are used");
    w.tick(10 + DAY);
    w.try_claim(1, capital, 10 + DAY, PalwHoldModeV1::PanelOne, 0).expect("Panel=1 issuance recovers after one day");
    // Every claim terminal at once (a colluding immediate Final): the liability hold ends after H_L; the long view does not.
    w.close_all_open(10 + DAY);
    for now in [10 + DAY + w.p.liability_hold_daa + 1, 10 + 3 * DAY, 10 + 7 * DAY - 1] {
        assert!(w.s.withdrawal_holds(&bond(1), now), "exit held at {now}: the long view is 7 days for a Panel=1-only bond");
    }
    assert_eq!(PALW_BOND_EXIT_HOLD_DAYS_V1, 7);
}

#[test]
fn c2_a_mode_switch_never_resets_the_kept_liability() {
    // A Panel=0 claim closes keeping its liability (formula v2) until `until`; the bond then switches to Panel=1. The liability view
    // keeps counting in the concurrent-collateral check across the switch, and only its own clock ends it.
    let mut p = policy(1);
    p.hold.as_mut().unwrap().concurrent_liability_permille = 500;
    p.validate().unwrap();
    let capital = 10_000 * KAS;
    let mut w = World::new(p);
    w.tick(1);
    w.try_claim(1, capital, 1, PalwHoldModeV1::PanelZero, 4_000 * KAS).unwrap();
    let until = 50 * DAY;
    let mut j = Vec::new();
    w.s.close_keeping_liability_v1(&id(1, 1), until, &mut j);
    assert_eq!(w.s.kept_liability_v1(&bond(1)), (4_000 * KAS) as u128);
    w.tick(10 * DAY);
    // 4,000 kept + 2,000 new > 5,000 (φ = 500 ‰): refused in Panel=1 exactly as it would be in Panel=0.
    assert_eq!(
        w.try_claim(1, capital, 10 * DAY, PalwHoldModeV1::PanelOne, 2_000 * KAS),
        Err(PalwBudgetRefusalV1::LiabilityExceedsCapital)
    );
    assert_eq!(
        w.try_claim(1, capital, 10 * DAY, PalwHoldModeV1::PanelZero, 2_000 * KAS),
        Err(PalwBudgetRefusalV1::LiabilityExceedsCapital)
    );
    assert_eq!(w.s.kept_liability_v1(&bond(1)), (4_000 * KAS) as u128, "the switch kept the liability");
    w.tick(until + 1);
    assert_eq!(w.s.kept_liability_v1(&bond(1)), 0, "only the liability's own clock ends it");
}

#[test]
fn c2_the_liability_hold_only_extends() {
    // Closing a later claim with an EARLIER hold never shortens the bond's liability hold (a re-close, a mode switch or a colluding
    // early Final cannot reset it).
    let p = policy(1);
    let capital = 10_000 * KAS;
    let mut w = World::new(p);
    w.tick(1);
    w.try_claim(1, capital, 1, PalwHoldModeV1::PanelZero, 100 * KAS).unwrap();
    w.try_claim(1, capital, 1, PalwHoldModeV1::PanelOne, 100 * KAS).unwrap();
    let mut j = Vec::new();
    w.s.close_at(&id(1, 1), Some(20 * DAY), &mut j);
    w.s.close_at(&id(1, 2), Some(2 * DAY), &mut j);
    assert!(w.s.liability_holds(&bond(1), 19 * DAY), "the later hold stands");
    assert!(!w.s.liability_holds(&bond(1), 20 * DAY + 1));
}

#[test]
fn c2_exit_then_re_registration_never_beats_the_long_view() {
    // The capital of bond 1 is consumed in Panel=1 (3/day), held for 7 days, then re-registered as bond 2 (a fresh key, a fresh
    // ledger). Over any 7-day window the SAME capital never issues more than 7·daily, because bond 2 can only exist once bond 1's long
    // view released (the withdrawal gate), so the two bonds' issuing windows never overlap.
    let p = policy(1);
    let capital = 1_000 * KAS;
    let mut w = World::new(p);
    let mut issued: Vec<u64> = Vec::new();
    for now in (1..7 * DAY).step_by(10) {
        w.tick(now);
        if w.try_claim(1, capital, now, PalwHoldModeV1::PanelOne, 0).is_ok() {
            issued.push(now);
        }
    }
    // Every claim terminal (its liability hold starts); only the long view keeps the capital after that.
    w.close_all_open(7 * DAY);
    // The earliest DAA bond 1's capital can leave.
    let exit = (1..=15 * DAY).find(|now| !w.s.withdrawal_holds(&bond(1), *now)).expect("the hold ends");
    let last = *issued.last().expect("bond 1 issued");
    assert!(exit >= last + 7 * DAY, "exit no earlier than the long view of the last reservation");
    for now in (exit..exit + 7 * DAY).step_by(10) {
        w.tick(now);
        if w.try_claim(2, capital, now, PalwHoldModeV1::PanelOne, 0).is_ok() {
            issued.push(now);
        }
    }
    for start in (0..exit + 7 * DAY).step_by(DAY as usize) {
        let in_window = issued.iter().filter(|d| **d >= start && **d < start + 7 * DAY).count();
        assert!(in_window <= 21, "the same capital issued {in_window} claims in the 7 days from {start}");
    }
}
