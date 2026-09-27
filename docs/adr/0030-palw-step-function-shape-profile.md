# ADR-0030: The PALW step function, pinned at tile granularity — shape profile v3

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0030-palw-step-function-shape-profile.md](../design/palw/archive/0030-palw-step-function-shape-profile.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Accepted. The schema is frozen, and every class-specific value is measured at registration.
* Date: 2026-08-16

## Context

ADR-0027's one-step refutation needed its step function pinned: which operator, which tile shape,
which reduction order `shape_profile_id` binds. Facts were read from the pinned tree and the kernel
internals.

## Decision

- **1 — The step space.**
- **2 — Shape profile v3:** what `shape_profile_id` binds.
- **3 — The step leg:** execution commitment v2.
- **4 — Adjudication:** `ExecutionStepRefutationV1` becomes implementable.
- **5 — Validation gates** before any class registers a v3 profile.

→ spec 04 PALW-EX-10.

## Consequences

- The design basis for the step leg, the canonical transcendentals (ADR-0031) and the refutation
  object.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0030-palw-step-function-shape-profile.md](../design/palw/archive/0030-palw-step-function-shape-profile.md)
