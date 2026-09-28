#!/usr/bin/env python3
"""GGUF fixtures for misaka-palw-tir-lower, written in numpy per the GGUF file format and ggml's block
layouts (`ggml-common.h`, `dequantize_row_*`). No gguf package, no llama.cpp, no hub access: run with
HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1.

Each fixture is a tiny random decoder built with transformers' own config class, converted the way
llama.cpp's `convert_hf_to_gguf.py` does it, then quantised per tensor with a type plan:

  * names: `token_embd`, `blk.N.attn_{norm,q,k,v,output}`, `blk.N.ffn_{norm,gate,up,down}`,
    `output_norm`, `output` (+ `attn_{q,k}_norm` for qwen3, `post_attention_norm`/`post_ffw_norm`
    for gemma2);
  * `llama`: q/k rows permuted `[heads, 2, d/2] -> [heads, d/2, 2]` (the converter's `permute`);
  * gemma/gemma2: every `*norm.weight` stored as `1 + w`;
  * 1-D tensors F32; matrices per the plan (Q4_K_M/Q5_K_M: Q6_K for `attn_v`/`ffn_down` on
    llama.cpp's `use_more_bits` layers and for `output`, the base type elsewhere; K-quants fall
    back to Q5_0 / Q5_1 / Q8_0 when a row is not a multiple of 256).

Quantisation is round-to-nearest per block (Q8_0, Q4_0/Q5_0 as llama.cpp's reference quantiser,
Q4_1/Q5_1, and the K-quants with 6-bit sub-block scales/minimums or signed 8-bit Q6_K scales under
an fp16 super-block scale): any valid block is a valid fixture, since the dequantisation is what
defines the weights. THE REFERENCE is transformers' float model with the weights dequantised from
the packed bytes (an independent numpy decoder), llama's q/k rows permuted back.

Modes:
  gen_gguf_fixtures.py fixtures [name ...]
      tests/fixtures/gguf/<name>/{model.gguf, hf_config.json, logits.json}
  gen_gguf_fixtures.py audit OUT [name ...]
      OUT/<name>/{model.gguf, hf.json, hf-logits.f32, calib.json} and OUT/<name>-float/ (the same
      dequantised weights as a Hugging Face F32 checkpoint: the W8 path's twin)
"""

import json
import math
import os
import struct
import sys

import numpy as np
import torch
from safetensors.numpy import save_file

try:
    import transformers
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
except ImportError as e:  # pragma: no cover
    sys.exit(f"transformers is required: {e}")

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "gguf")
sys.path.insert(0, HERE)

V = 256
T = 16

# ggml types
F32, F16, Q4_0, Q4_1, Q5_0, Q5_1, Q8_0, Q4_K, Q5_K, Q6_K, BF16 = 0, 1, 2, 3, 6, 7, 8, 12, 13, 14, 30
BLOCK = {Q4_0: (32, 18), Q4_1: (32, 20), Q5_0: (32, 22), Q5_1: (32, 24), Q8_0: (32, 34), Q4_K: (256, 144),
         Q5_K: (256, 176), Q6_K: (256, 210)}
TNAME = {F32: "F32", F16: "F16", BF16: "BF16", Q4_0: "Q4_0", Q4_1: "Q4_1", Q5_0: "Q5_0", Q5_1: "Q5_1",
         Q8_0: "Q8_0", Q4_K: "Q4_K", Q5_K: "Q5_K", Q6_K: "Q6_K"}
# GGUF metadata value types
GT_U32, GT_I32, GT_F32, GT_STR, GT_ARR = 4, 5, 6, 8, 9


def h(x):
    return np.float16(x)


# ───────────────────────────── quantisers (block layouts of ggml-common.h) ─────────────────────────────

def q8_0(b):
    amax = np.abs(b).max()
    d = h(amax / 127.0)
    q = np.clip(np.round(b / float(d)), -127, 127) if float(d) else np.zeros(32)
    return d.tobytes() + q.astype(np.int8).tobytes()


