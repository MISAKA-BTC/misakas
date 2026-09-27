# Verification: panels, receipts, replay and DA — design

> **Not normative.** This document explains why the rules in
> [spec/palw/08-verification.md](../../spec/palw/08-verification.md) are what they are.

**Decisions recorded in:** ADR-0026, 0028, 0034, 0098, 0099, 0108, 0111, 0124, 0130, 0133, 0147,
[ADR-0152](../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) (SW, Q, DA) and
[ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md) (the panel seed).
**Full texts:** [archive/](archive/).
**Last revised:** 2026-09-27

## 1. Problem

"The inference ran as committed" has to be checked by someone other than the producer, without a
vote. It must also hold against an attacker who can register many cheap identities, and against one
who withholds the data a check needs.

## 2. The design in one paragraph

A panel of bonded seats is drawn per claim. Since R-core+ the draw is stake-weighted: an exponential
race over keys `L/W`, one seat per operator. It binds only in the claim's anchor block, on one state,
from a seed the producer cannot grind: the anchor attempt's execution commitment. A seat attests by
re-execution, and its receipt is evidence, not a vote. Counting is by independent attesters per
segment (`basis_k`), never by door name. `Sampled` receipts are audit, not verification. A missing
unit of data is contested in a DA session that the producer, or any locked signer, must answer.

## 3. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| One ticket per operator (ADR-0124 D5 / 0130) | A fresh key is free, so m Sybils at the seat floor are m full entries; the collusion residual was 20 operators (2.60M MSK) | archive 0152/06 SW, "What it replaces" |
| A walk over cumulative weight intervals | Adding or removing another operator changes everyone's result; exponential keys do not | archive 0152/06 SW-3 |
| A weighted admission jury | It refuses honest classes that genesis seats do not load (0.1325 vs 0.8769 per audit at 20 × 130k) | archive 0152/06 SW-5 |
| A weight-ordered full seat | Sends every full replay to the heaviest operators, and one heavy Sybil takes the role | archive 0152/06 SW-6 |
| A seed from the anchor block's identity | Re-signing gives free re-rolls: a coverage lie reached Final with 0 slash after about 1,000 re-signs and 2 Sybil seats | [t12-panel-seed-2026-09-25.md](../../t12-panel-seed-2026-09-25.md) |
| Counting `Sampled` as served | Sample sites are public at bind, so a producer can serve exactly those and withhold the rest (C1) | archive 0152/03 SR-1 |
| Pricing coverage at 3 by door name | A lie in segment i needs only the full seat and segment i's holder | archive 0152/06 Q-3 |
| The DA draft's `max(⌈reserved/5⌉, …)` exposure | On 2M the refuted cost was 1.9 × the reward | archive 0152/05 DA-6 |

## 4. Threat model

- **Collusion thresholds** under the stake draw depend on the honest *weight vector*. At launch
  (eight genesis seats) an undetected colluding pair turns EV-positive at 17.29M MSK of Sybil stake
  (12.74M in the worst saturation state SW-10 admits). Tables: archive 0152/07 §4.3.
- **Saturation.** Working seats fill their ceilings while idle Sybils stay eligible. The 875 ‰ floor
  turns that cliff into a halt rather than a capture (archive 0152/06 SW-9, SW-10).
- **After launch,** the halt itself burned escrow (DAA 600–749). Hence the DAA-1,300 retry (ADR-0155).
- **Residuals:**
  - an anchor producer's re-roll, at one inference per try once the seed became the execution
    commitment;
  - the operator-anchor stopgap, which trusts operators to seed panels;
  - a partial seat is not bound by DA defaults at launch (IA-11).

## 5. Measurements

The figures behind the stake draw (`v3calc/v31_stake_draw.py` and `v31_review_numbers.py`, on branch
`docs/adr-0152-v31-postedits`) are quoted in archive 0152/06. The earlier measurements are kept in the
archived texts below:

- the coverage of a one-token lie: 6.51 % with five seats and 3.96 % with three, on testnet-11's
  300-token claim (0098 §1);
- the capacity grid at 2.2 claims a span and seven seats (0133 §6);
- the readiness proof's weakness before V2: one eight-leaf window answered every challenge (0133
  §11.2).

## 6. Open questions

- Anchoring on a committed seed after K heartbeats, so claims can bind through a heartbeat-only
  stretch (archive 0152/10 §9.2).
- Delivery-digest receipts.
- A coverage basis of 3.
- The weighted jury.

## Source texts (archived ADR bodies)

- [ADR-0062 — The data-availability court: stop a vote from taking a bond](archive/0062-data-availability-court.md)
- [ADR-0133 — Verification is its own clock: a class verifies over spans, and a starved class stops only itself](archive/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)
- [ADR-0034: PALW re-verification routing — four execution-class families, five model bands, one deciding binding](archive/0034-palw-execution-class-model-band-routing.md)
- [ADR-0028: PALW challenge sampling — a scheduler for re-execution, never a verdict](archive/0028-palw-challenge-sampling-protocol.md)
- [ADR-0124 — The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid for the compute it certifies](archive/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
- [ADR-0026: PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof](archive/0026-palw-v2-runtime-separated-verification.md)
- [ADR-0111 — A seat may demand the committed leaf it needs to judge](archive/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
- [ADR-0098 — The panel's coverage is a number, and a seat that found a lie files nothing else](archive/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
