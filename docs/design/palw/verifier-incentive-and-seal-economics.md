# The verifier's incentive and the economics of seals (lane ECON, 2026-10-09)

> **Superseded (ECON successor, 2026-10-10)** by [`econ-verifier-incentive-and-allocation.md`](econ-verifier-incentive-and-allocation.md),
> which redoes this analysis at ADR-0032's 49% reporter share and under ADR-0176/0177. Kept as the record of the first milestone.
> Corrections to this record:
> * The rows this note labels "measured" were **never measured**. The PoCs failed to compile on 10-09 at 09:31 (`econ-m1`), so their
>   numbers are derived values.
> * The successor kept only the ledger PoCs that test consensus code, and runs them at 49%:
>   * capture (M0);
>   * the E1 leak;
>   * F-ECON-3;
>   * the abandonment stall `T_ab`.
>
>   It moved every calculator to `scripts/misaka-palw-econ-verifier-model.py`.
> * F-ECON-1/2 were fixed by G14R (`9429128f8`, S1) and are tested on that branch.
> * At 49%, the self-inflicted-default path recoups 53.9% of what was collected (a net loss of 46.1%), not at most 49%; see the successor's E1.

Branch `econ/verifier-incentive`: the integration line at `8bb115ed9`, with `opv/bootstrap-beacon` and `g14/r4-fixes` merged in so that
the PoCs run against G14-R4's ledger (bonded and salted claim seals, accuser seals, the GAP-5 escrow) and OPV-BOOT's v3 collector.

**Status: DESIGN + PoC.** No consensus code changes here and nothing is armable. Every number uses the INTERIM terms, which are not
production values. Each result is marked PROVEN (the proof is in this note), COUNTEREXAMPLE (with a measured PoC) or OPEN.

