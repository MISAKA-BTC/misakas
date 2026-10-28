#!/usr/bin/env python3
"""FR-09 for GLM-MoE-DSA: a tiny random-init `GlmMoeDsaForCausalLM` and its logits, with the IR's tie rule.

The same shape as `tests/fixtures/fr09/deepseek_v32` (two layers, the first MLP dense, `index_topk` 4 over the same ten tokens,
so the selection selects from the fifth position on), as GLM-MoE-DSA: MLA and indexer rotary INTERLEAVED, every layer's indexer
`full` (cross-layer index sharing is refused by the adapter, not modelled). Weights are seeded random (`torch.manual_seed`), in
float32, saved as safetensors; nothing is downloaded. `GlmMoeDsaIndexer.forward` ends in `index_scores.topk(topk)`, whose order
among equal scores torch leaves unspecified; as for DeepSeek-V3.2 (`gen_dsa_ties_fixture.py`) the stored reference replaces that
one line by a stable descending sort (ties to the lowest index, the IR's rule) and keeps `torch.topk`'s own order beside it:

    logits_full             the lowest-index tie rule (every other operation is transformers')
    logits_full_torch_topk  torch.topk's own order

Usage (offline):
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 OMP_NUM_THREADS=2 python -I tools/gen_glm_dsa_fixture.py tests/fixtures/fr09/glm_moe_dsa
"""

import inspect
import json
import math
import os
import sys
import textwrap

import torch
import transformers
from transformers.models.glm_moe_dsa import modeling_glm_moe_dsa as M

SEED = 20261008
TOKENS = [0, 14, 30, 48, 4, 26, 50, 12, 40, 6]


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    cfg = transformers.GlmMoeDsaConfig(
        vocab_size=64,
        hidden_size=32,
        intermediate_size=64,
        moe_intermediate_size=16,
        num_hidden_layers=2,
        num_attention_heads=4,
        num_key_value_heads=4,
        n_shared_experts=1,
        n_routed_experts=8,
        num_experts_per_tok=2,
        n_group=4,
        topk_group=2,
        routed_scaling_factor=2.5,
        kv_lora_rank=8,
        q_lora_rank=12,
        qk_rope_head_dim=4,
        qk_nope_head_dim=8,
        v_head_dim=8,
        head_dim=4,
        max_position_embeddings=128,
        rms_norm_eps=1e-6,
        index_topk=4,
        index_head_dim=8,
        index_n_heads=2,
        first_k_dense_replace=1,
        mlp_layer_types=["dense", "sparse"],
        indexer_types=["full", "full"],
        rope_parameters={"rope_theta": 10000.0, "rope_type": "default"},
        tie_word_embeddings=False,
        bos_token_id=0,
        eos_token_id=1,
    )
    torch.manual_seed(SEED)
    model = transformers.GlmMoeDsaForCausalLM(cfg).to(torch.float32)
    model.eval()
    # The corpus fixtures' initialisation (tools/gen_hf_fixtures.py `randomise`, copied): every weight live and O(1)-scaled —
    # embeddings N(0, 1), matrices N(0, 1)/sqrt(hidden), vectors and buffers perturbed by N(0, 0.1) — then rounded to bfloat16, so
    # this fixture's integer error is comparable with the DeepSeek-V3.2 one's.
    g = torch.Generator().manual_seed(SEED + 1)
    hidden = cfg.hidden_size
    sd_names = set(model.state_dict().keys())
    with torch.no_grad():
        for name, p in model.named_parameters():
            if not p.is_floating_point():
                continue
            if p.ndim >= 2 and any(h in name for h in ("embed", "wte", "wpe", "word_embeddings", "embeddings.weight", "embed_in")):
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(hidden))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
        for name, b in model.named_buffers():
            if name in sd_names and b.is_floating_point():
                b.add_(torch.randn(b.shape, generator=g) * 0.1)
                b.copy_(b.to(torch.bfloat16).to(torch.float32))
    model.to(torch.bfloat16)
    model.save_pretrained(out, safe_serialization=True)
    model = transformers.GlmMoeDsaForCausalLM.from_pretrained(out, dtype=torch.float32, attn_implementation="eager")
    model.eval()
    tokens = torch.tensor([TOKENS])
    with torch.no_grad():
        torch_topk = model(input_ids=tokens).logits[0]
    src = textwrap.dedent(inspect.getsource(M.GlmMoeDsaIndexer.forward))
    old = "return index_scores.topk(topk, dim=-1).indices.to(torch.int32)"
    assert old in src, "the indexer's last line moved: re-read GlmMoeDsaIndexer.forward"
    src = src.replace(old, "return torch.sort(index_scores, dim=-1, descending=True, stable=True).indices[..., :topk].to(torch.int32)")
    ns = dict(M.__dict__)
    exec(src, ns)
    M.GlmMoeDsaIndexer.forward = ns["forward"]
    with torch.no_grad():
        lowest = model(input_ids=tokens).logits[0]
    diff = (lowest - torch_topk).abs().max(-1).values
    print("max |lowest-index - torch.topk| per position:", [f"{v:.2e}" for v in diff.tolist()])
    meta = {
        "kind": "hf",
        "tokens": TOKENS,
        "seed": SEED,
        "transformers": transformers.__version__,
        "torch": torch.__version__,
        "weights": "bfloat16-exact (rounded before the forward); the corpus fixtures' initialisation, generated offline by tools/gen_glm_dsa_fixture.py",
        "logits_full": lowest.tolist(),
        "logits_full_torch_topk": torch_topk.tolist(),
        "tie_rule": "lowest index (stable descending sort in place of torch.topk); logits_full_torch_topk keeps torch.topk",
    }
    json.dump(meta, open(os.path.join(out, "logits.json"), "w"))


if __name__ == "__main__":
    main()
