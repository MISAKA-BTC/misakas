#!/usr/bin/env python3
"""Tokenise plain-text files with a model's own tokenizer.json into a token file for
`palw-tir-fidelity` (`{"source": ..., "sequences": [[id, ...], ...]}`).

Offline only: reads a local tokenizer.json with the `tokenizers` library, never the hub
(refuses to run without HF_HUB_OFFLINE=1). Each file contributes up to `--chunks` consecutive
chunks of `--len` tokens from its start; the text is used as is (no chat template, no BOS).

    HF_HUB_OFFLINE=1 python tools/tokenize_docs.py --tokenizer DIR/tokenizer.json \
        --len 128 --chunks 1 --out eval.json docs/README.md docs/model-requests.md
"""
import argparse
import hashlib
import json
import os
import sys


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    ap = argparse.ArgumentParser()
    ap.add_argument("--tokenizer", required=True)
    ap.add_argument("--len", type=int, default=128)
    ap.add_argument("--chunks", type=int, default=1)
    ap.add_argument("--out", required=True)
    ap.add_argument("files", nargs="+")
    a = ap.parse_args()
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(a.tokenizer)
    # A tokenizer.json may carry a truncation or padding setting (state-spaces/mamba-370m-hf
    # truncates at 1,024): a document is tokenised whole.
    tok.no_truncation()
    tok.no_padding()
    seqs, source = [], []
    for f in a.files:
        text = open(f, encoding="utf-8").read()
        ids = tok.encode(text, add_special_tokens=False).ids
        n = 0
        for c in range(a.chunks):
            chunk = ids[c * a.len:(c + 1) * a.len]
            if len(chunk) < a.len:
                break
            seqs.append(chunk)
            n += 1
        source.append({"file": f, "sha256": hashlib.sha256(text.encode()).hexdigest(), "tokens": len(ids), "chunks": n})
    json.dump({"source": {"tokenizer": a.tokenizer, "len": a.len, "files": source}, "sequences": seqs}, open(a.out, "w"))
    print(f"{len(seqs)} sequences of {a.len} tokens from {len(a.files)} files -> {a.out}")


if __name__ == "__main__":
    main()
