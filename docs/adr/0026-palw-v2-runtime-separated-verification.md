# ADR-0026: PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof

Status: **Accepted (architecture).** Operating envelope unchanged: devnet, shadow mode,
consensus-visible zero-credit only. Every activation gate in the v2 design §12 stands; this ADR
moves none of them.
Date: 2026-08-15
Relates to: ADR-0007 (layered PoW), ADR-0021 (superseded PALW PoW record — kept as history),
ADR-0024 (verified-LLM token-weighted BFT),
[`palw-full-logits-trace-v2-design.md`](../palw-full-logits-trace-v2-design.md) (operative
safety model), [`ambient-pol-binary-audit-2026-08-15.md`](../ambient-pol-binary-audit-2026-08-15.md)
(the evidence this ADR rests on), the detailed / VPS-canonical-worker / secure-OTA design set,
`consensus/core/src/palw_v2.rs` (frozen preimages).

## Thesis

**Adopt Ambient's implementation architecture; do not adopt Ambient's proof-security model.** The
runtime/verification separation, the multi-runtime adapter surface, the Merkle-commit →
post-commit-challenge → recompute flow, asynchronous verification and bond/slashing are the right
shape and MISAKA takes them. The parts that make Ambient's *proof* weaker than PALW needs — a
tolerant (RBO / p95-band) comparison, single-token-as-sufficient framing, logits-only-when-final
proof material, LStake-only validator selection, and a runtime-embedded closed verifier — MISAKA
replaces with an exact-within-pinned-class comparison over a deeper trace (logits + activations +
GEMM), a dynamically-sized challenge count, a diversified validator set, and an open kernel.

## Context

### Where the v2 scheme stands

The PALW v2 execution scheme (informally "algo 2"; canonically `PALW_EXECUTION_ALGO_ID_V2`, a
PALW-internal namespace value that is **not** the header `pow_algo_id = 2`) commits one pinned-LLM
execution to a `full_logits_sequence_root`: each decode step's full logits vector
(n_vocab = 248 320, f32) becomes a `job_context_hash`-bound event hash, the ordered events a
domain-separated Merkle root, and that root a keyed outer hash binding job, network, runtime,
class and token budget. Implemented so far, all consensus-inert: the worker's `v2-job` path
(token-ID envelopes, exact decode, fail-closed non-finite handling, build-measured
`RuntimeManifestV2`), golden-vector boot gating, `misaka-palw-agent` Phase A, kaspad
`--compute-endpoint`, and `PalwCapabilityDeclarationV2` (v2 design §16).

The honesty baseline (v2 design §4) is load-bearing everywhere below: a trace root is an **audited
commitment, not a cryptographic proof**. Any claimant can announce an arbitrary root; only
canonical replay by independent bonded verifiers, plus bonds/slashing/challenge-windows/maturity,
makes a false one costly. Acceptance security is economic, and reduces to `P_detect · S > G`.

### Ambient, now measured (not inferred)

The `ambient-xyz/llama.cpp-ambient-bin` `macos-arm64` binary was fetched via Git-LFS and audited
(strings + exports, diffed against upstream llama.cpp; see the companion audit doc). This replaces
the earlier survey's inferences with facts:

- **Verification runs inside the llama.cpp fork.** The Ambient `llama-server` exposes
  `/ambient/v1/inference/verify` and `/…/token-data`, does the recompute, and checks the Merkle
  proof itself. The compute path is source-unreleased but **runtime-embedded** — a verifier must
  run Ambient's build. (Survey hypothesis C confirmed; B refuted for the compute path.)
- **The comparison is tolerant, not exact.** Acceptance is governed by `logit_min_prob` (score
  only logits above a softmax floor), `logit_min_rbo_score` (a Rank-Biased-Overlap ranking-
  similarity score on top logits), `mlp_output_p95_abs_diff` (a p95 absolute-difference *band* on
  MLP-layer outputs), and `num_mlp_failure_threshold` (a bounded per-layer failure allowance).
  Ambient absorbs hardware FP divergence in the *comparison*.
