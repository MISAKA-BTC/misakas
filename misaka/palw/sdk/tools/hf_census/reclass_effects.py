#!/usr/bin/env python3
"""**The effect of each D_complete reclassification rule, separately** (RFC-0011 §18: "each reclassification rule is published with the
census, and its effect is reported separately"). Offline: reads two listing-depth classifications of the same snapshot — the baseline's
compact rows and the new tree's full classified rows — and attributes every repository whose listing verdict moved to the rule that
moved it, by the new row's task source.

    reclass_effects.py --compact-base p4/compact-v3.jsonl --classified-new hfx/m2/classified-v4.jsonl.gz --out hfx/m2/reclass.json

Rules attributed (task inference v4, `census::listing::task_of` and `census::gates::lower_listing`):

* `inferred:hf-automap`     a class transformers' auto-model tables list under exactly one task;
* `inferred:gguf-converter` a GGUF architecture llama.cpp's converter registers only for classes of one task;
* `gguf-model-class`        a GGUF of a llama.cpp model architecture with no derivable task: `TASK_UNKNOWN` with a model class
                            (inside D_b) instead of `no-config` (outside it);
* `head-profile`            a head task's listing code `MODALITY_PROFILE_MISSING` → `PROFILE_NOT_ARMED` (same bucket, NEW_KERNEL).

Every count is exact (the listing decides it). A repository the new tree leaves undecided is counted under `now_undecided` — its verdict
is the sample's, never assumed here.
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from buckets import EXTERNAL, bucket_of  # noqa: E402


def base_key(r: dict):
    if not r.get("d"):
        return ("undecided", "UNDECIDED", None, None)
    k = r.get("k") or []
    if len(k) < 2:
        return None
    return (k[0], k[1], k[2] if len(k) > 2 else None, k[3] if len(k) > 3 else None)


def stop_of_row(row: dict):
    for g in row.get("technical") or []:
        if g.get("status") == "PASS":
            continue
        if g.get("gate") == "pack" and g.get("blocking") == "NOT_RUN_NEEDS_WEIGHTS":
            continue
        return (g.get("gate"), g.get("blocking") or g.get("status"), g.get("arg"), g.get("class"))
    return ("none", "PASS", None, None)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--compact-base", required=True)
    ap.add_argument("--classified-new", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    base = {}
    for line in open(a.compact_base):
        r = json.loads(line)
        base[r["r"]] = base_key(r)
    moves = collections.Counter()
    by_rule = collections.defaultdict(collections.Counter)
    into_db = collections.Counter()
    out_of_db = collections.Counter()
    sources = collections.Counter()
    n = 0
    opener = gzip.open if a.classified_new.endswith(".gz") else open
    with opener(a.classified_new, "rt") as f:
        for line in f:
            c = json.loads(line)
            row = c.get("row") or c
            repo = row.get("repo")
            n += 1
            src = (row.get("task") or {}).get("source", "?")
            sources[src] += 1
            new = stop_of_row(row) if c.get("decided", True) else ("undecided", "UNDECIDED", None, None)
            old = base.get(repo)
            # The compact rows keep the argument truncated and `None` where the full row has none: compare normalised.
            if old is None or (old[0], old[1], (old[2] or "")[:60]) == (new[0], new[1], (new[2] or "")[:60]):
                continue
            ob = "UNDECIDED" if old[1] == "UNDECIDED" else bucket_of(old[0], old[1], old[2], old[3])[0]
            nb = "UNDECIDED" if new[1] == "UNDECIDED" else bucket_of(new[0], new[1], new[2], new[3])[0]
            rule = src if src in ("inferred:hf-automap", "inferred:gguf-converter") else None
            if rule is None and old[1] == "TASK_UNKNOWN" and old[2] == "no-config" and new[1] == "TASK_UNKNOWN" and new[2] is None:
                rule = "gguf-model-class"
            if rule is None and old[1] == "MODALITY_PROFILE_MISSING" and new[1] == "PROFILE_NOT_ARMED":
                rule = "head-profile"
            rule = rule or "other"
            by_rule[rule][f"{old[1]}:{old[2] or ''} -> {new[1]}:{(new[2] or '')[:60]}"] += 1
            moves[(rule, ob, nb)] += 1
            if ob in EXTERNAL and nb not in EXTERNAL:
                into_db[rule] += 1
            if ob not in EXTERNAL and nb in EXTERNAL:
                out_of_db[rule] += 1
    res = {
        "rows": n,
        "task_sources": dict(sources.most_common()),
        "moves_by_rule_and_bucket": [{"rule": r, "from": o, "to": t, "repos": v} for (r, o, t), v in moves.most_common()],
        "into_d_complete_by_rule": dict(into_db),
        "out_of_d_complete_by_rule": dict(out_of_db),
        "leading_moves_by_rule": {r: c.most_common(12) for r, c in by_rule.items()},
    }
    Path(a.out).write_text(json.dumps(res, indent=1))
    print(json.dumps({"into_d_complete": dict(into_db), "out_of_d_complete": dict(out_of_db), "rules": {r: sum(c.values()) for r, c in by_rule.items()}}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
