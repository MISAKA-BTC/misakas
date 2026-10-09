# 5. The composition argument

How per-relation soundness (S1–S3) becomes a bound for a whole claim (many relations, many positions), how it composes with the
beacon (S6) and the adversary's retries, grinding and adaptivity (S8), and where the selection, enforcement and environment terms
enter. The formula is implemented as a pure integer function, `misaka-palw-challenge/src/composition.rs`
(`composed_false_accept_bits_v1`), and every number in §5.5 is pinned by `misaka-palw-challenge/tests/composition.rs`.

## 5.1 The event

Fix a **false claim** `c`: a committed claim whose committed trace is not the reference execution of its class on its job (A-REF).
Its **false acceptance** `FA(c)` is: `c` reaches Final (or a Panel licence that weighs) and no conviction of `c` lands before its
liability horizon ends. The unit of the bound is one claim; a network-wide statement over a horizon multiplies the algebraic term by
the number `Q` of claims the adversary tries (§5.3).

## 5.2 Decomposition

```text
FA(c)  ⊆  E_bind  ∪  E_sel  ∪  E_alg  ∪  E_enf  ∪  E_env

E_bind : two different openings of one commitment are produced (a BLAKE2b-512 collision / second preimage)          A-H2
E_sel  : no honest verifier's checked scope contains a false relation instance of c                               A-HV, coverage
E_alg  : some honest verifier's scope contains a false instance, and every check in that scope passes             S1, S2, S5, S6
E_enf  : a false instance was found, but no conviction lands in time (filing not included, court saturated,
         fraud laundered into a default)                                                                           A-NET, S7.3, F-C4R3-02/03
E_env  : an environment assumption fails: no honest unpredictable beacon source (ε_src), reorg past the lock,
         budgets violated, DA                                                                                      A-B2, A-B4, A-BUDGET, A-DA

P[FA(c)] ≤ ε_bind + ε_sel + ε_alg + ε_enf + ε_env
```

A union bound needs no independence between the events. Exact families (S3) contribute nothing to `ε_alg`: a false exact relation in
a checked scope is always found, conditional on binding (`E_bind`).

**Why the false instance exists (L-COV).** By Lemma L-COV ([01](01-statements.md) §S3), if every relation instance of `c` held on
its committed values, `c`'s trace would be the reference execution. So a false claim has at least one false instance, in a relation
the plan covers (coverage is code-derived: no relation can be omitted, duplicated or weakened, `check.rs:126–210`). This is what
makes the per-relation statements claim statements; a missing relation would have no error bound at all.

## 5.3 The algebraic term, over a claim and over the beacon

**One scope, one uniform seed.** Let a verifier check scope `S` (a set of relation instances) with vectors from a seed `σ` that is
uniform and independent of the committed claim. If `S` contains a false instance `i*`, acceptance requires `i*`'s check to pass:

```text
ε_scope ≤ q(i*)^(−t)  ≤  2^(−t·b_min)                         (SingleFalseInstance)
ε_scope ≤ Σ_{i ∈ S, probabilistic} q(i)^(−t) ≤ R_S·2^(−t·b_min)   (UnionBound — what the kernel derives, plan.rs:309)
```

The kernel uses the union bound with `R` = probabilistic instances per position × positions (`plan.rs:352–386`): conservative in two
ways — the union is not needed for a commit-then-challenge, non-aggregated check (one fixed false instance must pass its own check),
and the verifier batches weight products across a scope into one stacked instance (`verify.rs:829`) where the plan counts one per
position. Multi-modulus relations contribute `q(i) ≥ 2^89 − 1` (S2). The binding term is added by the union: the kernel's
`min(t·b − ⌈log2 R⌉, binding) − 1`.

**Over the beacon (public coins).** The adversary may (a) choose among at most `G` beacon outputs per beacon (A-B3), (b) consume `β`
beacons per attempt (an interactive suite's rounds; 1 for Freivalds), (c) retry `R_a` times (each a new commitment and a new beacon;
`retry_limit`), (d) try `Q` statements. Its strategy is a tree with at most `Q·(R_a + 1)·G^β` leaves; at each leaf the seed is a
random-oracle output on a preimage that contains a commitment fixed before the leaf's choices (A-H1, A-B2), so each leaf accepts a
false statement with probability at most `ε_scope`. Hence

