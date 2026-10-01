#!/usr/bin/env python3
"""The architecture corpus v2 — one entry per architecture (the single source of truth).

Lane H (corpus and coverage). The corpus answers one question on a representative set of Hugging
Face architectures: *can this model be added permissionlessly — by data alone, with no core
developer action?* Each entry carries

* what it is (`id`, `category`, `route`, `hf_arch` = the `architectures[0]` of published
  checkpoints, `model_type`);
* why it is in the corpus (`why`) and how much of the hub it stands for (`usage` tier and, where
  `hf-coverage.md` §2 has one, `share`);
* how to build its tiny random-init fixture (`builder`, `cfg`, `options`) — `transformers` /
  `diffusers` config classes only, fixed seeds, nothing downloaded.

`corpus_v2.json` is exported from this file (`gen_fixtures.py --manifest`); the Rust harness
(`tests/corpus_v2.rs`) and the report renderer read that JSON.

Honesty about weights. The hub cannot be queried offline. `share` is `hf-coverage.md` §2's
estimate of the architecture's share of decoder-only text-generation repositories by repository
count (±30 % relative); `usage` is a coarse ordinal tier (`vh` very high … `l` low) by this lane's
recollection of hub download rankings through 2025/26, ±1 tier. Neither is measured.
"""

import copy
import json
import os
import sys

V = 64  # vocabulary of every tiny model

# usage tier -> weight in the usage-weighted summaries (a coarse ordinal scale, stated as such)
USAGE_WEIGHT = {"vh": 8, "h": 4, "m": 2, "l": 1}

# transformers config keys shrunk to a tiny model when the class has them (`auto_tiny`)
SHRINK = {
    "vocab_size": V, "hidden_size": 32, "n_embd": 32, "d_model": 32, "dim": 32,
    "intermediate_size": 64, "ffn_dim": 64, "d_ff": 64, "hidden_dim": 64, "ffn_hidden_size": 64,
    "num_hidden_layers": 2, "n_layer": 2, "num_layers": 2, "n_layers": 2,
    "num_attention_heads": 4, "n_head": 4, "num_heads": 4, "n_heads": 4,
    "num_key_value_heads": 2, "max_position_embeddings": 128,
}
TOKEN_IDS = {"pad_token_id": 0, "bos_token_id": 1, "eos_token_id": 2}


def entry(id, category, route, hf_arch, model_type, builder, cfg=None, *, usage, share=None, why="",
          examples=(), options=None, real_config=None, tiny=True, note=""):
    return {
        "id": id, "category": category, "route": route, "hf_arch": hf_arch, "model_type": model_type,
        "builder": builder, "cfg": cfg or {}, "usage": usage, "share": share, "why": why,
        "examples": list(examples), "options": options or {}, "real_config": real_config, "tiny": tiny,
        "note": note,
    }


ENTRIES = []


def add(*a, **k):
    ENTRIES.append(entry(*a, **k))


# ─────────────────────────────── A. text generation — dense decoders already in the pack ───────────────────────────────
CAT = "text/dense"
add("llama", CAT, "decoder", "LlamaForCausalLM", "llama", "causal",
    dict(num_key_value_heads=2, tie_word_embeddings=False, max_position_embeddings=128,
         rope_parameters={"rope_type": "llama3", "rope_theta": 500000.0, "factor": 8.0, "low_freq_factor": 1.0,
                          "high_freq_factor": 4.0, "original_max_position_embeddings": 16}),
    usage="vh", share=30.0, why="the lineage a third of text-generation repositories derive from (Llama 1-3.3, TinyLlama, Vicuna, Yi, DeepSeek-LLM, SmolLM, CodeLlama)",
    examples=["meta-llama/Llama-3.1-8B-Instruct", "TinyLlama/TinyLlama-1.1B-Chat-v1.0", "01-ai/Yi-6B"],
    real_config="llama-3.1-8b")
add("qwen2", CAT, "decoder", "Qwen2ForCausalLM", "qwen2", "causal",
    dict(num_key_value_heads=2, tie_word_embeddings=True, use_sliding_window=False),
    usage="vh", share=20.0, why="q/k/v bias, tied head: the second-largest family (Qwen1.5/2/2.5, QwQ, R1-Distill-Qwen)",
    examples=["Qwen/Qwen2.5-7B-Instruct", "deepseek-ai/DeepSeek-R1-Distill-Qwen-7B"], real_config="qwen2.5-0.5b")
add("mistral", CAT, "decoder", "MistralForCausalLM", "mistral", "causal",
    dict(num_key_value_heads=2, head_dim=8, sliding_window=4),
    usage="vh", share=8.0, why="sliding-window GQA (Mistral, Zephyr, OpenHermes, Nemo)",
    examples=["mistralai/Mistral-7B-Instruct-v0.3"], real_config="mistral-7b-v0.1")
add("gpt2", CAT, "decoder", "GPT2LMHeadModel", "gpt2", "causal",
    dict(n_positions=64, scale_attn_by_inverse_layer_idx=True),
    usage="vh", share=7.0, why="learned positions, Conv1D weights, fused c_attn: the pre-Llama lineage's head",
    examples=["openai-community/gpt2", "distilbert/distilgpt2"], real_config="gpt2")
add("qwen3", CAT, "decoder", "Qwen3ForCausalLM", "qwen3", "causal",
    dict(num_key_value_heads=2, head_dim=8, tie_word_embeddings=True),
    usage="vh", share=6.0, why="per-head QK-norm (the dense Qwen of 2025)", examples=["Qwen/Qwen3-8B", "Qwen/Qwen3-0.6B"],
    real_config="qwen3-8b")
add("gemma2", CAT, "decoder", "Gemma2ForCausalLM", "gemma2", "causal",
    dict(head_dim=8, num_key_value_heads=2, sliding_window=4, query_pre_attn_scalar=8, attn_logit_softcapping=50.0,
         final_logit_softcapping=30.0, layer_types=["sliding_attention", "full_attention"]),
    usage="h", share=2.0, why="sandwich norms, score and logit soft-caps, alternating windows", examples=["google/gemma-2-9b-it"],
    real_config="gemma-2-9b")
add("gemma3_text", CAT, "decoder", "Gemma3ForCausalLM", "gemma3_text", "causal",
    dict(head_dim=8, num_key_value_heads=2, num_hidden_layers=3, sliding_window=4, query_pre_attn_scalar=8,
         layer_types=["sliding_attention", "sliding_attention", "full_attention"],
         rope_parameters={"sliding_attention": {"rope_type": "default", "rope_theta": 10000.0},
                          "full_attention": {"rope_type": "linear", "factor": 8.0, "rope_theta": 1000000.0}}),
    usage="vh", share=3.0, why="5:1 sliding/global, two rope tables, QK-norm (text of Gemma-3 1B and the VLM)",
    examples=["google/gemma-3-1b-it"], real_config="gemma-3-1b-it")
add("gemma4_text", CAT, "decoder", "Gemma4ForCausalLM", "gemma4_text", "causal",
    dict(num_hidden_layers=3, head_dim=8, global_head_dim=16, num_global_key_value_heads=1, attention_k_eq_v=True,
         sliding_window=4, layer_types=["sliding_attention", "sliding_attention", "full_attention"],
         hidden_size_per_layer_input=8, vocab_size_per_layer_input=V, enable_moe_block=True, num_experts=4,
         top_k_experts=2, moe_intermediate_size=16, final_logit_softcapping=5.0),
    usage="h", why="2026 flagship dense+MoE: per-layer inputs, K=V globals, KV sharing in the E models",
    examples=["google/gemma-4-*"], tiny=False)
