#!/usr/bin/env python3
"""Pre-quantised fixtures (GPTQ, AWQ) for misaka-palw-tir-lower — quantised here, in numpy, per each
format's specification. No quantisation library (auto-gptq, gptqmodel, optimum, autoawq) and no hub
access: run with HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1.

Every fixture is a tiny random decoder (transformers' own config classes) whose block projections
(q/k/v/o, gate/up/down) are quantised by round-to-nearest over groups of input columns:

  GPTQ (AutoGPTQ / GPTQModel, `checkpoint_format` gptq = v1 or gptq_v2):
    scale, zero per (group, output) as GPTQ's `Quantizer.find_params` (sym: zero = 2^(b-1));
    q = clamp(round(W / fp16(scale)) + zero, 0, 2^b - 1);
    act-order (`desc_act`): columns quantised in the order of a random "Hessian diagonal",
    groups over that order, g_idx[i] = position(i) // group_size;
    qweight int32 [in*b/32, out]: word r holds inputs r*32/b + k at bits b*k;
    qzeros  int32 [G, out*b/32]: word c holds outputs c*32/b + k, v1 stores (zero - 1) mod 2^b;
    scales  fp16 [G, out]; g_idx int32 [in].
  AWQ (AutoAWQ GEMM, 4-bit):
    scale = (max - min) / 15 (min 1e-5), zero = clamp(-round(min / scale), 0, 15) per (group, output);
    q = clamp(round(W / fp16(scale) + zero), 0, 15);
    qweight int32 [in, out/8] and qzeros int32 [G, out/8]: slot k of word c holds output
    8c + [0,2,4,6,1,3,5,7][k]; scales fp16 [G, out].

  FP8 with block scales (`quant_method: fp8`, DeepSeek-V3 / Qwen3-FP8): `weight` float8_e4m3fn + `weight_scale_inv`
    float32 [ceil(out/bo), ceil(in/bi)], scale = block amax / 448, w8 = fp8(w / scale).
  MXFP4 as Hugging Face stores it (gpt-oss, `quant_method: mxfp4`): the expert tensors `gate_up_proj` [E, in, out]
    and `down_proj` are kept as `<name>_blocks` uint8 [E, out, in/32, 16] and `<name>_scales` uint8 [E, out, in/32]
    (E8M0 exponent, bias 127), one shared exponent per 32 inputs, codes the nearest FP4 (E2M1) value; the reference's
    expert weights are transformers' own `convert_moe_packed_tensors` of the stored tensors.
  bitsandbytes (`quant_method: bitsandbytes`, tools/bnb_ref.py): 4-bit nf4 / fp4 (`weight` uint8 [N/2, 1], `weight.absmax`, `weight.quant_map`,
    `weight.quant_state.bitsandbytes__nf4` JSON, and under double quantisation uint8 absmax + `nested_absmax` + `nested_quant_map`) and LLM.int8
    (`weight` int8, `SCB`, `weight_format`).
  compressed-tensors (llm-compressor):
    pack-quantized (W4A16 / W8A16): `weight_packed` int32 [out, in/pack] (codes along the INPUT, offset by
    2^(b-1)), `weight_scale` fp16 [out, G], asymmetric `weight_zero_point` int32 [out/pack, G] (packed along the
    OUTPUT), `weight_shape` int64 [2], act-order `weight_g_idx` int32 [in];
    float-quantized (FP8, per channel): `weight` fp8 + `weight_scale` [out, 1];
    int-quantized (INT8, per channel): `weight` int8 + `weight_scale` [out, 1].

Every other tensor is stored in fp16 (the float model is rounded to fp16 first). THE REFERENCE is
transformers' float model with the dequantised weights substituted, W[o,i] = s[g,o]*(q[i,o] - z[g,o]),
where the dequantisation is recomputed from the PACKED tensors (not from the quantiser's arrays),
so a packing mistake shows as a mismatch against the lowerer's own unpacking.

Modes:
  gen_quant_fixtures.py fixtures [name ...]
      tests/fixtures/hf-quant/<name>/{config.json, model.safetensors, logits.json}
  gen_quant_fixtures.py audit OUT [name ...]
      OUT/<name>/       config.json (with quantization_config), model.safetensors,
                        hf.json, hf-logits.f32, calib.json (as tools/audit_e2e.py writes them)
      OUT/<name>-float/ the same model with the dequantised weights as a plain F32 checkpoint
                        (the float path's twin: W8 re-quantisation of the identical weights)
"""

import json
import math
import os
import sys

import numpy as np
import torch
from safetensors.numpy import save_file

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bnb_ref

try:
    import transformers
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
except ImportError as e:  # pragma: no cover
    sys.exit(f"transformers is required: {e}")

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-quant")

