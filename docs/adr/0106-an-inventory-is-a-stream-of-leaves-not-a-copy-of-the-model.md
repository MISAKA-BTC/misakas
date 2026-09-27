# ADR-0106 — An inventory is a stream of leaves, not a copy of the model

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md), [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md](../design/palw/archive/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed 2026-09-11. **W1–W7 implemented, consensus-inert.** The leaf, its preimage, the
  tree, the layout rules and every opening are the ones the court already used. Only how a node builds
  them changed.
* Date: 2026-09-11

## Context

Building an artifact's inventory root held every row in memory at once. For a large model that alone
exhausted the host.

## Decision

- **W1 — The leaf over borrowed parts.**
- **W2 — Rows without bytes,** and the rules spelled once.
- **W3 — One emitter,** with sinks for the rest.
- **W4 — Row-sized scratch.**
- **W5 — The measurement streams.**
- **W6 — The registration root streams.**
- **W7 — The frontier:** one peak per level (`PalwArtifactMerkleFrontierV1`).

→ spec 14 PALW-ND-12, 04 PALW-EX-13.

## Consequences

- An inventory is a stream of leaves, not a copy of the model.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md](../design/palw/archive/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md)
