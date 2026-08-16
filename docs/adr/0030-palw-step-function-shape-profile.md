# ADR-0030: The PALW step function, pinned at tile granularity — shape profile v3

Status: **Accepted (schema frozen; every class-specific value is registration-measured).**
Activates nothing. This ADR pins the *step function* of ADR-0027's one-step refutation: which
operator, which tile shape, which reduction order `shape_profile_id` binds. It is the
prerequisite ADR-0027's Consequences named for Stage 2, and the design basis for the
execution-commitment **v2** (step leg), the canonical transcendentals (ADR-0031), and
`ExecutionStepRefutationV1`.
Date: 2026-08-16
Relates to: ADR-0027 §1/§2 (the refutation this makes adjudicable; the reference arithmetic),
ADR-0026 §2 (activation/GEMM legs, "committed" → "adjudicable"), ADR-0028 §6 (Stage 2
prerequisite), ADR-0029 §3/§6 (carriage mass budgets; chunked evidence),
`consensus/core/src/palw_reference.rs` (ruleset v1 this extends),
`consensus/core/src/palw_legs.rs` (the v1 composite this versions past),
`misaka-palw-worker/src/shim.c` (the execution pins the profile must restate).

## Facts this design stands on (verified in the pinned tree, 2026-08-16)

All statements below were read out of the pinned runtime source
(`llama.cpp @ 030ebb558`, the tree `misaka-palw-runtime/llama.cpp-cpu`, CMake profile
`GGML_NATIVE=OFF / GGML_CPU_ALL_VARIANTS=OFF / GGML_OPENMP=OFF / GGML_BLAS=OFF`) and the
pinned GGUF (`Qwen3.5-2B-Q4_K_M.gguf`, sha256 `aaf42c8b…`), not assumed from upstream
knowledge. Every one of them moved the design.

1. **The pinned model is a hybrid, not a dense transformer.** `qwen35.full_attention_interval
   = 4`: of 24 layers, **18 are Gated-DeltaNet (linear attention) layers** and 6 (indices
   3, 7, 11, 15, 19, 23) are full-attention layers. GDN layers carry per-head recurrent state
   (16 heads × 128×128 f32 = 1 MiB/layer) instead of KV cache; attention layers use GQA
   (8 query heads, 2 KV heads, head_dim 256) with F16 KV cache and **partial IMROPE** (64 of
   256 head channels rotated, sections `[11,11,10,0]`). A step taxonomy written for a
   transformer would mis-pin 18 of 24 layers.
2. **The LM head is the tied embedding**: no `output.weight` exists; logits are a
   `Q6_K × F32` matmul against `token_embd.weight` (2048 × 248 320).
3. **Decode matmuls do not run the classic `vec_dot` kernels.** The build defines
   `GGML_USE_CPU_REPACK`; weight tensors are repacked at load and single-token matmuls run
   the repack **gemv** kernels (on the aarch64 class: `q4_K/q5_K/q6_K_8x4_q8_K`, `q8_0_4x4`;
   on x86: the 8×8 AVX2 twins). Their reduction shape differs from the classic kernels: per
   256-element superblock, the aarch64 q4_K gemv performs **five fused multiply-adds on a
   per-output f32 accumulator, sequential in k** — exact int32 dot-products inside, `fma` at
   the float seam, no lane-tree reduction. The **one exception** is the LM head: `token_embd`
   lives in a plain buffer (it is also a `get_rows` operand), so the logits matmul is the one
   classic-path kernel (`ggml_vec_dot_q6_K_q8_K`) in the decode graph.
4. **Per-tensor dtypes vary by layer.** `attn_v` is Q6_K in 4 attention layers and Q4_K in 2;
   `ffn_down` is Q6_K in 12 layers and Q4_K in 12. The kernel binding is per op *instance*,
   not per op kind.
5. **The graph runs two exp implementations in one row.** `soft_max` and `silu` vector bodies
   use `ggml_v_expf` (a SIMD polynomial compiled from source, 4 lanes on NEON / 8 on AVX2)
   and fall to **libm `expf`** for the `n % lanes` tail. The polynomial has a
   lane-content-dependent fast/slow path. RoPE uses **libm `cosf`/`sinf`** per element, with
   theta advanced by repeated multiplication (never `powf` per index). Transcendental
   provenance is therefore split between "compiled from the pinned source" and "the class's
   libm" — and the second is currently pinned by nothing.
6. **Wide accumulations are double.** `rms_norm`, `l2_norm` and the `soft_max` sum accumulate
   in `double` (scalar, index-ascending) and narrow once; `soft_max` normalizes by a
   **double reciprocal then f32 multiply**, not a divide. The reference arithmetic therefore
   needs binary64 soft-float ops, not just binary32.
