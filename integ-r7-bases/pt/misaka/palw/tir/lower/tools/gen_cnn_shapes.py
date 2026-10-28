#!/usr/bin/env python3
"""The tensor names and shapes of real convolutional checkpoints, from transformers' own model code on the META device (no
weights are allocated or downloaded): `tests/configs/cnn/<name>.shapes.json` is `{tensor name: shape}` of the model the
config describes — what a `.safetensors` header of that checkpoint would list. `tests/cnn.rs` holds the data adapters' reading
of each real config against it: every tensor accounted for, every parameter found at the shape the lowering needs.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_cnn_shapes.py [config-name ...]
"""

import json
import os
import sys

import torch
import transformers

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIGS = os.path.join(os.path.dirname(HERE), "tests", "configs", "cnn")
SKIP = ("resnet",)  # the ResNet configs predate this tool (their fixtures cover the structure)


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    names = sys.argv[1:] or sorted(f[:-5] for f in os.listdir(CONFIGS) if f.endswith(".json") and not f.endswith(".shapes.json"))
    for name in names:
        if name.startswith(SKIP):
            continue
        cfg_json = json.load(open(os.path.join(CONFIGS, name + ".json")))
        arch = cfg_json["architectures"][0]
        cfg = transformers.AutoConfig.for_model(cfg_json["model_type"], **{k: v for k, v in cfg_json.items() if k not in ("model_type", "architectures")})
        cls = getattr(transformers, arch)
        with torch.device("meta"):
            model = cls(cfg)
        shapes = {k: list(v.shape) for k, v in model.state_dict().items()}
        with open(os.path.join(CONFIGS, name + ".shapes.json"), "w") as f:
            json.dump(shapes, f, indent=0, sort_keys=True)
        print(f"ok {name:26s} {len(shapes)} tensors, {sum(__import__('math').prod(s) for s in shapes.values()) / 1e6:.1f} M elements")


if __name__ == "__main__":
    main()
