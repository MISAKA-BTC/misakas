# ADR-0111 — A seat may demand the committed leaf it needs to judge

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md](../design/palw/archive/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Proposed and **implemented 2026-09-11**; both paths were drilled to a slash on a devnet. It
  rides the held regime and ADR-0062's court. **Amended on testnet-12 by ADR-0152 DA-3:** past
  `palw_rcore_plus` a named `StepLeaf` is free, inside the committed bound, and keyed by the DA session.
* Date: 2026-09-11

## Context

A seat judging a held-context claim does not retain the whole capture. To judge a leaf it may need
committed material that only the executor holds.

## Decision

- **D1 — The unit is a leaf's evidence** (`PalwLeafEvidenceV1`), which is the one-move court's object.
- **D2 — The fast path:** a signed leaf request on the interval lane, off chain.
- **D3 — The slow path:** a demand on chain (`DefaultAccusedHeld`, `StepLeaf`), bounded by chain facts.
- **D4 — The answer is an adjudication:** `MaterialDisclosedHeld` with the evidence, judged by the
  verdict of D1.
- **D5 — The held regime needs the data-availability court.** `palw_held_context` refuses to arm without
  `palw_da_court` and `palw_fp_da_pins`.
- **D6 — The node carries both halves, and they ship together.**
- **D7 — The chain bounds the executor's burden:** one demand per seat per claim, for a leaf that seat
  was assigned.

## Consequences

- Withholding a leaf a seat needs becomes prosecutable (spec 08 PALW-VF-35).

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md](../design/palw/archive/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
