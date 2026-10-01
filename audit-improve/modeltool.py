#!/usr/bin/env python3
"""audit-improve/modeltool.py — the drills' model side, OUTSIDE consensus (RFC-0004 A13).

The chain never trains anything: a candidate is a LoRA adapter somebody trained elsewhere and registered as the
head's composite. This script is that somebody for the drills. It needs torch and transformers (the tir-venv:
~/Downloads/MISAKA-wt-b/tir-venv/bin/python), runs offline, and writes only files.

  modeltool.py items   --out items.json --n 16 [--seed S] [--prompt-len 4] [--key-len 3] [--key-alphabet 8] [--vocab 64]
        a synthetic exact-match pool: each prompt is [BOS, a, b, c...] over the free ids (3..vocab-1), each key a
        sequence over a small alphabet of free ids (the first --key-alphabet of them: an adapter this small learns
        a mapping onto few symbols, not onto sixty). The file is a drill setter-set spec's body ({"prompts": [...], "keys": [...]}), so the
        model and the chain read one file.
  modeltool.py keys-from-eval --eval eval.json --out items.json [--items items.json]
        the keys of an item pool become what a class GENERATED for them (palw-class improve eval's output): a pool
        the class passes by construction — the "keep" items a regressing candidate must not break.
  modeltool.py train   --fixture <hf dir> --items items.json --mode win|lose|noop --out <adapter dir>
                       [--rank 8] [--alpha 16] [--steps 1500] [--lr 2e-2] [--margin 3.0] [--seed 7]
        a PEFT-format adapter (adapter_config.json, adapter_model.safetensors; F32; every linear module of every
        layer) over the tiny HF checkpoint, trained with teacher forcing + a logit-margin hinge so the greedy
        decode survives the integer pipeline's quantisation:
          win   greedy decode of len(key) tokens equals the key on every item (the candidate passes them all)
          lose  greedy decode differs from the key at every item (the candidate fails every item)
          noop  B = 0: the parent's own behaviour (a candidate that changes nothing)
        Prints one JSON line: the float model's pass count and the smallest margin.
  modeltool.py merge   --fixture <hf dir> --adapter <adapter dir> --out <merged hf dir>
        the adapter merged into the weights (W + s·B·A, F32): a FULL-WEIGHT candidate's checkpoint, which
        palw-tir-fidelity lowers as a model of its own (RFC-0004 §6.4: a candidate that cannot reuse its
        parent's scales is full weights). The drills use it while a composite class cannot yet prove
        readiness (see audit-improve/README in dm.sh plan).
"""
import argparse
import json
import math
import os
import random
import sys

FREE_LO = 3  # ids 0 (pad), 1 (BOS) and 2 (EOS) are never part of a pool's prompts or keys


def cmd_items(a):
    rng = random.Random(a.seed)
    seen, prompts, keys = set(), [], []
    while len(prompts) < a.n:
        body = tuple(rng.randrange(FREE_LO, a.vocab) for _ in range(a.prompt_len - 1))
        if body in seen:
            continue
        seen.add(body)
        prompts.append([1, *body])
        keys.append([rng.randrange(FREE_LO, FREE_LO + a.key_alphabet) for _ in range(a.key_len)])
    json.dump({"prompts": prompts, "keys": keys}, open(a.out, "w"), indent=1)
    print(f"wrote {a.out}: {a.n} items, prompts of {a.prompt_len}, keys of {a.key_len}")


def cmd_keys_from_eval(a):
    ev = json.load(open(a.eval))
    rows = sorted(ev["results"], key=lambda r: r["item"])
    out = {"prompts": [r["prompt"] for r in rows], "keys": [r["generated"] for r in rows]}
    json.dump(out, open(a.out, "w"), indent=1)
    print(f"wrote {a.out}: {len(rows)} items whose keys are what class {ev['class'][:16]}… generated")


def load_model(fixture):
    import torch
    from transformers import AutoModelForCausalLM

    model = AutoModelForCausalLM.from_pretrained(fixture, dtype=torch.float32, attn_implementation="eager")
    model.eval()
    for p in model.parameters():
        p.requires_grad_(False)
    return model


def linear_modules(model):
    import torch

    return [n for n, m in model.named_modules() if isinstance(m, torch.nn.Linear) and ".layers." in f".{n}"]