V = 128
T = 16
PROJ = ("q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj", "w1", "w2", "w3")
AWQ_ORDER = [0, 2, 4, 6, 1, 3, 5, 7]

BASE = dict(hidden_size=128, intermediate_size=256, num_attention_heads=4, num_key_value_heads=2, vocab_size=V,
            num_hidden_layers=2, max_position_embeddings=1024)
MODELS = {
    # gpt-oss: experts [E, in, out] fused; the in-width of each expert tensor is a multiple of 32 (MXFP4's block).
    "gptoss": ("gpt_oss", "GptOssForCausalLM", dict(hidden_size=64, intermediate_size=32, num_attention_heads=4, num_key_value_heads=2,
                                                    vocab_size=V, head_dim=16, num_hidden_layers=2, num_local_experts=4, num_experts_per_tok=2,
                                                    sliding_window=4, max_position_embeddings=128,
                                                    rope_scaling={"rope_type": "yarn", "factor": 4.0, "beta_fast": 32.0, "beta_slow": 1.0,
                                                                  "truncate": False, "original_max_position_embeddings": 32})),
    "qwen3moe": ("qwen3_moe", "Qwen3MoeForCausalLM", dict(BASE, moe_intermediate_size=128, num_experts=4, num_experts_per_tok=2,
                                                        norm_topk_prob=True, head_dim=32)),
    "mixtral": ("mixtral", "MixtralForCausalLM", dict(BASE, intermediate_size=128, num_local_experts=4, num_experts_per_tok=2)),
    "llama": ("llama", "LlamaForCausalLM", dict(BASE)),
    # Qwen2: q/k/v biases (a quantised module's float bias), tied embeddings.
    "qwen2": ("qwen2", "Qwen2ForCausalLM", dict(BASE, tie_word_embeddings=True)),
}

# name -> (model, quantization_config)
CONFIGS = {
    "gptq_b4_g32": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": 32, "desc_act": False, "sym": True}),
    "gptq_b4_g64_act": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": 64, "desc_act": True, "sym": True}),
    "gptq_b4_g128_act_asym": ("qwen2", {"quant_method": "gptq", "bits": 4, "group_size": 128, "desc_act": True, "sym": False}),
    "gptq_b8_g128": ("llama", {"quant_method": "gptq", "bits": 8, "group_size": 128, "desc_act": False, "sym": True}),
    "gptq_b8_g32_asym_act": ("qwen2", {"quant_method": "gptq", "bits": 8, "group_size": 32, "desc_act": True, "sym": False}),
    "gptq_b4_perrow": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": -1, "desc_act": False, "sym": True}),
    "gptq_v2_b4_g64": ("qwen2", {"quant_method": "gptq", "bits": 4, "group_size": 64, "desc_act": False, "sym": False,
                                 "checkpoint_format": "gptq_v2"}),
    "gptq_b2_g32": ("llama", {"quant_method": "gptq", "bits": 2, "group_size": 32, "desc_act": False, "sym": True}),
    "awq_g32": ("llama", {"quant_method": "awq", "bits": 4, "group_size": 32, "zero_point": True, "version": "gemm"}),
    "awq_g64": ("qwen2", {"quant_method": "awq", "bits": 4, "group_size": 64, "zero_point": True, "version": "gemm"}),
    "awq_g128": ("llama", {"quant_method": "awq", "bits": 4, "group_size": 128, "zero_point": True, "version": "gemm"}),
    # Experts: one stored module per expert; the router stays float.
    "gptq_qwen3moe_b4_g32_act": ("qwen3moe", {"quant_method": "gptq", "bits": 4, "group_size": 32, "desc_act": True, "sym": True}),
    "awq_mixtral_g64": ("mixtral", {"quant_method": "awq", "bits": 4, "group_size": 64, "zero_point": True, "version": "gemm",
                                    "modules_to_not_convert": ["gate"]}),
}


def ct_config(bits, group_size, sym, strategy, actorder=None, fmt="pack-quantized", typ="int"):
    """A compressed-tensors `quantization_config` as llm-compressor writes it."""
    return {"quant_method": "compressed-tensors", "format": fmt, "quantization_status": "compressed", "ignore": ["lm_head"],
            "kv_cache_scheme": None, "sparsity_config": {}, "transform_config": {}, "global_compression_ratio": 2.0, "version": "0.10.0",
            "config_groups": {"group_0": {"targets": ["Linear"], "input_activations": None, "output_activations": None,
                                          "weights": {"num_bits": bits, "type": typ, "symmetric": sym, "strategy": strategy, "group_size": group_size,
                                                      "actorder": actorder, "dynamic": False, "observer": "minmax", "observer_kwargs": {}, "block_structure": None}}}}


