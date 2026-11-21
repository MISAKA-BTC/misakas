"""Extract the rows used by the isolated prompt and check live peer lookups against the pinned BF16 tensor."""
import argparse
import ast
import hashlib
import json
from fractions import Fraction
import re
import struct
from pathlib import Path

import requests

from check_qwen_source import MODEL, REVISION

HERE = Path(__file__).resolve().parent
ROW = 4096 * 2
VOCAB = 151936


def inputs(path):
    records = re.findall(r"QF1 input pid=(\d+) attempt=([0-9a-f]+) tokens=(\[[0-9, ]+\])", path.read_text())
    if len(records) != 3 or len({pid for pid, _, _ in records}) != 3 or len({(a, t) for _, a, t in records}) != 1:
        raise ValueError("expected three independent peers on the same attempt and tokens")
    return {pid: ast.literal_eval(tokens) for pid, _, tokens in records}


def extract(target):
    # All 16 hex digits occur in the retained prompt + challenge, so this set
    # also covers a new seed for the same private job.
    tokens = next(iter(inputs(HERE / "panel-run.txt").values()))
    ids = set(tokens)
    start = 9336  # first tensor, immediately after the pinned shard header
    end = start + VOCAB * ROW - 1
    whole = hashlib.sha256()
    rows = bytearray()
    url = f"https://huggingface.co/{MODEL}/resolve/{REVISION}/model-00001-of-00005.safetensors"
    with requests.get(url, params={"embedding_range": f"{start}-{end}"},
                      headers={"Range": f"bytes={start}-{end}", "Accept-Encoding": "identity"},
                      stream=True, timeout=(15, 120)) as resp:
        if resp.status_code != 206 or resp.headers.get("Content-Range") != f"bytes {start}-{end}/3996250744":
            raise ValueError(f"wrong shard range {resp.status_code} {resp.headers.get('Content-Range')}")
        for first in range(0, VOCAB, 256):
            count = min(256, VOCAB - first)
            chunk = resp.raw.read(count * ROW)
            if len(chunk) != count * ROW:
                raise ValueError("truncated embedding")
            whole.update(chunk)
            for i in sorted(k for k in ids if first <= k < first + count):
                pos = (i - first) * ROW
                rows.extend(struct.pack("<I", i))
                rows.extend(chunk[pos:pos + ROW])
        if resp.raw.read(1):
            raise ValueError("extra embedding bytes")
    tensor_sha = json.loads((HERE / "upstream-payloads.json").read_text())["tensor_sha256"]["model.embed_tokens.weight"]
    if whole.hexdigest() != tensor_sha:
        raise ValueError("embedding differs from previously verified whole shard")
    target.write_bytes(rows)
    print(json.dumps(dict(rows=len(ids), source_bytes=VOCAB * ROW, tensor_sha256=tensor_sha,
                          bundle_sha256=hashlib.sha256(rows).hexdigest())))


def half_away(value):
    magnitude = abs(value)
    rounded = (2 * magnitude.numerator + magnitude.denominator) // (2 * magnitude.denominator)
    return -rounded if value < 0 else rounded


def lift(raw):
    # BF16 values are exact powers-of-two multiples; use rational arithmetic
    # independently of the Rust lowerer's floating-point import.
    values = [Fraction(struct.unpack("<f", b"\x00\x00" + raw[i:i+2])[0]) for i in range(0, ROW, 2)]
    peak = max(map(abs, values))
    if not peak:
        return bytes(ROW)
    scale = peak / 32767
    ratio = scale * 32768
    exponent = ratio.numerator.bit_length() - ratio.denominator.bit_length()
    if ratio < Fraction(2) ** exponent:
        exponent -= 1
    shift = min(62, max(0, 30 - exponent))
    multiplier = half_away(ratio * (1 << shift))
    codes = [half_away(v / scale) for v in values]
    residual = [max(-32767, min(32767, half_away(Fraction(c * multiplier, 1 << shift)))) for c in codes]
    return struct.pack("<4096h", *residual)


def check(path, log):
    data = path.read_bytes()
    if len(data) % (ROW + 4):
        raise ValueError("partial row")
    rows = {int.from_bytes(data[i:i+4], "little"): data[i+4:i+4+ROW] for i in range(0, len(data), ROW + 4)}
    if hashlib.sha256(data).hexdigest() != "04dbb6c1ca08772b35c89e1ab308699c24a30b0fc50e900787b682d3aa02a676":
        raise ValueError("unexpected embedding bundle")
    lifted = {i: lift(row) for i, row in rows.items()}
    records = inputs(log)
    actual = re.findall(r"QF1 embedding pid=(\d+) rows=(\d+) bf16_sha256=([a-f0-9]{64}) residual_i16_sha256=([a-f0-9]{64})", log.read_text())
    expected = sorted((p, str(len(t)), hashlib.sha256(b"".join(rows[i] for i in t)).hexdigest(),
                       hashlib.sha256(b"".join(lifted[i] for i in t)).hexdigest()) for p, t in records.items())
    if sorted(actual) != expected:
        raise ValueError("live embeddings differ from the source rows in token order")
    print(json.dumps(dict(peers=len(actual), positions=int(expected[0][1]), width=ROW // 2,
                          bundle_sha256=hashlib.sha256(data).hexdigest(), ordered_bf16_sha256=expected[0][2],
                          ordered_i16_sha256=expected[0][3], residual_scale="1/32768")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--check", type=Path, metavar="LIVE_LOG")
    args = parser.parse_args()
    if args.check:
        check(args.bundle, args.check)
    else:
        extract(args.bundle)
