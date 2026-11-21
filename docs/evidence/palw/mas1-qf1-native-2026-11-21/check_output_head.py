"""Independent whole-head comparison against the TIR converter and integer dot kernel."""
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

import numpy as np


PIN = "20c2d6366ab85c90786ccdd829cd2b9e7d30ef3b2ebbb998280e7e4014b542ff"


def check(path, expected):
    rows, width = 151936, 4096
    source = hashlib.sha256()
    with path.open("rb") as f:
        length = f.read(8)
        size = int.from_bytes(length, "little")
        if size > 65536:
            raise ValueError("unexpected header size")
        header_bytes = f.read(size)
        header = json.loads(header_bytes)["lm_head.weight"]
        if header != {"dtype": "BF16", "shape": [rows, width], "data_offsets": [0, rows * width * 2]}:
            raise ValueError("wrong upstream output projection")
        source.update(length + header_bytes)
        weights, factors, logits = (hashlib.sha256() for _ in range(3))
        hidden = ((np.arange(width, dtype=np.int64) * 73 + 19) % 2047) - 1023
        values, samples = [], []
        for start in range(0, rows, 128):
            count = min(128, rows - start)
            raw = f.read(2 * width * count)
            source.update(raw)
            bf16 = np.frombuffer(raw, dtype="<u2").astype("<u4")
            floats = (bf16 << 16).view("<f4").reshape(count, width).astype(np.float64)
            if not np.isfinite(floats).all():
                raise ValueError("nonfinite weight")
            maxima = np.max(np.abs(floats), axis=1)
            scale = np.where(maxima == 0, 1.0, maxima / 127)
            fraction, integer = np.modf(np.abs(floats) / scale[:, None])
            codes = np.copysign(integer + (fraction >= 0.5), floats).astype(np.int8)
            weights.update(codes.tobytes())
            dots = codes @ hidden
            for i, (s, acc) in enumerate(zip(scale.tolist(), dots.tolist())):
                ratio = s * 4
                shift = max(0, min(62, 31 - math.frexp(ratio)[1]))
                fixed = math.ldexp(ratio, shift)
                m = int(fixed) + (fixed - int(fixed) >= 0.5)
                factors.update(struct.pack("<qB", m, shift))
                quotient, remainder = divmod(abs(acc * m), 1 << shift)
                q = quotient + (2 * remainder >= 1 << shift)
                value = max(-(2**31), min(2**31 - 1, -q if acc < 0 else q))
                values.append(value)
                logits.update(struct.pack("<i", value))
                if start + i in (0, 151643, 151645, rows - 1):
                    samples.append(dict(row=start + i, accumulator=acc, multiplier=m, shift=shift, logit=value))
        if f.read(1) or source.hexdigest() != PIN:
            raise ValueError("different upstream payload")
    peak = max(values)
    result = dict(source_sha256=PIN, source_bytes=path.stat().st_size, width=width, rows=rows, input_scale="1/1024",
                  logit_scale="1/4096", weights_i8_sha256=weights.hexdigest(), mul_shift_sha256=factors.hexdigest(),
                  logits_i32le_sha256=logits.hexdigest(), min_logit=min(values), max_logit=peak, max_ties=values.count(peak),
                  argmax=values.index(peak), sample_rows=samples)
    if result != expected:
        raise ValueError(f"projections differ at {[k for k in result if result[k] != expected.get(k)]}")
    return result


if __name__ == "__main__":
    print(json.dumps(check(Path(sys.argv[1]), json.loads(Path(sys.argv[2]).read_text())), indent=2))
