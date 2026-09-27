# ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/08](../spec/palw/08-verification.md); the full text as written → [design/palw/archive/0071-the-attempt-lanes-price-and-the-tickets-bound.md](../design/palw/archive/0071-the-attempt-lanes-price-and-the-tickets-bound.md); the reasoning is summarised in [design/palw/lottery.md](../design/palw/lottery.md).

* Status: **Implemented, with Decision 1 withdrawn** (2026-09-02; proposed 2026-09-01). D1's target
  freeze shipped to the public testnet, was measured to remove the only control on block interval
  (41–54 blocks per minute against 0.5), and was reverted. D1a (the ceiling), D2's nonce bucket (as
  the anchor's position field) and D3 govern. D2's pwu is superseded by ADR-0072 D5.
* Date: 2026-09-01

## Context

The attempt lane's price, the ticket's bound to its execution, and who may judge a class had to be
decided together, because each changes what the others protect.

## Decision

- **D1 — Freeze the attempt lane's price off `header.bits`.** *Withdrawn:* `bits` keeps the block
  interval.
- **D1a — The expectation stays relative, and the repair is a ceiling:** an idle class converges toward
  the producing classes' price, never past it (`converge_idle_target_v1`). → spec 06 PALW-EL-10.
- **D2 — The ticket is bound to its execution by the nonce bucket** (`palw_nonce_bucket_v1`, 2^22).
- **D3 — A panel seat must be able to run the class it judges** (`capable_classes`, carried by
  `BondRegistered`). → spec 08 PALW-VF-26.

## Consequences

- The security amendment of 2026-09-02 is in the full text.
- A false capability declaration is not yet priced (§5, open).

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [08 Verification](../spec/palw/08-verification.md)
- Design: [design/palw/lottery.md](../design/palw/lottery.md)
- Full text as written: [design/palw/archive/0071-the-attempt-lanes-price-and-the-tickets-bound.md](../design/palw/archive/0071-the-attempt-lanes-price-and-the-tickets-bound.md)
