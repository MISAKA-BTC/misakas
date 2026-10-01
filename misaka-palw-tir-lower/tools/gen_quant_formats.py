#!/usr/bin/env python3
"""Writes the built-in quant-format descriptors (quant-formats/*.json, misaka.palw.quant-format.v1) and
their test vectors.

The descriptors are DATA — the expressions below are the format definitions, transcribed from ggml's
block layouts and `dequantize_row_*` (ggml/src/ggml-common.h, ggml-quants.c; MIT). The grid and value
tables are parsed out of ggml-common.h, not typed. The TEST VECTORS are not from this script's
definitions: they are decoded by gguf-py's numpy dequantisers (`gguf.quants.dequantize`, the project's
own independent implementation) on random valid blocks, and for the two types gguf-py does not have
(Q1_0, Q2_0) by a numpy transcription of the C. The Rust interpreter must reproduce every value bit
for bit (`QuantFormat::from_json` refuses a descriptor that does not).

Usage: LLAMA_CPP=/path/to/llama.cpp python tools/gen_quant_formats.py   (offline; needs numpy only)
"""
import json, os, re, struct, sys
import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bnb_ref

LLAMA = os.environ.get("LLAMA_CPP", os.path.expanduser("~/Downloads/misaka-palw-runtime/llama.cpp"))
sys.path.insert(0, os.path.join(LLAMA, "gguf-py"))
import gguf
from gguf import quants as gq, GGMLQuantizationType as T

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "quant-formats")
SCHEMA = "misaka.palw.quant-format.v1"

# ───────────────────────────── tables out of ggml-common.h ─────────────────────────────
hdr = open(os.path.join(LLAMA, "ggml", "src", "ggml-common.h")).read()

def c_table(name):
    m = re.search(r"GGML_TABLE_BEGIN\((\w+), %s, \w+\)(.*?)GGML_TABLE_END" % re.escape(name), hdr, re.S)
    ty, body = m.group(1), m.group(2)
    body = re.sub(r"//.*", "", body)
    vals = [int(x, 0) for x in re.findall(r"-?0x[0-9a-fA-F]+|-?\d+", body)]
    bits = {"uint8_t": 8, "int8_t": 8, "uint32_t": 32, "uint64_t": 64}[ty]
    return ty, bits, vals

def table_json(name):
    ty, bits, vals = c_table(name)
    signed = ty.startswith("int")
    w = bits // 8
    hexs = b"".join((v & ((1 << bits) - 1)).to_bytes(w, "little") for v in vals).hex()
    return {"bits": bits, "signed": signed, "hex": hexs}

TABLES = {n: table_json(n) for n in
          ["iq2xxs_grid", "iq2xs_grid", "iq2s_grid", "iq3xxs_grid", "iq3s_grid", "iq1s_grid", "ksigns_iq2xs", "kvalues_iq4nl", "kvalues_fp4"]}
# iq1s_grid's eight int8 lanes make a u64 with the top bit set: read it as the same 64 bits, signed.
TABLES["iq1s_grid"]["signed"] = True
for n, expect in [("iq2xxs_grid", 256), ("iq2xs_grid", 512), ("iq2s_grid", 1024), ("iq3xxs_grid", 256), ("iq3s_grid", 512),
                  ("iq1s_grid", 2048), ("ksigns_iq2xs", 128), ("kvalues_iq4nl", 16), ("kvalues_fp4", 16)]:
    got = len(TABLES[n]["hex"]) // 2 // (TABLES[n]["bits"] // 8)
    assert got == expect, (n, got, expect)

def fld(name, at, ty, count=None):
    d = {"name": name, "at": at, "type": ty}
    if count is not None:
        d["count"] = count
    return d

def integers(name, ids, elems, nbytes, fields, group, q, scale, code, zero=None, mn=None, tables=None, doc=""):
    dec = {"target": "integers", "group": {"size": group}, "q": q, "scale": scale, "code": {"min": code[0], "max": code[1]}}
    if zero is not None:
        dec["zero"] = zero
    if mn is not None:
        dec["min"] = mn
    d = {"schema": SCHEMA, "name": name, "doc": doc, "ids": [{"scheme": "ggml", "id": i} for i in ids],
         "layout": {"kind": "blocks", "elems": elems, "bytes": nbytes, "fields": fields}, "decode": dec}
    if tables:
        d["tables"] = {t: TABLES[t] for t in tables}
    return d

def floats(name, ids, nbytes, ty, doc=""):
    return {"schema": SCHEMA, "name": name, "doc": doc, "ids": [{"scheme": "ggml", "id": i} for i in ids],
            "layout": {"kind": "blocks", "elems": 1, "bytes": nbytes, "fields": [fld("v", 0, ty)]},
            "decode": {"target": "floats", "value": "v"}}

K4_SC = lambda: "j < 4 ? (sc[j] & 63) : ((sc[j + 4] & 15) | ((sc[j - 4] >> 6) << 4))"
K4_MN = lambda: "j < 4 ? (sc[j + 4] & 63) : ((sc[j + 4] >> 4) | ((sc[j] >> 6) << 4))"
Q3_SC6 = "(((scales[4 * ((j / 4) % 2) + j % 4] >> (4 * (j / 8))) & 15) | (((scales[8 + j % 4] >> (2 * (j / 4))) & 3) << 4))"
pow3 = {"bits": 8, "signed": False, "hex": bytes([1, 3, 9, 27, 81, 243]).hex()}

F = {}
F["f32"] = floats("F32", [0], 4, "f32", "IEEE binary32.")
F["f16"] = floats("F16", [1], 2, "f16", "IEEE binary16.")
F["bf16"] = floats("BF16", [30], 2, "bf16", "bfloat16.")
F["f64"] = floats("F64", [28], 8, "f64", "IEEE binary64.")
F["q4_0"] = integers("Q4_0", [2], 32, 18, [fld("d", 0, "f16"), fld("qs", 2, "u8", 16)], 32,
                     "(qs[e % 16] >> (4 * (e / 16))) & 15", "d", (0, 15), zero="8", doc="ggml block_q4_0: W = d·(q − 8).")
F["q4_1"] = integers("Q4_1", [3], 32, 20, [fld("d", 0, "f16"), fld("m", 2, "f16"), fld("qs", 4, "u8", 16)], 32,
                     "(qs[e % 16] >> (4 * (e / 16))) & 15", "d", (0, 15), mn="-m", doc="ggml block_q4_1: W = d·q + m.")
F["q5_0"] = integers("Q5_0", [6], 32, 22, [fld("d", 0, "f16"), fld("qh", 2, "u32"), fld("qs", 6, "u8", 16)], 32,
                     "((qs[e % 16] >> (4 * (e / 16))) & 15) | (((qh >> e) & 1) << 4)", "d", (0, 31), zero="16",
                     doc="ggml block_q5_0: W = d·(q − 16), the fifth bit from qh.")
F["q5_1"] = integers("Q5_1", [7], 32, 24, [fld("d", 0, "f16"), fld("m", 2, "f16"), fld("qh", 4, "u32"), fld("qs", 8, "u8", 16)], 32,
                     "((qs[e % 16] >> (4 * (e / 16))) & 15) | (((qh >> e) & 1) << 4)", "d", (0, 31), mn="-m", doc="ggml block_q5_1: W = d·q + m.")
F["q8_0"] = integers("Q8_0", [8], 32, 34, [fld("d", 0, "f16"), fld("qs", 2, "i8", 32)], 32, "qs[e]", "d", (-128, 127), doc="ggml block_q8_0: W = d·q.")
F["q2_k"] = integers("Q2_K", [10], 256, 84, [fld("scales", 0, "u8", 16), fld("qs", 16, "u8", 64), fld("d", 80, "f16"), fld("dmin", 82, "f16")], 16,
                     "(qs[32 * (e / 128) + e % 32] >> (2 * ((e / 32) % 4))) & 3", "d * (scales[j] & 15)", (0, 3), mn="dmin * (scales[j] >> 4)",
                     doc="ggml block_q2_K: 16 groups of 16, 4-bit scale and 4-bit min each under d and dmin.")
F["q3_k"] = integers("Q3_K", [11], 256, 110, [fld("hmask", 0, "u8", 32), fld("qs", 32, "u8", 64), fld("scales", 96, "u8", 12), fld("d", 108, "f16")], 16,
                     "((qs[32 * (e / 128) + e % 32] >> (2 * ((e / 32) % 4))) & 3) | (((hmask[e % 32] >> (e / 32)) & 1) << 2)",
                     "d * (%s - 32)" % Q3_SC6, (0, 7), zero="4", doc="ggml block_q3_K: 3-bit codes (2 low bits + a mask bit), 6-bit signed scales packed in 12 bytes.")
