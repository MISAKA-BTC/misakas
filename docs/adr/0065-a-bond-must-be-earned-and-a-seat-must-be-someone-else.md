# ADR-0065 — A bond must be earned, and a failure is not a verdict

> **Body moved (2026-09-27).** Normative rules → [spec/palw/10](../spec/palw/10-collateral-and-economics.md), [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md](../design/palw/archive/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md); the reasoning is summarised in [design/palw/collateral.md](../design/palw/collateral.md).

* Status: D1, D2 (restated as D2a) and D4 landed, dormant at first. **D1 was made armable on
  2026-08-31**; testnet-12 arms it at DAA 1,000 (`palw_bond_maturity`), and non-genesis bonds from 750
  (`palw_bond_maturity_early`, ADR-0154). D2 as first written was unimplementable and is restated as D2a.
  **D3 was withdrawn**: "a seat is someone else" is not a checkable predicate. D5 and D6 are decided.
  The filename keeps the withdrawn clause.
* Date: 2026-08-30

## Context

The live chain showed that a bond registered moments before a claim's anchor could sit on that
claim's panel. Seats held by the producer could be bought just in time. A quorum of `Unavailable`
receipts, a failure rather than a refusal, could take a bond as if it were a verdict.

## Decision

- **D1 — Seat maturity.** A bond may be drawn for a panel only if it was registered at or before
  `anchor_daa − bond_maturity_daa`, judged at the claim's anchor. → spec 10 PALW-CO-7.
- **D2a — Frontier provenance, as a comparison-site rule at the deep-reorg gate** (D2 as written put a
  fork-point-dependent value into the state root). Dormant on testnet-12 (`palw_frontier_provenance`).
  → spec 13.
- **D3 — "A seat must be someone else."** *Withdrawn:* the dedup exists, and "distinct operator" cannot
  be checked.
- **D4 — `Unavailable` must be evidence of a refusal, not of a failure.** It abstains and convicts
  nobody (`palw_unavailable_abstains`). → spec 08 PALW-VF-17.
- **D5 — The registry is append-only, by decision.**
- **D6 — Re-pricing alone is not a remedy:** raising the floor does not close this class of attack.

## Consequences

- Two operational rules follow, and are kept in the full text.
- ADR-0152 B-4 builds on D5: the only way back above the floor is a new bond.

## Links

- Spec: [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md) · [08 Verification](../spec/palw/08-verification.md) · [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/collateral.md](../design/palw/collateral.md)
- Full text as written: [design/palw/archive/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md](../design/palw/archive/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