add("phi3", CAT, "decoder", "Phi3ForCausalLM", "phi3", "causal",
    dict(num_key_value_heads=4, pad_token_id=0, bos_token_id=1, eos_token_id=2, max_position_embeddings=32,
         original_max_position_embeddings=8, sliding_window=None,
         rope_scaling={"type": "longrope", "short_factor": [1.0, 1.5, 2.0, 2.5], "long_factor": [2.0, 4.0, 8.0, 16.0]}),
    options={"decode": True}, usage="h", share=2.0,
    why="fused qkv and gate_up, LongRoPE (the 128k variants): per-position rope semantics",
    examples=["microsoft/Phi-3.5-mini-instruct", "microsoft/phi-4"], real_config="phi-3.5-mini-instruct")
add("gpt_neox", CAT, "decoder", "GPTNeoXForCausalLM", "gpt_neox", "causal",
    dict(rotary_pct=0.25, use_parallel_residual=True), usage="h", share=2.0,
    why="parallel residual with two LayerNorms, partial rotary, per-head fused qkv (Pythia, Dolly, RedPajama)",
    examples=["EleutherAI/pythia-1.4b"], real_config="pythia-1.4b")
add("opt", CAT, "decoder", "OPTForCausalLM", "opt", "causal",
    dict(ffn_dim=64, max_position_embeddings=64), usage="m", share=1.5,
    why="learned positions at pos+2, ReLU MLP, (OPT-350m) post-LN and a projected embedding", examples=["facebook/opt-1.3b"],
    real_config="opt-125m")
add("falcon", CAT, "decoder", "FalconForCausalLM", "falcon", "causal",
    dict(new_decoder_architecture=True, num_kv_heads=2, bias=False), usage="m", share=1.0,
    why="kv-group fused qkv, parallel attention, ALiBi in the RW variants", examples=["tiiuae/falcon-40b"], real_config="falcon-40b")
add("starcoder2", CAT, "decoder", "Starcoder2ForCausalLM", "starcoder2", "causal",
    dict(num_key_value_heads=2, sliding_window=4, use_bias=True), usage="m", share=0.5,
    why="LayerNorm + biases, plain GELU MLP, window (code models)", examples=["bigcode/starcoder2-3b"], real_config="starcoder2-3b")
add("cohere2", CAT, "decoder", "Cohere2ForCausalLM", "cohere2", "causal",
    dict(head_dim=8, num_key_value_heads=2, num_hidden_layers=4, sliding_window=4,
         layer_types=["sliding_attention", "sliding_attention", "sliding_attention", "full_attention"]),
    usage="m", share=0.3, why="parallel residual from ONE bias-free LayerNorm, interleaved rope on sliding layers only",
    examples=["CohereForAI/c4ai-command-r7b-12-2024"], real_config="c4ai-command-r7b-12-2024")
add("granite", CAT, "decoder", "GraniteForCausalLM", "granite", "causal",
    dict(num_key_value_heads=2, embedding_multiplier=2.0, residual_multiplier=0.5, attention_multiplier=0.2, logits_scaling=3.0),
    usage="m", share=0.5, why="muP-style multipliers on embedding, branches, attention and logits", examples=["ibm-granite/granite-3.1-8b-instruct"],
    real_config="granite-3.1-8b-instruct")
add("glm4", CAT, "decoder", "Glm4ForCausalLM", "glm4", "causal",
    dict(head_dim=8, num_key_value_heads=2, pad_token_id=0), usage="m", share=0.3,
    why="fused gate_up, partial interleaved rope, post-attention/post-MLP norms (GLM-4-0414, Z1)", examples=["THUDM/GLM-4-9B-0414"])
add("smollm3", CAT, "decoder", "SmolLM3ForCausalLM", "smollm3", "causal",
    dict(num_hidden_layers=4, num_key_value_heads=2, no_rope_layer_interval=2, pad_token_id=0, bos_token_id=1, eos_token_id=2),
    usage="m", share=0.3, why="NoPE every n-th layer", examples=["HuggingFaceTB/SmolLM3-3B"], real_config="smollm3-3b")

# ─────────────────────────────── B. dense decoders NOT in the pack (2025-26 and long tail) ───────────────────────────────
add("arcee", "text/dense", "decoder", "ArceeForCausalLM", "arcee", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="l", why="Llama with a non-gated ReLU² MLP (AFM-4.5B)", examples=["arcee-ai/AFM-4.5B"])
add("apertus", "text/dense", "decoder", "ApertusForCausalLM", "apertus", "causal",
    dict(num_key_value_heads=2, head_dim=8,
         rope_parameters={"rope_type": "llama3", "rope_theta": 12000000.0, "factor": 8.0, "original_max_position_embeddings": 16,
                          "low_freq_factor": 1.0, "high_freq_factor": 4.0}),
    usage="l", why="xIELU activation with learned per-layer parameters, QK-norm, llama3 rope (Swiss AI)", examples=["swiss-ai/Apertus-8B-Instruct-2509"])
add("helium", "text/dense", "decoder", "HeliumForCausalLM", "helium", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="l", why="Llama-shaped (Kyutai) with its own norm epsilon: a Level A control", examples=["kyutai/helium-1-2b"])
add("hunyuan_v1_dense", "text/dense", "decoder", "HunYuanDenseV1ForCausalLM", "hunyuan_v1_dense", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="m", why="Llama + QK-norm placed after the rotation (Hunyuan-7B/4B/1.8B)", examples=["tencent/Hunyuan-7B-Instruct"])
add("seed_oss", "text/dense", "decoder", "SeedOssForCausalLM", "seed_oss", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="l", why="Llama with q/k/v bias and a bias-free output projection (ByteDance Seed-OSS)", examples=["ByteDance-Seed/Seed-OSS-36B-Instruct"])
add("ernie4_5", "text/dense", "decoder", "Ernie4_5ForCausalLM", "ernie4_5", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="m", why="Llama with GLM-style interleaved rotary pairs (ERNIE 4.5 dense)", examples=["baidu/ERNIE-4.5-0.3B-PT"])
add("bitnet", "text/dense", "decoder", "BitNetForCausalLM", "bitnet", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="l", why="ternary weights with per-token int8 activation quantisation and sub-layer norms (BitNet b1.58)", examples=["microsoft/bitnet-b1.58-2B-4T"])
add("persimmon", "text/dense", "decoder", "PersimmonForCausalLM", "persimmon", "causal",
    dict(), usage="l", why="LayerNorm, QK layer-norm, ReLU², partial rotary, per-head fused qkv (Adept)", examples=["adept/persimmon-8b-base"])
add("diffllama", "text/dense", "decoder", "DiffLlamaForCausalLM", "diffllama", "causal",
    dict(num_key_value_heads=2, head_dim=8), usage="l", why="differential attention: the difference of two softmax maps", examples=["kajuma/DiffLlama-0.3B-handson"])
add("cwm", "text/dense", "decoder", "CwmForCausalLM", "cwm", "causal",
    dict(num_key_value_heads=2, head_dim=8, sliding_window=4, layer_types=["full_attention", "sliding_attention"]),
    usage="l", why="Llama with per-layer sliding windows and llama3 rope (Code World Model)", examples=["facebook/cwm"])