F["q4_k"] = integers("Q4_K", [12], 256, 144, [fld("d", 0, "f16"), fld("dmin", 2, "f16"), fld("sc", 4, "u8", 12), fld("qs", 16, "u8", 128)], 32,
                     "(qs[32 * (e / 64) + e % 32] >> (4 * ((e / 32) % 2))) & 15", "d * (%s)" % K4_SC(), (0, 15), mn="dmin * (%s)" % K4_MN(),
                     doc="ggml block_q4_K: 8 groups of 32, 6-bit scale and min (get_scale_min_k4) under d and dmin.")
F["q5_k"] = integers("Q5_K", [13], 256, 176, [fld("d", 0, "f16"), fld("dmin", 2, "f16"), fld("sc", 4, "u8", 12), fld("qh", 16, "u8", 32), fld("qs", 48, "u8", 128)], 32,
                     "((qs[32 * (e / 64) + e % 32] >> (4 * ((e / 32) % 2))) & 15) | (((qh[e % 32] >> (e / 32)) & 1) << 4)",
                     "d * (%s)" % K4_SC(), (0, 31), mn="dmin * (%s)" % K4_MN(), doc="ggml block_q5_K: as Q4_K with a fifth bit from qh.")
F["q6_k"] = integers("Q6_K", [14], 256, 210, [fld("ql", 0, "u8", 128), fld("qh", 128, "u8", 64), fld("scales", 192, "i8", 16), fld("d", 208, "f16")], 16,
                     "((ql[64 * (e / 128) + e % 32 + 32 * (((e / 32) % 4) % 2)] >> (4 * (((e / 32) % 4) / 2))) & 15) | (((qh[32 * (e / 128) + e % 32] >> (2 * ((e / 32) % 4))) & 3) << 4)",
                     "d * scales[j]", (0, 63), zero="32", doc="ggml block_q6_K: W = d·sc·(q − 32), 16 groups of 16 with signed 8-bit scales.")
F["iq4_nl"] = integers("IQ4_NL", [20], 32, 18, [fld("d", 0, "f16"), fld("qs", 2, "u8", 16)], 32,
                       "kvalues_iq4nl[(qs[e % 16] >> (4 * (e / 16))) & 15]", "d", (-127, 113), tables=["kvalues_iq4nl"],
                       doc="ggml block_iq4_nl: a 4-bit index into a non-uniform 16-entry table, W = d·table[i].")
F["iq4_xs"] = integers("IQ4_XS", [23], 256, 136, [fld("d", 0, "f16"), fld("scales_h", 2, "u16"), fld("scales_l", 4, "u8", 4), fld("qs", 8, "u8", 128)], 32,
                       "kvalues_iq4nl[(qs[16 * (e / 32) + e % 16] >> (4 * ((e / 16) % 2))) & 15]",
                       "d * ((((scales_l[j / 2] >> (4 * (j % 2))) & 15) | (((scales_h >> (2 * j)) & 3) << 4)) - 32)", (-127, 113), tables=["kvalues_iq4nl"],
                       doc="ggml block_iq4_xs: IQ4_NL codes with 6-bit signed per-32 scales.")
TQ1 = lambda b, n: "(((%s * pow3[%s]) & 255) * 3) >> 8" % (b, n)
F["tq1_0"] = integers("TQ1_0", [34], 256, 54, [fld("qs", 0, "u8", 48), fld("qh", 48, "u8", 4), fld("d", 52, "f16")], 256,
                      "e < 160 ? %s : (e < 240 ? %s : %s)" % (TQ1("qs[e % 32]", "e / 32"), TQ1("qs[32 + (e - 160) % 16]", "(e - 160) / 16"), TQ1("qh[(e - 240) % 4]", "(e - 240) / 4")),
                      "d", (0, 2), zero="1", doc="ggml block_tq1_0: ternary digits, five per byte (base 3), W = d·(digit − 1).")
F["tq1_0"]["tables"] = {"pow3": pow3}
F["tq2_0"] = integers("TQ2_0", [35], 256, 66, [fld("qs", 0, "u8", 64), fld("d", 64, "f16")], 256,
                      "(qs[32 * (e / 128) + e % 32] >> (2 * ((e / 32) % 4))) & 3", "d", (0, 3), zero="1", doc="ggml block_tq2_0: 2-bit codes, W = d·(q − 1).")
F["mxfp4"] = integers("MXFP4", [39], 32, 17, [fld("se", 0, "u8"), fld("qs", 1, "u8", 16)], 32,
                      "kvalues_fp4[(qs[e % 16] >> (4 * (e / 16))) & 15]", "e8m0(se) * 0.5", (-12, 12), tables=["kvalues_fp4"],
                      doc="ggml block_mxfp4 (OCP MX FP4): E2M1 codes (doubled) under a power-of-two E8M0 scale; W = 2^(e−128)·table[i].")
F["nvfp4"] = integers("NVFP4", [40], 64, 36, [fld("d", 0, "u8", 4), fld("qs", 4, "u8", 32)], 16,
                      "kvalues_fp4[(qs[8 * (e / 16) + (e % 16) % 8] >> (4 * ((e % 16) / 8))) & 15]",
                      "(d[j] == 0 || d[j] == 127) ? 0.0 : 0.5 * ((((d[j] >> 3) & 15) == 0) ? (d[j] & 7) * pow2(-9) : (1.0 + (d[j] & 7) / 8.0) * pow2(((d[j] >> 3) & 15) - 7))",
                      (-12, 12), tables=["kvalues_fp4"], doc="ggml block_nvfp4: E2M1 codes (doubled) under a UE4M3 scale per 16.")
F["q1_0"] = integers("Q1_0", [41], 128, 18, [fld("d", 0, "f16"), fld("qs", 2, "u8", 16)], 128, "2 * ((qs[e / 8] >> (e % 8)) & 1) - 1", "d", (-1, 1),
                     doc="ggml block_q1_0: one sign bit per weight, W = ±d.")
F["q2_0"] = integers("Q2_0", [42], 64, 18, [fld("d", 0, "f16"), fld("qs", 2, "u8", 16)], 64, "(qs[e / 4] >> (2 * (e % 4))) & 3", "d", (0, 3), zero="1",
                     doc="ggml block_q2_0: 2-bit codes (00=-1, 01=0, 10=+1, 11=+2), W = d·(q − 1).")
IQ2_SIGN = "(1 - 2 * ((ksigns_iq2xs[%s] >> (e %% 8)) & 1))"
F["iq2_xxs"] = integers("IQ2_XXS", [16], 256, 66, [fld("d", 0, "f16"), fld("qb", 2, "u8", 64), fld("qw", 2, "u32", 16)], 32,
                        "((iq2xxs_grid[qb[8 * (e / 32) + (e % 32) / 8]] >> (8 * (e % 8))) & 255) * " + IQ2_SIGN % "(qw[2 * (e / 32) + 1] >> (7 * ((e % 32) / 8))) & 127",
                        "d * (0.5 + (qw[2 * j + 1] >> 28)) * 0.25", (-43, 43), tables=["iq2xxs_grid", "ksigns_iq2xs"],
                        doc="ggml block_iq2_xxs: 8-weight grid points (256-entry codebook) with 7-bit sign patterns, a 4-bit scale per 32.")
IQ2_NIB = "d * (0.5 + ((scales[j / 2] >> (4 * (j % 2))) & 15)) * 0.25"
F["iq2_xs"] = integers("IQ2_XS", [17], 256, 74, [fld("d", 0, "f16"), fld("qs", 2, "u16", 32), fld("scales", 66, "u8", 8)], 16,
                       "((iq2xs_grid[qs[4 * (e / 32) + (e % 32) / 8] & 511] >> (8 * (e % 8))) & 255) * " + IQ2_SIGN % "qs[4 * (e / 32) + (e % 32) / 8] >> 9",
                       IQ2_NIB, (-43, 43), tables=["iq2xs_grid", "ksigns_iq2xs"], doc="ggml block_iq2_xs: 9-bit grid index and 7-bit signs per 8 weights, a 4-bit scale per 16.")
F["iq2_s"] = integers("IQ2_S", [22], 256, 82, [fld("d", 0, "f16"), fld("qs", 2, "u8", 64), fld("qh", 66, "u8", 8), fld("scales", 74, "u8", 8)], 16,
                      "((iq2s_grid[qs[4 * (e / 32) + (e % 32) / 8] | ((qh[e / 32] << (8 - 2 * ((e % 32) / 8))) & 768)] >> (8 * (e % 8))) & 255) * (1 - 2 * ((qs[32 + 4 * (e / 32) + (e % 32) / 8] >> (e % 8)) & 1))",
                      IQ2_NIB, (-43, 43), tables=["iq2s_grid"], doc="ggml block_iq2_s: 10-bit grid index (qs + qh) and an explicit sign byte per 8 weights.")
