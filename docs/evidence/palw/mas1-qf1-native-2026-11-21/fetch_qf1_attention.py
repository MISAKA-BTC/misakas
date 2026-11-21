"""Fetch the BF16 source tensors used by the first pre-attention block."""
from contextlib import contextmanager
import argparse
import hashlib
import json
from pathlib import Path

import requests
from check_qwen_source import MODEL, REVISION

HERE = Path(__file__).resolve().parent
SHARD = "model-00001-of-00005.safetensors"
SHAPES = {"input_layernorm": [4096], "self_attn.q_proj": [4096, 4096],
          "self_attn.k_proj": [1024, 4096], "self_attn.v_proj": [1024, 4096],
          "self_attn.q_norm": [128], "self_attn.k_norm": [128]}


@contextmanager
def range_stream(start, end, shard, total):
    with requests.get(f"https://huggingface.co/{MODEL}/resolve/{REVISION}/{shard}",
                      params={"qf1_attention_range": f"{start}-{end}"},
                      headers={"Range": f"bytes={start}-{end}", "Accept-Encoding": "identity"},
                      stream=True, timeout=(15, 90)) as response:
        if response.status_code != 206 or response.headers.get("Content-Range") != f"bytes {start}-{end}/{total}":
            raise ValueError(f"invalid upstream shard range: {response.status_code}")
        yield response.raw


def read_range(start, end, shard=SHARD, total=3996250744):
    with range_stream(start, end, shard, total) as stream:
        data = stream.read(end-start+1)
        if len(data) != end-start+1 or stream.read(1):
            raise ValueError("upstream range size")
        return data


def main(target):
    inventory = json.loads((HERE / "upstream-payloads.json").read_text())
    if inventory["revision"] != REVISION or inventory["model"] != MODEL:
        raise ValueError("upstream inventory identity")
    size = int.from_bytes(read_range(0, 7), "little")
    if size != 9328:
        raise ValueError("unexpected header size")
    header = json.loads(read_range(8, 7+size))
    target.mkdir(parents=True, exist_ok=True)
    for short, shape in SHAPES.items():
        name = f"model.layers.0.{short}.weight"
        entry = header[name]
        if entry["shape"] != shape or entry["dtype"] != "BF16":
            raise ValueError(name)
        start, end = entry["data_offsets"]
        data = read_range(8+size+start, 7+size+end)
        if hashlib.sha256(data).hexdigest() != inventory["tensor_sha256"][name]:
            raise ValueError(name)
        (target / (name + ".bf16")).write_bytes(data)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("target", type=Path)
    main(p.parse_args().target)
