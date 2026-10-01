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
               "v2": {"config": "checkpoint_format", "default": 0, "map": {"gptq": 0, "gptq_v2": 1}}},
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
               "code": {"min": 0, "max": "(1 << bits) - 1"}},
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
    "decode": {"target": "floats", "value": "weight[o, i] * scale_inv[o / bo, i / bi]"},
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

def vectors(d, kinds=("random", "random", "random", "random", "extreme", "extreme")):
    name, L = d["name"], d["layout"]
    if L["kind"] != "blocks":
        return []
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