IQ3_SIGN = "(1 - 2 * ((ksigns_iq2xs[(sw[e / 32] >> (7 * ((e % 32) / 8))) & 127] >> (e % 8)) & 1))"
IQ3_G = "((iq3xxs_grid[qs[8 * (e / 32) + 2 * ((e % 32) / 8) + (e % 8) / 4]] >> (8 * (e % 4))) & 255)"
F["iq3_xxs"] = integers("IQ3_XXS", [18], 256, 98, [fld("d", 0, "f16"), fld("qs", 2, "u8", 64), fld("sw", 66, "u32", 8)], 32,
                        IQ3_G + " * " + IQ3_SIGN, "d * (0.5 + (sw[j] >> 28)) * 0.5", (-62, 62), tables=["iq3xxs_grid", "ksigns_iq2xs"],
                        doc="ggml block_iq3_xxs: two 4-weight grid points per 8 weights, 7-bit signs and a 4-bit scale per 32 in a trailing word.")
IQ3S_G = "((iq3s_grid[qs[8 * (e / 32) + 2 * ((e % 32) / 8) + (e % 8) / 4] | ((qh[e / 32] << (8 - 2 * ((e % 32) / 8) - ((e % 8) / 4))) & 256)] >> (8 * (e % 4))) & 255)"
F["iq3_s"] = integers("IQ3_S", [21], 256, 110, [fld("d", 0, "f16"), fld("qs", 2, "u8", 64), fld("qh", 66, "u8", 8), fld("signs", 74, "u8", 32), fld("scales", 106, "u8", 4)], 32,
                      IQ3S_G + " * (1 - 2 * ((signs[4 * (e / 32) + (e % 32) / 8] >> (e % 8)) & 1))",
                      "d * (1 + 2 * ((scales[j / 2] >> (4 * (j % 2))) & 15))", (-15, 15), tables=["iq3s_grid"],
                      doc="ggml block_iq3_s: 9-bit grid indices (qs + qh), an explicit sign byte per 8 weights, a 4-bit scale per 32.")
F["iq1_s"] = integers("IQ1_S", [19], 256, 50, [fld("d", 0, "f16"), fld("qs", 2, "u8", 32), fld("qh", 34, "u16", 8)], 32,
                      "sext((iq1s_grid[qs[4 * (e / 32) + (e % 32) / 8] | (((qh[e / 32] >> (3 * ((e % 32) / 8))) & 7) << 8)] >> (8 * (e % 8))) & 255, 8)",
                      "d * (2 * ((qh[j] >> 12) & 7) + 1)", (-1, 1), tables=["iq1s_grid"],
                      mn="-(d * (2 * ((qh[j] >> 12) & 7) + 1)) * ((qh[j] & 32768) != 0 ? -0.125 : 0.125)",
                      doc="ggml block_iq1_s: ternary 8-weight grid points (2048-entry codebook), W = dl·(g + δ) with δ = ±1/8 per 32.")
IQ1M_D = "f16((sc[0] >> 12) | ((sc[1] >> 8) & 240) | ((sc[2] >> 4) & 3840) | (sc[3] & 61440))"
IQ1M_DL = "(" + IQ1M_D + " * (2 * ((sc[(j / 4) / 2] >> (6 * ((j / 4) % 2) + 3 * ((j % 4) / 2))) & 7) + 1))"
F["iq1_m"] = integers("IQ1_M", [29], 256, 56, [fld("qs", 0, "u8", 32), fld("qh", 32, "u8", 16), fld("sc", 48, "u16", 4)], 8,
                      "sext((iq1s_grid[qs[4 * (e / 32) + (e % 32) / 8] | ((qh[2 * (e / 32) + ((e % 32) / 8) / 2] << (8 - 4 * (((e % 32) / 8) % 2))) & 1792)] >> (8 * (e % 8))) & 255, 8)",
                      IQ1M_DL, (-1, 1), tables=["iq1s_grid"],
                      mn="-" + IQ1M_DL + " * ((qh[2 * (j / 4) + (j % 4) / 2] & (8 << (4 * ((j % 4) % 2)))) != 0 ? -0.125 : 0.125)",
                      doc="ggml block_iq1_m: IQ1_S codebook with scales packed in four u16 (the f16 d is spread over their top nibbles), delta per 8 weights.")

# ───────────────────────────── multi-tensor formats (safetensors) ─────────────────────────────

def role(name, suffix, dtypes, rank, required=True):
    d = {"name": name, "suffix": suffix, "dtypes": dtypes, "rank": rank}
    if not required:
        d["required"] = False
    return d

FLOATS = ["F16", "BF16", "F32"]
PACK = "(32 / bits)"
MASK = "((1 << bits) - 1)"
F["gptq"] = {
    "schema": SCHEMA, "name": "GPTQ", "doc": "AutoGPTQ / GPTQModel: qweight packs 32/bits codes per int32 along the input axis; qzeros pack along the output axis (v1 stores z − 1); scales per (group, output); g_idx (act-order) names each column's group.",
    "ids": [{"scheme": "config", "method": "gptq"}],
    "params": {"bits": {"config": "bits", "default": 4, "allowed": [2, 4, 8]}, "group_size": {"config": "group_size", "default": 128},
               "v2": {"config": "checkpoint_format", "default": 0, "map": {"gptq": 0, "gptq_v2": 1}},
               "sym": {"config": "sym", "default": 1}},
    "layout": {"kind": "tensors",
               "roles": [role("qweight", ".qweight", ["I32"], 2), role("qzeros", ".qzeros", ["I32"], 2), role("scales", ".scales", FLOATS, 2),
                         role("g_idx", ".g_idx", ["I32"], 1, required=False)],
               "dims": {"out": "dim_qweight[1]", "inp": "dim_qweight[0] * " + PACK},
               "checks": [
                   {"expr": "dim_scales[1] == out", "message": "the scales are not one per output channel"},
                   {"expr": "dim_scales[0] == ng", "message": "the number of scale groups is not the input width over the group size"},
                   {"expr": "dim_qzeros[0] == ng && dim_qzeros[1] * " + PACK + " == out", "message": "qzeros is not [groups, outputs / pack]"},
                   {"expr": "!has_g_idx || dim_g_idx[0] == inp", "message": "g_idx is not one entry per input"}]},
    "decode": {"target": "integers",
               "group": {"size": "group_size < 1 ? inp : group_size", "index": "has_g_idx ? g_idx[i] : i / gs"},
               "q": "(qweight[i / %s, o] >> (bits * (i %% %s))) & %s" % (PACK, PACK, MASK),
               "scale": "scales[g, o]",
               "zero": "(((qzeros[g, o / %s] >> (bits * (o %% %s))) & %s) + 1 - v2) & %s" % (PACK, PACK, MASK, MASK),
               "code": {"min": 0, "max": "(1 << bits) - 1"},
               # 8-bit asymmetric codes minus their zero point do not fit i8: the lowering carries a per-group offset term.
               "offset_term": "bits == 8 && !sym"},
}
F["awq"] = {
    "schema": SCHEMA, "name": "AWQ", "doc": "AutoAWQ GEMM: eight 4-bit codes per int32 along the OUTPUT axis in the order 0,2,4,6,1,3,5,7; qzeros the same; scales per (group, output).",
    "ids": [{"scheme": "config", "method": "awq"}],
    "params": {"bits": {"config": "bits", "default": 4, "allowed": [4]}, "group_size": {"config": "group_size", "default": 128}},
    "layout": {"kind": "tensors",
               "roles": [role("qweight", ".qweight", ["I32"], 2), role("qzeros", ".qzeros", ["I32"], 2), role("scales", ".scales", FLOATS, 2)],
               "dims": {"out": "dim_qweight[1] * 8", "inp": "dim_qweight[0]"},
               "checks": [
                   {"expr": "inp % gs == 0", "message": "the input width is not a multiple of the group size"},
                   {"expr": "dim_scales[0] == ng && dim_scales[1] == out", "message": "the scales are not [groups, outputs]"},
                   {"expr": "dim_qzeros[0] == ng && dim_qzeros[1] == dim_qweight[1]", "message": "qzeros is not [groups, outputs / 8]"}]},
    "tables": {"awq_slot": {"bits": 8, "signed": False, "hex": bytes([0, 4, 1, 5, 2, 6, 3, 7]).hex()}},
    "decode": {"target": "integers", "group": {"size": "group_size < 1 ? inp : group_size"},
               "q": "(qweight[i, o / 8] >> (4 * awq_slot[o % 8])) & 15", "scale": "scales[g, o]",
               "zero": "(qzeros[g, o / 8] >> (4 * awq_slot[o % 8])) & 15", "code": {"min": 0, "max": 15}},
}
F["fp8_block"] = {
    "schema": SCHEMA, "name": "FP8_BLOCK", "doc": "float8_e4m3fn weights with a float scale per (bo × bi) block (DeepSeek-V3, Qwen3-FP8: `weight` + `weight_scale_inv`): W = fp8(w) · scale_inv. The elements are floats, not integer codes under a group scale, so it decodes to floats and the weight takes the ordinary W8 path.",
    "ids": [{"scheme": "config", "method": "fp8"}],
    "params": {"bo": {"config": "weight_block_size[0]", "default": 128}, "bi": {"config": "weight_block_size[1]", "default": 128}},
    "layout": {"kind": "tensors",
               "roles": [role("weight", ".weight", ["F8_E4M3"], 2), role("scale_inv", ".weight_scale_inv", FLOATS, 2)],
               "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1]"},
               "checks": [{"expr": "dim_scale_inv[0] == (out + bo - 1) / bo && dim_scale_inv[1] == (inp + bi - 1) / bi",
                           "message": "the block scales are not [ceil(out / bo), ceil(in / bi)]"}]},
    "config": {"inert": ["activation_scheme", "fmt"], "skip": "modules_to_not_convert", "skip_match": "contains", "lm_head": "never"},
    "decode": {"target": "floats", "value": "weight[o, i] * scale_inv[o / bo, i / bi]"},
}