```text
ε_alg ≤ Q · (R_a + 1) · G^β · ε_scope          (+ ε_src in E_env when A-B2 fails: then the seed is adversarial)
```

The `G^β` union is valid for any protocol; for a round-by-round sound interactive protocol it could be replaced by roughly
`G·β·max_j ε_j`, which needs a per-protocol proof (Q-09). A quantum adversary searching a reachable space for a bad seed gains
quadratically (A-H4).

**Private coins (OPV outsiders).** An outsider draws its vectors from its own salt (A-HV item 2), placed in the kernel binding's
`beacon` field; the binding also contains the claim id and the evidence root (`challenge.rs:22–40`), so every claim faces fresh
vectors. The producer cannot observe or steer the salt: `G = 1`, `β = 1`, no retry against the same verifier, and only `Q` remains
for a horizon statement. **This is the only soundness-bearing check path implemented today** (SG-05).

## 5.4 The selection term

If an honest verifier checks `s` of a claim's `N` units (positions, segments) uniformly and unpredictably, the adversary's best lie
is a single false unit (fixed at commitment), and

```text
ε_sel = (N − s) / N        (exact; without replacement)          bits = ⌊log2 (N / (N − s))⌋
```

No repetition count, field size or modulus changes this term (RFC-0011 §15.1, RFC-0007 §V.7). With complete coverage `ε_sel = 0`.
Several independent honest samplers multiply their miss probabilities; correlated ones (the same public seed) do not.

Under K2S v4, one prosecution reads two positions, but **finding** the faulty position is the detection layer: a whole-claim check
reads the producer's whole DA obligation (≈ 12.6–16 TB at 9B-8k). Whether some honest verifier reads it in time is A-HV with complete
coverage. If not, `ε_sel` dominates and only the economic layer remains:

```text
deterrence:  reservation · (1 − a) · P_dc ≥ gain ,   P_dc ≤ 1 − ε_sel − ε_enf
⇒ reservation ≥ ⌈ max(gain + default_penalty, ⌈gain / P_dc⌉) / (1 − a) ⌉      (the G14-R4 form of opv.rs required_reservation)
```

## 5.5 The full bound and the 9B-8k worked numbers

```text
effective bits = min( check − loss , binding , selection ) − ⌈log2 #present terms⌉

check     = min_f (t_f · b_f) − ⌈log2 Σ_f R_f⌉            [UnionBound]      (SingleFalseInstance drops the R term)
loss      = ⌈log2 (R_a + 1)⌉ + β · ⌈log2 G⌉ + ⌈log2 Q⌉
binding   = 256 (classical; A-H2)
selection = ⌊log2 (N / (N − s))⌋                           (absent for complete coverage)
```

`ε_enf` and `ε_env` are not inside this number: they are the assumptions' failure probabilities and are reported beside it (§5.6).

**Inputs (9B-8k, `coverage-p1p2-record.md` §1.2):** `P = 8,192` positions; 2,210 probabilistic instances per position, so
`R = 18,104,320` and `⌈log2 R⌉ = 25`; `t = 2`; `b = 126` (K2-TIR-v1) or 88 (K2-TIR-v2, descriptor-wide).

