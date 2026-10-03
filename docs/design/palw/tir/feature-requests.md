# PALW-TIR — feature requests from the architecture corpus (lane H → lane G)

| Field | Value |
| --- | --- |
| Status | from the 100-entry measurement of [`corpus-v2.md`](corpus-v2.md); every request names the entries that need it and how to verify it |
| Rule | **features, never family code**: each request is a generic addition to the vocabulary of `model/features.rs` (a spec field with a default, a generic lowerer, an adapter-language surface), decomposed into the existing 25 primitives. A request that needs a primitive says so and names the minimal GENERAL one; none of the requests below does |
| How to verify | every request lists acceptance entries: `PALW_CORPUS_ONLY=<id> cargo test -p misaka-palw-tir-lower --test corpus_v2 corpus_v2_full -- --ignored --nocapture` must show `level B` (or `A`) and no failed stage. The third-party adapters that already exist for them are in `misaka-palw-tir-lower/tools/corpus/adapters/` |
| Size | a rough guess of the lowering lane's effort: S days, M one to two weeks, L weeks, XL months |

Reading guide. "Evidence" says what was *run*: **adapter** = a third-party adapter was written and the harness
shows the single failing step; **patched** = with the one storage difference removed in a copy of the fixture
(`tools/corpus/patch_checkpoint.py`) every stage holds, court included; **analysis** = read from the transformers
code, not yet run. Semantics are cited as `models/<arch>/modeling_<arch>.py:<Class.function>` in transformers 5.17.0.

## Decisions (coordinator, 2026-10-01)

| decision | what |
| --- | --- |
| Order for lane G, after its CP2 | **FR-01** weights as data (any remaining Rust weight mapping blocks permissionlessness) → **FR-18** encoder–decoders as data → **FR-17** rows-mode encoders → **FR-19** vision towers and convolutions (this also feeds audio) → **FR-02** post-rotation q/k norm → **FR-09** DeepSeek sparse attention |
| Owner of FR-22 and FR-23 | RFC-0003's lane D. The FR-22 sizing discrepancy (box demand in the code against element-exact demand and a 16.8 MB close in RFC-0003 §II.1.5.4, with PALW-TIR-38's 3.2 MB carried close) was forwarded to D |
| FR-25 accepted | a user-supplied adapter may override a built-in refusal, provided it passes the same validation; the report says "user adapter overrides built-in refusal: <the refusal>" (the harness does, `refusal_overridden`) |
| FR-26 accepted | a Level A without a reference check is labelled "A (unconfirmed)" until a transformers/diffusers class confirms it (the harness does, `level_label`; confirmation is `float_vs_hf` on the same weights) |
| Not adopted | per-layer-type rope in the standard template: the candidate (`tools/corpus/a-candidates/standard-v3.json`) moved none of the six families it targets, so there is no evidence. The convention search stays a development tool in `tools/corpus`, documented in `corpus-v2.md` §6, and is not a product feature |

## Status on `tir/generic` (measured by the corpus lane after the merge of `c3f1c27ca`)

