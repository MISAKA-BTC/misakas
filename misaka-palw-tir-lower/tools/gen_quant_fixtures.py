#!/usr/bin/env python3
"""Pre-quantised fixtures (GPTQ, AWQ) for misaka-palw-tir-lower — quantised here, in numpy, per each
format's specification. No quantisation library (auto-gptq, gptqmodel, optimum, autoawq) and no hub
access: run with HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1.

Every fixture is a tiny random decoder (transformers' own config classes) whose block projections
(q/k/v/o, gate/up/down) are quantised by round-to-nearest over groups of input columns:

  GPTQ (AutoGPTQ / GPTQModel, `checkpoint_format` gptq = v1 or gptq_v2):
    scale, zero per (group, output) as GPTQ's `Quantizer.find_params` (sym: zero = 2^(b-1));
    q = clamp(round(W / fp16(scale)) + zero, 0, 2^b - 1);
    act-order (`desc_act`): columns quantised in the order of a random "Hessian diagonal",
    groups over that order, g_idx[i] = position(i) // group_size;
    qweight int32 [in*b/32, out]: word r holds inputs r*32/b + k at bits b*k;
    qzeros  int32 [G, out*b/32]: word c holds outputs c*32/b + k, v1 stores (zero - 1) mod 2^b;
    scales  fp16 [G, out]; g_idx int32 [in].
  AWQ (AutoAWQ GEMM, 4-bit):
    scale = (max - min) / 15 (min 1e-5), zero = clamp(-round(min / scale), 0, 15) per (group, output);
    q = clamp(round(W / fp16(scale) + zero), 0, 15);
    qweight int32 [in, out/8] and qzeros int32 [G, out/8]: slot k of word c holds output
    8c + [0,2,4,6,1,3,5,7][k]; scales fp16 [G, out].

Every other tensor is stored in fp16 (the float model is rounded to fp16 first). THE REFERENCE is
transformers' float model with the dequantised weights substituted, W[o,i] = s[g,o]*(q[i,o] - z[g,o]),
where the dequantisation is recomputed from the PACKED tensors (not from the quantiser's arrays),
so a packing mistake shows as a mismatch against the lowerer's own unpacking.

Modes:
  gen_quant_fixtures.py fixtures [name ...]
      tests/fixtures/hf-quant/<name>/{config.json, model.safetensors, logits.json}
  gen_quant_fixtures.py audit OUT [name ...]
      OUT/<name>/       config.json (with quantization_config), model.safetensors,
                        hf.json, hf-logits.f32, calib.json (as tools/audit_e2e.py writes them)
      OUT/<name>-float/ the same model with the dequantised weights as a plain F32 checkpoint
                        (the float path's twin: W8 re-quantisation of the identical weights)
"""

import json
import math
import os
import sys

import numpy as np
import torch
from safetensors.numpy import save_file

try:
    import transformers
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
except ImportError as e:  # pragma: no cover
    sys.exit(f"transformers is required: {e}")

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-quant")

V = 128
T = 16
PROJ = ("q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj", "w1", "w2", "w3")
AWQ_ORDER = [0, 2, 4, 6, 1, 3, 5, 7]

BASE = dict(hidden_size=128, intermediate_size=256, num_attention_heads=4, num_key_value_heads=2, vocab_size=V,
            num_hidden_layers=2, max_position_embeddings=1024)
MODELS = {
    "qwen3moe": ("qwen3_moe", "Qwen3MoeForCausalLM", dict(BASE, moe_intermediate_size=128, num_experts=4, num_experts_per_tok=2,
                                                        norm_topk_prob=True, head_dim=32)),
    "mixtral": ("mixtral", "MixtralForCausalLM", dict(BASE, intermediate_size=128, num_local_experts=4, num_experts_per_tok=2)),
    "llama": ("llama", "LlamaForCausalLM", dict(BASE)),
    # Qwen2: q/k/v biases (a quantised module's float bias), tied embeddings.
    "qwen2": ("qwen2", "Qwen2ForCausalLM", dict(BASE, tie_word_embeddings=True)),
}

