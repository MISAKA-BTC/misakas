#!/usr/bin/env python3
"""**Reduce a `classified*.jsonl.gz` (one ClassifiedV1 per repository, the listing depth) to one small line per repository**, in the
same order, for the coverage report (`coverage_report.py`). Streaming; memory is one line.

    compact_rows.py CLASSIFIED.jsonl.gz > compact.jsonl           (or: compact_rows.py CLASSIFIED.jsonl.gz OUT.jsonl)

A line: `{"r": repo, "d": decided, "dl": downloads, "g": task group, "f": format, "sp": source gate passes (technical view),
"k": [gate, code, arg, class] of the first technical gate that is not PASS (the `pack` gate's NOT_RUN_NEEDS_WEIGHTS skipped, as
report.py's stop_key does) or null, "rp": the card-license policy confirms rights}` — or `{"err": …, "r": repo}` for a record that did not
read. No network.
"""
import gzip
import json
import sys


def first_stop(technical):
    for g in technical:
        if g["status"] == "PASS":
            continue
        if g["gate"] == "pack" and g.get("blocking") == "NOT_RUN_NEEDS_WEIGHTS":
            continue
        arg = g.get("arg")
        if not arg:
            # The evidence-derived labels report.py gives a gate row without an `arg` are not needed for the buckets.
            arg = ""
        return [g["gate"], g.get("blocking") or g["status"], arg, g.get("class")]
    return None


def main() -> int:
    src = sys.argv[1]
    out = open(sys.argv[2], "w") if len(sys.argv) > 2 else sys.stdout
    n = 0
    with gzip.open(src, "rt") as f:
        for line in f:
            c = json.loads(line)
            n += 1
            if "error" in c:
                out.write(json.dumps({"err": c["error"][:80], "r": c.get("repo")}, separators=(",", ":")) + "\n")
                continue
            r = c["row"]
            tech = r["technical"]
            src_pass = next(g for g in tech if g["gate"] == "source")["status"] == "PASS"
            out.write(
                json.dumps(
                    {
                        "r": r["repo"],
                        "d": 1 if c["decided"] else 0,
                        "dl": r["downloads"],
                        "g": r["strata"]["task_group"],
                        "f": r["strata"]["format"],
                        "sp": 1 if src_pass else 0,
                        "k": first_stop(tech),
                        "rp": 1 if c["rights_by_policy"].get("permissive-card-v0") else 0,
                    },
                    separators=(",", ":"),
                )
                + "\n"
            )
    if out is not sys.stdout:
        out.close()
    print(f"{n} records", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