def bnb_config(load4, qtype="nf4", dq=False, skip=None, threshold=6.0, compute="bfloat16"):
    """A `BitsAndBytesConfig.to_dict()` as transformers writes it into config.json."""
    return {"_load_in_4bit": load4, "_load_in_8bit": not load4, "bnb_4bit_compute_dtype": compute, "bnb_4bit_quant_storage": "uint8",
            "bnb_4bit_quant_type": qtype, "bnb_4bit_use_double_quant": dq, "llm_int8_enable_fp32_cpu_offload": False,
            "llm_int8_has_fp16_weight": False, "llm_int8_skip_modules": skip, "llm_int8_threshold": threshold, "load_in_4bit": load4,
            "load_in_8bit": not load4, "quant_method": "bitsandbytes"}


# The dtype the quantised weights had when bitsandbytes quantised them (the JSON document's `dtype`) and the block size, per fixture.
BNB_STATE = {"bnb_nf4_g64": ("float16", 64), "bnb_nf4_dq_bf16": ("bfloat16", 64), "bnb_fp4_g128": ("bfloat16", 128), "bnb_fp4_dq_f32": ("float32", 64),
             "bnb_nf4_skip_down": ("float16", 64), "bnb_nf4_dq_qwen3moe": ("bfloat16", 64)}


CONFIGS.update({
    "bnb_nf4_g64": ("llama", bnb_config(True, "nf4", False)),
    "bnb_nf4_dq_bf16": ("qwen2", bnb_config(True, "nf4", True)),
    "bnb_fp4_g128": ("llama", bnb_config(True, "fp4", False)),
    "bnb_fp4_dq_f32": ("qwen2", bnb_config(True, "fp4", True, compute="float16")),
    # llm_int8_skip_modules: every down_proj and the head stay float (transformers reads an entry as a module's last component or a path prefix)
    "bnb_nf4_skip_down": ("llama", bnb_config(True, "nf4", False, skip=["lm_head", "down_proj"])),
    "bnb_nf4_dq_qwen3moe": ("qwen3moe", bnb_config(True, "nf4", True, skip=["gate"])),
    "bnb_int8": ("qwen2", bnb_config(False, threshold=0.0)),
    "bnb_int8_llama": ("llama", bnb_config(False, threshold=0.0, skip=["lm_head"])),
})

CONFIGS.update({
    "mxfp4_gptoss": ("gptoss", {"quant_method": "mxfp4", "modules_to_not_convert": ["model.layers.*.self_attn", "model.layers.*.mlp.router", "model.embed_tokens", "lm_head"]}),
    "fp8_block_32": ("llama", {"quant_method": "fp8", "activation_scheme": "dynamic", "fmt": "e4m3", "weight_block_size": [32, 32]}),
    # Blocks that do not divide the weight (128 / 48, 256 / 96): ragged edges.
    "fp8_block_ragged": ("qwen2", {"quant_method": "fp8", "activation_scheme": "dynamic", "fmt": "e4m3", "weight_block_size": [48, 96]}),
    "fp8_block_qwen3moe": ("qwen3moe", {"quant_method": "fp8", "activation_scheme": "dynamic", "fmt": "e4m3", "weight_block_size": [32, 32]}),
    "ct_pack_b4_g32": ("llama", ct_config(4, 32, True, "group")),
    "ct_pack_b4_g64_asym": ("qwen2", ct_config(4, 64, False, "group")),
    "ct_pack_b4_g32_act": ("llama", ct_config(4, 32, True, "group", "group")),
    "ct_pack_b8_g32_asym_act": ("qwen2", ct_config(8, 32, False, "group", "group")),
    "ct_pack_b4_channel": ("llama", ct_config(4, None, True, "channel")),
    "ct_pack_qwen3moe_b4_g32": ("qwen3moe", ct_config(4, 32, True, "group")),
    "ct_fp8_channel": ("llama", ct_config(8, None, True, "channel", None, "float-quantized", "float")),
    "ct_int8_channel": ("qwen2", ct_config(8, None, True, "channel", None, "int-quantized", "int")),
})


def f16(x):
    return np.asarray(x, dtype=np.float32).astype(np.float16).astype(np.float64)


# ───────────────────────────── quantisers (format specifications) ─────────────────────────────