CT = "config_groups.group_0.weights."
CT_COMMON = [
    {"path": "config_groups.group_1", "one_of": [None], "message": "more than one config group: modules of one checkpoint use different formats"},
    {"path": "config_groups.group_0.targets", "one_of": [["Linear"]], "message": "targets other than all Linear modules"},
    {"path": "kv_cache_scheme", "one_of": [None], "message": "a quantised KV cache changes what the model computes"},
    {"path": "sparsity_config", "one_of": [None, {}], "message": "a sparse checkpoint stores its tensors differently"},
    {"path": "transform_config", "one_of": [None, {}], "message": "transforms (rotations) are applied to the weights at run time"},
    {"path": CT + "dynamic", "one_of": [None, False], "message": "dynamic weight quantisation"},
]
CT_CONFIG = {"inert": ["format", "global_compression_ratio", "compression_ratio", "quantization_status", "version"], "skip": "ignore", "skip_match": "exact", "lm_head": "unless_skipped"}
F["ct_pack"] = {
    "schema": SCHEMA, "name": "CT_PACK_QUANTIZED",
    "doc": "compressed-tensors, format pack-quantized (llm-compressor W4A16 / W8A16): `weight_packed` int32 [out, in/pack] packs 32/bits codes along the INPUT axis, stored unsigned (offset by 2^(bits-1)); `weight_scale` [out, groups]; an asymmetric checkpoint adds `weight_zero_point` int32 [out/pack, groups] packed along the OUTPUT axis (offset the same way, so the offsets cancel in q - zero); `weight_shape` [out, in]; act-order adds `weight_g_idx` [in]. W = scale * (q - zero).",
    "ids": [{"scheme": "config", "method": "compressed-tensors/pack-quantized"}],
    "config": dict(CT_CONFIG, checks=CT_COMMON + [
        {"path": CT + "type", "one_of": ["int"], "message": "float weights are format float-quantized"},
        {"path": CT + "strategy", "one_of": ["group", "channel"], "message": "a weight strategy other than group or channel"},
    ]),
    "params": {"bits": {"config": CT + "num_bits", "default": 4, "allowed": [4, 8]},
               "group_size": {"config": CT + "group_size", "default": -1},
               "sym": {"config": CT + "symmetric", "default": 1},
               "actorder": {"config": CT + "actorder", "default": 0, "map": {"group": 1, "weight": 1, "static": 1, "dynamic": 1}}},
    "layout": {"kind": "tensors",
               "roles": [role("packed", ".weight_packed", ["I32"], 2), role("scale", ".weight_scale", FLOATS, 2),
                         role("zp", ".weight_zero_point", ["I32"], 2, required=False), role("shape", ".weight_shape", ["I64", "I32"], 1),
                         role("g_idx", ".weight_g_idx", ["I32"], 1, required=False)],
               "dims": {"out": "shape[0]", "inp": "shape[1]"},
               "checks": [
                   {"expr": "dim_packed[0] == out && dim_packed[1] == (inp + " + PACK + " - 1) / " + PACK, "message": "weight_packed is not [out, ceil(in / pack)]"},
                   {"expr": "dim_scale[0] == out && dim_scale[1] == ng", "message": "weight_scale is not [out, groups]"},
                   {"expr": "sym || (has_zp && dim_zp[0] * " + PACK + " >= out && dim_zp[1] == ng)", "message": "the checkpoint is asymmetric but weight_zero_point is missing or not [out / pack, groups]"},
                   {"expr": "!has_g_idx || dim_g_idx[0] == inp", "message": "weight_g_idx is not one entry per input"}]},
    "decode": {"target": "integers",
               "group": {"size": "group_size < 1 ? inp : group_size", "index": "has_g_idx ? g_idx[i] : i / gs"},
               "q": "(packed[o, i / %s] >> (bits * (i %% %s))) & %s" % (PACK, PACK, MASK),
               "scale": "scale[o, g]",
               "zero": "has_zp ? ((zp[o / %s, g] >> (bits * (o %% %s))) & %s) : (1 << (bits - 1))" % (PACK, PACK, MASK),
               "code": {"min": 0, "max": "(1 << bits) - 1"},
               "offset_term": "bits == 8 && !sym", "order": "actorder != 0"},
}
F["ct_fp8"] = {
    "schema": SCHEMA, "name": "CT_FP8_CHANNEL",
    "doc": "compressed-tensors, format float-quantized, FP8 weights with one scale per output channel (RedHatAI *-FP8-dynamic): `weight` float8_e4m3fn [out, in], `weight_scale` [out, 1]. W = fp8(w) * scale; the elements are floats, so it decodes to floats and takes the ordinary W8 path.",
    "ids": [{"scheme": "config", "method": "compressed-tensors/float-quantized"}],
    "config": dict(CT_CONFIG, checks=CT_COMMON + [
        {"path": CT + "type", "one_of": ["float"], "message": "integer weights are another format"},
        {"path": CT + "num_bits", "one_of": [8], "message": "FP8 is 8 bits"},
        {"path": CT + "strategy", "one_of": ["channel"], "message": "a weight strategy other than per-channel"}]),
    "params": {},
    "layout": {"kind": "tensors",
               "roles": [role("weight", ".weight", ["F8_E4M3"], 2), role("scale", ".weight_scale", FLOATS, 2)],
               "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1]"},
               "checks": [{"expr": "dim_scale[0] == out && dim_scale[1] == 1", "message": "weight_scale is not [out, 1]"}]},
    "decode": {"target": "floats", "value": "weight[o, i] * scale[o, 0]"},
}
F["ct_int8"] = {
    "schema": SCHEMA, "name": "CT_INT8_CHANNEL",
    "doc": "compressed-tensors, format int-quantized, symmetric INT8 weights with one scale per output channel (W8A8): `weight` int8 [out, in], `weight_scale` [out, 1]. W = scale * q.",
    "ids": [{"scheme": "config", "method": "compressed-tensors/int-quantized"}],
    "config": dict(CT_CONFIG, checks=CT_COMMON + [
        {"path": CT + "type", "one_of": ["int"], "message": "float weights are format float-quantized"},
        {"path": CT + "num_bits", "one_of": [8], "message": "this format is 8-bit"},
        {"path": CT + "symmetric", "one_of": [True], "message": "asymmetric INT8 weights"},
        {"path": CT + "strategy", "one_of": ["channel"], "message": "a weight strategy other than per-channel"}]),
    "params": {},
    "layout": {"kind": "tensors",
               "roles": [role("weight", ".weight", ["I8"], 2), role("scale", ".weight_scale", FLOATS, 2)],
               "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1]"},
               "checks": [{"expr": "dim_scale[0] == out && dim_scale[1] == 1", "message": "weight_scale is not [out, 1]"}]},
    "decode": {"target": "integers", "group": {"size": "inp"}, "q": "weight[o, i]", "scale": "scale[o, 0]", "zero": "0",
               "code": {"min": -128, "max": 127}},
}

F["mxfp4_hf"] = {
    "schema": SCHEMA, "name": "MXFP4_HF",
    "doc": "OCP MXFP4 as Hugging Face stores it (gpt-oss): a weight tensor [E, out, in] kept as `<name>_blocks` (uint8 [E, out, in/32, 16]: two 4-bit codes a byte, the low nibble first) and `<name>_scales` (uint8 [E, out, in/32]: the E8M0 exponent of the block's scale, bias 127). The tensor the model uses is `<name>` [E, in, out] — the export's float tensor, transposed — served as floats: every binding that reads the float export reads this one unchanged. A leading expert axis and per-block scales over a 3-D tensor are the first lane `e` and a read `scales[e, o, i / 32]`: nothing here is specific to a model.",
    "ids": [{"scheme": "config", "method": "mxfp4"}],
    "config": {"skip": "modules_to_not_convert", "skip_match": "regex", "lm_head": "never"},
    "params": {},
    # value x 2 of the sixteen FP4 (E2M1) codes: 0, .5, 1, 1.5, 2, 3, 4, 6, -0, -.5, -1, -1.5, -2, -3, -4, -6
    "tables": {"fp4x2": {"bits": 8, "signed": True, "hex": bytes([0, 1, 2, 3, 4, 6, 8, 12, 0, 255, 254, 253, 252, 250, 248, 244]).hex()}},
    "layout": {"kind": "virtual",
               "roles": [role("blocks", "_blocks", ["U8"], 4), role("scales", "_scales", ["U8"], 3)],
               "axes": ["e", "i", "o"],
               "shape": ["dim_blocks[0]", "dim_blocks[2] * 32", "dim_blocks[1]"],
               "checks": [
                   {"expr": "dim_blocks[3] == 16", "message": "a block is not 16 bytes (32 four-bit codes)"},
                   {"expr": "dim_scales[0] == dim_blocks[0] && dim_scales[1] == dim_blocks[1] && dim_scales[2] == dim_blocks[2]", "message": "the scales are not one per block"}]},
    "decode": {"target": "tensor",
               "value": "fp4x2[(blocks[e, o, i / 32, (i % 32) / 2] >> (4 * (i % 2))) & 15] * 0.5 * e8m0(scales[e, o, i / 32])"},
}


