# ADR-0028: PALW challenge sampling — a scheduler for re-execution, never a verdict

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0028-palw-challenge-sampling-protocol.md](../design/palw/archive/0028-palw-challenge-sampling-protocol.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Accepted (architecture). **Promoted by ADR-0038** to L1 machinery. **Status amendment
  2026-09-18 (ADR-0133):** its windows were stated against the 120 s cadence of the day. On V2,
  verification runs on its own clock (the receipt window of 600 DAA, the challenge window, the court).
* Date: 2026-08

## Context

Once a refutation, not a vote, decides fraud (ADR-0027), sampling can no longer convict. It needed a
role: who re-executes what, when, and who pays for it.

## Decision

- **D1 — Every credited job is fully re-executed.** An attestation allocates credit, and a refutation
  decides fraud. → spec 08 PALW-VF-36.
- **D2 — Assignment:** the `select_verifiers` ticket, adopted as the duty lottery. A reorg that
  replaces the anchor re-derives it.
- **D3 — Windows are DAA-denominated and stall-tolerant, and pruning caps the credited job.** A duty
  presumes the assignee can obtain its inputs.
- **D4 — `q`, funding, and the inequality that must hold before any reward exists.**
  - The replay fee is an issuance split, not a fee market.
  - An attestation assumes liability.
  - No-show is priced against griefing.
  - The challenger economy is rivalrous.
  - Admission is doubly capped.
  - *§4e's remedies were superseded by ADR-0045 D2 and ADR-0042 D6.*
- **D5 — The audit layer:** opening calls act as the DA heartbeat, answerable but not a proof of
  correctness.

## Consequences

- A sampling result never slashes. The V2 panel, receipts and DA court (spec 08) implement D1 and D5.

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0028-palw-challenge-sampling-protocol.md](../design/palw/archive/0028-palw-challenge-sampling-protocol.md)
