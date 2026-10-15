# ADR-0176 — cap derivation and measurement plan (2026-10-15)

**Status:** plan + harness written; **nothing measured, nothing run, no value proposed.** The harness sources are on
`integ/adr-0162-0178`; running them is the validation phase. Production values come from this procedure and the adversarial
tests, never from current code values or past examples ("100 claims / 7 days", 350 BILI, `rho = 10,000`, `W = 400` are not
official).

Sources: [ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) (amendments of 2026-10-15),
[RFC-0015 §8.3–8.5](../../rfc/0015-panel-free-permissionless-verification.md), the user's guidance of 2026-10-15
(ユーザー方針 2026-10-15) and ruling (ユーザー判断 2026-10-15: bond式の再設計).

## 0. Primary condition

With the same eligible bond capital, the same verification mode and the same period, skipping computation to forge claims fast
must not obtain more claim slots, reward blocks, rewards or Final weight than honest computation — **and** the caps must be set
so a reference honest miner can practically use them. "Caps equal" is not "the honest miner reaches the cap": both are tested,
plus that REAL production does not stop for lack of cap while demand, equipment and verification exist.

## 1. What is tuned (together, never the hold days alone)

| Target | Code | Notes |
|---|---|---|
| Per-day base caps `q, b, r, f` (per capital unit) | `PalwBondBudgetPolicyV1` V3: `claims_per_unit`, `block_units_per_unit`, `reward_per_unit_sompi`, `final_weight_per_unit` (per DAY) | Panel=1 period caps = `daily(C)` over 1 day; Panel=0 = `7·daily(C)` over 7 days (equalized per day) |
| Per-claim rights and reservation | `palw_bond_formula_v2` (`PalwReservationSplitV2`) | rights = period totals split over the period's claims; reservation = weight + right + collectable-loss top-up + court/DA/retention |
| Concurrent-liability cap | `PalwBondHoldPolicyV1::concurrent_liability_permille`; formula `u` | `u = 500 ‰` is the provisional starting point — **not tuned first** |
| Short-term burst / rate cap | `burst_window_daa`, `burst_factor_permille` | additional limit over the same ledger; derived from DAG parallelism and measured submission patterns |

Model-allocation increases never raise a bond's absolute caps (`palw_bond_budget_caps_v1` reads only the bond's capital).

## 2. Collateral sizing

