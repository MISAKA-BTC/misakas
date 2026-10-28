#!/usr/bin/env python3
"""Generate the Qwen4-Exp (`qwen4_exp_text`) reference fixtures for misaka-palw-tir-lower.

A synthetic, randomly initialised, tiny `Qwen4ExpForCausalLM` per feature combination — never the
published weights (no hub access: run with HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1). Every fixture
is a **combination of the generic features** the lowerer has (gated residual / hyper-connections,
sparse block attention, gated delta net at a key:value head ratio, hashed n-gram per-layer
embedding, a routed + shared MoE): nothing here is a Qwen4 lowering.

For each fixture, as `tools/gen_hf_fixtures.py` does:

    tests/configs/tiny/<name>.json
    tests/fixtures/hf/<name>/config.json
    tests/fixtures/hf/<name>/model.safetensors        (BF16, exact: rounded before the forward)
    tests/fixtures/hf/<name>/logits.json              {"tokens", "logits_full", "logits_decode"?, ...}

and, because the features have internals worth pinning on their own:

    tests/fixtures/hf/<name>/ngram_ids.json   per PLE layer, the ids the module feeds its table
                                              ([position][head]) — the hash vectors `src/ngram.rs` is held to
    tests/fixtures/hf/<name>/qsa_selected.json per QSA layer, per query position, the token indices
                                              the indexer lets the query attend over
                                              (+ the block scores' top-k tie evidence)

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_qwen4_fixtures.py [name ...]
"""

import importlib.util
import json
import math
import os
import sys

import torch

try:
    import transformers
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
except ImportError as e:  # pragma: no cover
    sys.exit(f"transformers is required: {e}")

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
# Written beside every other fixture: the loops over `tests/fixtures/hf` and `tests/configs/tiny`
# (fidelity, admission, three-way, golden, streaming) cover them. QWEN4_TINY / QWEN4_FIX redirect.
TINY = os.path.join(CRATE, "tests", "configs", os.environ.get("QWEN4_TINY", "tiny"))
FIX = os.path.join(CRATE, "tests", "fixtures", os.environ.get("QWEN4_FIX", "hf"))

# Reuse the generic helpers (`randomise`, `decode_logits`) of the other generator.
_spec = importlib.util.spec_from_file_location("gen_hf_fixtures", os.path.join(HERE, "gen_hf_fixtures.py"))
_g = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_g)
randomise, decode_logits = _g.randomise, _g.decode_logits

V = 64
EOS = 2

BASE = dict(
    model_type="qwen4_exp_text",
    architectures=["Qwen4ExpForCausalLM"],
    vocab_size=V,
    hidden_size=32,
    num_hidden_layers=4,
    num_attention_heads=4,
    num_key_value_heads=2,
    head_dim=8,
    linear_key_head_dim=8,
    linear_value_head_dim=8,
    linear_num_key_heads=2,
    linear_num_value_heads=4,
    moe_intermediate_size=16,
    shared_expert_intermediate_size=16,
    num_experts_per_tok=2,
    num_experts=8,
    hc_count=4,
    hc_lowrank=8,
    ple_layer_ids=[1, 2],
    ple_embed_dim=16,
    ngram_size=3,
    heads_per_ngram=2,
    ngram_vocab_size_base=61,
    make_ngram_vocab_size_divisible_by=8,
    seed=1234,
    split_ngram_parts=2,
    indexer_n_heads=2,
    indexer_kv_heads=1,
    indexer_head_dim=8,
    indexer_budget=4,
    indexer_compress_ratio=2,
    rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "partial_rotary_factor": 0.5},
    max_position_embeddings=128,
    eos_token_id=EOS,
    pad_token_id=0,
    bos_token_id=1,
)


def cfg(**kw):
    d = dict(BASE)
    d.update(kw)
    return d


def no_ple(**kw):
    return cfg(ple_layer_ids=[], **kw)


