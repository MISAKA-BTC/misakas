#!/usr/bin/env python3
"""WEIGHT_ROTATION_HADAMARD_V1: a tiny rotated GGUF and its unrotated twin, folded INDEPENDENTLY of the Rust lowering.

From the tiny Qwen3.5 fixture (`tests/fixtures/gguf/gguf_qwen35_q8_0/model.gguf`), dequantised here in pure Python (Q8_0, Q4_0, F32),
two files with the same float weights:

    plain/model.gguf    every tensor F32, no rotation
    rotated/model.gguf  the same model folded as PrismML's `prism.hadamard` v1 declares it (PrismML-Eng/llama.cpp @7dffb158,
                        `build_lora_mm`: y = W'·(H·(s ⊙ x)); the embedding restored as h = s ⊙ (H·z')): every listed weight stored
                        as W' = W·diag(s)·H per block of its input axis, the token table stored as z' = H·(s ⊙ z), the GDN output
                        projection folded in grouped value-head order (gdn_v_grouped), ssm_alpha / ssm_beta NOT rotated (as in the
                        real file): their activation stays unrotated beside the rotated qkv/z of the same input.

H is the normalised Sylvester-Hadamard matrix (H[i][j] = (-1)^popcount(i & j) / sqrt(B), the producer's own formula), B = 32, the
signs seeded. If the Rust reader and lowering apply the declared transform as the producer does, both files compute the same
function (to f32 rounding); that is what tests/weight_rotation.rs checks.

Usage (pure Python, no network):
    python3 -I tools/gen_prism_rotated_fixture.py tests/fixtures/gguf/gguf_qwen35_q8_0/model.gguf tests/fixtures/gguf/prism_rotated
"""
import os
import random
import struct
import sys

B = 32
SEED = 20261008


def f16(b):
    return struct.unpack("<e", b)[0]


class Reader:
    def __init__(self, data):
        self.d, self.i = data, 0

    def take(self, n):
        v = self.d[self.i:self.i + n]
        self.i += n
        return v

    def rd(self, fmt):
        n = struct.calcsize(fmt)
        return struct.unpack("<" + fmt, self.take(n))[0]

    def rs(self):
        return self.take(self.rd("Q")).decode()


SCALAR = {0: "B", 1: "b", 2: "H", 3: "h", 4: "I", 5: "i", 6: "f", 7: "?", 10: "Q", 11: "q", 12: "d"}


def read_value(r, t):
    if t in SCALAR:
        return r.rd(SCALAR[t])
    if t == 8:
        return r.rs()
    et, n = r.rd("I"), r.rd("Q")
    return (et, [read_value(r, et) for _ in range(n)])


def write_value(out, t, v):
    if t in SCALAR:
        out += struct.pack("<" + SCALAR[t], v)
    elif t == 8:
        b = v.encode()
        out += struct.pack("<Q", len(b)) + b
    else:
        et, items = v
        out += struct.pack("<IQ", et, len(items))
        for x in items:
            write_value(out, et, x)


