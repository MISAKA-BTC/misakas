# PALW economic parameters — derivation, and the leverage defect it found

**Date:** 2026-08-16 · **Tool:** `scripts/misaka-palw-economics-sim.py` (re-run it; every number
below is its output) · **Normative:** ADR-0028 §4 (issuance split, no-show floor, admission
caps), ADR-0032 (fee-bond escrow), ADR-0027 §4 (bounty cap) · **Gate:** B15

Every number ADR-0028 and ADR-0032 marked "economic-simulation-gated" is derived here from
measured inputs, each against the inequality it must satisfy. **Two constraints fail at the
live parameters**; both are reported rather than tuned away.

## 1. Inputs (measured, cited)

| input | value | source |
| --- | --- | --- |
| `base(C)` per credited block, 120 s net | **4 445.62 MSK** | genesis rate `370 468 345` sompi at 10 BPS, rate-preserved (`params.rs`) — matches the documented figure exactly |
| bond | 20 000 MSK | live on t10 (`t10-bond-registered-2026-08-15`) |
| `p99` cold replay, slowest host, D=512 | 90.7 s | fleet bench 2026-08-16 |
| `w_replay` / `W_challenge` / unbonding | 30 / 720 / 10 083 blocks | ADR-0028 §3 two-minute set; testnet `DnsParams` |
| carriage masses | call 3.3 k, answer 152 k, refutation 15 k | ADR-0029 §3 |
| `q`, `ρ_v`, `λ` | 2, 1.0, 2.0 | ADR-0028 §4 |

## 2. Derived values (Stage-1 registration candidates)

| parameter | derived value | binding constraint |
| --- | --- | --- |
| attester fee | 4 445.62 MSK (× q) — issuance 3.0 × base | `(1 + q·ρ_v)·base(C)` |
| **`F_call`** | **4.04 MSK** | ≥ answerer's total cost (152 k mass fee 1.52 MSK + one replay ≈ 2.52 MSK-equivalent). The caller's own mass fee at the relay floor is 0.033 MSK — **80× too small on its own**, which is exactly why a *minimum fee rule* is needed and mass alone is not a DoS price. |
| **`B_cap`** | **2 000 MSK** (10 % of a full bond) | ≤ 10 % of slash; self-slash ROI 0.10 < 1 |
| no-show floor | see §3 defect 1 | 2·floor > orphan-equivalent AND ≤ bond |
| physical admission cap | ≤ 78 credited jobs / 30-block window (4-host fleet) | `R_jobs·q ≤ Σ capacity(p99)` |
| **per-validator credit cap** | **2.2 jobs per unbonding period** | see §3 defect 2 |

Verifier's dilemma (§4b) checks out with enormous slack: one replay costs ≈ 2.5 MSK-equivalent
against a 20 000 MSK bond at risk, so replaying stays dominant while `P(fraud)·P_refute >
1.3 × 10⁻⁴` — about 7 900× of headroom.

## 3. Two constraint failures

### Defect 1 — the no-show floor is uncollectible

ADR-0028 §4c's placeholder is `≥ 100 · ρ_v · base` = **444 562 MSK**, against a **20 000 MSK**
bond: 22× more than exists to slash. A floor you cannot collect is theatre — the *actual*
deterrent is capped at the bond.

**Resolution:** state the floor as `min(100 · ρ_v · base, bond)` and note that the griefing
inequality still holds at the cap — 2 × 20 000 MSK of slash against a griefer's gain of one
orphan-equivalent (4 445.62 MSK) is an ROI of 0.11, comfortably negative. The 100× multiplier
is only meaningful once bonds exceed ~222 k MSK; below that, *the bond is the floor*.

### Defect 2 — `max_leverage ≤ 1` fails by four orders of magnitude

ADR-0028 §4e admits two readings of `G_max`, and they differ enormously:

* **Reading A, per-commitment** ("credit mintable from *the* dishonest commitment"):
  `G_max = 4 445.62 MSK`, need `S_eff ≥ 8 891 MSK` — **satisfied**, 2.25× margin.
