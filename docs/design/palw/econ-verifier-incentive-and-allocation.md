# The verifier's incentive, self-dealing at 49%, the seal deposit, and model-bond allocation (lane ECON, 2026-10-10)

Branch `econ/verifier-incentive` (integration head `b8ae9412b` merged; G14-R4 and OPV-BOOT merged earlier for the PoCs).
Supersedes the first-milestone note `verifier-incentive-and-seal-economics.md` (10-09, at a 50% accuser share), which stays as the
record of that milestone.

**Status: DESIGN + PoC.** No consensus code changes. Nothing here is armable. The user's 2026-10-10 rules apply throughout:
* **ADR-0032:** the PALW reporter share is **49%** (`palw_reporter_share_v2`). A self-reporter recovers 49%, so its net loss is never
  the gross slash.
* **ADR-0176:** bond and a common window `W` bound claims, reward blocks, rewards and Final weight (Q/B/R/F).
* **ADR-0177:** the chain does not interfere with model acquisition. Effective detection `p` can be 0 for a closed model. Model
  coinbase is allocated through `f(S_m)`.

**Labels.** Every number below carries one label:

| Label | Meaning |
|---|---|
| MEASURED | read from consensus code, or asserted by a ledger PoC on this branch |
| INTERIM | a code value marked interim; never a production value |
| DERIVED | computed from labelled inputs, in the model |
| ASSUMED | an illustrative input with no measurement yet; MEAS or the user replaces it |
| PROPOSED | a POLICY value ECON proposes for the user to fix |

**Executable model:** `python3 -I scripts/misaka-palw-econ-verifier-model.py`. It covers §1–§8 and exits 1 if any of its checks
fails.

**Ledger PoCs:** `misaka-palw-kernel/tests/econ_bounty.rs` and `econ_seal_veto.rs`. These test only claims about consensus code.

Amounts are in BILI.

---

## 0. Results

### 0.1 The results table

