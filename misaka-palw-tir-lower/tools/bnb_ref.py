"""bitsandbytes' checkpoint format and its dequantisation, written out here in numpy / torch.

bitsandbytes itself is NOT installed in the environment this was written in (no network), so nothing here is the
library's code: it is a re-implementation of what the library stores and computes, from its documented behaviour
and source (`bitsandbytes/functional.py`: `quantize_4bit`, `dequantize_4bit`, `QuantState.as_dict`,
`create_dynamic_map`, `int8_vectorwise_dequant`; `csrc/kernels.cu`: `kDequantizeBlockwise`). The descriptors
(`quant-formats/bnb_*.json`) are tested against THIS code, which shares nothing with their expression language.

What is stored (safetensors, `Linear4bit` / `Linear8bitLt` `state_dict`):

  4-bit (`nf4`, `fp4`), per quantised module `<m>`:
    <m>.weight                               uint8 [ceil(N / 2), 1]   N = out * in; the flattened row-major weight, two codes a byte,
                                                                       the FIRST (even index) code in the HIGH nibble
    <m>.weight.absmax                        float32 [ceil(N / blocksize)]       the absolute maximum of each block of the flat weight
                                             (double quantisation: uint8, a code of `nested_quant_map` scaled by `nested_absmax`)
    <m>.weight.quant_map                     float32 [16]            the 16-entry code table
    <m>.weight.quant_state.bitsandbytes__nf4 uint8 [n]               a JSON document: quant_type, blocksize, dtype, shape,
                                                                     (double quantisation) nested_blocksize, nested_dtype, nested_offset
    <m>.weight.nested_absmax                 float32 [ceil(blocks / 256)]    double quantisation only
    <m>.weight.nested_quant_map              float32 [256]                    double quantisation only

  Dequantisation (the CUDA kernel's arithmetic, what `dequantize_4bit` runs on a GPU):
    absmax' = absmax                                                          (float)
            = nested_quant_map[absmax] * nested_absmax[block / 256] + nested_offset   (two float32 operations; double quantisation)
    W[k] = round_to_dtype( float32( quant_map[code(k)] * absmax'[k / blocksize] ) )
  — the product is rounded to float32 and then ONCE to the module's own dtype (bfloat16, float16 or float32: the document's `dtype`).
  (bitsandbytes' pure-PyTorch fallback for devices without the kernel rounds the code table to that dtype FIRST, which differs by up to
  one unit in the last place of bfloat16 in many elements; this follows the kernel.)

  8-bit (LLM.int8), per quantised module:
    <m>.weight         int8 [out, in]     <m>.SCB  float32 [out]  the absolute maximum of each row of the original weight
    <m>.weight_format  uint8 scalar       0 = row-major (anything else is a hardware-reordered layout)
    W[o, i] = float32(float32(weight[o, i] * SCB[o]) * float32(1 / 127))       (`int8_vectorwise_dequant`)
"""

import json

import numpy as np
import torch

# bitsandbytes.functional.get_4bit_type("nf4"): the NormalFloat-4 code (a float32 table).
NF4 = np.array(
    [-1.0, -0.6961928009986877, -0.5250730514526367, -0.39491748809814453, -0.28444138169288635, -0.18477343022823334,
     -0.09105003625154495, 0.0, 0.07958029955625534, 0.16093020141124725, 0.24611230194568634, 0.33791524171829224,
     0.44070982933044434, 0.5626170039176941, 0.7229568362236023, 1.0], dtype=np.float32)

INV127 = np.float32(7.874015718698502e-3)   # float32(1 / 127), as `int8_vectorwise_dequant` multiplies by it


def fp4_map():
    """get_4bit_type("fp4"): the E2M1-like values 0, 0.0625, 8, 12, 4, 6, 2, 3 (and their negatives) divided by 12 in float32."""
    d = np.array([0, 0.0625, 8, 12, 4, 6, 2, 3, 0, -0.0625, -8, -12, -4, -6, -2, -3], dtype=np.float32)
    return (d / np.float32(12)).astype(np.float32)


def dynamic_map(signed=True, max_exponent_bits=7, total_bits=8):
    """bitsandbytes.functional.create_dynamic_map: the 256-entry code the (nested) absmax is quantised with."""
    data = []
    non_sign_bits = total_bits - 1
    additional_items = 2 ** (non_sign_bits - max_exponent_bits) - 1
    for i in range(max_exponent_bits):
        fraction_items = int(2 ** (i + non_sign_bits - max_exponent_bits) + 1)
        boundaries = torch.linspace(0.1, 1, fraction_items)
        means = (boundaries[:-1] + boundaries[1:]) / 2.0
        data += ((10 ** (-(max_exponent_bits - 1) + i)) * means).tolist()
        if signed:
            data += (-(10 ** (-(max_exponent_bits - 1) + i)) * means).tolist()
    assert additional_items == 0
    data.append(0)
    data.append(1.0)
    assert len(data) == 2 ** total_bits
    data.sort()
    return torch.tensor(data).numpy().astype(np.float32)


def round_dtype(v32, dtype):
    """float32 values rounded (to nearest even) to the module dtype and back."""
    if dtype == "float32":
        return np.ascontiguousarray(v32, dtype=np.float32)
    if dtype == "bfloat16":
        return torch.from_numpy(np.ascontiguousarray(v32, dtype=np.float32)).to(torch.bfloat16).to(torch.float32).numpy()
    if dtype == "float16":
        return v32.astype(np.float16).astype(np.float32)
    raise ValueError(dtype)