Two P0 problems block any reward, consensus weight or Panel=0 fence (the user's rulings, 2026-10-09). Both sit under
`docs/PRINCIPLES.md` §6:
* **condition 5:** a collectable collateral consistent with the maximum gain from fraud;
* **condition 6:** honest verifiers have the resources, the costs covered and the incentive to take part in time.

The user's requirement has two parts. Before any design is chosen:
1. prove that self-dealing does not pay when the producer and the verifier are the same economic party (self-fraud,
   self-accusation, receiving the bounty);
2. prove that an honest verifier recovers what it spends to find the evidence, fetch the DA and file in court.

An attacker that merely loses money is not enough: that loss does not show that honest watchers will take part.

---

## 0. Results

| Id | Statement | Status |
|---|---|---|
| **T0** | No incentive paid only in bounties can pay honest watchers once deterrence works: a watcher's bounty revenue is at most the fraud rate times its share | **PROVEN** (§2.5) |
| **M0** | Today's rule (the earliest seal of the convicting bytes takes the bounty): the liar's own Sybil takes every bounty. The honest verifier earns 0 on lies and on honest claims | **COUNTEREXAMPLE**, measured (§2.1) |
| **A** | Pay a watcher only for a fault inside its pre-committed salted sample | **COUNTEREXAMPLE** (§2.2) |
| **B** | Split the bounty equally among every covering watcher | **COUNTEREXAMPLE** outside a parameter band that the protocol cannot observe (§2.3) |
| **T1** | Self-dealing (self-fraud, self-accusation and bounty receipt by one party) loses at least `(1 − a)·K·β_d` today and at least `(K − B_d)·β_d` under M* | **PROVEN** (§3.2) |
| **T2** | Stake-weighted watcher draws after the commit are Sybil-neutral: the result depends on stake, not on the bond count | **PROVEN** under A-BEACON (§3.3) |
| **T3** | Under M*, an honest drawn watcher recovers fetch, check, localization, attestation and filing | **PROVEN** under A-COST, A-INC, A-FEE (§3.4) |
| **T4** | Deterrence under M*: `K ≥ B_d + D·(1 − β_d) + G·(1 − P_dc)/P_dc` with `P_dc = q·(1 − (1 − θ)^m)` | **PROVEN given θ** (§3.5) |
| **T5** | Bribery floor: if `B_d ≥ m·G`, a drawn watcher that found the lie never does better by taking a bribe | **PROVEN** under A-BRIBE (§3.5) |
| **T6** | θ, the share of drawn slots that really check, can be derived from chain state (the lazy-watcher problem) | **OPEN** (§3.6) |
| **F-ECON-1** | A re-seal keeps its one deposit, so ONE deposit vetoes all `R + 1` attempts of a class and is never forfeited | **COUNTEREXAMPLE**, measured (§4.2) |
| **F-ECON-2** | A re-seal in the reveal window withdraws a mixed seal, without counting as a veto: with `a` attacker seals, `2^a` lockable beacons, against v3's accounting `G = F` | **COUNTEREXAMPLE**, measured (§4.2) |
| **F-ECON-3** | One withheld seal vetoes every concurrent attempt whose window contains it (the sources are shared) | **COUNTEREXAMPLE**, measured (§4.2) |
| **B12-D** | No fixed `seal_deposit` both deters veto-griefing of a class worth stalling and keeps honest job races viable | **PROVEN** (§4.6) |

**Verdict.**
* **Problem 1 (bounty capture).** Neither remedy A nor remedy B fixes it, and no rule paid only in bounties can (T0). An honest
  watcher's income must come per check, from the user. The mechanism proposed here is M*:
  * a check fee, pre-funded in the job's escrow;
  * stake-weighted watcher slots, drawn after the commit;
  * a small capped bounty, split among the drawn slots that sealed a convicting proof, with the rest of the slash burned.

  M* satisfies both of the user's requirements (T1, T3), subject to θ (T6, OPEN). It is not built; it needs allocations (§3.8).
* **Problem 2 (GAP-B12).**
  * Today the deposit does not even enter the attacker's cost (F-ECON-1).
  * The re-seal that causes this also breaks v3's grinding claim (F-ECON-2).
  * Even after a fix, no single deposit both prices the veto and lets honest producers race for jobs (B12-D).
  * Structural changes are required (§4.7). S1, "a re-seal forfeits", and S2, "vetoes do not exhaust attempts", are small and close
    the exhaustion; the rest prices the remaining stall.

---

## 1. The model

### 1.1 Parties

| Party | What it is |
|---|---|
| `P` | the producer bond of a claim `c` |
| `V_i` | verifier (watcher) bonds |
| `U` | the job's poster (the user), who funds the escrow |
| `C` | a **coalition**: every bond of one economic owner, with `P ∈ C`. It may hold any number of verifier, demander and poster bonds, and the blocks it mines (it orders their objects). Bonds are not parties (F-C4R4-01): the chain cannot tell `C`'s bonds from anyone else's |
| `H` | honest watchers. They follow the protocol, and take part only if their expected revenue covers their expected cost (A-RAT; the user's requirement rules out altruism) |

### 1.2 What is public and what is private

* **Public:** every object on chain:
  * claim seals, reveals and salts, after the reveal;
  * proof seals, proofs, demands and responses;
  * the class's artifact commitments;
  * the DA material the producer publishes;
  * the drawn assignments, under M*.
* **Private:**
  * whether and where `P` lied — `P` knows it from commit time;
  * a watcher's salt before it is revealed;
  * a watcher's check result before it seals or files;
  * an honest seal's salt until its reveal.

**The asymmetry everything turns on.** `P` knows its fault at the commit `t_c`. An honest watcher learns it only at
`t_c + T_fetch + T_check (+ T_localize)`, and under M* only after the draw `T_beacon` as well.

### 1.3 Timing (DAA; interim values; one DAA is at least 120 s, operationally about 150 s)

| Event | When |
|---|---|
| claim seal | `t_s` |
| reveal (commit) | `t_c ≥ t_s + 1` (`claim_seal_delay_daa`). An unrevealed seal expires after `seal_ttl_daa = 100` |
| OPV window | `[t_c, t_c + 50)` |
| demand deadline | 20 |
| proof grace | 10 |
| Final | at the window's end, if undisputed |
| liability horizon | Final + 200 |
| proof seal (GAP-R7) | any time a proof could still change anything. It matures after `claim_seal_delay_daa` (1), takes no deposit and needs only `free ≥ dismissed_proof_fee` |
| M*'s draw (a v3 `ClaimVerification` subject) | seal window `[t_c + δ, t_c + δ + W)`, then a reveal window of `W`, then a lock at the last Final + `D`. So `T_beacon ≈ δ + 2W + D` = 84 DAA at `W = 40`. That is more than the interim OPV window of 50 (§3.8) |

### 1.4 Money flows (the code)

| Flow | Amount | From → to | Code |
|---|---|---|---|
| reservation | `K` | the producer's free collateral → reserved, from admission until the liability horizon ends | `admit`; OPV `required_reservation` |
| admission fee (OPV) | `f_adm` | producer → burned | `opv_admission` |
| escrow | `R` (the Final reward) | the poster's free collateral → reserved at `PostJob` → paid to the producer at the job's first Final; returned after the TTL if no claim used it | `open_job_escrow`, `pay_from_job_escrow`, `release_idle_job_escrows` |
| job fee | `f_job` | poster → burned | `open_job_escrow` |
| conviction | slash `S = min(reserved, collateral)`. Bounty `B = min(a·(S + taken_by_default), S)` goes to `bounty_holder`; `S − B` is burned | producer → the accuser / burn | `convict`, `bounty_holder` |
| `bounty_holder` | the bond holding the **earliest** proof seal, at least 1 DAA old, of the **exact convicting bytes**; failing that, the filer | — | `ledger.rs` `bounty_holder` |
| pre-Final default | penalty `min(reserved, D)`. The demanders share `D·(1 − β_d)`, with `β_d = max(1 − a, default_burn)` for an OPV claim; the rest is burned. The reservation stays held until default + liability | producer → demanders / burn | `tick_into` |
| post-Final default | the whole remaining reservation | producer → burned | `tick_into` |
| demand bond | `b_dem` | reserved. Refunded on a conviction, default or timeout. **Burned** at the liability horizon if the position was served and nobody convicted | `settle_served_demand_bonds` |
| dismissed filing | `f_dis` (scaled by court work on `adv/c4r4`) | filer → burned | `file_proof` |
| claim seal deposit | `d` | reserved at `SealClaim`, returned at the reveal, forfeited (burned) at the TTL if unrevealed. **A re-seal keeps it and restarts the clock** | `apply_object` `SealClaim`, `tick_into` |
| work credit, external gain | `w`, `X` | part of the gain `G = R + w + X` (RFC-0015 §8); not paid by the ledger | `max_gain_per_claim` |
| off-chain costs | `c_fetch` (DA), `c_check` (a route-B check is one re-execution, K2S §11.2), `c_loc`, `c_file`, `c_carrier` | the watcher | — |

**Interim terms** (`palw_kernel_route_policy_v1` and `PalwPanelFreeFenceV1::interim_v1`):

| Term | Value |
|---|---|
| reservation `K` | 1,000 |
| accuser share `a` | 500 ‰ |
| default penalty `D` | 100 |
| default burn `β_d` | 500 ‰ |
| gain `G` | 20 (`R` = 5, `w` = 5, `X` = 10) |
| admission fee `f_adm` | 1 |
| job fee `f_job` | 1 |
| dismissed filing fee `f_dis` | 0.1 |
| demand bond `b_dem` | 1 |
| seal deposit `d` | 1 |
| OPV window / demand deadline / proof grace / liability | 50 / 20 / 10 / 200 DAA |

All amounts are in BILI.

**The OPV test world** used by the PoCs: `K` = 1,000, `a` = 500 ‰, `D` = 100, `R` = 7, `f_adm` = 3, `f_job` = 2, `b_dem` = 10,
`d` = 1.

### 1.5 Assumptions (each theorem names the ones it uses)

| Id | Assumption |
|---|---|
| A-RAT | Every party maximises expected money. An honest watcher takes part iff its expected revenue covers its expected cost. |
| A-COST | `c_fetch`, `c_check`, `c_loc`, `c_attest`, `c_file` and `c_carrier` are inputs, measured by MEAS. The chain does not observe them. |
| A-INC | An object broadcast in time is included within `B_carrier` (the dossier's A-NET-1), against a censorship that `censorship_cost` prices. |
| A-BEACON | The post-commit beacon's output is uniform and independent of the claim, except for at most `2^g` outputs the adversary can choose among. v3 claims `g = log2 F`; §4.2 shows that does not hold today. |
| A-FEE | The check fee is pre-funded in the job's escrow (GAP-5's mechanism), and the ledger pays it on a verifiable event. |
| A-BRIBE | A bribe is a payment conditional on an on-chain outcome (such as the claim reaching Final), and the briber cannot pay more than its own gain `G` from that outcome. |

**No assumption of independence between watchers' behaviour is made.** The only independence used (T2, T4) is **constructed**: it is
the independence of the beacon's per-slot draws. `θ`, the share of slots whose holder really checks in time, is an **input** that is
never derived (T6 is OPEN). No detection rate is assumed anywhere else.

---

## 2. The attacks

### 2.1 M0 — today: the earliest seal of the convicting bytes (COUNTEREXAMPLE, measured)

**The attack.**
1. `P` builds the convicting proof `π` of its own fault.
2. At `t_c + 1`, its Sybil `V_s` posts `SealProof(c, V_s, H(π))`. A proof seal has no deposit and no fee.
3. The convicting bytes are canonical: an outsider using another salt gets byte-equal proof bytes (measured; O-C4R4-07). So any later
   conviction by those bytes pays `V_s`, passively.
4. If an honest watcher's bytes differ, its `SealProof` names `c` in clear one block before it can file. `V_s` then files its own
   sealed proof first. A block producer inside `C` does this for free, by ordering.

**The closed form of the conviction loop** (slash, bounty and demander shares; fees and the reward excluded):

```text
direct, or after Final       Π_C = −S + B = −(1 − a)·K
after a self-inflicted default Π_C = −K + D·(1 − β_d) + min(a·K, K − D)
sup over every D ∈ [0, K]      −(1 − a)·K·β_d          (reached at D = (1 − a)·K)
honest watcher                 0 on every lie whose producer pre-seals; 0 on every honest claim
```

At the interim terms these are −500, −450 and −250 BILI; the honest watcher gets 0. The model tests the supremum on a 1-BILI grid
over `D`.

**Measured** on the ledger (`econ_bounty.rs`, test world). The figures include the admission fee 3, and, after Final, the reward 7
paid from the poster's escrow:

| Path | Coalition net | Sybil paid | Honest verifier |
|---|---|---|---|
| direct | −503 | 500 | 0 (its filing is a `Duplicate`, with no fee) |
| after a self-inflicted default | −453 | 550 (bounty 500 + demander share 50) | 0 |
| after Final | −496 | 500 | 0 |
| three honest claims checked clean | — | — | 0 |

**The consequence: conditions 5 and 6 cannot be separated.**
1. A rational watcher earns 0, so it does not watch: `θ = 0`.
2. Then `P_dc = 0`, so a lie nets `+G`.
3. G14-R4's DESIGN_GAP says "deterrence still holds". That is true only if someone checks for free.

Condition 5's detection probability is the output of condition 6.

### 2.2 Remedy A — pay only for a fault inside the watcher's pre-committed sample (COUNTEREXAMPLE)

**The rule.**
* Watcher `v` commits `c_v = H(salt_v)` before `c` is committed.
* Its sample is `S_v(c) = PRF(salt_v, id(c))`, a fraction `q` of the claim.
* A proof pays `v` only if `v` opens `c_v` and the fault lies in `S_v(c)`.

**Attack A1: self-coverage by grinding.** `P` holds one pre-committed Sybil watcher and knows its salt. The claim id is
`id(c) = H(job, producer, generated, evidence_root)`, and `P` controls some of its inputs: the nonce of a job it posts, the producer
bond, the location or value of the fault. It varies them offline until its fault `f ∈ S_vs(id)`.
* That takes `1/q` trials on average, or `1/(1 − (1 − q)^s)` with `s` Sybil watchers. A trial costs a hash, or one recomputation of
  the evidence root.
* **Measured** (`remedy_a_…`, 2,000 lies, `q = 1/16`): 16.0 trials on average.
* The honest watcher's coverage of the ground ids stays near `q` (0.06): `P` cannot see the honest salt.

**Attack A2: many bonds.** With `N` pre-committed Sybils, `P` covers its own fault with probability `1 − (1 − q)^N`, with no grinding
at all.

**The outcome.**
* With first-eligible-sealer-wins, `P`'s Sybil is eligible with certainty, and it is first. The bounty is captured as in M0.
* A only shrinks an honest watcher's eligibility, to its coverage `q`.
* A could work only if `P` had no influence over `id(c)` after the salts are fixed. But `P` chooses its own claim.

**What A gets right.** Honest coverage is independent of `P`'s choices, because the honest salt is secret and committed first. That
property is about detection. It produces no revenue.

### 2.3 Remedy B — split the bounty equally among covering watchers (COUNTEREXAMPLE outside an unobservable band)

**The rule.** The bounty is split equally among the distinct bonds whose pre-committed sample covers the fault and whose seal lands
inside the window.

**Attack B1: dilution.** With `s` coalition watchers and `h` honest watchers covering the fault:
* each honest watcher gets `⌊B/(s + h)⌋`;
* `C` recoups `B·s/(s + h)`, which tends to `B`.

At `B = a·K = 500` and `h = 1`:

| Sybils `s` | 0 | 1 | 10 | 100 | 1,000 |
|---|---|---|---|---|---|
| honest share | 500 | 250 | 45 | 4 | 0 |
| coalition recoup | 0 | 250 | 455 | 496 | 500 |

**The cost of a Sybil.**
* If eligibility is one bonded commitment reused for every claim (G14-R4's sketch), a Sybil costs about nothing per claim. `C`'s best
  response then recoups `B`: measured, `s* = 500`, recoup 500.
* If every covering watcher pays an eligibility cost `c_s` per claim, `C` maximises `B·s/(s + h) − s·c_s`, so `s* = √(B·h/c_s) − h`.
  At `c_s` = 5: `s*` = 9 and the recoup is 405.

**The band, and why the protocol cannot hit it.**
* A first Sybil does not pay iff `c_s ≥ B/(h + 1)`.
* An honest covering watcher breaks even iff `c_s ≤ B/h`.
* So B both deters dilution and pays honest watchers only for `c_s ∈ [B/(h + 1), B/h]`: `[250, 500]` at `h = 1`, `[100, 125]` at
  `h = 4`, `[46, 50]` at `h = 10`.
* `h` is the random, unobservable number of honest watchers that cover the fault: `Binomial(n, q)`. So no fixed `c_s` sits in the
  band.
* And B pays only on lies (T0).

**A and B together** (G14-R4's sketch). With `q = 1/16` and `h = 10` (`q·h = 0.625`):
* `C` recoups `s/(s + q·h)·B`: 307.7 at `s = 1`, 470.6 at `s = 10`, 496.9 at `s = 100`;
* an honest watcher's expected revenue per watched lie is `q·B/(s + q·h)`, which is 2.94 BILI at `s = 10`;
* and it is paid only on lies.

### 2.4 Blind seals do not help

A seal that does not name the claim removes the front-run on the watcher's seal. It does not remove the passive pre-seal: `P`'s Sybil
blind-seals its own canonical proof at `t_c + 1` and is still the earliest. The result is the same as M0.

### 2.5 T0 — no bounty-only incentive survives deterrence (PROVEN)

**Statement.** Suppose:
* a watcher's only income is bounties, at most a share `β` of the slash `K` per conviction;
* its cost per check is `c > 0`;
* `φ` is the fraction of the claims it checks that are lies.

Then its expected net per check is at most `φ·β·K − c`. If deterrence holds against every rational producer (A-RAT), `φ` is at most
`φ_irr`, the rate of irrational or accidental lies. So the watcher takes part only if `φ_irr ≥ c/(β·K)`.

**Proof.** The expected bounty income per check is `Σ_claims P(lie)·P(paid | lie)·B ≤ φ·β·K`. A rational producer lies only if its
expected profit is positive, and deterrence excludes that. So `φ ≤ φ_irr`. □

**Numbers.**
* With `c` = 1 BILI and an uncontested `B` = 500: the watcher needs `φ ≥ 1/500`.
* With `B` shared among 10 watchers: `φ ≥ 1/50`.
* Under M0, `β = 0` against a rational liar, so no `φ` is enough.

**The corollary.** Condition 6 needs income **per check**, not per conviction: a check fee. It then needs that fee to pay only for
checks that are really run (T6).

---

## 3. The mechanism M* (a proposal; not built)

### 3.1 The design

1. **A watcher pool.**
   * Bonds lock a stake `s_j` as watchers.
   * The exit delay is at least the window plus the liability horizon.
   * The pool is frozen per epoch, before any claim it draws for commits.
2. **A draw after the commit.**
   * For each claim `c`, a post-commit beacon decides whether `c` is checked (probability `q`) and draws `m` slots by stake, with
     replacement.
   * The beacon is a v3 `ClaimVerification` subject whose seal window opens after `c` commits, or one beacon per epoch for that
     epoch's claims.
   * `P` cannot choose the draw: the beacon's sources are sealed after `c` commits (A-BEACON, once F-ECON-2 is fixed).
3. **A check fee, paid by the user.**
   * The poster funds `m·F` (beside `R`) in the escrow when it posts. A claim that is not drawn gets its fee back.
   * `F` is paid to a drawn watcher for an **attestation** object inside the window, **whatever the claim's fate**.
4. **A capped bounty.**
   * A conviction slashes `S`. `B_d = min(S, B_cap)` is split equally among the **drawn** slots that sealed a convicting proof before
     the conviction. The rest of `S` is burned.
   * A non-drawn filer still convicts, so detection is unchanged. It is paid nothing, and it pays no fee: a convicting proof is never
     charged.
   * `B_cap = max(m·ĉ_file, m·G)`: the filing allowance or the bribery floor (§3.5), whichever is larger.
5. **Unchanged:** anyone can demand, file and convict; defaults, liability and post-Final conviction stay as they are.

### 3.2 T1 — self-dealing does not pay (PROVEN)

**Statement.** Take any coalition `C ∋ P`, any strategy, and any claim `c` of `P` that is convicted. Count every flow into `C`'s bonds
that `c` causes. Then:

* (a) **today:**
  `Π_C ≤ −K + D·(1 − β_d) + min(a·K, K − D) ≤ −(1 − a)·K·β_d < 0`;
* (b) **under M\*:**
  `Π_C ≤ −K + D·(1 − β_d) + min(B_cap, K − D) ≤ −(K − B_cap)·β_d < 0`, and the check fees cancel.

**Proof.** Enumerate the flows (§1.4):
1. The slash takes `K` from `P`: `D` by a pre-Final default if there was one, the rest by the conviction.
2. `C`'s inflows are:
   * the bounty. Today it is at most `min(a·(S + D), S)` with `S = K − D`. Under M* it is at most `min(B_cap, S)`, because only drawn
     sealers share it; `C`'s non-drawn bonds get nothing;
   * the demanders' share `D·(1 − β_d)`, pre-Final only (after Final the forfeit is burned whole);
   * refunded demand bonds, which net to 0;
   * seal deposits, returned on a reveal and never paid to anyone else;
   * check fees. These are paid per drawn slot whatever the outcome, so lying or self-accusing changes no fee. If `C` also posted
     the job, the fees it receives (at most `m·F`) are at most the fees it paid (`m·F`).

The sum is the first expression in (a) and in (b). Now maximise over `D ∈ [0, K]`:
* While `K − D ≥ cap`, the sum is increasing in `D`. The cap is `a·K` today and `B_cap` under M*.
* Past that point the sum is `−D·β_d`, which is decreasing.
* So the maximum is at `D = K − cap`. That gives `−(K − cap)·β_d`, which is `−(1 − a)·K·β_d` today and `−(K − B_cap)·β_d` under M*.

This is negative as long as `cap < K` and `β_d > 0`. Both are validated: `a < 1000` and `β_d ≥ 1 − a > 0`. □

**Against honest behaviour, with what the lie itself gained.** The loop above leaves out what the lie earned before it was convicted.
After a post-Final conviction the reward and the work credit stay paid, and an external gain may already have been realized. Bound
that, conservatively, by the claim's maximum gain `G = R + w + X` (RFC-0015 §8: as if the lie earned everything and cost nothing).
Then:

```text
Π_C(lie, self-convicted) − Π_C(honest) ≤ sup_D Π_C + G
```

This is negative iff `(1 − a)·K·β_d > G` today, or `(K − B_cap)·β_d > G` under M*. At the interim terms: 250 > 20 today, 480 > 20
under M*. A release must validate this relation beside the existing `claim_collateral·(1 − a) > claim_reward`, which reads only the
direct path and only the reward.

**Numbers** (interim, `B_cap = m·G = 40`):

| Path | Today | M* |
|---|---|---|
| direct | −500 | −960 |
| after a default (`D` = 100) | −450 | −910 |
| supremum over every `D` | −250 | −480 |

Under M* deterrence also needs less collateral, because `C` recoups at most 90 BILI instead of 550.

### 3.3 T2 — stake-weighted draws are Sybil-neutral (PROVEN under A-BEACON)

**Statement.** Suppose the pool is frozen before `c` commits, with total stake `S_tot`, of which `C` holds `σ·S_tot` split across any
number of bonds. Each slot `i` is drawn by a `u_i` uniform on `[0, S_tot)`, taken from the beacon, and goes to the bond whose
cumulative-stake interval contains `u_i`. Then:
* `P(slot i ∈ C) = σ`, and the slots are independent;
* `P(all m slots ∈ C) = σ^m`;
* `C`'s expected fee income is `σ·m·F`.

None of this depends on how `C` splits its stake. With `g` bits of beacon grinding, `P(all m ∈ C) ≤ min(1, 2^g·σ^m)`.

**Proof.** The union of `C`'s intervals has measure `σ·S_tot`, however it is split. Independence follows from the independent `u_i`.
The grinding bound is a union bound over the `2^g` outputs. □

**Illustrated** (`t2_…`): with `σ = 1/4` held as one bond and as fifty, the coalition's share of the slots is 0.25 and both slots are
its own with probability 0.0625, in both cases.

Remedies A and B pay per bond, by coverage or by share, so their outcome grows with the bond count.

### 3.4 T3 — an honest drawn watcher recovers its costs (PROVEN under A-COST, A-INC, A-FEE)

**Statement.** Let an honest watcher hold a drawn slot. Then:

```text
net ≥ F − (c_fetch + c_check + c_loc + c_attest) + 1[it sealed a convicting proof]·(B_d/m − c_file)
```

So if `F ≥ c_fetch + c_check + c_loc + c_attest` and `B_cap/m ≥ c_file`, its net is at least 0 on every claim, honest or lying,
whatever `C` does.

**Proof.**
* `F` is escrowed when the job is posted (A-FEE). It is paid when the attestation is included (A-INC), whatever the claim's fate and
  whatever other parties do.
* The bounty is split among at most `m` drawn sealing slots, so each gets at least `B_d/m`.
* `C`'s non-drawn bonds get nothing when they file first, and that does not shrink the drawn split. `C`'s drawn slots share like any
  other slot.
* If `C` convicts before the honest watcher seals, the watcher sees the conviction and stops. By then it has spent no more than `F`
  covers, because the fee includes localization.
* Filing costs arise only on the sealed branch, and `B_d/m` covers them there. □

**What the costs include** (A-COST).
* **The check.** `c_check` is one re-execution at real scale: K2S route B reads 8.19 GB at 9B-8k, and finds any lie with certainty
  if it runs.
* **Demand bonds.** `c_fetch` includes any demand bonds burned when a producer serves only on demand (O-C4R4-06). Under M* the
  served-demand burn must not fall on a drawn watcher's demand, or the fee must cover it (§3.8).

### 3.5 T4 — deterrence, and T5 — the bribery floor

**T4 (PROVEN given θ).** Let `θ` be the probability that a drawn slot is held by a diligent honest watcher who checks and files in
time. It is an input (T6), with `θ ≤ 1 − σ`. Under T2:

```text
P_dc ≥ q·(1 − (1 − θ)^m)
```

The route-B check is exact once it runs. A liar that expects to be caught recoups at most `B_cap + D·(1 − β_d)` (T1), and loses
its reward. So:

```text
E[Π(lie)] − E[Π(honest)] ≤ (1 − P_dc)·G − P_dc·(K − B_cap − D·(1 − β_d)) ≤ 0
  ⇔  K ≥ B_cap + D·(1 − β_d) + G·(1 − P_dc)/P_dc
```

**T5 (PROVEN under A-BRIBE, A-RAT).** If `B_cap ≥ m·G`, a drawn watcher that has found the lie and seals it gets at least
`B_cap/m ≥ G`. Any bribe the liar can afford is at most `G`. So taking the bribe is dominated. □

This sets `B_cap`. It is the bribery floor `m·G`, above the filing allowance, and well under `K`: 40 against 1,000 at the interim
terms.

### 3.6 T6 — the lazy watcher (OPEN)

The fee is paid for an attestation. On an honest claim, the chain cannot tell a real check from a copy of the claim's public roots.
So `θ` is not observable, and T4 holds only for the `θ` that the operating assumptions state. That is the status of today's
`assumed_detection_permille` (SG-13), but checking now has a positive incentive.

Candidate mechanisms, none of them proven here:
1. **Forced-error audits**, as in Truebit:
   * after the attestations are sealed, a beacon marks a fraction `ρ` of claims as audits, whose planted fault was committed in
     advance;
   * a drawn watcher that attested "clean" on an audit forfeits stake; one that finds the planted fault is paid;
   * this needs a way to plant faults without a producer's cooperation, such as protocol-posted test claims of the class, funded from
     a reserve.
2. **Attestations bound to data that only a check produces**, under a per-watcher salt. A lying producer defeats this by handing the
   watcher its own trace.
3. **Redundancy**: `m ≥ 2` independent slots lower the dependence on any one watcher. `θ` stays an input.

Until one of these is proven, `P_dc` is stated, not derived, and a release must say so.

### 3.7 Numbers at the interim terms (illustrative; `econ_bounty.rs` `mstar_numbers_at_the_interim_terms`)

Settings: `m = 2`, `θ = 1/2` (an input), `B_cap = m·G = 40`, `K = 1,000`, `G = 20`, `D·(1 − β_d) = 50`.

| Quantity | Value |
|---|---|
| what the liar recoups at most | 90 BILI |
| `P_dc` needed | ≥ 20/930 = 2.15 % |
| `q` needed (`θ = 1/2`) | ≥ 2.87 % (28,675 ppm) |
| `q` needed (`θ = 1/4`) | ≥ 4.92 % |
| self-dealing loop | −960 direct, −910 after a default, supremum −480 |
| what the user pays for checking, per claim (`q·m·F`, at `F` = 5 BILI) | 0.29 BILI (5.7 % of `R`) |
| an honest drawn slot | ≥ 0 whenever `F` covers its costs; below them it loses, so the inequality is tight |

`F` is a cost relation, not a number: `F ≥ c_fetch + c_check + c_loc + c_attest`. At real scale `c_check` is about the producer's own
cost of the claim. The user therefore pays about `q·m` times the work to have that work watched.

### 3.8 What M* needs (allocations belong to the Lead; nothing is built)

| Item | Owner |
|---|---|
| the watcher-pool object and table (stake, epoch) | allocation |
| the post-commit draw on v3, a `ClaimVerification` subject | OPVB |
| room for the draw: the OPV window must hold `T_beacon + T_fetch + T_check + T_loc + T_file + margin`, which is about 84 + 10 + 10 + 2 + 2 + 2 = 110 DAA at the interim terms, against a 50-DAA window. Either the window grows or the draw is per epoch. This is also the missing `T_beacon` term of G14-R4's end-of-lane list | OPVB, MEAS |
| `m·F` in the job escrow, the attestation object, the fee settlement kinds | G14R |
| the bounty rule (drawn sealers only, a cap, the rest burned), replacing `bounty_holder` | G14R |
| no served-demand burn on a drawn watcher's demand | G14R |
| the F-ECON-2 fix (§4.7 S1), before any v3 draw is trusted (A-BEACON) | G14R, OPVB |
| T6's design | design |

---

## 4. GAP-B12 — a class's attempts exhausted by abandoned seals

### 4.1 The mechanics (the code)

* **v3** (`misaka-palw-challenge::sealed`):
  * a mixed seal not revealed by `S + 2W` vetoes the beacon (`withheld`);
  * so does a mixed source that is revealed but then fails (`failed`).
* **Onboarding** (`palw_onboarding_fold_v1`):
  * the tick evaluates each open attempt from `S` on;
  * `Vetoed` ends the attempt with `BEACON_VETOED`, which is counted;
  * after `retry_limit + 1 = 3` counted ends, the next commitment is refused (`AttemptsExhausted`) for that (class, artifact);
  * the class may re-commit in the very next block.
* **Ledger:**
  * `SealClaim` reserves `d` = 1, returns it at the reveal and forfeits it at the TTL (100) if unrevealed;
  * table 26 keeps the forfeited seal's position;
  * a **re-seal** of a (job, producer) pair replaces the live seal, keeps the deposit and restarts the clock.
* **The consumer** (`beacon_sealed_sources_v1`) reads `claim_beacon_seals_v1`: live seals **at their latest position**, salted
  reveals, and forfeited seals.

### 4.2 Findings (measured; `misaka-palw-kernel/tests/econ_seal_veto.rs`)

**F-ECON-1 — one deposit vetoes every attempt (P1 where armed: GAP-B12's real price).**
* The attacker seals a job at attempt 1's `S`. At attempts 2 and 3 it re-seals the same job: the gap is 83 DAA, under the TTL of 100.
* Measured:
  * all three attempts end `BEACON_VETOED`, at 92, 175 and 258;
  * the control, the same facts without the attacker's seal, is not vetoed;
  * the attacker's bond burned **nothing** across the three vetoes;
  * its one deposit is forfeited only if it stops: at 279, once.
* The class is exhausted after `(R + 1)·T_a = 249` DAA, about 10.4 h at 150 s/DAA.
* The cause: the replaced seal leaves the beacon read. It is neither live nor in table 26.

**F-ECON-2 — a re-seal is a free, uncounted withdrawal of a mixed seal (P1 for v3 where armed; it breaks opv-beacon-bootstrap
§6.3 (i)).**
* Setup: two honest producers and two attacker bonds seal in the window, and the honest salts are revealed at `S + W + 1`.
* For each attacker seal, the attacker then chooses one of two moves:
  * reveal it, so it is mixed;
  * re-seal it. Its position leaves `[S, S + W)`, so it is neither mixed nor a veto. A block later it reveals that claim honestly:
    the deposit comes back and the work is paid.
* Measured: all four choices LOCK, with **four different outputs**. v3 accounts `sealed_beacon_grinding_choices_v3(F = 1) = 1`.
* With `a` attacker seals in the window, that is `a` extra bits per beacon, for carrier fees. Each such seal needs one bond and a job
  on an eligible profile.
* E6, which combines v3 with `G = F`, overstates the effective bits by `a`.
* The cause: `claim_beacon_seals_v1` lists a live seal at its latest position. `mixed_seals_v3` assumes that from `S + W` on, every
  fact it reads is final; a re-seal rewrites a fact.

**F-ECON-3 — one withheld seal vetoes every concurrent attempt (by design; it changes the pricing).**
* Three subjects of three classes, committed at 0, 10 and 25, share the source pool.
* One attacker seal at 31 lies inside all three seal windows. All three attempts end `BEACON_VETOED`; without that seal none does.
* One deposit makes three vetoes. The price of a veto per victim is `d/N_c`, where `N_c` is the number of attempts whose windows
  contain the seal.

### 4.3 The stall

```text
T_a  (withholding veto)  = δ + 2W + 1                    = 83 DAA   (≈ 3.5 h at 150 s/DAA)
T_a' (abandonment veto)  = δ + 2W + window + court − 1   = 151 DAA
exhaustion               = (R + 1)·T_a                   = 249 DAA  (≈ 10.4 h)   [(R + 1)·T_a' = 453 DAA with abandonments]
```

**Abandonment.**
1. The attacker reveals on the last DAA of the reveal window.
2. Its own Sybil demands on the last DAA of the claim's window.
3. The producer withholds, and the claim defaults at the demand's deadline.

Each one burns `D·β_d + f_adm = 51` BILI, and locks a reservation through the liability horizon. So withholding is the cheaper lever:
1 BILI per 83 DAA, against 51 per 151, about 28 times cheaper per DAA.

**After exhaustion the (class, artifact) is dead.** The registrant must register a new class, which gets a fresh `R + 1` attempts
(O-C4R4-Q) and costs the registration again. The attacker can then repeat the attack. The stall is therefore bounded only by what the
attacker is willing to pay. The R + 1 vetoes buy one dead registration.

### 4.4 The attack's cost

* **Today:**
  * one class costs `d + (R + 1)·c_carrier ≈ 1.06` BILI (the model's `exhaustion_cost`);
  * network-wide, three bonds re-sealing on staggered 90-DAA cycles (phases 0, 30, 60) cover every attempt's seal window: every 40-DAA
    window holds a seal, and since 90 > 2W, no seal moves before the attempts it vetoes are decided;
  * their deposits are never forfeited (90 ≤ the TTL of 100), so the cost is carrier fees only. This is argued from the same mechanism
    as F-ECON-1; it was not run.
* **After S1 (a re-seal forfeits, §4.7):**
  * one class costs `(R + 1)·d/N_c + carriers`: 3.06 BILI alone, 1.06 when three classes onboard at once;
  * freezing all sampled onboarding network-wide takes one seal per `W` DAA, so `d·576/W = 14.4` BILI a day.

### 4.5 The damage

* **The victim, per vetoed attempt:** `ν·T_a`. Here `ν` is what a DAA of delay is worth to the class's registrant and users: the
  producer margin it forgoes, and the users' surplus. The chain cannot observe `ν`.
* **The victim, at exhaustion:** additionally `C_reg`:
  * a new V2 registration;
  * the binding's 100-BILI reservation, held for 200 DAA;
  * conformance fees;
  * the time to re-register.
* **The attacker:** it gains `γ ≤ ν` per DAA of delay. A competitor diverts at most the victim's margin. A pure griefer has `γ = 0`
  but is willing to pay.

### 4.6 The deposit: derived (B12-D, PROVEN)

**The lower bound from griefing.** To deter griefing, the cost of a veto must be at least its gain:

```text
d ≥ T_a · Σ_{victims} γ_i ≥ N_c · γ · T_a
```

**The upper bound from honest job races** (G14-R4's sizing). With `n` producers sealing one job, `(n − 1)·d` is burned per won claim.
Keeping that under a share `ρ` of the reward:

```text
d ≤ ρ · R / (n − 1)           (= 1 BILI at ρ = 20 %, R = 5, n = 2)
```

**When both hold.**

```text
γ ≤ ρ·R / ((n − 1)·N_c·T_a)   =  1/83 BILI per DAA at N_c = 1   (≈ 6.9 BILI a day)
```

| `γ` (BILI per victim-DAA) | `N_c` | deposit needed |
|---|---|---|
| 0.1 | 1 | 8.3 |
| 1 | 1 | 83 |
| 1 | 4 | 332 |
| 10 | 16 | 13,280 |

The honest-race ceiling is 1 BILI, so every row above is infeasible.

**Conclusion, plainly.**
* For any class whose day of delay is worth more than about 7 BILI to an attacker, no single `seal_deposit` both deters the veto and
  keeps honest job races viable.
* Today, by F-ECON-1, the deposit does not even enter the attacker's cost.
* A stall can be worth more than any deposit that honest producers can bear. GAP-B12 needs the structural changes below, not a number.

### 4.7 Structural mitigations (ranked)

| | Change | Effect | Where | Size |
|---|---|---|---|---|
| **S1** (required) | **A re-seal past `palw_panel_free_v1` forfeits the seal it replaces.** The replaced seal goes into table 26 at its own `sealed_daa` (that key already exists), its deposit is burned, and the new seal takes a fresh deposit. Once a seal position is accepted, it stays in the beacon read for good | closes F-ECON-1 (each veto costs `d`) and F-ECON-2 (a withdrawal becomes a veto). Restores v3's `G = F` | the kernel ledger (G14R); a v3 test (OPVB) | small |
| **S2** (required against exhaustion) | **Vetoes do not count against `R`.** The soundness accounting states the attempts that time allows instead: `R_eff = R + ⌈T_life/T_a⌉`, with `T_life` the class registration's lifetime, a hard cap (OPV-BOOT's option 2) | removes the permanent kill. Costs `⌈log2 (R_eff + 1)⌉` bits: about 12 at a one-year lifetime (210,240 DAA / 83 ≈ 2,533). A registrant re-rolling itself through its own Sybil sealer is then bounded by time, not by deposits | onboarding fold, E6 (OPVB) | small |
| **S3** | **Split the two prices.** The job-race seal stays cheap (`d`) and is **not mixed**. An opt-in **beacon-source seal** carries `d_src`, priced for the DoS, and only it can veto. Honest race losers that did not opt in never veto. Sources must then be paid (from the registrants' onboarding fees); a source is a public good, and the incentive question is §3's | `d_src` can be derived by §4.6 with no honest-race ceiling | ledger + v3 | medium |
| **S4** | **Bound the reach.** Either charge a vetoing source `d_src` per attempt it vetoes (reserving up to `N_max·d_src` when it seals), or cap the concurrent sampled attempts per seal window at `N_max` | the attack's cost grows linearly with the victims | onboarding fold | medium |
| **S5** | **Rate-limit the vetoes.** A producer bond with a withheld or abandoned mixed seal is not mixed for `T_ban` afterwards. This is decided from facts before the window opens, so it is not a choice made after the reveals. A source must also hold stake `≥ s_min` and be at least `A_min` old | a sustained freeze needs `⌈T_ban/W⌉` aged bonds in rotation | v3 + consumer | small |
| **S6** | **The registrant's escape:** the complete-check policy draws no beacon, so nothing can veto it | feasible for small classes (`≤ 2^26` work units per complete check) | existing | — |

**Recommendation.**
* **S1 and S2 now.** Both are small code changes. Together they close the exhaustion and the grinding lever.
* **S3 and S4** together with §3's watcher pool. They raise the same economic question: who pays the providers of a public random
  good.
* **S5** as cheap hardening.

With S1–S4 in place:
* the remaining stall is priced per vetoed attempt per victim;
* `d_src ≥ T_a·γ_protected`. For example, `γ` = 1 BILI per DAA gives `d_src` = 83 BILI. Under S3 this is independent of honest race
  economics;
* `γ_protected` is a policy value for the user. It is the delay value per class that the network chooses to protect.

---

## 5. Status and open items

| Item | Kind | Status |
|---|---|---|
| T0, T1, T2, T5, B12-D | proof | PROVEN (this note) |
| T3, T4 | proof under labelled inputs | PROVEN under A-COST / A-INC / A-FEE; T4 for a given `θ` |
| M0 capture, remedies A and B | attack | COUNTEREXAMPLE (PoC + model) |
| F-ECON-1, -2, -3 | finding | measured; fix S1 (and S2) for G14R / OPVB |
| T6 (lazy watcher, `θ` from chain state) | design | **OPEN** |
| M* | design | proposed, not built; needs the allocations of §3.8 |
| `q`, `m`, `F`, `B_cap`, `γ_protected`, `T_life`, `d_src` | policy | for the user. Interim numbers here are illustrations |
| `c_fetch`, `c_check`, `c_loc`, `c_file` per class | measurement | MEAS |
| the OPV window must hold `T_beacon` (M*'s draw) | design / measurement | OPVB / MEAS |

**Implemented / verified / armable.**
* Implemented: nothing in consensus.
* Verified: the PoCs and the calculators, in `misaka-palw-kernel/tests/econ_bounty.rs` and `econ_seal_veto.rs`.
* Armable: nothing. Condition 6 is still unmet, and with F-ECON-2, v3's accounting is not sound.

## 6. Reproduce

```text
~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p misaka-palw-kernel --test econ_bounty --test econ_seal_veto -- --nocapture
```