add("gemma3n_text", "text/dense", "decoder", "Gemma3nForCausalLM", "gemma3n_text", "causal",
    dict(num_hidden_layers=6, intermediate_size=[64] * 6, head_dim=8, num_attention_heads=4, num_key_value_heads=2,
         sliding_window=4, layer_types=["sliding_attention", "sliding_attention", "sliding_attention", "full_attention",
                                        "sliding_attention", "full_attention"],
         num_kv_shared_layers=2, activation_sparsity_pattern=[0.95, 0.95, 0.0, 0.0, 0.0, 0.0], altup_num_inputs=4,
         laurel_rank=4, hidden_size_per_layer_input=8, vocab_size_per_layer_input=V, final_logit_softcapping=30.0),
    usage="h", why="AltUp streams, LAuReL, per-layer embeddings, activation sparsity, KV sharing (Gemma-3n E2B/E4B)",
    examples=["google/gemma-3n-E4B-it"], real_config="gemma-3n-e4b")

# ─────────────────────────────── C. MoE decoders already in the pack ───────────────────────────────
add("mixtral", "text/moe", "decoder", "MixtralForCausalLM", "mixtral", "causal",
    dict(num_key_value_heads=2, num_local_experts=4, num_experts_per_tok=2, sliding_window=None), usage="h", share=1.0,
    why="softmax top-2 of 8 experts, renormalised", examples=["mistralai/Mixtral-8x7B-Instruct-v0.1"], real_config="mixtral-8x7b-v0.1")
add("qwen2_moe", "text/moe", "decoder", "Qwen2MoeForCausalLM", "qwen2_moe", "causal",
    dict(num_key_value_heads=2, num_experts=4, num_experts_per_tok=2, moe_intermediate_size=16,
         shared_expert_intermediate_size=24, mlp_only_layers=[0]), usage="m", share=0.5,
    why="sigmoid-gated shared expert, dense first layer", examples=["Qwen/Qwen1.5-MoE-A2.7B"], real_config="qwen1.5-moe-a2.7b")
add("qwen3_moe", "text/moe", "decoder", "Qwen3MoeForCausalLM", "qwen3_moe", "causal",
    dict(num_key_value_heads=2, head_dim=8, num_experts=4, num_experts_per_tok=2, moe_intermediate_size=16, norm_topk_prob=True),
    usage="h", share=0.5, why="QK-norm + renormalised top-k, no shared expert (Qwen3-30B-A3B, 235B)", examples=["Qwen/Qwen3-30B-A3B"], real_config="qwen3-30b-a3b")
add("granitemoe", "text/moe", "decoder", "GraniteMoeForCausalLM", "granitemoe", "causal",
    dict(num_key_value_heads=2, num_local_experts=4, num_experts_per_tok=2, intermediate_size=16, embedding_multiplier=2.0,
         residual_multiplier=0.5, attention_multiplier=0.2, logits_scaling=3.0), usage="l",
    why="top-k of logits then softmax; fused expert tensors", examples=["ibm-granite/granite-3.1-3b-a800m-instruct"], real_config="granite-3.1-3b-a800m")
add("deepseek_v3", "text/moe", "decoder", "DeepseekV3ForCausalLM", "deepseek_v3", "causal",
    dict(head_dim=4, num_key_value_heads=4, first_k_dense_replace=1, n_routed_experts=8, n_shared_experts=1,
         num_experts_per_tok=2, moe_intermediate_size=16, kv_lora_rank=8, q_lora_rank=12, qk_rope_head_dim=4,
         qk_nope_head_dim=8, v_head_dim=8, n_group=4, topk_group=2, routed_scaling_factor=2.5,
         rope_scaling={"rope_type": "yarn", "factor": 4.0, "original_max_position_embeddings": 32, "mscale": 1.0, "mscale_all_dim": 1.0}),
    usage="h", share=0.5, why="MLA + sigmoid routing with selection bias and group-limited top-k (DeepSeek-V3/R1, Kimi-K2)",
    examples=["deepseek-ai/DeepSeek-V3", "deepseek-ai/DeepSeek-R1"], real_config="deepseek-v3-bf16")
add("gpt_oss", "text/moe", "decoder", "GptOssForCausalLM", "gpt_oss", "causal",
    dict(num_key_value_heads=2, head_dim=8, num_local_experts=4, num_experts_per_tok=2, intermediate_size=16, sliding_window=4,
         rope_scaling={"rope_type": "yarn", "factor": 4.0, "beta_fast": 32.0, "beta_slow": 1.0, "truncate": False,
                       "original_max_position_embeddings": 32}), usage="vh",
    why="attention sinks, clamped SwiGLU, interleaved fused experts, YaRN without truncation", examples=["openai/gpt-oss-20b", "openai/gpt-oss-120b"],
    real_config="gpt-oss-20b-bf16")
add("llama4_text", "text/moe", "decoder", "Llama4ForCausalLM", "llama4_text", "causal",
    dict(num_key_value_heads=2, num_hidden_layers=4, head_dim=8, num_local_experts=4, num_experts_per_tok=1,
         intermediate_size=16, intermediate_size_mlp=48, interleave_moe_layer_step=2, no_rope_layer_interval=2,
         attention_chunk_size=4, floor_scale=3, attn_scale=0.5,
         rope_parameters={"rope_type": "llama3", "factor": 8.0, "low_freq_factor": 1.0, "high_freq_factor": 4.0,
                          "original_max_position_embeddings": 16, "rope_theta": 500000.0}),
    usage="h", why="chunked attention, NoPE layers with query temperature, top-1 sigmoid MoE scaling the expert input", examples=["meta-llama/Llama-4-Scout-17B-16E-Instruct"])
add("glm4_moe", "text/moe", "decoder", "Glm4MoeForCausalLM", "glm4_moe", "causal",
    dict(num_key_value_heads=2, head_dim=8, first_k_dense_replace=1, n_routed_experts=8, n_shared_experts=1,
         num_experts_per_tok=2, moe_intermediate_size=16, n_group=4, topk_group=2, routed_scaling_factor=2.5,
         attention_bias=True, use_qk_norm=True, partial_rotary_factor=0.5, pad_token_id=0),
    usage="h", why="GLM-4.5/4.6: DeepSeek-V3 routing over GQA with partial rotary", examples=["zai-org/GLM-4.5", "zai-org/GLM-4.6"])

# ─────────────────────────────── D. MoE decoders NOT in the pack ───────────────────────────────
add("dbrx", "text/moe", "decoder", "DbrxForCausalLM", "dbrx", "causal",
    dict(attn_config={"clip_qkv": 8.0, "kv_n_heads": 2, "rope_theta": 10000.0},
         ffn_config={"ffn_hidden_size": 64, "moe_num_experts": 4, "moe_top_k": 2}, num_key_value_heads=2), usage="l", why="fused expert tensors, QKV clip, bias-free LayerNorm (Databricks)", examples=["databricks/dbrx-instruct"])
add("jetmoe", "text/moe", "decoder", "JetMoeForCausalLM", "jetmoe", "causal",
    dict(), usage="l", why="MoE for BOTH the MLP and the attention (mixture-of-attention heads)", examples=["jetmoe/jetmoe-8b"])
add("ernie4_5_moe", "text/moe", "decoder", "Ernie4_5_MoeForCausalLM", "ernie4_5_moe", "causal",
    dict(), usage="m", why="softmax router with a correction bias, shared experts, interleaved rotary (ERNIE-4.5-21B/300B-A47B)", examples=["baidu/ERNIE-4.5-21B-A3B-PT"])
add("hunyuan_v1_moe", "text/moe", "decoder", "HunYuanMoEV1ForCausalLM", "hunyuan_v1_moe", "causal",
    dict(head_dim=8), usage="m", why="Hunyuan-A13B: QK-norm, shared expert, top-k routing", examples=["tencent/Hunyuan-A13B-Instruct"])
