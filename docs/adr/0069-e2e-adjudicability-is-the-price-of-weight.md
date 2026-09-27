# ADR-0069 — End-to-end adjudicability is the price of weight

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0069-e2e-adjudicability-is-the-price-of-weight.md](../design/palw/archive/0069-e2e-adjudicability-is-the-price-of-weight.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Accepted and **implemented (2026-09-01; reviewed and amended 2026-09-02)**.
  - **Amended by ADR-0075:** the certified set is genesis ∪ chain, and an entrant is seated by an
    object, not by a regenesis.
  - **ADR-0144 alignment (2026-09-21):** the leftover pwu hole is closed by shrinking Attempt, not by
    adding synthetic weight.
* Date: 2026-09-01

## Context

The test the goal sets: no class may carry fork-choice weight unless the court can try every step of
its execution end to end. Kernel-level adjudicability had been measured, but whole-graph adjudicability
had not.

## Decision

- **D1 — Two adjudicability properties, named apart:** kernel coverage and end-to-end.
- **D2 — End-to-end certification is committed like the catalogue.** *Now a consensus object
  (ADR-0075).*
- **D3 — The certification drill, defined.**
- **D4 — The graph must not lie about the engine** (ADR-0049 F, enforced).
- **D5 — The admission gate grants weight only to certified families.** → spec 03 PALW-CL-15.
- **D6 — Permissionlessness and the doctrine are preserved:** an uncertified class may register, and
  weighs nothing.
- **D7 — An uncertified family's blocks weigh nothing.**

## Consequences

- The security amendment of 2026-09-02 (the open item as a fork-choice question) is in the full text.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0069-e2e-adjudicability-is-the-price-of-weight.md](../design/palw/archive/0069-e2e-adjudicability-is-the-price-of-weight.md)
