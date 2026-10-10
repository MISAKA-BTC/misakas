# PESG-B — worst-case fault placement and the detection probability (PESG §4 B, T2)

Lane PESG-B, branch `pesg/detection-bounds` (from integration `0fc729c55`), 2026-10-10. This is the B part of the
[Probabilistic–Economic Security Gate](probabilistic-economic-security-gate.md). It fills in the T1 coverage table for B and gives
the T2 analysis: the probability and cryptography terms. It changes no consensus code, fence, wire id or parameter. The only code
it adds is a test file, `misaka-palw-kernel/tests/pesg_b_detection.rs` (§3, §7).

Verdicts use only **PASS / FAIL / UNKNOWN**. An unknown is never a pass. B judges one thing: `p_check`, the probability that a
capable honest verifier who **runs** the profile's check, on time and with the material, finds the worst fault. B judges neither
`P_run`, `p_evidence`, `p_inclusion` nor `p_collect` (lanes MEAS, codex/G14 and ECON). So a B PASS is never an activation PASS.

## 0. Verdicts per profile

`ε_check` is the checker's soundness: the probability that the worst false claim passes a check that runs. Then
`p_check ≥ 1 − ε_check`. Numbers are for one claim. "Bits" means `−log2 ε_check`.

| Profile | Checking method of the reward-bearing relations (§2) | Worst placement (§3) | `p_check` lower bound (§4) | B verdict |
|---|---|---|---|---|
| K2-TIR-v1 single program, outsider with a private salt | Freivalds (one modulus `2^127 − 1`) on every MatMul; complete exact check on every other node; every delivered token's decode checked | any rank-1 MatMul error (one element, one row, or one column across stacked positions): `1/q` per repetition | `1 − R·2^-252 − 2^-256` (t = 2); **226 bits** at `R = 2^25` | **PASS** (conditional on a whole-claim read; see the note below) |
| K2-TIR-v2 single program (CRT `i128`) | the same, CRT over `2^127−1, 2^107−1, 2^89−1` | a rank-1 error that is a multiple of the two larger moduli: `2^-88` per repetition | **150 bits** at `R = 2^25`; ≥ 128 bits needs `R ≤ 2^47` at t = 2 | **PASS** (conditional, as above) |
| K2-TIR-v3 media pipelines | as v2 per stage, plus the exact edge recompute | as v2 | `1 − Σ_stages R_s·2^-176 − 2^-256` | **PASS** (conditional) |
| K2-TIR-v4 segmented, route A (sample `m` of `P` positions) | random sampled positions, each checked completely | **one self-consistent lie at one position** (any family; the last delivered token) | `m/P` exactly: 9B-8k, `m = 8`: **2^-10** | **FAIL** (counter-example A, §5) |
| K2-TIR-v4 segmented, route B on every claim (re-execute, compare roots) | complete check (hash binding) | none: placement-independent | `1 − 2^-256` (binding only) | **PASS** for `p_check`; it costs one re-execution per claim |
| K2-TIR-v4, route B on a post-commit `q`-sample of claims | sampled claims × complete check | a lie in an unsampled claim | `q·(1 − ε_src)` at best; `q^(R+1)` if a veto re-draws; about 0 on today's v2 beacon | **UNKNOWN** (the claim draw is not wired, §4.6) |
| K2-TIR-v5 encoders and heads (`max_positions = 1`) | complete per-element check of the one position | none (`P = 1`) | `1 − 2^-256` | **PASS** |
| Typed roots: memory, retrieval, composite | memory: as v2 per step plus derived boundaries; retrieval: complete scan; composite: union of its stages | as v2 (memory); none (retrieval) | as v2 (memory); `1 − 2^-256` (retrieval) | **PASS** (conditional) |
| Sampled conformance (S5) | random sampled draws (prompts with replacement, leaves without replacement) | one faulty leaf; one trigger prompt | leaf: `1 − ((N−s)/N)^r`; trigger prompt: about 0 | **FAIL** (sparse faults; the vector-logits relation has no court, T1 FAIL) |
| Complete-check bootstrap conformance | complete check | none | 1 (`ε = 0`) | **PASS** |
| Legacy V2 route (Panel V2, S1/S3, ADR-0077 interval draw) | under T3 collusion: none (the seats' replay); random interval draws (`k = 4` of `N`) and a voluntary watchdog | the last interval, a selection lie | 0 under T3; at most `1 − (1 − k/N)^h` with `h` honest seats (1.34 % at `h = 1`, `N = 299`) | **FAIL** |

**Note on "conditional".** The v1/v2/v3/memory figures assume that the honest verifier reads the whole claim: every position's
values, once each. That is how `OutsiderV1::check_program` works (`ScopeV1::WholeClaim`). Whether such a read finishes inside
`B_cold + B_check` is `P_run`, which MEAS measures. A verifier that samples positions to fit its budget falls into the route-A row:
`p_check = m/P`. At real scale the v2 gate already refuses these classes (9B-8k: 12.6 TB of public bytes). So the v1/v2 PASS
covers only classes small enough to pass that gate.

**T1 outcome for B (§2).** These reward- or gate-bearing relations have the method **none**. Each is a **T1 FAIL** for its family:
- **N1** — conformance vector logits and commit digests have no court (SG-11).
- **N2** — the delivered generation length is bound by no rule (finding F-B1, §6). This affects every generative family.
- **N3** — every relation of the legacy V2 route under T3 collusion.

## 1. Scope and definitions

Fix a profile (class family × verification plan × checking method). The adversary is the producer, every Panel seat and every
related bond acting as one party (PESG §4). It chooses the fault and its placement before the challenge exists. Then it uses every
lever of RFC-0007 Part VI after the commitment: retries, aborts, vetoes, seed grinding, beacon withholding, reorgs, and many
statements.

- **`ε_check`**: the probability that a check of the profile accepts the adversary's best false claim, given that one honest
  verifier runs the check to completion on authenticated public material. It is the checker's soundness, PESG §2's first 128-bit
  target.
- **`p_check = 1 − ε_check`**: B's quantity.
- **`p_convict = P_run · p_check · p_evidence · p_inclusion · p_collect`**: PESG §4 D, combined by conditional bounds and not by
  assuming independence. Not B's. B never writes a number for `P_run`. In particular, B never reads "a check exists" as "a check runs"
  (PRINCIPLES §4).
- **Placement**: the value, element, position, family and structure where the adversary puts its fault, and how the rest of the
  trace is made consistent with it.
- **Never from Monte Carlo.** Every probability in this document is exact by enumeration (small geometry, §3) or a proven bound
  (real scale, §4). The one frequency check in the test file is a sanity check that the sampler reaches every subset, and it is
  labelled as such.

## 2. Coverage table (T1 input for B)

Methods: **S** = random sampled interval/unit; **A** = Freivalds or algebraic; **G** = GKR/sum-check; **C** = complete check;
**none**. "Who" is the party whose check carries `p_check` when the producer and every seat collude (T3): **O** = the Panel-outside
public verifier, using its private salt; **W** = a drawn or voluntary watcher.

### 2.1 K2-TIR-v1 / v2 single-program claims (`ClaimBodyV1::Program`)

The plan checker (`check_plan_v1`, `check.rs:126–214`) makes coverage code-derived. Every node of every scheduled occurrence has
exactly one relation, with no omission, no duplicate and no weaker checker. Lemma L-COV then turns "every relation holds" into
"the trace is the reference execution". The outsider checks `ScopeV1::WholeClaim` (`ledger.rs` `check_program`).

| Relation | Method | Who | Code |
|---|---|---|---|
| MatMul (dense, attention `Q·Kᵀ`/`P·V`, gathered experts) | **A** — v1: Freivalds mod `2^127 − 1`; v2: CRT Freivalds; batched per weight across a scope, per instance otherwise; every row (right side) or column (left side, GEMV) compared | O | `verify.rs` `freivalds_once`, `check_instance` |
| Structure, exact arithmetic, quantization/rounding (`Div`, `Clamp`, `Log2Floor`, `Cast`), nonlinear (`IntExp`, `IntRsqrt`, `IntLn`), selection (`Compare`, `Select`, `TopK`, `ReduceMax`, `Gather`), routing | **C** (exact recompute) | O | `check_instance` `ExactRecompute` |
| `Fixed` state (`StateWrite`), `Hist` append and derived windows/views | **C** (state continuity; derived values rebuilt from the committed rows) | O | `StateContinuity`; `is_derived` path |
| Value types (dtype, shape) | **C** (`well_typed`, `Malformed`) | O | `verify.rs` |
| Inputs: tokens, params against the registered commitments, consts, initial state | **C** (bound in the evidence; params authenticated at every opening) | O | `shape_and_binding`, `check_evidence_v1` |
| Segment entry/exit state roots | **C** (recomputed from the commitments) | chain at inclusion | `evidence.rs:184` |
| Each delivered token (greedy decode) | **C** | O | `check_program` |
| Generation length (`1 ≤ len ≤ max_new_tokens`, fixed reward) | **none** — any length is accepted (F-B1) | — | `job.rs:153` |
| Model-weight binding (param commitments ↔ artifact root) | **C**, refutable by a verifier that holds the bytes (tag 105); conditional on acquisition (ADR-0177) | O | readiness matrix, GAP 1 withdrawn |
| Interim Panel seats (PanelLicensed) | **A + C** on `WholeClaim`, but with **public coins grindable by the claim id** (`palw_kernel_interim_seed_v1`) | seats | not soundness-bearing (SG-05) |

### 2.2 K2-TIR-v3 pipelines (`ClaimBodyV1::Pipeline`)

Each stage is checked like §2.1 (`FreshPipelineVerifierV1::check_salted`, `WholeClaim` per stage). Stage edges (`EdgeRecompute`:
job values, earlier stages' committed rows, RFC-0003's `R` from `random_binding`) are **C**. The stream decode is **C**.
Generation length is **none** (F-B1). RFC-0003's `R` is a public function of the job's seed. That makes it a generation-randomness
question, not a detection gap: the relation `R = H(seed, item)` is checked exactly.

### 2.3 K2-TIR-v4 segmented (`ClaimBodyV1::Segmented`)

| Relation | Method | Who | Code |
|---|---|---|---|
| Every committed value of a checked position (every family, derived windows included) | **C** per position (element evaluator); a mismatch gives one element court | O | `element::check_positions_v1` |
| Which positions are checked | route A: **S** (the verifier's choice of `m` of `P`); route B: **C** (re-execute every position, compare the roots) | O/W | `seg_detect::check_claim_by_reexecution_v1` |
| Decode at a selecting position | **C** at the checked positions (route A); every position (route B) | O | `element.rs:1021` |
| Cross-segment continuity | **C** (by wiring: position `p` reads `p − 1`'s committed values; no separate boundary statement) | O | K2S §1.3 |
| Sublinear-read aggregate check (Freivalds/GKR over a polynomial commitment) | **none exists** (DESIGN_GAP, K2S §11.4) | — | — |
| Generation length | **none** (F-B1) | — | — |

### 2.4 K2-TIR-v5 encoders and heads

One position (`max_positions = 1`): **C** of every element of that position (route A at `m = P = 1` is the whole claim; route B is
one forward pass). There is no delivered decision on chain (`HEAD_DECODE_V1` has no court, GAP-40), so no reward-bearing decision
relation exists to cover.

### 2.5 Typed roots (`ClaimBodyV1::Spec`, `spec::outsider::SpecOutsiderV1`)

| Kind | Relations | Method |
|---|---|---|
| memory | each step's program (as §2.1, `WholeClaim` over the step's overlay); boundary states derived (`boundary_states_v1`); each step's decode | **A + C**; **C**; **C** |
| retrieval | every returned entry re-opened and re-scored; a scan of the whole snapshot for a better excluded item | **C** (`SnapshotDataV1::find_fault`) |
| composite | `StageLogits` edges; each stage by its component's check | **C**; as its components |

### 2.6 Conformance (RFC-0011 §17, OB-P0)

| Relation | Method | Court |
|---|---|---|
| sampled leaf decode (`leaves_per_repetition` of `N` leaves, without replacement per repetition) | **S** | `LeafDecode` refutation |
| sampled vector tokens (prompts with replacement) | **S** | `VectorTokens` via a Final kernel claim |
| sampled vector **logits / commit digests** | **S** | **none** (SG-11) → **N1** |
| complete-check bootstrap (`PostComplete`, tag 109) | **C** in the fold (every input, every leaf) | not needed (the fold computed it) |

### 2.7 Legacy V2 route (Panel V2: ADR-0133 S1/S3, ADR-0077 Decision 8, ADR-0098)

| Relation | Method | Who |
|---|---|---|
| every step leaf | full replay by the drawn full seat; one partial seat per segment | seats only |
| sampled intervals | **S**: `k = 4` distinct intervals per seat (`palw_fp_interval_draw_v1`; `word % N`, slightly biased; seeded by the anchor block below `palw_panel_seed_execution`, so the anchor producer can grind it) | seats |
| layer/position sites (S3) | **S**: 8 sites (`palw_layer_sample_v3`; layer and position share draw bits 16–63; capped at 4,096 attempts) | seats |
| a bonded watchdog's full re-run | **C** if anyone runs it (voluntary, unpaid) | W |

Under T3 (every seat colluding) the seats' checks contribute nothing. What remains is a voluntary watchdog. So for T1 the method
of every relation under T3 is **none** (**N3**).

## 3. Exhaustive small-geometry search

`misaka-palw-kernel/tests/pesg_b_detection.rs` (11 tests). The fixture is RFC-0007 Part II's dense + MoE + history class
(`dense_moe_v1(7)`), with MatMul, quantization, the integer nonlinears, TopK routing, `Gather` embeddings, `Fixed` and `Hist`
state, and derived windows. The claim has `P = 5` positions. Every check and every court is the crate's real code.

**Fault placement.** Every committed value of every position is changed at its first and at its last element. Each change is
made two ways:
- **isolated**: one committed value changes. Its relation and its consumers' relations are false.
- **self-consistent**: the adversary's tracer computes everything after the lie from the lie, so **exactly one relation is
  false**. This is the adversary's best placement.

For each placement the test measures `D`, the set of single positions whose `AuditOnly([q])` check detects the lie and whose filing
the court convicts. Then:
1. `WholeClaim` detects every placement and the court convicts it. This is complete coverage: miss 0.
2. For every sample size `s`, the miss is computed by enumerating all `C(P, s)` samples. It equals `C(P − |D|, s) / C(P, s)`.
3. The real verifier's result on every segment scope equals "the segment meets `D`".
4. On **every one** of the 31 subsets of positions, the real verifier's detection equals `S ∩ D ≠ ∅`. This is checked for one
   placement of every family, in both modes. So the closed form over single-position detection sets is the real verifier's
   behaviour, not an assumption.

**Measured** (release build, all 11 tests pass; the exhaustive six are `#[ignore]`d and run with `--ignored`):

| Search | Placements (values × {first, last element}) per mode | Result |
|---|---|---|
| K2-TIR-v1, every family | dense-matrix 185, exact-arithmetic 1,140, quant-range 1,295, nonlinear 130, selection 255 (252 self-consistent; 3 have no consistent continuation: a later primitive refuses the lie), recurrent-state 20, structure 545 = 3,570 | `WholeClaim` detected and convicted all 7,137; `\|D\|` = 1 for every self-consistent placement and for every isolated one except some structure values (`\|D\|` up to 5); the worst miss is `(P − s)/P` for every family: 4/5, 6/10, 4/10, 1/5, 0 at `s` = 1…5 |
| K2-TIR-v2, MatMul (CRT checker) | 185 | the same: `\|D\|` = 1, worst miss `(P − s)/P` |
| union property (real verifier on all 31 subsets) | one placement of each family × 2 modes | 496 scope checks, every one equal to `S ∩ D ≠ ∅` |
| segment scopes (3 segments of 2, 2, 1 positions) | every placement above | every verdict equal to "the segment meets `D`" |
| K2-TIR-v4 route A | the same 3,570 (route A per position) | `\|D\|` min 1 in every family and both modes: miss `1 − m/P` |
| K2-TIR-v4 route B | the same | every placement found and convicted (`verify_seg_fault_v1`) |
| last delivered token (v4) | decode lie over honest values | route A: seen by the last position only; route B: `first_decode_mismatch_v1 = 4`, convicted |
| substituted weight multiplied at every position | 13 weights | `D` = all 5 positions |
| borrowed trace | prefix-sharing job (last token differs) / unrelated job | `D` = {4} / all 5 |
| withholding position `w` | `w` = 0…4 | seen (DA path, never a pass, never a conviction) by positions `w…4`: a later position's history window reads `w`'s rows, so the worst is withholding the **last** position (seen by one) |
| false disclosure (served bytes ≠ commitment) | every non-derived value at position 2 | `Unavailable` on `WholeClaim`: never a pass, never a conviction |
| Freivalds exhaustive | GF(7) 4,095 error patterns × 49 vectors; GF(31) 4,095 × 29,791; right and left variants | passes = `p^(n − rank)` in every case; worst non-alias miss `1/p`; aliasing patterns pass all vectors |
| CRT exhaustive | `e ∈ [−434, 434]` × all 217 challenge pairs | miss = `Π_q (q ∣ e ? 1 : 1/q)`; ≤ 1/7 below 217; 1 at ±217, ±434 |

**Worst placement per method:**

| Method | Worst placement | Exact miss in the fixture | Closed form |
|---|---|---|---|
| `WholeClaim` (v1/v2 outsider) | none; every placement is found and convicted | 0 (plus the algebraic residual below) | `ε_alg` only |
| `s`-of-`P` positions (`AuditOnly`; v4 route A) | a self-consistent lie at one position, in any family; a last-token decode lie; a borrowed trace of a job that differs only in the last token; withholding the last position | `(P − s)/P` | `1 − s/P` |
| one segment of a Panel's partial seat | a self-consistent lie in a segment the seat does not hold | `1 − |seg|/P` | — |
| a substituted weight that every position multiplies by (a whole-relation operand fault) | none; every position detects it | 0 for every `s ≥ 1` | — |
| v4 route B | none; every placement is found with at most `⌈log2 S⌉` probes and convicted | 0 | binding only |
| Freivalds, one modulus (toy GF(7), GF(31)) | any rank-1 error | `1/q` | `q^-rank` |
| Freivalds, error ≡ 0 mod q (aliasing) | an integer error that is a multiple of `q` | 1 | refused at admission (`span < q`) |
| CRT (toy moduli 7, 31) | an error that is a multiple of the larger modulus | `1/7` (the smaller modulus) | `Π_q (q ∣ e ? 1 : 1/q)` |
| Freivalds with `G` adversary choices | — | `1 − (1 − 1/q)^G` | `≤ G/q` |
| sampled check with `G` adversary choices | — | `1 − (s/P)^G` | — |
| two watchers on one seed / on two private salts | — | `1 − s/P` / `(1 − s/P)^2` | — |
| a claim draw (`q`) with `R` vetoes that re-draw | — | detection `q^(R+1)` | — |

**Algebraic residual, exhaustive.** Every error matrix over the alphabet `{0, ±1, p}` of shape 3 × 2 (GF(7): three stacked
positions of one batched weight product), and `{0, 1, −2, 31}` of shape 2 × 3 (GF(31)), is tested against **every** challenge
vector (49 and 29,791 of them). This is done for the right-projection check, which compares every row like the kernel's
`first_bad_row`, and for the left (GEMV) check, which compares every column. In every case the number of passing vectors is exactly
`p^(n − rank_p(E))`. So the miss is `p^-rank`. The worst placement is any rank-1 error, whether one element, one row, or one column
spread across any number of stacked positions: `1/p`. An error made of multiples of `p` passes every vector. That is why admission
refuses any MatMul whose span reaches the modulus (`check.rs:143–146`, S2). The field arithmetic is the kernel's own
(`Mersenne<E>`).

**Samplers.** The kernel's field sampler `FieldElemV1::of_word` (behind `ChallengeStreamV1::next_in`) maps the low `E` bits and
rejects only `p`. Tested exhaustively over every low pattern for `E ∈ {2, 3, 5, 7}` under three high-bit patterns: every element
has exactly one preimage, so it is exactly uniform given uniform words. The challenge contract's `distinct_indices` (Floyd's
algorithm over the rejection sampler `index_below`, exact by S6.4's proof) reaches every `s`-subset of the five positions, so no
position is a blind spot. The legacy samplers (`palw_fp_interval_draw_v1`, `palw_layer_sample_v3`) use `word % N`. Their bias is at
most `N/2^64` per draw, and S3's layer and position share draw bits, with a total-variation distance of about `L/2^16`. That is
negligible next to their selection term. They are recorded as not exact (§2.7), not tested.

## 4. Real-scale bounds

### 4.1 Composition

By a union bound, which needs no independence:

```text
ε_check ≤ ε_cov + ε_sel + ε_alg + ε_grind + ε_src + ε_reorg + ε_bind
```

| Term | Value | Where it comes from |
|---|---|---|
| `ε_cov` | 0 if L-COV holds (code-derived by `check_plan_v1`); **1** if any reward-bearing relation has method none (§2: N1–N3) | coverage |
| `ε_sel` | `(N − s)/N` for `s`-of-`N` uniform units without replacement, against the worst placement (one self-consistent unit). For `r` independent repetitions: `((N − s)/N)^r`. **No repetition count, field size or modulus changes it.** | §3, exact |
| `ε_alg` | per false instance: `q^-t` (one modulus), `q_min^-t` (CRT, the smallest modulus used). The kernel's accounting takes a union over `R` probabilistic instances: `R·q^-t` (conservative, SG-08). | §3 exhaustive; S1/S2 |
| `ε_grind` | public coins only. Algebraic: `ε_alg·Q·(R_a + 1)·G^β`. Sampled: `1 − (1 − ε_sel)^(G·(R_a+1)·F)`, exact for independent draws (§3). The union bound `G·ε_sel` is vacuous here: two draws already halve a 2-of-4 check. Private coins (the outsider's salt): `G = β = 1`, no retry against the verifier. | §3; dossier §5.3 |
| `ε_src` | v3 sealed source: `ρ^((W − δ)·b)`; interim `W = 40, δ = 10, b = 1, ρ = ½` gives **2^-30**. 128 bits need `W − δ ≥ 128` blocks at `ρ = ½`. v2 accumulator: unbounded (last contributor, SG-01). When the source fails, the seed is adversarial and a sampled check's miss is 1. | OPVB §6.3 |
| withheld/veto retry | v3: a veto ends the attempt as a counted retry, so `R_a + 1` attempts. A veto that re-draws a **claim** draw turns detection into `q^(R+1)` (§3). | §4.6 |
| `ε_reorg` | each canonical branch is one more draw: a factor `F` inside `ε_grind`; `F = 1` under the settlement assumption A-B4 (FINX) | A-B4 |
| correlation | watchers that derive from **one seed** count as **one** check (§3: the second adds nothing). Private salts are independent in their coins, but their **existence** is common-mode (A-ACQ). B does not claim `(1 − p)^k` for `k` watchers. | §3; K2S §11.3 |
| `ε_bind` | `2^-256` classical (BLAKE2b-512 collision); about `2^-170` against a quantum adversary (A-H2, Q-11) | binding |

`p_check ≥ 1 − ε_check`. Every logarithm is rounded against the claim.

### 4.2 Algebraic profiles (v1, v2, v3, memory)

Take an outsider with a private salt over `WholeClaim`: `ε_sel = ε_grind = ε_src = ε_reorg = 0`.

```text
v1:  ε ≤ R·2^(−126·t) + 2^-256    t = 2, R = 2^25 (9B-8k instance count):  2^-226   (dossier row A)
v2:  ε ≤ R·2^(−88·t)  + 2^-256    t = 2, R = 2^25:                         2^-150   (row B);   128 bits need R ≤ 2^47
     t = 3:  2^-238                                                                              (row E with no loss)
```

Q statements over a horizon multiply the expected number of false acceptances network-wide, not the per-claim bound. Against a
**public-coin** seat path with `h` bits of seed grinding, the bound is `227 − h` (v1) or `151 − 2 − h − ⌈log2 Q⌉` (v2). Today the
interim seats' seed is the claim id, so `h` is the adversary's offline hash budget. That path is **UNKNOWN**, and G14 does not count
it (SG-05).

Feasibility: these bounds assume a whole-claim read. At 9B-8k that read is 12.6 TB (v2) and the v2 gate refuses the class, so the
algebraic profiles cover only gate-sized classes. The batched Freivalds check reads every operand: `O(P·(d_in + d_out))` per weight
relation plus the weight once. It is cheaper in compute than replay, but not sublinear in the claim's material.

### 4.3 Sampled-unit profiles (v4 route A, legacy intervals, conformance)

```text
v4 route A, m of P positions:          p_check = m/P              9B-8k (P = 8,192):  m = 8 → 2^-10;  m = 819 → 0.1;  m = 4,096 → ½
  128 bits:                            m = P (the 19.2 TB whole read)
  with G adversary choices (public):   p_check = (m/P)^G
legacy V2 seats, k = 4 of N = 299:     1 − (1 − k/N)^h   (h honest seats):  1.34 % (h = 1), 6.51 % (h = 5), 0 (T3)
conformance leaf, s of N, r reps:      1 − ((N − s)/N)^r      a trigger prompt: 1 − (1 − 1/|Π|)^(v·r) ≈ 0,  |Π| = Σ_{l ≤ L} T^l
```

For conformance, B uses the fault model the adversary chooses (sparse), not `A-FAULT`'s density `f`. Under the scope-v1 density
`(½, 1/16)` the bound is the dossier's row J (128 bits at 211 vectors + 1,686 leaves). Under a sparse fault, no reachable
scope gives one bit (Q-06). Conformance therefore certifies "implementation agreement on a uniform sample" only.

### 4.4 Complete profiles (v4 route B on every claim, v5, retrieval, complete-check bootstrap)

`ε_check = ε_bind` (plus 0 for the exact families and the decode rule). Route B reads `A + ≤ 10 position paths + 2 positions` and
costs one re-execution. v5's re-execution is one forward pass, which is cheap enough for every claim. B's verdict is **PASS** for
`p_check`. Whether someone runs it on every claim is `P_run`.

### 4.5 `p_check` lower bounds, summarised

See the table in §0. For ECON the figure to carry is `p_check` itself. **`p_min = P_run · p_check · …` is not B's**, and B
supplies no `P_run`.

### 4.6 The post-commit claim draw (K2S §11.3) — what it needs before it can be a PASS

The draw is not wired (CODE, OPVB). Before any `q`-sampled profile can pass, these hold:

1. **A veto must not re-draw.** If an attempt whose beacon the producer vetoes (a withheld seal, an abandoned source) is simply
   retried, the producer escapes unless every attempt draws "checked": `p_check = q^(R+1)` (enumerated in §3). The rule must be one
   of these:
   - a vetoed claim draw counts as **checked** (`q = 1` for that claim);
   - the claim defaults.
2. **A sealed source with `ε_src ≤ 2^-128`**: `W − δ ≥ 128` blocks at `ρ = ½`. The interim `W = 40` gives 30 bits. On today's v2
   accumulator a last contributor steers the draw to "unchecked" with an expected `1/(1 − q)` offline tries: `p_check ≈ 0`.
3. **Watchers drawn by one seed are one check** (correlation), and a drawn watcher's model acquisition is common-mode (A-ACQ, MEAS).

## 5. Counter-example A (PESG §5)

**Shown in the fixture** (`pesg_b_operand_and_borrowed_trace_placements` and `pesg_b_v1_every_fault…`):
- A weight perturbed in a relation that every position computes (a whole-relation operand fault) is detected by **every** position:
  `|D| = P`, so any one sample catches it.
- The same weight error confined to one self-consistent site (one element at one position) is detected by **one** position:
  `|D| = 1`, so `p = s/P`.
- A borrowed trace of another job that differs only in the last token is likewise detected by the last position only.

Moving the fault collapses detection from 1 to `s/P`. At 9B-8k with `m = 8` that is from 1 to `2^-10`.

**Does an algebraic whole-relation check cover it?**

| Profile | Cover | Verdict on counter-example A | Collateral multiplier B implies |
|---|---|---|---|
| v1/v2/v3/memory with a whole-claim outsider | yes: batched Freivalds over every row of every position; placement-independent miss `≤ 1/q` per repetition (§3, exhaustive) | **PASS** | about `1 + ε` (nothing beyond the other factors) |
| v4 route A | **no**: hash-tree commitments localize and adjudicate but do not aggregate; a sublinear-read proof is a DESIGN_GAP (K2S §11.4) | **FAIL** | `L/G > (1 − p)/p` with `p = m/P`: **> 1,023×** at `m/P = 1/1,024`; > 9× at `m = 819`; still divided by `P_run·p_evidence·p_inclusion·p_collect` and grossed up for the 49 % self-recoup (ECON) |
| v4 route B on every claim | complete check instead of algebra | **PASS** for `p_check` | about 1 (the cost moves to one re-execution per claim) |
| v4 route B, `q`-sampled | the claim draw, as §4.6 | **UNKNOWN** | `(1 − q)/q`: 15× at `q = 1/16`, 63× at `q = 1/64`, before `P_run` |
| v5 | `P = 1`: a complete check | **PASS** | about 1 |
| sampled conformance | no | **FAIL** | not applicable (a gate, not a reward) |
| legacy V2 | no | **FAIL** | unbounded under T3 (`p = 0`) |

## 6. Findings

- **F-B1 (T1, P1 for rewards) — the generation length is a reward-bearing quantity with no relation.** `job.rs:153` accepts any
  `1 ≤ len ≤ max_new_tokens`. There is no stop/EOS rule. `claim_reward` and `work_credit_per_claim` are fixed per claim (`opv.rs:156`).
  So a producer that delivers one token earns the same as one that delivers `max_new_tokens`. Every delivered token is correct, so
  no court can convict this, yet the paid useful work is unbound. This affects v1–v4 and the pipeline stream. Fix options (a design
  decision):
  - define the length by the job: exactly `max_new_tokens`, or a committed stop rule checked like the decode;
  - make reward and credit proportional to the verified positions.
- **F-B2 (T2) — the claim draw's veto semantics decide whether `q`-sampling has any power** (§4.6): re-drawing after a veto gives
  `q^(R+1)`.
- **F-B3 (T2, accounting) — never charge grinding of a sampled check by a union bound.** The exact escape is
  `1 − (1 − ε_sel)^G`. For `ε_sel` close to 1, a "`G·ε_sel`" figure is meaningless. Any effective-bits figure for a sampled scope
  must be computed with the exact form. `effective_false_accept_bits_v1` charges `⌈log2 G⌉` against `−log2 ε`. That is correct for
  small `ε` (algebraic) but gives no usable figure when the selection term dominates. The dossier's row G (−2 bits) already shows
  this.
- **F-B4 — the interim Panel seats' public coins** (claim id) are grindable. The `WholeClaim` check is complete in scope, so with the
  126-bit field grinding costs `2^126` hashes per success (v1). At v2's 88 bits it costs `2^88` per repetition. The path is not
  soundness-bearing (SG-05) and stays UNKNOWN.
- **F-B5 — the legacy samplers are not exact** (modulo bias; S3's shared bits; a silent 4,096-attempt cap that can return fewer
  sites). The effect is negligible, but the samplers are not reviewed. The legacy route is FAIL anyway.
- **F-B6 — sampled conformance has no power against sparse faults** (§4.3). A conformance PASS must never be read as model fidelity
  or per-claim correctness (it is not, by statement, `palw_conformance_evidence_v1.rs:197`).

## 7. Tests and reproduction

```text
BUILDSLOT_SINCE=… CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh \
  cargo test --offline -p misaka-palw-kernel --test pesg_b_detection --no-run
(cd misaka-palw-kernel && ../target/release/deps/pesg_b_detection-<hash> --ignored --nocapture)   # add --release to the build
```

| Test | Ignored? | Release time |
|---|---|---|
| `pesg_b_v1_every_fault_in_every_position_against_every_sample` | yes | 1,553 s |
| `pesg_b_v2_every_matmul_fault_in_every_position` | yes | 91 s |
| `pesg_b_v4_route_a_samples_and_route_b_reexecution` | yes | 452 s |
| `pesg_b_a_scope_detects_exactly_when_one_of_its_positions_does` | yes | 14 s |
| `pesg_b_withholding_and_false_disclosure_are_the_da_path_and_seen_only_where_read` | yes | 12 s |
| `pesg_b_freivalds_miss_is_exactly_p_to_the_minus_rank_for_every_error_placement` | yes | 17 s |
| `pesg_b_operand_and_borrowed_trace_placements` | no | 1.7 s |
| `pesg_b_crt_miss_is_the_smallest_moduluss_below_the_product` | no | < 0.1 s |
| `pesg_b_the_field_sampler_is_exactly_uniform_and_the_streams_reach_every_element` | no | < 0.1 s |
| `pesg_b_the_subset_sampler_reaches_every_subset` | no | < 0.1 s |
| `pesg_b_grinding_retries_correlation_and_vetoes_enumerated` | no | < 0.1 s |

The exhaustive six take hours in a debug build, so they are ignored by default. Run them with
`cargo test --offline --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored`.

## 8. What B hands on, and what it needs

- **ECON** (A/E): use `p_check` from §0 and the multipliers of §5. Never treat `p_check` as `p_min`.
- **MEAS** (D): `P_run` for whole-claim reads (v1/v2/v3/memory) and for route B (one re-execution) per class. Each conditional
  PASS in §0 depends on it.
- **OPVB/CODE**: the claim draw (§4.6, its three conditions), F-B1's length rule.
- **External review**: the S1/S2 statements, L-COV, and the exact forms in §4.1. Nothing here approves a policy. The approval
  registry stays empty.
