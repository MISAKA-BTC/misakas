#!/usr/bin/env python3
"""Generate Hugging Face reference fixtures for misaka-palw-tir-lower.

For every tiny configuration below this script builds a randomly initialised model with
`AutoModelForCausalLM.from_config` (no hub access: run with HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1),
re-randomises its weights so every feature is live (non-zero biases, gains away from 1, O(1)
activations), rounds every weight to bfloat16 so the saved BF16 file is exact, and records the
logits of a fixed token sequence:

    tests/configs/tiny/<name>.json          the config as transformers saves it (input to Rust tests)
    tests/fixtures/hf/<name>/config.json
    tests/fixtures/hf/<name>/model.safetensors   (BF16)
    tests/fixtures/hf/<name>/logits.json    {"tokens", "logits_full", "logits_decode"?, versions, seed}

`logits_full` is one causal forward over the whole sequence. `logits_decode` (for configs whose rope
depends on the sequence length: dynamic NTK, LongRoPE) feeds one token at a time through the cache,
which is the per-position semantics the lowerer implements.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_hf_fixtures.py [name ...]
"""

import json
import math
import os
import sys

import torch

try:
    import transformers
    from transformers import AutoModelForCausalLM, AutoModelForImageTextToText
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
except ImportError as e:  # pragma: no cover
    sys.exit(f"transformers is required: {e}")

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
TINY = os.path.join(CRATE, "tests", "configs", "tiny")
FIX = os.path.join(CRATE, "tests", "fixtures", "hf")

T = 10  # positions
V = 64  # vocab

L = dict(hidden_size=32, intermediate_size=64, num_attention_heads=4, num_key_value_heads=2, vocab_size=V)
LNI = {k: v for k, v in L.items() if k != "intermediate_size"}
VIS_SIGLIP = dict(model_type="siglip_vision_model", hidden_size=16, intermediate_size=32, num_hidden_layers=1,
                  num_attention_heads=2, image_size=28, patch_size=14)


def c(model_type, arch, *bases, **kw):
    out = {"model_type": model_type, "architectures": [arch]}
    for b in bases:
        out.update(b)
    out.update(kw)
    return out


