# Collateral, locks and vesting — design

> **Not normative.** This document explains why the rules in
> [spec/palw/10-collateral-and-economics.md](../../spec/palw/10-collateral-and-economics.md) (and the
> charging rules of [07](../../spec/palw/07-claim-lifecycle.md) §7.7) are what they are. The rules are
> only in the Spec.

**Decisions recorded in:** [ADR-0152](../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md)
(R-core+), [ADR-0151](../../adr/0151-liveness-is-structural-collateral-covers-fraud.md) (liveness is
structural), [ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md) and
[ADR-0155](../../adr/0155-testnet-12-flag-day-daa-1300.md) (the post-launch lock changes). Earlier:
ADR-0061, 0064, 0065, 0124.
**Full texts:** [archive/0152/](archive/0152/README.md). The earlier ADRs are archived as each is slimmed.
**Last revised:** 2026-09-27

## 1. Problem

Before R-core+, a claim reserved a reward-sized exposure on its producer's bond for its whole life,
and each seat carried a flat exposure per duty. Three things followed:

- The reservation protected the interval that needed it least. The escrowed reward is safest before
  licence, when nothing has been paid out.
- A seat's capital sat idle in locks.
- Throughput was capped per bond by collateral rather than by what the network can verify.

Removing the reserve outright fails in a different way: without attribution nothing ties a lie to the
bond that told it, so there is nothing to recover. (Archive 02, §1.1–§1.4.)

## 2. The design in one paragraph

The bond is one account of standing, slashable stake (spec PALW-CO B-rules). Cumulative work is
unlimited, and only concurrent, unresolved risk is reserved. A claim commits `w + E` (weight plus
escrow) until its licence, and only `w` after a licence that every seat served with at least two
independent attesters per segment. A seat's lock is priced by the recount of those attesters, and the
producer's escrow is paid into a **vesting row** that matures on the lock's clocks. So a conviction
inside the window can still take back what the fraud minted. One committed ledger keeps
`committed + accuser ≤ C` at every gate. Slashing is by action, capped at `3 × G`. Throughput is
limited by protocol caps (cadence, panel room, the unminted-reward ceiling), not by bond caps. The
design bar is the operator's principle 6 (archive 02 §1.5): **the most an absconder can definitely
extract is at most what the chain can definitely recover**, given a conviction in its window.

## 3. Principles (operator, binding; archive 02 §1.5)

1. Keep cumulative separate from concurrent.
2. The bond is standing stake.
3. Slashing is by action.
4. Use throughput caps instead of bond caps.
5. Max definitely-extractable profit ≤ definitely-recoverable value.
6. The bar the team uses for any claim-collateral change: record the uncovered maximum loss at 1,000
   concurrent claims.

## 4. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| Pure no-reserve | Without attribution, nothing is recoverable | archive 02 §1.4 |
| Option A: hold `w + E` to Final always | 9–17 % less producer capacity than v3, and newcomers are hurt most | archive 04 §3.7 T-2(d) |
| A flat `/3` lock divisor | Under-prices a coverage licence, which has two attesters per segment | archive 03 L-2 |
| Charging a first failed panel (v3 draft) | One silent or `Sampled` partial seat sinks an honest claim; the expected forfeit is 1,422–2,522 MSK per floor claim, free to the griefer | archive 03 SR-5 |
| Weight = free stake | It pays idleness, since idle Sybils keep full weight, and buys no extra recoverability | archive 06 SW-2 |
| Ejection status and tombstones (S5) | Identity is not checkable, and they wait on runtime determinism (Q1) | archive 03 B-4, 04 §3.6 |
| An uncapped Eq slash | Until runtime nondeterminism is settled, an honest genesis bond could be destroyed; capped at `min(C, 3·G_eq)` on testnet-12 | archive 03 B-4 |
| `lock_2` reserved at bind on 2M | Amplification of 2.65, above the ≤ 1 bound (F14) | archive 03 L-4b |

## 5. Threat model

The invariant and its preconditions are in archive 07 §4.1:

- **(i) filed**: filing rests on automatic filers, not on the reporter reward.
- **(ii) provable**: by an admitted contradiction or a DA default.
- **(iii) detection latency fits the court window**: not on 2M.
- **(iv)** heartbeat carriers.
- **(v)** runtime determinism.
- **(vi)** weight valued at `w`: false for 2M.
- **(vii)** a second-clock depth of 30.
- **(viii)** the draw is not captured.

Per-strategy analysis and the success probability per licence door are in archive 07 §4.2–§4.3. The
chain-kept table (R-6) shows the 2M row failing when only one or two of its load-bearing locks are
convicted. That is why 2M is closed at launch and waits for ADR-0153.

Named residuals:

- The FP lane's execution quanta mature at F + 1,200, before the row's F + 3,000 (V-2b).
- A silent seat can hold escrow for free (SR-1b, T-2(d)).
- The trickle and halt regimes of V-8.
- The 2M lock top-up can be starved (L-4b).

## 6. What changed after launch, and why

- **The lock budget (DAA 750, V02 option a).** An honest `Valid` seat's post-Final lock (about
  160–240 MSK per floor Final) sat in the 500 ‰ work budget for `window_court` (3,000 DAA). At 2–4
  floor claims per DAA, every seat's bind room closed within about 600–1,700 DAA of the first Final,
  and claims voided at the anchor slot with their escrow burned. Resolved locks now count against the
  whole collateral, with a four-floor accuser reserve (52,000 MSK on testnet-12).
  (`params.rs`, `palw_final_lock_full_collateral`.)
- **The lock life (DAA 750).** A resolved `Valid` lock lives F + 1,000, which roughly triples seat
  capital throughput. The liability record, the vesting row's clawback of `E` and the court and DA
  windows keep `window_court`, so conviction still fires through F + 3,000. Past the fence a lock
  stops being slashable when it stops being committed, which closes a per-collateral double commit.
  (`palw_final_lock_life`.)
- **The retroactive re-date (DAA 1,300).** At DAA 816 each genesis seat still held 959–1,583 locks
  dated F + 3,000 from Finals before 750. The room would have closed again around DAA 1,350–1,575, and
  SW-10 would have refused every draw, as it did between DAA 600 and 749, when 169–172 claims voided
  and about 541,000–551,000 MSK of escrow was burned. (`palw_final_lock_life_retro`; the launch note
  §000.)

## 7. Measurements

Genesis values from the v3 script: E = 3,200.85 MSK on every row, `w`, `R`, `G`, the locks and the
duty per class. They are in archive 02 §2, with the honest waits (nominal 3,147 DAA to mint, 7.3 days
at 200 s/DAA) in archive 03 V-4. The live figures after launch are in the launch note
([t12-launch-2026-09-25.md](../../t12-launch-2026-09-25.md)).

## 8. Open questions

- How a producer below the floor tops up: re-registration today, or an in-place top-up object
  (archive 10 §9.3 Q11).
- The honest weight vector of the stake draw: register operator seats from the main wallet at or below
  the 1,000,000 MSK cap (archive 10 Q10).
- Deferred (archive 10 §9.2):
  - S5 and the Eq value for mainnet
  - ADR-0153 before `c_2M ≥ 2`
  - a coverage basis of 3
  - delivery-digest receipts
  - the F7 reporter slot
  - a stake-weighted admission jury
  - anchoring after K heartbeats
- Claim capacity separated from collateral price: ADR-0160, imported after int-6 (`claim-capacity.md`).
