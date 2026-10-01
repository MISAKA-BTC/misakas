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

What transformers 5.17 computes is **not** the paper's two-softmax map. `modular_diffllama.py:DiffLlamaAttention.forward`
runs the ordinary attention twice with the *same* q/k and two value halves: `v1 = repeat(v[:KV/2], 2)`,
`v2 = repeat(v[KV/2:], 2)` along the KV-head axis, `o1 = A·v1`, `o2 = A·v2`; per head `[o1; o2]` (width `2d`); then the
first `H/2` heads minus `λ` times the last `H/2` heads; `(1 − λ_init) · RMSNorm_{2d}(·)` (no gain); `o_proj` over `(H/2)·2d`.
`λ = exp(Σ λq1⊙λk1) − exp(Σ λq2⊙λk2) + λ_init(layer)`, `λ_init = 0.8 − 0.6·e^{−0.3·layer}` — a function of weights only,
so it is folded into a Q24 constant per layer at conversion (no on-line transcendental). **Spec**: `AttnSpec.differential:
Option<DiffSpec>`; tensors `lambda_q1/k1/q2/k2 [d]`. **Decomposition**: one softmax map (cost of ordinary attention) plus a
width-`2d` norm; no primitive. The reference is the 5.17 code (V-halving), not the paper. **Acceptance**: `diffllama`.

### FR-05 `ACT_LEARNED_POINTWISE_V1` — xIELU (S, unlocks `apertus`)

`y = x>0 ? αp·x² + β·x : (expm1(min(x, ε)) − x)·αn + β·x` with `αp = softplus(p)`, `αn = β + softplus(n)`, per-layer scalars
`p, n` (tensors `act_fn.alpha_p`, `alpha_n`, shape `[1]`) and buffers `β = 0.5`, `ε = −1e−6`
(`activations.py:XIELUActivation._xielu_python`). Pointwise on the activation code, so the library's `act_table` applies: a
65,536-entry table built at conversion from the layer's two scalars, stored as a per-layer param (it exceeds the 64 KiB const
limit), `Gather` at run time. Or `Select(x>0, αp·x² + β·x, …)` with `IntExp` on the non-positive branch. **Spec**:
`Act::Xielu` carrying its parameter tensor names. No primitive. **Acceptance**: `apertus` (adapter to write when the
activation exists; the rest of Apertus is Llama + QK-norm + llama3 rope).

## Decoder-side features that need a new mixer, FFN or residual kind

(Each is a generic feature with the entries it unlocks; decompositions are into the 25 primitives. Details below are from
the research pass over the transformers code; sections marked *pending* are being filled.)

### FR-06 `ATTN_EXPERT_HEADS_V1` — mixture-of-attention (M, unlocks `jetmoe`)

JetMoE routes the *attention* like an MoE: a top-k router picks `k` experts per token; each expert owns a query
projection and an output projection (`JetMoeParallelExperts`, weight `[E, out, in]`); K/V are shared and computed once; the
`k` expert outputs are gate-weighted (softmax over the `k` kept logits) and summed, plus a bias
(`modeling_jetmoe.py:JetMoeMoA.map/reduce`). **Spec**: `AttnSpec.expert_heads: Option<{experts, top_k, router}>`
reusing `RouterSpec` (top-k of the raw logits, then softmax over the `k`: `TopKThenSoftmax`); tensors
`self_attention.experts.input_linear.weight [E, kv_channels·KV, D]` (the per-expert query projections),
`experts.output_linear.weight [E, D, kv_channels·KV]`, `experts.router.layer.weight [E, D]`, `experts.bias [D]`,
`self_attention.kv_proj.weight [2·kv_channels·KV, D]` (shared K and V); the MLP is a JetMoE MoE with the same
`input_linear [E, 2I, D]` (gate rows first = `FusedGateFirst`), `output_linear`, `router.layer` and a `bias`. **Decomposition**: `TopK` (committed) → `Gather` of the selected experts' weight rows → `MatMul` per
selected expert (the MoE FFN lowering already does this for the FFN) → attention over the shared history → per-expert `MatMul`
with the output projection → gate-weighted `ReduceSum`. No primitive. *(research pass pending for the exact cone costs.)*

### FR-08 … FR-16 — *pending: filled from the research pass (zero-computation experts, token-level indexer attention, DeepSeek-V4, parallel mixers, Gemma-3n, Zamba2, mixer-less layers, short-convolution mixer, Kimi delta attention)*

## Routes (a whole kind of model with no data-driven lowering)

### FR-17 … FR-19 — *pending: bidirectional encoders, encoder–decoders, vision towers from a spec*

### FR-22, FR-23 — *pending: image generation and audio*

## Named in the registry already

### FR-20 `ATTN_PREFIX_LM_V1` (`paligemma`) and FR-21 `ATTN_CROSS_V1` (`mllama`, and the encoder–decoders' cross-attention) — *pending*

## Infrastructure and honesty

### FR-25 user adapters may override built-in refusals; remove the stale Granite-hybrid refusal (S)

`hf_schema::read_model` consults `builtin::refusal_for(arch)` **before** choosing any adapter, so an adapter a user supplies
can never override an entry of `adapters/refusals.json`. The entry for `GraniteMoeHybridForCausalLM` says the layer pattern is
not mapped (`MIXER_LAYER_PATTERN_HYBRID_V1`) — but the third-party adapter
`tools/corpus/adapters/granitemoehybrid.json` expresses it (a Mamba-2 or NoPE-attention mixer per `layer_types`, a
Granite-MoE FFN with a shared MLP, Granite's multipliers) and passes every stage on a patched checkpoint (FR-01). Request:
a `refuse` entry applies only when no adapter, built-in or user, claims the architecture; delete the stale entry. The harness
bypasses the refusal for such adapters by renaming the architecture in memory and records `refusal_bypassed_for_user_adapter`.

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