def parse(path):
    data = open(path, "rb").read()
    r = Reader(data)
    assert r.take(4) == b"GGUF"
    version = r.rd("I")
    nt, nkv = r.rd("Q"), r.rd("Q")
    kvs = []
    for _ in range(nkv):
        k = r.rs()
        t = r.rd("I")
        kvs.append((k, t, read_value(r, t)))
    infos = []
    for _ in range(nt):
        name = r.rs()
        nd = r.rd("I")
        dims = [r.rd("Q") for _ in range(nd)]
        ty, off = r.rd("I"), r.rd("Q")
        infos.append((name, dims, ty, off))
    align = dict((k, v) for k, _, v in kvs).get("general.alignment", 32)
    start = (r.i + align - 1) // align * align
    tensors = {}
    for name, dims, ty, off in infos:
        n = 1
        for d in dims:
            n *= d
        raw = data[start + off:]
        if ty == 0:
            vals = list(struct.unpack("<%df" % n, raw[:4 * n]))
        elif ty == 8:  # Q8_0: fp16 d, 32 x int8
            vals = []
            for b in range(n // 32):
                blk = raw[b * 34:(b + 1) * 34]
                d = f16(blk[:2])
                vals.extend(d * q for q in struct.unpack("<32b", blk[2:]))
        elif ty == 2:  # Q4_0: fp16 d, 16 bytes; low nibbles are elements 0..15, high 16..31; value d * (q - 8)
            vals = []
            for b in range(n // 32):
                blk = raw[b * 18:(b + 1) * 18]
                d = f16(blk[:2])
                qs = blk[2:]
                vals.extend(d * ((x & 0xF) - 8) for x in qs)
                vals.extend(d * ((x >> 4) - 8) for x in qs)
        else:
            raise SystemExit(f"{name}: type {ty} is not dequantised here")
        tensors[name] = (dims, vals)
    return version, kvs, [i[0] for i in infos], tensors


def write(path, version, kvs, order, tensors):
    out = bytearray(b"GGUF")
    out += struct.pack("<IQQ", version, len(order), len(kvs))
    for k, t, v in kvs:
        b = k.encode()
        out += struct.pack("<Q", len(b)) + b + struct.pack("<I", t)
        write_value(out, t, v)
    off = 0
    offs = []
    for name in order:
        dims, vals = tensors[name]
        offs.append(off)
        off += 4 * len(vals)
        off = (off + 31) // 32 * 32
    for name, o in zip(order, offs):
        dims, _ = tensors[name]
        b = name.encode()
        out += struct.pack("<Q", len(b)) + b + struct.pack("<I", len(dims)) + b"".join(struct.pack("<Q", d) for d in dims)
        out += struct.pack("<IQ", 0, o)
    out += bytes((-len(out)) % 32)
    for name, o in zip(order, offs):
        _, vals = tensors[name]
        blob = struct.pack("<%df" % len(vals), *vals)
        out += blob + bytes((-len(blob)) % 32)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "wb").write(out)


def h(i, j):
    return -1.0 if bin(i & j).count("1") % 2 else 1.0


def fold_rows(vals, width, signs):
    """Each row (length `width`) times diag(s)·H per block: row'[kB+i] = sum_j row[kB+j] s[kB+j] H[j][i] / sqrt(B)."""
    sc = 1.0 / (B ** 0.5)
    out = []
    for r0 in range(0, len(vals), width):
        row = vals[r0:r0 + width]
        for k in range(width // B):
            for i in range(B):
                out.append(sum(row[k * B + j] * signs[k * B + j] * h(j, i) for j in range(B)) * sc)
    return out


def table_rows(vals, width, signs):
    """Each table row z stored as z' = H·(s ⊙ z) per block (so that s ⊙ (H·z') = z)."""
    sc = 1.0 / (B ** 0.5)
    out = []
    for r0 in range(0, len(vals), width):
        row = vals[r0:r0 + width]
        for k in range(width // B):
            for i in range(B):
                out.append(sum(h(i, j) * signs[k * B + j] * row[k * B + j] for j in range(B)) * sc)
    return out


def main():
    src, outdir = sys.argv[1], sys.argv[2]
    version, kvs, order, tensors = parse(src)
    # The tiny fixture ties its head to the embedding; a rotated table under a tied head is not a model (the head would read the
    # rotated rows), so both files are untied with an explicit output.weight equal to the table (the same function untied).
    if "output.weight" not in tensors:
        tensors["output.weight"] = tensors["token_embd.weight"]
        order = order[:1] + ["output.weight"] + order[1:]
    meta = dict((k, v) for k, _, v in kvs)
    kvs = [(k, t, (0 if k == "general.file_type" else v)) for k, t, v in kvs]
    write(os.path.join(outdir, "plain", "model.gguf"), version, kvs, order, tensors)
    nk, nv = meta["qwen35.ssm.group_count"], meta["qwen35.ssm.time_step_rank"]
    dv = meta["qwen35.ssm.inner_size"] // nv
    rep = nv // nk
    rng = random.Random(SEED)
    widths = sorted({dims[0] for name, (dims, _) in tensors.items() if len(dims) == 2})
    widths = [w for w in widths if w % B == 0]
    signs = {w: [rng.choice((-1, 1)) for _ in range(w)] for w in widths}
    kinds = ("attn_qkv", "attn_gate", "ssm_out", "ffn_gate", "ffn_up", "ffn_down", "attn_q", "attn_k", "attn_v", "attn_output")
    rotated = [n for n in order if n == "output.weight" or (n.startswith("blk.") and n.split(".")[2] in kinds and n.endswith(".weight"))]
    rt = dict(tensors)
    for n in rotated:
        dims, vals = tensors[n]
        width = dims[0]
        if n.endswith("ssm_out.weight"):
            # llama.cpp stores the GDN output projection's columns in tiled value-head order; the fold is computed in grouped order:
            # grouped column i is tiled column (v * nk * dv + k * dv + e) for i = (k, v, e) in grouped order.
            perm = []
            for i in range(nk * rep * dv):
                k, rest = i // (rep * dv), i % (rep * dv)
                v, e = rest // dv, rest % dv
                perm.append(v * nk * dv + k * dv + e)
            vals = [vals[r0 + perm[i]] for r0 in range(0, len(vals), width) for i in range(width)]
        rt[n] = (dims, fold_rows(vals, width, signs[width]))
    dims, vals = tensors["token_embd.weight"]
    rt["token_embd.weight"] = (dims, table_rows(vals, dims[0], signs[dims[0]]))
    flat = []
    for w in widths:
        flat.extend(signs[w])
    extra = [
        ("prism.hadamard.version", 4, 1),
        ("prism.hadamard.block_size", 4, B),
        ("prism.hadamard.transform", 8, "normalized-sylvester-walsh-hadamard"),
        ("prism.hadamard.axis", 8, "input-last-dimension"),
        ("prism.hadamard.sign_mode", 8, "explicit"),
        ("prism.hadamard.weight_names", 9, (8, rotated)),
        ("prism.hadamard.sign_widths", 9, (5, widths)),
        ("prism.hadamard.sign_values", 9, (5, flat)),
        ("prism.hadamard.inverse_weight_names", 9, (8, ["token_embd.weight"])),
        ("prism.hadamard.gdn_v_grouped", 7, True),
    ]
    write(os.path.join(outdir, "rotated", "model.gguf"), version, kvs + extra, order, rt)
    print(f"{len(rotated)} rotated weights, sign widths {widths}, block {B}")


if __name__ == "__main__":
    main()