7. **`flash_attn = DISABLED` in the shim is load-bearing beyond its stated reason.** The
   flash path this pin fences off splits the KV axis across threads once KV length ≥ 512 and
   merges partials in chunk order — logits would depend on thread count, and the credited
   ceiling (512 decode + ~70 prefill) **crosses that threshold mid-job**. With the pin,
   attention is `MUL_MAT + SOFT_MAX + MUL_MAT`, all thread-count-independent: no float
   reduction crosses a thread boundary anywhere in the pinned graph (mul_mat splits outputs,
   never k; norms and softmax are one-row-per-thread; the repack activation quantization at
   n=1 runs on thread 0 only).
8. **Two numerics-relevant build facts are pinned by nothing today**: `GGML_CPU_REPACK=ON`
   and `GGML_LLAMAFILE=ON` (defaults). Both were ON for every fleet measurement, so the
   *measured class is consistent* — but `CPU_BUILD_PROFILE` and `shape_string_v2` do not name
   them, and llamafile's tinyBLAS **does** engage at prefill (n ≥ 4) for F16/F32 operands
   while declining at decode (n < 2). Prefill and decode run different matmul kernels; both
   are normative.
9. **Scalar float expressions are compiler-contraction-dependent.** `-O3` clang contracts
   `a*b±c` into `fmla`/`fmls` (arm) or `vfmadd` (x86) at its discretion (RoPE's rotation,
   the q6_K trailing expression). Source alone cannot resolve which; only the shipped
   binary's disassembly can. Contraction state is therefore a **per-class measured fact**,
   not a spec constant.
10. **The GDN fused op is active** (`fused_gdn_ar/ch` default true, CPU implements
    `GGML_OP_GATED_DELTA_NET`) and is a different arithmetic than the unfused decomposition
    (which routes through single-threaded `SUM_ROWS`). The fused kernel's internal order is
    the largest single transcription surface (18 of 24 layers).
11. **The reference ruleset v1 cannot express the class kernels.** Ruleset v1 is frozen as
    "no fused multiply-add"; the active gemv kernels are built on `fma`. Expressing them as
    mul-then-add would compute different bits. v1 stays frozen; the step function needs an
    **additive ruleset v2** with `fma` (and the binary64 ops of Fact 6).

### Facts, second pass (kernel internals, read 2026-08-16 after the first eleven)

