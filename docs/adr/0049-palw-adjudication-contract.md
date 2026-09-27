# ADR-0049: The adjudication contract — what a court opens, and the bound that makes it model-size-independent

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0049-palw-adjudication-contract.md](../design/palw/archive/0049-palw-adjudication-contract.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Proposed (activates nothing), and governing as amended:
  - D-C was amended on 2026-08-26: the court-cost ceilings became numbers, and `max_close_bytes` =
    80 KiB is checked by the RC identity gate.
  - D-H was refined by ADR-0069 D5/D6 and ADR-0075 D5: an uncertified family takes share 0, and a
    certified entrant is seated by a chain object.
* Date: 2026-08

## Context

The central claim, that a court's cost is independent of model size, was measured and did not hold
as specified. The adjudication contract had to say what a court opens and what bound makes it
model-size-independent.

## Decision

- **A — Operands are addressed in bytes,** under a pinned canonical encoding.
- **B — The terminal adjudication is tile-local.**
- **C — Admission bounds the court from the class's own geometry** (as amended: numeric ceilings,
  80 KiB closes).
- **D — Coverage is over reachable coordinates,** not over kernel ids.
- **E — Decode is adjudicable** by challenging the argmax rather than proving it.
- **F — The engine, the profile, the adjudicator and the inventory are projections of one
  description.**
- **G — One canonical artifact inventory,** and one meaning for "class id".
- **H — Post-genesis class registration is allowed,** at the minimum share, gated by Decision C.

→ spec 09 §9.1, 03.

## Consequences

- Permissionless registration (ADR-0056, 0135) rests on H.
- "The refusal that stood here was never a policy, it was the absence of a check" (ADR-0144 §2).

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md) · [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0049-palw-adjudication-contract.md](../design/palw/archive/0049-palw-adjudication-contract.md)
