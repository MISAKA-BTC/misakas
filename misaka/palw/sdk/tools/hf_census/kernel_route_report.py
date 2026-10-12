#!/usr/bin/env python3
"""**The kernel-route column over a stratified head-task cohort** (HFX 2026-10-10; RFC-0011 §18, kept beside the three numbers, never inside them).

    kernel_route_report.py --design DIR/sample/cohort_heads_design.json --rows b=ROWS_DIR_OR_FILE [--rows a=...] --out report.json

A head-task repository (`text-classification`, `token-classification`, `question-answering`, `fill-mask`, `zero-shot-classification`,
`text-ranking`) is decided by the listing under the generative route (`PROFILE_NOT_ARMED`), so the census's sampling frame never
fetched one. `sample_head_cohort.py` draws `n_h` of each task stratum's `N_h` complete-looking repositories; `fetch.py` reads their
metadata and safetensors headers; `palw-class census gates` judges each (the class lowered shape-only, the generative `Head` admission
hypothetical, and — for a bidirectional encoder — the K2-TIR-v5 route, `kernel_route = <shipped>/<hypothetical>`).

For each stratum this counts, among the `n_h` rows (rate -> times `N_h`), exactly and separately:

* `source_pass`        — the source gate passes (weights complete, rights not judged here);
* `lowers`             — the convert stage has no blocker (the class lowers shape-only), the listing's PROFILE_NOT_ARMED aside;
* `generative_admits`  — the hypothetical `Head` admission (generative route, fence armed hypothetically) passes;
* `kernel_eligible`    — `lowers` and the K2-TIR-v5 route's hypothetical outcome is `ELIGIBLE_AT`;
* `stopped_at`         — the first failing gate/code histogram of the rest, so the next blocker is read off the data.

Nothing here is a verdict that a model registers: K2-TIR-v5 is `Implemented`, never active (`KERNEL_NOT_ACTIVE` as shipped), reached
through the kernel route's fences (tag 110, OPV), which are dormant everywhere. The numbers are shape-level properties of the class.
The standard error is the stratified one (finite-population correction applied).
"""
import argparse
import collections
import glob
import json
import math
import os


def rows_of(path):
    files = [path] if os.path.isfile(path) else sorted(glob.glob(os.path.join(path, "*.jsonl")))
    for f in files:
        if "judge-cache" in os.path.basename(f):
            continue
        for line in open(f):
            line = line.strip()
            if not line:
                continue
            x = json.loads(line)
            yield x.get("row", x)


def stratum_of(row, strata):
    """The row's design stratum: its declared task (`head-<task>`)."""
    t = (row.get("task") or {}).get("task")
    return f"head-{t}" if t else None


def classify(row):
    """The row's flags and where it stopped."""
    gates = {g["gate"]: g for g in row.get("technical", [])}
    pf = row.get("preflight") or {}
    src = gates.get("source", {}).get("status") == "PASS"
    blockers = pf.get("blockers", [])
    convert = [b for b in blockers if b.startswith("convert:")]
    lowers = src and not convert
    register = [b for b in blockers if b.startswith("register:")]
    # The hypothetical Head admission: no register blocker other than the fence itself.
    admits = lowers and all(b.startswith("register:FENCE_NOT_ARMED") for b in register)
    route = pf.get("kernel_route") or ""
    hypo = route.split("/")[-1] if route else ""
    kernel = lowers and hypo == "ELIGIBLE_AT"
    first = None
    if not src:
        g = gates.get("source", {})
        first = f"source:{g.get('blocking') or g.get('status')}"
    elif convert:
        first = convert[0].replace("convert:", "convert:")
    elif not kernel and not admits:
        first = f"kernel:{hypo or 'no-route'}"
    return {"source_pass": src, "lowers": lowers, "generative_admits": admits, "kernel_eligible": kernel, "first": first}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--design", required=True)
    ap.add_argument("--rows", action="append", required=True, help="RULESET=path (a rows file or a directory of *.jsonl)")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    design = json.load(open(a.design))["strata"]
    out = {"schema": "misaka.palw.hf-census-kernel-route.v1", "design": design, "rulesets": {}}
    for spec in a.rows:
        name, path = spec.split("=", 1)
        per = collections.defaultdict(list)
        for row in rows_of(path):
            s = stratum_of(row, design)
            if s is None or s.replace("head-", "") not in design:
                continue
            per[s.replace("head-", "")].append((row, classify(row)))
        report = {}
        tot = collections.Counter()
        var = collections.Counter()
        for t, d in design.items():
            rows = per.get(t, [])
            n, N = len(rows), d["N"]
            rec = {"N": N, "n_designed": d["n"], "n_judged": n}
            for flag in ("source_pass", "lowers", "generative_admits", "kernel_eligible"):
                k = sum(1 for _, c in rows if c[flag])
                p = k / n if n else 0.0
                fpc = (1 - n / N) if N else 0.0
                se = math.sqrt(p * (1 - p) / max(n - 1, 1) * fpc) * N if n > 1 else 0.0
                rec[flag] = {"k": k, "rate": round(p, 4), "est": round(p * N), "se": round(se)}
                tot[flag] += p * N
                var[flag] += se * se
            stops = collections.Counter(c["first"] for _, c in rows if c["first"])
            rec["stopped_at"] = stops.most_common(8)
            report[t] = rec
        N_all = sum(d["N"] for d in design.values())
        report["_total"] = {
            "N": N_all,
            **{f: {"est": round(tot[f]), "share": round(tot[f] / N_all, 4), "se": round(math.sqrt(var[f]))} for f in tot},
        }
        out["rulesets"][name] = report
    json.dump(out, open(a.out, "w"), indent=1)
    for name, rep in out["rulesets"].items():
        t = rep["_total"]
        print(f"[{name}] N={t['N']}  " + "  ".join(f"{f}={t[f]['est']} ({t[f]['share']:.1%})" for f in ("source_pass", "lowers", "generative_admits", "kernel_eligible") if f in t))


if __name__ == "__main__":
    main()