# name -> (model, quantization_config)
CONFIGS = {
    "gptq_b4_g32": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": 32, "desc_act": False, "sym": True}),
    "gptq_b4_g64_act": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": 64, "desc_act": True, "sym": True}),
    "gptq_b4_g128_act_asym": ("qwen2", {"quant_method": "gptq", "bits": 4, "group_size": 128, "desc_act": True, "sym": False}),
    "gptq_b8_g128": ("llama", {"quant_method": "gptq", "bits": 8, "group_size": 128, "desc_act": False, "sym": True}),
    "gptq_b8_g32_asym_act": ("qwen2", {"quant_method": "gptq", "bits": 8, "group_size": 32, "desc_act": True, "sym": False}),
    "gptq_b4_perrow": ("llama", {"quant_method": "gptq", "bits": 4, "group_size": -1, "desc_act": False, "sym": True}),
    "gptq_v2_b4_g64": ("qwen2", {"quant_method": "gptq", "bits": 4, "group_size": 64, "desc_act": False, "sym": False,
                                 "checkpoint_format": "gptq_v2"}),
    "gptq_b2_g32": ("llama", {"quant_method": "gptq", "bits": 2, "group_size": 32, "desc_act": False, "sym": True}),
    "awq_g32": ("llama", {"quant_method": "awq", "bits": 4, "group_size": 32, "zero_point": True, "version": "gemm"}),
    "awq_g64": ("qwen2", {"quant_method": "awq", "bits": 4, "group_size": 64, "zero_point": True, "version": "gemm"}),
    "awq_g128": ("llama", {"quant_method": "awq", "bits": 4, "group_size": 128, "zero_point": True, "version": "gemm"}),
    # Experts: one stored module per expert; the router stays float.
    "gptq_qwen3moe_b4_g32_act": ("qwen3moe", {"quant_method": "gptq", "bits": 4, "group_size": 32, "desc_act": True, "sym": True}),
    "awq_mixtral_g64": ("mixtral", {"quant_method": "awq", "bits": 4, "group_size": 64, "zero_point": True, "version": "gemm",
                                    "modules_to_not_convert": ["gate"]}),
}


def f16(x):
    return np.asarray(x, dtype=np.float32).astype(np.float16).astype(np.float64)


# ───────────────────────────── quantisers (format specifications) ─────────────────────────────

