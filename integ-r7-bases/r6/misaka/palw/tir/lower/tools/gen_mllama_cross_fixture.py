"""The Mllama cross-attention fixture: transformers' MllamaForConditionalGeneration over a tiny checkpoint with the vision tower
NOT run — `cross_attention_states` (the projected tower rows, hidden_size wide) are a declared input, here seeded random rows
rounded to bf16 — against which the float reference and the integer program are compared (`tests/cross_states.rs`).
Offline, one model per process:  tir-venv/bin/python tools/gen_mllama_cross_fixture.py tests/fixtures/hf/mllama [rows]"""
import json, sys, os
import torch
from transformers import MllamaForConditionalGeneration

d = sys.argv[1]
rows = int(sys.argv[2]) if len(sys.argv) > 2 else 5
m = MllamaForConditionalGeneration.from_pretrained(d, dtype=torch.float32, attn_implementation="eager").eval()
ref = json.load(open(os.path.join(d, "logits.json")))
tokens = ref["tokens"]
g = torch.Generator().manual_seed(2024)
hidden = m.config.text_config.hidden_size
cs = (torch.randn(1, rows, hidden, generator=g) * 0.7).to(torch.bfloat16).to(torch.float32)
with torch.no_grad():
    out = m(input_ids=torch.tensor([tokens]), cross_attention_states=cs, use_cache=False)
json.dump({"tokens": tokens, "rows": rows, "cross_states": cs[0].tolist(), "logits_full": out.logits[0].tolist(),
           "transformers": __import__("transformers").__version__, "seed": 2024}, open(os.path.join(d, "cross.json"), "w"))
print("ok", out.logits.shape)
