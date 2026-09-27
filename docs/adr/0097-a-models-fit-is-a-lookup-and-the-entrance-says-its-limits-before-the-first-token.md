# ADR-0097 — A model's fit is a lookup, and the entrance says its limits before the first token

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md](../design/palw/archive/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed and **implemented 2026-09-10, consensus-inert**: no fence, no `Params` field, no
  object, no fingerprint move.
* Date: 2026-09-10

## Context

The chain could carry a model the size of Kimi K3 at ten positions of context, and nothing wider,
because of nine named walls, four of them numbers inside the ruleset id. Nobody could see which wall
refused what.

## Decision

- **D1 — The fit is a lookup** (`palw_model_fit_v1`).
- **D2 — The entrance says its limits before the first token** (`GET /v1/models`).
- **D3 — The geometry ceiling is a wall with a name,** and this ADR does not move it.
- **D4 — A stand-in is a verdict, never a row.**
- **D5 — What a network that wants a K3-class model at long context must mint with.**
- **D6 — What "practical use" means on this lane,** with the sixty-item checklist in Appendix A.

→ spec 14 PALW-ND-15.

## Consequences

- Limits are stated up front instead of discovered by refusal.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md](../design/palw/archive/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
