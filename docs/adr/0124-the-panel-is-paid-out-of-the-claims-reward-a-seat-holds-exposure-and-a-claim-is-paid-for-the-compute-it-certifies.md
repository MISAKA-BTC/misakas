# ADR-0124 — The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid for the compute it certifies

> **Body moved (2026-09-27).** Normative rules → [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../design/palw/archive/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md); the reasoning is summarised in [design/palw/verification.md](../design/palw/verification.md).

* Status: Proposed and **implemented 2026-09-17**, behind `palw_panel_economy` and
  `palw_work_priced_reward`. Armed on testnet-11 at 6,001, and on testnet-12 from genesis. **Amended
  on testnet-12 by ADR-0152:** D3's `3 × claim.reserved` exposure is retired (A-4: `duty_bind`), and
  D5's one-ticket draw is replaced by the stake-weighted draw (SW).
* Date: 2026-09-17

## Context

The operator asked for two things: the panel had no reward, and one claim per block favoured small
models over large ones.

## Decision

- **D1 — A `Final` claim's reward is split 80 / 20** between the producer and the panel pool
  (`PALW_PANEL_POOL_PERMILLE_V1` = 200). → spec 08 PALW-VF-37.
- **D2 — A seat is paid one fixed share of the pool** for a `Valid` receipt the chain credited in
  time. What the pool does not pay goes to the reserve, never to the producer. → spec 08 PALW-VF-38.
- **D3 — A drawn seat reserves three times the claim's exposure, and that is what it loses.**
  *Retired on testnet-12.*
- **D4 — A bond is drawn only while it holds ten producer floors** and its free collateral covers the
  reservation. → spec 08 PALW-VF-26.
- **D5 — Every eligible bond draws one ticket.** *Replaced on testnet-12 by SW.*
- **D6 — A model-class claim is paid the fraction of its escrow** that its class's canonical inference
  is of the heaviest class's. The rest is never minted. → spec 08 PALW-VF-40.
- **D7 — A mainnet card states both fences from genesis.**

## Consequences

- Seats earn from the claims they judge. Pricing by work lets large models earn in proportion.

## Links

- Spec: [08 Verification](../spec/palw/08-verification.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/verification.md](../design/palw/verification.md)
- Full text as written: [design/palw/archive/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../design/palw/archive/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
