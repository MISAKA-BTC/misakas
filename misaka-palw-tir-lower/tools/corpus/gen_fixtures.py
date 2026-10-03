#!/usr/bin/env python3
"""Build the tiny random-init fixtures of the architecture corpus v2 (and its manifest).

Offline only: `HF_HUB_OFFLINE=1` is required, every model is built from a config class with a fixed
seed (`sum(ord(c))` of its id), weights are re-randomised so every feature is live and rounded to
bfloat16 (exactly as `tools/gen_hf_fixtures.py`), saved, reloaded fresh in float32 with eager
attention and run on fixed inputs. Nothing is downloaded.

    $OUT/<id>/config.json, model.safetensors (BF16), reference.json     the heavy fixture (not committed)
    tools/corpus/specs/<id>/config.json, tensors.json                   the light spec (committed)
    tools/corpus/corpus_v2.json                                         the manifest the Rust harness reads

Usage
    python gen_fixtures.py --manifest                                    write corpus_v2.json
    HF_HUB_OFFLINE=1 python gen_fixtures.py --probe [id ...]             instantiate + forward, no files
    HF_HUB_OFFLINE=1 python gen_fixtures.py --out DIR --specs [id ...]   build fixtures (all when no id)

One model at a time, two threads (the machine is shared): this script never parallelises.
"""

import argparse
import json
import math
import os
import struct
import sys
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import corpus_def as C  # noqa: E402

SPECS = os.path.join(HERE, "specs")
REAL = os.path.join(os.path.dirname(HERE), "..", "tests", "configs", "real")
T = 10  # positions of a causal-LM reference
V = C.V
EMBED_HINTS = ("embed", "wte", "wpe", "word_embeddings", "embeddings.weight", "embed_in")


def _lazy():
    """Import torch/transformers only when a model is built (the manifest needs neither)."""
    global torch, transformers, CONFIG_MAPPING
    import torch  # noqa: F401
    import transformers  # noqa: F401
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING  # noqa: F401
    # The machine is shared with nine other lanes: two threads, one model at a time (coordinator, 10-01).
    torch.set_num_threads(2)


# ───────────────────────────────────────── configs ─────────────────────────────────────────

def auto_tiny(model_type, overrides):
    """The family's default config, shrunk to a tiny model (keys the class has), then `overrides`."""
    d = CONFIG_MAPPING[model_type]().to_dict()
    kw = {k: v for k, v in C.SHRINK.items() if k in d}
    if d.get("head_dim") is not None:
        kw["head_dim"] = 8
    for k, v in C.TOKEN_IDS.items():
        if isinstance(d.get(k), int) and d[k] >= V:
            kw[k] = v
    kw.update(overrides)
    return CONFIG_MAPPING[model_type](**kw)


def entry_config(e):
    """A `transformers` config for the entry: raw kwargs (`options.raw`) or the auto-shrunk family default."""
    if e["options"].get("raw"):
        return CONFIG_MAPPING[e["model_type"]](**e["cfg"])
    return auto_tiny(e["model_type"], e["cfg"])


# ───────────────────────────────────────── helpers ─────────────────────────────────────────

def randomise(model, hidden, seed, conv_scale=None):
    """Every weight live and O(1)-scaled, then bfloat16-exact (as tools/gen_hf_fixtures.py)."""
    g = torch.Generator().manual_seed(seed + 1)
    sd_names = set(model.state_dict().keys())
    with torch.no_grad():
        for name, p in model.named_parameters():
            if not p.is_floating_point():
                continue
            if p.ndim >= 2 and any(h in name for h in EMBED_HINTS):
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2 and "conv1d" in name:
                p.copy_(torch.randn(p.shape, generator=g) * 0.5)
            elif p.ndim >= 2 and "time_mix" not in name and "time_maa" not in name:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(hidden))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
        for name, b in model.named_buffers():
            if name in sd_names and b.is_floating_point():
                b.add_(torch.randn(b.shape, generator=g) * 0.1)
                b.copy_(b.to(torch.bfloat16).to(torch.float32))


def round_bf16(model):
    with torch.no_grad():
        for p in model.parameters():
            if p.is_floating_point():
                p.copy_(p.to(torch.bfloat16).to(torch.float32))


