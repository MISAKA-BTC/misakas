# ADR-0026: PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/04](../spec/palw/04-execution-semantics.md); the full text as written → [design/palw/archive/0026-palw-v2-runtime-separated-verification.md](../design/palw/archive/0026-palw-v2-runtime-separated-verification.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Accepted (architecture). Restored in full on 2026-08-26 by ADR-0053, after ADR-0051 walked it
  back for one family. **Promoted by ADR-0038** from credit machinery to L1 machinery.
* Date: 2026-08 (v2 architecture)

## Context

The first PALW (ADR-0021) committed to output text, and that commitment was shown to be forgeable
without running the model. Ambient's design separates the scheme kernel from the runtime. The
question was what to borrow from it, and how much deeper than logits a proof must reach to be worth
slashing on.

## Decision

- **D1 — Borrow Ambient's architecture, and keep the scheme kernel out of the runtime.**
- **D2 — Prove deeper than logits:** logits, activations and the GEMM trace are committed. → spec 04 §4.4.
- **D3 — Exactness inside a pinned class; never a tolerance in a slashing verdict.** A backend is a
  determinism class. Within a class, equality is full 64-byte equality, and across classes nothing is
  compared. → spec 04 §4.1.
- **D4 — Commit, then challenge after the commitment, then recompute, then quorum**, with the challenge
  unpredictable. → spec 08, 09.
- **D5 — The challenge count `q` is derived from `P_detect · S > G`**, never fixed at 1.
- **D6 — Verification is asynchronous.** *Changed by ADR-0038:* PALW is the consensus work.
- **D7 — An open kernel; self-originated jobs, with no auction; the tokenizer outside the runtime.**
- **D8 — Distribution** is a signed artifact bundle per class, from public source.

## Consequences

- Every later court and panel design (ADR-0027, 0028, 0049, 0053) builds on D3's exactness rule.
- ADR-0051's tolerant family was withdrawn by ADR-0053, which restored this ADR in full.

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md) · [04 Execution semantics](../spec/palw/04-execution-semantics.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0026-palw-v2-runtime-separated-verification.md](../design/palw/archive/0026-palw-v2-runtime-separated-verification.md)