# name -> (config, options). Options: T positions, decode (per-token cached logits too), tokens (explicit
# token list), edge (scale the gate weights so the sigmoids saturate), zero_indexer (all index scores tie).
CONFIGS = {
    # the whole combination: 4 streams, PLE on two layers, QSA with 2-token blocks keeping 2, GDN 1:2, 8 experts top-2
    "qwen4_exp": (cfg(), {"T": 14, "decode": True}),
    # GR-01 / GR-02: one stream (transformers' config validation refuses it, its modules compute it: the
    # config check is bypassed and the logits come from the in-memory model) and two streams
    "qwen4_hc1": (cfg(hc_count=2, ple_layer_ids=[1]), {"T": 10, "hc1": True}),
    "qwen4_hc2": (cfg(hc_count=2), {"T": 10}),
    # GR-03: gate edge values — the mixers' sigmoids and silus driven into saturation
    "qwen4_hc_edge": (cfg(), {"T": 10, "edge": True}),
    # PLE-01 bigram, PLE-02 trigram (the base), PLE-03 hash boundary (ids at the head tables' ends, eos inside n-grams)
    "qwen4_ple_bigram": (cfg(ngram_size=2, heads_per_ngram=2, ple_embed_dim=8), {"T": 14, "decode": True}),
    "qwen4_ple_trigram": (cfg(ngram_size=3, heads_per_ngram=2), {"T": 14, "decode": True}),
    "qwen4_ple_boundary": (cfg(), {"T": 16, "boundary": True, "decode": True}),
    # QSA-01 K=1 (budget = one block), QSA-02 K=max (the budget covers every block), the others come from the
    # positions (QSA-04 an incomplete trailing block, QSA-05 causal boundary at block starts, QSA-06 cached decode)
    "qwen4_qsa_k1": (no_ple(indexer_budget=2, indexer_compress_ratio=2), {"T": 13, "decode": True}),
    "qwen4_qsa_kmax": (no_ple(indexer_budget=128, indexer_compress_ratio=2), {"T": 13, "decode": True}),
    "qwen4_qsa_r3": (no_ple(indexer_budget=6, indexer_compress_ratio=3), {"T": 14, "decode": True}),
    # QSA-03 score tie: an all-zero indexer, every block scores 0
    "qwen4_qsa_tie": (no_ple(), {"T": 12, "zero_indexer": True}),
    # GDN-01..04: key:value head ratios 1:1, 1:2 (the base), 1:3, 1:4 and 2:... arbitrary integer ratios
    "qwen4_gdn_1_1": (no_ple(linear_num_key_heads=2, linear_num_value_heads=2), {"T": 10}),
    "qwen4_gdn_1_3": (no_ple(linear_num_key_heads=2, linear_num_value_heads=6), {"T": 10}),
    "qwen4_gdn_1_4": (no_ple(linear_num_key_heads=1, linear_num_value_heads=4), {"T": 10}),
    "qwen4_gdn_sigmoid_gate": (no_ple(output_gate_type="sigmoid"), {"T": 10}),
    # MoE at the real model's shape: 512 experts, top-10, (tiny experts)
    "qwen4_moe512": (no_ple(num_experts=512, num_experts_per_tok=10, moe_intermediate_size=4, shared_expert_intermediate_size=4),
                     {"T": 10}),
}


