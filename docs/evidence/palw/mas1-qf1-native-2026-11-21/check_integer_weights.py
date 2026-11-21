"""Stream the pinned 8B model into QF1 integer parameter digests, before activation calibration."""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np
from check_qwen_source import MODEL, REVISION, expected_shapes
from check_qwen_weights import open_shard

HERE = Path(__file__).resolve().parent


def encode(raw, width, dtype):
    bf16 = np.frombuffer(raw, dtype="<u2").reshape(-1, width)
    absolute = bf16 & 0x7fff
    peaks = absolute.max(axis=1)
    if np.any(peaks >= 0x7f80):
        raise ValueError("nonfinite source parameter")
    floats = (bf16.astype("<u4") << 16).view("<f4").astype(np.float64)
    if dtype == "<i4":
        value = floats * (1 << 20)
    else:
        limit = np.iinfo(dtype).max
        scales = (peaks.astype("<u4") << 16).view("<f4").astype(np.float64) / limit
        scales[scales == 0] = 1
        value = floats / scales[:, None]
    magnitude = np.abs(value)
    floor = np.floor(magnitude)
    value = np.copysign(floor + ((magnitude - floor) >= 0.5), value)
    bounds = np.iinfo(dtype)
    if np.any(value < (bounds.min if dtype == "<i4" else -bounds.max)) or np.any(value > bounds.max):
        raise ValueError("converted parameter outside its declared range")
    return value.astype(dtype).tobytes(), (b"" if dtype == "<i4" else peaks.astype("<u2").tobytes())


def exact(stream, size):
    data = stream.read(size)
    if len(data) != size:
        raise ValueError("truncated upstream shard")
    return data


def shard(stream, item, shapes, pins, sink=None):
    size_bytes = exact(stream, 8)
    header_length = int.from_bytes(size_bytes, "little")
    if not 0 < header_length <= 1024**2:
        raise ValueError("invalid header length")
    header = exact(stream, header_length)
    records = json.loads(header)
    records.pop("__metadata__", None)
    whole = hashlib.sha256(size_bytes + header)
    result = {}
    cursor = 0
    for name, entry in sorted(records.items(), key=lambda x: x[1]["data_offsets"][0]):
        shape = shapes[name]
        n = math.prod(shape)
        if entry["shape"] != shape or entry["dtype"] != "BF16" or entry["data_offsets"] != [cursor, cursor + 2*n]:
            raise ValueError(f"invalid tensor geometry: {name}")
        dtype = "<i4" if len(shape) == 1 else ("<i2" if name == "model.embed_tokens.weight" else "i1")
        width = shape[-1]
        rows = n // width
        step = max(1, 1024**2 // width)
        source, codes, peaks = (hashlib.sha256() for _ in range(3))
        scale_bytes = bytearray()
        if sink is not None:
            sink.write(name.encode("ascii") + b"\n")
        for first in range(0, rows, step):
            chunk = exact(stream, min(rows - first, step)*width*2)
            source.update(chunk)
            whole.update(chunk)
            quantized, scales = encode(chunk, width, dtype)
            codes.update(quantized)
            peaks.update(scales)
            if sink is not None:
                sink.write(quantized)
                scale_bytes.extend(scales)
        if source.hexdigest() != pins[name]:
            raise ValueError(f"upstream payload mismatch: {name}")
        if sink is not None:
            sink.write(scale_bytes)
        cursor += n*2
        result[name] = dict(dtype=dtype, shape=shape, codes_sha256=codes.hexdigest())
        if len(shape) == 2:
            result[name]["absmax_bf16le_sha256"] = peaks.hexdigest()
    if (whole.hexdigest() != item["upstream_sha256"] or stream.read(1) or len(records) != item["tensors"]
            or cursor + header_length + 8 != item["bytes"]):
        raise ValueError(f"shard identity mismatch: {item['file']}")
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--local", type=Path, help="directory with any available full source shards")
    p.add_argument("--stream", action="store_true", help="binary tensor stream for the native full-model evaluator on stdout")
    args = p.parse_args()
    if args.stream and sys.stdout.isatty():
        p.error("--stream requires a pipe or binary file")
    geometry = json.loads((HERE / "upstream-geometry.json").read_text())
    inventory = json.loads((HERE / "upstream-payloads.json").read_text())
    if any((x["model"], x["revision"]) != (MODEL, REVISION) for x in (geometry, inventory)):
        raise ValueError("incorrect source revision")
    shapes = expected_shapes()
    result = {}
    for item in geometry["source_shards"]:
        with open_shard(item["file"], args.local) as stream:
            values = shard(stream, item, shapes, inventory["tensor_sha256"], sys.stdout.buffer if args.stream else None)
        if result.keys() & values.keys():
            raise ValueError("duplicate source tensor")
        result.update(values)
        print(f"{item['file']}: {len(values)} verified integer tensors", file=sys.stderr, flush=True)
    if result.keys() != shapes.keys() or result.keys() != inventory["tensor_sha256"].keys():
        raise ValueError("missing or unexpected tensors")
    head = json.loads((HERE / "output-head.json").read_text())
    if result["lm_head.weight"]["codes_sha256"] != head["weights_i8_sha256"]:
        raise ValueError("integer head differs from the independent Rust converter")
    if args.stream:
        if result != json.loads((HERE / "integer-weights.json").read_text())["tensors"]:
            raise ValueError("integer inventory differs; withholding stream completion")
        sys.stdout.buffer.write(b"END\n")
    else:
        print(json.dumps(dict(model=MODEL, revision=REVISION, tensors=result), sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
