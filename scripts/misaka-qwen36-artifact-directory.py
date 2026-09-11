#!/usr/bin/env python3
"""Read a PALW-QWEN36 artifact's header only, and sum its tensors by what a token reads of them.

The residency budget of ADR-0112 is arithmetic over these sums: the always-set (everything that is
not a routed expert, less the embedding table) is pinned; one token's routed experts is the other
term of the floor; a fifth of the weight bytes is the default budget. This prints the same numbers
`kaspad` prints at load, from the file alone, so an operator can size a host before starting one.

    python3 scripts/misaka-qwen36-artifact-directory.py /root/palw-class/qwen36.palwq36

Mirrors `parse_header_v1` in misaka-palw-base0/src/qwen36.rs: magic, layer kinds, 15 usizes, an
i64, one byte, the rope table, the parameter rows, the directory, data_start. Reads the header
(0.71 GiB on the 33 GiB class) and never touches the weight region. No dependencies.
"""
import struct
import sys
from collections import defaultdict

p = sys.argv[1]
f = open(p, "rb")
i = 0


def take(n):
    global i
    b = f.read(n)
    assert len(b) == n, "short read inside the header"
    i += n
    return b


def usize():
    return struct.unpack("<Q", take(8))[0]


assert take(8) == b"PALWQ361", "not a PALW-QWEN36 artifact"
n_layers = usize()
kinds = take(n_layers)
fields = [usize() for _ in range(15)]
eps = struct.unpack("<q", take(8))[0]
router_bits = take(1)[0]
d_head = usize()
max_pos = usize()
for _ in range(2):
    n = usize()
    take(n * 4)
n_params = usize()
param_bytes = 0
for _ in range(n_params):
    n = usize()
    take(n)
    m = usize()
    take(m)
    param_bytes += m
n_tensors = usize()
entries = []
for _ in range(n_tensors):
    n = usize()
    name = take(n).decode()
    off = usize()
    ln = usize()
    entries.append((name, off, ln))
data_start = usize()

names = ["d_model", "n_heads", "n_kv_heads", "head_dim", "rotary_dim", "linear_k_heads", "linear_v_heads",
         "linear_head_dim", "conv_kernel", "n_experts", "experts_per_token", "moe_dim", "shared_dim", "vocab",
         "max_position"]
print("shape:", dict(zip(names, fields)), "layers", n_layers, "linear", kinds.count(0), "full", kinds.count(1))
print(f"params: {n_params} rows, {param_bytes / 2**30:.3f} GiB; tensors: {n_tensors}; data_start {data_start} ({data_start / 2**30:.3f} GiB of header)")

by = defaultdict(int)
per_layer_expert = defaultdict(int)
expert_tensor_sizes = defaultdict(int)
for name, off, ln in entries:
    if ".ffn_expert." in name:
        by["routed experts"] += ln
        li = int(name.split(".")[1])
        per_layer_expert[li] += ln
        expert_tensor_sizes[name.split("_")[-1]] += 0
        expert_tensor_sizes[name.rsplit("_", 1)[-1]] = ln
    elif name == "token_embd.weight":
        by["embedding"] += ln
    elif name.startswith("output") or "unembed" in name or name == "lm_head.weight":
        by["unembedding"] += ln
    elif ".ffn_shared" in name:
        by["shared experts"] += ln
    elif ".attn_" in name or ".ssm_" in name or ".linear" in name or ".gdn" in name:
        by["attention / recurrence"] += ln
    elif ".ffn_router" in name:
        by["routers"] += ln
    else:
        by["other (" + name.split(".")[-1] + ")"] += ln
total = sum(ln for _, _, ln in entries)
print(f"total weight bytes {total} = {total / 2**30:.2f} GiB")
for k, v in sorted(by.items(), key=lambda kv: -kv[1]):
    print(f"  {k:<28} {v / 2**30:7.3f} GiB  ({100 * v / total:5.1f} %)")
always = total - by["routed experts"]
print(f"always-set (everything but routed experts) {always / 2**30:.3f} GiB; without the embedding {(always - by['embedding']) / 2**30:.3f} GiB")
ept = fields[10]
one_expert = sum(expert_tensor_sizes.values())
expert_layers = len(per_layer_expert)
one_token = ept * one_expert * expert_layers
print(f"one expert (gate+up+down, exponents) {one_expert / 2**20:.2f} MiB; one token's routed experts {one_token / 2**30:.3f} GiB "
      f"({ept} of them in each of {expert_layers} layers)")
pinned = always - by["embedding"]
floor = pinned + one_token
fifth = -(-total // 5)
print(f"ADR-0112: pinned {pinned / 2**30:.3f} GiB + one token {one_token / 2**30:.3f} GiB = floor {floor / 2**30:.3f} GiB "
      f"(ratio {total / floor:.1f}x); the default budget, a fifth, is {fifth / 2**30:.3f} GiB "
      f"= pinned + {(fifth - pinned) / 2**30:.3f} GiB of routed experts (about {(fifth - pinned) / one_token:.1f} tokens)"
      + ("" if fifth >= floor else " -- BELOW THE FLOOR: this class is not a mixture a fifth can hold; state --palw-class-resident-bytes at or above the floor"))
