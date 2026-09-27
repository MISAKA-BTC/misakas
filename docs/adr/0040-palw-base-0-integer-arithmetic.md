# ADR-0040: `PALW-BASE-0` — the integer-only arithmetic normative specification

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0040-palw-base-0-integer-arithmetic.md](../design/palw/archive/0040-palw-base-0-integer-arithmetic.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Proposed 2026-08-17 as the arithmetic's normative specification. **Governing:** implemented
  as `PALW-BASE-0`, extended by ADR-0047 (A16) and ADR-0050 (the residual site). **Its Decision sections
  A–F and H are transcribed verbatim into [spec/palw/04a](../spec/palw/04a-integer-arithmetic.md).**
* Date: 2026-08-17

## Context

ADR-0039 required a liveness floor that any CPU can verify and any court can convict on. That meant
an integer-only class whose kernel catalogue closes. The float classes need seventeen op kinds and
libm, which cannot be adjudicated portably.

## Decision

- **A — Integer-only means integer-only.**
- **B — Representation.**
- **C — The three arithmetic rules,** stated once and used everywhere.
- **D — The op set, closed and minimal.**
- **E — Reduction order is free,** and this is the class's central property.
- **F — The two integer transcendentals, as algorithms.**
- **G — What this buys the catalogue** (rationale, kept in the full text).
- **H — `Rescale`, the tenth op:** a scale change that is allowed to amplify.

→ spec 04 §4.2, 04a.

## Consequences

- The floor class can be verified and convicted on any CPU. Every model class since reuses its
  arithmetic.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0040-palw-base-0-integer-arithmetic.md](../design/palw/archive/0040-palw-base-0-integer-arithmetic.md)
