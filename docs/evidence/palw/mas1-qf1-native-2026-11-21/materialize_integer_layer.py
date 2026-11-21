"""Recover one real QF1 layer's frozen integer source parameters from the independent upstream revision."""
import argparse
import hashlib
import json
import math
import tempfile
from pathlib import Path

from check_integer_weights import HERE, MODEL, REVISION, encode
from check_qwen_source import expected_shapes
from fetch_qf1_attention import read_range


def materialize(layer, manifest, index, directory):
    if directory.exists():
        raise ValueError("output directory must be new")
    geometry = json.loads((HERE / "upstream-geometry.json").read_text())
    source = json.loads((HERE / "upstream-payloads.json").read_text())
    if any((x["model"], x["revision"]) != (MODEL, REVISION) for x in (geometry, source, manifest)):
        raise ValueError("incorrect model revision")
    if hashlib.sha256(index).hexdigest() != geometry["source_files_sha256"]["model.safetensors.index.json"]:
        raise ValueError("incorrect upstream index")
    placement = json.loads(index)["weight_map"]
    sizes = {s["file"]: s["bytes"] for s in geometry["source_shards"]}
    prefix = f"model.layers.{layer}."
    shapes = {k: v for k, v in expected_shapes().items() if k.startswith(prefix)}
    if len(shapes) != 11:
        raise ValueError("expected the eleven Qwen3 block tensors")
    headers = {}
    with tempfile.TemporaryDirectory(dir=directory.parent) as temp:
        out = Path(temp) / "integers"
        out.mkdir()
        for name, shape in sorted(shapes.items()):
            shard = placement[name]
            size = sizes[shard]
            if shard not in headers:
                n = int.from_bytes(read_range(0, 7, shard, size), "little")
                if not 0 < n <= 1024**2:
                    raise ValueError("source header length")
                headers[shard] = (8+n, json.loads(read_range(8, 7+n, shard, size)))
            offset, header = headers[shard]
            entry = header[name]
            start, end = entry["data_offsets"]
            if entry["shape"] != shape or entry["dtype"] != "BF16" or end-start != math.prod(shape)*2:
                raise ValueError(f"upstream geometry mismatch: {name}")
            data = read_range(offset+start, offset+end-1, shard, size)
            if hashlib.sha256(data).hexdigest() != source["tensor_sha256"][name]:
                raise ValueError(f"upstream source mismatch: {name}")
            dtype = "i1" if len(shape) == 2 else "<i4"
            record = manifest["tensors"][name]
            if record["shape"] != shape or record["dtype"] != dtype:
                raise ValueError(f"incorrect destination geometry: {name}")
            codes = hashlib.sha256()
            peaks = hashlib.sha256()
            chunk_size = shape[-1]*2 * max(1, 1024**2 // shape[-1])
            with (out / (name + ".codes")).open("wb") as f:
                peak_bytes = bytearray()
                for i in range(0, len(data), chunk_size):
                    block, scales = encode(data[i:i+chunk_size], shape[-1], dtype)
                    f.write(block)
                    codes.update(block)
                    peaks.update(scales)
                    peak_bytes.extend(scales)
            if codes.hexdigest() != record["codes_sha256"]:
                raise ValueError(f"incorrect integer tensor: {name}")
            if len(shape) == 2:
                if peaks.hexdigest() != record["absmax_bf16le_sha256"]:
                    raise ValueError(f"incorrect row scales: {name}")
                (out / (name + ".row_absmax.bf16")).write_bytes(peak_bytes)
        out.rename(directory)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("layer", type=int, choices=range(36))
    p.add_argument("manifest", type=Path)
    p.add_argument("source_index", type=Path)
    p.add_argument("new_directory", type=Path)
    args = p.parse_args()
    materialize(args.layer, json.loads(args.manifest.read_text()), args.source_index.read_bytes(), args.new_directory)
