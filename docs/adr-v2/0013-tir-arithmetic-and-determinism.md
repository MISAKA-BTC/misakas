# ADR-v2-0013 — TIR算術・tile・BASE-0・決定論

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 7本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0002](../rfc-v2/0002-tensor-ir-and-shared-query-data.md)

## 1. この文書の役割

計算の同一性を左右するtile列挙、算術、量子化境界とkernel別の固定意味を統合する。

## 2. 統合した判断の読み方

### 2.1

stepをcall・node・position・tileで一意に列挙し、runtimeとcourtが同じshape profileを読む。[旧ADR-0030](../adr/0030-palw-step-function-shape-profile.md)

### 2.2

exp／log等を単なる数学関数名で済ませず固定algorithmとして識別する。integer-only classと固定浮動小数点classを混ぜない。[旧ADR-0031](../adr/0031-palw-canonical-transcendentals.md) [旧ADR-0040](../adr/0040-palw-base-0-integer-arithmetic.md)

### 2.3

A16の幅、蓄積とnarrowingの境界、residual site、hybrid graph、held attention historyの幅を対応class／版に結び付ける。[旧ADR-0047](../adr/0047-palw-a16-activation-tier.md) [旧ADR-0050](../adr/0050-palw-base0-residual-site.md) [旧ADR-0052](../adr/0052-palw-qwen36-hybrid-class.md) [旧ADR-0116](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)

## 3. 維持する境界・後続改定

原文にはProposedのclass拡張・修正案もある。統合は全kernelが採択済み・実装済み・有効という宣言ではない。

## 4. 隣接する判断との分担

backend最適化は[ADR-v2-0014](0014-versioned-kernels-and-backends.md)、実際の登録と資源admissionは[ADR-v2-0015](0015-permissionless-model-registration.md)・[ADR-v2-0018](0018-long-context-measurement-and-admission.md)、終端裁定は[ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0030-palw-step-function-shape-profile.md](../adr/0030-palw-step-function-shape-profile.md) | L1–L325 | L15–L27 |
| [0031-palw-canonical-transcendentals.md](../adr/0031-palw-canonical-transcendentals.md) | L1–L88 | L12–L19 |
| [0040-palw-base-0-integer-arithmetic.md](../adr/0040-palw-base-0-integer-arithmetic.md) | L1–L456 | L12–L15 |
| [0047-palw-a16-activation-tier.md](../adr/0047-palw-a16-activation-tier.md) | L1–L102 | L22–L24 |
| [0050-palw-base0-residual-site.md](../adr/0050-palw-base0-residual-site.md) | L1–L191 | L12–L14 |
| [0052-palw-qwen36-hybrid-class.md](../adr/0052-palw-qwen36-hybrid-class.md) | L1–L258 | L15–L17 |
| [0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) | L1–L177 | L15–L26 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0030-palw-step-function-shape-profile"></a>

<details>
<summary>0030-palw-step-function-shape-profile.md — 原文 L1–L325</summary>

[原文を開く](../adr/0030-palw-step-function-shape-profile.md) · SHA-256: `c994f81843e20ae4cf957067971e1caac576c27d9e15b30c9f7c1dbb167f4222`

<!-- ADR_SOURCE_BEGIN:0030-palw-step-function-shape-profile.md -->
~~~~~~~~markdown
# ADR-0030: The PALW step function, pinned at tile granularity — shape profile v3

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


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
`misaka/palw/worker/src/shim.c` (the execution pins the profile must restate).

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

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0030-palw-step-function-shape-profile.md -->

</details>

<a id="source-0031-palw-canonical-transcendentals"></a>

<details>
<summary>0031-palw-canonical-transcendentals.md — 原文 L1–L88</summary>

[原文を開く](../adr/0031-palw-canonical-transcendentals.md) · SHA-256: `fe1c008699f2ac716a277fa27531282d373b82d128f8e44687eea2cc9080443e`

<!-- ADR_SOURCE_BEGIN:0031-palw-canonical-transcendentals.md -->
~~~~~~~~markdown
# ADR-0031: Canonical transcendentals — exp and log are algorithms, not functions

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Accepted (programs land consensus-inert; every binding is registration-measured).**
IEEE-754 does not require correctly-rounded libm, so `expf` is not one function — it is
whichever algorithm the class actually runs. This ADR pins how transcendental sites are
identified, which programs exist today, and what can never be pinned.
Date: 2026-08-16
Relates to: ADR-0030 (Fact 15, the site taxonomy and `transcendental_algorithm_id`),
ADR-0027 §2 (reference semantics), `consensus/core/src/palw_reference.rs` (ruleset v2 — the
arithmetic these programs are written in), `consensus/core/src/palw_transcendental.rs`.

## Facts (read from the pinned tree and the fleet's libc lineage, 2026-08-16)

1. **The vector exp is per-lane pure, and NEON ≡ AVX2 in values.** `ggml_v_expf`'s two
   implementations differ in lane width (4 vs 8), argument order, and blend structure — but
   term-by-term the expressions are the same single-rounding operations (fma argument order
   commutes; the "fast path" `k + j·k` and the slow-path blend's `k + k·j` are the same
   value; the group-wide fast/slow branch selects between per-lane-equal expressions). One
   per-element transcription therefore serves both CPU classes, and the vector exp is NOT
   what separates them (the separators remain fp16 accumulation and repack coverage,
   ADR-0030 Facts 13–14).
2. **The fleet's libm `expf`/`logf` are glibc 2.39's, in the FMA multiarch build.**
   `sysdeps/x86_64/fpu/multiarch/e_expf-fma.c` recompiles the generic C with FMA enabled and
   IFUNC-dispatches on FMA-capable CPUs — both fleet microarchitectures qualify. Which of the
   three (expf) / five (logf) candidate `a·b±c` sites the compiler fused is a property of the
   distro's build, resolvable only by disassembling the fleet's `libm.so.6` — a per-class
   measured contraction fact, exactly ADR-0030 Fact 9's pattern. Both contraction variants
   are implemented; registration binds the disassembly-confirmed one.
3. **The glibc algorithms are fully transcribable.** `e_expf.c` (EXP2F_TABLE_BITS=5: a
   32-entry u64 table, degree-3 scaled polynomial, double internals, the `+0x1.8p52` round
   trick) and `e_logf.c` (LOGF_TABLE_BITS=4: 16 `{invc, logc}` pairs, degree-3 poly, double
   internals) — sources archived alongside the transcription; all `/N` constant scalings are
   by powers of two and therefore exact.
4. **Apple libm is closed.** The scalar tails, `sigmoid`, `softplus` and the GDN decay on the
   Metal/arm classes call an algorithm whose source cannot be transcribed. Consequence: the
   arm-side classes **cannot bind libm sites** and remain structural-fault-only for steps
   that traverse them. That is an honest admission boundary (ADR-0027: hardware that cannot
   match the reference is not admitted to arithmetic adjudication), not a gap to paper over.
5. **`sqrt` and division need no ADR**: IEEE-754 basic operations, correctly rounded by
   ruleset v2 (`rms_norm`'s `1/sqrtf(mean+eps)` and `l2_norm`'s `1/max(sqrtf(sum), eps)` are
   adjudicable with ruleset ops alone). RoPE's `sinf`/`cosf` (glibc, with their own -fma
   twins and large reduction tables) are **named but not yet transcribed**: RoPE steps stay
   catalog-pending until their programs land; nothing else blocks on them.

## Decision

* Transcendental identity = `transcendental_algorithm_id_v1(descriptor)` (ADR-0030's
  domain). Descriptors in the catalog today:
  - `source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1` — the vector exp, per-element.
  - `source-poly/ggml-v-silu/llama-030ebb558/per-lane/v1` — `x / (1 + v_expf(0 − x))`, true
    divide, `0 − x` transcribed literally (not negation).
  - `libm/glibc-2.39/expf/{fma,nofma}/v1`, `libm/glibc-2.39/logf/{fma,nofma}/v1` — the
    scalar sites (sigmoid `1/(1+expf(−x))`, softplus `x>20 ? x : logf(1+expf(x))`, the GDN
    decay `expf(g)`, vector-op tails). One of `{fma,nofma}` per class, by disassembly.
  - `libm/glibc-2.39/sinf/…`, `…/cosf/…` — **reserved, unimplemented** (RoPE gate).
* Programs are written in ruleset-v2 arithmetic only (soft f32/f64; integer bit steps are
  native integers), live in `consensus/core/src/palw_transcendental.rs`, and are frozen by
  golden vectors. New algorithm = new descriptor = new id; never an edit.
* NaN policy: committed bytes are finite (fail-closed), so a transcendental's NaN handling
  is transiently observable at most; programs canonicalize NaN outputs like the ruleset does
  and the divergence from glibc's payload-preserving `x + x` is recorded as unobservable in
  adjudication.
* Validation: local twins (hardware-fma expression mirror for v_expf; loose ≤1-ulp envelope
  against the host libm for the glibc programs — the host is Apple, glibc's 0.502-ulp budget
  makes exact agreement wrong to demand) now; **exact-bits differential against the fleet's
  actual `libm.so.6`/compiled `vec.h` on the class hosts is the ADR-0030 §5.1 registration
  gate** — a program that has not run against its kernel is not a candidate id.

## Consequences

* The GDN layers (18 of 24) become arithmetically adjudicable end-to-end once the glibc expf
  program is fleet-validated: the recurrence itself needs only ruleset ops + one `expf` per
  (token, head).
* Softmax and SwiGLU steps bind `v-expf/per-lane` + ruleset ops (the double-precision sum
  and reciprocal are ruleset binary64); their tails bind the libm ids — for the pinned
  geometry no tail ever executes, and the profile records the ids anyway so an off-geometry
  job cannot dodge adjudication.
* RoPE steps and (on arm classes) every libm site remain catalog-pending — named, honest,
  and visible in the profile rather than silently unadjudicable.
~~~~~~~~
<!-- ADR_SOURCE_END:0031-palw-canonical-transcendentals.md -->

