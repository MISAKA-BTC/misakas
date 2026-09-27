# The court: adjudication, dissection and attribution — design

> **Not normative.** This document explains why the rules in
> [spec/palw/09-court-and-offences.md](../../spec/palw/09-court-and-offences.md) are what they are.

**Decisions recorded in:** ADR-0026 D3, ADR-0027, ADR-0049, ADR-0053, ADR-0069, ADR-0070, ADR-0080
(superseded in part), ADR-0082, ADR-0085, ADR-0086, ADR-0092, ADR-0093, ADR-0100,
[ADR-0152](../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) (J).
**Full texts:** [archive/](archive/) (listed below), and [archive/0152/05](archive/0152/05-rules-j-da.md)
for attribution.
**Last revised:** 2026-09-27

## 1. Problem

A lie in one step of a billion-parameter inference must be provable on any CPU. The proof must be
bounded in bytes, whatever the model size or context length. It must bind the bond that told the lie,
and never an honest seat that replayed the committed execution.

## 2. The design in one paragraph

Refute one committed step with canonical integer arithmetic. Where the challenger lacks the inputs,
narrow by k-ary dissection, with each rung on a deadline and silence counted as a default.

- The close is assembled from what the executor already served (the interval opening's annex), so
  the challenger needs no capture.
- Attention is one fused node, refuted by dissection over its history.
- A decode token is refuted by opening two tiles of its row.
- Admission refuses any class the court could not try. The ladder is minted once per ruleset, so the
  binding constraint is the court's wall-clock window, not the model's size.
- Attribution (F1/F2) ties every conviction to a root and a job identity, so a borrowed root can never
  convict its honest lender.

## 3. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| A tolerance in the slashing verdict | It turns fraud proofs into votes on noise | archive 0026 D3 |
| A quorum of verifiers deciding fraud | Unilateral fraud proofs need no honest majority | archive 0027 |
| One job cut into N verification segments | The measured refutation (context width, not length) | archive 0080 §3 (withdrawn) |
| Binary bisection | k-ary dissection fits the move budget in fewer rounds | archive 0082 D3 |
| A claimed-value tiled pin | "Fabricate a value above the row" has somewhere to hide; two opened tiles do not | `palw_step_refute.rs` |
| Raising the ladder for a wider model | The ladder is a one-time consensus commitment; a wider model is a new class | archive 0092 D4 |

## 4. Threat model

- **The honest seat's safety** under attribution: a dissection's bottom proves the producer's
  disclosure, not the execution a seat replayed. Hence `CourtHeldVerdict`, which kind 3 cannot read
  against signers (the split-δ probe, 2026-09-24).
- **A court default** is charged as a verdict but convicts no signer (IA-9).
- **Open gap at launch:** an arithmetic lie in an `AttnFused` step leaf of a held-context class has no
  conviction route (ADR-0152 post-edit 8).

## 5. Measurements

The measured failure of the central claim (ADR-0049 §Context) and the close-size budget (80 KiB, the
2026-08-26 amendment) are in the archived texts below. So are the fused-row responder drills
(ADR-0093 §9).

## Source texts (archived ADR bodies)

- [ADR-0093 — The court can try a fused row; the responder is what is missing](archive/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
- [ADR-0049: The adjudication contract — what a court opens, and the bound that makes it model-size-independent](archive/0049-palw-adjudication-contract.md)
- [ADR-0086 — the opening carries the fold, not the leaves](archive/0086-the-opening-carries-the-fold-not-the-leaves.md)
- [ADR-0027: PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal](archive/0027-palw-slash-unilateral-fraud-proofs.md)
- [ADR-0092 — The ladder is minted once, and the wall clock is what binds](archive/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
- [ADR-0085: The close is assembled from what the executor served — a disputed tile, not a capture](archive/0085-the-close-is-assembled-from-what-was-served.md)
