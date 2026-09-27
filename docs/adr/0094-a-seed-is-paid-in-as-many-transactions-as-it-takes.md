# ADR-0094 — A seed is paid in as many transactions as it takes

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md](../design/palw/archive/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed and **implemented 2026-09-07**. It amends ADR-0090 D2's last clause. Amendment
  (2026-09-07) in the full text.
* Date: 2026-09-07

## Context

A hundred thousand MSK does not fit in one post-quantum transaction: fifteen ML-DSA-87 inputs is the
most the 480,000 mass cap allows.

## Decision

- **D1 — The seed accumulates,** and each payment locks on arrival.
- **D2 — The market opens on the payment that crosses the floor,** with the whole supply.
- **D3 — Anyone may pay,** and the row names who started it.
- **D4 — What a participant reads** (`seed_pledged_sompi`).
- **D5 — The CLI stops asking for one UTXO.**
- **D6 — Same fences.**

→ spec 15 PALW-MK-6.

## Consequences

- A seed of any size can be paid within the mass cap.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md](../design/palw/archive/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