</details>

<a id="source-0040-palw-base-0-integer-arithmetic"></a>

<details>
<summary>0040-palw-base-0-integer-arithmetic.md — 原文 L1–L456</summary>

[原文を開く](../adr/0040-palw-base-0-integer-arithmetic.md) · SHA-256: `6c789337305ac72c5ddb352555941acc554e746432c2ac93a010671f94c4db54`

<!-- ADR_SOURCE_BEGIN:0040-palw-base-0-integer-arithmetic.md -->
~~~~~~~~markdown
# ADR-0040: `PALW-BASE-0` — the integer-only arithmetic normative specification

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Proposed.** Activates nothing and registers no class. Specifies the arithmetic a
`PALW-BASE-0` implementation must reproduce bit-for-bit, and the op set it is allowed to contain.
Implementation follows this ADR; the class registers only after two independent implementations
agree and its kernel catalog is complete.

Date: 2026-08-17
Relates to: ADR-0039 (which requires this class and puts it FIRST), ADR-0030 (the step space and
shape profile this op set instantiates), ADR-0031 (canonical transcendentals — superseded *for
this class* by having none), ADR-0027/0028 (the court this class exists to make reachable),
`consensus/core/src/palw_step.rs` (`PalwStepOpKindV1`, the 17-kind vocabulary the float classes
need), `consensus/core/src/palw_pwu.rs` (`pwu_per_inference`, which this makes countable).

## Context — why an integer class, and why it is built first

ADR-0039 makes `PALW-BASE-0` the permanent liveness floor and, more importantly, requires that
**no class may carry fork-choice weight until its kernel catalog is complete**. Under optimistic
verification, conviction is the only thing standing between a fabricated block and full weight, so
a class whose lies terminate `Unadjudicable` cannot safely carry weight.

The float classes cannot close their catalog soon. Today it resolves 6 of 17 op kinds, and the
11 outstanding include the ones that carry the computation — `MatMulQuant`, `MatMulF16`,
`SoftMax`, `RopeImrope`. Each needs an exact-bits transcription that must also pin glibc `expf`,
`logf`, `sinf`, `cosf`, FMA contraction, and the reduction order of every threaded sum. That work
is real and slow, and the 2026-08-17 audit found the class identity did not even *name* libm.

An integer class removes the entire category rather than transcribing it.

## Decision A — Integer-only means integer-only

