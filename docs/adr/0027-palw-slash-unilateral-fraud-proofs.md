# ADR-0027: PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal

Status: **Accepted (architecture).** Operating envelope unchanged: devnet / shadow / zero-credit.
No slashing is enabled by this ADR; it fixes what a slash may ever rest on.
Date: 2026-08-15
Relates to: ADR-0026 (v2 verification architecture — amended here in §3/§4/§5),
[`misaka-palw-slash-protocol-design-v0.1.md`](../misaka-palw-slash-protocol-design-v0.1.md)
(the input specification this ADR adopts and amends),
[`palw-full-logits-trace-v2-design.md`](../palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates), `consensus/core/src/vlt.rs` (`ComputeFraudKind` — the existing enforcement of the same
rule at the VLT layer).

## Premises (given, not derived)

- **P1 — no BFT dependence.** No honest-majority committee may be the source of truth for a
  verdict. A vote may schedule work or allocate rewards; it may never decide that a computation
  was wrong.
- **P2 — no challenge-randomness dependence.** No slash outcome may depend on a hash-derived
  challenge position being unpredictable or unbiasable. *(Read as: the unpredictability of the
  challenge is not a security assumption. Hash functions remain in use as commitment primitives —
  Merkle openings, domain-separated preimages — which is a different dependency. If the intended
  reading was stronger, this section is the one to correct.)*
- **P3 — fraud terminates in slash.** The final consequence of fraud is bond movement. Not chain
  invalidity, not a reorg, not a cryptographic soundness claim.

These premises are not a restriction added on top of a working design — they **select** the design.
The rest of this ADR is what they force.

## What the premises force

**P1 ⇒ every slashable offense must be objectively refutable from published material by a single
party.** This is already the enforced rule one layer down: `ComputeFraudKind` slashes only on
`ContradictoryVerification` (one verifier, two signatures that cannot both be true) and explicitly
refuses to slash on `ForgedReceipt` — "I re-ran it and got something else" is not checkable without
re-running it, and consensus cannot re-run it. PALW-S generalizes that rule rather than carving an
exception for it.

**P1 also deletes "false PASS by majority" as a concept.** A PASS is not a positive finding of
correctness; it is **the absence of a refutation within the window**. Nobody is ever slashed for
failing to find fraud (unprovable). A party that *chooses* to sign a positive attestation stakes a
bond against later refutation — that remains objective, because the slash evidence is
`signature ∧ refutation`, not a jury's opinion.

**P2 ⇒ the disputed position is chosen adversarially by the challenger, not drawn by a PRF.**
Sampling exists to catch a cheater who does not know where you will look; if unpredictability is
not assumable, sampling has no security to offer and is demoted to an incentive/coverage device.
What replaces it is a challenger who **re-executes and points at the first divergence**.

**P2 also removes an entire failure surface**: anchor-reorg fragility (v0.1 §8.2), commitment
grinding over favorable challenges (§8.3, §26.4), and the requirement that a "finalized" anchor
exist before a challenge may be issued. None of it is load-bearing any more.

**P1 + P2 ⇒ exactness is mandatory, not preferred.** ADR-0026 §3 chose exact-within-class over a
tolerance band on measurement grounds. Under P1 that choice becomes structural: a tolerance
verdict ("close enough?") is not objectively checkable by one party, so it can only be settled by
a vote — which is precisely the BFT dependence P1 forbids. **A tolerant comparator and a BFT-free
slash cannot coexist.**

**P3 ⇒ un-refuted fraud is a credit error, not a chain fault.** The worst case of a fraud nobody
checks is wrongly-minted credit, which is bounded by exposure caps and the staged ladder (v0.1
§20.2) — never by rolling back the DAG.

## Decision

### 1. Direct refutation is the primary path; bisection is the DA-degraded fallback

Any deviation from the pinned execution implies **at least one locally-refutable step**: at the
first divergent step, the committed input still equals the honest input (the prefix matched) while
the committed output does not equal the step function applied to it. So a challenger who
re-executes does not need to search — it locates the first divergence and names it.

```
challenger re-executes the job under the pinned profile
  → first index i where its state ≠ the miner's committed state
  → submits ExecutionStepRefutationV1 { proof_id, step_index i,
        opening(committed input state at i), opening(committed output state at i) }
  → every node recomputes ONE step from the opened input under canonical reference semantics (§2)
  → if output ≠ committed output: the miner is slashed. If it matches: the challenger is slashed.
```

One round, one bounded check, no vote. **Bisection is retained only for the degraded case** where
the miner has withheld intermediate states so the challenger cannot name a step with openings: the
interactive ladder forces disclosure incrementally (≈log₂(steps) ≈ 20 rounds at 10⁶ steps), each
round objectively checkable, terminating in the same one-step check. Non-response at any rung is
an objective offense (v0.1 §17.1 `M-O3`), so withholding is not an escape — it is a faster loss.

This makes **data availability load-bearing for the BFT-free property**, not merely an anti-
griefing nicety: v0.1 §10's DA manifest, retention and multi-source failure evidence are adopted
in full, and `DATA_WITHHOLDING` keeps its objective status.