# name -> (config dict, options)
CONFIGS = {
    "llama": (c("llama", "LlamaForCausalLM", L, num_hidden_layers=2, attention_bias=True, mlp_bias=True,
                rope_scaling={"rope_type": "llama3", "factor": 8.0, "low_freq_factor": 1.0, "high_freq_factor": 4.0,
                              "original_max_position_embeddings": 16}, rope_theta=500000.0, max_position_embeddings=128), {}),
    "llama_linear_tied": (c("llama", "LlamaForCausalLM", L, num_hidden_layers=2, tie_word_embeddings=True,
                            rope_scaling={"rope_type": "linear", "factor": 4.0}), {}),
    "mistral_window": (c("mistral", "MistralForCausalLM", L, num_hidden_layers=2, sliding_window=4, head_dim=16), {}),
    "qwen2_dynamic": (c("qwen2", "Qwen2ForCausalLM", L, num_hidden_layers=2, tie_word_embeddings=True,
                        rope_scaling={"rope_type": "dynamic", "factor": 2.0}, max_position_embeddings=6), {"decode": True}),
    "qwen2_sliding": (c("qwen2", "Qwen2ForCausalLM", L, num_hidden_layers=3, use_sliding_window=True, sliding_window=4,
                        max_window_layers=1), {}),
    "qwen3_yarn": (c("qwen3", "Qwen3ForCausalLM", L, num_hidden_layers=2, head_dim=16, attention_bias=True,
                     rope_scaling={"rope_type": "yarn", "factor": 4.0, "original_max_position_embeddings": 32},
                     max_position_embeddings=128), {}),
    "gemma": (c("gemma", "GemmaForCausalLM", L, num_hidden_layers=2, head_dim=16), {}),
    "gemma2": (c("gemma2", "Gemma2ForCausalLM", L, num_hidden_layers=2, head_dim=16, sliding_window=4,
                 query_pre_attn_scalar=12, attn_logit_softcapping=5.0, final_logit_softcapping=3.0), {}),
    "gemma3": (c("gemma3_text", "Gemma3ForCausalLM", L, num_hidden_layers=3, head_dim=16, sliding_window=4,
                 layer_types=["sliding_attention", "sliding_attention", "full_attention"],
                 rope_parameters={"sliding_attention": {"rope_type": "default", "rope_theta": 10000.0},
                                  "full_attention": {"rope_type": "linear", "factor": 8.0, "rope_theta": 1000000.0}},
                 query_pre_attn_scalar=16), {}),
    "phi": (c("phi", "PhiForCausalLM", hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
              vocab_size=V, qk_layernorm=True, partial_rotary_factor=0.5), {}),
    "phi3_longrope": (c("phi3", "Phi3ForCausalLM", L, num_hidden_layers=2, partial_rotary_factor=0.75,
                        pad_token_id=0, bos_token_id=1, eos_token_id=2,
                        max_position_embeddings=32, original_max_position_embeddings=8,
                        rope_scaling={"type": "longrope", "short_factor": [1.0, 1.5, 2.0], "long_factor": [2.0, 4.0, 8.0]},
                        sliding_window=5), {"decode": True}),
    "gpt2": (c("gpt2", "GPT2LMHeadModel", n_embd=32, n_layer=2, n_head=4, vocab_size=V, n_positions=16,
               scale_attn_by_inverse_layer_idx=True), {}),
    "gpt_neo": (c("gpt_neo", "GPTNeoForCausalLM", hidden_size=32, num_layers=2, num_heads=4, vocab_size=V,
                  max_position_embeddings=16, attention_types=[[["global", "local"], 1]], window_size=4), {}),
    "gpt_neox": (c("gpt_neox", "GPTNeoXForCausalLM", hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                   num_attention_heads=4, vocab_size=V), {}),
    "gpt_neox_seq": (c("gpt_neox", "GPTNeoXForCausalLM", hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                       num_attention_heads=4, vocab_size=V, use_parallel_residual=False, rotary_pct=0.5), {}),
    "gptj": (c("gptj", "GPTJForCausalLM", n_embd=32, n_layer=2, n_head=4, vocab_size=V, rotary_dim=4, n_positions=16), {}),
    "falcon_mq": (c("falcon", "FalconForCausalLM", hidden_size=32, num_hidden_layers=2, num_attention_heads=4, vocab_size=V), {}),
    "falcon_new": (c("falcon", "FalconForCausalLM", hidden_size=32, num_hidden_layers=2, num_attention_heads=4,
                     vocab_size=V, new_decoder_architecture=True, num_kv_heads=2, bias=True), {}),
    "falcon_alibi": (c("falcon", "FalconForCausalLM", hidden_size=32, num_hidden_layers=2, num_attention_heads=4,
                       vocab_size=V, alibi=True, multi_query=False, parallel_attn=False, bias=True), {"attn": "sdpa"}),
    "stablelm": (c("stablelm", "StableLmForCausalLM", L, num_hidden_layers=2, qk_layernorm=True, use_qkv_bias=True), {}),
    "stablelm_parallel": (c("stablelm", "StableLmForCausalLM", L, num_hidden_layers=2, use_parallel_residual=True,
                            partial_rotary_factor=0.5), {}),
    "starcoder2": (c("starcoder2", "Starcoder2ForCausalLM", L, num_hidden_layers=2, sliding_window=4), {}),
    "gpt_bigcode": (c("gpt_bigcode", "GPTBigCodeForCausalLM", n_embd=32, n_layer=2, n_head=4, vocab_size=V, n_positions=16), {}),
    "gpt_bigcode_mha": (c("gpt_bigcode", "GPTBigCodeForCausalLM", n_embd=32, n_layer=2, n_head=4, vocab_size=V,
                          n_positions=16, multi_query=False), {}),
    "olmo": (c("olmo", "OlmoForCausalLM", L, num_hidden_layers=2, clip_qkv=1.5), {}),
    "olmo2": (c("olmo2", "Olmo2ForCausalLM", L, num_hidden_layers=2), {}),
    # OLMo-3: OLMo-2 with 3 sliding : 1 full layers and rope parameters per layer type (yarn on the
    # full-attention layers, a different θ on the sliding ones).
    "olmo3": (c("olmo3", "Olmo3ForCausalLM", L, num_hidden_layers=4, sliding_window=4,
                rope_parameters={"full_attention": {"rope_type": "yarn", "rope_theta": 500000.0, "factor": 4.0,
                                                    "original_max_position_embeddings": 32},
                                 "sliding_attention": {"rope_type": "default", "rope_theta": 10000.0}},
                max_position_embeddings=128), {}),
    # GLM (glm-4-9b-chat-hf) and GLM-4 (GLM-4-0414): fused gate_up, q/k/v biases, partial rotary on
    # interleaved pairs; GLM-4's post-norms.
    "glm": (c("glm", "GlmForCausalLM", L, num_hidden_layers=2, head_dim=16, pad_token_id=0), {}),
    "glm4": (c("glm4", "Glm4ForCausalLM", L, num_hidden_layers=2, head_dim=16, pad_token_id=0), {}),
    # Ministral (8B-2410): Mistral with per-layer sliding windows.
    "ministral": (c("ministral", "MinistralForCausalLM", L, num_hidden_layers=3, sliding_window=4, head_dim=16,
                    layer_types=["sliding_attention", "full_attention", "sliding_attention"]), {}),
    "cohere": (c("cohere", "CohereForCausalLM", L, num_hidden_layers=2, use_qk_norm=True, logit_scale=0.5), {}),
    "cohere2": (c("cohere2", "Cohere2ForCausalLM", L, num_hidden_layers=4, head_dim=8, sliding_window=4,
                  layer_types=["sliding_attention", "sliding_attention", "sliding_attention", "full_attention"]), {}),
    "granite": (c("granite", "GraniteForCausalLM", L, num_hidden_layers=2, embedding_multiplier=2.0,
                  residual_multiplier=0.5, attention_multiplier=0.2, logits_scaling=3.0), {}),
    "nemotron": (c("nemotron", "NemotronForCausalLM", L, num_hidden_layers=2, head_dim=8), {}),
    "exaone4": (c("exaone4", "Exaone4ForCausalLM", L, num_hidden_layers=4, sliding_window=4, sliding_window_pattern=4,
                  layer_types=["sliding_attention", "sliding_attention", "sliding_attention", "full_attention"]), {}),
    "smollm3": (c("smollm3", "SmolLM3ForCausalLM", L, num_hidden_layers=4, pad_token_id=0, bos_token_id=1,
                  eos_token_id=2, no_rope_layer_interval=2), {}),
    "bloom": (c("bloom", "BloomForCausalLM", hidden_size=32, n_layer=2, n_head=4, vocab_size=V), {}),
    "mpt": (c("mpt", "MptForCausalLM", d_model=32, n_layers=2, n_heads=4, vocab_size=V, max_seq_len=16,
              attn_config={"clip_qkv": 2.0}), {}),
    "opt": (c("opt", "OPTForCausalLM", hidden_size=32, num_hidden_layers=2, num_attention_heads=4, ffn_dim=64,
              vocab_size=V, max_position_embeddings=16), {}),
    "opt_postln_proj": (c("opt", "OPTForCausalLM", hidden_size=32, num_hidden_layers=2, num_attention_heads=4, ffn_dim=64,
                          vocab_size=V, max_position_embeddings=16, do_layer_norm_before=False, word_embed_proj_dim=16), {}),
    "mixtral": (c("mixtral", "MixtralForCausalLM", L, num_hidden_layers=2, num_local_experts=4, num_experts_per_tok=2), {}),
    "qwen2_moe": (c("qwen2_moe", "Qwen2MoeForCausalLM", L, num_hidden_layers=2, num_experts=4, num_experts_per_tok=2,
                    moe_intermediate_size=16, shared_expert_intermediate_size=24, mlp_only_layers=[0]), {}),
    "qwen3_moe": (c("qwen3_moe", "Qwen3MoeForCausalLM", L, num_hidden_layers=2, head_dim=8, num_experts=4,
                    num_experts_per_tok=2, moe_intermediate_size=16, norm_topk_prob=True), {}),
    "olmoe": (c("olmoe", "OlmoeForCausalLM", L, num_hidden_layers=2, num_experts=4, num_experts_per_tok=2,
                intermediate_size=16), {}),
    "granitemoe": (c("granitemoe", "GraniteMoeForCausalLM", L, num_hidden_layers=2, num_local_experts=4,
                     num_experts_per_tok=2, intermediate_size=16, embedding_multiplier=2.0, residual_multiplier=0.5,
                     attention_multiplier=0.2, logits_scaling=3.0), {}),
    "deepseek_v2": (c("deepseek_v2", "DeepseekV2ForCausalLM", L, num_hidden_layers=2, num_key_value_heads=4, first_k_dense_replace=1,
                      n_routed_experts=8, n_shared_experts=1, num_experts_per_tok=2, moe_intermediate_size=16,
                      kv_lora_rank=8, q_lora_rank=12, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8,
                      topk_method="group_limited_greedy", n_group=4, topk_group=2, routed_scaling_factor=2.0,
                      rope_scaling={"rope_type": "yarn", "factor": 4.0, "original_max_position_embeddings": 32,
                                    "mscale": 0.707, "mscale_all_dim": 0.707, "beta_fast": 32, "beta_slow": 1},
                      max_position_embeddings=128), {}),
    "deepseek_v2_lite": (c("deepseek_v2", "DeepseekV2ForCausalLM", L, num_hidden_layers=2, num_key_value_heads=4, first_k_dense_replace=1,
                           n_routed_experts=4, n_shared_experts=2, num_experts_per_tok=2, moe_intermediate_size=16,
                           kv_lora_rank=8, q_lora_rank=None, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8), {}),
    "deepseek_v3": (c("deepseek_v3", "DeepseekV3ForCausalLM", L, num_hidden_layers=2, num_key_value_heads=4, first_k_dense_replace=1,
                      n_routed_experts=8, n_shared_experts=1, num_experts_per_tok=2, moe_intermediate_size=16,
                      kv_lora_rank=8, q_lora_rank=12, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8,
                      n_group=4, topk_group=2, routed_scaling_factor=2.5,
                      rope_scaling={"rope_type": "yarn", "factor": 4.0, "original_max_position_embeddings": 32,
                                    "mscale": 1.0, "mscale_all_dim": 1.0}, max_position_embeddings=128), {}),
    # GLM-4.5 (Glm4Moe): DeepSeek-V3 routing over GQA with q/k/v biases, per-head QK norm, partial
    # rotary on NeoX halves; one dense layer first.
    "glm4_moe": (c("glm4_moe", "Glm4MoeForCausalLM", L, num_hidden_layers=2, head_dim=16, first_k_dense_replace=1,
                   n_routed_experts=8, n_shared_experts=1, num_experts_per_tok=2, moe_intermediate_size=16,
                   n_group=4, topk_group=2, routed_scaling_factor=2.5, attention_bias=True, use_qk_norm=True,
                   partial_rotary_factor=0.5, pad_token_id=0), {}),
    # Phi-3.5-MoE (Phimoe): sparsemixer top-2 (a jitter of 0.3 keeps several experts under each
    # threshold, so the weights are real softmaxes), LayerNorms, biases everywhere incl. the head,
    # LongRoPE with its short factors and an mscale (HF 5.17 never switches to the long factors).
    "phimoe": (c("phimoe", "PhimoeForCausalLM", L, num_hidden_layers=2, num_local_experts=4, num_experts_per_tok=2,
                 attention_bias=True, lm_head_bias=True, router_jitter_noise=0.3, max_position_embeddings=32,
                 rope_parameters={"rope_type": "longrope", "rope_theta": 10000.0, "short_factor": [1.0, 1.25, 1.5, 2.0],
                                  "long_factor": [2.0, 4.0, 6.0, 8.0], "short_mscale": 1.2, "long_mscale": 1.2,
                                  "original_max_position_embeddings": 8}, sliding_window=None), {"decode": True}),
    "gpt_oss": (c("gpt_oss", "GptOssForCausalLM", L, num_hidden_layers=2, head_dim=8, num_local_experts=4,
                  num_experts_per_tok=2, intermediate_size=16, sliding_window=4,
                  rope_scaling={"rope_type": "yarn", "factor": 4.0, "beta_fast": 32.0, "beta_slow": 1.0,
                                "truncate": False, "original_max_position_embeddings": 32},
                  max_position_embeddings=128), {}),
    "qwen3_next": (c("qwen3_next", "Qwen3NextForCausalLM", L, num_hidden_layers=4, head_dim=8, num_experts=4,
                     num_experts_per_tok=2, moe_intermediate_size=16, shared_expert_intermediate_size=16,
                     linear_key_head_dim=8, linear_value_head_dim=8, linear_num_key_heads=2, linear_num_value_heads=4,
                     mlp_only_layers=[1]), {}),
    "qwen3_5": (c("qwen3_5_text", "Qwen3_5ForCausalLM", L, num_hidden_layers=4, head_dim=8, linear_key_head_dim=8,
                  linear_value_head_dim=8, linear_num_key_heads=2, linear_num_value_heads=6), {}),
    "qwen3_5_moe": (c("qwen3_5_moe_text", "Qwen3_5MoeForCausalLM", LNI, num_hidden_layers=4, head_dim=8,
                      linear_key_head_dim=8, linear_value_head_dim=8, linear_num_key_heads=2, linear_num_value_heads=4,
                      num_experts=4, num_experts_per_tok=2, moe_intermediate_size=16,
                      shared_expert_intermediate_size=16), {}),
    "jamba": (c("jamba", "JambaForCausalLM", L, num_hidden_layers=4, num_experts=4, num_experts_per_tok=2,
                attn_layer_period=2, attn_layer_offset=1, expert_layer_period=2, expert_layer_offset=1,
                mamba_d_state=4, mamba_dt_rank=4, use_mamba_kernels=False), {}),
    "mamba": (c("mamba", "MambaForCausalLM", hidden_size=32, num_hidden_layers=2, vocab_size=V, state_size=4,
                time_step_rank=4, use_bias=True), {}),
    "falcon_mamba": (c("falcon_mamba", "FalconMambaForCausalLM", hidden_size=32, num_hidden_layers=2, vocab_size=V,
                       state_size=4, time_step_rank=4), {}),
    "mamba2": (c("mamba2", "Mamba2ForCausalLM", hidden_size=32, num_hidden_layers=2, vocab_size=V, state_size=4,
                 num_heads=8, head_dim=8, n_groups=2, use_bias=True), {}),
    "rwkv": (c("rwkv", "RwkvForCausalLM", hidden_size=32, num_hidden_layers=4, vocab_size=V, attention_hidden_size=32,
               intermediate_size=64, rescale_every=2), {}),
    # VLMs: only the text decoder is lowered; prompts are text-only (no pixel_values).
    "gemma3_vlm": (c("gemma3", "Gemma3ForConditionalGeneration",
                     text_config=dict(L, model_type="gemma3_text", num_hidden_layers=2, head_dim=8, sliding_window=4,
                                      layer_types=["sliding_attention", "full_attention"]),
                     vision_config=VIS_SIGLIP, mm_tokens_per_image=4, image_token_index=V - 4, boi_token_index=V - 3,
                     eoi_token_index=V - 2), {"vlm": True}),
    "qwen3_5_vlm": (c("qwen3_5", "Qwen3_5ForConditionalGeneration",
                      text_config=dict(L, model_type="qwen3_5_text", num_hidden_layers=4, head_dim=8, linear_key_head_dim=8,
                                       linear_value_head_dim=8, linear_num_key_heads=2, linear_num_value_heads=4),
                      vision_config=dict(depth=1, hidden_size=16, intermediate_size=32, num_heads=2, out_hidden_size=32,
                                         patch_size=4, spatial_merge_size=2, temporal_patch_size=2),
                      image_token_id=V - 4, video_token_id=V - 3, vision_start_token_id=V - 2, vision_end_token_id=V - 1),
                    {"vlm": True}),
    "mistral3_vlm": (c("mistral3", "Mistral3ForConditionalGeneration",
                       text_config=dict(L, model_type="mistral", num_hidden_layers=2, head_dim=8, sliding_window=None),
                       vision_config=dict(model_type="pixtral", hidden_size=16, intermediate_size=32, num_hidden_layers=1,
                                          num_attention_heads=2, image_size=28, patch_size=14, head_dim=8),
                       image_token_index=V - 4, spatial_merge_size=2), {"vlm": True}),
    "llava": (c("llava", "LlavaForConditionalGeneration", text_config=dict(L, model_type="llama", num_hidden_layers=2),
                vision_config=dict(model_type="clip_vision_model", hidden_size=16, intermediate_size=32, num_hidden_layers=1,
                                   num_attention_heads=2, image_size=28, patch_size=14, projection_dim=16),
                image_token_index=V - 4), {"vlm": True}),
}

