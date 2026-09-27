# ADR-0100 — A model is data: the one-move court, the held measurement and the licence per shard, and the boundary "permissionless" means

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../design/palw/archive/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Proposed 2026-09-10. **D1–D4 and D6 implemented, consensus-inert** on every shipped preset.
  The one-move court is a consensus object behind ADR-0099's fence, and per-shard licensing is built.
  Both are dormant on testnet-12.
* Date: 2026-09-10

## Context

Two reviews (2026-09-10) said the same thing from two sides. Registration, certification, admission,
the draw, the court and the market are already permissionless. What a model still needed was to be
data end to end: measured by a tool, tried by a court that fits a shard, and licensed per shard.

## Decision

- **D1 — The one-move court is built.** Arming it is a ruleset move. → spec 09 PALW-CT-22.
- **D2 — The inventory measures the artifact,** and a shard's rows are the inventory's.
- **D3 — `palw-class measure` and `verify`.**
- **D4 — Licensing per shard,** built, with no flag day needed (amended the same day).
- **D5 — The economics come before the arming.**
- **D6 — What "permissionless" means is fixed:** the model is permissionless, the ruleset is not.
- **D7 — What "a model is data" still needs,** in order.
- **D8 — A Position is a membership, never money** (the market, spec 15).

## Consequences

- The boundary in D6 is the one ADR-0144 P7 restates.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../design/palw/archive/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