def safetensors_index(path):
    """name -> {dtype, shape} from the header of a .safetensors file (no tensor data read)."""
    with open(path, "rb") as f:
        n = struct.unpack("<Q", f.read(8))[0]
        hdr = json.loads(f.read(n))
    return {k: {"dtype": v["dtype"], "shape": v["shape"]} for k, v in hdr.items() if k != "__metadata__"}


def checkpoint_index(d):
    out = {}
    for fn in sorted(os.listdir(d)):
        if fn.endswith(".safetensors"):
            out.update(safetensors_index(os.path.join(d, fn)))
    return out


def seed_of(e):
    return sum(ord(ch) for ch in e["id"])


def hidden_of(cfg):
    tc = cfg.get_text_config() if hasattr(cfg, "get_text_config") else cfg
    for k in ("hidden_size", "n_embd", "d_model", "dim"):
        v = getattr(tc, k, None)
        if v:
            return v
    return 32


def finite(rows):
    return all(math.isfinite(x) for r in rows for x in r)


def decode_logits(model, ids):
    out, past = [], None
    for t in range(ids.shape[1]):
        o = model(input_ids=ids[:, t:t + 1], past_key_values=past, use_cache=True)
        past = o.past_key_values
        out.append(o.logits[0, -1].tolist())
    return out


def write_light_spec(e, d):
    """The committed light spec: config.json + the tensor index of a built fixture."""
    out = os.path.join(SPECS, e["id"])
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(d, "config.json")) as f:
        cfgtxt = json.load(f)
    with open(os.path.join(out, "config.json"), "w") as f:
        json.dump(cfgtxt, f, indent=2, sort_keys=True)
        f.write("\n")
    idx = checkpoint_index(d)
    with open(os.path.join(out, "tensors.json"), "w") as f:
        json.dump(idx, f, indent=1, sort_keys=True)
        f.write("\n")


def tclass(name):
    return getattr(transformers, name)


def model_class(e):
    b, o = e["builder"], e["options"]
    if o.get("class"):
        return tclass(o["class"])
    if b == "causal":
        return transformers.AutoModelForCausalLM
    if b == "vlm":
        return transformers.AutoModelForImageTextToText
    if b == "seq2seq":
        return transformers.AutoModelForSeq2SeqLM
    if b == "encoder":
        return transformers.AutoModel
    if b.startswith("audio:"):
        return tclass(b.split(":", 1)[1])
    raise RuntimeError(f"no model class for builder {b}")


MAX_PARAMS = 20_000_000          # a corpus model is TINY: more than this means a config key was not shrunk
MAX_TENSOR_BYTES = 64 * 1024 * 1024


def size_guard(build):
    """Instantiate `build()` on the META device (no storage) and refuse a model that is not tiny.

    Never instantiate a model to discover its size (incident of 2026-10-01: a census probe over
    every transformers family built models with unshrunk defaults and grew to 43 GB): count the
    parameters on `meta`, skip anything over 20 M parameters or with a single tensor over 64 MB.
    """
    with torch.device("meta"):
        m = build()
    n = sum(p.numel() for p in m.parameters())
    big = max((p.numel() * 4 for p in m.parameters()), default=0)
    del m
    if n > MAX_PARAMS or big > MAX_TENSOR_BYTES:
        raise RuntimeError(f"not tiny: {n:,} parameters, largest tensor {big / 2**20:.0f} MiB (shrink its config keys)")
    return n


def _build(e, cfg):
    cls = model_class(e)
    if hasattr(cls, "from_config") and cls.__name__.startswith("Auto"):
        return cls.from_config(cfg)
    try:
        return cls(cfg)
    except TypeError:
        return cls._from_config(cfg)


def instantiate(e, cfg):
    size_guard(lambda: _build(e, cfg))
    return _build(e, cfg)


