# ADR-0160 stage 3 (rcore/cap-s1) — status: gate PASS

> **Status (2026-10-02).** The DAA-1,500 release was never rolled out. The same eight-entry package
> (`PALW_T12_CAPACITY_RHO10_FENCES_V1`) is testnet-12's third post-launch list, armed at **DAA 1,700**
> (`PALW_T12_POST_LAUNCH_FENCE_V3_DAA` in `consensus/core/src/config/params.rs`), and the dormant-package
> test is now `palw_capacity_stage3_rho10.rs::testnet12_ships_the_package_armed_at_1700_over_the_daa1300_release`.
> Read "1,500" below as that flag day. Fingerprints quoted below are the values at the time; the current
> identity is in [`release.json`](../../release.json).

Branch `rcore/cap-s1` (off `c3dbaee3c`, rcore/int-5 merged at `afb2be54d`). Commit `b21791934`. Nothing
armed: testnet-12 as shipped is the DAA-1,300 release to the id — params `24e1aec3…`, identity
`5de80e64…`, schedule `d263d7f2…`.

## What rides the DAA-1,500 flag day from this lane (the user's decision of 2026-09-27)

The ρ = 10 architecture of stages 1, 2 and 4 plus stage 3's FIXED ρ = 10 — eight entries, each a named
`PalwPostLaunchFenceV1` constant, so the flag day's list takes each in one line (the order below; every
entry at the list's height, through its own `set`, mirrors included):