add("minimax_m2", "text/moe", "decoder", "MiniMaxM2ForCausalLM", "minimax_m2", "causal",
    dict(), usage="m", why="sigmoid router + bias, QK-norm over the whole projection, partial rotary (MiniMax-M2)", examples=["MiniMaxAI/MiniMax-M2"])
add("longcat_flash", "text/moe", "decoder", "LongcatFlashForCausalLM", "longcat_flash", "causal",
    dict(num_layers=2, head_dim=4, ffn_hidden_size=64, q_lora_rank=12, kv_lora_rank=8, qk_nope_head_dim=8,
         qk_rope_head_dim=4, v_head_dim=8, qk_head_dim=12, moe_topk=2, n_routed_experts=8, zero_expert_num=2,
         expert_ffn_hidden_size=16, num_key_value_heads=4), usage="l", why="MLA, zero-computation experts and a shortcut-connected dense branch (Meituan LongCat-Flash)", examples=["meituan-longcat/LongCat-Flash-Chat"])
add("deepseek_v32", "text/moe", "decoder", "DeepseekV32ForCausalLM", "deepseek_v32", "causal",
    dict(head_dim=4, num_key_value_heads=4, index_n_heads=2, index_head_dim=8, index_topk=4, kv_lora_rank=8,
         q_lora_rank=12, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8, n_routed_experts=8, n_shared_experts=1,
         n_group=4, topk_group=2, num_experts_per_tok=2, moe_intermediate_size=16, first_k_dense_replace=1,
         layer_types=["deepseek_sparse_attention"] * 2, mlp_layer_types=["dense", "sparse"]), usage="h", why="DeepSeek sparse attention: a learned lightning indexer picks the top-k keys (V3.2)", examples=["deepseek-ai/DeepSeek-V3.2"])
add("deepseek_v4", "text/moe", "decoder", "DeepseekV4ForCausalLM", "deepseek_v4", "causal",
    dict(), usage="h", why="2026 DeepSeek: hyper-connections, compressed sparse attention, hash-routed experts", examples=["deepseek-ai/DeepSeek-V4"])

# ─────────────────────────────── E. hybrids, SSMs, recurrent models already in the pack ───────────────────────────────
add("qwen3_next", "text/hybrid", "decoder", "Qwen3NextForCausalLM", "qwen3_next", "causal",
    dict(num_hidden_layers=4, num_key_value_heads=2, head_dim=8, num_experts=4, num_experts_per_tok=2, moe_intermediate_size=16,
         shared_expert_intermediate_size=16, linear_key_head_dim=8, linear_value_head_dim=8, linear_num_key_heads=2,
         linear_num_value_heads=4, mlp_only_layers=[1]), usage="h", share=0.5,
    why="gated delta rule (3:1 with attention), sigmoid-gated attention, shared-expert MoE", examples=["Qwen/Qwen3-Next-80B-A3B-Instruct"], real_config="qwen3-next-80b-a3b-instruct")
add("qwen3_5_moe", "text/hybrid", "decoder", "Qwen3_5MoeForCausalLM", "qwen3_5_moe_text", "causal",
    dict(num_hidden_layers=4, num_key_value_heads=2, head_dim=8, linear_key_head_dim=8, linear_value_head_dim=8,
         linear_num_key_heads=2, linear_num_value_heads=4, num_experts=4, num_experts_per_tok=2, moe_intermediate_size=16,
         shared_expert_intermediate_size=16), usage="h", why="Qwen3.5 (split GDN projections) with MoE", examples=["Qwen/Qwen3.5-35B-A3B"])
add("jamba", "text/hybrid", "decoder", "JambaForCausalLM", "jamba", "causal",
    dict(num_hidden_layers=4, num_key_value_heads=2, num_experts=4, num_experts_per_tok=2, attn_layer_period=2,
         attn_layer_offset=1, expert_layer_period=2, expert_layer_offset=1, mamba_d_state=4, mamba_dt_rank=4, use_mamba_kernels=False),
    usage="m", why="Mamba-1 + attention + MoE interleaving", examples=["ai21labs/AI21-Jamba-1.5-Mini"], real_config="jamba-v0.1")
add("mamba", "text/hybrid", "decoder", "MambaForCausalLM", "mamba", "causal",
    dict(state_size=4, time_step_rank=4, use_bias=True), usage="m", why="selective scan (state-spaces/mamba-*-hf)", examples=["state-spaces/mamba-130m-hf"], real_config="mamba-130m-hf")
add("mamba2", "text/hybrid", "decoder", "Mamba2ForCausalLM", "mamba2", "causal",
    dict(state_size=4, num_heads=8, head_dim=8, n_groups=2, use_bias=True), usage="m",
    why="state-space duality, grouped B/C, gated RMSNorm (Codestral-Mamba)", examples=["mistralai/Mamba-Codestral-7B-v0.1"], real_config="mamba-codestral-7b-v0.1")
add("falcon_mamba", "text/hybrid", "decoder", "FalconMambaForCausalLM", "falcon_mamba", "causal",
    dict(state_size=4, time_step_rank=4), usage="l", why="Mamba with weightless B/C/dt norms", examples=["tiiuae/falcon-mamba-7b"], real_config="falcon-mamba-7b")
add("rwkv", "text/hybrid", "decoder", "RwkvForCausalLM", "rwkv", "causal",
    dict(num_hidden_layers=4, attention_hidden_size=32, rescale_every=2), usage="l", why="RWKV-4: a recurrent model with no attention at all",
    examples=["RWKV/rwkv-4-169m-pile"], real_config="rwkv-4-169m-pile")

# ─────────────────────────────── F. hybrids NOT in the pack ───────────────────────────────
add("falcon_h1", "text/hybrid", "decoder", "FalconH1ForCausalLM", "falcon_h1", "causal",
    dict(), usage="m", why="attention and Mamba-2 in PARALLEL inside every layer, muP multipliers (TII)", examples=["tiiuae/Falcon-H1-7B-Instruct"])
add("granitemoehybrid", "text/hybrid", "decoder", "GraniteMoeHybridForCausalLM", "granitemoehybrid", "causal",
    dict(num_key_value_heads=2, mamba_n_heads=8, mamba_d_head=8, mamba_d_state=4, mamba_n_groups=2, mamba_chunk_size=8,
         layer_types=["linear_attention", "full_attention"], num_local_experts=4, num_experts_per_tok=2,
         shared_intermediate_size=16), usage="m", why="Granite 4: Mamba-2 and attention layers with a shared-expert MoE per layer", examples=["ibm-granite/granite-4.0-h-small"])
add("zamba2", "text/hybrid", "decoder", "Zamba2ForCausalLM", "zamba2", "causal",
    dict(num_hidden_layers=3, layers_block_type=["linear_attention", "hybrid", "linear_attention"], hybrid_layer_ids=[1],
         mamba_d_state=4, n_mamba_heads=4, mamba_headdim=16, attention_hidden_size=64, attention_head_dim=16,
         num_key_value_heads=4, num_query_groups=4, kv_channels=16, use_mamba_kernels=False, adapter_rank=4), usage="l", why="Mamba-2 backbone with ONE shared attention block reused at several depths", examples=["Zyphra/Zamba2-7B-Instruct"])
add("nemotron_h", "text/hybrid", "decoder", "NemotronHForCausalLM", "nemotron_h", "causal",
    dict(), usage="m", why="single-block layers (Mamba-2, attention, MLP or MoE) in a pattern string", examples=["nvidia/NVIDIA-Nemotron-Nano-9B-v2"])