No IEEE-754 value, no `libm` symbol, and no floating-point instruction may appear on the
consensus path of a `PALW-BASE-0` implementation: not in the kernels, not in the scale factors,
not in the activation functions, not in the rotary tables. Scales are integer
`(multiplier, shift)` pairs, never a float. A build that links `libm` for this class is not a
conforming implementation, and the class identity records the absence rather than a version
(contrast the float classes, where ADR-0031 makes glibc's `expf` normative arithmetic *inside the
PoW tag*, and where an unpinned libm is a false-conviction vector — the 2026-08-17 audit's B8).

## Decision B — Representation

```
weights      int8, per-output-channel scale as (multiplier: i32, shift: u8)
activations  int8, per-tensor scale as (multiplier: i32, shift: u8)
accumulator  i32
requantize   an EXPLICIT op (Decision D), never an implicit narrowing
```

Per-channel weight scales and per-tensor activation scales are the shape profile's, frozen at
registration. There is no dynamic (per-inference) rescaling anywhere: a scale computed from the
data would make the arithmetic depend on the data's range, and two implementations that disagree
by one ulp about a range would diverge on everything downstream.

## Decision C — The three arithmetic rules, stated once and used everywhere

**C1. Every site that loses information is named, and each names its own rule. There is more than
one rule, and that is deliberate.**

| site | rule |
| --- | --- |
| `RoundingShiftRight` (below) | round half **away from zero** |
| `SRDHM` (C2) | round half **up** (toward +∞) — gemmlowp's |
| the internal `>> k` inside `RmsNorm`, `RopeTable`, `IntExp`, `IntRsqrt`, `Rescale` | **floor** (arithmetic shift, toward −∞) |

Everything else — the `int8 × int8` products, the accumulations, the adds — is exact.

An earlier version of this heading claimed a single round-half-away rule happening "only in
`RoundingShiftRight`", with "every other integer operation is exact". Both clauses were false, in
opposite directions, and each was load-bearing for someone: a third party implementing `SRDHM` from
the first clause rounds half-away and is convicted (see C2), and one reading the second clause looks
for exactness in the `>> k` steps that in fact floor. The table is the contract; the prose below
gives each rule's reason.

```
RoundingShiftRight(x, s) -> i32                   // s in 0..=31
    if s == 0 { return x }
    let magnitude = |x|                           // widened; |i32::MIN| does not fit i32
    let rounded   = (magnitude + 2^(s-1)) / 2^s   // exact division; the numerator is >= 0
    if x < 0 { -rounded } else { rounded }
```

`RoundingShiftRight` rounds half **away from zero**: `RSR(3,1) = 2`, `RSR(-3,1) = -2`, symmetric
about zero. What makes an exact-bits second implementation tractable is not that there is one rule,
but that the set of lossy sites is closed and each one's rule is pinned — a `>> k` that floors is
just as reproducible as a rounding one, provided the specification says which it is.

**`SRDHM` (C2) rounds half UP (toward +∞), not half-away — and this is intentional.** Its asymmetric
nudge `1 − 2^30` composed with truncation gives `SRDHM(-1, 2^30) = 0` where half-away would give
`-1`. The two rules diverge on exactly the negative exact-half products (`|a·b| ≡ 2^30 mod 2^31`),
which are freely constructible (take any `b = 2^30`), not statistically rare. `SRDHM` must round
this way because C2's entire purpose is bit-identity with gemmlowp, which rounds half-up; changing
it to half-away to match the other rule would break the property C2 exists for. A third party
implementing `SRDHM` from the old single-rule heading would have produced half-away and disagreed
with gemmlowp (and with the reference) on every negative exact-half, which under ADR-0027's court is
a conviction, not a rounding difference. `misaka-palw-base0-ref2`'s differential cannot surface
this: both sides derive from the same gemmlowp, so both are half-up; only the normative text was
wrong. Verified independently by exhaustive comparison against the vendored upstream on the
exact-half family.

**Round the MAGNITUDE, then reapply the sign. Do not write `(x ± 2^(s−1)) >> s`.** That form was
this ADR's original pseudocode and the first implementation followed it, and it is wrong for every
negative input: an arithmetic shift floors, so for `x < 0` the nudge and the floor push the same
way instead of opposing. `RSR(−64, 1)` returns `−33` where the exact quotient is `−32` and needs no
rounding at all. Measured against gemmlowp's `RoundingDivideByPOT`, the two disagreed on **50 % of
random `(x, s)` pairs** — every negative one — and the same form overflowed `i32` on a further
3.2 %, wrapping the sign of the largest accumulators. Found by the second implementation
(`misaka-palw-base0-ref2`) on its first run. Note the two failures are distinct: *this* one was the
pseudocode contradicting a correctly-stated rule (half-away for `RoundingShiftRight`), whereas the
heading's "one rule, one site" universal was the statement itself being wrong.

**C2. `SaturatingRoundingDoublingHighMul` is the one fixed-point multiply.**

```
SRDHM(a: i32, b: i32) -> i32
    if a == i32::MIN && b == i32::MIN { return i32::MAX }   // the single saturating case
    let p: i64 = (a as i64) * (b as i64)                    // a·b — NOT 2·a·b
    let nudge: i64 = if p >= 0 { 1 << 30 } else { 1 - (1 << 30) }
    ((p + nudge) / (1 << 31)) as i32                        // TRUNCATING division, not a shift
```

This is gemmlowp's primitive verbatim, deliberately: it is already implemented identically in
several independent codebases, which is exactly the property a second implementation needs.

**The division truncates toward zero; it is not a shift.** The nudge is asymmetric — `1 − 2^30`
for negatives rather than `−2^30` — for exactly one reason: it compensates for truncation. Pairing
it with an arithmetic shift, which floors, applies the correction twice, and the first
implementation of this ADR did precisely that. Measured against upstream gemmlowp, `>> 31`
disagreed on **50.1 % of random `(a, b)` pairs**, every one a negative product, always one unit
further from zero: `SRDHM(−2^30, 2^30)` returned `−2^29 − 1` where the exact value is `−2^29`.

This mattered out of proportion to its size, and the reason is the paragraph below it: SRDHM was
chosen *because* it is already implemented identically elsewhere. A third party writing BASE-0
against real gemmlowp would have disagreed with the reference on half of all inputs — and under
optimistic verification a systematic disagreement is not a rounding difference, it is a conviction
and a slashed bond.

**The product is `a·b`, not `2·a·b`.** The "doubling" in the name describes the relationship to
the hardware `VQRDMULH` — `(a·b) >> 31` *is* `(2·a·b) >> 32` — it is not a factor to apply on top
of a 31-bit shift. A first draft of this ADR wrote the 2 explicitly and still shifted by 31,
which doubles every product: in Q31, `0.5 × 0.5` returns `0.5` instead of `0.25`, and
`~1.0 × ~1.0` overflows `i32` outright. Recorded because the error was made and because a second
implementation reading only the formula would reproduce it.

**C3. Overflow is impossible at accumulation and saturating at narrowing.**

Accumulation is `i32` and the shape profile must PROVE it cannot overflow. Every narrowing
(`i32 → int8`) saturates to `[-128, 127]`; nothing wraps anywhere. Wrapping would turn a
one-unit error into a full-scale one, and — worse for this design — it would break Decision E.

The operand type is therefore the whole of `int8`, so `|product| ≤ 128 × 128 = 16_384`, a dot
product of length `K` is bounded by `K × 16_384`, and the registration-time rule is:

```
K_max × 16_384  ≤  2^31 − 1    ⟹  K_max ≤ 131_071
```

A class whose graph exceeds that must accumulate in `i64` and declare it.

**Amended 2026-08-20: this clause derived the bound from `127 × 127 = 16_129` and gave
`K_max ≤ 133_144`.** That contradicted the saturation sentence directly above it in the same
clause — the narrowing produces `-128`, and `(-128)²` is wider than `127²`. The wrong figure was
never reachable (`d_model` and `d_ff` are thousands, not a hundred thousand) but it was not a
premise Decision E could use: it held over a subset of the operand type, while nothing range-checks
an artifact's weight bytes and the refutation path decodes operands with `i8::try_from`. Narrowing
the operand range to `[-127, 127]` instead was considered and rejected — it would change frozen
catalog op 2 for every input that currently saturates to `-128`, and would leave every entry point
that does not pass through `Requantize` still open. `MAX_DOT_LEN` is `131_071`.

## Decision D — The op set, closed and minimal

`PALW-BASE-0`'s graph is **not the float classes' graph in integers.** Integerising
GatedDeltaNet, interleaved-multimodal RoPE and fused SwiGLU would reproduce the catalog problem
this class exists to escape. The class is a plain decoder-only transformer whose op set is chosen
for closability:

| # | Op | Integer definition |
| --- | --- | --- |
| 0 | `EmbedLookup` | int8 row gather; no arithmetic |
| 1 | `MatMulQuant` | `i32 acc = Σ (int8 × int8)`, exact |
| 2 | `Requantize` | `Saturate8(RoundingShiftRight(SRDHM(acc, mult), shift))` |
| 3 | `RmsNorm` | integer mean of squares (i64 acc), then `IntRsqrt` (Decision F) |
| 4 | `RopeTable` | rotation by a **pinned integer sin/cos table** — see below |
| 5 | `SoftMax` | `IntExp` (Decision F) + integer sum + `IntRecip` |
| 6 | `Silu` | `x · IntSigmoid(x)`, where `IntSigmoid` reuses `IntExp` |
| 7 | `MulElem` | exact `i32` multiply, then `Requantize` |
| 8 | `AddElem` | exact `i32` add (scales pre-aligned at registration) |
| 9 | `Rescale` | `Saturate32(RoundingShiftRight64(acc · mult, shift))` — **added by Decision H**; unlike `Requantize` its gain may exceed 1 |

Ten kinds (nine as first frozen; see Decision H for the tenth and for why the nine could not compute), against the float vocabulary's seventeen. Two absences are deliberate and are the
whole point:

* **`RopeTable` has no `sinf`/`cosf`.** The rotary angles depend only on (position, dimension),
  both of which are bounded by the registered shape — so the table is *precomputed once and
  pinned as a registration artifact*, exactly like the model weights. A transcendental evaluated
  at registration is data; the same transcendental evaluated at inference is normative arithmetic
  that every implementation must reproduce. This converts ADR-0031's hardest surface into a hash.
* **`CpyF32F16` does not exist**, because no cache holds floats.

## Decision E — Reduction order is free, and this is the class's central property

Integer addition is associative and commutative **exactly**, with no rounding and no
contraction. Therefore:

> On `PALW-BASE-0`, the order in which a dot product, a norm sum, or a softmax denominator is
> accumulated **cannot change the result** — across thread counts, SIMD widths, tile shapes,
> compilers, or CPU vendors.

This is the single largest difference from every float class, where reduction order, FMA
contraction and threading are each an independent divergence source and each needs its own pin.
An entire category of cross-host disagreement is not mitigated here; it is *absent*.

**The property is conditional on C3 and that is why C3 is load-bearing.** Saturating addition is
NOT associative (`sat(sat(a+b)+c) ≠ sat(a+sat(b+c))` at the boundary), so associativity holds only
while accumulation cannot overflow. The registration-time bound is not a safety nicety; it is the
premise of Decision E. A class that needs `i64` accumulators must prove the bound there too.

## Decision F — The two integer transcendentals, as algorithms

Both are exact integer algorithms with a **fixed** iteration/term count. Fixed, not
convergence-tested: a loop that stops when it converges stops at different times on different
inputs, and "different times" is a divergence.

Both forms below were validated numerically before this ADR was written, at `k = 24`; the
measured accuracies are quoted so an implementation has a target to differential-test against
rather than a shape to guess at. **Two errors were found and corrected in that pass**, and both
are recorded because each would have shipped an algorithm that silently returns garbage.

**F1. `IntExp(x)` for `x ≤ 0`, in Qk fixed point.** Range-reduce by the pinned integer constant
`LN2_Q = round(ln 2 · 2^k)` (`= 11_629_080` at k = 24):

```
z = min(floor(-x / LN2_Q), Z_MAX)          // integer division, floor
p = x + z · LN2_Q                           // p ∈ (-LN2_Q, 0]
IntExp(x) = RoundingShiftRight(Poly2(p), z)

Poly2(p) = ((A · ((p + B)² >> k)) >> k) + C          // the SHIFTED-SQUARE form
           A = round(0.3585 · 2^k), B = round(1.353 · 2^k), C = round(0.344 · 2^k)
```

`Poly2` is `A(p + B)² + C`, **not** a Horner-form polynomial with coefficients `A, B, C`. The
distinction is the whole algorithm: the shifted-square form gives `Poly2(0) ≈ 1.0003` and
`Poly2(−ln2) ≈ 0.5000`, which are the two endpoints `exp` must hit, while reading the same three
numbers as `c₂p² + c₁p + c₀` gives `Poly2(0) = 0.344` — every result low by a factor of three,
uniformly enough to look like a scale bug rather than a wrong algorithm. *Measured*: max relative
error **0.0048** over `x ∈ [−16, 0]` wherever `exp(x) > 1e−5`, and **0.0021** across a
representative softmax row.

`Z_MAX` is pinned so the shift never exceeds 31; beyond it the result is 0, which is exact enough
because `exp(−Z_MAX·ln2)` is already below the Qk floor. Softmax subtracts the row max first, so
`x ≤ 0` always holds — that subtraction is part of the op, not an optimisation.

**F2. `IntRsqrt(v)` by Newton, with a pinned iteration count.** Normalise `v = m · 2^{2e}` with
`m ∈ [1, 4)`, iterate on `m`, then undo the normalisation by `e`:

```
y_0 = SEED[top-4-bits-of-m]                  // pinned table
y_{i+1} = (y_i · (3·2^k − ((m · ((y_i·y_i) >> k)) >> k))) >> (k + 1)
IntRsqrt(v) = y_N >> e         // N pinned; no early exit, no residual test
```

**The seed table is a correctness requirement, not an optimisation.** Newton for `1/√v`
converges only from `y₀ ≤ √(3/m)`; a seed above that basin diverges *to zero* rather than
oscillating, so the failure is silent and total. A first draft seeded from the leading bit alone
(`y₀ = 2^{(k−bit)/2}`) lands exactly on the boundary at `m = 3` and returned **0**. Every `SEED`
entry must therefore be the reciprocal square root of its bucket's **upper** end, which is
conservative by construction. *Measured with that table*: max relative error **6.4e−6** over
`v ∈ (0, 200]` at `N = 3`, and `N = 4` and `N = 5` are not better — so `N = 3` is the pinned
count, and a larger one buys nothing but time.

`IntRecip` for the softmax denominator is `IntRsqrt` composed with itself, or its own pinned
Newton iteration — chosen at implementation time and then frozen. Either is admissible; drifting
between them is not.

## Decision G — What this buys the catalog

The primitives a second implementation must reproduce are exactly seven:

```
integer add, integer multiply, RoundingShiftRight, RoundingShiftRight64, SRDHM, IntExp, IntRsqrt
```

(seven since Decision H; `RoundingShiftRight64` is the same rule at a wider type)

All six are total functions on `i32`/`i64` with no environment: no rounding mode, no denormals,
no errno, no libm version, no contraction flag. That is why this class's catalog can reach 100 %
while the float classes' cannot, and it is the reason ADR-0039 orders this class first.

`pwu_per_inference` (ADR-0038 D, `palw_pwu`) becomes exactly countable for the same reason: the op
count of one canonical inference is a property of the frozen graph, with no data-dependent
branches to estimate around.

## Decision H — `Rescale`, the tenth op: a scale change that is allowed to amplify

**Amended 2026-08-17, after building the engine. Decision D said nine ops; it is ten.**

### The defect

Decision D's op 2 is `Requantize(acc, mult, shift) = Saturate8(RoundingShiftRight(SRDHM(acc,
mult), shift))`. `SRDHM` *contains* a `>> 31` (C2). With `mult ≤ i32::MAX` and `shift ≥ 0`, the
composition's gain is therefore **at most 1** at every parameter setting — it can only attenuate.

But ops 5 and 6, `SoftMax` and `Silu`, are defined on **Qk** inputs, because both are built on
`IntExp` and `IntExp`'s domain is Qk (F1). And the accumulators that feed them do not reach Qk.
Measured on random `int8` rows:

| reduction | typical \|acc\| | in Qk |
| --- | --- | --- |
| attention logit, `d_head = 64` | 3.6e4 | **0.0022** |
| attention logit, `d_head = 128` | 5.0e4 | 0.0030 |
| FFN gate, `d_model = 2048` | 1.8e5 | 0.0110 |
| FFN down, `d_ff = 8192` | 3.9e5 | 0.0232 |

At those magnitudes the two ops degenerate:

* `SoftMax` over eight such logits returns `0.1248 … 0.1255` against a uniform `0.125` —
  **attention is flat and the keys are indistinguishable**.
* `IntSigmoid` returns `0.501`, so `Silu(x) = x · 0.501` — **SwiGLU's gate is linear and stops
  gating**.

So a conforming BASE-0 implementation, exactly as Decision D froze it, could be executed,
audited, bisected and convicted — and could not compute. The catalog was closed around a graph it
could not express.

This was found by writing the engine (`misaka-palw-base0`), not by review. It is worth recording
why review missed it: every op is individually correct, every op's own tests pass, and the
composition fails only through a *scale* relationship that no single op's contract mentions.

### The repair

`Requantize` composes two shifts, and the `>> 31` inside `SRDHM` is what makes the gain
one-sided. Doing the multiply and the shift **once**, in `i64`, removes that:

```
RoundingShiftRight64(x: i64, s: u8) -> i64      // the C1 rule, at 64 bits
    identical round-half-away-from-zero; C1 still describes ONE rule, at two widths

