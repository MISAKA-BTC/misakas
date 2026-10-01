#!/usr/bin/env python3
"""The census: every causal-LM family of transformers 5.17 that is not already a corpus entry.

The corpus (`corpus_def.py`) is curated: 100 architectures chosen to be representative. The census is the
opposite: no choice. Every `model_type` in `MODEL_FOR_CAUSAL_LM_MAPPING_NAMES` is shrunk to a tiny model
by `gen_fixtures.auto_tiny` (the class defaults with the dimensions cut down) and run through the same
harness, to measure the long tail without selection bias. A family whose tiny model cannot be derived
automatically (the parameter count on the META device is over 20 M, or one tensor is over 64 MB, or the
config needs hand-made sub-configs) is recorded as such, never dropped silently.

SAFETY (incident 2026-10-01): a model is never instantiated to find out its size. Each family is probed in
its OWN subprocess: (1) the model is built on the meta device and its parameters counted; only a tiny one
is then really built and run; (2) the parent polls the child's RSS and kills it above 4 GB or after 120 s.
Two threads, one child at a time.

    HF_HUB_OFFLINE=1 python census_def.py probe  > census_probe.jsonl     # which families derive a tiny model
    python census_def.py manifest                                          # census_v2.json from the probe
"""
import json
import os
import subprocess
import sys
import time
import warnings

warnings.filterwarnings("ignore")
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.setdefault("HF_HUB_OFFLINE", "1")
os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
os.environ.setdefault("OMP_NUM_THREADS", "2")
import corpus_def as C  # noqa: E402

RSS_LIMIT_MB = 4096
TIMEOUT_S = 120

# The tiny default is derived automatically (`gen_fixtures.auto_tiny`); a family whose extra keys (experts, MLA ranks, n-gram
# tables) leave it over the size guard gets explicit overrides here, checked on the META device like everything else.
MLA = dict(head_dim=4, num_key_value_heads=4, kv_lora_rank=8, q_lora_rank=12, qk_rope_head_dim=4, qk_nope_head_dim=8, v_head_dim=8)
MOE8 = dict(n_routed_experts=8, num_experts_per_tok=2, moe_intermediate_size=16)
INDEX = dict(index_n_heads=2, index_head_dim=8, index_topk=4)
OVERRIDES = {
    "axk1": dict(MLA, **MOE8),
    "axk2": dict(MLA, **MOE8, **INDEX),
    "deepseek_v2": dict(MLA, **MOE8),
    "glm_moe_dsa": dict(MLA, **MOE8, **INDEX),
    "hy_v3": dict(num_experts=8, num_experts_per_tok=2, moe_intermediate_size=16),
    "hy_v4": dict(MLA, **MOE8, **INDEX),
    "inkling_text": dict(n_routed_experts=8, moe_intermediate_size=16, swa_head_dim=8, swa_num_attention_heads=4, sliding_window_size=8, rel_extent=16),
    "mimo_v2_flash": dict(MOE8, v_head_dim=8, sliding_window=8),
    "qwen4_exp_text": dict(num_experts=8, moe_intermediate_size=16, shared_expert_intermediate_size=16, hc_lowrank=8, ngram_vocab_size_base=64,
                           split_ngram_parts=4, make_ngram_vocab_size_divisible_by=8, linear_key_head_dim=8, linear_value_head_dim=8),
    "solar_open": dict(MOE8),
    "xlm": dict(emb_dim=32),
    "minicpm3": dict(MLA),
    "youtu": dict(MLA),
    "codegen": dict(rotary_dim=4),
    "gptj": dict(rotary_dim=4),
    "gpt_neo": dict(attention_types=[[["global", "local"], 1]]),
    "bamba": dict(mamba_n_heads=4, mamba_d_head=16),
    "lfm2_moe": dict(num_dense_layers=1),
    "ministral": dict(rope_parameters={"rope_type": "default", "rope_theta": 10000.0}),
    "qwen3_5_text": dict(layer_types=["linear_attention", "full_attention"]),
    "recurrent_gemma": dict(num_hidden_layers=3),
    "zamba": dict(layers_block_type=["mamba", "hybrid"]),
}

# Families the census does not build, with the reason. Never dropped silently.
EXCLUDED = {
    "blenderbot": "the decoder half of an encoder-decoder (its `ForCausalLM` is not a standalone model)",
    "plbart": "the decoder half of an encoder-decoder",
    "reformer": "bidirectional by default; the causal mode needs `is_decoder` and LSH chunking",
    "xmod": "an encoder with per-language adapters; the causal head needs a default language",
    "musicgen_melody": "a composite audio model; the decoder alone is not a text family",
    "gemma3n": "the multimodal wrapper (needs `timm`); its text tower is the corpus entry `gemma3n_text`",
    "gemma4_assistant": "an assistant/drafter head with no vocabulary of its own",
    "gemma4_unified_assistant": "an assistant/drafter head with no vocabulary of its own",
    "llama4": "the multimodal wrapper; its text tower is the corpus entry `llama4_text`",
    "gemma4": "the multimodal wrapper; its text tower is the corpus entry `gemma4_text`",
    "gemma4_unified": "the multimodal wrapper; the text tower is `gemma4_text`",
    "qwen3_5": "the multimodal wrapper; the text tower is `qwen3_5_text`",
    "qwen3_5_moe": "the multimodal wrapper; the text tower is the corpus entry `qwen3_5_moe`",
    "emu3": "an image-generating multimodal model with a nested VQ config",
    "git": "a captioning decoder over a CLIP tower (nested vision config)",
    "got_ocr2": "an OCR VLM with a nested SAM-style tower",
    "phi4_multimodal": "a text+vision+audio model with nested towers",
    "blt": "a byte-latent transformer (n-gram hash tables of 12 GB at the default; its patcher is a second model)",
}


