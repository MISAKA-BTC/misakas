#!/usr/bin/env python3
"""Copy a corpus fixture and apply a lossless weight-layout transformation to it.

    python patch_checkpoint.py ID OUTDIR squeeze REGEX        # drop size-1 dimensions of tensors whose name matches
    python patch_checkpoint.py ID OUTDIR reshape REGEX D0,D1  # reshape matching tensors
    python patch_checkpoint.py ID OUTDIR split_gate_up REGEX  # split a fused [2I, D] tensor into <name>.gate_proj / <name>.up_proj
    python patch_checkpoint.py ID OUTDIR dbrx_experts E I     # DBRX flat experts [E*I, D] -> per-expert tensors (w2 transposed)

Used to answer one question for a feature request: with this ONE storage difference removed (and
nothing else), do the later stages of the harness hold? The result is evidence for the request, not
a Level: the entry stays at C until the generic feature exists.
"""
import json
import os
import re
import shutil
import sys

from safetensors.torch import load_file, save_file

SRC = os.environ.get("PALW_CORPUS_FIXTURES", os.path.expanduser("~/Downloads/MISAKA-wt-b/corpus-fixtures"))


def main():
    id_, out, op, pat = sys.argv[1:5]
    if op == "dbrx_experts":
        pat = r"ffn\.experts\.mlp\.(w1|v1|w2)$"
    d = os.path.join(out, id_)
    if os.path.exists(d):
        shutil.rmtree(d)
    shutil.copytree(os.path.join(SRC, id_), d)
    for fn in os.listdir(d):
        if not fn.endswith(".safetensors"):
            continue
        t = load_file(os.path.join(d, fn))
        for k in list(t):
            if re.search(pat, k):
                if op == "squeeze":
                    t[k] = t[k].reshape([x for x in t[k].shape if x != 1] or [1]).contiguous()
                elif op == "reshape":
                    t[k] = t[k].reshape([int(x) for x in sys.argv[5].split(",")]).contiguous()
                elif op == "dbrx_experts":
                    n_e, inter = int(sys.argv[4]), int(sys.argv[5])
                    w = t.pop(k)
                    base, which = k.rsplit(".", 1)
                    for e in range(n_e):
                        part = w[e * inter:(e + 1) * inter]
                        if which == "w2":
                            part = part.t()  # stored [I, D] per expert: the [out, in] matrix is its transpose
                        t[f"{base}.{e}.{which}.weight"] = part.contiguous()
                    print("split", k, "->", n_e, "experts")
                    continue
                elif op == "split_gate_up":
                    w = t.pop(k)
                    h = w.shape[0] // 2
                    base = k[: -len(".input_linear.weight")] if k.endswith(".input_linear.weight") else k
                    t[base + ".gate_proj.weight"] = w[:h].contiguous()
                    t[base + ".up_proj.weight"] = w[h:].contiguous()
                    print("split", k, "->", base + ".gate_proj/up_proj")
                    continue
                print("patched", k, tuple(t[k].shape))
        save_file(t, os.path.join(d, fn), metadata={"format": "pt"})


if __name__ == "__main__":
    main()