# ───────────────────────────── bitsandbytes ─────────────────────────────
# What is stored and how it is dequantised is written out in tools/bnb_ref.py. In short: a 4-bit weight is a uint8 [ceil(N / 2), 1] of
# the flattened row-major weight (the even element in the HIGH nibble), one absmax per `blocksize` flat elements, a 16-entry code table
# stored in the checkpoint, and a JSON document (as a uint8 tensor) holding the logical shape, the block size and the module dtype; double
# quantisation makes absmax uint8 and adds a nested absmax (float, per 256 of them) with its own 256-entry code and an offset.
BNB_INERT_4 = ["bnb_4bit_compute_dtype", "bnb_4bit_use_double_quant", "llm_int8_enable_fp32_cpu_offload", "llm_int8_has_fp16_weight",
               "llm_int8_threshold", "_load_in_4bit", "_load_in_8bit"]
BNB_INERT_8 = ["bnb_4bit_compute_dtype", "bnb_4bit_quant_type", "bnb_4bit_quant_storage", "bnb_4bit_use_double_quant",
               "llm_int8_enable_fp32_cpu_offload", "llm_int8_has_fp16_weight", "_load_in_4bit", "_load_in_8bit"]

def bnb4(kind):
    K = "(o * shape1 + i)"
    CODE = "((weight[%s / 2, 0] >> (4 * (1 - %s %% 2))) & 15)" % (K, K)
    B = "(%s / bs)" % K
    ABSMAX = "(has_nabs ? rnd(rnd(nmap[absmax[%s]] * nabs[%s / nbs], 0) + rnd(noff, 0), 0) : absmax[%s])" % (B, B, B)
    when = [{"path": "load_in_4bit", "one_of": [True]},
            {"path": "bnb_4bit_quant_type", "one_of": ["nf4"] if kind == "nf4" else ["fp4", None]}]
    def jp(path, kind="int", default=None):
        d = {"from_role": {"role": "qstate", "path": path}}
        if kind != "int":
            d["from_role"]["kind"] = kind
        if default is not None:
            d["default"] = default
        return d
    return {
        "schema": SCHEMA, "name": "BNB_" + kind.upper(),
        "doc": ("bitsandbytes 4-bit " + ("NormalFloat (nf4)" if kind == "nf4" else "FP4") + ", as `Linear4bit` saves it to safetensors. The weight is a uint8 [ceil(N / 2), 1]: the flattened row-major [out, in] "
                "weight, two 4-bit codes a byte, the first (even-index) element in the HIGH nibble; `.weight.absmax` has one float per `blocksize` flat elements "
                "(under double quantisation, `bnb_4bit_use_double_quant`, it is uint8: a code of `.weight.nested_quant_map` scaled by `.weight.nested_absmax`, one per `nested_blocksize` of "
                "them, plus the document's `nested_offset`); `.weight.quant_map` is the 16-entry code table, stored in the checkpoint, and `.weight.quant_state.bitsandbytes__" + kind + "` is a JSON document "
                "(a uint8 tensor) holding the logical `shape`, `blocksize`, the module's `dtype` and the nested parameters. A weight element is the float32 product of its code and its (de-nested) absmax, rounded once to the module "
                "dtype — bitsandbytes' CUDA kernel's arithmetic (its pure-PyTorch fallback rounds the code table to that dtype first and differs by up to one bfloat16 ulp). Double quantisation is exact here: "
                "each of its float32 operations (the product by the nested absmax, the sum with the offset) is a `rnd(., 0)` of a float64 operation on float32 operands, which is the correctly rounded float32 result. "
                "The code table is read from the checkpoint, so the format is table-agnostic; for FP4 the CUDA kernel's own literal for code 1 (0.00520833) differs from the stored table's (0.0625 / 12 in float32) in the seventh digit, "
                "and this follows the stored table. Decodes to floats, so the weight takes the ordinary W8 path. "
                "Refused by name: `bnb_4bit_quant_storage` other than uint8 (the packed bytes are stored in another dtype and shape)."),
        "ids": [{"scheme": "config", "method": "bitsandbytes", "when": when}],
        "config": {"inert": BNB_INERT_4, "skip": "llm_int8_skip_modules", "skip_match": "path", "lm_head": "unless_skipped",
                   "checks": [
                       {"path": "load_in_4bit", "one_of": [True], "message": "this is the 4-bit format"},
                       {"path": "load_in_8bit", "one_of": [None, False], "message": "8-bit and 4-bit together"},
                       {"path": "bnb_4bit_quant_type", "one_of": ["nf4"] if kind == "nf4" else ["fp4", None], "message": "the 4-bit type is " + kind},
                       {"path": "bnb_4bit_quant_storage", "one_of": [None, "uint8"],
                        "message": "4-bit weights packed into a storage dtype other than uint8 (FSDP-style) are stored with another dtype and shape than this format reads"}]},
        "params": {
            "shape0": jp("shape[0]"), "shape1": jp("shape[1]"),
            "shape2": jp("shape[2]", default=0),
            "bs": jp("blocksize"),
            "dt": dict(jp("dtype", "string"), map={"float32": 0, "bfloat16": 1, "float16": 2}),
            "nbs": jp("nested_blocksize", default=256),
            "noff": jp("nested_offset", "float", default=0),
        },
        "layout": {"kind": "tensors",
                   "roles": [role("weight", ".weight", ["U8"], 2), role("absmax", ".weight.absmax", ["F32", "U8"], 1),
                             role("qmap", ".weight.quant_map", ["F32"], 1),
                             role("qstate", ".weight.quant_state.bitsandbytes__" + kind, ["U8"], 1),
                             role("nabs", ".weight.nested_absmax", ["F32"], 1, required=False),
                             role("nmap", ".weight.nested_quant_map", ["F32"], 1, required=False)],
                   "dims": {"out": "shape0", "inp": "shape1"},
                   "checks": [
                       {"expr": "shape2 == 0", "message": "the logical weight is not a matrix (its `shape` has more than two entries)"},
                       {"expr": "dim_weight[1] == 1 && dim_weight[0] * 2 >= out * inp && dim_weight[0] * 2 <= out * inp + 1", "message": "the packed weight is not [ceil(out * in / 2), 1]"},
                       {"expr": "bs > 0 && dim_absmax[0] == (out * inp + bs - 1) / bs", "message": "absmax is not one per block of the flat weight"},
                       {"expr": "dim_qmap[0] == 16", "message": "the code table does not have 16 entries"},
                       {"expr": "has_nabs == has_nmap", "message": "nested_absmax and nested_quant_map come together"},
                       {"expr": "has_nabs + float_absmax == 1", "message": "absmax is 8-bit exactly when it is double-quantised (nested_absmax present) and float otherwise"},
                       {"expr": "!has_nabs || (nbs > 0 && dim_nmap[0] == 256 && dim_nabs[0] == (dim_absmax[0] + nbs - 1) / nbs)",
                        "message": "the nested statistics are not one float per `nested_blocksize` absmax values with a 256-entry code"}]},
        "decode": {"target": "floats", "value": "rnd(rnd(qmap[%s] * %s, 0), dt)" % (CODE, ABSMAX)},
    }