add("lfm2", "text/hybrid", "decoder", "Lfm2ForCausalLM", "lfm2", "causal",
    dict(), usage="m", why="gated short convolutions interleaved with attention (Liquid LFM2)", examples=["LiquidAI/LFM2-1.2B"])
add("kimi_linear", "text/hybrid", "decoder", "KimiLinearForCausalLM", "kimi_linear", "causal",
    dict(layer_types=["linear_attention", "full_attention"], mlp_layer_types=["dense", "sparse"], linear_head_dim=8,
         linear_num_heads=4, kv_lora_rank=8, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8, head_dim=4,
         num_experts=8, num_experts_per_token=2, moe_intermediate_size=16, num_shared_experts=1, num_key_value_heads=4), usage="m", why="Kimi Delta Attention (channel-wise gated delta rule) with MLA layers (Kimi-Linear-48B-A3B)", examples=["moonshotai/Kimi-Linear-48B-A3B-Instruct"])


# ─────────────────────────────── G. encoders and embedding models ───────────────────────────────
BERT = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4)
add("bert", "encoder", "encoder-bidir", "BertModel", "bert", "encoder",
    dict(BERT, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu", layer_norm_eps=1e-12, pad_token_id=0),
    options={"raw": True, "pad": 0, "lmax": 12}, usage="vh",
    why="the most downloaded encoder family: bert-base-uncased and every all-MiniLM / bge / e5 / gte sentence embedder built on it",
    examples=["google-bert/bert-base-uncased", "sentence-transformers/all-MiniLM-L6-v2", "BAAI/bge-base-en-v1.5"], real_config="bert-base-uncased")
add("roberta", "encoder", "encoder-bidir", "RobertaModel", "roberta", "encoder",
    dict(BERT, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0, eos_token_id=2),
    options={"raw": True, "pad": 1, "lmax": 12, "seqs": [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 2]]}, usage="vh",
    why="positions from padding_idx + 1; the base of XLM-R, CamemBERT, BGE-M3", examples=["FacebookAI/roberta-base"], real_config="roberta-base")
add("xlm_roberta", "encoder", "encoder-bidir", "XLMRobertaModel", "xlm-roberta", "encoder",
    dict(BERT, num_hidden_layers=3, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0, eos_token_id=2),
    options={"raw": True, "pad": 1, "lmax": 12, "seqs": [[0, 5, 2], [0, 40, 9, 17, 33, 21, 8, 30, 12, 2]]}, usage="vh",
    why="multilingual encoder and embedder backbone (BGE-M3, multilingual-e5)", examples=["FacebookAI/xlm-roberta-base", "BAAI/bge-m3"], real_config="xlm-roberta-base")
add("distilbert", "encoder", "encoder-bidir", "DistilBertModel", "distilbert", "encoder",
    dict(vocab_size=V, dim=32, hidden_dim=64, n_layers=2, n_heads=4, max_position_embeddings=32, activation="gelu", pad_token_id=0),
    options={"raw": True, "pad": 0, "lmax": 12}, usage="h", why="BERT without token types under its own names", examples=["distilbert/distilbert-base-uncased"], real_config="distilbert-base-uncased")
add("mpnet", "encoder", "encoder-bidir", "MPNetModel", "mpnet", "encoder",
    dict(BERT, max_position_embeddings=34, hidden_act="gelu", layer_norm_eps=1e-5, relative_attention_num_buckets=32, pad_token_id=1, bos_token_id=0, eos_token_id=2),
    options={"raw": True, "pad": 1, "lmax": 12, "seqs": [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 30, 12, 2]]}, usage="vh",
    why="all-mpnet-base-v2: bidirectional + a T5-bucket relative-position bias", examples=["sentence-transformers/all-mpnet-base-v2"], real_config="all-mpnet-base-v2")
add("deberta_v2", "encoder", "encoder-bidir", "DebertaV2Model", "deberta-v2", "encoder",
    dict(BERT, max_position_embeddings=32, relative_attention=True, pos_att_type=["p2c", "c2p"], position_buckets=8,
         max_relative_positions=-1, norm_rel_ebd="layer_norm", share_att_key=True, position_biased_input=False, type_vocab_size=0, pad_token_id=0),
    options={"raw": True, "pad": 0, "lmax": 12}, usage="h",
    why="disentangled attention (content-to-position and position-to-content terms): DeBERTa-v3 is the usual classification / NLI / reranker backbone",
    examples=["microsoft/deberta-v3-base"])
add("albert", "encoder", "encoder-bidir", "AlbertModel", "albert", "encoder",
    dict(vocab_size=V, embedding_size=16, hidden_size=32, num_hidden_layers=3, num_hidden_groups=1, num_attention_heads=4,
         intermediate_size=64, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu_new", pad_token_id=0),
    options={"raw": True, "pad": 0, "lmax": 12}, usage="m", why="one layer's weights shared across the depth, factorised embedding", examples=["albert/albert-base-v2"])
add("modernbert", "encoder", "encoder-bidir", "ModernBertModel", "modernbert", "encoder",
    dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=3, num_attention_heads=4, max_position_embeddings=64,
         global_attn_every_n_layers=3, local_attention=4, pad_token_id=0, bos_token_id=1, eos_token_id=2, cls_token_id=1, sep_token_id=2),
    options={"raw": True, "pad": 0, "lmax": 12, "seqs": [[1, 11, 25, 7, 2], [1, 40, 9, 17, 33, 21, 8, 2]]}, usage="h",
    why="2024-25 encoder: RoPE, GeGLU, alternating local/global attention, bias-free LayerNorm", examples=["answerdotai/ModernBERT-base"])
add("nomic_bert", "encoder", "encoder-bidir", "NomicBertModel", "nomic_bert", "encoder",
    dict(), options={"pad": 0, "lmax": 12}, usage="h", why="a RoPE BERT with a fused gated MLP: nomic-embed-text, a leading open embedder", examples=["nomic-ai/nomic-embed-text-v1.5"])
add("clip_text", "encoder", "encoder-causal", "CLIPTextModelWithProjection", "clip_text_model", "encoder",
    dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, max_position_embeddings=16,
         projection_dim=24, hidden_act="quick_gelu", bos_token_id=62, eos_token_id=63, pad_token_id=1, layer_norm_eps=1e-5),
    options={"raw": True, "class": "CLIPTextModelWithProjection", "pad": 1, "lmax": 12, "seqs": [[62, 5, 17, 33, 8, 63], [62, 40, 2, 29, 11, 50, 7, 21, 63]]},
    usage="vh", why="the text tower of CLIP and every Stable Diffusion text encoder; causal with learned positions", examples=["openai/clip-vit-large-patch14"])

# ─────────────────────────────── H. encoder-decoders ───────────────────────────────
BART = dict(vocab_size=V, d_model=32, encoder_layers=2, decoder_layers=2, encoder_attention_heads=4, decoder_attention_heads=4,
            encoder_ffn_dim=64, decoder_ffn_dim=64, max_position_embeddings=64)
T5K = dict(vocab_size=V, d_model=32, d_kv=8, d_ff=64, num_layers=2, num_decoder_layers=2, num_heads=4,
           relative_attention_num_buckets=8, relative_attention_max_distance=8, pad_token_id=0, eos_token_id=1, decoder_start_token_id=0)
add("t5", "encdec", "encdec", "T5ForConditionalGeneration", "t5", "seq2seq",
    dict(T5K, feed_forward_proj="relu"), options={"raw": True, "embed_mul": 0.3}, usage="vh",
    why="relative-position buckets, RMSNorm without a mean, unscaled scores: the encoder-decoder head (t5-small/base)", examples=["google-t5/t5-small"], real_config="t5-small")
