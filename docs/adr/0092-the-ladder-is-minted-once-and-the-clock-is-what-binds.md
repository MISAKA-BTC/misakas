# ADR-0092 — The ladder is minted once, and the wall clock is what binds

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md](../design/palw/archive/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Accepted 2026-09-06 (design first, at the operator's word); §8 records the implementation.
* Date: 2026-09-06

## Context

A model's width costs the court rounds, not leaves, and rounds are logarithmic. What stops a bigger
model is that every round costs wall-clock time inside the court window.

## Decision

- **D1 — A mainnet mints its ladder at the top of the wall-clock budget,** not at the width of today's
  models.
- **D2 — The class-admission gate keeps all three refusals,** named as one rule.
- **D3 — The arity is the protocol-level knob** that trades dissection depth against per-round cost.
- **D4 — A model too wide for the minted ladder is a new class on a new ruleset,** not a raised ladder.
- **D5 — The time-direction split is the free-prompt lane's.** The attempt lane does not need it.

→ spec 09 PALW-CT-21.

## Consequences

- The ladder is a one-time consensus commitment. The court window, not model size, is the binding
  constraint.

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md) · [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md](../design/palw/archive/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
