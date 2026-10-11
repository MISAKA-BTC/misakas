#!/usr/bin/env python3
"""Rescale the tied embedding/head of an MLX fixture so its random logits fit the Q24 logit format (HFX 2026-10-10).

`LOGITS_Q24_V1` holds |logit| < 128 (the lowering refuses a calibrated range past 120). `mlx_b3_g64_bf16`'s random 3-bit embedding table
(std 1, tied to the head) gave logits up to 123.1 — "a property of the checkpoint, not of the lowering". MLX is not needed to fix that:
the table's `scales` and `biases` are halved (a power of two: exact in bfloat16), which halves the tied head's logits, and the reference
logits are recomputed exactly as `gen_mlx_fixtures.py fixtures` computes them (transformers on the weights the PACKED file defines).

    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 tir-venv/bin/python -I tools/rescale_mlx_fixture.py mlx_b3_g64_bf16 model.embed_tokens 0.5
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gen_mlx_fixtures as g  # noqa: E402


def main():
    name, module, factor = sys.argv[1], sys.argv[2], float(sys.argv[3])
    assert factor in (0.5, 0.25, 2.0), "a power of two keeps bfloat16 / float16 scales exact"
    d = os.path.join(g.FIX, name)
    path = os.path.join(d, "model.safetensors")
    raw = bytearray(open(path, "rb").read())
    n = struct.unpack("<Q", raw[:8])[0]
    header = json.loads(raw[8 : 8 + n])
    base = 8 + n
    import numpy as np

    for suffix in (".scales", ".biases"):
        h = header[module + suffix]
        a, b = h["data_offsets"]
        dt = h["dtype"]
        if dt == "BF16":
            v = (np.frombuffer(bytes(raw[base + a : base + b]), "<u2").astype(np.uint32) << 16).view(np.float32) * np.float32(factor)
            out = (v.view(np.uint32) >> 16).astype("<u2").tobytes()  # exact: halving changes only the exponent
        elif dt == "F16":
            v = np.frombuffer(bytes(raw[base + a : base + b]), "<f2").astype(np.float32) * np.float32(factor)
            out = v.astype("<f2").tobytes()
        else:
            raise SystemExit(f"{module}{suffix}: dtype {dt}")
        assert len(out) == b - a
        raw[base + a : base + b] = out
    open(path, "wb").write(bytes(raw))
    # The reference logits, as the generator computes them.
    import torch
    import transformers  # noqa: F401
    from transformers import AutoModelForCausalLM
    from transformers.models.auto.configuration_auto import CONFIG_MAPPING

    model_name, dtype, block, keys = g.FIXTURES[name]
    model_type, arch, kw = g.MODELS[model_name]
    seed = sum(ord(c) for c in name) + 91
    cfg = CONFIG_MAPPING[model_type](**kw)
    cfg.architectures = [arch]
    ck = g.read_safetensors(path)
    deq = g.dequantised(ck, block)
    ref = AutoModelForCausalLM.from_config(cfg)
    ref.eval()
    missing, unexpected = ref.load_state_dict({k: torch.from_numpy(v) for k, v in deq.items()}, strict=False)
    missing = [m for m in missing if not (cfg.tie_word_embeddings and m == "lm_head.weight")]
    if missing or unexpected:
        raise RuntimeError(f"{name}: the packed file does not load: missing {missing}, unexpected {unexpected}")
    if cfg.tie_word_embeddings:
        ref.tie_weights()
    ids = [(seed * 7 + 13 * i + i * i) % g.V for i in range(g.T)]
    with torch.no_grad():
        full = ref(input_ids=torch.tensor([ids])).logits[0].double().tolist()
    meta = json.load(open(os.path.join(d, "logits.json")))
    assert meta["tokens"] == ids
    meta["logits_full"] = full
    meta["rescaled"] = f"{module} scales and biases x {factor} (tools/rescale_mlx_fixture.py): the random logits fit LOGITS_Q24_V1"
    json.dump(meta, open(os.path.join(d, "logits.json"), "w"))
    print(name, "max|logit|", max(abs(x) for r in full for x in r))


if __name__ == "__main__":
    main()
