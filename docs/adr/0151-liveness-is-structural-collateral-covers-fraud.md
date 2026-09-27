# ADR-0151 — Liveness is structural; collateral covers fraud

> **Body moved (2026-09-27).** Normative rules → [spec/palw/10](../spec/palw/10-collateral-and-economics.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0151-liveness-is-structural-collateral-covers-fraud.md](../design/palw/archive/0151-liveness-is-structural-collateral-covers-fraud.md); the reasoning is summarised in [design/palw/collateral.md](../design/palw/collateral.md).

* Status: **D2, D3, D4 and D5 in force on testnet-12 from genesis. D1's genesis half landed and its
  runtime half is open. D6 is partly done.** D4 and D5 were drafted as open and turned out to be
  already built; the full text records that correction.
* Date: 2026-09-22
* Retires, on testnet-12: the `window_bind × dearest claim` genesis requirement
  (`BondCannotSustainBindWindow`) wherever the structural guarantee holds.

## Context

Genesis collateral had been sized so that every bond could hold `window_bind` of the dearest claim at
once. That bought liveness with capital, and it wedged the first testnet-12 at block 601. testnet-12
is the last testnet before mainnet. The operator decided (2026-09-22) to start it on the separated
design: liveness from structure, and collateral only for fraud.

## Decision

- **D1 — Collateral is reachable fraud liability:** concurrency per class, and per-claim liability
  `palw_max_fraud_gain_v1` (escrow + fork weight). The genesis half has landed; the runtime half is open.
- **D2 — The genesis-only collateral rule is conditional, then gone.**
- **D3 — The clock does not depend on any bond's collateral.** The heartbeat lane is armed from
  genesis, and no `bits`-priced lane is producible.
- **D4 — Admission capacity and slash liability are separate ledgers.**
- **D5 — One derived profile per class; duration is not weight.**
- **D6 — The gate asks for progress, not for capital.** The reachable-state search is still owed as a
  drill.

→ spec 10 §10.9, 13 §13.3.

## Consequences

- At one tick per 120 s, the 600-DAA bind window is about 20 hours and the exposure span about 10
  days: bounded, and independent of collateral.
- The addenda of 2026-09-23 (the first testnet-12 fleet's two defects and the memory paths the
  acceptance ladder found) are in the full text.

## Links

- Spec: [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/collateral.md](../design/palw/collateral.md)
- Full text as written: [design/palw/archive/0151-liveness-is-structural-collateral-covers-fraud.md](../design/palw/archive/0151-liveness-is-structural-collateral-covers-fraud.md)