def gptq_quantize(w, bits, group, sym, desc_act, rng):
    """w: [out, in] float64. Returns q [in, out], zero [G, out], scale16 [G, out], g_idx [in]."""
    out, inp = w.shape
    gs = inp if group == -1 else group
    maxq = 2 ** bits - 1
    if desc_act:
        hdiag = rng.random(inp) + 0.1
        perm = np.argsort(-hdiag, kind="stable")
    else:
        perm = np.arange(inp)
    pos = np.empty(inp, dtype=np.int64)
    pos[perm] = np.arange(inp)
    g_idx = (pos // gs).astype(np.int32)
    ng = -(-inp // gs)
    q = np.zeros((inp, out), dtype=np.int64)
    zero = np.zeros((ng, out), dtype=np.int64)
    scale16 = np.zeros((ng, out))
    for g in range(ng):
        cols = perm[g * gs:(g + 1) * gs]
        wg = w[:, cols]
        xmin = np.minimum(wg.min(1), 0.0)
        xmax = np.maximum(wg.max(1), 0.0)
        if sym:
            xmax = np.maximum(np.abs(xmin), xmax)
            xmin = np.where(xmin < 0, -xmax, xmin)
        both = (xmin == 0) & (xmax == 0)
        xmin[both], xmax[both] = -1.0, 1.0
        scale = (xmax - xmin) / maxq
        z = np.full(out, (maxq + 1) // 2) if sym else np.round(-xmin / scale)
        s16 = f16(scale)
        qq = np.clip(np.round(wg / s16[:, None]) + z[:, None], 0, maxq)
        q[cols, :] = qq.T.astype(np.int64)
        zero[g] = z.astype(np.int64)
        scale16[g] = s16
    return q, zero, scale16, g_idx


def gptq_pack(q, zero, scale16, g_idx, bits, v2):
    inp, out = q.shape
    pack = 32 // bits
    maxq = 2 ** bits - 1
    qw = np.zeros((inp // pack, out), dtype=np.uint32)
    for k in range(pack):
        qw |= (q[k::pack, :].astype(np.uint32) & maxq) << np.uint32(bits * k)
    zs = zero if v2 else (zero - 1) & maxq
    qz = np.zeros((zero.shape[0], out // pack), dtype=np.uint32)
    for k in range(pack):
        qz |= (zs[:, k::pack].astype(np.uint32) & maxq) << np.uint32(bits * k)
    return {"qweight": qw.view(np.int32), "qzeros": qz.view(np.int32), "scales": scale16.astype(np.float16),
            "g_idx": g_idx.astype(np.int32)}


def gptq_dequant(t, bits, v2):
    """From the PACKED tensors only."""
    qw, qz, sc, gi = t["qweight"].view(np.uint32), t["qzeros"].view(np.uint32), t["scales"].astype(np.float64), t["g_idx"]
    pack = 32 // bits
    maxq = 2 ** bits - 1
    inp, out = qw.shape[0] * pack, qw.shape[1]
    q = np.zeros((inp, out), dtype=np.int64)
    for k in range(pack):
        q[k::pack, :] = (qw >> np.uint32(bits * k)) & maxq
    z = np.zeros((qz.shape[0], out), dtype=np.int64)
    for k in range(pack):
        z[:, k::pack] = (qz >> np.uint32(bits * k)) & maxq
    if not v2:
        z = (z + 1) & maxq
    w = sc[gi, :] * (q - z[gi, :])  # [in, out]
    return w.T


def awq_quantize(w, group):
    out, inp = w.shape
    ng = inp // group
    q = np.zeros((inp, out), dtype=np.int64)
    zero = np.zeros((ng, out), dtype=np.int64)
    scale16 = np.zeros((ng, out))
    for g in range(ng):
        wg = w[:, g * group:(g + 1) * group]
        mx, mn = wg.max(1), wg.min(1)
        scale = np.maximum(mx - mn, 1e-5) / 15.0
        z = np.clip(-np.round(mn / scale), 0, 15)
        s16 = f16(scale)
        qq = np.clip(np.round(wg / s16[:, None] + z[:, None]), 0, 15)
        q[g * group:(g + 1) * group, :] = qq.T.astype(np.int64)
        zero[g] = z.astype(np.int64)
        scale16[g] = s16
    return q, zero, scale16


def awq_pack(q, zero, scale16):
    inp, out = q.shape
    qw = np.zeros((inp, out // 8), dtype=np.uint32)
    qz = np.zeros((zero.shape[0], out // 8), dtype=np.uint32)
    for k, o in enumerate(AWQ_ORDER):
        qw |= (q[:, o::8].astype(np.uint32) & 15) << np.uint32(4 * k)
        qz |= (zero[:, o::8].astype(np.uint32) & 15) << np.uint32(4 * k)
    return {"qweight": qw.view(np.int32), "qzeros": qz.view(np.int32), "scales": scale16.astype(np.float16)}


def awq_dequant(t, group):
    qw, qz, sc = t["qweight"].view(np.uint32), t["qzeros"].view(np.uint32), t["scales"].astype(np.float64)
    inp, out = qw.shape[0], qw.shape[1] * 8
    q = np.zeros((inp, out), dtype=np.int64)
    z = np.zeros((qz.shape[0], out), dtype=np.int64)
    for k, o in enumerate(AWQ_ORDER):
        q[:, o::8] = (qw >> np.uint32(4 * k)) & 15
        z[:, o::8] = (qz >> np.uint32(4 * k)) & 15
    gi = np.arange(inp) // group
    w = sc[gi, :] * (q - z[gi, :])
    return w.T


# ───────────────────────────── models ─────────────────────────────

def build(name):
    """The float model, its quantised checkpoint tensors, the reference model and the config.

    The quantisation works on the CHECKPOINT's tensors (transformers' `save_pretrained` names:
    one module per expert), and the reference is `from_pretrained` of the same checkpoint with the
    dequantised weights — so fused in-memory layouts (transformers 5's experts) need no handling.
    """
    import tempfile
    from safetensors.numpy import load_file
    model_name, qc = CONFIGS[name]
    model_type, arch, cfg_kw = MODELS[model_name]
    seed = sum(ord(ch) for ch in name) + 77
    cfg = CONFIG_MAPPING[model_type](**cfg_kw)
    cfg.architectures = [arch]
    torch.manual_seed(seed)
    model = AutoModelForCausalLM.from_config(cfg)
    model.eval()
    g = torch.Generator().manual_seed(seed + 1)
    hidden = cfg.hidden_size
    with torch.no_grad():
        for pname, p in model.named_parameters():
            if p.ndim >= 2 and "embed" in pname:
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(hidden))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.float16).to(torch.float32))
    tmp = tempfile.mkdtemp(prefix="quantfix-")
    save_float(model, tmp)
    ck = load_file(os.path.join(tmp, "model.safetensors"))
    rng = np.random.default_rng(seed)
    tensors, deq = {}, {}
    method, bits = qc["quant_method"], qc.get("bits", 4)
    group = qc.get("group_size", 128)
    v2 = qc.get("checkpoint_format") == "gptq_v2"
    for k in sorted(ck):
        v = ck[k]
        if k.endswith(".weight") and k.split(".")[-2] in PROJ and ".layers." in k:
            mod = k[: -len(".weight")]
            w = v.astype(np.float64)
            if method == "gptq":
                q, z, s16, gi = gptq_quantize(w, bits, group, qc.get("sym", True), qc.get("desc_act", False), rng)
                packed = gptq_pack(q, z, s16, gi, bits, v2)
                back = gptq_dequant(packed, bits, v2)
            else:
                q, z, s16 = awq_quantize(w, group)
                packed = awq_pack(q, z, s16)
                back = awq_dequant(packed, group)
            # The packed tensors reproduce the quantiser's grid (a packing self-check).
            if method == "gptq":
                gi = packed["g_idx"]
                ref = (s16[gi, :] * (q - z[gi, :])).T
            else:
                gi = np.arange(w.shape[1]) // group
                ref = (s16[gi, :] * (q - z[gi, :])).T
            assert np.array_equal(back, ref), f"{name}: {mod} does not unpack to its quantiser's grid"
            for part, arr in packed.items():
                tensors[f"{mod}.{part}"] = arr
            # C order: safetensors writes the buffer as laid out (`back` is a transposed view).
            deq[k] = np.ascontiguousarray(back, dtype=np.float32)
        else:
            tensors[k] = v.astype(np.float16)
            deq[k] = v.astype(np.float16).astype(np.float32)
    # The reference: the same checkpoint with the dequantised weights, as transformers loads it.
    save_file(deq, os.path.join(tmp, "model.safetensors"), metadata={"format": "pt"})
    ref_model = AutoModelForCausalLM.from_pretrained(tmp, dtype=torch.float32, attn_implementation="eager")
    ref_model.eval()
    qcfg = dict(qc)
    if method == "gptq":
        qcfg.setdefault("checkpoint_format", "gptq")
        qcfg.update({"damp_percent": 0.01, "true_sequential": True, "static_groups": False})
    return ref_model, tensors, qcfg, seed


def save_float(model, d):
    """`save_pretrained` (config.json as transformers writes it, F32 weights), no generation config."""
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d)
    gp = os.path.join(d, "generation_config.json")
    if os.path.exists(gp):
        os.remove(gp)


def save_quant(model, tensors, qcfg, d):
    """The quantised checkpoint: transformers' config.json plus `quantization_config`, and the
    packed tensors in place of the float file."""
    save_float(model, d)
    save_file(tensors, os.path.join(d, "model.safetensors"), metadata={"format": "pt"})
    cp = os.path.join(d, "config.json")
    with open(cp) as f:
        cfg = json.load(f)
    cfg["quantization_config"] = qcfg
    cfg.pop("dtype", None)
    cfg["torch_dtype"] = "float16"
    with open(cp, "w") as f:
        json.dump(cfg, f, indent=2, sort_keys=True)
        f.write("\n")


def logits_of(model, ids):
    with torch.no_grad():
        return model(input_ids=torch.tensor([ids])).logits[0].double().tolist()


def write_fixture(name):
    model, tensors, qcfg, seed = build(name)
    d = os.path.join(FIX, name)
    save_quant(model, tensors, qcfg, d)
    ids = [(seed * 7 + 13 * i + i * i) % V for i in range(T)]
    full = logits_of(model, ids)
    meta = {"tokens": ids, "logits_full": full, "transformers": transformers.__version__, "torch": torch.__version__,
            "seed": seed, "weights": "dequantised from the packed tensors (numpy), float32"}
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    return os.path.getsize(os.path.join(d, "model.safetensors"))


PROMPTS, PROMPT_LEN, NEW = 3, 12, 12
SEQS, SEQ_LEN = 2, 48


def write_refs(model, d, vocab, seed):
    """hf.json, hf-logits.f32, calib.json exactly as tools/audit_e2e.py writes them."""
    rng = np.random.default_rng(seed)
    model.generation_config.eos_token_id = None
    gc = transformers.GenerationConfig(max_new_tokens=NEW, do_sample=False, num_beams=1, eos_token_id=None, pad_token_id=0)
    recs = []
    with torch.no_grad():
        for _ in range(PROMPTS):
            ids = [int(t) for t in rng.integers(0, vocab, size=PROMPT_LEN)]
            t = torch.tensor([ids])
            gen = model.generate(input_ids=t, attention_mask=torch.ones_like(t), generation_config=gc)[0, PROMPT_LEN:].tolist()
            lg = model(input_ids=torch.tensor([ids + gen[:-1]])).logits[0].tolist()
            margins = []
            for row in lg[PROMPT_LEN - 1:]:
                top = sorted(row, reverse=True)[:2]
                margins.append(top[0] - top[1])
            recs.append({"input_ids": ids, "generated": gen, "margins": margins})
        seqs = [[int(t) for t in rng.integers(0, vocab, size=SEQ_LEN)] for _ in range(SEQS)]
        rows = []
        for s in seqs:
            rows.extend(model(input_ids=torch.tensor([s])).logits[0].tolist())
    np.asarray(rows, dtype="<f4").tofile(os.path.join(d, "hf-logits.f32"))
    json.dump({"records": recs, "sequences": seqs, "vocab": vocab, "transformers": transformers.__version__},
              open(os.path.join(d, "hf.json"), "w"))
    calib = [[int(t) for t in rng.integers(0, vocab, size=512)] for _ in range(2)]
    json.dump({"source": "random (audit)", "sequences": calib}, open(os.path.join(d, "calib.json"), "w"))
    return recs


def write_audit(out_root, name):
    model, tensors, qcfg, seed = build(name)
    d = os.path.join(out_root, name)
    # The float twin first: identical weights, no quantization_config, F32 (the dequantised
    # values are exact in f32, not always in f16).
    fd = os.path.join(out_root, name + "-float")
    save_float(model, fd)
    save_quant(model, tensors, qcfg, d)
    recs = write_refs(model, d, V, seed)
    for fn in ("hf.json", "hf-logits.f32", "calib.json"):
        with open(os.path.join(d, fn), "rb") as a, open(os.path.join(fd, fn), "wb") as b:
            b.write(a.read())
    return recs


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1")
    if len(sys.argv) < 2 or sys.argv[1] not in ("fixtures", "audit"):
        sys.exit(__doc__)
    mode = sys.argv[1]
    rest = sys.argv[2:]
    out = None
    if mode == "audit":
        out, rest = rest[0], rest[1:]
    bad = 0
    for n in rest or list(CONFIGS):
        try:
            if mode == "fixtures":
                sz = write_fixture(n)
                print(f"ok   {n:24s} {sz} bytes", flush=True)
            else:
                recs = write_audit(out, n)
                print(f"ok   {n:24s} {[r['generated'][:6] for r in recs]}", flush=True)
        except Exception as e:  # report and continue
            bad += 1
            print(f"FAIL {n:24s} {type(e).__name__}: {str(e)[:300]}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
