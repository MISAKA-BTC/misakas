#!/usr/bin/env python3
"""**Shadow directories for a re-run of the gates on an earlier fetch** — no network.

    shadow_dirs.py --snapshot DIR --out DIR/p4 [--listing dall.listing-v2.jsonl.gz] [--sample sample.jsonl ...] [--cohort NAME ...]

A fetched repository directory holds the listing it was fetched with (`listing.json`) and the headers it read. A re-run of
`palw-class census gates` on the current tree should see the listing the current tree classified (`dall.listing-v2.jsonl.gz`: the Hub's
listing with renamed bases followed), not the one of the fetch. For every sampled repository this writes `OUT/shadow/<hh>/<name>/`
with a symlink to each entry of the fetched directory except `listing.json`, which is the current listing record. The fetched
directories are never written. Writes `OUT/sample.dirs` and `OUT/cohort_<name>.dirs` (one directory per line, then `END`), and
`OUT/shadow.json` (what was found and what was not).
"""
import argparse
import gzip
import hashlib
import json
import os
import sys
from pathlib import Path


def repo_dir(root: Path, repo: str) -> Path:
    """`fetch.repo_dir` (restated: the fetcher needs an HTTP client this tool does not)."""
    return root / hashlib.sha256(repo.encode()).hexdigest()[:2] / repo.replace("/", "__")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--listing", default="dall.listing-v2.jsonl.gz")
    ap.add_argument("--cohort", action="append", default=[])
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = Path(a.out).expanduser()
    sets = {"sample": [json.loads(l)["id"] for l in open(snap / "sample" / "sample.jsonl")]}
    for c in a.cohort:
        sets[f"cohort_{c}"] = [json.loads(l)["id"] for l in open(snap / "sample" / f"cohort_{c}.jsonl")]
    wanted = {i for ids in sets.values() for i in ids}
    found = {}
    with gzip.open(snap / a.listing, "rt") as f:
        for line in f:
            # the id is the first key of a listing record; avoid parsing every line
            if not line.startswith('{"id":'):
                l = json.loads(line)
                if l.get("id") in wanted:
                    found[l["id"]] = line
                continue
            rid = line[7 : line.index('"', 7)]
            if rid in wanted:
                found[rid] = line
    report = {"listing": a.listing, "wanted": len(wanted), "listing_found": len(found), "no_fetch_dir": [], "sets": {}}
    for name, ids in sets.items():
        dirs = []
        for rid in ids:
            src = repo_dir(snap / "repos", rid)
            if rid not in found or not src.is_dir():
                report["no_fetch_dir"].append(rid)
                continue
            dst = out / "shadow" / src.parent.name / src.name
            dst.mkdir(parents=True, exist_ok=True)
            for e in src.iterdir():
                if e.name == "listing.json":
                    continue
                link = dst / e.name
                if not link.exists() and not link.is_symlink():
                    os.symlink(e, link)
            (dst / "listing.json").write_text(found[rid])
            dirs.append(str(dst))
        (out / f"{name}.dirs").write_text("".join(d + "\n" for d in dirs) + "END\n")
        report["sets"][name] = {"repos": len(ids), "dirs": len(dirs)}
    (out / "shadow.json").write_text(json.dumps(report, indent=1))
    print(json.dumps({k: v for k, v in report.items() if k != "no_fetch_dir"} | {"no_fetch_dir": len(report["no_fetch_dir"])}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
