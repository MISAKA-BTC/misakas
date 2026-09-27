# ADR-0050: The BASE-0 residual site — the narrowing that was never declared, and the amplification that was

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0050-palw-base0-residual-site.md](../design/palw/archive/0050-palw-base0-residual-site.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Proposed 2026-08. It changes no op and amends no catalogue. **Governing:** the residual site's
  narrowing node and gain parameters are part of the dense classes. Decisions A–D are transcribed into
  [spec/palw/04a](../spec/palw/04a-integer-arithmetic.md). It was blocked on ADR-0049 Decision A (E).
* Date: 2026-08

## Context

The question was whether a BASE-0 residual add may amplify. The larger finding was a declared graph
that could not be adjudicated at its residual sites, because a narrowing was never declared.

## Decision

- **A — The residual site gains its narrowing node** (a correctness fix).
- **B — The residual may amplify,** and its gain is a registration artifact.
- **C — The gain is per tensor, per layer, per site,** not per channel.
- **D — The new parameters join the artifact inventory** as named tensors.
- **E — This is blocked on ADR-0049 Decision A,** and the block is not incidental.

## Consequences

- The residual path is adjudicable. Its parameters are pinned like weights.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0050-palw-base0-residual-site.md](../design/palw/archive/0050-palw-base0-residual-site.md)