def _nearest(y, code):
    return np.abs(y[..., None] - code).argmin(-1)


def quantize_4bit(w, kind, blocksize, double_quant, dtype):
    """-> tensors (name suffix -> array, relative to `<m>`), exactly what a `Linear4bit` module saves."""
    w = np.ascontiguousarray(w, dtype=np.float32)
    out, inp = w.shape
    flat = w.reshape(-1)
    n = flat.size
    nb = -(-n // blocksize)
    x = np.concatenate([flat, np.zeros(nb * blocksize - n, np.float32)]).reshape(nb, blocksize)
    absmax = np.abs(x).max(1).astype(np.float32)
    code = NF4 if kind == "nf4" else fp4_map()
    y = x / np.where(absmax > 0, absmax, np.float32(1))[:, None]
    idx = _nearest(y, code).reshape(-1)[:n]
    if n % 2:
        idx = np.append(idx, 0)
    packed = ((idx[0::2] << 4) | idx[1::2]).astype(np.uint8).reshape(-1, 1)
    state = {"quant_type": kind, "blocksize": blocksize, "dtype": dtype, "shape": [out, inp]}
    t = {".weight": packed, ".weight.quant_map": code.copy()}
    if double_quant:
        offset = np.float32(absmax.mean())
        a = (absmax - offset).astype(np.float32)
        nb2 = -(-a.size // 256)
        a2 = np.concatenate([a, np.zeros(nb2 * 256 - a.size, np.float32)]).reshape(nb2, 256)
        nabs = np.abs(a2).max(1).astype(np.float32)
        nmap = dynamic_map()
        y2 = a2 / np.where(nabs > 0, nabs, np.float32(1))[:, None]
        q = _nearest(y2, nmap).reshape(-1)[: a.size].astype(np.uint8)
        t[".weight.absmax"] = q
        t[".weight.nested_absmax"] = nabs
        t[".weight.nested_quant_map"] = nmap
        state.update({"nested_blocksize": 256, "nested_dtype": "float32", "nested_offset": float(offset)})
    else:
        t[".weight.absmax"] = absmax
    # `pack_dict_to_tensor`: json.dumps(...).encode("utf-8") as a uint8 tensor
    t[f".weight.quant_state.bitsandbytes__{kind}"] = np.frombuffer(json.dumps(state).encode("utf-8"), dtype=np.uint8).copy()
    return t


def dequantize_4bit(t, dtype_of_doc=True):
    """The module's float32 [out, in] weight from its stored tensors (suffix -> array), by the kernel's arithmetic above."""
    kind = "nf4" if ".weight.quant_state.bitsandbytes__nf4" in t else "fp4"
    state = json.loads(bytes(t[f".weight.quant_state.bitsandbytes__{kind}"]).decode("utf-8"))
    out, inp = state["shape"]
    n = out * inp
    flat = t[".weight"].reshape(-1)
    codes = np.empty(flat.size * 2, np.int64)
    codes[0::2] = flat >> 4
    codes[1::2] = flat & 15
    codes = codes[:n]
    if ".weight.nested_absmax" in t:
        nbs = state["nested_blocksize"]
        a = (t[".weight.nested_quant_map"].astype(np.float32)[t[".weight.absmax"].astype(np.int64)]
             * np.repeat(t[".weight.nested_absmax"].astype(np.float32), nbs)[: t[".weight.absmax"].size]).astype(np.float32)
        a = (a + np.float32(state["nested_offset"])).astype(np.float32)
    else:
        a = t[".weight.absmax"].astype(np.float32)
    blk = np.arange(n) // state["blocksize"]
    v32 = (t[".weight.quant_map"].astype(np.float32)[codes] * a[blk]).astype(np.float32)
    return round_dtype(v32, state["dtype"]).reshape(out, inp)


def quantize_int8(w):
    """Row-wise absmax int8: SCB[o] = max |w[o, :]|, CB = round(w * 127 / SCB). -> tensors (suffix -> array)."""
    w32 = np.ascontiguousarray(w, dtype=np.float32)
    scb = np.abs(w32).max(1).astype(np.float32)
    cb = np.round(w32 * np.float32(127) / np.where(scb > 0, scb, np.float32(1))[:, None]).astype(np.int8)
    return {".weight": cb, ".SCB": scb, ".weight_format": np.array(0, dtype=np.uint8)}


def dequantize_int8(t):
    """What the descriptor declares: the stored integers times the row scale SCB / 127 (float32). `bnb_dequant_int8` is the library's
    own expression, which rounds one operation more (they agree to one unit in the last place)."""
    scale = (t[".SCB"].astype(np.float32) * INV127).astype(np.float32)
    return (t[".weight"].astype(np.float32) * scale[:, None]).astype(np.float32)


def bnb_dequant_int8(t):
    """`int8_vectorwise_dequant`: A * stats.view(-1, 1) * 7.874015718698502e-3 (torch, float32)."""
    a = torch.from_numpy(t[".weight"].astype(np.int8))
    return (a * torch.from_numpy(t[".SCB"]).view(-1, 1) * 7.874015718698502e-3).numpy()
