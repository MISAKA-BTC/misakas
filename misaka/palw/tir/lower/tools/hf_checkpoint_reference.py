#!/usr/bin/env python3
"""Pinned, bounded independent source-checkpoint logits, outside consensus.

Use the checkpoint publisher's Transformers implementation and complete safetensors
weights. The supplied source manifest fixes bytes; its revision label remains the
caller's assertion unless separately authenticated to the publisher. This diagnostic
does not establish chat/task quality, integer bit equality, chain admission or Final.
All input, model-parameter and output limits are checked before importing Torch or
loading model tensors. The final reference remains compatible with runtime_pack/hfref.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import struct
import sys
import time

MAX_JSON = 2 << 20
MAX_HEADER = 4 << 20
WIDTHS = {"BOOL": 1, "U8": 1, "I8": 1, "I16": 2, "U16": 2,
          "F16": 2, "BF16": 2, "I32": 4, "U32": 4, "F32": 4,
          "I64": 8, "U64": 8, "F64": 8}


def json_file(path: Path, limit: int = MAX_JSON) -> dict:
    with path.open("rb") as f:
        raw = f.read(limit + 1)
    if len(raw) > limit:
        raise ValueError(f"JSON byte limit: {path.name}")
    doc = json.loads(raw)
    if not isinstance(doc, dict):
        raise ValueError("JSON object required")
    return doc


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def source_pins(source: Path, manifest: dict, maximum: int) -> list[dict]:
    revision = manifest.get("revision")
    if (not isinstance(revision, str) or len(revision) != 40
            or any(c not in "0123456789abcdef" for c in revision)
            or not isinstance(manifest.get("repository"), str)
            or not manifest["repository"]):
        raise ValueError("repository and exact lowercase revision required")
    files = manifest.get("source_files")
    if not isinstance(files, list) or not 1 <= len(files) <= 1024:
        raise ValueError("bounded source file list required")
    seen, total = set(), 0
    pins = []
    for row in files:
        name = row.get("path") if isinstance(row, dict) else None
        if (not isinstance(name, str) or not name or "\\" in name
                or Path(name).is_absolute() or any(x in (".", "..") for x in name.split("/"))
                or name in seen):
            raise ValueError("unique relative source path required")
        seen.add(name)
        size, digest = row.get("bytes"), row.get("sha256")
        if (type(size) is not int or size <= 0 or not isinstance(digest, str)
                or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest)):
            raise ValueError("source size and SHA-256 required")
        total += size
        if total > maximum:
            raise ValueError("source byte limit")
        path = source / name
        if not path.is_file() or path.stat().st_size != size:
            raise ValueError(f"source byte identity: {name}")
        pins.append({"path": name, "bytes": size, "sha256": digest})
    if "config.json" not in seen:
        raise ValueError("config.json must be pinned")
    weights = {p.name for p in source.glob("*.safetensors") if p.is_file()}
    declared = {p["path"] for p in pins if p["path"].endswith(".safetensors")}
    if not weights or weights != declared:
        raise ValueError("all safetensors shards must be pinned; other storage needs another reference route")
    if len(weights) > 1 and "model.safetensors.index.json" not in seen:
        raise ValueError("sharded model index must be pinned")
    return pins


def verify_pins(source: Path, pins: list[dict]) -> None:
    for row in pins:
        path = source / row["path"]
        if path.stat().st_size != row["bytes"] or sha256_file(path) != row["sha256"]:
            raise ValueError(f"source SHA-256 identity: {row['path']}")


def model_bytes(source: Path, pins: list[dict], maximum: int) -> int:
    """Float32 size of all stored tensors; no tensor data is read or allocated."""
    total = 0
    for row in pins:
        if not row["path"].endswith(".safetensors"):
            continue
        with (source / row["path"]).open("rb") as f:
            prefix = f.read(8)
            if len(prefix) != 8:
                raise ValueError("truncated safetensors prefix")
            count = struct.unpack("<Q", prefix)[0]
            if not 2 <= count <= MAX_HEADER or count + 8 > row["bytes"]:
                raise ValueError("safetensors header byte limit")
            header = json.loads(f.read(count))
        if not isinstance(header, dict) or len(header) > 65536:
            raise ValueError("safetensors tensor declaration limit")
        for name, tensor in header.items():
            if name == "__metadata__":
                continue
            shape = tensor.get("shape") if isinstance(tensor, dict) else None
            dtype = tensor.get("dtype") if isinstance(tensor, dict) else None
            offsets = tensor.get("data_offsets") if isinstance(tensor, dict) else None
            if (not isinstance(shape, list) or len(shape) > 16
                    or any(type(n) is not int or not 0 <= n <= (1 << 32) for n in shape)
                    or dtype not in WIDTHS or not isinstance(offsets, list) or len(offsets) != 2
                    or any(type(n) is not int for n in offsets)):
                raise ValueError("safetensors dtype/shape/offset declaration")
            elements = math.prod(shape)
            if (not 0 <= offsets[0] <= offsets[1] <= row["bytes"] - 8 - count
                    or offsets[1] - offsets[0] != elements * WIDTHS[dtype]):
                raise ValueError("safetensors tensor extent")
            total += elements * max(4, WIDTHS[dtype])
            if total > maximum:
                raise ValueError("float32 model parameter byte limit")
    return total


def bounded_sequences(tokens: dict, vocab: int, context: int, positions: int, values: int) -> list[list[int]]:
    seqs = tokens.get("sequences")
    if (type(vocab) is not int or vocab <= 0 or not isinstance(seqs, list)
            or not 1 <= len(seqs) <= 256 or min(context, positions, values) <= 0):
        raise ValueError("vocabulary and bounded nonempty sequences required")
    total = 0
    for seq in seqs:
        if (not isinstance(seq, list) or not 1 <= len(seq) <= context
                or any(type(t) is not int or not 0 <= t < vocab for t in seq)):
            raise ValueError("token, sequence or context limit")
        total += len(seq)
        if total > positions or total * vocab > values:
            raise ValueError("position or reference logit value limit")
    return seqs


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source", type=Path)
    ap.add_argument("source_manifest", type=Path)
    ap.add_argument("tokens", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--context", type=int, required=True)
    ap.add_argument("--max-positions", type=int, default=256)
    ap.add_argument("--max-logit-values", type=int, default=64 << 20)
    ap.add_argument("--max-source-bytes", type=int, default=8 << 30)
    ap.add_argument("--max-model-parameter-bytes", type=int, default=8 << 30)
    a = ap.parse_args()
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        raise ValueError("HF_HUB_OFFLINE=1 required")
    if a.out.exists():
        raise ValueError("output exists; use a fresh evidence directory")
    if min(a.max_source_bytes, a.max_model_parameter_bytes) <= 0:
        raise ValueError("positive source/model byte limits required")
    manifest = json_file(a.source_manifest)
    pins = source_pins(a.source, manifest, a.max_source_bytes)
    config = json_file(a.source / "config.json")
    seqs = bounded_sequences(json_file(a.tokens), config.get("vocab_size"), a.context,
                             a.max_positions, a.max_logit_values)
    if not 0 < a.context <= config.get("max_position_embeddings", 0):
        raise ValueError("context exceeds source model declaration")
    parameter_bytes = model_bytes(a.source, pins, a.max_model_parameter_bytes)
    verify_pins(a.source, pins)
    started = time.monotonic()
    import numpy as np
    import torch
    import transformers
    from transformers import AutoModelForCausalLM

    model = AutoModelForCausalLM.from_pretrained(
        a.source, torch_dtype=torch.float32, attn_implementation="eager",
        trust_remote_code=False, local_files_only=True,
    ).eval()
    vocab = int(model.get_output_embeddings().weight.shape[0])
    if vocab != config["vocab_size"]:
        raise ValueError("loaded model vocabulary differs from checked declaration")
    a.out.mkdir(parents=True, exist_ok=False)
    logits_path = a.out / "hf-reference.f32"
    # Stream complete sequences separately; neither accumulate all logits nor pad prompts.
    with logits_path.open("xb") as out, torch.inference_mode():
        for index, seq in enumerate(seqs):
            lg = model(input_ids=torch.tensor([seq]), use_cache=False).logits[0].float().numpy()
            if lg.shape != (len(seq), vocab) or not np.isfinite(lg).all():
                raise ValueError("reference logit shape or finite-value requirement")
            out.write(np.asarray(lg, dtype="<f4").tobytes())
            print(f"source sequence {index + 1}/{len(seqs)}: {len(seq)} positions", file=sys.stderr, flush=True)
    verify_pins(a.source, pins)
    elapsed = time.monotonic() - started
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform != "darwin":
        rss *= 1024
    doc = {
        "schema": "misaka.palw.hf-reference.v1",
        "producer": {
            "implementation": "Hugging Face Transformers publisher-selected built-in model class",
            "model_class": type(model).__name__, "transformers": transformers.__version__,
            "torch": torch.__version__, "numpy": np.__version__,
            "reference_dtype": "float32", "source_storage": "safetensors", "device": "cpu",
            "attention": "eager", "use_cache": False, "trust_remote_code": False,
            "repository": manifest["repository"], "revision": manifest["revision"],
            "source_manifest_sha256": sha256_file(a.source_manifest),
            "token_file_sha256": sha256_file(a.tokens), "source_files": pins,
            "source_before_and_after_sha256_match": True,
            "source_revision_label_is_authenticated_by_this_tool": False,
            "elapsed_seconds": elapsed, "whole_process_max_rss_bytes": rss,
            "limits": {"context": a.context, "positions": a.max_positions,
                       "logit_values": a.max_logit_values, "source_bytes": a.max_source_bytes,
                       "model_parameter_bytes": a.max_model_parameter_bytes},
            "declared_float32_parameter_bytes": parameter_bytes,
            "diagnostic_is_complete_task_or_chain_admission": False,
        },
        "vocab": vocab, "logits_file": logits_path.name,
        "sequences": [{"tokens": seq} for seq in seqs],
    }
    with (a.out / "hf-reference.json").open("x") as out:
        json.dump(doc, out, indent=2, allow_nan=False)
        out.write("\n")
    print(json.dumps({"positions": sum(map(len, seqs)), "elapsed_seconds": elapsed,
                      "max_rss_bytes": rss, "logits_sha256": sha256_file(logits_path)}))


if __name__ == "__main__":
    main()
