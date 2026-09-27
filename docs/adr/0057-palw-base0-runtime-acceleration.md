# ADR-0057: BASE-0 runtime acceleration — backends below the semantic boundary

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md), [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0057-palw-base0-runtime-acceleration.md](../design/palw/archive/0057-palw-base0-runtime-acceleration.md); the reasoning is summarised in [design/palw/execution.md](../design/palw/execution.md).

* Status: Accepted. **Consensus-inert:** no class id, catalogue entry, ruleset field or fingerprint moves
  under anything it permits.
* Date: 2026-08

## Context

The question was how BASE-0 could run fast. The wrong answer was to certify kernels.

## Decision

- **D1 — The semantic boundary is the catalogued kernel.** → spec 04 PALW-EX-2.
- **D2 — No kernel certificates.**
- **D3 — The gate is differential, per backend,** and it must fire.
- **D4 — The order of work.**
- **D5 — What the survey suggested is refused,** by name.
- **D6 — Fusion stops at the committed row.**

→ spec 14 PALW-ND-14.

## Consequences

- Efficiency is the miner's to gain (P5), and it never moves the arithmetic.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/execution.md](../design/palw/execution.md)
- Full text as written: [design/palw/archive/0057-palw-base0-runtime-acceleration.md](../design/palw/archive/0057-palw-base0-runtime-acceleration.md)
