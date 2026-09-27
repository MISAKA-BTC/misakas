# ADR-0103 — The context is held off the chain, and the chain carries a root, an opening and a logarithm

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md](../design/palw/archive/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed 2026-09-11. **Implemented 2026-09-11** (§10: every decision built, with the
  generator's 2M table replacing §1.2's arithmetic). The held regime is armed on testnet-12 from
  genesis.
* Date: 2026-09-11

## Context

A 2M context is not refused by one number. It is refused by six terms that grow with the context, and
each is a term the chain carries, walks or waits on for something the executor or a seat could hold
instead.

## Decision

- **D1 — The court opens at a named leaf,** and nobody walks the leaf ladder.
- **D2 — The seat's unit is an interval of positions** over the whole job, prefill included.
- **D3 — State chunk map v4:** append-only, with the checkpoint root as a frontier.
- **D4 — The ids never ride above one standard transaction,** and a DA accusation names a unit.
- **D5 — A fused leaf's dissection opens at the named leaf,** with the arity derived.
- **D6 — The registration gate costs one position,** and the geometry ceiling is a published number.
- **D7 — A seat holds a shard of the model and of its state.**
- **D8 — Every wall prints its order,** and R-held is a predicate.
- **D9 — What earns and what is at stake do not change,** and the deterrent at 2M is priced.

→ spec 04 §4.5, 09.

## Consequences

- Held context scales to 2M at bounded chain cost. The 2M row itself stays closed until ADR-0153.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md](../design/palw/archive/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