### 2. Adjudication semantics: one step, canonical reference arithmetic, reproduced by every node

The one-step check must be reproducible by **every node on heterogeneous hardware**, which native
float cannot promise. It is therefore defined in **canonical reference arithmetic** — deterministic
software IEEE-754 (soft-float) or the profile's integer form (v0.1 §5.2 `CanonicalTensorV1`), with
pinned operation order, no FMA contraction, RNE, no fast-math. Cost is not an obstacle at this
granularity: one decode step of a 2B model is ~4·10⁹ FLOPs, while one pinned GEMM tile (e.g. 64³)
is ~5·10⁵ — four orders below — and a 10–100× soft-float penalty on one tile is still negligible
beside ordinary block validation. **The claim is 10¹⁵ ops; the adjudication is 10⁵–10⁷.** That
ratio is the entire reason a BFT-free dispute is affordable.

This upgrades the class definition of ADR-0026 §3. A determinism class stops being *"these hosts
agreed pairwise"* and becomes:

```
runtime_class_id conforms  ⟺  it reproduces the canonical reference implementation
                              bit-exactly over the conformance corpus
```

An absolute reference replaces pairwise agreement; conformance is then transitive, and the
adjudicator needs no class membership of its own. Honesty about the cost: a class conforms only if
its reduction order is pinned (true of the current CPU profile by construction; **hard on GPUs**,
which reorder reductions via split-K/atomics unless deterministic kernels are mandated). A backend
that cannot match the reference cannot be admitted — that is a clean admission criterion, and it
is expected to exclude some hardware.

### 3. What replaces `P_detect`: one funded honest re-execution, and f-independence

v0.1 §9 sizes bonds from `P_detect = 1-(1-f)^q`, and v0.1 §31.1 correctly names its own fatal
weakness: an attacker who makes `f` arbitrarily small makes sampling arbitrarily weak. Under
direct refutation that parameter disappears:

```
sampling model      P_detect = 1-(1-f)^q            ← adversary minimizes f
refutation model    P_detect = P(at least one honest full re-execution + timely inclusion)
                             ← f-independent: a full re-executor finds ANY deviation
```

A single mis-multiplied tile is caught with the same certainty as a wholly fabricated trace. The
price is stated plainly and must not be hidden later: **the honest verification tax is one full
replay per checked job, so a checked job costs ≥2× primary execution.** This is the honest
counterpart of the "one-token verification is cheap" claim already banned in the v2 design §15;
per-class cold, no-KV, p99 replay cost is what the §10 economics must carry.

`q` survives with a changed meaning: not *how many positions to sample* but **how many independent
re-executors to fund**, i.e. the redundancy of the 1-of-N assumption. v0.1 §9.2's dynamic inputs
(reward, bond, gain, model size, length, reputation, recent mismatch) apply unchanged to that
quantity, and v0.1 §9.1's tiers become funding tiers. The inequality keeps its shape, with
`P_detect` now an incentive property rather than a combinatorial one:

```
P_check · S_eff ≥ λ · G_max,   λ ≥ 2.0 initially,   P_check measured, never assumed
```

### 4. Amendments to the v0.1 specification

| v0.1 mechanism | Verdict under P1/P2 | Replacement |
|---|---|---|
| §12.3 primary quorum 4-of-5 / 5-of-7 | BFT — may not convict | verifiers are watchers; no vote is counted |
| §16.3 appeal jury 9-of-13 | BFT | one-step check under §2; no jury exists |
| §15.2 `ComputationFaultCertificateV1` (quorum bitmap + aggregate sigs) | BFT | `ExecutionStepRefutationV1` (§1): one step, openings, objectively checkable |
| §8 future-anchor challenge randomness as security | P2 violation | challenger-chosen position; randomness kept only as an optional coverage sampler that decides nothing |
| §9 `P_detect = 1-(1-f)^q` for bond sizing | f-minimizable | §3: funded full re-execution, f-independent |
| §14.3 "primary quorum + appeal ⇒ FAIL_COMPUTE" | BFT | FAIL only via §1 refutation; no quorum state |
| §13 verifier commit-reveal | needed only because votes were counted | optional hygiene for the attestation market; not a verdict input |
| §18.2 `V-C1` false PASS | keep — but re-grounded | slashed on `signature ∧ later refutation`, not on a jury's overturn |
| §18.2 `V-C2` false FAIL | keep — re-grounded | challenger loses the one-step check ⇒ ChallengeBond slashed |
| §19.2 statistical circuit-breaker triggers | may not slash; may freeze | §5: objective freeze trigger; statistics stay advisory |

Adopted from v0.1 **unchanged**: the four-bond model (§4) including `max_leverage ≤ 1.0` and
unbonding ≥ dispute lifetime; DA rules (§10); the objective offense taxonomy (§17.1, §17.3 O-codes,
§17.4); reward/bond separation and correlated-offense caps (§18.1, §24.2); `slash_id` idempotency
(§24.1); slash distribution weighted away from challenger bounty (§18.4); delayed settlement and
credit caps (§20); DAA-score-denominated deadlines with the "DAA stalls ⇒ deadlines stall" rule
(§23); the prohibitions (§27); and the test plan (§28), whose canonicalization matrix now doubles
as the reference-conformance corpus of §2.

