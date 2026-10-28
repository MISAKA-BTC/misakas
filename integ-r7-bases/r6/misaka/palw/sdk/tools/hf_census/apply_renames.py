#!/usr/bin/env python3
"""**Base resolution v2: follow the Hub's renames** (RFC-0002 §II.10.2, "resolve every referenced component").

    apply_renames.py --snapshot DIR          → DIR/dall.listing-v2.jsonl.gz (+ DIR/dall.summary-v2.json)

`resolve_renames.py` asked the Hub once for every base id the compactor did not find in the snapshot and recorded where a renamed one
points (metadata only). Here a base reference that was `found: false` and whose id the Hub redirects to a repository that IS in the
snapshot becomes that repository's facts (its sha at T0, gated, disabled, license) with `renamed_from` naming the id the card wrote.
Nothing else changes; v1 stays beside it. No network.
"""
import argparse
import gzip
import json
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    ren = json.loads((snap / "renames.json").read_text())
    to = {k: v["to"] for k, v in ren.items() if v.get("status") == "renamed" and v.get("to")}
    targets = set(to.values())
    facts = {}
    with gzip.open(snap / "dall.listing.jsonl.gz", "rt") as f:
        for line in f:
            r = json.loads(line)
            if r["id"] in targets:
                facts[r["id"]] = {"id": r["id"], "found": True, "sha": r.get("sha"), "gated": bool(r.get("gated")), "disabled": bool(r.get("disabled")), "license": r.get("license")}
    n = changed = refs = 0
    out = snap / "dall.listing-v2.jsonl.gz"
    tmp = out.with_suffix(".tmp")
    with gzip.open(snap / "dall.listing.jsonl.gz", "rt") as f, gzip.open(tmp, "wt", compresslevel=6) as o:
        for line in f:
            r = json.loads(line)
            n += 1
            hit = False
            br = []
            for b in r.get("base_resolved") or []:
                t = to.get(b.get("id")) if not b.get("found") else None
                if t and t in facts:
                    br.append(dict(facts[t], renamed_from=b.get("id")))
                    hit = True
                    refs += 1
                else:
                    br.append(b)
            if hit:
                r["base_resolved"] = br
                changed += 1
                line = json.dumps(r, separators=(",", ":")) + "\n"
            o.write(line)
    tmp.rename(out)
    summ = {"base_resolution": "v2 (renames followed)", "repos": n, "repos_changed": changed, "references_resolved": refs, "renamed_ids": len(to), "targets_in_snapshot": len(facts)}
    (snap / "dall.summary-v2.json").write_text(json.dumps(summ, indent=1))
    print(json.dumps(summ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
