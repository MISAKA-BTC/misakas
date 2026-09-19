# Audit — is PALW a practical reward foundation for LLM mining? (2026-09-19)

Four agents: a protocol/consensus auditor, an ML-systems economist, an adversarial economist, and a
red-team verifier whose job was to refute the other three. Every number below is reproduced by a
probe that links `kaspa-consensus-core` and calls the repo's own functions; sources and outputs are
under `scratchpad/reward-audit/`. The red-team refuted two findings that the first three filed as
confirmed, and corrected the lead's own ground truth. Those corrections are kept in the report
rather than quietly dropped, because which claims failed is part of the result.

---

## Executive conclusion

**CONDITIONAL.** PALW can be a practical reward foundation for LLM mining. It is not one today, and
the gap is not a tuning problem.

**Is it practical now?** No. Nine of eleven reward invariants are violated; one holds; one is
untested. The quantity that decides fork choice — `claim.pwu` — is a multiple of a number the class
registrant declares and signs, and that is live on testnet-11 right now.

**Is there a release blocker?** Yes, and it has a deadline. The build rolled on 2026-09-19 schedules
`palw_model_registry` at **DAA 7,101** (`params.rs:20454`, verified at runtime off
`palw_rc_shipped_params()`). At the time of writing the chain is at **6,767** and moving; that is
~334 DAA away. 7,101 opens permissionless class registration, which is the precondition of every
blocking finding below. **Existing classes are unaffected** — `check_class_admits_claim` refuses a
rowless class only when `work_target_active`, dormant on every preset — so the release is safe for
what is running and unsafe for what it invites.

**What assumption is dangerous when a future model is added?** That a class's declared numbers
describe its work. They do not describe it now, and no future model makes them start.

---

## Confirmed findings

Ordered by what they cost, not by when they were found.

### F1 — The fork-choice weight is a number the registrant chooses (CRITICAL, live)

**Precondition.** One post-genesis class registration. Live on testnet-11 today; `claim.pwu` needs
no fence.

**Mechanism.** `claim.pwu = expected_attempts(class_target) × pwu_per_inference`, and
`pwu_per_inference` is the STEP-LEAF count of a canonical job the registrant declares
(`palw_class_admission_v2.rs:2152`). Admission binds the *declaration* to the graph — declared ==
counted — and never binds the graph to arithmetic. Two levers move leaves per unit of real work:

* the **decode declaration**, which is the larger one now that `palw_prefill_draw` has been armed
  since DAA 4,000. Past that fence an attempt executes `exact_decode_tokens = 1`
  (`palw_attempt_v2.rs:227`) while the price still counts every declared decode call;
* **`tile_len`**, free in `[4, 65_536]` and never read by the ADR-0131 cost model
  (`palw_economic_compute_v1.rs:427`).

**Reproduction** (`agent4-redteam/src/bin/table.rs`, `counterexample-table.txt`, family (b)/(d)) —
one graph, one artifact, one kernel set, therefore one certification:

| declared canonical (P,D) | `pwu_per_inference` | MAC-eq actually executed | fork-choice weight index | pay index |
|---|---|---|---|---|
| (63, 2) — shipped | 6,630,544 | 83,102,171,136 | 100 | 100 |
| (1, 256) | 51,535,376 | 83,102,171,136 | **777** | 100 |
| (1, 432) — max admissible | 52,714,368 | 1,546,037,392 | **2,457,197** | 107 |

**7.8× the weight for identical arithmetic and identical pay. 24,572× the weight per unit of
arithmetic at the admissible extreme.** All three rows pass every check.

**Why exposure does not brake it.** `slash_value_per_pwu = 5` sompi, so the heaviest admissible
class reserves 2.64 MSK against a 2,756 MSK block escrow.

**Impact.** Consensus, not just economic: safe weight is fork choice's second key
(`palw_fork_choice.rs:72-79`). A registrant can buy chain weight with a declaration.

**Mitigation.** Make `claim.pwu` a function of CCU behind a fence — `economic_ccu_per_claim` is
already derived by the graph walk the chain performs — or state plainly that weight is a tiebreak
and not a work measure. `palw_pwu.rs:53-62` already says "a pwu magnitude must never be read as a
cross-class price"; the code does not obey its own comment.

### F2 — The free-prompt lane's work is a self-report (CRITICAL for the lane ADR-0144 makes primary)

**Mechanism.** `work_leaves` is the executor's own field and the acceptance walk never recomputes
it. The repo's own test proves this: `palw_fp_objects_v3.rs:885` multiplies `work_leaves` by ten and
asserts the walk still passes. The only party that checks it is a 3-of-5 panel drawn exclusively
from bonds that declared capability for the class — which for a permissionlessly registered class is
normally the registrant's own (F3).

