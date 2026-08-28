#!/usr/bin/env python3
"""Build a tiny, *real* llama-architecture GGUF that `llama-server` can actually load.

Why this exists: the llama.cpp backend is the one the Studio ships with, and testing it needs an
engine and a model. The engine is a build; the model, until now, was a multi-gigabyte download —
which meant the default backend's load path, streaming path and usage accounting were exercised
only by hand, if at all.

This writes a ~60 kB model with random weights: one layer, 32-dimensional embeddings, a 32-token
vocabulary. It generates nonsense, and that is fine — nothing here tests what a model *says*. It
tests that the Studio can start an engine, wait for it, stream from it, count its tokens, and
derive a runtime identity from the binary that produced them.

    python3 make_tiny_gguf.py out/tiny-llama-F32.gguf

No third-party dependencies: GGUF is a length-prefixed key/value format and this writes it
directly. `gguf-py` would work too, and would put a pip install between a contributor and a test.
"""

import struct
import sys
from pathlib import Path

# --- the model's shape -------------------------------------------------------
# Small enough to be instant, large enough to be a valid llama: the head dimension must divide the
# embedding width, and RoPE needs a head dimension it can rotate.
N_VOCAB = 32
N_EMBD = 32
N_HEAD = 4
N_HEAD_KV = 4
N_LAYER = 1
N_FF = 64
N_CTX = 512
HEAD_DIM = N_EMBD // N_HEAD
RMS_EPS = 1e-5
ALIGNMENT = 32

# GGUF metadata value types.
T_UINT32, T_FLOAT32, T_STRING, T_ARRAY, T_INT32 = 4, 6, 8, 9, 5


def gguf_string(text: str) -> bytes:
    data = text.encode("utf-8")
    return struct.pack("<Q", len(data)) + data


def kv(key: str, value_type: int, payload: bytes) -> bytes:
    return gguf_string(key) + struct.pack("<I", value_type) + payload


def kv_u32(key: str, value: int) -> bytes:
    return kv(key, T_UINT32, struct.pack("<I", value))


def kv_f32(key: str, value: float) -> bytes:
    return kv(key, T_FLOAT32, struct.pack("<f", value))


def kv_str(key: str, value: str) -> bytes:
    return kv(key, T_STRING, gguf_string(value))


def kv_array(key: str, element_type: int, elements: list) -> bytes:
    body = struct.pack("<I", element_type) + struct.pack("<Q", len(elements))
    for element in elements:
        if element_type == T_STRING:
            body += gguf_string(element)
        elif element_type == T_FLOAT32:
            body += struct.pack("<f", element)
        elif element_type == T_INT32:
            body += struct.pack("<i", element)
        else:
            raise ValueError(f"unsupported array element type {element_type}")
    return kv(key, T_ARRAY, body)


class Lcg:
    """A deterministic pseudo-random source, so the fixture is byte-identical every time.

    Byte-identical matters more than it looks: the model's SHA-256 is its identity (`h_M`), and a
    fixture that changed on every run would give a different identity on every test.
    """

    def __init__(self, seed: int = 0x5EED) -> None:
        self.state = seed

    def next_float(self) -> float:
        # Numerical Recipes' LCG; the values only need to be small, varied and reproducible.
        self.state = (self.state * 1664525 + 1013904223) & 0xFFFFFFFF
        return ((self.state >> 8) / 0xFFFFFF - 0.5) * 0.1


def tensors() -> list[tuple[str, list[int]]]:
    """Every tensor a llama model needs, with GGUF dimensions (fastest-varying first)."""
    out: list[tuple[str, list[int]]] = [
        ("token_embd.weight", [N_EMBD, N_VOCAB]),
        ("output_norm.weight", [N_EMBD]),
        ("output.weight", [N_EMBD, N_VOCAB]),
    ]
    for layer in range(N_LAYER):
        out += [
            (f"blk.{layer}.attn_norm.weight", [N_EMBD]),
            (f"blk.{layer}.attn_q.weight", [N_EMBD, N_HEAD * HEAD_DIM]),
            (f"blk.{layer}.attn_k.weight", [N_EMBD, N_HEAD_KV * HEAD_DIM]),
            (f"blk.{layer}.attn_v.weight", [N_EMBD, N_HEAD_KV * HEAD_DIM]),
            (f"blk.{layer}.attn_output.weight", [N_HEAD * HEAD_DIM, N_EMBD]),
            (f"blk.{layer}.ffn_norm.weight", [N_EMBD]),
            (f"blk.{layer}.ffn_gate.weight", [N_EMBD, N_FF]),
            (f"blk.{layer}.ffn_up.weight", [N_EMBD, N_FF]),
            (f"blk.{layer}.ffn_down.weight", [N_FF, N_EMBD]),
        ]
    return out