12. **The GDN kernel is fully mapped and thread-safe by construction**: the fused
    `GGML_OP_GATED_DELTA_NET` computes, per `(head, seq)` owned end-to-end by one thread, a
    strictly sequential per-position recurrence — decay scale by one **scalar libm `expf`**
    per (token, head), 128 pinned-order f32 dots of length 128 (4 × 4-lane `vfmaq_f32`
    accumulators, frozen lane-reduce, no tail at 128), 128 `vec_mad` outer-product updates,
    128 output dots scaled by `1/sqrtf(128)` **applied to the output** (the unfused graph
    scales q instead — a rounding difference; the fused path is the class's). The state is
    written transposed into the output tensor's tail and serialized from there. No float
    reduction crosses a thread boundary; ≤ 16-way parallelism.
13. **The aarch64 build accumulates attention dots in fp16 lanes** (`vfmaq_f16` into
    `float16x8_t`, widened only at the final reduce) because bare `-arch arm64` implies
    `__ARM_FEATURE_FP16_VECTOR_ARITHMETIC` — invisible in any CMake flag. The x86 twin
    accumulates the same op in f32 (F16C converts, then `_mm256_fmadd_ps`). This is a
    *structural* cross-class divergence that grows with context length. Consequence: an
    aarch64-class step profile would need binary16 *arithmetic* in a future additive ruleset;
    the fleet (x86) class needs only the v2 conversions.
14. **x86 repack coverage differs from arm**: with AVX2 (no AVX512), only Q4_0/Q4_K-family
    weights repack (`q4_K_8x8_q8_K`, int16-maddubs inner accumulation, mins subtracted once
    at the end); **Q5_K, Q6_K and Q8_0 have no AVX2 repack** and run the classic
    `ggml_vec_dot_*` kernels. On this model that reroutes `attn_qkv`/`ssm_out` (Q5_K),
    half the `ffn_down`s (Q6_K) and `ssm_alpha/beta` (Q8_0) — the per-class catalogs are
    genuinely different lists, not lane-width variants of one list.
15. **Transcendental sites, precisely**: `ggml_v_expf` uses identical polynomial constants on
    NEON and AVX2 (4 vs 8 lanes; a content-dependent fast/slow path per lane-group; the
    AVX512 variant is a *different expression* and must never be assumed bit-equal); scalar
    libm `expf` runs in vector-op tails (never for this model's row lengths — FFN 6144 and
    padded KV rows divide the lane count — but the catalog pins it for generality), in
    `sigmoid` (`1/(1+expf(−x))`, scalar, single-threaded), in `softplus` (threshold branch
    `x > 20 → x`, else `logf(1+expf(x))` — libm `logf` too), and per (token, head) in the GDN
    decay; libm `sinf`/`cosf` in RoPE. `l2_norm` is `1/max(sqrtf(sum), eps)` — a *different*
    composition than `rms_norm`'s `1/sqrtf(mean+eps)`.
16. **The KV-cache f32→f16 write is the software RNE bit-twiddle on arm** (and F16C RNE on
    x86 — same values), via single-threaded `SET_ROWS`: exactly ruleset v2's `f32→f16`.
    `quantize_row_q8_K` is one shared scalar recipe on both arches (`iscale = −127/max`,
    first-tie-wins amax scan, magic-bias RNE, `d = 1/iscale` — the double-division last-ulp
    form); the 4-row prefill variants differ only in byte layout and tie-sign convention,
    which cancel in every product.
17. **Padded KV rows must not be committed**: scores at padded positions are dots over
    unwritten cache bytes, masked to −∞ only *after* the tensor exists. Step-leg captures of
    ctx-length nodes slice to the true `kv_len`; the padded region is outside the committed
    bytes, or determinism claims would rest on allocator behavior.

## Premises

Inherited: measure-before-pin (tap-profile discipline — schema frozen now, class values at
registration), no post-hoc fields (a new commitment form is a new scheme version), class =
**conformance to the canonical reference** (ADR-0027 §2), fail-closed production (an honest
executor aborts rather than commit a refutable byte). Three step-specific premises join them:

* **The reference follows the class, not the other way around.** The canonical semantics of a
  step is a *transcription of what the pinned kernel actually computes*, expressed in
  reference arithmetic. We do not define a "nicer" order and demand the runtime match it —
  that would fork every honest host. Consequence: the reference is per-class where kernels
  differ (NEON 8×4 vs AVX2 8×8 are different reduction orders, hence different
  `kernel_semantics_id`s), and a shape profile is a per-class registration fact.
* **Reduction order is code, not data.** A profile names frozen reference programs by id
  (`kernel_semantics_id`); it never carries an interpretable order description. An order DSL
  interpreted at adjudication time is an attack surface and a second implementation of every
  kernel; a named program is a golden-tested function. New order = new id = new program.
* **A step's inputs must be committed material.** Every step must be recomputable from
  openings of the same commitment (plus the pinned model artifact and the job context) —
  never from data only the miner holds. Where an input would otherwise be uncommitted
  (recurrent state mid-interval), the checkpoint tree must be structured so the needed slice
  opens at carriageable size.

## Decision

### 1. The step space

A **step** is one operator invocation on one output tile:

```
step = (call_index, node_slot, position, tile_index)
  call_index  ∈ 0 .. D−1          call 0 = prefill (P positions), calls 1.. = decode (1 position)
  node_slot   ∈ 0 .. nodes(layer_kind)−1   the profile's node table for that graph position
  position    ∈ 0 .. P−1 for call 0, = 0 otherwise   (the activation-leg convention)
  tile_index  ∈ 0 .. ceil(out_len(node, call) / tile_len(node))−1
```

The step index is the rank of this tuple in the pinned enumeration (call-major, then node in
graph order, then position, then tile). The enumeration and both directions of the bijection
are pure functions of `(job context, shape profile)` — implemented once in
`consensus/core/src/palw_step.rs`, property-tested as a bijection. A refutation names a step
*index*; coordinates are derived, never trusted from the wire (the legs-v1 discipline).

The node tables are profile **data** (two templates: GDN layer, attention layer, plus
pre/post-graph nodes: embedding lookup, final norm, LM head). The op-kind taxonomy is a
frozen enum (`PalwStepOpKindV1`): `EmbedLookup, RmsNorm, MatMulQuant, MatMulF16, RopeImrope,
SoftMax, Sigmoid, Softplus, SsmConv, Silu, Glu, GatedDeltaNet, L2Norm, MulElem, AddElem,
Scale, CpyF32F16` — the closed set of operator shapes the pinned graph can contain. A graph
needing a kind outside the set is a new profile version, not a stretched meaning.

### 2. Shape profile v3 — what `shape_profile_id` binds

A new preimage under a new domain (`misaka-palw/shape-profile/v3`); the v2 shape-string
domain stays frozen and deployed contexts keep meaning what they meant. The context field
does not change shape — which profile id a class's jobs carry is a registration fact, the
same way the commitment form is.

`PalwShapeProfileV3` binds, in one canonical hash:

1. **Model geometry** (from the pinned GGUF, restated so the profile is self-contained):
   layers, layer-kind map (`full_attention_interval`), hidden, ffn, heads/kv-heads/head_dim,
   rope dims + sections + freq_base, GDN heads/head_k/head_v/conv_kernel, vocab, rms/l2 eps.
2. **Execution-shape facts** — the v2 shape string's pins plus every fact 8-class knob:
   `repack=on, llamafile=on, flash_attn=disabled, fused_gdn=on, use_ref=off,
   kv_dtype=f16, threads, n_ctx/n_batch/n_ubatch/n_seq`. What was under-pinned is now in the
   signed identity.
3. **The node tables** (§1) — per node: op kind, weight reference (GGUF tensor name + dtype,
   so Fact 4's per-layer dtype variance is bound), output-length rule, `tile_len`, and the
   node's `kernel_semantics_id`.
4. **`reference_arithmetic_ruleset_id` (v2)** — the arithmetic the kernel programs are
   written in.
5. **Transcendental provenance** (ADR-0031 ids): which exp/sin/cos/expf-tail algorithm each
   transcendental site binds — split by Fact 5 into `source-polynomial/…` ids (transcribed
   from the pinned tree) and `libm/…` ids (the class's libm algorithm, e.g. glibc 2.39's) —
   closing the "pinned by nothing" gap.
6. **Contraction facts** (Fact 9): per named scalar site, `contracted | not-contracted`, as
   measured by disassembly of the class binary at registration.
7. **Aux series declarations** (§3): KV-chunk geometry, checkpoint chunk map id.

Frozen **now**: the schema, its validation rules, the id derivation, the op-kind taxonomy,
the enumeration. Registration-measured **per class**: every value above, exactly like
`tap_semantics_id` before it. The catalog of `kernel_semantics_id` values grows by
transcription + differential validation (§5), never by edit.

### 3. The step leg — execution-commitment v2

A new composite scheme family `misaka-palw/execution-commitment/v2` (new domains throughout;
v1 and its goldens untouched):

```
execution_commitment_root_v2 = H( job_context_hash ‖ full_logits_trace_root(v2, frozen)
                                ‖ activation_leg_root(v1 discipline) ‖ checkpoint_leg_root(v2)
                                ‖ step_leg_root )
```

* **Step leg**: a Merkle tree (legs-v1 tree discipline: domain-separated leaf/node keys, odd
  promote, count bound outside) over **node-output tiles** — leaf = exact f32-LE bytes of one
  tile of one node invocation, at the §1 coordinates — plus declared **aux leaves**:
  KV-chunk leaves (the F16 K/V rows of `chunk_calls` consecutive calls for one kv-head,
  re-hashed so a ctx-length reduction opens ~10 chunks instead of ~580 single rows). The v2
  logits rows stay committed where they are; the step leg *additionally* commits the LM-head
  output in 2048-element tiles, which retires the 0.99 MiB single-leaf opening problem for
  composite-v2 classes (ADR-0029 §6's chunked carriage remains the bare-v2 story).
  Fail-closed extends: a non-finite value in any committed tile is refutable; honest
  execution aborts instead.
* **Checkpoint leg v2**: the flat `state_root` becomes a Merkle root over **canonical state
  chunks** aligned to semantic units — per (GDN layer, head) recurrent-state slices (64 KiB),
  per-layer conv tails, per (attention layer, kv-head, call-range) cache spans — under a
  registration-measured `state_chunk_map_id` (the serializer's layout is a measurement, not a
  guess: `llama_state_seq_get_data`'s internal order is runtime-version fact). The ancestry
  chain rule is unchanged. This is what makes a mid-interval GDN step adjudicable at
  carriageable size: open one head's 64 KiB slice, not a ~26 MiB state blob.
* Which commitment form a class produces — bare v2, composite v1, composite v2 — remains a
  registration fact; contexts keep `trace_scheme_id = v2` (the logits meaning is unchanged).

Sizing, computed for the pinned geometry at the credited ceiling (P=70, D=512, tile 256;
`scripts/` sizing script, re-run before any number here is quoted): step leaves ≈ 5.7 k per
decode call → **≈ 3.26 M leaves/job** including the prefill call — inside a `1 << 22` cap
with headroom; tree depth 22. Worker-side hashing ≈ 3.75 GB/job ≈ 4 s (the v2 logits trace
already hashes ~0.6 GB of that today); checkpoint-chunk hashing adds ≈ 1.3 GB at interval 8.
Worst-case single-step refutations: quant-gemv ≈ 12–61 KB, LM-head tile ≈ 29 KB, GDN
interval replay ≈ 126 KB, full-context attention scores/mix ≈ 339–344 KB — **every step
opens inside one 480 KB standard transaction**; nothing needs ADR-0029 §6's chunked carriage
except the bare-v2 legacy case it already covers.

### 4. Adjudication — `ExecutionStepRefutationV1` becomes implementable

Exactly ADR-0027 §1's object, now with defined coordinates: `{ committed root binding,
step_index, openings(input tiles/chunks/checkpoint slices), opening(output tile) }`. The
adjudicator derives the step's coordinates (§1), resolves its `kernel_semantics_id` (§2),
recomputes **one tile** with the reference program from the opened inputs + pinned weights,
and compares exact bytes. Recompute ≠ committed output → miner slashed; equal → challenger
slashed (`NoFaultFound`). Reference programs run in ruleset v2 arithmetic:
binary32 add/sub/mul (v1, frozen) **+ fma + div + sqrt** (all IEEE-exact, soft-implementable)
**+ binary64 add/mul/div + exact f32↔f64 + RNE f64→f32 narrowing + RNE f32↔f16** (Facts 6,
and the KV write seam) — an additive ruleset with a new id; v1's id and goldens do not move.
Transcendental sites call ADR-0031 programs by id. The claim stays ADR-0027 §2's: the job is
10¹⁵ ops, the adjudication is one tile — ≤ ~10⁷ soft-float ops even for a full-context
attention row.

### 5. Validation gates — before any class registers a v3 profile

1. **Kernel differential harness**: every `kernel_semantics_id` program is validated against
   the *actual pinned kernel* (linked from the pinned static libs, driven over random +
   adversarial + boundary inputs, on the class's own architecture) — exact-bits, per class,
   artifacts kept. A transcription that has not run against its kernel is not a candidate id.
2. **Contraction disassembly**: the §2.6 facts are read out of the shipped binary
   (`objdump` for `fmla/fmls/vfmadd` at the named sites), not out of source.
3. **Whole-graph conformance**: the capture-ON committed tree's final tiles must equal the
   runtime's node outputs, and the run's v2 logits root must equal the bare (capture-OFF)
   execution's — the legs-v1 neutrality gate, re-run at step-leg tap density (the eval
   callback changes scheduler splits; neutrality at ~40 taps/graph is a measurement, not an
   assumption). Drift ⇒ the class cannot register a step leg, full stop.
4. **Cross-host**: the committed step-leg root is bit-identical across the class's hosts on
   the conformance corpus (the 60-seed audit discipline, extended to the new root).

## Consequences

* **Build order becomes unblocked**: ruleset v2 primitives → `palw_step.rs` (schema, ids,
  enumeration) → step-leg scheme v2 (builder/openings/structural faults) → kernel program
  catalog + differential harness → `ExecutionStepRefutationV1` → bisection ladder (ADR-0027's
  degraded path, over the same step space). The transcription catalog and the fleet
  differential runs are the long pole, and they parallelize per kernel.
* **The GDN fused kernel** (Fact 10) is the largest transcription unit and gates 18 of 24
  layers' adjudicability. Its map (internal order, threading, state write-back) is required
  before its `kernel_semantics_id` can exist. Until then, structural faults + logits/
  activation/checkpoint refutations remain the reachable convictions — Stage 2 stays gated.
* **Prefill kernels are normative** (Fact 8): tinyBLAS f16 paths and the repack gemm 4-column
  tiles need their own ids; "decode-only" transcription would leave prefill steps
  unadjudicable and the prefill activation rows unprovable.
* **The profile self-describes the previously unpinned flags**; any future build that flips
  `repack`/`llamafile`/`fused_gdn` is a different profile id and (per registration rules) a
  different class — the silent-kernel-swap hazard is closed at the identity layer.
* **arm and x86 are separate catalogs** by construction (different orders ⇒ different ids ⇒
  different profiles). Nothing pretends one reference order covers both.
* **What this deliberately does not do**: no consensus wiring, no store, no carriage kind for
  the new objects (ADR-0029's rails carry them when Stage 1 lands), no economic change. The
  §12 gate and ADR-0028's stage ladder are unmoved: this ADR converts "cannot even in
  principle" into "implementable and measurable", nothing more.
