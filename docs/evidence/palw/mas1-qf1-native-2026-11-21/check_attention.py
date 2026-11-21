"""Independently replay the real first-layer TIR prefix from BF16 rows and native EXEC tokens."""
import argparse
import hashlib
import json
import math
import re
from pathlib import Path

import numpy as np
from check_embedding import ROW, inputs, lift

HERE = Path(__file__).resolve().parent
Q24 = 1 << 24
SEEDS = (15395829, 14307657, 13421772, 12682383, 12053107, 11509075, 11032629, 10610843,
         10234005, 9894662, 9586980, 9306325, 9048957, 8811825, 8592409, 8388608)
STAGES = ("input_norm", "q", "k", "v", "q_norm", "k_norm")


def hafz(value, denom):
    mag = (2 * abs(value) + denom) // (2 * denom)
    return -mag if value < 0 else mag


def codes(values):
    return np.array([min(32767, max(-32767, v)) for v in values], dtype="<i2")


def rsqrt(v):
    if not v:
        return 0
    exponent = (v.bit_length() - 1 - 24) // 2
    m = v >> (2 * exponent) if exponent >= 0 else v << (-2 * exponent)
    y = SEEDS[min(15, max(0, (m-Q24)*16 // (3*Q24)))]
    for _ in range(3):
        y = max(1, (y * (3*Q24 - ((m * ((y*y) >> 24)) >> 24))) >> 25)
    return y >> exponent if exponent >= 0 else y << -exponent


def rms(row, gain, frac):
    x = row.astype(np.int64)
    mean = ((int(x @ x) << 24) // len(x)) + hafz(1 << (24 + 2*frac), 1_000_000)
    h = max(0, (mean.bit_length() - 1 - 24) // 2)
    inv = rsqrt(mean >> (2*h))
    unit = np.clip((x * inv) >> h, -(1 << 31), (1 << 31)-1)
    return codes(hafz(int(u)*int(g), 1 << 34) for u, g in zip(unit, gain, strict=True))


def source(directory, name, inventory):
    name = f"model.layers.0.{name}.weight"
    raw = (directory / (name + ".bf16")).read_bytes()
    if hashlib.sha256(raw).hexdigest() != inventory[name]:
        raise ValueError(f"upstream weight mismatch: {name}")
    return (np.frombuffer(raw, dtype="<u2").astype("<u4") << 16).view("<f4").astype(np.float64)


def projection(directory, name, width, norm, inventory):
    weights = source(directory, name, inventory).reshape(width, 4096)
    scale = np.max(np.abs(weights), axis=1) / 127
    scale[scale == 0] = 1
    magnitudes = np.floor(np.abs(weights) / scale[:, None] + 0.5)
    w8 = np.copysign(np.minimum(magnitudes, 127), weights)
    # The integer products and every partial sum are below 2^35, exactly
    # representable in binary64 even with a different BLAS reduction order.
    dots = (norm.astype(np.float64) @ w8.T).astype(np.int64)
    shifts = [min(62, max(0, 30 - (math.frexp(float(s))[1]-1))) for s in scale]
    mult = [math.floor(math.ldexp(s, n) + 0.5) for s, n in zip(scale, shifts)]
    return np.array([codes(hafz(int(x)*m, 1 << s) for x, m, s in zip(row, mult, shifts, strict=True))
                     for row in dots])


def reference(directory, bundle, tokens):
    data = bundle.read_bytes()
    if hashlib.sha256(data).hexdigest() != "04dbb6c1ca08772b35c89e1ab308699c24a30b0fc50e900787b682d3aa02a676":
        raise ValueError("unrecognized input embedding bundle")
    rows = {int.from_bytes(data[i:i+4], "little"): np.frombuffer(lift(data[i+4:i+4+ROW]), dtype="<i2")
            for i in range(0, len(data), ROW+4)}
    ids = sorted(set(tokens))
    inventory = json.loads((HERE / "upstream-payloads.json").read_text())["tensor_sha256"]
    def gain(name):
        v = source(directory, name, inventory) * (1 << 20)
        return np.copysign(np.floor(np.abs(v)+0.5), v).astype(np.int64)
    g = gain("input_layernorm")
    norm = np.array([rms(rows[i], g, 15) for i in ids])
    q, k, v = [projection(directory, f"self_attn.{name}_proj", width, norm, inventory)
               for name, width in (("q", 4096), ("k", 1024), ("v", 1024))]
    def per_head(matrix, name):
        gain128 = gain(name)
        return np.array([rms(head, gain128, 10) for head in matrix.reshape(-1, 128)]).reshape(matrix.shape)
    matrices = (norm, q, k, v, per_head(q, "self_attn.q_norm"), per_head(k, "self_attn.k_norm"))
    positions = [ids.index(i) for i in tokens]
    return [hashlib.sha256(m[positions].astype("<i2").tobytes()).hexdigest() for m in matrices]


def check(directory, bundle, log):
    peers = inputs(log)
    tokens = next(iter(peers.values()))
    hashes = reference(directory, bundle, tokens)
    reports = re.findall(r"QF1 attention pid=(\d+) positions=(\d+) layer=0 artifact=([0-9a-f]{64}) i16_sha256=([0-9a-f,]+)", log.read_text())
    artifacts = {r[2] for r in reports}
    if artifacts != {"a1b8fd848030164c433a51690ac6a09bf145f46fdb398dd50ebca9bfc481315b"}:
        raise ValueError("peers must reproduce the pinned partial integer artifact")
    artifact = artifacts.pop()
    if sorted(reports) != sorted((pid, str(len(tokens)), artifact, ",".join(hashes)) for pid in peers):
        raise ValueError(f"native reports differ from independent calculation: {reports} vs {hashes}")
    return dict(positions=len(tokens), peers=len(peers), layer=0, artifact_sha256=artifact, sha256=dict(zip(STAGES, hashes)))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("directory", "bundle", "log"):
        p.add_argument(arg, type=Path)
    args = p.parse_args()
    print(json.dumps(check(args.directory, args.bundle, args.log), indent=2))
