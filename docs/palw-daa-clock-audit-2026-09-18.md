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

## 2. Current testnet-11, measured (2026-09-18)

> **This section replaces an earlier version that was WRONG, and the correction is the finding that
> stopped the release.** The first pass recorded "300 / 300 are algo 3 (the hash anchor); no PALW
> block on the chain in 44 h". The opposite is true. The numbers below were read from the chain
> itself with `scripts/misaka-t11-lane-walk.py`, which walks the selected parent back from the sink
> over the node's JSON wRPC and counts `powAlgoId`. Run it before trusting any lane claim here.

| window | algo 6 (PALW attempt) | algo 8 (heartbeat) | algo 3 (hash anchor) |
|---|---|---|---|
| last 60 selected-chain blocks (4.58 h) | 60 — **100 %** | 0 | **0** |
| last 300 selected-chain blocks (41.5 h) | 261 — 87 % | 39 — 13 % | **0** |

| quantity | value |
|---|---|
| chain blocks | 13.1 / h over the recent window, 7.2 / h over the long one |
| DAA | **275 s per DAA** recently, 457 s over 41.5 h (nominal 120 s) |
| virtual DAA at 10:19Z | 5,998 — three below the 6,000 flag day |

**There is no anchor block on this chain, and there cannot be one.** The block template declares
`bundle.algorithm_id`, which `PalwRulesetV2::validate` requires to be the committed attempt id on
every ConsensusV2 network, so a Layer-0 hash miner asking a testnet-11 node for work is handed an
algo-6 template and refuses it — `misaminer` stops with "this network mines with PoW algo 6 … there
is no external-miner client for it". The registry drill's devnet reproduced this exactly.

**So `palw_anchor_clock` must not be armed at 6,001 as this ADR is written.** Past the fence a lane
advances the DAA only where `bits` prices it, and on testnet-11 that set is EMPTY. The only tick left
is the heartbeat stand-in, and `heartbeat_interval_ms` answers the NOMINAL one-hour interval whenever
the selected parent is a PALW-v2 block — which on this chain it always is. The clock would fall from
275 s per DAA to 3,600 s at best, and to nothing at all across a stretch like the most recent 4.58
hours, which carried no heartbeat. Every DAA-denominated window stretches with it: fence 6,100 at
~100 h instead of 3.3, fence 6,900 at ~37 days, the 120-DAA challenge window at ~5 days, `T_leak` at
~7 months. And the whole chain's clock would rest on one unbonded heartbeat producer on one host.

The per-mergeset stand-in of §"the freeze" is still the right rule — it is strictly safer than either
alternative — but it does not rescue this. A one-hour beat gives a one-hour DAA.

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

---

# Part II — the re-audit of the fix (2026-09-18, RC `05169552` frozen)

Four gates, run against the frozen candidate. Two things the fix itself created, both closed here;
one residual on the blue axis, reported as a release blocker below.

## 7. ADR-0138 re-audited: the clock's own holes

**Every DAA increment site.** One: `SampledDifficultyManager::internal_calc_daa_score`
(`consensus/src/processes/difficulty.rs`). Both callers — the template's
`calc_daa_score_and_mergeset_non_daa_blocks` and validation's `block_daa_window` → `calc_daa_score`
(`window.rs:351,357`) — end in it, and `check_difficulty_and_daa_score`
(`pre_pow_validation.rs:31`) refuses a header that claims another number. No second arithmetic
exists anywhere in the tree.

**The rule is a pure function.** `palw_lane_advances_daa_v1(algo, daa, anchor_clock, single_lottery,
receipt_rows)` — no store, no clock, no host state. Pinned by
`the_clock_is_the_anchor_and_the_heartbeat_and_nothing_else_past_the_fence` (every lane at every
fence combination, including the heights where the attempt and receipt lanes are still priced and
therefore still tick) and `the_classification_is_a_pure_function_every_node_computes_alike`.

**The freeze the first version created, and the double-count the obvious fix would have created.**
ADR-0066 took the heartbeat out of `bits`, so the first cut of ADR-0138 exempted it too — and a chain
whose hash lane stops still beats, which means its DAA score would have **frozen** while blocks kept
coming: every fence, deadline, retention and leak window stopped with it. The registry drill showed
this on a running chain: virtual DAA 20, unmoved, with 183 blocks accepted.

Counting the heartbeat unconditionally would have been the opposite error, and a live one.
`heartbeat_interval_ms` falls back to the 120-second RECOVERY interval whenever the selected parent
is not a PALW-v2 block, and testnet-11's selected chain is 300 out of 300 algo-3 blocks — so every
parent takes that branch. A heartbeat producer would have added a tick beside every anchor tick and
run the clock at about twice its nominal rate. testnet-11 runs one such producer today, on the .113
host, so this was not hypothetical.

