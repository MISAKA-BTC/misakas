#!/usr/bin/env python3
"""Storage formats the pack does not know, written as THIRD-PARTY descriptors (misaka.palw.quant-format.v1), with test vectors.

This is lane H's second axis: models are added by DATA (adapters) and so are the formats their weights are stored in. The built-in pack
(`quant-formats/`) holds the ggml types, GPTQ, AWQ, FP8 block scales, compressed-tensors, MXFP4 and NVFP4. These are the ones it lacks:

  mlx_affine.json  MLX affine group quantisation (mlx.core.quantize): `weight` uint32 [out, in*bits/32] with 32/bits codes per word
                   (the first element in the LOWEST bits), `scales` and `biases` [out, in/group_size]; W = scale*q + bias.
  bnb_int8.json    bitsandbytes LLM.int8() checkpoints: `weight` int8 [out, in], `SCB` float32 [out]; W = weight * SCB / 127.

HONESTY. No reference library (mlx, bitsandbytes) is available offline, so the vectors here are NOT from the library's own dequantiser:
they come from a numpy transcription of the documented layout (random valid codes, scales and biases exactly representable in binary16, the
weight computed in float64 and rounded to float32). A descriptor that passes them proves the descriptor says what this script says, not that
the script says what the library does. The layouts are transcribed from the libraries' documentation and source as recalled; each is marked.

Not written, and why (see docs/design/palw/tir/corpus-v2.md §8): bitsandbytes NF4/FP4 (the weight's [out, in] shape lives only in a JSON blob
stored as a uint8 tensor, `weight.quant_state.bitsandbytes__nf4`, and the descriptor language can read a shape only from the role tensors),
HQQ (the same, in a pickled `meta`), EXL2/EXL3, AQLM, Quanto, torchao (tensor subclasses with metadata).

    python3 gen_formats.py        # numpy only; writes mlx_affine.json and bnb_int8.json next to this file
"""
import json
import os

import numpy as np

SCHEMA = "misaka.palw.quant-format.v1"
HERE = os.path.dirname(os.path.abspath(__file__))
rng = np.random.default_rng(20261001)


def hexof(a, dt):
    return np.ascontiguousarray(a.astype(dt)).tobytes().hex()


def role(a, dtype, np_dtype):
    return {"dtype": dtype, "shape": list(a.shape), "hex": hexof(a, np_dtype)}


