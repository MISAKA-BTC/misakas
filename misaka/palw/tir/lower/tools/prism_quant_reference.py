#!/usr/bin/env python3
"""PTQ1_0 (ggml type 143) and PQ2_0 (type 142), PrismML — INDEPENDENT pure-Python dequantisers, written line by line from the C source text
`dequantize_row_ptq1_0` of PrismML-Eng/llama.cpp at commit 7dffb158de30ebb8ef9d64f33c6b0b2d7c1e6313 (release prism-b10685,
ggml/src/ggml-quants.c; the block struct `block_ptq1_0` of ggml/src/ggml-common.h): 128 weights per 28-byte block,
`qs[24]` (5 base-3 digits a byte), `qh[2]` (4 digits a byte), `d` (fp16) LAST; the digits read in stages of 16 then 8 bytes
(`ptq1_0_stages = {32, 16, 8}`, a stage used while `j + c <= 24`); digit n of byte b is `((uint8)(b * 3^n) * 3) >> 8`, the
weight `(digit - 1) * d`. It shares no code with the Rust descriptor interpreter (`quantfmt::blocks`), whose expression for
the same format (`quant-formats/ptq1_0.json`) was derived separately; the two are compared value by value.

PQ2_0, from `dequantize_row_pq2_0` and `block_pq2_0` of the same commit: 128 weights per 34-byte block, `d` (fp16) FIRST,
then `qs[32]`; weight j is `(((qs[j / 4] >> 2 * (j % 4)) & 3) - 1) * d` (00 = -1, 01 = 0, 10 = +1, 11 = +2).

Usage:
  python -I tools/prism_quant_reference.py vectors <ptq1_0|pq2_0> <n> <seed>  # synthetic blocks: (block_hex, values_f32_hex)
  python -I tools/prism_quant_reference.py gguf <file.gguf> <tensor> <rows>    # a real PTQ1_0 tensor's first rows: digest
"""
import hashlib
import json
import random
import struct
import sys

QK = 128
QS = (QK - 4 * QK // 64) // 5  # 24
QH = QK // 64  # 2
BLOCK = QS + QH + 2  # 28
STAGES = (32, 16, 8)
POW3 = (1, 3, 9, 27, 81, 243)


def f16(b):
    return struct.unpack("<e", b)[0]


def dequant_block(blk):
    assert len(blk) == BLOCK
    qs, qh, d = blk[:QS], blk[QS:QS + QH], f16(blk[QS + QH:QS + QH + 2])
    out = []
    j = 0
    for c in STAGES:
        while j + c <= QS:
            for n in range(5):
                for m in range(c):
                    q = (qs[j + m] * POW3[n]) & 0xFF
                    xi = (q * 3) >> 8
                    out.append(float(xi - 1) * d)
            j += c
    for n in range(4):
        for h in range(QH):
            q = (qh[h] * POW3[n]) & 0xFF
            xi = (q * 3) >> 8
            out.append(float(xi - 1) * d)
    assert len(out) == QK
    return out


PQ2_BLOCK = 2 + QK // 4  # 34


def dequant_block_pq2(blk):
    assert len(blk) == PQ2_BLOCK
    d, qs = f16(blk[:2]), blk[2:]
    out = []
    for j in range(QK):
        q = (qs[j // 4] >> ((j % 4) * 2)) & 0x03
        out.append(float(q - 1) * d)
    return out


def f32_bytes(vals):
    # The f32 the C code stores: (float)(xi - 1) * d, d an fp16 widened exactly; the product of +-1 or 0 and d is exact.
    return b"".join(struct.pack("<f", v) for v in vals)


def vectors(kind, n, seed):
    rng = random.Random(seed)
    if kind == "ptq1_0":
        blocks = [bytes([0xFF] * (QS + QH)) + struct.pack("<e", 1.5), bytes(QS + QH) + struct.pack("<e", -0.25)]
        while len(blocks) < n:
            blocks.append(bytes(rng.randrange(256) for _ in range(QS + QH)) + struct.pack("<e", rng.uniform(-8.0, 8.0)))
        dq = dequant_block
    else:
        blocks = [struct.pack("<e", 1.5) + bytes([0xFF] * 32), struct.pack("<e", -0.25) + bytes(32)]
        while len(blocks) < n:
            blocks.append(struct.pack("<e", rng.uniform(-8.0, 8.0)) + bytes(rng.randrange(256) for _ in range(32)))
        dq = dequant_block_pq2
    for b in blocks:
        print(json.dumps({"block_hex": b.hex(), "values_f32_hex": f32_bytes(dq(b)).hex()}))


def gguf_tensor(path, name, rows):
    f = open(path, "rb")

    def rd(fmt):
        k = struct.calcsize(fmt)
        return struct.unpack("<" + fmt, f.read(k))[0]

    def rs():
        k = rd("Q")
        return f.read(k).decode("utf-8", "replace")

    T = {0: "B", 1: "b", 2: "H", 3: "h", 4: "I", 5: "i", 6: "f", 7: "?", 10: "Q", 11: "q", 12: "d"}

    def rv(t):
        if t in T:
            return rd(T[t])
        if t == 8:
            return rs()
        et = rd("I")
        k = rd("Q")
        return [rv(et) for _ in range(k)]

    assert f.read(4) == b"GGUF"
    rd("I")
    nt, nkv = rd("Q"), rd("Q")
    kv = {}
    for _ in range(nkv):
        k = rs()
        kv[k] = rv(rd("I"))
    infos = {}
    for _ in range(nt):
        nm = rs()
        nd = rd("I")
        dims = [rd("Q") for _ in range(nd)]
        ty = rd("I")
        off = rd("Q")
        infos[nm] = (dims, ty, off)
    align = kv.get("general.alignment", 32)
    data0 = (f.tell() + align - 1) // align * align
    dims, ty, off = infos[name]
    assert ty == 143, f"{name} is type {ty}"
    per_row = dims[0] // QK * BLOCK
    rows = min(rows, dims[1] if len(dims) > 1 else 1)
    f.seek(data0 + off)
    raw = f.read(per_row * rows)
    vals = []
    for i in range(0, len(raw), BLOCK):
        vals.extend(dequant_block(raw[i:i + BLOCK]))
    print(json.dumps({"tensor": name, "dims": dims, "rows": rows, "values": len(vals),
                      "f32_sha256": hashlib.sha256(f32_bytes(vals)).hexdigest(), "f32_blake2b": hashlib.blake2b(f32_bytes(vals)).hexdigest(), "first": vals[:8],
                      "counts": {"-1": sum(1 for v in vals if v < 0), "0": sum(1 for v in vals if v == 0), "+1": sum(1 for v in vals if v > 0)}}))


if __name__ == "__main__":
    if sys.argv[1] == "vectors":
        vectors(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]))
    else:
        gguf_tensor(sys.argv[2], sys.argv[3], int(sys.argv[4]))