The rule is therefore per MERGESET. `palw_lane_advances_daa_v1` answers `false` for the heartbeat
lane, and `daa_exempt_count` gives back exactly one exemption when the mergeset carries no priced
block and at least one heartbeat. Where the hash lane runs the heartbeat adds nothing to the clock;
where it stops, one heartbeat a mergeset keeps the clock alive. Pinned by
`the_clock_is_the_anchor_and_the_heartbeat_and_nothing_else_past_the_fence` in both directions, and
rehearsed on a running chain by the drill's fault phase, which stops the hash miner and requires the
DAA to keep moving.

**Exempt is not excluded.** The exemption is arithmetic on the score only. A merged attempt block
stays blue, stays out of `mergeset_non_daa` — the set the coinbase skips (`coinbase.rs:233`) and the
PALW fold ignores (`palw_v2_merged_works(.., &merged_non_daa, ..)`) — so it keeps its subsidy and its
claim still folds. The heartbeat test asserts exactly this for a real merged block: blue, not in
`mergeset_non_daa`, and the state root moves for it.

**Reorg and replay.** The score is `stored_parent_score + (mergeset − non_daa − exempt)`, every term
a function of stored headers and the block's own ghostdag data, so a replay of the same block
computes the same number by construction; the rooted PALW state's delta revert is pinned per feature
(`revert_delta_v2` equality in each of the 6,001 tests). No wall clock, no local storage, no
iteration order enters either.

## 8. RELEASE BLOCKER, NOW CLOSED (blue axis) — the DNS leak's evidence window is walked in blue score while the leak is decided in DAA

ADR-0138 made the DAA score slower than the blue score, and one rule reads both.

* The leak fires when `anchor_daa − last_attested_daa ≥ t_leak_daa` (5,040 on testnet-11):
  `dns_bft_v1.rs:368`, decided in **DAA**.
* `last_attested_daa` comes from the walk. Where the walk finds no attestation it falls back to
  `window_lower_edge_daa` — the lowest DAA the walk reached (`dns_bft_v1.rs:249-257`).