def vocabulary() -> tuple[list[str], list[float], list[int]]:
    """A 32-token SentencePiece-shaped vocabulary.

    The first three entries are the control tokens llama.cpp expects to find, and `token_type` 3
    marks them as control so the tokenizer does not emit them as text.
    """
    tokens = ["<unk>", "<s>", "</s>"]
    types = [2, 3, 3]
    # `▁` (U+2581) is SentencePiece's word-boundary marker; a vocabulary without it tokenizes no
    # spaces at all.
    for ch in "abcdefghijklmnopqrstuvwxyz":
        tokens.append("▁" + ch)
        types.append(1)
    tokens += ["▁", ".", ","]
    types += [1, 1, 1]
    assert len(tokens) == N_VOCAB, f"vocabulary is {len(tokens)} tokens, expected {N_VOCAB}"
    scores = [0.0] * N_VOCAB
    return tokens, scores, types


def build() -> bytes:
    tokens, scores, types = vocabulary()

    metadata = b"".join(
        [
            kv_str("general.architecture", "llama"),
            kv_str("general.name", "MISAKA Studio test fixture"),
            kv_u32("general.file_type", 0),  # LLAMA_FTYPE_ALL_F32
            kv_u32("general.alignment", ALIGNMENT),
            kv_u32("llama.block_count", N_LAYER),
            kv_u32("llama.context_length", N_CTX),
            kv_u32("llama.embedding_length", N_EMBD),
            kv_u32("llama.feed_forward_length", N_FF),
            kv_u32("llama.attention.head_count", N_HEAD),
            kv_u32("llama.attention.head_count_kv", N_HEAD_KV),
            kv_f32("llama.attention.layer_norm_rms_epsilon", RMS_EPS),
            kv_u32("llama.rope.dimension_count", HEAD_DIM),
            kv_str("tokenizer.ggml.model", "llama"),
            kv_array("tokenizer.ggml.tokens", T_STRING, tokens),
            kv_array("tokenizer.ggml.scores", T_FLOAT32, scores),
            kv_array("tokenizer.ggml.token_type", T_INT32, types),
            kv_u32("tokenizer.ggml.bos_token_id", 1),
            kv_u32("tokenizer.ggml.eos_token_id", 2),
            kv_u32("tokenizer.ggml.unknown_token_id", 0),
            # A chat template, because a model without one is rendered by whatever the engine
            # falls back to — and llama.cpp's fallback is ChatML, whose `<|im_start|>` and
            # `<|im_end|>` are not in this 32-token vocabulary. The engine then throws
            # `unordered_map::at` and answers 400, which is a genuinely terrible error message and
            # took a real run to diagnose. Real models ship a template; so does this fixture. It is
            # deliberately plain text with no special tokens, so it tokenizes in any vocabulary.
            kv_str(
                "tokenizer.chat_template",
                "{% for message in messages %}{{ message.content }} {% endfor %}",
            ),
        ]
    )
    kv_count = 20

    specs = tensors()
    # The tensor index carries each tensor's offset within the data section, so the offsets have to
    # be computed before the index is written — every tensor starts on an alignment boundary.
    infos = b""
    offset = 0
    payloads: list[tuple[int, bytes]] = []
    rng = Lcg()
    for name, dims in specs:
        elements = 1
        for dim in dims:
            elements *= dim
        data = struct.pack(f"<{elements}f", *[rng.next_float() for _ in range(elements)])
        infos += gguf_string(name)
        infos += struct.pack("<I", len(dims))
        for dim in dims:
            infos += struct.pack("<Q", dim)
        infos += struct.pack("<I", 0)  # GGML_TYPE_F32
        infos += struct.pack("<Q", offset)
        payloads.append((offset, data))
        offset += len(data)
        padding = (-offset) % ALIGNMENT
        offset += padding

    header = b"GGUF" + struct.pack("<I", 3) + struct.pack("<Q", len(specs)) + struct.pack("<Q", kv_count)
    front = header + metadata + infos
    front += b"\x00" * ((-len(front)) % ALIGNMENT)

    body = bytearray()
    for tensor_offset, data in payloads:
        body.extend(b"\x00" * (tensor_offset - len(body)))
        body.extend(data)
    return front + bytes(body)


if __name__ == "__main__":
    destination = Path(sys.argv[1] if len(sys.argv) > 1 else "tiny-llama-F32.gguf")
    destination.parent.mkdir(parents=True, exist_ok=True)
    blob = build()
    destination.write_bytes(blob)
    print(f"wrote {destination} ({len(blob)} bytes, {N_LAYER} layer, {N_EMBD}d, {N_VOCAB} tokens)")
