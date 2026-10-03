# PALW-TIR — Hugging Face coverage (lowerer Gate 1)

| Field | Value |
| --- | --- |
| Status | Gate 1 complete (HF → ArchSpec → HL graph → f32 reference); Gate 2 (TIR expansion, quantisation, verdicts) not started |
| Branch | `tir/lower` (off `rcore/int-6` 08481e720), crate `misaka-palw-tir-lower` (non-consensus) |
| Reference implementation | **transformers 5.17.0** modeling code (torch 2.14.0, CPU, fp32, eager attention) |
| Related | RFC-0002 §1 (corpus, freeze criterion 5), §2 (layering), §3.3 (composites), §4.1 (`TirProgramV1`), §8 (`check-architecture`) |

## 概要(日本語)

- **HF の `config.json` + safetensors を、モデル固有コードなしで HL グラフ(RFC-0002 §2 の「高レベル ML グラフ」)に落とす層を作った。**
  54 種の `architectures[0]`(別名を含む)を扱い、全ファミリー(tiny 構成 57 本)で transformers 5.17 自身が出した logits と f32 参照が
  **最大誤差 2.8e-5(典型 1e-6)で一致**する。HF fixture はオフラインで生成(モデルのダウンロードなし)。
- **保守的**: config のキーは「数式を変えるので読む」「数式を変えないと分かっている(dropout・生成設定など)」のどちらかに
  分類され、どちらでもないキー・未実装の値・未知の architecture・未モデル化の remote code・量子化済み checkpoint は
  すべて `NOT_LOWERABLE(理由)`。黙って無視するものはない。
- **HL グラフは frontend 中立**: HF のキー名・テンソル名・融合レイアウトは `hf_schema`(アダプタ = データファイル `adapters/*.json`、[model-adapter-v1.md](model-adapter-v1.md))と `hf_weights` にしかなく、HL の
  op と param は数学的な属性(rope 変種、norm の `1+w`、GQA のグルーピング規則など)だけを持つ(テストで強制)。
- **推定カバー率**: HF の decoder-only テキスト生成 checkpoint の約 **91%**(件数ベース、`architectures[0]` 別)。
- **HF 側の曖昧さ**: 参照そのものが割れている箇所がある(Gemma-2 の soft-cap を sdpa が落とす、Falcon の eager ALiBi が
  5.17 で二重加算、dynamic NTK/LongRoPE の prefill と decode の不一致、DeepSeek-V2 の native と remote code の不一致)。
  RFC の基準 5「HF float 原典」は **transformers のバージョン・eager attention・位置ごと(decode)意味論** を固定すべき。
- **GDN**: HF は value head `vh` に key head `vh / (nv/nk)`(grouping)を当てる。live の Q36 kernel は `vh % nk`(tiling)。
  両者は value head の並べ替えで一致する(単体テストで証明)。HF から lower する限り grouping。

## 1. What "covered" means here

A decoder architecture is **covered** when all of the following hold:

