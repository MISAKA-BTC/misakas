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
- **HL グラフは frontend 中立**: HF のキー名・テンソル名・融合レイアウトは `hf_config` と `hf_weights` にしかなく、HL の
  op と param は数学的な属性(rope 変種、norm の `1+w`、GQA のグルーピング規則など)だけを持つ(テストで強制)。
- **推定カバー率**: HF の decoder-only テキスト生成 checkpoint の約 **91%**(件数ベース、`architectures[0]` 別)。
- **HF 側の曖昧さ**: 参照そのものが割れている箇所がある(Gemma-2 の soft-cap を sdpa が落とす、Falcon の eager ALiBi が
  5.17 で二重加算、dynamic NTK/LongRoPE の prefill と decode の不一致、DeepSeek-V2 の native と remote code の不一致)。
  RFC の基準 5「HF float 原典」は **transformers のバージョン・eager attention・位置ごと(decode)意味論** を固定すべき。
- **GDN**: HF は value head `vh` に key head `vh / (nv/nk)`(grouping)を当てる。live の Q36 kernel は `vh % nk`(tiling)。
  両者は value head の並べ替えで一致する(単体テストで証明)。HF から lower する限り grouping。

## 1. What "covered" means here

A decoder architecture is **covered** when all of the following hold:

1. `hf_config` parses its `config.json` into an `ArchSpec` using the config class's own defaults
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
a new `hf_config` parser over existing HL ops unless stated):

| Architecture | Why not yet |
| --- | --- |
| `Llama4ForCausalLM`/`…ConditionalGeneration` | chunked attention (a new window shape), NoPE layers with attention temperature tuning, Llama-4 MoE — needs a `chunked` Hist read |
| `Gemma3nForConditionalGeneration` | AltUp, Laurel, per-layer embeddings, activation sparsity (a top-k-by-value gate) |
| `Glm4ForCausalLM`, `GlmForCausalLM`, `Glm4MoeForCausalLM`, ChatGLM (remote) | not modelled yet; GLM-4.5 is DeepSeek-V3-style routing with partial rope — mostly existing ops |
| `Olmo3ForCausalLM`, `GraniteMoeHybridForCausalLM`, `NemotronHForCausalLM`, `FalconH1ForCausalLM`, `Zamba2ForCausalLM`, `BambaForCausalLM` | hybrids of existing ops (Mamba2 + attention); config parsers not written; Falcon-H1 adds µP multipliers |
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

**Frontend neutrality.** HF config keys, tensor names and fused layouts live only in `hf_config`
(`ArchSpec::hf: HfStorage`) and `hf_weights` (param ← tensor expression: row slices of fused tensors,
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
| weights | W8: `i8` per output row; `(m, s, z)` as three typed per-channel params (`i64`, `i8`, `i64`) |
| norm unit rows, softmax probabilities, decays, gates | Q24 `i32` |
| attention and router logits | Q14 `i32` (±131,072: Qwen2.5's layer-0 logits reach 24,000) |
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
export CARGO_TARGET_DIR=…/tir-lower-target CARGO_BUILD_JOBS=4
cargo test --release -p misaka-palw-tir-lower  # 190 tests; HF fixtures are skipped if absent
HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 \
  …/tir-venv/bin/python misaka-palw-tir-lower/tools/gen_hf_fixtures.py [name …]   # regenerate
cargo run -p misaka-palw-tir-lower --bin palw-tir-check -- \
  --config misaka-palw-tir-lower/tests/configs/real/qwen3-next-80b-a3b-instruct.json [--tile-len 64 --h-chunk 64]
# The real checkpoint (one process at a time; ~7.4 GB peak):
HF_HUB_OFFLINE=1 …/tir-venv/bin/python misaka-palw-tir-lower/tools/tokenize_docs.py …   # calib / eval token files
cargo run --release -p misaka-palw-tir-lower --bin palw-tir-fidelity -- ~/Downloads/Qwen2.5-1.5B-Instruct \
  --calib qwen25-calib.json --eval qwen25-eval.json --eval-seqs 2 --positions 128 \
  --artifact-out qwen25.palwtir --tir-out qwen25.tir --json
```
