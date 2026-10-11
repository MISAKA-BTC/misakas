#!/usr/bin/env python3
"""Gate 2: cross-host determinism comparison for the PALW v1 PoW path.

Inputs: gate2/<label>.jsonl fetched from each fleet host, plus the local
Metal run (gate 1's palw_audit_results.jsonl). Compares, per seed label:
  - the 5 tag fields (output_commitment, gemm_trace_root,
    operation_schedule_commitment, prefill_tokens, decode_tokens)
    across all x86 hosts: byte-equality is the class claim.
  - identity fields: constant within each host, equal across x86 hosts,
    and explicitly DIFFERENT from the Metal class (expected split).
  - x86 vs Metal per-label: where does text agree while the trace differs?
"""
import json, os, sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
X86_HOSTS = ["A-broadwell-8c", "B-epyc-6c", "C-epyc-8c-c", "ibm-epyc-8c-d"]
TAG_FIELDS = ["output_commitment", "gemm_trace_root", "operation_schedule_commitment",
              "prefill_tokens", "decode_tokens"]
IDENTITY_FIELDS = ["model_profile_id", "runtime_class_id", "trace_scheme_id", "cu_ruleset_id",
                   "runtime_manifest_hash", "shape_profile_id", "schema"]

def load(path):
    recs = {}
    for line in open(path):
        r = json.loads(line)
        recs[r["label"]] = r
    return recs

hosts = {h: load(os.path.join(HERE, "gate2", f"{h}.jsonl")) for h in X86_HOSTS}
metal = load(os.path.join(HERE, "palw_audit_results.jsonl"))

labels = sorted(metal.keys())
for h, recs in hosts.items():
    missing = [l for l in labels if l not in recs]
    if missing:
        print(f"!! {h}: {len(missing)} missing labels: {missing[:5]}")
        sys.exit(1)
    errs = [l for l, r in recs.items() if "error" in r]
    if errs:
        print(f"!! {h}: errors on {errs}")
        sys.exit(1)

# 1. Seed sanity: every host must have derived identical seeds per label
for l in labels:
    seeds = {h: hosts[h][l]["seed"] for h in X86_HOSTS} | {"metal": metal[l]["seed"]}
    if len(set(seeds.values())) != 1:
        print(f"!! seed derivation differs at {l}: {seeds} — harness bug, not a class result")
        sys.exit(1)
print(f"[seeds] identical derivation on all hosts for {len(labels)} labels")

# 2. The class claim: 4-host byte-equality of every tag field
mismatch = 0
for l in labels:
    for f in TAG_FIELDS:
        vals = {h: hosts[h][l][f] for h in X86_HOSTS}
        if len(set(map(str, vals.values()))) != 1:
            mismatch += 1
            print(f"MISMATCH {l}.{f}:")
            for h, v in vals.items():
                print(f"    {h}: {str(v)[:32]}")
n_checks = len(labels) * len(TAG_FIELDS)
print(f"[x86 class] {n_checks - mismatch}/{n_checks} field-comparisons identical across 4 hosts"
      + (" — PASS" if mismatch == 0 else f" — {mismatch} MISMATCHES"))

# 3. Identity fields: constant per host, equal across x86, split vs Metal
print("\n[identity]")
for f in IDENTITY_FIELDS:
    per_host = {}
    for h in X86_HOSTS:
        vals = set(str(hosts[h][l][f]) for l in labels)
        if len(vals) != 1:
            print(f"  !! {f} VARIES within {h} ({len(vals)} values)")
        per_host[h] = next(iter(vals))
    x86_vals = set(per_host.values())
    m = str(metal[labels[0]][f])
    x = next(iter(x86_vals))
    rel = "== metal" if m == x else "!= metal (class split)"
    print(f"  {f}: {'x86 uniform' if len(x86_vals)==1 else 'X86 SPLIT ' + str(per_host)}, {rel}")

# 4. x86 vs Metal per-label tag relationship
same_out = same_gemm = same_decode = 0
for l in labels:
    x = hosts[X86_HOSTS[0]][l]
    m = metal[l]
    same_out += x["output_commitment"] == m["output_commitment"]
    same_gemm += x["gemm_trace_root"] == m["gemm_trace_root"]
    same_decode += x["decode_tokens"] == m["decode_tokens"]
print(f"\n[x86 vs metal] over {len(labels)} labels: output_commitment equal {same_out}, "
      f"gemm_trace_root equal {same_gemm}, decode_tokens equal {same_decode}")

# 5. Distinctness on the x86 class (the gate-1 analysis re-run on fleet data)
main = [hosts[X86_HOSTS[0]][l] for l in labels if not l.startswith("repeat/")]
gemms = set(r["gemm_trace_root"] for r in main)
tags = set((r["output_commitment"], r["gemm_trace_root"], r["operation_schedule_commitment"],
            r["prefill_tokens"], r["decode_tokens"]) for r in main)
outs = set(r["output_commitment"] for r in main)
print(f"[x86 distinctness] gemm {len(gemms)}/{len(main)}, full tag {len(tags)}/{len(main)}, output {len(outs)}/{len(main)}")
rep = hosts[X86_HOSTS[0]].get("repeat/nonce0-rerun")
orig = hosts[X86_HOSTS[0]].get("pow/nonce0")
if rep and orig:
    same = all(rep[f] == orig[f] for f in TAG_FIELDS)
    print(f"[x86 determinism] nonce0 rerun identical: {same}")

# 6. Timing per host
print("\n[timing s/inference]")
for h in X86_HOSTS + []:
    ts = [hosts[h][l]["_secs"] for l in labels]
    print(f"  {h}: min {min(ts):.1f} mean {sum(ts)/len(ts):.1f} max {max(ts):.1f}")
ts = [metal[l]["_secs"] for l in labels]
print(f"  metal-local: min {min(ts):.1f} mean {sum(ts)/len(ts):.1f} max {max(ts):.1f}")