Rescale(acc: i32, mult: i32, shift: u8) -> i32  // op 9
    Saturate32(RoundingShiftRight64(acc · mult, shift))
```

The gain is `mult · 2^−shift`. Because `mult` is read as a Q31 fraction, **`shift = 31` is unity
and any `shift < 31` amplifies**, up to `2^31`. No new arithmetic concept is introduced: an `i64`
multiply and one rounding shift, both already normative.

`Rescale` is **not** `Requantize` with the clamp removed, and the two are not interchangeable:
`Requantize` rounds twice (at bit 31 inside `SRDHM`, then again at `shift`) and `Rescale` rounds
once, so they differ by up to one unit. `Requantize` keeps its exact frozen behaviour on the
`int8` narrowing path — re-expressing it through `Rescale` would move the value of every
already-pinned narrowing, which is a different and worse change than adding an op.

### Consequences

* The catalog is **ten** ops. Decision D's closability argument is unaffected: `Rescale` is total,
  environment-free, and reproducible from an `i64` multiply and a shift.
* The primitives a second implementation must reproduce become **seven**, adding
  `RoundingShiftRight64` — which is the same rule as `RoundingShiftRight` at a wider type, so the
  differential surface grows by a width, not by an algorithm.
* An artifact must carry the amplifying scales (`attn_logit_scale`, `ffn_gate_scale` in
  `misaka-palw-base0`), and they belong **inside the class digest**: they move every logit and
  every gate, so an artifact whose digest omitted them could be retuned in place while still
  claiming the class.
* The gain targets are calibration, not consensus. Swept and measured for the reference fixture:
  at `2^22` the gate's `|min|/max` is 0.55 — SiLU still near-linear; at `2^23` it is 0.28, which
  is SiLU's own floor of −0.278; above `2^25` the softmax collapses to a hard argmax. `2^23` is
  what the reference artifact uses.
* **A closed catalog is not evidence that the class can compute.** Every property Decision D
  claims held while attention was flat. The engine is therefore instrumented (`ForwardProbe`) so
  attention spread and gate asymmetry are *measured* rather than assumed, and the degenerate
  configuration is pinned as a test rather than left as a comment.

## What this ADR does not decide

* **The model.** Depth, width, vocabulary, the quantized weight artifact and its hash are
  registration inputs, not consensus rules. Post-training quantization of an existing model is
  expected and is a tooling question.
* **The exact constants.** `k`, `Z_MAX`, `LN2_Q`, the `Poly2` triple, `N` and the `SEED` table are
  pinned by the implementation and frozen at registration; this ADR fixes their *form*, the
  accuracy they must reach, and that they are frozen, because a constant that can drift is a
  divergence with extra steps. The `k = 24` figures in F1/F2 are the validated reference point,
  not a mandate — an implementation choosing another `k` re-runs the same measurement.
* **Performance.** BASE-0 is the slow floor by design (ADR-0039 §2a: degrade throughput, never
  difficulty).
* **Whether the float classes are ever completed.** They may remain uncatalogued and therefore
  weightless; that is ADR-0039's rule, not a new decision here.

## Consequences

* **A second implementation is tractable, is required, and has now been built.**
  `misaka-palw-base0-ref2` re-derives all seven primitives with exact `i128` division and **no
  shift operator anywhere**, and `tests/differential.rs` compares them at exact equality over ~3M
  inputs — exhaustive on small windows, complete on the type boundaries, then sampled.

  It earned its cost on the first run, finding three defects that the first implementation's own
  tests had passed: the C1 rounding rule (above), the C2 truncation (above), and a
  `RoundingShiftRight64` that **panicked on overflow** for inputs near `i64`'s ends — a panic
  reachable from a public function, which is the remote-halt failure mode `palw_base0_ops` refuses
  by construction.

  Two lessons generalise. First, all three defects were on **negative or extreme inputs**, and all
  three survived tests that used positive mid-range values; the first implementation's own
  negative cases were all exact halves, where the defective and correct forms happen to agree.
  Second, the second implementation must **re-declare the pinned constants rather than import
  them**: sharing them made a mutation of `RSQRT_ITERS` invisible to the differential, because
  both sides moved together. Constants are specification (F2), so they are compared, not shared.

* **Authorship independence is now held for C1 and C2, and only for those.** Upstream gemmlowp is
  vendored byte-identically at commit `16e8662c34917be0065110bfcd9cc27d30f52fdf` and called through
  an `extern "C"` shim that contains no arithmetic. The specification now agrees with it exactly
  over ~1.8M inputs for `SRDHM` and `RoundingDivideByPOT`, so C2's justification — *that this
  primitive is already implemented identically elsewhere* — is a measured fact rather than an
  assumption. It was not one before: the reference would have been convicting third parties for
  being correct.

  gemmlowp's fixed-point layer is header-only, so nothing of upstream's is compiled and there is no
  build configuration to reproduce — a stronger position than the vendored SoftFloat, where one
  must be. "Byte-identical, no edits ever" is enforced by pinned SHA-256 digests checked at test
  time rather than asserted in a README, because an oracle this project has edited is not an
  oracle.

* **Five primitives still have no third party, and that is now the whole of the gap.** `IntExp`
  (F1), `IntRsqrt` and `IntRecip` (F2), `Rescale` (H) and `RoundingShiftRight64` are this ADR's own
  definitions — the `Poly2` triple and the `IntRsqrt` seed table have no upstream at all. For those
  five, one author wrote both sides, so a misreading of *this document* would be reproduced rather
  than caught. The nearest candidates are the I-BERT reference for `IntExp` and a published
  integer-Newton `rsqrt`. Both are one-way: they can refute, and their agreement would be evidence,
  but neither is normative for a class this ADR defines.
* **ADR-0031 does not apply to this class.** Its canonical-transcendental machinery exists to pin
  libm; there is no libm here. The class identity records "no libm" as a fact rather than
  recording a version — and the `libm_transcribed` registry flag is trivially satisfiable for
  BASE-0 precisely because it binds no libm site.
* **The audit's B8 cannot occur on this class.** Two honest implementations cannot disagree by
  one ulp, so the false-positive conviction that would cost an honest operator a bond and void
  block weight is unrepresentable here — which matters most on the class that is the floor.
* **`PalwStepOpKindV1` gains a BASE-0 arm or its own vocabulary.** Nine kinds that do not map
  onto the float seventeen; whichever encoding is chosen, the two op sets must not share
  discriminants, or a step refutation could name an op the other class never ran.
* **The first honest end-to-end conviction becomes reachable.** With a complete catalog, a lie in
  a BASE-0 matmul is localizable to one step and convictable — which is Layer 1's goal and the
  precondition ADR-0039 puts on carrying weight at all.

---

*Amended by [ADR-0047](0047-palw-a16-activation-tier.md) (2026-08-21): the catalog gains op 5W
(`softmax_shifted` — the wide softmax whose max subtraction happens before the widening; measured
necessity at Qwen-scale logits, seven bits past the Qk domain) and the A16 activation tier
(`palw_base0_a16`). BASE-0's own ten-op profile, class id and behavior are unchanged. (This trailer
was written with ADR-0047 on `palw-mainnet-rc-integration` and is restored here with it.)*
~~~~~~~~
<!-- ADR_SOURCE_END:0040-palw-base-0-integer-arithmetic.md -->

</details>

<a id="source-0047-palw-a16-activation-tier"></a>

<details>
<summary>0047-palw-a16-activation-tier.md — 原文 L1–L102</summary>

[原文を開く](../adr/0047-palw-a16-activation-tier.md) · SHA-256: `de05f8d78f9c024bd21982205eefc92c27a64c8d3f2bf8fdf13e066f5c3b6da8`

<!-- ADR_SOURCE_BEGIN:0047-palw-a16-activation-tier.md -->
~~~~~~~~markdown
# ADR-0047: The A16 activation tier — sixteen-bit activations for the classes int8 cannot carry

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Residency note (2026-09-02).** This ADR was written on `palw-mainnet-rc-integration`
> (`d3880149`) and never reached `main`'s `docs/adr/`, although the tier it specifies did:
> `palw_base0_a16`, the `qwen25_a16_*` profiles, ADR-0052 ("Activations are A16 codes (ADR-0047)"),
> and code comments in `misaka/palw/base0/src/{artifact,engine_a16}.rs` and
> `consensus/core/src/palw_base0_a16.rs` all cite it by number. It is restored here verbatim so the
> number resolves. Later ADRs moved the A16 class, not this arithmetic: the registered class is
> the court-capable graph-v2 row (ADR-0067 — its `artifact_root` is the openable inventory root,
> not the flat digest), its step space is adjudicable end to end (ADR-0070), and its weight is the
> price of that certification (ADR-0069, on chain by ADR-0075).

**Status:** Accepted (2026-08-21). Amends ADR-0040's catalog by addition; BASE-0 itself is
untouched — its profile references none of the new ops, its class id and behavior are unchanged,
and it remains the liveness floor exactly as ADR-0039 ordered.

## Context: the measured ceiling

The Qwen2.5-1.5B class was built first on int8 activations, with every known remedy applied and
measured one at a time — static per-site calibration, per-channel norm scales carrying the LN
gain, SmoothQuant-style folds, split-scale attention logits, per-channel FFN rows, stream-bias
separation, per-row weight scales, saturation headroom, a per-position-zero scale column. Each
moved the fidelity score by one or two tokens. None was the budget, and the fake-quantization
ladder inside the f32 reference finally said why (top-1 agreement against the exact model over a
57-position prompt):

| activations (float-simulated, weights int8) | top-1 |
| --- | --- |
| dynamic 8-bit | 44/57 |
| dynamic 7-bit | 31/57 |
| dynamic 6-bit | **4/57 — the static-int8 engine's exact score** |
| 15-bit | 57/57 |

Static scales, power-of-two rounding and multi-stage requantization cost ~2 bits, so a static
int8-activation pipeline lands at effective A6 regardless of calibration quality. Separately,
one experiment identified the sink: W8A8 with ONLY position 0 clamped to the generic range
collapses 44/57 → 3/57 — the attention-sink token's giant channels are load-bearing through the
k/v cache, so any tier needs a per-position-zero parameter column.

Two candidate extensions were measured and **rejected**: a fine-row-normalized-by-full-rms norm
op (negligible gain) and dynamic activation scaling (lands at the razor-thin 44/57 and needs
runtime-scale plumbing through every op signature).

## Decision

1. **The A16 op set** (`palw_base0_a16`): `MatMulRequant`, `MatMulRequantRow`, `MatMulRescale`,
   `RmsNorm`, `Requant`, `AddElem`, `MulElem`, `Rope` — i8 weights per row, i16 activation
   codes, `i64` accumulation. Decision E carries to this width: `|w·x| ≤ 127·32767 < 2^22`,
   `A16_MAX_DOT_LEN` keeps every accumulation exact in `i64`, and the differential tests run
   interleaved/blocked/reversed reductions bit-identical.
2. **The boundary rule: `i64` never crosses a step boundary.** Step tiles ride 4-byte lanes, so
   every op whose intermediate exceeds 32 bits is FUSED with its narrowing — committed rows are
   A16 codes or Q24 `i32`, and the wide accumulator lives and dies inside one adjudicable op.
   This is why the tier's matmuls are fused rather than a bare dot, and why the committed output
   row (and therefore the class argmax, lowest index on ties) is defined over i16 logit CODES.
3. **Reuse over redefinition:** `rope_table`'s pinned table (head-tiled at the oracle; the tier's
   own `Rope` adds only the i16 saturation), `silu` (defined on the Q24 values `MatMulRescale`
   commits), the embedding gather, and op 5W (`softmax_shifted`) — which this ADR also
   registers in the catalog (`base0/softmax-shifted/...`, `up_bits` as a one-byte oracle row).
4. **One parameter store.** Every runtime parameter is an integer `(m: i64, shift ≤ 62,
   zero: i64)` triple (17 bytes on the wire), derived at conversion, held in the artifact's
   `a16_params` keyed exactly as the shape profile names them, digest-covered. The engine
   pre-resolves its tables FROM those bytes; the dispute oracle serves THOSE bytes. What the
   engine ran and what the court recomputes with cannot come apart.
5. **The sink lane is court-visible.** At position zero, a parameter name resolves with the
   `.sink0` suffix first (generic row on absence) — in `a16_row` and in the engine alike.
6. **The registry keeps its single-source invariant.** `KERNEL_CATALOG` holds the descriptors;
   `catalogued_kernel_ids_v1` (the coverage gate) and `a16_row` (the court) read the same table;
   `kernel_can_serve_node_v1` gained the tier's shape arms. `recompute_step_row_v1` exposes the
   dispatch for external verifiers.

## Measured consequences

* Formalized-engine fidelity (integer params, committed i16 logit codes): calibration prompt
  44/57 top-1, 56/57 top-5, ρ 0.863; held-out 39/48, 46/48, ρ 0.849 — the fidelity gate
  (top-1 ≥ 3/4, ρ ≥ 0.5) is green on both.
* `qwen25_a16_profile_v1` — 2/27/3 nodes, 100% servable, inventory closed over the graph
  including the implied `.a16` params names. The RC's two-class genesis registers THIS profile;
  the adjudicability boundary re-measured at 3,875,229 leaves for the whole-context job
  (tile 2,048 / ctx 2,048), inside the 2²² ladder.
* The artifact digest gained the `a16_params` presence tag, so derived ids and the RC floor
  root re-froze (`3713ae8e…6d9af6`) — the floor's LOGITS are unchanged; the rename is the
  review gate doing its job.

## What remains open (recorded, not hidden)

* ADR-0040's second-party caveat extends to the new primitives: the A16 ops are this tree's own
  definitions with differential self-tests; no third-party normative reference exists yet.
* The engine's attention arms call the ops per head; the court's canonical input set builds the
  same rows head-major. The loop-closing test pins the risky seams (sink resolution, params
  tiling, head-tiled rope) bit-for-bit; a full per-node replay of an entire job through
  `a16_row` remains future hardening.
* Decode-position embedding (G5d's second half) is inherited unchanged from the int8 record.
~~~~~~~~
<!-- ADR_SOURCE_END:0047-palw-a16-activation-tier.md -->

</details>

<a id="source-0050-palw-base0-residual-site"></a>

<details>
<summary>0050-palw-base0-residual-site.md — 原文 L1–L191</summary>

[原文を開く](../adr/0050-palw-base0-residual-site.md) · SHA-256: `575d07814066d0e3e48cbe8a67634085d16e9b8a3d104eb62bd75e9740961139`

<!-- ADR_SOURCE_BEGIN:0050-palw-base0-residual-site.md -->
~~~~~~~~markdown
# ADR-0050: The BASE-0 residual site — the narrowing that was never declared, and the amplification that was

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: **Proposed.** Changes no op, amends no catalog, and activates nothing. It fixes a declared
graph that cannot be adjudicated at its residual sites, and settles whether the residual add may
amplify.

Date: 2026-08-21

Relates to: ADR-0040 (`PALW-BASE-0`; Decision D's op set is **unchanged** by this, and Decision H is
the precedent that decides the amplification question), ADR-0049 (the adjudication contract — this
ADR is an instance of its Decision F, found by looking for one), `palw_base0_profile.rs`,
`misaka/palw/base0/src/engine.rs`, `consensus/core/src/palw_step_refute.rs`.

**The premises are untouched.** Block production stays a hash-independent PALW lottery; the
free-prompt lane stays. This is arithmetic inside one class's graph.

---

> **Landed (index reconciliation, 2026-09-02).** The residual site's narrowing nodes and their
> parameters are in the shipped floor profile (`palw_base0_profile.rs`:
> `blk.{layer}.attn_residual.scale` / `.requant` and `blk.{layer}.ffn_residual.scale` / `.requant`),
> so Decisions A and D are the shipped floor and the class id this ADR said would move moved with
> them. The Status line's "Proposed" is the 2026-08-21 state. Map: [`README.md`](README.md).

## Context — the question asked, and the larger one found underneath it

The question put to this ADR was: BASE-0's residual stream is `int8` because ADR-0040 Decision D
fixes `AddElem` at `i8 → i32`, a `Requantize` can only ever attenuate (its gain is
`multiplier / 2^shift` with the multiplier a Q31 fraction), and per-layer calibration against a real
checkpoint bottomed out at `shift: 0` on every layer but one while the residual peak still fell to
5 of 127. A decayed stream needs amplification. Does BASE-0 get an amplifying residual, and is that
an op-set amendment?

Investigating it found that **the residual site in the declared graph cannot be adjudicated at all
today**, which is a larger and more urgent problem than the one asked about.

### The residual path has no narrowing node, and the court reads it as `int8`

`palw_base0_profile.rs:246` declares the attention residual as a bare add:

```rust
plain(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, hidden, vec![9, PALW_STEP_INPUT_LAYER_IN]),
```

Node 11, the FFN norm, consumes it directly (`:249-257`, `input_refs: vec![10]`). And the court's
`RmsNorm` arm reads its input through `as_i8` (`palw_step_refute.rs:354-359`), where

```rust
let as_i8 = |row| i8::try_from(*v as i32).map_err(|_| InputSetNotCanonical("base0 int8 lane out of range"));
```

`AddElem` returns the **`i32` sum of two `int8` codes**, range `[-256, 254]`. So the moment the
residual carries anything past `±127` — which is exactly when it is carrying signal — the court
returns `InputSetNotCanonical` and the step does not adjudicate.

The FFN residual is worse: node 17 is `AddElem(16, 10)` where node 16 is a raw `MatMulQuant`
accumulator and node 10 is the raw sum above, so **both** operands are out of the lane the arm reads.

The engine does not have this problem, because the engine narrows:
`engine.rs:267` is `requantize_row_uniform(&add_elem(&h, &projected)?, residual_requant)`. The
engine performs a narrowing the profile does not declare and the court does not know about — which
is ADR-0049 Decision F's defect, and this is where it bites first.

### The amplification question, answered

The catalog already contains every op needed. `KDESC_BASE0_ADD_ELEM`, `KDESC_BASE0_RESCALE` and
`KDESC_BASE0_REQUANTIZE` are rows of the single-source `KERNEL_CATALOG`
(`palw_step_refute.rs:176-193`), each with a live adjudicator arm, and `kernel_can_serve_node_v1`
already admits a `Rescale` node in the same arm as `Requantize` and `Rope` (`:262-268`). The
step-leg machinery carries this shape elsewhere in the same graph: `input_refs` below
`PALW_STEP_INPUT_SENTINEL_MIN` are intra-table node indices, and BASE-0's lane is
`PalwStepLaneV1::Int32`, so a node consuming another node's `i32` output is ordinary.

The op set is frozen at ten in one place (`KDESC_BASE0_ALL`, held by an assertion at
`palw_step_refute.rs:2330`) and the graph is a per-class function of geometry in another
(`base0_profile_v1`). **A residual `Rescale` changes the second and not the first.**

Decision D's own text settles it: op 8 is *"`AddElem` — exact `i32` add (**scales pre-aligned at
registration**)"*. A per-layer gain that is a registration artifact **is** scale pre-alignment. And
Decision H already ruled on this exact class of change when it added `attn_logit_scale` and
`ffn_gate_scale`, calling the gain targets *"calibration, not consensus"*. A residual scale is the
third member of that set.

---

## Decision A — the residual site gains its narrowing node, and this is a correctness fix

Each residual site becomes, in the profile and in the engine identically:

```
AddElem(h, projected)  →  Rescale(·, residual_scale[site])  →  Requantize(·, residual_requant[site])
```

The `Requantize` is **not optional and not new** — the engine has always performed it. Declaring it
is what makes the profile describe the computation the engine runs, and what stops the court from
reading an `i32` sum as an `int8` lane.

This is a defect fix, not a feature. A graph whose residual sites do not adjudicate is a graph whose
class cannot be weight-bearing, and BASE-0 is the liveness floor.

## Decision B — the residual may amplify, and its gain is a registration artifact

`Rescale` sits between the add and the narrowing so a decayed stream can be lifted before it is
re-quantized. The gain is per layer and per site, frozen at registration, inside the artifact
digest, and it is **calibration** in Decision H's sense: chosen by measuring what a class's own
layers produce, not computed from the data at inference time.

No op is added. ADR-0040's Decision D table is unchanged, and this ADR asks only for an additive
note under Decision H's consequences naming the residual as a third amplification site.

## Decision C — the gain is per tensor, per layer, per site — not per channel

The court's `Rescale` arm reads exactly one parameter triple for the whole node
(`palw_step_refute.rs:418-439`: `weight_row(name, layer, 0, 1)`, then `row.len() != 5` refuses
anything else). A per-channel residual gain is not expressible without changing that arm, and
changing it is an op-semantics change rather than a graph change.

Per-tensor is also the right granularity on the evidence: the measured collapse is a whole layer's
stream falling to 5 of 127, not a few channels diverging.

## Decision D — the new parameters join the artifact inventory as named tensors

`BASE0_TENSOR_NAMES` (`palw_base0_profile.rs:84-99`) gains, per layer and per site:

```
blk.{layer}.attn_residual.scale     ·  5 bytes  (i32 multiplier LE + u8 shift)
blk.{layer}.attn_residual.requant   ·  9 bytes  (i32 multiplier LE + u8 shift + i32 zero LE)
blk.{layer}.ffn_residual.scale      ·  5 bytes
blk.{layer}.ffn_residual.requant    ·  9 bytes
```

They must be **tensors** rather than struct fields because the court resolves a `Rescale` node's
parameters through `PalwWeightOracleV1` — the gain has to be openable against `artifact_root` or the
step is `Unadjudicable`. That is a constraint the design already imposes, and satisfying it is what
makes the gain a committed fact rather than a number a producer asserts.

## Decision E — this is blocked on ADR-0049 Decision A, and the block is not incidental

The `Rescale` arm asks the oracle for **one element** and then requires **five bytes**:

```rust
let row = weights.weight_row(node.weight_name.as_str(), layer, 0, 1)…;   // :420
if row.len() != 5 { return Err(InputSetNotCanonical("base0 rescale params are not 5 bytes")); }  // :421
```

`PalwProvenOperandsV1` returns `elements` **bytes** (`palw_artifact.rs:187`). So through the
production oracle this arm receives one byte and refuses every `Rescale` step. The existing test
`rescale_is_adjudicable_and_bounded` (`:1279-1315`) passes only because `FixedRow` ignores the size
it was asked for.

**Op 9 has never adjudicated through a real opening.** Landing Decisions A–D against the current
oracle would produce a graph whose residual sites are `Unadjudicable` for a second, unrelated
reason. ADR-0049 Decision A — byte addressing — is a prerequisite, and this is the concrete case
that proves it is not a tidiness concern.

---

## Consequences

* **`shape_profile_id` moves**, so `execution_class_id` moves, so the RC genesis artifact re-mints.
  Nothing is registered yet, which is why this is the moment. The court **ladder** is unaffected —
  gate 5 provisions at `PALW_STEP_MAX_LEAVES` regardless of the class.
* **Two nodes per layer are added to the step space**, so `step_leaf_count` rises and every leaf
  count, canonical `pwu_per_inference` and `tile_len`/`n_ctx` admissibility figure must be
  re-measured before a class using this graph is registered.
* **The residual calibration must be re-run**, because the parameter it solves for changed shape:
  it now chooses a gain and a narrowing per site rather than a narrowing alone. The prior finding —
  that per-layer narrowing alone bottoms out at `shift: 0` — is what motivates the gain, and does
  not predict its value.
* **Amplifying a decayed stream amplifies its quantization error by the same factor.** A stream that
  has fallen to `±1` carries about one bit, and a gain restores its range without restoring its
  information. The gain is therefore a remedy for a stream that is *small*, not for one that is
  *dead*, and calibration must be checked against `ForwardProbe` rather than against the peak alone.

## What this ADR does not decide

* **The value of any gain.** That is a measurement per class, and for the first real artifact it is
  part of the PTQ pipeline's output.
* **Whether `AddElem` should widen.** Widening the residual to `i16`/`i32` is a genuine op-set
  amendment with a much larger blast radius, and it is not needed if Decisions A–D hold. If
  calibration after this ADR still cannot keep a deep class's stream healthy, that is the point at
  which widening earns its own ADR.
~~~~~~~~
<!-- ADR_SOURCE_END:0050-palw-base0-residual-site.md -->

</details>

<a id="source-0052-palw-qwen36-hybrid-class"></a>

<details>
<summary>0052-palw-qwen36-hybrid-class.md — 原文 L1–L258</summary>

[原文を開く](../adr/0052-palw-qwen36-hybrid-class.md) · SHA-256: `81e034aa79ea8fcb686aa370a158ab98694a3026b4b0123663d7a8513b92055d`

<!-- ADR_SOURCE_BEGIN:0052-palw-qwen36-hybrid-class.md -->
~~~~~~~~markdown
# ADR-0052: `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid graph

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed.** Registers no class and activates nothing. Specifies the ops a Qwen3.6
implementation must reproduce bit-for-bit, records what has been measured about them, and states
what is still missing before the class could carry weight.

