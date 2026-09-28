#!/usr/bin/env python3
"""Generate Hugging Face reference fixtures for the encoder lowering (RFC-0003 Part II.3).

Each tiny configuration below is built with random weights, re-randomised and rounded to bfloat16
exactly as `gen_hf_fixtures.py` does (its `randomise`). The model is saved, reloaded fresh in f32
with eager attention, and run on fixed token sequences. The outputs are recorded:

    tests/fixtures/hf-enc/<name>/config.json
    tests/fixtures/hf-enc/<name>/model.safetensors   (BF16)
    tests/fixtures/hf-enc/<name>/outputs.json        {"sequences": [{"tokens", "hidden", "pooled",
                                                      "embeds"}], "kind", versions, seed}

`hidden` is the model's last hidden state per position, `pooled` is its pooled row, and `embeds`
is the class output: CLIP's `text_embeds`, or the sentence-transformers pooling and normalisation
applied by hand.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_hf_encoder_fixtures.py [name ...]
"""

import json
import os
import sys

import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from gen_hf_fixtures import randomise  # noqa: E402

import transformers  # noqa: E402

CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-enc")
V = 64

# name -> (config class, model class, config kwargs, sequences, kind)
CONFIGS = {
    # CLIP's text tower with its projection: causal, learned positions, quick_gelu; bos 62, eos 63.
    "clip_text": (
        "CLIPTextConfig",
        "CLIPTextModelWithProjection",
        dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
             max_position_embeddings=16, projection_dim=24, hidden_act="quick_gelu", bos_token_id=62,
             eos_token_id=63, pad_token_id=1, layer_norm_eps=1e-5),
        [[62, 5, 17, 33, 8, 63], [62, 40, 2, 29, 11, 50, 7, 21, 63]],
        "clip_text",
    ),
    # Bidirectional encoders over a padded token axis (L_max = 12), sentence-transformers pooling
    # applied by hand: masked mean (and CLS), then L2 normalisation. [CLS] 2, [SEP] 3.
    "bert": (
        "BertConfig",
        "BertModel",
        dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
             max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu", layer_norm_eps=1e-12,
             pad_token_id=0),
        [[2, 11, 25, 7, 3], [2, 40, 9, 17, 33, 21, 8, 3]],
        "bidir:0:12",
    ),
    # RoBERTa: positions start at padding_idx + 1 = 2; one token type. <s> 0, </s> 2, <pad> 1.
    "roberta": (
        "RobertaConfig",
        "RobertaModel",
        dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
             max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5,
             pad_token_id=1, bos_token_id=0, eos_token_id=2),
        [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 2]],
        "bidir:1:12",
    ),
    # XLM-R: RoBERTa's model under its own class (the tokenizer is what differs).
    "xlm_roberta": (
        "XLMRobertaConfig",
        "XLMRobertaModel",
        dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=3, num_attention_heads=4,
             max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5,
             pad_token_id=1, bos_token_id=0, eos_token_id=2),
        [[0, 5, 2], [0, 40, 9, 17, 33, 21, 8, 30, 12, 2]],
        "bidir:1:12",
    ),
}


def make(name):
    cfg_cls, model_cls, kw, seqs, kind = CONFIGS[name]
    seed = sum(ord(ch) for ch in name)
    torch.manual_seed(seed)
    cfg = getattr(transformers, cfg_cls)(**kw)
    extra = {"add_pooling_layer": False} if kind.startswith("bidir") else {}
    model = getattr(transformers, model_cls)(cfg, **extra)
    model.eval()
    randomise(model, cfg.hidden_size, seed)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    fresh = getattr(transformers, model_cls).from_pretrained(d, dtype=torch.float32, attn_implementation="eager")
    fresh.eval()
    out = []
    with torch.no_grad():
        for s in seqs:
            if kind == "clip_text":
                o = fresh(input_ids=torch.tensor([s]))
                rec = {"tokens": s, "hidden": o.last_hidden_state[0].tolist(),
                       "embeds": o.text_embeds[0].tolist()}
            elif kind.startswith("bidir"):
                _, pad, lmax = kind.split(":")
                pad, lmax = int(pad), int(lmax)
                ids = torch.tensor([s + [pad] * (lmax - len(s))])
                mask = torch.tensor([[1] * len(s) + [0] * (lmax - len(s))])
                o = fresh(input_ids=ids, attention_mask=mask)
                h = o.last_hidden_state[0]
                m = mask[0].unsqueeze(-1).float()
                mean = (h * m).sum(0) / m.sum(0).clamp(min=1e-9)
                rec = {"tokens": s, "padded": ids[0].tolist(), "count": len(s), "hidden": h.tolist(),
                       "cls": h[0].tolist(), "mean": mean.tolist(),
                       "mean_normalized": torch.nn.functional.normalize(mean, p=2, dim=0).tolist(),
                       "cls_normalized": torch.nn.functional.normalize(h[0], p=2, dim=0).tolist(),
                       "embeds": torch.nn.functional.normalize(mean, p=2, dim=0).tolist()}
            else:
                raise RuntimeError(kind)
            out.append(rec)
    meta = {"kind": kind, "sequences": out, "transformers": transformers.__version__, "torch": torch.__version__,
            "seed": seed, "weights": "bfloat16-exact (rounded before the forward)", "attn_implementation": "eager"}
    with open(os.path.join(d, "outputs.json"), "w") as f:
        json.dump(meta, f)
    return max(abs(x) for r in out for x in r["embeds"])


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            m = make(n)
            print(f"ok   {n:22s} max|embed| {m:.3f}")
        except Exception as e:  # report and continue
            bad += 1
            print(f"FAIL {n:22s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
