#!/usr/bin/env python3
"""H1 diagnosis: three sets of logits for the same tokens, compared pairwise.

    kl_compare.py --vocab V --positions N  hf=hf-reference.f32  float=float.f32  int=int.f32  [--slice-positions A:B]

Each file is raw little-endian f32, `[positions, vocab]`. The three are: the Hugging Face float32 reference (`hf`), the lowering's own
float reference (`float`, written by `palw-tir-fidelity --exec --logits-out`) and the integer program (`int`, the same flag). For every
pair (a, b) it prints the pack builder's four numbers (slope and correlation of b on a over every logit, top-1 agreement, mean
KL(a || b) in nats) and, when asked, the same over a slice of positions. A large hf-vs-float gap with a small float-vs-int gap
says the lowering's float program is not the checkpoint's (a semantic mismatch, not quantisation noise); the reverse says the integer
quantisation is what is lost. Needs numpy only (stdlib otherwise); run with `python3 -I`.
"""
import argparse
import sys

import numpy as np


def load(path, n, v):
    a = np.fromfile(path, dtype="<f4")
    if a.size != n * v:
        sys.exit(f"{path}: {a.size} values, expected {n} x {v}")
    return a.reshape(n, v).astype(np.float64)


def logsoftmax(x):
    m = x.max(axis=1, keepdims=True)
    z = x - m
    return z - np.log(np.exp(z).sum(axis=1, keepdims=True))


def stats(a, b):
    af, bf = a.ravel(), b.ravel()
    am, bm = af.mean(), bf.mean()
    cov = ((af - am) * (bf - bm)).mean()
    slope = cov / ((af - am) ** 2).mean()
    corr = cov / (af.std() * bf.std())
    top1 = float((a.argmax(axis=1) == b.argmax(axis=1)).mean())
    la, lb = logsoftmax(a), logsoftmax(b)
    kl = float((np.exp(la) * (la - lb)).sum(axis=1).mean())
    return slope, corr, top1, kl, float(np.abs(af - bf).max())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vocab", type=int, required=True)
    ap.add_argument("--positions", type=int, required=True)
    ap.add_argument("--slice-positions", action="append", default=[])
    ap.add_argument("named", nargs="+", help="name=file.f32")
    a = ap.parse_args()
    sets = {}
    for nf in a.named:
        k, f = nf.split("=", 1)
        sets[k] = load(f, a.positions, a.vocab)
    names = list(sets)
    slices = [(0, a.positions)] + [tuple(int(x) for x in s.split(":")) for s in a.slice_positions]
    for lo, hi in slices:
        print(f"positions {lo}..{hi}")
        for i, x in enumerate(names):
            for y in names[i + 1:]:
                s = stats(sets[x][lo:hi], sets[y][lo:hi])
                print(f"  {x:>6} -> {y:<6} slope {s[0]:.4f}  corr {s[1]:.5f}  top-1 {s[2]:.3f}  KL {s[3]:.5f}  max|d| {s[4]:.3f}")


if __name__ == "__main__":
    main()
