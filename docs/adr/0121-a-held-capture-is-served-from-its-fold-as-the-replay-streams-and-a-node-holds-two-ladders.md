# ADR-0121 — A held capture is served from its fold as the replay streams, and a node holds two ladders

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md](../design/palw/archive/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed and **implemented 2026-09-12**. It is ADR-0119's node half and consensus-inert: every
  streamed object is the one the whole-capture path would produce.
* Date: 2026-09-12

## Context

A held capture is too large to materialize for every request, but a node must still serve, check,
name and prosecute it.

## Decision

- **D1 — Two ladders:** the class's walks, and the network's materializes.
- **D2 — Every held route streams.**
- **D3 — A held class folds at 12,** and a block request counts from its interval.
- **D4 — What is refused rather than built.**
- **D5 — A lie at an interval's edge is named from the edges.**

→ spec 14 PALW-ND-13.

## Consequences

- Held captures are served from their fold as the replay streams.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md](../design/palw/archive/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md)
