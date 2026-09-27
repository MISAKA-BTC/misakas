# ADR-0074: The attempt is a claim, drawn by the chain

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/05](../spec/palw/05-canonical-work.md); the full text as written → [design/palw/archive/0074-the-attempt-is-a-claim-drawn-by-the-chain.md](../design/palw/archive/0074-the-attempt-is-a-claim-drawn-by-the-chain.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: Implemented (2026-09-02, branch `palw-adr0073-fp-weight`).
  - **Status amendment (2026-09-18, ADR-0137):** past `palw_work_target` a class's share is a result.
    The class target, class DAA, epoch budget and admission share this ADR reads are not read past the
    fence.
  - D2 (the beacon draw) and D4 (one inference, one claim) stand.
  - D5's leaf price is superseded by the derived work (ADR-0148, 0149).
* Date: 2026-09-02

## Context

The attempt lane was self-drawn, while the free-prompt lane drew from a beacon. A validator-drawn
alternative would have reintroduced a trusted party.

## Decision

- **D1 — Every paid unit of LLM work is a claim committed before its draw.**
- **D2 — The draw is the chain's beacon** (`fp_quantum_ticket_v3`). → spec 06 §6.1.
- **D3 — The self-drawn lane stays,** as the liveness-and-beacon lane.
- **D4 — One inference, one claim.**
- **D5 — The price is the capture's leaf count, and a quantum is an eighth of the canonical job.**
  *Superseded by derived work.*
- **D6 — The floor's free-prompt lane bears weight.**
- **D7 — Rollout:** job version 3 → 4.

## Consequences

- The order "commit, then draw" of ADR-0144 §4 is enforced on both lanes.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [05 Canonical work](../spec/palw/05-canonical-work.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0074-the-attempt-is-a-claim-drawn-by-the-chain.md](../design/palw/archive/0074-the-attempt-is-a-claim-drawn-by-the-chain.md)