| entry (constant) | fence name | what it arms |
|---|---|---|
| `PALW_T12_CAPACITY_WEIGHT_CAP_V1` | `palw_capacity_weight_cap` | F-W: J-1, reservation `⌈w/ρ⌉` |
| `PALW_T12_CAPACITY_ESCROW_AT_LICENCE_V1` | `palw_capacity_escrow_at_licence` | F-E: escrow slot `m_c`, void keeps its obligation |
| `PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1` | `palw_capacity_aggregate_liability` | F-L: one step `(H, ρ 10, q 250‰)` — credited |
| `PALW_T12_CAPACITY_BATCH_LICENCE_V1` | `palw_capacity_batch_licence` | F-B |
| `PALW_T12_CAPACITY_VERIFY_ROOM_V1` | `palw_capacity_verify_room` | F-R (room ≤ ρ × shipped) |
| `PALW_T12_CAPACITY_AUDIT_DOOR_V1` | `palw_capacity_audit_door` | F-Q: credited claims `Final` only through `k_aud` receipts |
| `PALW_T12_CAPACITY_ISSUANCE_SLOTS_V1` | `palw_capacity_issuance_slots` | F-S (burst = one DAA's refill) |
| `PALW_T12_CAPACITY_NETWORK_ROOM_V1` | `palw_capacity_network_room` | F-N: network level + split-neutral fair share |

`PALW_T12_CAPACITY_RHO10_FENCES_V1` is the eight together. They arm together: `validate_palw_v2`
refuses a list that leaves out any one of them except F-N (F-L's credited step needs F-Q; F-Q needs F-L,
F-B; F-E needs F-W, F-L; F-S needs F-W, F-L, F-E; F-N needs F-R, F-S) — **F-N is the one entry the
validator does not force; the flag-day list must carry it** (it is the stage-4 property). The stage-1
proof list `PALW_T12_CAPACITY_FENCES_V1` keeps F-L at ρ = 1 (its drill and gates). Armed at 1,500 over
the release: the params and schedule ids move, the identity does not, the fork id refuses a node without
them from 1,500; `set(None)` gives the release back to the id. **Integration note:** the flag day's build
flips `testnet12_ships_the_package_dormant` (and the stage 1/2/4 `*_is_t12_only` pins — `t12-repin.sh`).

**Not riding 1,500:** the ready variants (below), stages 5–7.

## Stage 3's own changes

* **F1 — `G` at full weight.** F-W reserves `⌈w/ρ⌉`; every `G` read the reservation as the weight term,
  so at ρ > 1 a conviction's `G`, the seat lock and the held accuser surplus would have fallen by ρ (and
  AS-2's credited discount applied twice). Now `G` reads `reserved × ρ` (`PalwCapacityGainScaleV1`) in the
  fold's claim `G`, the frozen `g_res`, the held surplus / accuser ledger (seat filter, admission,
  producer facts), the fraud facts (weight recovered from the scaled reservation: never below today's)
  and the shadow. Identity at ρ = 1 (every shipped preset: byte for byte).
* **D-18 — the floor alone is credited.** The audit door credits only the base class (a CPU replay every
  operator card runs); the escrow's sampled `m*(q)` credit is withheld from any class the door does not
  credit. A model class is priced as today at any ρ (escrow `E`, seat duty and lock), its room ≤ ρ ×
  shipped under F-R.
* **Ready variants** (their own later flag days, each F-L's appended step and fork-id slot):
  `PALW_T12_CAPACITY_RHO25_STEP_2_V1` then `PALW_T12_CAPACITY_RHO100_STEP_3_V1`, or
  `PALW_T12_CAPACITY_RHO100_STEP_2_V1` straight after ρ 10. Arming a step before the flag days it builds
  on panics (never a silently dropped ρ); `None` drops the step and every later one; moving the ρ = 10
  entry keeps the appended steps; a step at or below the previous height is refused. Two variants at one
  height share the schedule id and differ in the params id.
* `validate`: an F-L value at `never()` credits nothing (was: refused as a credited step without F-Q).

## Gate — PASS (`palw_capacity_stage3_rho10.rs`, 12 tests)

* entries: the ρ = 10 list is the proof list with F-L at ρ 10 (every other `set` identical); armed at
  1,500: validate, ids, fork id, `set(None)` / `never()`, the partial-list refusals; variants as above.
* crossing at 1,500 beside a release twin fed the same blocks: below 1,500 the release's price to the
  sompi and `Final` without an audit; at 1,500 credited — `w` 10,752,660 → 1,075,266, slot `⌈E/10⌉`,
  `G` the twin's — waits for its audit, `Final` through it, the producer keeps its collateral.
* F1/D-18 at ρ 10/25/100: floor `G` 320,095,402,740 today → +0 / +15 / +40 sompi (the ceiling's
  rounding, < ρ), slot 3,200.8 → 320.1 / 128.0 / 32.0 MSK, credited; 8k `G` and slot unchanged, not credited.
* invariants at ρ 10/25/100: honest-no-loss (five credited claims audited and `Final`, every bond whole,
  nobody frozen; an 8k claim `Final` as today); liability (a receipted fraud takes the whole bond and
  excludes its auditor; an unaudited credited claim is never paid); J-1 + `N_out` under 12 attempts a DAA
  for 25 DAA at 13k (20 / 50 / 200 issued = `N_out`) with a rewind replayed root for root; no-free-void
  (the void holds its slot, charged and paid nothing; serving pays); a credited tape reverts, IBDs,
  restarts at every tip and reorgs to a reversed-receipt fork root for root.
* D-13 at ρ 10/25/100: the 2M row admits one claim and refuses a second bond's while it is live (its
  row's static `max_inflight_claims`, which no ρ scales), never credited, escrow and `G` today's; only
  its reservation is `⌈w/ρ⌉` (F-W, every attempt's).

## Battery on the merged tree (int-5 + stage 3)

* core `--lib --tests`: 207 binaries, **3,843 passed, 0 failed** (41 ignored) — stage-1 invariants and
  state diff, stage-2 gate, stage-4 network and every `*_is_t12_only` pin among them; re-run after the
  clippy fix: stage 1/2/3/4 gates 9 + 10 + 12 + 6 passed.
* consensus `--lib`: 540 passed, 1 failed — `t12_capacity_shadow::s_t5_a8` (card 0's `frozen_would_be`
  depends on which seats the node pipeline draws for B): **flaky on the same binary alone (2 of 3 runs
  pass)**, and stage 3 cannot reach it (the test runs the shipped ruleset, every capacity fence dormant:
  the gain scale is the identity). Pre-existing (stage 4's battery had it too).
* kaspad `--lib` 431 passed; rpc-core / grpc-core / rpc-service / misaka-cli / palw-extension 249 passed;
  testing-integration `--tests` check clean (all with this worktree's protos regenerated).
* clippy (warn mode) on the eight crates: **0 findings on branch lines** (154 pre-existing on the base).
