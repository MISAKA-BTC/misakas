#!/usr/bin/env python3
"""**A before/after cohort for one blocker bucket** (RFC-0002 §II.10.4.5: "one generic change per bucket, its before/after cohort").

    sample_cohort.py --snapshot DIR --code ADAPTER_UNCHECKED [--gate lower] --seed SEED --n 150 --palw-class BIN --name adapters

Draws a seeded simple random sample of the repositories the baseline census (`classified.jsonl.gz`, the listing depth) stopped at
`--gate`/`--code`, re-plans them with the new build (`palw-class census classify`, whose plan may read more — an adapter's base), and
writes `DIR/sample/cohort_<name>.jsonl` (the fetcher's input) and `cohort_<name>_design.json` (N, n, seed). The cohort's "before" is
the bucket (every member failed there); its "after" is the new build's gate rows over the fetched sample, expanded by N / n.
"""
import argparse
import gzip
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--snapshot", required=True)
ap.add_argument("--code", required=True)
ap.add_argument("--gate", default="lower")
ap.add_argument("--seed", required=True)
ap.add_argument("--n", type=int, default=150)
ap.add_argument("--palw-class", required=True)
ap.add_argument("--name", required=True)
ap.add_argument("--tree", default="unstated")
ap.add_argument("--listing", default="dall.listing.jsonl.gz", help="the listing the cohort is re-planned from (same order as v1; e.g. dall.listing-v2.jsonl.gz)")
a = ap.parse_args()
snap = Path(a.snapshot).expanduser()
key = lambda repo: hashlib.blake2b(f"{a.seed}\0cohort\0{a.name}\0{repo}".encode(), digest_size=8).hexdigest()  # noqa: E731
pop = []
with gzip.open(snap / "classified.jsonl.gz", "rt") as fc, gzip.open(snap / a.listing, "rt") as fl:
    for lc, ll in zip(fc, fl):
        c = json.loads(lc)
        if "error" in c or not c["decided"]:
            continue
        g = next((x for x in c["row"]["technical"] if x["status"] != "PASS"), None)
        if g is None or g["gate"] != a.gate or g.get("blocking") != a.code:
            continue
        pop.append((key(c["row"]["repo"]), ll))
pop.sort(key=lambda x: x[0])
chosen = [json.loads(ll) for _, ll in pop[: a.n]]
# Re-plan with the new build.
inp = "".join(json.dumps(l) + "\n" for l in chosen)
out = subprocess.run([a.palw_class, "census", "classify", "--snapshot", snap.name, "--tree", a.tree], input=inp, capture_output=True, text=True, check=True)
plans = [json.loads(x) for x in out.stdout.splitlines() if x.strip()]
with open(snap / "sample" / f"cohort_{a.name}.jsonl", "w") as f:
    for l, c in zip(chosen, plans):
        f.write(json.dumps({"id": l["id"], "stratum": f"cohort-{a.name}", "design_stratum": f"cohort-{a.name}", "pi": len(chosen) / max(1, len(pop)), "certainty": False, "listing": l, "plan": c["plan"]}) + "\n")
d = {"schema": "misaka.palw.hf-census-cohort.v1", "name": a.name, "bucket": {"gate": a.gate, "code": a.code}, "seed": a.seed, "N": len(pop), "n": len(chosen)}
(snap / "sample" / f"cohort_{a.name}_design.json").write_text(json.dumps(d, indent=1))
print(json.dumps(d))
