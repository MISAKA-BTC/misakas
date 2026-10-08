# 1. The statements under review

Every statement below names its code at the dossier's base commit `b676927de` (branch `sound/review-dossier`) unless another ref is
given. "Design" means a document on a branch with no code yet. A statement's **assumptions** are the `A-*` items of
[02-assumptions.md](02-assumptions.md); its **parameters** are in [03-parameters.md](03-parameters.md).

## 1.0 Notation

| Symbol | Meaning |
|---|---|
| `H(d; x)` | keyed BLAKE2b-512 with the domain string `d` as the key, over `u64_le(len x) ‖ x`, `x` a canonical Borsh encoding (`misaka-palw-challenge/src/hash.rs:10`, `misaka-palw-kernel/src/hash.rs:19`) |
| `C(T)` | a tensor commitment (row tree and column tree over the elements, BLAKE2b-512; `misaka-palw-kernel/src/merkle.rs`, `tensor_commitment_v2`) |
| `q` | a Mersenne prime used as a check modulus: `2^127 − 1`, `2^107 − 1` or `2^89 − 1` (`misaka-palw-kernel/src/field.rs:15,21`) |
| `b(q)` | `⌊log2 q⌋` = 126, 106, 88: the bits one repetition buys (`field.rs:18`, `family.rs:107`) |
| `t` | repetitions per relation instance (the descriptor's `soundness.repetitions`, `descriptor.rs:154`) |
| `R` | the number of probabilistic relation instances in a statement (`plan.rs:352`, `probabilistic_instances_per_position × positions`) |
| `σ` | a challenge seed; `r ← Stream(σ, label)` a vector drawn by a labelled stream from it |
| `G`, `β`, `R_a`, `Q` | adversary choices per beacon, beacons per attempt, counted retries, adaptive statements ([05-composition.md](05-composition.md)) |
| span | `span(k, a, b, out) = max|out| + k · max|a| · max|b|`, the largest integer error a `MatMul` with these operand types can make (`plan.rs:233`) |

A **statement** (claim, conformance commitment, work slice) is *committed* when its commitment is accepted on the canonical chain; a
check is *post-commit* when its randomness is derived from a value that did not exist at that point.

---

## S1. Freivalds, single modulus (K2-TIR-v1 `FreivaldsM127`)

**Statement S1.** Let `X ∈ Z^{m×k}`, `W ∈ Z^{k×n}`, `Y ∈ Z^{m×n}` be fixed before `r` is drawn, and let `E = Y − X·W` over `Z`.
If `E ≢ 0 (mod q)` and `r` is uniform on `F_q^n` and independent of `(X, W, Y)`, then

```text
Pr[ X·(W·r) ≡ Y·r (mod q) ] ≤ 1/q ,      and with t independent uniform r_1..r_t:   ≤ q^(−t) < 2^(−t·b(q)).
```

*Proof sketch.* Take a row `e` of `E` that is nonzero mod `q`; `e·r` is a nontrivial linear form in uniform `r`, hence uniform on
`F_q`, hence zero with probability exactly `1/q`. Repetitions with independent vectors multiply.

**Integer-to-field condition.** A nonzero integer error is nonzero mod `q` whenever `|E_ij| < q`. The verifier enforces
`|Y_ij| ≤ max|out|` (every opened output is range-checked against its dtype, `verify.rs:416` `well_typed`) and operands are their
dtypes' values, so `|E_ij| ≤ span`; admission refuses a `MatMul` relation unless `span < 2^127 − 1` (`check.rs:143–146`). The
exact-result rule (ranges proven at admission, `check.rs:108–115`) guarantees an honest product never overflows its type, so the
honest claim always satisfies the check (completeness).

**Variants implemented** (`verify.rs:538` `freivalds_once`, `:757` `check_instance`):

| Variant | Check | One instance is |
|---|---|---|
| weight on the right, batched | `X_p·(W·r) = Y_p·r` for every position `p` of the scope, one `r` | the stacked matrix over all positions (`:829`: counted once) — a nonzero stacked error is caught w.p. `≥ 1 − 1/q` |
| weight on the left (GEMV lowering), batched | `(rᵀ·W)·X_p = rᵀ·Y_p` | same, columns |
| activation × activation (`Q·Kᵀ`, `P·V`, gathered experts) | `X·(W·r) = Y·r` per position and batch slice, fresh `r` | one `(position, occurrence, node, slice)` |

The batched form compares the weight's commitment at every position with the one projected (`verify.rs:824–827`); a different weight
falls back to the per-instance form. The verifier computes `W·r` itself from the authenticated `W` — no projection is ever taken
from the producer (RFC-0007 §V.3).

**Vector derivation.** `r` comes from `ChallengeStreamV1` (`misaka-palw-kernel/src/challenge.rs:56–113`) keyed by
`ChallengeBindingV1::seed()` (`challenge.rs:33`) and the label `(kind, position, occurrence, node, repetition, slice)`; each element
reads a 128-bit word, keeps `e` bits and rejects the single value `2^e − 1` — exactly uniform on `F_q` given uniform words (§S6.4).

**Not claimed by S1.** Anything about the authenticity of `X` or `W` (other relations and openings, §S3); anything if `r` is
predictable when `Y` is fixed (see finding SG-05: the public record's `beacon` field is `[0; 64]`); any aggregation across relations.

---

## S2. Freivalds, multi-modulus CRT for `i128` accumulators (K2-TIR-v2 `FreivaldsCrtV2`)

**Statement S2.** Let `M` be the fewest moduli of `(2^127 − 1, 2^107 − 1, 2^89 − 1)`, largest first, with `∏_{q∈M} q > span`
(`plan.rs:246` `dense_moduli_v2`). For each `q ∈ M` and each repetition `j < t`, draw an independent uniform `r_{q,j} ∈ F_q^n`
(label `j·3 + index(q)`, `verify.rs:838`). If `E ≠ 0` over `Z`, then

```text
Pr[ all t·|M| checks pass ] ≤ max_{q∈M} q^(−t) ≤ (2^89 − 1)^(−t) < 2^(−88·t).
```

*Proof sketch.* Some entry `e ≠ 0` has `|e| ≤ span < ∏ q`. The moduli are distinct primes, so `∏ q ∤ e` and some `q* ∈ M` has
`q* ∤ e`; then `E ≢ 0 (mod q*)` and the `t` checks modulo `q*` use independent vectors, so S1 applies with `q*`. The adversary picks
`E`, so it can make `q*` the smallest modulus (e.g. `e = (2^127 − 1)(2^107 − 1)`); the bound is therefore the smallest modulus's.

**How "span < ∏ q" is established.** `dense_moduli_v2` returns 1 modulus when `span < 2^127 − 1` exactly; otherwise it takes
`s = max(⌈log2 k⌉ + ⌈log2|a|⌉ + ⌈log2|b|⌉, ⌈log2|out|⌉) + 2` (`plan.rs:238`, so `span < 2^s`) and the fewest `j ≥ 2` moduli with
`s < Σ e_i`; since `∏ (2^{e_i} − 1) > 2^{Σ e_i − 1}`, `span < 2^s ≤ 2^{Σe − 1} < ∏ q`. A span beyond all three is
`KERNEL_EXTENSION_REQUIRED` (`check.rs:147–165`). `from_i128` maps every `i128` including `i128::MIN` (`field.rs:91`, tests `:215`).

**Descriptor accounting.** The whole K2-TIR-v2 descriptor prices every repetition at 88 bits (`descriptor.rs:106–113`,
`family.rs:107–113`), even for relations whose span needs only `2^127 − 1`. This is conservative (finding SG-08).

**Tests.** `misaka-palw-kernel/tests/k2_wide.rs` (`a_lie_that_aliases_to_zero_mod_2_127_minus_1_is_caught_by_the_second_modulus`,
`every_scalar_lie_in_the_i128_product_is_caught_and_convicted`, `moduli_are_the_fewest_whose_product_exceeds_the_span`);
`field.rs` unit tests (`an_integer_error_below_the_moduli_product_survives_in_some_modulus`); the toy-field experiment of this dossier
(`misaka-palw-challenge/tests/composition.rs`, `toy_crt_…`): with moduli 7 and 31, `e = 31` passes with probability 1/7 and
`e = 217 = 7·31` always passes — a span at the product is a hole, which is why admission refuses it.

---

## S3. Exact recompute families, state continuity, and the media-pipeline edge

**Statement S3 (per relation).** For a relation instance whose checker is `ExactRecompute` (families Structure, ExactArithmetic,
QuantRange, Nonlinear, Selection) or `StateContinuity` (RecurrentState), with every input and the output opened and authenticated
against the commitments its wiring names (`verify.rs:378` `open`), the verifier accepts iff `eval_node(inputs, prior rows) = output`
(`verify.rs:873–879`). The error term is **0**, conditional on: binding of the commitments (A-H2), the reference evaluator being the
semantics (A-REF), and the exact-result rule (A-RANGE). A derived value (a `Hist` window or its view) is rebuilt from its inputs and
committed rows and its commitment compared (`verify.rs:768–791`).

**The media-pipeline edge (K2-TIR-v3, `EdgeRecompute`).** Each pipeline stage input at each position is recomputed exactly from its
binding — a job value (scalars, token templates, counts, a canonical image), an earlier stage's committed output rows or final value
(authenticated copy with zero pad), or RFC-0003's `R` — and must lie in the input's declared interval (`family.rs:94–97`,
`descriptor.rs:186–201`, `pipeline.rs:136` `expected_edges`, `:321` `stage_input_v1`; court `:821` `verify_edge_fault_v1`). Error 0
under the same conditions. Note for the beacon (§S6): `R` is bound by `random_binding` (`pipeline.rs:463`), a commitment of the
job's seed and item — whoever chooses the job chooses `R`.

**Lemma L-COV (coverage turns per-relation soundness into claim soundness).** Suppose (i) every node of every scheduled block
occurrence has exactly one relation with the descriptor's checker, court and repetitions — no omission, no duplicate, no weaker
checker (`check.rs:126–210`); (ii) every state has its boundary (`check.rs:211–214`); (iii) each delivered token satisfies the
decode relation against the committed logits (`ledger.rs:2033` `OutsiderV1::check_program`); (iv) every input — tokens, params (against the
registered param commitments), consts, initial state — is authenticated. Then *every relation holds on the committed values* implies
*the committed trace equals the reference execution*: induction over the evaluation order, each node's inputs being earlier values
or authenticated inputs. Consequently a claim whose trace is not the reference execution has at least one false relation instance.
**L-COV is the load-bearing link between S1–S3 and a whole claim; its preconditions are code-derived (`check_plan_v1`), not
declared.**

---

## S4. Element courts and the first-wrong-value induction (K2S, K2-TIR-v4 — design)

Source: `docs/design/palw/k2-real-scale.md` on branch `k2/real-scale` (`b8870bde0`, design only; "the test pins exact values" is
pending). A v4 tensor commitment has a row tree and a column tree whose leaves are tiles of at most `T = 4,096` elements of one
line; a position root is a Merkle root over `(occurrence, node, C(value))`; segments of `S = 1,024` positions; the claim carries the
segment roots. A fault names one output element `e` of one committed value `(p, s, n)`; the court authenticates one leaf containing
`e` and, per input, the leaves covering `e`'s dependency line (one leaf for elementwise/structure; index + data leaf for `Gather`;
`⌈n/T⌉` line leaves for reductions/`TopK`; `⌈k/T⌉` row leaves of `X` and column leaves of `W` for `MatMul`; the new row or the
previous position's window leaf for `HistAppend`), and recomputes `e` with the reference semantics.

**Statement S4a (court soundness — no honest conviction).** If the committed trace is the reference execution and every commitment
is consistent, every element court dismisses — except with the probability of breaking binding (A-H2).

**Statement S4b (prosecution completeness).** Call a committed value *wrong* if any element of any of its leaves (row tree **or**
column tree) differs from the reference value. If some committed value is wrong, let `v*` be the first wrong value in
`(position, occurrence, node)` order and `e` a wrong element of it. Every input of `v*` is either an earlier committed value (at `p`
or `p − 1`) — right by minimality — or a param / const / zeros / prompt-tile token authenticated against a registered root. The
element court on `e` therefore evaluates the reference value from right inputs and convicts, because the committed element differs.
Two authenticated leaves of one operand that disagree on an element are an inconsistent commitment and convict directly. If the
producer withholds material a court would need, the demand path ends in a **default**, never a conviction (no court reads withheld
bytes).

**Preconditions a reviewer must check** (each is a place the induction can break):

1. `(p, s, n)` order is a topological order of the *whole* dependency graph, including cross-position reads (`Fixed` states and
   `Hist` windows of `p − 1`, the `δ` shift of a full window) and derived values, which v4 commits (no "omitted" value).
2. The leaf sets per primitive cover the entire dependency line of `e` (the table in K2S §3), including broadcasting and the
   `Gather` data address read from the index.
3. The court's reduced evaluation (`eval_primitive` on a line, a `1×k · k×1` product, a scalar) equals the full semantics at `e`,
   including the exact-result rule, `Cast` fit, `Div` by ≥ 1 and `Gather` bounds — a refusal of the semantics is a conviction.
4. The decode relation: greedy token `t` is wrong iff some `j` has `logits[j] > logits[t]`, or equality with `j < t` (two leaves).
5. Withholding: the economic consequence of a withheld lie is the default penalty, not the slash (§S7, finding SG-14).

S4b is a *prosecution-completeness* theorem: it says a wrong trace **can** be convicted by whoever finds it. It says nothing about
whether anybody looks (A-HV, §5.4): per K2S §2, an outsider checking `m` random positions of `P` finds a one-position lie with
probability `m/P`, and a whole-claim check reads `P·M_pos` (≈ 16 TB at 9B-8k).

---

## S5. Sampled conformance (RFC-0011 §17, RFC-0013 §9, OB-P0 tag 109)

Code: `consensus/core/src/palw_conformance_evidence_v1.rs` (the pure half; the fold applies it), `misaka-palw-challenge/src/
conformance.rs`. A committed scope (`ConformanceScopeV1`, `:91`) fixes, before randomness: `vectors_per_repetition` prompts (full
forward passes of the reference, the independent and the typed-backend implementation, all three required — `:141–145`),
`leaves_per_repetition` artifact leaves (opened against the artifact root and decoded by each implementation), and the fault model
`(vector_fault_ppm, leaf_fault_ppm)`. The seed selects the checks (`derive_selection_v1`, `:276`): prompts with replacement (length
and tokens by `index_below`), leaves **without** replacement within a repetition (`distinct_indices`).

**Statement S5.** Fix a family with `n` draws. If the implementation set disagrees with the reference on at least a fraction `f` of
the family's draw space (assumption A-FAULT), and the draws are uniform and independent of the commitment, then

```text
Pr[ all n draws agree ] ≤ (1 − f)^n ≤ 2^(−n·f·log2 e),
```

(without replacement the hypergeometric miss probability is no larger). `derived_epsilon_bits` (`:174–189`) returns
`⌊n·ppm·14,426 / 10^10⌋ ≤ n·f·log2 e` (`log2 e > 1.4426`), the minimum over the present families.

**Conditionality, stated by the code itself.** The fault density is an *assumption*, approved by a soundness policy
(`approved_fault_model_v1`, `:77`) — the candidate can neither choose it nor drop an implementation (C4 F-C4-11 / GAP-C4-D, fixed).
The only approved model today is the unreviewed test id's `f = 1` (`:78`), for which the bound is vacuous. A sparse fault (a trigger,
a dtype-local decoder bug touching fewer than `f` of the leaves) is outside S5. **A conformance PASS is not semantic admission, not
whole-model fidelity and not per-claim correctness** (`statement()`, `:197`).

**On-chain judgement (optimistic).** The fold rebuilds the selection from the chain's own beacon and seed and recomputes every
count, status and root it can (`verify_conformance_evidence_v1`, `misaka-palw-challenge/src/conformance.rs:209`); forward passes
and leaf decodes are not in the block, so a posted pass opens a refutation window (80 DAA interim) for `LeafDecode` (an opening whose
reference decode differs) and `VectorTokens` (a Final kernel claim on the selected prompt whose greedy tokens differ). Stated residual:
a vector outcome's logits / commit digests have no on-chain court (`:30–34`).

**Composition with the beacon.** S5's draws are uniform only if the seed is; with `G` adversary-reachable seeds the bound becomes
`G·(1 − f)^n` (§5.3).

---

## S6. The PALW Work Beacon, the seed and the samplers (`misaka-palw-challenge`)

**S6.1 Canonical collection (proved by construction and tests).** `collect_work_beacon_v1` (`beacon.rs:310`) is a deterministic
function of `(context, the multiset of events settled by the tip, the tip)`:

* order-independent: sorted by `(settlement_position, occurrence_index, canonical_work_id)`, ties broken by the whole event
  (`beacon.rs:282–286`); the first occurrence of a work identity counts, eligible or not (`:290–294`);
* eligibility (`beacon.rs:145–179`): REAL useful work only; profile Active and G14-complete at the subject's commitment; not the
  candidate and not depending on it; commitment accepted at or after `S = commitment + anchor_delay`; settled in `[S, S + window)`;
  Final; DA satisfied; validity independent of the consumer; Panel-independent for a `PanelAssignment` subject;
* `acc_0 = H(WORK-BEACON; genesis, ruleset, policy id, commitment root, epoch)`, `acc_i = H(MIX; acc_{i−1}, i, H(ITEM; profile,
  work id, execution commitment))`, output `acc_k` (`:252–264`); anchor `H(CHALLENGE-ANCHOR; policy id, epoch, S, window, work ids)`
  (`:273`);
* `Locked` once `k` sources exist and the tip is `D` past the `k`-th settlement; `Unavailable` (never a fallback) if the window closes
  with fewer (`:321–348`); a presented beacon is a claim to recompute (`verify_work_beacon_v1`, `:367`).

Tests: `misaka-palw-challenge/tests/contract.rs` (canonical order, dedup, lock depth, no fallback, reorg recompute, forged evidence,
ties), C4's `adv_c4_contract` (720 permutations; every field edit refused; branch `adv/c4-e2e`).

**S6.2 Seed binding.** `challenge_seed_v1 = H(CHALLENGE; subject, anchor, output)` (`seed.rs:30–49`) is minted only from a
`VerifiedWorkBeaconV1` of the same context (a type that cannot be decoded or assembled by a caller, `beacon.rs:209`), and refuses a
subject whose policy, kind, commitment root, chain or ruleset differ. Every subject kind and every bound root gives a distinct seed
(contract tests).

**S6.3 Bias — the statement under review (not proved by the code).**

```text
For every predicate B on beacon outputs:   Pr[output ∈ B] ≤ G · μ(B) + ε_src ,
```

where `μ` is the uniform measure on `{0,1}^512`, `G` the number of outputs the adversary can select among (A-B3) and `ε_src` the
probability that no selected source is honest and unpredictable (A-B2). In the ROM this follows from: one source whose item is
unpredictable to the adversary until its last influencing choice makes every reachable `acc_k` a fresh random-oracle output, and the
adversary selects among at most `G` of them. **What `G` is today:** with the v2 accumulator the *last contributor* sees
`acc_{k−1}` (all earlier sources are public before it commits) and can generate candidate works offline — each funded job nonce
gives a new `canonical_work_id` — so `G` is bounded only by its hash budget (finding SG-01, recorded `9ea89994b`). The fix under
design, a **sealed-source beacon v3**, orders sources by their bonded claim **seal** position, seals inside the window and reveals
after it closes; then the reachable outputs are the subsets of the adversary's own sealed sources it can withhold (each withheld
seal forfeits its deposit) times the branches it can make canonical: `G ≤ 2^{a}·F`. Whether at least one selected source is honest
(`ε_src`) depends on the selection rule (finding SG-01, reviewer question Q-02).

