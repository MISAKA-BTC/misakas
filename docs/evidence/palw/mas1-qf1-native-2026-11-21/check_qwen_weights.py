"""Read actual pinned shards once. Check every BF16 tensor span and the whole-file digest."""
import argparse
import hashlib
import json
import math
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

import requests

from check_qwen_source import MODEL, REVISION, expected_shapes


@contextmanager
def open_shard(name, directory):
    if directory and (directory / name).is_file():
        with (directory / name).open("rb") as f:
            yield f
        return
    url = f"https://huggingface.co/{MODEL}/resolve/{REVISION}/{name}"
    with requests.get(url, params={"download": "true"}, headers={"Accept-Encoding": "identity"},
                      stream=True, timeout=(15, 60)) as response:
        if response.status_code != 200:
            raise ValueError(f"{name} full shard HTTP status {response.status_code}")
        yield response.raw


def read_shard(stream, item, shapes, vectors):
    length = stream.read(8)
    size = int.from_bytes(length, "little")
    if len(length) != 8 or not 0 < size <= 1024**2:
        raise ValueError("invalid safetensors header length")
    header = stream.read(size)
    if len(header) != size:
        raise ValueError("incomplete safetensors header")
    entries = json.loads(header)
    entries.pop("__metadata__", None)
    source = hashlib.sha256(length + header)
    result = {}
    cursor = 0
    for name, tensor in sorted(entries.items(), key=lambda pair: pair[1]["data_offsets"][0]):
        shape = shapes[name]
        n = math.prod(shape) * 2
        if tensor["dtype"] != "BF16" or tensor["shape"] != shape or tensor["data_offsets"] != [cursor, cursor + n]:
            raise ValueError(f"incorrect QF1 source tensor at {name}")
        cursor += n
        digest = hashlib.sha256()
        remaining = n
        while remaining:
            chunk = stream.read(min(4 * 1024**2, remaining))
            if not chunk:
                raise ValueError(f"incomplete source tensor at {name}")
            source.update(chunk)
            digest.update(chunk)
            if vectors is not None and len(shape) == 1:
                vectors.setdefault(name, bytearray()).extend(chunk)
            remaining -= len(chunk)
        result[name] = digest.hexdigest()
    if (stream.read(1) or source.hexdigest() != item["upstream_sha256"] or len(result) != item["tensors"]
            or cursor != item["tensor_data_bytes"] or 8 + size + cursor != item["bytes"]):
        raise ValueError(f"upstream shard identity or size differs at {item['file']}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", type=Path, help="read available full shards here, fetching the others")
    parser.add_argument("--vectors", type=Path, help="new output directory for verified source norm gains in little-endian BF16")
    args = parser.parse_args()
    if args.vectors is not None and args.vectors.exists():
        parser.error("--vectors requires a new directory")
    source = json.loads(Path(__file__).with_name("upstream-geometry.json").read_text())
    if source["revision"] != REVISION or source["model"] != MODEL:
        raise ValueError("unrecognized upstream inventory")
    shapes = expected_shapes()
    tensors = {}
    vectors = {} if args.vectors is not None else None
    for item in source["source_shards"]:
        name = item["file"]
        with open_shard(name, args.local) as stream:
            result = read_shard(stream, item, shapes, vectors)
        if tensors.keys() & result.keys():
            raise ValueError("duplicate tensor across shards")
        tensors.update(result)
        print(f"{name}: {len(result)} tensors, {item['bytes']} bytes verified", file=sys.stderr, flush=True)
    if tensors.keys() != shapes.keys():
        raise ValueError("incomplete model tensor inventory")
    if vectors is not None:
        with tempfile.TemporaryDirectory(dir=args.vectors.parent) as staging:
            complete = Path(staging) / "vectors"
            complete.mkdir()
            for name, data in sorted(vectors.items()):
                (complete / (name + ".bf16")).write_bytes(data)
            complete.rename(args.vectors)
    print(json.dumps(dict(model=MODEL, revision=REVISION, source_shards_verified=len(source["source_shards"]),
                          whole_file_bytes=sum(s["bytes"] for s in source["source_shards"]),
                          bf16_parameters=sum(map(math.prod, shapes.values())),
                          tensor_sha256=tensors), sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
