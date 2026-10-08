#!/usr/bin/env python3
"""Watch one class's lifecycle on the devnet (read-only) and record every transition.

    lifecycle_watch.py --class C --port JSONPORT --out DIR [--every 120] [--hours 12] [--until Active]

Appends one row per sample to DIR/lifecycle.tsv (time, tip DAA, registry state, ready seats now/required, probes passed/failed, class
status, share, inflight, reason) and rewrites DIR/lifecycle.json (first time each state was seen, the last row, whether a state that
admits weight-bearing claims was reached). Stops at --until, at the deadline, or never earlier. Stdlib + audit-tir/rpc.py.
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.join(os.environ.get("H1_WT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."), "audit-tir"))
import rpc  # noqa: E402

WEIGHT_BEARING = ("Probation", "ActiveLimited", "Active")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--class", dest="cid", required=True)
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--every", type=int, default=120)
    ap.add_argument("--hours", type=float, default=12)
    ap.add_argument("--until", default="Active")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    tsv = os.path.join(a.out, "lifecycle.tsv")
    if not os.path.exists(tsv):
        open(tsv, "w").write("time\tdaa\tstate\tready_now\tready_required\tprobes_passed\tprobes_failed\tclass_status\tshare\tinflight\treason\n")
    first = {}
    deadline = time.time() + a.hours * 3600
    last = None
    while time.time() < deadline:
        try:
            reg = rpc.call(a.port, "getPalwModelRegistry", {}, timeout=60)
            cls = rpc.call(a.port, "getPalwClasses", {}, timeout=60)
            r = next((c for c in reg.get("classes", []) if c.get("classId") == a.cid), {}) or {}
            c = next((c for c in cls.get("classes", []) if c.get("classId") == a.cid), {}) or {}
            row = {"time": time.strftime("%Y-%m-%dT%H:%M:%S"), "daa": reg.get("tipDaa"), "state": r.get("state"), "ready_now": r.get("readySeatsNow"),
                   "ready_required": r.get("requiredReadySeats"), "probes_passed": r.get("probesPassed"), "probes_failed": r.get("probesFailed"),
                   "class_status": c.get("status"), "share": c.get("sharePermille"), "inflight": r.get("inflightNow"), "reason": (r.get("reason") or "")[:300]}
        except Exception as e:  # a node restarting is not a lifecycle event
            row = {"time": time.strftime("%Y-%m-%dT%H:%M:%S"), "error": str(e)[:200]}
        if "error" not in row:
            open(tsv, "a").write("\t".join(str(row.get(k, "")) for k in ("time", "daa", "state", "ready_now", "ready_required", "probes_passed",
                                                                        "probes_failed", "class_status", "share", "inflight", "reason")) + "\n")
            if row["state"] not in first:
                first[row["state"]] = {"time": row["time"], "daa": row["daa"]}
            last = row
        json.dump({"class_id": a.cid, "first_seen": first, "last": last, "reached_weight_bearing": any(str(s).startswith(WEIGHT_BEARING) for s in first),
                   "updated": time.strftime("%Y-%m-%dT%H:%M:%S")}, open(os.path.join(a.out, "lifecycle.json"), "w"), indent=1)
        if last and last.get("state") == a.until:
            break
        time.sleep(a.every)
    return 0


if __name__ == "__main__":
    sys.exit(main())
