# ADR-0139 — O13 decided: the execution lane's gas throughput scales with the lane, one budget a round

**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built**. Rides
`palw_execution_lane` (testnet-11 DAA 6,001): below the lane the cap is what a chain block always
had; past it the cap is a function of the round blocks the chain block merges. Decides the open
item O13 of `docs/misaka-evm-design-v0.4.md` (§14.3: "per-second G_limit derivation + measured
propagation, frozen before activation").

## 1. The question

ADR-0125 gives transactions a fast lane — one execution round a second, one permit a round on
testnet-11 — "without the PALW chain getting a fast clock". Round blocks reach the chain only
through the chain block that merges them, and the EVM accepts user gas per **chain block**:
`MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK = EVM_GAS_LIMIT = 30 M`, executed under one 30 M env
(`kaspa-evm/src/env.rs`). With chain blocks every 120 seconds, "1 BPS" of round blocks was 1 BPS
of **scheduling** and 30 M gas per 120 s of **throughput** — 250 k gas a second, whatever the
lane's width. The operator asked which one the lane means.

## 2. Decision

**Both.** A round buys gas: every DISTINCT permitted round among the round blocks a chain block
merges adds `EVM_ROUND_GAS_BUDGET_V1 = 3,000,000` to that chain block's accepted-user-gas cap, on
top of the base, under a ceiling of `EVM_CHAIN_BLOCK_GAS_CEILING_V1 = 30 M + 120 × 3 M = 390 M`
(one anchor's worth of rounds). `evm_user_gas_cap_v1(permitted_rounds)` is the one spelling.

* **Deterministic in consensus data, never in a clock.** The rounds are the `round` fields of the
  permit uses the merging block's own verdicts record (`PalwRoundVerdictsV1::uses`, the same list the
  PALW fold writes); a round index is derived from the round block's header timestamp against the
  genesis, and the round block is consensus data. The node's wall clock enters nowhere.
* **Committed.** The cap is the EVM header's `gas_limit`, so the commitment root covers it: a block
  that claims another cap does not reconstruct, and a block re-executed from its stores (the
  reconstruct driver) runs under the cap it was validated with, read from its committed header.
* **Never below the base.** `user_gas_cap.max(MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK)`: the system
  ops (deposit claims, market settlements) that fit before still fit. The per-transaction bound
  (`gas_limit ≤ MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK`, `tx.rs`) is unchanged.
* **The template computes what validation will compute** (`evm_template_fields`): the distinct
  permitted rounds of the round blocks the template merges, from the tip state's verdicts.
* The base-fee controller keeps `EVM_GAS_LIMIT / 2` as its target: fee dynamics are per chain block
  as before; the lane widens the ceiling, not the target.

## 3. Why 3 M a round — the benchmark

`o13_bench_execution_gas_per_second` (`kaspa-evm`, `--ignored --release`), on the build host
(Apple silicon, in-memory `CacheDB`), 2026-09-18:

| workload | gas | time | gas/s | bytes/gas |
|---|---|---|---|---|
| 1,000 signed EIP-1559 transfers | 21.0 M | 0.121 s | 174 M | 0.0050 |
| 200 calls of 40 fresh SSTOREs (+ deploy) | 181.5 M | 0.030 s | 6.0 G | 0.0002 |

* **Execution** is not the bound: even a host 5× slower with a disk-backed flat state runs 30 M+
  gas a second; 3 M a round is a tenth of that.
* **Propagation** is not the bound: at 0.005 bytes/gas a full round is 15 KB, a full ceiling block
  ~2 MB at the anchor cadence.
* **State growth is the bound.** A fresh account or storage slot costs ~20 k gas and writes ~100
  bytes of state; 3 M gas a second is ≤ 15 KB/s of new state, **~1.3 GB a day at full load** —
  what a testnet host absorbs. The transfer workload (fresh accounts at 21 k gas each) is exactly
  this worst case. Raising the budget is a number, not a rule; the state-growth line is the one to
  re-measure first.

## 4. Pinned

`the_cap_is_the_base_below_the_lane_and_one_budget_a_permitted_round_under_the_ceiling`
(consensus-core `evm`): base below the lane, base with no merged round, one budget a round, the
ceiling at 120 rounds and beyond, saturating. The executor's own suites (77) and the consensus EVM
suites (43) run under the threaded cap. No fingerprint moves: the cap rides the lane's fence and
the EVM header's commitment.
