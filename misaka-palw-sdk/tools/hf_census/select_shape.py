#!/usr/bin/env python3
"""**The shape pass's order: a random second phase** (two-phase sampling).

    select_shape.py --snapshot DIR --headers-rows DIR/rows/runA.jsonl --shape-rows DIR/rows/run2.jsonl ... --seed SEED --out DIR/rows/runB.dirs

The headers pass judged `source` and `lower` for every sampled repository. The shape depth (admission at a declared context) is
expensive, so it runs on the *eligible* repositories — `lower` PASS, a decoder class (admit `NOT_RUN_DEPTH_HEADERS`) — **in a random
order** fixed by the seed (BLAKE2b-64(seed ‖ 0 ‖ "shape" ‖ 0 ‖ repo_id)). Whatever prefix of that order is judged when a report is
made is a simple random subsample of the eligible repositories within every stratum, so the report's estimate is unbiased however far
the pass got (`report.py` uses the longest fully judged prefix). Repositories already judged at the shape depth (an earlier run) are
skipped here but keep their place in the order.
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fetch import repo_dir  # noqa: E402


def shape_key(seed: str, repo: str) -> str:
    return hashlib.blake2b(f"{seed}\0shape\0{repo}".encode(), digest_size=8).hexdigest()


def eligible(row: dict) -> bool:
    g = {x["gate"]: x for x in row["technical"]}
    return g["source"]["status"] == "PASS" and g["lower"]["status"] == "PASS" and g["admit"].get("blocking") == "NOT_RUN_DEPTH_HEADERS"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--headers-rows", required=True)
    ap.add_argument("--shape-rows", nargs="*", default=[])
    ap.add_argument("--seed", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    judged = set()
    for p in a.shape_rows:
        for line in open(p):
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if "repo" in r:
                judged.add(r["repo"])
    el = []
    for line in open(a.headers_rows):
        r = json.loads(line)
        if "repo" in r and eligible(r):
            el.append((shape_key(a.seed, r["repo"]), r["repo"]))
    el.sort()
    todo = [repo for _, repo in el if repo not in judged]
    with open(a.out, "w") as f:
        for repo in todo:
            f.write(str(repo_dir(snap / "repos", repo)) + "\n")
        f.write("END\n")
    print(json.dumps({"eligible": len(el), "already_judged": len(el) - len(todo), "to_judge": len(todo)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