Date: 2026-08-26
Relates to: ADR-0040 (the integer arithmetic this extends), ADR-0047 (the A16 activation tier),
ADR-0039 (which forbids weight without a complete catalog), ADR-0051 (execution families),
ADR-0027/0028/0030 (the court this class does not yet reach),
`consensus/core/src/palw_qwen36_ops.rs`, `misaka/palw/base0/src/qwen36.rs`.

> **Status as of the index reconciliation (2026-09-02).** "Registers no class and activates
> nothing … must not carry weight" is the 2026-08-26 morning state; the same-day Amendment below
> moved `consensus_params_id` and derived the genesis-form `ClassRegistered`. Since then the class
> has been re-described as the court-capable graph-v3 row ([ADR-0067](0067-classes-are-chain-data-kernels-are-the-build.md);
> the registered v1 row stays liveness-admissible and unprosecutable), its step space is adjudicable
> end to end ([ADR-0070](0070-the-model-tiers-step-spaces-are-adjudicable.md)), and it carries
> weight on testnet-11 as a certified family ([ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md),
> on chain by [ADR-0075](0075-certification-is-a-consensus-object.md)). The "ADR-0051 (execution
> families)" this file relates to was superseded the same day by [ADR-0053](0053-palw-one-execution-family.md);
> the A16 tier it cites is [ADR-0047](0047-palw-a16-activation-tier.md), resident since 2026-09-02.
> Map: [`README.md`](README.md).