- **Ambient already captures intermediate activations.** `ambient_capture_layers` and the MLP-
  output metric mean the proof material spans logits **and** selected MLP-layer activations.
- **The challenge is teacher-forced.** `ambient_forced_tokens` fixes the token sequence so the
  verifier recomputes a challenged position without sampling drift.
- **Merkle + HDF5 + SGLang-compatible capture.** The proof is a Merkle commitment; captured
  tensors travel as HDF5 with SGLang-compatible export, so the *capture format* is runtime-
  portable even though the *comparison code* is not. On-chain `VerificationState`
  (`merkle_root`, `assigned_verifiers`, `assigned_verifiers_token_ranges`, `verified_tokens`,
  `output_hash`) maps onto the server's per-request `start_token`/`end_token` window and verdict
  counters.

Two facts reframe the earlier ADR draft. First, Ambient and MISAKA give **opposite answers to the
same problem** (hardware FP divergence): Ambient fuzzes the comparison; MISAKA pins the class and
compares exactly. Second, Ambient's tolerant comparison is only safe *because* their verification
is asynchronous and optimistic — off the chain-validity path, where a fuzzy verdict triggers a
probabilistic slash rather than a fork. That coupling — comparison strictness ↔ where the verdict
is consumed — is the hinge of this ADR.

## Decision

### 1. Borrow Ambient's architecture; keep the scheme kernel out of the runtime

