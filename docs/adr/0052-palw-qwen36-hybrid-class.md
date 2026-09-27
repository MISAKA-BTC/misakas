# ADR-0052: `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid graph

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0052-palw-qwen36-hybrid-class.md](../design/palw/archive/0052-palw-qwen36-hybrid-class.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Proposed 2026-08-26. **Its same-day amendment decided calibration and the court.** Qwen3.6
  is adjudicable end to end and weight-bearing once certified (ADR-0069, 0070). Decisions A–F are
  transcribed into [spec/palw/04a](../spec/palw/04a-integer-arithmetic.md).
* Date: 2026-08-26

## Context

ADR-0040 had declined the hybrid graph (routed experts, gated delta rule, partial rotation). Qwen3.6
then went through the integer runtime with 100 % kernel-catalogue coverage.

## Decision

- **A — Everything ADR-0040 says still holds.**
- **B — The router is a selection,** and its tie rule is normative.
- **C — The combine has one accumulator.**
- **D — `IntLn`, the fourth transcendental.**
- **E — The gated delta rule,** and why an integer state is stable.
- **F — Partial rotation is not an optimisation.**

→ spec 04 PALW-EX-4, 04a.

## Consequences

- ADR-0051's motive, a native-speed family for this model, expired.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0052-palw-qwen36-hybrid-class.md](../design/palw/archive/0052-palw-qwen36-hybrid-class.md)
