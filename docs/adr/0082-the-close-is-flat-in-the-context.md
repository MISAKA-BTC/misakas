# ADR-0082: The close is flat in the context — attention is refuted by dissection, the capture is a fold, and the answer is what earns

> **Body moved (2026-09-27).** Normative rules → [spec/palw/04](../spec/palw/04-execution-semantics.md), [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0082-the-close-is-flat-in-the-context.md](../design/palw/archive/0082-the-close-is-flat-in-the-context.md); the reasoning is summarised in [design/palw/held-context.md](../design/palw/held-context.md).

* Status: **Implemented (2026-09-04)** for the testnet-11 Relaunch 5f cut. §10 is the implementation
  record and corrects every figure the measurement moved. It supersedes ADR-0080 and ADR-0081 in part.
* Date: 2026-09-03

## Context

A person types a prompt of thousands of tokens and receives an answer of thousands more, and the chain
must hold one claim whose verification is bounded in bytes and in compute. ADR-0080 and ADR-0081 tried
segment chains, and the measurement refuted them.

## Decision

- **Part A — The court is flat in the context.**
  - **D1:** attention is one fused node per site.
  - **D2:** a fused leaf is refuted by dissection.
  - **D3:** the dissection is k-ary.
  - **D4:** the bottom opens tiles.
  - **D5:** prompt ids ride as a Merkle root.
  - **D6:** the close ceiling is re-derived from the flat terms.
  - → spec 09 §9.2, 04.
- **Part B — The executor is flat in the context.**
  - **D7:** the capture is a fold.
  - **D8:** the executor prices against the ruleset's ladder.
  - → spec 04 PALW-EX-15.
- **Part C — The seat is bounded in bytes.**
  - **D9:** it recomputes the cache from the prompt it holds.
- **Part D — What earns.**
  - **D10:** free-prompt quanta are earned by decode leaves.
  - **D11:** decoding is a seeded argmax.
  - **D12:** the lane's throughput ceiling is derived.
  - → spec 11. *The decode rules of D10/D11 (`palw_fp_decode_rules`) are dormant on testnet-12.*
- **Part E — D13:** Ambient, named: what is borrowed and what is refused.

## Consequences

- One claim covers a long answer at bounded cost. The measurements of §1 and the implementation
  record of §10 are in the full text.

## Links

- Spec: [04 Execution semantics](../spec/palw/04-execution-semantics.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md) · [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/held-context.md](../design/palw/held-context.md)
- Full text as written: [design/palw/archive/0082-the-close-is-flat-in-the-context.md](../design/palw/archive/0082-the-close-is-flat-in-the-context.md)