**Reproduction** (`close-output.txt` C4): the same answer with a padded prefix moves
`work_leaves` 965,104 → 51,645,040, quanta 1 → 62, `claim.pwu` 828,818 → 51,386,716 — **62×** — while
a producer holding the prefix's KV cache spends 3.09 G MAC-eq in **every** row. Capped at 8× the
canonical pwu, so it lets a small job reach the cap rather than pumping without bound.

**The asymmetry that survives the cap.** Each of the five panel seats replays in full and may not
cache, so the network spends ~5 × 669 G verifying a claim whose producer spent 3.09 G.

**Impact.** ADR-0144 makes this lane the product. Until `work_leaves` is recomputed on the
acceptance path, the lane's weight is a number the miner types.

**Mitigation.** Recompute it. The chain holds the class's shape profile and
`step_leaf_count_capped_v1`; the seat already runs exactly that derivation.

### F3 — A class can be judged entirely by its own registrant (HIGH)

`palw_panel_eligible_bonds_v2` draws only from bonds that declared capability for the class
(`palw_panel_v2.rs:548-556`), and capability is registrant-controlled. It excludes the executor's own
bond, operator id and pubkey — so the price of self-certification is six distinct operator keys
rather than five, not a different shape. The free-prompt lane is also certifiable permissionlessly
by kernel subset (`palw_state_v2.rs:13946-13960`).

**Mitigation.** Require a class's quorum to include at least one seat the registrant does not
control — e.g. drawn from bonds capable of the BASE class, which every node runs, with a veto.
`spare_seats` already exists in `PalwRegistryGlobalsV1`.

### F4 — A class can be admitted at a ladder its own legal jobs exceed (HIGH, needs one registration)

`worst_case_step_leaf_count_capped_v1` enumerates one decode call while a real job pays a logits
term at every one. Measured under on all four shipped classes: BASE-0 +2.9 %, Qwen3.6 +6.9 %,
Qwen2.5-A16@512 **+18.4 %**, Qwen3.8-27B +4.1 %. A class admitted with a declared worst case under
the 2^26 ladder can then run legal jobs of 70–78 M leaves above it — and the court's refutation
walker is capped at the ladder, so **no dispute over such a claim can be opened**: accepted and
unprosecutable. Not live on the shipped @512 row; one registration away.

### F5 — Pay per unit of compute falls like 1/model width (HIGH, live)

Leaves count activations; cost counts weights. Leaves per G-MAC-eq across the live classes:
355,901 / 148,731 / 79,788 / 52,052 — a **6.8× spread, monotone in model width**. The largest model
is paid least per unit of arithmetic it runs. This reproduces ADR-0131's own pinned +86.4 % between
the two live model classes exactly.

**This is the finding that matters for ADR-0144's P5**: the basis in force pays a strictly better
model strictly less, and both proposed replacements price efficiency at exactly zero.

---

## Refuted, and why it is in the report

| filed as | verdict | why |
|---|---|---|
| One registration cuts every class's pay to 13.4 %, never minted (CRITICAL/CONFIRMED) | **REFUTED** | Filed from the base preset constants. The builder overrides them at `params.rs:12839`; ADR-0132's snapshot arms at the same height and takes precedence (`palw_state_v2.rs:10019`), so the victim rows are never computed on any block past the fence. |
| MAC basis is 13.7× apart on fleet replay times (HIGH) | **REFUTED** | The numbers are `#[cfg(test)] mod tests` fixtures in `palw_verification_profile_v1.rs`, not measurements, and the production path overrides a measured p99 with a reference-rate estimate. The qualitative point — MAC-eq carries no memory-traffic term — survives; the quantity does not. |
| A padding class reaches 1.00 MAC-eq per leaf (22,077×) | **WEAKENED** | Not rebuilt, so neither confirmed nor refuted; two supports fail. F1's 24,572× is the defensible version and comes from the shipped graph. |

**A process finding.** Three of four agents read fences off the base preset constants
(`params.rs:8250-8700`) rather than `palw_rc_shipped_params()`, and all three therefore worked from a
flag day three moves out of date. That single mistake is what turned a refuted claim into a
CONFIRMED/CRITICAL one. **Read fences at runtime.**

---

## Reward invariants

| | invariant | status |
|---|---|---|
| i | Surface prompt inflation must not raise reward | **VIOLATED** (F2) |
| ii | No reward for compute not performed | **VIOLATED** (F1 decode lever, F2) |
| iii | More non-useful work must not mean more profit | **VIOLATED** |
| iv | Efficiency allowed, shortcut arbitrage bounded | **VIOLATED** (F5) |
| v | No model self-report in reward | **VIOLATED** (F2) |
| vi | Protocol difficulty not set from miner-controlled metadata | **VIOLATED** (F1) |
| vii | A class's declared worst case must bound every legal job | **VIOLATED** (F4) |
| viii | Collateral reserved must scale with the weight bought | **VIOLATED** (F1) |
| ix | The quantity deciding fork choice must be one nobody can declare | **VIOLATED** (F1) |
| x | A claim's price must be recomputable without trusting any executor | **HELD** |
| xi | A class's economic profile must be attested, not estimated | **UNTESTED** |