**S6.4 Samplers are exactly uniform (proved; given i.i.d. uniform 64-bit words).**

| Sampler | Code | Rule | Exactness |
|---|---|---|---|
| `index_below(n)` | `seed.rs:130` | reject `w ≥ 2^64 − (2^64 mod n)`, return `w mod n` | uniform on `[0, n)`; rejection probability `< n/2^64` |
| `distinct_indices(n, c)` | `seed.rs:145` | Floyd's algorithm over `index_below` | uniform over the `C(n, c)` subsets |
| `field_m127` | `seed.rs:160` | two words, mask to 127 bits, reject `2^127 − 1` | uniform on `GF(2^127 − 1)`; rejection `2^-127` |
| kernel `next_in::<Mersenne<e>>` | `misaka-palw-kernel/src/challenge.rs:90` | one 128-bit word, mask to `e` bits, reject `2^e − 1` | uniform on `GF(2^e − 1)`; rejection `2^-e` |

Exhaustion is a named error, never a biased fallback: a challenge-crate stream draws at most `STREAM_MAX_WORDS_V1 = 2^24` words
(`seed.rs:74`) — a `GF(2^127 − 1)` vector longer than `2^23` elements exhausts it (finding SG-09). The kernel's stream has no bound.

**S6.5 Streams.** A challenge-crate stream is `H(STREAM-BLOCK; H(STREAM-KEY; seed, label), counter)`; a kernel stream is
`H(kernel/challenge-stream; seed ‖ label fields ‖ counter)` over fixed-width fields. Under A-H1 distinct labels give independent
uniform words. Distinct kinds (`query`, `segment`, `tensor-range`, `vector`, `freivalds`, `aggregation`, `proof-round`) are distinct
labels (`seed.rs:54–62`).

