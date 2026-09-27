# Canonical work, pricing and the work target — design

> **Not normative.** This document explains why the rules in
> [spec/palw/05-canonical-work.md](../../spec/palw/05-canonical-work.md) (and the payout of
> [10](../../spec/palw/10-collateral-and-economics.md) §10.8) are what they are.

**Decisions recorded in:** ADR-0045, 0074, 0131, 0132, 0137, 0145, 0146, 0148, 0149, and ADR-0144 P4/P5.
**Last revised:** 2026-09-27

## 1. Problem

The 2026-09-19 reward audit found that a registrant set the unit price of its own work. Under that
regime:

- `tile_len` could move pay per MAC by 427×;
- a legal profile could reach 1.00 MAC-eq per leaf, against the live classes' 3,957–22,077;
- a free prompt was paid about 62× more for a long prompt whose prefix the producer already held in
  cache.

One scalar cannot price dense GEMM, routed experts, attention, KV traffic and quantised kernels
together, because hardware does not (ADR-0144 §5).

## 2. The design in one paragraph

- Work is a vector derived by the chain from the registered graph and the execution facts.
- A coefficient may convert a dimension only if the arbitrage it permits is bounded. The search found
  none justified, so there is no table.
- An attempt's pwu *is* the derivation.
- A free prompt is credited its compute over new positions, and its quanta scale odds, not credit.
- One network work target `W` prices a unit of work from any model. A model's share of blocks is
  whatever its verified production makes it, never an input.

## 3. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| STEP leaves as the unit | Registrant-chosen `tile_len`, 427× spread | ADR-0144 §5 |
| MAC-equivalents as one scalar | No memory-traffic term; not shown to track real cost | ADR-0144 §5, archive 0146 |
| A calibrated coefficient table | Unjustifiable by being "right"; the bound measured 1.0× | archive 0146 §3, §9 |
| Shares as lottery inputs | Cold start, sawtooth, inverse-cost share, cap trap, cadence collapse | archive 0137 §3 |
| A scalar CCU armed after shadow | Withheld by 0144 and 0146 | archive 0131 |

## 4. Residual

Synthetic Attempt work still carries most of the economy. ADR-0144 §6 item 5 (shrink Attempt) is the
open direction. Record the live mix here as it is measured.

## Source texts (archived ADR bodies)

- [ADR-0137 — A block buys one unit of work from any model, and a share is a result, not an input](archive/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)
- [ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves](archive/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md)
- [ADR-0146 — A coefficient is justified by the arbitrage it permits, not by being right](archive/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)
- [ADR-0148 — The free-prompt lane prices compute, and prices it the same for every class](archive/0148-the-free-prompt-lane-prices-compute.md)
- [ADR-0045: The class economy is chain state — derived PWU, block-denominated epoch budgets, and the registration-granted share table](archive/0045-palw-class-economy-on-chain.md)
- [ADR-0145 — Canonical work is derived, not declared; admission is earned, not registered](archive/0145-canonical-work-is-derived-and-admission-is-earned.md)
- [ADR-0132 — What a model is actually paid per forward it ran, and why the gap is liveness before it is price](archive/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md)