# ───────────────────────────── MLX affine ─────────────────────────────
def mlx_case(out, inp, bits, gs):
    pack = 32 // bits
    q = rng.integers(0, 2 ** bits, size=(out, inp)).astype(np.uint64)
    ng = inp // gs
    scales = (rng.integers(1, 48, size=(out, ng)) / 64.0).astype(np.float16)
    biases = (rng.integers(-64, 64, size=(out, ng)) / 64.0).astype(np.float16)
    words = np.zeros((out, inp // pack), dtype=np.uint64)
    for k in range(pack):
        words |= q[:, k::pack] << np.uint64(bits * k)
    g = np.arange(inp) // gs
    w = scales.astype(np.float64)[:, g] * q.astype(np.float64) + biases.astype(np.float64)[:, g]
    return {
        "config": {"bits": bits, "group_size": gs, "quant_method": "mlx"},
        "roles": {
            "weight": role(words, "U32", "<u4"),
            "scales": role(scales, "F16", "<f2"),
            "biases": role(biases, "F16", "<f2"),
        },
        "values_f32_hex": hexof(w, "<f4"),
    }


mlx = {
    "schema": SCHEMA,
    "name": "MLX_AFFINE",
    "doc": "MLX affine group quantisation (mlx.core.quantize, mlx-lm checkpoints): `weight` is uint32 [out, in*bits/32] holding 32/bits codes per word, the FIRST element in the lowest bits; `scales` and `biases` are [out, in/group_size]; W = scale*q + bias. TRANSCRIBED from MLX's documentation and source as recalled; the vectors are a numpy transcription, not MLX's own dequantiser (not available offline). A checkpoint announces itself by `quantization` / `quantization_config` {group_size, bits} WITHOUT a quant_method: the registry reads only `quant_method`, so the converter must be told `quant_method: mlx` (FR-33).",
    "ids": [{"scheme": "config", "method": "mlx"}],
    "layout": {
        "kind": "tensors",
        "roles": [
            {"name": "weight", "suffix": ".weight", "dtypes": ["U32"], "rank": 2},
            {"name": "scales", "suffix": ".scales", "dtypes": ["F16", "BF16", "F32"], "rank": 2},
            {"name": "biases", "suffix": ".biases", "dtypes": ["F16", "BF16", "F32"], "rank": 2},
        ],
        "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1] * (32 / bits)"},
        "checks": [
            {"expr": "inp % gs == 0", "message": "the input width is not a multiple of the group size"},
            {"expr": "dim_scales[0] == out && dim_scales[1] == ng", "message": "the scales are not [out, groups]"},
            {"expr": "dim_biases[0] == out && dim_biases[1] == ng", "message": "the biases are not [out, groups]"},
        ],
    },
    "config": {
        "inert": ["mode"],
        "checks": [{"path": "mode", "one_of": [None, "affine"], "message": "only MLX's affine mode is described (mxfp4/nvfp4 modes are other formats)"}],
    },
    "params": {
        "bits": {"config": "bits", "allowed": [2, 4, 8], "default": 4},
        "group_size": {"config": "group_size", "default": 64},
    },
    "decode": {
        "target": "integers",
        "group": {"size": "group_size"},
        "q": "(weight[o, i / (32 / bits)] >> (bits * (i % (32 / bits)))) & ((1 << bits) - 1)",
        "scale": "scales[o, g]",
        "zero": "0",
        "min": "-biases[o, g]",
        "code": {"min": 0, "max": "(1 << bits) - 1"},
    },
    "tests": [mlx_case(4, 128, 4, 64), mlx_case(3, 64, 8, 32), mlx_case(2, 64, 2, 32)],
}


# ───────────────────────────── bitsandbytes LLM.int8() ─────────────────────────────
def bnb_case(out, inp):
    q = rng.integers(-127, 128, size=(out, inp)).astype(np.int8)
    scb = (rng.integers(1, 200, size=(out,)) / 16.0).astype(np.float32)
    w = q.astype(np.float64) * (scb.astype(np.float64) / 127.0)[:, None]
    return {
        "config": {"quant_method": "bitsandbytes", "load_in_8bit": True, "load_in_4bit": False, "llm_int8_threshold": 6.0},
        "roles": {"weight": role(q, "I8", "<i1"), "SCB": role(scb, "F32", "<f4")},
        "values_f32_hex": hexof(w, "<f4"),
    }


bnb = {
    "schema": SCHEMA,
    "name": "BNB_INT8",
    "doc": "bitsandbytes LLM.int8() checkpoints (load_in_8bit): `weight` int8 [out, in] and `SCB` float32 [out], one absmax scale per output row; W = weight * SCB / 127. The run-time mixed-precision decomposition (llm_int8_threshold: activation outlier columns in fp16) is a kernel behaviour, not a storage fact, and is declared inert. 4-bit checkpoints (nf4/fp4, double quantisation) announce the same quant_method and are REFUSED here by name: their weights are a flat [N/2, 1] uint8 tensor whose [out, in] shape is stored only in a JSON blob (`weight.quant_state.bitsandbytes__nf4`), which a descriptor cannot read (FR-33). TRANSCRIBED from the bitsandbytes documentation as recalled; the vectors are a numpy transcription (bitsandbytes is not available offline).",
    "ids": [{"scheme": "config", "method": "bitsandbytes"}],
    "layout": {
        "kind": "tensors",
        "roles": [
            {"name": "weight", "suffix": ".weight", "dtypes": ["I8"], "rank": 2},
            {"name": "SCB", "suffix": ".SCB", "dtypes": ["F32"], "rank": 1},
        ],
        "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1]"},
        "checks": [{"expr": "dim_SCB[0] == out", "message": "SCB is not one scale per output row"}],
    },
    "config": {
        "inert": [
            "llm_int8_threshold", "llm_int8_has_fp16_weight", "llm_int8_enable_fp32_cpu_offload", "bnb_4bit_compute_dtype", "bnb_4bit_quant_type",
            "bnb_4bit_use_double_quant", "bnb_4bit_quant_storage", "_load_in_8bit", "_load_in_4bit", "llm_int8_skip_modules",
        ],
        "skip": "llm_int8_skip_modules",
        "skip_match": "exact",
        "checks": [
            {"path": "load_in_8bit", "one_of": [True], "message": "this descriptor is bitsandbytes 8-bit; 4-bit (nf4/fp4) stores a flat packed tensor whose shape is in a JSON blob (FR-33)"},
            {"path": "load_in_4bit", "one_of": [None, False], "message": "bitsandbytes 4-bit is not described (FR-33)"},
        ],
    },
    "params": {},
    "decode": {
        "target": "integers",
        "group": {"size": "inp"},
        "q": "weight[o, i]",
        "scale": "SCB[o] / 127",
        "zero": "0",
        "code": {"min": -128, "max": 127},
    },
    "tests": [bnb_case(3, 32), bnb_case(2, 64)],
}

for name, d in (("mlx_affine", mlx),):  # bnb_int8 was promoted to the built-in pack
    with open(os.path.join(HERE, name + ".json"), "w") as f:
        json.dump(d, f, indent=1, sort_keys=True)
        f.write("\n")
    print("wrote", name + ".json", f"({len(d['tests'])} test vectors)")
