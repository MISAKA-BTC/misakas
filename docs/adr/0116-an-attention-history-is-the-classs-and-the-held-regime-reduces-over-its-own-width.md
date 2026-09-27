# ADR-0116 — An attention history is the class's, and the held regime reduces over its own width

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md](../design/palw/archive/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: Proposed and **implemented 2026-09-11**, reversing ADR-0103 §10.7's decision of the same day
  at the operator's instruction.
* Date: 2026-09-11

## Context

The A16 attention-history bound had been tied to the projection tier. A held class needed its own
width.

## Decision

- **D1 — The bound is read off the class** (`palw_attn_history_bound_v1`): `2^21` for the held regime,
  and `2^18` otherwise.
- **D2 — The ops take the bound,** and the old names keep the old one.
- **D3 — Every consumer reads the same bound,** the court included.
- **D4 — A held context past the bound is refused at the gate.**
- **D5 — The vectors' wall is their class's.**

→ spec 04 PALW-EX-16.

## Consequences

- One bound per class, read identically by the executor, seats and the court.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md](../design/palw/archive/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