1. The HF schema reader (`hf_schema`, with the architecture's adapter — a data file, `adapters/*.json`; `hf_config` is the entry point) parses its `config.json` into a `ModelSpec` (`ArchSpec`) using the config class's own defaults
   (these matter: `save_pretrained` diffs a nested `text_config` against the class defaults, so a
   VLM's text config often names only the fields that differ), and refuses anything it does not model.
2. `hl::build_program` builds the HL program; `hf_weights::bind` maps every HL param to HF tensors.
3. A **fixture from transformers itself** matches: a tiny randomly initialised model of that
   architecture (`tools/gen_hf_fixtures.py`, offline, `AutoModelForCausalLM.from_config`), with every
   weight made live (non-zero biases, gains away from one, O(1) activations) and rounded to bf16,
   saved, **re-loaded from the saved files**, run in fp32 with eager attention. The Rust float
   reference must consume every tensor in the file, and its logits must agree to 1e-4·scale with
   the same argmax at every position. Observed agreement is 1e-7 … 3e-5 (table in §3.9).

Semantics chosen where HF has more than one answer (see §7): eager attention; per-position
(one-token decode) semantics for sequence-length-dependent rope; transformers 5.17 over remote code
where they disagree; top-k ties to the lowest index (PALW-TIR-11; `torch.topk` does not specify).

`palw-tir-check --config config.json [--weights …]` prints the result for any config
(ArchSpec, block schedule, per-position MACs/state, weights check, verdict).

## 2. Share of Hugging Face checkpoints

Estimated share of **decoder-only text-generation** checkpoints on the hub by `architectures[0]`,
by repository count including fine-tunes and merges. These are estimates from the hub's model trees
and fine-tune counts as I know them through 2025 — they could not be measured offline. The ORDER is
robust; the percentages are ±30 % relative.

| `architectures[0]` | est. share | status |
| --- | ---: | --- |
| `LlamaForCausalLM` (Llama 1–3.3, TinyLlama, Vicuna, Yi, DeepSeek-LLM/Coder v1, Falcon3, SmolLM 1/2, CodeLlama, Solar, …) | ~30 % | covered |
| `Qwen2ForCausalLM` (Qwen1.5/2/2.5, QwQ, DeepSeek-R1-Distill-Qwen, OpenThinker, …) | ~20 % | covered |
| `MistralForCausalLM` (Mistral, Zephyr, OpenHermes, Nemo, Ministral, …) | ~8 % | covered |
| `GPT2LMHeadModel` (GPT-2, DistilGPT-2, countless small experiments) | ~7 % | covered |
| `Qwen3ForCausalLM` | ~6 % | covered |
| `Gemma2ForCausalLM`, `Gemma3ForCausalLM`, `Gemma3ForConditionalGeneration` (text), `GemmaForCausalLM` | ~6 % | covered |
| `Phi3ForCausalLM`, `PhiForCausalLM` | ~3 % | covered |
| `GPTNeoXForCausalLM` (Pythia, Dolly, RedPajama-INCITE, StableLM-alpha) | ~2 % | covered |
| `OPTForCausalLM` | ~1.5 % | covered |
| `Qwen2MoeForCausalLM`, `Qwen3MoeForCausalLM`, `Qwen3NextForCausalLM`, `Qwen3_5*` | ~1.5 % | covered |
| `MixtralForCausalLM` | ~1 % | covered |
| `BloomForCausalLM`, `GPTNeoForCausalLM` | ~2 % | covered |
| Falcon, StableLM, StarCoder2, GPTBigCode, GPT-J, OLMo 1/2, OLMoE, Cohere 1/2, Granite(+MoE), Nemotron, EXAONE-4, SmolLM3, MPT, DeepSeek-V2/V3, gpt-oss, Mamba 1/2, FalconMamba, Jamba, RWKV-4, LLaVA/Mistral-3 text decoders | ~4 % together | covered |
| InternLM2, MiniCPM, EXAONE-3 (remote code) | ~1 % | lowerable, **unverified** (no offline fixture) |
| Not covered: ChatGLM/GLM-4, Baichuan, Llama-4, Gemma-3n, RWKV-5/6/7, Phi-3.5-MoE, DBRX, the long tail (§4) | ~7 % | `NOT_LOWERABLE` |

**Covered ≈ 91 % by count (≈ 92 % with the unverified remote-code three).** By downloads the
covered share is higher (the head of the distribution is Llama/Qwen/Mistral/Gemma/Phi). Encoder-only,
encoder–decoder and non-text models are excluded from the denominator (they are out of scope, §4).

## 3. Architectures in scope

For each architecture: the config fields that change the math (everything else is either inert —
dropout, init, token ids, generation defaults, attention-backend flags — or refused), the HL ops it
needs, and its fixtures. **Defaults are the transformers 5.17 config-class defaults.** "HL ops"
omits the ops every decoder uses: `Embedding`, `Norm`, `Linear`, `Act`, `Add`, `Mul`, `Rope`,
`HistAppend`, `Attention`.

### 3.1 Llama lineage (C1, C2)

| Architecture | Math-changing fields (effect) | Extra HL ops | Fixture(s) |
| --- | --- | --- | --- |
| `LlamaForCausalLM` | `hidden_size`, `intermediate_size`, `num_hidden_layers`, `num_attention_heads`, `num_key_value_heads` (GQA via `repeat_kv`), `head_dim` (default hidden/heads), `hidden_act`, `rms_norm_eps`, `rope_theta`/`rope_scaling`/`rope_parameters` (§3.8), `attention_bias` (q,k,v **and o**), `mlp_bias` (gate,up,down), `tie_word_embeddings`, `max_position_embeddings` (dynamic rope only). `pretraining_tp` inert (same function, split matmuls) | — | `llama` (llama3 rope, biases), `llama_linear_tied` |
| `MistralForCausalLM` | Llama fields minus biases (`attention_bias`/`mlp_bias` accepted only false); `sliding_window` (default **4096** when absent, `null` = off): window on every layer; `layer_types` if present | window | `mistral_window` |
| `Qwen2ForCausalLM` | Llama fields; q/k/v bias always, o no bias; `use_sliding_window` (default false) × `sliding_window` × `max_window_layers` (layers ≥ it slide), or `layer_types`; `use_mrope` must be false | window | `qwen2_dynamic` (dynamic NTK, tied), `qwen2_sliding` |
| `Qwen3ForCausalLM` | Qwen2 fields + `head_dim` (default 128), `attention_bias` (all four), per-head RMS QK-norm (gain `[head_dim]`, eps `rms_norm_eps`) before rope | per-head `Norm` | `qwen3_yarn` (YaRN) |
| `GraniteForCausalLM` | Llama fields + `embedding_multiplier` (× embedding), `residual_multiplier` (× each branch), `attention_multiplier` (the softmax scale itself), `logits_scaling` (logits ÷) | `Scale` | `granite` |
| `SmolLM3ForCausalLM` | Qwen2-style sliding fields + `no_rope_layers` (1 = rope, 0 = NoPE; default every `no_rope_layer_interval`-th layer NoPE) | NoPE layers | `smollm3` |
| `MiniCPMForCausalLM` (remote) | Llama fields + `scale_emb` (× embedding), `scale_depth`/√L (× branch), `dim_model_base` (head input × base/hidden); default tied | `Scale` | none offline — **unverified** |
| `InternLM2ForCausalLM` (remote) | Llama fields; fused `wqkv` rows `[kv][q_0..q_{g−1}, k, v][d]`; `bias` (default **true**) on wqkv and wo; dynamic/linear rope | — | none offline — **unverified** |
| `ExaoneForCausalLM` (EXAONE-3, remote) | Llama math under GPT-2-style names (`transformer.h.{i}.attn.attention.q_proj`, `mlp.c_fc_0/1`); `activation_function`, `layer_norm_epsilon` (RMS), `num_layers` | — | none offline — **unverified** |
| `Exaone4ForCausalLM` | post-norms only (`post_attention_layernorm`, `post_feedforward_layernorm`), per-head QK-norm; `sliding_window` × `sliding_window_pattern`/`layer_types`: 3 sliding : 1 global, **global layers NoPE when a window is set** | NoPE, window | `exaone4` |
| `NemotronForCausalLM` | LayerNorm1P `(1+w)·x̂ + b` (`norm_eps`), `relu2` **non-gated** MLP, `partial_rotary_factor` (0.5), biases per `attention_bias`/`mlp_bias` | partial rope | `nemotron` |
| `StableLmForCausalLM` | LayerNorm with bias, `partial_rotary_factor` (0.25), `use_qkv_bias`, `qk_layernorm` (per-head **separate** LayerNorms without bias), `use_parallel_residual` (one shared LN) | parallel residual | `stablelm`, `stablelm_parallel` |
| `Starcoder2ForCausalLM` | LayerNorm+bias (`norm_epsilon`), `use_bias` (all projections), plain GELU-tanh MLP, `sliding_window`; legacy `mlp_type`/`norm_type` accepted only at `default`/`layer_norm` | window | `starcoder2` |
| `OlmoForCausalLM` | **non-parametric** LayerNorm (eps fixed 1e-5), `clip_qkv` (clamp q,k,v) | `Clamp` | `olmo` |
| `Olmo2ForCausalLM` | post-norms only; RMS QK-norm over the **whole** q/k projection (`[H·d]`, `[KV·d]`) | — | `olmo2` |
| `CohereForCausalLM` | bias-free LayerNorm, ONE norm feeding a **parallel** attention+MLP, **interleaved** (GPT-J) rope, `logit_scale` (× logits), `use_qk_norm` (per-head separate LayerNorm), `rope_theta` default **500000** | parallel residual | `cohere` |
| `Cohere2ForCausalLM` | Cohere + `sliding_window` × `sliding_window_pattern`/`layer_types`; **rope only on sliding layers** (globals NoPE); legacy `order_of_interleaved_layers`, `position_embedding_type`, `rotary_pct`, `use_gated_activation`, `use_embedding_sharing` accepted only at the values transformers hard-codes | NoPE, window | `cohere2` |

### 3.2 Gemma (C1, C2)

| Architecture | Math-changing fields | Extra HL ops | Fixture(s) |
| --- | --- | --- | --- |
| `GemmaForCausalLM` | `(1+w)` RMSNorm, embedding × √hidden (HF casts √hidden to the activation dtype; the fp32 reference uses fp32), GeGLU with **`hidden_act`** (transformers 5 ignores `hidden_activation` for Gemma-1; 4.40–4.4x used it — a disagreement is flagged), tied head, `head_dim` (256) | `Scale` | `gemma` |
| `Gemma2ForCausalLM` | Gemma + sandwich norms (pre/post around attention and MLP), `query_pre_attn_scalar` (scale = its −½ power), `attn_logit_softcapping` (tanh cap on scaled scores), `final_logit_softcapping`, `sliding_window` + `layer_types` (default: even layers slide), activation **`hidden_activation`**; legacy `sliding_window_size` must equal `sliding_window` | `Softcap` | `gemma2` |
| `Gemma3ForCausalLM` / `…ForConditionalGeneration` (text) | Gemma-2 wiring, per-head `(1+w)` QK-norm, soft-caps usually null, 5:1 sliding/global (`sliding_window_pattern` or `layer_types`), **two rope tables**: `rope_local_base_freq` (no scaling) on sliding layers, `rope_theta` + `rope_scaling` on global layers (or `rope_parameters` keyed by layer type in 5.x configs); `use_bidirectional_attention` must be false | 2 rope tables | `gemma3`, `gemma3_vlm` |

### 3.3 Phi (C1, C2)

| Architecture | Math-changing fields | Extra HL ops | Fixture(s) |
| --- | --- | --- | --- |
| `PhiForCausalLM` (Phi-1/1.5/2) | LayerNorm+bias, ONE norm → parallel attention+MLP, `partial_rotary_factor` (0.5; Phi-2 0.4), biases everywhere incl. **LM head**, `qk_layernorm` (per-head shared LN with bias), `hidden_act` (gelu_new) | parallel residual | `phi` |
| `Phi3ForCausalLM` (Phi-3/3.5/4/4-mini) | fused `qkv_proj` (`[q|k|v]` rows) and `gate_up_proj` (`[gate|up]`), `partial_rotary_factor` (4-mini 0.75), LongRoPE with **top-level `original_max_position_embeddings`** (long factors used once `pos+1 > original`; `attention_factor = √(1+ln f / ln original)`, f = max/original unless the dict gives `factor`), `sliding_window` on every layer | LongRoPE switch | `phi3_longrope` (decode path) |

### 3.4 Pre-Llama lineages (C1, C2)

| Architecture | Math-changing fields | Extra HL ops | Fixture(s) |
| --- | --- | --- | --- |
| `GPT2LMHeadModel` | learned positions (`n_positions`), `Conv1D` weights stored `[in,out]`, fused `c_attn` `[q|k|v]`, `activation_function`, `n_inner` (4·hidden), `scale_attn_weights`, `scale_attn_by_inverse_layer_idx` (÷(layer+1): per-layer block kinds), tied head; the hub's `gpt2` files lack the `transformer.` prefix (alias) | `PosEmbedding` | `gpt2` |
| `GPTNeoForCausalLM` | learned positions, `attention_types` (global/local alternation), `window_size`, **no 1/√d scaling**, q/k/v without bias, out_proj/MLP with bias | window | `gpt_neo` |
| `GPTNeoXForCausalLM` | per-head interleaved `query_key_value` `[head][q,k,v][d]`, partial rope (`rotary_pct` / `rope_parameters.partial_rotary_factor`, 0.25), `rotary_emb_base`, `use_parallel_residual` (**two** LNs), `attention_bias`, exact GELU | parallel residual | `gpt_neox`, `gpt_neox_seq` |
| `GPTJForCausalLM` | interleaved (`rotate_every_two`) rope on the first `rotary_dim`, θ fixed 10000, one LN → parallel, biased MLP and **LM head**; legacy `rotary`/`scale_attn_weights` must be true | interleaved rope | `gptj` |
| `FalconForCausalLM` / `RWForCausalLM` | `new_decoder_architecture` (kv-group fused qkv `[kv][q…,k,v]`, `num_kv_heads`, `num_ln_in_parallel_attn` 1/2), `multi_query` (`[q heads…, k, v]`), else per-head `[q,k,v]`; `parallel_attn`; `bias`; `alibi` (**added before the 1/√d product, bf16-rounded** — §7); `ffn_hidden_size`; `activation` | ALiBi | `falcon_mq`, `falcon_new`, `falcon_alibi` |
| `GPTBigCodeForCausalLM` | GPT-2 wiring with `nn.Linear`, `multi_query` (MQA `[q|k|v]`) or per-head `[q,k,v]`, learned positions, `scale_attn_weights`; runner/upcast flags inert | MQA | `gpt_bigcode`, `gpt_bigcode_mha` |
| `BloomForCausalLM` | embedding LayerNorm, per-head fused qkv, ALiBi **added after** the 1/√d product (unscaled), GELU-tanh (constant 0.79788456), tied head; Megatron training keys inert; `apply_residual_connection_post_layernorm` must be false | ALiBi | `bloom` |
| `MptForCausalLM` / `MPTForCausalLM` | ALiBi (slopes from `alibi_bias_max`, which the native port pins at 8), bias-free LayerNorm and projections, fused `Wqkv` with `clip_qkv` clamp, `softmax_scale`, exact GELU, 4× MLP; `expansion_ratio`≠4, `no_bias`=false, `logit_scale`, `prefix_lm`, `qk_ln`, non-MHA `attn_type` refused | ALiBi, `Clamp` | `mpt` |
| `OPTForCausalLM` | learned positions at **pos + 2**, `do_layer_norm_before` (false ⇒ post-LN, OPT-350m), `word_embed_proj_dim` (`project_in`/`project_out`), `_remove_final_layer_norm`, `enable_bias`, `layer_norm_elementwise_affine`, `activation_function` (ReLU) | post-LN, projections | `opt`, `opt_postln_proj` |

### 3.5 Mixture of experts (C3, C8)

| Architecture | Math-changing fields | Router semantics | Fixture(s) |
| --- | --- | --- | --- |
| `MixtralForCausalLM` | `num_local_experts`, `num_experts_per_tok`, `sliding_window`; `router_jitter_noise` training-only | softmax over all → top-k → renormalise | `mixtral` |
| `Qwen2MoeForCausalLM` | `num_experts`, `num_experts_per_tok`, `moe_intermediate_size`, `norm_topk_prob`, `shared_expert_intermediate_size` (**sigmoid-gated** shared expert), `decoder_sparse_step`/`mlp_only_layers` (dense layers), `qkv_bias` | softmax → top-k → optional renorm | `qwen2_moe` |
| `Qwen3MoeForCausalLM` | as above, no shared expert, per-head QK-norm | softmax → top-k → optional renorm | `qwen3_moe` |
| `OlmoeForCausalLM` | OLMo-2-style whole-projection QK-norm with pre-norm residual, `clip_qkv`, `norm_topk_prob` | softmax → top-k | `olmoe` |
| `GraniteMoeForCausalLM` | Granite multipliers; fused `input_linear` `[E, 2I, D]` (gate rows first), `output_linear` | **top-k of logits → softmax over the k** | `granitemoe` |
| `DeepseekV2ForCausalLM` | MLA (below); `first_k_dense_replace`, `n_routed_experts`, `n_shared_experts` (one wide shared MLP), `topk_method` (`greedy` / `group_limited_greedy` with `n_group`, `topk_group`), `routed_scaling_factor`; `norm_topk_prob=true` refused (§7); `moe_layer_freq`≠1 refused | softmax; groups scored by **max**; masked experts **0.0**; weights × `routed_scaling_factor`, **never renormalised** (transformers 5 native) | `deepseek_v2`, `deepseek_v2_lite` |
| `DeepseekV3ForCausalLM` (also Kimi-K2, Moonlight) | as V2 + `e_score_correction_bias` (selection only), `scoring_func`=sigmoid, `topk_method`=noaux_tc, `rope_interleave`, `num_nextn_predict_layers` (MTP layers ignored by name) | **sigmoid**; choice = s + bias; groups scored by **top-2 sum**; masked **−∞**; weights = unbiased s, renormalised (+1e-20), × `routed_scaling_factor` | `deepseek_v3` |
| `GptOssForCausalLM` | attention **sinks**, `sliding_window` (128) alternating via `layer_types`, YaRN with `truncate=false`, biased router, experts `gate_up_proj` `[E, D, 2I]` with **interleaved** gate/up columns, `down_proj` `[E, I, D]`, expert biases; clamped SwiGLU (α 1.702, limit 7.0 — hard-coded in 5.17, so `swiglu_limit`≠7 is refused); `experts_per_token` alias | top-k of biased logits → softmax over k | `gpt_oss` |

**MLA (DeepSeek-V2/V3).** `q_lora_rank` (None ⇒ direct `q_proj`, else `q_a_proj → RMSNorm → q_b_proj`),
`kv_lora_rank`, `qk_nope_head_dim`, `qk_rope_head_dim`, `v_head_dim`, `attention_bias` (on the `_a`
projections), rope on the last `qk_rope_head_dim` of each q head and on the single shared rotary key
(interleaved pairs), softmax scale `(nope+rope)^−½ · mscale²` with `mscale = 0.1·mscale_all_dim·ln(factor)+1`
when the rope is not default (`yarn_apply_mscale`). The history is the **compressed latent**
(`kv_lora_rank`) plus the shared rotary key — 576 values/position for V3, as transformers 5 caches it;
per-head keys/values are `kv_b_proj · latent`. The HL `MlaAttention` op is evaluated in the absorbed form
(`q̃ = W_kᵀ q_nope`, context over the latent, then `W_v`), equal to the expanded form (unit test).
`num_key_value_heads` must equal `num_attention_heads`.

### 3.6 Linear attention, SSM, recurrent, hybrid (C4–C7)

| Architecture | Math-changing fields | Extra HL ops | Fixture(s) |
| --- | --- | --- | --- |
| `Qwen3NextForCausalLM` | `layer_types` / `full_attention_interval` (3 GDN : 1 attention), `linear_num_key_heads`, `linear_num_value_heads`, `linear_key_head_dim`, `linear_value_head_dim`, `linear_conv_kernel_dim`, fused `in_proj_qkvz` rows **per key head** `[q dk, k dk, v r·dv, z r·dv]` and `in_proj_ba` `[b r, a r]` (r = nv/nk); zero-centred `(1+w)` RMSNorms; attention with **per-head output gate** (q_proj rows per head `[q, gate]`, output × σ(gate)), per-head QK-norm, `partial_rotary_factor` (0.25); MoE with sigmoid-gated shared expert, `decoder_sparse_step`, `mlp_only_layers`; MTP heads ignored by name | `CausalConv1d`, `L2Norm`, `GatedDelta`, `GatedRmsNorm` | `qwen3_next` |
| `Qwen3_5ForCausalLM`, `Qwen3_5MoeForCausalLM`, `…ForConditionalGeneration` (text) | Qwen3-Next with **split** projections `in_proj_qkv`/`in_proj_z`/`in_proj_b`/`in_proj_a`; dense variant has a plain MLP; MoE variant always renormalises; multimodal rope reduces to plain rope for text-only positions | as above | `qwen3_5`, `qwen3_5_moe`, `qwen3_5_vlm` |
| `JambaForCausalLM` | `attn_layer_period`/`offset` (attention, **NoPE**), else Mamba-1 with `mamba_d_state`, `mamba_d_conv`, `mamba_expand`, `mamba_dt_rank`, `mamba_conv_bias`, `mamba_proj_bias`, and weighted RMSNorms on dt/B/C; `expert_layer_period`/`offset` (MoE: softmax top-k, **not renormalised**) | `SelectiveScan` | `jamba` |
| `MambaForCausalLM` | `state_size`, `expand` (inner = expand·hidden; the -hf `intermediate_size` must agree), `conv_kernel`, `time_step_rank` (`auto` = ⌈hidden/16⌉), `use_bias`, `use_conv_bias`; `n_layer` alias must agree; `rms_norm` must be true | `CausalConv1d`, `SelectiveScan` | `mamba` |
| `FalconMambaForCausalLM` | Mamba + weightless RMS norms (`mixer_rms_eps`) on dt, B, C before `dt_proj` | — | `falcon_mamba` |
| `Mamba2ForCausalLM` | `num_heads`·`head_dim` = expand·hidden, `n_groups` (B/C shared per group), `state_size`, `conv_kernel`, `in_proj` rows `[z | xBC | dt]`, dt = softplus(dt + `dt_bias`), `A_log`, `D`, gated RMSNorm **gate-then-norm over the full width**; `time_step_limit` must be (0, ∞) (§7); `norm_before_gate` must be false | `Ssd`, `GatedRmsNorm` | `mamba2` |
| `RwkvForCausalLM` (RWKV-4) | `attention_hidden_size`, `intermediate_size`, `layer_norm_epsilon`, `rescale_every` (inference: output weights of block i ÷ 2^(i//N), hidden halved after every N-th block), `pre_ln` on block 0 | `TokenShift`, `Lerp`, `Wkv4` | `rwkv` |

### 3.7 VLM text decoders

`Gemma3ForConditionalGeneration`, `LlavaForConditionalGeneration` (Llama/Mistral/Qwen2 text configs),
`Mistral3ForConditionalGeneration` and `Qwen3_5(Moe)ForConditionalGeneration` are lowered **as their
text decoder alone**, for text-only prompts: with no image the decoder computes exactly the text
model's function. Both weight layouts on the hub are accepted (`language_model.model.*`, ≤ 4.51, and
`model.language_model.*`), and the vision tower / projector tensors are ignored by name. **Embedding
tying follows the composite config** (transformers 5 reads `tie_word_embeddings` on the VLM, not on its
text config), with each class's own default: Gemma-3 and Mistral-3 default to **tied**, LLaVA to
untied-or-the-text-config's-flag, Qwen3.5 to untied — so a Mistral-3 config without a root flag ties
its head in 5.17 (the `mistral3_vlm` fixture confirms it). Out of scope: anything
whose text decoder reads vision states (Mllama cross-attention layers), prefix-LM attention over the
prompt (PaliGemma), and audio models.

### 3.8 RoPE variants (all architectures)

`rope_theta` + `rope_scaling` (4.x) or `rope_parameters` (5.x; flat, or keyed by layer type), with the
exact transformers formulas in float32: `default`; `linear` (inv_freq ÷ factor); `dynamic` (NTK:
base·((f·s/max) − (f−1))^(d/(d−2)) with s = `pos+1` once past `max_position_embeddings`); `yarn`
(ramp between the floor/ceil correction dims unless `truncate=false`; `attention_factor` or
`mscale`/`mscale_all_dim` ratio or 0.1·ln f+1 multiplies cos and sin; an explicit `factor` wins over
max/original as in 5.x); `llama3` (low/high wavelength bands, smooth blend); `longrope`/`su`
(short/long factor lists, switch at `pos+1 > original_max`, `attention_factor` √(1+ln f/ln original));
`mrope` (text-only ⇒ plain). Partial rotary (`partial_rotary_factor`, `rotary_pct`, `rotary_dim`),
`rotate_half` vs interleaved pairs, and a rotation offset inside the head (MLA) are op attributes.
Any other rope type, or any unknown key inside the rope dict, is refused.

### 3.9 Fixture agreement (transformers 5.17 vs the f32 reference, 10 positions)

max |Δlogit| (scale = max |logit|); "decode" = one-token-at-a-time reference.

| fixture | max \|Δ\| | fixture | max \|Δ\| | fixture | max \|Δ\| |
| --- | --- | --- | --- | --- | --- |
| llama | 1.4e-6 (3.6) | gpt_bigcode | 6.4e-6 (21.5) | deepseek_v2 | 1.3e-6 (4.0) |
| llama_linear_tied | 5.0e-6 (25.1) | gpt_bigcode_mha | 5.7e-6 (20.4) | deepseek_v2_lite | 9.5e-7 (3.2) |
| mistral_window | 9.5e-7 (3.3) | olmo | 7.2e-7 (3.8) | deepseek_v3 | 7.2e-7 (3.4) |
| qwen2_dynamic (decode) | 5.3e-6 (33.1) | olmo2 | 1.1e-6 (3.3) | gpt_oss | 1.3e-6 (3.3) |
| qwen2_sliding | 1.0e-6 (3.0) | cohere | 1.9e-6 (10.2) | qwen3_next | 3.7e-6 (3.8) |
| qwen3_yarn | 9.5e-7 (3.2) | cohere2 | 4.0e-7 (1.6) | qwen3_5 | 2.6e-6 (3.0) |
| gemma | 3.8e-6 (38.6) | granite | 2.4e-7 (1.2) | qwen3_5_moe | 2.3e-6 (3.7) |
| gemma2 | 1.4e-6 (3.0) | nemotron | 2.4e-6 (7.2) | qwen3_5_vlm | 2.8e-5 (3.3) |
| gemma3 | 3.8e-6 (32.5) | exaone4 | 2.1e-6 (3.4) | jamba | 2.7e-6 (3.8) |
| gemma3_vlm | 3.8e-6 (33.4) | smollm3 | 6.3e-6 (19.4) | mamba | 5.7e-6 (34.6) |
| phi | 8.2e-7 (3.6) | bloom | 6.9e-6 (18.8) | falcon_mamba | 7.6e-6 (36.3) |
| phi3_longrope (decode) | 7.8e-7 (3.4) | mpt | 6.7e-6 (20.6) | mamba2 | 9.5e-7 (2.9) |
| gpt2 | 4.3e-6 (18.9) | opt | 3.8e-6 (19.9) | rwkv | 1.6e-6 (3.5) |
| gpt_neo | 5.7e-6 (17.2) | opt_postln_proj | 4.3e-6 (15.8) | mixtral | 1.1e-6 (3.1) |
| gpt_neox | 7.2e-7 (3.0) | stablelm | 1.0e-6 (3.1) | qwen2_moe | 9.5e-7 (3.5) |
| gpt_neox_seq | 8.3e-7 (3.3) | stablelm_parallel | 6.3e-7 (3.7) | qwen3_moe | 7.2e-7 (3.0) |
| gptj | 1.1e-6 (2.8) | starcoder2 | 3.8e-6 (20.9) | olmoe | 6.7e-7 (3.6) |
| falcon_mq | 5.5e-6 (18.9) | falcon_new | 5.3e-6 (17.4) | granitemoe | 1.3e-7 (1.0) |
| falcon_alibi | 6.9e-6 (21.8) | llava | 8.3e-7 (3.1) | mistral3_vlm | 4.8e-6 (23.5) |

## 4. Out of scope for v1, and not yet modelled

**Out of scope (by construction of a one-position decoder program):**

- **Encoder-only** (BERT, RoBERTa, DeBERTa, ModernBERT): bidirectional attention over the whole
  input; no next-token logits at a position, so there is no causal step to scan.
- **Encoder–decoder** (T5, BART, Whisper, Florence-2): the decoder cross-attends to an encoder
  output computed from the whole input; the program would need a second input stream.
- **Vision/audio towers** (SigLIP, CLIP, Whisper encoders): continuous inputs, bidirectional; a
  VLM's text decoder alone is in scope (§3.7).

**Decoders in transformers 5.17 that are refused today** (`NOT_LOWERABLE` naming the reason; each is
a new adapter (a data file) over existing HL ops unless stated):

| Architecture | Why not yet |
| --- | --- |
| `Llama4ForCausalLM`/`…ConditionalGeneration` | chunked attention (a new window shape), NoPE layers with attention temperature tuning, Llama-4 MoE — needs a `chunked` Hist read |
| `Gemma3nForConditionalGeneration` | AltUp, Laurel, per-layer embeddings, activation sparsity (a top-k-by-value gate) |
| `Glm4ForCausalLM`, `GlmForCausalLM`, `Glm4MoeForCausalLM`, ChatGLM (remote) | not modelled yet; GLM-4.5 is DeepSeek-V3-style routing with partial rope — mostly existing ops |
| `Olmo3ForCausalLM`, `NemotronHForCausalLM`, `FalconH1ForCausalLM`, `Zamba2ForCausalLM`, `BambaForCausalLM` | hybrids of existing ops (Mamba2 + attention); config parsers not written; Falcon-H1 adds µP multipliers |
| `PhimoeForCausalLM` | sparsemixer routing (a second top-1 over a masked, jittered distribution) |
| `DbrxForCausalLM`, `JetMoeForCausalLM`, `Ernie4_5*`, `HunYuan*`, `MiniMax*`, `Lfm2*`, `RecurrentGemma`, `xLSTM`, `DiffLlama`, `BitNet`, … | long tail; each needs its parser, some a new op (xLSTM's exponential gating, BitNet's ternary weights) |
| `Rwkv5ForCausalLM`, `Rwkv6ForCausalLM` (remote), `RWKV7ForCausalLM` (flash-linear-attention) | not in transformers 5.17: the group-norm eps, rescale and LoRA widths cannot be checked offline. The **recurrences are HL ops** (`Wkv6` covers RWKV-5/6 with a per-channel decay, `Wkv7` the RWKV-7 generalised delta rule), tested against their closed forms |
| Pre-quantised checkpoints (GPTQ, AWQ, fp8, mxfp4 …) | the lowerer quantises; it does not inherit a quantisation. Lower from the BF16/F16/F32 export (e.g. gpt-oss, DeepSeek-V3) |

## 5. The HL graph, for the IR lane

`HlProgram` is organised as `TirProgramV1` (RFC §4.1): `blocks[pre]` embeds, `blocks[post]` produces
logits, every other block is a **layer kind**; `schedule[l]` names the block of layer `l` (equal layer
specs share a block, so Gemma-3 has 2 kinds, Qwen3-Next 2, Jamba 3, GPT-2 with inverse-layer scaling
one per layer). Blocks exchange the residual stream as a carry. Params are per-layer or global and
are named by **role** (`attn.q.w [H·d, D]`, `gdn.A [vh]` = the negative decay rate, `moe.experts.gate
[E, I, D]` …). States are `Fixed` (conv windows `[K−1, C]`, token shift, GDN `S [vh, dk, dv]`, Mamba
`h [I, N]`, SSD `h [H, P, N]`, RWKV num/den/max) or `Hist { window }` (K/V rows, MLA latent + rotary key).
One program step computes one position.

**Ops** (float semantics; composites are expanded by the TIR library in Gate 2):

| Op | Semantics / attributes |
| --- | --- |
| `Embedding`, `PosEmbedding{offset}` | row gather by token / by `pos+offset` |
| `Linear{bias}` | `W x + b`, W `[out, in]` (fused checkpoint tensors are sliced at load time) |
| `Add`, `Sub`, `Mul`, `Scale{c}`, `Clamp`, `Lerp` | elementwise (a 1-element operand broadcasts) |
| `Act(f)` | SiLU, GELU (erf), GELU-tanh, quick-GELU, ReLU, ReLU², sigmoid, tanh, softplus (torch threshold 20), identity |
| `Softcap{cap}` | `tanh(x/cap)·cap` (scores, logits) |
| `Norm{kind, eps, gain, bias, groups}` | RMS or LayerNorm per group; gain `W` or `1+W`; gain shape full / shared by groups / per group |
| `GatedRmsNorm{groups, gate_first}` | Mamba2: `norm(x·silu(z))·w`; Qwen3-Next: `norm(x)·w·silu(z)` |
| `L2Norm{groups, eps}` | `x·rsqrt(Σx²+eps)` (FLA) |
| `Rope{heads, head_dim, rotary_dim, offset, style, table}` | pair rotation by a per-position table (`style` = half / interleaved) |
| `HistAppend`, `Attention{…}` | softmax over the last `min(pos+1, window)` rows; GQA grouping `h / (H/KV)`; scale; score soft-cap; ALiBi slopes (scaled or not, bf16 or not); sinks |
| `MlaAttention{…}` | latent attention (absorbed form) |
| `CausalConv1d{channels, kernel, bias, act}` | depthwise over the last `kernel` inputs |
| `GatedDelta{k_heads, v_heads, dk, dv, head_map, q_scale}` | `S ← S·e^g; S += k (β(v − Sᵀk))ᵀ; o = Sᵀq·q_scale` |
| `SelectiveScan{inner, state}` | Mamba-1 step, `h ← e^{dt·A} h + dt·B·x; y = C·h + D x` |
| `Ssd{heads, head_dim, groups, state}` | Mamba-2 step, scalar decay per head, grouped B/C |
| `TokenShift`, `Wkv4`, `Wkv6`, `Wkv7`, `DecayExpNegExp` | RWKV recurrences (§4) |
| `Route{router, experts, top_k}` | two outputs: ids (index order, ties → lowest index), weights |
| `MoeExperts{top_k, act, glu, bias}` | gather experts by id, GLU (or gpt-oss clamped SwiGLU), weighted sum |
| `Slice`, `Concat`, `Zeros` | structure |

**Sites.** Every node that the integer program will requantise carries a site name (`attn.q`,
`attn.q_rope`, `attn.ctx`, `gdn.log_decay`, `moe.route` …); composites report internal sub-sites
(`attn.ctx.scores`, `attn.ctx.probs`, `gdn.core.state`, `mamba2.scan.state`, `moe.routed.hidden`,
`moe.route.logits`). `float_ref::Session::with_site_stats` records absmax / rms / count per
`L{layer}.{site}` — what Gate 2 calibrates from. A test checks every site of every layer records.

**Frontend neutrality.** HF config keys, tensor names and fused layouts live only in the adapters and the schema reader
(`ModelSpec::hf: HfStorage`) and `hf_weights` (param ← tensor expression: row slices of fused tensors,
per-head strides, Conv1D transposes, expert stacking, `−exp(A_log)`, RWKV rescale). The HL builder
never reads `HfStorage`; a test asserts no HF tensor name appears in a serialised HL program. A GGUF
or ONNX importer fills its own storage description and produces the same graph.

## 6. What the IR lane should know: decompositions that look hard or lossy

1. **Activations on wide inputs.** After a `Linear` the activation input is an i32-range value; a
   256/65,536-entry table needs a requantisation to i8/i16 first. Fine for SiLU/GELU (asymptotically
   linear or zero: clamp the table domain, e.g. ±8, identity/zero outside) but it is a lossy site
   before every activation. GELU-erf vs GELU-tanh differ by < 3e-3 — the table makes them equal cost.
2. **softplus → exp chains that define decay rates.** Mamba `exp(dt·A)` per (channel, state), GDN
   `exp(A·softplus(a + dt_bias))`, SSD `exp(dt·A)` per head: decays close to 1 (1 − 1e-3) must be
   represented well or long-range memory collapses. softplus's input is unbounded (identity above 20,
   `exp(x)` far below 0), and the product `dt·A` of two quantised values sits inside the exponent.
   Mamba-1 evaluates `inner × state` exponentials per token (131K per Jamba layer) — transcendental
   cost dominates there.
3. **Double exponentials.** RWKV-6/7 decay `exp(−exp(w))` with data-dependent `w`: an `IntExp` of an
   `IntExp`, or a table over a clamped domain. (RWKV-4's `−exp(time_decay)` is a param, folded at load.)
4. **Long-context RoPE.** A per-position table of `rotary/2 × history_bound` cos/sin pairs is large
   (64 MiB at 2^18 positions × 64 pairs × cos+sin × i16) — not a 64 KiB const. The table must be a pinned artifact param
   or generated by a primitive: phase = `pos · inv_freq` in fixed point (i64 product, range reduction
   mod 2π), then a sin/cos table. YaRN, Llama-3, linear and LongRoPE only change `inv_freq` constants
   and a cos/sin multiplier (fold into the table); **dynamic NTK needs a per-position base**
   `θ·((f·(pos+1)/L) − (f−1))^(d/(d−2))`, i.e. `pow` per position (IntLn + IntExp) — or refuse dynamic
   NTK in v1 (few models use it; InternLM2 does).
5. **LayerNorm mean/variance.** Two reductions and a division by `n` where `n` is often not a power
   of two (2880, 4544, 5120, 7168): `DivConst` on the mean, then Σ(x − mean)² in i64, then `IntRsqrt`.
   `E[x²] − E[x]²` would cancel catastrophically; the two-pass form is the right composite.
   `(1+w)` gains fold into the weight at quantisation, losslessly.
6. **Softmax variants.** Two-pass (max, then exact sums — RFC §5.4) plus: an extra sink logit in the
   denominator only (gpt-oss); tanh soft-capping of scores before the max (Gemma-2: `IntTanh` or a
   clamped table); ALiBi as `slope·j` (an integer multiply by a fixed-point slope; Falcon's bf16
   rounding of that product is an HF precision artifact, irrelevant to fidelity).
7. **Divisions.** Router renormalisation `w/Σw` (+1e-20), RWKV-4 `num/den`, attention `1/Σ`: `IntRecip`.
8. **Selection.** Group-limited routing needs top-2-sum group scores → `TopK` over groups → a mask
   (`Select` with −∞, or 0 for V2) → `TopK` over experts; weights are gathered from a DIFFERENT tensor
   than the choice scores (V3: unbiased sigmoid). Softmax-over-selected (gpt-oss, GraniteMoE) runs the
   softmax on k values only. All selections must be commit points (PALW-TIR-11).
9. **MLA absorbed form** chains two contractions (W_kᵀ q over `nope`, then over the latent `kv_lora`):
   a requantisation site between them, and the latent (512) is the history row — 576 values/position
   instead of 40,960 for expanded V3 keys/values.
10. **The gated delta rule** keeps a `[vh, dk, dv]` state per layer (Qwen3-Next: 32·128·128 = 524K
    values/layer, 18.9M over its 36 GDN layers) updated by rank-1 outer products; its boundedness rests on the
    L2-normalised key and β ∈ (0, 1) (the contraction argument RFC §4.2 cites), with `StateWrite`
    saturation making the analysis total.

## 7. Where the "HF float original" is itself ambiguous

The RFC's freeze criterion 5 compares against "the family's Hugging Face float reference". In
transformers 5.17 that reference is not unique:

- **Attention backend.** The default `sdpa` path **drops Gemma-2's attention soft-cap** (the softcap
  kwarg never reaches SDPA); eager applies it. Falcon's **eager** ALiBi path adds the bias twice (once
  in the attention, once through the merged mask) while `sdpa` is correct. The lowerer follows the
  models' defining math: eager everywhere except Falcon-ALiBi (sdpa).
- **Prefill vs decode.** Dynamic NTK and LongRoPE compute frequencies from the forward call's
  sequence length: a single prefill of length T rotates every position with T's frequencies, while
  token-by-token decoding rotates position p with p+1's. The lowerer implements the per-position
  (decode) semantics; fixtures for those two record the decode logits.
- **In-place state.** RWKV-4 rescales its weights in place on the first inference forward, so a model
  object that has run is not the file on disk (the fixture generator saves first, then reloads).
- **Native vs remote code.** DeepSeek-V2: transformers 5 multiplies routing weights by
  `routed_scaling_factor` and never renormalises; the checkpoint's remote code renormalises when
  `norm_topk_prob` is set — refused for true. DeepSeek-V3: 5.x masks non-selected groups with −∞, the
  remote code with 0.0 (followed: 5.x). `moe_layer_freq` is ignored by 5.x and honoured by the remote
  code — refused unless 1.
- **Hard-coded constants.** gpt-oss's SwiGLU limit is 7.0 regardless of `swiglu_limit`; MPT's ALiBi
  uses 8 regardless of `alibi_bias_max`; Mamba2 clamps `dt` to `time_step_limit` in the chunked path
  but not in the one-token path. Configs that would make the paths or the intent disagree are refused.
- **Gemma-1 activation.** 5.x reads `hidden_act`, 4.40–4.4x read `hidden_activation`; google/gemma-7b
  carries both with different values (exact vs tanh GELU, < 3e-3 apart). 5.x is followed and the
  disagreement is flagged in the spec.
- **GDN head map.** HF Qwen3-Next/3.5 use `repeat_interleave`: value head `vh` reads key head
  `vh / (nv/nk)` (**grouping**). The live Q36 kernel hard-codes `vh % nk` (**tiling**) on GGUF weights.
  The two compute the same function exactly when the value-head-indexed tensors are permuted by
  `vh ↦ (vh % nk)·(nv/nk) + vh / nk` (unit test `gdn_value_heads_group_in_hf_and_tile_in_the_live_kernel`),
  which is what a GGUF writer that reorders V heads for a broadcast-friendly layout produces. So the
  live kernel is consistent with its GGUF only if that GGUF was reordered — worth checking in Phase 0
  (I could not inspect llama.cpp offline). The HL op carries `head_map` explicitly; an HF lowering is
  always `Group`.
- **Top-k ties.** `torch.topk` does not specify ties; the lowerer breaks them to the lowest index.
  Random logits never tie; quantised router logits can — a fidelity-only effect.

Proposal: criterion 5 should pin **(transformers version, eager attention, per-position decode
semantics, fp32 weights as loaded from the checkpoint)**, plus the per-family exceptions above.

## 8. Fields and semantics I was unsure of

Confirmed by fixtures (transformers 5.17): every architecture in §3 except the three remote-code
llama-likes. Still unsure:

- **InternLM2 (remote):** the `wqkv` row layout `[kv][q_0..q_{g−1},k,v][d]` (einops rearrange in the
  remote file), the class default `bias=true`, and its dynamic-NTK growth rule. No fixture offline.
- **MiniCPM (remote):** default `tie_word_embeddings=true`; `scale_depth/√L` on both branches;
  `dim_model_base` on the head input. No fixture offline.
- **EXAONE-3 (remote):** tensor names (`transformer.h.{i}.attn.attention.*`, `mlp.c_fc_0/1`) and RMSNorm.
- **transformers 4.x vs 5.x differences** a 4.x-era fidelity run would hit: YaRN's `factor` (4.4x–4.5x
  replaced it by max/original when `original_max_position_embeddings` was given; 5.x keeps an explicit
  factor — matters for Qwen3-with-YaRN configs), LongRoPE's factor source, Gemma-1's activation key.
