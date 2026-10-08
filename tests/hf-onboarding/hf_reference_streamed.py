#!/usr/bin/env python3
"""The Hugging Face reference of a model too large to hold in RAM, one decoder layer at a time (H1; the 10 GB-RSS watchdog).

    HF_HUB_OFFLINE=1 python -I hf_reference_streamed.py <model dir> <out dir> --tokens tokens.json [--dtype float32]

Writes exactly what misaka-palw-tir-lower/tools/hf_reference.py writes (misaka.palw.hf-reference.v1: hf-reference.json +
hf-reference.f32, positions x vocab little-endian float32), computed by transformers' OWN modules: the CausalLM is built on the meta
device from the checkpoint's config; the rotary embedding and the final norm are materialised; each decoder layer is materialised from
the safetensors (bf16 -> --dtype) just before its forward and returned to the meta device right after; the embedding gathers only the
rows of the given tokens; the head computes the logits in row blocks of the head matrix read from the file. The forward itself is the
library's (masks, rotary, GDN/attention, MLP), so the numbers are the model's function, not a re-implementation. Peak RSS is about one
layer plus one head block plus the activations. A model whose decoder keys cannot be mapped to the checkpoint is refused, never guessed.
"""
import argparse
import json
import os
import re
import sys
import time

import numpy as np


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("out")
    ap.add_argument("--tokens", required=True)
    ap.add_argument("--dtype", default="float32", choices=["float32", "bfloat16"])
    ap.add_argument("--head-block", type=int, default=16384)
    a = ap.parse_args()

    import torch
    import transformers
    from safetensors import safe_open
    from transformers import AutoConfig, AutoModelForCausalLM

    dtype = getattr(torch, a.dtype)
    torch.set_grad_enabled(False)
    cfg = AutoConfig.from_pretrained(a.model)
    idx_path = os.path.join(a.model, "model.safetensors.index.json")
    if os.path.exists(idx_path):
        weight_map = json.load(open(idx_path))["weight_map"]
    else:
        f = next(n for n in os.listdir(a.model) if n.endswith(".safetensors"))
        with safe_open(os.path.join(a.model, f), "pt") as h:
            weight_map = {k: f for k in h.keys()}
    handles = {}

    def tensor(key):
        f = weight_map[key]
        if f not in handles:
            handles[f] = safe_open(os.path.join(a.model, f), "pt")
        return handles[f].get_tensor(key)

    def rows(key, lo, hi):
        f = weight_map[key]
        if f not in handles:
            handles[f] = safe_open(os.path.join(a.model, f), "pt")
        return handles[f].get_slice(key)[lo:hi]

    with torch.device("meta"):
        model = AutoModelForCausalLM.from_config(cfg, dtype=dtype, attn_implementation="eager")
    model.eval()
    text = model.model
    tcfg = text.config
    # Map every module key of the text model to exactly one checkpoint key (decoder layers, norm, embedding, head).
    ck_text = [k for k in weight_map if not k.startswith("mtp") and ".visual." not in k and not k.startswith("model.visual")]

    def ck_key(mod_key):
        # mod_key: "model.layers.3.mlp.up_proj.weight" -> the checkpoint key with the same tail after the text prefix
        tail = mod_key.split("model.", 1)[1] if mod_key.startswith("model.") else mod_key
        hits = [k for k in ck_text if k == mod_key or k.endswith("." + tail) or k == tail]
        hits = [k for k in hits if re.sub(r"^(model\.language_model\.|model\.)", "", k) == tail]
        if len(hits) != 1:
            raise SystemExit(f"cannot map {mod_key} to one checkpoint key (found {hits[:4]})")
        return hits[0]

    # Rotary embedding and final norm: small, materialised on the CPU.
    rot_cls = type(text.rotary_emb)
    text.rotary_emb = rot_cls(config=tcfg)
    text.norm.to_empty(device="cpu")
    text.norm.weight.copy_(tensor(ck_key("model.norm.weight")).to(dtype))

    seqs = json.load(open(a.tokens))["sequences"]
    vocab = int(tcfg.vocab_size)
    head_key = "lm_head.weight" if "lm_head.weight" in weight_map else ck_key("model.embed_tokens.weight")
    emb_key = ck_key("model.embed_tokens.weight")

    class GatherEmbedding(torch.nn.Module):
        def forward(self, ids):
            flat = ids.reshape(-1).tolist()
            out = torch.stack([rows(emb_key, t, t + 1)[0].to(dtype) for t in flat])
            return out.reshape(*ids.shape, -1)

    text.embed_tokens = GatherEmbedding()

    def load_layer(i, layer):
        layer.to_empty(device="cpu")
        sd = {}
        for name, p in list(layer.named_parameters()) + list(layer.named_buffers()):
            key = ck_key(f"model.layers.{i}.{name}")
            sd[name] = tensor(key).to(p.dtype if p.dtype.is_floating_point else p.dtype)
        missing, unexpected = layer.load_state_dict(sd, strict=True, assign=False), None
        return layer

    for i, layer in enumerate(text.layers):
        orig = layer.forward

        def fwd(*args, _i=i, _layer=layer, _orig=orig, **kw):
            t0 = time.time()
            load_layer(_i, _layer)
            out = _orig(*args, **kw)
            _layer.to("meta")
            print(f"  layer {_i:3d} {time.time() - t0:6.1f}s", file=sys.stderr, flush=True)
            return out

        layer.forward = fwd

    logits_rows = []
    t_all = time.time()
    for s in seqs:
        ids = torch.tensor([s])
        out = text(input_ids=ids, use_cache=False)
        h = out.last_hidden_state[0].to(torch.float32)  # positions x hidden
        lg = torch.empty((h.shape[0], vocab), dtype=torch.float32)
        for lo in range(0, vocab, a.head_block):
            hi = min(vocab, lo + a.head_block)
            w = rows(head_key, lo, hi).to(dtype).to(torch.float32)
            lg[:, lo:hi] = h @ w.T
        logits_rows.append(lg.numpy())
        print(f"sequence of {len(s)} tokens done ({time.time() - t_all:.0f}s)", file=sys.stderr, flush=True)
    os.makedirs(a.out, exist_ok=True)
    np.concatenate(logits_rows).astype("<f4").tofile(os.path.join(a.out, "hf-reference.f32"))
    doc = {
        "schema": "misaka.palw.hf-reference.v1",
        "producer": {"transformers": transformers.__version__, "torch": torch.__version__, "dtype": a.dtype, "device": "cpu",
                     "streamed": "one decoder layer at a time (tests/hf-onboarding/hf_reference_streamed.py)"},
        "vocab": vocab,
        "logits_file": "hf-reference.f32",
        "sequences": [{"tokens": s} for s in seqs],
    }
    with open(os.path.join(a.out, "hf-reference.json"), "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {len(seqs)} sequences, {sum(len(s) for s in seqs)} positions, vocab {vocab} to {a.out}")


if __name__ == "__main__":
    main()
