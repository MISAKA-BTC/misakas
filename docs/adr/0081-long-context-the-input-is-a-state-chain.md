# ADR-0081: Long context — the input is a state chain

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0081-long-context-the-input-is-a-state-chain.md](../design/palw/archive/0081-long-context-the-input-is-a-state-chain.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: **Superseded in part by ADR-0082 (2026-09-03).** §3's prompt as a chain of prefill segments is
  withdrawn, except Decision 3: `prompt_token_ids_hash` becomes a Merkle root over the ids.
* Date: 2026-09-03

## Context

A long prompt had to be verifiable without a court or a seat holding the whole capture.

## Decision

- **D1 — The prompt is a chain of prefill segments.** *Withdrawn.*
- **D2 — The transition is what is committed and verified.** *Withdrawn.*
- **D3 — `prompt_token_ids_hash` becomes a Merkle root over the ids.** *Kept.* → spec 04 PALW-EX-11.
- **D4–D9** (segment widths, tiles as addressing, no reward multiplication, one canonical root,
  coverage as a rule, "GPT-like is two ADRs"). *Withdrawn or absorbed by ADR-0082.*

## Consequences

- ADR-0082 replaced the segment chain with a flat close and a folded capture.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0081-long-context-the-input-is-a-state-chain.md](../design/palw/archive/0081-long-context-the-input-is-a-state-chain.md)
