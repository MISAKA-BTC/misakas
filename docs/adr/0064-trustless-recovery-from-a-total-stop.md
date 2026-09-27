# ADR-0064 — Trustless recovery from a total producer stop: the bond becomes usable in the block that registers it

> **Body moved (2026-09-27).** Normative rules → [spec/palw/10](../spec/palw/10-collateral-and-economics.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0064-trustless-recovery-from-a-total-stop.md](../design/palw/archive/0064-trustless-recovery-from-a-total-stop.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: **Partially superseded by its own correction** (2026-08-30). The one-lookup change landed
  behind `palw_bootstrap_activation` (armed on testnet-12 from genesis), but it does not close the
  total-stop deadlock it was written for. Facts A and B stand. The deadlock is answered on armed
  presets by the clock lane (ADR-0066, 0068).
* Date: 2026-08-30

## Context

ADR-0060 §12 left one question open once the audit withdrew the heartbeat lane: can a chain whose
every producer has stopped recover without trusting anyone?

## Decision

- **Fact A — "the network was silent" is not a checkable predicate.**
- **Fact B — a zero-weight lane hands fork choice to the hash.**
- **Decision — no new lane; move one lookup.** A block's own attempt resolves its bond against
  `bonds(parent) ∪ BondRegistered in its own mergeset`. → spec 10 PALW-CO-49.
- **Correction:** a block's own body is never in its own mergeset, so this does not let a would-be
  producer bootstrap alone. The deadlock remains for this ADR. The clock lane closes it (spec 13).

## Consequences

- Recovery from a total stop rests on the heartbeat clock lane, not on bond lookups. The full text
  records the P0 objection this exposed.

## Links

- Spec: [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0064-trustless-recovery-from-a-total-stop.md](../design/palw/archive/0064-trustless-recovery-from-a-total-stop.md)
