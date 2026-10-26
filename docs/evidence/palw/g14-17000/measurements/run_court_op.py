#!/usr/bin/env python3
"""Run one pinned court-verifier operation with load and source provenance.

Build separately under the shared buildslot. This runner also runs under that
buildslot, so the measured process owns one slot for the duration of its batch.
It never connects to a node or chooses a policy value.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


MAX_LOAD = 5.0
MEASURE_LINE = re.compile(r"^MEASURE .*", re.MULTILINE)
RSS_LINE = re.compile(r"(\d+)\s+maximum resident set size")
USER_LINE = re.compile(r"([\d.]+)\s+user")
SYS_LINE = re.compile(r"([\d.]+)\s+sys")


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_output(command):
    return subprocess.run(command, capture_output=True, text=True, check=True).stdout.strip()


def snapshot():
    return {"utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "uptime": command_output(["uptime"]),
            "load": list(os.getloadavg())}


def run_one(binary, operation, n, scale, output_dir, ordinal, kind, allow_under_load):
    before = snapshot()
    if before["load"][0] > MAX_LOAD and not allow_under_load:
        return {"status": "NOT_RUN_LOAD", "before": before}
    env = os.environ.copy()
    env.update({"MEASURE_OP": operation, "MEASURE_N": str(n)})
    if scale is not None:
        env["MEASURE_SCALE"] = str(scale)
    command = ["/usr/bin/time", "-l", str(binary), "court_verify_cpu_measure", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    start = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    elapsed = time.monotonic() - start
    after = snapshot()
    prefix = f"{kind}-{ordinal:02d}"
    (output_dir / f"{prefix}.stdout.log").write_text(result.stdout)
    (output_dir / f"{prefix}.stderr.log").write_text(result.stderr)
    measure = MEASURE_LINE.findall(result.stderr + "\n" + result.stdout)
    rss = RSS_LINE.search(result.stderr)
    user = USER_LINE.search(result.stderr)
    system = SYS_LINE.search(result.stderr)
    return {
        "status": "PASS" if result.returncode == 0 and len(measure) == 1 else "FAILED",
        "command": command, "env": {key: env[key] for key in ("MEASURE_OP", "MEASURE_N", "MEASURE_SCALE") if key in env},
        "exit_code": result.returncode, "wall_seconds": elapsed,
        "before": before, "after": after,
        "under_load": before["load"][0] > MAX_LOAD or after["load"][0] > MAX_LOAD,
        "peak_rss_bytes": int(rss.group(1)) if rss else None,
        "user_seconds": float(user.group(1)) if user else None,
        "system_seconds": float(system.group(1)) if system else None,
        "measure": measure[0] if len(measure) == 1 else None,
        "stdout_log": f"{prefix}.stdout.log", "stderr_log": f"{prefix}.stderr.log",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-record", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--op", required=True)
    parser.add_argument("--n", type=int, default=1000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--baseline-runs", type=int, default=3)
    parser.add_argument("--scale", type=int)
    parser.add_argument("--allow-under-load", action="store_true", help="exploratory only; recorded UNDER_LOAD and not arming evidence")
    args = parser.parse_args()
    if args.n < 20 or args.runs < 1 or args.baseline_runs < 1:
        parser.error("n must be >=20 and both run counts positive")
    record = json.loads(args.build_record.read_text())
    if record.get("stage") != "built" or record.get("exit_code") != 0 or record.get("identity_matches") is not True:
        parser.error("build record does not establish a matching release executable")
    binary = Path(record["pinned_binary"]["path"])
    if sha256(binary) != record["pinned_binary"]["sha256"]:
        parser.error("pinned binary SHA-256 changed")
    args.out.mkdir(parents=True, exist_ok=False)
    result = {
        "operation": args.op, "n": args.n, "runs_requested": args.runs, "baseline_runs_requested": args.baseline_runs,
        "scale": args.scale, "source_commit": record["head"], "source_files": record["files"],
        "binary_path": str(binary), "binary_sha256": record["pinned_binary"]["sha256"],
        "build_command": record["command"], "features": "default", "minimum_hardware": False,
        "hardware": command_output(["sysctl", "hw.model", "hw.memsize", "hw.ncpu"]),
        "runs": [], "status": "NOT_RUN",
    }
    for kind, count, n in (("baseline", args.baseline_runs, 0), ("sample", args.runs, args.n)):
        for ordinal in range(count):
            one = run_one(binary, args.op, n, args.scale, args.out, ordinal, kind, args.allow_under_load)
            result["runs"].append(one)
            (args.out / "raw.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
            if one["status"] != "PASS":
                result["status"] = one["status"]
                (args.out / "raw.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
                return 75 if one["status"] == "NOT_RUN_LOAD" else 1
    result["status"] = "UNDER_LOAD" if any(row["under_load"] for row in result["runs"]) else "PASS"
    (args.out / "raw.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"op": args.op, "status": result["status"], "runs": len(result["runs"]), "output": str(args.out)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
