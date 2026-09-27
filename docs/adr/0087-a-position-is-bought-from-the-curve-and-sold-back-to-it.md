# ADR-0087 — a position is bought from the curve and sold back to it

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../design/palw/archive/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-05, **implemented the same day** behind `palw_model_market`.
  - **Amended by ADR-0088** (keyed by line, the owner's leg), **0089** (the EVM window), **0090**
    (whole positions, the seed opens the market), **0091** (the reward buys the pair) and **0114**
    (the owner's leg is 5 %).
  - §1's "the EVM lane is optional" is stale (the lane is a default build).
* Date: 2026-09-05

## Context

The operator asked for Model Positions: a per-model, fixed-supply position whose price is set by what
participants believe the model is worth, without it being a copy of ERC-20.

## Decision

- **D1 — A position is a balance in the state fold,** not a coin and not a UTXO.
- **D2 — The curve is constant product over the reserve.** *The virtual reserve was replaced by the
  seed (0090).*
- **D3 — Two moves, and only two:** buy and sell. *A third, the seed, was added by 0090.*
- **D4 — The fee is on the MSK leg of every move, split three ways.** *Amended by 0090 and 0114.*
- **D5 — No transfer exists.**
- **D6 — The market is a consensus rule,** armed by activation, never by regenesis.
- **D7 — A new version is a new class.** *Narrowed by 0088 to a new graph.*
- **D8 — What a participant reads** (`getPalwModelMarket`).

→ spec 15 §15.2.

## Consequences

- A position is bought from the curve and sold back to it, never traded between holders.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../design/palw/archive/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)
