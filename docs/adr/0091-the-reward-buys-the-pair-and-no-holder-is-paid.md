# ADR-0091 — The reward buys the pair, and no holder is paid

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md](../design/palw/archive/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-06 (design first) and **implemented** behind `palw_model_market` and
  `palw_model_lines`. It amends ADR-0087.
* Date: 2026-09-06

## Context

The operator asked that mining reward support a model's market, without paying holders anything.

## Decision

- **D1 — The slice is 5 % of the claim's escrowed worker reward.**
- **D2 — At `Final` the slice buys from the line's pair,** and the chain keeps what it buys.
- **D3 — Where there is no pair, the miner is paid in full.** Nothing is burned in its place.
- **D4 — Retired positions are the chain's,** and they never move.
- **D5 — Nothing else changes at `Final`,** and nothing changes for a claim that voids.
- **D6 — What a participant reads** (`buyback_sompi`).
- **D7 — Same fences.**
- **D8 — The emission identity:** a carve, never an addition.

→ spec 15 PALW-MK-8, 10 PALW-CO-42.

## Consequences

- The buyback slice is priced into the seat lock at its cap (ADR-0152 L-1).

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md](../design/palw/archive/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
