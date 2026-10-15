#!/usr/bin/env python3
"""Write a tiny synthetic Gemma-4 Unified checkpoint (genesis 8 #4's architecture) — stdlib only, deterministic.

`Gemma4UnifiedForConditionalGeneration` at toy scale with the 12B's structure: sliding layers on the default rope with
their own q/k/v projections, a full layer with its own head width and ONE KV head (global_head_dim,
num_global_key_value_heads), k = v there (attention_k_eq_v: no v_proj), four sandwich norms, per-head q/k norms, the
layer scalar, the GeGLU MLP, the final logit soft-cap, tied embeddings, and the image/audio embedders the text route
ignores. Values are a fixed LCG, not a trained model: the fixture exercises lowering, execution and the court.

    python3 scripts/genesis/gen_tiny_gemma4_unified.py [--out misaka/palw/tir/lower/tests/fixtures/genesis8-tiny/gemma4_unified]
"""

from __future__ import annotations

import argparse
import json
import struct
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DEFAULT_OUT = ROOT / "misaka/palw/tir/lower/tests/fixtures/genesis8-tiny/gemma4_unified"

HIDDEN, HEADS, HD, KV, GHD, GKV, INTER, VOCAB = 32, 4, 8, 2, 16, 1, 64, 64
TYPES = ["sliding_attention", "sliding_attention", "full_attention"]


