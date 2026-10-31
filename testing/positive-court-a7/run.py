#!/usr/bin/env python3
"""Run A7 against ONE source tree; preserve raw logs, hashes and explicit gaps."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]


def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="fresh output directory; never overwrites evidence")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    build_env = {"CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0"}
    env = dict(os.environ, PC_A7_REPORT_DIR=str(out), **build_env)
    # Hash the complete relevant source trees, including independently added tests.
    source = {}
    for tree in ["consensus/core", "misaka/palw/tir/core", "misaka/palw/tir/exec", "testing/positive-court-a7"]:
        for path in sorted((ROOT / tree).rglob("*")):
            if path.is_file() and path.suffix in {".rs", ".toml", ".py", ".md"}:
                if out == path or out in path.parents:
                    continue
                source[str(path.relative_to(ROOT))] = sha(path)
    record = {
        "schema": "positive-court-a7-run-v1",
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "commit": capture("git", "rev-parse", "HEAD"),
        "status_before": capture("git", "status", "--short"),
        "host": {"platform": platform.platform(), "machine": platform.machine(), "cpus": os.cpu_count()},
        "rustc": capture("rustc", "-Vv"), "cargo": capture("cargo", "-V"),
        "build_env": build_env,
        "source_sha256": source,
        "cargo_lock_sha256": sha(ROOT / "Cargo.lock"),
        "activation_allowed": False,
        "unverified": ["native carrier/admission integration", "actual reward/DAG/settlement consumers",
                       "native reorg/pruning/IBD", "crash-consistent source-fee/attempt persistence",
                       "provider/cold-model delivery", "real LLM and concurrent network load",
                       "approved resource caps and production p95/p99"],
        "commands": [],
    }
    commands = [
        ["cargo", "test", "-p", "misaka-palw-tir-exec", "--features", "node", "--test", "positive_court_a7", "--no-run", "--message-format=json"],
        ["cargo", "test", "-p", "misaka-palw-tir-exec", "--features", "node", "--test", "positive_court_a7", "--", "--nocapture", "--test-threads=1"],
        ["cargo", "test", "-p", "misaka-palw-tir-exec", "--features", "node", "--test", "positive_court_roles", "--test", "wide_q24", "--test", "court", "--test", "node_leg", "--test", "capture_codec"],
        ["cargo", "test", "-p", "kaspa-consensus-core", "--lib", "palw_positive_court_v2"],
        ["cargo", "test", "-p", "kaspa-consensus-core", "--lib", "palw_tir"],
        ["cargo", "test", "-p", "kaspa-consensus-core", "--test", "palw_tir_v1_fence"],
        ["cargo", "test", "-p", "kaspa-consensus-core", "--lib", "palw_kernel_route_fold_v1"],
    ]
    for i, command in enumerate(commands):
        command = [command[0], "--config", "profile.dev.package.kaspa-consensus-core.opt-level=0",
                   "--config", "profile.test.package.kaspa-consensus-core.opt-level=0", *command[1:]]
        print(f"A7 [{i+1}/{len(commands)}] {' '.join(command)}", flush=True)
        log = out / f"{i:02d}.log"
        start = time.monotonic()
        with log.open("w") as stream:
            try:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=900)
                code = result.returncode
            except subprocess.TimeoutExpired:
                code = "TIMEOUT"
        counts = [{"passed":int(p), "failed":int(f), "ignored":int(i)} for p, f, i in
                  re.findall(r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", log.read_text())]
        entry = {"argv": command, "exit_code": code, "test_counts":counts,
                 "elapsed_seconds": time.monotonic()-start, "log": log.name, "sha256": sha(log)}
        if i > 0 and code == 0 and sum(x["passed"] for x in counts) == 0:
            entry["validation_error"] = "NO_EXECUTED_TESTS"
            code = "NO_EXECUTED_TESTS"
        record["commands"].append(entry)
        if i == 0 and code == 0:
            for line in log.read_text().splitlines():
                try:
                    item = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if (item.get("reason") == "compiler-artifact" and item.get("executable")
                        and item.get("target", {}).get("name") == "positive_court_a7"):
                    binary = Path(item["executable"])
                    record["a7_binary"] = {"path": str(binary), "sha256": sha(binary)}
        (out / "run.json").write_text(json.dumps(record, indent=2)+"\n")
        if code != 0:
            print(log.read_text()[-10000:], file=sys.stderr)
            return 1
    report = out / "process-report.json"
    if not report.is_file():
        raise RuntimeError("process acceptance produced no report")
    record["process_report_sha256"] = sha(report)
    record["result"] = "PASS_PRIVATE_PROTOTYPE_ONLY"
    record["finished_utc"] = datetime.now(timezone.utc).isoformat()
    (out / "run.json").write_text(json.dumps(record, indent=2)+"\n")
    print(f"A7 PASS (private prototype only): {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
