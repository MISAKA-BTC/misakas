# ADR-0027: PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0027-palw-slash-unilateral-fraud-proofs.md](../design/palw/archive/0027-palw-slash-unilateral-fraud-proofs.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Accepted (architecture). **Promoted by ADR-0038** from credit machinery to L1 machinery.
* Date: 2026-08

## Context

Slashing a PALW producer for fraud must not depend on a BFT vote, on challenge randomness, or on an
honest majority of verifiers. Two premises are given: an honest re-execution exists somewhere, and
the canonical arithmetic is reproducible by every node.

## Decision

- **D1 — Direct refutation is the primary path.** Bisection is the fallback when the data is
  unavailable. → spec 09 PALW-CT-12.
- **D2 — Adjudication is one step of canonical reference arithmetic, reproduced by every node.** → spec
  09 PALW-CT-11.
- **D3 — `P_detect` is replaced** by one funded honest re-execution, and independence from f.
- **D4 — Amendments to the v0.1 specification.**
- **D5 — Freeze is permissive; slash is strict.**
- **D6 — Settlement is slash-terminal.** → spec 09 PALW-CT-16.

## Consequences

- The assumptions that remain are stated in the full text so they can be attacked.
- The court of every later ADR (0049, 0082, 0093) implements D1 and D2.

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0027-palw-slash-unilateral-fraud-proofs.md](../design/palw/archive/0027-palw-slash-unilateral-fraud-proofs.md)