* **Reading B, aggregate** (what the `max_leverage ≤ 1` sentence actually says: "credit
  mintable **within one unbonding period** must not exceed `S_eff`"): a miner crediting every
  physically-allowed slot mints **116.5 M MSK** against a 20 000 MSK bond —
  **violated by 11 655×** at `P_check = 1.0`, and by 58 000× at `P_check = 0.2`.

Reading B governs: it is the one the leverage sentence states, and it is the one an actual
repeat offender exploits — nothing stops a miner from cheating on job after job against the
same bond. **This is a real defect at live parameters, not a modelling artifact.**

**Resolution — four equivalent moves, and the only credible ones are the first two:**

1. **Cap credited jobs per validator**: ≤ `S_eff / (λ · base)` = **2.2 jobs per unbonding
   period** (1 job per ≈ 4 483 blocks ≈ 6.2 days per validator). A registration-time
   admission rule, the same shape as the physical cap.
2. **Make PALW credit a small fraction of the subsidy**: crediting once every 10 blocks
   requires `base(C) ≤ 9.92 MSK` — i.e. PALW credit is **0.22 %** of a block subsidy, not all
   of it. This is the move that keeps a useful crediting *rate*.

> **CORRECTED 2026-08-17 — both printed remedies were computed in the wrong unit, and
> remedy 1 does not exist.** Everything above prices a job at `base(C)`. A credited job
> actually mints `base(C) + q · ρ_v · base(C)`: the executor's base *plus* one share per paid
> attester. At the live panel (`q = 2`, `ρ_v = 1 000‰`) that is **3 × base(C)**, and the
> encoded check `max_leverage_holds_v1` derived the same base-only figure — so it validated
> the bond against a third of what the crediting walk pays out. Consequences:
>
> * **Remedy 1 is unreachable at this panel, at every rate.** `jobs ≥ 1` for any interval, and
>   one full-subsidy job pays `3 × 4 445.62 = 13 336.86 MSK`; `λ · G_max` exceeds a 20 000 MSK
>   bond before the rate lever is consulted. Widening the interval past the whole unbonding
>   period does not help. The largest base a *single* job per unbonding period admits is
>   **749‰** (3 329.77 MSK base, 9 989.31 MSK paid) — so "full subsidy at a slow rate" was
>   never available once the shares were counted.
> * **Remedy 2 survives, at a different pair.** (10 blocks, 0.2 %) fails. The tightest
>   interval 0.1 % admits is **14 blocks** (13 fails). Measured alternatives:
>
>   | `base(C)` | `ρ_v` | payout / job | max jobs per period | tightest interval |
>   |---|---|---|---|---|
>   | 1‰ (4.45 MSK) | 1 000‰ | 13.34 MSK | 749 | **14** |
>   | 2‰ (8.89 MSK) | 1 000‰ | 26.67 MSK | 374 | 27 |
>   | 1‰ (4.45 MSK) | 200‰ | 6.22 MSK | 1 606 | 7 |
>   | 2‰ (8.89 MSK) | 200‰ | 12.45 MSK | 803 | 13 |
>
> * **The remedy space is four-dimensional**, not the two levers described above:
>   `(min_credit_interval_daa, base_subsidy_permille, q, ρ_v)`. Shrinking `ρ_v` buys rate as
>   directly as shrinking `base(C)` does — at `ρ_v = 200‰` the printed 0.2 % base is admissible
>   at one job per 13 blocks, close to the original intent.
>
> The fleet fixture now registers **(14 blocks, 0.1 %)**. `consensus/core/src/palw_schedule.rs`
> pins each boundary in both directions, and one function
> (`one_job_payout_sompi_v1`) is now the sole source of the payout for both the per-block
> ceiling and this inequality, so the two cannot drift apart again.
3. Raise the bond to 233 M MSK per validator — not credible.
4. Shorten the unbonding period — bounded below by `W_challenge`, so at best 14×; nowhere
   near four orders of magnitude.

Options 1 and 2 are the same inequality solved for different variables, and the choice is a
registration decision, not a runtime one. **Whichever is chosen must be written into
ADR-0028 §4e before Stage 2**, because Stage 2 is where credit becomes real; Stage 0 and
Stage 1 are unaffected (they credit nothing).

## 4. What this changes

* `F_call` and `B_cap` now have derived values with their constraints attached — B15's
  deliverable for ADR-0032's E1 phase.
* Two ADR amendments are required before Stage 2, both recorded above: the no-show floor's
  `min(…, bond)` form, and §4e's `G_max` disambiguation plus whichever leverage remedy is
  chosen.
* Nothing here is activated. The credit gate is unwired (B14) and Stage 2 is blocked on
  independent grounds; this analysis is what a Stage-2 promotion record must answer to.
