# ADR-0148 — The free-prompt lane prices compute, and prices it the same for every class

> **Body moved (2026-09-27).** Normative rules → [spec/palw/05](../spec/palw/05-canonical-work.md), [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0148-the-free-prompt-lane-prices-compute.md](../design/palw/archive/0148-the-free-prompt-lane-prices-compute.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: **Implemented behind the ADR-0145 economic bundle.** `palw_canonical_work` sets the unit and
  `palw_fp_derived_work` the derivation. Addenda of 2026-09-20: the entrance prices with the ledger's
  expression, and its budget is sized in the same unit. Implementation record of 2026-09-21. Armed on
  testnet-12 from genesis.
* Date: 2026-09-20

## Context

A free-prompt claim was priced by its leaf count and by per-class receipt targets, none of which was
the compute it cost. Pay scaled about 62× with prompt length while a producer holding the prefix's KV
cache spent about 3 % more (ADR-0144 §5).

## Decision

1. **The credit is compute:** canonical work over new positions.
2. **One network quantum**, from the floor's derived draw.
3. **Quanta scale the odds, not the credit.**
4. **One receipt target for the lane:** the pooled target.
5. **The price ceiling:** a quantum never beats a forward of the same compute at `W`.
6. **A spend weighs one network quantum.**
7. **The reservation is the claimed compute.**

→ spec 05 PALW-WK-8 and PALW-WK-14, 11 §11.6. Paid prompt rows are kept past the claim for a bounded
time (§3 of the full text).

## Consequences

- The two lanes price one unit of compute identically. No class has a target, share or census of its
  own in the compute era.

## Links

- Spec: [05 Canonical work](../spec/palw/05-canonical-work.md) · [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0148-the-free-prompt-lane-prices-compute.md](../design/palw/archive/0148-the-free-prompt-lane-prices-compute.md)