| request | status | evidence |
| --- | --- | --- |
| FR-01 weights as data | **landed** (`spec.hf.weights`) | `dbrx`, `ernie4_5_moe`, `granitemoehybrid` Level B (built-in); this lane's `aria_text`, `codegen`, `granitemoeshared` (real shared expert) Level B by weight expressions |
| FR-02 post-rotation q/k norm | **landed** (`AttnSpec.qk_norm_after_rope`) | `hunyuan_v1_dense`, `hunyuan_v1_moe` Level B by this lane's adapters with the flag |
| FR-07 softmax selection bias + dropped-flag guards | **landed** | `ernie4_5_moe` Level B |
| FR-18 phase 1 encoder–decoders as data | **landed** (adapters of kind `encdec`) | `t5`, `t5_gated`, `bart`, `mbart`, `marian` Level B, lower + admit only; spec equals the Rust parser's up to one ulp of `1/sqrt(d)` (fixed on c3f1c27ca via detmath: the harness compares at 1e-12 and no longer sees a difference) |
| FR-25 refusal override | **landed** | `minicpm3`, `codegen`, `granitemoehybrid` carry "user adapter overrides built-in refusal" |
| FR-26 Level A honesty, panics | **landed** (the `albert` panic is a refusal now: stops at `lower`) | harness `level_label`; `albert` |
| FR-29 head transform | **landed** (`HEAD_TRANSFORM_V1`) | `modernbert_decoder` Level B, every stage incl. court (third-party adapter) |
| FR-30 separate output gate | **landed** (`ATTN_OUTPUT_GATE_SEPARATE_V1`) | `afmoe`, `laguna` Level B |
| FR-31 attention value scale | **landed** (`ATTN_VALUE_SCALE_V1`) | `mimo_v2_flash` Level B (and the `_unscaled` variant) |
| FR-35 rotation by -theta | **landed** (`ROPE_REVERSED_V1`) | `nanochat` Level B |
| FR-04 differential attention | **landed** (`AttnSpec.differential`, `ATTN_DIFFERENTIAL_V1`; lane R2-A) | `diffllama` Level B built-in adapter, every stage incl. court: float vs transformers 3.6e-7 relative, integer KL 9e-4, three-way identical. No primitive: the double-wide value is Slice/Concat (a tree, at most 8 inputs a node), λ and 1−λ_init are `[1]` constants made at conversion by the new `Src::Combine`. One deviation from the FR text: both halves are narrowed by the attention before the subtraction (the integer difference of two `i16` contexts), not subtracted inside one accumulator; the tiny fixture's KL shows no loss |
| FR-06 mixture of attention | **landed** (`AttnSpec.moa`, `MIXER_MOA_V1`, `MLP_MOE_OUT_BIAS_V1`; lane R2-A) | `jetmoe` Level B built-in adapter, every stage incl. court. The FR's two factored ops (`ExpertLinear`, `WeightedSum`) plus `Transpose01` (heads laid out `(kv head, slot)` so the grouped read lands on HF's tiled kv head) |
| FR-08 zero-computation experts, shortcut MoE | **landed** (`MoeSpec.zero_experts`, `Ffn::MlpShortcut`, `MLP_MOE_ZERO_EXPERT_V1`, `FFN_SHORTCUT_MOE_V1`; lane R2-A) | `longcat_flash` Level B built-in adapter, every stage incl. court (float vs transformers 7e-6). Deviations from the FR text: the identity share is one more ROW of the experts' single combine (the input at the experts' output scale, weighted by the summed weights of the identity slots) rather than a `Select` per slot; the side value is a carry of its own at the residual's scale (`CarryDecl::resid`) instead of `LayerSpec.side`; the two LoRA scales and the layer folding are weights as data (`Scale` maps, `{L/2}`/`{L%2}` name placeholders), so `MlaSpec.q_post_scale` and `layer_fold` were not needed |
| FR-21 cross-attention layers | **text-only stage landed**, states path open (`Mixer::CrossAttention`, `VLM_CROSS_LAYERS_SKIPPED_V1`; `ATTN_CROSS_V1` stays `Specified`) | `mllama`: Level B as the Llama text stage — HF skips every cross layer when no states are bound, so the program omits them (the spec keeps them and the model's layer numbering; tensors dormant; image rows bound to the spec are refused by name). Not built: the layers WITH vision states (the projected tower rows as a declared input, a stage-0 K/V stack) |
| FR-16 Kimi delta attention, NoPE latent attention | **landed** (`Mixer::Kda`, `MlaSpec.rope: None`; no primitive) | `kimi_linear` Level B, every stage incl. court; `generic-frontend-v1.md` §9.14 |
| FR-10 DeepSeek-V4 set | **landed** (nine features: `Residual::Mhc` + `ModelSpec.mhc`, `Mixer::SharedKv` ([`SharedKvSpec`]: q low-rank, K = V, grouped output, sinks) with `CompressedKvSpec` (HCA/CSA) and `EntryIndexerSpec`, `Scoring::SqrtSoftplus[Hash]`, `Glu::LimitedGlu`; HL `MhcMap`, `MhcPre`, `WindowWrite`, `WindowPool`, `EntrySelect`, `EntryAttention`; `lower/dsv4.rs`) | `deepseek_v4` Level B (built-in `deepseek-v4`): float reference = transformers to 1e-6 over ten positions with the CSA indexer selecting 4 of up to 5 entries and both compressors closing windows; integer vs transformers top-1 1.00 / KL 5e-5; ref = ref2 = exec; court replays every node. Deviations from the request, recorded: the mixing weights `pre`/`post`/`comb` leave `MhcMap` as fixed-point codes (Q14/Q15) rather than as committed Q24 vectors; a mHC layer is three blocks (mixer-site weights and query; attention; FFN site) — four for a compressed layer (the compressors and the indexer's rows in a block of their own) — handing values on in the carry past the streams, and identical blocks of different layer kinds are one block (the FFN site does not depend on the attention kind); with an indexer the attention gathers the `topk` named rows of the store (a terminal tile is `heads·topk·head_dim`, not `heads·blocks·head_dim`); `tid2eid` is an `I64` checkpoint tensor read as exact floats; FP8/FP4 hub checkpoints stay refused by the storage rules |
| FR-12 Gemma-3n set | **landed** (`Residual::AltUp`, `ModelSpec.altup`, `MlpSpec.sparsity`; HL `StreamMix`, `RmsMatch`, `GaussianTopK`; `lower/altup.rs`) | `gemma3n_text` Level B (built-in `gemma3n-text`): float reference = transformers to 1e-5, integer vs float top-1 1.00 / KL 5e-4, ref = ref2 = exec, court replays every node; the real E4B text decoder lowers and is admitted. Deviations from the request, recorded: LAuReL is a field of `Residual::AltUp` (not `LayerSpec`: a `LayerSpec` literal is built in a dozen places); the K streams ride in a carry of `(K+1)·D` lanes (the extra slot is the layer's intermediate between its two blocks); the activation sparsity of a layer is DATA of the block (`Op::GaussianTopK::layers`), so dense and sparse layers run one block — a program is capped at 16 blocks and the real model has 8 layer kinds otherwise |
| FR-03, 05, 09, 11, 13..15, 17, 19..20, 22..24, 27, 28, 32..34 | open (some landed on other lanes; see each) | see each |

## Findings for lane G (from the re-measure on `239179132`)

1. **One ulp in the encoder–decoder adapters.** `read_encdec` builds `EncDecSpec` for `t5`, `bart`, `mbart` and `marian` equal to `parse_encdec`'s except a float scale: `head_scale` / `attn_scale` come out as `0.17677669529663687` (adapter arithmetic, `1/sqrt(d)`) against `0.1767766952966369` (the Rust parser's `powf(-0.5)`), and `0.35355339059327373` against `0.3535533905932738`. The harness accepts it (relative 1e-12) and records `ulp_only`; a Q-format conversion of the scale could turn it into a different program digest. Worth making the two spellings bit-equal (one expression for `d^-1/2`).
2. **The `LOGITS_Q24_V1` bound meets random fixtures.** The tiny random `minicpm3` reached 305 natural-log units (`scale_emb` 12, `dim_model_base` 256 multiplying the head input by 8) and `materialise` refused it ("hold |logit| < 128"). The census config now keeps the logits near 30, so this is a property of the fixture, but a registrant's own random-init fixture can hit it and the refusal names no way out; a hint ("scale the fixture's weights") would help.
3. **FR-35** — NanoChat's rotation sign (below): the only gap left once FR-02 is in, proved by a patched-reference variant.

## Ranked

<!-- BEGIN GENERATED: frs -->
| FR | feature | size | entries it names | entries |
| --- | --- | :-: | ---: | --- |
| FR-18 | encoder-decoders lowered from a spec (cross-attention, relative bias, local attention) | L | 7 | `t5`, `t5_gated`, `bart`, `mbart`, `marian`, `longt5`, `t5_encoder` |
| FR-22 | image generation: conv/GroupNorm/adaLN/timestep blocks, denoise loop, VAE | XL | 6 | `unet2d_condition`, `unet_sdxl`, `dit`, `flux`, `sd3`, `vae_kl` |
| FR-23 | audio: front end, Conv1d, codec/vocoder, audio input and output bindings | XL | 5 | `whisper`, `wav2vec2`, `speecht5`, `musicgen`, `encodec` |
| FR-17 | bidirectional encoders lowered from a ModelSpec (not BERT-shaped Rust) | L | 4 | `deberta_v2`, `albert`, `modernbert`, `nomic_bert` |
| FR-19 | vision towers and convolutions lowered from a spec | L | 4 | `clip_vision`, `siglip_vision`, `vit`, `resnet` |
| FR-01 | weights as data: tensor expressions (slice, reshape, squeeze, transpose, stack) in adapter `names` | M | 3 | `dbrx`, `ernie4_5_moe`, `granitemoehybrid` |
| FR-02 | q/k norm applied after the rotation | S | 2 | `hunyuan_v1_dense`, `hunyuan_v1_moe` |
| FR-03 | sub-layer norms (norm before o_proj, norm before down_proj) | S | 1 | `bitnet` |
| FR-04 | differential attention | M | 1 | `diffllama` |
| FR-05 | learned pointwise activation (xIELU) | S | 1 | `apertus` |
| FR-06 | mixture-of-attention (routed per-expert q/o projections) | M | 1 | `jetmoe` |
| FR-07 | selection bias under softmax routing (+ refuse a flag the lowerer would drop) | S | 1 | `ernie4_5_moe` |
| FR-08 | zero-computation experts and shortcut-connected MoE | M | 1 | `longcat_flash` |
| FR-09 | token-level top-k indexer attention (DeepSeek sparse attention) | L | 1 | `deepseek_v32` |
| FR-10 | DeepSeek-V4 set: hyper-connections, compressed attention, hash routing, sqrt-softplus | L | 1 | `deepseek_v4` |
| FR-11 | parallel mixers in one layer (attention + Mamba-2) and muP multipliers | M | 1 | `falcon_h1` |
| FR-12 | Gemma-3n set: AltUp, LAuReL, activation sparsity | L | 1 | `gemma3n_text` |
| FR-13 | shared attention block with per-depth LoRA (Zamba2) | L | 1 | `zamba2` |
| FR-14 | layers without a mixer (a layer that is one block) | S | 1 | `nemotron_h` |
| FR-15 | gated short-convolution mixer | M | 1 | `lfm2` |
| FR-16 | Kimi delta attention | L | 1 | `kimi_linear` |
| FR-20 | prefix-LM attention | M | 1 | `paligemma` |
| FR-21 | cross-attention layers in a decoder | L | 1 | `mllama` |
| FR-24 | a way to pin a remote-code reference | policy | 1 | `chatglm3` |
| FR-25 | user adapters override built-in refusals; drop the stale Granite-hybrid refusal | S | 1 | `granitemoehybrid` |
| FR-26 | lowerer robustness and Level A honesty (panics, dropped flags, unread tensors) | S | 1 | `albert` |
<!-- END GENERATED: frs -->

## Cheap and certain (adapter-language and lowerer details found by running the corpus)

### FR-01 `WEIGHTS_EXPR_V1` — weights as data (S–M, unlocks `dbrx`, `granitemoehybrid`, `ernie4_5_moe`)

**Problem.** An adapter's `names` maps a *role* to a tensor-name template; the binder (`hf_weights.rs`) knows a closed
set of layouts (`QkvLayout` ×4, `MlpLayout` ×4, `GdnLayout` ×2, conv1d, table shards) and hard-codes others (the
shared expert is always `MlpLayout::Separate`, `hf_weights.rs:612-626`). Storage that departs from them cannot be
written as data, so a model whose *only* difference is how its tensors are laid out needs a core developer.

**Instances found.**

| entry | storage | what the harness says | evidence |
| --- | --- | --- | --- |
| `dbrx` | experts flat: `ffn.experts.mlp.{w1,v1,w2}` each `[E·I, D]`; expert `e`'s gate is rows `[e·I,(e+1)·I)` of `w1`, up of `v1`, and down is rows of `w2` **transposed** (`modeling_dbrx.py:DbrxExpertGLU.forward`, `DbrxExperts.forward`) | `missing tensor transformer.blocks.0.ffn.experts.mlp.w1.weight` | patched: all stages hold (float vs HF 2.9e-7, 1,032 commit points replayed) |
| `granitemoehybrid` | the shared expert's gate and up fused: `shared_mlp.input_linear [2I, D]` (gate rows first) | `param moe.shared.up.w: checkpoint gives [32, 32], graph needs [16, 32]` | patched: all stages hold (float vs HF 4.4e-7, 1,104 replays, Mamba-2 + attention layers) |
| `ernie4_5_moe` | `moe_statics.e_score_correction_bias` is `[1, E]`, the graph wants `[E]` | `param moe.sel_bias: checkpoint gives [1, 8], graph needs [8]` | patched bind passes; the run then fails on FR-07 |
| `granitemoeshared` (real checkpoints) | the shared expert's gate and up fused: `shared_mlp.input_linear [2·shared, D]`; the default tiny config has `shared_intermediate_size = 0`, so the family itself is Level B in the census and the variant `granitemoeshared_shared` (`shared_intermediate_size = 32`) is the one that stops | the adapter's own check: `shared_intermediate_size > 0 … needs weights as data (FR-01)` | adapter |

**Proposal.** `weights::Src` is already a total, deterministic expression tree (`Tensor`, `Take{axis, Pick}`,
`Transpose`, `Stack`, `Reshape`, `Map`, `Quant`; `weights/mod.rs`). Give it a surface in the adapter language: a
`names` value may be an object instead of a template string, e.g.

```jsonc
"moe.gate": {"tensor": "transformer.blocks.{L}.ffn.experts.mlp.w1",
             "slice": {"axis": 0, "start": {"$mul": [{"$var": "E"}, {"$var": "inter"}]}, "len": {"$var": "inter"}}},
"moe.down": {"tensor": "transformer.blocks.{L}.ffn.experts.mlp.w2",
             "slice": {"axis": 0, "start": {"$mul": [{"$var": "E"}, {"$var": "inter"}]}, "len": {"$var": "inter"}}, "transpose": true},
"moe.sel_bias": {"tensor": "…e_score_correction_bias", "squeeze": [0]},
"moe.shared.gate": {"tensor": "…shared_mlp.input_linear", "slice": {"axis": 0, "start": 0, "len": {"$var": "shared"}}},
```

Operations: `slice` (a `Pick::Range`/`Strided`), `reshape`, `squeeze`, `transpose`, `stack` over `{E}`/`{H}`, `cat`; template
variables `{L}`, `{E}`, `{H}` as today; arithmetic from the existing expression language over config values. All are
exact copies (no arithmetic on weights), so conversion stays a pure re-indexing and the artifact tensor is the same.
Then the four enum layouts become built-in *adapter snippets* rather than Rust. Cheapest first step: honour the
`shared` expert's layout like the routed ones, accept an expert tensor given as one stacked flat tensor, and let a
checkpoint tensor bind to a graph param when the shapes differ only by size-1 dimensions.

**Acceptance.** `dbrx.json`, `granitemoehybrid.json` and `ernie4_5_moe.json` in `tools/corpus/adapters/` pass on the
unpatched fixtures (the last also needs FR-07).

### FR-07 `MOE_SELECTION_BIAS_SOFTMAX_V1` and a guard (S, unlocks `ernie4_5_moe`; protects every future MoE)

**Problem (a bug found by the corpus).** `RouterSpec.selection_bias` is read from the adapter and carried in the spec, but
the float reference (`float_ref/mod.rs::route`, the `Scoring::Softmax` arm: `(p.clone(), p)`) and the lowering
(`lower/mod.rs::lower_route`, `Scoring::Softmax => (probs, probs, 0)`) add the bias **only under `Sigmoid`**. With
softmax scoring the flag is dropped silently. ERNIE-4.5-MoE selects `topk(softmax(logits) + e_score_correction_bias)`
and weights the selected experts by the **unbiased** softmax renormalised by `max(Σ, moe_norm_min)`
(`modeling_ernie4_5_moe.py:Ernie4_5_MoeTopKRouter.forward`).

**Evidence.** adapter + patched bind: float vs HF rel 1.7e-1, 2 argmax differences.

**Request.** (1) Add the bias to the *choice* scores for softmax scoring as for sigmoid (one `Add` before the `TopK`; the
weights stay the unbiased probabilities). (2) Make the reader refuse (`NOT_LOWERABLE`, naming the flag) any spec flag the
lowerer would not apply — `selection_bias` with `TopKThenSoftmax`/`TopKThenSigmoid`, `jitter_eps` with anything but
`SparseMixer`, `per_expert_scale` where unsupported — so a dropped flag can never be a silent wrong model (FR-26).
`norm_eps ≤ 1e-9` is already accepted (`moe_norm_min = 1e-12` passes).

**Acceptance.** `ernie4_5_moe` (with FR-01).

### FR-02 `ATTN_QK_NORM_POST_ROPE_V1` (S, unlocks `hunyuan_v1_dense`, `hunyuan_v1_moe`)

`q = rope(q); k = rope(k); q = RMSNorm_head(q); k = RMSNorm_head(k)` — the per-head RMS norms (`query_layernorm`,
`key_layernorm`, gain `[head_dim]`) act **after** the rotation (`modular_hunyuan_v1_dense.py:HunYuanDenseV1Attention.forward`);
Qwen3's order is the reverse. Rotation preserves the L2 norm but the per-channel gain does not commute with it, so the
two orders differ (measured with the pre-rope approximation: 8.9e-2 and 4.6e-2 of the logit scale on the two fixtures).
**Spec**: `QkNorm.after_rope: bool` (default false, additive). **Decomposition**: the existing per-head norm node placed after
`Rope`; no new primitive; the rotated q/k are what the history keeps today, so the only change is the node order.
Also noted: Hunyuan's `rope_type: dynamic` with `alpha` is a *static* NTK (`base = θ·α^(d/(d−2))`, set at init), i.e. the
default rope at a different base — already expressible by `$pow` in an adapter — but beyond `max_position_embeddings`
HF recomputes with the factor-only dynamic rule and drops `alpha` (an inconsistency inside HF; lower only the window).
**Acceptance**: `hunyuan_v1_dense`, `hunyuan_v1_moe` (adapters exist; flip `after_rope` on).

### FR-03 `SUBLAYER_NORMS_V1` (S, unlocks `bitnet`)

BitNet b1.58 inserts a norm between the attention heads and `o_proj` (`attn_sub_norm = RMSNorm(hidden)`) and between the
gated product and `down_proj` (`ffn_sub_norm = RMSNorm(intermediate)`) — `modular_bitnet.py:BitNetAttention.forward`,
`BitNetMLP.forward`. **Spec**: `AttnSpec.o_norm: Option<NormSpec>`, `MlpSpec.inner_norm: Option<NormSpec>`; tensors
`self_attn.attn_sub_norm`, `mlp.ffn_sub_norm`. **Decomposition**: the RMS template over `heads·v_dim` / `intermediate`.
No primitive. (BitNet's *storage* — packed ternary weights with per-token int8 activation quantisation in the quantised
loader — is a quant-format question, separate; the bf16-master checkpoint lowers with the sub-norms.) Found because the
standard template read BitNet as Llama and left four tensors unread (§4.2 of `corpus-v2.md`). Same pattern: sub-LN of
MAGNETO/RetNet-style models. **Acceptance**: `bitnet`.

### FR-04 `ATTN_DIFFERENTIAL_V1` (M, unlocks `diffllama`)

What transformers 5.17 computes is **not** the paper's two-softmax-per-head map. `modular_diffllama.py:DiffLlamaAttention.forward`
runs the ordinary attention twice with the *same* q/k and two value halves: `v1 = repeat(v[:KV/2], 2)`,
`v2 = repeat(v[KV/2:], 2)` along the KV-head axis, `o1 = A·v1`, `o2 = A·v2`; per head `[o1; o2]` (width `2d`); the result is
then split **along heads**: the first `H/2` heads minus `λ` times the last `H/2` heads; `(1 − λ_init) · RMSNorm_{2d}(·)` (no
gain); `o_proj` over `(H/2)·2d`. Per head there is exactly one softmax map. Unfolding `repeat_kv` (`G = H/Hkv`, pair
`(i, i+H/2)`, `g = i//G ∈ [0, Hkv/2)`): head `i` reads key head `g`, head `i+H/2` reads key head `g+Hkv/2`, both read the same
double-wide value `[V_g | V_{g+Hkv/2}]`, so `y_i = RMS_2d((softmax(q_i K_g) − λ·softmax(q_{i+H/2} K_{g+Hkv/2}))·[V_g|V_{g+Hkv/2}])·(1−λ_init)`
(derived by hand from the code, not run). `λ = exp(Σ λq1⊙λk1) − exp(Σ λq2⊙λk2) + λ_init(layer)`, `λ_init = 0.8 − 0.6·e^{−0.3·layer}`
(0-based) is a function of weights only, so it is a Q24 constant per layer at conversion. HF computes the sums in fp32 but casts
each `exp` to the model dtype (bf16 rounds λ by about 0.4 %). It needs an even `num_key_value_heads` and rejects
`attention_dropout > 0`.
**Spec**: `AttnSpec.differential: Option<DiffSpec{lambda_init, norm_eps}>` plus a value head map (`v_head_dim = 2d`,
`kv_heads_v = Hkv/2`); tensors `lambda_q1/k1/q2/k2 [d]`. **Decomposition**: the library `attention` `P·V` against the double-wide
value (attention MACs 1.5× GQA's); keep both contractions as exact `i64` sums and subtract `λ·O_{i+H/2}` **before** the single
narrowing (narrowing each half first would amplify error, since the point of the subtraction is cancellation); then the
`RMSw`-over-`2d` template times the constant `(1−λ_init)`. No new commit-point kind; no primitive. No other family in 5.17
uses differential attention. **Acceptance**: `diffllama`.

### FR-05 `ACT_LEARNED_POINTWISE_V1` — xIELU (S, unlocks `apertus`)

`y = x>0 ? αp·x² + β·x : (expm1(min(x, ε)) − x)·αn + β·x` with `αp = softplus(p)`, `αn = β + softplus(n)`, per-layer scalars
`p, n` (tensors `act_fn.alpha_p`, `alpha_n`, shape `[1]`) and buffers `β = 0.5`, `ε = −1e−6`
(`activations.py:XIELUActivation._xielu_python`). Pointwise on the activation code, so the library's `act_table` applies: a
65,536-entry table built at conversion from the layer's two scalars, stored as a per-layer param (it exceeds the 64 KiB const
limit), `Gather` at run time. Or `Select(x>0, αp·x² + β·x, …)` with `IntExp` on the non-positive branch. **Spec**:
`Act::Xielu` carrying its parameter tensor names. No primitive. **Acceptance**: `apertus` (adapter to write when the
activation exists; the rest of Apertus is Llama + QK-norm + llama3 rope).

### FR-27 five small lowerer details found while reading the decoder families (S each)

Each is a few lines in the lowerer or an adapter default; they are listed once because three or four entries hit them.

* **(a) The gated-norm gate is hard-coded SiLU.** `hl/build.rs:gdn()`, `lower_gated_norm` (`TableFn::Act(Act::Silu)`) and the
  `float_ref` `GatedRmsNorm` arm all use SiLU; `GdnSpec.gate_act` is read by the `qwen4_exp` adapter and never consumed (an
  unread flag, FR-26). Kimi's `KimiLinearRMSNormGated` gates with **sigmoid**. Add `Op::GatedRmsNorm.act` (additive, default SiLU).
* **(b) A router renormalisation epsilon above `1e-9` is refused** ("not in Gate 2a"). `lfm2_moe` divides by `Σ + 1e-6`
  (`modeling_lfm2_moe.py:Lfm2MoeSparseMoeBlock.route_tokens_to_experts`); DeepSeek, Kimi and Nemotron use `1e-20`, which is accepted.
  Accept any positive epsilon: it is a constant in the `Div` divisor.
* **(c) MLA's `q_a`/`kv_a` norm eps is mis-generalised.** The built-in deepseek-v3 adapter sets both to `rms_norm_eps`; HF
  hard-codes `1e-6` (`DeepseekV3RMSNorm(dim)` with the default). Equal for V3 (`rms_norm_eps = 1e-6`), **wrong** for Kimi-Linear
  (`rms_norm_eps = 1e-5`) and LongCat. Make the MLA norm eps its own adapter variable defaulting to `1e-6`.
* **(d) Mamba-2's dt clamp differs between prefill and decode.** Zamba2 and Nemotron-H hard-code `time_step_limit =
  (time_step_min, inf)`; the clamp exists only in `mamba2_chunk_scan` (prefill) and `mamba2_selective_state_update` (decode)
  never clamps. TIR's reference is decode, so adapters set `Mamba2Spec.dt_min = 0` and record a note; a refusal would be wrong
  (for Nemotron-H the config key `time_step_limit` is dead).
* **(e) Three mechanisms recur and should be built once**: *side carries* (KV sharing already is one; Zamba2's `e0` and `t`,
  LongCat's shortcut `s`), a *multi-stream carry of `K·D` lanes* (Gemma-3n AltUp and every hyper-connection model:
  `deepseek_v4`, `glm5_next`, `hy_v4`, `qwen4_exp`) and *weight sharing across layers* (Zamba2's shared block, ALBERT's one
  layer; FR-17 `share_layers`).

## Decoder-side features that need a new mixer, FFN or residual kind

(Each is a generic feature with the entries it unlocks; decompositions are into the 25 primitives. Details are from a
read-only pass over the transformers 5.17 code and the tiny fixtures; nothing below was run unless it says *evidence*.)

### FR-06 `MIXER_MOA_V1` — mixture-of-attention (M, unlocks `jetmoe`)

`modeling_jetmoe.py:JetMoeAttention.forward`, `JetMoeMoA.map/reduce`: the layer norms use `JetMoeRMSNorm(hidden)` with the
**default eps 1e-6** (only the final norm uses `rms_norm_eps`); `num_attention_heads = num_key_value_heads × num_experts_per_tok`,
`head_dim = kv_channels`. One router (`TopKThenSoftmax`: top-k of the logits, softmax over the `k`); slot `j` (the j-th selected
expert) computes `q_j = W_in[e_j]·x` (`[kvh·hd]`, **ungated**); the `k` slots form heads `(j,h) → j·kvh + h`; a single shared
`kv_proj(x) → [K|V]`; rope (default, rotate_half) on q and k; K and V are **tiled** (`repeat(1, top_k, 1, 1)`: head `(j,h)`
reads kv head `h`, not `repeat_interleave`); output `Σ_j g_j·W_out[e_j]·ctx_j + bias [D]`; scale `hd^−½`. Slot order does not matter
(the sum is permutation-invariant). The MoE FFN is Level B shape already: `input_linear [E,2I,D]` (gate first =
`FusedGateFirst`), `output_linear [E,D,I]`, same router, `+ bias [D]` after the sum. **Spec**: `AttnSpec.moa:
Option<MoaSpec{experts, top_k, router, out_bias}>`; two generic HL ops factored from `MoeExperts` — `ExpertLinear{top_k}`
(`[x | ids | W [E,R,C]] → [k,R]`, the input shared across slots for q, per slot for o) and `WeightedSum{top_k}`; `AttnSpec.kv_map:
HeadMap` (Tile exists for GDN); `MoeSpec.out_bias`. Tensors: `self_attention.experts.{input_linear [E,Hkv,D], output_linear
[E,D,Hkv], router.layer [E,D], bias [D]}`, `self_attention.kv_proj [2·Hkv, D]`. **Decomposition**: `TopK` (committed) →
`Gather(W_in, idx_j)` → `MatMul` per slot → rope → `HistAppend` of the shared k, v → attention over the history (q reshaped
`[k,kvh,hd]` → `[kvh,k,hd]`) → `Gather(W_out, idx_j)` → `MatMul` → `moe_combine_q36` + bias. No primitive. **Acceptance**:
`jetmoe`.

### FR-09 `ATTN_TOKEN_INDEXER_V1` — DeepSeek sparse attention (L, unlocks `deepseek_v32`; the same scorer is in `glm_moe_dsa`, `hy_v4`, `axk2`, `glm5_next`, `qwen4_exp`)

**Semantics** (`modeling_deepseek_v32.py:DeepseekV32Indexer.forward`, `DeepseekV32Attention.forward`): ordinary V3 MLA softmax
restricted to the keys `S_p = topk(s, min(index_topk, T))`, with `qr = q_a_layernorm(q_a_proj x)` (the MLA q-latent, shared),
`q = wq_b(qr)` reshaped `[Hi, Di]` with the first `qk_rope_head_dim` lanes rotated by rotate-half (rotated lanes **first**,
the opposite of MLA's nope-first layout), `k = LayerNorm(wk x)` (weight, bias, eps 1e-6 hard-coded; one head; rotated the same
way; cached per position), `s_t = Σ_h w_h·ReLU(Di^−½ · q_h·k_t)` with `w = weights_proj(x)·Hi^−½`. HF quirks to pin:
`__post_init__` overwrites the config key `head_dim` with `qk_rope_head_dim` (it is not the attention head dim); HF drops
the reference's Hadamard rotation and FP8 scoring (dot-product-preserving, precision only); ReLU makes exact-zero scores common,
so ties at the k-th place are not rare and `torch.topk`'s tie order is unspecified — the IR `TopK` rule (lowest index) is the
pinned choice; for `p+1 ≤ index_topk` every key is selected and the layer *is* dense MLA.
**Not `ATTN_SPARSE_BLOCK_V1` at block size 1.** That feature (registered, spec only) scores *block means* of raw keys with
the layer input as query, RMS q/k norms and an unweighted head sum; DSA scores tokens with a latent query, a LayerNorm'd key
and learned per-head weights.
**Spec** (additive): `MlaSpec.indexer` / `AttnSpec.indexer: Option<TokenIndexerSpec{heads, head_dim, topk, q_source:
Latent|Hidden, q_norm: Option<NormSpec>, k_norm: NormSpec, rope: RopeSpec, head_weights: bool, shared_from: Option<usize>}>`;
config keys `index_n_heads`, `index_head_dim`, `index_topk` (`layer_types` is inert); tensors
`layers.L.self_attn.indexer.{wq_b [Hi·Di, q_lora], wk [Di, hidden], k_norm.{weight,bias} [Di], weights_proj [Hi, hidden]}`.
**Decomposition (no primitive).** `TopK` needs a `Fixed` axis (spec 04b §6.6) and `TopK`/`Gather` along `H` would break "dissectability
is structural" (§10.3), so the selection is an exact **top-k mask by counting**: a `Hist` `ik` (i16 `[W, Di]`, the MLA window);
`qi`, `ki` (rope'd; the `HistAppend` input) and `w` committed; `s_H = MatMul(w, Clamp(narrow(MatMul(qi, ikᵀ)), 0, ·))` (the
positive scales `Di^−½`, `Hi^−½` are rank-neutral and fold into the narrowing); the composite key
`κ_t = (s_t + off)·2^b + (2^b − 1 − t)` (`t` from `Iota_H`, `b = ⌈log2 W⌉`) is distinct, so the k-th largest `τ'` is unique and
is found by a 16-ary radix search of `⌈(31+b)/4⌉ = 12` passes of `ReduceSum_H(Compare(κ, cand[15,1], Ge))`; `τ'` is committed
(two i32 lanes; `τ' = 0` when `H < k`); `vis_H = Compare(κ, τ', Ge)` selects exactly `k` positions with ties to the lowest index
(the IR rule); `Select(vis_H, logits, MIN)` precedes the library `softmax_shifted` in `mla_absorbed`.
**Cost** (V3.2: `Hi = 64`, `Di = 128`, `k = 2048`, `D = 7168`): 14.0 M MACs of projections per layer plus `8192·H` scoring
MACs and about `360·H` elementwise for the passes; at `W = 2^16` the indexer adds 33.6 G MACs over 61 layers, +5.7 % on
the 5.93e11 of `freeze-v1.md` §7. The selection is masked-dense: **no compute saving** over dense MLA, only the exact function.
A court tile for the `τ'` cone has 12 reductions over `H` (O-1 allows 16), its bottom at `h_tile = 64` costs 524 K MACs and opens
about 65 KB, and the root claim carries `V = 12·15 = 180` values (O-5 allows 4,096).
**Vacuous case**: when `max_window ≤ index_topk` lower plain MLA with no indexer nodes — exact, and Level B at once (the
fixture's `index_topk` is below the window, so it needs the real thing). Alternative for small `W`: a `Fixed(W)` key store with
`TopK` over `Fixed(W)`; replay needs `C·W·Di ≤ 2^26`, so `C ≤ 64` at `W = 8192`.
**Acceptance**: `deepseek_v32`. *Evidence: analysis (hand-checked against the O-1..O-5 obligations, NF-8 and NF-12; no program built).*
Worth prototyping first as a library template, since four other families reuse it.

### FR-10 the DeepSeek-V4 set (L, unlocks `deepseek_v4`; parts reused by `glm5_next`, `hy_v4`, `minimax_m3_vl`)

Nine additive features, none in the registry today (`RESIDUAL_GATED_HC_V1` is Qwen4's per-channel gates with no mixing matrix,
`ATTN_SPARSE_BLOCK_V1` pools by mean, `MLP_GLU_CLAMPED_V1` is gpt-oss's `(up+1)·g·σ(αg)`). All semantics are in
`modular_deepseek_v4.py` (hub names map through `conversion_mapping.py`; FP8/FP4 hub checkpoints are refused by the storage rules).

| # | proposed id | semantics | decomposition and cost |
| --- | --- | --- | --- |
| 1 | `RESIDUAL_MHC_SINKHORN_V1` (+ the final `HyperHead` collapse) | `n = hc_mult` streams; `flat = RMS_unweighted(h.flatten())`, `[pre, post, comb] = fn·flat`; `pre = σ(·scale0+base)+eps`, `post = 2σ(…)`, `comb` = row-softmax then Sinkhorn column/row normalisation `iters` times; `x = Σ pre_i h_i`, `y = block(RMS x)`, `h'_k = post_k·y + Σ_j comb[j,k] h_j` | ≈ 200 nodes per site (19×2 normalisations, `Div` by data), two sites per layer → 3–4 blocks per layer; carries `collapsed [D]`, `post [n]`, `comb [n·n]`; the largest NF-12 consumer |
| 2 | `ATTN_Q_LOWRANK_V1` + split q/k norms | `qr = RMS_w(q_a x)`, `q = RMS_unweighted_per_head(q_b qr)` then rope; `QkNorm` holds one `NormSpec` for both and V4's q is weightless while k has a gain | existing norm templates |
| 3 | `ATTN_KV_SHARED_ROTATED_V1` | K = V = the post-norm, post-rope `kv`; the output's rope slice is counter-rotated with `−sin` at the query position (`ATTN_K_EQ_V_V1` is Gemma-4's pre-norm variant) | one history; MQA with 1 kv head, 64 heads × 512 |
| 4 | `ATTN_OUT_GROUPED_LOWRANK_V1` | `o_a` block-diagonal over `o_groups`, then `o_b` | batched `MatMul` |
| 5 | `ATTN_COMPRESSED_KV_V1` | HCA (`m'=128`) / CSA (`m=4`, 2m-overlapped) compressors: per-channel softmax over the window of `kv_proj`+`gate_proj`+`ape`, `RMS_w`, rope at the window's first position; softmax keys = window (128) ∪ complete entries ∪ per-head sink; a query at `p` sees `(p+1)//m` entries | `Fixed` window buffers and a `Fixed [n_max = W/m, c]` entry store written by a one-hot `Select` + `StateWrite` at window close; the pooling softmax runs every position (no conditional execution: 65 K `IntExp` per HCA layer); replay bound `C ≤ 2^26/(n_max·c)` (HCA at `W = 2^16`: 256; CSA at `W = 8192`: 64) |
| 6 | `ATTN_ENTRY_INDEXER_V1` | the FR-09 scorer with the compressed entries as keys, `index_topk = 512` | `TopK` over `Fixed(n_max)` (legal), `Gather` the rows, mask by `idx < (p+1)//m`; window part and entry part joined by max/sum/ctx combination (a new library template); 16.8 M MACs scoring + 33.6 M entry attention at `n = 2048` |
| 7 | `MLP_MOE_ROUTER_HASH_V1` | `hash_moe` layers pick experts by a frozen `tid2eid [vocab, k]` table | `Gather(param, Input(0))` |
| 8 | `MLP_MOE_ROUTER_SQRTSOFTPLUS_V1` | `topk(sqrtsoftplus(logits) + bias)`, weights = unbiased scores renormalised (always) × 1.5 | softplus is in the library; `sqrt(s) = s·IntRsqrt(s)` |
| 9 | `MLP_GLU_LIMITED_V1` | `silu(min(gate, L))·clamp(up, −L, L)` (`swiglu_limit`; also `glm5_next`, `hy_v4`, `minimax_m3_vl`; `step3p7` clamps after the activation and is a different function) | `Clamp` before the table |

HF inconsistencies to pin: `norm_topk_prob` and `n_shared_experts` are never read; the router runs in model dtype (not
fp32); the pooling softmax is fp32 but cast to bf16 before the multiply; prefill and decode agree (the HCA bias is `None` at
decode and equals the prefill threshold). The rope covers the trailing 64 of 512 dims (`RopeSpec.offset`, already read on the
attention path), interleaved; layers with a compressor use the "compress" rope (θ = 1.6e5, yarn with `attention_factor` forced
to 1.0), sliding-only layers the "main" one (θ = 1e4). Per layer, a CSA layer is about 0.37 G MACs at `W = 8192` (0.18 G is the
MoE). **No primitive** (`Div` by data, Fixed-axis `TopK`/`Gather` exist). An optional general state attribute `Hist{stride m}`
would make compressed stores cheap at long windows but conflicts with one-window-per-block; not recommended now.
**Acceptance**: `deepseek_v4`. *Evidence: analysis; the fixture is `tools/corpus/specs/deepseek_v4`.*

### FR-15 `MIXER_SHORT_CONV_V1` — gated short convolution (M, unlocks `lfm2`; also `lfm2_moe`, the `lfm2_vl` text tower)

`modeling_lfm2.py:Lfm2ShortConv.forward`: `[B|C|x] = in_proj(x)` (three D-wide chunks), `u = B·x`, `v = causal_depthwise_conv(u)`
with kernel `conv_L_cache` and **no activation**, output `out_proj(C·v)`; one flag `conv_bias` gives biases to the conv,
`in_proj` and `out_proj` together. Decode reads `state[1:] + new` (effective memory `K−1` rows, as in the library; prefill's
left zero-padding equals a zero initial state). The rest of the layer is existing vocabulary: pre-norm sequential
residual (`operator_norm`, `ffn_norm`), attention with per-head q/k RMS norm before rope, gated SiLU MLP, tied head;
`embedding_norm` is the FINAL norm. **Spec**: `Mixer::ShortConv(ShortConvSpec{kernel, bias})`; HL: three Linears
`shortconv.in.{b,c,x}` sliced by `Pick::Range` (like Mamba-2's `in_proj`) and the existing `CausalConv1d{act: None}`, which
hl, `float_ref` and `lower_conv` already handle. Tensors (hub names): `conv.in_proj [3D, D]`, `conv.conv [D,1,K]`,
`conv.out_proj [D, D]`. **Decomposition**: `Lin` ×3 (boundaries committed) → `N(Mul(b,x))` (commit: the conv row) →
`Concat(State[K−1,D], row)`, `StateWrite(Slice(win,1..K))` → `ReduceSum(Mul(win, taps))` + narrowing (commit) → `N(Mul(c,y))` →
`Lin(out_proj)`. No primitive. Also needed by `lfm2_moe`: the router divides by `Σ + 1e-6` (see FR-27(b)).
**Acceptance**: `lfm2`.

### FR-16 `MIXER_KDA_V1` and `MIXER_MLA_NOPE_V1` — Kimi delta attention (L, unlocks `kimi_linear`; partly `glm5_next`)

`modeling_kimi_linear.py:KimiLinearDeltaAttention.forward` (decode: `recurrent_kimi_delta_attention`), `H = linear_num_heads`,
`d = linear_head_dim`: `q, k, v` projections each `[H·d]`; a depthwise causal conv (kernel 4, no bias, SiLU) on each; per-head
L2 norm of `q, k` (eps 1e-6) and `q *= d^−½`; a **channel-wise** forget gate `g[h,c] = −exp(A_log[h])·softplus(f_b(f_a(x))[h,c] +
dt_bias[h,c])`; `β = σ(b_proj x)`; per head `S ← S ⊙ exp(g)[:, None]` (decay per KEY channel), `u = β(v − Sᵀk)`, `S += k uᵀ`,
`o = Sᵀq`; output `o_norm(o, gate)` = per-head RMS norm × **sigmoid** gate (`KimiLinearRMSNormGated`), then `o_proj`. The MLA
layers have **no rope at all** (`__post_init__` deletes `rope_parameters`; `k_rot` is the raw shared projection), scale
`qk_head_dim^−½`, `q_a`/`kv_a` norm eps 1e-6 (not `rms_norm_eps`), `q_lora_rank` null in the release. The MoE is the
DeepSeek-V3 router (sigmoid + selection bias, top-2-sum groups, `/(Σ+1e-20)`, `× routed_scaling_factor`) — existing.
**Spec**: `Mixer::Kda(KdaSpec{heads, head_dim, conv_kernel, forget_rank, gate_rank, conv_act, l2_eps, norm_eps, gate_act})`; HL:
`Op::GatedDelta.channel_decay: bool` (input `g` is `[H·dk]`, `k_heads = v_heads`), `Op::GatedRmsNorm.act` (additive, default
SiLU), binder `Pick::Repeat{each: d, groups: H}` so `−exp(A_log)` becomes a `[H·d]` param; three separate `CausalConv1d` ops
(depthwise channels are independent: equal to the concatenated conv and binds 1:1 to the hub tensors). `MlaSpec.rope:
Option<RopeSpec>` with `None` = no `Op::Rope`. **Decomposition**: `gdn_step_q36` with the decay broadcast `[H,1,dk]` instead of
`[H,1,1]` (the only change); 2 `IntExp` + 2 `IntLn` per (h,c) = 16,384 per layer at H=32, d=128 (below the Mamba-1
precedent); commit the conv rows, `decay`, `beta`, `gate`, the carry-out; `S` at checkpoints only (`32·128·128`, as Qwen3.6).
No primitive. Also: the checkpoint concatenates the three convs at load in 5.17 (`conversion_mapping.py`); adapters bind hub
names. **Acceptance**: `kimi_linear`.

### FR-14 `LAYER_MIXER_NONE_V1` and plain experts (M, unlocks `nemotron_h`)

`modeling_nemotron_h.py:NemotronHBlock.forward`: `h ← h + mixer(norm h)` with ONE norm and one mixer per layer, the mixer chosen
by `layers_block_type ∈ {linear_attention (Mamba-2), full_attention, mlp, moe}`. Three additive pieces: (1) **`Mixer::None`**
(a layer that is only an FFN block, `h += ffn(norm h)`; both norm roles map to the same tensor
`backbone.layers.{L}.norm`) — the registry's `LAYER_FFN_ONLY_V1`; (2) **`MoeSpec.gated: bool`** (default true) and
`SharedExpertSpec.gated` — Nemotron's experts and shared expert are ungated (`up [E,I,Din]`, `down [E,Din,I]`, ReLU²;
`hf_weights.rs` hard-codes `gated: true` for the shared expert); (3) **`MoeSpec.latent: Option<usize>`** (`fc1_latent_proj
[L,D]`, `fc2_latent_proj [D,L]`). The Mamba-2 mixer is the existing one with `proj_bias = use_bias`, `norm_eps =
layer_norm_epsilon`, `norm_groups = n_groups`, `dt_min = 0` (decode semantics, FR-27(d)); attention has no rotary, no q/k norm,
`bias=False` hard-coded. **Decomposition** (plain MoE): router (sigmoid, selection bias, `/(Σ+1e-20)`, scale) → `Gather(W_up, idx)`
→ `MatMul` → ReLU² table → `Gather(W_down, idx)` → `MatMul` → `moe_combine_q36`; no gate `MatMul`, no product. No primitive.
**Acceptance**: `nemotron_h`.

### FR-11 `MIXER_PARALLEL_BRANCH_V1` and muP scalars (M, unlocks `falcon_h1`)

`modeling_falcon_h1.py:FalconH1DecoderLayer.forward`: `x = input_layernorm(h)`; `m = mamba(x·ssm_in)·ssm_out`;
`a = attn(x·attention_in)·attention_out`; `h += m + a` (parallel, ONE norm for both); then `h += ffn(pre_ff_layernorm(h))` with
MLP multipliers; `embed·embedding_multiplier`, logits `× lm_head_multiplier`; `key = k_proj(x)·key_multiplier` after the bias
before rope (folds into `AttnSpec.scale`). Mamba details: `in_proj` bias is `mamba_proj_bias` but `out_proj` bias is
`projectors_bias` (two flags); `projected × mup_vector` scales the segments `[z|x|B|C|dt]` by `ssm_multipliers`; `mamba_rms_norm`
defaults **False** (the output is `y·silu(z)` with no norm; if True, a grouped gated RMS norm honouring `mamba_norm_before_gate`).
**Spec**: `Mixer::Parallel(Vec<MixerBranch{mixer, in_scale, out_scale}>)` (the pattern `Residual::Parallel` already uses),
scalar fields `Mamba2Spec.in_scales: [f64;5]`, `MlpSpec.{gate_scale, down_scale}` lowered with `Op::Scale` (free in TIR: it folds
into the next narrowing), `Mamba2Spec.norm: {GateFirst, NormFirst, None}` and `out_bias`. For `[x|B|C]` scale before the
conv with three Linears (`gdn()` does this): scaling after the conv is not equal because of the conv bias. No primitive.
**Acceptance**: `falcon_h1`.

### FR-08 `MLP_MOE_ZERO_EXPERT_V1` and the shortcut MoE (M–L, unlocks `longcat_flash`)

`modeling_longcat_flash.py`: `num_layers` counts logical layers (the model doubles it: read `num_layers`). A logical layer is two
MLA+dense-MLP sub-layers with the MoE's output held and added at the end (`h4 = h3 + mlps[1](n3) + s`). The router is a softmax
over `n_routed + zero_expert_num` experts, choice = `p + e_score_correction_bias`, top-k = `moe_topk`, weights = the unbiased
`p × routed_scaling_factor` with **no renormalisation**; a "zero-computation" expert is the **identity** (it contributes
`w_j·x`). Four additive pieces: `MoeSpec.zero_experts` (router, bias and `Route.experts` run to `E+Z`; ids `≥ E` select the
input; expert tensors keep `E` rows); `LayerSpec.side` — a side carry written by sub-layer 0 and added to sub-layer 1's output
(the same mechanism Zamba2 needs for `e0`); `MlaSpec.q_post_scale`/`kv_post_scale` (the `√(D/q_lora_rank)` and `√(D/kv_lora_rank)`
factors, foldable); a binder `layer_fold` (`{F} = i / fold`, `{G} = i % fold`) for `layers.{F}.{…}.{G}`; and FR-07 (softmax
scoring with a selection bias). **Decomposition**: `Softmax` → `Add(p, bias)` → `TopK` (commit) → `Gather(p, idx)·6.0`;
`Select(Compare(idx, E, Ge), x, expert_out)` replaces a zero-selected slot (it runs a dummy routed expert whose result is
discarded). No primitive. **Acceptance**: `longcat_flash` (needs FR-07 and the above).

### FR-13 `ATTN_SHARED_BLOCK_V1` is necessary but not sufficient (L, unlocks `zamba2`; also `zamba`, `hrm_text`)

`modeling_zamba2.py:Zamba2HybridLayer.forward`: a *hybrid* layer feeds `u = RMS_2D(concat[h, e0])` (`e0` = the raw embedding) through
a **shared** transformer (attention `q,k,v: 2D → H·hd` with `hd = 2D/H` and scale `(hd/2)^−½`, then a gated GELU MLP) with **no
internal residuals**, a per-layer `D×D` linear `t`, and adds only `t` to the *Mamba input* of that layer: `h ← h +
mamba(input_layernorm(h + t))`. Each hybrid depth keeps its own K/V history (only the weights are shared); with
`use_shared_attention_adapter` each of q/k/v gets a per-depth low-rank `B(A·u)`, and the MLP's gate_up unconditionally does.
Four orthogonal additive pieces: **`ATTN_SHARED_BLOCK_V1`** (a `share` group makes the branch params global, activation scales stay
per occurrence), **`EMBED_CARRY_V1`** (`ModelSpec.embed_carry`: a pass-through carry `e0`; `AttnSpec.in_dim`), **`LAYER_PRE_BRANCH_V1`**
(`LayerSpec.pre_branch`: an attention+MLP branch whose output is added to the mixer input), **`LINEAR_LOWRANK_ADAPTER_V1`** (the
existing `LoraOp` with a `{O}` hybrid-ordinal placeholder). Mamba-2 is the existing mixer with `conv_bias = true` (hard-coded),
`norm_eps = 1e-5` (hard-coded), `dt_min = 0` (FR-27(d)). No primitive. **Acceptance**: `zamba2`.

### FR-12 `RESIDUAL_ALTUP_V1`, `RESIDUAL_LAUREL_V1`, `FFN_ACTIVATION_SPARSITY_V1` (L, unlocks `gemma3n_text`)

`modeling_gemma3n.py` (K = `altup_num_inputs` = 4, active stream 0): per layer, predict `pred_i = h_i + Σ_j C[i,j]·h_j` with
`C = prediction_coefs(m)`, `m = tanh(modality_router(router_norm(h_0)·D⁻¹))`; run the sandwich block on `pred_0` with LAuReL
(`lo = an + post_laurel_norm(linear_right(linear_left(an)))`, joined as `(a + at + lo)/√2`); correct `cor_i = pred_i + (afl − pred_0)·
(correction_coefs(m') + 1)_i`; the per-layer input (PLE) is added to streams 1..K−1. Streams are created and merged by a
magnitude match `m0·U_i(h_i)/√max(ms, 1e-5)`. The MLP of the sparse layers computes `gate ← relu(gate − (mean + z·std))` with
`z = Φ⁻¹(sparsity)` over the width (biased std) before the activation. The norm gain is `w`, not `1+w`. **Spec**:
`ModelSpec.altup`, `LayerSpec.laurel`, `MlpSpec.sparsity_sigma`; HL ops `StreamMix{K}` (one batched `MatMul [K,K]×[K,D]`),
`RmsMatch{floor}`, `GaussianTopK`; the `K·D`-lane carry is the plumbing hyper-connections need too (`deepseek_v4`, `glm5_next`,
`hy_v4`, `qwen4_exp`). Reuses `Residual::Sandwich`, `PleSpec`, `ATTN_KV_SHARE_V1`, `v_norm`, scale 1.0, `HEAD_SOFTCAP_V1`.
**Decomposition**: coefficient rows by `Lin` + a tanh table (committed), the mix as `MatMul` over `i64`, the Gaussian top-k as
exact centring `n·g − Σg`, `Σ c²` in `i128` (= `n³·Var`), `IntRsqrt`, `Table_gelu_tanh`; `Φ⁻¹` is a registration-time
constant. No primitive. **Acceptance**: `gemma3n_text`.

## Routes (a whole kind of model with no data-driven lowering)

Everything above is a feature added to one lowering that already exists. These are the model kinds whose lowering is
**Rust per family**, so a third party cannot add a member with data: `lower/bidir.rs` is BERT-shaped (890 lines),
`lower/encdec.rs` has five hard-coded encoder-decoder families (1,674 lines), `lower/vision.rs` has CLIP/SigLIP/Qwen towers
(1,438 lines), and there is no lowering at all for convolutions, diffusion denoisers or audio. The layer block is hand-written
three times, so every new feature must be added three times. The measured consequence is the biggest block of Level C in
[`corpus-v2.md`](corpus-v2.md): 21 of the 47 entries that are not Level A/B.

### FR-17 `ENC_ROWS_V1` — one rows-mode encoder lowerer driven by the `ModelSpec` (L, unlocks `modernbert`, `nomic_bert`, `albert`, `deberta_v2`; the lineage of ~20 encoder families)

**What `lower/bidir.rs` hard-wires** (line numbers in the worktree): all layers equal (`:64`); MHA only, no GQA, window, qk-norm or
sinks (`:68`); all four attention biases required (`:71`); ungated MLP with both biases (`:75`); post-LN with biased LayerNorm only
(`:78, :82-87`); embedding LayerNorm and learned positions both required (`:80-81`); the embedding block gives every width as
`hl.hidden` except the table (`:233-300`); pooling is CLS or masked mean (`:418-460`). **`arch_of` never reads** `at.position`,
`spec.final_norm`, `embedding.proj_in`, `embedding.scale`, `softcap`, `clip_qkv` — so a naively written RoPE encoder adapter
(ModernBERT, nomic, jina-v3, EuroBERT) lowers to a *wrong program with no error*. This is FR-26's class of defect and the first thing to fix:
make `arch_of` a strict allow-list (any unread field not equal to its default is refused, naming the field).

**The ALBERT panic has two causes and two silent errors behind it** (the adapter `tools/corpus/adapters/albert.json` is
otherwise complete). It panics in `b.add(word, pos, I64)` at `bidir.rs:291` ("Add operands do not broadcast",
`misaka-palw-tir/src/builder.rs:228-231`). *Cause 1* (HL builder): `hl/build.rs:270-289` builds the embedding in OPT's order and at
hidden width — table `[V,E]`, bias-free `embed.proj_in` (E→d), `embed.pos_table [rows,d]`, `embed.norm` over d, `embed.type_table
[rows,d]` — while ALBERT/ELECTRA need positions, type rows and the norm at width E and then a *biased* projection after the norm
(DeBERTa with E≠d wants the bias-free projection after the sum and before the norm). *Cause 2* (`bidir_block`): it takes `d =
hl.hidden` as the width of everything but the word table, so `[L,E] + [L,d]`. *Silent*: `arch_of` never inspects `embedding.proj_in`;
`float_forward` indexes the E-wide word table with stride `d` (`:813`); `hf_weights::bind` (`hf_weights.rs:120-122`) binds only
`embed.proj_in.w`, so `embedding_hidden_mapping_in.bias` has no home; `EmbeddingSpec.proj_in` is a bare `bool` (`spec.rs:722`).

**Request.** A single *rows-mode* lowerer over the HL graph (the graph the decoder already uses), vectorised over the padded
token axis, replacing the three hand-written layer blocks: `Linear` → `linear_rows`, `Norm` → `norm_rows_kind` (Layer/Rms,
±bias), `Act`/`Add`/`Mul` → existing table and row helpers, `Rope` → vision's pinned Q24 `[1, L, dh]` tables generalised to 1-D,
`Attention` without `HistAppend` (q/k/v rows, `softmax_rows`/`split_softmax`). New fields, all additive (`serde(default)`):

| field | meaning | entries |
| --- | --- | --- |
| `AttnSpec.bidirectional: bool` | no causal mask; `window` then means *half-width* (ModernBERT local `local_attention//2`, inclusive, `masking_utils.py`; LongT5 `local_radius`) | modernbert, nomic_bert, jina-v3, EuroBERT |
| `Position::Bucketed{kind: T5\|DebertaLog\|Table2d, buckets, max_distance, scope: Shared\|PerLayer}` | relative-position bias tables (UMT5 is per layer); MPNet's `EmbeddingSpec.rel_bias` migrates here | t5 family, mpnet, deberta |
| `Position::Disentangled{c2p, p2c, share_key, span, buckets, max_distance, table_norm}` | DeBERTa's content-to-position and position-to-content terms | deberta_v2 |
| `EmbeddingSpec.proj_in_bias`, `proj_at: AfterLookup\|AfterSum\|AfterNorm` | positions, type rows and norm live at width E *before* `proj_at`, at d after (default `AfterLookup` = OPT) | albert, electra, deberta (E≠d) |
| `EmbeddingSpec.positions.kind: Learned\|Sinusoid{computed}` | Marian computes the table, Pegasus loads it | FR-18 |
| `ModelSpec.share_layers: bool` | layer weight codes become global params; the `.m/.s/.z` narrowing params stay per layer (calibration is per occurrence) | albert (stops storing 12 copies; today the adapter just omits `{L}`) |
| `OutputSpec::Rows` | the encoder alone (T5 as a text encoder for diffusion) | t5_encoder |

Registry ids with the extra primitives they use (`BASE_PRIMITIVES` plus): `ENC_ROWS_V1` (Iota, Compare, Select, Transpose,
ReduceMax; umbrella), `ATTN_BIDIR_V1`, `ATTN_BAND_V1` (Iota, Sub, Compare), `ROPE_ROWS_V1` (Slice, Concat), `EMBED_PROJ_ORDER_V1`,
`LAYER_WEIGHTS_SHARED_V1`, `POS_DISENTANGLED_V1` (Transpose, Gather), `POS_RELATIVE_BIAS_V1` (extended kinds).

**Per-family mapping once those exist (adapter variables only):**

| family | embedding | attention | FFN | residual | extras |
| --- | --- | --- | --- | --- | --- |
| ALBERT | dim = E, positions, type rows, LN(E), `proj_at = AfterNorm`, bias | MHA, biases | plain gelu_new | post-LN | `share_layers` |
| ModernBERT | LN, no positions | bidirectional, rope per layer type (θ from `rope_parameters[layer_type]`), `window = 64` on sliding layers, no bias, `FusedConcat` | gated GeLU, `Wi` rows `[input\|gate]` = `FusedGateFirst` | sequential pre-norm, layer 0 has no `attn_norm`, LN no bias | `final_norm` |
| NomicBERT | type rows, LN | bidirectional, rope half-split, no bias, `FusedConcat` | gated SiLU, `fc11` = up, `fc12` = gate, `fc2` = down | post-LN | — |
| DeBERTa-v2 | [positions][type][proj `AfterSum` if E≠d] LN | bidirectional, `Disentangled` | plain gelu | post-LN | `rel_embeddings`, `encoder.LayerNorm` |
| jina-v3 | type rows, LN | bidirectional, rope (θ 20000), biases | plain gelu | post-LN | task LoRAs are PEFT adapters, not the base |
| EuroBERT | none | bidirectional GQA, rope | gated SiLU | sequential RMS | `final_norm` — the strongest case for `bidirectional: true` on the *decoder* spec |

**DeBERTa decomposition (no primitive).** The position table is `LN(rel_embeddings)[0:2·span]`; with `share_att_key` each layer
projects it with its own `key_proj`/`query_proj`. `δ[i,j] = clamp(span + bucket(i−j), 0, 2·span−1)` with `make_log_bucket_position` in f32;
c2p reads `Q_i·Pk[δ_ij]`, p2c reads `K_j·Pq[δ_ij]` (the p2c index relies on `bucket` being an odd function), all scores divided by
`sqrt(dh·(1+#types))`. `Pk_l`, `Pq_l` are exact functions of the weights and are **pinned per layer at conversion** (0 in-program MACs;
in-program they would cost `2·T2·d²` = 604 M per layer at d = 768). c2p: `MatMul(Q[h,L,dh], PkT[h,dh,T2])`, `Transpose`,
`Reshape[L·T2, h]`, `Gather(axis 0)` with a pinned `[L,L]` index `i·T2+δ_ij`, `Transpose`; p2c the same with `j·T2+δ_ij` — the MPNet
bucket-gather pattern; a logits tile costs `3·64·dh` = 12 K MACs. `v3` has `position_biased_input = False`, `type_vocab_size = 0`; the
`conv_kernel_size` ConvLayer exists only in v2-xlarge-type configs.

**What decides registrability is TIR's limits, not the lowering** (lane model that reproduces the repo's own figures: BERT-base at 512
gives 2.14 M step leaves against the recorded 2,146,596; CLIP ViT-B/16 gives 0.54 M against 546,848): one carry signature per program
(`validate.rs:469-482`), tensor rank ≤ 4, ≤ 16 blocks, ≤ 512 nodes per block, `max_step_leaves` 2^22 and `max_position_macs` 2^40
(`admit.rs:56-69`).

| at L = 512 | job GMAC | step leaves (of 2^22) | without logits commit | max L (as is / no logits commit) |
| --- | --- | --- | --- | --- |
| BERT-base (calibration) | 48.3 | 2.14 M (51 %) | 1.55 M | 832 / 1,344 |
| ModernBERT-base | 65.3 | 3.52 M (84 %) | 2.44 M | 576 / 864 |
| ModernBERT-large | 190.7 | 6.95 M (166 %) | 5.11 M | 320 / 384 |
| ALBERT-base | 48.4 | 2.14 M (51 %) | 1.55 M | 832 / 1,344 |
| ALBERT-xxlarge (repo defaults) | 1,263 (over 2^40) | 11.4 M | 8.3 M | 192 / 256 (MAC cap: 416) |
| nomic v1.5 | 62.8 | 2.73 M (65 %) | 2.14 M | 704 / 992 |
| jina-v3 (5.17 defaults) | 167.5 | 5.71 M (136 %) | 4.13 M | 384 / 512 |
| EuroBERT-210m | 62.8 | 2.66 M (63 %) | 2.07 M | 704 / 1,024 |
| DeBERTa-v3-base (T2 = 512) | 53.2 | 2.14 M (51 %) | 1.55 M | 832 / 1,344 |
| DeBERTa-v3-large | 180.4 | 5.71 M (136 %) | 4.13 M | 384 / 512 |

Large and long variants miss the leaf cap at `tile_len` 64 (a lean q/k/v commit policy, as `vision.rs` uses, saves 10–13 %). The
enabler is **per-commit tile lengths** (`phase-f-integration.md` F6 allows up to 8 distinct ones), an admission/layout choice, not a
primitive; dropping the committed logits costs about `64·L·dh` MACs per tile (2.1 M at L = 512, 8.4 M at L = 2,048 against 16.8 M).
Blocked local layers for ModernBERT (block 64, 3 key blocks) save about 0.43 M leaves and ~60 % of those layers' score MACs.

**Suggested order:** (1) fix the ALBERT/embedding path and make `arch_of` strict; (2) make `bidir.rs` spec-driven (bidirectional/Band,
rope rows, gated/pre-norm/RMS/no-bias, `final_norm`, GQA, `share_layers`) — then ModernBERT, nomic, jina-v3, EuroBERT, ALBERT and
ELECTRA are adapters only; (3) merge the three layer blocks into one rows lowerer; (4) `Disentangled` and `Bucketed`.
**Acceptance**: `modernbert`, `nomic_bert`, `albert`, `deberta_v2`. *Evidence: adapters for the first three exist and stop at the
single failing step (`albert`: the panic above); capacity by lane model; DeBERTa decomposition by analysis.*

### FR-18 `ENCDEC_FROM_SPEC_V1` — encoder-decoders as data (L, unlocks `t5`, `t5_gated`, `bart`, `mbart`, `marian`, `longt5`, `t5_encoder`; ~35 true seq2seq families in 5.17)

**State.** `parse_encdec` (`lower/encdec.rs`) hard-wires five families in `EncDecSpec` with per-family tensor tables; every field
already maps onto a spec field: `d_kv ≠ d/h` → `AttnSpec.head_dim`; `act`/`gated` (`wi_0` = gate, `wi_1` = up) → `MlpSpec`; `rms`,
`bias`, `eps` → `NormSpec`; `pre_norm` → `Residual`; `Learned{rows, offset = 2}` → `positions` (BART); `Sinusoidal` → `positions.kind`
(Marian computed, Pegasus loaded); `Relative{buckets, max_distance}` → `Position::Bucketed`; `attn_scale` (T5: 1.0) → `AttnSpec.scale`;
`embed_scale`/`embed_norm`/`final_norm` → existing fields; `head_scale` (`d^−½`), `logits_bias` → `HeadSpec.pre_scale` / the existing
`lm_head_bias` role (`final_logits_bias`); `decoder_start` → class template, outside the spec.

**Request.** An encoder-decoder is **two `ModelSpec`s** mirroring the pipeline: the decoder is an ordinary causal spec with
`LayerSpec.cross: Option<CrossSpec{attn, norm}>` placed between mixer and FFN (the order in all five families) and
`ModelSpec.encoder: Option<Box<ModelSpec>>`, a bidirectional stage whose `output = CrossKv`, which stacks the decoder layers'
`xattn.k/v` projections into one `MatMul` (as `encdec.rs:1406-1505` does today). Roles use the decoder vocabulary (`embed`,
`pos_embed`, `embed_norm`, `rel_bias`, `attn.q|k|v|o`, `norm.mix`, `xattn.q|k|v|o`, `norm.cross`, `mlp.gate|up|down`, `norm.ffn`,
`final_norm`, `lm_head`, `lm_head_bias`); a shared embedding is one tensor name in both stages; per-family differences become adapter
variables. The wide q/k (`i16` weights for unscaled scores) is a lowering policy keyed on `scale == 1.0 && !bias`, not a spec field.
Features: `ATTN_CROSS_V1` (FR-21), `POS_SINUSOID_V1`, per-layer relative-bias tables (UMT5), `ATTN_BAND_V1` + blocked local attention,
`ATTN_TRANSIENT_GLOBAL_V1`, `OutputSpec::Rows` (FR-17). **Consensus-side, outside the spec:** a `TokenSource::Source` for the source token
list and a forced decoder prefix (`decoder_start`, mBART target language) in the class template.

**LongT5 (no primitive).** *Local*: `block_len = local_radius + 1`, a query attends to `{j : |i−j| ≤ local_radius, j < count}` — equal to
the Band mask plus T5's bidirectional bucket bias; a dense `[h,L,L]` with the band mask is identical, blocking (`Reshape`, three shifted
`Slice`s, `Concat`, pinned mask) only saves MACs and logit lanes. *Transient-global*: tokens are cut into blocks of `global_block_size`
(orphan tail joins the last complete block, padding −1); per layer, `G = L/gbs` block **sums** of the layer input (not means) followed by
that layer's `global_input_layer_norm` (RMS); side keys/values are the same `k`/`v` projections of those `G` rows; keys per query are its
local band plus all `G` side keys; scores unscaled; local bias from `relative_attention_bias[bucket(j−i)]`, side bias from
`global_relative_attention_bias[bucket(g − block_id[i])]`, both from layer 0 and reused. `full = Div(count, gbs, Floor)`,
`bid = Select(i<count, Clamp(Div(i,gbs), 0, full−1), −1)`, `S = Cast(Compare(Eq, bid[None,:], Iota_g[:,None]))`, `Gsum = MatMul(S, x)`
(commit, RMS, commit), `Kc = Concat([K, sk])`, split softmax over `L+G` keys. Side projections cost `2·G·d·inner` per layer (~1 % of the layer).

**Capacity** (pre-norm gated T5 stacks; relu for t5-small/base) — a cap limit, not a lowering limit:

| model | L | GMAC | step leaves | within caps? |
| --- | --- | --- | --- | --- |
| t5-small | 512 | 11.3 | 0.69 M | yes |
| t5-base / v1.1-base | 512 | 48.3 | 2.07 M / 2.26 M | yes |
| v1.1-large | 512 / 256 | 170.7 / 82.1 | 6.1 M / 2.66 M | over 2^22 / yes |
| v1.1-xl | 128 / 256 | 149.8 / 302.8 | 2.36 M / 5.1 M | yes / over 2^22 |
| v1.1-xxl | 128 / 256 / 512 | 595.9 / 1,198 / 2,422 | 4.7 M / 10.2 M / 23.6 M | over 2^22 / over 2^40 / over both |

So **T5-XXL, the encoder of Flux and SD3, is not registrable as one position at 256 or 512 tokens** under the present ceilings (FR-22
C5 and F6's per-commit tile lengths are the levers). LongT5's point is long input, but under `tile_len` 64 the `a·L·d` terms alone exceed
the leaf cap near L ≈ 1,300 for 12 layers (dense logits at L = 1,024: 5.7 M; blocked: 4.4 M; at L = 4,096 19 M even blocked, 4.8 M with 256-lane tiles).
Flux, SD3 and PixArt pass no attention mask to T5, so pad tokens are attended (the class must bind the padded `Final`).
**Acceptance**: `t5`, `t5_gated`, `bart`, `mbart`, `marian` first (today's Rust route must be reproduced by data, byte-identical
programs are not required, only the same admission/fidelity/court results), then `longt5` and `t5_encoder`.
*Evidence: capacity by lane model; semantics by analysis; the existing Rust route lowers the first five today.*

### FR-19 `VISION_FROM_SPEC_V1`, `CONV_2D_V1`, `POOL_2D_V1` — vision towers and convolutions as data (L, unlocks `vit`, `clip_vision`, `siglip_vision`, `resnet`; ~90 convolutional and ~50 ViT-style families)

**ViT family from one spec.** `EmbeddingSpec.patches: Option<PatchEmbed{image, patch, channels, temporal, bias, mean, std, prefix:
Vec<ParamRow>, downscale, merge}>` (CLS, distillation token, registers); positions learned (interpolated at conversion for the class's fixed
grid, bicubic for DINOv2) or `Rope2d{normalised}` (DINOv3, coordinates in [−1, 1], patch rows only) or per-layer relative tables (BEiT:
`[(2Wh−1)(2Ww−1)+3, h]` gathered by a pinned `[N+1,N+1]` index); outputs `Rows | Cls{norm, pooler} | MeanPool{fc_norm} | AttnPool |
Merger | Projector`; the body is the same rows-mode stack as FR-17. Patch embedding is a non-overlapping Conv2d = `Reshape`/`Transpose` +
`MatMul` (as `vision.rs:1085-1095`). **Two exact folds cost no node**: LayerScale `λ1/λ2` joins the per-output-channel narrowing multiplier `m`
and bias `z` of `attn.o` and `mlp.down`; BatchNorm's scale joins the same multiplier and its shift joins `z` (per-row quantisation absorbs positive
row scales, so the codes equal those of the unfolded weights).

**Conv2d by a pinned index table (no primitive).** Activations `[H·W, C]`; `x_ext = Concat([x, zero_row])` (the zero row is a `Broadcast` of a
scalar, since constants cap at 64 KiB); a pinned `idx[P_out, kh·kw]` param clamped to `[0, bound)` for range analysis (the `idx_param`
helper in `vision.rs`); `cols = Gather(x_ext, idx, axis 0)` → `[P, taps, C]` → `Reshape[P, taps·C]` → `MatMul(cols, W2)` → i64 → the
usual narrowing (`m`, `s`, `z`; ReLU is the narrowing's `Clamp` with `lo = 0`; BN folded). Stride, padding, dilation, asymmetric 'same'
padding, transposed conv (conv over a zero-stuffed input) and nearest upsampling are index-table content. Non-overlapping windows
(patchify, Swin patch merging, ConvNeXt stem/downsample) need only `Reshape`/`Transpose` at rank ≤ 4. Groups: a group axis leads and a batched
`MatMul` does the rest; depthwise: `Gather` → `[P, taps, C]` × a taps param `[1, taps, C]` → `ReduceSum` over taps (3,136 elementwise ops per tile for 7×7, no MACs).
Pooling: MaxPool(3, s2, p1) gathers windows with a sentinel row (dtype minimum) and `ReduceMax`es the taps axis (576 elementwise, 0 MACs);
AvgPool `ReduceSum` then `Div` (HalfAwayFromZero), with a pinned per-position divisor for `count_include_pad = False`; global/adaptive average pool
the same over the spatial axis.

**Box-demand verdict.** `operand_demand` counts elements (`d·contraction` to `MatMul` operands, `d` to `Gather`'s data and index), so a 64-lane tile is
over-counted by up to 64× — conservative, it admits more than the truth. ResNet-50's stem (7×7/2, 3→64 @224: P = 12,544, K = 147, 118 M MACs;
9,408 per output pixel; `cols` 1.84 M elements; idx 614,656 entries) and the 256-channel 3×3 @56² (P = 3,136, K = 2,304, 1.85 G MACs; tile 147 K MACs = 0.88 % of
16 Mi; `cols` 7.23 M elements < 2^28; the tile opens ~1.3 MB against the 64 MiB cap, with < 8 operand sources against 64) are both admitted — **provided every conv output is a
commit point** (i16 codes after narrowing); otherwise the next conv's tile demands ~147 K outputs of the previous one, ~340 M MACs, and fails.
ResNet-50 as a whole: 53 convs, 4.09 GMAC (0.4 % of `max_position_macs`), ~11.1 M conv-output lanes plus 12.9 M carry lanes, ~0.39 M step leaves.

**What blocks a CNN is the program format** (found by reading `validate.rs`, `program.rs`, `types.rs`, not by running): (1) *one carry signature* — every
layer block's carry-in and carry-out, and the post block's carry-in, must equal the pre block's carry-out (`validate.rs:469-482`), so a feature map
that changes shape per stage (ResNet-50 `[3136,256] → [784,512] → [196,1024] → [49,2048]`) needs a fixed worst-case flat carry (802,816 elements) with each block
`Slice`+`Reshape`-ing the valid prefix and padding its output back with `Concat` of a `Broadcast` zero tail (Swin and ConvNeXt have the same problem);
(2) rank ≤ 4 (`types.rs:159`): windows, Conv3d and grouped conv need merged dims; (3) ≤ 16 blocks: ResNet-50 needs 8 layer kinds plus pre and post, EfficientNet-B0 exactly 16,
MobileNetV3 or NAS nets exceed it; (4) ≤ 512 nodes per block is not binding (a bottleneck is ~50). FR-22's **C1** (per-occurrence carry signatures) removes (1) and (3).
If a primitive were ever wanted, the minimal GENERAL one is a strided window view `Unfold{axis, size, stride, dilation}` of kind S, which removes the index params and makes demand exact; **not needed now**.

**ResNet as data.** A `ConvNetSpec` with ops `Conv{out,k,stride,pad,dil,groups,bias,bn,act}`, `Act`, `Pool{max|avg,k,s,p}`, `GlobalPool`, `Linear`, `LayerNorm2d`, `SE`
and a per-block `shortcut: Identity|Conv1x1{stride}`; an adapter generates the stages with `$map`/`$range` over `depths` and `hidden_sizes`. Fixture names
`embedder.embedder.convolution`, `…normalization.{weight,bias,running_mean,running_var,num_batches_tracked}`, `encoder.stages.S.layers.B.layer.N.…`;
the unread `num_batches_tracked` goes in `ignored_prefixes`.

| tower | rows | GMAC | step leaves | without logits commit |
| --- | --- | --- | --- | --- |
| ViT-B/16 @224 | 197 | 17.4 | 0.54 M | 0.45 M |
| ViT-B/16 @384 | 577 | 55.1 | 2.08 M (50 %) | 1.33 M |
| ViT-L/14 @224 | 257 | 80.9 | 1.98 M | 1.58 M |
| ViT-L/14 @336 | 577 | 190.6 | 5.55 M (132 %) | 3.55 M |
| ViT-H/14 @224 | 257 | 167.1 | 3.16 M (75 %) | 2.64 M |
| SigLIP so400m/14 @224 / @384 | 256 / 729 | 109.3 / 332.7 | 2.37 M / 9.08 M (216 %) | 1.93 M / 5.49 M |
| DINOv2-L/14 @518 | 1,370 | 506.0 | 19.7 M | 8.4 M |

**Acceptance**: `vit`, `clip_vision`, `siglip_vision` (reproduce today's Rust route by data), then `resnet` (needs the padded carry or C1).
Where the registry already says it: RFC-0003 owns the vision-stage binding; this request is only about lowering a tower from a spec.

### FR-22 `GEN_DENOISER_V1` and its protocol pieces — image generation (XL, unlocks `dit`, `flux`, `sd3`, `vae_kl` as routes and `unet2d_condition`, `unet_sdxl` after C1)

Basis: a read of diffusers 0.40, `04b` §5–10 and §15, RFC-0003 §I.1/II.1/5/6, `pipeline.rs`, `program_v2.rs`, `admit*.rs`, `demand.rs`, and the six tiny
fixtures; **nothing was executed**, every number below is the research pass's own arithmetic.

**Verdicts.** (1) **No new primitive**: all 17 operators lower to the 25 (table below). (2) **DiT, MMDiT, Flux, SD3 and AutoencoderKL are a route problem**: NF-5 holds if the carry
is the concatenated text+image stream; the loop (`TripRule::JobSteps`), latent state (global `Fixed`, `post` `StateWrite`, NF-29), noise (`Random{domain, Normal|Uniform, per_step}`),
tables, CFG scalars (`JobScalar`), negative prompt (`TokenSource::Negative`), edges, VAE stage chain and `ImageRgb8` output already exist in `TirProgramV2`/`TirPipelineV1`.
(3) **UNets are not expressible today** — blockers NF-5 (one carry signature), NF-4 (≤ 8 carries; SD-1.5 holds 12 skips) and NF-21 (every carry-out is committed, so each skip
is re-committed per block), plus NF-2 (≤ 16 blocks; SD-1.5 has ~21 distinct kinds, ≤ 4,096 params, 256 KiB program); RFC-0003 §3.5 already defers this as "carry pass-through".
(4) **Ceilings** (`legacy_court_v1`, `open_v1`): SD-1.5 and DiT-XL/2 pass; SDXL needs position MACs ×3.1, SD3-medium ×3.8, FLUX ×34 (and exps ×6.8), T5-XXL ×2.2.

**Operators (each a recipe over existing primitives; "lossy sites" = one narrowing or table approximation):**

| operator | recipe | lossy | cost per pass |
| --- | --- | --- | --- |
| Conv2d 3×3 s1 p1 | `Concat` zero row; `Gather` im2col (idx `[P,9]` param or in-program `Iota/Div/Select/Clamp`); `Reshape [P,9C]`; `MatMul`; narrowing (the 9×2 `Slice` form costs ~45 nodes vs ~6) | 1 | 9·P·Cin·Cout (SD 64², 320→320: 3.8 G) |
| strided conv, 1×1, patch-embed, depthwise/grouped | as above on the P/4 grid; `MatMul` (batch dim for groups) | 1 | — |
| GroupNorm | `Reshape [P,G,C/G]`; two chained i64 `ReduceSum`s (associative: bit-identical to a multi-axis sum, PALW-TIR-24); exact centring `n·x−Σx`; `Log2Floor` + `IntRsqrt`; γ,β in the narrowing; **commit row partials `[H,G,2]`** | 2 | ~6·P·C elementwise, 0 MACs |
| timestep/label/guidance embedding | sinusoid evaluated at registration into a pinned table; `Gather` by `Input(1)` + `JobScalar`; MLP = `MatMul`, SiLU table, `MatMul` | 2 | 2 M (SD), 10 M (Flux) |
| SiLU / GELU | `act_table`: `Gather(T[65 536], code+32 768)`, T a param (128–256 KiB, above the 64 KiB const cap) | 1 | P·C gathers |
| Upsample ×2 | nearest: `Reshape`→`Broadcast`→`Reshape` (twice at rank 4) or one `Gather`; bilinear: 4 `Gather` + dyadic `Mul`/`Add` + `Div` | 0 / 1 | then a conv at 4× pixels |
| spatial self/cross-attention | q,k,v `MatMul`; head-major; `S = MatMul(q,kᵀ)` → Q24 → two-pass softmax (`ReduceMax`,`Sub`,`IntExp`,`ReduceSum`,`IntRecip`) → `MatMul(P,v)`; commit `m`,`r` | ~4 | 4P·c² + 2P²·c (SD L0: 12.4 G, 134 M `IntExp`) |
| ResNet block with timestep | GN→SiLU→conv3→`Add(Broadcast(Linear(SiLU(temb))))`→GN→SiLU→conv3→`Add` shortcut | ~8 | SD L0: 7.5 G |
| adaLN-Zero / continuous / single | `MatMul(SiLU(c),W)` `[1,k·d]` committed; `Slice` chunks; `layer_norm_exact`; `Mul(ONE+scale)`, `Div`, `Add(shift)`; `x + gate·f(x)` | ~3 | Flux: 76 linears ≈ 4 G per pass |
| patchify/unpatchify | `Reshape,Transpose,Reshape,Transpose,Reshape` at rank ≤ 4 | 0 | 0 MACs |
| joint text–image attention (SD3, Flux double) | carry = concatenated `[L+N,d]`; `Slice` per stream; per-stream q,k,v + QK-RMSNorm; `Concat`; ONE attention; `Slice` back | ~10 | Flux double block 6.5e11 |
| Flux single stream | `AdaLayerNormZeroSingle`; `proj_mlp` ∥ attention; `Concat`; `proj_out`; gate; residual | ~8 | 38 blocks 2.5e13 |
| 3-axis RoPE (`FluxPosEmbed`) | static ids per class by `Iota/Div/Sub`; per-axis cos/sin Q24 param tables (θ in f64 at registration); `Gather`+`Concat` | 1 | elementwise |
| VAE decoder stack | chain of single-position stages per level (NF-5); last stage `Clamp(Div_HAFZ((y+ONE)·255, 2·ONE), 0, 255)` + `Transpose` | ~40 | 1.26e12 (512²), 5.2e12 (1024²) |

Notes: DiT's diffusers conversion repeats the timestep/class embedder in every block (28 copies in DiT-XL), so these are per-layer params. Stable-Audio, CogView4
and Cosmos use half-split rotation (`rope_partial` has both pair styles).

**Cost at real sizes** (assumptions: ResNet commits conv1 and conv2+residual outputs; a UNet transformer layer commits ~14–15·N·c lanes; a DiT block ~9–10·N·d; partial stats committed;
each commit point uses the largest tile with `T·K ≤ 16 Mi` MACs and ≤ 3.2 MB of 1-byte weights; a lane is 4 bytes):

| model | tokens (dh, heads) | MACs/pass | `IntExp`/pass | leaves/pass: box → exact pricing | position ceilings (2^40 MACs, 2^32 exp, 2^22 leaves) | job vs 2^50 |
| --- | --- | --- | --- | --- | --- | --- |
| SD-1.5 UNet 64² | 4,096 (40, 8) | 4.0e11 | 7.3e8 | 2.6e5 → 1.2e5 | all ok | 0.02 |
| SDXL UNet 128² | 4,096 (64, 10) | 3.4e12 | 3.1e9 | 2.5e6 → 1.0e6 | MACs ×3.1 | 0.18 |
| DiT-XL/2 32² | 256 (72, 16) | 1.2e11 | 2.9e7 | 6.5e4 | ok | 0.01–0.05 |
| SD3-medium 1024² | 4,250 (64, 24) | 4.2e12 | 1.0e10 | 6.0e6 → 1.1e6 | MACs ×3.8, exp ×2.4, leaves over (box) | 0.21 |
| FLUX.1 1024² | 4,608 (128, 24) | 3.7e13 | 2.9e10 | 6.1e7 → 1.0e7 | MACs ×34, exp ×6.8 | dev 28: 0.93; 50 steps: 1.65 |
| VAE decode 512² / 1024² | mid-attn 4,096 / 16,384 | 1.26e12 / 5.2e12 | 1.7e7 / 2.7e8 | — | at 1024², 3 of 5 level stages exceed 2^40 | — |

Three findings on admission: **(a) box vs exact pricing** — an attention-output tile of T lanes is charged `T·N·(dh+1)` MACs (the exact figure is `(⌈T/dh⌉+1)·N·dh + T·N`; the BERT
refusal in `hf-coverage` §12 is the same mechanism); with `m` and `r` committed the largest T under 16 Mi is SD L0 64 (exact 1,024), SDXL 63, SD3 60, Flux 28 (exact 1,024), DiT-XL 897, VAE
mid 7; `admit_class` prices every cone at the stage's widest tile with tile ceilings `u64::MAX`, so today nothing refuses and the court would later. **(b) carried bytes** — one head's K,V is
`8·N·dh` bytes: 1.3 MB (SD L0), 2.1 (SDXL), 2.2 (SD3), 0.15 (DiT), **4.7 (Flux)**; the RFC's "15.6k tokens at dh 128" becomes ~3.1k under PALW-TIR-38's 3.2 MB (t12), so Flux is over by ×1.5
and the VAE mid-attention (16.8 MB at 512²) needs chunked partials or dissection. **(c) commit volume dominates**: SD-1.5 commits 33 GB per job (20 steps, CFG), SDXL 0.39 TB, Flux-dev 0.9 TB;
the leaf count in `admit_class` is "a necessary condition only". *Discrepancy to resolve first*: RFC-0003 §II.1.5.4 sizes cones with element-exact demand and a 16.8 MB close, while the code prices by
box demand (04b §10.3 `tile_demand`) and PALW-TIR-38 bounds a carried close at 3.2 MB.

**Protocol pieces:** the fixed-trip loop, latent state, noise `R`, σ/coefficient tables, guidance scalars, text-encoder stages (CLIP causal `Rows`, T5), VAE decode and output, img2img/control
images all exist (§4 of the research; the integer forms below); missing: UNet skips (C1) and the exact leaf count across stages (04b §15.11). Integer forms with `x` the latent `Fixed i32`
at `q_lat` and tables as consts per `(steps_idx, pos)`: flow-match Euler `x' = StateWrite(x + HAFZ(dσ_i·v/2^24))`; DDIM/DDPM/Euler-discrete `x' = HAFZ((A_i·x + B_i·m + C_i·z)/2^24)` with
ε/v/x0-prediction and η folded into Q24 table entries (the `C·z` term only for stochastic samplers); multistep samplers add columns and `post`-written `Fixed` states; CFG
`m = m_u + HAFZ(g_q·(m_c − m_u)/16)` with `g_q` a `JobScalar`, as a batch axis B = 2 or two positions; init `x = Select(pos == 0, HAFZ(noise/2^(24−q)), State)`.

**Requests (all general):**

* **C1 `CARRY_HETERO_PASSTHROUGH_V1`** (program format; unlocks UNets): a per-occurrence carry signature with an adjacency check replaces NF-5; a carry-out may name a `CarryIn` as an alias — no node, no
  new commit, the court resolves it to the producer's leaf (a structural edge, as `StageFinal` is); caps blocks 16→64, carries 8→32, params 4,096→16,384, program bytes 256 KiB→1 MiB, or use the Gather-conv form.
* **C2 `ADMIT_RANGE_PRICING_V1`** (admission, under a fence in the style of `TirDemandRulesV1::H7`): row- and range-aware pricing of tiles (`MatMul`, `Reduce`, `Broadcast`, `Gather`) and per-commit-point
  tile lengths; not a primitive, not a court kernel (`demand.rs` already pulls element-exactly); cuts 4k-token attention leaves by 10–60×. This is also FR-17's per-commit tile-length lever.
* **C3 `DISSECT_DECLARED_AXIS_V1`** (RFC §5.6 / PALW-TIR-32′; 04b §15.11 lists it as coming "with video"): needed earlier than the RFC says — Flux-class at dh = 128 above ≈ 3.1k tokens, and the VAE mid-block.
* **C4 `TRIP_STEPS_MUL_V1`** (optional `TripRule::JobStepsMul{k}`): CFG as two positions halves per-position MACs, leaves and exps; also Heun/RK2 and frame loops.
* **C5 profile ceilings** (consensus parameters, not code): image profile `max_position_macs ≥ 2^46`, `max_position_transcendentals ≥ 2^35`, position step leaves ≥ 2^24, `max_job_macs ≥ 2^51` for 50-step Flux-class; the
  decoder is split per level (8 stages at 1024²; the total is ≈ 12 of the cap 16).

**Library/lowerer vocabulary** (ids in the `<AREA>_<NAME>_V<n>` style): `CONV_DENSE_V1` (nd 1/2/3, kernel, stride, dilation, pad mode, groups; 3-D = Σ 2-D taps, causal cache = `Fixed`), `CONV_TRANSPOSE_V1`,
`RESAMPLE_FIXED_V1`, `NORM_GROUP_SPATIAL_V1` (i64-wide: library `group_norm_exact` groups the last axis only and casts `n·x−Σx` to i32, and `n = P·C/G` is 40,960 at SD L0 and 4.2 M at the 1024² VAE, so the interval proof fails —
an i64-wide variant of `rms_norm_wide_q36` is needed; a stat tile's box demand is the whole tensor, 5.2 MB at SD L0, 537 MB at the 1024² VAE, so row partials must be committed), `ACT_TABLE_V1`, `EMBED_TIMESTEP_TABLE_V1`,
`POS_TABLE_V1`, `MOD_ADALN_V1`, `ATTN_FIXED_AXIS_V1`, `ATTN_JOINT_STREAMS_V1`, `ROPE_AXES_STATIC_V1`, `PATCH_EMBED_V1`, `BLOCK_RESNET_TEMB_V1`, `UNET_SKIP_V1` (needs C1), `GEN_STAGE_DENOISE_V1`, `GEN_SAMPLER_AFFINE_V1`,
`GEN_GUIDANCE_V1`, `GEN_STAGE_VAE_V1`, `GEN_TEXT_TAP_V1`. Params are artifact tensors (≤ 2^40 elements), so an im2col index of 147 KB (SD 64²), 590 KB (SDXL 128²) or 37.7 MB (VAE 1024²) is fine against NF-9's 64 KiB const total;
a param index needs a never-firing `Clamp`, because the Gather obligation sees the full dtype range; an in-program index is uncommitted and the court computes only demanded elements; the 9-slice conv form (~45 nodes × ~60 convs) would overshoot
the 256 KiB program cap. Spec skeleton (in the style of `ModelSpec`, layers fully expanded):

```
GenSpec { profile, latent{channels,compression,pack,scaling,shift,resolution,q_lat},
          sampler{family,prediction,schedule,offered_steps,order}, guidance{mode,range,rescale},
          denoiser: Unet(UNetSpec) | Dit(DitSpec), text: [TextSpec{role,tap,pool,template}], vae: VaeSpec }
DitSpec { patch, d, heads, head_dim, cond{timestep,class,pooled,guidance},
          layers: [{kind: Single|Joint|SingleStream, mod{chunks}, norm, attn{qk_norm, rope, joint, cross, mask}, ffn{ratio,act,gated}, context_pre_only}], out }
UNetSpec { in/out_ch, time, add_embed, down/mid/up: [{resnets, channels, attn?{depth,heads,ctx}, resample}], groups, eps, act, linear_proj }
```

**Reader gaps for a data adapter:** treat `model_index.json` plus each component's `config.json` as one JSON bundle (as VLMs nest `text_config`; the adapter `$scope`/`$root` operators already handle this). Component
configs carry `_class_name`, not `architectures`, so `hf_schema::read_model` refuses at `hf_schema/mod.rs:170–177`. UNet keys: `attention_head_dim` is the head **count** (the code does `num_attention_heads or attention_head_dim`), while in DiT,
SD3 and Flux it is the true head dim; `transformer_layers_per_block` is an int or a list (reversed for up blocks); `use_linear_projection` selects 1×1-conv weights vs Linear; `addition_embed_type = text_time` with its dim keys;
inert: `sample_size` (the class carries the resolution), `dropout`, `upcast_attention`, VAE `force_upcast`. Scheduler keys (`_class_name`, `prediction_type`, `beta_schedule`, `timestep_spacing`, `shift`, `use_dynamic_shifting`,
`use_karras_sigmas`, `solver_order`) are all consumed at registration into tables. Tensor names: conv weights are OIHW; GEGLU `ff.net.0.proj` has 2× rows (Flux/SD3: plain 4d); the VAE mid-attention carries biases; the DiT embedder sits at
`transformer_blocks.N.norm1.emb`. Closest existing code: `lower/vision.rs` (patchify, normalisation folded into the projection, 2-D rope with window masks, `split_softmax`/`softmax_committed`/`resid_commit_needed`), `bidir.rs` (a DiT
block minus modulation), `encdec.rs` (cross-attention to a `StageFinal`), `encoder.rs::lift_v2`, `vision_pipeline()`; **not close**: no conv, no spatial GroupNorm, no modulation, no loop/state/`Random` stage, no `_class_name` reader, no float reference for diffusion, no multi-component bundle.

**Ranked by diffusers families that need them** (marker-greps over diffusers 0.40 `models/`: 68 transformer files, 11 UNets, 35 autoencoders, 13 ControlNets; approximate): (1) `ATTN_FIXED_AXIS_V1` + C2 ≈ 80 (the gate for any real size);
(2) `MOD_ADALN_V1` + `EMBED_TIMESTEP_TABLE_V1` ≈ 60; (3) `ROPE_AXES_STATIC_V1` ≈ 54; (4) QK-norm ≈ 51 (implemented); (5) conv + spatial GroupNorm + resample ≈ 35 (prerequisite of every image output); (6) 3-D conv + causal state + C3 ≈ 33;
(7) `ATTN_JOINT_STREAMS_V1` ≈ 27; (8) `UNET_SKIP_V1` (C1) ≈ 22 — the largest by repo volume, since SD1.x/XL finetunes dominate the text-to-image repositories (unmeasured); (9) Conv1d + transpose + snake ≈ 8; (10) linear-ReLU attention 3.
Unlock curve in the order 2, 3, 5, 7, 1: the image-DiT core ≈ 28 families (DiT, PixArt, SD3/3.5, Flux.1/Kontext/Fill/Flux.2/Chroma, AuraFlow, Lumina, CogView3/4, Z-Image, Bria, HunyuanImage, HiDream, …); with C1 ≈ 16 more (SD1.x/2.x/XL/Turbo/LCM,
Kandinsky, ControlNet family); with 3-D conv ≈ 20 video; with ranks 9–10 ≈ 8 audio/linear-attention.
**Acceptance**: `dit` (smallest: needs conv-free patchify, adaLN, joint-free attention, loop), then `sd3`/`flux`, `vae_kl`; `unet2d_condition`/`unet_sdxl` need C1.
*Evidence: analysis only. Cheapest checks: admit a synthetic 4,096-token block (dh = 40, T = 64 vs 128) to confirm 10.5 M vs refusal, and measure SD-1.5 program bytes for the Gather-conv form. A16 fidelity of the VAE (outlier channels) is unassessed.*

### FR-23 `AUDIO_IO_V1` and the audio feature set — front end, Conv1d, codecs, audio input and output (XL, unlocks `whisper`, `wav2vec2`, `speecht5`, `musicgen`, `encodec`; 49 of 59 audio-related directories of transformers 5.17 after four build steps)

Basis: transformers 5.17 (whisper, wav2vec2, speecht5, musicgen, encodec, moonshine, hifigan), the five tiny fixtures, the IR/pipeline/admission sources of this branch, the consensus gen code on `rfc3/lower` and RFC-0003; **nothing was run**, all sizes are the research
pass's own arithmetic (its commit-density model reproduces the measured BERT-base row, 2.15 M leaves at 512 tokens, and the Qwen2-VL tower, 3.5 M, to ~15 %).

**Findings.** (1) **No new primitive**: the DFT as one `MatMul` costs 4.8e8 MACs per 30 s window (0.04 % of Whisper-large's encoder), and a conv tile costs `tile_len·K` MACs under box demand however it is written. (2) The gaps are protocol surface, lowerer features and an audio
ceiling set: there is **no `JobAudio`** in code or docs (only RFC §II.5's `AudioInputRefV1` text and hf-coverage §14's one-liner); the Audio profile exists as an enum value (tag 3) with output kind `PcmI16` and RNG domains 3 and 4, but "a profile with no body is not built" (RFC §I.4.5).
(3) Five facts in existing code decide feasibility, each with a fix and no primitive:

* **One position, one ceiling.** A tower is one position, so `max_position_macs` binds: 2^40 = 1.10e12 in the legacy and drill ceilings; Whisper-large-v3's encoder is 1.14e12 (1.29e12 with cross K/V). Fix: split the tower into 2–4 stages joined by committed `[1500, 1280]` rows, or set an audio ceiling (none exists; the drill values are placeholders).
* **Committed logits at L = 1,500.** `bidir::split_softmax` commits the masked logits `[h, L, L]`: 27 M (small) or 45 M (large-v3) lanes per layer — 22.5 M leaves at tile 64 against `max_step_leaves` 2^22. Fix (`ATTN_STAT_COMMIT_V1`, the same lever as FR-17/FR-22 C2): commit only the row max and reciprocal and recompute the logits in the cone; box bound ≤ 6.2 M MACs at 64 lanes, 12.3 M at 128, against 16.8 M; ~1,000× fewer lanes.
* **Commit and `IntLn` traps.** i64 and i128 are not committable (PALW-TIR-5) and `IntLn(x ≤ 0) = 0`: wide values must be narrowed or split into two i32 limbs before a commit point, and the log-mel's `1e-10` floor must come before `IntLn`.
* **NF-5.** Every layer block carries the same tensor type, so a conv stack whose length and width change per layer cannot be "layers": it goes in `pre` (≤ 512 nodes) or in separate pipeline stages; HiFi-GAN and the codecs are stage chains (FR-22 C1 would lift this too).
* **NF-P9′.** The text stage must be the last and no stage can read its generated ids, so a codec-LM followed by a codec decoder (MusicGen, Dia) cannot use `TextStream` today.

**Operators (E = exact integer reorganisation, L = lossy site; per pass):**

| operator | TIR form | E/L | cost |
| --- | --- | --- | --- |
| Whisper front end: pad/trim to 30 s | the gateway zero-pads to the class's `frames` (480,000); the zeros are committed input | E | none |
| reflect-pad and frame (`center = True`, 3,001 frames, last dropped) | `Iota`, `Add`, `Select` build the reflect index; one `Gather` to `i16 [3000, 400]` | E | 1.2 M elements |
| Hann window + 400-pt DFT | one `MatMul` with a pinned `[400, 402]` table (window·cos or −window·sin; i16 Q14 or i32 Q30) into i64, `Div` HAFZ 2^9, `Clamp` to i32 re/im (commit) | L: table rounding, the shift | 4.8e8 |
| power and mel (Slaney) | `Mul`, `Mul`, `Add` in i128, `Div` to i64, `MatMul [3000,201]·[201,n_mel]` (weights i32 Q32; ≤ 0.015, so Q15 keeps ~9 bits) | L | 4.8e7 (80 bins), 7.7e7 (128) |
| log10, clamp, scale | `Select` floor, `IntLn` (i64 in, Q24 out), `Mul` log10 e; `ReduceMax` ×2 to a committed scalar, `Sub` 8, `max2`; `(x+4)/4` folded into conv1 (`W/4`, `b+ΣW`) | `IntLn` ~1e-7; rest exact | 240 K (384 K) transcendentals; the max cone reads 0.96–1.5 MB |
| Conv1d stems (k3 s1, k3 s2, pad 1) | `Concat` zero row, `Gather` windows (index `Iota·s + Iota`, no table), `MatMul [T_out, 3·Cin]·[3·Cin, d]`; GELU by `act_table` (exact on the A16 grid) or `gelu_erf_q24` (≤ 1.5e-7) | E; L at narrowing/GELU | 1.5e9 + 7.4e9 (large-v3) |
| Whisper self-attention, 1,500 frames | `q·(1/8)` folded into `W_q`, `k_proj` has no bias; stat-only softmax; `layer_norm_exact` | L: `IntExp`, floor | 3.5e10 a layer; 1.1e12 for 32 layers |
| cross K/V | all `2·D` projections in one `MatMul` in `post`, `Final i16 [D, 2, 1500, d]` (the `encdec` design) | L | 1.6e11; 123 M lanes (246 MB) |
| decoder | `encdec` decoder (`Hist` K/V, cross-attention over the fixed axis, learned positions, tied head); `suppress_tokens` as an `Add` of a pinned mask | | 0.92 GMAC per token (large-v3) |
| wav2vec2/HuBERT/WavLM: utterance normalise | Σx and Σx² as two i32 limbs per chunk, then `IntRsqrt` | L | ~3N |
| 7 strided Conv1d | as the stems; 10 s gives 31,999 → 499 frames; GroupNorm(512, 512) on layer 0 is a per-channel statistic over time, done as committed chunk partials (RFC §II.1.5.5); LayerNorm per frame in `layer` models | E/L | 2.5e10 |
| pos-conv (k128, groups 16, weight norm) | fold `g·v/‖v‖` over dims (0,1) at registration; `Gather` windows `[T,128,768]` → `[16,T,6144]` against a batched `MatMul`; `Slice` drops the SamePad element; GELU, `Add` | E | 2.4e9 |
| encoder + CTC head | the `bidir.rs` body (post-LN base, pre-LN stable-LN large); head `MatMul` to Q24 `Rows [T,V]`; optional `TopK(1)` argmax; the CTC collapse is outside | | 4.7e10 (12 layers, 10 s) |
| SpeechT5 pre/post-net | **HF applies dropout p = 0.5 even in eval**, so the reference is stochastic: model it with `Random` AUDIO_STEP_NOISE (domain 4, Uniform16) against a threshold, ×2 — matches HF in law only; speaker x-vector via `l2_norm`; Shaw relative keys `MatMul(q, pe_kᵀ)` + `Gather` by clipped `j−i`; post-net conv k5, BatchNorm folded, tanh, residual | | encoder 8.7e9 (100 tokens), post 7e8 |
| mel autoregressive loop | a `Rows` program over `JobSteps`; `Fixed` state `prev_mel[80]` written in `post` (the denoiser pattern); `prob_out` through `Sig` as a committed stop row; the client trims | | 5.3e7 a step, 312 steps |
| HiFi-GAN | per stage leaky ReLU (`Select`, slope as Q24 `Mul`; the last leaky ReLU uses torch's default 0.01, not the config's 0.1); ConvTranspose (k8 s4) as one **polyphase** `MatMul [T, 2·Cin]·[2·Cin, 4·Cout]` + `Reshape` (exact; the RFC's zero insertion wastes 3/4 of the MACs); 3 resblocks (k 3/7/11, dilations 1/3/5 via `Gather` windows) averaged by `Div` 3; conv_post, `tanh_q24`, ×32767, `Clamp` to `PcmI16` | L | 8.5e10 per 10 s (4 stages ≈ 2.1e10 each) |
| MusicGen embeddings and heads | one `Gather` from `[K·V, d]` (id + k·V) then `ReduceSum` over K; sinusoid table (cos then sin halves) at registration; K heads as one `MatMul [K·V, d]`; `enc_to_dec_proj` folded into the cross K/V weights | E | 4.0e8 per step (small) |
| delay, CFG, sampling | delay pattern `Select(Compare(pos, k), pad, id)`, un-delay by `Gather` (index `t+k`); CFG as a batch axis of 2 with `l_u + g(l_c−l_u)` on log-softmax; in-program sampling (`Random` Uniform16 → an artifact Gumbel table, `TopK(1)`; top-k via `TopK` + a `Compare` mask) | exact given R | ×1,503 ×2 = 1.2e12 |
| EnCodec conv encoder/decoder | reflect pad by `Gather`; dilated residual blocks; ELU as `Select` + `IntExp`; weight norm folded; ConvT polyphase + `Slice` trim | L | decoder 30 s: 4.0e10 (24 kHz), 1.2e11 (32 kHz) |
| 2-layer LSTM with residual | a stage with one position per frame; `Fixed` h, c per layer; gates in one `MatMul` over `[x; h]`; `Sig` and `tanh_q24` (i,f,g,o order) | L | 9.4e9 / 2.5e10 per 30 s |
| RVQ | encode: `MatMul(x, Eᵀ) − ‖e‖²`, `TopK(1)`, `Gather`, `Sub` residual per codebook; decode: `Gather` + `ReduceSum` | E given inputs | `T·V·128` per codebook (2.9e8–3.9e8) |

**New general primitive? No.** FFT: the DFT by `MatMul`; conv/ConvTranspose: `Iota` + `Gather` windows and `MatMul` (polyphase for ConvT); argmin/argmax: `TopK(k = 1)` with ties to the lowest index (matches `torch.argmax`'s first occurrence); cumsum and length regulator: a `MatMul` with a pinned
lower-triangular matrix (`n²` MACs, `n ≤ 1,024`), searchsorted as `Compare` + `ReduceSum`; sin, snake, ELU, leaky, tanh, GELU: tables with range reduction, `IntExp`, `Select` (the library has `act_table`, `tanh_q24`, `gelu_erf_q24`); LSTM/autoregression: a stage per step with `Fixed` state (PALW-TIR-36 forbids a `Scan`, and none is needed);
noise: `Random` inputs exist (Normal, Uniform16); sqrt: `IntRsqrt·x`. If one thing were ever added it would be RFC §II.1.5.6's dissection over a declared reduction axis — a flag plus a court version, not a primitive — and it would only save commit volume beyond ~15 k rows.

**Protocol pieces (what exists, what is missing):**

| piece | exists | missing, precisely |
| --- | --- | --- |
| audio input | `Binding::JobImage` (tag 6, `pipeline.rs:82-103`); `ParamLeafV1::JobImage` at 1 byte per lane (`admit.rs:166-196`); `PalwGenImageInputRefV1` and offer slots; `input_image_root_v1` | `Binding::JobAudio` (tag 7) into `i16 [frames(, ch)]` over [−32768, 32767]; `PipelineJob.audio`; `ParamLeafV1::JobAudio` at 2 bytes per lane; `input_pcm_root_v1`; `PalwGenAudioOfferV1` in the offers (borsh-positional, so it changes every class id: append a versioned struct before the `palw_gen_v1` arms); an audio tail on FP V5 or a new job version; a court answer variant; close-builder carriage; the close-size twin's read set; golden vectors |
| derived frame count | the `JobTokenCount` pattern | `JobAudioFrames` (rank-0 idx) for masked classes |
| audio output | `OutputKindV1::PcmI16`, `OutputSpecV1::pcm_i16`, profile Audio = 3 (output must be `PcmI16`), domains 3 and 4, a ceilings slot | the job body, and real ceilings |
| tensor outputs (CTC rows, mel) | `TensorLe` kind 5; `EmbeddingI32 [n, d]` (Q24 works for `[T, V]`) | no profile carries `TensorLe`: add `Tensor = 6` or overload Embedding |
| speaker vector | `JobScalar` is rank-0 only | `JobTensor` (rank ≤ 2, hashed like prompt ids), or one class per speaker |
| multi-stream codec-LM | `Rows` programs with post `StateWrite`; `JobSteps` (≤ 8 offered counts); `Random` per-step inputs | see option B below |
| variable length | `Fixed` trips and offers | one class per length bucket (as per image resolution); count-masked classes for layer-norm models only |

**Codec-LM streams ("TextStream with K codebooks?").** *Option B, recommended: a `Rows` stage* — the codec-LM runs as a `Rows` stage over `JobSteps` positions with K sampled i16 ids in a `Fixed` state written in `post`; the sampler is a generic library template (Gumbel-max over `Random` Uniform16 words through an artifact Gumbel table, then `TopK`; temperature, top-k and guidance are `JobScalar`s);
the codec decoder reads the ids by `StageRows`. No consensus change beyond the audio job and offers and no new RNG domain; RFC-0001's decode lane (penalties, stop sequences) is unavailable to such classes. *Option A: multi-row `TextStream`* — K logits rows per position in the FP scheme is RFC open question 11 (a new FP job version, a `StageGenerated` binding, NF-P9′ relaxed); worth it only if codec-LMs must share RFC-0001's decode controls.
Whisper-like decoders keep `TextStream` unchanged. Long audio is one class per window (the client slices; Whisper's long-form algorithm is client orchestration of independent jobs); beam search and temperature fallback are outside the scheme; timestamps are expressible in `post` as a mask driven by a state fed from `Input(0)` (ship v1 with the no-timestamps prompt); `dither ≠ 0` must be refused by name (it is random).

**Cost and admissibility** (ceilings: per tile 16 Mi MACs, 1 Mi transcendentals, close carriage ≤ 3.2 MB on testnet-12; per position 2^40 MACs, 2^32 transcendentals, 2^22 step leaves (format cap 2^26); per job `open_v1` 2^50 MACs, 2^32 leaves; drill 2^46 / 2^30; leaves at tile 64 unless stated):

| class | job MACs | evidence and leaves | binding ceiling |
| --- | --- | --- | --- |
| Whisper tiny, base | 2.0e10, 4.8e10 | 1.6 M, 3.2 M leaves | none |
| Whisper-small, 30 s | 1.9e11 | K/V 27.6 M lanes (55 MB); committed logits 9.6 M leaves at 64, 2.4 M at 256; stat-only 1.1 M at 256 | step leaves unless tile ≥ 256 or stat-only |
| Whisper-medium | 6.4e11 | 25.6 M with logits; stat-only at 256: 3.0 M | step leaves; stat-only needed |
| Whisper-large-v3, 30 s | 1.29e12 + decode (≤ 224 tokens × 0.92e9) | K/V 123 M lanes (246 MB), ~5 GB hashed; 42.7 M leaves with logits, stat-only at 512: 2.5 M | `max_position_macs`, leaves; fix: stat-only + a 2–4 stage split |
| wav2vec2-base, 10 s / large | 7.4e10 / 1.9e11 | conv0 output 16.4 M lanes; 2.5–3.1 M / 5.0–6.5 M leaves | none / tile ≥ 128; base at 30 s: 7.6–12.6 M, needs tile ≥ 128 or stat-only |
| SpeechT5 + HiFi-GAN, 10 s | 1.1e11 (vocoder 76 %) | ~365 M lanes (1.5 GB) | none, if the vocoder is stage-split |
| MusicGen-small, 30 s | 1.2e12 (4.0e8 × 1,503 × 2 for CFG) + codec 1.2e11 | K/V history 295 MB; 8.3 K leaves per step, ~25 M per job (cap 2^32); ~6 GB hashed | none per position |
| EnCodec 24 kHz decode, 30 s | 4.0e10 | LSTM 9.4e9; RVQ encode 2.4e9 (8 books) to 9.4e9 (32) | none |

One disputed tile (box bounds): Whisper-large attention row-max or ctx tile 6.2 M MACs, 96 K exps, ~0.8 MB carried; fc2 at 128 lanes 0.66 M MACs, 0.66 MB; conv2 tile 0.5 MB; the log-mel global-max cone 384 K lanes (1.5 MB); a HiFi-GAN conv tile ≤ 0.2 M MACs; LSTM replay at `d = 1,024` is 16.8 M MACs, exactly 2^24, so the checkpoint interval is 1. Attention tiles must stay ≤ 128 lanes.

**Generic vocabulary** (W = Whisper-like, C = CTC encoder, M = codec-LM; "existing" means in the lowerer today): `INPUT_AUDIO_PCM_V1` (protocol: rate, frames, channels, tile_len — W C); `OUTPUT_AUDIO_PCM_V1` / `OUTPUT_TENSOR_V1` (protocol — M); `FRONTEND_STFT_MEL_V1` (n_fft, win, window {hann, hamming, povey, rect}, hop, center {reflect, zero, none}, preemph, dc_remove, power, drop_last, mel {n, fmin, fmax, scale, norm}, log {base, floor | add_eps}, clamp, post {affine, mean_std, cmvn}, stack — W; HF keys `feature_size`, `n_fft`, `hop_length`, `chunk_length`, `preemphasis`, `mel_floor`, `do_normalize`, `mean`, `std`; refuse `dither ≠ 0`);
`CONV_1D_V1` (cin, cout, k, stride, dilation, groups, pad {zero(l,r), reflect(l,r), causal, same}, bias, transposed + trim, fold {weight_norm, batch_norm}, act {gelu, elu, leaky(slope), tanh, snake} — W C M); `NORM_STAT_TIME_V1` (group / instance / cmvn, eps, chunk; committed i32-limb partials — C); position variants (sinusoid layouts, stored tables, conv embedding {k, groups}, Shaw `rel_key`); `ATTN_CROSS_V1` (FR-21 — W M);
`ATTN_STAT_COMMIT_V1` and `STAGE_SPLIT_V1` (automatic from ceilings — W); `MIXER_LSTM_V1` (M, codec); `QUANT_RVQ_V1` (n_q, size, dim, euclid, bandwidth → n_q — M); `GEN_CODEC_LM_V1` (K, delay, pad id, embed sum, K heads, sampler {gumbel, top_k, temperature}, cfg — M); `PLACEMENT_AUDIO_ROWS_V1` (placeholder id, rows per clip, hf-coverage §16 cursor — audio-LLMs).
**Adapter-language changes**: the adapter reads `config.json` only and `generation_config.json` is never read — add `sources` for `preprocessor_config.json` and `generation_config.json` (operators `$pre`, `$gen`; `$scope`, `$map`, `$range` already cover MusicGen's nested configs and the `conv_*` lists); FR-01 needs two arithmetic folds in `weights::Src` (`weight_norm(g, v, dim)` and `batch_norm`) and LSTM gate slices;
the registry is stale — `ATTN_CROSS_V1` is listed Missing although `encdec.rs` implements it, there is no sinusoid or conv feature, and `Area` lacks Input, Frontend, Conv and Codec. Closest readers: Whisper-like `lower/encdec.rs` (decoder, two-stage pipeline) + the `lower/vision.rs` tower pattern and `encoder::vision_pipeline()` as the template for the `JobAudio` stage; wav2vec2/HuBERT `lower/bidir.rs` behind a `vision.rs`-style conv prologue; codec-LM `encdec.rs` decoder + `encoder::lift_v2`; the LSTM, codec and vocoder stages have no reader (`library::recur` state patterns are nearest; a new `lower/audio.rs`).

**Ranked by families needing it** (of 59 audio-related directories in transformers 5.17, out of 518; the bracketed figure excludes 10 families with another blocker — `sew_d` disentangled attention, `clap` Swin/HTSAT, `pe_audio_video`, `gemma3n` AltUp, `nemotron_asr_streaming` and `nemotron3_5_asr` transducer/streaming, `csm`, `moshi`, `vibevoice` nested decode loops, `bark` EOS-terminated three stages):
`CONV_1D_V1` 55 (46); `INPUT_AUDIO_PCM_V1` 49 (42); `FRONTEND_STFT_MEL_V1` 27 (23); `OUTPUT_AUDIO_PCM_V1` 21 (17); `BLOCK_CONFORMER_V1` 15 (12); `QUANT_RVQ_V1` 13 (10); `PLACEMENT_AUDIO_ROWS_V1` 12 (11); `NORM_STAT_TIME_V1` 10 (9); `GEN_CODEC_LM_V1` 9 (5); `ATTN_STAT_COMMIT` + stage split 8 (8); `CONV_2D_V1` 8 (4); `ACT_SNAKE` 8 (7); length regulator and flows 4; `MIXER_LSTM_V1` 4 (3); relative-position variants 2.
**Build order, cumulative families:** (1) protocol, front end, conv, stat-commit, placement, group stats → 20 (Whisper, the audio-LLM towers, Speech2Text, data2vec and the wav2vec2 cluster, Moonshine); (2) output, snake, RVQ, LSTM, codec-LM → 35; (3) conformer and conv2d → 45; (4) length regulator and flows → 49.
**Acceptance**: the five fixtures at all eight harness stages, plus the integer log-mel against `WhisperFeatureExtractor`, and MusicGen compared by teacher-forced logits rather than samples. **Not verified**: no front-end error was measured (expected ~1e-4 to 1e-3 from the Q14 DFT table and the 9-bit shift); the close-sizing CPU cap (2^26 steps) is not estimated (assumed a range twin under `palw_tir_fence2`); calibration needs an audio set; CTC models are known to be sensitive to quantisation; the HF reference is ambiguous in places (torch vs numpy front ends, SpeechT5 dropout, the HiFi-GAN slope quirk); the registrable share by repo count is a guess (ASR 36.8 k and TTS 8.0 k repos on 09-28, ~1.4 % of HF; downloads are far higher).

## Named in the registry already

### FR-20 `ATTN_PREFIX_LM_V1` (M, unlocks `paligemma`; also Gemma-3/4 image blocks, `gemma4_unified`, `pi0`, `git`, `deepseek_ocr2`, `diffusion_gemma`, `hrm_text`)

`modeling_paligemma.py:PaliGemmaModel.forward`: rope positions are 1-indexed (`position_ids + 1`); image rows are the SigLIP `last_hidden_state` through `multi_modal_projector.linear` (a plain Linear with bias) scattered into `inputs_embeds`,
**not** scaled by `√d` (only token lookups are, by `GemmaTextScaledWordEmbedding`); when `token_type_ids` is given and it is the first iteration (prefill), `block_sequence_ids = where(token_type_ids == 0, 0, −1)` and
`blockwise_overlay` ORs "same block id ≥ 0" into the causal mask (for Gemma-2 sliding layers the OR is applied *after* the window AND, so the prefix ignores the window); decode steps use plain causal. The config forces
`use_bidirectional_attention = True` but eager ignores it; the processor always returns `token_type_ids`, raw `input_ids` give a causal model. **The class semantics must be pinned** (prompt = prefix, generated by `generate` with
`token_type_ids`); per-position HF replays (re-running prefixes) would not equal prefill-then-decode.
**It is not expressible in a causal scan**: prefix K/V at layer `l ≥ 1` depend on *later* prefix tokens through the layers below, and one stage per layer exceeds the 16-stage cap. It *is* expressible as a **pipeline with no primitive**
(the encoder-decoder structure with the decoder's own layers as the "encoder"): stage 0 the vision tower (`Final [R, d]`, bound by `JobImage`); stage 1 *prefix* — one position over the padded prompt axis
`[Lp, d]` (inputs `JobTokens`, `JobTokenCount`, `StageFinal{0}`), running the decoder layers bidirectionally with `Compare(Iota < count)` masks and the window-ignored-in-prefix rule, `Final` = post-rope K and V of every layer
`[layers, 2, Lp, kv·hd]`; stage 2 `TextStream` binding `StageFinal{1}`, where at prompt positions the K/V `HistAppend` rows are `Select(pos < n_prompt, Gather(xkv[l], pos), own_row)` (the existing single-`H` attention library is
unchanged; the last prompt position's own computation equals the bidirectional one by induction).
**Cost**: stage 1 is a full-model bidirectional forward in one position — PaliGemma-3B at `Lp ≈ 280` is ≈ 5.6e11 MACs, between the 2^37 provisional `palw_tir_v1` ceiling and the legacy court's 2^40, so it depends on the armed ceiling; the K/V `Final` is
18·2·280·256 = 2.6 M lanes; the text stage recomputes the prompt positions causally (another prefill) unless a small general capability is added — a `TextStream` start offset (or a Hist seeded from a bound `Final`).
**Spec**: `ModelSpec.prefix_lm: Option<PrefixLmSpec{window_ignored_in_prefix: bool}>` defined with a block-id mask so it also covers image blocks, a rope position offset of 1 (additive), a SigLIP `last_hidden_state` tower variant (no head; `vision_tower.head.*` unused).
Tensors: `language_model.model.layers.L.self_attn.{q,k,v,o}_proj.weight`, `…mlp.{gate,up,down}_proj`, `{input,post_attention}_layernorm`, `multi_modal_projector.linear.{weight [32,16], bias [32]}`.
**Acceptance**: `paligemma`. Today `refusals.json` refuses `PaliGemmaForConditionalGeneration` on this feature. *Evidence: analysis.*

### FR-21 `ATTN_CROSS_V1` (L, unlocks `mllama`; the same mechanism is the cross-attention of FR-18 and of `idefics`, `t5gemma`, `blt`, `dia`, `canary`/`cohere_asr`, `evolla`, `udop`)

`modeling_mllama.py`: `MllamaTextModel.forward` skips a cross layer entirely when `cross_attention_states is None` and the cache is empty, so text-only mllama is a Llama with the cross layers deleted (the embedding has `vocab + 8` rows).
`MllamaCrossAttentionDecoderLayer.forward`: `x += tanh(g_attn)·cross(RMS(x))`; `x += tanh(g_mlp)·(rowmask·MLP(RMS(x)))` with `g_attn`, `g_mlp` learned scalars. `MllamaTextCrossAttention.forward`: `q = RMS_head(q_proj x)`,
`k = RMS_head(k_proj cs)`, `v = v_proj cs`, **no rope**, GQA, scale `head_dim^−½`; `cs` is the projected vision states, K/V cached at prefill. **Row-mask quirk**: `_prepare_cross_attention_mask` sets fully masked rows to mask 0, so those rows see *all*
vision tokens; only the MLP branch is zeroed (`full_text_row_masked_out_mask`), the attention branch is not — a text token before the first `<|image|>` still cross-attends to the whole image. Vision side: a tile-based ViT, 1,601 patches per 560×560 tile,
up to 4 tiles, aspect-ratio embeddings, local plus global encoder, `intermediate_layers` features concatenated, then `multi_modal_projector` (Linear with bias).
**What exists generalises as a pattern, not as code**: stage 0 emits every decoder layer's cross K/V as one `Final i16 [D, 2, L, inner]` (one stacked `MatMul`, narrowed per channel); the decoder binds it by `StageFinal`; each layer picks its slice by a per-layer
`idx` param through `Gather`; attention runs over the `Fixed` source axis with keys masked by count to `i32::MIN`; `softmax_committed`/`split_softmax` handle large Fixed axes; `External` inputs are constant over the scan, which is "a second history".
The code is a dedicated T5/BART path (`lower/encdec.rs`), so a generic HL cross op is required.
**Spec** (additive): `Mixer::CrossAttention(CrossAttnSpec{heads, kv_heads, head_dim, q_norm, k_norm})`; per-branch residual scales (`tanh(gate)`, tensor-derived at conversion; `Residual::Sequential{multiplier}` is a single multiplier); `layer_pattern` from `cross_attention_layers`.
Tensors (fixture): `language_model.model.layers.1.cross_attn.{q,k,v,o}_proj.weight`, `cross_attn.{q,k}_norm.weight [8]`, `cross_attn_attn_gate [1]`, `cross_attn_mlp_gate [1]`, `multi_modal_projector.{weight, bias}`.
**Decomposition**: stage 0 = vision tower + projector + stacked per-cross-layer `k_proj`, `RMS_head k_norm`, `v_proj` → `Final [Dc, 2, Nvis, inner]` (`Dc` = number of cross layers, 8 in 11B; ≈ 8·2·6,404·1,024 = 105 M elements < 2^28); tile count and aspect ratio fixed per class (`JobImage` allows one size); text-stage
cross layer = `q` + `RMS_head`, `Gather` the layer's `[2, Nvis, inner]` slice, masked Fixed softmax; the mask is a cursor state counting `<|image|>` tokens (tokens before the first see all tiles with the MLP branch zeroed).
**Cost** (11B: 32 heads, 8 kv heads, head_dim 128): a cross layer is ≈ 52 M MACs per position at `Nvis = 1,601` (210 M at 6,404); committed logits `[32, Nvis]` = 51 K lanes (205 K); a Fixed-axis softmax is not dissected, with `tile_len ≤ 1,024` for logits and ≤ 32 for ctx a tile opens ≈ 0.5 MB; a
one-head `tile_len = Nvis` logits tile at `Nvis = 6,404` opens ≈ 3.3 MB, over the 3.2 MB cap. No primitive (the second history is a constant `External` input).
**Acceptance**: `mllama` (text stage first: the cross layers are skipped when no image is bound, which is Level B today as a Llama), then with an image. *Evidence: analysis.*

### FR-28 `EMBED_DEEPSTACK_V1` (S–M, **not blocking**: the text stage of `qwen3_vl` is Level B; images need it plus the vision tower, which is RFC-0003's lane)

`modeling_qwen3_vl.py`: `Qwen3VLTextModel.forward`/`_deepstack_process` — for each layer index `i < K = len(deepstack_visual_embeds)`, *after* layer `i`'s full output, `h[visual_pos] += deepstack_embeds[i]` (rows in placeholder order). The tower returns the final merged features
plus `K` deepstack features, each `deepstack_merger_list[i]` (LayerNorm over the merged `4·hidden`, "postshuffle") applied to the hidden states of ViT block `deepstack_visual_indexes[i]`. M-RoPE is the interleaved layout (`j % 3 == 1` takes H while `j < 3·mrope_section[1]`, `j % 3 == 2` takes W,
the rest T; the config key `mrope_interleaved` is never read; `rope::MRope` has it, unit-tested against HF's index map, no VLM fixture yet). `get_rope_index` places image token `i` at `(s, s + i//W, s + i%W)`, then `current_pos += max(H, W)//merge`. **Spec**: `ImageRows.deepstack:
Option<DeepStack{layers: K, units: Vec<f64>}>`; the text stage needs `K` extra row tables (one `External` input `[K, R, d]`, each with its own unit and proven interval, narrowed to the residual scale), the first `K` layer blocks end with `x_out + Select(is_img_layer, narrow(Gather(ds[l], cursor_layer)), 0)`
(per-layer cursors, reusing the M-RoPE mechanism), and the tower (LayerNorm blocks, fused qkv, axial 2-D rope, pinned interpolation tables for a fixed canvas, Conv3d patch embed with `temporal_patch_size` 2, plus the deepstack mergers). Cost: one `Gather` of a `d`-lane row per deepstack layer. No primitive.
Also needed by `qwen3_vl_moe`, `qwen3_omni_moe`, `granite4_vision`, `cosmos3_omni`, `cohere_compass`. **Acceptance**: `qwen3_vl` with an image bound (RFC-0003 interface).

## Infrastructure and honesty

### FR-25 user adapters may override built-in refusals; remove the stale Granite-hybrid refusal (S)

`hf_schema::read_model` consults `builtin::refusal_for(arch)` **before** choosing any adapter, so an adapter a user supplies
can never override an entry of `adapters/refusals.json`. The entry for `GraniteMoeHybridForCausalLM` says the layer pattern is
not mapped (`MIXER_LAYER_PATTERN_HYBRID_V1`) — but the third-party adapter
`tools/corpus/adapters/granitemoehybrid.json` expresses it (a Mamba-2 or NoPE-attention mixer per `layer_types`, a
Granite-MoE FFN with a shared MLP, Granite's multipliers) and passes every stage on a patched checkpoint (FR-01). Request:
a `refuse` entry applies only when no adapter, built-in or user, claims the architecture; delete the stale entry. The harness
bypasses the refusal for such adapters by renaming the architecture in memory and records `refusal_bypassed_for_user_adapter`.
**Second instance (the census):** `MiniCPM3ForCausalLM` is refused by name ("MLA + muP remote code is not modelled yet"), yet the native transformers 5 class
is DeepSeek-V2's MLA with the plain rotation, a dense MLP and muP scalings — all existing features. `tools/corpus/census-adapters/minicpm3.json`
(25 lines, extends the built-in `deepseek-v3`) passes every stage, court included, once the refusal is bypassed. The refusal is stale for the same
reason: it describes the remote-code model, not the class transformers 5.17 ships (which also moved the head scaling into `config.logits_scaling`).

### FR-26 lowerer robustness and Level A honesty (S)

1. **A panic on a legal spec.** `lower::bidir::lower_bidir` panics `Add operands do not broadcast` when
   `spec.embedding.dim != hidden_size` with `proj_in` (ALBERT's factorised embedding: `tools/corpus/adapters/albert.json`).
   Every refusal must be `NOT_LOWERABLE`, never a panic.
2. **A flag the lowerer drops** (FR-07's guard): refuse, name the flag.
3. **Level A is unconfirmed.** The standard template reads a class no adapter claims without any refusal, and three of the
   twelve such classes in the corpus were misread (`ArceeForCausalLM`: gated MLP assumed; `HeliumForCausalLM`: interleaved
   rotary pairs assumed half-split — a 28 % logit error with *no* error anywhere in the pipeline; `BitNetForCausalLM`: four
   sub-norm tensors per layer pair unread). `palw-class check-architecture` should (a) when a tensor index is given, run the
   binder on names and shapes and **refuse if any tensor is unread** (the check `bind` does in the harness; it is what caught
   Arcee and BitNet), and (b) print "Level A: UNCONFIRMED — rope pairing, norm placement and MLP gating are class code, not
   configuration" unless a fixture check passed.

### FR-29 `HEAD_TRANSFORM_V1` — a prediction head before the vocabulary projection (S, found by the census: `modernbert_decoder` and the `ForCausalLM` mode of the whole BERT lineage)

`modeling_modernbert_decoder.py:ModernBertDecoderPredictionHead`: `h → LayerNorm(act(dense(h)))`, then the tied vocabulary projection plus a `decoder.bias [V]` — the same shape as BERT's `cls.predictions.transform` (`dense → gelu → LayerNorm`) and `decoder.bias`. In the fixture the head owns
`lm_head.dense.weight`, `lm_head.norm.weight`, `decoder.bias` and the tied embedding. **Spec**: `HeadSpec.transform: Option<HeadTransformSpec{bias, act, norm}>` and `HeadSpec.bias`; **decomposition**: `Lin` → the activation table → the norm template (`layer_norm_exact`), then the existing tied head with an `Add` of the bias; no primitive, no new commit-point kind. The rest of
`modernbert_decoder` is readable by data once the per-layer-type rope convention of FR-27's neighbours exists in the template (measured: `standard-v3` fixes the rope refusal and the family then stops at its own keys `norm_eps`, `hidden_activation`, `local_attention`, `classifier_*`, which an adapter maps) — layer 0 has no `attn_norm` (`Identity`), the Q/K/V are separate (not ModernBERT's fused `Wqkv`), the GeGLU `Wi` rows are `[input|gate]` (`FusedGateFirst`).
The same head is what blocks **eleven more census families** read as decoders — `camembert`, `data2vec_text`, `electra` (`generator_predictions`), `ernie`, `megatron_bert`, `rembert`, `roberta_prelayernorm`, `roc_bert`, `roformer`, `xlm_roberta_xl`, `big_bird` — whose fixtures all carry `lm_head.dense` + `lm_head.layer_norm` + a bias (RoBERTa spelling) or `cls.predictions.transform.{dense,LayerNorm}` + `cls.predictions.bias` (BERT spelling), and which are otherwise BERT blocks run with a causal mask: it is the lever for the lineage's decoder mode and, for encoders, for MLM logits. They are hardly served as decoders on the hub, so it is a small lever by usage and a large one by family count.
**Acceptance**: `modernbert_decoder` (census), then one of the BERT-lineage rows. *Evidence: read from the transformers code and the census fixtures' tensor names (`tools/corpus/census-specs/*/tensors.json`); no adapter run.*

### FR-30 `ATTN_OUTPUT_GATE_SEPARATE_V1` — an attention output gate from its own tensor, with a choice of activation and granularity (S–M, census: `afmoe`, `laguna`)

Today `AttnSpec.output_gate: bool` means Qwen3-Next's layout only: `q_proj` emits `[q, gate]` per head and the output is multiplied by `σ(gate)` per element (`hf_weights.rs:302-322` slices the gate out of the fused `q_proj` rows into `attn.gate.w`). Two census families gate the attention output from a **separate** projection:
`afmoe` — `output *= sigmoid(gate_proj(x))` with `gate_proj [H·hd, D]` per element (`modeling_afmoe.py:AfmoeAttention.forward`); `laguna` — `output *= softplus(g_proj(x))` with `g_proj [H, D]` **per head** (a scalar per head; `[H·hd, D]` per element when `gating` is false), `num_attention_heads_per_layer` differing per layer, per-layer-type partial rotary, a router with logit
soft-capping and a shared expert (`modeling_laguna.py:LagunaAttention.forward`). **Spec**: `AttnSpec.output_gate: Option<OutputGateSpec{source: FusedQ | Separate, act: Sigmoid | Softplus, per: Element | Head}>` (the existing `bool` stays as `FusedQ, Sigmoid, Element`, additive); the binder accepts the role `attn.gate` as a named tensor when `source = Separate`. **Decomposition**: `Lin` (committed) → the activation table → `Mul` broadcast over the head dim; no primitive, no new commit-point kind.
Per-layer head counts are already data (`layers[i]` is fully expanded: the block kinds group equal layers, as Gemma-4's wider global heads do). **Acceptance**: `afmoe`, then `laguna`. *Evidence: read from the transformers code and `hf_weights.rs`; no adapter run (quiet window).*

### FR-31 `ATTN_VALUE_SCALE_V1` — a constant on the values (S, census: `mimo_v2_flash`)

`modeling_mimo_v2_flash.py:MiMoV2FlashAttention.forward`: `value_states = v_proj(x)·v_scale` with `v_scale = attention_value_scale` (0.707 in the release; 1.0 when absent), before the cache. The scale cannot move into a weight at registration (the weights are bound raw from the checkpoint). **Spec**: `AttnSpec.v_scale: f64` (default 1.0, additive). **Decomposition**: free — the constant joins the V narrowing's multiplier `m` (the same mechanism as LayerScale and BatchNorm folds in FR-19); no node, no primitive. The rest of the family is data: the sliding layers use `2·num_key_value_heads` kv heads (a per-layer `kv_heads` in `l_mixer`), sinks exist on the sliding layers only (under the hub name `attention_sink_bias`), partial rotary 0.334 per layer type, a sigmoid router with `e_score_correction_bias`.
**Acceptance**: `mimo_v2_flash`. *Evidence (run): the adapter refuses the real config by its own check naming FR-31; `mimo_v2_flash_unscaled` (the scale removed) is Level B with every stage including the court, per-layer kv heads, sinks on the sliding layers and per-layer-type partial rotary included (a `param_prefix` for the layers whose projections differ in shape).*

### FR-32 `MIXER_LIGHTNING_V1` — linear attention with a constant per-head decay (M, census: `minimax`; MiniMax-01 and its hybrids)

`modeling_minimax.py:MiniMaxLightningAttention.forward`, decode form (the block-wise prefill form is the same function): `qkv = SiLU(qkv_proj x)` split per head `[q|k|v]` (`hd` each, *the SiLU is applied to the projection output*); per head `S ← λ_h·S + kᵀv` with `S [hd, hd]`, `λ_h = exp(−slope_h)`, `slope_h = base^(h+1)·(1 − layer/(L−1+1e-5) + 1e-5)`, `base = 2^(−8/H)` (a registration-time constant per layer and head); `o = q·S`; `RMSNorm` over **all** heads (weight `[H·hd]`); `o ← σ(output_gate x)·o` (a separate `[H·hd, D]` gate projection, sigmoid, per element — the FR-30 gate again); `out_proj`. Other layers of the hybrid are ordinary softmax attention (`layer_types`) with a softmax-top-k MoE. **It is Mamba-2's state-space update with a constant decay and no conv, no `D` skip**:
the existing `Mamba2Spec` has the state, the decay multiply and the outer-product update, but its decay is the data-dependent `exp(dt·A)`. **Spec**: `Mixer::Lightning(LightningSpec{heads, head_dim, qkv_act, slope_base, layer_factor})`; HL: reuse the `Mamba2` recurrence with a per-head constant `Q24` decay param instead of the `dt` branch, an `Op::Act` on the qkv rows, the existing grouped RMS norm (`groups = 1` over `H·hd`) and the FR-30 output gate. **Decomposition**: `Lin` + table SiLU (commit) → `Mul(S, λ_h)` + `Mul(k[:,None], v[None,:])` + `Add` (state write; the state is `H·hd²` — 64·128·128 = 1 M entries for MiniMax-01, twice Qwen3-Next's 524 K, so it is a checkpoint-interval question) → `MatMul(q, S)` → norm (`rms_norm_wide`) → `Mul` gate → `Lin`. No primitive. **Acceptance**: `minimax` (with FR-30's separate gate). *Evidence: read from the transformers code; no adapter run (quiet window).*

### FR-33 two gaps in the quant-format registry: announcement by roles or by a config sub-key, and the weight's shape from the model (S–M, found by writing third-party descriptors; `tools/corpus/formats/`)

A format is a descriptor file (`misaka.palw.quant-format.v1`); the third-party axis of lane H wrote two the pack lacks — `MLX_AFFINE` (`weight` uint32 `[out, in·bits/32]` with 32/bits codes per word, first element in the lowest bits; `scales`, `biases` `[out, in/group]`; `W = scale·q + bias`, expressed as `scale = scales[o,g]`, `zero = 0`, `min = −biases[o,g]`) and `BNB_INT8` (`weight` int8 `[out, in]`, `SCB` float32 `[out]`; `W = weight·SCB/127`) — and found two limits that no descriptor can work around:
**(a) Announcement.** `QuantRegistry::config(method, format)` selects a descriptor by `quantization_config.quant_method` (with an optional `format`). MLX's `config.json` carries `quantization: {group_size, bits}` (and the same under `quantization_config`) **with no `quant_method`**, so the reader says `quant_method=unknown has no quant-format descriptor ... supply it` — a refusal that is wrong, because supplying the descriptor does not help (test `fr33_mlx_announces_itself_without_a_quant_method…`). bitsandbytes uses one `quant_method` for 8-bit and 4-bit and tells them apart by `load_in_8bit` / `load_in_4bit`, so two descriptors cannot both own `bitsandbytes`; `BNB_INT8` claims it and refuses 4-bit by a config check. **Request**: `ids` entries `{scheme: "roles", suffixes: [".scales", ".biases"]}` (a checkpoint whose linear modules carry exactly those roles announces the format) and `{scheme: "config", method, when: {path, one_of}}` (several descriptors may share a method when their `when` clauses differ). Both are fields of the descriptor; the registry change is generic.
**(b) Shape.** A `tensors` descriptor reads the weight's `[out, in]` only from its role tensors' shapes (`dims`). bitsandbytes 4-bit (nf4, fp4, with or without double quantisation) stores the weight as a flat `uint8 [N/2, 1]` and the real shape **only in a JSON dictionary stored as a uint8 tensor** (`weight.quant_state.bitsandbytes__nf4`); HQQ keeps it in a pickled `meta`. **Request**: `dims` may read `spec_out` and `spec_in`, the shape the model's own spec gives the linear being bound (the binder knows it). NF4 is then data: with `e = o·spec_in + i`, `q = (weight[e/2, 0] >> (4·(1 − e%2))) & 15` (the first element in the high nibble), `value = nf4[q]·absmax[e/64]` with the 16-entry code table in `tables`, and double quantisation as two more roles (`absmax` uint8, `nested_absmax` float32 per 256 blocks, `nested_quant_map` float32[256], an `nested_offset`).
**Not written, by reading only**: EXL2/EXL3 (variable bit widths per group and row permutations: plausible with indexed reads, not attempted), AQLM (codebook sums; plausible for at most two codebooks, not attempted), Marlin repacks (a fixed permutation of GPTQ), Quanto and torchao (tensor subclasses with metadata).
**Honesty about the evidence**: no reference library (mlx, bitsandbytes) is available offline, so the vectors are a numpy transcription of each layout as recalled from its documentation and source — they prove the descriptor says what the script says, not that the script says what the library does. **Acceptance**: `mlx_affine.json` selected from an unmodified mlx-community `config.json`; an NF4 descriptor that loads and passes its vectors. *Evidence (run): both descriptors load and pass their vectors, the registry reads `mlx` and `bitsandbytes` and refuses 4-bit by name, and the MLX configuration without a `quant_method` is refused as `quant_method=unknown` (`tests/corpus_formats.rs`).*

### FR-34 `EMBED_SCALE_TOKENS_ONLY_V1` — the embedding scale on the token embedding alone (S, census: `biogpt`, `xglm`; fairseq-lineage models)

`hl/build.rs:pre_block` builds `x = embed(token)`, then `x += pos_embed(pos)`, then `x *= embedding.scale`, then the optional embedding norm: the scale multiplies the **sum**. BioGPT (`BioGptScaledWordEmbedding`, `embed_scale = √hidden` when `scale_embedding`), XGLM, M2M-100, Speech2Text and the other fairseq-lineage decoders multiply the **token embedding only** and add the positions afterwards (`tok·scale + pos`). Gemma, Granite and MiniCPM have no learned positions, so the order never mattered until a model has both. **Spec**: `EmbeddingSpec.scale_scope: Sum | Tokens` (additive, default `Sum`); **decomposition**: move the `Op::Scale` before the position add — the same nodes in another order, no primitive. **Acceptance**: `biogpt` (the census variant `biogpt_unscaled` shows the rest of the family is data). *Evidence (run): the adapter stops at float_vs_hf (0.43 of the logit scale); `biogpt_unscaled` is Level B with every stage including the court.*

### FR-35 `ROPE_NEGATED_V1` — a rotation by −θ (S, census: `nanochat`)

`modeling_nanochat.py:rotate_half` returns `cat(x2, −x1)` ("flipped signs for NanoChat"), so the rotation is `(x1·cos + x2·sin, x2·cos − x1·sin)`: the angle is **−θ**. Both q and k are rotated, so the scores depend on `sin` with the opposite sign — a different function from the standard rotation, not something a weight can absorb. `RopeSpec` has `style: Half | Interleaved` and an explicit frequency table, but no sign; a negative frequency cannot be written as data (`$rope`/`$rope_plain` build `1/θ^(2i/d)`). **Spec**: `RopeSpec.sign: i8` (`+1` default, additive) or a style `HalfNegated`; **decomposition**: the same `Mul`/`Add` nodes with `sin` negated in the table — no primitive. **Evidence (run)**: with FR-02 in (tir/generic), the third-party adapter `nanochat.json` still stops at float_vs_hf (0.48 of the logit scale); the census variant `nanochat_std_rope` — the same family with the standard `rotate_half` patched into the *reference* — is Level B with every stage including the court, so the sign is the only gap. **Acceptance**: `nanochat`.

### FR-24 `REFERENCE_REMOTE_CODE_V1` — a way to pin a remote-code reference (policy; `chatglm3`)

ChatGLM3's forward lives in `modeling_chatglm.py` of the repository, not in transformers. No data can express a model
whose semantics are not pinned to a library version. A mechanism the chain could accept: the registrant supplies the hash
of the modeling file and a tiny fixture produced by it; the adapter declares `reference: {remote_code: <hash>}`; the
lowering is checked against that fixture. Whether that is acceptable is a policy decision, not an engineering one.

<!-- BEGIN GENERATED: corpus -->
| # | id | category | `architectures[0]` | usage | share | why it is in the corpus |
| ---: | --- | --- | --- | :-: | ---: | --- |
| 1 | `llama` | text/dense | `LlamaForCausalLM` | vh | 30 % | the lineage a third of text-generation repositories derive from (Llama 1-3.3, TinyLlama, Vicuna, Yi, DeepSeek-LLM, SmolLM, CodeLlama) |
| 2 | `qwen2` | text/dense | `Qwen2ForCausalLM` | vh | 20 % | q/k/v bias, tied head: the second-largest family (Qwen1.5/2/2.5, QwQ, R1-Distill-Qwen) |
| 3 | `mistral` | text/dense | `MistralForCausalLM` | vh | 8 % | sliding-window GQA (Mistral, Zephyr, OpenHermes, Nemo) |
| 4 | `gpt2` | text/dense | `GPT2LMHeadModel` | vh | 7 % | learned positions, Conv1D weights, fused c_attn: the pre-Llama lineage's head |
| 5 | `qwen3` | text/dense | `Qwen3ForCausalLM` | vh | 6 % | per-head QK-norm (the dense Qwen of 2025) |
| 6 | `gemma2` | text/dense | `Gemma2ForCausalLM` | h | 2 % | sandwich norms, score and logit soft-caps, alternating windows |
| 7 | `gemma3_text` | text/dense | `Gemma3ForCausalLM` | vh | 3 % | 5:1 sliding/global, two rope tables, QK-norm (text of Gemma-3 1B and the VLM) |
| 8 | `gemma4_text` | text/dense | `Gemma4ForCausalLM` | h | unknown | 2026 flagship dense+MoE: per-layer inputs, K=V globals, KV sharing in the E models |
| 9 | `phi3` | text/dense | `Phi3ForCausalLM` | h | 2 % | fused qkv and gate_up, LongRoPE (the 128k variants): per-position rope semantics |
| 10 | `gpt_neox` | text/dense | `GPTNeoXForCausalLM` | h | 2 % | parallel residual with two LayerNorms, partial rotary, per-head fused qkv (Pythia, Dolly, RedPajama) |
| 11 | `opt` | text/dense | `OPTForCausalLM` | m | 1.5 % | learned positions at pos+2, ReLU MLP, (OPT-350m) post-LN and a projected embedding |
| 12 | `falcon` | text/dense | `FalconForCausalLM` | m | 1 % | kv-group fused qkv, parallel attention, ALiBi in the RW variants |
| 13 | `starcoder2` | text/dense | `Starcoder2ForCausalLM` | m | 0.5 % | LayerNorm + biases, plain GELU MLP, window (code models) |
| 14 | `cohere2` | text/dense | `Cohere2ForCausalLM` | m | 0.3 % | parallel residual from ONE bias-free LayerNorm, interleaved rope on sliding layers only |
| 15 | `granite` | text/dense | `GraniteForCausalLM` | m | 0.5 % | muP-style multipliers on embedding, branches, attention and logits |
| 16 | `glm4` | text/dense | `Glm4ForCausalLM` | m | 0.3 % | fused gate_up, partial interleaved rope, post-attention/post-MLP norms (GLM-4-0414, Z1) |
| 17 | `smollm3` | text/dense | `SmolLM3ForCausalLM` | m | 0.3 % | NoPE every n-th layer |
| 18 | `arcee` | text/dense | `ArceeForCausalLM` | l | unknown | Llama with a non-gated ReLU² MLP (AFM-4.5B) |
| 19 | `apertus` | text/dense | `ApertusForCausalLM` | l | unknown | xIELU activation with learned per-layer parameters, QK-norm, llama3 rope (Swiss AI) |
| 20 | `helium` | text/dense | `HeliumForCausalLM` | l | unknown | Llama-shaped (Kyutai) with its own norm epsilon: a Level A control |
| 21 | `hunyuan_v1_dense` | text/dense | `HunYuanDenseV1ForCausalLM` | m | unknown | Llama + QK-norm placed after the rotation (Hunyuan-7B/4B/1.8B) |
| 22 | `seed_oss` | text/dense | `SeedOssForCausalLM` | l | unknown | Llama with q/k/v bias and a bias-free output projection (ByteDance Seed-OSS) |
| 23 | `ernie4_5` | text/dense | `Ernie4_5ForCausalLM` | m | unknown | Llama with GLM-style interleaved rotary pairs (ERNIE 4.5 dense) |
| 24 | `bitnet` | text/dense | `BitNetForCausalLM` | l | unknown | ternary weights with per-token int8 activation quantisation and sub-layer norms (BitNet b1.58) |
| 25 | `persimmon` | text/dense | `PersimmonForCausalLM` | l | unknown | LayerNorm, QK layer-norm, ReLU², partial rotary, per-head fused qkv (Adept) |
| 26 | `diffllama` | text/dense | `DiffLlamaForCausalLM` | l | unknown | differential attention: the difference of two softmax maps |
| 27 | `cwm` | text/dense | `CwmForCausalLM` | l | unknown | Llama with per-layer sliding windows and llama3 rope (Code World Model) |
| 28 | `gemma3n_text` | text/dense | `Gemma3nForCausalLM` | h | unknown | AltUp streams, LAuReL, per-layer embeddings, activation sparsity, KV sharing (Gemma-3n E2B/E4B) |
| 29 | `mixtral` | text/moe | `MixtralForCausalLM` | h | 1 % | softmax top-2 of 8 experts, renormalised |
| 30 | `qwen2_moe` | text/moe | `Qwen2MoeForCausalLM` | m | 0.5 % | sigmoid-gated shared expert, dense first layer |
| 31 | `qwen3_moe` | text/moe | `Qwen3MoeForCausalLM` | h | 0.5 % | QK-norm + renormalised top-k, no shared expert (Qwen3-30B-A3B, 235B) |
| 32 | `granitemoe` | text/moe | `GraniteMoeForCausalLM` | l | unknown | top-k of logits then softmax; fused expert tensors |
| 33 | `deepseek_v3` | text/moe | `DeepseekV3ForCausalLM` | h | 0.5 % | MLA + sigmoid routing with selection bias and group-limited top-k (DeepSeek-V3/R1, Kimi-K2) |
| 34 | `gpt_oss` | text/moe | `GptOssForCausalLM` | vh | unknown | attention sinks, clamped SwiGLU, interleaved fused experts, YaRN without truncation |
| 35 | `llama4_text` | text/moe | `Llama4ForCausalLM` | h | unknown | chunked attention, NoPE layers with query temperature, top-1 sigmoid MoE scaling the expert input |
| 36 | `glm4_moe` | text/moe | `Glm4MoeForCausalLM` | h | unknown | GLM-4.5/4.6: DeepSeek-V3 routing over GQA with partial rotary |
| 37 | `dbrx` | text/moe | `DbrxForCausalLM` | l | unknown | fused expert tensors, QKV clip, bias-free LayerNorm (Databricks) |
| 38 | `jetmoe` | text/moe | `JetMoeForCausalLM` | l | unknown | MoE for BOTH the MLP and the attention (mixture-of-attention heads) |
| 39 | `ernie4_5_moe` | text/moe | `Ernie4_5_MoeForCausalLM` | m | unknown | softmax router with a correction bias, shared experts, interleaved rotary (ERNIE-4.5-21B/300B-A47B) |
| 40 | `hunyuan_v1_moe` | text/moe | `HunYuanMoEV1ForCausalLM` | m | unknown | Hunyuan-A13B: QK-norm, shared expert, top-k routing |
| 41 | `minimax_m2` | text/moe | `MiniMaxM2ForCausalLM` | m | unknown | sigmoid router + bias, QK-norm over the whole projection, partial rotary (MiniMax-M2) |
| 42 | `longcat_flash` | text/moe | `LongcatFlashForCausalLM` | l | unknown | MLA, zero-computation experts and a shortcut-connected dense branch (Meituan LongCat-Flash) |
| 43 | `deepseek_v32` | text/moe | `DeepseekV32ForCausalLM` | h | unknown | DeepSeek sparse attention: a learned lightning indexer picks the top-k keys (V3.2) |
| 44 | `deepseek_v4` | text/moe | `DeepseekV4ForCausalLM` | h | unknown | 2026 DeepSeek: hyper-connections, compressed sparse attention, hash-routed experts |
| 45 | `qwen3_next` | text/hybrid | `Qwen3NextForCausalLM` | h | 0.5 % | gated delta rule (3:1 with attention), sigmoid-gated attention, shared-expert MoE |
| 46 | `qwen3_5_moe` | text/hybrid | `Qwen3_5MoeForCausalLM` | h | unknown | Qwen3.5 (split GDN projections) with MoE |
| 47 | `jamba` | text/hybrid | `JambaForCausalLM` | m | unknown | Mamba-1 + attention + MoE interleaving |
| 48 | `mamba` | text/hybrid | `MambaForCausalLM` | m | unknown | selective scan (state-spaces/mamba-*-hf) |
| 49 | `mamba2` | text/hybrid | `Mamba2ForCausalLM` | m | unknown | state-space duality, grouped B/C, gated RMSNorm (Codestral-Mamba) |
| 50 | `falcon_mamba` | text/hybrid | `FalconMambaForCausalLM` | l | unknown | Mamba with weightless B/C/dt norms |
| 51 | `rwkv` | text/hybrid | `RwkvForCausalLM` | l | unknown | RWKV-4: a recurrent model with no attention at all |
| 52 | `falcon_h1` | text/hybrid | `FalconH1ForCausalLM` | m | unknown | attention and Mamba-2 in PARALLEL inside every layer, muP multipliers (TII) |
| 53 | `granitemoehybrid` | text/hybrid | `GraniteMoeHybridForCausalLM` | m | unknown | Granite 4: Mamba-2 and attention layers with a shared-expert MoE per layer |
| 54 | `zamba2` | text/hybrid | `Zamba2ForCausalLM` | l | unknown | Mamba-2 backbone with ONE shared attention block reused at several depths |
| 55 | `nemotron_h` | text/hybrid | `NemotronHForCausalLM` | m | unknown | single-block layers (Mamba-2, attention, MLP or MoE) in a pattern string |
| 56 | `lfm2` | text/hybrid | `Lfm2ForCausalLM` | m | unknown | gated short convolutions interleaved with attention (Liquid LFM2) |
| 57 | `kimi_linear` | text/hybrid | `KimiLinearForCausalLM` | m | unknown | Kimi Delta Attention (channel-wise gated delta rule) with MLA layers (Kimi-Linear-48B-A3B) |
| 58 | `bert` | encoder | `BertModel` | vh | unknown | the most downloaded encoder family: bert-base-uncased and every all-MiniLM / bge / e5 / gte sentence embedder built on it |
| 59 | `roberta` | encoder | `RobertaModel` | vh | unknown | positions from padding_idx + 1; the base of XLM-R, CamemBERT, BGE-M3 |
| 60 | `xlm_roberta` | encoder | `XLMRobertaModel` | vh | unknown | multilingual encoder and embedder backbone (BGE-M3, multilingual-e5) |
| 61 | `distilbert` | encoder | `DistilBertModel` | h | unknown | BERT without token types under its own names |
| 62 | `mpnet` | encoder | `MPNetModel` | vh | unknown | all-mpnet-base-v2: bidirectional + a T5-bucket relative-position bias |
| 63 | `deberta_v2` | encoder | `DebertaV2Model` | h | unknown | disentangled attention (content-to-position and position-to-content terms): DeBERTa-v3 is the usual classification / NLI / reranker backbone |
| 64 | `albert` | encoder | `AlbertModel` | m | unknown | one layer's weights shared across the depth, factorised embedding |
| 65 | `modernbert` | encoder | `ModernBertModel` | h | unknown | 2024-25 encoder: RoPE, GeGLU, alternating local/global attention, bias-free LayerNorm |
| 66 | `nomic_bert` | encoder | `NomicBertModel` | h | unknown | a RoPE BERT with a fused gated MLP: nomic-embed-text, a leading open embedder |
| 67 | `clip_text` | encoder | `CLIPTextModelWithProjection` | vh | unknown | the text tower of CLIP and every Stable Diffusion text encoder; causal with learned positions |
| 68 | `t5` | encdec | `T5ForConditionalGeneration` | vh | unknown | relative-position buckets, RMSNorm without a mean, unscaled scores: the encoder-decoder head (t5-small/base) |
| 69 | `t5_gated` | encdec | `T5ForConditionalGeneration` | vh | unknown | T5 v1.1 / Flan-T5 / UL2: gated-GELU FFN, an untied head (also the text encoder of Flux, SD3 and PixArt) |
| 70 | `bart` | encdec | `BartForConditionalGeneration` | h | unknown | post-norm, learned positions at pos+2: summarisation and the BART/mBART family |
| 71 | `mbart` | encdec | `MBartForConditionalGeneration` | m | unknown | pre-norm BART with final norms: multilingual translation |
| 72 | `marian` | encdec | `MarianMTModel` | h | unknown | sinusoidal positions: the Helsinki-NLP/opus-mt translation models (thousands of repositories) |
| 73 | `longt5` | encdec | `LongT5ForConditionalGeneration` | l | unknown | T5 with local / transient-global encoder attention: long-input summarisation |
| 74 | `t5_encoder` | encdec | `T5EncoderModel` | vh | unknown | T5EncoderModel alone: the text encoder of Flux, SD3, PixArt, Wan, Sana (encoder-only reuse of a seq2seq checkpoint) |
| 75 | `llava` | vlm | `LlavaForConditionalGeneration` | vh | unknown | LLaVA 1.5/1.6, the CLIP-tower + MLP projector + Llama pattern behind most open VLMs |
| 76 | `qwen2_vl` | vlm | `Qwen2VLForConditionalGeneration` | vh | unknown | M-RoPE (3-D positions), a native-resolution vision tower, patch merger |
| 77 | `qwen2_5_vl` | vlm | `Qwen2_5_VLForConditionalGeneration` | vh | unknown | window-attention RMSNorm/SwiGLU tower + M-RoPE: the default open VLM of 2025 |
| 78 | `qwen3_vl` | vlm | `Qwen3VLForConditionalGeneration` | h | unknown | DeepStack: intermediate vision features are ADDED into the early text layers' hidden states; interleaved M-RoPE |
| 79 | `gemma3_vlm` | vlm | `Gemma3ForConditionalGeneration` | vh | unknown | SigLIP tower + Gemma-3 text (the 4B/12B/27B are VLMs) |
| 80 | `paligemma` | vlm | `PaliGemmaForConditionalGeneration` | h | unknown | PREFIX-LM attention: the image and prompt tokens attend bidirectionally, the answer causally |
| 81 | `idefics3` | vlm | `Idefics3ForConditionalGeneration` | h | unknown | SmolVLM / Idefics3: pixel-shuffle connector over a SigLIP-style tower, Llama text |
| 82 | `mllama` | vlm | `MllamaForConditionalGeneration` | h | unknown | text layers CROSS-ATTEND to vision states (Llama-3.2-Vision) |
| 83 | `clip_vision` | vision | `CLIPVisionModelWithProjection` | vh | unknown | the image tower of CLIP, LLaVA, Stable Diffusion image-conditioning and zero-shot classification |
| 84 | `siglip_vision` | vision | `SiglipVisionModel` | h | unknown | the image tower of Gemma-3, PaliGemma, Idefics3; attention-pooling head |
| 85 | `vit` | vision | `ViTForImageClassification` | vh | unknown | image classification at hub scale: ViT, DeiT, BEiT, DINOv2 share this encoder |
| 86 | `resnet` | vision | `ResNetForImageClassification` | vh | unknown | CONVOLUTIONAL classification (ResNet, ConvNeXt, EfficientNet, timm): the largest family by downloads with no attention at all |
| 87 | `unet2d_condition` | image-gen | `UNet2DConditionModel` | vh | unknown | Stable Diffusion 1.x/2.x: the denoiser of the largest image-generation family (convolutions, GroupNorm, cross-attention, timestep embedding) |
| 88 | `unet_sdxl` | image-gen | `UNet2DConditionModel` | vh | unknown | SDXL: added text/time conditioning and a transformer stack per resolution (the most used open image model) |
| 89 | `dit` | image-gen | `DiTTransformer2DModel` | m | unknown | the DiT: patchified latents, adaLN-Zero, class conditioning (ancestor of PixArt, SD3, Flux) |
| 90 | `flux` | image-gen | `FluxTransformer2DModel` | vh | unknown | MMDiT with 3-axis RoPE, double- and single-stream blocks (FLUX.1 dev/schnell) |
| 91 | `sd3` | image-gen | `SD3Transformer2DModel` | h | unknown | MMDiT with joint text-image attention and adaLN (Stable Diffusion 3/3.5) |
| 92 | `vae_kl` | image-gen | `AutoencoderKL` | vh | unknown | every diffusion pipeline decodes through a convolutional VAE with a mid-block self-attention |
| 93 | `whisper` | audio | `WhisperForConditionalGeneration` | vh | unknown | speech recognition: log-mel front end, two strided Conv1d, sinusoidal-position encoder, cross-attending decoder |
| 94 | `wav2vec2` | audio | `Wav2Vec2ForCTC` | vh | unknown | raw-waveform CTC: a conv feature extractor, a grouped-conv positional embedding, a transformer |
| 95 | `speecht5` | audio | `SpeechT5ForTextToSpeech` | m | unknown | text-to-speech: a mel decoder with a pre-net/post-net (a vocoder follows) |
| 96 | `musicgen` | audio | `MusicgenForConditionalGeneration` | m | unknown | music generation: an autoregressive decoder over several EnCodec codebooks (delay pattern) and a T5 text encoder |
| 97 | `encodec` | audio | `EncodecModel` | m | unknown | the neural audio codec under MusicGen, Bark, Moshi-class models: causal conv encoder/decoder + residual vector quantisation |
| 98 | `chatglm3` | remote-code | `ChatGLMModel` | h | unknown | trust_remote_code: THUDM/chatglm3-6b is among the most downloaded Chinese chat models; its forward lives in the repository, not in transformers |
| 99 | `internlm2` | remote-code | `InternLM2ForCausalLM` | m | 0.5 % | trust_remote_code Llama-lineage with fused wqkv (InternLM2/2.5) |
| 100 | `minicpm` | remote-code | `MiniCPMForCausalLM` | m | 0.3 % | trust_remote_code with muP-style scale_emb / scale_depth / dim_model_base |
<!-- END GENERATED: corpus -->

<!-- BEGIN GENERATED: summary -->
| | entries | share of corpus | usage-weighted |
| --- | ---: | ---: | ---: |
| Level A — the config alone | 6 | 6 % | 9 % |
| Level B — a thin data adapter | 47 | 47 % | 46 % |
| Level C — a capability is missing | 47 | 47 % | 45 % |
| **A + B (expressible with existing features)** | **53** | **53 %** | **55 %** |

Level C by kind of gap (this lane's classification, `tools/corpus/blockers.json`):

| kind | entries | meaning |
| --- | ---: | --- |
| route | 21 | no data route exists for this kind of model yet (the lowering is Rust per family, or absent); existing primitives suffice |
| feature | 20 | a generic FEATURE is missing; lowerable in principle with the existing primitives |
| protocol | 5 | a protocol capability beyond the IR is needed (an input binding, a second history, a stage kind) |
| remote | 1 | the reference semantics are remote code outside transformers (cannot be pinned) |

Court coverage: 47880 commit points over 45 lowered models reproduced by the cone evaluator from their opened leaves (reference evaluator and typed backend; 0 failures); 25 of the 25 primitives and 6 commit-point roles reached.

| category | A | B | C | total |
| --- | ---: | ---: | ---: | ---: |
| audio | 0 | 0 | 5 | 5 |
| encdec | 0 | 0 | 7 | 7 |
| encoder | 0 | 6 | 4 | 10 |
| image-gen | 0 | 0 | 6 | 6 |
| remote-code | 0 | 2 | 1 | 3 |
| text/dense | 6 | 17 | 5 | 28 |
| text/hybrid | 0 | 7 | 6 | 13 |
| text/moe | 0 | 9 | 7 | 16 |
| vision | 0 | 0 | 4 | 4 |
| vlm | 0 | 6 | 2 | 8 |
<!-- END GENERATED: summary -->

<!-- BEGIN GENERATED: results -->
| id | category | level | route / adapter | features | missing / why not | failed stage |
| --- | --- | :-: | --- | ---: | --- | --- |
| `llama` | text/dense | **A** | standard template (no adapter) | 8 |  |  |
| `qwen2` | text/dense | **A** | standard template (no adapter) | 9 |  |  |
| `mistral` | text/dense | **A** | standard template (no adapter) | 8 |  |  |
| `gpt2` | text/dense | **B** | built-in adapter `gpt2` | 11 |  |  |
| `qwen3` | text/dense | **A** | standard template (no adapter) | 10 |  |  |
| `gemma2` | text/dense | **B** | built-in adapter `gemma2` | 14 |  |  |
| `gemma3_text` | text/dense | **B** | built-in adapter `gemma3-text` | 15 |  |  |
| `gemma4_text` | text/dense | **B** | built-in adapter `gemma4-text` | 24 |  |  |
| `phi3` | text/dense | **B** | built-in adapter `phi3` | 8 |  |  |
| `gpt_neox` | text/dense | **B** | built-in adapter `gpt-neox` | 10 |  |  |
| `opt` | text/dense | **B** | built-in adapter `opt` | 11 |  |  |
| `falcon` | text/dense | **B** | built-in adapter `falcon` | 8 |  |  |
| `starcoder2` | text/dense | **B** | built-in adapter `starcoder2` | 11 |  |  |
| `cohere2` | text/dense | **B** | built-in adapter `cohere2` | 12 |  |  |
| `granite` | text/dense | **A** | standard template (no adapter) | 10 |  |  |
| `glm4` | text/dense | **B** | built-in adapter `glm4` | 11 |  |  |
| `smollm3` | text/dense | **B** | built-in adapter `smollm3` | 9 |  |  |
| `arcee` | text/dense | **B** | third-party adapter `arcee` (tools/corpus/adapters) | 7 |  |  |
| `apertus` | text/dense | **C** | - |  | xIELU activation (learned per-layer parameters) → FR-05 | read |
| `helium` | text/dense | **B** | third-party adapter `helium` (tools/corpus/adapters) | 7 |  |  |
| `hunyuan_v1_dense` | text/dense | **C** | third-party adapter `hunyuan-v1-dense` (tools/corpus/adapters) (refuted: float_vs_hf) | 9 | q/k RMS norm applied AFTER the rotation → FR-02 | float_vs_hf |
| `seed_oss` | text/dense | **B** | third-party adapter `seed-oss` (tools/corpus/adapters) | 8 |  |  |
| `ernie4_5` | text/dense | **B** | third-party adapter `ernie4-5` (tools/corpus/adapters) | 9 |  |  |
| `bitnet` | text/dense | **C** | standard template (no adapter) (refuted: bind) | 7 | attn_sub_norm before o_proj and ffn_sub_norm before down_proj → FR-03 | bind |
| `persimmon` | text/dense | **B** | third-party adapter `persimmon` (tools/corpus/adapters) | 12 |  |  |
| `diffllama` | text/dense | **C** | - |  | differential attention (head-pair difference, 2d RMS group norm, derived lambda) → FR-04 | read |
| `cwm` | text/dense | **A** | standard template (no adapter) | 9 |  |  |
| `gemma3n_text` | text/dense | **C** | - |  | RESIDUAL_ALTUP, RESIDUAL_LAUREL, FFN_ACTIVATION_SPARSITY → FR-12 | read |
| `mixtral` | text/moe | **B** | built-in adapter `mixtral` | 9 |  |  |
| `qwen2_moe` | text/moe | **B** | built-in adapter `qwen2-moe` | 12 |  |  |
| `qwen3_moe` | text/moe | **B** | built-in adapter `qwen3-moe` | 11 |  |  |
| `granitemoe` | text/moe | **B** | built-in adapter `granitemoe` | 11 |  |  |
| `deepseek_v3` | text/moe | **B** | built-in adapter `deepseek-v3` | 15 |  |  |
| `gpt_oss` | text/moe | **B** | built-in adapter `gpt-oss` | 14 |  |  |
| `llama4_text` | text/moe | **B** | built-in adapter `llama4-text` | 20 |  |  |
| `glm4_moe` | text/moe | **B** | built-in adapter `glm4-moe` | 17 |  |  |
| `dbrx` | text/moe | **C** | third-party adapter `dbrx` (tools/corpus/adapters) (refuted: bind) | 10 | experts stored flat [E*I, D]; everything else is data → FR-01 | bind |
| `jetmoe` | text/moe | **C** | - |  | mixture-of-attention: routed per-expert q/o projections → FR-06 | read |
| `ernie4_5_moe` | text/moe | **C** | third-party adapter `ernie4-5-moe` (tools/corpus/adapters) (refuted: bind) | 14 | selection bias [1,E] needs a squeeze; softmax router ignores selection_bias → FR-01, FR-07 | bind |
| `hunyuan_v1_moe` | text/moe | **C** | third-party adapter `hunyuan-v1-moe` (tools/corpus/adapters) (refuted: float_vs_hf) | 12 | q/k RMS norm applied AFTER the rotation → FR-02 | float_vs_hf |
| `minimax_m2` | text/moe | **B** | third-party adapter `minimax-m2` (tools/corpus/adapters) | 12 |  |  |
| `longcat_flash` | text/moe | **C** | - |  | zero-computation experts and shortcut-connected MoE → FR-08 | read |
| `deepseek_v32` | text/moe | **C** | - |  | DeepSeek sparse attention: a token-level top-k indexer → FR-09 | read |
| `deepseek_v4` | text/moe | **C** | - |  | hyper-connections, compressed sparse/heavily compressed attention, hash routing, sqrt-softplus scoring → FR-10 | read |
| `qwen3_next` | text/hybrid | **B** | built-in adapter `qwen3-next` | 19 |  |  |
| `qwen3_5_moe` | text/hybrid | **B** | built-in adapter `qwen3-5-moe` | 18 |  |  |
| `jamba` | text/hybrid | **B** | built-in adapter `jamba` | 11 |  |  |
| `mamba` | text/hybrid | **B** | built-in adapter `mamba` | 7 |  |  |
| `mamba2` | text/hybrid | **B** | built-in adapter `mamba2` | 6 |  |  |
| `falcon_mamba` | text/hybrid | **B** | built-in adapter `falcon-mamba` | 7 |  |  |
| `rwkv` | text/hybrid | **B** | built-in adapter `rwkv4` | 8 |  |  |
| `falcon_h1` | text/hybrid | **C** | - |  | MIXER_PARALLEL_BRANCH, SCALE_MUP → FR-11 | read |
| `granitemoehybrid` | text/hybrid | **C** | third-party adapter `granitemoehybrid` (tools/corpus/adapters) (refuted: bind) | 11 | fused shared-expert gate/up tensor; a built-in refusal cannot be overridden → FR-01, FR-25 | bind |
| `zamba2` | text/hybrid | **C** | - |  | ATTN_SHARED_BLOCK → FR-13 | read |
| `nemotron_h` | text/hybrid | **C** | - |  | LAYER_FFN_ONLY → FR-14 | read |
| `lfm2` | text/hybrid | **C** | - |  | gated short convolution mixer → FR-15 | read |
| `kimi_linear` | text/hybrid | **C** | - |  | Kimi delta attention (channel-wise gated delta rule) → FR-16 | read |
| `bert` | encoder | **B** | built-in adapter `bert` | 12 |  |  |
| `roberta` | encoder | **B** | built-in adapter `roberta` | 12 |  |  |
| `xlm_roberta` | encoder | **B** | built-in adapter `roberta` | 12 |  |  |
| `distilbert` | encoder | **B** | built-in adapter `distilbert` | 11 |  |  |
| `mpnet` | encoder | **B** | built-in adapter `mpnet` | 12 |  |  |
| `deberta_v2` | encoder | **C** | - |  | an adapter for `DebertaV2Model` → FR-17 | read |
| `albert` | encoder | **C** | third-party adapter `albert` (tools/corpus/adapters) (refuted: panic) | 13 | factorised embedding in the bidirectional lowerer (it panics) → FR-17, FR-26 | panic |
| `modernbert` | encoder | **C** | - |  | an adapter for `ModernBertModel` → FR-17 | read |
| `nomic_bert` | encoder | **C** | - |  | an adapter for `NomicBertModel` → FR-17 | read |
| `clip_text` | encoder | **B** | built-in adapter `clip-text` | 10 |  |  |
| `t5` | encdec | **C** | core Rust route |  | an adapter for `T5ForConditionalGeneration` → FR-18 | read |
| `t5_gated` | encdec | **C** | core Rust route |  | an adapter for `T5ForConditionalGeneration` → FR-18 | read |
| `bart` | encdec | **C** | core Rust route |  | an adapter for `BartForConditionalGeneration` → FR-18 | read |
| `mbart` | encdec | **C** | core Rust route |  | an adapter for `MBartForConditionalGeneration` → FR-18 | read |
| `marian` | encdec | **C** | core Rust route |  | an adapter for `MarianMTModel` → FR-18 | read |
| `longt5` | encdec | **C** | - |  | an adapter for `LongT5ForConditionalGeneration` → FR-18 | read |
| `t5_encoder` | encdec | **C** | - |  | an adapter for `T5EncoderModel` → FR-18 | read |
| `llava` | vlm | **B** | built-in adapter `vlm-llama` | 7 |  |  |
| `qwen2_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen2_5_vl` | vlm | **B** | built-in adapter `vlm-qwen2-vl` | 9 |  |  |
| `qwen3_vl` | vlm | **B** | third-party adapter `qwen3-vl` (tools/corpus/adapters) | 10 |  |  |
| `gemma3_vlm` | vlm | **B** | built-in adapter `vlm-gemma3` | 14 |  |  |
| `paligemma` | vlm | **C** | - |  | ATTN_PREFIX_LM → FR-20 | read |
| `idefics3` | vlm | **B** | third-party adapter `idefics3` (tools/corpus/adapters) | 7 |  |  |
| `mllama` | vlm | **C** | - |  | ATTN_CROSS → FR-21 | read |
| `clip_vision` | vision | **C** | core Rust route |  | an adapter for `CLIPVisionModelWithProjection` → FR-19 | read |
| `siglip_vision` | vision | **C** | core Rust route |  | an adapter for `SiglipVisionModel` → FR-19 | read |
| `vit` | vision | **C** | - |  | an adapter for `ViTModel` → FR-19 | read |
| `resnet` | vision | **C** | - |  | an adapter for `ResNetModel` → FR-19 | read |
| `unet2d_condition` | image-gen | **C** | - |  | Conv2d, GroupNorm, timestep embedding, cross-attention, denoise loop → FR-22 | read |
| `unet_sdxl` | image-gen | **C** | - |  | as the SD UNet plus added text/time conditioning → FR-22 | read |
| `dit` | image-gen | **C** | - |  | patchify, adaLN-Zero, class conditioning, denoise loop → FR-22 | read |
| `flux` | image-gen | **C** | - |  | MMDiT double/single blocks, 3-axis RoPE, flow-matching loop → FR-22 | read |
| `sd3` | image-gen | **C** | - |  | MMDiT joint attention, adaLN, flow-matching loop → FR-22 | read |
| `vae_kl` | image-gen | **C** | - |  | convolutional encoder/decoder with a mid-block attention → FR-22 | read |
| `whisper` | audio | **C** | - |  | an adapter for `WhisperForConditionalGeneration` → FR-23 | read |
| `wav2vec2` | audio | **C** | - |  | an adapter for `Wav2Vec2ForCTC` → FR-23 | read |
| `speecht5` | audio | **C** | - |  | an adapter for `SpeechT5ForTextToSpeech` → FR-23 | read |
| `musicgen` | audio | **C** | - |  | an adapter for `MusicgenForConditionalGeneration` → FR-23 | read |
| `encodec` | audio | **C** | - |  | an adapter for `EncodecModel` → FR-23 | read |
| `chatglm3` | remote-code | **C** | - |  | REFERENCE_REMOTE_CODE → FR-24 | read |
| `internlm2` | remote-code | **B** | built-in adapter `internlm2` | 8 |  |  |
| `minicpm` | remote-code | **B** | built-in adapter `minicpm` | 11 |  |  |
<!-- END GENERATED: results -->

<!-- BEGIN GENERATED: uplift -->
The standard template (no adapter) reads 10 of the 65 decoder-route entries without refusing; for the rest it names the keys it does not model:

| convention the template lacks | entries it blocks | the keys |
| --- | ---: | --- |
| MoE hyper-parameters and layer pattern | 13 | `decoder_sparse_step`, `first_k_dense_replace`, `mlp_only_layers`, `moe_intermediate_size`, `moe_topk`, `n_group`, `n_routed_experts`, `n_shared_experts`, `norm_topk_prob`, `num_experts`, `num_experts_per_tok`, `num_local_experts`, `num_nextn_predict_layers`, `routed_scaling_factor`, `router_jitter_noise`, `shared_expert_intermediate_size`, `topk_group` |
| MLA dimensions | 2 | `kv_lora_rank`, `q_lora_rank`, `qk_head_dim`, `qk_nope_head_dim`, `qk_rope_head_dim`, `v_head_dim` |
| nested VLM wrapper (text_config, vision_config, token ids) | 6 | `boi_token_index`, `eoi_token_index`, `image_token_id`, `image_token_index`, `mm_tokens_per_image`, `text_config`, `video_token_id`, `vision_config`, `vision_end_token_id`, `vision_start_token_id` |
| bias switches | 8 | `attention_out_bias`, `bias`, `enable_bias`, `qkv_bias`, `use_bias` |
| norm epsilon / norm kind aliases | 8 | `_remove_final_layer_norm`, `do_layer_norm_before`, `layer_norm_elementwise_affine`, `layer_norm_eps`, `layer_norm_epsilon`, `norm_epsilon` |
| activation aliases | 5 | `activation`, `activation_function`, `hidden_activation` |
| dimension aliases (n_embd, n_head, n_layer, ...) | 5 | `d_model`, `ffn_dim`, `ffn_hidden_size`, `max_seq_len`, `n_embd`, `n_head`, `n_heads`, `n_inner`, `n_layer`, `n_layers`, `n_positions`, `word_embed_proj_dim` |
| rope / position keys | 4 | `alibi`, `multi_query`, `new_decoder_architecture`, `no_rope_layer_interval`, `original_max_position_embeddings`, `use_parallel_residual` |
| everything else (family-specific keys) | 18 | 60 distinct keys |
<!-- END GENERATED: uplift -->
