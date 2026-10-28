#!/usr/bin/env python3
"""End-to-end audit models: every `gen_hf_fixtures.py` configuration rebuilt with its position
limits raised to 1,024 (so a class can be declared at 512 positions), random bf16-exact weights, and
the Hugging Face references a class is held to:

    <out>/<name>/config.json, model.safetensors (BF16)
    <out>/<name>/hf.json        {"records": [{"input_ids", "generated"}], "sequences": [[id, ...]]}
    <out>/<name>/hf-logits.f32  teacher-forced logits over "sequences", f32 LE [positions, vocab]
    <out>/<name>/calib.json     {"sequences": [...]}: 2 random sequences of 512 (a recurrent program
                                is calibrated at the context it is declared at)

Greedy is `generate(do_sample=False)` with no EOS stop. Configurations whose rope depends on the
sequence (dynamic NTK, LongRoPE) are teacher-forced a token at a time through the cache (the
per-position semantics); the rest in one forward.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/audit_e2e.py OUT_DIR [name ...]
"""

import json
import os
import sys

import numpy as np
import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gen_hf_fixtures as g  # noqa: E402

POS_KEYS = ("max_position_embeddings", "n_positions", "n_ctx", "max_seq_len")
PROMPTS, PROMPT_LEN, NEW = 3, 12, 12
SEQS, SEQ_LEN = 2, 48


def raise_positions(d):
    """Raise every position limit (top level and in a nested text config) to at least 1,024."""
    out = dict(d)
    for k in POS_KEYS:
        if k in out and isinstance(out[k], int) and out[k] < 1024:
            out[k] = 1024
    for k in ("text_config",):
        if isinstance(out.get(k), dict):
            out[k] = raise_positions(out[k])
    return out


def make(out_root, name):
    cfg_dict, opts = g.CONFIGS[name]
    cfg_dict = raise_positions(dict(cfg_dict))
    model_type = cfg_dict.pop("model_type")
    arch = cfg_dict["architectures"]
    seed = sum(ord(ch) for ch in name) + 1000
    cfg = g.CONFIG_MAPPING[model_type](**cfg_dict)
    cfg.architectures = arch
    torch.manual_seed(seed)
    auto = g.AutoModelForImageTextToText if opts.get("vlm") else g.AutoModelForCausalLM
    model = auto.from_config(cfg)
    model.eval()
    tc = cfg.get_text_config()
    hidden = getattr(tc, "hidden_size", None) or getattr(tc, "n_embd", None) or getattr(tc, "d_model")
    g.randomise(model, hidden, seed)
    d = os.path.join(out_root, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    for extra in ("generation_config.json",):
        p = os.path.join(d, extra)
        if os.path.exists(p):
            os.remove(p)
    fresh = auto.from_pretrained(d, dtype=torch.float32, attn_implementation=opts.get("attn", "eager"))
    fresh.eval()
    vocab = tc.vocab_size
    rng = np.random.default_rng(seed)
    fresh.generation_config.eos_token_id = None
    gc = g.transformers.GenerationConfig(max_new_tokens=NEW, do_sample=False, num_beams=1, eos_token_id=None,
                                         pad_token_id=0)
    recs = []
    with torch.no_grad():
        for _ in range(PROMPTS):
            ids = [int(t) for t in rng.integers(0, vocab, size=PROMPT_LEN)]
            t = torch.tensor([ids])
            gen = fresh.generate(input_ids=t, attention_mask=torch.ones_like(t), generation_config=gc)[0, PROMPT_LEN:].tolist()
            # HF's top-2 margin at every generated step (a departure there is judged against it).
            st = torch.tensor([ids + gen[:-1]])
            lg = g.decode_logits(fresh, st) if opts.get("decode") else fresh(input_ids=st).logits[0].tolist()
            margins = []
            for row in lg[PROMPT_LEN - 1:]:
                top = sorted(row, reverse=True)[:2]
                margins.append(top[0] - top[1])
            recs.append({"input_ids": ids, "generated": gen, "margins": margins})
        seqs = [[int(t) for t in rng.integers(0, vocab, size=SEQ_LEN)] for _ in range(SEQS)]
        rows = []
        for s in seqs:
            ids = torch.tensor([s])
            if opts.get("decode"):
                rows.extend(g.decode_logits(fresh, ids))
            else:
                rows.extend(fresh(input_ids=ids).logits[0].tolist())
    np.asarray(rows, dtype="<f4").tofile(os.path.join(d, "hf-logits.f32"))
    json.dump({"records": recs, "sequences": seqs, "vocab": vocab, "transformers": g.transformers.__version__},
              open(os.path.join(d, "hf.json"), "w"))
    calib = [[int(t) for t in rng.integers(0, vocab, size=512)] for _ in range(2)]
    json.dump({"source": "random (audit)", "sequences": calib}, open(os.path.join(d, "calib.json"), "w"))
    return recs


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    out = sys.argv[1]
    names = sys.argv[2:] or list(g.CONFIGS)
    bad = 0
    for n in names:
        try:
            recs = make(out, n)
            print(f"ok   {n:20s} {[r['generated'][:6] for r in recs]}", flush=True)
        except Exception as e:  # report and continue
            bad += 1
            print(f"FAIL {n:20s} {type(e).__name__}: {str(e)[:300]}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