Invariant (x) holding is the reason this is CONDITIONAL and not NO: the machinery to recompute is
there and correct; it is simply not what the reward reads.

---

## Model-admission policy

Registration and reward eligibility become separate states. A new class passes:

1. **Registered** — on chain, rent paid, no weight, no pay.
2. **Calibration** — the chain derives the class's canonical work vector from its graph. Nothing the
   registrant declares is read. A declaration that disagrees with the derivation is the registration
   refused, not the derivation adjusted.
3. **Shadow** — claims accepted, verified and recorded; weight and pay are zero. Its ratio of
   declared-to-derived work is published beside every other class's.
4. **Capped** — weight and pay begin at a ceiling that no single class may exceed, so an accounting
   anomaly cannot become a majority before anyone reads the chart.
5. **Admitted** — the cap lifts after N epochs without an anomaly.

At every stage the quorum must include a seat the registrant does not control (F3), and the class's
declared worst case must bound its deepest legal job (F4).

**Efficiency is not an anomaly.** A model that does the same canonical work in a tenth of the time
keeps the profit. The anomaly to catch is *canonical work small, reward large* — which is a ratio the
chain can compute for every class without judging any model.

---

## Attack matrix

| attack | prereq | miner cost | reward | profitable | detectable | impact |
|---|---|---|---|---|---|---|
| Decode declaration (F1) | 1 registration | unchanged | **7.8× weight**, same pay | yes | yes, from chain | fork choice |
| Extreme decode + tile (F1) | 1 registration | 1/54 arithmetic | **24,572× weight/MAC** | yes | yes | fork choice |
| `work_leaves` inflation (F2) | 1 registration + own panel | ~0 | **62× pwu** | yes | only by recompute | pay + weight |
| Prefix padding with KV cache (F2) | free-prompt lane | +3 % | up to 8× cap | yes | no | pay |
| Self-certifying panel (F3) | 6 operator keys | 6 bonds | approves own claims | enabler | partly | verification |
| Over-ladder admission (F4) | 1 registration at n_ctx 576+ | honest | unprosecutable claims | defensive | yes, at admission | court |
| Prompt padding, mining lane | — | — | — | **no** | — | anchor-derived prompt closes it |
| Benchmark memorisation | — | — | — | **no** | — | no benchmark exists |
| Ticket grinding | — | — | — | **no** | — | beacon post-dates the claim |

The last three are worth stating: three classic attacks are structurally closed, and the closures are
real. The damage is concentrated in class registration and in the one lane whose work is self-reported.

---

## Recommended architecture

**Keep the shape, replace the quantity.** The commitment scheme, the panel, the court, the
beacon ordering and the execute-now-settle-later flow are sound and are what make invariant (x)
hold. What must go is *paying by a number the registrant writes*.

1. **Weight and pay read a derived quantity, never a declared one.** The derivation exists.
2. **Canonical work is a vector, not a scalar** (ADR-0144 §5), because one number cannot price dense
   GEMM, routed experts, attention and KV traffic at once. Coefficients are protocol-set, versioned,
   and calibrated by an experiment nobody has run yet — run it before arming anything.
3. **Recompute `work_leaves` on the acceptance path**, or the primary lane's work is a self-report.
4. **A demand term, eventually.** Nothing in any basis pays for an *answer*. The one demand-linked
   quantity already in chain state is `PalwVersionUsageV1` per model version — and it must be derived
   from **fees actually paid**, never claim counts, which are self-dealable. Until such a term
   exists, the honest operational statement is that PALW buys arithmetic, not answers.

**Difficulty normalisation is not needed** if 1–3 land: the arbitrage is not that some tasks are
easy, it is that the measure is declared. Market pricing is the right long-run answer to (4), not to
(1).

---

## Verdict

**CONDITIONAL** — on F1 and F2 being closed before permissionless registration opens, F3 and F4
before any stranger's class bears weight, and the work-vector calibration before any replacement
basis arms.

Against the standard that matters — *a future model ten or a hundred times more efficient, or
extremely optimised for one task family* — the answer today is that such a model would be paid
**less**, in exact proportion to how much better it is (F5), while a model that declares more decode
calls than it runs would be paid the same and weigh 7.8× more (F1). That is the wrong way round, and
it is the whole finding.
