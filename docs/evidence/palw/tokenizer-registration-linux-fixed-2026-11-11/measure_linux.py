#!/usr/bin/env python3
"""Run prepared tokenizer measurements in fresh processes; preserve raw evidence."""
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
    parser.add_argument("--core-source-commit", help="Core revision when reusing an identified release library")
    parser.add_argument("--mode", choices=("parser", "native", "native-multipass", "native-candidate", "scanner"), default="parser")
    parser.add_argument("--source-count", type=int, default=780, choices=range(1, 781), metavar="N", help="Scanner sources, 1..780")
    parser.add_argument("--repetitions", type=int, default=5, choices=range(1, 101), metavar="N", help="Repetitions, 1..100")
    args = parser.parse_args()
    binary, cases, output = args.binary.resolve(), args.cases.resolve(), args.output.resolve()
    output.mkdir()  # Existing evidence is never overwritten.
    record = {
        "schema": "palw-tokenizer-linux-measurement-v2",
        "sourceCommit": args.source_commit,
        "coreSourceCommit": args.core_source_commit or args.source_commit,
        "sourceCommitScope": "measurement tool; core separately identified" if args.core_source_commit else "entire source",
        "binarySha256": digest(binary),
        "runnerSha256": digest(Path(__file__)),
        "before": observation(),
        "runs": [],
        "scope": {"parser": "Parser", "native": "Two-pass native replay of seeded accepted-parent bundles",
                  "native-multipass": "Five-pass native replay of seeded accepted-parent bundles",
                  "native-candidate": "Modeled 780-carrier native candidate; no DAG or UTXO validation",
                  "scanner": "Source/candidate scanner only; no native fold or parser"}[args.mode]
                 + "; shared host, default affinity, nice 10. Not complete native service qualification.",
        "mode": args.mode,
        "publicProfileSelected": False,
        "publicDeployment": False,
    }
    env = dict(os.environ, LC_ALL="C")
    jobs = [(case, case, ["run", str(cases), case, str(args.repetitions)]) for case in CASES]
    if args.mode in ("native", "native-multipass", "native-candidate"):
        command = {"native": "native-run", "native-multipass": "native-run-multipass", "native-candidate": "native-run-candidate"}[args.mode]
        jobs = [(case, f"{case}-{iteration}", [command, str(cases / case)])
                for case in ("actual", "maximum_raw") for iteration in range(args.repetitions)]
    elif args.mode == "scanner":
        jobs = [("actual", f"scanner-{iteration}", ["native-scan-max", str(cases / "actual"), str(args.source_count)])
                for iteration in range(args.repetitions)]
    for case, name, arguments in jobs:
        command = ["/usr/bin/time", "-v", "-o", str(output / (name + ".time")),
                   "nice", "-n", "10", str(binary), *arguments]
        with (output / (name + ".jsonl")).open("w") as stdout, (output / (name + ".stderr")).open("w") as stderr:
            completed = subprocess.run(command, env=env, stdout=stdout, stderr=stderr)
        run = {
            "case": case, "name": name, "command": command, "exitCode": completed.returncode,
            "stdoutSha256": digest(output / (name + ".jsonl")),
            "stderrSha256": digest(output / (name + ".stderr")),
            "timeSha256": digest(output / (name + ".time")),
        }
        if args.mode == "parser":
            run.update(rawSha256=digest(cases / (case + ".raw")), operandsSha256=digest(cases / (case + ".case")))
        else:
            run["bundleFilesSha256"] = {p.name: digest(p) for p in sorted((cases / case).iterdir()) if p.is_file()}
        record["runs"].append(run)
        # Persist progress even if a later invocation is interrupted.
        (output / "measurement.json").write_text(json.dumps(record, indent=2) + "\n")
        print(json.dumps({"case": case, "name": name, "exitCode": completed.returncode}), flush=True)
    record["after"] = observation()
    (output / "measurement.json").write_text(json.dumps(record, indent=2) + "\n")
    raise SystemExit(0 if all(run["exitCode"] == 0 for run in record["runs"]) else 1)


if __name__ == "__main__":
    main()
