#!/usr/bin/env python3
"""Measure the ignored EXEC origin reader probe in isolated processes; never approve a resource gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

TEST = "palw_state_v2::tests::exec_v2_fold_v1::exec_v2_origin_resource_probe"
CASES = ("baseline", "wrong-class", "matching-fake", "pinned-identity", "eight-refusals", "one-parent")


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def duration_ms(value):
    if value == "None":
        return None
    match = re.fullmatch(r"Some\(([0-9.]+)(ns|µs|ms|s)\)", value)
    if not match:
        raise ValueError(f"unexpected Rust duration {value!r}")
    return float(match[1]) * {"ns": 1e-6, "µs": 1e-3, "ms": 1, "s": 1000}[match[2]]


def parse(text, case):
    if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in text:
        raise ValueError("probe did not report exactly one successful explicitly invoked test")
    match = re.search(
        r"\[EXEC origin resource\] case=(\S+) parents=(\d+) levels=(\d+) encoded_bytes=(\d+) "
        r"parent_bytes_per_digest=(\d+) repetitions=(\d+) samples=(\d+) "
        r"median_batch=(\S+) median_per_reader=(\S+)", text,
    )
    if not match or match[1] != case:
        raise ValueError("missing or mismatched probe case")
    if sys.platform == "darwin":
        rss = re.search(r"^\s*(\d+)\s+maximum resident set size\s*$", text, re.M)
        multiplier = 1
    else:
        rss = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", text)
        multiplier = 1024
    if not rss:
        raise ValueError("time did not report maximum RSS")
    samples = int(match[7])
    if samples != (0 if case == "baseline" else 6):
        raise ValueError("unexpected sample count")
    return {
        "case": case,
        "parents": int(match[2]),
        "levels": int(match[3]),
        "encoded_bytes": int(match[4]),
        "parent_bytes_per_digest": int(match[5]),
        "repetitions": int(match[6]),
        "samples": samples,
        "median_batch_raw": match[8],
        "median_per_reader_raw": match[9],
        "median_batch_ms": duration_ms(match[8]),
        "median_per_reader_ms": duration_ms(match[9]),
        "maximum_process_rss_bytes": int(rss[1]) * multiplier,
    }


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--binary", required=True, type=Path, help="Cargo's actual consensus-core test executable")
    args.add_argument("--profile", required=True, choices=("dev-opt2", "canonical-release"))
    args.add_argument("--build-log", type=Path, help="required for release: Cargo JSON artifact/profile evidence")
    args.add_argument("--output-dir", required=True, type=Path, help="new directory; existing evidence is never overwritten")
    args.add_argument("--cases", nargs="+", choices=CASES, default=CASES)
    options = args.parse_args()
    if sys.platform not in ("darwin", "linux"):
        args.error("maximum-RSS measurement currently supports macOS and Linux")
    binary = options.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        args.error("binary must be an executable file")
    if len(set(options.cases)) != len(options.cases):
        args.error("duplicate cases would overwrite logs")
    artifact = None
    if options.profile == "canonical-release":
        if options.build_log is None:
            args.error("canonical release requires its Cargo JSON build log")
        for line in options.build_log.read_text(encoding="utf-8").splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if item.get("reason") == "compiler-artifact" and item.get("executable"):
                if Path(item["executable"]).resolve() == binary and item["target"]["name"] == "kaspa_consensus_core":
                    artifact = item
        if not artifact or artifact["profile"].get("opt_level") != "3" or not artifact["profile"].get("test"):
            args.error("release binary does not match an opt-level 3 Cargo test artifact in the supplied log")
        if artifact["profile"].get("debug_assertions") or not artifact["profile"].get("overflow_checks"):
            args.error("release artifact does not match the workspace assertion/overflow policy")
    options.output_dir.mkdir(parents=True, exist_ok=False)
    timer = ["/usr/bin/time", "-l" if sys.platform == "darwin" else "-v"]
    record = {
        "approval": False,
        "scope": "structural arithmetic fixtures, actual read-only origin admission; not native header admission or a chunk/P2P batch",
        "rss_scope": "whole child process including fixture setup, test harness, loaded code and retained input; not incremental reader RSS",
        "profile_label": options.profile,
        "profile_label_requires_build_log": True,
        "platform": platform.platform(),
        "binary": str(binary),
        "binary_sha256": digest(binary),
        "build_log": str(options.build_log.resolve()) if options.build_log else None,
        "build_log_sha256": digest(options.build_log) if options.build_log else None,
        "cargo_artifact_profile": artifact["profile"] if artifact else None,
        "cases": [],
    }
    summary = options.output_dir / "summary.json"
    for case in options.cases:
        log = options.output_dir / f"{case}.log"
        environment = os.environ.copy()
        environment["MISAKA_EXEC_ORIGIN_RESOURCE_CASE"] = case
        environment["LC_ALL"] = "C"
        command = timer + [str(binary), "--ignored", "--exact", TEST, "--nocapture", "--test-threads=1"]
        with log.open("w", encoding="utf-8") as output:
            result = subprocess.run(command, env=environment, stdout=output, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(f"{case} exited {result.returncode}; preserved log: {log}")
        row = parse(log.read_text(encoding="utf-8"), case)
        row["log"] = str(log.resolve())
        row["command"] = command
        record["cases"].append(row)
        summary.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(row, ensure_ascii=False), flush=True)
    if digest(binary) != record["binary_sha256"]:
        raise RuntimeError("binary changed during measurement; results are not a stable-binary comparison")
    record["complete"] = True
    summary.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Evidence: {summary}")


if __name__ == "__main__":
    main()