**S6.6 Interactive modes (no checker uses them yet).** `staged_round_challenge_v1` (`seed.rs:190`) refuses a round whose message was
committed at or after its new beacon window's start; `FiatShamirTranscriptV1` (`seed.rs:211`) absorbs the statement, policy, source
binding, round index and every prior `(message, challenge)` pair. Its soundness (random-oracle and quantum-random-oracle, adaptive
grinding) is an external gate (RFC-0007 §VI.6); `verify()` does not know the expected statement, so the consumer must compare it
(GAP-C4-B).

---

## S7. OPV: window, economics and liability (RFC-0015)

Code: `misaka-palw-kernel/src/opv.rs`, `consensus/core/src/palw_panel_free_v1.rs` (dormant fence, refused at every real height).

**S7.1 Timing (proved: the relations are validated; their premises are assumptions).** `OpvPolicyV1::validate` (`opv.rs:185`)
enforces, with `W = base window + verification horizon`, `L` the ledger policy:

```text
first step   B_cold + B_check + B_carrier + B_reorg                         ≤ W
response     B_disclose + B_carrier                                         ≤ court_deadline
proof        B_localize + B_court + B_carrier + B_reorg                     ≤ proof_grace
whole path   Σ all B                                                        ≤ W + court_deadline + proof_grace   (hard deadline)
liability    liability > W + court_deadline + proof_grace
```