## Context — what ADR-0040 declined, and why the bill came due

ADR-0040 named the three things it was leaving out: "integerising GatedDeltaNet, interleaved
multimodal RoPE and fused SwiGLU would reproduce the catalog problem". That was the right call for
a liveness floor whose whole selling point is a catalog that can close.

It is also exactly the bill for Qwen3.6-35B-A3B. Its forty layers are thirty GatedDeltaNet arms and
ten gated-attention arms, each followed by a 256-expert mixture:

```
layer_types      30 × linear_attention, 10 × full_attention (every 4th)
d_model          2048          vocab 248,320 (padded)
full attention   16 q heads over 2 kv heads, head_dim 256, partial_rotary 0.25 (64 rotated lanes),
                 QK-norm per head before the rotation, attn_output_gate: true
linear attention 16 key heads / 32 value heads @ 128, conv kernel 4, recurrent state per head
MoE              256 experts, top-8 + 1 shared, intermediate 512
```

The court's step space already names `GatedDeltaNet`, `SsmConv`, `RopeImrope`, `Glu` and `L2Norm`
— it was designed for this architecture. What has never existed is the integer arithmetic under
them.

## Decision A — Everything ADR-0040 says still holds

Integer-only, no libm, no float on the execution path. Activations are A16 codes (ADR-0047):
`i16` values in `i32` lanes. Scales are `(multiplier, shift, zero)` triples frozen at registration.
Every lossy site is named and each names its own rule. Reduction order is free, conditional on the
no-overflow bound, which every op below proves at its own entry.

## Decision B — The router is a SELECTION, and its tie rule is normative

Every op in ADR-0040's catalog is a total function of its input row. Two implementations that
disagree, disagree by a value, and the court localises it to one arithmetic step.

A top-8-of-256 router is not that. It makes a discrete choice, and a different choice means a
different expert's weights enter the next matmul — the outputs are then unrelated, no bisection
converges on an arithmetic step, and the disagreement reads as fraud on both sides.

**Ties break to the LOWEST expert index**, the rule the class's argmax already uses. This is not an
exotic case: Q[K] has 24 fractional bits, 256 experts routinely produce probabilities that underflow
it, and on a confident token the tail of the kept set is chosen among exact zeros by the index rule
alone. The kept set is returned in index order, not weight order — the combine is a sum and integer
addition does not care, but the committed row must have one order and index order does not change
when two weights are equal.

## Decision C — The combine has one accumulator

`Σ_e w_e · y_e` in `i64` across all `k` experts, narrowed once. Requantizing per expert and adding
afterwards rounds `k` times and makes the result depend on how the caller grouped the experts.

## Decision D — `IntLn`, the fourth transcendental