add("t5_gated", "encdec", "encdec", "T5ForConditionalGeneration", "t5", "seq2seq",
    dict(T5K, d_kv=6, d_ff=48, feed_forward_proj="gated-gelu", tie_word_embeddings=False), options={"raw": True, "embed_mul": 0.1, "untie": True}, usage="vh",
    why="T5 v1.1 / Flan-T5 / UL2: gated-GELU FFN, an untied head (also the text encoder of Flux, SD3 and PixArt)", examples=["google/flan-t5-base"], real_config="flan-t5-base")
add("bart", "encdec", "encdec", "BartForConditionalGeneration", "bart", "seq2seq",
    dict(BART, activation_function="gelu", scale_embedding=False, pad_token_id=1, bos_token_id=0, eos_token_id=2, decoder_start_token_id=2,
         forced_bos_token_id=None, forced_eos_token_id=None), options={"raw": True, "embed_mul": 0.3}, usage="h",
    why="post-norm, learned positions at pos+2: summarisation and the BART/mBART family", examples=["facebook/bart-large-cnn"], real_config="bart-large-cnn")
add("mbart", "encdec", "encdec", "MBartForConditionalGeneration", "mbart", "seq2seq",
    dict(BART, activation_function="gelu", scale_embedding=True, pad_token_id=1, bos_token_id=0, eos_token_id=2, decoder_start_token_id=2,
         forced_bos_token_id=None, forced_eos_token_id=None), options={"raw": True, "embed_mul": 0.1}, usage="m",
    why="pre-norm BART with final norms: multilingual translation", examples=["facebook/mbart-large-50"], real_config="mbart-large-50")
add("marian", "encdec", "encdec", "MarianMTModel", "marian", "seq2seq",
    dict(BART, activation_function="swish", scale_embedding=True, pad_token_id=63, eos_token_id=0, decoder_start_token_id=63, forced_eos_token_id=None),
    options={"raw": True, "embed_mul": 0.3}, usage="h", why="sinusoidal positions: the Helsinki-NLP/opus-mt translation models (thousands of repositories)",
    examples=["Helsinki-NLP/opus-mt-en-de"], real_config="opus-mt-en-de")
add("longt5", "encdec", "encdec", "LongT5ForConditionalGeneration", "longt5", "seq2seq",
    dict(vocab_size=V, d_model=32, d_kv=8, d_ff=64, num_layers=2, num_heads=4, local_radius=2, encoder_attention_type="local",
         relative_attention_num_buckets=8, relative_attention_max_distance=8, pad_token_id=0, eos_token_id=1, decoder_start_token_id=0),
    options={"raw": True, "embed_mul": 0.3}, usage="l", why="T5 with local / transient-global encoder attention: long-input summarisation", examples=["google/long-t5-tglobal-base"])
add("t5_encoder", "encdec", "encdec", "T5EncoderModel", "t5", "encoder",
    dict(T5K, feed_forward_proj="gated-gelu", tie_word_embeddings=False),
    options={"raw": True, "class": "T5EncoderModel", "pad": 0, "lmax": 12, "seqs": [[3, 4, 5, 6, 7, 8, 9, 1], [11, 25, 7, 3, 1]]}, usage="vh",
    why="T5EncoderModel alone: the text encoder of Flux, SD3, PixArt, Wan, Sana (encoder-only reuse of a seq2seq checkpoint)", examples=["google/t5-v1_1-xxl"])

# ─────────────────────────────── I. vision-language models (the text stage) ───────────────────────────────
VIS_CLIP = dict(model_type="clip_vision_model", hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2,
                image_size=28, patch_size=14, projection_dim=16)
LV = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, num_key_value_heads=2, max_position_embeddings=128)
add("llava", "vlm", "vlm", "LlavaForConditionalGeneration", "llava", "vlm",
    dict(text_config=dict(LV, model_type="llama"), vision_config=VIS_CLIP, image_token_index=V - 4, vision_feature_layer=-2,
         vision_feature_select_strategy="default", projector_hidden_act="gelu", multimodal_projector_bias=True),
    options={"raw": True}, usage="vh", why="LLaVA 1.5/1.6, the CLIP-tower + MLP projector + Llama pattern behind most open VLMs", examples=["llava-hf/llava-1.5-7b-hf"], real_config="llava-1.5-7b-hf")
add("qwen2_vl", "vlm", "vlm", "Qwen2VLForConditionalGeneration", "qwen2_vl", "vlm",
    dict(text_config=dict(LV, rms_norm_eps=1e-6, tie_word_embeddings=False,
                          rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "mrope_section": [2, 1, 1]}),
         vision_config=dict(depth=2, embed_dim=32, hidden_size=32, hidden_act="quick_gelu", mlp_ratio=2, num_heads=4, in_channels=3,
                            patch_size=7, spatial_merge_size=2, temporal_patch_size=2),
         image_token_id=63, vision_start_token_id=62, vision_end_token_id=61, video_token_id=60, tie_word_embeddings=False),
    options={"raw": True}, usage="vh", why="M-RoPE (3-D positions), a native-resolution vision tower, patch merger", examples=["Qwen/Qwen2-VL-7B-Instruct"])
add("qwen2_5_vl", "vlm", "vlm", "Qwen2_5_VLForConditionalGeneration", "qwen2_5_vl", "vlm",
    dict(text_config=dict(LV, rms_norm_eps=1e-6, tie_word_embeddings=False,
                          rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "mrope_section": [2, 1, 1]}),
         vision_config=dict(depth=2, hidden_size=32, intermediate_size=64, num_heads=4, out_hidden_size=32, patch_size=7,
                            spatial_merge_size=2, temporal_patch_size=2, window_size=28, fullatt_block_indexes=[1], hidden_act="silu"),
         image_token_id=63, vision_start_token_id=62, vision_end_token_id=61, video_token_id=60, tie_word_embeddings=False),
    options={"raw": True}, usage="vh", why="window-attention RMSNorm/SwiGLU tower + M-RoPE: the default open VLM of 2025", examples=["Qwen/Qwen2.5-VL-7B-Instruct"])
add("qwen3_vl", "vlm", "vlm", "Qwen3VLForConditionalGeneration", "qwen3_vl", "vlm",
    dict(text_config=dict(LV, head_dim=8, rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "mrope_section": [2, 1, 1], "mrope_interleaved": True}),
         vision_config=dict(depth=2, hidden_size=32, intermediate_size=64, num_heads=4, in_channels=3, patch_size=7, spatial_merge_size=2,
                            temporal_patch_size=2, out_hidden_size=32, num_position_embeddings=16, deepstack_visual_indexes=[0, 1]),
         image_token_id=63, video_token_id=62, vision_start_token_id=61, vision_end_token_id=60, tie_word_embeddings=False),
    options={"raw": True}, usage="h", why="DeepStack: intermediate vision features are ADDED into the early text layers' hidden states; interleaved M-RoPE", examples=["Qwen/Qwen3-VL-8B-Instruct"])
add("gemma3_vlm", "vlm", "vlm", "Gemma3ForConditionalGeneration", "gemma3", "vlm",
    dict(text_config=dict(LV, model_type="gemma3_text", head_dim=8, sliding_window=4, layer_types=["sliding_attention", "full_attention"]),
         vision_config=dict(model_type="siglip_vision_model", hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2,
                            image_size=28, patch_size=14), mm_tokens_per_image=4, image_token_index=V - 4, boi_token_index=V - 3, eoi_token_index=V - 2),
    options={"raw": True}, usage="vh", why="SigLIP tower + Gemma-3 text (the 4B/12B/27B are VLMs)", examples=["google/gemma-3-4b-it"], real_config="gemma-3-4b-it")