*Implication.* If the budgets `B_*` hold on the network (A-BUDGET) and an object broadcast at `t` is included by `t + B_carrier`
(A-NET), a verifier that starts by `verifier_start_cutoff(a) = a + W − (B_cold + B_check + B_carrier + B_reorg)` (`opv.rs:166`) has
its first filing included inside the window; a demand is answered or defaults by its deadline; a proof enabled by a served demand is
included inside the proof grace; and Final cannot precede a timely valid proof (Final waits the grace; `g14_kernel_route_final_waits_
the_proof_grace…`). The budgets themselves are **claims about the network**, measured outside the code (MEAS lane).

**S7.2 Deterrence (a rational-adversary statement, not a cryptographic one).** A producer lies when
`gain > P_dc · loss`, with `P_dc` the probability of detection *and* conviction. The reservation rule makes
`reservation · (1 − a) · P_dc ≥ gain` hold with `P_dc` replaced by the *assumed* `assumed_detection_permille`:

```text
HEAD (opv.rs:150):           reservation ≥ max(gain + default_penalty, ⌈1000·gain / det‰⌉)
g14/r4-fixes (GAP-R7):       reservation ≥ ⌈ max(gain + default_penalty, ⌈1000·gain / det‰⌉) · 1000 / (1000 − accuser‰) ⌉
```

