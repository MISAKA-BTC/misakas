#!/usr/bin/env python3
"""Generate tiny PEFT-format LoRA adapters over the tiny HF fixtures, with their HF references.

Each entry below names a base fixture (tests/fixtures/hf/<base>) and an adapter:
- targets, rank, alpha, rsLoRA;
- random A ~ N(0, 1/in) and B ~ N(0, 0.2/r)/s, so every adapter changes its weights by about 20%.
The script writes what PEFT would save, without peft installed:

    tests/fixtures/hf-lora/<name>/adapter_config.json
    tests/fixtures/hf-lora/<name>/adapter_model.safetensors   (F32; keys base_model.model.<module>.lora_{A,B}.weight)
    tests/fixtures/hf-lora/<name>/logits.json                 {"base", "tokens", "logits_merged", "logits_base"}

`logits_merged` is transformers' forward with every targeted weight merged, `W + s·B·A` in f32 (the
float original of the candidate). `logits_base` is the parent's.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_hf_lora_fixtures.py [name ...]
"""

import json
import math
import os
import sys

import torch
from safetensors.torch import save_file
from transformers import AutoModelForCausalLM

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
BASE = os.path.join(CRATE, "tests", "fixtures", "hf")
OUT = os.path.join(CRATE, "tests", "fixtures", "hf-lora")

ALL7 = ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"]
CONFIGS = {
    # q/k/v/o and the gated MLP, rank 16, alpha 32 (s = 2).
    "llama_r16": dict(base="llama", targets=ALL7, r=16, alpha=32),
    # "all-linear" over Qwen2 (biased q/k/v), rank 4, alpha 12 (s = 3).
    "qwen2_all_r4": dict(base="qwen2_sliding", targets="all-linear", r=4, alpha=12),
    # Phi's q/k/v/dense and fc1/fc2, rank 64, rsLoRA alpha 16 (s = 16/8 = 2).
    "phi_rs_r64": dict(base="phi", targets=["q_proj", "k_proj", "v_proj", "dense", "fc1", "fc2"], r=64, alpha=16, rslora=True),
    # The common q/v-only adapter, rank 8, a fractional alpha 16.5 (s = 33/16).
    "mistral_qv_r8": dict(base="mistral_window", targets=["q_proj", "v_proj"], r=8, alpha=16.5),
}


def linear_modules(model, targets):
    """Module paths PEFT would target: leaf names, or every Linear under the layers ("all-linear")."""
    out = []
    for name, mod in model.named_modules():
        if not isinstance(mod, torch.nn.Linear) or ".layers." not in f".{name}":
            continue
        leaf = name.rsplit(".", 1)[-1]
        if targets == "all-linear" or leaf in targets:
            out.append(name)
    return out


def make(name):
    c = CONFIGS[name]
    base_dir = os.path.join(BASE, c["base"])
    seed = sum(ord(ch) for ch in name)
    g = torch.Generator().manual_seed(seed)
    model = AutoModelForCausalLM.from_pretrained(base_dir, dtype=torch.float32, attn_implementation="eager")
    model.eval()
    tokens = json.load(open(os.path.join(base_dir, "logits.json")))["tokens"]
    ids = torch.tensor([tokens])
    with torch.no_grad():
        base_logits = model(input_ids=ids).logits[0].tolist()
    r, alpha, rs = c["r"], c["alpha"], c.get("rslora", False)
    s = alpha / (math.sqrt(r) if rs else r)
    tensors = {}
    mods = linear_modules(model, c["targets"])
    assert mods, name
    with torch.no_grad():
        for path in mods:
            mod = model.get_submodule(path)
            out_f, in_f = mod.weight.shape
            a = torch.randn(r, in_f, generator=g) / math.sqrt(in_f)
            b = torch.randn(out_f, r, generator=g) * (0.2 / math.sqrt(r)) / s
            tensors[f"base_model.model.{path}.lora_A.weight"] = a.contiguous()
            tensors[f"base_model.model.{path}.lora_B.weight"] = b.contiguous()
            mod.weight += s * (b @ a)
        merged = model(input_ids=ids).logits[0].tolist()
    d = os.path.join(OUT, name)
    os.makedirs(d, exist_ok=True)
    save_file(tensors, os.path.join(d, "adapter_model.safetensors"))
    cfg = {
        "peft_type": "LORA", "task_type": "CAUSAL_LM", "base_model_name_or_path": f"tests/fixtures/hf/{c['base']}",
        "r": r, "lora_alpha": alpha, "lora_dropout": 0.05, "bias": "none", "fan_in_fan_out": False,
        "use_rslora": rs, "use_dora": False, "target_modules": c["targets"], "modules_to_save": None,
        "layers_to_transform": None, "layers_pattern": None, "rank_pattern": {}, "alpha_pattern": {},
        "init_lora_weights": True, "inference_mode": True, "revision": None,
    }
    with open(os.path.join(d, "adapter_config.json"), "w") as f:
        json.dump(cfg, f, indent=2)
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump({"base": c["base"], "tokens": tokens, "logits_merged": merged, "logits_base": base_logits,
                   "targeted": mods, "scale": s}, f)
    delta = max(abs(x - y) for rm, rb in zip(merged, base_logits) for x, y in zip(rm, rb))
    return len(mods), delta


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            k, dmax = make(n)
            print(f"ok   {n:16s} {k} modules, max |merged - base| logit {dmax:.3f}")
        except Exception as e:  # report and continue
            bad += 1
            print(f"FAIL {n:16s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
