#!/usr/bin/env python3
"""Run prepared parser cases in fresh processes; preserve all raw evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

CASES = (
    "actual", "maximum_dense", "last_leaf_mismatch", "giant_string",
    "sparse_legal", "sparse_out_of_range", "duplicate_key_last", "many_keys",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def observation():
    return {
        "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "uname": list(platform.uname()),
        "cpuCount": os.cpu_count(),
        "cpuinfo": Path("/proc/cpuinfo").read_text(),
        "meminfo": Path("/proc/meminfo").read_text(),
        "loadavg": Path("/proc/loadavg").read_text().strip(),
        "competingProcesses": subprocess.check_output(
            ["ps", "-eo", "pid,comm,pcpu,pmem,rss", "--sort=-rss"], text=True
        ).splitlines()[:12],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("cases", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--repetitions", type=int, default=5, choices=range(1, 101))
    args = parser.parse_args()
    binary, cases, output = args.binary.resolve(), args.cases.resolve(), args.output.resolve()
    output.mkdir()  # Existing evidence is never overwritten.
    record = {
        "schema": "palw-tokenizer-parser-linux-measurement-v1",
        "sourceCommit": args.source_commit,
        "binarySha256": digest(binary),
        "runnerSha256": digest(Path(__file__)),
        "before": observation(),
        "runs": [],
        "scope": "Parser only; shared host, default affinity, nice 10. Not complete native service qualification.",
        "publicProfileSelected": False,
        "publicDeployment": False,
    }
    env = dict(os.environ, LC_ALL="C")
    for case in CASES:
        command = ["/usr/bin/time", "-v", "-o", str(output / (case + ".time")),
                   "nice", "-n", "10", str(binary), "run", str(cases), case, str(args.repetitions)]
        with (output / (case + ".jsonl")).open("w") as stdout, (output / (case + ".stderr")).open("w") as stderr:
            completed = subprocess.run(command, env=env, stdout=stdout, stderr=stderr)
        run = {
            "case": case, "command": command, "exitCode": completed.returncode,
            "rawSha256": digest(cases / (case + ".raw")),
            "operandsSha256": digest(cases / (case + ".case")),
            "stdoutSha256": digest(output / (case + ".jsonl")),
            "stderrSha256": digest(output / (case + ".stderr")),
            "timeSha256": digest(output / (case + ".time")),
        }
        record["runs"].append(run)
        # Persist progress even if a later invocation is interrupted.
        (output / "measurement.json").write_text(json.dumps(record, indent=2) + "\n")
        print(json.dumps({"case": case, "exitCode": completed.returncode}), flush=True)
    record["after"] = observation()
    (output / "measurement.json").write_text(json.dumps(record, indent=2) + "\n")
    raise SystemExit(0 if all(run["exitCode"] == 0 for run in record["runs"]) else 1)


if __name__ == "__main__":
    main()