The second form prices self-recoup (a colluder convicts itself first and keeps the accuser share `a`). Each claim reserves from free
collateral (no double use), per-producer and total live caps bound the aggregate gain (`opv.rs:534–575`), and the gain is
`claim_reward + work_credit + external_gain_bound` (`opv.rs:144`). **`P_dc` is an input, not derived** (finding SG-13); with a
sampled coverage `P_dc ≤ m/P` (§5.4).

**S7.3 Censorship cost (a lower bound).** `censorship_cost = runs · dismissed_proof_fee · (W + liability)` with
`runs = min(max_adjudications_per_block, ⌈max_court_work_per_block / worst_court_work⌉)` (`opv.rs:177`) must exceed the claim's
maximum gain at registration. C4 round 3 found cheaper paths around it (F-C4R3-02: laundering a fraud into a default, ≈ 12× cheaper;
F-C4R3-03: the chunk lane) — both fixed on `g14/r4-fixes` ([04](04-gaps-and-findings.md)).

**S7.4 Liability and the beacon fact.** A Final OPV claim can still be convicted until `liability_daa` past Final; a
`FinalReceiptV1` becomes a `WorkFinalEventV1` with `FinalPathV1::PanelIndependent` and nothing else (`opv.rs:402–440`); a convicted or
DA-forfeited Final stops being an eligible source.

