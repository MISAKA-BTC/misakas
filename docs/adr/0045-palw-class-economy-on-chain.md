# ADR-0045: The class economy is chain state — derived PWU, block-denominated epoch budgets, and the registration-granted share table

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0045-palw-class-economy-on-chain.md](../design/palw/archive/0045-palw-class-economy-on-chain.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: Accepted (implemented on the V2 lineage).
  - D1's `DerivedV1` expected-attempts term is drawn once per inference (ADR-0072, 0076).
  - D2's block-denominated epoch budget stands where budgets remain.
  - D3's share table went to ADR-0054 and then to ADR-0137, where a share is a result.
* Date: 2026-08-20

## Context

The epoch cap's currency had been elected in pwu, which starved every above-mean class. Shares were
params constants.

## Decision

- **D1 — pwu has exactly one legal value:** the derivation. → spec 05 PALW-WK-7.
- **D2 — The epoch budget's currency is the block.** → spec 05 PALW-WK-15.
- **D3 — The share table is chain state,** granted at registration and conserved by arithmetic.
  *Superseded by ADR-0137.*

## Consequences

- Defects (a)–(e) of ADR-0039 D5's amendment are closed.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0045-palw-class-economy-on-chain.md](../design/palw/archive/0045-palw-class-economy-on-chain.md)
