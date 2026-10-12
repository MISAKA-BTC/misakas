#!/usr/bin/env python3
"""Run `palw-class census gates` politely on a shared machine: `nice -n 19`, `RAYON_NUM_THREADS=1`, a few worker threads, and —
for the shape depth — paused (SIGSTOP) while the 5-minute load average is above `--max-load`, resumed (SIGCONT) below it minus 5.

    run_gates.py --snapshot DIR --palw-class BIN --run NAME --dirs FILE [--follow] [--depth shape|headers] [--threads 2] [--tree SHA]
"""
import argparse, datetime as dt, os, signal, subprocess, sys, time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--snapshot", required=True)
ap.add_argument("--palw-class", required=True)
ap.add_argument("--run", required=True)
ap.add_argument("--dirs", required=True)
ap.add_argument("--follow", action="store_true")
ap.add_argument("--depth", default="shape")
ap.add_argument("--threads", type=int, default=2)
ap.add_argument("--tree", default="unstated")
ap.add_argument("--max-load", type=float, default=30.0)
ap.add_argument("--assume-task")
ap.add_argument("--judge-budget-secs", type=int)
ap.add_argument("--judge-cache")
a = ap.parse_args()
snap = Path(a.snapshot).expanduser()
rows = snap / "rows"
cmd = ["nice", "-n", "19", a.palw_class, "census", "gates", "--snapshot", snap.name, "--tree", a.tree, "--threads", str(a.threads), "--depth", a.depth, "--dirs", a.dirs]
if a.follow:
    cmd.append("--follow")
if a.assume_task:
    cmd += ["--assume-task", a.assume_task]
if a.judge_budget_secs:
    cmd += ["--judge-budget-secs", str(a.judge_budget_secs)]
if a.judge_cache:
    cmd += ["--judge-cache", a.judge_cache]
env = dict(os.environ, RAYON_NUM_THREADS="1")
log = open(rows / f"{a.run}.watch.log", "a")
with open(rows / f"{a.run}.jsonl", "a") as out, open(rows / f"{a.run}.err", "a") as err:
    child = subprocess.Popen(cmd, stdout=out, stderr=err, env=env)
    stopped = False
    while child.poll() is None:
        if a.depth == "shape":
            l5 = os.getloadavg()[1]
            if not stopped and l5 > a.max_load:
                child.send_signal(signal.SIGSTOP)
                stopped = True
                log.write(f"{dt.datetime.now().isoformat(timespec='seconds')} paused (load5 {l5:.1f})\n")
                log.flush()
            elif stopped and l5 < a.max_load - 5:
                child.send_signal(signal.SIGCONT)
                stopped = False
                log.write(f"{dt.datetime.now().isoformat(timespec='seconds')} resumed (load5 {l5:.1f})\n")
                log.flush()
        time.sleep(30)
    sys.exit(child.returncode)