F["bnb_nf4"] = bnb4("nf4")
F["bnb_fp4"] = bnb4("fp4")
F["bnb_int8"] = {
    "schema": SCHEMA, "name": "BNB_INT8",
    "doc": ("bitsandbytes LLM.int8, as `Linear8bitLt` saves it: `.weight` int8 [out, in] and `.SCB` float32 [out], the absolute maximum of each row of the original weight, so W[o, i] = weight[o, i] * SCB[o] / 127 "
            "(row-wise absmax quantisation); `.weight_format` (uint8 scalar) must be 0, the row-major layout. The stored integers are lowered as they are (the per-row scale is SCB * float32(1 / 127), rounded to float32; "
            "bitsandbytes' own `int8_vectorwise_dequant` rounds twice and agrees to one unit in the last place). "
            "Refused by name: `llm_int8_threshold` above 0 — LLM.int8 then splits every matmul at run time (the activation columns whose magnitude passes the threshold are multiplied in float16 against the dequantised weight "
            "columns, the rest in int8), which a fixed integer graph does not reproduce; and a hardware-reordered `weight_format` (col32 / col_turing / col_ampere)."),
    "ids": [{"scheme": "config", "method": "bitsandbytes", "when": [{"path": "load_in_8bit", "one_of": [True]}]}],
    "config": {"inert": BNB_INERT_8, "skip": "llm_int8_skip_modules", "skip_match": "path", "lm_head": "unless_skipped",
               "checks": [
                   {"path": "load_in_8bit", "one_of": [True], "message": "this is the 8-bit format"},
                   {"path": "load_in_4bit", "one_of": [None, False], "message": "4-bit and 8-bit together"},
                   {"path": "llm_int8_threshold", "one_of": [0, 0.0],
                    "message": "llm_int8_threshold above 0 means outlier decomposition: LLM.int8 splits each matmul at run time (activation columns above the threshold go through float16 against the dequantised weight columns, the rest through int8), which a fixed integer graph does not reproduce — a checkpoint saved with llm_int8_threshold 0 is read"}]},
    "params": {},
    "layout": {"kind": "tensors",
               "roles": [role("weight", ".weight", ["I8"], 2), role("scb", ".SCB", ["F32"], 1), role("wfmt", ".weight_format", ["U8"], 0, required=False)],
               "dims": {"out": "dim_weight[0]", "inp": "dim_weight[1]"},
               "checks": [{"expr": "dim_scb[0] == out", "message": "SCB is not one scale per output row"},
                          {"expr": "!has_wfmt || wfmt == 0", "message": "the int8 weight is stored in a hardware-reordered layout (`weight_format` col32 / col_turing / col_ampere), not row-major"}]},
    "decode": {"target": "integers", "group": {"size": "inp"}, "q": "weight[o, i]", "scale": "rnd(scb[o] * rnd(0.007874015718698502, 0), 0)",
               "zero": "0", "code": {"min": -128, "max": 127}},
}

# ───────────────────────────── test vectors ─────────────────────────────
rng = np.random.default_rng(0x4D495341)