`concurrent reserved principal ≈ λ · a · H` (daily acceptances × per-claim liability × average ACTUAL hold) plus margin and the
bond's other liabilities (seat locks, accusation reserves, outstanding liability). A longer hold needs more concurrent
collateral, so per-claim right, reservation and reward are tuned together (ADR-0176 D5). No fictitious collateral and no double
reservation: the harness counts each ledger once (`palw_bond_formula_concurrent_admits_v2`, the budget's open + liability views).

## 3. Stage 1 — the honest minimal configuration (measure)

* **Setup:** Qwen2.5-1.5B A16, a fixed artifact / context / job, the reference hardware (record CPU/GPU model, memory, driver,
  OS, node build hash), the minimum producer bond of the target ruleset (13,000 BILI on testnet-12 today; take the target
  ruleset's), one bond, Panel=1 and Panel=0 runs separately (Panel=0 only where its fences are armed in an isolated network).
* **Driver:** `scripts/drills/adr0176_stage1_measure.py --bond <txid>:<i> --mode {1|0} --capital-bili <C> --duration-daa
  <≥ 3 days, 2160> --node-log <kaspad.log> --out <dir>` on the node host (local wRPC-JSON). `--self-test` checks the summary
  logic without a node.
* **Per claim, per stage:** compute → lottery win → claim acceptance (REAL block) → verification (bound / licensed) → Final →
  settlement (vesting stage) → bond capacity recovery (`capacityRecoveryDaa` from `getPalwClaims.reservationSplit` where the
  formula fence is armed; `accepted + W_M` otherwise).
* **Report:** the SUSTAINED accepted / REAL / Final rates (over the whole span, not the peak), lottery wait, refusals by reason,
  verification wait, Final delay, reservation recovery; the time where compute and submission were possible but REAL stopped
  only for lack of bond capacity (`capStallSpans`); heartbeat-dependent time (`heartbeatOnlySpans`); post-recovery adoption.
* **Heartbeat:** never aim for zero heartbeat, never weaken the heartbeat clock or fallback rules to improve the numbers. A REAL
  event that moves the floor state is an attempt-lane block that passed the checks and wrote a claim (`palw_real_share_v1`);
  riders are not counted the same. Recovery from heartbeat-only spans to REAL is its own mandatory test (§5).

## 4. Stage 2 — sweep (simulate on the real engine)

`palw_bond_cap_sweep_v1` runs the budget engine (V3 holds, burst view, concurrent liability) under the measured profile and each
candidate, and runs the same bond as a fast forger:

```text
ADR0176_PROFILE="<intervals>;<final delays>;<capital BILI>;<mode>" \
  cargo test -p kaspa-consensus-core --test adr0176_cap_sweep -- --ignored stage2_sweep_from_a_measured_profile --nocapture
```

| Region | Meaning | Typical cause |
|---|---|---|
| `TooLow` | honest miner runs out of cap; REAL gaps | per-day caps below the sustained honest rate |
| `TooHigh` | honest utilization < 80 % of its cap | only a forger could use the rest |
| `CollateralBound` | reservation × concurrent claims > `φ·C` | reservation too big for the hold |
| `UnderCollectable` | reservation < collectable loss | reservation too small |
| `VerificationShort` | Finals/day < accepted/day | verification capacity short: Final backlog |
| `Feasible` | none of the above | |

Pick the region where the honest miner uses the cap, collateral suffices and REAL gaps stay the pipeline's own. **If none
exists, report which constraints conflict** (the sweep returns them) — never invent numbers. Other models: report how much they
can use under the same economic caps; no per-model guarantee is required.

## 5. Stage 3 — adversarial and boundary tests (sources written, not run)

| Test (guidance item 9 / ruling item 7) | Source |
|---|---|
| accounting cap: forgery, colluding Valid, mass parallel submission never exceed the same bond's caps | `adr0176_mode_hold_v1.rs` `honest_and_fast_forger_reach_the_same_counts`, `mass_parallel_submission_is_cut_by_the_burst_view`; `adr0176_cap_sweep.rs` `the_fast_forger_never_gets_more_than_the_cap` |
| honest-miner reachability | `adr0176_mode_hold_v1.rs` `honest_minimal_miner_sustains_its_daily_cap`; `adr0176_cap_sweep.rs` `minimal_configuration_sustains_near_cap_with_bounded_gaps` |
| network continuity (no long REAL stop from too-low caps) | `adr0176_cap_sweep.rs` `a_cap_below_the_honest_rate_is_too_low_and_stalls_real` (detection), Stage 1 `capStallSpans` (measurement) |
| colluding immediate Valid cannot shorten Panel=1's day | `adr0176_mode_hold_v1.rs` `colluding_immediate_valid_does_not_shorten_panel_one_day`; `adr0176_bond_formula_v2.rs` `rotation_by_early_valid_is_blocked` |
| Panel=0/1 switching cannot reset history, reservations or liability | `panel_zero_then_panel_one_gets_no_fresh_day`, `panel_one_then_panel_zero_gets_no_fresh_week` |
| mass parallel + period-boundary hammering within rolling and burst caps | `period_boundary_hammering_never_exceeds_any_rolling_window` |
| restart / reorg / halt recovery keeps reservations and recovery times; REAL resumes from heartbeat | `restart_and_reorg_restore_the_same_views`, `after_a_halt_and_heartbeat_only_spans_the_budget_releases_and_real_resumes` |
| decomposition consistency; reservation ≥ collectable loss at any ρ; kernel escrow separation; per-type pricing; `u` and period caps both enforced | `adr0176_bond_formula_v2.rs` |

## 6. The four pass conditions (per claim type)

1. **Collateral accounting** — no double use across concurrent claims, courts and exit; the stated liability is collectable.
2. **Period caps** — honest and fast forgery get equal Q/B/R/F; a colluding Valid cannot accelerate recovery.
3. **Practicality** — the reference Qwen2.5-1.5B A16 on the reference hardware and the minimum producer bond sustain the
   planned allowance.
4. **Continuous REAL** — no long REAL stall caused only by reservation size or period caps while demand, compute and
   verification exist.

Each condition is reported PASS / FAIL / UNKNOWN; UNKNOWN is never PASS. For each claim type the report also shows the
per-claim reservation split into weight / escrow-right / other liabilities with their actual hold times, and `λ·a·H`.
