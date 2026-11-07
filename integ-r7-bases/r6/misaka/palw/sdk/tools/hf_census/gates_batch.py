#!/usr/bin/env python3
"""Run `palw-class census gates` over the fetched repositories of a snapshot that have no row yet, smallest first.

    gates_batch.py --snapshot DIR --palw-class BIN --run NAME [--threads 3] [--tree SHA]

Writes DIR/rows/NAME.jsonl (one row per repository, in the order given: by the Hub's parameter count, unknown last). Rows already
in any DIR/rows/*.jsonl are skipped, so batches can follow a fetch that is still running.
"""
import argparse, json, subprocess, sys
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--snapshot", required=True)
ap.add_argument("--palw-class", required=True)
ap.add_argument("--run", required=True)
ap.add_argument("--threads", type=int, default=3)
ap.add_argument("--tree", default="unstated")
ap.add_argument("--limit", type=int, default=0)
a = ap.parse_args()
snap = Path(a.snapshot).expanduser()
done = set()
for p in (snap / "rows").glob("*.jsonl"):
    for line in open(p):
        try:
            done.add(json.loads(line)["repo"])
        except Exception:
            pass
todo = []
for fj in (snap / "repos").glob("*/*/fetch.json"):
    d = fj.parent
    if d.name.count(".tmp"):
        continue
    l = json.loads((d / "listing.json").read_text())
    if l["id"] in done:
        continue
    p = l.get("st_total") or l.get("gguf_total")
    todo.append((p if p else 1 << 62, l["id"], str(d)))
todo.sort()
if a.limit:
    todo = todo[: a.limit]
dirs = snap / "rows" / f"{a.run}.dirs"
dirs.write_text("".join(t[2] + "\n" for t in todo))
print(f"{len(todo)} repositories to judge", flush=True)
with open(snap / "rows" / f"{a.run}.jsonl", "w") as out:
    r = subprocess.run([a.palw_class, "census", "gates", "--snapshot", snap.name, "--tree", a.tree, "--threads", str(a.threads), "--dirs", str(dirs)], stdout=out)
sys.exit(r.returncode)
