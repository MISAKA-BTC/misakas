# Liveness, the heartbeat and fork choice — design

> **Not normative.** This document explains why the rules in
> [spec/palw/13-fork-choice-and-heartbeat.md](../../spec/palw/13-fork-choice-and-heartbeat.md) (and the
> clocks of [06](../../spec/palw/06-eligibility-and-block-production.md) §6.4) are what they are.

**Decisions recorded in:** ADR-0039, 0041, 0060, 0064, 0066, 0068, 0105, 0138, 0140, 0142, 0151;
[ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md) (the strict economic win, and heartbeat
transparency on the same chain).
**Last revised:** 2026-09-27

## 1. Problem

A chain whose blocks are won by inference can stop when nobody produces. Time must keep moving without
anyone's permission or capital. But the lane that keeps time must never be worth mining instead of
the work, must never outweigh bonded work, and must never give an attacker a cheap way to reorder or
double-spend.

## 2. The design in one paragraph

The doctrine (ADR-0060): **time is permissionless, weight is bonded, and finality is an overlay.**

- **The heartbeat lane** (`algo_id` 8, fixed target, at most four per mergeset) moves the clock. It
  carries no economic weight, and it never turns a bonded block red.
- **Fork choice** orders by settled PALW work first. A deep reorg must strictly win it, and since
  DAA 750 a tie keeps the incumbent, except within two DAA ticks, where GHOSTDAG's order lets honest
  slot races converge.
- **Heartbeat transparency** is the rule that a heartbeat does not block an attempt's merge. It stops
  at the merging block's own chain, which closes the double spend it allowed.

## 3. Lessons that set the rules

- **The slow-producer trap** (testnet-11, 2026-09-10): once a heartbeat was the selected parent,
  bonded blocks turned red. That led to ADR-0105.
- **A clock that could be postponed** by blocks that did not advance it led to ADR-0142's cursor.
- **The double spend through transparency** (2026-09-25): an unbonded heartbeat miner could absorb
  public attempts within the merge depth. That led to `palw_heartbeat_transparent_same_chain`.
- **Pruning-proof takeover** (audit, hf-pptake2): a node could be switched to a chain that did not
  strictly win. That led to `palw_pruning_proof_strict_economic_win`.

## Source texts (archived ADR bodies)

- [ADR-0064 — Trustless recovery from a total producer stop: the bond becomes usable in the block that registers it](archive/0064-trustless-recovery-from-a-total-stop.md)
- [ADR-0105 — A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has landed](archive/0105-a-heartbeat-never-turns-a-bonded-block-red.md)
- [ADR-0041: PALW pruning-proof verification — exhaustive and amortised, not sampled](archive/0041-palw-pruning-proof-verification.md)
- [ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory](archive/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
- [ADR-0060: The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay](archive/0060-the-liveness-doctrine.md)
- [ADR-0140 — The heartbeat is the emergency generator: it must not touch the economy or the difficulty while the chain is producing](archive/0140-the-heartbeat-is-the-emergency-generator.md)
- [ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the clock may not postpone it](archive/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)
