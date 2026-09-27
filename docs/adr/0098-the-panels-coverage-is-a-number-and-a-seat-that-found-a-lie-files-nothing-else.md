# ADR-0098 — The panel's coverage is a number, and a seat that found a lie files nothing else

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/09](../spec/palw/09-court-and-offences.md); the full text as written → [design/palw/archive/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md](../design/palw/archive/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Proposed 2026-09-10. **Decisions 1–3 implemented the same day.** D1 is a report. D2 and D3
  change what a seat files: node duty, with no consensus change and no fingerprint. Decisions 4–6 are
  stated, not built.
* Date: 2026-09-10

## Context

On the 300-token claim testnet-11 licensed on 2026-09-05, the panel's sampling caught a one-token lie
6.51 % of the time with five replaying seats, and 3.96 % with three. A seat that had caught a lie
could still go on to certify the claim.

## Decision

- **D1 — A panel's coverage is a number, and it is generated** (`palw_seat_coverage_v1`). → spec 08
  PALW-VF-30.
- **D2 — A seat that found a fault in a claim files nothing else about it.** → spec 08 PALW-VF-34.
- **D3 — Every fault a seat proves is recorded,** at the most specific address known.
- **D4 — The expected cost of a lie is the coverage times the claim's reservation.**
- **D5 — What a network whose seats hold shards must have** (a stratified panel, and more), named and
  not built.
- **D6 — The pending fall-through is the operator's** decision, named.

## Consequences

- Coverage is reported, not assumed. On testnet-12 attribution (ADR-0152 J) and the recount (Q-3)
  price what D4 describes.

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md) · [09 Court and offences](../spec/palw/09-court-and-offences.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md](../design/palw/archive/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
