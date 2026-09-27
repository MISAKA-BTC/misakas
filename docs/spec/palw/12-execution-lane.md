# PALW spec — 12. Execution lane

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/exec-lane.md](../../design/palw/exec-lane.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md). EVM execution semantics are specified
> in `spec/evm` (Phase 3). This chapter covers how the lane is scheduled, accepted and settled.

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (the lane is open from genesis at
one permit per round; parents-first acceptance from DAA 750)
**Reconciled with code at:** `55a7be02f` (2026-09-27)

Transactions get a fast lane without the PALW chain getting a fast clock. Every second is a round. A
round's permits go to bonds whose attempts reached `Final`. A permit is a **round block**: a light,
fee-only block that adds no confirmations and no finality.

## 12.1 Round blocks and permits

- **PALW-XL-1 (the round block).** A round block MUST be `algo_id` 10 (`POW_ALGO_ID_PALW_ROUND_V1`),
  admitted only past `palw_execution_lane`, with the constant target `2⁻¹⁶`. It is:
  - never a selected parent;
  - always red;
  - outside the DAA set;
  - of zero blue work and zero subsidy;
  - never a claim.

  The chain's GHOSTDAG, windows, retargets and depths are exactly those of the DAG without it.
- **PALW-XL-2 (anchoring).** A non-round block MUST name at least one non-round parent, and its selected
  parent is the heaviest of those. A round block names at most one non-round parent, and that parent
  is its anchor.
- **PALW-XL-3 (rounds and permits).** `round(t) = (t − genesis_timestamp) / 1000`. A span is
  `schedule_span_daa` of DAA (1 on testnet-12). A span's permits are scheduled from the attempts that
  reached `Final` in the span before it:
  - quotas are proportional to the compute those attempts certified;
  - each security domain is capped at 45 %;
  - one permit per operator;
  - no domain in two consecutive rounds (each operator is assigned one parity);
  - a round whose parity has no eligible operator has no permit. It is a counted miss, never a relaxed
    rule.
- **PALW-XL-4 (the schedule's seed).** At the first chain block of span `n`, span `n`'s schedule is the
  snapshot taken at the start of span `n − 1` from the finals of span `n − 2`. It is seeded with
  `H(A ‖ n ‖ safe frontier)`, where `A` is the last chain block carrying an admitted attempt in span
  `n − 1`. So the participants are fixed before the anchor that orders them, and the seed delay is f+2.
- **PALW-XL-5 (the permit).** A round block's `palw_commitment` is a signed `PXR1` envelope
  `{version, network domain, round, permit index, bond, pubkey, signature}`. Its round MUST be its
  header timestamp's round.
- **PALW-XL-6 (width).** The lane opens at `permits_per_round` (1: BPS 1). It widens only by a stage table
  of fenced heights, up to 10 (`PALW_EXEC_MAX_PERMITS_PER_ROUND_V1`), and a span keeps the width it
  opened with. Widening is deferred under ADR-0144 §6 item 0.
- **PALW-XL-7 (maturity).** On testnet-12, an execution-lane quantum matures 120 DAA after it is earned
  (`palw_exec_quantum_maturity_daa`). Rights that matured before a conviction are not revoked, but
  unminted rights are forfeited (10 PALW-CO-34).

**Sources:** ADR-0125 D1–D7, ADR-0130 D2–D6 (D2's one entry per operator, D3's parity, D4's future
anchor), ADR-0137 §12. **Code:** `core/palw_execution_lane_v1.rs`, `core/palw_execution_quanta_v1.rs`;
node side `kaspad/src/palw_round_producer.rs`.

## 12.2 Acceptance and gas

- **PALW-XL-8 (the merging block decides).** A chain block MUST grant a round block's permit from its own
  parent state (`palw_round_verdicts_v1`), accept the transactions of granted round blocks, and pay
  their fees to the bond's payout. A mergeset holds at most:
  - `max_per_mergeset` round blocks in all;
  - `permits_per_round` of any one round;
  - one per `(round, index)`;
  - round blocks of at most 64 bonds.

  A round block merges only rounds older than its own.
- **PALW-XL-9 (parents first, from DAA 750).** Past `palw_lane_accept_parents_first`, a merging block
  MUST apply a tied round lane parents first, so no child is applied before its parent and no
  transaction is dropped. **From DAA 750 a round block carries no EVM payload.** *Code:*
  `cons/pipeline/body_processor/body_validation_in_isolation.rs`, `cons/pipeline/body_processor/processor.rs`,
  `consensus/src/model/stores/ghostdag.rs`, `cons/pipeline/virtual_processor/processor.rs`.
- **PALW-XL-10 (gas).** Each distinct permitted round among the round blocks a chain block merges adds
  `EVM_ROUND_GAS_BUDGET_V1` = 3,000,000 to that chain block's accepted-user-gas cap, under the ceiling
  `EVM_CHAIN_BLOCK_GAS_CEILING_V1` = 390,000,000 (`evm_user_gas_cap_v1`). The cap is deterministic in
  consensus data and committed as the EVM header's `gas_limit`.
- **PALW-XL-11 (round-permit equivocation).** Two round blocks for one permit from one bond are an
  offence (`RoundPermitEquivocated`, tag 46).

**Sources:** ADR-0125 D5–D6, D8; ADR-0139; ADR-0154 (D13). **Code:** `core/palw_execution_lane_v1.rs`,
`consensus/core/src/evm/`.

## 12.3 Finality

- **PALW-XL-12 (anchors, not blocks).** Execution blocks carry no finality. No number of round blocks is
  a confirmation. A payment's finality is its settlement depth: the settled PALW anchors after it
  (07 PALW-LC-21). Merging permitted round blocks leaves the sink's fork-choice inputs exactly as they
  are without them.
- **PALW-XL-13.** A licensing quorum on an anchor is a quorum over the anchor's transactions and the
  state they leave, because the attempt's challenge covers the header's merkle roots and UTXO
  commitment. A settled double spend therefore needs both winning anchors and quorums of seats on the
  conflicting branch.

**Sources:** ADR-0129 D1–D5, ADR-0127. **Code:** `core/palw_state_v2.rs` (`round_finals`).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | the execution lane from genesis at one permit per round, span 1 DAA. `palw_lane_accept_parents_first` from DAA 750 |
