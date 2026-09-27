# ADR-0137 — A block buys one unit of work from any model, and a share is a result, not an input

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md](../design/palw/archive/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: Proposed 2026-09-18, with a reproducible simulation; the shadow was built the same day.
  **Armed on testnet-11 at 6,001** with the registry, the economic payout and the single lottery.
  **Armed on testnet-12 from genesis.** It supersedes share as a lottery input (ADR-0054, 0076, 0107,
  0135 D5) past `palw_work_target`.
* Date: 2026-09-18

## Context

A class's share had been an input to the lottery: granted at admission and walked by production. It
produced loops and traps: a permanent cold start, a lifecycle sawtooth, an inverse-cost share, a cap
trap, cadence collapse, and a dormant usage loop (§3). The ticket had been 3.589 × 10⁻³ for reasons
nobody chose.

## Decision

- **D1 — The ticket:** `T_m = MAX · min(1, CCU_m / W)`. A class has no target. → spec 05 PALW-WK-9.
- **D2 — The work target `W`** is one chain value, walked like `bits` over model blocks only. → spec 05
  PALW-WK-10.
- **D3 — The floor is the residual,** and it is unpaid. → spec 05 PALW-WK-11.
- **D4 — The reward:** a Final pays `E`, split by the economic payout's panel share. → spec 10
  PALW-CO-42.
- **D5 — The verification budget replaces the per-class caps.** → spec 10 PALW-CO-36.
- **D6 — The lifecycle is a gate, not a price.**
- **D7 — Share is a reader's number.** → spec 05 PALW-WK-12.
- **D8 — The network draw stays, for now** (it was later folded by ADR-0132 S, the single lottery).

## Consequences

- Every block pays the same escrow for the same expected work, from whichever model did it.
- Fairness, a new model's entry, a hundred models, attacks, migration and the simulation are §9–§20 of
  the full text.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md](../design/palw/archive/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)