class Lcg:
    def __init__(self, seed: int):
        self.x = seed

    def uniform(self) -> float:
        self.x = (self.x * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        return ((self.x >> 11) / float(1 << 53)) * 2.0 - 1.0  # [-1, 1)


def bf16(v: float) -> bytes:
    (u,) = struct.unpack("<I", struct.pack("<f", v))
    u = (u + 0x7FFF + ((u >> 16) & 1)) >> 16  # round to nearest even
    return struct.pack("<H", u & 0xFFFF)


def config() -> dict:
    text = {
        "attention_bias": False, "attention_dropout": 0.0, "attention_k_eq_v": True, "bos_token_id": 2,
        "enable_moe_block": False, "eos_token_id": 1, "final_logit_softcapping": 30.0, "global_head_dim": GHD,
        "head_dim": HD, "hidden_activation": "gelu_pytorch_tanh", "hidden_size": HIDDEN, "hidden_size_per_layer_input": 0,
        "initializer_range": 0.02, "intermediate_size": INTER, "layer_types": TYPES, "max_position_embeddings": 128,
        "model_type": "gemma4_unified_text", "moe_intermediate_size": None, "num_attention_heads": HEADS, "num_experts": None,
        "num_global_key_value_heads": GKV, "num_hidden_layers": len(TYPES), "num_key_value_heads": KV, "num_kv_shared_layers": 0,
        "pad_token_id": 0, "rms_norm_eps": 1e-06,
        "rope_parameters": {"full_attention": {"partial_rotary_factor": 0.25, "rope_theta": 1000000.0, "rope_type": "proportional"},
                            "sliding_attention": {"rope_theta": 10000.0, "rope_type": "default"}},
        "sliding_window": 4, "tie_word_embeddings": True, "top_k_experts": None, "use_bidirectional_attention": "vision",
        "use_cache": True, "use_double_wide_mlp": False, "vocab_size": VOCAB, "vocab_size_per_layer_input": VOCAB,
    }
    return {
        "architectures": ["Gemma4UnifiedForConditionalGeneration"],
        "audio_config": {"audio_embed_dim": 8, "audio_samples_per_token": 8, "hidden_size": 8, "model_type": "gemma4_unified_audio",
                         "output_proj_dims": 8, "rms_norm_eps": 1e-06},
        "audio_token_id": 60, "boa_token_id": 61, "boi_token_id": 62, "dtype": "bfloat16", "eoa_token_index": 63, "eoi_token_id": 59,
        "eos_token_id": [1], "image_token_id": 58, "initializer_range": 0.02, "model_type": "gemma4_unified", "text_config": text,
        "tie_word_embeddings": True, "transformers_version": "5.10.0.dev0", "video_token_id": 57,
        "vision_config": {"mm_embed_dim": HIDDEN, "mm_posemb_size": 4, "model_patch_size": 4, "model_type": "gemma4_unified_vision",
                          "num_soft_tokens": 4, "output_proj_dims": HIDDEN, "patch_size": 2, "pooling_kernel_size": 2, "rms_norm_eps": 1e-06},
    }


def tensors() -> list[tuple[str, list[int], str]]:
    """(name, shape, init) in a fixed order; init: 'norm' (≈1), 'scalar', 'proj'."""
    p = "model.language_model."
    out = [(p + "embed_tokens.weight", [VOCAB, HIDDEN], "proj")]
    for i, t in enumerate(TYPES):
        full = t == "full_attention"
        hd, kv = (GHD, GKV) if full else (HD, KV)
        L = f"{p}layers.{i}."
        out += [
            (L + "input_layernorm.weight", [HIDDEN], "norm"),
            (L + "post_attention_layernorm.weight", [HIDDEN], "norm"),
            (L + "pre_feedforward_layernorm.weight", [HIDDEN], "norm"),
            (L + "post_feedforward_layernorm.weight", [HIDDEN], "norm"),
            (L + "layer_scalar", [1], "scalar"),
            (L + "mlp.gate_proj.weight", [INTER, HIDDEN], "proj"),
            (L + "mlp.up_proj.weight", [INTER, HIDDEN], "proj"),
            (L + "mlp.down_proj.weight", [HIDDEN, INTER], "proj"),
            (L + "self_attn.q_proj.weight", [HEADS * hd, HIDDEN], "proj"),
            (L + "self_attn.k_proj.weight", [kv * hd, HIDDEN], "proj"),
            (L + "self_attn.o_proj.weight", [HIDDEN, HEADS * hd], "proj"),
            (L + "self_attn.q_norm.weight", [hd], "norm"),
            (L + "self_attn.k_norm.weight", [hd], "norm"),
        ]
        if not full:
            out.append((L + "self_attn.v_proj.weight", [kv * hd, HIDDEN], "proj"))
    out.append((p + "norm.weight", [HIDDEN], "norm"))
    # The embedders the text route ignores (present, as in the real checkpoint).
    out += [
        ("model.vision_embedder.patch_ln1.weight", [48], "norm"), ("model.vision_embedder.patch_ln1.bias", [48], "proj"),
        ("model.vision_embedder.patch_dense.weight", [HIDDEN, 48], "proj"), ("model.vision_embedder.patch_dense.bias", [HIDDEN], "proj"),
        ("model.vision_embedder.patch_ln2.weight", [HIDDEN], "norm"), ("model.vision_embedder.patch_ln2.bias", [HIDDEN], "proj"),
        ("model.vision_embedder.pos_embedding", [4, 2, HIDDEN], "proj"),
        ("model.vision_embedder.pos_norm.weight", [HIDDEN], "norm"), ("model.vision_embedder.pos_norm.bias", [HIDDEN], "proj"),
        ("model.embed_vision.embedding_projection.weight", [HIDDEN, HIDDEN], "proj"),
        ("model.embed_audio.embedding_projection.weight", [HIDDEN, 8], "proj"),
    ]
    return out


def write(out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    rng = Lcg(0x6E4A)
    header, data = {}, bytearray()
    for name, shape, init in tensors():
        n = 1
        for d in shape:
            n *= d
        begin = len(data)
        for _ in range(n):
            u = rng.uniform()
            v = 1.0 + 0.2 * u if init == "norm" else (0.75 + 0.25 * u if init == "scalar" else 0.25 * u)
            data += bf16(v)
        header[name] = {"dtype": "BF16", "shape": shape, "data_offsets": [begin, len(data)]}
    header["__metadata__"] = {"format": "pt", "generator": "scripts/genesis/gen_tiny_gemma4_unified.py"}
    h = json.dumps(header, separators=(",", ":")).encode()
    h += b" " * ((8 - len(h) % 8) % 8)
    (out / "model.safetensors").write_bytes(struct.pack("<Q", len(h)) + h + bytes(data))
    (out / "config.json").write_text(json.dumps(config(), indent=2) + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    a = ap.parse_args()
    write(a.out)
    print(f"wrote {a.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
