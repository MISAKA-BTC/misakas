# ADR-0078: What was made from it is committed; the thing itself never rides

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md](../design/palw/archive/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Proposed (2026-09-02); implementation on `palw-adr0078-impl`. **ADR-0144 alignment
  (2026-09-21):** D7 (a derivation transformer earns PALW leaf weight) is withdrawn, unless the
  transformer is the user's local inference itself (P3).
* Date: 2026-09-02

## Context

ADR-0077 makes an inference at a usable width a certified, spendable claim. It does not make "generate
a 3D model with Qwen3.6, verified end to end" a claim. A line had to be drawn between the inference
and what is made from it.

## Decision

- **D1 — A derived artifact is a derivation, committed; the artifact never rides.** → spec 11
  PALW-FP-10.
- **D2 — The DSL is the claim's output,** canonicalized by a registered grammar.
- **D3 — Transformers are content-named pure functions.**
- **D4 — The object, and what the chain checks.**
- **D5 — Verification belongs to the consumer,** and the chain makes it possible.
- **D6 — Delivery,** and what is under a data-availability obligation.
- **D7 — A transformer that is a step space is a class.** *Withdrawn as a reward path.*
- **D8–D11 — The kind table,** the kind space, four modes beyond generation, and the order in which
  domains open.

## Consequences

- PALW's reward stays on the inference, and derived work is provenance.

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md](../design/palw/archive/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
