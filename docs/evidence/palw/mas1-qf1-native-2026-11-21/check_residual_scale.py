"""Compare first-token predictions with two residual scales on a frozen local input."""
import argparse
import hashlib
import io
import json
import struct
import math
import sys
from pathlib import Path

import numpy as np
from tokenizers import Tokenizer

from check_attention import rms
from check_class import check as verify_class
from check_class import exact, field, integer, program_declarations
from check_decoder import causal, narrow, rotary, silu
from check_qf1_answers import HERE, TOKENIZER, json_file


def open_tensors(path):
    expected = json_file("class-check.json")
    actual = verify_class(path)
    if actual != expected:
        raise ValueError("artifact differs from verified class")
    with path.open("rb") as stream:
        exact(stream, 10)
        header = io.BytesIO(field(stream))
    integer(header, 2)
    declarations = program_declarations(field(header))
    field(header)
    exact(header, 64)
    tensors = {}
    for _ in range(integer(header, 4)):
        index = integer(header, 2)
        layer = integer(header, 2) if integer(header, 1) else None
        offset, size = struct.unpack("<QQ", exact(header, 16))
        name, width, shape, _ = declarations[index]
        tensors[layer, name.decode()] = np.memmap(path, dtype=f"<i{width}", mode="r", offset=offset, shape=tuple(shape))
    return tensors


def project(tensors, layer, name, x, shift_delta=0, wide=False):
    name += ".weight"
    w, mult, shift = (tensors[layer, name + suffix] for suffix in ("", ".multiplier", ".shift"))
    # Bounded row tiles keep the source mmap shared; products are exact integers in binary64.
    out = np.empty((len(x), len(w)), dtype=np.int64)
    x = x.astype(np.float64)
    for i in range(0, len(w), 512):
        sl = slice(i, i+512)
        dots = (x @ w[sl].astype(np.float64).T).astype(np.int64)
        s = shift[sl].astype(np.int64) + shift_delta
        if np.any((s < 0) | (s > 62)):
            raise ValueError("narrowing shift out of bounds")
        out[:, sl] = narrow(dots, mult[sl], s, wide)
    return out


def normalized(x, gain, frac):
    return np.asarray([rms(row, gain, frac) for row in x])


def replay(tensors, ids, frac, expected_token, tokenizer):
    # Embedding and the O and down projections write residual codes; their
    # ingress conversion and the residual RMS epsilon must use the same scale.
    change = 15-frac
    w, m, s = (tensors[None, "model.embed_tokens.weight"+suffix][ids] for suffix in ("", ".multiplier", ".shift"))
    x = narrow(w.astype(np.int64), m[:, None], s[:, None].astype(np.int64)+change)
    endpoints = []
    for layer in range(36):
        norm = normalized(x, tensors[layer, "input_layernorm.weight"], frac)
        q, k, v = (project(tensors, layer, f"self_attn.{n}_proj", norm) for n in "qkv")
        def heads(z, which):
            gain = tensors[layer, f"self_attn.{which}_norm.weight"]
            return rotary(normalized(z.reshape(-1,128), gain, 10).reshape(z.shape))
        attention, _ = causal(heads(q, "q"), heads(k, "k"), v)
        residual = np.clip(x + project(tensors, layer, "self_attn.o_proj", attention, change), -32767, 32767)
        norm = normalized(residual, tensors[layer, "post_attention_layernorm.weight"], frac)
        gate = project(tensors, layer, "mlp.gate_proj", norm, wide=True)
        up = project(tensors, layer, "mlp.up_proj", norm)
        product = narrow(silu(gate)*up, 1, 24)
        x = np.clip(residual + project(tensors, layer, "mlp.down_proj", product, change), -32767, 32767)
        endpoints.append(int(np.count_nonzero(np.abs(x[-1]) == 32767)))
        print(f"Q{frac} layer {layer+1}/36", file=sys.stderr, flush=True)
    norm = normalized(x[-1:], tensors[None, "model.norm.weight"], frac)
    logits = project(tensors, None, "lm_head", norm, wide=True)[0]
    top = int(np.argmax(logits))
    # Equal logits break toward the lower ID.
    rank = 1 + int(np.count_nonzero(logits > logits[expected_token])) + int(np.count_nonzero(logits[:expected_token] == logits[expected_token]))
    return dict(residual_fraction_bits=frac, token=top, text=tokenizer.decode([top], skip_special_tokens=False), digit_four_rank=rank,
                logits_sha256=hashlib.sha256(logits.astype("<i4").tobytes()).hexdigest(), last_position_endpoint_counts=endpoints)


