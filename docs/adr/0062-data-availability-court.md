# ADR-0062 — The data-availability court: stop a vote from taking a bond

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0062-data-availability-court.md](../design/palw/archive/0062-data-availability-court.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: **Implemented behind `Params::palw_da_court`**. The amended form landed on 2026-09-02, and
  SA-7 widened the same fence on 2026-09-03. Armed on testnet-12 from genesis. **Amended on testnet-12
  by ADR-0152 DA-1…DA-9** past `palw_rcore_plus`. Below that fence, its rules and SA-1…SA-7 stand.
* Date: 2026-08-30 (authored as 0059, renumbered)

## Context

A panel seat that was not served the material it needed could only file `Unavailable`. A quorum of
`Unavailable` receipts could take a producer's bond on a vote. Withholding needed a court that
decides availability by disclosure, not by vote.

## Decision

- **D1 — An accusation must name what is missing.**
- **D2 — Accepting the accusation opens a session.** It does not void the claim.
- **D3 — The producer answers by publishing the missing event** (the disclosure).
- **D4 — A verified disclosure refutes the accusation.**
- **D5 — Silence past the window confirms the default.**
- Security amendments SA-1…SA-7 (2026-09-02/03) harden it before arming. They are in the full text.

## Consequences

- `Unavailable` stops being a verdict: ADR-0065 D4 makes it an abstention.
- On testnet-12 the court is ADR-0152's redesign: sessions per (claim, accuser), drawn units, pause
  credit and re-keyed rows (spec 08 §8.6).

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0062-data-availability-court.md](../design/palw/archive/0062-data-availability-court.md)