- **Real-checkpoint configs** in `tests/configs/real/` are written from memory of the published files
  (values such as Phi-3.5's long/short factor lists are placeholders of the right length); the
  legacy keys they carry (`sliding_window_size`, `norm_before_gate`, `use_mrope`, Cohere2's
  `position_embedding_type` …) are my recollection of which keys appear. A real config with a key I
  did not anticipate is refused, which is the safe failure.
- **Mamba2 gated norm grouping:** transformers normalises over the full inner width; mamba_ssm's
  `RMSNormGated` uses `group_size = d_ssm / ngroups`. The fixture confirms transformers; whether the
  original Codestral weights expect the grouped norm is a fidelity question for Phase A.
- **RWKV-5/6/7:** refused; the `Wkv6`/`Wkv7` op semantics follow the published reference recurrences
  (RWKV-LM demos, flash-linear-attention's `-kk`/`kk·a` form for v7) and are tested only against their
  own closed forms.

## 9. Suggested changes to RFC-0002

1. **§1.2 criterion 5**: pin the reference as in §7 (version, eager, per-position decode), and name
   the reference for families transformers does not ship (RWKV-5/6/7: flash-linear-attention or the
   RWKV-LM reference at a pinned revision).
2. **§4.1 consts ≤ 64 KiB**: rope tables for long contexts do not fit; either tables are artifact
   params (hashed like weights) or v1 needs a phase → sin/cos primitive. Dynamic NTK additionally
   needs a per-position `pow` — or refuse it in v1.
3. **§3.3 composites**: add MLA (absorbed latent attention), attention sinks, score soft-capping and
   the clamped SwiGLU to the library list; add "gather weights from a different tensor than the
   selection scores" to the routing templates (DeepSeek-V3).
4. **§4.2 states**: MLA's Hist row is the latent (576 values/pos for V3), not per-head K/V; sliding
   windows are exactly `HistRead(window)` with `kv > q − window` (verified for Mistral, Gemma-2/3,
   Qwen2, StarCoder2, Cohere2, gpt-oss, GPT-Neo, Phi-3).
5. **§1.1 corpus**: add Qwen3.5 (split GDN projections, the likely "Qwen3.6" family), gpt-oss (sinks,
   clamped SwiGLU, softmax-over-top-k), and OPT-350m-style post-LN; C4's ratios (16:32, 16:48 …) are
   HL shape data here too.
6. **§8 verdicts**: add `LOWERABLE_UNVERIFIED` (lowered, no reference fixture) so remote-code families
   are visibly weaker than fixture-backed ones.
7. **Phase 0 (GDN)**: check whether the GGUF the live kernel reads has tiled V heads (§7) before
   "fixing" `vh % k_heads`; the IR's `head_map` makes both explicit.

## 10. Gate 2a — the lowering, the artifact, fidelity

`misaka_palw_tir_lower::lower` expands the HL graph into a `TirProgramV1` whose structure depends on
the architecture only; every number is a param filled per layer from the checkpoint and the
calibration statistics (`lower::fill`), so a recalibration moves the artifact, never the program.

| value | format |
| --- | --- |
| residual stream (the carry) | `i32`, ONE scale for the program (first-token massive activations need no position-0 lane) |
| matmul inputs, op boundaries | A16: `i16` codes `±32767`, static scale per site and layer (headroom 2× over the calibrated absmax) |
| weights | W8: `i8` per output row. A table gathered by row (an embedding, a learned position table) is `i16` per row, and a head tied to it reads the same codes (`i16 × i16`, exact in `i64`). `(m, s, z)` are three typed per-channel params (`i64`, `i8`, `i64`) |
| norm unit rows, softmax probabilities, decays, gates | Q24 `i32` |
| attention and router logits | Q14 `i32` (±131,072: Qwen2.5's layer-0 logits reach 24,000) |
| the text program's logits | Q24 `i32`: natural-log units × 2^24 whatever the model (`LOGITS_Q24_V1`, §22; `|logit| < 128`) |
| activations | `Table(x; T)`: 65,536 `i16` entries per site and layer, the float function rounded on the code grid |

**Outlier channels.** A value read only by projections splits off its `k ≤ 16` channels with the
largest calibrated absmax; each gets its own activation scale and the projection routes them through
`i32` fixed-point columns (`acc·2^f + acco`, exact), the rest stays W8 with those columns zeroed —
the static int8 "outlier decomposition". Qwen2.5-1.5B's first-token MLP channels reach 1000× every
other position; without the split, one static scale leaves ordinary tokens ~5 bits (KL 2.9 → 0.002).

**The library.** The lowering emits `tir_library_v1` templates wherever one gives the values the
lowering needs: `narrow` (every narrowing; three nodes without a zero term), `rms_unit_q24` and
`l2_unit_q15` (the 21- and 17-node unit rows), `attention` (Q14 scores, the softmax lifted by
`up_bits` 10, sinks), `softmax_shifted`, `softmax_with_sink`, `int_sigmoid`, `int_recip`,
`rope_pairs`, `rope_angles_two_level`, `causal_conv` (taps stored window-major), `grouped_topk`,
`selection_bias`, `renormalize_recip`, `moe_combine_q36`, `gdn_step_q36`, `decay_q36`,
`softplus_q36` and `exp_refined_q36`. The lean narrowing and unit rows and the renormalisation's
`[0, 2^25]` clamp were added to the library for this lowering (tir/core `23c6d4efd`): with the
earlier 5-, 39- and 27-node forms Qwen3.5-MoE, Qwen3-Next and DeepSeek-V3 did not fit NF-12's 512
nodes a block at all. Switching to them left every program's bytes unchanged (the renormalisation's
shift is typed `i128` there: the MoE programs' bytes moved, their values did not), and switching
attention left all 57 fixtures' metrics identical to every printed digit. The lowering keeps its own
composite only where the function differs:

| the lowering's own | why not the library's |
| --- | --- |
| soft-capping | `cap·tanh(s/cap)` formed in Q24 from the score's narrowing (no division), then narrowed to Q14; `softcap_q24` divides by the cap. |
| ALiBi | Falcon's bfloat16-rounded `slope·j` table, and `slope·(j − pos)` (far keys stay small instead of relying on the maximum subtraction). |
| attention with a value width unlike the query's | the template reads one `head_dim` for both. |
| MLA | the same contractions as `mla_absorbed` (the value half stored per row, not transposed), with commit points on the absorbed query and the latent context the template does not expose. |
| LayerNorm | inputs of any width: the exact centring `n·x − Σx` is shifted into `i32` (`layer_norm_exact` needs `n·x` to fit) and normed by the 21-node unit row with one `i64` eps. |
| partial rotary | YaRN's attention factor scales the pass-through lanes too; `rope_partial` passes them unchanged. |
| RWKV-4 WKV, the selective scan | half-away rounding and a Q16 denominator (WKV); `D` folded into its narrowing and one decay form for Mamba-1 and -2 (the scan) — the same recurrences as `rwkv4_step`, `mamba1_step`/`mamba2_step`, rounded differently. |

**Node economy.** When a block still does not fit (the gated-delta + MoE layers of Qwen3.5-MoE and
Qwen3-Next), the lowering drops the outlier splits of the most-read values and retries;
`Lowered::budget_fallbacks` names such blocks (today only those two, at 467 nodes with splits for
values read by at most three projections).

**History windows (NF-8).** A node output has at most `2^28` elements at the worst case, so a
history row of more than 1,024 lanes does not fit a `2^18` window. The lowering caps a block's
window at the largest power of two its widest history row fits (a 4,096-lane MHA row: `2^16`):
up to that window the program is the model, and a layout whose `max_context` stays inside it never
sees the cap. Before the cap, 17 real configurations (every MHA 7B, Phi-2/3.5/4, Gemma-7B/2-9B, OLMo,
Pythia …) did not lower at all.

**Admission.** `tir_admit_v1` (tir/core, spec 04b §10.3) admits every lowered program the tools
produce: all 57 fixtures and every lowerable real configuration at the legacy court's ceilings
(`tile_len` 64, `h_chunk` 64; `tests/admission.rs`), except DeepSeek-V3, refused by name (2.26e12
MACs a position at a `2^18` window, past `2^40`). The worst terminal tile is Falcon-40B's 2.6 Mi
MACs of 16 Mi, so no commit point is added for cone size. `palw-tir-check` prints the admission
(costs, windows, cones, `C`, cone work, or the refusal), and `palw-class check-architecture`'s IR
mode takes its verdict from it under the network's `palw_tir_v1` ceilings.

**Artifact.** `artifact::write` writes a `PALWTIR1` container (`misaka-palw-tir-artifact`, Phase F
F3): the program, its layout and tokenizer id, and every param tensor typed little-endian in
inventory order, 64-byte aligned; the consensus inventory root (`palw_tir_artifact_v1`) hashes it
front to back. (Gate 2a's crate-local `PALWTIRA` grew into it.)

**Fidelity on the tiny fixtures** (`tests/fidelity_tiny.rs`: calibration 6×32 random tokens, evaluation
3×24 others; reference evaluator vs the f32 reference, which matches transformers 5.17 to ~1e-6):
all 57 fixtures lower, pass the 04b §7 range analysis and agree at top-1 ≥ 0.90 (mean 0.98), mean
KL median 1.4e-4 (max 0.012, Qwen3.5-MoE), |perplexity Δ| ≤ 2.1 %:

| group | fixtures | top-1 | mean KL (nats) |
| --- | --- | --- | --- |
| dense (Llama, Mistral, Qwen2/3, Gemma 1/2/3, Phi, GPT-2/J/Neo/NeoX, Falcon incl. ALiBi, BLOOM, MPT, OPT, StarCoder2, StableLM, OLMo 1/2, Cohere 1/2, Nemotron, EXAONE-4, Granite, SmolLM3, VLM text decoders) | 39 | 0.90–1.00 | ≤ 2.3e-3 |
| MoE (Mixtral, Qwen2/3-MoE, OLMoE, GraniteMoE, gpt-oss, DeepSeek-V2/V2-Lite/V3 with MLA and group-limited or sigmoid routing) | 9 | 0.97–1.00 | ≤ 5.2e-4 |
| recurrent / hybrid (Qwen3-Next, Qwen3.5 dense/MoE/VL — gated delta; Mamba, FalconMamba, Mamba2, Jamba; RWKV-4) | 9 | 0.93–1.00 | ≤ 1.2e-2 |

**Qwen2.5-1.5B-Instruct** (the on-disk checkpoint, BF16 safetensors; no download). Calibration:
8 sequences × 128 tokens of repository text (`docs/archival.md`, `docs/crescendo-guide.md`,
`docs/connecting-ethereum-tooling.md`, `docs/node-liveness-probe.md`, two chunks each, tokenised
offline by `tools/tokenize_docs.py`), streamed layer by layer: 300 s, 1.7 GB peak. Artifact:
1,636 tensors, 1,527.9 MiB, a `PALWTIR1` container bound to the checkpoint's `tokenizer.json` (the
legacy tokenizer commitment); `palw-class manifest` streams its inventory root over 1,445,615
leaves in ~6 s. Residual scale 1.348e-5, logit scale 6.052e-8. Program:
312 nodes, 13,903 bytes, one `attn+mlp` block of 270 nodes ×28, admitted (2.41e10 MACs a
position at `W = 2^18`, 17 cones, the worst tile 0.57 Mi MACs). Evaluation on held-out text
(the first 128 tokens of `docs/README.md` and of `docs/evm-differences-from-ethereum.md`), the
integer program on the reference evaluator against the f32 reference:

| positions | top-1 | mean KL (max) | perplexity float → integer |
| --- | --- | --- | --- |
| 256 (2 × 128) | 0.973 | 0.00138 (0.0063) | 60.37 → 60.69 (+0.53 %) |

The 256-position run used the program as it was before the lean norms (388 nodes, the same values
by construction); the final program (312 nodes, after the tir/core merge and the library switch)
reproduces the 16-position check of `docs/README.md` exactly — top-1 1.000, KL 0.00161 (max
0.0057), perplexity 185.51 → 189.46 — and writes the tensors whose inventory root the manifest
above records.

The reference evaluator (every value an `i128`) takes ~20 s a position at 1.5B and peaks at
7.4 GB with a 128-row history; the typed backend (tir/node, F9) is the fast path.

## 11. Reproducing

```sh
export CARGO_TARGET_DIR=…/tir-lower-target CARGO_BUILD_JOBS=3
cargo test --release -p misaka-palw-tir-lower  # HF fixtures are skipped if absent
HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 \
  …/tir-venv/bin/python misaka-palw-tir-lower/tools/gen_hf_fixtures.py [name …]   # regenerate
cargo run -p misaka-palw-tir-lower --bin palw-tir-check -- \
  --config misaka-palw-tir-lower/tests/configs/real/qwen3-next-80b-a3b-instruct.json [--tile-len 64 --h-chunk 64]
# The real checkpoint (one process at a time; ~7.4 GB peak):
HF_HUB_OFFLINE=1 …/tir-venv/bin/python misaka-palw-tir-lower/tools/tokenize_docs.py …   # calib / eval token files
cargo run --release -p misaka-palw-tir-lower --bin palw-tir-fidelity -- ~/Downloads/Qwen2.5-1.5B-Instruct \
  --calib qwen25-calib.json --eval qwen25-eval.json --eval-seqs 2 --positions 128 \
  --artifact-out qwen25.palwtir --tir-out qwen25.tir --json
# The same on the typed backend (byte-identical, ~300x faster at 1.5B), with a reference cross-check
# and, for recurrent models, the 4,096-position drift. A recurrent model must be calibrated on a
# sequence as long as the evaluated context (freeze-v1 §5.2), so the drift run takes a calibration
# file with one held-out 4,096-token sequence; --max-window N lowers for a layout's shorter context
# (the same option on palw-tir-check):
cargo run --release -p misaka-palw-tir-lower --bin palw-tir-fidelity -- <checkpoint> \
  --calib calib.json --eval eval.json --exec --cross-check 2 [--calib calib-long.json --eval drift.json --positions 4096 --drift]
# The legacy dense row and its IR program, same logits and rows (D-F1, offline):
cargo run --release -p misaka-palw-base0 --bin palw-a16-to-tir -- --artifact <512-wide.palwart> --respan 8192 --out genesis-8k.palwtir
cargo run --release -p misaka-palw-sdk --bin palw-tir-equiv -- --network testnet-12 \
  --artifact <512-wide.palwart> --respan --tir genesis-8k.palwtir --prompts 32
```

## 12. Encoders (RFC-0003 Part II.3, the Embedding profile)

On `rfc3/lower`, over `rfc3/impl`'s `TirProgramV2` and `TirPipelineV1`. Programs are written with
the version-1 builder, and future inputs are declared as global params. They are then lifted by
`TirProgramV2::from_v1_lifting_params`, the interface agreed with the RFC-0003 agent. The primitive
set is unchanged.

| class shape | architectures | program | pipeline stage | output |
| --- | --- | --- | --- | --- |
| causal encoder | `CLIPTextModel`, `CLIPTextModelWithProjection` | the decoder lowering; `post` is final LN, then `text_projection` (`encoder::causal_v2`) | `TokenCount` over `bos ‖ prompt ‖ eos`, `max_trip` = the learned positions | `Final [1, d]`: the row at the first `eos` (a `Rows [d]` variant feeds a denoiser) |
| last-token embedder | any lowerable decoder via `encoder::as_last_token_embedder` (Qwen3-Embedding's shape) | the decoder lowering without the head: final norm, L2 | `TokenCount` over `prompt ‖ end` | `Final [1, d]`, Q30 |
| bidirectional encoder | `BertModel`, `RobertaModel`, `XLMRobertaModel` | `lower::bidir`: one position over `[L, d]` rows. Inputs `input.ids` (`[L]`) and `input.count` (`[]`). Full attention over the token axis, masked by `Compare(Iota < count)`. Post-LN layers. RoBERTa positions start at `padding_idx + 1` | `Fixed { n: 1 }`: `JobTokens` and `JobTokenCount` over `cls ‖ prompt ‖ sep`, padded to `L` | `Final [1, d]`: CLS or the masked mean, optionally L2 |

- **Output fixed point** (`OutputSpecV1::embedding_i32(n, d, q, normalised)`). A normalised output
  is Q30. An unnormalised one is in the smallest power-of-two unit at least its calibrated `i32`
  scale (`Base::Pow2Site`, `q = 27` for the CLIP fixture).
- **sentence-transformers** stacks (`modules.json`, `1_Pooling/config.json`,
  `sentence_bert_config.json`) are read by `encoder::sentence_transformers`: `cls`, `mean`,
  `lasttoken`, `Normalize` and `max_seq_length`. Other pooling modes and `Dense` modules are
  refused by name.
- **Fidelity.** The tiny HF fixtures (`tools/gen_hf_encoder_fixtures.py`, bf16-exact random
  weights) run through `tests/encoders.rs`:

  | fixture | float reference vs HF | integer cosine vs HF |
  | --- | --- | --- |
  | CLIP text | 1.5–2.0e-7 | 0.99997 – 0.99998 |
  | Qwen3 as last-token embedder | 2.1–2.4e-7 | 0.99996 – 0.99997 |
  | BERT (mean+L2, CLS) | 1.7–2.5e-7 | 0.99995 |
  | RoBERTa (mean+L2) | 1.4–1.5e-7 | 0.99992 – 0.99994 |
  | XLM-R (CLS+L2, mean) | 1.1–3.2e-7 | 0.99993 – 0.99995 |

  In every case the V2 program's run equals its pipeline's run byte for byte, and
  `tir_admit_program_v2` and `tir_admit_pipeline_v1` admit both.
- **Admission at real sizes.** Admission charges a commit point's tile by box demand (04b §10.3):
  a `MatMul` operand is demanded `d · contraction` elements for `d` outputs, and a reduction's
  operand `d` times the reduced axis. Over the fixed token axis that widened two cones past the
  legacy court's tile ceiling (16.8 M MACs, 1.05 M exponentials):
  - the softmax's row maximum and row sum, broadcast back over their rows, reach the whole
    `[h, L, L]` score matrix from any tile downstream of them;
  - a post-LN norm reduces a whole row, so a tile of it reaches `d` columns of every row of the
    projection its input came from.

  A BERT-base-shaped encoder was refused at 128 tokens (75.5 M MACs a tile). The fix is commit
  points, which change no value. Past half the ceilings (`bidir::split_softmax`,
  `resid_commit_needed`), the masked logits, the row maximum and the row reciprocal are committed
  (`softmax_committed`, the library softmax bit for bit), and so is each residual sum a norm
  reads. The tiny fixtures stay below the thresholds, so their programs are unchanged. With
  `real_encoders_lower_and_are_admitted_at_128_to_512_tokens` (hand-written hub configs, no
  weights, mean pooling with L2, the one-stage pipeline):

  | encoder | tokens | nodes (max per block) | commit points | job MACs | exponentials | step leaves |
  | --- | --- | --- | --- | --- | --- | --- |
  | BERT-base (`bert-base-uncased` as `BertModel`) | 128 / 256 / 512 | 243 (168) | 22 | 1.12e10 / 2.30e10 / 4.83e10 | 2.4e6 / 9.5e6 / 3.8e7 | 426,084 / 925,860 / 2,146,596 |
  | all-MiniLM-L6-v2 | 128 / 256 / 512 | 242–243 (168) | 19–22 | 1.43e9 / 3.02e9 / 6.64e9 | 1.2e6 / 4.7e6 / 1.9e7 | 97,554 / 269,394 / 686,226 |
  | RoBERTa-base, XLM-R-base | 128 / 256 / 512 | 243 (168) | 22 | as BERT-base | as BERT-base | as BERT-base |

  The per-position cap on step leaves is 4,194,304. The committed logits dominate them: `h·L²` lanes a
  layer.
- §17 has T5-style encoder–decoders (relative position buckets, cross-attention).

## 13. Vision input (RFC-0003 Part II.4)

On `rfc3/lower` (`lower::vision`, `tests/vision.rs`, `tools/gen_hf_vision_fixtures.py`). A tower is
one position over a fixed patch axis. The canonical image enters as `input.image`, an `i16
[H, W, 3]` lifted into an `External` input over `[0, 255]`, and the output rows are `Final`.
- A vision class is the one-stage pipeline `encoder::vision_pipeline()`: `Fixed { n: 1 }`, with
  the image bound by `JobImage { index: 0 }` (rfc3/impl `4bd8f3b2b`).
- Every tower's `run_pipeline` over a `PipelineJob` with the image equals its standalone program
  byte for byte, and `tir_admit_pipeline_v1` admits the pipeline.

**Preprocessing** (in the program, rank ≤ 4):
- optional fixed-ratio box downscale: two `MatMul`s with pinned 0/1 matrices and an exact rounded
  `Div` by `r²`;
- patchify by `Reshape`/`Transpose`, row-major or Qwen2-VL's block-major;
- normalisation folded into the patch projection: `W' = W/(255·std)`, `b' = b − Σ W·mean/std`,
  with `Conv3d` temporal copies summed. The projection reads exact pixels with `i16` weights.

| tower | output | float vs HF | integer cosine vs HF (min per row) |
| --- | --- | --- | --- |
| CLIP (`CLIPVisionModelWithProjection`) | `image_embeds [1, 24]` | 2.1–2.8e-7 | 0.99992 |
| SigLIP (`SiglipVisionModel`, attention-pooling head) | `pooler_output [1, 32]` | 2.0–3.4e-7 | 0.99991 |
| Qwen2-VL tower + merger (2D rope) | merged `[4, 48]` | 3.4–3.8e-7 | 0.99993 |
| Qwen2.5-VL tower + merger (RMSNorm, SwiGLU, window attention: pinned order, id mask, restore) | merged `[16, 48]` | 3.2e-7 | 0.99993 |
| LLaVA tower (feature layer −2, CLS dropped) + projector | image rows `[16, 32]` | 3.3e-7 | 0.99988 |
| CLIP behind a ×2 box downscale (56×56 input, pixels replicated 2×2) | `image_embeds` | — | 0.99991 |

**Real sizes.** A tower's attention over its patch rows meets §12's box-demand widening, and so
does a pre-norm over the patch projection's rows. CLIP ViT-B/16 was refused at 224×224 (115.6 M
MACs a tile) and SigLIP-B/16 too (29.5 M). With §12's commit points (the split softmax, the
committed patch rows) `real_size_towers_are_admitted` admits them at 224×224:

| tower | rows | nodes (max per block) | MACs | step leaves |
| --- | --- | --- | --- | --- |
| CLIP ViT-B/16 | 197 | 242 (155) | 1.76e10 | 546,848 |
| SigLIP-B/16 | 196 | 316 (155) | 1.77e10 | 548,448 |
| Qwen2-VL-2B's tower | 256 | 238 (173) | 1.69e11 | 3,499,520 |

The committed logits grow as `h·L²` a layer, so Qwen2-VL's tower reaches the 4,194,304 step-leaf cap
just past 224×224. A larger image needs the cap raised, or a demand model that follows rows.

**Image + text → generated ids** is the two-stage text pipeline of §16 (the off-chain check this
paragraph described is superseded by it).

## 14. Next: cross-attention (T5, Whisper) — what `TirProgramV2` needs

Built for T5 and the BART family in §17, as planned here. Whisper (the audio input binding) is not.
The analysis as written before:

- **The encoder is a stage** like §12's bidirectional encoder. T5 needs:
  - RMSNorm without a mean;
  - unscaled scores;
  - gated-GELU FFNs;
  - a relative position bias: a pinned `[h, L, L]` param for the encoder, and for the decoder
    `Gather(bucket table, pos − j)` over `Iota` along `H`.

  Whisper's encoder reads PCM `i16`. Its log-mel front end is integer TIR throughout:
  - STFT as `MatMul`s with pinned Q24 DFT matrices, and power as `Mul`/`Add`;
  - the mel filterbank as a pinned `MatMul`;
  - `IntLn`, then the global `ReduceMax` clamp and the `(x+4)/4` scaling;
  - the two strided conv stems as `Reshape`/`Gather` windows into `MatMul`s.
- **Cross-attention needs no new primitive.** Recomputing each decoder layer's cross K/V from the
  encoder rows at every position would cost `L_enc·d²` a position per layer. Instead:
  - the encoder stage emits every decoder layer's K/V as its one `Final` output
    `[layers, 2, L_enc, d]` (Whisper-large: 123M elements < `2^28`);
  - the decoder binds it with `StageFinal`;
  - each layer occurrence picks its slice with a per-layer `idx` param through `Gather`;
  - attention runs over the encoder axis, which is `Fixed` (not `H`), under the same cone ceilings
    as §12, masked by `StageRowCount`/`JobTokenCount`.
- **What V2 lacks for it:**
  1. A `Logits` stage: T5 and Whisper generate. RFC-0003 §II.2.1's text stage (`TextStream`) is
     that stage now (§16); the decoder would be it, bound to the encoder stage's K/V.
  2. For Whisper, an audio input binding. It is `JobImage`'s pattern with PCM `i16` and 2-byte
     leaves against `input_root` (RFC-0003 II.5).
  3. Optionally, stages with more than one output node. That would avoid concatenating the
     per-layer K/V into one tensor, but the concatenation works today.

## 15. LoRA adapters (RFC-0004: candidate = parent + adapter)

`crate::lora`, `lower::lower_lora`, `tests/lora.rs`, `tools/gen_hf_lora_fixtures.py`. A PEFT adapter
(`adapter_config.json` + `adapter_model.safetensors`) is read directly, with no `peft` installed.
Every targeted projection keeps its parent weight and adds an unmerged path:

```
y = W·x (+ b) + (num/den) · B·(A·x)        A: [r, in], B: [out, r]
num/den = lora_alpha / r  (rsLoRA: / √r, a square rank)   — an exact rational
```

**The integer path.**
1. `A·x` is one exact `i32 × i16` product over the parent projection's input codes. `A` is stored
   as `A' = A·diag(s_x/s_x0)` at per-row `i32` codes, so a split input needs no extra columns. It is
   narrowed to `i16` at its calibrated site `{site}.lora_a`.
2. `B·a` uses per-row `i16` codes, exact in `i64`.
3. The rational is applied as an integer `Mul` by `num` and a rounded `Div` by `den`.
4. One narrowing takes the result into the projection's output scale, then an `Add` and a `Clamp`.

The node stays a `Linear`, so no pattern of the lowering changes. It adds about 20 nodes per adapted
projection.

**The parent reused.** `lower::adapter_params_last` puts every adapter param behind the parent's.
The candidate is materialised with the parent's calibration plus the adapter's own `lora_a` sites.
Its params `0..P` are then the parent program's, and their tensors the parent artifact's, byte for
byte. Params `P..` are the adapter section.

| fixture (parent) | targets | r | scale | float vs HF merged | integer vs HF merged: top-1, KL (parent vs merged) | adapter section | MACs a position |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `llama_r16` (llama) | q, k, v, o, gate, up, down | 16 | 32/16 = 2 | 2.9e-7 | 1.000, 5e-5 (0.083) | 42 params, 55.8 KB | 33,792 → 50,176 |
| `qwen2_all_r4` (qwen2_sliding) | `all-linear` | 4 | 12/4 = 3 | 3.0e-7 | 1.000, 5e-5 (0.101) | 84 params, 26.1 KB | 41,728 → 47,872 |
| `phi_rs_r64` (phi) | q, k, v, dense, fc1, fc2 | 64 | rsLoRA 16/8 = 2 | 2.3e-7 | 1.000, 3e-5 (0.029) | 36 params, 183 KB | 30,208 → 87,552 |
| `mistral_qv_r8` (mistral_window) | q, v | 8 | 16.5/8 = 33/16 | 2.9e-7 | 1.000, 5e-5 (0.030) | 12 params, 9.2 KB | 34,304 → 36,864 |

All four candidates are equal on the reference evaluator, ref2 and exec (`three_way.rs`), and
`tir_admit_v1` admits them.
- The MAC share is large only because the fixtures are 32 wide. The adapter adds `r·(in + out)`
  per projection, about 2 % of a 1,536-wide model at `r = 16`.
- **Refused by name:** DoRA, trained biases (`bias ≠ none`, `lora_bias`), `modules_to_save`,
  `layers_to_transform`/`layers_pattern`, targets on fused projections (`qkv_proj`, `gate_up_proj`,
  `c_attn`), `fan_in_fan_out`, rsLoRA at a non-square rank, and module-specific rank or alpha
  patterns.

**What the lowering needs from RFC-0004.**
1. **An artifact root that composes.** The candidate's `artifact_root` should commit to the
   parent's artifact root plus the adapter section's root, for example two leaves or a section
   tree, so the parent's tensors are never re-committed. The candidate program itself is new, with
   its own `graph_ir_root`: the parent's nodes plus the adapter path's.
2. **A class id that binds four things:** the parent (its class id, or its program digest and
   artifact root), the candidate's `graph_ir_root`, the adapter root, and `P`, the index where the
   adapter section starts.
3. **A calibration rule.** The adapter's own narrowing params come from a calibration of the
   candidate on some set. RFC-0004 pins that set, or treats the params as data in the adapter root,
   which they already are. The parent's scales are reused, so an adapter that moves activations
   past the parent's calibrated headroom (2×) clips. RFC-0004 must either bound the adapter's
   effect or allow recalibration, and recalibration changes parent-side narrowing params, which
   gives up "parent reused".
4. **The node budget.** At about 20 nodes per adapted projection, `all-linear` on the largest
   blocks (MoE and gated-delta hybrids at 468 of 512) does not fit. A leaner adapter form or a
   target limit is needed there.

## 16. The VLM text stage (RFC-0003 §II.2.1)

`LowerOpts::image_rows` (`lower::ImageRows`), `tests/vision.rs` (`text_stage`),
`tools/gen_hf_vision_fixtures.py` (`llava`, `qwen2_vl`, `qwen2_5_vl`: whole tiny VLMs, 8 greedy ids).
A VLM class is two stages: the tower (§13) over `JobImage { index: 0 }` (`Fixed { n: 1 }`), then
the LM as the text stage — a `Logits` program over `TripRule::TextStream` — with its one external
input `input.image_rows` (`i32 [rows, d]`) bound by `StageFinal { stage: 0 }`. The input's
interval is the tower's output interval.

**Placement by a cursor (no job field).** The pre block keeps a `Fixed` state `image.cursor`
(`i32 [1]`, `[0, rows]`). At a position whose token is the placeholder id while `cursor < rows`,
the LM reads row `cursor` (narrowed from the tower's unit to the residual scale) instead of its
token embedding, and the cursor advances. Once every row is placed, the placeholder is an ordinary
token, as HF embeds a generated one.
- **This differs from RFC-0003 §II.2.1's text** ("one with more re-reads the last":
  `Gather(image_rows, min(cursor, N_img − 1))` at every placeholder). The two agree on every prompt
  with at most `N_img` placeholders, which the class's chat template guarantees. They differ on a
  placeholder id after the `N_img`-th, including one the model generates: HF embeds that as a token
  (its image features enter only at the prefill). Switching to the RFC's rule is one condition in
  `lower::image_token`.

**M-RoPE (Qwen2-VL, Qwen2.5-VL), exact.** `get_rope_index` for one image: image token `i` at
`(s, s + i / W, s + i % W)` with `s` its first row's stream position and `(H, W)` the merged grid;
every later token at `p − rows + max(H, W)` in all three components. A layer block reads only
per-layer states (NF-15), so each layer keeps its own copy `image.cursor.layer` and advances it by
the pre block's rule. Each component's cos/sin come from the two-level tables, and each frequency
takes its component through two `Select`s with pinned masks. `rope::MRope` carries both HF layouts:
Qwen2-VL's contiguous sections and Qwen3-VL's/Qwen3.5's interleaved one (unit-tested against HF's
index map; the interleaved layout has no VLM fixture yet). Text-only programs are unchanged.

| VLM | float LM vs HF over the stream (HF's rows) | greedy ids vs HF `generate` | teacher-forced top-1, mean KL, logits rel | nodes: tower / LM (max per block) | pipeline admission at `max_trip` 64: MACs, step leaves |
| --- | --- | --- | --- | --- | --- |
| LLaVA (Llama LM, 16 rows) | 2.9e-7 | 16/16 | 58/58, 1.1e-4, 1.4e-2 | 225 / 333 (266) | 2.43M, 2,131 |
| Qwen2-VL (4 rows, grid 2×2) | 2.6–2.9e-7 | 16/16 | 32/32, 5e-5, 1.1–1.2e-2 | 237 / 405 (338) | 2.61M, 2,242 |
| Qwen2.5-VL (16 rows, grid 4×4, window tower) | 2.4–2.5e-7 | 8/16 | 54/56, 5e-5, 1.0–1.2e-2 | 411 / 405 (338) | 4.63M, 3,272 |

- Qwen2.5-VL's second image departs at its first id, where HF's own top-2 margin is 0.0070. Both
  teacher-forced misses are at HF margins ≤ 0.0146. The test requires every difference to sit at a
  near-tie (HF margin < 0.05).
- **The M-RoPE control.** The same float LM with 1-D positions is 0.13–0.16 rel from HF, against
  the integer stage's 0.010–0.012. The integer positions are HF's.
- Every run replays exactly: `run_pipeline` with `job.generated` gives `run_text_pipeline`'s output.
- The fixture's `mrope_positions` (HF's `get_rope_index`, with the processor's `mm_token_type_ids`)
  equal the placement's at every position of every stream.
- Out of scope here: several images or videos (`video_grid_thw`, the temporal component), Qwen2.5-VL's
  `second_per_grid_t` (videos only), and a VLM whose tower size varies by job.

## 17. Encoder–decoders (T5, BART, mBART, Marian, Pegasus)

`lower::encdec`, `encoder::{encdec_encoder_v2, encdec_decoder_v2, encdec_pipeline}`, `tests/encdec.rs`,
`tools/gen_hf_encdec_fixtures.py`. A sequence-to-sequence class is two stages, §14's plan:

| stage | program | trip | bindings | output |
| --- | --- | --- | --- | --- |
| 0: encoder | one position over the padded source `[L, d]` | `Fixed { n: 1 }` | `JobTokens`, `JobTokenCount` of the source template | `Final`: every decoder layer's cross keys and values, `i16 [D, 2, L, inner]` |
| 1: decoder | one position per stream id (the text stage) | `TextStream` | `StageFinal { stage: 0 }`, `JobTokenCount` (the same template) | `Logits` |

- **The cross K/V.** The encoder's last rows (after its final norm) go through all `2·D` stacked
  projections in ONE `MatMul`, channel `(l·2 + kv)·inner + j`. Each is narrowed per channel to
  decoder layer `l`'s own code scale (statistics `post.xkv.L{l}.k`/`v`, recorded by the float
  encoder; the decoder reads the same values as `L{l}.xattn.k`/`v`, so one calibration gives both
  programs one scale). Each decoder layer picks its `[2, L, inner]` slice with a per-layer `idx`
  param through `Gather`. Cross-attention runs over the fixed source axis, keys at or past the
  count masked to `i32::MIN`.
- **T5.** RMSNorm without a mean, pre-norm, and unscaled scores. Relative position buckets come
  from layer 0's table, shared by every layer of a stack:
  - encoder: bidirectional buckets, a pinned `[L, L]` bucket map;
  - decoder: causal buckets, through `lower_attention`'s `RelBias`, a `[max_distance + 1]` bucket
    table over `pos − j`, clamped where the bucket saturates.

  `t5_bucket` is torch's `f32` arithmetic and equals HF's `_relative_position_bucket` over −40..40,
  both directions. Also:
  - ReLU or gated FFNs (`gated-gelu` is `gelu_new`);
  - the decoder output scaled by `d^−½` before the head when `scale_decoder_outputs` (5.17 saves it;
    older configs read `tie_word_embeddings`);
  - the head from `lm_head.weight` when the checkpoint has it, else the shared table;
  - q, k, cross-q and the cross keys take per-row `i16` weights (`encdec::wide_qk`): an unscaled
    score multiplies their error by itself.
- **The BART family.** Biases everywhere, LayerNorm (eps 1e-5), attention scaled by `head_dim^−½`,
  the head tied to the shared table plus `final_logits_bias`.

  | | norms | positions | `√d` scale | `layernorm_embedding` | final norms |
  | --- | --- | --- | --- | --- | --- |
  | BART | post | learned, `pos + 2` | config | yes | no |
  | mBART | pre | learned, `pos + 2` | config | yes | yes |
  | Marian | post | sinusoid, computed (not saved) | yes | no | no |
  | Pegasus | pre | sinusoid, the checkpoint's table | yes | no | yes |

- **Admission at real sizes** needs §12's commit points in the encoder (the split softmax past
  half the tile ceilings) and every sublayer's residual sum committed.