| Id | Statement | Status |
|---|---|---|
| **M0** | **Capture.** Today the earliest seal of the exact convicting bytes takes the bounty. On every self-dealing path, the liar's own Sybil takes the whole 49%, and the honest verifier who found the lie earns 0. Checking an honest claim also earns 0. | **COUNTEREXAMPLE**. MEASURED at 49% (`econ_bounty`). |
| **E1** | **The 49% does not bound a self-dealer.** After a self-inflicted pre-Final default, the coalition recoups the demanders' share and the bounty: 539 of 1,000 collected. Its net loss is 46.1%, below ADR-0032's 51%. Over every penalty `D`, the loss can fall to `(1 − a)²·K` = 26%. | **FINDING**. MEASURED (`econ_bounty`) and DERIVED. Fix: §2.2. |
| **S** | **Self-dealing never pays** (self-fraud, self-accusation, bounty receipt, check fees). With E1 fixed, a coalition that convicts its own lie loses at least `(1 − a)` of what was collected. An honest claim cannot be convicted. Self-dealing only turns gross `K` into `L_net = (1 − a_eff)·K` inside ADR-0176 D6. | **PROVEN** (§2.3). |
| **Y** | **Per bond and window** (ADR-0176). With fully backed reservations, every forging strategy is deterred only if reward / capital per window `< p·(1 − a_eff)·W / (H_L·(λ + x − p·r_risk))`. At `p` = 0 no positive reward yield is safe. | **PROVEN** (§2.4). |
| **C** | **Correlation and shared collateral.** With fully backed reservations (`κ ≤ 1`), expected collection is linear and correlation is irrelevant. With `κ > 1`, the worst case (comonotone detection, which a forger can induce) divides deterrence by `κ`. | **PROVEN** (§2.5). |
| **T0** | **A bounty-only verifier cannot be paid** once deterrence works: its income is at most the fraud rate times its share. | **PROVEN** (predecessor). Numbers redone at 49% (§3.1). |
| **A, B** | Remedy A (pay only inside the watcher's salted sample) falls to grinding, about `1/q` trials. Remedy B (equal split) is diluted by Sybils: 245, 44 and 4 BILI at 1, 10 and 100 Sybils. | **COUNTEREXAMPLE**. DERIVED (§3.1). |
| **H** | **Under M\*-49, an honest drawn verifier recovers** its evidence, DA fetch, check, localization, attestation and filing costs. This holds given the assumptions in §3.3: the fee is escrowed by the user and paid on the attestation or on the claim's fate, the verifier holds the model, drawn demands are not burned, and costs are bounded. | **PROVEN** under stated assumptions (§3.3). Counterexamples to its scope: H-1 to H-6. |
| **T6** | Whether a drawn verifier really checks (the lazy verifier) can be made observable on chain. | **OPEN**. |
| **B12** | The seal deposit is derived. A withheld seal stalls 83 DAA. An abandoned one stalls 151 DAA (MEASURED) for 52 BILI. Parity gives `d* = c_ab·T_w/T_ab ≈ 28.6` BILI. A deposit above `d*` buys nothing. The deterred delay value is 0.345/`N_c` BILI per victim-DAA. `d*` exceeds the honest-race ceiling of 1 BILI, so the prices must be split (S3). | **DERIVED** (§4). |
| **F1–F5** | **Open versus closed for `f(S_m)`.** F1: split-proof and merge-proof together force `f` linear. F2: equal self-funding beats publishing for every `f`. F3: the publish premium *is* a concentration premium. F4: at `p` = 0, a closed model out-earns an open one by `1/(1 − λ)` for every `f`. F5: the budget and the per-bond cap saturate. | **PROVEN** (§5). Grid DERIVED from an ASSUMED market. |
| **F-bar** | **No curve makes publication "overwhelmingly better".** No `f` beats equal self-funding (F2). On the grid, no `f` meets the proposed bar against a closed model that can forge. The one curve that meets it against a non-forging closed model, `S²`, fails the concentration bar with a premium of 1.9–2.8. | **COUNTEREXAMPLE** (§5.4). |

### 0.2 Verdicts

**The P0 proof.**
* **Part 1, proved.** Self-dealing does not pay once E1 is fixed. Self-dealing never makes a deterred lie profitable.
* **Part 2, proved for M\*-49 only, under its assumptions.** The honest verifier recovers its costs. Today it is false (M0).
* **Its scope is the model the verifier can obtain.** For a closed model, no honest party can verify, and `p` = 0.

**GAP-B12.** The deposit is derived (§4.4). It needs S3, a separate source seal. S2 and S4 are recommended with it.

**`f(S_m)`.**
* No allocation curve achieves the publish goal.
* At `p` = 0, model coinbase is capital-proportional issuance for no work. That contradicts ADR-0176 §1 and PRINCIPLES §6.
* `palw_model_bond_allocation_v1` cannot be armed with any `f` until the user decides §6, D-F.

---

## 1. The model

### 1.1 Parties

* `P`: the producer bond of claim `c`.
* `C`: a coalition, meaning every bond of one economic owner, with `P ∈ C`. It may hold any number of verifier, demander, poster and
  block-producer bonds. Wallets and bonds are not parties: the chain cannot tell `C`'s bonds from anyone else's (ADR-0177 D6).
* `H`: honest verifiers. They follow the protocol and take part only if their expected revenue covers their expected cost (A-RAT).
* `U`: the job's poster.

### 1.2 Flows on the kernel route

The code is `misaka-palw-kernel/src/ledger.rs`. Values are INTERIM, from `palw_kernel_route_policy_v1` and `interim_v1`, with
`a` = 49%.

| Flow | Amount | Code |
|---|---|---|
| reservation | `K` = 1,000, held from admission until the liability horizon ends. Undisputed: window 50 + liability 200 = `H_L` = 250 DAA. Demanded: up to 280 | `admit`, OPV |
| pre-Final default | penalty `D` = 100. The demanders share `D·(1 − β_d)` equally, with `β_d = max(1 − a, default_burn) = 0.51`. The rest is burned | `tick_into` |
| conviction | slash `S = min(reserved, collateral)`. The bounty `min(⌊a·(S + taken_by_default)⌋, S)` goes to `bounty_holder`, the earliest proof seal of the exact bytes, else the filer. The rest is burned | `convict`, `bounty_holder` |
| reward | `R` = 5, from the poster's escrow, paid at the job's first Final. A post-Final conviction does not claw it back on this route | `pay_from_job_escrow` |
| fees | admission `f_adm` = 1; job `f_job` = 1; dismissed filing 0.1; demand bond 1, refunded on conviction, default or timeout, and burned at the horizon if served and nobody convicted | — |
| claim seal | `d` = 1. Returned at the reveal; forfeited at the TTL or, past the fence, on a re-seal (G14R `9429128f8`) | `SealClaim` |
| gain | `G = R + w + X` = 5 + 5 + 10 (RFC-0015 §8) | `max_gain_per_claim` |

The R-core route differs:
* Its R-1 rule is `⌊0.49·max(0, collected − X)⌋`.
* It deducts the already-extracted gain `X`.
* It pays no demanders' share.

E1 below concerns the kernel route.

### 1.3 Assumptions

| Id | Assumption |
|---|---|
| A-RAT | Every party maximizes expected money. An honest verifier participates iff revenue ≥ cost. Altruism is excluded. |
| A-COST | Verifier costs per check (`c_fetch`, `c_check`, `c_loc`, `c_attest`, `c_carrier`, `c_file`) are bounded inputs. MEAS measures them. |
| A-INC | An object broadcast in time is included within `B_carrier`, against a censorship that `censorship_cost` prices. |
| A-BEACON | The post-commit beacon is uniform except for `2^g` adversarial choices. F-ECON-2's withdrawal is closed by S1. |
| A-ACQ | **(ADR-0177.)** A verifier can check model `m` only if it holds `m`, authenticated against the registered root. For a closed model no honest party does. |
| A-FEE | The check fee is pre-funded in the job's escrow and paid on a verifiable event. |

No independence between verifiers is assumed. The only independence used is that of the beacon's per-slot draws.

---

## 2. Self-dealing at 49%

### 2.1 The loop, measured

`econ_bounty.rs` runs the OPV test world at `a` = 490‰: `K` = 1,000, `D` = 100, `β_d` = 510‰, admission 3, reward 7. The coalition
`{PRODUCER, SPAM1}` lies. Its Sybil seals the canonical convicting proof one block after the commit; a proof seal carries no deposit
and no fee. An honest outsider later finds the lie. MEASURED:

| Path | Coalition net | The conviction loop | Sybil paid | Honest verifier | Loss / collected |
|---|---|---|---|---|---|
| direct | −513 | −510 | 490 | 0 (its filing is a `Duplicate`) | 51.0% |
| after a self-inflicted pre-Final default | −464 | **−461** | **539** (bounty 490 + demanders' share 49) | 0 | **46.1%** |
| after Final | −506 | −510, plus the 7 reward kept | 490 | 0 | 51.0% |

On every path, burn + payouts = collected + fees.

Three honest claims checked clean earn the verifier 0 (`checking_honest_claims_pays_the_verifier_nothing`).

### 2.2 E1: a leak in the 49% bound, and its fix

The ledger caps each payout at the reporter share separately:
* the demanders' share of a default, at `1 − β_d ≤ a`;
* the bounty, with basis `S + taken_by_default`.

Each cap is respected, but one coalition can collect **both** on one claim:

```text
recoup_now = D·(1 − β_d) + min(⌊a·K⌋, K − D)        a_eff = recoup / K
interim, a = 0.49:   49 + 490 = 539 → net −461 (46.1%)                           [MEASURED]
sup over D ∈ [0, K]:  −(1 − a)·β_d·K = −(1 − a)²·K = −260.1 (26.0%) at D = 510  [DERIVED, 1-BILI grid]
```

ADR-0032 states that a self-reporter's net loss is "at least 51% of the slash". On this path that fails twice:
* the bounty alone is 490 of a 900 slash (54%), because its basis includes the penalty the default already took;
* the coalition also takes the demanders' 49 of the penalty.

Measured against everything the claim lost (1,000), the recoup is 53.9%.

**Fix (G14R's ledger; not built here).** One claim, one 49%: every payout to accusers and demanders out of one claim's collected money
(penalty + slash) is capped at `⌊a·collected⌋`. Concretely, in `convict`:

```text
reward = min(⌊a·(slashed + taken_by_default)⌋ − demanders_paid_on_this_claim, slashed)
```

With the fix, every path loses exactly 510 (51%), and no penalty `D` does better. DERIVED; the model checks the 1-BILI grid.

The OPV reservation relation must count the leak until the fix lands. The code's `required_reservation` is
`max(G + D, G/p)/(1 − a)`; with the leak counted it is `max(G + D, G/p + D·(1 − β_d))/(1 − a)`:

| p | code | with the leak counted |
|---|---|---|
| 0.05 | 784 | 880 |
| 0.005 | 7,843 | 7,939 |

DERIVED. At the interim `p` = 0.5 the `G + D` term binds and the two relations agree.

### 2.3 Theorem S: self-dealing does not pay (PROVEN)

**Setting.**
* Any coalition `C ∋ P`, any claim `c` of `P`, any strategy.
* "Collected" `Γ ≤ K` is what the claim's penalty and slash actually took.
* `a_eff = a` with E1. Otherwise `a_eff = recoup_now/K` (§2.2), the worst case over paths, which is at most
  `a + (D/K)·(1 − β_d)`.

**(S-1) Self-accusing an honest claim.** The exact court dismisses a proof against a correct claim (RFC-0014). The filer pays
`f_dis`. Nothing flows to `C`. So `Π_C ≤ −f_dis ≤ 0`.

**(S-2) Bounty farming: fraud plus self-accusation, no external gain.** Every flow into `C` caused by `c`'s conviction is one of:
* the bounty;
* the demanders' share;
* refunds and returned deposits, which net to 0;
* fees under M\*-49 (S-3).

With E1 the first two sum to at most `⌊a·Γ⌋`, so `Π_C ≤ −(1 − a)·Γ − f_adm < 0`. Without E1, `Π_C ≤ −(1 − a_eff)·K < 0`, because
`a_eff < 1` whenever `a < 1` and `β_d > 0`, both validated.

**(S-3) Check fees (M\*-49, §3.2).** A drawn slot is paid its fee whatever the claim's fate. So:
* `C`'s fee income is the same whether `c` is honest or a lie, and whether or not `C` self-accuses;
* if `C` posted the job, its fee receipts are at most its fee payment.

Fees therefore cancel in every comparison of strategies.

**(S-4) Against honesty.** Let:
* `p` = the probability that some party other than `C` would convict `c` in time, at the information `C` has when it commits (§2.6);
* `G_lie = C_saved + X + ΔR`, where `ΔR` is the reward an honest miner of the same bond could not have earned.

`C` self-convicts only when an outside conviction is coming. Self-convicting otherwise costs `(1 − a_eff)·K` for nothing (S-2).
Pre-empting changes `C`'s loss from `K` to `(1 − a_eff)·K` and no lower. So:

```text
E[Π(lie, best self-dealing)] − Π(honest) ≤ G_lie − p·(R_risk + (1 − a_eff)·K)
```

That is ADR-0176 D6, `p·(R_risk + L_collectible_net) > C_saved`, with `L_collectible_net = (1 − a_eff)·K` and the external gain
added. Self-dealing cannot make a lie profitable that D6 deters. It is fully priced by using `L_net` instead of the gross slash. □

**Per claim at the interim terms** (`C_saved = R` = 5). DERIVED; `p_min` is the minimum effective `p`.

| `X` | when the lie is caught | `p_min` with E1 | `p_min` today |
|---|---|---|---|
| 0 | post-Final (reward kept) | 0.98% | 1.08% |
| 0 | pre-Final (reward forfeited) | 0.97% | 1.07% |
| 10 | post-Final | 2.94% | 3.25% |
| 10 | pre-Final | 2.91% | 3.22% |

At `p` = 0, for a closed model (ADR-0177 D7), no reservation deters.

### 2.4 Theorem Y: the ceilings bound the gain; the yield must be below `p` (PROVEN)

ADR-0176 bounds a bond of capital `C` over window `W` by the same Q/B/R/F whether it computes or forges. So a forger's advantage over
honesty per window is at most `λ·R_W + X_W`, where:
* `R_W ≤ R_max(C, W)`;
* `λ ≤ 1` is the share of a reward that forging saves: compute, or reward an honest miner of the bond could not have earned.

Every claim must satisfy D6 with its own reservation: `K ≥ k·R`, with `k = (λ + x − p·r_risk)/(p·(1 − a_eff))`. The reservations must
also be fully backed: `Σ K·H_L ≤ C·T` (ADR-0176 D4). Together:

```text
R_W / C  ≤  W · p·(1 − a_eff) / (H_L · (λ + x − p·r_risk))
```

Two consequences:
* A cap `R_max(C, W)` above this ceiling cannot be reached by any deterred claim set.
* At `p` = 0 the ceiling is 0.

The binding guard is the per-claim D6. Y converts it into the largest safe reward yield on bonded capital.

| `p` | max reward / capital per DAA | per day (576 DAA, ASSUMED) |
|---|---|---|
| 0 | 0 | 0 |
| 0.001 | 2.0e-6 | 0.118% |
| 0.01 | 2.0e-5 | 1.18% |
| 0.05 | 1.0e-4 | 5.88% |

DERIVED, with `λ` = 1, `x` = 0, post-Final, and `H_L` = 250, the shortest hold, so that this is an upper bound. The E1 leak lowers
every ceiling by 9.6%.

ADR-0176 D5's illustration (350 BILI, 100 claims a week; not adopted): about 6.2 claims are held at once, so each claim can reserve
56.4 BILI. The deterrence-safe reward per claim is then 0.029 BILI at `p` = 0.001, or 1.44 BILI at `p` = 0.05. DERIVED.

### 2.5 Concurrent claims (PROVEN)

Take `N` claims with detection events `D_i` (any joint law), reservations `K_i` and bond capital `C`. Let `κ = ΣK_i/C`. The expected
collection is `E[min(Σ D_i·K_i, C)]`:

* **`κ ≤ 1`:** the min never binds, so the expectation is `Σ p_i·K_i` by linearity, **whatever the correlation**.
* **`κ > 1`:** comonotone detection (all or none) yields `p·C = Σ p·K_i / κ`. A forger induces it by putting every forged claim on one
  model or one fault pattern. A closed model is the extreme case: every `p_i` is 0 together.

| `C` | `κ` | independent | comonotone | cascade |
|---|---|---|---|---|
| 10,000 | 1 | 500 | 500 | 4,013 |
| 5,000 | 2 | 500 | 250 | 2,006 |

DERIVED, at `N` = 10, `p` = 0.05, `K` = 1,000.

"Cascade" means one detection convicts all of the bond's claims. It helps only if verifiers are paid to re-check, and it is not relied
on.

**Rule:** reservations must be fully backed, as ADR-0176 D4 already requires. Otherwise D6 is evaluated on `min(ΣK, C)`, not on `ΣK`.

### 2.6 External gains and adaptive timing

* **External gains.** Fork-choice gains and external-market gains are part of `X`. **Q/B/R/F does not bound them.** They need their
  own bound, and without one no reservation suffices. This is ADR-0176 D6's "fork-choice/外部市場利得は別途計上". The fork-choice part
  belongs to the fork-choice lane and is not analysed here.
* **Adaptive timing.** A producer that observes on chain that the honest verifiers are absent forges only then. So `p` in D6 is the
  minimum over the states the attacker can observe before it commits, not an average.

---

## 3. The verifier's incentive (P0, part 2)

### 3.1 What fails

**M0 (MEASURED, §2.1).**
* Under first-sealer-takes, the honest verifier earns 0 on every lie whose producer pre-seals, and 0 on every honest claim.
* So a rational verifier does not watch: `θ = 0`, then `p = 0`, and a lie nets `+G`.
* Conditions 5 and 6 of PRINCIPLES §6 cannot be separated.

**T0 (PROVEN by the predecessor).** A verifier paid only by bounties breaks even only if the fraud rate is at least `c/(a·K)`:
* 0.20% of checked claims for an uncontested 490;
* 2.0% when shared by 10.

Deterrence drives that rate to 0. So condition 6 needs **income per check**.

**Remedies A and B (COUNTEREXAMPLES, DERIVED).**
* **A:** the producer grinds its claim id until the fault lies in its own Sybil's sample. That takes 16.1 trials on average at
  `q = 1/16`, while the honest verifier's coverage stays at 0.065.
* **B:** at a 490 bounty and one honest covering verifier, the honest share is 245, 44 and 4 at 1, 10 and 100 Sybils.

Both pay on lies only (T0).

### 3.2 M\*-49: the proposed mechanism (not built)

1. **Watcher pool per model.**
   * Bonds lock stake for model `m`'s pool. This is not miner capital and never counts in `S_m` (ADR-0177 D3).
   * Each pool is frozen per epoch. Exit takes at least the window plus the liability horizon.
   * The same stake is never counted in two pools.
2. **A post-commit draw.**
   * A v3 `ClaimVerification` beacon whose seal window opens after `c` commits decides whether `c` is checked (probability `q`).
   * It then draws `m` slots by stake, with replacement.
   * T2 (PROVEN by the predecessor): the draw is Sybil-neutral. The result depends on stake, not on the bond count.
3. **A check fee, paid by the user.**
   * The poster escrows `m·F` beside `R` at `PostJob` (GAP-5's mechanism). An undrawn claim refunds it.
   * Each drawn slot is paid `F` on **its attestation, or on the claim's conviction or default before the attestation deadline**
     ("pay on fate", fixing H-4).
   * A drawn slot's demands are exempt from the served-demand burn (fixing H-5).
4. **The bounty stays the 49%, with one change of recipient** (inside ADR-0032; rate and total unchanged):
   * of `⌊a·collected⌋`, the drawn slots that sealed a convicting proof share up to `B_cap` equally;
   * the remainder goes to the earliest sealer, as today;
   * the rest of the collected amount is burned.

   With E1 the coalition still recoups at most `a·collected`. So S holds unchanged.
5. **`B_cap = m·G`**, the bribery floor (T5). At the interim terms that is 40 of the 490. DERIVED.

**Budgets named** (ADR-0176 requires every new payment to name its source).
* `F` comes from the poster's escrow. It is a user transfer, not issuance, not `R_PALW`, and not any producer's `R_max`.
* The bounty comes from collected slash (ADR-0032).
* No coinbase is touched.
* `C`'s watcher slots earn `F` on `C`'s own claims at the same rate as on anyone's (T2), so the fee is not a renamed producer reward
  (RFC-0015 §8.3.2).

### 3.3 Theorem H: an honest drawn verifier recovers its costs (PROVEN under A-FEE, A-INC, A-ACQ, A-COST)

**The fee floor.**

```text
F_min = c_fetch + c_check + c_loc + c_attest + c_carrier + E[burned demand bond] + c_acquire,m / n_m
```

Here `c_acquire,m / n_m` amortizes one acquisition of model `m` over the verifier's expected draws on `m`. The fee therefore depends on
the model's volume: a niche model's users pay more per check.

**Statement.** If `F ≥ F_min` and `B_cap/m ≥ c_file`, then an honest drawn slot nets at least 0 on every claim, honest or lying,
whatever `C` does:

```text
net ≥ F − F_min + 1[it sealed a convicting proof]·(⌊min(B_cap, a·Γ) / n_s⌋ − c_file) ≥ 0,   n_s ≤ m
```

**Proof.**
1. `F` is escrowed (A-FEE). It is paid on the attestation, or on the claim's fate if that comes first. So no ordering by `C` can leave
   a slot that started work unpaid, and the fee covers everything up to localization.
2. On a fault:
   * the drawn sealers take the first `min(B_cap, a·Γ)`, ahead of `C`'s earliest-sealer Sybil;
   * there are at most `m` drawn sealers;
   * so each gets at least `B_cap/m ≥ c_file`.
3. If `C` convicts first through a non-drawn bond, the slot stops having spent no more than the fee covers.
4. A drawn slot's demands are never burned (A-DEM, part of A-FEE).
5. A-INC bounds inclusion. A-COST bounds the court's cost by the class's prosecution bounds. A-ACQ makes the check possible. □

**Numbers** (ASSUMED costs until MEAS):

| Item | Value |
|---|---|
| fetch | 0.9 |
| check | 4, about one re-execution of a reward-5 claim |
| localize | 0.05 |
| attest | 0.02 |
| carrier | 0.02 |
| demand-bond burn | 0.1 |
| acquisition | 50, over 100 checks |
| **`F_min`** | **5.59**, DERIVED |

`p` needed (C_saved = R, `X` = 10, E1): 2.94%. The verifiers' probability of catching the fault is
`P_dc = q·(1 − (1 − (1 − σ)·θ_h)^m)`, where:
* `σ` = the coalition's share of model `m`'s pool;
* `θ_h` = the honest stake's diligence, an input (T6).

At `m` = 2, DERIVED:

| `σ` | `θ_h` | `q` needed | the user pays per claim (`q·m·F_min`) |
|---|---|---|---|
| 0 | 1 | 2.94% | 0.33 |
| 0.25 | 0.5 | 4.83% | 0.54 |
| 0.5 | 0.5 | 6.72% | 0.75 |
| 0.9 | 0.5 | 30.2% | 3.37 |
| 1 (closed) | any | **infeasible** | — |

### 3.4 Where H stops (counterexamples to its scope)

* **H-1. A closed model.** No honest party satisfies A-ACQ. H holds vacuously, for nobody, and `p` = 0. No verifier-incentive design
  can create `p > 0` for a model nobody else holds.
* **H-2. The lazy verifier (T6, OPEN).** The chain cannot tell an attestation backed by a real check from one copied from the
  claim's public roots, so `θ_h` is unobservable. Forced-error
  audits (Truebit-style) need a planter that holds the model, and for a closed model only `C` does.
* **H-3. Pool capture.** The fee that makes H true also pays `C`'s pool stake. So holding `σ` of a model's pool is self-financing for
  `C`, as it is for honest stake. `P_dc` is bounded by `σ`, which wallet counts cannot reveal. Security is relative to `C`'s share of
  that model's verifier capital, as in proof of stake.
* **H-4. Fee only on attestation.** `C` self-convicts after the drawn verifiers have fetched, and they are not paid. Fixed by pay on
  fate.
* **H-5. The served-demand burn** (G14R's demand-bond fate). A drawn verifier that must demand from an honest producer loses its bond.
  Fixed by the A-DEM exemption, or by `F` covering it.
* **H-6. Adaptive timing** (§2.6). `P_dc` must be taken at the attacker's information.

The DA default's demanders' share is split **equally among demanders** (`tick_into`). That is remedy B's dilution again. Under M\*-49
it is not needed for cost recovery, because `F` covers the demand.

### 3.5 What M\*-49 needs (allocations belong to the Lead; nothing is built)

| Item | Owner |
|---|---|
| E1, one claim one 49%, a ~3-line change in `convict` | G14R |
| the per-model watcher-pool object and table | allocation |
| the v3 post-commit draw | OPVB |
| `m·F` in the escrow, pay on fate, the attestation object, and the fee settlement kinds | G14R |
| the bounty recipient rule: drawn sealers first, inside the 49% | G14R |
| the A-DEM exemption | G14R |
| the window must hold `T_beacon + T_fetch + T_check + T_loc + T_file + margin`: about 110 DAA, against 50 | OPVB, MEAS |
| T6 | design, OPEN |
| a dormant fence | Lead |

---

## 4. GAP-B12: the seal deposit, derived (not chosen)

### 4.1 The stall per veto

The beacon has the interim sealed shape: anchor 2, `W` = 40, `R + 1` = 3 attempts.

| Route | Stall, commitment to the next attempt | Label |
|---|---|---|
| withhold a mixed seal | `T_w = anchor + 2W + 1` = **83** | MEASURED (`econ_seal_veto`, F-ECON-3: vetoed at the commitment + 82) |
| abandon: reveal on the last DAA, own Sybil demands on the last window DAA, the producer withholds, default | `T_ab = anchor + 2W + window + court − 1` = **151** | **MEASURED** (`econ_seal_veto`) |
| self-convict at the end of the proof grace | `T_cv = anchor + 2W + window + court + grace − 1` = **161** | DERIVED; the hard deadline caps it at 162 |

**F-ECON-3 (MEASURED).** One withheld seal vetoes every concurrent attempt whose window contains it: three attempts in the test.

### 4.2 The price per veto

After G14R's S1 (`9429128f8`), a re-seal forfeits, so every withheld veto costs one deposit.

| Route | Price | At the interim terms, 49% | Per victim-DAA (`N_c` = 1) |
|---|---|---|---|
| withhold | `d` + carrier | 1.02 | 0.0123 |
| abandon | `f_adm + D·β_d` + 3 carriers | 52.06 (MEASURED in the test world: 3 + 51 = 54) | 0.3448 |
| self-convict | `f_adm + (1 − a)·K` + carriers | 511.06 | 3.17 |

Carriers are ASSUMED at 0.02 each. The abandonment's reservation stays locked for the liability horizon; its capital cost is not
counted.

### 4.3 What `R + 1` abandonments buy, the attack cost, and the damage

| Quantity | Value | Label |
|---|---|---|
| stall to exhaustion, `(R + 1)·T_ab` | 453 DAA, about 18.9 h | DERIVED |
| stall to exhaustion, `(R + 1)·T_cv` | 483 DAA | DERIVED |
| price of exhaustion, `(R + 1)·c_ab` | 156 BILI | DERIVED |

**Damage** is `N_c·(ν·(R + 1)·T_ab + C_reg)`. Here `ν` is the value of one DAA of delay to a victim. `C_reg` is a dead registration: its re-registration, the binding's 100-BILI reservation held 200 DAA, conformance fees and time.

| `ν` | `N_c` | `C_reg` | damage / cost (griefing factor) |
|---|---|---|---|
| 0.1 | 1 | 200 | 245 / 156 = 1.6× |
| 1 | 1 | 200 | 653 / 156 = 4.2× |
| 1 | 4 | 200 | 2,612 / 156 = 16.7× |
| 10 | 16 | 2,000 | 104,480 / 156 = 669× |

The damage inputs are ASSUMED.

### 4.4 The derivation

1. **Parity.** Withholding must not buy the same stall more cheaply than abandoning:

   ```text
   d* = c_ab · T_w / T_ab − carrier
   ```

   That is **28.6 BILI** at the interim terms and 49% (DERIVED). **A deposit above `d*` buys nothing**: the attacker abandons
   instead. A deposit below `d*` makes withholding the cheap lever, 28× cheaper per DAA at today's `d` = 1.
2. **What it deters.**

   ```text
   γ* = c_ab / (N_c · T_ab) = 0.345 / N_c BILI per victim-DAA ≈ 199 / N_c BILI per day
   ```

   A class whose delay is worth more than `γ*` to an attacker is not protected by any deposit. Protecting it means raising `c_ab`
   (`D·β_d`, `f_adm`) or rate-limiting (S5), not raising `d`.
3. **Exhaustion.** While vetoes count against `R`, 156 BILI buys a dead registration (`C_reg`). **S2**, under which vetoes do not
   count, removes `C_reg` from the damage. The attacker can still stall, at `c_ab` per `T_ab`, but can no longer kill the
   registration.
4. **Concurrency.** Both the withhold route and the abandon route veto all `N_c` concurrent attempts.
   * Charging `d` per vetoed attempt (the predecessor's S4 option) prices only the withhold route. It pushes the attacker to
     abandonment, which then costs `c_ab/N_c` per victim.
   * **S4 must therefore cap concurrency**: at most `N_max` sampled attempts per seal window.
5. **Feasibility.** Honest job races cap a deposit that every racer posts at `ρ·R/(n − 1)` = 1 BILI (ρ = 20%, `n` = 2). `d*` = 28.6
   exceeds that. So **S3 is required**:
   * the job-race seal stays at `d` and is not mixed;
   * an opt-in beacon-source seal carries `d_src = d*` and is the only kind that can veto.

**Freezing all sampled onboarding network-wide** takes one withheld seal per `W`:

| `d` | cost per day | Label |
|---|---|---|
| 1 (today) | 14.4 BILI | DERIVED |
| `d*` | 412 BILI | DERIVED |

S4 and S5 bound this further.

---

## 5. ADR-0177 D6: open versus closed economics for `f(S_m)`

### 5.1 Setting

```text
R_m = R_PALW · f(S_m) / Σ_j f(S_j),   with f ≥ 0, monotone, f(0) = 0
g(S) = f(S)/S                          (the per-unit weight)
```

Inside a model, the allocation goes pro rata to locked capital, subject to each bond's Q/B/R/F cap. The incumbent holds `S_0`; external
capital is `E = e·S_0`. The chain sees only `S_m` (D6).

### 5.2 Theorems (PROVEN)

**F1. Split-proof and merge-proof force linear.**
* Splitting a model's capital across near-duplicate registrations never pays iff `f` is superadditive.
* Concentrating never pays iff `f` is subadditive.
* Both hold iff `f` is additive. For a monotone `f` that means `f(S) = c·S` (Cauchy).

A near-duplicate registration is always available (ADR-0175: any change is a new registration). So split-proofness is necessary.
Every non-linear, split-proof `f` therefore pays concentration.

**F2. Equal self-funding beats publishing, for every `f`.** Suppose the incumbent can borrow at `r_b ≤ r_ext`, the rate at which
external capital would join. Then:

```text
π(self-fund E) − π(open E) = E·[ρ(S)·(1 − λ) − r_b] + c_dist ≥ 0
```

Here `ρ(S) = R_m(S)/S` and `S = S_0 + E`. External capital joins only if `ρ(S)·(1 − λ) ≥ r_ext`. The difference is larger by `λ·R_m`
when the closed incumbent can forge (`p` = 0).

So a publish advantage over equal self-funding is a capital-market friction (`r_b > r_ext`). No curve supplies it.

**F3. The publish premium is a concentration premium.** Take a capital-constrained incumbent whose closed model computes, while the
model's share of the budget is small (`f(S) ≪ Σ_j f`). Publishing multiplies its per-unit return by `g(S_0 + E)/g(S_0)`. Any single owner of `S_0 + E` gets exactly that premium over a model of `S_0`.
So a bar of "publishing ≥ Φ× closed" forces a concentration premium of at least Φ.

**F4. At `p` = 0, closed out-earns open by `1/(1 − λ)`, for every `f`.**
* A closed coalition need not compute.
* At equal `S_m`, its per-unit profit is `ρ`; an honest open model's is `ρ·(1 − λ)`.
* The ratio is `1/(1 − λ)`: 2× at λ = 0.5.
* `f` cannot see openness, so no `f` removes it.

**This is stake-only mining, which ADR-0176 §1 forbids.**

**F5. Saturation.**
* The budget is fixed (D4). Once model `m` holds most of `R_PALW`, external capital only dilutes the incumbent: at `f = S²` and
  `S_0` = 4, open/closed is 0.83.
* The per-bond cap clips the incumbent's payout. At an ASSUMED cap of 1.5× the average yield, open/closed is 0.95 for linear `f` and
  1.50 for `S²`.

### 5.3 The comparisons ADR-0177 §3 requires

Market (ASSUMED):
* 20 other models of capital 1;
* `R_PALW` = 1 per epoch;
* λ = 0.5;
* `r` = 1% per epoch;
* `C_reg` = 0.005 per epoch;
* no distribution cost.

All values are DERIVED from these inputs.

| Scenario | Result |
|---|---|
| closed self-capital | the baseline. At `p` = 0 it forges and earns `ρ` per unit (F4) |
| equal self-funding | beats open for every curve and every grid point: open/self-fund 0.03–0.23 (F2) |
| public external capital, linear, `S_0` = 1, `e` = 1 | open/closed 0.34 when the closed model forges; 0.92 when it computes |
| an attacker with all the capital | equals equal self-funding: it gets the open allocation while staying closed (F2 + F4) |
| Sybil and model split (`S_0` = 4 into `k` = 2, 4, 10) | linear loses only `C_reg`. `√S` gains +0.028 to +0.104 of the whole budget; `min(S, 2)` gains +0.071. Superlinear loses |
| borrowing and exit | with a snapshot and no lock, borrowing is free: linear open/self-fund 0.16 at `r_b` = 0. The allocation needs time-integrated capital locked for at least the epoch plus `W` |
| concentration and competing models | when external capital equal to all existing capital arrives, the top model takes 0.049 of the budget under linear `f`; 0.83 under `S^1.5`; 0.96 under `S²`; 0.998 under `S³`; 0.68 under the threshold curve |
| incumbent cap saturation | F5 |
| the closed model's lower `p` | 2× for every `f` (F4) |

### 5.4 The proposed bar (POLICY; to fix before any curve is chosen) and the verdict

**PROPOSED bar for "overwhelmingly better to publish":**

| Bar | Condition |
|---|---|
| **O** | open ≥ **2×** closed self-capital, for incumbents at or below the median model (`S_0` ∈ {0.25, 1}) and external capital 1×, 3× and 9× their own |
| **S** | open ≥ equal self-funding at the market rate |
| **A** | concentration premium ≤ 1: a single owner of 10%, 25% or 50% of the capital gets no more than its share |
| **M** | no gain from a model split |
| market range | λ ∈ [0.3, 0.7]; `r` ∈ [0.5%, 2%] per epoch |

The user fixes Φ\* = 2, the ranges and the market assumptions.

**Candidate curves on the grid** (DERIVED):

| `f` | O, closed forges | O, closed computes | S | A (premium at x = 0.1 / 0.25 / 0.5) | M |
|---|---|---|---|---|---|
| `S` (linear) | no (0.18–0.37) | no (0.48–0.98) | no | **yes** (1.00) | **yes** |
| `√S` | no | no | no | yes (0.69 / 0.46 / 0.37) | **no** |
| `min(S, 2)` | no | no | no | yes | **no** |
| `S^1.5` | no | no | no | no (1.42 / 1.85 / 1.63) | yes |
| `S²` | no | **yes** (≥ 2.29) | no | **no (1.98 / 2.76 / 1.90)** | yes |
| `S³` | no | no (small incumbents do not attract capital) | no | no (3.54 / 3.75 / 2.00) | yes |
| `S·min(1, S/2)` | no | no (1.09 at `S_0` = 1, `e` = 9) | no | no (1.98 / 1.88 / 1.60) | yes |

**Verdict.**
* **No curve achieves bar S.** That is a theorem (F2).
* **On the grid, no curve achieves bar O against a closed model that can forge.** By F4, any curve that did would need a per-unit
  premium of at least Φ\*/(1 − λ) = 4. By F3, that means a concentration premium of at least 4.
* The one curve that meets O against a computing closed model, `S²`, fails A by the factor F3 predicts.
* Linear is the only curve that is Sybil-neutral and concentration-neutral. It gives no publish premium: publishing costs the
  incumbent its forging option plus a share of its allocation.
* The publish goal is **not** achievable by `f` under ADR-0177 D6. Per ADR-0177 §1 and D6, "the problem" stays **unsolved**.

**The decisive consequence.**
* At `p` = 0, model coinbase pays a closed coalition `g`-weighted, capital-proportional issuance for no work.
* No chain-observable condition can distinguish that coalition from an open model:
  * capital independence is unobservable (D6);
  * the coalition can be its own verifier pool (H-3);
  * forced-error audits need a holder of the model (H-2).
* This contradicts ADR-0176 §1 ("stakeだけで計算を省略して採掘できる規則にしない") and PRINCIPLES §6, conditions 2 and 6.
* The same holds for Final weight at `p` = 0. It becomes capital-proportional weight without work. Its consensus consequences are
  outside this note.
* User-paid escrow rewards are different. A closed coalition that self-posts jobs pays itself, which is zero-sum, so self-dealing
  gains nothing there. An outside user who buys from a closed model bears its own `p` = 0 risk.

---

## 6. Status, proofs, POLICY

### 6.1 Implemented, verified, armable

* **Implemented:** nothing in consensus. The PoCs and the model.
* **Verified:** the PoCs `econ_bounty.rs` and `econ_seal_veto.rs`, see §7; the model, with every check holding.
* **Armable:** nothing.

Every reward, consensus-weight and Panel=0 fence stays refused:
* condition 6 is unmet today (M0);
* M\*-49 is not built;
* T6 is open;
* E1 is unfixed;
* BUDGET's `palw_bond_budget_v1` does not exist;
* `palw_model_bond_allocation_v1` has no admissible `f` (§5.4).

### 6.2 Proved versus open

| Proved | Open |
|---|---|
| S (with E1) | T6, the lazy verifier |
| Y | `p` for any closed model |
| C | `θ_h` and `σ` per model |
| T0 | MEAS's costs per class |
| H, under its assumptions | the OPV window holding `T_beacon` |
| T2 | a bound on external and fork-choice gains `X` |
| F1–F5 | — |
| the B12 derivation (its inputs are labelled) | — |

### 6.3 POLICY decisions for the user, with ECON's recommendation

| Id | Decision | Recommendation |
|---|---|---|
| D-E1 | Fix E1 (one claim, one 49%) or keep the leak | **Fix it.** It restores ADR-0032's 51% on every path at about 3 lines; until then, add `D·(1 − β_d)` to the reservation relation |
| D-M\* | Adopt M\*-49 as the direction for verifier pay: per-check fees from the user's escrow, and drawn sealers first inside the 49% | **Adopt.** It is the only design here that satisfies both halves of P0. Keep T6 open, and keep every reward fence refused until T6 is closed |
| D-q | `q`, `m`, `F`, `B_cap = m·G` per class | Derive them from `p_min` (§3.3) and MEAS's costs. None is set now |
| D-B12 | The seal deposit | `d_src = c_ab·T_w/T_ab` (28.6 at the interim terms and 49%) on an opt-in source seal (S3). Also S2 (vetoes do not count) and S4 as a concurrency cap `N_max`. `γ*` is set through `D`, not `d` |
| D-F | Model coinbase at `p` = 0 | **(a)** no model coinbase leg in the release: rewards are user-paid escrow only. **(b)** if the user wants the leg: linear `f`, an explicit cap on its share of `R_PALW`, and an amendment to ADR-0176 §1 accepting a capital-proportional residual for closed models. ECON recommends **(a)** |
| D-bar | The numeric bar for "overwhelmingly better" | Fix Φ\* (2 proposed), the `S_0` and `e` ranges, and λ and `r` before any curve is evaluated. Record that F2 and F4 make the bar unattainable by `f` alone |
| D-snap | The allocation snapshot | Time-integrated capital, locked for at least the epoch plus `W`. Never a point snapshot |

---

## 7. Reproduce

```text
python3 -I scripts/misaka-palw-econ-verifier-model.py
CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p misaka-palw-kernel --test econ_bounty --test econ_seal_veto -- --nocapture
```