EMBED_HINTS = ("embed", "wte", "wpe", "word_embeddings", "embeddings.weight", "embed_in")


def randomise(model, hidden, seed):
    """Make every weight live and O(1)-scaled, then round everything to bfloat16."""
    g = torch.Generator().manual_seed(seed + 1)
    sd_names = set(model.state_dict().keys())
    with torch.no_grad():
        for name, p in model.named_parameters():
            if not p.is_floating_point():
                continue
            if p.ndim >= 2 and any(h in name for h in EMBED_HINTS):
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2 and "conv1d" in name:
                p.copy_(torch.randn(p.shape, generator=g) * 0.5)
            elif p.ndim >= 2 and "time_mix" not in name and "time_maa" not in name:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(hidden))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
        for name, b in model.named_buffers():
            if name in sd_names and b.is_floating_point():
                b.add_(torch.randn(b.shape, generator=g) * 0.1)
                b.copy_(b.to(torch.bfloat16).to(torch.float32))


def decode_logits(model, ids):
    out, past = [], None
    for t in range(ids.shape[1]):
        o = model(input_ids=ids[:, t:t + 1], past_key_values=past, use_cache=True)
        past = o.past_key_values
        out.append(o.logits[0, -1].tolist())
    return out


def make(name, cfg_dict, opts):
    cfg_dict = dict(cfg_dict)
    model_type = cfg_dict.pop("model_type")
    arch = cfg_dict["architectures"]
    seed = sum(ord(ch) for ch in name)
    cfg = CONFIG_MAPPING[model_type](**cfg_dict)
    cfg.architectures = arch
    torch.manual_seed(seed)
    auto = AutoModelForImageTextToText if opts.get("vlm") else AutoModelForCausalLM
    model = auto.from_config(cfg)
    model.eval()
    tc = cfg.get_text_config()
    hidden = getattr(tc, "hidden_size", None) or getattr(tc, "n_embd", None) or getattr(tc, "d_model")
    randomise(model, hidden, seed)
    ids = torch.tensor([[(seed * 7 + 13 * i + i * i) % V for i in range(T)]])
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    # Save FIRST, then compute the logits from a fresh load of the saved files: the reference is
    # what transformers computes from exactly the bytes under test (RWKV, for one, rescales its
    # weights in place on the first inference forward, so a model that has run is not the file).
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    for extra in ("generation_config.json",):
        pth = os.path.join(d, extra)
        if os.path.exists(pth):
            os.remove(pth)
    # The eager attention is the models' defining math (sdpa silently drops Gemma-2's score
    # soft-cap); Falcon's eager ALiBi path adds the bias twice in transformers 5.17, so that one
    # fixture uses sdpa (see the options).
    impl = opts.get("attn", "eager")
    fresh = auto.from_pretrained(d, dtype=torch.float32, attn_implementation=impl)
    fresh.eval()
    with torch.no_grad():
        full = fresh(input_ids=ids).logits[0].tolist()
        dec = decode_logits(fresh, ids) if opts.get("decode") else None
    for row in full:
        if not all(math.isfinite(x) for x in row):
            raise RuntimeError(f"{name}: non-finite logits")
    with open(os.path.join(d, "config.json")) as f:
        saved = json.load(f)
    os.makedirs(TINY, exist_ok=True)
    with open(os.path.join(TINY, f"{name}.json"), "w") as f:
        json.dump(saved, f, indent=2, sort_keys=True)
        f.write("\n")
    meta = {
        "tokens": ids[0].tolist(),
        "logits_full": full,
        "transformers": transformers.__version__,
        "torch": torch.__version__,
        "seed": seed,
        "weights": "bfloat16-exact (rounded before the forward)",
        "attn_implementation": impl,
    }
    if dec is not None:
        meta["logits_decode"] = dec
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    return max(abs(x) for row in full for x in row)


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        cfg, opts = CONFIGS[n]
        try:
            m = make(n, cfg, opts)
            print(f"ok   {n:22s} max|logit| {m:.3f}")
        except Exception as e:  # report and continue: one broken config must not hide the rest
            bad += 1
            print(f"FAIL {n:22s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
