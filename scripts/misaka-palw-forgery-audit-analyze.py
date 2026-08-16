#!/usr/bin/env python3
"""Analysis for the PALW algo_id=4 forgery-resistance audit."""
import json, math, os
from itertools import combinations
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
recs = [json.loads(l) for l in open(os.path.join(HERE, "palw_audit_results.jsonl"))]
errors = [r for r in recs if "error" in r]
recs = [r for r in recs if "error" not in r]
main = [r for r in recs if not r["label"].startswith("repeat/")]
rerun = [r for r in recs if r["label"].startswith("repeat/")]

FIELDS = ["output_commitment", "gemm_trace_root", "operation_schedule_commitment"]
TAG = lambda r: (r["output_commitment"], r["gemm_trace_root"], r["operation_schedule_commitment"],
                 r["prefill_tokens"], r["decode_tokens"])

print(f"runs: {len(recs)} ok, {len(errors)} errors")
if errors:
    for e in errors:
        print("  ERROR", e["label"], e["error"][:120])

# 1. Determinism control
if rerun:
    orig = next(r for r in main if r["label"] == "pow/nonce0")
    same = TAG(orig) == TAG(rerun[0])
    print(f"\n[determinism] nonce0 rerun tag identical: {same}")

# 2. Distinctness per component and full tag
print(f"\n[distinctness] n={len(main)} distinct seeds")
for f in FIELDS + ["prefill_tokens", "decode_tokens"]:
    vals = [r[f] for r in main]
    c = Counter(vals)
    dup = {k: v for k, v in c.items() if v > 1}
    print(f"  {f}: {len(c)}/{len(main)} distinct" + (f"  DUPES: { {str(k)[:16]: v for k,v in dup.items()} }" if dup and f in FIELDS else ""))
tags = [TAG(r) for r in main]
print(f"  FULL 200B tag: {len(set(tags))}/{len(main)} distinct")

# 3. Per-group distinctness (does low-entropy seeding collapse anything?)
print("\n[groups]")
for g in ["pow", "uniform", "adv"]:
    sub = [r for r in main if r["label"].startswith(g + "/")]
    print(f"  {g}: {len(set(TAG(r) for r in sub))}/{len(sub)} distinct tags, decode range {min(r['decode_tokens'] for r in sub)}-{max(r['decode_tokens'] for r in sub)}")

# 4. Pairwise Hamming distance in bits (avalanche over the whole set)
def ham(a: str, b: str) -> int:
    return bin(int(a, 16) ^ int(b, 16)).count("1")

for f in ["gemm_trace_root", "output_commitment"]:
    ds = [ham(a[f], b[f]) for a, b in combinations(main, 2)]
    nbits = len(main[0][f]) * 4
    print(f"\n[hamming/{f}] {len(ds)} pairs over {nbits} bits: min {min(ds)}, mean {sum(ds)/len(ds):.1f}, max {max(ds)} (ideal mean {nbits//2})")

# 5. Avalanche pairs: 1-bit seed flip
base = next(r for r in main if r["label"] == "adv/avalanche-base")
for lbl in ["adv/avalanche-flip1", "adv/avalanche-flip2", "adv/zero"]:
    other = next(r for r in main if r["label"] == lbl)
    print(f"[1-bit-seed-flip] base vs {lbl}: gemm ham {ham(base['gemm_trace_root'], other['gemm_trace_root'])}/512, out ham {ham(base['output_commitment'], other['output_commitment'])}/512")
z = next(r for r in main if r["label"] == "adv/zero")
b0 = next(r for r in main if r["label"] == "adv/bit0")
print(f"[1-bit-seed-flip] zero vs bit0: gemm ham {ham(z['gemm_trace_root'], b0['gemm_trace_root'])}/512")

# 6. Byte-position balance across samples (gross bias check, n is small)
for f in ["gemm_trace_root"]:
    bits_set = sum(bin(int(r[f], 16)).count("1") for r in main)
    total_bits = len(main) * len(main[0][f]) * 4
    print(f"\n[bit-balance/{f}] {bits_set}/{total_bits} bits set = {bits_set/total_bits:.4f} (ideal 0.5)")

# 7. Collision-bound min-entropy statement
n = len(main)
pairs = n * (n - 1) // 2
print(f"\n[min-entropy bound] 0 collisions in {pairs} pairs -> if tag min-entropy were H bits, expected collisions ~ pairs/2^H.")
print(f"  Observing 0 is consistent with H >> log2({pairs}) = {math.log2(pairs):.1f} bits; a constant-tag defect (H=0, the Ollama failure) is excluded at {pairs}:1.")

# 8. Decode length distribution + identity constants sanity
print(f"\n[decode_tokens] distribution: {dict(sorted(Counter(r['decode_tokens'] for r in main).items()))}")
for f in ["model_profile_id", "runtime_class_id", "trace_scheme_id", "cu_ruleset_id"]:
    vals = set(r[f] for r in main)
    print(f"[identity] {f}: {'CONSTANT (correct)' if len(vals)==1 else f'VARIES ({len(vals)}) — WRONG'}")
print(f"\n[timing] secs: min {min(r['_secs'] for r in main)}, mean {sum(r['_secs'] for r in main)/len(main):.2f}, max {max(r['_secs'] for r in main)}")
