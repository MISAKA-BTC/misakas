# ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md), [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md](../design/palw/archive/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed 2026-09-11. **D1–D5 implemented (§9), consensus-inert.** The 512 and 2,048 vectors
  run in CI, and the 2M vector is published.
* Date: 2026-09-11

## Context

A wider context had been armed from measurements taken on the maintainer's workstation. Nobody else
could reproduce the evidence a release rested on.

## Decision

- **D1 — A vector is a name, a seed and a geometry.** Everything else is derived.
- **D2 — The verifier is the network's pipeline,** stage by stage, through the node's own seam.
- **D3 — One canonical document,** whose id covers only what must agree.
- **D4 — A receipt is evidence, not a vote.**
- **D5 — CI runs the small widths on every change.** The 2M vector is published, not assumed.
- **D6 — Arming a wider context is a release,** and the release cites its evidence.
- **D7 — The document says what a vector does not prove.**

→ spec 03 PALW-CL-23.

## Consequences

- Context limits are armed from reproducible public evidence.

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md) · [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md](../design/palw/archive/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