add("paligemma", "vlm", "vlm", "PaliGemmaForConditionalGeneration", "paligemma", "vlm",
    dict(text_config=dict(LV, model_type="gemma", head_dim=8, num_image_tokens=4),
         vision_config=dict(hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2, image_size=28, patch_size=14, projection_dim=32),
         image_token_index=V - 1, vocab_size=V, projection_dim=32, hidden_size=32),
    options={"raw": True}, usage="h", why="PREFIX-LM attention: the image and prompt tokens attend bidirectionally, the answer causally", examples=["google/paligemma2-3b-pt-224"])
add("idefics3", "vlm", "vlm", "Idefics3ForConditionalGeneration", "idefics3", "vlm",
    dict(text_config=dict(LV, model_type="llama", hidden_act="silu", rms_norm_eps=1e-5),
         vision_config=dict(hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2, image_size=28, patch_size=14),
         image_token_id=V - 1, scale_factor=2),
    options={"raw": True}, usage="h", why="SmolVLM / Idefics3: pixel-shuffle connector over a SigLIP-style tower, Llama text", examples=["HuggingFaceTB/SmolVLM-Instruct"])
add("mllama", "vlm", "vlm", "MllamaForConditionalGeneration", "mllama", "vlm",
    dict(text_config=dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, num_key_value_heads=2,
                          cross_attention_layers=[1], max_position_embeddings=128, pad_token_id=0, bos_token_id=1, eos_token_id=2,
                          rope_parameters={"rope_type": "default", "rope_theta": 10000.0}),
         vision_config=dict(hidden_size=16, intermediate_size=32, num_hidden_layers=2, num_global_layers=1, attention_heads=2, image_size=28,
                            patch_size=14, vision_output_dim=32, max_num_tiles=4, intermediate_layers_indices=[0]), image_token_index=V - 1),
    options={"raw": True}, usage="h", why="text layers CROSS-ATTEND to vision states (Llama-3.2-Vision)", examples=["meta-llama/Llama-3.2-11B-Vision-Instruct"])

# ─────────────────────────────── J. vision encoders (RFC-0003 vision input) ───────────────────────────────
add("clip_vision", "vision", "vision", "CLIPVisionModelWithProjection", "clip_vision_model", "vision",
    dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, image_size=28, patch_size=7, projection_dim=24,
         hidden_act="quick_gelu", layer_norm_eps=1e-5), options={"raw": True, "class": "CLIPVisionModelWithProjection", "size": 28}, usage="vh",
    why="the image tower of CLIP, LLaVA, Stable Diffusion image-conditioning and zero-shot classification", examples=["openai/clip-vit-base-patch32"])
add("siglip_vision", "vision", "vision", "SiglipVisionModel", "siglip_vision_model", "vision",
    dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, image_size=28, patch_size=7,
         hidden_act="gelu_pytorch_tanh", layer_norm_eps=1e-6, vision_use_head=True), options={"raw": True, "class": "SiglipVisionModel", "size": 28}, usage="h",
    why="the image tower of Gemma-3, PaliGemma, Idefics3; attention-pooling head", examples=["google/siglip-so400m-patch14-384"])
add("vit", "vision", "vision", "ViTForImageClassification", "vit", "vision",
    dict(hidden_size=32, num_hidden_layers=2, num_attention_heads=4, intermediate_size=64, image_size=28, patch_size=7),
    options={"raw": True, "class": "ViTModel", "size": 28}, usage="vh", why="image classification at hub scale: ViT, DeiT, BEiT, DINOv2 share this encoder", examples=["google/vit-base-patch16-224"])
add("resnet", "vision", "vision", "ResNetForImageClassification", "resnet", "vision",
    dict(embedding_size=8, hidden_sizes=[8, 16], depths=[1, 1], layer_type="basic"), options={"raw": True, "class": "ResNetModel", "size": 32}, usage="vh",
    why="CONVOLUTIONAL classification (ResNet, ConvNeXt, EfficientNet, timm): the largest family by downloads with no attention at all", examples=["microsoft/resnet-50", "timm/resnet50.a1_in1k"])

# ─────────────────────────────── K. image generation (diffusers) ───────────────────────────────
add("unet2d_condition", "image-gen", "diffusion", "UNet2DConditionModel", "unet", "diffusers:UNet2DConditionModel",
    dict(sample_size=8, in_channels=4, out_channels=4, layers_per_block=1, block_out_channels=[32, 64],
         down_block_types=["CrossAttnDownBlock2D", "DownBlock2D"], up_block_types=["UpBlock2D", "CrossAttnUpBlock2D"],
         cross_attention_dim=32, attention_head_dim=8, norm_num_groups=8),
    options={"inputs": "unet"}, usage="vh", why="Stable Diffusion 1.x/2.x: the denoiser of the largest image-generation family (convolutions, GroupNorm, cross-attention, timestep embedding)",
    examples=["stable-diffusion-v1-5/stable-diffusion-v1-5", "stabilityai/stable-diffusion-2-1"])
add("unet_sdxl", "image-gen", "diffusion", "UNet2DConditionModel", "unet", "diffusers:UNet2DConditionModel",
    dict(sample_size=8, in_channels=4, out_channels=4, layers_per_block=1, block_out_channels=[32, 64],
         down_block_types=["DownBlock2D", "CrossAttnDownBlock2D"], up_block_types=["CrossAttnUpBlock2D", "UpBlock2D"],
         cross_attention_dim=32, attention_head_dim=[4, 8], norm_num_groups=8, transformer_layers_per_block=[1, 2],
         addition_embed_type="text_time", addition_time_embed_dim=8, projection_class_embeddings_input_dim=64),
    options={"inputs": "unet_xl"}, usage="vh", why="SDXL: added text/time conditioning and a transformer stack per resolution (the most used open image model)",
    examples=["stabilityai/stable-diffusion-xl-base-1.0"])
add("dit", "image-gen", "diffusion", "DiTTransformer2DModel", "dit", "diffusers:DiTTransformer2DModel",
    dict(num_attention_heads=2, attention_head_dim=8, in_channels=4, num_layers=2, sample_size=8, patch_size=2, num_embeds_ada_norm=10),
    options={"inputs": "dit"}, usage="m", why="the DiT: patchified latents, adaLN-Zero, class conditioning (ancestor of PixArt, SD3, Flux)", examples=["facebook/DiT-XL-2-256"])
add("flux", "image-gen", "diffusion", "FluxTransformer2DModel", "flux", "diffusers:FluxTransformer2DModel",
    dict(patch_size=1, in_channels=16, num_layers=1, num_single_layers=1, attention_head_dim=8, num_attention_heads=2, joint_attention_dim=32,
         pooled_projection_dim=16, axes_dims_rope=[2, 2, 4], guidance_embeds=False),
    options={"inputs": "flux"}, usage="vh", why="MMDiT with 3-axis RoPE, double- and single-stream blocks (FLUX.1 dev/schnell)", examples=["black-forest-labs/FLUX.1-dev"])
add("sd3", "image-gen", "diffusion", "SD3Transformer2DModel", "sd3", "diffusers:SD3Transformer2DModel",
    dict(sample_size=8, patch_size=2, in_channels=4, num_layers=2, attention_head_dim=8, num_attention_heads=2, joint_attention_dim=32,
         caption_projection_dim=16, pooled_projection_dim=16, out_channels=4, pos_embed_max_size=16),
    options={"inputs": "sd3"}, usage="h", why="MMDiT with joint text-image attention and adaLN (Stable Diffusion 3/3.5)", examples=["stabilityai/stable-diffusion-3.5-large"])
