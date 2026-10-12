#!/usr/bin/env python3
"""**Build a tiny reference from a remote-code architecture and PIN the code it came from** (FR-24, `REFERENCE_REMOTE_CODE_V1`).

An architecture whose forward lives in the model repository (`trust_remote_code`: ChatGLM3, InternLM2, ...) has no semantics pinned to a library version, so a
lowering of it cannot be checked against `transformers`. What a registrant can offer is a PIN: the sha256 of the modelling files of the checkout the model was
written against, and a tiny random-init model THOSE FILES produce, with the logits they give. This script is that flow, offline and in one process:

    HF_HUB_OFFLINE=1 python tools/remote_reference.py CODE_DIR CONFIG.json OUT_DIR [--seed N] [--tokens N]

* CODE_DIR holds the repository's `*.py` (its `configuration_*.py` and `modeling_*.py`); CONFIG.json is a TINY `config.json` with the repository's `auto_map`
  (the real keys, small dimensions). The files are copied beside the config and loaded through `AutoConfig` / `AutoModelForCausalLM` with
  `trust_remote_code=True`, from the local directory only. Nothing is downloaded.
* The model is built on the meta device first (a model that is not tiny is refused), randomised so every parameter is live, rounded to bfloat16, saved, RELOADED fresh
  in float32, and run on fixed tokens: `OUT_DIR/{config.json, model.safetensors, reference.json}` in the layout `tests/corpus_v2.rs` reads.
* `OUT_DIR/pin.json` records `{"files": {name: sha256}, "model_class", "transformers", "torch"}`. An adapter that declares `"remote_code_pin": {"<module>":
  "<sha256 of the module's file>"}` states which code its lowering follows; `check-architecture` prints it, and a verdict that rests on a pinned file says so
  (`LOWERABLE_UNVERIFIED (remote code ..., pinned to sha256 ...)`).

What the pin is NOT: an attestation. The hash says which file the registrant's fixture came from; it does not say the file is the repository's. The verdict stays
LOWERABLE_UNVERIFIED — the chain has no way to run remote code — and a registrant's fixture, run by the registrant, is the evidence the lowering follows that file.
The corpus entry `chatglm3` runs this on `tools/corpus/remote/chatglm3/`, a RECONSTRUCTION written from the documented forward (the repository is not available
offline), and says so in its report (`float_vs_hf.reference_unverified`).
"""

import argparse
import hashlib
import json
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "corpus"))

T = 10  # positions of the reference
MAX_PARAMS = 20_000_000


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build(code_dir, config_path, out_dir, seed=None, vocab=None, ids=None):
    """Build the tiny model from the code in `code_dir`, write the fixture and the pin into `out_dir`. Returns a short summary."""
    import torch
    import transformers
    from transformers import AutoConfig, AutoModelForCausalLM

    torch.set_num_threads(2)
    os.makedirs(out_dir, exist_ok=True)
    files = sorted(f for f in os.listdir(code_dir) if f.endswith(".py"))
    if not files:
        raise RuntimeError(f"{code_dir} holds no .py files")
    for f in files:
        shutil.copy(os.path.join(code_dir, f), os.path.join(out_dir, f))
    with open(config_path) as f:
        cfg_json = json.load(f)
    if "auto_map" not in cfg_json:
        raise RuntimeError("the config has no auto_map: this is not a remote-code architecture")
    with open(os.path.join(out_dir, "config.json"), "w") as f:
        json.dump(cfg_json, f, indent=2, sort_keys=True)
    cfg = AutoConfig.from_pretrained(out_dir, trust_remote_code=True)
    seed = seed if seed is not None else sum(ord(c) for c in os.path.basename(os.path.normpath(out_dir)))
    torch.manual_seed(seed)
    # A model that is not tiny is refused BEFORE it is built (the 2026-10-01 incident: never instantiate to discover a size).
    with torch.device("meta"):
        m = AutoModelForCausalLM.from_config(cfg, trust_remote_code=True)
    n = sum(p.numel() for p in m.parameters())
    del m
    if n > MAX_PARAMS:
        raise RuntimeError(f"not tiny: {n:,} parameters")
    model = AutoModelForCausalLM.from_config(cfg, trust_remote_code=True)
    model.eval()
    import gen_fixtures as G  # tools/corpus/gen_fixtures.py: the corpus's randomiser

    G.torch = torch
    hidden = getattr(cfg, "hidden_size", None) or 32
    G.randomise(model, hidden, seed)
    model.to(torch.bfloat16)
    model.save_pretrained(out_dir)
    for f in files:  # save_pretrained may rewrite the code files; the pin is of what the registrant supplied
        shutil.copy(os.path.join(code_dir, f), os.path.join(out_dir, f))
    fresh = AutoModelForCausalLM.from_pretrained(out_dir, trust_remote_code=True, dtype=torch.float32)
    fresh.eval()
    vocab = vocab or getattr(cfg, "vocab_size", None) or getattr(cfg, "padded_vocab_size", 64)
    tokens = ids or [(seed * 7 + 13 * i + i * i) % vocab for i in range(T)]
    with torch.no_grad():
        full = fresh(input_ids=torch.tensor([tokens])).logits[0].tolist()
    if not all(x == x and abs(x) != float("inf") for r in full for x in r):
        raise RuntimeError("non-finite logits")
    with open(os.path.join(out_dir, "reference.json"), "w") as f:
        json.dump({"kind": "causal_lm", "tokens": tokens, "logits_full": full, "seed": seed, "transformers": transformers.__version__,
                   "torch": torch.__version__, "weights": "bfloat16-exact (rounded before the forward)",
                   "reference": "remote code (trust_remote_code, local files)"}, f)
    pin = {"files": {f: sha256_file(os.path.join(code_dir, f)) for f in files}, "model_class": type(fresh).__name__,
           "transformers": transformers.__version__, "torch": torch.__version__}
    with open(os.path.join(out_dir, "pin.json"), "w") as f:
        json.dump(pin, f, indent=1, sort_keys=True)
        f.write("\n")
    return {"model": type(fresh).__name__, "max_logit": round(max(abs(x) for r in full for x in r), 2), "params": n, "pin": pin["files"]}


def check(code_dir, fixture_dir):
    """Does `fixture_dir/pin.json` name the files now in `code_dir`? Returns the list of mismatches (empty: the fixture is of this code)."""
    with open(os.path.join(fixture_dir, "pin.json")) as f:
        pin = json.load(f)["files"]
    bad = []
    for name, want in pin.items():
        p = os.path.join(code_dir, name)
        got = sha256_file(p) if os.path.exists(p) else None
        if got != want:
            bad.append(f"{name}: pinned {want}, now {got}")
    for name in sorted(f for f in os.listdir(code_dir) if f.endswith(".py")):
        if name not in pin:
            bad.append(f"{name}: not in the pin")
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("code_dir")
    ap.add_argument("config", help="the tiny config.json (with --check: the fixture directory holding pin.json)")
    ap.add_argument("out_dir", nargs="?")
    ap.add_argument("--seed", type=int)
    ap.add_argument("--check", action="store_true", help="verify that a fixture's pin.json names the files now in CODE_DIR (no torch, no model)")
    a = ap.parse_args()
    if a.check:
        bad = check(a.code_dir, a.config)
        print("\n".join(bad) if bad else "pin matches")
        sys.exit(1 if bad else 0)
    if not a.out_dir:
        sys.exit("OUT_DIR is required")
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (the code and the config are local; nothing is downloaded)")
    os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
    print(json.dumps(build(a.code_dir, a.config, a.out_dir, a.seed), indent=1))


if __name__ == "__main__":
    main()