def q4_0(b, nq=16):
    i = int(np.argmax(np.abs(b)))
    d = h(b[i] / -(nq // 2))
    q = np.clip(np.floor(b / float(d) + nq // 2 + 0.5), 0, nq - 1).astype(np.int64) if float(d) else np.full(32, nq // 2)
    return d, q


def pack_q4_0(b):
    d, q = q4_0(b, 16)
    return d.tobytes() + (q[:16] | (q[16:] << 4)).astype(np.uint8).tobytes()


def qh_bits(q):
    v = 0
    for j in range(32):
        v |= int((q[j] >> 4) & 1) << j
    return struct.pack("<I", v)


def pack_q5_0(b):
    d, q = q4_0(b, 32)
    return d.tobytes() + qh_bits(q) + ((q[:16] & 15) | ((q[16:] & 15) << 4)).astype(np.uint8).tobytes()


def q4_1(b, nq):
    mn, mx = b.min(), b.max()
    d, m = h((mx - mn) / (nq - 1)), h(mn)
    q = np.clip(np.floor((b - float(m)) / float(d) + 0.5), 0, nq - 1).astype(np.int64) if float(d) else np.zeros(32, np.int64)
    return d, m, q


def pack_q4_1(b):
    d, m, q = q4_1(b, 16)
    return d.tobytes() + m.tobytes() + (q[:16] | (q[16:] << 4)).astype(np.uint8).tobytes()


def pack_q5_1(b):
    d, m, q = q4_1(b, 32)
    return d.tobytes() + m.tobytes() + qh_bits(q) + ((q[:16] & 15) | ((q[16:] & 15) << 4)).astype(np.uint8).tobytes()


def k_scales(sc, mn):
    """The 12-byte 6-bit (scale, min) packing that get_scale_min_k4 reads."""
    q = np.zeros(12, dtype=np.uint8)
    for j in range(4):
        q[j] = (sc[j] & 63) | ((sc[j + 4] >> 4) << 6)
        q[j + 4] = (mn[j] & 63) | ((mn[j + 4] >> 4) << 6)
        q[j + 8] = (sc[j + 4] & 15) | ((mn[j + 4] & 15) << 4)
    return q.tobytes()


def pack_qk(b, nq):
    """Q4_K (nq = 16) and Q5_K (nq = 32): 8 sub-blocks of 32, x = d*sc*q - dmin*m."""
    subs = b.reshape(8, 32)
    mins = -np.minimum(subs.min(1), 0.0)
    scales = (subs.max(1) + mins) / (nq - 1)
    d, dmin = h(scales.max() / 63.0), h(mins.max() / 63.0)
    sc = np.clip(np.round(scales / float(d)), 0, 63).astype(np.int64) if float(d) else np.zeros(8, np.int64)
    mq = np.clip(np.round(mins / float(dmin)), 0, 63).astype(np.int64) if float(dmin) else np.zeros(8, np.int64)
    q = np.zeros(256, dtype=np.int64)
    for j in range(8):
        s, m = float(d) * sc[j], float(dmin) * mq[j]
        if s > 0:
            q[32 * j:32 * (j + 1)] = np.clip(np.round((subs[j] + m) / s), 0, nq - 1)
    out = d.tobytes() + dmin.tobytes() + k_scales(sc, mq)
    if nq == 32:
        qh = np.zeros(32, dtype=np.int64)
        for c in range(4):
            qh |= ((q[64 * c:64 * c + 32] >> 4) & 1) << (2 * c)
            qh |= ((q[64 * c + 32:64 * c + 64] >> 4) & 1) << (2 * c + 1)
        out += qh.astype(np.uint8).tobytes()
    qs = np.zeros(128, dtype=np.int64)
    for c in range(4):
        qs[32 * c:32 * c + 32] = (q[64 * c:64 * c + 32] & 15) | ((q[64 * c + 32:64 * c + 64] & 15) << 4)
    return out + qs.astype(np.uint8).tobytes()


def pack_q6_k(b):
    subs = b.reshape(16, 16)
    mx = subs[np.arange(16), np.argmax(np.abs(subs), axis=1)]
    scales = mx / -32.0
    big = scales[int(np.argmax(np.abs(scales)))]
    if big == 0:
        return bytes(208) + h(0.0).tobytes()
    iscale = -128.0 / big
    d = h(1.0 / iscale)
    sc = np.clip(np.round(iscale * scales), -128, 127).astype(np.int64)
    q = np.full(256, 32, dtype=np.int64)
    for k in range(16):
        s = float(d) * sc[k]
        if s != 0:
            q[16 * k:16 * (k + 1)] = np.clip(np.round(subs[k] / s), -32, 31) + 32
    ql = np.zeros(128, dtype=np.int64)
    qh = np.zeros(64, dtype=np.int64)
    for n in range(2):
        for l in range(32):
            a, b2, c, e = q[128 * n + l], q[128 * n + l + 32], q[128 * n + l + 64], q[128 * n + l + 96]
            ql[64 * n + l] = (a & 15) | ((c & 15) << 4)
            ql[64 * n + l + 32] = (b2 & 15) | ((e & 15) << 4)
            qh[32 * n + l] = (a >> 4) | ((b2 >> 4) << 2) | ((c >> 4) << 4) | ((e >> 4) << 6)
    return ql.astype(np.uint8).tobytes() + qh.astype(np.uint8).tobytes() + sc.astype(np.int8).tobytes() + d.tobytes()


PACK = {Q8_0: q8_0, Q4_0: pack_q4_0, Q5_0: pack_q5_0, Q4_1: pack_q4_1, Q5_1: pack_q5_1,
        Q4_K: lambda b: pack_qk(b, 16), Q5_K: lambda b: pack_qk(b, 32), Q6_K: pack_q6_k}


def quantize(x, ty):
    """x: [rows, ne0] float -> the tensor's bytes."""
    if ty == F32:
        return x.astype("<f4").tobytes()
    if ty == F16:
        return x.astype("<f2").tobytes()
    if ty == BF16:
        u = x.astype(np.float32).view(np.uint32)
        return ((u + 0x7FFF + ((u >> 16) & 1)) >> 16).astype("<u2").tobytes()
    be, _ = BLOCK[ty]
    out = bytearray()
    for b in x.reshape(-1, be):
        out += PACK[ty](b.astype(np.float64))
    return bytes(out)


# ───────────────────────────── the independent decoder (dequantize_row_*) ─────────────────────────────

def f16_at(b, i):
    return float(np.frombuffer(b[i:i + 2], dtype="<f2")[0])


def scale_min_k4(j, q):
    if j < 4:
        return q[j] & 63, q[j + 4] & 63
    return (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4), (q[j + 4] >> 4) | ((q[j] >> 6) << 4)


def dequant(raw, ty, rows, ne0):
    if ty == F32:
        return np.frombuffer(raw, dtype="<f4").reshape(rows, ne0).astype(np.float64)
    if ty == F16:
        return np.frombuffer(raw, dtype="<f2").reshape(rows, ne0).astype(np.float64)
    if ty == BF16:
        return (np.frombuffer(raw, dtype="<u2").astype(np.uint32) << 16).view(np.float32).reshape(rows, ne0).astype(np.float64)
    be, bb = BLOCK[ty]
    y = np.zeros(rows * ne0)
    for i in range(rows * ne0 // be):
        b = raw[i * bb:(i + 1) * bb]
        o = y[i * be:(i + 1) * be]
        if ty == Q8_0:
            o[:] = f16_at(b, 0) * np.frombuffer(b[2:34], dtype=np.int8)
        elif ty in (Q4_0, Q4_1, Q5_0, Q5_1):
            d = f16_at(b, 0)
            at = 4 if ty in (Q4_1, Q5_1) else 2
            m = f16_at(b, 2) if ty in (Q4_1, Q5_1) else 0.0
            hi = np.zeros(32, dtype=np.int64)
            if ty in (Q5_0, Q5_1):
                qh = struct.unpack("<I", b[at:at + 4])[0]
                hi = np.array([((qh >> j) & 1) << 4 for j in range(32)])
                at += 4
            qs = np.frombuffer(b[at:at + 16], dtype=np.uint8).astype(np.int64)
            q = np.concatenate([qs & 15, qs >> 4]) | hi
            if ty == Q4_0:
                o[:] = (q - 8) * d
            elif ty == Q5_0:
                o[:] = (q - 16) * d
            else:
                o[:] = q * d + m
        elif ty in (Q4_K, Q5_K):
            d, dmin = f16_at(b, 0), f16_at(b, 2)
            sc = np.frombuffer(b[4:16], dtype=np.uint8).astype(np.int64)
            if ty == Q5_K:
                qh = np.frombuffer(b[16:48], dtype=np.uint8).astype(np.int64)
                ql = np.frombuffer(b[48:176], dtype=np.uint8).astype(np.int64)
            else:
                qh = np.zeros(32, dtype=np.int64)
                ql = np.frombuffer(b[16:144], dtype=np.uint8).astype(np.int64)
            for c in range(4):
                for hh in range(2):
                    s = 2 * c + hh
                    scv, mv = scale_min_k4(s, sc)
                    nib = (ql[32 * c:32 * c + 32] & 15) if hh == 0 else (ql[32 * c:32 * c + 32] >> 4)
                    q = nib + ((qh >> s) & 1) * 16
                    o[64 * c + 32 * hh:64 * c + 32 * hh + 32] = d * scv * q - dmin * mv
        elif ty == Q6_K:
            ql = np.frombuffer(b[0:128], dtype=np.uint8).astype(np.int64)
            qh = np.frombuffer(b[128:192], dtype=np.uint8).astype(np.int64)
            sc = np.frombuffer(b[192:208], dtype=np.int8).astype(np.int64)
            d = f16_at(b, 208)
            for n in range(2):
                for l in range(32):
                    hq = qh[32 * n + l]
                    q1 = ((ql[64 * n + l] & 15) | ((hq & 3) << 4)) - 32
                    q2 = ((ql[64 * n + l + 32] & 15) | (((hq >> 2) & 3) << 4)) - 32
                    q3 = ((ql[64 * n + l] >> 4) | (((hq >> 4) & 3) << 4)) - 32
                    q4 = ((ql[64 * n + l + 32] >> 4) | (((hq >> 6) & 3) << 4)) - 32
                    is_ = 8 * n + l // 16
                    o[128 * n + l] = d * sc[is_] * q1
                    o[128 * n + l + 32] = d * sc[is_ + 2] * q2
                    o[128 * n + l + 64] = d * sc[is_ + 4] * q3
                    o[128 * n + l + 96] = d * sc[is_ + 6] * q4
    # llama.cpp's f32: one rounding of the exact value.
    return y.astype(np.float32).astype(np.float64).reshape(rows, ne0)


# ───────────────────────────── the GGUF container ─────────────────────────────

def s_(s):
    b = s.encode()
    return struct.pack("<Q", len(b)) + b


def kv(key, ty, v):
    out = s_(key) + struct.pack("<I", ty)
    if ty == GT_U32:
        return out + struct.pack("<I", v)
    if ty == GT_I32:
        return out + struct.pack("<i", v)
    if ty == GT_F32:
        return out + struct.pack("<f", v)
    if ty == GT_STR:
        return out + s_(v)
    if ty == GT_ARR:
        et, items = v
        body = struct.pack("<IQ", et, len(items))
        for it in items:
            body += s_(it) if et == GT_STR else struct.pack("<i", it)
        return out + body
    raise ValueError(ty)


def write_gguf(path, kvs, tensors, align=32):
    head = b"GGUF" + struct.pack("<IQQ", 3, len(tensors), len(kvs)) + b"".join(kvs)
    infos, blob = b"", b""
    for name, dims, ty, data in tensors:
        pad = (-len(blob)) % align
        blob += bytes(pad)
        infos += s_(name) + struct.pack("<I", len(dims)) + b"".join(struct.pack("<Q", d) for d in dims)
        infos += struct.pack("<IQ", ty, len(blob))
        blob += data
    body = head + infos
    body += bytes((-len(body)) % align)
    with open(path, "wb") as f:
        f.write(body + blob)


# ───────────────────────────── models and plans ─────────────────────────────

BASE = dict(hidden_size=256, intermediate_size=256, num_attention_heads=4, num_key_value_heads=2, vocab_size=V,
            num_hidden_layers=2, max_position_embeddings=1024, head_dim=64)
MODELS = {
    "llama": ("llama", "LlamaForCausalLM", dict(BASE, tie_word_embeddings=False, rms_norm_eps=1e-5, rope_theta=500000.0)),
    "qwen2": ("qwen2", "Qwen2ForCausalLM", dict(BASE, tie_word_embeddings=True, rms_norm_eps=1e-6, rope_theta=1000000.0)),
    "qwen3": ("qwen3", "Qwen3ForCausalLM", dict(BASE, tie_word_embeddings=False, rms_norm_eps=1e-6, rope_theta=1000000.0)),
    "gemma": ("gemma", "GemmaForCausalLM", dict(BASE, tie_word_embeddings=True, rms_norm_eps=1e-6, hidden_activation="gelu_pytorch_tanh")),
    "gemma2": ("gemma2", "Gemma2ForCausalLM", dict(BASE, tie_word_embeddings=True, rms_norm_eps=1e-6, sliding_window=8,
                                                   query_pre_attn_scalar=64, attn_logit_softcapping=50.0,
                                                   final_logit_softcapping=30.0, hidden_activation="gelu_pytorch_tanh")),
    # Mistral converts to llama.cpp's `llama` architecture.
    "mistral": ("mistral", "MistralForCausalLM", dict(BASE, intermediate_size=288, tie_word_embeddings=False, sliding_window=None,
                                                      rms_norm_eps=1e-5, rope_theta=1000000.0)),
    # 32-wide rows (Q8_0/Q4_0 blocks) keep these small: Gemma-3 needs six layers for llama.cpp's
    # fixed pattern (every 6th global), Qwen3.5 two value heads per key head (the tiled order).
    "gemma3": ("gemma3_text", "Gemma3ForCausalLM", dict(hidden_size=64, intermediate_size=128, num_attention_heads=2,
                                                        num_key_value_heads=1, head_dim=32, vocab_size=V, num_hidden_layers=6,
                                                        max_position_embeddings=1024, sliding_window=4, query_pre_attn_scalar=32,
                                                        rms_norm_eps=1e-6, tie_word_embeddings=True,
                                                        rope_parameters={"sliding_attention": {"rope_type": "default", "rope_theta": 10000.0},
                                                                         "full_attention": {"rope_type": "linear", "factor": 8.0,
                                                                                            "rope_theta": 1000000.0}})),
    "phi3": ("phi3", "Phi3ForCausalLM", dict(hidden_size=64, intermediate_size=128, num_attention_heads=2, num_key_value_heads=2,
                                             vocab_size=V, num_hidden_layers=2, max_position_embeddings=256,
                                             original_max_position_embeddings=64, sliding_window=None, rms_norm_eps=1e-5,
                                             tie_word_embeddings=False, pad_token_id=0, bos_token_id=1, eos_token_id=2,
                                             rope_scaling={"type": "longrope",
                                                           "short_factor": [1.0 + 0.1 * i for i in range(16)],
                                                           "long_factor": [2.0 + 0.5 * i for i in range(16)]})),
    "qwen3moe": ("qwen3_moe", "Qwen3MoeForCausalLM", dict(hidden_size=64, intermediate_size=128, moe_intermediate_size=64,
                                                        num_experts=4, num_experts_per_tok=2, norm_topk_prob=True,
                                                        num_attention_heads=2, num_key_value_heads=1, head_dim=32, vocab_size=V,
                                                        num_hidden_layers=2, max_position_embeddings=1024, rms_norm_eps=1e-6,
                                                        tie_word_embeddings=False)),
    "mixtral": ("mixtral", "MixtralForCausalLM", dict(hidden_size=64, intermediate_size=64, num_local_experts=4,
                                                      num_experts_per_tok=2, num_attention_heads=2, num_key_value_heads=1,
                                                      head_dim=32, vocab_size=V, num_hidden_layers=2, max_position_embeddings=1024,
                                                      rms_norm_eps=1e-5, tie_word_embeddings=False, sliding_window=None)),
    "qwen35": ("qwen3_5_text", "Qwen3_5ForCausalLM", dict(hidden_size=64, intermediate_size=128, num_attention_heads=2,
                                                         num_key_value_heads=1, head_dim=32, vocab_size=V, num_hidden_layers=4,
                                                         max_position_embeddings=1024, rms_norm_eps=1e-6, tie_word_embeddings=True,
                                                         full_attention_interval=2, linear_num_key_heads=2, linear_num_value_heads=4,
                                                         linear_key_head_dim=16, linear_value_head_dim=16, linear_conv_kernel_dim=4)),
}
GGUF_ARCH = {"llama": "llama", "qwen2": "qwen2", "qwen3": "qwen3", "gemma": "gemma", "gemma2": "gemma2", "mistral": "llama",
             "gemma3": "gemma3", "phi3": "phi3", "qwen35": "qwen35", "qwen3moe": "qwen3moe", "mixtral": "llama"}


def use_more_bits(i, n):
    return i < n // 8 or i >= 7 * n // 8 or (i - n // 8) % 3 == 2


def k_mix(base):
    """llama.cpp's Q4_K_M / Q5_K_M: Q6_K for attn_v and ffn_down on use_more_bits layers and for
    output, `base` elsewhere; a row that is not whole 256-blocks falls back."""
    fallback = {Q4_K: Q5_0, Q5_K: Q5_1, Q6_K: Q8_0}

    def plan(gname, layer, n_layers, ne0):
        if gname == "output.weight":
            t = Q6_K
        elif layer is not None and gname.split(".")[2] in ("attn_v", "ffn_down") and use_more_bits(layer, n_layers):
            t = Q6_K
        else:
            t = base
        return fallback[t] if ne0 % 256 else t
    return plan


def fixed(t, embd=None, out=None):
    def plan(gname, layer, n_layers, ne0):
        if gname == "token_embd.weight" and embd is not None:
            return embd
        if gname == "output.weight" and out is not None:
            return out
        return t
    return plan


def mistral_mix(gname, layer, n_layers, ne0):
    if gname == "token_embd.weight":
        return F16
    if gname == "output.weight":
        return BF16
    part = gname.split(".")[2] if layer is not None else ""
    return {"attn_q": Q4_1, "attn_k": Q5_1, "attn_v": Q5_0, "attn_output": Q4_0, "ffn_gate": Q4_K, "ffn_up": Q4_K,
            "ffn_down": Q5_0 if layer == 0 else Q8_0}[part]


CONFIGS = {
    "gguf_llama_q4_k_m": ("llama", k_mix(Q4_K), 15),
    "gguf_qwen2_q5_k_m": ("qwen2", k_mix(Q5_K), 17),
    "gguf_qwen3_q8_0": ("qwen3", fixed(Q8_0), 7),
    "gguf_gemma_q4_0": ("gemma", fixed(Q4_0, embd=Q8_0), 2),
    "gguf_gemma2_q6_k": ("gemma2", fixed(Q6_K), 18),
    "gguf_mistral_mix": ("mistral", mistral_mix, 1),
    "gguf_gemma3_q4_0": ("gemma3", fixed(Q4_0, embd=Q8_0), 2),
    "gguf_phi3_q8_0": ("phi3", fixed(Q8_0, out=Q4_0), 7),
    "gguf_qwen35_q8_0": ("qwen35", fixed(Q8_0, embd=Q4_0), 7),
    "gguf_qwen3moe_q4_0": ("qwen3moe", fixed(Q4_0, embd=Q8_0, out=Q8_0), 2),
    "gguf_mixtral_q8_0": ("mixtral", fixed(Q8_0), 7),
}


def permute(w, n_head):
    """convert_hf_to_gguf.py's LlamaModel.permute (rows)."""
    return w.reshape(n_head, 2, w.shape[0] // n_head // 2, *w.shape[1:]).swapaxes(1, 2).reshape(w.shape)


def unpermute(w, n_head):
    return w.reshape(n_head, w.shape[0] // n_head // 2, 2, *w.shape[1:]).swapaxes(1, 2).reshape(w.shape)


def hf_to_gguf(name, arch):
    if name == "model.embed_tokens.weight":
        return "token_embd.weight"
    if name == "model.norm.weight":
        return "output_norm.weight"
    if name == "lm_head.weight":
        return "output.weight"
    p = name.split(".")
    l, rest = p[2], ".".join(p[3:])
    moe = {"mlp.gate.weight": "ffn_gate_inp.weight", "mlp.experts.gate_proj3d": "ffn_gate_exps.weight",
           "mlp.experts.up_proj3d": "ffn_up_exps.weight", "mlp.experts.down_proj": "ffn_down_exps.weight"}
    if rest in moe:
        return f"blk.{l}.{moe[rest]}"
    if arch == "phi3" and rest in ("self_attn.qkv_proj.weight", "mlp.gate_up_proj.weight"):
        return f"blk.{l}.{'attn_qkv' if 'qkv' in rest else 'ffn_up'}.weight"
    if arch == "qwen35" and rest.startswith("linear_attn."):
        la = {"in_proj_qkv.weight": "attn_qkv.weight", "in_proj_z.weight": "attn_gate.weight", "in_proj_a.weight": "ssm_alpha.weight",
              "in_proj_b.weight": "ssm_beta.weight", "conv1d.weight": "ssm_conv1d.weight", "A_log": "ssm_a", "dt_bias": "ssm_dt.bias",
              "norm.weight": "ssm_norm.weight", "out_proj.weight": "ssm_out.weight"}
        return f"blk.{l}.{la[rest[len('linear_attn.'):]]}"
    table = {
        "input_layernorm.weight": "attn_norm.weight",
        "self_attn.q_norm.weight": "attn_q_norm.weight",
        "self_attn.k_norm.weight": "attn_k_norm.weight",
        "mlp.gate_proj.weight": "ffn_gate.weight",
        "mlp.up_proj.weight": "ffn_up.weight",
        "mlp.down_proj.weight": "ffn_down.weight",
    }
    for proj, g in (("q_proj", "attn_q"), ("k_proj", "attn_k"), ("v_proj", "attn_v"), ("o_proj", "attn_output")):
        for sfx in ("weight", "bias"):
            table[f"self_attn.{proj}.{sfx}"] = f"{g}.{sfx}"
    if arch in ("gemma2", "gemma3"):
        table.update({"post_attention_layernorm.weight": "post_attention_norm.weight",
                      "pre_feedforward_layernorm.weight": "ffn_norm.weight",
                      "post_feedforward_layernorm.weight": "post_ffw_norm.weight"})
    elif arch == "qwen35":
        table["post_attention_layernorm.weight"] = "post_attention_norm.weight"
    else:
        table["post_attention_layernorm.weight"] = "ffn_norm.weight"
    return f"blk.{l}.{table[rest]}"


def build(name):
    model_name, plan, ftype = CONFIGS[name]
    model_type, hf_arch, kw = MODELS[model_name]
    arch = GGUF_ARCH[model_name]
    seed = sum(ord(ch) for ch in name) + 311
    cfg = CONFIG_MAPPING[model_type](**kw)
    cfg.architectures = [hf_arch]
    torch.manual_seed(seed)
    # Eager attention is the defining math: sdpa silently drops Gemma-2's score soft-cap.
    model = AutoModelForCausalLM.from_config(cfg, attn_implementation="eager")
    model.eval()
    g = torch.Generator().manual_seed(seed + 1)
    with torch.no_grad():
        for pname, p in model.named_parameters():
            if p.ndim >= 2 and "embed" in pname:
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(cfg.hidden_size))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            # bf16-exact values: `1 + w` and back are exact in f32 (Gemma's norms).
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
    sd = {k: v.double().numpy() for k, v in model.state_dict().items()}
    n_layers, heads, kvh = cfg.num_hidden_layers, cfg.num_attention_heads, cfg.num_key_value_heads
    hd = getattr(cfg, "head_dim", None) or cfg.hidden_size // heads
    gemma = arch.startswith("gemma")
    qwen35 = arch == "qwen35"
    # `1 + w` RMSNorms stored as `1 + w` (convert_hf_to_gguf.py): Gemma, and Qwen3.5 except its
    # gated norm.
    shifted = lambda k: k.endswith("norm.weight") and (gemma or (qwen35 and not k.endswith("linear_attn.norm.weight")))
    if qwen35:
        nk, nv = cfg.linear_num_key_heads, cfg.linear_num_value_heads
        dk, dv, rr = cfg.linear_key_head_dim, cfg.linear_value_head_dim, nv // nk

        def tiled(d):
            # GGUF position j = (v, k, e) holds HF index (k, v, e): `_reorder_v_heads`.
            return np.array([(k * rr + v) * d + e for v in range(rr) for k in range(nk) for e in range(d)])

        def qkv_perm():
            qk = 2 * nk * dk
            return np.concatenate([np.arange(qk), qk + tiled(dv)])
    tensors, deq = [], {}
    types = {}
    # transformers 5 keeps experts fused in memory (`experts.gate_up_proj [E, 2I, D]`); GGUF stores
    # `ffn_gate_exps`/`ffn_up_exps [E, I, D]` and `ffn_down_exps [E, D, I]`.
    fused = [k for k in sd if k.endswith("mlp.experts.gate_up_proj")]
    for k in fused:
        gu = sd.pop(k)
        half = gu.shape[1] // 2
        base = k[: -len("gate_up_proj")]
        sd[base + "gate_proj3d"], sd[base + "up_proj3d"] = gu[:, :half, :], gu[:, half:, :]
    for k, w in sd.items():
        if cfg.tie_word_embeddings and k == "lm_head.weight":
            continue
        gname = hf_to_gguf(k, arch)
        layer = int(gname.split(".")[1]) if gname.startswith("blk.") else None
        x = w.copy()
        if arch == "llama" and (".q_proj." in k or ".k_proj." in k):
            x = permute(x, heads if ".q_proj." in k else kvh)
        if shifted(k):
            x = (x.astype(np.float32) + np.float32(1.0)).astype(np.float64)
        rows_perm, cols_perm = None, None
        if qwen35 and ".linear_attn." in k:
            if k.endswith("conv1d.weight"):
                x = x.reshape(x.shape[0], x.shape[-1])
            if k.endswith(("in_proj_qkv.weight", "conv1d.weight")):
                rows_perm = qkv_perm()
            elif k.endswith("in_proj_z.weight"):
                rows_perm = tiled(dv)
            elif k.endswith(("in_proj_a.weight", "in_proj_b.weight", "A_log", "dt_bias")):
                rows_perm = tiled(1)
            elif k.endswith("out_proj.weight"):
                cols_perm = tiled(dv)
            if rows_perm is not None:
                x = x[rows_perm]
            if cols_perm is not None:
                x = x[:, cols_perm]
            if k.endswith("A_log"):
                x = -np.exp(x.astype(np.float32)).astype(np.float64)
        if x.ndim == 1:
            ty = F32
            raw = quantize(x.reshape(1, -1), F32)
            back = dequant(raw, F32, 1, x.shape[0]).reshape(-1)
        elif x.ndim == 3:
            ne, rows, ne0 = x.shape
            ty = plan(gname, layer, n_layers, ne0)
            raw = quantize(x.reshape(ne * rows, ne0), ty)
            back = dequant(raw, ty, ne * rows, ne0).reshape(x.shape)
        elif gname.endswith("ffn_gate_inp.weight"):
            # llama.cpp keeps the router in float.
            ty = F32
            raw = quantize(x, F32)
            back = dequant(raw, F32, *x.shape)
        else:
            rows, ne0 = x.shape
            # llama.cpp keeps the convolution kernels in float.
            ty = F32 if gname.endswith("ssm_conv1d.weight") else plan(gname, layer, n_layers, ne0)
            raw = quantize(x, ty)
            back = dequant(raw, ty, rows, ne0)
        types[gname] = TNAME[ty]
        # The blocks decode to the weights they quantised (a packing check): relative RMS error
        # within the type's own resolution.
        rel = float(np.sqrt(((back.reshape(x.shape) - x) ** 2).sum() / max((x ** 2).sum(), 1e-30)))
        bound = {F32: 0.0, F16: 1e-3, BF16: 5e-3, Q8_0: 0.02, Q6_K: 0.05, Q5_0: 0.1, Q5_1: 0.1, Q5_K: 0.1}.get(ty, 0.2)
        assert rel <= bound, f"{name}: {gname} ({TNAME[ty]}) decodes to relative error {rel:.4f} > {bound}"
        tensors.append((gname, list(reversed(x.shape)), ty, raw))
        # The reference weight: the dequantised tensor in Hugging Face's parameterisation.
        r = back.reshape(x.shape)
        if arch == "llama" and (".q_proj." in k or ".k_proj." in k):
            r = unpermute(r, heads if ".q_proj." in k else kvh)
        if shifted(k):
            r = (r.astype(np.float32) - np.float32(1.0)).astype(np.float64)
        if qwen35 and ".linear_attn." in k:
            if k.endswith("A_log"):
                r = np.log(-r.astype(np.float32)).astype(np.float64)
            if rows_perm is not None:
                inv = np.empty_like(rows_perm)
                inv[rows_perm] = np.arange(len(rows_perm))
                r = r[inv]
            if cols_perm is not None:
                inv = np.empty_like(cols_perm)
                inv[cols_perm] = np.arange(len(cols_perm))
                r = r[:, inv]
            r = r.reshape(w.shape)
        deq[k] = torch.from_numpy(r.astype(np.float32))
    for k in fused:
        base = k[: -len("gate_up_proj")]
        deq[k] = torch.cat([deq.pop(base + "gate_proj3d"), deq.pop(base + "up_proj3d")], dim=1)
        sd.pop(base + "gate_proj3d")
        sd.pop(base + "up_proj3d")
        sd[k] = deq[k].double().numpy()
    if cfg.tie_word_embeddings:
        # One parameter under two names: both must carry the dequantised table.
        deq["lm_head.weight"] = deq["model.embed_tokens.weight"]
    model.load_state_dict({**{k: torch.from_numpy(v.astype(np.float32)) for k, v in sd.items()}, **deq})
    p = arch + "."
    kvs = [
        kv("general.architecture", GT_STR, arch),
        kv("general.name", GT_STR, name),
        kv("general.file_type", GT_U32, ftype),
        kv("general.quantization_version", GT_U32, 2),
        kv(p + "context_length", GT_U32, cfg.max_position_embeddings),
        kv(p + "embedding_length", GT_U32, cfg.hidden_size),
        kv(p + "block_count", GT_U32, n_layers),
        kv(p + "feed_forward_length", GT_U32, cfg.intermediate_size),
        kv(p + "attention.head_count", GT_U32, heads),
        kv(p + "attention.head_count_kv", GT_U32, kvh),
        kv(p + "attention.layer_norm_rms_epsilon", GT_F32, cfg.rms_norm_eps),
        kv(p + "attention.key_length", GT_U32, hd),
        kv(p + "attention.value_length", GT_U32, hd),
        kv(p + "rope.dimension_count", GT_U32, int(hd * (cfg.rope_parameters or {}).get("partial_rotary_factor", 1.0))),
        kv(p + "vocab_size", GT_U32, V),
        kv("tokenizer.ggml.model", GT_STR, "gpt2"),
        kv("tokenizer.ggml.tokens", GT_ARR, (GT_STR, [f"<t{i}>" for i in range(V)])),
        kv("tokenizer.ggml.token_type", GT_ARR, (GT_I32, [1] * V)),
    ]
    rp = cfg.rope_parameters or {}
    theta = getattr(cfg, "rope_theta", None) or rp.get("full_attention", rp).get("rope_theta", 10000.0)
    kvs.append(kv(p + "rope.freq_base", GT_F32, float(theta)))
    if arch == "gemma2":
        kvs += [kv(p + "attn_logit_softcapping", GT_F32, cfg.attn_logit_softcapping),
                kv(p + "final_logit_softcapping", GT_F32, cfg.final_logit_softcapping),
                kv(p + "attention.sliding_window", GT_U32, cfg.sliding_window)]
    if arch == "gemma3":
        kvs.append(kv(p + "attention.sliding_window", GT_U32, cfg.sliding_window))
        full = rp["full_attention"]
        if full.get("rope_type") == "linear":
            kvs += [kv(p + "rope.scaling.type", GT_STR, "linear"), kv(p + "rope.scaling.factor", GT_F32, full["factor"])]
    if arch == "phi3":
        orig = cfg.original_max_position_embeddings
        scale = cfg.max_position_embeddings / orig
        kvs += [kv(p + "attention.sliding_window", GT_U32, cfg.sliding_window or 0),
                kv(p + "rope.scaling.original_context_length", GT_U32, orig),
                kv(p + "rope.scaling.attn_factor", GT_F32, math.sqrt(1 + math.log(scale) / math.log(orig)))]
        for which in ("long", "short"):
            f = np.asarray(rp[f"{which}_factor"], dtype=np.float32)
            tensors.append((f"rope_factors_{which}.weight", [len(f)], F32, f.astype("<f4").tobytes()))
    ne = getattr(cfg, "num_experts", None) or getattr(cfg, "num_local_experts", None)
    if ne:
        kvs += [kv(p + "expert_count", GT_U32, ne), kv(p + "expert_used_count", GT_U32, cfg.num_experts_per_tok)]
        if arch == "qwen3moe":
            kvs.append(kv(p + "expert_feed_forward_length", GT_U32, cfg.moe_intermediate_size))
    if qwen35:
        kvs += [kv(p + "rope.dimension_sections", GT_ARR, (GT_I32, [11, 11, 10, 0])),
                kv(p + "ssm.conv_kernel", GT_U32, cfg.linear_conv_kernel_dim),
                kv(p + "ssm.state_size", GT_U32, dk),
                kv(p + "ssm.group_count", GT_U32, nk),
                kv(p + "ssm.time_step_rank", GT_U32, nv),
                kv(p + "ssm.inner_size", GT_U32, nv * dv),
                kv(p + "full_attention_interval", GT_U32, cfg.layer_types.index("full_attention") + 1)]
    return model, cfg, kvs, tensors, types, seed


def logits_of(model, ids):
    with torch.no_grad():
        return model(input_ids=torch.tensor([ids])).logits[0].double().tolist()


def write_fixture(name):
    model, cfg, kvs, tensors, types, seed = build(name)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    write_gguf(os.path.join(d, "model.gguf"), kvs, tensors)
    with open(os.path.join(d, "hf_config.json"), "w") as f:
        c = cfg.to_diff_dict()
        c["architectures"] = cfg.architectures
        json.dump(c, f, indent=2, sort_keys=True)
        f.write("\n")
    ids = [(seed * 7 + 13 * i + i * i) % V for i in range(T)]
    meta = {"tokens": ids, "logits_full": logits_of(model, ids), "types": types, "transformers": transformers.__version__,
            "seed": seed, "weights": "dequantised from the GGUF blocks (numpy), float32"}
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    return os.path.getsize(os.path.join(d, "model.gguf")), sorted(set(types.values()))


def write_audit(out_root, name):
    import gen_quant_fixtures as gq
    model, cfg, kvs, tensors, types, seed = build(name)
    d = os.path.join(out_root, name)
    os.makedirs(d, exist_ok=True)
    write_gguf(os.path.join(d, "model.gguf"), kvs, tensors)
    fd = os.path.join(out_root, name + "-float")
    gq.save_float(model, fd)
    recs = gq.write_refs(model, d, V, seed)
    for fn in ("hf.json", "hf-logits.f32", "calib.json"):
        with open(os.path.join(d, fn), "rb") as a, open(os.path.join(fd, fn), "wb") as b:
            b.write(a.read())
    return recs


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    if len(sys.argv) < 2 or sys.argv[1] not in ("fixtures", "audit"):
        sys.exit(__doc__)
    mode, rest = sys.argv[1], sys.argv[2:]
    out = None
    if mode == "audit":
        out, rest = rest[0], rest[1:]
    bad = 0
    for n in rest or list(CONFIGS):
        try:
            if mode == "fixtures":
                sz, tys = write_fixture(n)
                print(f"ok   {n:22s} {sz} bytes {tys}", flush=True)
            else:
                recs = write_audit(out, n)
                print(f"ok   {n:22s} {[r['generated'][:6] for r in recs]}", flush=True)
        except Exception as e:  # report and continue
            bad += 1
            import traceback
            traceback.print_exc()
            print(f"FAIL {n:22s} {type(e).__name__}: {str(e)[:300]}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