ADR-0040 Decision F gives the class `IntExp`, `IntRsqrt` and `IntRecip`. The decay gate is
`exp(−exp(A_log) · softplus(a))`, and with `c = exp(A_log)` and `u = sigmoid(−a)` the identity
`exp(−c·softplus(a)) = (1 + e^a)^(−c) = u^c` turns it into a power. At `c = 1` that is
`int_sigmoid`; at any other `c` it is `exp(c · ln u)`, and there was no logarithm.

`ln x = ln M + s·ln2` with `M ∈ [1, 2)`, and `ln M` by the atanh series in `t = (M−1)/(M+1) ∈
[0, 1/3]`, truncated after `t¹¹` — an error under two units of Q[K]. `t` is an exact `i128`
division rather than `int_recip`, whose three Newton steps would otherwise be the dominant error in
the series' own argument.

**A Newton refinement was tried and removed.** `y ← y − 1 + x·e^(−y)` from `int_exp` and
`int_recip` looked elegant and made the answer **fourteen times worse**: at `x ≈ 0.0045` the series
lands 494 Q[K] units out and the refined result lands 7,202 out. A Newton step squares an error only
when the function it evaluates is more accurate than the estimate it corrects, and `int_exp` is
4e−4 where the series is 5e−6.

## Decision E — The gated delta rule, and why an integer state is stable

```
S ← decay · S          (the gate)
w  = S k               (what the state already predicts for this key)
u  = β · (v − w)       (the correction, in v's units)
S ← S + u kᵀ           (rank-one write)
o  = S q
```

The worry with a recurrence in fixed point is that rounding compounds. It does not, and the reason
is structural: error injected at step `t` is carried forward **through the decay**, so by step `T`
it is worth `decay^(T−t)`. The recurrence is a contraction and its fixed point is not the errors'
fixed point.

**Measured** against an `f64` reference of the same rule: worst relative output error 9.1e−4 at 128
steps, 8.8e−4 at 512, 1.1e−3 at 2048 — flat in the sequence length. A sixteen-fold longer run costs
30 % more error, not sixteen times more.

Two consequences are load-bearing:

* **The gate must be a real multiply**, not a shift. A shift-only decay quantizes the contraction
  rate and, at `decay` near 1, quantizes it to 1 — which is where the argument above stops holding.
* **`‖k‖ = 1` is part of the definition**, not conditioning. The state's magnitude bound is
  `max‖v‖ · β / (1 − decay)`, derived from it, and that bound is what sets how many bits the state
  scale carries above the value scale.

The state's two narrowings are `i64` rather than the tier's `i128` `a16_scale_round`. A decay
touches `d_v · d_k` lanes per head, which at this geometry is 15.7 million narrowings per token; at
`i128` that is the whole token. The bound is proved at the op's entry instead of bought with a
width.

## Decision F — Partial rotation is not an optimisation

`partial_rotary_factor: 0.25` with `head_dim: 256` means 64 rotated lanes and 192 carried through
untouched. The unrotated lanes are position-independent by design; rotating them makes every one a
different number. A full-rotation implementation is a different model, not a slower one.

## What has been measured about the frozen arithmetic

Recorded rather than repaired — these values are the class id, and every op built on them inherits
their accuracy:

| primitive | measured relative error | note |
| --- | --- | --- |
| `int_exp` | up to **3.0e−3** | worst near `x = −0.53`, where its quadratic is weakest |
| `int_rsqrt` | 2.4e−5 near `2^25`, **1.0e−3** at `1e11` | depends on the argument's MAGNITUDE: the answer is Q[K] and a small answer has few significant bits there |
| `int_sigmoid` | 6.8e−5 high at 0, and **not monotone across the origin** | both follow from `int_exp(0)` overshooting `ONE` |

Two callers have to know the second one. `rms_norm` divides by the row length before calling
`int_rsqrt`; `L2Norm` takes the exponent out itself and applies it to the product, which is the
difference between a 0.22 % norm and a 0.02 % one.

And the third one is why `u^c` clamps to `[0, ONE]`: the argument reaches zero whenever `c · ln u`
rounds away, which on real weights is any head whose `dt` is strongly negative, and `int_exp(0)`
sits above `ONE`. The first run on a real checkpoint refused at the very first GatedDeltaNet head
for exactly that reason.

## What this ADR does not decide

**Calibration.** The scales in the converter today are derived from each site's fan-in, which is
the right shape — a random dot product grows like the square root of its length — and is not a
measurement. Fidelity needs a float reference of the hybrid graph and the ranges it produces. Until
then the class runs and its output is not claimed to be faithful.

**The court.** There is no step space for the hybrid graph: no coordinates, no tile leaves, no
refutation prover. `Qwen36Backend` takes the trait's honest defaults for `bisect_prefix_state` and
`refutation_for_index`, and ADR-0039 already says what follows — **no class may carry fork-choice
weight until its kernel catalog is complete**. A Qwen3.6 class registered on this arithmetic is
admissible for liveness and must not carry weight.

**The weight width.** `int8` weights make this model 33 GiB, which is more RAM than most machines
that would produce a block. A four-bit tier with power-of-two group scales would halve it and is
the same Decision E argument; it is not specified here.

## Consequences

* The arithmetic for the hybrid graph exists and is tested, including the recurrence, which was the
  one part that could have made the architecture impossible rather than merely expensive.
* A producer can run Qwen3.6 and commit to it. A court cannot yet adjudicate it, and the gate that
  keeps that from mattering already exists.
* Two ops in this set — the router's tie rule and the decay's clamp — are places where "close" is
  not a degraded answer but a different model. Both are stated as rules rather than left to an
  implementation's judgement.

## Amendment (2026-08-26, same day): the two "not decided" items above are now decided

Both undecided sections closed the same day, by measurement rather than by plan.

### Calibration → the class is faithful, and the check that proved it is now part of the tool

The converter calibrates from a float reference of the hybrid graph over a real prompt, and the
40-layer class measures **cosine 0.9967 / rank correlation 0.9598 / top-1 151/155** against it —
after which text in, text out works ("日本で二番目に高い山は北岳です。", stopped on the stop
token). The decisive instrument was not any per-site cosine: **an architecture error shared by the
reference and the engine is invisible to every differential**, and the whole model sat at chance
(median rank of the true next token 123,653 of 248,320) while every site read 0.99+. The converter
therefore prints, on every run, whether the f32 reference predicts its own calibration text
(top-1 72.1 %, median rank 0 once two tensor-layout misreadings were fixed). `--reference-only`
runs that check in five minutes without writing the artifact.

### The court → the catalog is complete and the admission gate passes

The paragraph above said "no step space: no coordinates, no tile leaves, no refutation prover."
All three exist now:

* **Twenty-three descriptors** — the A16 tier's nine and this graph's fourteen — in
  `palw_step_refute`'s catalog, each naming only its accumulator width, because integer addition
  leaves an op no other degree of freedom. The adjudicator calls the SAME functions the engine
  calls; there is no second implementation to diverge.
* **The profile is projected from an IR that transcribes the engine's own step order**
  (`palw_qwen36_profile`), 48 nodes per recurrent layer and 47 per attention layer. The mixture is
  six nodes: eight experts against one input row are arithmetic-identical to one concatenated
  matrix, and which eight is the committed router row's answer.
* **The recurrence is adjudicated by genesis-anchored replay**, exactly as the float `GdnCore` is:
  five committed rows per position, the court replays the state. The registration-opaque
  checkpoint sentinel stays refused; a registered state chunk map later makes replay
  checkpoint-anchored. The conv window is the last four positions' projections, a per-ref
  position set like the KV arms'.
* **Per-head structure lives inside the kernels** (L2, the wide norm, the recurrence): a one-head
  node would declare a step space that does not contain heads 1..31 — the exact defect the
  `KvPerHead` width fixed for attention.