* The walk's bound is **blue score**: it stops at `evidence_floor_blue_score = anchor_blue −
  (t_leak_daa + reentry_final_depth_daa + epoch_length_blue + lag_blue)` = 5,244 blue on
  testnet-11 (`dns_bft_v1.rs:55-60,110`; the runtime loop at
  `virtual_processor/dns_bft.rs:186-203` breaks on `compact.blue_score < floor`).

Before ADR-0138 blue and DAA advanced together, so walking 5,244 of blue reached back ≥ 5,040 of DAA
and ADR-0128's own sentence held: *"Absence inside a window at least `t_leak_daa` long is silence of
at least `t_leak_daa`, so the lower-edge fallback is exact rather than a guess."* Now the attempt
lane adds blue without adding DAA: at the work target's steady state (`fp_attempt_share_permille =
900`, so ~0.9 attempt blocks an anchor) 5,244 blue reaches back **~2,760 DAA**, and
`anchor_daa − lower_edge ≈ 2,760 < 5,040` for **every** bond.

**Impact: the inactivity leak can never fire past 6,001.** The direction is safe for the bond (no
honest validator is leaked — the fallback under-states silence, never over-states it) and wrong for
the overlay: a validator that stops attesting is never removed from the counted set, so a two-stage
BFT quorum can be held below threshold by bonds that never speak, the DNS-final anchor stops
advancing, and the stake reorg gate freezes at its last confirmation. ADR-0128's stated purpose —
"inactivity leak は再実装して … 正しく有効化できるようにして" — is defeated on the flag day, silently.

**Fixed (2026-09-18, same session).** The window covers both spans:
`DnsBftRulesV1::evidence_floor_daa_score` is the DAA edge (`anchor_daa − (t_leak + reentry)`),
`in_evidence_window` is the union of the two spans — both clocks fall as the walk goes back, so the
union's start is the deeper of them and the slice stays contiguous — and the runtime loop ends only
where both are behind it. `dns_bft_window_lower_edge_daa_v1` and `dns_bft_is_final_evidence_v1` read
the same predicate. Where the two clocks agree, which is every height below `palw_anchor_clock`, the
window is the blue-only one ADR-0128 shipped, block for block; and the DNS gate and the leak both
arm at 6,001, so nothing below the flag day changes. Pinned by
`a_silent_bond_is_leaked_whether_blue_runs_with_the_daa_clock_or_twice_as_fast`: at blue-to-DAA
ratios of 1, 2 and 4 the window still reaches back at least `t_leak_daa` of DAA and a silent bond is
still leaked. Cost, as predicted: walk until BOTH spans are covered — the blue terms (`stake_score_window`,
`epoch_length`, `lag`) in blue score as today, AND the DAA terms (`t_leak_daa +
reentry_final_depth_daa`) in DAA score — stopping only when each is satisfied. Deterministic, and it
restores the ADR's sentence exactly. Cost: the walk reads ~1.9× the blocks in steady state, and
`validate_palw_v2`'s "pruning depth ≥ the walk" check must be re-derived against the longer blue
span (testnet-11's derived 12,002 covers the ~9,600 blue the fixed walk needs at the steady ratio,
but not an arbitrary burst — bounding the model lane per epoch, the audit's A-05, is what makes the
ratio a bound rather than an average).

## 9. The other blue-score windows, measured

Counted in blue score, and the blue clock is now paced by the anchor **plus** the attempt lane
(~1.9× the anchor at the steady state above; unbounded within one epoch until A-05's per-epoch cap
exists):

| window | blue blocks | at 120 s anchors only | at the steady 1.9× |
|---|---|---|---|
| merge depth (`MERGE_DEPTH_DURATION` 1 h) | 30 | 1.0 h | 32 min |
| finality (`FINALITY_DURATION` 12 h) | 360 | 12 h | 6.3 h |
| pruning (`PRUNING_DURATION` 30 h) | ~900 | 30 h | 15.8 h |
| DNS attestation epoch | 100 | 3.3 h | 1.75 h |

None of these is a split risk: blue score is a deterministic function of the DAG, identical on every
node. The effects are liveness and availability — a slow producer's block falls outside the merge
window sooner, a node offline longer than the (shortened) pruning window needs a fresh pruned sync,
and DNS validators must attest more often per unit time. Reaching finality depth sooner makes the
chain harder to reorg, which is the safe direction. **Recommendation:** accept and document these
four, and close §8 before arming — it is the one that turns a security mechanism off.

## 10. ADR-0139 re-audited

`evm_distinct_permitted_rounds_v1` is the whole rule and it is now one function in consensus-core,
read by validation and by the template alike. `one_round_buys_one_budget_however_many_blocks_or_permits_it_carries`
pins: two permits of one round buy one budget; one round across two spans buys one; 200 uses over 5
rounds buy 5; a gap in the round indices buys nothing extra; order and repetition do not matter.
Upstream of the count, a round block is in `uses` only if the parent state granted its permit
(`round_permit_used` refuses a permit twice, `round_equivocated` refuses an equivocation), so
filling one round with blocks cannot even reach the count. The cap saturates at
`EVM_CHAIN_BLOCK_GAS_CEILING_V1` (390 M) and is committed as the EVM header's `gas_limit`, so a
reorg that changes the round set changes the commitment and the block does not reconstruct.

## 11. ADR-0133 §11.3 re-audited — the units

`verification_window_spans` (spans) × `fold.span_daa` (DAA a span) vs `window_receipt()` (DAA): one
unit on both sides. The safety the operator asked about is inside the derivation, not the
comparison — `palw_verification_window_spans_v1` multiplies by `safety_permille` (≥ 1.0) and adds
`receipt_allowance_spans` (1 span) before the guard ever sees the number. Pinned at the three points
by `adr0133_the_window_fits_the_deadline_in_daa_at_deadline_minus_one_deadline_and_plus_one`
(deadline − one span fits, the deadline exactly fits, deadline + one span is HELD), and the
multiplier itself is pinned by changing only `span_daa` and watching the verdict flip. Calibration
note: `reference_work_per_span` is derived from `PALW_SPAN_MS_V1` (600 s = 5 anchors × 120 s), which
matches testnet-11's `schedule_span_daa = 5`; the drill's devnet runs `span_daa = 2` on 10-second
blocks, so its derived windows are optimistic by construction — a fixture property, not a
testnet-11 one.

## 12. The registry fence re-audited — consensus side, not just the node

Holding the node's own submitter was not enough: anyone can put a `ClassRegistered` object on the
chain below the fence (its acceptance is gated by ADR-0049's admission, not by the registry's
height), and the C-1 fix means no node can ever derive that class's work from the chain. **The first
boundary past the fence now gives such a class an inert row** — `Registered`, zero work, admission
zero — instead of leaving it row-less: it admits no claim (`ClassNotAdmitting{Registered}`), it
prices nothing, and its permille returns to the room at that same boundary instead of standing for
ever as `legacy_sum` that no rowed class can use. Every node writes the same row, because "the build
cannot describe this class" is a fact about the build. A class that wants in registers again past
the fence, where the object carries the graph and the row opens in the same block. Pinned by
`a_class_registered_before_the_fence_gets_an_inert_row_and_gives_its_share_back`. Rejecting the
object below the fence was the alternative and was rejected: it would change consensus below DAA
6,000, which is exactly what the compatibility boundary exists to prevent.