def forward(model, overrides, ids):
    from torch.func import functional_call

    return functional_call(model, overrides, (ids,)).logits


def greedy(model, overrides, prompt, steps):
    import torch

    ids = list(prompt)
    for _ in range(steps):
        logits = forward(model, overrides, torch.tensor([ids]))[0, -1]
        ids.append(int(logits.argmax()))
    return ids[len(prompt):]


def cmd_train(a):
    import torch
    from safetensors.torch import save_file

    torch.manual_seed(a.seed)
    model = load_model(a.fixture)
    spec = json.load(open(a.items))
    prompts, keys = spec["prompts"], spec["keys"]
    n, plen, klen = len(prompts), len(prompts[0]), len(keys[0])
    assert all(len(p) == plen for p in prompts) and all(len(k) == klen for k in keys), "uniform item shapes"
    vocab = model.config.vocab_size
    # What the adapter must produce: the keys (win), or something else at every position (lose).
    if a.mode == "lose":
        targets = [[((t - FREE_LO + 17) % (vocab - FREE_LO)) + FREE_LO for t in key] for key in keys]
    else:
        targets = keys
    mods = linear_modules(model)
    s = a.alpha / a.rank
    adapters = {}
    for path in mods:
        w = model.get_submodule(path).weight
        out_f, in_f = w.shape
        A = (torch.randn(a.rank, in_f) / math.sqrt(in_f)).requires_grad_(True)
        B = torch.zeros(out_f, a.rank).requires_grad_(True)
        adapters[path] = (A, B)

    def overrides():
        return {f"{p}.weight": model.get_submodule(p).weight + s * (B @ A) for p, (A, B) in adapters.items()}

    result = {"mode": a.mode, "items": n, "rank": a.rank, "alpha": a.alpha, "modules": len(mods)}
    if a.mode != "noop":
        x = torch.tensor([p + t[:-1] for p, t in zip(prompts, targets)])
        y = torch.tensor(targets)
        params = [t for pair in adapters.values() for t in pair]
        opt = torch.optim.Adam(params, lr=a.lr)
        sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=a.steps, eta_min=a.lr / 20)
        best = (-1, -1e9, None)  # (items right, smallest margin, the adapters then)
        for step in range(a.steps):
            opt.zero_grad()
            logits = forward(model, overrides(), x)[:, plen - 1 :, :]
            ce = torch.nn.functional.cross_entropy(logits.reshape(-1, vocab), y.reshape(-1))
            tgt = logits.gather(-1, y.unsqueeze(-1)).squeeze(-1)
            rest = logits.masked_fill(torch.nn.functional.one_hot(y, vocab).bool(), float("-inf")).max(-1).values
            gap = tgt - rest
            margin = gap.min().item()
            right = int((gap.min(-1).values > 0).sum())
            if (right, margin) > best[:2]:
                best = (right, margin, {k: (A.detach().clone(), B.detach().clone()) for k, (A, B) in adapters.items()})
            if right == n and margin >= a.margin and step % 5 == 0:
                if all(greedy(model, overrides(), p, klen) == t for p, t in zip(prompts, targets)):
                    break
            hinge = torch.relu(a.margin - gap).mean()
            (ce + hinge).backward()
            torch.nn.utils.clip_grad_norm_(params, 5.0)
            opt.step()
            sched.step()
        # The best adapters seen (the last step is not always the best: the hinge makes the loss jumpy).
        with torch.no_grad():
            for k, (A, B) in best[2].items():
                adapters[k][0].copy_(A)
                adapters[k][1].copy_(B)
        result["steps"] = step + 1
        result["min_margin"] = round(best[1], 3)
    with torch.no_grad():
        ov = overrides()
        gens = [greedy(model, ov, p, klen) for p in prompts]
    passes = sum(g == k for g, k in zip(gens, keys))
    result["float_passes_vs_keys"] = passes
    if a.mode == "win" and passes != n:
        sys.exit(f"the win adapter does not pass every item in float ({passes}/{n}): raise --steps or --margin\n{json.dumps(result)}")
    if a.mode == "lose" and passes != 0:
        sys.exit(f"the lose adapter still passes {passes}/{n} items in float\n{json.dumps(result)}")
    os.makedirs(a.out, exist_ok=True)
    tensors = {}
    for path, (A, B) in adapters.items():
        tensors[f"base_model.model.{path}.lora_A.weight"] = A.detach().contiguous()
        tensors[f"base_model.model.{path}.lora_B.weight"] = B.detach().contiguous()
    save_file(tensors, os.path.join(a.out, "adapter_model.safetensors"))
    leaves = sorted({p.rsplit(".", 1)[-1] for p in mods})
    cfg = {
        "peft_type": "LORA", "task_type": "CAUSAL_LM", "base_model_name_or_path": a.fixture,
        "r": a.rank, "lora_alpha": a.alpha, "lora_dropout": 0.0, "bias": "none", "fan_in_fan_out": False,
        "use_rslora": False, "use_dora": False, "target_modules": leaves, "modules_to_save": None,
        "layers_to_transform": None, "layers_pattern": None, "rank_pattern": {}, "alpha_pattern": {},
        "init_lora_weights": True, "inference_mode": True, "revision": None,
    }
    json.dump(cfg, open(os.path.join(a.out, "adapter_config.json"), "w"), indent=2)
    json.dump({"generated": gens, "targets": targets, "keys": keys}, open(os.path.join(a.out, "float-check.json"), "w"))
    print(json.dumps(result))