### 5. Freeze is permissive, slash is strict

Freezing takes no property, so its trigger may be **permissive**; slashing transfers property, so
its trigger must be **objective**. A statistical anomaly may therefore freeze a profile but may
never slash anyone. To keep even the freeze off a governance vote, one objective trigger is
introduced:

```
ClassContradictionCertificateV1 = two hosts, same runtime_class_id, same golden set,
  same job envelope, two signed and differing roots
    ⇒ objectively contradictory: the class claim itself is refuted
    ⇒ profile/class freeze (fail-safe), bonds frozen not released, no slash
```

The rest of v0.1 §19 applies as written: new slashes stop, bonds freeze rather than release
(an attacker must not escape through the emergency exit, an honest operator must not be
liquidated by it), PALW credit and DNS weight stop accruing, and **the BlockDAG and the hash PoW
floor keep running**. A corrected profile is a new `profile_id`; profiles are never overwritten.

### 6. Slash-terminal settlement (P3)

No PALW outcome — pass, fail, dispute, freeze — touches block validity, fork choice, or a past
DAG. Block validity remains `valid permanent hash PoW AND (PALW certificate absent OR valid under
its activation stage)`. A refuted proof moves bonds and revokes provisional credit; it never
re-orders history. The v0.1 §21 ladder is adopted with its stage semantics restated under this
model:

```
Stage 0 Shadow          record refutations, slash nothing; measure P_check, replay cost, DA
Stage 1 Objective slash objective offenses only (opening/equivocation/deadline/DA/bond misuse)
Stage 2 Bounded         §1 refutation may slash WorkBond; BaseBond capped 5-10%
Stage 3 Full            repeat-offense BaseBond escalation; wider DNS weight; hash floor stays
```

Stage 1's "computation mismatch does not slash BaseBond" is not a temporary conservatism here — it
is the same rule as P1, applied while reference conformance (§2) is still being proven.

## Assumptions that remain (stated so they can be attacked)

1. **1-of-N honest re-execution, funded.** At least one independent party actually re-executes a
   given job. This is an incentive property; if nobody is paid to check, nothing is checked. It
   replaces the honest-majority assumption with an honest-*existence* assumption — weaker, but not
   free.
2. **Censorship-resistant inclusion inside the window.** A single refutation must reach the chain.
   Under BFT this was diluted across a committee; here it is the critical liveness assumption.
   Mitigations: windows sized in DAA score with the stall rule, multiple submission routes, and a
   window long enough for the degraded bisection ladder (≈20 rounds × response window — materially
   longer than v0.1 §23's 30-minute dispute window; sizing it is open work).
3. **Reference conformance of the miner's class.** If a class silently stops matching the
   reference, honest miners are refutable. §5's freeze is the fail-safe, and §2's corpus is what
   makes drift detectable before it is punitive.
4. **DA sufficiency.** Enough of the trace must be retrievable to name a step; otherwise the
   dispute degrades to bisection and, ultimately, to an objective non-response offense.

Not assumed anywhere: honest majority, unpredictable challenges, cryptographic soundness of the
commitment as a proof of computation.

## Consequences

* **New consensus objects to design** (none exist yet): `ExecutionStepRefutationV1`, the bisection
  ladder messages, `ClassContradictionCertificateV1`, and a step-check verifier over canonical
  reference arithmetic. `ComputationFaultCertificateV1` from v0.1 §15.2 is **not** implemented.
* **`ComputeFraudKind` gains a provable computation kind for the first time.** The memory-of-record
  rule — `ForgedReceipt` can never slash because it is uncheckable — stands for *unaided claims*.
  A refutation that carries openings and resolves to one reference-arithmetic step is checkable, so
  it is the adjudication that `FailedChallenge`/`ForgedReceipt` always lacked. Wiring it is future
  work behind the same staged gates; nothing in this ADR enables it.
* **The step function must be pinned at tile granularity** (`shape_profile_id`): which operator,
  which tile shape, which reduction order. This extends ADR-0026 §2's activation/GEMM legs from
  "committed" to "adjudicable", and is a prerequisite for Stage 2.
* **Reference implementation becomes a deliverable**: a soft-float/integer canonical evaluator,
  independently implemented **twice** (v0.1 §29 gate 1) — two implementations agreeing is evidence
  about the *specification*, which is a different and legitimate use of agreement than a jury
  deciding a fact.
* **GPU admission is now falsifiable**: a backend is admitted iff it matches the reference
  bit-exactly with pinned reductions. Expect some hardware to fail this and stay out.
* ADR-0026 §3 (class definition), §4 (challenge randomness) and §5 (`q` semantics) are amended by
  §2/§3 above; the amendment notes are carried in that ADR so the two cannot drift apart.
