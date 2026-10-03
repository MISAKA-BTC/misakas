#!/usr/bin/env python3
"""**The published, reproducible header sample** (RFC-0002 §II.10.2).

    sample.py --snapshot DIR --seed SEED --n 3000 [--certainty 200] [--min-per-stratum 20]

Inputs: `DIR/dall.listing.jsonl.gz` and `DIR/classified.jsonl.gz` (`palw-class census classify`, same order). The population sampled is
the **undecided** part of D_all: repositories whose technical outcome the listing does not decide (a decided repository has already
failed a gate on its listing alone — gated, no weights, a task with no profile, a format nothing reads — and needs no fetch).

The design, written to `DIR/sample/design.json` with every stratum's N_h, n_h and inclusion probability:

* **Strata**: task group × artifact format × size band (the Hub's parameter count), `unknown` a stratum of its own.
* **Certainty stratum**: the `--certainty` most downloaded undecided repositories (inclusion probability 1), so the download-weighted
  estimate rests on the head of the distribution it is dominated by.
* **Random part**: in every other stratum a simple random sample without replacement, proportional to N_h with at least
  `--min-per-stratum` (all of a smaller stratum). The draw is reproducible from the seed alone: each repository's key is
  `BLAKE2b-64(seed ‖ repo_id)` and a stratum takes its n_h smallest keys, so a larger `--n` extends the sample (nested) rather than
  redrawing it.

`DIR/sample/sample.jsonl`: one line per sampled repository — `id`, `stratum`, `pi`, `certainty`, the listing record and the fetch plan.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import sys
from collections import defaultdict
from pathlib import Path


def key(seed: str, repo: str) -> int:
    return int.from_bytes(hashlib.blake2b((seed + "\0" + repo).encode(), digest_size=8).digest(), "big")


def stratum_of(row: dict) -> str:
    s = row["strata"]
    return f"{s['task_group']}|{s['format']}|{s['size_band']}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--n", type=int, default=3000)
    ap.add_argument("--certainty", type=int, default=200)
    ap.add_argument("--min-per-stratum", type=int, default=20)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = snap / "sample"
    out.mkdir(exist_ok=True)

    # Pass 1: the undecided population, its strata, keys and downloads.
    members: dict[str, list[tuple[int, int]]] = defaultdict(list)  # stratum -> [(key, index)]
    downloads: list[tuple[int, int]] = []  # (downloads, index) of undecided
    n_all = n_decided = 0
    with gzip.open(snap / "classified.jsonl.gz", "rt") as f:
        for i, line in enumerate(f):
            c = json.loads(line)
            n_all += 1
            if c["decided"]:
                n_decided += 1
                continue
            r = c["row"]
            members[stratum_of(r)].append((key(a.seed, r["repo"]), i))
            downloads.append((r["downloads"], i))
    n_und = sum(len(v) for v in members.values())
    downloads.sort(key=lambda x: (-x[0], x[1]))
    certain = {i for _, i in downloads[: a.certainty]}
    # Allocation over the rest.
    rest = {h: [m for m in v if m[1] not in certain] for h, v in members.items()}
    n_rest = sum(len(v) for v in rest.values())
    budget = max(0, a.n - len(certain))
    alloc: dict[str, int] = {}
    for h, v in rest.items():
        N = len(v)
        if N == 0:
            alloc[h] = 0
            continue
        prop = math.floor(budget * N / max(1, n_rest))
        alloc[h] = min(N, max(min(N, a.min_per_stratum), prop))
    chosen: dict[int, tuple[str, float, bool]] = {}
    for i in certain:
        chosen[i] = ("certainty", 1.0, True)
    design_strata = {}
    for h, v in rest.items():
        v.sort()
        n_h = alloc[h]
        pi = n_h / len(v) if v else 0.0
        for _, i in v[:n_h]:
            chosen[i] = (h, pi, False)
        design_strata[h] = {"N": len(v), "n": n_h, "pi": pi}
    design = {
        "schema": "misaka.palw.hf-census-sample.v1",
        "seed": a.seed,
        "key": "BLAKE2b-64(seed || 0x00 || repo_id), the n_h smallest per stratum",
        "strata_by": "task_group|format|size_band (the listing-depth row's strata)",
        "population": "the undecided part of D_all (no gate failed on the listing alone)",
        "d_all": n_all,
        "decided": n_decided,
        "undecided": n_und,
        "certainty": {"rule": f"the {a.certainty} most downloaded undecided repositories", "n": len(certain)},
        "n_target": a.n,
        "n": len(chosen),
        "min_per_stratum": a.min_per_stratum,
        "strata": dict(sorted(design_strata.items())),
    }
    (out / "design.json").write_text(json.dumps(design, indent=1))
    # Pass 2: the sampled repositories' listing records and plans.
    n_out = 0
    with gzip.open(snap / "dall.listing.jsonl.gz", "rt") as fl, gzip.open(snap / "classified.jsonl.gz", "rt") as fc, open(
        out / "sample.jsonl.tmp", "w"
    ) as fo:
        for i, (ll, lc) in enumerate(zip(fl, fc)):
            if i not in chosen:
                continue
            l = json.loads(ll)
            c = json.loads(lc)
            if l["id"] != c["row"]["repo"]:
                print(f"line {i}: the listing and the classification disagree ({l['id']} / {c['row']['repo']})", file=sys.stderr)
                return 2
            h, pi, cert = chosen[i]
            fo.write(json.dumps({"id": l["id"], "index": i, "stratum": stratum_of(c["row"]), "design_stratum": h, "pi": pi, "certainty": cert, "listing": l, "plan": c["plan"]}) + "\n")
            n_out += 1
    (out / "sample.jsonl.tmp").rename(out / "sample.jsonl")
    print(json.dumps({k: design[k] for k in ("d_all", "decided", "undecided", "n")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
