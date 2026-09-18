# The DAA clock under the 6,001 bundle — lane inventory, window arithmetic, verdict (2026-09-18)

Read-only pass over `feat/palw-exec-lane-and-validator-retirement` @ `d819d999`, before any rollout.
Every cell below is a line of code or a measurement, not a setting.

## 1. Every block lane, and what it does to the three clocks

| lane | `pow_algo_id` | advances the DAA score? | priced by `bits` (in the difficulty window)? | blue score / blue work | block level |
|---|---|---|---|---|---|
| hash PoW (BLAKE2b‖SHA3) — **the anchor** | 3 | **yes** | **yes** | blue; `calc_work(bits)` | derived from the digest |
| PALW attempt (committed / exec) | 6 / 9 | **yes** — not in `mergeset_non_daa` | yes below the single lottery; **no from 6,001** (`algo_id_is_priced_by_bits_v3`) | blue; **2^20 fixed** past `palw_attempt_work` (armed on t11) | 0 from 6,001 (`calc_block_level_check_pow_layer0_v2`) |
| receipt spend | 7 | **yes** — not in `mergeset_non_daa` | no since ADR-0083 (`palw_receipt_rows_unpriced`) | **never blue, work 0** (`algo_id_carries_no_chain_position`) | none |
| heartbeat | 8 | **yes** | no | blue; work ε = 1 past the heartbeat lane; ≤ 4 per mergeset | none |
| execution round (ADR-0125) | 10 | **no** — `is_round_block` puts it in `mergeset_non_daa` past the lane fence | no | never blue, work 0 | none |

Sources: `consensus/src/processes/difficulty.rs::calc_daa_score_and_mergeset_non_daa_blocks` (the only
lane it excludes is the round lane), `consensus/src/processes/window.rs::sampled_mergeset_iterator` (the
validation path marks the same set), `consensus/core/src/pow_layer0.rs` (`algo_id_is_priced_by_bits{,_v2,_v3}`,
`algo_id_carries_no_chain_position`, `PALW_ATTEMPT_BLUE_WORK_LOG2 = 20`),
`consensus/src/processes/ghostdag/protocol.rs:607-673` (blue work per lane).

**Fork-choice weight** is a fourth clock: `safe_weight`/`bounded_immature` grow by a claim's `pwu`
(`palw_claim_safe_contribution_v2`), only for attempt claims of weight-bearing classes, and only at
Final (safe) or acceptance (immature). Receipt, heartbeat and round blocks add none. The 6,001 audit's
C-2 fix makes `pwu` a function of `CCU/W`, so this clock is now paced by compute, not by a declared
target.

**What the table says in one line.** From 6,001 the attempt lane stops being priced by `bits` but keeps
advancing the DAA score and the blue score. Every window the chain measures in DAA — and every depth
it measures in blue score — is then paced by however fast producers can run forwards, not by the
120-second anchor the windows were sized against.

## 2. Current testnet-11, measured (2026-09-18, 300 selected-chain blocks, 43.99 h)

| quantity | value |
|---|---|
| selected-chain lanes | 300 / 300 are algo 3 (the hash anchor); no PALW block on the chain in 44 h |
| chain blocks | 6.82 / h — one every 8.8 min |
| DAA | 8.0 / h — **450 s per DAA** (nominal 120 s; the anchor lane under-produces at difficulty 1.0) |
| PALW attempt lane | budget-exhausted floor, merged as blues, not selected (the 09-12 stall) |

So today one DAA is 450 s. The windows are longer than designed, which is the safe direction.

## 3. Window arithmetic — DAA windows