def hub_rename(d, rules):
    """Rewrite tensor names of a saved checkpoint by `[regex, replacement]` rules (re.sub, in order), keeping dtype and values.

    `save_pretrained` writes the REVERSE of the conversion mapping `transformers` applies when it loads a hub checkpoint, and a blanket
    reverse rule can name a tensor differently from the hub's own checkpoint (Kimi-Linear: the dense layers' `mlp.*` are saved as
    `block_sparse_moe.*`, the MoE layers' name; the hub keeps `mlp.*` for the dense layers). A rule here puts the saved name back to the hub's;
    the model is then RELOADED from the renamed files, so the reference proves `transformers` reads the hub's names.
    """
    if not rules:
        return
    import re
    from safetensors.torch import load_file, save_file
    for fn in sorted(os.listdir(d)):
        if not fn.endswith(".safetensors"):
            continue
        t = load_file(os.path.join(d, fn))
        out = {}
        for k, v in t.items():
            for pat, rep in rules:
                k = re.sub(pat, rep, k)
            assert k not in out, f"hub_rename: two tensors named {k}"
            out[k] = v
        save_file(out, os.path.join(d, fn), metadata={"format": "pt"})


def save_and_reload(e, model, cfg, outdir, cls=None):
    d = os.path.join(outdir, e["id"])
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    hub_rename(d, e["options"].get("hub_rename", []))
    gc = os.path.join(d, "generation_config.json")
    if os.path.exists(gc):
        os.remove(gc)
    cls = cls or model_class(e)
    impl = e["options"].get("attn", "eager")
    try:
        fresh = cls.from_pretrained(d, dtype=torch.float32, attn_implementation=impl)
    except Exception:
        fresh = cls.from_pretrained(d, dtype=torch.float32)
    fresh.eval()
    return d, fresh


# ───────────────────────────────────────── builders ─────────────────────────────────────────