**Fidelity** (two sources of 10 and 7 ids, 8 greedy ids each, under an explicit
`GenerationConfig`: no forced BOS/EOS, no EOS stop). The tiny tied models repeat ids, so the
fixtures scale the embedding and the decoder's output norm. Every float reference equals HF within
1e-6 (encoder rows, decoder logits on HF's stream and on a random one).

| model | greedy ids vs HF `generate` | top-1 (HF stream + random stream) | mean KL | nodes: encoder / decoder (max per block) |
| --- | --- | --- | --- | --- |
| T5 (ReLU, tied, scaled) | 16/16 | 31/32 | 4.6e-5 | 182 / 266 (136 / 221) |
| T5 v1.1 (gated-GELU, untied, `d_kv·h ≠ d`) | 11/16 | 28/32 | 1.1e-2 | 194 / 278 (148 / 233) |
| BART | 16/16 | 32/32 | 8.5e-4 | 229 / 328 (167 / 262) |
| mBART | 16/16 | 32/32 | 4.9e-5 | 247 / 341 (157 / 247) |
| Marian | 11/16 | 30/32 | 7.4e-4 | 196 / 295 (167 / 262) |
| Pegasus | 16/16 | 32/32 | 7.2e-4 | 214 / 308 (157 / 247) |

- Every difference is a tie within the integer program's noise: HF's top-2 margin there is at most
  1.3× the integer logits' RMS error (`tie_ratio`). Replays are exact.
- T5 v1.1 is the weakest: its logits span 27 units, and its error is W8 accumulation (the per-site
  diagnosis, `ENCDEC_DIAG=1`: ~0.5 % a projection, 1.8 % at the encoder's cross K/V, 4.5 % at the
  logits). Nothing is wrong at any one site.
- **Real configs** (`tests/configs/encdec/`: t5-small, flan-t5-base, bart-large-cnn,
  mbart-large-50, opus-mt-en-de, pegasus-xsum) lower and are admitted at `L = 512`, `max_trip` 512:
  - job MACs 3.6e10 (t5-small) to 3.3e11 (mbart-large-50, whose 250k-row head is 1.3e11 of it);
  - 1.5 M to 7.5 M step leaves a job.

**What the consensus side lacks** (not touched here):
1. **A second token list.** The encoder reads the source through `TokenSource::Negative`, the only
   other list a `PipelineJob` has; the tests put the source in `negative`. A text profile with an
   encoder needs its own source field (a `TokenSource::Source`, or the profile admitting
   `negative` for it), bound by `input_root` like the prompt.
2. **The decoder's prompt is a forced prefix.** It is `decoder_start_token_id`, plus the target
   language for mBART-50 (HF's `forced_bos_token_id`). A `TextStream` stage has no token rule, so
   the class's template cannot supply it. Either the Text profile checks the prompt's head, or
   `TextStream` gets a prefix.
3. **Evidence size.** The cross K/V edge is `D·2·L·inner` codes (bart-large at 512: 12.6 M
   lanes). A dispute over one of its leaves is the encoder's (PALW-TIR-33), and it rides the same
   material limits as D-F1's captures.

## 18. Coverage sweep (2026-09-29): end to end, and the next families

**What "admitted end to end" means here.** It is a tiny random model from transformers' own
config class, rebuilt with its position limits at 1,024 (`tools/audit_e2e.py`), and
`tools/audit_e2e.sh` takes it through every step:
1. lowering, calibrated at 512 (`palw-tir-fidelity --artifact-out`);
2. the int-8 `palw-class`: `declare-layout --max-context 512` on testnet-12's ceilings (ADMISSIBLE),
   then `close-sizes` (every close fits);
3. greedy ids against HF `generate` (3 prompts × 12 ids, no EOS stop), and teacher-forced top-1/KL
   against HF's logits (2 × 48 positions), through `palw-tir-generate`.

A departure is judged by HF's top-2 margin there against the integer logits' RMS error.

**The 57 fixture families of §3.** 56 were admitted end to end at the first pass. Falcon-ALiBi
was refused: its bf16 bias table spans the whole history bound, a 17 MB close. It is now sized by
the lowering window, and lowered with `--max-window 512` it is admitted (worst close 69 KB,
greedy 36/36).

- Worst closes 69–203 KB (the recurrent families are the largest); programs 236–1,392 nodes.
- Teacher-forced top-1 ≥ 0.927 (median 0.990), KL median 8e-5 (max 5.5e-3). Greedy
  1,876/2,016 identical.
- Every departure is a near-tie, except three explained cases:
  - phi3_longrope (0/36): HF's `generate` prefills the whole prompt with LongRoPE's long factors,
    and its own decode path does not (§7). The class follows the per-position semantics, and
    teacher-forced on those it agrees (top-1 0.99, KL 8e-5).
  - qwen3_5_moe (20/36) and deepseek_v3 (27/36): an expert-routing near-tie flips one layer's
    experts, and the row's error jumps (0.38 RMS against 0.05). This is the MoE sensitivity, not a
    lowering fault.
- The rest: InternLM2, MiniCPM and EXAONE-3 need remote code (no offline fixture). Of §2's
  not-covered list, native transformers has GLM, Phi-3.5-MoE, Llama-4, Gemma-3n and DBRX.

**New families (first batch).** Each has an HF fixture, integer fidelity and the end-to-end audit:

| family | what it needed | float vs HF | integer vs float (top-1, KL) | end to end at 512: greedy, worst close |
| --- | --- | --- | --- | --- |
| `GlmForCausalLM` (GLM-4-9B-chat-hf, GLM-Edge) | fused `gate_up`, q/k/v bias, partial rotary 0.5 on interleaved pairs | 9.8e-7 | 0.972, 5e-5 | 29/36 (ties ≤ 0.2×), 69.7 KB |
| `Glm4ForCausalLM` (GLM-4-0414, Z1) | GLM + post-attention/post-MLP norms | 1.0e-6 | 1.000, 7e-5 | 36/36, 69.9 KB |
| `MinistralForCausalLM` (Ministral-8B) | Mistral with `layer_types` | 1.4e-6 | 0.972, 8e-5 | 28/36 (ties ≤ 0.6×), 70.0 KB |
| `Olmo3ForCausalLM` (OLMo-3) | OLMo-2, 3 sliding : 1 full, rope per layer type | 1.1e-6 | 0.972, 2.1e-4 | 36/36, 70.0 KB |
| `MPNetModel` / `…ForMaskedLM` (all-mpnet-base-v2) | bidirectional + relative-position bias (T5 buckets) | 1.6–1.9e-7 | cosine ≥ 0.99995 | real config admitted at 128–512 |
| `DistilBertModel` / `…ForMaskedLM` | BERT without token types | 1.6–2.6e-7 | cosine ≥ 0.99991 | real config admitted at 128–512 |

**The registrable share, updated.** Counts are the hub's on 2026-09-28; the architecture shares
are §2's estimates, adjusted for 2026 families not in that table.
- **Text generation (RFC-0002, live on testnet-12 from DAA 2,000):** text-generation repos with
  safetensors (325,599 of 416,122) × a covered architecture (≈ 0.89: §2's 91–92 % less ≈ 3 % of
  2026 families not yet covered) × weights in bf16/f16/f32 rather than a pre-quantised format
  (≈ 0.9). That is **≈ 260 k repos, ≈ 63 % of text-generation, ≈ 8.4 % of all 3.10 M HF repos**.
- **With RFC-0003's classes (dormant under `palw_gen_v1`):**
  - embeddings: ≈ 75 % of the 41 k feature-extraction + sentence-similarity repos (BERT, RoBERTa,
    XLM-R, MPNet, DistilBERT, CLIP text, decoder embedders);
  - encoder–decoders: ≈ 70 % of the 53 k text2text repos (T5, BART, mBART, Marian, Pegasus);
  - VLM text stages: ≈ 35 % of the 41 k image-text-to-text repos (LLaVA, Qwen2-VL, Qwen2.5-VL).

  That adds ≈ 82 k, **≈ 340 k in all, ≈ 11 % of all HF**. By downloads the share is far higher;
  this is not measured.
- **The largest still missing,** by estimated repo share:
  - Gemma-4 (per-layer head sizes, K = V globals, KV sharing, per-layer inputs, a parallel MoE:
    the heaviest);
  - Llama-4 (chunked attention, NoPE temperature, MoE);
  - Ministral-3 (Llama-4 query scaling);
  - Gemma-3n; GLM-4.5 MoE; Phi-3.5-MoE;
  - the remote-code families (ChatGLM, InternLM2, Baichuan, MiniCPM, EXAONE-3), which have no
    offline fixture;
  - GGUF and pre-quantised checkpoints: an importer, not a lowering.

## 19. Pre-quantised checkpoints: GPTQ, AWQ and GGUF

A GPTQ or AWQ checkpoint stores each projection as small integers. Every group of input columns
has a scale and a zero point: `W[o, i] = s[g, o] · (q[i, o] − z[g, o])`. These projections are
lowered **from the stored integers, with nothing re-quantised** (`crate::prequant`,
`lower::qlinear`). The float reference, and HF's reference for the fixtures, is the same model
with the dequantised weights substituted.

**Formats read** (`quantization_config`; every key is read, known inert, or a refusal):

| format | bits | groups | column order | zero point |
| --- | --- | --- | --- | --- |
| GPTQ `gptq` (v1), `gptq_v2` | 2, 4, 8 | any, or −1 (one per row) | `g_idx` (act-order permutes it) | v1 stores `z − 1` mod `2^b` |
| AWQ, GEMM packing | 4 | any dividing `in` | `i / group` | as stored; words interleaved `[0,2,4,6,1,3,5,7]` |

- Honoured: `modules_in_block_to_quantize`, `modules_to_not_convert`, GPTQModel's `lm_head`.
- Refused, with the reason:
  - GPTQ 3-bit (its words straddle int32 boundaries), the Marlin and BitBLAS repacks, `dynamic`
    per-module overrides, and packing into anything but int32;
  - AWQ GEMV, GEMV-fast and LLM-AWQ, and the model types where AutoAWQ adds its per-channel
    activation scale (`ScaledActivation`);
  - bitsandbytes, fp8, compressed-tensors and every other method.
- Decoders whose layers are attention + MLP. Quantised MoE experts, MLA and recurrent mixers are
  refused for now.

**The program, per projection:**

```text
xg   = Reshape(Gather(x, order), [G, gs, 1])          GPTQ: the g_idx order, every group contiguous
acc  = MatMul(codes:i8[G, out, gs], xg) → i64 [G, out]   exact
t    = a[g, o] · acc − c[g, o] · Σ_{i∈g} x[i]          the per-group scales, as exact integers
T[o] = Σ_g t[g, o] + acco[o] · 2^g[o]                  the outlier channels, as in the W8 path's split
y    = N(T; m, s, z)                                   the one rounding, as for every projection
```

- **Codes.** A code is `q − z` when that fits `i8` (every format up to 7 bits, and symmetric
  8-bit), and then the `c` term does not exist. An 8-bit asymmetric code is `q − 128`, with
  `c = s · (z − 128)`.
- **Scales.** Each row's fp16 scales become integers at one unit `2^e[o]` per row. The unit puts
  the row's largest `|a|` in `(2^19, 2^20]`, and `2^e[o]` goes into the output narrowing.
  - fp16 has 11 significant bits, so every scale within `2^9` of its row's largest is exact.
  - A smaller one would be rounded at `2^−20` of the row's largest. Such roundings are counted
    (`Materialised::quant_inexact`); there are none on any fixture.
- **Ranges.** Clamps state `|a| ≤ 2^20`, `|c| ≤ 2^28` and `|wo| ≤ 2^24`, so the sum stays inside
  `i64` for every `in ≤ 2^16` (spec 04b §7).
- **Cost.** MACs are the W8 path's (`out · in`). The layer block grows by 29 nodes (AWQ) up to 90
  (8-bit asymmetric with act-order): 266 → 295–356 on the 2-layer fixtures, against NF-12's 512.

**Fidelity and admission.** There are 11 fixtures (`tools/gen_quant_fixtures.py`):
- tiny Llama and Qwen2 models, quantised in numpy per each format's spec;
- their reference dequantises the packed tensors independently of the quantiser.

Every one is admitted end to end at 512 (§18's pipeline: the int-8 `palw-class`, testnet-12
ceilings, every close fits, worst 89 KB). The "twin" is the same dequantised weights as a float
checkpoint, through the W8 path.

| fixture | float vs HF | int vs float: top-1, KL (twin) | end to end: greedy (twin), TF KL (twin) |
| --- | --- | --- | --- |
| `gptq_b4_g32` | 2.4e-6 | 1.000, 2e-5 (0.986, 7e-5) | 36/36 (31/36), 1.6e-5 (7.1e-5) |
| `gptq_b4_g64_act` | 1.8e-6 | 0.958, 2e-5 (0.958, 8e-5) | 36/36 (32/36), 1.8e-5 (7.5e-5) |
| `gptq_b4_perrow` | 1.9e-6 | 0.972, 2e-5 (0.944, 7e-5) | 36/36 (36/36), 1.7e-5 (7.6e-5) |
| `gptq_b8_g128` | 2.4e-6 | 0.986, 2e-5 (0.972, 4e-5) | 36/36 (25/36), 1.7e-5 (4.6e-5) |
| `gptq_b2_g32` | 2.4e-6 | 1.000, 2e-5 (1.000, 7e-5) | 36/36 (36/36), 1.7e-5 (7.0e-5) |
| `awq_g32` | 1.9e-6 | 0.986, 2e-5 (0.972, 8e-5) | 36/36 (33/36), 1.7e-5 (7.8e-5) |
| `awq_g128` | 1.9e-6 | 1.000, 2e-5 (0.986, 8e-5) | 36/36 (27/36), 1.6e-5 (7.7e-5) |
| `gptq_b4_g128_act_asym`, `gptq_b8_g32_asym_act`, `gptq_v2_b4_g64`, `awq_g64` (Qwen2, tied) | ≤ 3.8e-5 of logits ≈ 100 | 1.000, 0 | 36/36 each; saturated softmax |

- Greedy: 396/396 identical to HF, against 356/396 for the twins.
- Teacher-forced KL is 4× lower than the twins' on every Llama fixture.
- The twins' departures are near-ties the W8 re-quantisation tips.

**Size.**
- Per projection weight, the artifact holds 1 byte of code and `4/gs` bytes of scale (twice that
  for 8-bit asymmetric), plus GPTQ's order at `4/out`. The W8 path holds 1 byte and `9/in`.
  - At group 128 that is +3 % over the W8 artifact; at group 32, +12.5 %.
  - On the fixtures, whose activation tables and embeddings dominate, whole artifacts are
    1.01–1.09× the twins'.
- Against the checkpoints: fp16 is 2 bytes a weight, a 4-bit GPTQ/AWQ file ≈ 0.52. The IR has no
  type narrower than `i8`, so a 4-bit code costs a byte: ≈ 0.5× the fp16 artifact, ≈ 2× the
  packed file.

### 19.1 GGUF (llama.cpp)

A GGUF file is read directly (`crate::gguf`), with no gguf package and no llama.cpp:
- the header (v2/v3), its metadata and its tensor table, every count and length bounded before
  anything is allocated;
- each block format unpacked to its stored integers, with per-group scales, zero points and float
  minimums, exactly.

The projections then lower through the same `lower::qlinear` as GPTQ/AWQ. The float view is the
exact value rounded once, which is what llama.cpp's `dequantize_row_*` computes.

| type | group | `W = scale · (q − zero) − min` | program codes | offset term |
| --- | --- | --- | --- | --- |
| `Q8_0` | 32 | `d · q` (signed) | `q` | no |
| `Q4_0`, `Q5_0` | 32 | `d · (q − 8)`, `d · (q − 16)` | `q − z` | no |
| `Q4_1`, `Q5_1` | 32 | `d · q + m` | `q` | `c = −m` |
| `Q4_K`, `Q5_K` | 32 | `d·sc · q − dmin·m` (6-bit `sc`, `m`) | `q` | `c = dmin·m` |
| `Q6_K` | 16 | `d·sc · (q − 32)` (signed 8-bit `sc`) | `q − 32` | no |

- **Per-layer types.** A Q4_K_M/Q5_K_M mix stores `attn_v` and `ffn_down` as Q6_K on llama.cpp's
  `use_more_bits` layers and as the base type elsewhere. A module's program layout takes the finest
  group over its layers (16) and an offset term if any layer's type has minimums. Every layer's
  integers are placed in that layout, with nothing rounded.
- **Outlier channels with an offset term** leave the main product through an input mask
  (`qkeep`), because `a · 0 − c · x` would not vanish. They are added back exactly through `wo`.
- **Embeddings.** A quantised `token_embd` is dequantised and stored as the table's per-row `i16`
  codes. The re-coding error is ≤ 2^−16 of the row's absmax, about 2,000× finer than a 4-bit
  step. An exact gathered form is possible but not built.

**The Hugging Face view** (`GgufModel`). The metadata of `llama` (also Mistral, which converts to
it), `qwen2`, `qwen3`, `gemma` and `gemma2` becomes the Hugging Face `config.json` of the same
model. That config goes through `hf_config` as any other, so the lowering downstream is unchanged.
- Every other `{arch}.*` key is refused, since it may change the math.
- Llama-3's `rope_freqs.weight` becomes its `rope_scaling`, but only when it equals a known factor
  set value by value.
- Gemma-2's query scale follows llama.cpp: `1/√head_dim`, except the 27B's `1/√(hidden/heads)`.

What llama.cpp's converter stores differently is undone exactly:
- `llama`'s q/k rows are permuted `[heads, 2, d/2] → [heads, d/2, 2]` for its interleaved rotary;
  the rows are taken back.
- Gemma's RMSNorm gains are stored as `1 + w`, and the spec multiplies by the stored value.

**Fixtures** (`tools/gen_gguf_fixtures.py`). Six tiny models are written to GGUF in numpy per the
file format and `ggml-common.h`:
- each block type has its own quantiser, checked against its decoder;
- the reference is transformers (eager attention) with the weights decoded from the packed bytes by
  an independent numpy decoder, llama's q/k rows permuted back.

| fixture | types | float vs HF | int vs float: top-1, KL (twin) | end to end at 512: greedy (twin), TF KL (twin), worst close |
| --- | --- | --- | --- | --- |
| `gguf_llama_q4_k_m` | Q4_K + Q6_K mix, untied Q6_K head | 2.4e-6 | 1.000, 0 (1.000, 8e-5) | 36/36 (36/36), 2.2e-7 (7.7e-5), 116 KB |
| `gguf_qwen3_q8_0` | Q8_0, q/k norms | 2.9e-6 | 1.000, 0 (0.972, 7e-5) | 36/36 (36/36), 2.2e-7 (6.7e-5), 105 KB |
| `gguf_mistral_mix` | F16, BF16, Q4_0, Q4_1, Q5_0, Q5_1, Q4_K, Q8_0 | 2.4e-6 | 0.986, 2e-5 (0.986, 9e-5) | 36/36 (28/36), 2.3e-5 (8.8e-5), 109 KB |
| `gguf_gemma2_q6_k` | Q6_K; soft-caps, sliding window, post norms | 1.8e-5 of 30 | 1.000, 0 | 36/36 (36/36), 1.1e-6 (2.7e-6), 98 KB |
| `gguf_qwen2_q5_k_m`, `gguf_gemma_q4_0` | Q5_K + Q6_K; Q4_0 + Q8_0 (tied) | ≤ 4.6e-5 of logits ≈ 200 | 1.000, 0 | 36/36 each (saturated softmax), ≤ 146 KB |

- **Admission.** All six are ADMISSIBLE at 512 on testnet-12 (the int-8 `palw-class`), and every
  close fits.
- **Checks.** Every GGUF tensor is read, and the mapped configs equal the configs the models were
  built from. The reference evaluator, ref2 and exec agree byte for byte on all 17 pre-quantised
  fixtures (`tests/three_way.rs`).
- **Cost.** The largest block has 301–373 nodes (NF-12 allows 512), against 266 for the W8
  Llama twin; the K-quant mixes' `qkeep` masks and offset terms are most of the difference.

**A real file.** The decoders read a real llama.cpp file and match its F16 twin within the
quantisation's own error (`tests/gguf_real.rs`, ignored; local files only):
- the file is Qwen3.5-2B-Q4_K_M, the local copy under `~/Downloads/misaka-palw-runtime/models`,
  compared with the F16 GGUF of the same model; nothing was copied from the hosts;
- relative RMS error: `Q4_K` 7.5 %, `Q5_K` 3.8 %, `Q6_K` 1.9 %, `Q8_0` 0.64 %, halving with every
  bit (a wrong layout reads as ≈ 100 %).

Its `qwen35` architecture (hybrid Gated DeltaNet) has no GGUF mapping yet. With one, the hosts'
copy of the same file is the later real-file test end to end.

**Size**, in artifact bytes per projection weight against the packed file:

| format | artifact | file | ratio | against fp16 |
| --- | --- | --- | --- | --- |
| `Q8_0` | 1.125 | 1.0625 | 1.06× | 0.56× |
| `Q6_K` | 1.25 | 0.82 | 1.5× | 0.63× |
| `Q5_K` | 1.25 | 0.69 | 1.8× | 0.63× |
| `Q4_K` | 1.25 | 0.56 | 2.2× | 0.63× |
| `Q4_0` | 1.125 | 0.56 | 2.0× | 0.56× |
| GPTQ/AWQ 4-bit, g128 | 1.03 | 0.52 | 2.0× | 0.52× |

- A code costs a byte: v1 has no `i4`. That is the v2 candidate recorded in freeze-v1 §8.
- The per-group scales are `i32`, with the K-quants' minimums beside them. A two-level scale (fp16
  `d` per 256, `i8` sub-scales) would cut 0.25 bytes to ≈ 0.07 with no dtype change: open lowering
  work.
- Whole artifacts are 1.06–1.15× the W8 twins' on the fixtures.

### 19.1b More GGUF architectures, and quantised experts (2026-09-29)

**GGUF mappings added.** Each has a numpy-written fixture that passes the float-vs-HF check and the
integer check, and is admitted end to end at 512 on testnet-12 (every close fits):

| GGUF arch | Hugging Face | what the file stores differently (undone exactly) | float vs HF | greedy (twin) |
| --- | --- | --- | --- | --- |
| `qwen35` | `Qwen3_5ForCausalLM` | `ssm_a = −exp(A_log)`, read as stored (`fix_binding`); value heads tiled `[v][k]` when there are more of them than key heads (rows, `conv1d` channels, `A`, `dt_bias`, and `out_proj`'s columns taken back); `1 + w` gains except the gated norm; `conv1d` squeezed | 2.1e-5 of 45 | 36/36 (36/36) |
| `gemma3` | `Gemma3ForCausalLM` | llama.cpp's fixed pattern (every 6th layer global), local rope base 10,000 unscaled, `1 + w` gains incl. q/k norms, the 27B's query scale | 1.1e-5 of 64 | 36/36 (36/36) |
| `phi3` | `Phi3ForCausalLM` | LongRoPE from `rope_factors_long/short` (its `attn_factor` checked), sliding window 0 = none, fused `qkv` and `gate_up` | 1.8e-6 | 36/36 (29/36) |
| `qwen3moe`, `qwen2moe` | `Qwen3MoeForCausalLM`, `Qwen2MoeForCausalLM` | experts stacked `[E, rows, cols]` (each read as its slice), llama.cpp's top-k renormalisation (Qwen3: yes, Qwen2: no) | 1.4e-6 | 36/36 (36/36) |
| `llama` + experts | `MixtralForCausalLM` | the same, and llama's q/k permutation | 1.7e-6 | 34/36 (34/36) |

**Quantised experts** (GPTQ, AWQ, GGUF). Each expert is its own stored module (one per expert, or one
slice of GGUF's stacked tensor). The selected experts' codes, scales and (GPTQ) column orders are
gathered by the router's ids, then multiplied exactly as for a dense projection.
- Storage is row-major per expert (`[E, rows, G, gs]`); the product runs group-major after one
  transpose.
- Fixtures: `gptq_qwen3moe_b4_g32_act`, `awq_mixtral_g64`, `gguf_qwen3moe_q4_0`,
  `gguf_mixtral_q8_0`. All are admitted end to end.
- Integer against float: KL 2e-5 to 5e-4. Routing near-ties dominate, as for every MoE (§18).

**Quantised params are row-major.** A codes param is `[out, G, 1, gs]` (experts
`[E, rows, G, gs]`), and its scales are `[out, G]`. An inventory leaf is then one output row, and a
tile of rows opens only those rows.
- The first layout, group-major `[G, out, gs]`, made every group a leaf. A 64-row tile opened the
  whole weight: the real Q4_K_M class below had closes of 19 MB, 20 of them over the 3.2 MB cap.
- The tiny fixtures could not show it, since their tensors fit in a few leaves.

The reference evaluator, ref2 and exec agree byte for byte on all 24 pre-quantised fixtures.

### 19.1c A real file: Qwen3.5-2B-Q4_K_M

The file is the local copy under `~/Downloads/misaka-palw-runtime/models` (Unsloth's
Qwen3.5-2B-Q4_K_M, with imatrix), read only; nothing was copied from the hosts. It holds 320
tensors: Q4_K 98, Q5_K 36, Q6_K 17, Q8_0 36, F32 133.
- **Mapping.** The config maps (24 layers, 18 gated-delta, 6 full attention). All 362 bindings
  resolve by shape and no tensor is left unread.
- **Program.** 963 nodes; the gated-delta block has 474, against 512.
- **Class at 512.** Calibrated on 4 × 512 real tokens (the Qwen3.5 tokenizer, MISAKA's docs). The
  integer class against the float reference of the same Q4_K_M weights, over 2 × 128 held-out
  tokens: top-1 0.953, KL 0.0057 (max 0.096), perplexity 49.25 → 49.62 (+0.74 %). The artifact is
  2.73 GiB.
- **Against its F16 twin** (the same model in F16, the ollama blob, through the W8 path):
  - The twin's own class (W8, calibrated the same way) against its F16 float: top-1 0.938, KL
    0.0104 (max 0.584), perplexity 49.97 → 51.18 (+2.44 %). The exact Q4_K_M path's error against
    its own float is about half of that. Its artifact is 2.34 GiB against the Q4_K_M class's 2.73
    GiB (per-group `a`/`c` of the K-quant layouts at groups of 16 and 32).
  - The two classes against each other on the same 256 positions: top-1 0.910, KL 0.024 (max
    0.387). That is mostly the Q4_K_M quantisation itself (llama.cpp's 4.5 bits), measured here
    through both classes.
- **Admission: REFUSED on testnet-12**, and not because of the quantisation:
  - The refusal is `COURT_COST_EXCEEDS_CEILING`: cone evaluation work 69,139,968 against 2^26.
  - Its cause is the state replay at the declared checkpoint interval C = 1,024. The gated-delta
    block's conv window costs 67,584 elementwise operations a position (0 MACs), charged 1,023
    times. The recurrent state `gdn.S` costs 200,944 a position per head group, which would be
    205.6 M.
  - `tir_admit_v1`'s `C_j` bounds only MACs and transcendentals, so C stays 1,024, and
    `palw-class declare-layout` does not halve C for this check.
  - The W8 program of the same model has identical state costs. Every real-size Qwen3.5 class
    (0.8B included, whose gated-delta dimensions are the same) is refused the same way.
  - C ≤ 256 would admit. The fix belongs to admission: `C_j` bounding elementwise work too, or
    `declare-layout` halving on this refusal.
- **Closes** (`palw-class close-sizes`, row-major layout): all 92 fit the 3.2 MB cap. The worst is
  1.14 MB (the logits tile over the tied `i16` table); the layer carry-outs are 667 KB. With the
  first, group-major layout, 20 closes were over the cap, the worst 18.98 MB.

### 19.2 The registrable share, with pre-quantised repositories counted

The hub counts are §18's (2026-09-28). The GPTQ/AWQ repo count and the GGUF architecture mix are
estimates, not measured.

| population | repos | registrable | how |
| --- | --- | --- | --- |
| text-generation, float safetensors | 325,599 × 0.9 | ≈ 260 k | §18: covered architecture × not pre-quantised |
| text-generation, GPTQ/AWQ safetensors | ≈ 20 k of the ≈ 33 k pre-quantised | ≈ 16 k | dense attention + MLP of a covered family (× 0.8); bitsandbytes, fp8, compressed-tensors, EXL2 and MLX are not read |
| GGUF (`library=gguf`) | 207,184 | ≈ 120 k | `llama`/`qwen2`/`qwen3`/`gemma`/`gemma2` dense (≈ 0.6) × a Q8_0/Q6_K/Q5_K/Q4_K/Q4_0/Q5_0/Q4_1/Q5_1 file (≈ 0.97) |
| **text generation in all** | | **≈ 395 k** | **≈ 12.7 % of all 3.10 M HF repos** (was 8.4 %) |
| + RFC-0003 classes (§18) | | + ≈ 82 k | **≈ 477 k, ≈ 15.4 %** (was 11 %) |

The largest pre-quantised gaps:
- GGUF `gemma3`, `phi3`, `qwen35`/`qwen3moe`/`llama4`/`gpt-oss`, Mixtral-in-`llama` (experts) and
  the IQ types;
- bitsandbytes-4bit and MLX repos (both numerous);
- quantised MoE experts in GPTQ/AWQ.

**Updated after §19.1b** (the same counts; the factors are estimates):

| population | registrable | change |
| --- | --- | --- |
| text-generation, float safetensors | ≈ 253 k | −7 k: Qwen3.5 and Qwen3-Next classes are refused by admission at real sizes (§19.1c) until admission bounds C by its elementwise work |
| GPTQ/AWQ safetensors | ≈ 18 k | quantised experts (× 0.9) |
| GGUF | ≈ 150 k | + `gemma3`, `phi3`, `qwen3moe`, `qwen2moe`, Mixtral (≈ 0.75 of the repos; `qwen35` mapped but refused like its float class) |
| **text generation in all** | **≈ 420 k, ≈ 13.5 % of HF** | |
| + RFC-0003 classes | **≈ 500 k, ≈ 16 %** | |

The GDN admission fix would add back ≈ 7 k float repos and the `qwen35` GGUF repos (≈ 5 % of GGUF,
≈ 10 k).

## 20. The 2026 families (2026-09-29): Llama-4, GLM-4.5, Phi-3.5-MoE, Gemma-4 (its KV-sharing E models too), Ministral-3

Each family has a tiny fixture built from transformers 5.17's own config class
(`tools/gen_hf_fixtures.py`), a float check against HF over 10 positions, integer fidelity, and
`tir_admit_v1` with the three-way equality (reference, ref2, exec). Every one of its features is
switched on inside the 10 positions. The real shapes below are transformers' default configs, which
are the published models' shapes (GLM-4.5 with its `head_dim` 128).

| family | what it needed | float vs HF | integer (top-1, KL) | real shape, per-layer blocks |
| --- | --- | --- | --- | --- |
| `Glm4MoeForCausalLM` (GLM-4.5/4.6) | DeepSeek-V3 routing over GQA with q/k/v biases, per-head QK-norm, partial NeoX rotary | 1.1e-6 | 0.958, 6e-5 | 272 / 358 nodes, admitted |
| `PhimoeForCausalLM` (Phi-3.5-MoE) | `sparsemixer` top-2 (`Scoring::SparseMixer`: two committed TopKs, the jitter threshold in exact i64); LongRoPE as 5.17 runs it (short factors, mscale) | 7.2e-7 | 0.944, 7.4e-3 | 339 nodes, admitted |
| `Llama4ForCausalLM`, text of `Llama4ForConditionalGeneration` | chunked attention, exact (the keys before the query's chunk masked over an iota of H); the NoPE layers' query temperature (`Op::PosScale`, a Q24 table over ⌊(p+1)/floor⌋); top-k sigmoid experts that scale their INPUT (`MoeSpec::input_scaled`); L2 QK-norm moved before the rotation (it keeps the RMS) | 1.2e-6, 8.3e-7 | 0.986, 1.000 | 408 / 296 nodes, admitted |
| `Gemma4ForCausalLM` | wider full-attention heads with their own KV heads, K = V, a weightless V-norm, the proportional rope, per-layer inputs recomputed per layer from the token (`Pick::PerLayer` slices), the MoE block beside the MLP with a per-expert scale, `layer_scalar`; a layer runs as two blocks (its mixer half and its FFN half, `HlProgram::layer_of`) since it is ≈ 750 nodes | 3.3e-6 | 1.000, 1e-4 | 289 + 244 nodes (451 with the MoE block), admitted |
| `Ministral3ForCausalLM` | Mistral with Llama-4's query scaling over the original length (`llama_4_scaling_beta`) | 1.0e-6 | 1.000, 7e-5 | 276 nodes, admitted |

**KV sharing (Gemma-4's E models, `num_kv_shared_layers`)** is lowered as more carries. The last
`num_kv_shared_layers` layers project queries only. Each attends over the keys and values of the
last earlier layer of its own type (transformers' `store_full_length_kv`). With
`use_double_wide_mlp`, its MLP is twice as wide (its own HL name, `mlp2x`).
- The HL program carries the residual and then, per KV slot (one per layer type that has sharing
  layers), a key row and a value row (`KvShare::{Source, Consumer}`; at most 8 carries, NF-4).
- The pre block carries zeros. The source layer carries out the very row it appended, which is
  already committed. Every other block passes the rows through with an identity clamp. A sharing
  layer appends the carry-in to a history of its own; NF-20 admits a carry-in as an appended row.
- The rows' scale is the source occurrence's. `Base::At { prefix, base }` resolves a scale at a
  fixed occurrence, whichever occurrence reads it. A source block that runs as more than one layer
  is refused.
- Fixture `gemma4_kvshare`: 6 layers, the last 3 sharing (two sliding and one K = V full, over two
  slots), a double-wide MLP, per-layer inputs, a window of 4 inside the 72 positions. Float vs HF
  8.0e-6; integer top-1 1.000, KL 1.3e-4; three-way equal over 32 positions; admitted.
- Real shape: transformers' default Gemma-4 with 10 of 30 layers sharing and the double-wide MLP
  lowers to 5 carries. The largest block is 293 nodes (a source's mixer half is 291, a sharing
  layer's 188). It is admitted by `tir_admit_v1`.

**Refused by name, with what it takes:**
- Gemma-3n: KV sharing as above, plus AltUp's four residual streams mixed by a per-token tanh
  router, Laurel's low-rank residual, the Gaussian top-k activation sparsity, and the streams'
  magnitude-matched projections in and out.
- Mistral-4 (MLA + MoE + the same query scaling) is not attempted yet.

**What "admitted" still means.** It means `tir_admit_v1` (normal form, ranges, per-position costs,
cones). A class also passes admission v10's close sizing. At real sizes, the sizing's 2^26-step
cap refuses these models until `palw_tir_fence2`'s range twin lands. This holds for most models
above ≈ 1.5 B (§19.1c and the LoRA budget's measurements).

**The share, updated** (the same counts as §19.2; the new families are few by repo count and
large by downloads):

| population | registrable | change |
| --- | --- | --- |
| text-generation, float safetensors | ≈ 256 k | + ≈ 3 k: GLM-4.5, Phi-3.5-MoE, Llama-4, Ministral-3 and Gemma-4, the E models included (≈ 1.1 % of 325.6 k × 0.9) |
| GPTQ/AWQ, GGUF | ≈ 168 k | unchanged (no `llama4`/`glm4moe`/`phimoe` GGUF mapping yet) |
| **text generation in all** | **≈ 423 k, ≈ 13.6 % of HF** | by `tir_admit_v1`; the sizing cap lowers it until fence2 |
| + RFC-0003 classes | **≈ 505 k, ≈ 16.3 %** | |


## 21. Generic feature lowerers: Qwen4-Exp as a combination, not a model (2026-10-01, RFC-0002 lane G)

The generic frontend's acceptance test. Qwen4-Exp (transformers 5.17 `qwen4_exp_text`) combines four
residual streams with gated mixes, gated delta layers at a key:value head ratio, sparse block
attention, hashed n-gram per-layer embeddings, a routed MoE and an attention output gate. It is read
by ONE data file (`adapters/qwen4-exp.json`, Level B) into a `ModelSpec` of generic features and
lowered by generic lowerers (`src/lower/generic.rs`). Nothing in the lowering knows the name `qwen4`;
**no new primitive, no new court kernel** (the registry says so for every feature below).

| feature | what the spec says | how it lowers (`lower/generic.rs`) |
| --- | --- | --- |
| `RESIDUAL_GATED_HC_V1` | `streams` rows of the hidden width; each block reads a gated mean of them and writes back through learned weights | the carry is `streams × hidden` `i32`; a grouped RMS norm, a low-rank pair of projections (SiLU between), a sigmoid gate, `StreamMean` (one `ReduceSum` and one narrowing), `StreamOuter` (one broadcast product and one narrowing); two blocks a layer |
| `EMBED_NGRAM_PLE_V1` | per layer, hashed 2- and 3-gram rows gate against the streams, a dilated causal conv adds context | the hash IN the program (below); a per-head table read; `GroupDot`, a signed-sqrt table and a sigmoid table; the third block of the layer |
| `ATTN_SPARSE_BLOCK_V1` | mean-pooled block keys, ReLU scores over index heads, top-K blocks plus the incomplete tail | `BlockMean`, `RopeAtBlock`, `BlockSelect`, `BlockWrite`, and a key mask in the attention (below) |
| `CONV_DEPTHWISE_CAUSAL_V1` at any dilation | taps `d` positions apart | a window state of `(k−1)·d` rows and one `Gather` by a constant index vector |
| `MIXER_GDN_V1` | any key:value head ratio, gate `silu` or `sigmoid` | the existing delta step; `GatedRmsNorm` now carries its gate's activation |

**Data-dependent values, never shapes.** A sparse-attention layer reads a different set of blocks at
every position, but every tensor keeps its static shape: the block scores are computed for ALL
`blocks` rows, an incomplete block scores `−1` (every real score is `≥ 0`), a fixed-K `TopK` picks
the ids (04b §6.6: the higher score first, equal scores the lower index first, the set in ascending
index order), and the logits of the keys outside the picked blocks and outside the incomplete tail
take the softmax's floor through a `Select`. The `1/√dim` of the float score moves no rank and is
dropped; the scores are shifted into `i32` so they can be committed (a `TopK` cone is then `blocks`
leaves, each a small matmul).

**The n-gram hash, in `i64`, with no consensus change.** `mixed_n = t₀·m₀ ⊕ … ⊕ t_{n−1}·m_{n−1}` needs an
XOR the primitive set does not have. It is bit decomposition: `bit_k(a) = ⌊a/2^k⌋ − 2⌊a/2^(k+1)⌋` for
the 63 bits of each product (a `Div` by a constant vector, a `Div`, a `Mul`, a `Sub`, a never-fires
`Clamp(0, 1)` to give the range analysis the interval it cannot derive), the XOR of the first `m`
products' bits as the parity of their sum (ONE `MatMul` by a 0/1 triangular constant gives every order),
the recomposition as a `MatMul` by `2^k` into `i128` and a `Clamp` back (the analysis bounds a
63-term sum of up to `2^62` terms by `2^68`), and `mixed mod size` as a floor-division, a `Mul` in `i128`
and a `Sub`. The multipliers are bounded by `i64::MAX / vocab` (a `Clamp` states it), so `token ·
multiplier` never leaves `i64`. The segment window (the last `n − 1` tokens, an `eos` ending a
segment) is a `Fixed` state of `token − eos`: a fresh sequence is all zeros.
- **Measured** (trigram layer, tiny fixture): the ids are **26 nodes**, the per-head table read 14 (one
  chunk), the whole PLE block 187 nodes with its three norms, the mixer half at most 450.
- **Evidence for a possible one-time primitive-set extension** (decided later, not required): general
  integer bit primitives — `XOR`, shifts, a wrapping multiply, a remainder — would cut the ids from 26
  nodes to about 6.
- The in-program ids equal transformers' ids at every (PLE layer × position) of every PLE fixture
  (`PLE_01..04`, `PLE_06`), including eos inside n-grams and the extreme token ids.

**A table above 2^24 rows is a lowering matter (NF-8).** Each hash head owns a contiguous range of the
layer's table, so the table becomes `[heads, rows, dim]` `i16` codes at one scale per layer, cut into
chunks of at most `2^24` rows (`LowerOpts::table_chunk_rows`), one batched `Gather` (`batch_dims` 1)
per chunk and a `Select` by the chunk index. Published sizes (20 M rows a head, 16 heads) need two
chunks. The fixture test cuts at 16 rows (`PLE_06`): the logits are bit-identical to the unchunked
lowering's and the three implementations agree.

**Results** (16 fixtures, `tests/qwen4_exp.rs`; float = the HL interpreter; KL in nats):

| check | result |
| --- | --- |
| float reference ↔ transformers | max abs logit difference 2.4e-7 … 3.4e-6 (logit scale ≈ 1.5–1.8) on all 16 |
| integer program ↔ transformers | KL 1e-5 … 4.1e-4; no top-1 differs where transformers leads by more than 8× the quantisation error |
| streaming program ↔ transformers' cached decode | the same (QSA-06, PLE-04) |
| block selection | at every position of K = 1, K = max, tie, ratio-3 and the full model, the program's selected blocks read exactly the tokens transformers' indexer keeps (an all-zero indexer included: the tie rule is the lower index) |
| reference ↔ ref2 ↔ exec | logits and every commit point equal, all fixtures |
| court | the demand evaluator reproduces every node of every occurrence from the committed leaves alone, with `Fixed` state replayed between checkpoints, and the dissection arithmetic of every reduction over `H`; all **25** primitives are evaluated |
| admission (`tir_admit_v1`) | admitted: largest block 450 nodes (NF-12: 512), 6–9 blocks, worst court terminal 4,096 MACs (16,384 at 512 experts) |
| the published shape (2048 wide, 40 layers, 512 experts of top 10, 4 streams, 16 hash heads of 20 M rows; declared, never instantiated: peak 23 MB) | lowers to 9 blocks, largest 450 nodes, the PLE block 196; every head's table takes two chunks (`[16, 2^24, 128]` and `[16, 3,225,183, 128]`); admitted at the legacy court ceilings (187 cones, cone work 6,120, 131,318 step leaves, 2.5e10 MACs a position) |
| the in-program hash at that scale | equal to the reference function on random tokens at a vocabulary of 248,320 and head tables of 20 M rows, through the range analysis (`lower::generic::tests`) — the function alone, never the table |
| robustness | 1,378 single-key mutations of two configs (a key deleted, or set to a null, 0, 1, 2, 3, 7, 65, −1, a float, a boolean, a string, a list): no panic, 710 lowered, the rest refused by name; 45 hostile huge numbers in the keys that size a feature: 40 refused before anything is sized on them, none allocates |

**Hardening this found.** A mutated config reached an 85 GB allocation through `head_dim = 10^12` (the rope's
frequency vector) and `hc_count = 10^12` (the stream repeat): rotary dimensions past 4,096 are refused, as are
more than 64 streams, more than 1,024 hash heads, a convolution past 64 taps or 4,096 rows and a block-key matrix
past 2^27 elements; a zero-tap convolution and a hash base of 0 were panics and are refusals now. The test binary
runs under an allocator that aborts on one allocation over 512 MiB, its mutants are small numbers, and each is sized
by arithmetic over what it declares.

**Limits found.**
- The PLE table of a real checkpoint (hundreds of millions of rows × `dim` × the PLE layers) is
  hundreds of GB: the lowering has no dimension problem, but an in-memory fill of it does. A row-wise
  (streamed) fill of the chunk params is the missing piece, not a primitive.
- The block-key matrix is written by a one-hot `Select` over `[blocks, dim]` (cost `O(blocks·dim)` a
  position, small beside the matmuls at published sizes); a dynamic-row write would make it `O(dim)`.
- `ple_layer_ids` are 1-based in the config and the tensors are named by the 0-based layer; the
  adapter does the shift and the fixtures pin it against transformers' ids.

## 22. `LOGITS_Q24_V1` (lowering version 2): a text program's logits are natural-log units × 2^24 (2026-10-01)

Until version 2 the logits left the head at a calibrated static scale — an arbitrary real
(`2^−27.5 … 2^−23.7` on the tiny fixtures) — so a sampler had to read the scale from the pack, and a
temperature meant something different for every class. Version 2 fixes the unit: **the logits are
natural-log units × 2^24 in an `i32`, whatever the model.**
- **What moved.** The head's last narrowing lands on `2^−24` (`ScaleKey::q24`) instead of the
  site's calibrated scale: the same nodes, other `(m, s)` params. The program's digest, node count
  and param count are identical to version 1's; only the artifact changes. The version-1 golden files
  hold the programs (what a deliberate change must NOT move) and `tests/golden/lowering_v2{,_libm}.json`
  pin the artifacts of version 2. `LOWERING_VERSION` is recorded in the convert and fidelity provenance.
- **The bound.** `i32` at `2^−24` holds `|logit| < 128`. A model whose calibrated logits reach 120
  natural-log units is refused (`NOT_LOWERABLE`, `LOGITS_Q24_V1`), not clipped; an input beyond 128 at
  run time saturates. Two synthetic GGUF fixtures whose random weights reached 200+ were regenerated
  with a final-norm gain.
- **Cost.** None on the logits tile: the close carries `i32` logits as before, so close sizes and the
  court's cone for the logits node are unchanged.
- **Check.** `tests/logits_q24.rs`: the logits scale is exactly `2^−24`; the integer logits divided by
  `2^24` agree with transformers' on eight head shapes (plain, tied, soft-capped, scaled, MoE,
  recurrent, learned-position, Gemma-4) within the quantisation's error and top-1 agrees wherever
  transformers leads by more than 8× that error; a head scaled to hundreds is refused by name.

## 23. The corpus measured on the integrated tree (lane R2, 2026-10-03)

`tir/generic` (the feature lowerers, the data adapters, fixtures 93) and `tir/corpus` (the 100-architecture harness, `corpus-v2.md`) are
merged on `rfc2/rest`, and the harness's data-route probe (vision towers, convolutional networks, encoder–decoders, diffusers components)
is part of it. **Existing features only** — nothing was added to a lowerer for the measurement.

| Level | Entries | Share |
| --- | ---: | ---: |
| A — the standard keys and names | 8 | 8 % |
| B — a data adapter (built in, or a file the corpus carries) | 75 | 75 % |
| **A + B** | **83** | **83 %** (usage-weighted 84.7 %; text-generation 50 of 57) |
| C — a capability is missing | 17 | 17 % |

**The target was 90 % and is not reached with existing features**; the corpus itself says why. The 17 Level-C entries each name what they lack:
the text models need features (`diffllama`: differential attention; `gemma3n_text`: AltUp/LAuReL/activation sparsity, FR-12; `jetmoe`:
mixture-of-attention heads; `longcat_flash`: zero-computation experts with a shortcut branch; `deepseek_v4`: hyper-connections with compressed
sparse attention and hash routing; `zamba2`: one shared attention block at several depths; `kimi_linear`: channel-wise gated delta rule),
the multimodal ones capabilities (`mllama`: cross-attention to vision states, FR-21; the diffusers denoisers `unet2d_condition`, `unet_sdxl`,
`dit`, `flux`: FR-22; the audio models `wav2vec2`, `speecht5`, `musicgen`, `encodec`: FR-23) and `chatglm3` a remote-code reference (FR-24).
Seven more entries would be needed for 90 %; the cheapest are the seven text features, each a general lowerer and not a per-model path.

**The weights-bearing stages are unchanged** from `corpus-v2.md`: of the entries that read, every one with weights passes admission, the float
reference against `transformers`, the integer program against the float reference, reference ↔ ref2 ↔ exec bit identity and the court property.

**A golden preflight per entry** (`misaka-palw-sdk/tests/corpus_preflight.rs`, pins in `tests/golden/corpus_preflight_v1.json`): each of the 100
entries is preflighted at the `shape` depth on testnet-12, from its committed light spec written back as header-only safetensors, at a declared
context of 128 positions, and its convert/register/mine statuses and blocker codes are pinned. The pins are the claim a user would be shown; a
change to a feature, an adapter, a check or a code moves one by name.