def cmd_merge(a):
    import shutil

    import torch
    from safetensors import safe_open
    from safetensors.torch import save_file

    cfg = json.load(open(os.path.join(a.adapter, "adapter_config.json")))
    s = cfg["lora_alpha"] / cfg["r"]
    ad = {}
    with safe_open(os.path.join(a.adapter, "adapter_model.safetensors"), "pt") as f:
        for k in f.keys():
            ad[k] = f.get_tensor(k).to(torch.float32)
    tensors = {}
    src = os.path.join(a.fixture, "model.safetensors")
    merged = 0
    with safe_open(src, "pt") as f:
        for k in f.keys():
            w = f.get_tensor(k).to(torch.float32)
            stem = k[: -len(".weight")] if k.endswith(".weight") else None
            if stem is not None:
                ka, kb = f"base_model.model.{stem}.lora_A.weight", f"base_model.model.{stem}.lora_B.weight"
                if ka in ad and kb in ad:
                    w = w + s * (ad[kb] @ ad[ka])
                    merged += 1
            tensors[k] = w.contiguous()
    if merged * 2 != len(ad):
        sys.exit(f"merged {merged} modules but the adapter has {len(ad)} tensors: a tensor was not read")
    os.makedirs(a.out, exist_ok=True)
    save_file(tensors, os.path.join(a.out, "model.safetensors"), metadata={"format": "pt"})
    c = json.load(open(os.path.join(a.fixture, "config.json")))
    c["dtype"] = "float32"
    c["torch_dtype"] = "float32"
    json.dump(c, open(os.path.join(a.out, "config.json"), "w"), indent=2)
    for extra in ("generation_config.json", "tokenizer.json", "tokenizer_config.json"):
        if os.path.exists(os.path.join(a.fixture, extra)):
            shutil.copy(os.path.join(a.fixture, extra), os.path.join(a.out, extra))
    print(json.dumps({"merged_modules": merged, "tensors": len(tensors), "out": a.out}))


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (the drills never download)")
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("items")
    p.add_argument("--out", required=True)
    p.add_argument("--n", type=int, default=16)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--prompt-len", type=int, default=4)
    p.add_argument("--key-len", type=int, default=3)
    p.add_argument("--key-alphabet", type=int, default=8)
    p.add_argument("--vocab", type=int, default=64)
    p.set_defaults(fn=cmd_items)
    p = sub.add_parser("keys-from-eval")
    p.add_argument("--eval", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_keys_from_eval)
    p = sub.add_parser("train")
    p.add_argument("--fixture", required=True)
    p.add_argument("--items", required=True)
    p.add_argument("--mode", choices=["win", "lose", "noop"], required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--rank", type=int, default=8)
    p.add_argument("--alpha", type=float, default=16)
    p.add_argument("--steps", type=int, default=1500)
    p.add_argument("--lr", type=float, default=2e-2)
    p.add_argument("--margin", type=float, default=3.0)
    p.add_argument("--seed", type=int, default=7)
    p.set_defaults(fn=cmd_train)
    p = sub.add_parser("merge")
    p.add_argument("--fixture", required=True)
    p.add_argument("--adapter", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_merge)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