Adopt, on evidence: (a) **runtime/verification separation** — vLLM / llama.cpp / TensorRT-LLM /
ROCm behind a Runtime Adapter API, none of them containing consensus logic; (b) a **runtime-
portable capture format** so a vLLM or SGLang miner and a llama.cpp verifier interoperate on the
same captured tensors (Ambient's HDF5/SGLang-export precedent); (c) **Merkle commitment**;
(d) **post-commit random challenge**; (e) **asynchronous verification** off the block-production
path; (f) **bond + slashing**; (g) pinned **model/runtime/tokenizer profiles**.

But where Ambient compiles the verifier into its llama.cpp fork, MISAKA does not. Every domain key
(`misaka-palw/<name>/v2`), preimage and canonical encoding is single-sourced in
`consensus/core/src/palw_v2.rs`, golden-frozen, runtime-independent; runtimes are replaceable
adapters *beneath* the kernel. Ambient's own build is the cautionary case: a consensus-critical
comparison living inside an inference runtime makes "the runtime updated" a consensus event — the
blockchain equivalent of `npm update` toppling the chain. The three layers are frozen:

```
runtime adapters (vLLM / llama.cpp / TensorRT / …)   ← replaceable, no consensus logic
        │  per-decode logits + selected activations + GEMM trace
        ▼
PALW Canonicalizer (CanonicalLogitsV1)               ← §3, deterministic integer form
        ▼
PALW scheme kernel (palw_v2.rs, golden-frozen)       ← single source of preimages/domains
        ▼
node: capability / certificate / challenge / slash   ← consumes verdicts, computes none
```

### 2. Prove deeper than logits — logits + activations + GEMM trace

Ambient treats logits as the computation's fingerprint and (via MLP capture) reaches one layer
deeper. PALW commits deeper still, because the property MISAKA must enforce is not "the claimant
knows the right logits" but "the claimant actually performed the pinned, high-cost computation."
Hashing a final result — `proof = H(Model(x))` — only binds knowledge of the result; the v2 scheme
already moved past this, and this ADR fixes the target as a per-token tuple:

```
T_i = H( Canonicalize(final_logits_i)
       ‖ Project(activation_i @ pinned layers)      // e.g. L8, L16, L24, L32
       ‖ Project(gemm_trace_i @ pinned ops) )
TokenRoot = MerkleRoot(T_0 … T_n)                    // domain-separated, per palw_v2.rs
```

The GEMM-trace leg is the leg Ambient does not have and is where PALW is strictly stronger:
a challenge can address `(token, layer, matrix-tile)`, so predicting final logits alone is
insufficient. Which layers/ops are pinned is a `shape_profile_id`-bound constant; changing it is a
new scheme version. (This is the `full_logits_sequence_root` ambition of the detailed design made
explicit as the challenge surface; the current Land code commits logits events — activations and
GEMM legs are staged additions under the same Merkle discipline, not a new mechanism.)

### 3. Exactness inside a pinned class — never a tolerance in the slashing verdict

This is the deliberate inverse of Ambient's RBO/p95 model, and it is forced by *where PALW's
verdict is consumed*. A fuzzy comparison means `Validator A → pass, Validator B → fail` on the
same input; Ambient survives that because a fuzzy verdict there is a probabilistic slash off the
critical path. PALW's slashing quorum must be **deterministic** — every honest verifier of a class
must reach the identical verdict — or the slash itself forks. Therefore:

- **A hardware/software backend is a determinism class.** Measured: Metal root `ba3b9994…` ≠ CPU
  root `d04672dc…` (same job); F16 x86 agreed 8/8 seeds but arm-vs-x86 only 7/8, the miss an
  argmax flip on a prefill batch-GEMM **near-tie**; the registry Q8_0 artifact split even EPYC vs
  Broadwell (4/8). Divergence concentrates at near-ties — exactly where any ε-band is undecidable,
  and exactly what RBO is built to paper over.
- **Within a class: full 64-byte equality.** Across classes: **no comparison** — a cross-class
  mismatch refutes nothing (same output, different trace is the expected physics). A replay refutes
  only within the declared `runtime_class_id`.
- Divergence is absorbed in **class membership** (exact pinned artifact), not in the comparator.
  Class membership is manifest-hash exact, never a label — the golden-set gate that compared a
  class *label* let a different build pass (`f9ab6ab`), and a class is a claim about *pairs* of
  hosts, so nothing with fewer than two measured sides is a class.
- A tolerant comparator is not banned from the system — it is banned from the **binding verdict**.
  A soft cross-class RBO/p95 signal may run in a *diagnostic, non-slashing* audit tier (anomaly
  detection, class-drift alarms). It never mints, never slashes, never gates a block. Any proposal
  to make ε binding is a new `trace_scheme_id`, a new ADR, and re-activation from zero-credit.

CanonicalLogitsV1 (v2 design §3, §8) is what makes within-class exactness reachable: hash the
canonical little-endian integer/byte form under a pinned FP environment (RNE, no FTZ/DAZ drift, no
fast-math, no FMA contraction) — `hash(canonical_tensor)`, never `hash(raw_float_tensor)`.

> **Amended by ADR-0027 §2:** the class definition is upgraded from pairwise agreement to
> conformance against a canonical reference implementation (soft-float/integer), so disputes are
> adjudicable by any node without class membership. Exactness itself is also re-grounded: under
> the no-BFT premise a tolerance verdict could only be settled by a vote, so exact-within-class is
> forced, not merely preferred.

### 4. Commit → post-commit challenge → recompute → quorum (the flow, made unpredictable)

Adopt Ambient's central move — decide *what to check after the commitment is fixed* — and bind the
selection to future chain randomness so the miner cannot know the audited positions at commit time:

```
C  = MerkleRoot(TokenRoot, activation_root, trace_root)          // published at commit
R  = H( C ‖ future_DAG_block_hash ‖ epoch_randomness )           // unknowable at commit
challenge_set = PRF(R) → { (token, layer, tile) … }              // q positions, §6
verifier: teacher-force the committed token IDs, recompute the challenged positions,
          compare exactly within the verifier's own runtime_class_id
verdict:  deterministic quorum over independent bonded verifiers of that class
```

Teacher-forcing is borrowed (Ambient's `ambient_forced_tokens`) — it removes sampling drift so a
mid-sequence position is deterministically recomputable. **The KV-cache caveat is a hard measurement
requirement, not a footnote:** "verify one token" is not "one token of FLOPs." A fresh verifier
holding no KV cache must prefill the forced prefix to reach a challenged position, so every replay-
cost number in the §10 inequality MUST be measured from a **cold, no-KV verifier**, per class, at
p99 — the "one-token verification is cheap" claim is prohibited (v2 design §15 extends to cover it).

> **Amended by ADR-0027 §1/§4:** challenge-position unpredictability is no longer a security
> dependency. The binding dispute path is a challenger-chosen first-divergence refutation
> (re-execute → name the step → one-step check under canonical reference arithmetic); the
> PRF-positions flow above survives only as an optional coverage sampler that decides nothing and
> never feeds a slash.

### 5. Dynamic challenge count `q`, derived from `P_detect · S > G` — never fixed at 1

Ambient's "verify one token" framing is the single weakest security claim, and PALW must not copy
its cardinality. For a fault touching a fraction `f` of positions, `P_detect = 1 − (1 − f)^q`.
A whole-output fault (`f = 1`) needs one challenge; a 1 %-localized fault (`f = 0.01`) is caught
with probability ≈1 % at `q = 1` and only ≈9.6 % at `q = 10`. So `q` is not a constant: it is sized
per job from bond `S`, expected cheating gain `G`, job value, computation amount and operator
reputation, so that `P_detect · S > G` holds for the *smallest plausible* `f` the scheme intends to
resist. The published §10 economics carry the `q`-sizing rule and its assumed minimum `f`; shipping
a fixed `q` (especially `q = 1`) is a rejected design.

> **Amended by ADR-0027 §3:** under the no-BFT / no-challenge-randomness premises the sampling
> form of `P_detect` is replaced by funded full re-execution (f-independent: one honest replay
> catches any deviation), and `q` changes meaning from "positions sampled" to "independent
> re-executors funded." The inequality keeps its shape; `P_detect` becomes an incentive property
> measured, never assumed.

### 6. Verification is asynchronous; PALW never gates block validity, and never on LStake alone

The BlockDAG must not stall on inference. Block validity stays `valid permanent hash PoW AND (PALW
certificate absent OR valid under its activation stage)` — the permanent hash floor is retained.
PALW runs as an overlay: commit on-DAG, challenge after future randomness resolves, verify,
settle, and slash **retrospectively** (Ambient's asynchronous/optimistic posture, which is
precisely what lets §3's exact verdict live off the critical path). Fork-choice/DNS/finality
weight is reached only through the staged ladder, never at once:

```
Stage 0  ShadowSidecar / zero-credit         (current envelope)
Stage 1  PALW rewards only
Stage 2  PALW → validator/DNS weight, WITH a weight cap
Stage 3  deeper security integration, post external audit + soak
```

And validator selection is **diversified**, not LStake-only. Ambient's LStake (influence ∝ past
verified work) invites a self-reinforcing loop: more PALW → more stake → more likely selected →
verify one's own class → more PALW. MISAKA gates selection on
`VRF ⊕ bond ⊕ independent-identity ⊕ PALW-reputation`, with executor self-verification excluded and
operator-aggregation/concentration caps (v2 design §10). If independent bonded credentials fall
below threshold, the committee is **not** shrunk to mint — PALW credit goes to zero.

### 7. Open kernel; self-originated jobs (no auction); tokenizer outside the runtime

- **Open verification kernel — the inverse of Ambient's binary-only verifier.** Preimage layouts
  (golden-frozen), the comparison rule (§3), challenge selection (§4), `q`-sizing (§5), and the
  bond/slash/maturity/window parameters plus **measured** per-class cold-verifier costs (§10) are
  all published *before* any non-zero credit. This is structural, not a preference: acceptance is
  economic, so `P_detect · S > G` is unauditable if challenge selection, the comparator or `S` are
  hidden — and MISAKA's own algo-5 history shows the failure mode (an output-text commitment that
  looked sound until measurement found a 1-distinct constant, forgeable without model execution;
  publication + measurement killed it in one cycle, a private spec would have preserved it).
  A vendor-supplied closed verifier also permissions the committee to the vendor and makes slashing
  on a closed component's word ungovernable. The domain keys are public keyed-BLAKE2b separations,
  not MAC secrets — obscurity buys this scheme nothing and would only gate verifiers.
- **Self-originated jobs, no auction tier.** PALW seeds are chain-derived (v2 design §9); there is
  no orderer, job market or job fee in scope, so Ambient's entire auction stack (`auction-*`) has
  no MISAKA equivalent and none is planned. This is also why security here does **not** depend on
  user query demand.
- **Tokenizer is a separately-pinned component.** The worker takes token IDs only;
  `tokenizer_id`/`tokenizer_sha256` pin it. A tokenizer drift is an identity change, not a runtime
  patch (Ambient's standalone `tokenizer` repo is convergent precedent).

### 8. Distribution: per-class signed artifact bundle, public source

Adopt Ambient's release *shape* (date-versioned, per-backend, verification-enabled runtime
artifacts — the `llama-2026.07.21.17-…` pattern) as the class unit: one signed bundle per class
containing the bit-identical static worker, its `RuntimeManifestV2`, the full-length golden set,
the GGUF/tokenizer pins and the launch verifier, under the secure-OTA role model (distribution and
activation separate). Unlike Ambient, **no part is binary-only**: full worker/patch source and
`patchset_root` stay public so reproducible-build checking is a public audit path — but bit-identity
of the *distributed* artifact is the *membership* rule (per-host builds silently minted divergent
artifacts once already: the ggml-OpenMP delta, `2825d99`). A class registration MUST reject any
manifest still carrying `"unpinned"` fields.

## Borrow / do not borrow (summary)

| Borrow from Ambient | Do **not** borrow |
|---|---|
| Runtime ↔ verification separation | Verifier compiled into the runtime fork |
| Multi-runtime adapters (vLLM / llama.cpp / …) | Logits-only proof material |
| Runtime-portable capture format (HDF5/SGLang-style) | Tolerant RBO / p95-band comparison in the binding verdict |
| Merkle commitment + post-commit challenge | Fixed 1-token verification |
| Teacher-forced recompute | "one-token verify = one-token FLOPs" costing |
| Random validator/range assignment | LStake-only validator selection |
| Asynchronous / retrospective slashing | PALW result gating block validity |
| Bond + slashing; pinned model/runtime profiles | Security that depends on user query demand |
| Release-artifact shape (per-backend, versioned) | Unspecified hardware tolerance |

## What the audit could not establish

Even with the binary: the exact Merkle-leaf preimage and canonical tensor byte-layout, the RBO
depth/weighting, Ambient's default threshold values (hidden behind format strings), whether cross-
backend verification shares thresholds, and everything in the unreleased Solana programs (challenge
selection, committee sampling, bond/slash amounts, LStake accounting). No binary was decompiled;
only exported symbols and static strings were read. The three x64 tarballs were not downloaded
(the macOS object settles the scheme). These unknowns do not affect any decision above — each rests
on MISAKA's own measurements and requirements and stands even if every remaining guess about
Ambient is wrong.

## Consequences

* **A GPU/vLLM path is class + adapter work, not scheme work.** A new adapter must supply the per-
  decode logits/activation/GEMM surface into the unchanged kernel preimages, FP-environment probes
  around model load, a build-measured manifest with zero `"unpinned"` fields, golden
  generation/self-test, and the envelope identity gate. GPU class granularity (per driver? per GPU
  generation?) is a measurement campaign before any registration; CUDA is not presumed batch-
  invariant.
* **Activation + GEMM legs are the next Land increment** under the existing Merkle discipline in
  `palw_v2.rs` — new leaf kinds and `shape_profile_id` pins, not a new mechanism; consensus stays
  inert until their own gates pass.
  > **Land status (2026-08-15, `consensus/core/src/palw_legs.rs`, consensus-inert):** the
  > activation and checkpoint legs are implemented as a new scheme family
  > (`misaka-palw/execution-commitment/v1`) wrapping the frozen v2 logits root; leg-era contexts
  > keep `trace_scheme_id = v2`, and which commitment form a class produces is a registry fact.
  > **§2's `Project(·)` is amended to exact canonical rows**: under ADR-0027 the legs feed
  > openings and one-step recomputation — a projection is neither an input state nor cheaper to
  > refute, and the hash already is the compression. The v2 fail-closed rule extends to
  > activations (a committed non-finite value is a refutable fault); checkpoint leaves chain
  > from a job-bound genesis, which moves v0.1 §17.2 `M-C4` "broken checkpoint ancestry" into
  > ADR-0027's *objective* column (two adjacent openings that do not chain convict — no
  > recomputation, no jury). Deliberately opaque until registration-time measurement:
  > `tap_semantics_id`, `state_layout_id`, tap-layer values, the interval. The GEMM/tile leg is
  > absent by design (blocked on step-function pinning; adding it is a new scheme version), and
  > worker capture of the legs is the next runtime increment — the schema froze first so capture
  > has one layout to hit.
  >
  > **Capture landed (2026-08-15, `misaka-palw-worker`, still consensus-inert):** the worker taps
  > `l_out-<il>` (post-block residual stream) and commits `llama_state_seq_get_data` checkpoints,
  > streaming both into the schema's own builder so no structural fault is constructible. The two
  > opaque identities are now *declared* (they become network facts only at registration), and
  > the increment's gate was measured rather than assumed: llama.cpp accepts a capture callback
  > only at context creation and it changes ggml's split-compute granularity, so **capture had to
  > be shown logits-neutral** — the legs bind the frozen v2 root, and a capturing executor whose
  > logits moved would disagree with a non-capturing verifier about an honest execution. Measured
  > 4/4 golden jobs unmoved on both the Metal and CPU classes, roots reproducible across runs,
  > 0 of 592 captured rows all-zero. A backend that fails this gate is a distinct determinism
  > class, not a bug to suppress. Evidence and run recipe:
  > `docs/palw-legs-capture-measurement-2026-08-15.md`.
  >
  > **Opening production landed 2026-08-16 (same doc, §6):** commitments are now answerable —
  > `v2-legs-open` re-executes and opens named leaves, refusing any root it cannot reproduce
  > (openings come from re-execution, so the class can answer for its honest members and nobody
  > can open a fraudulent tree), and `check_legs_opening_answer_v1` adjudicates answers
  > model-free. Measured commit → open-in-a-fresh-process → verify on both classes, with both
  > refusals (tampered commitment, tampered answer) held. The challenge *sampling* protocol
  > remains the future ADR above.
* **The challenge/`q`/randomness protocol and the cold-verifier cost model are new design work**
  (future ADR): future-randomness binding point, reorg handling, job-commit deadline, `q`-sizing
  formula with its assumed minimum `f`, and per-class p99 cold-prefill costs — all published before
  the first non-zero credit, as §12 gate items.
  > **Drafted as ADR-0028 (2026-08-16, Proposed):** binding point and reorg rule (anchor at
  > `daa(C) + Δ_bind`, chain-scoped duties), DAA-denominated windows with measured-p99 sizing,
  > `q` re-defined as funded-replay redundancy (its minimum-`f` parameter is gone with the
  > sampling security model), and the opening seam as the DA-audit transport. Randomness
  > schedules; it never decides — per ADR-0027 P2.
* **Any tolerance proposal is a new scheme version.** There is no ε parameter in the binding path.
* ADR-0021 stays the historical header-PoW record and points here; the v2 design doc stays the
  operative safety model and gate list; the binary-audit doc is the evidence of record. This ADR
  adds no activation and changes no consensus behavior.
