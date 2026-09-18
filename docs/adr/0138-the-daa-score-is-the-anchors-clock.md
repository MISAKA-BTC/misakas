# ADR-0138 — The DAA score is the anchor's clock: a block advances it iff `bits` priced it

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built and fenced**
(`Params::palw_anchor_clock`, `Some(6,001)` on testnet-11 — the single lottery's own day — `None`
elsewhere). Found by the operator's DAA-clock audit (`docs/palw-daa-clock-audit-2026-09-18.md`),
which held the rollout of main and testnet-11 until this was closed.

## 1. The defect

ADR-0132 S takes the attempt lane out of `bits`: from 6,001 an attempt header's Layer-0 digest is
admitted unconditionally and the class ticket `CCU/W` is the whole lottery. But the DAA score kept
counting attempt blocks (`calc_daa_score_and_mergeset_non_daa_blocks` excluded only the round lane),
and ADR-0137's work target states its expectation in DAA — `expected = epoch_length ×
fp_attempt_share_permille / 1000 = 900` model blocks per 1,000-DAA epoch. Nine of every ten DAA in
an epoch would then be model blocks, the anchor's 120 seconds would be shared over ten DAA, and one
DAA would be **12 seconds** in steady state (3.3 s in a burst: no in-epoch cap, `p → 1` when
`CCU ≥ W`). Every window the chain measures in DAA — `t_leak_daa` 5,040 (sized as seven days),
`window_receipt` 600 (twenty hours), `window_challenge` 120 (four hours), `withdrawal_delay` 7,500
(ten days) — would run ten times fast. The receipt and heartbeat lanes, unpriced since ADR-0083, had
been ticking the clock all along; the attempt lane at compute speed is what made it a blocker.

## 2. Decision

**A block advances the DAA score exactly when `bits` priced it.** Past `palw_anchor_clock`,
`internal_calc_daa_score` subtracts from the mergeset count every block whose lane is not priced at
its own DAA score — the same three-generation predicate the difficulty window already reads
(`algo_id_is_priced_by_bits` → `_v2` past `palw_receipt_rows_unpriced` → `_v3` past the single
lottery). Round blocks were already out (ADR-0125); attempt, receipt and heartbeat blocks join them.
The rule lives in one place (`SampledDifficultyManager::lane_advances_daa_at`) and both DAA paths —
the template's `calc_daa_score_and_mergeset_non_daa_blocks` and validation's `block_daa_window` →
`calc_daa_score` — end in the same arithmetic, so a header that claims otherwise is refused by
`check_difficulty_and_daa_score`.

**Not through `mergeset_non_daa`.** That set means "outside the DAA window": the coinbase pays no
block in it (`coinbase.rs:233`) and the PALW fold skips it (`palw_v2_merged_works(.., &merged_non_daa, ..)`).
An attempt block must keep its subsidy — the escrow is a carve of it — and its claim must keep
folding. The exemption is arithmetic on the score only: exempt blocks stay merged, blue where their
lane is blue, paid, folded. The test `adr0138_past_the_anchor_clock_a_heartbeat_block_ticks_no_daa`
pins all three.

**One decision, one setter.** `Params::set_palw_single_lottery` arms or clears the lottery and the
clock together; `validate_palw_v2` refuses the pair at two heights. The clock may stand alone (it has
receipts and heartbeats to exempt), the lottery may not run ahead of it on a shipped preset — the
flag-day table pins both at 6,001.

## 3. What follows

* The DAA score is the anchor lane's clock, retargeted to 120 s. Every DAA-denominated window in
  the audit's §3 keeps its nominal column, whatever the model lane does.
* The work target's expectation becomes what the number always meant: 900 model blocks for every
  1,000 anchors — nine model blocks for ten anchors, one every ~133 s network-wide — instead of a
  runaway that the model lane's own blocks feed.
* The DAA score is non-decreasing along a chain, never strictly increasing (`NonMonotonicContext`
  allows equality), and many attempt blocks may share one score: a clock, not an index.
* Blue-score depths (finality, merge, pruning) are still counted in blue blocks, and attempt blocks
  stay blue with a fixed 2^20 of work. Reaching finality depth sooner in wall-clock makes the chain
  harder to reorg, not easier; the release report measures the blue rate beside the DAA rate so
  the number is seen, not assumed.
* The devnet flag `--palw-anchor-clock-devnet` (must equal `--palw-single-lottery-devnet`) and the
  drill scripts' `ANCHOR_CLOCK_AT` arm it on a private chain; the drill's release report states
  anchor DAA/s, attempt DAA/s and receipt DAA/s from the logs.

## 3b. What the fix itself broke, and closed (the 2026-09-18 re-audit)

Two holes of its own, both found by re-auditing the fix rather than the bundle:

* **The freeze.** The first cut exempted every lane `bits` does not price, the heartbeat among them
  — and the heartbeat is the chain's liveness tick (ADR-0066 took it out of `bits` so a stock of
  certified quanta could not tighten the attempt lane's target, not because the chain does not need
  it). A chain whose hash lane stopped would have kept producing while its DAA score froze, taking
  every fence, deadline, retention and leak window with it. This was not a thought experiment: the
  registry drill froze at virtual DAA 20 with 183 blocks accepted, on a devnet whose only producers
  are PALW lanes.
* **The heartbeat as a stand-in, not a second clock.** Counting the heartbeat unconditionally would
  have been the opposite error. `heartbeat_interval_ms` falls back to the 120-second RECOVERY
  interval whenever the selected parent is not a PALW-v2 block, and on testnet-11 — whose selected
  chain is 300/300 algo-3 blocks — that is every block. A heartbeat miner would then have added a
  second tick beside every anchor tick and run the clock at roughly twice its nominal rate, which is
  the very error this ADR exists to remove. testnet-11 runs such a miner today, so this was live, not
  hypothetical. The rule is therefore per MERGESET, not per block: `palw_lane_advances_daa_v1`
  answers `false` for the heartbeat lane, and `daa_exempt_count` returns one fewer exemption only
  when the mergeset carries no priced block and at least one heartbeat. Where the hash lane runs the
  heartbeat adds nothing; where it stops, exactly one heartbeat a mergeset keeps the clock alive.
  `the_clock_is_the_anchor_and_the_heartbeat_and_nothing_else_past_the_fence` pins both directions,
  and the drill's fault phase stops the hash miner on a running chain and requires the DAA to keep
  moving.
* **The DNS leak's units.** ADR-0128 Decision 3 walks the evidence window in BLUE score and adds
  `t_leak_daa` into that blue total, which was the same thing while every lane ticked both clocks.
  With the DAA clock slower than the blue clock, a blue-bounded walk reaches back fewer DAA than the
  leak is decided over (5,244 blue ≈ 2,760 DAA at the work target's steady state), the lower-edge
  fallback under-states every silence, and **no bond would ever have been leaked**. The window now
  covers both spans — `DnsBftRulesV1::in_evidence_window`, the union of the blue span and the DAA
  span, whose start is the deeper of the two — and the runtime walk ends only where both are behind
  it. Where the clocks agree the window is the one ADR-0128 shipped, block for block. Pinned by
  `a_silent_bond_is_leaked_whether_blue_runs_with_the_daa_clock_or_twice_as_fast` (ratios 1, 2, 4).

**The blue axis this fence does NOT close**, and the operator accepted with the numbers in
`docs/palw-daa-clock-audit-2026-09-18.md` §9: finality (360 blue), merge depth (30), pruning (~900)
and the DNS attestation epoch (100) are counted in blue score, which the attempt lane still paces.
None is a split risk — blue score is a deterministic function of the DAG — and the effects are
liveness and availability, with finality moving in the safe direction.

## 4. Fingerprint

`c472a17b…` → `3d150afd…` on testnet-11 (the fence is hashed Some-only). The schedule is unchanged:
6,001 was already a scheduled height.
