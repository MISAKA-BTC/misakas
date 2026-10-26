#!/usr/bin/env python3
"""Run the pinned fixture-only court harness while holding one shared build slot.

Invoke via buildslot.sh. It waits for 1-minute load <=5 before each operation;
it does not change product code, nodes, or a running build.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time


OPS = [
    "close_cone_acquit", "close_cone_forged_last_stage", "root_claim_admit",
    "dissect_round_apply", "dissect_bottom_honest", "position_close_honest",
    "position_close_forged", "lookahead_expand", "d2_demand_open",
    "d2_answer_disclosed", "d2_answer_convicted", "d2_sweep", "mldsa87_verify",
]
SCALES = [
    ("position_close_honest", 8), ("position_close_honest", 16),
    ("d2_answer_disclosed", 16), ("d2_answer_disclosed", 4096),
    ("d2_sweep", 16), ("d2_sweep", 4096),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-record", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--n", type=int, default=1000)
    parser.add_argument("--max-wait-seconds", type=int, default=1800)
    parser.add_argument("--case", action="append", help="operation or operation:scale; repeat to select a bounded batch")
    parser.add_argument("--allow-under-load", action="store_true", help="exploratory only; never arming evidence")
    args = parser.parse_args()
    cases = [(op, None) for op in OPS] + SCALES
    if args.case:
        cases = []
        for value in args.case:
            op, separator, scale = value.partition(":")
            if op not in OPS or (separator and (not scale.isdigit() or int(scale) < 1)):
                parser.error(f"invalid case: {value}")
            cases.append((op, int(scale) if separator else None))
        if len(set(cases)) != len(cases):
            parser.error("duplicate cases are not allowed")
    args.out.mkdir(parents=True, exist_ok=False)
    record = {"build_record": str(args.build_record), "status": "RUNNING", "operations": [], "load_waits": [],
              "allow_under_load": args.allow_under_load, "arming_evidence": False}

    def save():
        (args.out / "batch.json").write_text(json.dumps(record, indent=2) + "\n")

    save()
    deadline = time.monotonic() + args.max_wait_seconds
    for op, scale in cases:
        label = op if scale is None else f"{op}-scale-{scale}"
        while True:
            load = os.getloadavg()
            record["load_waits"].append({"utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "op": label, "load": load})
            save()
            if load[0] <= 5.0 or args.allow_under_load:
                break
            if time.monotonic() >= deadline:
                record["status"] = "NOT_RUN_LOAD"
                save()
                return 75
            time.sleep(30)
        command = [sys.executable, str(Path(__file__).with_name("run_court_op.py")),
                   "--build-record", str(args.build_record), "--out", str(args.out / label),
                   "--op", op, "--n", str(args.n), "--runs", "3", "--baseline-runs", "3"]
        if scale is not None:
            command += ["--scale", str(scale)]
        if args.allow_under_load:
            command += ["--allow-under-load"]
        started = time.monotonic()
        result = subprocess.run(command, capture_output=True, text=True)
        row = {"op": op, "scale": scale, "command": command, "exit_code": result.returncode,
               "wall_seconds": time.monotonic() - started, "stdout": result.stdout, "stderr": result.stderr}
        record["operations"].append(row)
        save()
        if result.returncode != 0:
            record["status"] = "PARTIAL"
            save()
            return result.returncode
        raw = json.loads((args.out / label / "raw.json").read_text())
        row["measurement_status"] = raw["status"]
        if raw["status"] not in ("PASS", "UNDER_LOAD"):
            record["status"] = "PARTIAL"
            save()
            return 1
        save()
    record["status"] = ("UNDER_LOAD_FIXTURE_ONLY" if any(
        row["measurement_status"] == "UNDER_LOAD" for row in record["operations"]) else "PASS_FIXTURE_ONLY")
    save()
    return 0


if __name__ == "__main__":
    sys.exit(main())
