# ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps publishing

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md), [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../design/palw/archive/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-05. It was **revised the same day** (the first draft's exam was withdrawn by
  the operator: "not distributed training, a distributed market"), and **implemented the same day**
  behind `palw_model_lines`. It amends ADR-0056 D6 (roots in force) and ADR-0087 D1/D4/D7.
* Date: 2026-09-05

## Context

A model's owner keeps publishing new weights, but a class is its graph, and it is the unit of work,
certification and the court. Those must not move when weights do.

## Decision

- **D1 — A line is `(class, owner, name)`.** The class's own line is the first, and its id is the class
  id.
- **D2 — A version is one signed object,** signed by the developer.
- **D3 — The roots in force for a class are the union of its lines'.**
- **D4 — Usage is counted by the fold,** per version.
- **D5 — Evaluations are declarations,** from anyone.
- **D6 — Roles:** owner, developer and maintainer, and the owner may hand the line over.
- **D7 — Proposals:** open research, recorded, and paid when adopted.
- **D8 — The registrant leg is the owner's,** by line.
- **D9 — The market is keyed by line.**
- **D10 — State,** and no root bump.
- **D11 — Armed by activation,** never by regenesis.
- **D12 — What a participant reads and does.**

→ spec 15 §15.1, 03 PALW-CL-18.

## Consequences

- The security section (the four principles, checked) is in the full text.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md) · [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../design/palw/archive/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
