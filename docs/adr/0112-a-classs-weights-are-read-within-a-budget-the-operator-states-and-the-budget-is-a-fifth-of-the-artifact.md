# ADR-0112 — A class's weights are read within a budget the operator states, and the budget is a fifth of the artifact

> **Body moved (2026-09-27).** Normative rules → [spec/palw/14](../spec/palw/14-node-duties.md); the full text as written → [design/palw/archive/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md](../design/palw/archive/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md); the reasoning is summarised in [design/palw/node.md](../design/palw/node.md).

* Status: Proposed 2026-09-11, after testnet-11's producers were measured. **D1–D5, D7 and D8
  implemented (§10), consensus-inert.**
* Date: 2026-09-11

## Context

The same model and the same weights, run in different ways, used very different memory on the fleet.
Which bytes are resident is a runtime decision, and it should be made under a budget the operator
states, not by the kernel's page cache.

## Decision

- **D1 — The forward pass reads weights through the file descriptor** into memory the runtime owns.
- **D2 — The budget is one number, stated by the operator.** The default is a fifth of the artifact.
- **D3 — Two tiers, told apart by name,** and a floor.
- **D4 — A layer's experts are read together,** the moment its router commits.
- **D5 — The class's identity and arithmetic are untouched,** and a test says so.
- **D6 — The fleet measurement,** and the numbers this ADR is judged by.
- **D7 — The operator's arithmetic, printed** at load.
- **D8 — What one draw reads from storage is a log line.**

→ spec 14 PALW-ND-11.

## Consequences

- Hosts with less memory can serve large classes, trading reads for residency.

## Links

- Spec: [14 Node duties](../spec/palw/14-node-duties.md)
- Design: [design/palw/node.md](../design/palw/node.md)
- Full text as written: [design/palw/archive/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md](../design/palw/archive/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
