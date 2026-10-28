#!/usr/bin/env python3
"""A prefix checkpoint: the first N decoder layers of a local Hugging Face checkpoint, with the embedding, the final norm and the head
(H1 diagnostic -- a STAGED check of a large model, never a class and never evidence of registration).

    make_prefix_checkpoint.py <model dir> <out dir> --layers N

Copies, byte for byte and in the checkpoint's own dtype, the tensors of layers 0..N-1 (and the embedding, the final norm and the head)
into one model.safetensors; config.json is the source's with `num_hidden_layers` and `layer_types` cut to N (a wrapper's text_config
is cut, the rest kept); the tokenizer files are copied. Vision and multi-token-prediction tensors are not copied (no class reads them).
It reads the shards by header offset; nothing is converted. The same layer count, the same tensors, so a fit measured on it is the
fit of that prefix of the real model: a depth bisect for an out-of-tolerance conversion, nothing more.
"""
import argparse
import json
import os
import re
import shutil
import sys

import torch
from safetensors import safe_open
from safetensors.torch import save_file


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("out")
    ap.add_argument("--layers", type=int, required=True)
    a = ap.parse_args()
    cfg = json.load(open(os.path.join(a.model, "config.json")))
    tc = cfg.get("text_config", cfg)
    n_full = tc["num_hidden_layers"]
    if not 1 <= a.layers <= n_full:
        sys.exit(f"--layers must be in 1..{n_full}")
    index = json.load(open(os.path.join(a.model, "model.safetensors.index.json")))["weight_map"]
    keep = {}
    for name, shard in index.items():
        m = re.match(r"^(.*?\.layers)\.(\d+)\.", name)
        if m:
            if int(m.group(2)) < a.layers and "visual" not in name and "mtp" not in name:
                keep[name] = shard
        elif "visual" not in name and not name.startswith("mtp"):
            keep[name] = shard
    tensors = {}
    for shard in sorted(set(keep.values())):
        with safe_open(os.path.join(a.model, shard), framework="pt") as f:
            for name in f.keys():
                if keep.get(name) == shard:
                    tensors[name] = f.get_tensor(name).contiguous()
    os.makedirs(a.out, exist_ok=True)
    save_file(tensors, os.path.join(a.out, "model.safetensors"), metadata={"format": "pt"})
    tc["num_hidden_layers"] = a.layers
    if "layer_types" in tc:
        tc["layer_types"] = tc["layer_types"][: a.layers]
    tc["mtp_num_hidden_layers"] = 0
    json.dump(cfg, open(os.path.join(a.out, "config.json"), "w"), indent=1)
    for f in os.listdir(a.model):
        if f.startswith("tokenizer") or f in ("vocab.json", "merges.txt", "chat_template.jinja", "generation_config.json"):
            shutil.copy2(os.path.join(a.model, f), os.path.join(a.out, f))
    total = sum(t.numel() * t.element_size() for t in tensors.values())
    print(json.dumps({"layers": a.layers, "of": n_full, "tensors": len(tensors), "bytes": total, "dtype": str(next(iter(tensors.values())).dtype)}))


if __name__ == "__main__":
    main()