def gptq_quantize(w, bits, group, sym, desc_act, rng):
    """w: [out, in] float64. Returns q [in, out], zero [G, out], scale16 [G, out], g_idx [in]."""
    out, inp = w.shape
    gs = inp if group == -1 else group
    maxq = 2 ** bits - 1
    if desc_act:
        hdiag = rng.random(inp) + 0.1
        perm = np.argsort(-hdiag, kind="stable")
    else:
        perm = np.arange(inp)
    pos = np.empty(inp, dtype=np.int64)
    pos[perm] = np.arange(inp)
    g_idx = (pos // gs).astype(np.int32)
    ng = -(-inp // gs)
    q = np.zeros((inp, out), dtype=np.int64)
    zero = np.zeros((ng, out), dtype=np.int64)
    scale16 = np.zeros((ng, out))
    for g in range(ng):
        cols = perm[g * gs:(g + 1) * gs]
        wg = w[:, cols]
        xmin = np.minimum(wg.min(1), 0.0)
        xmax = np.maximum(wg.max(1), 0.0)
        if sym:
            xmax = np.maximum(np.abs(xmin), xmax)
            xmin = np.where(xmin < 0, -xmax, xmin)
        both = (xmin == 0) & (xmax == 0)
        xmin[both], xmax[both] = -1.0, 1.0
        scale = (xmax - xmin) / maxq
        z = np.full(out, (maxq + 1) // 2) if sym else np.round(-xmin / scale)
        s16 = f16(scale)
        qq = np.clip(np.round(wg / s16[:, None]) + z[:, None], 0, maxq)
        q[cols, :] = qq.T.astype(np.int64)
        zero[g] = z.astype(np.int64)
        scale16[g] = s16
    return q, zero, scale16, g_idx


def gptq_pack(q, zero, scale16, g_idx, bits, v2):
    inp, out = q.shape
    pack = 32 // bits
    maxq = 2 ** bits - 1
    qw = np.zeros((inp // pack, out), dtype=np.uint32)
    for k in range(pack):
        qw |= (q[k::pack, :].astype(np.uint32) & maxq) << np.uint32(bits * k)
    zs = zero if v2 else (zero - 1) & maxq
    qz = np.zeros((zero.shape[0], out // pack), dtype=np.uint32)
    for k in range(pack):
        qz |= (zs[:, k::pack].astype(np.uint32) & maxq) << np.uint32(bits * k)
    return {"qweight": qw.view(np.int32), "qzeros": qz.view(np.int32), "scales": scale16.astype(np.float16),
            "g_idx": g_idx.astype(np.int32)}


def gptq_dequant(t, bits, v2):
    """From the PACKED tensors only."""
    qw, qz, sc, gi = t["qweight"].view(np.uint32), t["qzeros"].view(np.uint32), t["scales"].astype(np.float64), t["g_idx"]
    pack = 32 // bits
    maxq = 2 ** bits - 1
    inp, out = qw.shape[0] * pack, qw.shape[1]
    q = np.zeros((inp, out), dtype=np.int64)
    for k in range(pack):
        q[k::pack, :] = (qw >> np.uint32(bits * k)) & maxq
    z = np.zeros((qz.shape[0], out), dtype=np.int64)
    for k in range(pack):
        z[:, k::pack] = (qz >> np.uint32(bits * k)) & maxq
    if not v2:
        z = (z + 1) & maxq
    w = sc[gi, :] * (q - z[gi, :])  # [in, out]
    return w.T


def awq_quantize(w, group):
    out, inp = w.shape
    ng = inp // group
    q = np.zeros((inp, out), dtype=np.int64)
    zero = np.zeros((ng, out), dtype=np.int64)
    scale16 = np.zeros((ng, out))
    for g in range(ng):
        wg = w[:, g * group:(g + 1) * group]
        mx, mn = wg.max(1), wg.min(1)
        scale = np.maximum(mx - mn, 1e-5) / 15.0
        z = np.clip(-np.round(mn / scale), 0, 15)
        s16 = f16(scale)
        qq = np.clip(np.round(wg / s16[:, None] + z[:, None]), 0, 15)
        q[g * group:(g + 1) * group, :] = qq.T.astype(np.int64)
        zero[g] = z.astype(np.int64)
        scale16[g] = s16
    return q, zero, scale16


def awq_pack(q, zero, scale16):
    inp, out = q.shape
    qw = np.zeros((inp, out // 8), dtype=np.uint32)
    qz = np.zeros((zero.shape[0], out // 8), dtype=np.uint32)
    for k, o in enumerate(AWQ_ORDER):
        qw |= (q[:, o::8].astype(np.uint32) & 15) << np.uint32(4 * k)
        qz |= (zero[:, o::8].astype(np.uint32) & 15) << np.uint32(4 * k)
    return {"qweight": qw.view(np.int32), "qzeros": qz.view(np.int32), "scales": scale16.astype(np.float16)}


def awq_dequant(t, group):
    qw, qz, sc = t["qweight"].view(np.uint32), t["qzeros"].view(np.uint32), t["scales"].astype(np.float64)
    inp, out = qw.shape[0], qw.shape[1] * 8
    q = np.zeros((inp, out), dtype=np.int64)
    z = np.zeros((qz.shape[0], out), dtype=np.int64)
    for k, o in enumerate(AWQ_ORDER):
        q[:, o::8] = (qw >> np.uint32(4 * k)) & 15
        z[:, o::8] = (qz >> np.uint32(4 * k)) & 15
    gi = np.arange(inp) // group
    w = sc[gi, :] * (q - z[gi, :])
    return w.T


def pack_along(vals, bits, axis):
    """Unsigned codes packed into int32 words along `axis`, lowest code in the lowest bits."""
    pack = 32 // bits
    v = np.moveaxis(np.asarray(vals).astype(np.uint64), axis, -1)
    n = v.shape[-1]
    assert n % pack == 0
    v = v.reshape(*v.shape[:-1], n // pack, pack)
    w = (v << (np.arange(pack, dtype=np.uint64) * bits)).sum(-1) & 0xFFFFFFFF
    return np.ascontiguousarray(np.moveaxis(w.astype(np.uint32).view(np.int32), -1, axis))


def ct_pack(q, zero, scale16, g_idx, bits, sym, act):
    """compressed-tensors pack-quantized tensors from GPTQ-style quantiser arrays (q [in, out] unsigned codes,
    zero [G, out], scale16 [G, out]); symmetric zero = 2^(b-1) is not stored."""
    inp, out = q.shape
    # (safetensors.numpy writes a buffer as it lies in memory: a transposed view must be made contiguous first.)
    d = {"weight_packed": pack_along(q.T, bits, 1), "weight_scale": np.ascontiguousarray(scale16.T.astype(np.float16)),
         "weight_shape": np.array([out, inp], dtype=np.int64)}
    if not sym:
        d["weight_zero_point"] = pack_along(zero.T, bits, 0)
    if act:
        d["weight_g_idx"] = g_idx.astype(np.int32)
    return d


def ct_dequant(t, bits, gs):
    """From the PACKED tensors only, as compressed_tensors' unpack_from_int32 + dequantize: signed codes
    (stored - 2^(b-1)), signed zero points the same way."""
    packed, sc = t["weight_packed"].view(np.uint32), t["weight_scale"].astype(np.float64)
    out, inp = (int(x) for x in t["weight_shape"])
    pack, mask, off = 32 // bits, (1 << bits) - 1, 1 << (bits - 1)
    q = np.zeros((out, inp), dtype=np.int64)
    for k in range(pack):
        q[:, k::pack] = (packed >> np.uint32(bits * k)) & mask
    q -= off
    z = np.zeros(sc.shape, dtype=np.int64)
    if "weight_zero_point" in t:
        zp = t["weight_zero_point"].view(np.uint32)
        for k in range(pack):
            z[k::pack, :] = (zp >> np.uint32(bits * k)) & mask
        z -= off
    gi = t["weight_g_idx"] if "weight_g_idx" in t else np.arange(inp) // (inp if gs in (None, -1) else gs)
    return sc[:, gi] * (q - z[:, gi])


FP4_MAG = np.array([0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0])


def mxfp4_quantize(w):
    """OCP MX with FP4 (E2M1) elements, 32 per block along the last axis: shared exponent floor(log2(amax)) - 2, each element the nearest
    FP4 value of x / 2^e (clamped to +-6). w: [..., n] float64 with n % 32 == 0. Returns blocks uint8 [..., n/32, 16], scales uint8 [..., n/32]."""
    *lead, n = w.shape
    assert n % 32 == 0
    x = w.reshape(*lead, n // 32, 32)
    amax = np.abs(x).max(-1)
    e = np.where(amax > 0, np.floor(np.log2(np.where(amax > 0, amax, 1.0))) - 2, 0).astype(np.int64)
    e = np.clip(e, -127, 127)
    y = x / np.exp2(e)[..., None].astype(np.float64)
    mag = np.abs(y)
    idx = np.abs(mag[..., None] - FP4_MAG).argmin(-1)           # nearest magnitude code 0..7 (ties: the lower)
    code = idx + 8 * (np.signbit(y) & (idx > 0))                 # sign bit for nonzero magnitudes (zero stays +0)
    code = code.astype(np.uint8)
    blocks = (code[..., 0::2] | (code[..., 1::2] << 4)).astype(np.uint8)
    return np.ascontiguousarray(blocks), np.ascontiguousarray((e + 127).astype(np.uint8))


def fp8_quantize(w, bo, bi):
    """Block-scaled float8_e4m3fn: scale = block amax / 448. Returns the fp8 bytes [out, in] and the float32 scales."""
    out, inp = w.shape
    nb = (-(-out // bo), -(-inp // bi))
    scale = np.ones(nb, dtype=np.float32)
    w32 = w.astype(np.float32)
    for a in range(nb[0]):
        for b in range(nb[1]):
            blk = w32[a * bo:(a + 1) * bo, b * bi:(b + 1) * bi]
            m = float(np.abs(blk).max())
            scale[a, b] = m / 448.0 if m > 0 else 1.0
    srep = np.repeat(np.repeat(scale, bo, 0), bi, 1)[:out, :inp]
    q = torch.from_numpy(np.ascontiguousarray(w32 / srep)).to(torch.float8_e4m3fn)
    return q.view(torch.uint8).numpy().copy(), scale


def fp8_dequant(w_u8, scale, bo, bi):
    """From the stored bytes only (the reference library's `weight_dequant`)."""
    w = torch.from_numpy(w_u8.copy()).view(torch.float8_e4m3fn).to(torch.float32)
    s = torch.from_numpy(scale.astype(np.float32)).repeat_interleave(bo, 0).repeat_interleave(bi, 1)
    return (w * s[: w.shape[0], : w.shape[1]]).numpy()


# ───────────────────────────── models ─────────────────────────────

def build(name):
    """The float model, its quantised checkpoint tensors, the reference model and the config.

    The quantisation works on the CHECKPOINT's tensors (transformers' `save_pretrained` names:
    one module per expert), and the reference is `from_pretrained` of the same checkpoint with the
    dequantised weights — so fused in-memory layouts (transformers 5's experts) need no handling.
    """
    import tempfile
    from safetensors.numpy import load_file
    model_name, qc = CONFIGS[name]
    model_type, arch, cfg_kw = MODELS[model_name]
    seed = sum(ord(ch) for ch in name) + 77
    cfg = CONFIG_MAPPING[model_type](**cfg_kw)
    cfg.architectures = [arch]
    torch.manual_seed(seed)
    model = AutoModelForCausalLM.from_config(cfg)
    model.eval()
    g = torch.Generator().manual_seed(seed + 1)
    hidden = cfg.hidden_size
    with torch.no_grad():
        for pname, p in model.named_parameters():
            if p.ndim >= 2 and "embed" in pname:
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(hidden))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.float16).to(torch.float32))
    tmp = tempfile.mkdtemp(prefix="quantfix-")
    save_float(model, tmp)
    ck = load_file(os.path.join(tmp, "model.safetensors"))
    rng = np.random.default_rng(seed)
    tensors, deq = {}, {}
    method, bits = qc["quant_method"], qc.get("bits", 4)
    group = qc.get("group_size", 128)
    v2 = qc.get("checkpoint_format") == "gptq_v2"
    ct_fmt = qc.get("format") if method == "compressed-tensors" else None
    if method == "compressed-tensors":
        cw = qc["config_groups"]["group_0"]["weights"]
        bits, group = cw["num_bits"], cw["group_size"] if cw["group_size"] else -1
    for k in sorted(ck):
        v = ck[k]
        if method == "mxfp4":
            if k.endswith(("mlp.experts.gate_up_proj", "mlp.experts.down_proj")):
                from transformers.integrations.mxfp4 import convert_moe_packed_tensors
                blocks, scales = mxfp4_quantize(np.ascontiguousarray(v.astype(np.float64).transpose(0, 2, 1)))   # [E, out, in] -> blocks over in
                tensors[f"{k}_blocks"], tensors[f"{k}_scales"] = blocks, scales
                ref = convert_moe_packed_tensors(torch.from_numpy(blocks), torch.from_numpy(scales), dtype=torch.bfloat16).to(torch.float32).numpy()
                assert ref.shape == v.shape, (k, ref.shape, v.shape)
                deq[k] = np.ascontiguousarray(ref, dtype=np.float32)
            else:
                tensors[k] = v.astype(np.float16)
                deq[k] = v.astype(np.float16).astype(np.float32)
            continue
        if k.endswith(".weight") and k.split(".")[-2] in PROJ and ".layers." in k:
            mod = k[: -len(".weight")]
            w = v.astype(np.float64)
            if method == "fp8":
                bo, bi = qc["weight_block_size"]
                w8, sc = fp8_quantize(w, bo, bi)
                tensors[f"{mod}.weight"] = torch.from_numpy(w8).view(torch.float8_e4m3fn)
                tensors[f"{mod}.weight_scale_inv"] = sc
                deq[k] = np.ascontiguousarray(fp8_dequant(w8, sc, bo, bi), dtype=np.float32)
                continue
            if method == "compressed-tensors" and ct_fmt in ("float-quantized", "int-quantized"):
                amax = np.maximum(np.abs(w).max(1), 1e-6)
                if ct_fmt == "float-quantized":
                    sc = (amax / 448.0).astype(np.float32).reshape(-1, 1)
                    q8 = torch.from_numpy(np.ascontiguousarray((w / sc).astype(np.float32))).to(torch.float8_e4m3fn)
                    tensors[f"{mod}.weight"] = q8
                    tensors[f"{mod}.weight_scale"] = sc
                    deq[k] = np.ascontiguousarray(fp8_dequant(q8.view(torch.uint8).numpy().copy(), sc, 1, w.shape[1]), dtype=np.float32)
                else:
                    sc = f16(amax / 127.0).astype(np.float16).reshape(-1, 1)
                    q8 = np.clip(np.round(w / sc.astype(np.float64)), -127, 127).astype(np.int8)
                    tensors[f"{mod}.weight"] = q8
                    tensors[f"{mod}.weight_scale"] = sc
                    deq[k] = np.ascontiguousarray((q8.astype(np.float64) * sc.astype(np.float64)), dtype=np.float32)
                continue
            if method == "bitsandbytes":
                skip = qc.get("llm_int8_skip_modules") or []
                leaf = mod.split(".")[-1]
                if leaf in skip or any(key + "." in mod or key == mod for key in skip):      # transformers' _replace_with_bnb_linear
                    tensors[k] = v.astype(np.float16)
                    deq[k] = v.astype(np.float16).astype(np.float32)
                    continue
                if qc["load_in_4bit"]:
                    dtype, bs = BNB_STATE[name]
                    t = bnb_ref.quantize_4bit(w, qc["bnb_4bit_quant_type"], bs, qc["bnb_4bit_use_double_quant"], dtype)
                    back = bnb_ref.dequantize_4bit(t)
                else:
                    t = bnb_ref.quantize_int8(w)
                    back = bnb_ref.dequantize_int8(t)
                for suffix, arr in t.items():
                    tensors[mod + suffix] = arr
                deq[k] = np.ascontiguousarray(back, dtype=np.float32)
                continue
            if method == "gptq" or (method == "compressed-tensors" and ct_fmt == "pack-quantized"):
                if method == "compressed-tensors":
                    cw = qc["config_groups"]["group_0"]["weights"]
                    q, z, s16, gi = gptq_quantize(w, bits, group, cw["symmetric"], cw["actorder"] is not None, rng)
                    packed = ct_pack(q, z, s16, gi, bits, cw["symmetric"], cw["actorder"] is not None)
                    for part, arr in packed.items():
                        tensors[f"{mod}.{part}"] = arr
                    deq[k] = np.ascontiguousarray(ct_dequant(packed, bits, group), dtype=np.float32)
                    ref = (s16[gi, :] * (q - z[gi, :])).T
                    assert np.array_equal(deq[k].astype(np.float64), ref.astype(np.float32).astype(np.float64)), f"{name}: {mod} does not unpack to its quantiser's grid"
                    continue
            if method == "gptq":
                q, z, s16, gi = gptq_quantize(w, bits, group, qc.get("sym", True), qc.get("desc_act", False), rng)
                packed = gptq_pack(q, z, s16, gi, bits, v2)
                back = gptq_dequant(packed, bits, v2)
            else:
                q, z, s16 = awq_quantize(w, group)
                packed = awq_pack(q, z, s16)
                back = awq_dequant(packed, group)
            # The packed tensors reproduce the quantiser's grid (a packing self-check).
            if method == "gptq":
                gi = packed["g_idx"]
                ref = (s16[gi, :] * (q - z[gi, :])).T
            else:
                gi = np.arange(w.shape[1]) // group
                ref = (s16[gi, :] * (q - z[gi, :])).T
            assert np.array_equal(back, ref), f"{name}: {mod} does not unpack to its quantiser's grid"
            for part, arr in packed.items():
                tensors[f"{mod}.{part}"] = arr
            # C order: safetensors writes the buffer as laid out (`back` is a transposed view).
            deq[k] = np.ascontiguousarray(back, dtype=np.float32)
        else:
            tensors[k] = v.astype(np.float16)
            deq[k] = v.astype(np.float16).astype(np.float32)
    # The reference: the same checkpoint with the dequantised weights, as transformers loads it.
    save_file(deq, os.path.join(tmp, "model.safetensors"), metadata={"format": "pt"})
    ref_model = AutoModelForCausalLM.from_pretrained(tmp, dtype=torch.float32, attn_implementation="eager")
    ref_model.eval()
    qcfg = dict(qc)
    if method == "gptq":
        qcfg.setdefault("checkpoint_format", "gptq")
        qcfg.update({"damp_percent": 0.01, "true_sequential": True, "static_groups": False})
    return ref_model, tensors, qcfg, seed


def save_float(model, d):
    """`save_pretrained` (config.json as transformers writes it, F32 weights), no generation config."""
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d)
    gp = os.path.join(d, "generation_config.json")
    if os.path.exists(gp):
        os.remove(gp)


def save_quant(model, tensors, qcfg, d):
    """The quantised checkpoint: transformers' config.json plus `quantization_config`, and the
    packed tensors in place of the float file."""
    save_float(model, d)
    if any(isinstance(v, torch.Tensor) for v in tensors.values()):
        # float8 tensors only exist in the torch flavour of safetensors
        from safetensors.torch import save_file as save_torch
        save_torch({k: (v if isinstance(v, torch.Tensor) else torch.from_numpy(np.ascontiguousarray(v))) for k, v in tensors.items()},
                   os.path.join(d, "model.safetensors"), metadata={"format": "pt"})
    else:
        save_file(tensors, os.path.join(d, "model.safetensors"), metadata={"format": "pt"})
    cp = os.path.join(d, "config.json")
    with open(cp) as f:
        cfg = json.load(f)
    cfg["quantization_config"] = qcfg
    cfg.pop("dtype", None)
    cfg["torch_dtype"] = "float16"
    with open(cp, "w") as f:
        json.dump(cfg, f, indent=2, sort_keys=True)
        f.write("\n")


def logits_of(model, ids):
    with torch.no_grad():
        return model(input_ids=torch.tensor([ids])).logits[0].double().tolist()


def write_fixture(name):
    model, tensors, qcfg, seed = build(name)
    d = os.path.join(FIX, name)
    save_quant(model, tensors, qcfg, d)
    ids = [(seed * 7 + 13 * i + i * i) % V for i in range(T)]
    full = logits_of(model, ids)
    meta = {"tokens": ids, "logits_full": full, "transformers": transformers.__version__, "torch": torch.__version__,
            "seed": seed, "weights": "dequantised from the packed tensors (numpy), float32"}
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    return os.path.getsize(os.path.join(d, "model.safetensors"))


PROMPTS, PROMPT_LEN, NEW = 3, 12, 12
SEQS, SEQ_LEN = 2, 48


def write_refs(model, d, vocab, seed):
    """hf.json, hf-logits.f32, calib.json exactly as tools/audit_e2e.py writes them."""
    rng = np.random.default_rng(seed)
    model.generation_config.eos_token_id = None
    gc = transformers.GenerationConfig(max_new_tokens=NEW, do_sample=False, num_beams=1, eos_token_id=None, pad_token_id=0)
    recs = []
    with torch.no_grad():
        for _ in range(PROMPTS):
            ids = [int(t) for t in rng.integers(0, vocab, size=PROMPT_LEN)]
            t = torch.tensor([ids])
            gen = model.generate(input_ids=t, attention_mask=torch.ones_like(t), generation_config=gc)[0, PROMPT_LEN:].tolist()
            lg = model(input_ids=torch.tensor([ids + gen[:-1]])).logits[0].tolist()
            margins = []
            for row in lg[PROMPT_LEN - 1:]:
                top = sorted(row, reverse=True)[:2]
                margins.append(top[0] - top[1])
            recs.append({"input_ids": ids, "generated": gen, "margins": margins})
        seqs = [[int(t) for t in rng.integers(0, vocab, size=SEQ_LEN)] for _ in range(SEQS)]
        rows = []
        for s in seqs:
            rows.extend(model(input_ids=torch.tensor([s])).logits[0].tolist())
    np.asarray(rows, dtype="<f4").tofile(os.path.join(d, "hf-logits.f32"))
    json.dump({"records": recs, "sequences": seqs, "vocab": vocab, "transformers": transformers.__version__},
              open(os.path.join(d, "hf.json"), "w"))
    calib = [[int(t) for t in rng.integers(0, vocab, size=512)] for _ in range(2)]
    json.dump({"source": "random (audit)", "sequences": calib}, open(os.path.join(d, "calib.json"), "w"))
    return recs


def write_audit(out_root, name):
    model, tensors, qcfg, seed = build(name)
    d = os.path.join(out_root, name)
    # The float twin first: identical weights, no quantization_config, F32 (the dequantised
    # values are exact in f32, not always in f16).
    fd = os.path.join(out_root, name + "-float")
    save_float(model, fd)
    save_quant(model, tensors, qcfg, d)
    recs = write_refs(model, d, V, seed)
    for fn in ("hf.json", "hf-logits.f32", "calib.json"):
        with open(os.path.join(d, fn), "rb") as a, open(os.path.join(fd, fn), "wb") as b:
            b.write(a.read())
    return recs


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    if len(sys.argv) < 2 or sys.argv[1] not in ("fixtures", "audit"):
        sys.exit(__doc__)
    mode = sys.argv[1]
    rest = sys.argv[2:]
    out = None
    if mode == "audit":
        out, rest = rest[0], rest[1:]
    bad = 0
    for n in rest or list(CONFIGS):
        try:
            if mode == "fixtures":
                sz = write_fixture(n)
                print(f"ok   {n:24s} {sz} bytes", flush=True)
            else:
                recs = write_audit(out, n)
                print(f"ok   {n:24s} {[r['generated'][:6] for r in recs]}", flush=True)
        except Exception as e:  # report and continue
            bad += 1
            print(f"FAIL {n:24s} {type(e).__name__}: {str(e)[:300]}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
