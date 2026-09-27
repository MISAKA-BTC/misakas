# ADR-0146 — A coefficient is justified by the arbitrage it permits, not by being right

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md); the full text as written → [design/palw/archive/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md](../design/palw/archive/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: **Search implemented, 2026-09-21. The bound measured 1.000000×.** There is no coefficient
  table, no fence and no `Params` field. A scalar CCU is not to be armed (ADR-0131 D3–D6). ADR-0147,
  0148 and 0149 may arm without contradicting P4.
* Date: 2026-09-21

## Context

The trap this ADR exists to avoid is choosing coefficients because they look right. The right
question is what arbitrage a coefficient would permit a miner who shapes work to exploit it.

## Decision

- **§2 — Most dimensions need no coefficient at all.**
- **§3 — The remaining coefficients cannot be validated by being "right".** Only a bound on the
  arbitrage they permit can justify them.
- **§4 — What the bound must be, and what happens if none is reached:** no coefficient.
- **§5 — Governance** of any future coefficient.
- **§9 — The bound was measured, and the answer was not a table.** → spec 05 PALW-WK-4.

## Consequences

- The controlled cross-hardware experiment (§6) cannot be run on this fleet. The full text says why,
  and what would be needed.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md](../design/palw/archive/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)