add("vae_kl", "image-gen", "diffusion", "AutoencoderKL", "vae", "diffusers:AutoencoderKL",
    dict(in_channels=3, out_channels=3, down_block_types=["DownEncoderBlock2D", "DownEncoderBlock2D"],
         up_block_types=["UpDecoderBlock2D", "UpDecoderBlock2D"], block_out_channels=[32, 64], latent_channels=4, layers_per_block=1,
         norm_num_groups=8, sample_size=16),
    options={"inputs": "vae"}, usage="vh", why="every diffusion pipeline decodes through a convolutional VAE with a mid-block self-attention", examples=["madebyollin/sdxl-vae-fp16-fix"])

# ─────────────────────────────── L. audio ───────────────────────────────
add("whisper", "audio", "audio", "WhisperForConditionalGeneration", "whisper", "audio:WhisperForConditionalGeneration",
    dict(vocab_size=V, d_model=32, encoder_layers=2, decoder_layers=2, encoder_attention_heads=4, decoder_attention_heads=4, encoder_ffn_dim=64,
         decoder_ffn_dim=64, num_mel_bins=8, max_source_positions=16, max_target_positions=16, pad_token_id=0, bos_token_id=1, eos_token_id=2,
         decoder_start_token_id=3), options={"inputs": "whisper"}, usage="vh",
    why="speech recognition: log-mel front end, two strided Conv1d, sinusoidal-position encoder, cross-attending decoder", examples=["openai/whisper-large-v3", "openai/whisper-small"])
add("wav2vec2", "audio", "audio", "Wav2Vec2ForCTC", "wav2vec2", "audio:Wav2Vec2ForCTC",
    dict(vocab_size=32, hidden_size=32, num_hidden_layers=2, num_attention_heads=4, intermediate_size=64, conv_dim=[16, 16], conv_stride=[5, 2],
         conv_kernel=[10, 3], num_feat_extract_layers=2, num_conv_pos_embeddings=8, num_conv_pos_embedding_groups=2, feat_extract_norm="group"),
    options={"inputs": "wav2vec2"}, usage="vh", why="raw-waveform CTC: a conv feature extractor, a grouped-conv positional embedding, a transformer", examples=["facebook/wav2vec2-base-960h"])
add("speecht5", "audio", "audio", "SpeechT5ForTextToSpeech", "speecht5", "audio:SpeechT5ForTextToSpeech",
    dict(vocab_size=V, hidden_size=32, encoder_layers=2, decoder_layers=2, encoder_attention_heads=4, decoder_attention_heads=4, encoder_ffn_dim=64,
         decoder_ffn_dim=64, num_mel_bins=8, speech_decoder_prenet_units=16, speech_decoder_prenet_layers=1, speech_decoder_postnet_units=16,
         speech_decoder_postnet_layers=1, speech_decoder_postnet_kernel=3, positional_dropout=0.0, max_text_positions=32, max_speech_positions=32),
    options={"inputs": "speecht5"}, usage="m", why="text-to-speech: a mel decoder with a pre-net/post-net (a vocoder follows)", examples=["microsoft/speecht5_tts"])
add("musicgen", "audio", "audio", "MusicgenForConditionalGeneration", "musicgen", "audio:MusicgenForConditionalGeneration",
    dict(text_encoder=dict(model_type="t5", vocab_size=V, d_model=32, d_kv=8, d_ff=64, num_layers=1, num_heads=4),
         audio_encoder=dict(model_type="encodec", hidden_size=8, num_filters=4, upsampling_ratios=[2, 2], codebook_size=16, codebook_dim=8,
                            target_bandwidths=[1.5, 3.0], num_residual_layers=1),
         decoder=dict(model_type="musicgen_decoder", vocab_size=16, hidden_size=32, num_hidden_layers=2, num_attention_heads=4, ffn_dim=64,
                      num_codebooks=2, max_position_embeddings=32, pad_token_id=16, bos_token_id=16, eos_token_id=None)),
    options={"inputs": "musicgen", "raw": True}, usage="m", why="music generation: an autoregressive decoder over several EnCodec codebooks (delay pattern) and a T5 text encoder",
    examples=["facebook/musicgen-small"])
add("encodec", "audio", "audio", "EncodecModel", "encodec", "audio:EncodecModel",
    dict(hidden_size=8, num_filters=4, upsampling_ratios=[2, 2], codebook_size=16, codebook_dim=8, target_bandwidths=[1.5, 3.0],
         num_residual_layers=1, sampling_rate=400),
    options={"inputs": "encodec", "raw": True}, usage="m", why="the neural audio codec under MusicGen, Bark, Moshi-class models: causal conv encoder/decoder + residual vector quantisation",
    examples=["facebook/encodec_24khz"])

# ─────────────────────────────── M. remote-code families (no offline fixture: published-style configs) ───────────────────────────────
add("chatglm3", "remote-code", "remote", "ChatGLMModel", "chatglm", "config-only",
    dict(config={"architectures": ["ChatGLMModel"], "model_type": "chatglm",
                 "auto_map": {"AutoConfig": "configuration_chatglm.ChatGLMConfig", "AutoModel": "modeling_chatglm.ChatGLMForConditionalGeneration",
                              "AutoModelForCausalLM": "modeling_chatglm.ChatGLMForConditionalGeneration"},
                 "add_bias_linear": False, "add_qkv_bias": True, "apply_query_key_layer_scaling": True, "apply_residual_connection_post_layernorm": False,
                 "attention_dropout": 0.0, "attention_softmax_in_fp32": True, "bias_dropout_fusion": True, "ffn_hidden_size": 13696,
                 "fp32_residual_connection": False, "hidden_dropout": 0.0, "hidden_size": 4096, "kv_channels": 128, "layernorm_epsilon": 1e-05,
                 "multi_query_attention": True, "multi_query_group_num": 2, "num_attention_heads": 32, "num_layers": 28, "original_rope": True,
                 "padded_vocab_size": 65024, "post_layer_norm": True, "rmsnorm": True, "seq_length": 8192, "use_cache": True, "torch_dtype": "float16",
                 "tie_word_embeddings": False, "eos_token_id": 2, "pad_token_id": 0}),
    usage="h", why="trust_remote_code: THUDM/chatglm3-6b is among the most downloaded Chinese chat models; its forward lives in the repository, not in transformers", examples=["THUDM/chatglm3-6b"], tiny=False,
    note="config written from the published repository as remembered; there is no offline reference implementation")
add("internlm2", "remote-code", "remote", "InternLM2ForCausalLM", "internlm2", "config-only", dict(config_file="internlm2.5-7b-chat"),
    usage="m", share=0.5, why="trust_remote_code Llama-lineage with fused wqkv (InternLM2/2.5)", examples=["internlm/internlm2_5-7b-chat"], real_config="internlm2.5-7b-chat", tiny=False)
add("minicpm", "remote-code", "remote", "MiniCPMForCausalLM", "minicpm", "config-only", dict(config_file="minicpm-2b-sft-bf16"),
    usage="m", share=0.3, why="trust_remote_code with muP-style scale_emb / scale_depth / dim_model_base", examples=["openbmb/MiniCPM-2B-sft-bf16"], real_config="minicpm-2b-sft-bf16", tiny=False)


if __name__ == "__main__":
    print(len(ENTRIES), "entries so far")
