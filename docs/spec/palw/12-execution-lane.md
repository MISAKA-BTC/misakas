# PALW spec — 12. Execution lane

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. EVM execution
> semantics are specified in `spec/evm` (Phase 3). This chapter covers only how the lane is scheduled,
> accepted and settled.

**Purpose.** Transactions should not wait for a 120-second PALW block. The execution lane places
**round blocks** between PALW blocks, one permit at a time (BPS 1 on testnet-12, with 10 BPS as the
target). A round block carries transactions, but it adds no confirmations and no finality. A payment
is as final as the settled anchors after it. This chapter defines round permits and their schedule,
gas per round, the order in which a merging block applies round blocks, the maturity of the
execution-lane quantum, and the rule that a double spend needs the anchors.

**Principles served:** none of P1–P7 directly. The lane is infrastructure that must not make the
product worse (chapter 01 §1.3: the product does not wait).

## 12.1 Round blocks and permits

- [ ] The execution lane is a second lane inside the cadence. Round blocks, permits, the schedule
  span and the seed delay. *Sources:* 0125 (with testnet-11's span 5 → 1 DAA as history), 0130 D2–D6
  (hardened before widening, including the f+2 seed delay). *Code:* `core/palw_execution_lane_v1.rs`,
  `core/palw_execution_quanta_v1.rs`; node side `kaspad/src/palw_round_producer.rs`.
- [ ] Round blocks join the round lane outside the DAA clock (chapter 06 §6.4). *Sources:* 0138.
- [ ] The execution-lane quantum matures 120 DAA after it is earned (testnet-12; operator decision
  2026-09-25). *Code:* `Params::palw_exec_quantum_maturity_daa`.
- [ ] What stays deferred: widening beyond BPS 1, the derived panel share, and the DA reward. *Sources:*
  0130 D7–D8 and §5, held under 0144 §6 item 0.

## 12.2 Gas

- [ ] One gas budget per distinct permitted round that a chain block merges (3 M gas), under a 390 M
  ceiling. It is deterministic in the round indices and committed. *Sources:* 0139.

## 12.3 Acceptance order

- [ ] From DAA 750, a merging block applies a tied round lane **parents first**, so a child is never
  applied before its parent and no transaction is dropped. **A round block carries no EVM payload.**
  *Fence:* `palw_lane_accept_parents_first`. *Sources:* the IBD audit of 2026-09-26 (the consensus
  finding). *Code:* `cons/pipeline/body_processor/body_validation_in_isolation.rs`,
  `cons/pipeline/body_processor/processor.rs`, `consensus/src/model/stores/ghostdag.rs`,
  `cons/pipeline/virtual_processor/processor.rs`.
- [ ] The node-side IBD fix (hand-off in parent-first order, no fence) is in chapter 14.

## 12.4 Finality

- [ ] **A double spend needs the anchors, not the blocks.** Execution blocks carry no finality. A
  payment settles at the settled-anchor depth of chapter 07 §7.6. *Sources:* 0129, 0127. The
  merchant-facing rule is in the launch note §3.
- [ ] Round finals in the PALW state. *Code:* `core/palw_state_v2.rs` `round_finals`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | the lane is open from genesis at BPS 1. `palw_lane_accept_parents_first` from DAA 750 |

**Design:** `design/palw/exec-lane.md`.