def tokens_for(name, opts, T):
    seed = sum(ord(ch) for ch in name)
    if "tokens" in opts:
        return opts["tokens"]
    if opts.get("boundary"):
        # Hash boundary: eos inside n-grams, the largest and smallest ids, a repeated run.
        base = [0, V - 1, V - 1, EOS, 5, EOS, EOS, V - 2, 0, 0, 0, EOS, 17, 31, V - 1, 7]
        return (base * 2)[:T]
    out = [(seed * 7 + 13 * i + i * i) % V for i in range(T)]
    # an eos in the middle so the segment rule is exercised (not as the first token)
    if T > 6:
        out[T // 2] = EOS
    return out


def edge(model):
    """Drive the hyper-connection gates into saturation (sigmoids to 0/1, silus into their tails)."""
    with torch.no_grad():
        for name, p in model.named_parameters():
            if "input_mix_weight" in name or "block_inject_weight" in name:
                p.mul_(7.0)
                p.copy_(p.to(torch.bfloat16).to(torch.float32))


def zero_indexer(model):
    with torch.no_grad():
        for name, p in model.named_parameters():
            if "indexer.index_qk_proj" in name:
                p.zero_()


def install_hooks(model, rec):
    """Record the n-gram ids each PLE module feeds its table and the tokens each indexer selects."""
    hooks = []
    for i, layer in enumerate(model.model.layers):
        if getattr(layer, "ple", None) is not None:
            def pre(mod, args, i=i):
                rec["ngram_ids"][str(i)] = args[0][0].tolist()  # [T][heads]
            hooks.append(layer.ple.ple_embedding.ngram_embedding.register_forward_pre_hook(pre))
        if getattr(layer, "self_attn", None) is not None:
            def post(mod, args, out, i=i):
                m = out[0, 0]  # [T, kv_len]
                keep = (m == 0) if m.dtype != torch.bool else m
                rec["qsa_selected"][str(i)] = [[j for j in range(keep.shape[1]) if bool(keep[t, j])] for t in range(keep.shape[0])]
            hooks.append(layer.self_attn.indexer.register_forward_hook(post))
    return hooks


def make(name, cfg_dict, opts):
    cfg_dict = dict(cfg_dict)
    model_type = cfg_dict.pop("model_type")
    arch = cfg_dict["architectures"]
    seed = sum(ord(ch) for ch in name)
    T = opts.get("T", 10)
    if opts.get("hc1"):
        # `validate_architecture` demands hc_count > 1; the modules themselves compute one stream. The
        # config check is switched off for this fixture (the class is restored afterwards).
        from transformers.models.qwen4_exp.configuration_qwen4_exp import Qwen4ExpTextConfig
        saved_validators = Qwen4ExpTextConfig.__class_validators__
        Qwen4ExpTextConfig.__class_validators__ = [v for v in saved_validators if v.__name__ != "validate_architecture"]
        cfg_dict["hc_count"] = 1
    try:
        return _make(name, model_type, cfg_dict, arch, opts, seed, T)
    finally:
        if opts.get("hc1"):
            Qwen4ExpTextConfig.__class_validators__ = saved_validators


def _make(name, model_type, cfg_dict, arch, opts, seed, T):
    config = CONFIG_MAPPING[model_type](**cfg_dict)
    config.architectures = arch
    torch.manual_seed(seed)
    model = AutoModelForCausalLM.from_config(config)
    model.eval()
    randomise(model, config.hidden_size, seed)
    if opts.get("edge"):
        edge(model)
    if opts.get("zero_indexer"):
        zero_indexer(model)
    ids = torch.tensor([tokens_for(name, opts, T)])
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    for extra in ("generation_config.json",):
        pth = os.path.join(d, extra)
        if os.path.exists(pth):
            os.remove(pth)
    fresh = AutoModelForCausalLM.from_pretrained(d, dtype=torch.float32, attn_implementation="eager")
    fresh.eval()
    rec = {"ngram_ids": {}, "qsa_selected": {}}
    hooks = install_hooks(fresh, rec)
    with torch.no_grad():
        full = fresh(input_ids=ids).logits[0].tolist()
    for h in hooks:
        h.remove()
    dec = None
    if opts.get("decode"):
        with torch.no_grad():
            dec = decode_logits(fresh, ids)
    for row in full:
        if not all(math.isfinite(x) for x in row):
            raise RuntimeError(f"{name}: non-finite logits")
    with open(os.path.join(d, "config.json")) as f:
        saved = json.load(f)
    os.makedirs(TINY, exist_ok=True)
    with open(os.path.join(TINY, f"{name}.json"), "w") as f:
        json.dump(saved, f, indent=2, sort_keys=True)
        f.write("\n")
    meta = {
        "tokens": ids[0].tolist(),
        "logits_full": full,
        "transformers": transformers.__version__,
        "torch": torch.__version__,
        "seed": seed,
        "weights": "bfloat16-exact (rounded before the forward)",
        "attn_implementation": "eager",
    }
    if dec is not None:
        meta["logits_decode"] = dec
        meta["decode_vs_full_max_abs"] = max(abs(a - b) for r1, r2 in zip(full, dec) for a, b in zip(r1, r2))
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    if rec["ngram_ids"]:
        with open(os.path.join(d, "ngram_ids.json"), "w") as f:
            json.dump({"eos": EOS, "vocab": V, "tokens": ids[0].tolist(), "ids": rec["ngram_ids"]}, f)
    if rec["qsa_selected"]:
        with open(os.path.join(d, "qsa_selected.json"), "w") as f:
            json.dump({"tokens": ids[0].tolist(), "ratio": config.indexer_compress_ratio, "budget": config.indexer_budget,
                       "selected": rec["qsa_selected"]}, f)
    return max(abs(x) for row in full for x in row), meta.get("decode_vs_full_max_abs")


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        c, opts = CONFIGS[n]
        try:
            m, dd = make(n, c, opts)
            extra = f"  decode-vs-full {dd:.2e}" if dd is not None else ""
            print(f"ok   {n:24s} max|logit| {m:.3f}{extra}")
        except Exception as e:  # report and continue: one broken config must not hide the rest
            bad += 1
            print(f"FAIL {n:24s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
