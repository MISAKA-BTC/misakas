# ADR-0117 — A draw is one forward

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md), [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md); the full text as written → [design/palw/archive/0117-a-draw-is-one-forward.md](../design/palw/archive/0117-a-draw-is-one-forward.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Proposed and **implemented 2026-09-11**, reversing ADR-0112 §8 at the operator's
  instruction. The prefill is consensus-neutral (bit-identical rows). The one-forward ticket is behind
  `palw_prefill_draw`, which is armed on testnet-12 from genesis.
* Date: 2026-09-11

## Context

A draw's job had included decode calls, so each draw read the weights several times.

## Decision

- **D1 — Past the fence, the attempt's job is the canonical job without its decode calls.**
- **D2 — The prefill is one pass over the weights.**
- **D3 — A held material answers the whole job its block asked for.**
- **D4 — The price stays the canonical job's.**

→ spec 04 PALW-EX-12, 06 PALW-EL-6.

## Consequences

- Every weight is read once per draw.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md) · [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0117-a-draw-is-one-forward.md](../design/palw/archive/0117-a-draw-is-one-forward.md)
