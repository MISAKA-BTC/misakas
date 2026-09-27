# Claim lifecycle and settlement — design

> **Not normative.** This document explains why the rules in
> [spec/palw/07-claim-lifecycle.md](../../spec/palw/07-claim-lifecycle.md) are what they are.

**Decisions recorded in:** ADR-0037 (the job state machine), ADR-0042 (the V2 ruleset), ADR-0124,
ADR-0127, ADR-0129, [ADR-0152](../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) (SR,
DL, SW-8), [ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md), [ADR-0155](../../adr/0155-testnet-12-flag-day-daa-1300.md).
**Last revised:** 2026-09-27

## 1. Problem

A claim's reward has to be paid for work that was verified. The chain must never have to reverse a
payment it has already made spendable. And the product must not wait for any of this (execute now,
settle later).

## 2. The design in one paragraph

A claim moves through one lattice:

```
Provisional → PanelBound → ReceiptLicensed → Final
                  (or Voided at any step before Final)
```

It binds only in its anchor block. It is licensed by receipts, then waits out a short challenge
window. At Final its reward is written into a vesting row rather than paid, so a later conviction
can still burn it (ADR-0042 D10 → ADR-0152 V).

The design is asymmetric about failure:

- capacity failures (`BindTimeout`, `NoCapablePanel`) cost nothing;
- a first failed panel redraws for free;
- a second failed panel forfeits the commitment;
- attributable fraud is slashed by action.

## 3. What the live chain taught

- **Heartbeats could not anchor** (IA-1a). A heartbeat-only stretch voids every claim whose slot falls
  in it, without forfeit. That is why only attempt blocks anchor.
- **The seats saturated at DAA 600–749.** SW-10 refused draws, and about 170 claims voided with their
  escrow burned. That led to the lock-budget and lock-life fences at 750 (collateral.md §6) and the
  refusal retry at 1,300 (ADR-0155).
- **Operator anchors (DAA 750).** A panel seed from the anchor's identity could be re-rolled for free,
  so the fix moved the seed to the execution commitment. Until the draw is trusted without it, only
  operator attempts anchor ([t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md)).

## Source texts (archived ADR bodies)

- [ADR-0042: The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint](archive/0042-palw-mainnet-candidate-ruleset.md)
- [ADR-0037: PALW off the block-critical path — an asynchronous, budgeted job state machine over a permanent hash floor](archive/0037-palw-async-job-state-machine.md)