PATCHES = {
    # NanoChat's `rotate_half` has FLIPPED signs (rotation by -theta). The variant patches it to the standard one in the REFERENCE, to
    # show that the rotation sign is the only gap left once FR-02 (the post-rotation q/k norm) is in (FR-35).
    "nanochat_rotate_half": lambda: setattr(
        __import__("transformers.models.nanochat.modeling_nanochat", fromlist=["x"]), "rotate_half",
        lambda x: torch.cat((-x[..., x.shape[-1] // 2:], x[..., : x.shape[-1] // 2]), dim=-1)),
}


def build_causal(e, outdir, probe):
    if e["options"].get("patch"):
        PATCHES[e["options"]["patch"]]()
    seed = seed_of(e)
    cfg = entry_config(e)
    torch.manual_seed(seed)
    model = instantiate(e, cfg)
    model.eval()
    ids = torch.tensor([[(seed * 7 + 13 * i + i * i) % V for i in range(T)]])
    if probe:
        with torch.no_grad():
            out = model(input_ids=ids).logits
        return {"model": type(model).__name__, "logits": list(out.shape)}
    randomise(model, hidden_of(cfg), seed)
    d, fresh = save_and_reload(e, model, cfg, outdir)
    with torch.no_grad():
        full = fresh(input_ids=ids).logits[0].tolist()
        dec = decode_logits(fresh, ids) if e["options"].get("decode") else None
    if not finite(full):
        raise RuntimeError("non-finite logits")
    meta = {"kind": "causal_lm", "tokens": ids[0].tolist(), "logits_full": full, "seed": seed,
            "transformers": transformers.__version__, "torch": torch.__version__,
            "weights": "bfloat16-exact (rounded before the forward)"}
    if dec is not None:
        meta["logits_decode"] = dec
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump(meta, f)
    return {"model": type(fresh).__name__, "max_logit": round(max(abs(x) for r in full for x in r), 2)}


def build_encoder(e, outdir, probe):
    """A bidirectional or causal encoder; the reference is the last hidden state (+ pooled rows)."""
    seed = seed_of(e)
    cfg = entry_config(e)
    torch.manual_seed(seed)
    model = instantiate(e, cfg)
    model.eval()
    o = e["options"]
    seqs = o.get("seqs", [[2, 11, 25, 7, 3], [2, 40, 9, 17, 33, 21, 8, 3]])
    pad, lmax = o.get("pad", 0), o.get("lmax", 12)

    def fwd(m, s):
        ids = torch.tensor([s + [pad] * (lmax - len(s))])
        mask = torch.tensor([[1] * len(s) + [0] * (lmax - len(s))])
        return ids, mask, m(input_ids=ids, attention_mask=mask)

    if probe:
        with torch.no_grad():
            _, _, out = fwd(model, seqs[0])
        return {"model": type(model).__name__, "out": type(out).__name__}
    randomise(model, hidden_of(cfg), seed)
    d, fresh = save_and_reload(e, model, cfg, outdir)
    recs = []
    with torch.no_grad():
        for s in seqs:
            ids, mask, out = fwd(fresh, s)
            h = out.last_hidden_state[0]
            m = mask[0].unsqueeze(-1).float()
            mean = (h * m).sum(0) / m.sum(0).clamp(min=1e-9)
            rec = {"tokens": s, "padded": ids[0].tolist(), "count": len(s), "hidden": h.tolist(), "cls": h[0].tolist(),
                   "mean": mean.tolist(), "mean_normalized": torch.nn.functional.normalize(mean, p=2, dim=0).tolist(),
                   "cls_normalized": torch.nn.functional.normalize(h[0], p=2, dim=0).tolist()}
            if getattr(out, "text_embeds", None) is not None:
                rec["embeds"] = out.text_embeds[0].tolist()
            recs.append(rec)
    meta = {"kind": "encoder", "sequences": recs, "pad": pad, "lmax": lmax, "seed": seed,
            "transformers": transformers.__version__}
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump(meta, f)
    return {"model": type(fresh).__name__}


def build_seq2seq(e, outdir, probe):
    """T5/BART-family: the decoder's logits for two source sequences (teacher-forced on a random stream)."""
    import numpy as np
    seed = seed_of(e)
    cfg = entry_config(e)
    torch.manual_seed(seed)
    model = instantiate(e, cfg)
    model.eval()
    o = e["options"]
    ids = torch.tensor([[3, 4, 5, 6, 7, 8, 9, cfg.eos_token_id]])
    dec = torch.tensor([[cfg.decoder_start_token_id, 4, 5, 6]])
    if probe:
        with torch.no_grad():
            out = model(input_ids=ids, decoder_input_ids=dec)
        return {"model": type(model).__name__, "logits": list(out.logits.shape)}
    randomise(model, cfg.d_model, seed)
    with torch.no_grad():  # a tied table of randn rows would make every token map to itself
        emb = model.get_input_embeddings().weight
        emb.mul_(o.get("embed_mul", 1.0))
        emb.copy_(emb.to(torch.bfloat16).to(torch.float32))
    d, fresh = save_and_reload(e, model, cfg, outdir)
    rng = np.random.default_rng(seed)
    recs = []
    with torch.no_grad():
        for n in (10, 7):
            src = [int(t) for t in rng.integers(3, V - 4, size=n - 1)] + [cfg.eos_token_id]
            rnd = [cfg.decoder_start_token_id] + [int(t) for t in rng.integers(0, V, size=7)]
            out = fresh(input_ids=torch.tensor([src]), decoder_input_ids=torch.tensor([rnd]))
            recs.append({"input_ids": src, "random_decoder_ids": rnd, "random_logits": out.logits[0].tolist()})
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump({"kind": "seq2seq", "records": recs, "seed": seed}, f)
    return {"model": type(fresh).__name__}


def build_vision(e, outdir, probe):
    seed = seed_of(e)
    cfg = entry_config(e)
    torch.manual_seed(seed)
    model = instantiate(e, cfg)
    model.eval()
    size = e["options"].get("size", 28)
    g = torch.Generator().manual_seed(seed)
    x = torch.rand(1, 3, size, size, generator=g) * 2 - 1
    if probe:
        with torch.no_grad():
            out = model(pixel_values=x)
        return {"model": type(model).__name__, "out": type(out).__name__}
    randomise(model, hidden_of(cfg) if hasattr(cfg, "hidden_size") else 32, seed)
    d, fresh = save_and_reload(e, model, cfg, outdir)
    with torch.no_grad():
        out = fresh(pixel_values=x)
    h = getattr(out, "last_hidden_state", None)
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump({"kind": "vision", "pixel_values": x[0].tolist(), "seed": seed,
                   "last_hidden_state": h[0].flatten().tolist() if h is not None else None,
                   "shape": list(h.shape) if h is not None else None}, f)
    return {"model": type(fresh).__name__}


# diffusers / audio: build, run one forward on fixed random inputs, save; no Rust route exists for these.
def _diff_inputs(kind, torch_):
    g = torch_.Generator().manual_seed(7)
    r = lambda *s: torch_.randn(*s, generator=g)  # noqa: E731
    if kind == "unet":
        return dict(sample=r(1, 4, 8, 8), timestep=torch_.tensor([10]), encoder_hidden_states=r(1, 6, 32))
    if kind == "unet_xl":
        return dict(sample=r(1, 4, 8, 8), timestep=torch_.tensor([10]), encoder_hidden_states=r(1, 6, 32),
                    added_cond_kwargs={"text_embeds": r(1, 16), "time_ids": r(1, 6)})
    if kind == "dit":
        return dict(hidden_states=r(1, 4, 8, 8), timestep=torch_.tensor([10]), class_labels=torch_.tensor([3]))
    if kind == "flux":
        return dict(hidden_states=r(1, 16, 16), encoder_hidden_states=r(1, 6, 32), pooled_projections=r(1, 16),
                    timestep=torch_.tensor([0.5]), img_ids=torch_.zeros(16, 3), txt_ids=torch_.zeros(6, 3))
    if kind == "sd3":
        return dict(hidden_states=r(1, 4, 8, 8), encoder_hidden_states=r(1, 6, 32), pooled_projections=r(1, 16),
                    timestep=torch_.tensor([10.0]))
    if kind == "vae":
        return dict(sample=r(1, 3, 16, 16))
    raise RuntimeError(kind)


def build_diffusers(e, outdir, probe):
    import diffusers
    seed = seed_of(e)
    cls = getattr(diffusers, e["builder"].split(":", 1)[1])
    torch.manual_seed(seed)
    size_guard(lambda: cls(**e["cfg"]))
    model = cls(**e["cfg"])
    model.eval()
    inp = _diff_inputs(e["options"]["inputs"], torch)
    with torch.no_grad():
        out = model(**inp)
    shape = list((out.sample if hasattr(out, "sample") else out[0]).shape)
    if probe:
        return {"model": cls.__name__, "out": shape}
    round_bf16(model)
    d = os.path.join(outdir, e["id"])
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d, safe_serialization=True)
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump({"kind": "diffusers", "class": cls.__name__, "out_shape": shape, "seed": seed}, f)
    return {"model": cls.__name__, "out": shape}


def _audio_inputs(kind, model, torch_):
    g = torch_.Generator().manual_seed(7)
    r = lambda *s: torch_.randn(*s, generator=g)  # noqa: E731
    if kind == "whisper":
        return dict(input_features=r(1, 8, 32), decoder_input_ids=torch_.tensor([[3, 5, 6]]))
    if kind == "wav2vec2":
        return dict(input_values=r(1, 1600))
    if kind == "speecht5":
        return dict(input_ids=torch_.tensor([[4, 5, 6, 7]]), decoder_input_values=r(1, 3, 8), speaker_embeddings=r(1, 512))
    if kind == "musicgen":
        return dict(input_ids=torch_.tensor([[1, 2, 3]]), decoder_input_ids=torch_.randint(0, 16, (2, 4), generator=g))
    if kind == "encodec":
        return dict(input_values=r(1, 1, 1024))
    raise RuntimeError(kind)


def build_audio(e, outdir, probe):
    seed = seed_of(e)
    cfg = entry_config(e)
    torch.manual_seed(seed)
    model = instantiate(e, cfg)
    model.eval()
    inp = _audio_inputs(e["options"]["inputs"], model, torch)
    with torch.no_grad():
        out = model(**inp) if e["id"] != "encodec" else model(inp["input_values"])
    first = next((v for v in (getattr(out, k, None) for k in ("logits", "spectrogram", "audio_values", "last_hidden_state")) if v is not None), None)
    shape = list(first.shape) if first is not None else None
    if probe:
        return {"model": type(model).__name__, "out": shape}
    round_bf16(model)
    d = os.path.join(outdir, e["id"])
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d)
    with open(os.path.join(d, "reference.json"), "w") as f:
        json.dump({"kind": "audio", "class": type(model).__name__, "out_shape": shape, "seed": seed}, f)
    return {"model": type(model).__name__, "out": shape}


def build_config_only(e, outdir, probe):
    """A remote-code family: a published-style config and no model (its forward is not in transformers)."""
    d = os.path.join(outdir, e["id"])
    os.makedirs(d, exist_ok=True)
    if "config" in e["cfg"]:
        cfg = e["cfg"]["config"]
    else:
        with open(os.path.join(REAL, e["cfg"]["config_file"] + ".json")) as f:
            cfg = json.load(f)
    with open(os.path.join(d, "config.json"), "w") as f:
        json.dump(cfg, f, indent=2, sort_keys=True)
    with open(os.path.join(d, "tensors.json"), "w") as f:
        json.dump(e["cfg"].get("tensors", {}), f)
    return {"model": "(config only)"}


def build(e, outdir, probe=False):
    b = e["builder"]
    if b in ("causal", "vlm"):
        return build_causal(e, outdir, probe)
    if b == "encoder":
        return build_encoder(e, outdir, probe)
    if b == "seq2seq":
        return build_seq2seq(e, outdir, probe)
    if b == "vision":
        return build_vision(e, outdir, probe)
    if b.startswith("diffusers:"):
        return build_diffusers(e, outdir, probe)
    if b.startswith("audio:"):
        return build_audio(e, outdir, probe)
    if b == "config-only":
        return build_config_only(e, outdir, probe)
    raise RuntimeError(f"unknown builder {b}")


# ───────────────────────────────────────── CLI ─────────────────────────────────────────

def manifest():
    path = os.path.join(HERE, "corpus_v2.json")
    ids = [e["id"] for e in C.ENTRIES]
    assert len(ids) == len(set(ids)), "duplicate corpus ids"
    doc = {"schema": "misaka.palw.corpus-v2", "positions": T, "vocab": V, "usage_weight": C.USAGE_WEIGHT, "entries": C.ENTRIES}
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {path}: {len(ids)} entries")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ids", nargs="*")
    ap.add_argument("--manifest", action="store_true")
    ap.add_argument("--probe", action="store_true")
    ap.add_argument("--out", default=os.path.expanduser("~/Downloads/MISAKA-wt-b/corpus-fixtures"))
    ap.add_argument("--specs", action="store_true", help="also write the light specs into tools/corpus/specs")
    ap.add_argument("--entries", help="build the entries of this manifest (e.g. census_v2.json) instead of the corpus")
    ap.add_argument("--specs-dir", help="where --specs writes (default tools/corpus/specs; census: tools/corpus/census-specs)")
    a = ap.parse_args()
    global SPECS
    if a.specs_dir:
        SPECS = os.path.abspath(a.specs_dir)
    entries_all = C.ENTRIES
    if a.entries:
        with open(a.entries) as f:
            entries_all = json.load(f)["entries"]
    if a.manifest:
        manifest()
        return
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
    _lazy()
    entries = [e for e in entries_all if not a.ids or e["id"] in a.ids]
    bad = 0
    for e in entries:
        t0 = time.time()
        try:
            info = build(e, a.out, probe=a.probe)
            if not a.probe and a.specs:
                src = os.path.join(a.out, e["id"])
                if e["builder"] == "config-only":
                    out = os.path.join(SPECS, e["id"])
                    os.makedirs(out, exist_ok=True)
                    with open(os.path.join(src, "config.json")) as f:
                        cfgtxt = json.load(f)
                    with open(os.path.join(out, "config.json"), "w") as f:
                        json.dump(cfgtxt, f, indent=2, sort_keys=True)
                        f.write("\n")
                    with open(os.path.join(out, "tensors.json"), "w") as f:
                        f.write("{}\n")
                else:
                    write_light_spec(e, src)
            print(f"ok   {e['id']:20s} {json.dumps(info)[:110]}  {time.time() - t0:.1f}s", flush=True)
        except Exception as ex:
            bad += 1
            if os.environ.get("CORPUS_TRACE"):
                traceback.print_exc()
            print(f"FAIL {e['id']:20s} {type(ex).__name__}: {str(ex).replace(chr(10), ' ')[:260]}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
