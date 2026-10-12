#!/usr/bin/env python3
"""FR-09: the DeepSeek-V3.2 logits with the IR's tie rule.

`DeepseekV32Indexer` ends in `index_scores.topk(topk)`, and `torch.topk` leaves the order among equal scores
unspecified (on CPU it returns, for a vector whose seven best scores are all 0.0, the indices [5, 1, 8, 3] and not the
lowest ones). ReLU makes exact-zero scores common, so a tied k-th place is not rare, and the lowering pins the IR's
`TopK` rule instead: ties to the lowest index of the window. This script runs transformers' OWN model on the fixture's
tokens with that single line replaced by a stable descending sort, and stores both references:

    logits_full             the lowest-index tie rule (the lowering's; every other operation is transformers')
    logits_full_torch_topk  `torch.topk`'s own order (it differs from the above exactly where the k-th place is tied)

Usage (offline, one model, tiny):
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 OMP_NUM_THREADS=2 python tools/gen_dsa_ties_fixture.py tests/fixtures/fr09/deepseek_v32
"""

import inspect
import json
import os
import sys
import textwrap

import torch
import transformers
from transformers.models.deepseek_v32 import modeling_deepseek_v32 as M


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    d = sys.argv[1]
    meta = json.load(open(os.path.join(d, "logits.json")))
    tokens = torch.tensor([meta["tokens"]])
    model = transformers.DeepseekV32ForCausalLM.from_pretrained(d, dtype=torch.float32, attn_implementation="eager")
    model.eval()
    with torch.no_grad():
        torch_topk = model(input_ids=tokens).logits[0]
    src = textwrap.dedent(inspect.getsource(M.DeepseekV32Indexer.forward))
    old = "return index_scores.topk(topk, dim=-1).indices.to(torch.int32)"
    assert old in src
    src = src.replace(old, "return torch.sort(index_scores, dim=-1, descending=True, stable=True).indices[..., :topk].to(torch.int32)")
    ns = dict(M.__dict__)
    exec(src, ns)
    M.DeepseekV32Indexer.forward = ns["forward"]
    with torch.no_grad():
        lowest = model(input_ids=tokens).logits[0]
    diff = (lowest - torch_topk).abs().max(-1).values
    print("max |lowest-index - torch.topk| per position:", [f"{v:.2e}" for v in diff.tolist()])
    meta["logits_full_torch_topk"] = torch_topk.tolist()
    meta["logits_full"] = lowest.tolist()
    meta["tie_rule"] = "lowest index (stable descending sort in place of torch.topk); logits_full_torch_topk keeps torch.topk"
    json.dump(meta, open(os.path.join(d, "logits.json"), "w"))


if __name__ == "__main__":
    main()
