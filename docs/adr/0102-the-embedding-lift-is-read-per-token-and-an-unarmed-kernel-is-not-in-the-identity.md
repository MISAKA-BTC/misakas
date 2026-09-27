# ADR-0102 — The embedding lift is read per token, and a kernel a network has not armed is not in its identity

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md](../design/palw/archive/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Proposed 2026-09-10. **D1–D4 implemented, consensus-inert** on every shipped preset. The fence
  `palw_token_lift` is armed on testnet-12 from genesis. D5 (the table row and the arming) is stated,
  not built.
* Date: 2026-09-10

## Context

The hybrid engine lifts each token's embedding row by that token's calibrated triple, but every hybrid
graph declared the lift per lane. So no calibrated hybrid artifact had an inventory, a measurement or
a court.

## Decision

- **D1 — Graph-v6 is graph-v5 with the lift read per token.**
- **D2 — A kernel a network has not armed is not in its identity.**
- **D3 — A graph-v6 registration pins the operand-inventory root.**
- **D4 — The manifest names the graph.**
- **D5 — Not the canonical table yet,** and why that is right.
- **D6 — `supports_court()` is not narrowed.**

→ spec 04 PALW-EX-7, PALW-EX-8.

## Consequences

- The calibrated hybrid is measurable and adjudicable.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md](../design/palw/archive/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