`s/DAA` regimes: **nominal** 120 s (anchor only); **today** 450 s (measured); **6,001 steady state
without a fix** — the work target's `expected = epoch_length × fp_attempt_share_permille / 1000 =
1,000 × 0.9 = 900` model blocks per 1,000-DAA epoch, so 90 % of every epoch's DAA is model blocks and
the anchor's 120 s is shared over 10 DAA → **12 s/DAA**; **burst** — no in-epoch cap on model blocks
(audit A-05), `p → 1` when `CCU ≥ W`, eight A16 producers at one forward per ~26 s ≈ 0.3 blocks/s →
**3.3 s/DAA**.

| window (DAA) | what it protects | nominal 120 s | today 450 s | 6,001 steady 12 s | burst 3.3 s | floor it was sized for |
|---|---|---|---|---|---|---|
| `t_leak_daa` 5,040 (ADR-0128) | a silent validator is leaked | **7.0 d** | 26 d | **16.8 h** | 4.6 h | 7 days ("DAA の 7 日") |
| `reentry_final_depth_daa` 200 | a re-entering validator's anchor depth | 6.7 h | 25 h | 40 min | 11 min | hours |
| `window_bind` 600 | a claim's panel binds | 20 h | 75 h | 2 h | 33 min | hours |
| `window_receipt` 600 | seats replay and file | 20 h | 75 h | **2 h** | **33 min** | 20 h (ADR-0133: replay ≤ 240 s on 7 seats, page-in ~10 GiB on a 24 GiB class) |
| `window_challenge` 120 (from 6,001, ADR-0132 §7.6) | a licensed claim can be challenged | **4 h** | 15 h | **24 min** | **6.6 min** | 4 hours |
| `window_court` 3,000 | a court's clock | 100 h | 375 h | 10 h | 2.75 h | days |
| `court_turn_deadline` 42 | one court move | 84 min | 5.3 h | 8.4 min | 2.3 min | > one replay |
| `claim_retirement` 3,000 | claim records stay adjudicable | 100 h | 375 h | 10 h | 2.75 h | ≥ court + challenge |
| `withdrawal_delay` 7,500 | a bond's exit | 10.4 d | 39 d | 25 h | 6.9 h | 10 days |
| `epoch_length` 1,000 | retarget, W step, budgets | 33 h | 125 h | 3.3 h | 55 min | ~1.4 days |
| span 5 (ADR-0125/0130) | schedule, readiness cadence | 10 min | 37 min | 60 s | 16 s | minutes |
| readiness V1 age 30 spans = 150 | a possession proof stands | 5 h | 19 h | 30 min | 8 min | hours |
| readiness V2 age 8 spans = 40 | (6,100) | 80 min | 5 h | 8 min | 2.2 min | tens of minutes |

**Blue-score depths are paced the same way.** `finality_depth = FINALITY_DURATION / 120 s` and the
merge and pruning depths are counted in blue blocks; attempt blocks and heartbeats are blue, receipts
and rounds are not. A model lane at 12 s per block reaches "finality depth" ten times sooner in
wall-clock than the 120-second design, and each attempt block's blue work is a fixed 2^20, which a
producer mints at its forward rate.

## 4. Verdict

**RELEASE BLOCKER.** Under the bundle as it stands, the single lottery removes the attempt lane from
`bits` but not from the DAA score, and the work target's expectation is stated in DAA — a clock the
model lane itself advances. In steady state every DAA-denominated security and liveness window is
**10× shorter** than designed, and in a burst **36×**: the challenge window falls from four hours to
minutes, the validator leak from a week to hours, a bond's withdrawal from ten days to a day. This
fails the operator's criterion ("DAA traffic 依存で security/liveness window が安全下限を下回る") for
`t_leak_daa`, `window_challenge`, `window_receipt`, `withdrawal_delay` and `reentry_final_depth_daa`.

No rollout of main or testnet-11 until the DAA clock is independent of lane traffic.

## 5. The fix — ADR-0138 `palw_anchor_clock`: a block advances the DAA score iff `bits` priced it

One rule, one place, fenced at 6,001 with the rest of the bundle:

* In `internal_calc_daa_score`, subtract from the mergeset count every block that is **not** priced
  by `bits` at its own DAA (the same predicate the difficulty window already uses —
  `algo_id_is_priced_by_bits_v3` past the single lottery). Round blocks were already out (ADR-0125);
  attempt, receipt and heartbeat blocks join them.
* **Not** through `mergeset_non_daa`: that set means "outside the DAA window, unpaid", the coinbase
  skips it (`coinbase.rs:233`) and the PALW fold skips it (`palw_v2_merged_works(.., &merged_non_daa, ..)`).
  An attempt block must keep its subsidy (the escrow is a carve of it) and its claim must keep
  folding. The exemption is arithmetic on the DAA score only.
* Both paths (template: `calc_daa_score_and_mergeset_non_daa_blocks`; validation:
  `block_daa_window` → `calc_daa_score`) end in `internal_calc_daa_score`, so they agree by
  construction, and `check_difficulty_and_daa_score` refuses a header that claims otherwise.
* Consequences, all intended: the DAA score becomes the anchor lane's clock (retargeted to 120 s);
  `expected = 900 model blocks per 1,000 anchors` becomes "nine model blocks for every ten
  anchors" — the cadence `fp_attempt_share_permille = 900` always meant — instead of a runaway;
  every window in §3 keeps its nominal column; DAA still never decreases along a chain
  (`NonMonotonicContext` allows equality, and many attempt blocks may share a DAA).
* Blue-score depths: attempt blocks stay blue (their claims fold as merged blues), so the blue clock
  keeps moving at the model lane's rate. Finality, merge and pruning depths are **not** security
  windows the model lane can shorten to its advantage — reaching finality depth sooner only makes
  the chain harder to reorg — but the numbers are recorded here so the release report measures
  them.

## 6. The other two clocks the operator asked about

* **EVM (O13).** Gas is capped per chain block (`MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK = EVM_GAS_LIMIT
  = 30 M`), the executor runs every merged payload of a chain block under one 30 M env
  (`kaspa-evm/src/env.rs:66`), and round blocks reach the chain only through the block that merges
  them (`utxo_validation.rs:368-382`). With chain blocks every 120 s, "1 BPS" of round blocks is
  1 BPS of **scheduling**: 30 M gas per 120 s of throughput, whatever the lane's width. The spec
  decision and its implementation are in ADR-0139.
* **Class-derived verification window vs the global receipt deadline.** `verification_window_spans`
  is read only by capacity (`panel_room_v1`, the profile); the receipt deadline is
  `bound_daa + window_receipt` for every class. The fail-closed guard is in ADR-0133 §11.3.
