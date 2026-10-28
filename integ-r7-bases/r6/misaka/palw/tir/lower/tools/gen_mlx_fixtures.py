#!/usr/bin/env python3
"""MLX affine quantisation (`MLX_AFFINE`, `MLX_QUANT_V1`): the descriptor's test vectors and the end-to-end fixtures, made with
MLX ITSELF as the quantiser and an independent numpy unpacker as the reference (HFX, 2026-10-10).

MLX (`mlx.core.quantize`, mode "affine") stores a quantised module as

    <module>.weight  uint32 [out, in * bits / 32]   the codes, a little-endian bit stream per row: code i is bits [i*bits, (i+1)*bits)
    <module>.scales  float  [out, in / group_size]  (the model's float type: float16, bfloat16 or float32)
    <module>.biases  float  [out, in / group_size]

and a weight is W[o, i] = scales[o, g] * q[o, i] + biases[o, g], g = i // group_size, bits in {2, 3, 4, 5, 6, 8} (3, 5 and 6 bits
straddle the uint32 words). The served value is that number computed in f64 (exact for float16 / bfloat16 operands) and rounded
once to float32. MLX's own `dequantize` on the GPU with float32 scales is a fused multiply-add, i.e. the same single rounding; on
the CPU it rounds twice (multiply, then add), and with float16 / bfloat16 scales it returns the model's float type: the vectors
are taken from the GPU with the scales and biases widened to float32 first.

Two interpreters: MLX lives in the system Python (`MLX_PYTHON`, default /opt/homebrew/bin/python3; no numpy there), transformers
and torch in the TIR venv. Modes:

  $MLX_PYTHON gen_mlx_fixtures.py vectors
      the descriptor's `tests` (JSON on stdout): one module per bit width, float16 / bfloat16 scales, group sizes 32 and 64
  $MLX_PYTHON gen_mlx_fixtures.py pack PLAN.json IN.safetensors OUT.safetensors
      quantise IN's 2-D weights with MLX as PLAN says, write MLX's checkpoint (what mlx-lm saves)
  HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 tir-venv/bin/python -I gen_mlx_fixtures.py fixtures [name ...]
      tests/fixtures/hf-quant/<name>/{config.json, model.safetensors, logits.json}: a tiny random decoder (transformers' own
      config class), quantised by MLX (`pack`, run in $MLX_PYTHON), its reference logits from transformers with the weights
      dequantised from the PACKED file by the numpy unpacker below (not from MLX's arrays), so a packing mistake on either side
      shows as a mismatch against the lowerer's own decode.
"""

import json
import os
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-quant")
MLX_PYTHON = os.environ.get("MLX_PYTHON", "/opt/homebrew/bin/python3")

V = 128
T = 16
BASE = dict(hidden_size=128, intermediate_size=256, num_attention_heads=4, num_key_value_heads=2, vocab_size=V, num_hidden_layers=2,
            max_position_embeddings=1024)
MODELS = {
    "llama": ("llama", "LlamaForCausalLM", dict(BASE)),
    # Qwen3: q/k norms, tied embeddings (the head reads the quantised embedding table).
    "qwen3": ("qwen3", "Qwen3ForCausalLM", dict(BASE, head_dim=32, tie_word_embeddings=True)),
}

# name -> (model, scales dtype, the quantisation block, which config keys carry it)
#   keys: "both" (mlx-lm since it writes `quantization_config` too), "quantization" (an older mlx-lm: that key alone)
FIXTURES = {
    "mlx_b4_g32": ("llama", "float16", {"group_size": 32, "bits": 4}, "both"),
    # 3 bits straddle the uint32 words; bfloat16 scales; the head is the quantised embedding table.
    "mlx_b3_g64_bf16": ("qwen3", "bfloat16", {"group_size": 64, "bits": 3, "mode": "affine"}, "quantization"),
    # Per-module entries: a projection at 8 bits / group 64, the embedding at 5 bits, one projection kept in float.
    "mlx_b6_mixed": ("llama", "float16", {
        "group_size": 32, "bits": 6, "mode": "affine",
        "model.layers.0.mlp.down_proj": {"group_size": 64, "bits": 8},
        "model.embed_tokens": {"group_size": 32, "bits": 5, "mode": "affine"},
        "model.layers.1.self_attn.o_proj": False,
    }, "both"),
}


# ───────────────────────────── the MLX side (system Python, no numpy) ─────────────────────────────

