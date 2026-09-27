# ADR-0095 — A position is a membership, not an income

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0095-a-position-is-a-membership-not-an-income.md](../design/palw/archive/0095-a-position-is-a-membership-not-an-income.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-07. It amends ADR-0087 D1's "grants nothing but the right to sell it back":
  a position also grants what its line declares. The benefit declarations are built
  (`ModelLineBenefitsDeclared`); the rest is design.
* Date: 2026-09-07

## Context

The operator asked what a position is for, if it pays no income.

## Decision

- A position buys no income and no vote. It buys what the line's developer owes its holders: the new
  version first, the private beta, the front of the queue, experimental modes, and a voice in what
  ships next.
- The boundary between the product (the line's) and the serving (the providers') is drawn, and each
  side is named.

→ spec 15 PALW-MK-9.

## Consequences

- ADR-0101 carries the membership half: proven by the chain, served by anyone.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0095-a-position-is-a-membership-not-an-income.md](../design/palw/archive/0095-a-position-is-a-membership-not-an-income.md)
