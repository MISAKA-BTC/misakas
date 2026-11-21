"""Independent full first-block integer replay in native EXEC input order."""
import argparse
import hashlib
import json
import math
import re
from pathlib import Path

import numpy as np
from check_attention import HERE, Q24, SEEDS, STAGES, input_stages, inputs, rms, rsqrt

OUTPUTS = (*STAGES, "rotary_q", "rotary_k", "causal_attention", "attention_residual", "gated_mlp", "post_block")
ARTIFACT = "0db27c06fa8980fa52a54e3bd6d24721ed7c3155e8d684d5416cb501c115697a"


def narrow(x, mult, shift, wide=False):
    # Freeze each division's HAFZ, including when the input is negative.
    if int(np.abs(x).max()) * int(np.max(mult)) >= (1 << 62):
        x = x.astype(object)
    acc = x * mult
    den = np.left_shift(np.int64(1), shift)
    mag = np.abs(acc)
    value = np.sign(acc) * (mag // den + ((mag % den)*2 >= den))
    return np.asarray(np.clip(value, -2147483648, 2147483647) if wide else np.clip(value, -32767, 32767),dtype=np.int64)


def ms(s):
    shift = min(62, max(0, 31-math.frexp(s)[1]))
    return math.floor(math.ldexp(s, shift)+0.5), shift


def fixed_exp(x):
    x = np.clip(x, -31*11629080, 0)
    z = -x // 11629080
    a = x + z*11629080 + 22699573
    poly = ((6014632 * ((a*a) >> 24)) >> 24) + 5771362
    return np.where(z == 31, 0, (2*poly + (1 << z)) >> (z+1))


def silu(x):
    e = fixed_exp(-np.abs(x))
    # IntRecip(e+1) via the normative three inverse-square-root iterations.
    # Its input is in [1,~2] in Q24, so its binary exponent is zero.
    den = e + Q24
    y = np.array(SEEDS, dtype=np.int64)[np.clip((den-Q24)*16//(3*Q24), 0, 15)]
    for _ in range(3):
        y = np.maximum(1, (y * (3*Q24 - ((den*((y*y) >> 24)) >> 24))) >> 25)
    recip = (y*y) >> 24
    sig = (np.where(x <= 0, e, Q24) * recip) >> 24
    return np.clip((x*sig) >> 24, -2147483648, 2147483647)


def rotary(matrix):
    inv = np.array([np.float32(1)/np.float32(math.pow(1_000_000., i/64)) for i in range(64)], dtype=np.float32)
    seq = matrix.reshape(len(matrix), -1, 128).astype(np.int64)
    out = []
    for pos, heads in enumerate(seq):
        angles = (np.float32(pos)*inv).astype(np.float64)
        def table(fn):
            f = np.array([np.float32(fn(a)) for a in angles], dtype=np.float64) * Q24
            return np.copysign(np.floor(np.abs(f)+0.5),f).astype(np.int64)
        c, s = table(math.cos), table(math.sin)
        a, b = heads[:, :64], heads[:, 64:]
        out.append(np.clip(np.concatenate([a*c-b*s, a*s+b*c], axis=1) >> 24, -32767, 32767))
    return np.array(out)


def causal(queries, keys, values):
    score_m, score_s = ms(16/math.sqrt(128))
    def attend(q, k, v):
        dots = np.einsum("hgd,thd->hgt", q.reshape(8,4,128), k)
        logits = narrow(dots, score_m, score_s, wide=True)
        exp = fixed_exp(np.maximum(logits-logits.max(axis=-1,keepdims=True), -2147483648))
        sums = exp.sum(axis=-1)
        inv = np.array([rsqrt(int(n))**2 >> 24 for n in sums.flat], dtype=np.int64).reshape(8,4,1)
        probs = (exp * inv) >> 24
        return narrow(np.einsum("hgt,thd->hgd", probs, v), 1, 24).reshape(-1)
    rows = []
    v = values.reshape(-1,8,128).astype(np.int64)
    for pos, q in enumerate(queries):
        rows.append(attend(q,keys[:pos+1],v[:pos+1]))
    # Hold position, query, current key and value fixed and discard earlier KV.
    last_only = attend(queries[-1], keys[-1:], v[-1:])
    changed = int(np.count_nonzero(rows[-1] != last_only))
    return np.array(rows), changed


class IntegerLayer:
    def __init__(self, directory, layer, inventory):
        self.directory, self.layer, self.inventory = directory, layer, inventory

    def get(self, name, suffix, digest, dtype):
        name = f"model.layers.{self.layer}.{name}.weight"
        raw = (self.directory / (name+"."+suffix)).read_bytes()
        if hashlib.sha256(raw).hexdigest() != self.inventory[name][digest]:
            raise ValueError(f"wrong integer operand: {name}")
        return np.frombuffer(raw, dtype=dtype)
    def project(self, name, x, n, delta, wide=False):
        w = self.get(name,"codes","codes_sha256","i1").reshape(n,-1)
        peak = self.get(name,"row_absmax.bf16","absmax_bf16le_sha256","<u2")
        scale = (peak.astype("<u4") << 16).view("<f4").astype(np.float64)/127
        scale[scale == 0] = 1
        m,s = zip(*(ms(math.ldexp(v,delta)) for v in scale))
        # Every dot-product term and each partial sum is an exact binary64 integer.
        # Limit converted weights to one output tile even for the 12288-lane MLP.
        out = np.empty((len(x),n),dtype=np.int64)
        x = x.astype(np.float64)
        for start in range(0,n,256):
            sl = slice(start,start+256)
            dots = (x @ w[sl].astype(np.float64).T).astype(np.int64)
            out[:,sl] = narrow(dots,np.array(m[sl]),np.array(s[sl]),wide)
        return out


def decoder_tail(data, hidden, prefix):
    q, k = rotary(prefix[4]), rotary(prefix[5])
    attention, changed = causal(q, k, prefix[3])
    r = np.clip(hidden.astype(np.int64)+data.project("self_attn.o_proj",attention,4096,5),-32767,32767)
    g = data.get("post_attention_layernorm","codes","codes_sha256","<i4")
    u = np.array([rms(row,g,15) for row in r])
    gate = data.project("mlp.gate_proj",u,12288,14,True)
    up = data.project("mlp.up_proj",u,12288,0)
    product = narrow(silu(gate)*up,1,24)
    final = np.clip(r+data.project("mlp.down_proj",product,4096,5),-32767,32767)
    return [q,k,attention,r,product,final], changed


def reference(raw_dir, bundle, directory, tokens):
    hidden, prefix = input_stages(raw_dir, bundle, tokens)
    inventory = json.loads((HERE / "integer-weights.json").read_text())["tensors"]
    tail, changed = decoder_tail(IntegerLayer(directory,0,inventory),hidden,prefix)
    return [hashlib.sha256(m.astype("<i2").tobytes()).hexdigest() for m in [*prefix,*tail]], changed


def check(raw, bundle, layer, log):
    peers = inputs(log)
    tokens = next(iter(peers.values()))
    hashes, changed = reference(raw,bundle,layer,tokens)
    reports = re.findall(r"QF1 decoder pid=(\d+) positions=(\d+) layer=0 artifact=([0-9a-f]{64}) i16_sha256=([0-9a-f,]+)", log.read_text())
    if sorted(reports) != sorted((pid,str(len(tokens)),ARTIFACT,",".join(hashes)) for pid in peers) or changed == 0:
        raise ValueError("first-block reproduction, artifact identity or causal history differs")
    return dict(peers=len(peers),positions=len(tokens),layer=0,artifact_sha256=ARTIFACT,
                last_position_lanes_changed_by_history=changed,sha256=dict(zip(OUTPUTS,hashes,strict=True)))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("raw","bundle","layer","log"):
        p.add_argument(arg,type=Path)
    args = p.parse_args()
    print(json.dumps(check(**vars(args)),indent=2))