def mlx_vectors():
    import mlx.core as mx
    mx.random.seed(20261010)
    out = []
    for bits, gs, dt in ((2, 32, "float16"), (3, 32, "bfloat16"), (4, 64, "float16"), (5, 32, "float16"), (6, 64, "bfloat16"), (8, 32, "float16")):
        dtype = getattr(mx, dt)
        rows, cols = 2, 64
        w = (mx.random.normal((rows, cols)) * 0.05).astype(dtype)
        wq, s, b = mx.quantize(w, group_size=gs, bits=bits)
        vals = mx.dequantize(wq, s.astype(mx.float32), b.astype(mx.float32), group_size=gs, bits=bits, stream=mx.gpu)
        mx.eval(vals)
        # The definition, independently: f64 scale * code + bias (exact for these operands), rounded once to float32.
        S, B, W = s.astype(mx.float32).tolist(), b.astype(mx.float32).tolist(), wq.tolist()
        for o in range(rows):
            stream = sum(int(x) << (32 * k) for k, x in enumerate(W[o]))
            for i in range(cols):
                q = (stream >> (i * bits)) & ((1 << bits) - 1)
                want = struct.unpack("<f", struct.pack("<f", S[o][i // gs] * q + B[o][i // gs]))[0]
                if want != vals[o, i].item():
                    raise SystemExit(f"bits {bits}: [{o}, {i}] MLX {vals[o, i].item()} against the definition {want}")
        out.append({
            "config": {"quant_method": "mlx", "group_size": gs, "bits": bits},
            "roles": {
                "weight": {"dtype": "U32", "shape": list(wq.shape), "hex": u32_hex(wq.tolist())},
                "scales": {"dtype": "F16" if dt == "float16" else "BF16", "shape": list(s.shape), "hex": f_hex(s, dt)},
                "biases": {"dtype": "F16" if dt == "float16" else "BF16", "shape": list(b.shape), "hex": f_hex(b, dt)},
            },
            "values_f32_hex": "".join(struct.pack("<f", x).hex() for row in vals.tolist() for x in row),
        })
    json.dump(out, sys.stdout, indent=1)
    sys.stdout.write("\n")


def u32_hex(rows):
    return "".join(struct.pack("<I", int(x)).hex() for row in rows for x in row)


def f_hex(a, dt):
    """A float16 / bfloat16 array's little-endian bytes, read through uint16 (MLX's `view`)."""
    import mlx.core as mx
    bits = a.view(mx.uint16).tolist()
    return "".join(struct.pack("<H", int(x)).hex() for row in bits for x in row)


def mlx_pack(plan_path, src, dst):
    """Quantise `src`'s 2-D weights with MLX as the plan says (mlx-lm's rule: every 2-D `.weight` whose input width the group
    size divides, unless the plan keeps it in float); every other tensor is cast to the plan's float type."""
    import mlx.core as mx
    plan = json.load(open(plan_path))
    dtype = getattr(mx, plan["dtype"])
    block = plan["block"]
    default = {"group_size": block["group_size"], "bits": block["bits"]}
    tensors = mx.load(src)
    out = {}
    for name, t in sorted(tensors.items()):
        t = t.astype(dtype)
        module = name[: -len(".weight")] if name.endswith(".weight") else None
        entry = block.get(module, True) if module else False
        if module is None or t.ndim != 2 or entry is False:
            out[name] = t
            continue
        p = dict(default)
        if isinstance(entry, dict):
            p.update({k: v for k, v in entry.items() if k in ("group_size", "bits")})
        if t.shape[1] % p["group_size"]:
            out[name] = t
            continue
        wq, s, b = mx.quantize(t, group_size=p["group_size"], bits=p["bits"])
        out[name] = wq
        out[module + ".scales"] = s
        out[module + ".biases"] = b
    mx.save_safetensors(dst, out, metadata={"format": "mlx"})


# ───────────────────────────── the reference side (TIR venv) ─────────────────────────────

def read_safetensors(path):
    """A minimal safetensors reader (numpy): {name: array} with float16 / bfloat16 widened to float64, uint32 as uint64."""
    import numpy as np
    raw = open(path, "rb").read()
    n = struct.unpack("<Q", raw[:8])[0]
    header = json.loads(raw[8:8 + n])
    base = 8 + n
    out = {}
    for name, h in header.items():
        if name == "__metadata__":
            continue
        a, b = h["data_offsets"]
        buf = raw[base + a: base + b]
        dt, shape = h["dtype"], h["shape"]
        if dt == "F32":
            v = np.frombuffer(buf, "<f4").astype(np.float64)
        elif dt == "F16":
            v = np.frombuffer(buf, "<f2").astype(np.float64)
        elif dt == "BF16":
            v = (np.frombuffer(buf, "<u2").astype(np.uint32) << 16).view(np.float32).astype(np.float64)
        elif dt == "U32":
            v = np.frombuffer(buf, "<u4").astype(np.uint64)
        else:
            raise ValueError(f"{name}: dtype {dt}")
        out[name] = (dt, v.reshape(shape))
    return out


def unpack_codes(words, bits, cols):
    """Codes of each row from its uint32 words: code i is bits [i*bits, (i+1)*bits) of the row's little-endian bit stream."""
    import numpy as np
    rows = words.shape[0]
    padded = np.concatenate([words, np.zeros((rows, 1), dtype=np.uint64)], axis=1)
    i = np.arange(cols, dtype=np.uint64)
    bit = i * np.uint64(bits)
    w, s = bit // np.uint64(32), bit % np.uint64(32)
    lo = padded[:, w] >> s
    hi = padded[:, w + np.uint64(1)] << (np.uint64(32) - s)
    straddle = (s + np.uint64(bits)) > np.uint64(32)
    v = np.where(straddle[None, :], lo | hi, lo)
    return (v & np.uint64((1 << bits) - 1)).astype(np.int64)


def dequantised(ck, block):
    """The float32 weights the packed file defines, by module: f64 scale * code + bias, rounded once to float32."""
    import numpy as np
    out = {}
    for name, (dt, v) in ck.items():
        if name.endswith((".scales", ".biases")):
            continue
        if dt == "U32":
            module = name[: -len(".weight")]
            entry = block.get(module, True)
            p = {"group_size": block["group_size"], "bits": block["bits"]}
            if isinstance(entry, dict):
                p.update({k: x for k, x in entry.items() if k in ("group_size", "bits")})
            cols = v.shape[1] * 32 // p["bits"]
            q = unpack_codes(v, p["bits"], cols)
            g = np.arange(cols) // p["group_size"]
            s, b = ck[module + ".scales"][1], ck[module + ".biases"][1]
            out[name] = (s[:, g] * q + b[:, g]).astype(np.float32)
        else:
            out[name] = v.astype(np.float32)
    return out


def build(name):
    import math
    import numpy as np
    import torch
    import transformers
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING
    from safetensors.torch import save_file
    model_name, dtype, block, keys = FIXTURES[name]
    model_type, arch, kw = MODELS[model_name]
    seed = sum(ord(c) for c in name) + 91
    cfg = CONFIG_MAPPING[model_type](**kw)
    cfg.architectures = [arch]
    torch.manual_seed(seed)
    model = AutoModelForCausalLM.from_config(cfg)
    model.eval()
    g = torch.Generator().manual_seed(seed + 1)
    with torch.no_grad():
        for pname, p in model.named_parameters():
            if p.ndim >= 2 and "embed" in pname:
                p.copy_(torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(cfg.hidden_size))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="mlxfix-") as tmp:
        state = {k: v.detach().clone().contiguous() for k, v in model.state_dict().items()}
        if cfg.tie_word_embeddings:
            state.pop("lm_head.weight", None)
        save_file(state, os.path.join(tmp, "float.safetensors"), metadata={"format": "pt"})
        plan = os.path.join(tmp, "plan.json")
        json.dump({"dtype": dtype, "block": block}, open(plan, "w"))
        subprocess.run([MLX_PYTHON, os.path.abspath(__file__), "pack", plan, os.path.join(tmp, "float.safetensors"),
                        os.path.join(d, "model.safetensors")], check=True)
        model.save_pretrained(tmp)
        config = json.load(open(os.path.join(tmp, "config.json")))
    ck = read_safetensors(os.path.join(d, "model.safetensors"))
    deq = dequantised(ck, block)
    ref = AutoModelForCausalLM.from_config(cfg)
    ref.eval()
    missing, unexpected = ref.load_state_dict({k: torch.from_numpy(v) for k, v in deq.items()}, strict=False)
    missing = [m for m in missing if not (cfg.tie_word_embeddings and m == "lm_head.weight")]
    if missing or unexpected:
        raise RuntimeError(f"{name}: the packed file does not load: missing {missing}, unexpected {unexpected}")
    if cfg.tie_word_embeddings:
        ref.tie_weights()
    config.pop("dtype", None)
    config["torch_dtype"] = dtype
    if keys in ("both", "quantization"):
        config["quantization"] = block
    if keys == "both":
        config["quantization_config"] = block
    with open(os.path.join(d, "config.json"), "w") as f:
        json.dump(config, f, indent=2, sort_keys=True)
        f.write("\n")
    ids = [(seed * 7 + 13 * i + i * i) % V for i in range(T)]
    with torch.no_grad():
        full = ref(input_ids=torch.tensor([ids])).logits[0].double().tolist()
    meta = {"tokens": ids, "logits_full": full, "transformers": transformers.__version__, "torch": torch.__version__, "seed": seed,
            "weights": "quantised by MLX (mlx.core.quantize, affine), dequantised from the packed file (numpy: f64 scale * code + bias, "
                       "rounded once to float32)"}
    with open(os.path.join(d, "logits.json"), "w") as f:
        json.dump(meta, f)
    return os.path.getsize(os.path.join(d, "model.safetensors"))


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "vectors":
        mlx_vectors()
    elif mode == "pack":
        mlx_pack(*sys.argv[2:5])
    elif mode == "fixtures":
        if os.environ.get("HF_HUB_OFFLINE") != "1":
            sys.exit("refusing to run without HF_HUB_OFFLINE=1")
        bad = 0
        for n in sys.argv[2:] or list(FIXTURES):
            try:
                print(f"ok   {n:20s} {build(n)} bytes", flush=True)
            except Exception as e:
                bad += 1
                print(f"FAIL {n:20s} {type(e).__name__}: {str(e)[:400]}", flush=True)
        sys.exit(1 if bad else 0)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
