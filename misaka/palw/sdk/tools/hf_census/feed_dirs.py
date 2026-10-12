#!/usr/bin/env python3
"""Feed fetched repository directories to `palw-class census gates --follow`, smallest first within each poll; `END` when the
fetch process has exited and every fetched directory is listed.

    feed_dirs.py --snapshot DIR --dirs FILE --fetch-pid PID [--interval 60]
"""
import argparse, json, os, time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--snapshot", required=True)
ap.add_argument("--dirs", required=True)
ap.add_argument("--fetch-pid", type=int, required=True)
ap.add_argument("--interval", type=int, default=60)
ap.add_argument("--done-rows", nargs="*", default=[], help="row files whose repositories are already judged")
a = ap.parse_args()
snap = Path(a.snapshot).expanduser()
listed = set()
if os.path.exists(a.dirs):
    listed = {l.strip() for l in open(a.dirs) if l.strip() and l.strip() != "END"}
done_repos = set()
for p in a.done_rows:
    for line in open(p):
        try:
            r = json.loads(line)
        except ValueError:
            continue  # a row cut short when its process was stopped
        if "repo" in r:
            done_repos.add(r["repo"])


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


while True:
    running = alive(a.fetch_pid)
    new = []
    for fj in (snap / "repos").glob("*/*/fetch.json"):
        d = str(fj.parent)
        if ".tmp" in fj.parent.name or d in listed:
            continue
        l = json.loads((fj.parent / "listing.json").read_text())
        if l["id"] in done_repos:
            listed.add(d)
            continue
        p = l.get("st_total") or l.get("gguf_total")
        new.append((p if p else 1 << 62, d))
    new.sort()
    with open(a.dirs, "a") as f:
        for _, d in new:
            f.write(d + "\n")
            listed.add(d)
    if not running and not new:
        with open(a.dirs, "a") as f:
            f.write("END\n")
        break
    time.sleep(a.interval)
