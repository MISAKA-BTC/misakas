#!/usr/bin/env python3
"""**A stratified sample of the repositories the head tasks stop** (HFX 2026-10-10; RFC-0011 §18).

    sample_head_cohort.py --snapshot DIR --seed SEED --per-task 60 --palw-class BIN --name heads [--listing dall.listing-v2.jsonl.gz]

A repository with a declared head task (`text-classification`, `token-classification`, `question-answering`, `fill-mask`,
`zero-shot-classification`, `text-ranking`) is decided by the LISTING (`PROFILE_NOT_ARMED`): the sampling frame of the census never
fetched it, so the census could not say how many of them lower, admit or fit a kernel route. This draws `--per-task` repositories at
random (seeded, a blake2b key) from each task's complete-looking members (a `config.json` and a safetensors weight file at the root
or the listing's shards), re-plans them with the new build (`palw-class census classify`), and writes
`DIR/sample/cohort_<name>.jsonl` (the fetcher's input; `pi = n_h / N_h` per task stratum) and `cohort_<name>_design.json`
(per-task N and n). The estimate is the stratified one: each stratum's rate times its N.

Reads only the snapshot's saved listing; the fetch that follows (`fetch.py`) reads metadata and safetensors headers only.
"""
import argparse
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

TASKS = ["text-classification", "token-classification", "question-answering", "fill-mask", "zero-shot-classification", "text-ranking"]


def complete_looking(row):
    sib = row.get("siblings") or []
    has_cfg = "config.json" in sib
    has_w = any(s.endswith(".safetensors") for s in sib)
    return has_cfg and has_w and not row.get("gated") and not row.get("disabled")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--per-task", type=int, default=60)
    ap.add_argument("--palw-class", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--tree", default="unstated")
    ap.add_argument("--listing", default="dall.listing-v2.jsonl.gz")
    ap.add_argument("--tasks", help="comma-separated pipeline tags to stratify on (default: the NLU head tasks)")
    a = ap.parse_args()
    global TASKS
    if a.tasks:
        TASKS = a.tasks.split(",")
    snap = Path(a.snapshot).expanduser()
    key = lambda repo: hashlib.blake2b(f"{a.seed}\0head-cohort\0{a.name}\0{repo}".encode(), digest_size=8).hexdigest()  # noqa: E731
    pop = {t: [] for t in TASKS}
    with gzip.open(snap / a.listing, "rt") as fl:
        for ll in fl:
            r = json.loads(ll)
            t = r.get("pipeline_tag")
            if t in pop and complete_looking(r):
                pop[t].append((key(r["id"]), ll))
    chosen, design = [], {}
    for t in TASKS:
        pop[t].sort(key=lambda x: x[0])
        take = pop[t][: a.per_task]
        design[t] = {"N": len(pop[t]), "n": len(take)}
        chosen.extend((t, json.loads(ll)) for _, ll in take)
    inp = "".join(json.dumps(l) + "\n" for _, l in chosen)
    out = subprocess.run(
        [a.palw_class, "census", "classify", "--snapshot", snap.name, "--tree", a.tree], input=inp, capture_output=True, text=True, check=True
    )
    plans = [json.loads(x) for x in out.stdout.splitlines() if x.strip()]
    assert len(plans) == len(chosen), (len(plans), len(chosen))
    with open(snap / "sample" / f"cohort_{a.name}.jsonl", "w") as f:
        for (t, l), c in zip(chosen, plans):
            pi = design[t]["n"] / max(1, design[t]["N"])
            f.write(json.dumps({"id": l["id"], "stratum": f"head-{t}", "design_stratum": f"head-{t}", "pi": pi, "certainty": False, "listing": l, "plan": c["plan"]}) + "\n")
    d = {"schema": "misaka.palw.hf-census-head-cohort.v1", "name": a.name, "seed": a.seed, "per_task": a.per_task, "strata": design,
         "frame": "pipeline_tag in the head tasks, config.json and a .safetensors file listed, ungated, not disabled"}
    (snap / "sample" / f"cohort_{a.name}_design.json").write_text(json.dumps(d, indent=1))
    print(json.dumps(d))


if __name__ == "__main__":
    main()
