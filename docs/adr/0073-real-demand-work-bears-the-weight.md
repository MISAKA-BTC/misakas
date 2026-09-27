# ADR-0073: Real-demand work bears the weight

> **Body moved (2026-09-27).** Normative rules → [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0073-real-demand-work-bears-the-weight.md](../design/palw/archive/0073-real-demand-work-bears-the-weight.md); the reasoning is summarised in [design/palw/free-prompt.md](../design/palw/free-prompt.md).

* Status: Governing in part.
  - **Status amendment (2026-09-18, ADR-0137):** past `palw_work_target` a share is a result, so
    Phase ④'s share mechanics are not read.
  - Phases ①–③ are in force.
  - **ADR-0144 alignment (2026-09-21):** D2's self-prompt reading ("exactly as good as a canonical job")
    is withdrawn as a PALW-reward claim, and Phase ④ is withheld until ADR-0144 §6 items 2–3.
  - The security amendment of 2026-09-02 (preconditions) still binds if ④ is built.
* Date: 2026-09-02

## Context

The finding was that free-prompt work, the real demand, bore no fork-choice weight, while synthetic
canonical jobs carried all of it.

## Decision

- **D1 — Phase ①:** a free-prompt claim is disputed in the same court as an attempt. → spec 11
  PALW-FP-7.
- **D2 — Phase ②:** real-demand work bears weight. *Its self-prompt reading is withdrawn.*
- **D3 — Phase ③:** one unit on both lanes. *Now the derived work of ADR-0148/0149.*
- **D4 — Phase ④:** weight and share move to the lane that does the work. *Withheld.*
- **D5 — Rollout.**

## Consequences

- A self-prompt nobody asked for is Attempt by another name (ADR-0144 P2/P3).

## Links

- Spec: [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/free-prompt.md](../design/palw/free-prompt.md)
- Full text as written: [design/palw/archive/0073-real-demand-work-bears-the-weight.md](../design/palw/archive/0073-real-demand-work-bears-the-weight.md)