def families():
    from transformers.models.auto.modeling_auto import MODEL_FOR_CAUSAL_LM_MAPPING_NAMES as M
    mine = {e["model_type"] for e in C.ENTRIES}
    return [(mt, M[mt]) for mt in sorted(M) if mt not in mine and mt not in EXCLUDED]


def one(mt):
    """In the child: meta-device size check first, then (only if tiny) a real build and a forward pass."""
    import gen_fixtures as G
    G._lazy()
    import torch
    import transformers
    G.torch, G.transformers = torch, transformers
    t = time.time()
    try:
        cfg = G.auto_tiny(mt, OVERRIDES.get(mt, {}))
        auto = transformers.AutoModelForCausalLM
        n = G.size_guard(lambda: auto.from_config(cfg))
        m = auto.from_config(cfg)
        m.eval()
        ids = torch.tensor([[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]])
        with torch.no_grad():
            m(input_ids=ids)
        rec = {"model_type": mt, "class": type(m).__name__, "ok": True, "params": n, "s": round(time.time() - t, 1)}
    except Exception as e:  # noqa: BLE001
        rec = {"model_type": mt, "ok": False, "error": f"{type(e).__name__}: {str(e)[:160]}".replace("\n", " ")}
    print(json.dumps(rec), flush=True)


def rss_mb(pid):
    try:
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True, timeout=5).stdout.strip()
        return int(out) / 1024 if out else 0
    except Exception:  # noqa: BLE001
        return 0


def run_child(mt, cls):
    p = subprocess.Popen([sys.executable, os.path.abspath(__file__), "one", mt], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    t0 = time.time()
    while p.poll() is None:
        time.sleep(0.25)
        if rss_mb(p.pid) > RSS_LIMIT_MB:
            p.kill()
            p.wait()
            return {"model_type": mt, "class": str(cls), "ok": False, "error": f"killed: RSS over {RSS_LIMIT_MB} MB"}
        if time.time() - t0 > TIMEOUT_S:
            p.kill()
            p.wait()
            return {"model_type": mt, "class": str(cls), "ok": False, "error": f"killed: over {TIMEOUT_S} s"}
    out = p.stdout.read().strip().splitlines()
    for line in reversed(out):
        if line.startswith("{"):
            rec = json.loads(line)
            rec.setdefault("class", str(cls))
            return rec
    return {"model_type": mt, "class": str(cls), "ok": False, "error": f"no result (exit {p.returncode})"}


def probe():
    for mt, cls in families():
        print(json.dumps(run_child(mt, cls)), flush=True)


def reprobe():
    """Re-run only the families that have overrides (their old row said 'not tiny' or a build error), replacing their rows in place."""
    path = os.path.join(HERE, "census_probe.jsonl")
    recs = [json.loads(l) for l in open(path) if l.startswith("{")]
    by = {r["model_type"]: r for r in recs}
    from transformers.models.auto.modeling_auto import MODEL_FOR_CAUSAL_LM_MAPPING_NAMES as M
    for mt in sorted(OVERRIDES):
        if mt in M:
            by[mt] = run_child(mt, M[mt])
            print(json.dumps(by[mt]), flush=True)
    with open(path, "w") as f:
        for mt in sorted(by):
            f.write(json.dumps(by[mt]) + "\n")


def manifest():
    recs = [json.loads(l) for l in open(os.path.join(HERE, "census_probe.jsonl")) if l.startswith("{")]
    recs = [r for r in recs if r["model_type"] not in EXCLUDED]
    entries = []
    for r in recs:
        if not r["ok"]:
            continue
        entries.append(C.entry(r["model_type"].replace("-", "_"), "census/text", "decoder", r["class"], r["model_type"], "causal", OVERRIDES.get(r["model_type"], {}),
                               usage="l", why="a transformers 5.17 causal-LM family at its tiny default (census)", options={}))
    failed = [r for r in recs if not r["ok"]]
    doc = {"schema": "misaka.palw.corpus-v2", "positions": 10, "vocab": C.V, "usage_weight": C.USAGE_WEIGHT, "entries": entries,
           "census": {"families": len(recs), "built": len(entries), "not_derivable": failed,
                      "excluded": [{"model_type": k, "why": v} for k, v in sorted(EXCLUDED.items())]}}
    with open(os.path.join(HERE, "census_v2.json"), "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"census_v2.json: {len(entries)} entries; {len(failed)} of {len(recs)} families have no automatic tiny config")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "one":
        one(sys.argv[2])
    else:
        {"probe": probe, "reprobe": reprobe, "manifest": manifest}[cmd]()
