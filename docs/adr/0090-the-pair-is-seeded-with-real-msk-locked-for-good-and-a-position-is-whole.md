# ADR-0090 — The pair is seeded with real MSK, locked for good, and a position is whole

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../design/palw/archive/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed and **implemented 2026-09-05**. It amends ADR-0087 D1–D4. **The least seed was raised
  to 1,000,000 MSK past `palw_model_seed_v2` (ADR-0120).** Multi-transaction seeds are ADR-0094.
* Date: 2026-09-05

## Context

A market that opened by itself on a virtual reserve could be opened with nothing at stake.

## Decision

- **D1 — A position is a whole number,** and there are 500,000 a line.
- **D2 — A market opens by a seed and by nothing else.** The seed is the reserve, and it is locked for
  good.
- **D3 — A third move,** unsigned like a buy (`ModelSeed`).
- **D4 — The fees are ADR-0087's,** and the seed pays none.
- **D5 — What a participant reads.**
- **D6 — Approval is what it already was.**
- **D7 — Same fences,** no new one.

→ spec 15 PALW-MK-4 to PALW-MK-6.

## Consequences

- The curve's product never falls, so the reserve never falls below the seed.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../design/palw/archive/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