`the_admission_gate_admits_this_class` runs the whole of `verify_class_admission_v2`'s
deterministic branch: shape validation, both coverage gates, ladder depth, court-cost ceilings and
the PWU recount. Two profile constants were set by that gate, not by preference: **tile_len 512**
(at 256 the worst case is 4,198,428 leaves against the ladder's 2^22 — over by 0.1 %) and
**n_ctx 256** (the step space is linear in context; the runtime's rotary table still covers 512,
and the bound prices the JOB a claim may declare, which the canonical 8+2 job is nowhere near).

`qwen36_registration_v1` derives the profile, catalog entry and genesis-form `ClassRegistered`
from one geometry. The court catalog is consensus — growing it moved testnet-11's
`consensus_params_id`, declared as a coordinated upgrade in the pin's own comment.

### Still open

The producer-side step-leg capture (the material an honest producer answers a bisection with) is
not yet emitted by `Qwen36Engine` — the profile above is its specification. And the weight-width
question (a four-bit tier) remains unspecified, now with the measured note that expert residency
plus the NEON grouped kernels took the 33 GiB class from 0.4 to 1.75 tok/s on a 24 GiB machine.

*2026-09-11:* that residency cache (`Qwen36Residency`, `MADV_WILLNEED` admission on the engine)
was never wired into the producer, and on the fleet's virtio disks `MADV_WILLNEED` prefetches
nothing — a draw read 12.8 GiB through three million page faults and took twenty minutes.
[ADR-0112](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
replaces it: residency is the artifact's, the always-set is read once through the file descriptor
and pinned, routed experts are read as the router chooses them and held under a budget the
operator states, a fifth of the artifact by default.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0052-palw-qwen36-hybrid-class.md -->

</details>

<a id="source-0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width"></a>

<details>
<summary>0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md — 原文 L1–L177</summary>

[原文を開く](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) · SHA-256: `edf43de1ee0161199aabc358828116cc7262f8b3a6e94fe78e4636475001e580`

<!-- ADR_SOURCE_BEGIN:0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md -->
~~~~~~~~markdown
# ADR-0116 — An attention history is the class's, and the held regime reduces over its own width

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0103 §10.7 recorded a decision to keep the bound; the operator then
  reversed it and asked for the change ("A16 の履歴上限 2^18 … の実装を行う方針に変更する").
  Consensus-inert on every shipped network: the bound a class reads is fixed by the held map
  registered in its class id, and the gate admits a held class only while `Params::palw_held_context`
  is armed. No preset's fingerprint moves.
* Builds on: [0040](0040-palw-base-0-integer-arithmetic.md) (Decision E: exact integer accumulation within
  a proven bound, which the A16 tier inherits), [0082](0082-the-close-is-flat-in-the-context.md)
  (the fused attention site and its tile route), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime, whose widest context is 2M), [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (the context vectors that found the wall).
* Reverses: ADR-0103 §10.7's "Decided 2026-09-11: the bound stays."

## 0. The sentence this ADR is

**The longest attention history an A16 op reduces over is a property of the class, not of the
projection tier: a class under the held regime reduces over the regime's width, `2^21` positions,
and every other class over the `2^18` it has always had. The number is read from the class's
registered profile by every engine, seat and court that runs it, so no two nodes can read it
differently, and a held class whose context is past its bound is refused at the gate by name.**

## 1. What was found

ADR-0103 §10.7 — "the eighth wall". The A16 tier bounds every reduction at `A16_MAX_DOT_LEN = 2^18`,
a number chosen for projections ("one power of two above the largest real reduction in the family").
W10 (`a16_attn_values`) and the fused site reused that constant as the bound on the attention
HISTORY, so no A16-family class could execute, replay or adjudicate past position 262,143, and the
2M context the held regime exists for failed at its 262,145th row. The shipped networks never met
the wall, because a free prompt there is at most 4,096 ids. The held regime did: `0110-dense-v7-2m`
was refused before produce, by name, and ADR-0103's claim that every wall admits at 2M was wrong
about one wall that was not a chain wall.

Three facts about the bound decided how it could move:

* **It is in no hashed descriptor.** The kernel descriptors do not encode it, `kernel_semantics_id`
  is a hash of the descriptor string, and the court catalog root is a hash of the ids. Changing the
  constant moves no id, root or fingerprint, so changing it alone would be a silent divergence
  between builds wherever a history past `2^18` is reachable.
* **It is shared with the matmuls.** The fast kernels' `sdot` path skips chunking under a
  compile-time assert that fails above 263,191. The projection bound cannot move with it.
* **A history past `2^18` is reachable only under the held regime.** Every other class's context is
  bounded by the enumeration ceiling (`n_ctx × layer_count`) and the ladder. Only a class that
  registered a held map has its context bounded by the ladder's depth, which admits 2M.

## 2. The requirement

A class under the held regime runs, replays and is adjudicated at every position its registered
context admits, up to ADR-0103's 2M, with the arithmetic exact under ADR-0040 Decision E at that
width. Every class that exists today computes and refuses exactly what it did before, and no node
can apply a different bound from another to the same class.

## 3. Decisions

**Decision 1 — the bound is read off the class.** `palw_state_chunk_map::palw_attn_history_bound_v1`
returns the held regime's `A16_MAX_ATTN_HISTORY_HELD_V1 = 2^21` for a profile whose state chunk map
is held, and `A16_MAX_ATTN_HISTORY_V1 = 2^18` for every other profile. The second is the old bound
byte for byte. It takes the profile alone because every engine, seat and court that runs a class
holds its profile and none of them holds a ruleset. The held map is inside the class id
(`shape_profile_id`), and the admission gate refuses a held class unless `Params::palw_held_context`
is armed, so the bound is the network's decision without being a value anyone could read
differently. No new fence: the held regime is what admits contexts past `2^18`, and its fence already
decides when that is true.

**Decision 2 — the ops take the bound, and the old names keep the old one.** `a16_attn_values_within`
and `a16_attn_fused_reference_within_v1` in consensus core, and `a16_attn_values_fast_within` and
`a16_attn_fused_uniform_fast_within` in the kernels, refuse a history past `min(max_history, 2^21)`,
so no caller widens past the held bound whatever it passes. The names without `_within` delegate at
`2^18`. The projection bound `A16_MAX_DOT_LEN` is untouched, and so is the tile route's per-tile
bound: a tile is a court parameter, not a history, and a whole history is folded from tiles.

**Decision 3 — every consumer reads the same bound.** The court's `AttnFused` and `AttnValues` arms
(`palw_step_refute::qwen36_row`) read the bound from the profile they already hold. The dense and
hybrid plans (`A16ProfilePlanV1`, `Qwen36ProfilePlanV1`) record it when they are compiled from the
registered profile, and every planned walk passes it to the attention kernels. The engines' own
unplanned paths, which exist only for the shipped canonical rows, keep `2^18`.

**Decision 4 — a held context past the bound is refused at the gate.** The held map lifted the context
ceiling; the ops still refuse past `2^21`. So `verify_class_admission_v8` refuses a held class that has
attention layers and a context above its bound, with both numbers and this ADR in the message. That
is better than a producer, a seat or a court discovering the wall at the class's first position past
it. The refusal is reachable only with the fence armed.

**Decision 5 — the vectors' wall is their class's.** `palw_context_vector_blocked_v1` reads the bound
of the held row every shipped vector runs. `0110-dense-v7-2m` is now inside it: its last position
attends to 2,097,151 rows. A job one row past the held bound is still refused before produce, by
name.

## 4. What it costs

* **Exactness: nothing.** A value product is below `2^30`, so `2^21` rows sum below `2^51`. A row of
  `2^21` Q24 exponents sums below `2^45`. Both are far inside `i64`, and a test runs the widest
  history at the extreme codes against the exact `i128` sum.
* **Precision, which is the model's arithmetic, not consensus.** At `2^21` rows, a near-uniform row's
  Q24 probabilities are about `8 / 2^24`, three bits each. Every node computes the same three bits, so
  nothing diverges; what a long, flat attention row loses is resolution. A probability format wider
  than Q24 would be new kernel semantics (a new descriptor, a new id), and it is not decided here (§7).
* **Memory:** the fused kernel's scratch rows are the history's own length, about 200 MB for twelve
  heads at `2^21`, for a class that asked for that context.
* **Time, which this ADR does not buy.** The seat's replay grows with the square of the width past the
  knee (ADR-0103 §10.6). The 131,072-position vector took 44.7 minutes, 819 s of it the seat's, on a
  12-core host (ADR-0110 §9.5). The bound moving makes 2M executable; it does not make it cheap.
  Running the 2M vector end to end is an external run of days, not a CI gate.

## 5. Invariants the tests hold

1. **I-1, the shipped bound is unchanged.** `a16_attn_values` still refuses `2^18 + 1` rows, and
   `A16_MAX_ATTN_HISTORY_V1 == A16_MAX_DOT_LEN`
   (`an_attention_history_past_the_bound_is_refused_and_that_is_the_eighth_wall`).
2. **I-2, the held bound is `2^21`, and exact there.** `2^18 + 1` rows and `2^21` rows are reduced,
   `2^21 + 1` rows are refused, no caller widens past it, and the accumulator equals the exact sum at
   the extreme codes (`the_held_history_bound_is_2_21_and_exact_there`).
3. **I-3, the fast kernels are still the catalog past the old wall**
   (`past_the_eighth_wall_the_held_kernels_are_the_catalog`).
4. **I-4, the bound is the class's.** The held dense and hybrid rows read `2^21`; the shipped dense
   row and the hybrid's graph-v6 read `2^18` (`the_attention_history_bound_is_the_classs`).
5. **I-5, the gate refuses a held context past the bound**
   (`a_held_context_past_the_attention_history_bound_is_refused_by_name`), and admits `2^21`.
6. **I-6, the 2M vector is inside the bound**, and a job one row past it is refused before produce
   (`the_2m_vector_is_inside_the_held_bound_and_one_row_past_it_is_refused_by_name`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0103 §10.7: "Decided 2026-09-11: the bound stays" | Decisions 1–5 |
| ADR-0110's `0110-dense-v7-2m`, "refused before produce, by name, until that wall moves" | Decision 5: inside the held bound |

## 7. What is deliberately not decided

* **A wider probability format.** Three-bit probabilities at `2^21` rows are exact and identical on
  every node, and they are coarse. A class that needs more resolution at that length needs a softmax
  and a requantization in a wider fixed point. That is a new kernel with its own descriptor, in the
  fenced table the way ADR-0102's lift is.
* **Arming the held regime anywhere.** This ADR changes what a held class can run; ADR-0103's fence
  still decides whether any network has held classes, and every shipped preset leaves it dormant.
* **Running the 2M vector end to end.** It is now possible and it takes days. Its first reproduction
  belongs with the 128K run's (ADR-0110 §9.5), as an external pin.

## 8. Number hygiene

Written as 0116 on `feat/adr-0103-held-context`, after `origin/main` (0114, 0115) and every local
branch were listed. 0113 was left unused by the writers of 0114 and 0115 and stays skipped. 0117 is
this line's other ADR of the same instruction. **The next free number is 0118.**

## 9. Implementation record (2026-09-11)

| Decision | where | what pins it |
|---|---|---|
| **1** the bound | `palw_state_chunk_map::palw_attn_history_bound_v1`; `palw_base0_a16::{A16_MAX_ATTN_HISTORY_V1, A16_MAX_ATTN_HISTORY_HELD_V1}` | I-4 |
| **2** the ops | `a16_attn_values_within`, `a16_attn_fused_reference_within_v1`; `kernels::{a16_attn_values_fast_within, a16_attn_fused_uniform_fast_within}` | I-1, I-2, I-3 |
| **3** the consumers | `palw_step_refute::qwen36_row` (`AttnFused`, `AttnValues`); `A16ProfilePlanV1::attn_history`, `Qwen36ProfilePlanV1::attn_history` | I-3, the plans' differentials |
| **4** the gate | `palw_class_admission_v2` (the held arm) | I-5 |
| **5** the vectors | `context_vector::palw_context_vector_blocked_v1` | I-6 |

The suites: consensus core (the A16 op, held, step-refute, chunk-map and admission modules) 109
passed; the held integration suite 11 passed; base0's kernels, engines, plans and vectors 40 passed.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md -->

</details>