**What S7 does not claim.** That any Final is correct (`FinalAssuranceV1::statement`, `opv.rs:337`), or that anyone checks (A-HV).

---

## S8. Effective-bits accounting (OPV-BOOT — design on `opv/bootstrap-beacon` `f2dca0e6d`, §6–7; no code yet)

```text
eff = min_i (r · s_i) − ⌈log2 m⌉ − ⌈log2 (R_a + 1)⌉ − β · ⌈log2 G⌉ − ⌈log2 Q⌉
```

with `s_i` the per-repetition soundness of relation family `i`, `m` the families (union bound), `r` the repetitions, `R_a` the
retry limit, `G` the grinding choices per beacon, `β` beacons per attempt, `Q` adaptive statements; `approved_v1` would require
`eff ≥ security_bits ≥ 128`. The dossier's calculator (`misaka-palw-challenge/src/composition.rs`) reproduces this formula exactly
when given no binding term and complete coverage, and extends it with the binding and selection terms (§5.2). Reviewer notes on the
formula itself:

1. **`G` for today's beacon is not `P(C, k)`.** OPV-BOOT's table bounds output *selection* among live works (`P(32, 2) = 992`); the
   later last-contributor finding (SG-01) shows offline regeneration of a work's identity, so `G` is the adversary's hash budget until
   the sealed-source v3 beacon exists.
2. **`β · ⌈log2 G⌉` is the union over the `G^β` challenge tuples** — sound for any protocol, but for a staged beacon of 30 rounds at
   10 bits it charges 300 bits (row M of §5.5). A round-by-round soundness argument would charge `≈ ⌈log2 G⌉ + ⌈log2 β⌉`; that
   needs a per-protocol proof (Q-09).
3. **`⌈log2 m⌉` and the instance union are conservative**: a committed false statement must pass the check of one fixed false
   instance, so for commit-then-challenge, non-aggregated checks the union is not needed (Q-04).
4. **No selection term**: a policy whose verifier checks a sample of units must add it (§5.4), and then no repetition count helps.
5. **Quantum grinding**: a quantum adversary searching a `G`-sized space for a bad seed gains quadratically (Q-11).