def numpy_c_q1_0(raw):
    out = []
    for b in raw.reshape(-1, 18):
        d = np.frombuffer(b[:2].tobytes(), np.float16)[0].astype(np.float32)
        for j in range(128):
            bit = (int(b[2 + j // 8]) >> (j % 8)) & 1
            out.append(d if bit else -d)
    return np.array(out, np.float32)

def numpy_c_q2_0(raw):
    out = []
    for b in raw.reshape(-1, 18):
        d = np.frombuffer(b[:2].tobytes(), np.float16)[0].astype(np.float32)
        for j in range(64):
            q = (int(b[2 + j // 4]) >> ((j % 4) * 2)) & 3
            out.append(np.float32(q - 1) * d)
    return np.array(out, np.float32)

QT = {"F32": T.F32, "F16": T.F16, "BF16": T.BF16, "Q4_0": T.Q4_0, "Q4_1": T.Q4_1, "Q5_0": T.Q5_0, "Q5_1": T.Q5_1, "Q8_0": T.Q8_0, "Q2_K": T.Q2_K,
      "Q3_K": T.Q3_K, "Q4_K": T.Q4_K, "Q5_K": T.Q5_K, "Q6_K": T.Q6_K, "IQ4_NL": T.IQ4_NL, "IQ4_XS": T.IQ4_XS, "TQ1_0": T.TQ1_0, "TQ2_0": T.TQ2_0,
      "MXFP4": T.MXFP4, "NVFP4": T.NVFP4, "IQ2_XXS": T.IQ2_XXS, "IQ2_XS": T.IQ2_XS, "IQ2_S": T.IQ2_S, "IQ3_XXS": T.IQ3_XXS, "IQ3_S": T.IQ3_S,
      "IQ1_S": T.IQ1_S, "IQ1_M": T.IQ1_M}

def reference(name, raw):
    if name == "Q1_0":
        return numpy_c_q1_0(raw)
    if name == "Q2_0":
        return numpy_c_q2_0(raw)
    if name == "F64":
        return np.frombuffer(raw.tobytes(), np.float64).astype(np.float32)
    bs, ts = gguf.GGML_QUANT_SIZES[QT[name]]
    with np.errstate(all="ignore"):
        return gq.dequantize(raw.reshape(1, -1), QT[name]).reshape(-1).astype(np.float32)

# ───────────────── multi-tensor vectors: torch re-implementations of each library's own dequantiser ─────────────────
# The role tensors are random but valid; the expected weight is computed FROM THE STORED TENSORS by code that follows
# the reference library (AutoGPTQ's QuantLinear dequant, AutoAWQ's dequantize_gemm, DeepSeek's weight_dequant,
# compressed-tensors' unpack_from_int32 + dequantize), not by the descriptor. Products are taken in float32.
trng = np.random.default_rng(0x54454E53)   # a separate stream: the blocks formats' vectors do not move when a tensors format is added

def pack_along(vals, bits, axis):
    """Unsigned codes packed into int32 words along `axis`, lowest code in the lowest bits."""
    pack = 32 // bits
    v = np.moveaxis(np.asarray(vals).astype(np.uint64), axis, -1)
    n = v.shape[-1]
    assert n % pack == 0
    v = v.reshape(*v.shape[:-1], n // pack, pack)
    w = (v << (np.arange(pack, dtype=np.uint64) * bits)).sum(-1) & 0xFFFFFFFF
    return np.ascontiguousarray(np.moveaxis(w.astype(np.uint32).view(np.int32), -1, axis))

def ti(a):
    return torch.from_numpy(np.ascontiguousarray(a).astype(np.int32))

def ref_gptq(qweight, qzeros, scales, g_idx, bits, v2):
    pack, mask = 32 // bits, (1 << bits) - 1
    wf = torch.arange(0, 32, bits, dtype=torch.int32)
    zeros = (torch.unsqueeze(ti(qzeros), 2).expand(-1, -1, pack) >> wf.unsqueeze(0)) & mask
    sc = torch.from_numpy(scales.astype(np.float32))
    zeros = zeros.reshape(sc.shape)
    if not v2:
        zeros = zeros + 1
    weight = (torch.unsqueeze(ti(qweight), 1).expand(-1, pack, -1) >> wf.unsqueeze(-1)) & mask
    weight = weight.reshape(-1, weight.shape[2])
    gi = torch.from_numpy(g_idx.astype(np.int64))
    w = sc[gi] * (weight - zeros[gi]).to(torch.float32)
    return w.t().contiguous().numpy()

AWQ_REVERSE_ORDER = [0, 4, 1, 5, 2, 6, 3, 7]

def ref_awq(qweight, qzeros, scales, bits, group_size):
    qw, qz = ti(qweight), ti(qzeros)
    shifts = torch.arange(0, 32, bits)
    iw = torch.bitwise_right_shift(qw[:, :, None], shifts[None, None, :]).to(torch.int16)
    iw = iw.view(iw.shape[0], -1)
    iz = torch.bitwise_right_shift(qz[:, :, None], shifts[None, None, :]).to(torch.int16)
    iz = iz.view(iz.shape[0], -1)
    rev = torch.arange(iw.shape[-1], dtype=torch.int32).view(-1, 32 // bits)[:, AWQ_REVERSE_ORDER].reshape(-1).long()
    iw, iz = iw[:, rev], iz[:, rev]
    iw = torch.bitwise_and(iw, (1 << bits) - 1)
    iz = torch.bitwise_and(iz, (1 << bits) - 1)
    sc = torch.from_numpy(scales.astype(np.float32)).repeat_interleave(group_size, dim=0)
    iz = iz.repeat_interleave(group_size, dim=0)
    w = (iw - iz).to(torch.float32) * sc
    return w.t().contiguous().numpy()

def ref_fp8_block(w_u8, scale_inv, bo, bi):
    w = torch.from_numpy(w_u8.copy()).view(torch.float8_e4m3fn).to(torch.float32)
    s = torch.from_numpy(scale_inv.astype(np.float32)).repeat_interleave(bo, 0).repeat_interleave(bi, 1)
    return (w * s[: w.shape[0], : w.shape[1]]).numpy()

def unpack_ct(value, num_bits, shape, packed_dim):
    """compressed_tensors.compressors.quantized_compressors.pack_quantized.unpack_from_int32"""
    pack_factor, mask = 32 // num_bits, (1 << num_bits) - 1
    if packed_dim == 1:
        out = torch.zeros((value.shape[0], value.shape[1] * pack_factor), dtype=torch.int32)
        for i in range(pack_factor):
            out[:, i::pack_factor] = (value >> (num_bits * i)) & mask
        out = out[:, : int(shape[1])]
    else:
        out = torch.zeros((value.shape[0] * pack_factor, value.shape[1]), dtype=torch.int32)
        for i in range(pack_factor):
            out[i::pack_factor, :] = (value >> (num_bits * i)) & mask
        out = out[: int(shape[0]), :]
    return (out - (1 << num_bits) // 2).to(torch.int8)

def ref_ct_pack(packed, scale, zp, shape, g_idx, bits, group_size):
    out, inp = int(shape[0]), int(shape[1])
    q = unpack_ct(ti(packed), bits, shape, 1).to(torch.int32)
    sc = torch.from_numpy(scale.astype(np.float32))
    ng = sc.shape[1]
    z = unpack_ct(ti(zp), bits, (out, ng), 0).to(torch.int32) if zp is not None else torch.zeros((out, ng), dtype=torch.int32)
    gi = torch.from_numpy(g_idx.astype(np.int64)) if g_idx is not None else torch.arange(inp) // (inp if group_size < 1 else group_size)
    w = (q - z[:, gi]).to(torch.float32) * sc[:, gi]
    return w.contiguous().numpy()

def f16_scales(shape, lo=0.02, hi=1.5):
    return trng.uniform(lo, hi, size=shape).astype(np.float16)

def role_json(arr, dtype):
    return {"dtype": dtype, "shape": list(arr.shape), "hex": np.ascontiguousarray(arr).tobytes().hex()}

def ct_config(bits, group_size, sym, strategy, actorder=None, fmt="pack-quantized", typ="int"):
    return {"quant_method": "compressed-tensors", "format": fmt, "quantization_status": "compressed", "ignore": ["lm_head"],
            "kv_cache_scheme": None, "sparsity_config": {}, "transform_config": {},
            "config_groups": {"group_0": {"targets": ["Linear"], "input_activations": None, "output_activations": None,
                                          "weights": {"num_bits": bits, "type": typ, "symmetric": sym, "strategy": strategy, "group_size": group_size,
                                                      "actorder": actorder, "dynamic": False, "observer": "minmax"}}}}

def tensor_vectors(d):
    name, out = d["name"], []
    def case(cfg, roles, ref):
        assert np.isfinite(ref).all()
        out.append({"config": cfg, "roles": roles, "values_f32_hex": ref.astype("<f4").tobytes().hex()})
    if name == "GPTQ":
        for bits, gs, v2, act, sym in [(4, 8, False, False, True), (4, 8, True, True, False), (8, 16, False, True, False), (2, 8, False, False, True),
                                       (4, -1, False, False, True), (8, 16, False, False, True)]:
            o, i = 16, 32
            pack, ggs = 32 // bits, (i if gs < 1 else gs)
            ng = -(-i // ggs)
            qweight = trng.integers(-2**31, 2**31, size=(i // pack, o), dtype=np.int64).astype(np.int32)
            zfield = trng.integers(0, (1 << bits) - 1, size=(ng, o))     # stored z (v2) or z - 1 (v1): v1's z = 0 edge avoided
            qzeros = pack_along(zfield, bits, 1)
            scales = f16_scales((ng, o))
            perm = trng.permutation(i)
            g_idx = ((perm // ggs) if act else (np.arange(i) // ggs)).astype(np.int32)
            ref = ref_gptq(qweight, qzeros, scales, g_idx, bits, v2)
            cfg = {"quant_method": "gptq", "bits": bits, "group_size": gs, "sym": sym, "desc_act": act, "checkpoint_format": "gptq_v2" if v2 else "gptq"}
            case(cfg, {"qweight": role_json(qweight, "I32"), "qzeros": role_json(qzeros, "I32"), "scales": role_json(scales, "F16"),
                       "g_idx": role_json(g_idx, "I32")}, ref)
        # no g_idx tensor at all (checkpoints written without act-order may omit it)
        o, i, bits, gs = 8, 16, 4, 8
        qweight = trng.integers(-2**31, 2**31, size=(i // 8, o), dtype=np.int64).astype(np.int32)
        qzeros = pack_along(trng.integers(0, 15, size=(2, o)), 4, 1)
        scales = f16_scales((2, o))
        case({"quant_method": "gptq", "bits": 4, "group_size": 8}, {"qweight": role_json(qweight, "I32"), "qzeros": role_json(qzeros, "I32"), "scales": role_json(scales, "F16")},
             ref_gptq(qweight, qzeros, scales, np.arange(i) // gs, 4, False))
    elif name == "AWQ":
        for gs in (8, 16, -1):
            o, i = 16, 32
            ggs = i if gs < 1 else gs
            ng = i // ggs
            qweight = trng.integers(-2**31, 2**31, size=(i, o // 8), dtype=np.int64).astype(np.int32)
            qzeros = trng.integers(-2**31, 2**31, size=(ng, o // 8), dtype=np.int64).astype(np.int32)
            scales = f16_scales((ng, o))
            case({"quant_method": "awq", "bits": 4, "group_size": gs, "zero_point": True, "version": "gemm"},
                 {"qweight": role_json(qweight, "I32"), "qzeros": role_json(qzeros, "I32"), "scales": role_json(scales, "F16")},
                 ref_awq(qweight, qzeros, scales, 4, ggs))
    elif name == "FP8_BLOCK":
        for o, i, bo, bi, sdt in [(12, 20, 4, 8, "F32"), (5, 7, 3, 4, "BF16"), (8, 16, 8, 16, "F16")]:
            w = trng.integers(0, 256, size=(o, i), dtype=np.uint8)
            w[(w & 0x7F) == 0x7F] = 0x38          # e4m3fn's NaN encodings
            ns = (-(-o // bo), -(-i // bi))
            if sdt == "BF16":
                sc32 = trng.uniform(0.001, 0.5, size=ns).astype(np.float32)
                sc = (sc32.view(np.uint32) >> 16).astype(np.uint16)             # bf16 bits
                sc_f = (sc.astype(np.uint32) << 16).view(np.float32)
                role = {"dtype": "BF16", "shape": list(ns), "hex": sc.astype("<u2").tobytes().hex()}
            elif sdt == "F16":
                sc_f = f16_scales(ns, 0.001, 0.5).astype(np.float32)
                role = role_json(sc_f.astype(np.float16), "F16")
            else:
                sc_f = trng.uniform(0.001, 0.5, size=ns).astype(np.float32)
                role = role_json(sc_f, "F32")
            case({"quant_method": "fp8", "activation_scheme": "dynamic", "fmt": "e4m3", "weight_block_size": [bo, bi]},
                 {"weight": role_json(w, "F8_E4M3"), "scale_inv": role}, ref_fp8_block(w, sc_f, bo, bi))
    elif name == "CT_PACK_QUANTIZED":
        for bits, gs, sym, act in [(4, 8, True, False), (4, 8, False, False), (8, 16, True, False), (8, 16, False, False), (4, -1, True, False),
                                   (4, 8, True, True), (4, 16, False, True)]:
            o, i = 16, 32
            pack, ggs = 32 // bits, (i if gs < 1 else gs)
            ng = i // ggs
            packed = trng.integers(-2**31, 2**31, size=(o, i // pack), dtype=np.int64).astype(np.int32)
            scale = f16_scales((o, ng))
            zp = None if sym else trng.integers(-2**31, 2**31, size=(o // pack, ng), dtype=np.int64).astype(np.int32)
            g_idx = (trng.permutation(i) // ggs).astype(np.int32) if act else None
            shape = np.array([o, i], dtype=np.int64)
            ref = ref_ct_pack(packed, scale, zp, shape, g_idx, bits, gs)
            roles = {"packed": role_json(packed, "I32"), "scale": role_json(scale, "F16"), "shape": role_json(shape, "I64")}
            if zp is not None:
                roles["zp"] = role_json(zp, "I32")
            if g_idx is not None:
                roles["g_idx"] = role_json(g_idx, "I32")
            case(ct_config(bits, gs if gs > 0 else None, sym, "group" if gs > 0 else "channel", "group" if act else None), roles, ref)
    elif name == "CT_FP8_CHANNEL":
        for o, i in [(8, 16), (5, 7)]:
            w = trng.integers(0, 256, size=(o, i), dtype=np.uint8)
            w[(w & 0x7F) == 0x7F] = 0x38
            sc = trng.uniform(0.001, 0.5, size=(o, 1)).astype(np.float32)
            ref = ref_fp8_block(w, sc, 1, i)
            case(ct_config(8, None, True, "channel", None, "float-quantized", "float"), {"weight": role_json(w, "F8_E4M3"), "scale": role_json(sc, "F32")}, ref)
    elif name == "CT_INT8_CHANNEL":
        for o, i in [(8, 16), (6, 9)]:
            w = trng.integers(-128, 128, size=(o, i)).astype(np.int8)
            sc = f16_scales((o, 1), 0.001, 0.2)
            ref = (torch.from_numpy(w.astype(np.int32)).to(torch.float32) * torch.from_numpy(sc.astype(np.float32))).numpy()
            case(ct_config(8, None, True, "channel", None, "int-quantized", "int"), {"weight": role_json(w, "I8"), "scale": role_json(sc, "F16")}, ref)
    elif name == "MXFP4_HF":
        # The library's own function: transformers.integrations.mxfp4.convert_moe_packed_tensors (the served tensor is its [E, in, out]).
        from transformers.integrations.mxfp4 import convert_moe_packed_tensors
        for E, O, G in [(2, 4, 2), (3, 2, 3), (1, 3, 1), (2, 5, 4)]:
            blocks = trng.integers(0, 256, size=(E, O, G, 16), dtype=np.uint8)
            scales = (127 + trng.integers(-7, 6, size=(E, O, G))).astype(np.uint8)
            ref = convert_moe_packed_tensors(torch.from_numpy(blocks), torch.from_numpy(scales), dtype=torch.bfloat16).to(torch.float32).numpy()
            assert ref.shape == (E, G * 32, O)
            case({"quant_method": "mxfp4", "modules_to_not_convert": ["model.layers.*.self_attn", "lm_head"]},
                 {"blocks": role_json(blocks, "U8"), "scales": role_json(scales, "U8")}, ref)
    elif name in ("BNB_NF4", "BNB_FP4"):
        kind = "nf4" if name == "BNB_NF4" else "fp4"
        def bcfg(dq, dtype, skip=None):
            return {"_load_in_4bit": True, "_load_in_8bit": False, "bnb_4bit_compute_dtype": "bfloat16", "bnb_4bit_quant_storage": "uint8",
                    "bnb_4bit_quant_type": kind, "bnb_4bit_use_double_quant": dq, "llm_int8_enable_fp32_cpu_offload": False,
                    "llm_int8_has_fp16_weight": False, "llm_int8_skip_modules": skip, "llm_int8_threshold": 6.0, "load_in_4bit": True,
                    "load_in_8bit": False, "quant_method": "bitsandbytes"}
        # (out, in, blocksize, double quantisation, module dtype): a ragged last block, an odd element count, several nested blocks
        for o, i, bs, dq, dt, nbs in [(8, 32, 64, False, "bfloat16", None), (6, 40, 64, False, "float16", None), (3, 5, 64, False, "float32", None),
                                      (5, 24, 64, True, "float16", 256), (12, 48, 64, True, "bfloat16", 4), (7, 33, 128, True, "float32", 2),
                                      (4, 16, 16, False, "bfloat16", None)]:
            w = (trng.standard_normal((o, i)) * trng.uniform(0.02, 0.4)).astype(np.float32)
            if (o, i) == (8, 32):
                w[2, :] = 0.0               # a block of zeros: absmax 0
            t = bnb_ref.quantize_4bit(w, kind, bs, dq, dt)
            if nbs is not None and nbs != 256:
                # a smaller nested block than bitsandbytes' 256, to have several nested blocks in a small tensor: requantise the absmax that way
                state = json.loads(bytes(t[f".weight.quant_state.bitsandbytes__{kind}"]).decode())
                nb = -(-(o * i) // bs)
                x = np.concatenate([np.abs(w.reshape(-1)).reshape(-1), np.zeros(nb * bs - o * i, np.float32)]).reshape(nb, bs)
                absmax = x.max(1).astype(np.float32)
                offset = np.float32(absmax.mean())
                a = (absmax - offset).astype(np.float32)
                nb2 = -(-a.size // nbs)
                a2 = np.concatenate([a, np.zeros(nb2 * nbs - a.size, np.float32)]).reshape(nb2, nbs)
                nabs = np.abs(a2).max(1).astype(np.float32)
                y2 = a2 / np.where(nabs > 0, nabs, np.float32(1))[:, None]
                q = bnb_ref._nearest(y2, t[".weight.nested_quant_map"]).reshape(-1)[: a.size].astype(np.uint8)
                t[".weight.absmax"], t[".weight.nested_absmax"] = q, nabs
                state.update({"nested_blocksize": nbs, "nested_offset": float(offset)})
                t[f".weight.quant_state.bitsandbytes__{kind}"] = np.frombuffer(json.dumps(state).encode(), dtype=np.uint8).copy()
            ref = bnb_ref.dequantize_4bit(t)
            assert ref.shape == (o, i)
            roles = {"weight": role_json(t[".weight"], "U8"),
                     "absmax": role_json(t[".weight.absmax"], "U8" if dq else "F32"),
                     "qmap": role_json(t[".weight.quant_map"], "F32"),
                     "qstate": role_json(t[f".weight.quant_state.bitsandbytes__{kind}"], "U8")}
            if dq:
                roles["nabs"] = role_json(t[".weight.nested_absmax"], "F32")
                roles["nmap"] = role_json(t[".weight.nested_quant_map"], "F32")
            case(bcfg(dq, dt), roles, ref)
    elif name == "BNB_INT8":
        for o, i in [(6, 16), (5, 9)]:
            w = (trng.standard_normal((o, i)) * trng.uniform(0.02, 0.4)).astype(np.float32)
            w[0, :] = 0.0                    # an all-zero row: SCB 0
            t = bnb_ref.quantize_int8(w)
            t[".weight"][1, 0] = -128        # the whole int8 range is read
            ref = bnb_ref.dequantize_int8(t)
            # the library's own expression agrees to one float32 unit in the last place
            lib = bnb_ref.bnb_dequant_int8(t)
            assert np.all(np.abs(ref - lib) <= np.abs(lib) * 2.4e-7), "the stored-integer scale and bitsandbytes' vectorwise dequantisation differ by more than an ulp"
            cfg = {"_load_in_4bit": False, "_load_in_8bit": True, "bnb_4bit_compute_dtype": "float32", "bnb_4bit_quant_storage": "uint8",
                   "bnb_4bit_quant_type": "fp4", "bnb_4bit_use_double_quant": False, "llm_int8_enable_fp32_cpu_offload": False,
                   "llm_int8_has_fp16_weight": False, "llm_int8_skip_modules": None, "llm_int8_threshold": 0.0, "load_in_4bit": False,
                   "load_in_8bit": True, "quant_method": "bitsandbytes"}
            roles = {"weight": role_json(t[".weight"], "I8"), "scb": role_json(t[".SCB"], "F32"), "wfmt": role_json(t[".weight_format"], "U8")}
            case(cfg, roles, ref)
    return out

def vectors(d, kinds=("random", "random", "random", "random", "extreme", "extreme")):
    name, L = d["name"], d["layout"]
    if L["kind"] != "blocks":
        return tensor_vectors(d)
    blocks = 1 if L["elems"] >= 64 else 4
    out = []
    for kind in kinds:
        for attempt in range(2000):
            if d["decode"]["target"] == "floats":
                raw = rng.integers(0, 256, size=L["bytes"] * 4, dtype=np.uint8)
            elif kind == "extreme":
                raw = np.full(L["bytes"] * blocks, 0xFF if attempt % 2 == 0 else 0x00, np.uint8)
                # a valid, mid-range f16 in every 2-byte field named d / dmin / m (value 1.5 = 0x3e00)
                for f in L["fields"]:
                    if f["type"] == "f16":
                        for b in range(blocks):
                            raw[b * L["bytes"] + f["at"]: b * L["bytes"] + f["at"] + 2] = np.frombuffer(np.float16(1.5).tobytes(), np.uint8)
            else:
                raw = rng.integers(0, 256, size=L["bytes"] * blocks, dtype=np.uint8)
            try:
                ref = reference(name, raw)
            except Exception:
                continue
            if np.isfinite(ref).all() and (name != "IQ1_M" or True):
                if name == "NVFP4" and False:
                    pass
                out.append({"block_hex": raw.tobytes().hex(), "values_f32_hex": ref.astype("<f4").tobytes().hex()})
                break
        else:
            raise SystemExit("no finite vector for " + name + " " + kind)
    return out

if len(sys.argv) > 1 and sys.argv[1] == "--corpus":
    # A large randomised cross-check corpus (not committed): JSON lines {format, block_hex, values_f32_hex}.
    path, n = sys.argv[2], int(sys.argv[3])
    with open(path, "w") as f:
        for key, d in F.items():
            for v in vectors(d, kinds=("random",) * n):
                f.write(json.dumps({"format": d["name"], **v}) + "\n")
    print("wrote", path)
    sys.exit(0)

os.makedirs(OUT, exist_ok=True)
for key, d in F.items():
    t = vectors(d)
    if t:
        d["tests"] = t
    # IQ1_M random sc → the f16 may be non-finite; the loop retries until the reference is finite.
    text = json.dumps(d, indent=1, sort_keys=True, ensure_ascii=False) + "\n"
    open(os.path.join(OUT, key + ".json"), "w").write(text)
    print("wrote", key, len(text), "bytes", len(d.get("tests", [])), "vectors")