def weight_reference(tensors, ids, digit, tokenizer):
    """Floating arithmetic over the exact quantized weight codes and their decoded row scales."""
    def scale(layer, name, ratio):
        m = tensors[layer, name+".multiplier"].astype(np.float64)
        shift = tensors[layer, name+".shift"].astype(np.int32)
        return (np.ldexp(m, -shift)/ratio).astype(np.float32)

    def linear(x, layer, name, ratio):
        name += ".weight"
        weights, factors = tensors[layer, name], scale(layer, name, ratio)
        y = np.empty((len(x), len(weights)), dtype=np.float32)
        for i in range(0, len(weights), 512):
            sl = slice(i, i+512)
            y[:, sl] = (x @ weights[sl].astype(np.float32).T) * factors[sl]
        return y

    def norm(x, layer, name):
        gain = tensors[layer, name+".weight"].astype(np.float32) / (1 << 20)
        return (x / np.sqrt(np.mean(x*x, axis=-1, keepdims=True) + np.float32(1e-6))) * gain

    count = len(ids)
    cos, sin = (tensors[None, f"rope.{f}"][:count, None, :].astype(np.float32) / (1 << 24) for f in ("cos","sin"))
    def rotate(x):
        left, right = np.split(x, 2, axis=-1)
        return np.concatenate([left*cos-right*sin, left*sin+right*cos], axis=-1)

    mask = np.triu(np.ones((count, count), dtype=bool), k=1)
    name = "model.embed_tokens.weight"
    x = tensors[None, name][ids].astype(np.float32) * scale(None, name, 32768)[ids, None]
    peaks = []
    for layer in range(36):
        n = norm(x, layer, "input_layernorm")
        q, k, v = (linear(n, layer, f"self_attn.{kind}_proj", 1).reshape(count, -1, 128) for kind in "qkv")
        q = rotate(norm(q, layer, "self_attn.q_norm"))
        k = np.repeat(rotate(norm(k, layer, "self_attn.k_norm")), 4, axis=1)
        scores = (q.transpose(1,0,2) @ k.transpose(1,2,0)) * np.float32(1/math.sqrt(128))
        scores[:, mask] = -np.inf
        weights = np.exp(scores - scores.max(axis=-1,keepdims=True))
        weights /= weights.sum(axis=-1,keepdims=True)
        a = (weights @ np.repeat(v, 4, axis=1).transpose(1,0,2)).transpose(1,0,2).reshape(count, 4096)
        x += linear(a, layer, "self_attn.o_proj", 32)
        n = norm(x, layer, "post_attention_layernorm")
        gate = linear(n, layer, "mlp.gate_proj", 16384)
        # Stable sigmoid for the complete FP32 gate range.
        e = np.exp(-np.abs(gate))
        activated = gate * np.where(gate >= 0, 1/(1+e), e/(1+e))
        product = activated * linear(n, layer, "mlp.up_proj", 1)
        x += linear(product, layer, "mlp.down_proj", 32)
        peaks.append(float(np.abs(x[-1]).max()))
    logits = linear(norm(x[-1:],None,"model.norm"),None,"lm_head",4)[0]
    if not np.isfinite(logits).all():
        raise ValueError("nonfinite weight reference")
    top = int(np.argmax(logits))
    rank = 1 + int(np.count_nonzero(logits > logits[digit])) + int(np.count_nonzero(logits[:digit] == logits[digit]))
    return dict(token=top, text=tokenizer.decode([top], skip_special_tokens=False), digit_four_rank=rank,
                final_position_residual_absmax=peaks)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("tokenizer", type=Path)
    parser.add_argument("--archived", action="store_true", help="use the independently checked full-replay input and verify every first-position logit")
    parser.add_argument("--weight-reference", action="store_true", help="read and apply the converted weights with floating arithmetic")
    args = parser.parse_args()
    raw = args.tokenizer.read_bytes()
    if hashlib.sha256(raw).hexdigest() != TOKENIZER:
        raise ValueError("wrong tokenizer")
    tokenizer = Tokenizer.from_buffer(raw)
    [four] = tokenizer.encode("4", add_special_tokens=False).ids
    run = json_file("real-lifecycle.json")
    if args.archived:
        prompt = json_file("full-replay-input.json")
        archived = json_file("full-replay.json")
        expected = archived["output_ids"][0]
        transcript = dict(first_position_logits_sha256=archived["logits_sha256"][0])
    else:
        prompt = run["input_ids"]
        expected = run["output_ids"][0]
        transcript = dict(original=run["panel"]["original"])
    tensors = open_tensors(args.artifact)
    if args.weight_reference:
        reference = weight_reference(tensors, prompt, four, tokenizer)
        print(json.dumps(dict(class_id=run["class_id"], **transcript, expected_digit_token=four, weight_reference=reference),
                         indent=2, ensure_ascii=False))
        sys.exit()
    baseline = replay(tensors, prompt, 15, four, tokenizer)
    if baseline["token"] != expected or (args.archived and baseline["logits_sha256"] != transcript["first_position_logits_sha256"]):
        raise ValueError("baseline differs from the recorded numerical execution")
    probe = replay(tensors, prompt, 10, four, tokenizer)
    print(json.dumps(dict(class_id=run["class_id"], **transcript, input_positions=len(prompt),
                          expected_digit_token=four, baseline=baseline, offline_scale_probe=probe), indent=2, ensure_ascii=False))
