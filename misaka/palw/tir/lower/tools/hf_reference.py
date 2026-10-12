#!/usr/bin/env python3
"""The Hugging Face reference a runtime pack holds an integer program to (`palw-class pack build
--hf-reference <dir>`): the float logits `transformers` computes for a few seeded sequences.

    HF_HUB_OFFLINE=1 python tools/hf_reference.py <model dir | .gguf> <out dir>
        [--sequences 2] [--length 16] [--seed 29] [--tokens tokens.json] [--dtype float32]

Writes <out dir>/hf-reference.json (misaka.palw.hf-reference.v1: the producer, the vocabulary, the
sequences) and <out dir>/hf-reference.f32 (their logits, positions x vocab little-endian float32, in
order). Float32 on the CPU by default: the reference is the model's function, not a kernel's rounding.

A GGUF file is loaded through transformers' own GGUF reader (which dequantises it); a model whose
quantisation transformers cannot load without its library (GPTQ, AWQ, compressed-tensors) is referenced
by loading the checkpoint with its tensors dequantised to float (tools/gen_quant_fixtures.py does that
for the fixtures) — the reference is always the float model the integers approximate.

Offline only: refuses to run without HF_HUB_OFFLINE=1.
"""
import argparse
import json
import os
import sys

import numpy as np


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("out")
    ap.add_argument("--sequences", type=int, default=2)
    ap.add_argument("--length", type=int, default=16)
    ap.add_argument("--seed", type=int, default=29)
    ap.add_argument("--tokens", help="JSON {\"sequences\": [[id, ...], ...]} instead of random tokens")
    ap.add_argument("--dtype", default="float32", choices=["float32", "bfloat16", "float16"])
    a = ap.parse_args()

    import torch
    import transformers
    from transformers import AutoModelForCausalLM

    dtype = getattr(torch, a.dtype)
    if a.model.endswith(".gguf"):
        d, f = os.path.split(os.path.abspath(a.model))
        model = AutoModelForCausalLM.from_pretrained(d, gguf_file=f, dtype=dtype, attn_implementation="eager")
    else:
        model = AutoModelForCausalLM.from_pretrained(a.model, dtype=dtype, attn_implementation="eager")
    model.eval()
    vocab = model.get_output_embeddings().weight.shape[0]
    if a.tokens:
        seqs = json.load(open(a.tokens))["sequences"]
    else:
        rng = np.random.default_rng(a.seed)
        seqs = [[int(t) for t in rng.integers(0, vocab, size=a.length)] for _ in range(a.sequences)]
    rows = []
    with torch.no_grad():
        for s in seqs:
            lg = model(input_ids=torch.tensor([s])).logits[0].float().numpy()
            assert lg.shape == (len(s), vocab), lg.shape
            rows.append(lg)
    os.makedirs(a.out, exist_ok=True)
    np.concatenate(rows).astype("<f4").tofile(os.path.join(a.out, "hf-reference.f32"))
    doc = {
        "schema": "misaka.palw.hf-reference.v1",
        "producer": {"transformers": transformers.__version__, "torch": torch.__version__, "dtype": a.dtype, "device": "cpu", "seed": a.seed},
        "vocab": int(vocab),
        "logits_file": "hf-reference.f32",
        "sequences": [{"tokens": s} for s in seqs],
    }
    with open(os.path.join(a.out, "hf-reference.json"), "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {len(seqs)} sequences, {sum(len(s) for s in seqs)} positions, vocab {vocab} to {a.out}")


if __name__ == "__main__":
    main()
