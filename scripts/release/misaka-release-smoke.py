#!/usr/bin/env python3
"""Start a built `kaspad` once and read the identity it prints, before any peer or database.

    misaka-release-smoke.py --kaspad bin/kaspad --release-json release.json \
        --platform <triple> --out release-identity-<triple>.json

**A release names the network it is for in one place, `release.json`, and this script
holds the binary to it.** It starts the node on that network in a throwaway app directory with DNS
seeding off, waits for the two lines `kaspad/src/daemon.rs` prints before it opens a database —
`Consensus params fingerprint: <fp> (network <net>)` and
`Consensus fence schedule: <heights> (schedule id <id>)` — stops the node, and compares:

  * the network the binary says it runs   == release.json `network`
  * the fingerprint it prints             == release.json `consensus_params_fingerprint`
  * the schedule id it prints             == release.json `consensus_schedule_id`

Any mismatch, a node that exits before printing them, or no lines within the timeout, fails with
a non-zero exit. What was measured is written to `--out` either way, so a failed release still
shows what the binary said. `genesis` is not printed at startup; it is carried through as declared.

Plain Python 3.8+, no dependencies: it runs on the Linux, Windows and macOS release runners.
    misaka-release-smoke.py --selftest   checks the parser and the comparison without a node
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import time

FP_RE = re.compile(r"Consensus params fingerprint: ([0-9a-f]{64}) \(network ([A-Za-z0-9-]+)\)")
SCHED_RE = re.compile(r"Consensus fence schedule: (.*) \(schedule id ([0-9a-f]{64})\)")


def network_args(network):
    """The kaspad flags that select `network` (the names `Params::net` prints)."""
    if network == "mainnet":
        return []
    m = re.fullmatch(r"(testnet|devnet|simnet)(?:-(\d+))?", network)
    if not m:
        raise SystemExit(f"release.json names a network this script cannot select: {network!r}")
    args = ["--" + m.group(1)]
    if m.group(2) is not None:
        args.append("--netsuffix=" + m.group(2))
    return args


def parse(lines):
    """The identity the node printed, from its log lines; missing fields are None."""
    got = {"network": None, "consensus_params_fingerprint": None, "fence_schedule": None, "consensus_schedule_id": None}
    for line in lines:
        m = FP_RE.search(line)
        if m:
            got["consensus_params_fingerprint"], got["network"] = m.group(1), m.group(2)
        m = SCHED_RE.search(line)
        if m:
            got["fence_schedule"] = [h.strip() for h in m.group(1).split(",") if h.strip()]
            got["consensus_schedule_id"] = m.group(2)
    return got


def compare(declared, measured):
    """Every way the binary disagrees with release.json, as sentences; empty means it matches."""
    problems = []
    for key in ("network", "consensus_params_fingerprint", "consensus_schedule_id"):
        if measured.get(key) is None:
            problems.append(f"the node never printed its {key}")
        elif measured[key] != declared[key]:
            problems.append(f"{key}: release.json says {declared[key]}, the binary says {measured[key]}")
    return problems


def run_node(kaspad, network, timeout):
    appdir = tempfile.mkdtemp(prefix="misaka-release-smoke-")
    cmd = [kaspad, *network_args(network), "--appdir", appdir, "--nodnsseed", "--nologfiles"]
    print("starting:", " ".join(cmd), flush=True)
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    lines = []

    def reader():
        for line in proc.stdout:
            lines.append(line.rstrip("\n"))
            print("  |", line.rstrip("\n"), flush=True)

    t = threading.Thread(target=reader, daemon=True)
    t.start()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        got = parse(lines)
        if got["consensus_params_fingerprint"] and got["consensus_schedule_id"]:
            break
        if proc.poll() is not None:
            break
        time.sleep(0.2)
    exited_early = proc.poll() is not None
    if not exited_early:
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    t.join(timeout=5)
    return parse(lines), exited_early, proc.returncode


def selftest():
    declared = {"network": "testnet-12", "consensus_params_fingerprint": "a" * 64, "consensus_schedule_id": "b" * 64}
    good = [
        "2026-09-25 INFO kaspad v1.0.0-abc",
        "2026-09-25 INFO Consensus params fingerprint: " + "a" * 64 + " (network testnet-12)",
        "2026-09-25 INFO Consensus fence schedule: 1000 (schedule id " + "b" * 64 + ")",
    ]
    m = parse(good)
    assert m["fence_schedule"] == ["1000"], m
    assert compare(declared, m) == [], compare(declared, m)
    wrong_net = [good[1].replace("testnet-12", "testnet-11"), good[2]]
    assert any("network" in p for p in compare(declared, parse(wrong_net)))
    wrong_fp = [good[1].replace("a" * 64, "c" * 64), good[2]]
    assert any("fingerprint" in p for p in compare(declared, parse(wrong_fp)))
    assert len(compare(declared, parse(good[:1]))) == 3
    assert parse([good[1], "Consensus fence schedule: 1150, 1900, 2125000 (schedule id " + "d" * 64 + ")"])["fence_schedule"] == ["1150", "1900", "2125000"]
    assert network_args("testnet-12") == ["--testnet", "--netsuffix=12"]
    assert network_args("mainnet") == []
    assert network_args("devnet") == ["--devnet"]
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--kaspad")
    ap.add_argument("--release-json", default="release.json")
    ap.add_argument("--platform", default="")
    ap.add_argument("--out")
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        selftest()
        return 0
    if not args.kaspad or not args.out:
        ap.error("--kaspad and --out are required")
    with open(args.release_json, encoding="utf-8") as f:
        declared = json.load(f)
    measured, exited_early, code = run_node(os.path.abspath(args.kaspad), declared["network"], args.timeout)
    problems = compare(declared, measured)
    if exited_early and problems:
        problems.append(f"the node exited early with status {code}")
    result = {"platform": args.platform, **measured, "matches_release_json": not problems, "problems": problems}
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps(result, indent=2))
    if problems:
        for p in problems:
            print("SMOKE FAILED:", p, file=sys.stderr)
        return 1
    print(f"smoke ok: {args.platform or 'this platform'} prints the identity release.json declares")
    return 0


if __name__ == "__main__":
    sys.exit(main())
