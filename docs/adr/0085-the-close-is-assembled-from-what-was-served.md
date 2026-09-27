# ADR-0085: The close is assembled from what the executor served — a disputed tile, not a capture

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0085-the-close-is-assembled-from-what-was-served.md](../design/palw/archive/0085-the-close-is-assembled-from-what-was-served.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Proposed 2026-09-04; §6 items 1–3 landed on 2026-09-04, and items 4–5 were built with
  ADR-0086 D6's transport. Consensus-inert by construction.
* Date: 2026-09-04

## Context

ADR-0084 U-07c: a challenger that holds no capture still needs to assemble the court's close. The
material exists: the executor serves it in the interval opening.

## Decision

- **D1 — The interval opening gains a close annex,** served on the same authenticated lane.
- **D2 — The refutation is assembled** from an opening and the challenger's own replay.
- **D3 — The terminal arm tries the opening path before it pulls.**
- **D4 — A seat's `Fault { leaf_index }` is a court case it can prosecute.**

→ spec 09 PALW-CT-18.

## Consequences

- The court no longer depends on a challenger having kept a capture.

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0085-the-close-is-assembled-from-what-was-served.md](../design/palw/archive/0085-the-close-is-assembled-from-what-was-served.md)