| Row | Case | check | loss | other terms | **effective bits** | test |
|---|---|---|---|---|---|---|
| A | K2-TIR-v1, uniform seed, complete coverage | 252 − 25 = 227 | 0 | binding 256 | **226** (= the kernel's `ε ≤ 2^-226`) | `rows_a_b_…` |
| B | K2-TIR-v2, same | 176 − 25 = 151 | 0 | binding 256 | **150** (= the kernel's `2^-150`) | `rows_a_b_…` |
| C | v2 + OPV-BOOT's interim adversary: `R_a = 2`, `G = P(32, 2) = 992` | 151 | 2 + 10 = 12 | binding | **138** | `rows_c_to_f_…` |
| D | v2 + **today's beacon**, a last contributor with `2^40` offline hashes, `R_a = 2`, `Q = 2^20` | 151 | 2 + 40 + 20 = 62 | binding | **88** — below 128 | `rows_c_to_f_…` |
| E | as D with `t = 3` | 264 − 25 = 239 | 62 | binding | **176** (`min_repetitions = 3` for v2, 2 for v1) | `rows_c_to_f_…` |
| F | v2, an outsider's **private salt**, `Q = 2^20` over the horizon | 151 | 20 | binding | **130** | `rows_c_to_f_…` |
| G | v2, an outsider that checks **8 of 8,192 positions** | 151 | 0 | binding 256, selection 0 | **−2** (no security; miss probability 8,184/8,192) | `row_g_…` |
| H | v2 with the **single-false-instance** composition (reviewer question, not policy) | 176 | 0 | binding | **175** | `row_h_…` |
| I | conformance, **interim** drill scope (2 vectors + 2 leaves, `f = 1`, one repetition), OPV-BOOT's interim adversary | 2 − 1 = 1 | 12 | — | **−11** | `rows_i_j_…` |
| J | conformance at the scope-v1 fault model (`f = 1/2`, `1/16`), `R_a = 2`, `G = 2^10`, `Q = 2^10`, binding | 152 − 1 = 151 | 22 | binding | **128** with **211 vectors + 1,686 leaves** (the fewest; 1,897 ≤ the 4,096 chain bound) | `rows_i_j_…` |
| K | OPV deterrent reservation (BILI), gain 20, default penalty 100, accuser 500 ‰ | — | — | — | **240** at `P_dc = 1/2` or `1`; **40,960** at `P_dc = 8/8,192`; none at `P_dc = 0` | `rows_k_l_m_…` |
| L | v2 + a **sealed-source v3** beacon where the adversary can withhold any subset of 8 sealed sources (`G ≤ 2^8`), `R_a = 2`, `Q = 2^10` | 151 | 2 + 8 + 10 = 20 | binding | **130** | `rows_k_l_m_…` |
| M | a staged-beacon interactive suite, 30 rounds × 10 grinding bits, `R_a = 2`, charged as `G^β` | — | **302** | — | unusable without a round-by-round argument | `rows_k_l_m_…` |

## 5.6 What the numbers say

1. **The per-relation algebra is not the bottleneck.** With a uniform seed and complete coverage, K2-TIR-v1 has ≈ 100 bits of
   margin and K2-TIR-v2 ≈ 22 (rows A, B). The descriptor's `t = 2` at 88 bits/repetition leaves v2 only 22 bits for retries,
   grinding and statements; `t = 3` restores ≈ 110 bits (row E). Tightening the accounting (row H, per-relation moduli) is a
   reviewer decision, not needed if `t = 3`.
2. **Today's beacon cannot carry a 128-bit public-coin bound** (row D): the last contributor's offline grinding is linear in its
   hash budget and nothing on chain bounds it (SG-01). With a sealed-source v3 beacon whose steering is bounded by bonded seals, the
   same claim keeps 130 bits (row L) — *if* honest inclusion (`ε_src`) holds (SG-01a, Q-02).
3. **Private-coin outsiders do not need the beacon at all** (row F). This is the only implemented soundness-bearing path, and it is
   why OPV detection does not inherit beacon bias.
4. **Coverage dominates at real scale** (row G). A verifier that cannot read the whole claim (≈ 12.6–16 TB at 9B-8k) gives under one
   bit per claim; the network is then protected by deterrence alone, at a reservation of `gain · P/m / (1 − a)` (row K: 40,960 BILI
   at the interim gain against 1,000 BILI interim reservation). This is the dossier's highest-risk question (Q-01).
5. **Sampled conformance needs ≈ 2,000 checks for 128 bits under the scope-v1 fault model** (row J) and is only as strong as that
   fault model (A-FAULT); the interim policy is a drill (row I).
6. **Interactive suites need a better grinding argument than `G^β`** (row M) before any GKR policy can be approved.

`ε_enf` (A-NET, the censorship and laundering paths C4 round 3 found and G14-R4 fixed on its branch) and `ε_env` (`ε_src`, fork-choice
safety — see the fork-choice safety analysis (internal, FINX) — budgets, DA) have no number yet; each must be bounded or accepted as
an explicit operating assumption before an effective-bits figure means what it says.
