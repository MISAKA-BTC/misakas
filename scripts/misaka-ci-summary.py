#!/usr/bin/env python3
"""Turn the test run's JUnit report into the CI job summary, so no test count is ever hand-typed.

    misaka-ci-summary.py --junit target/nextest/ci/junit.xml --commit <sha> --rust "<rustc -V>" \
        [--release-json release.json] [--readiness docs/mainnet-readiness.md] >> $GITHUB_STEP_SUMMARY

`cargo nextest run` writes the JUnit file under the `ci` profile (`.config/nextest.toml`; the
Test Suite job sets `NEXTEST_PROFILE=ci`). Every number this prints is read from a file of the
run: tests and failures from the JUnit report, grouped by area from each test binary's crate
name; the network and fingerprint the tree declares from `release.json` (labelled
*declared*: the release workflow is where a built binary is made to print them); and the
readiness tally from the PASS / PARTIAL / TODO marks in `docs/mainnet-readiness.md`.

It exits 0 even when tests failed. The test step's own status is the verdict; this step only
reports it, and it must still run (`if: always()`) when that step went red.

    misaka-ci-summary.py --selftest
"""

import argparse
import json
import os
import re
import sys
import xml.etree.ElementTree as ET

# First match wins, so the specific areas come before the broad ones.
AREAS = (
    ("PALW", lambda c: "palw" in c),
    ("EVM", lambda c: "evm" in c),
    ("Consensus", lambda c: c.startswith("kaspa-consensus") or c in ("kaspa-pow", "kaspa-math", "kaspa-merkle", "kaspa-muhash")),
    ("Crypto / PQ", lambda c: "pq" in c or c.startswith(("kaspa-hashes", "kaspa-txscript", "kaspa-addresses", "kaspa-bip32")) or "crypto" in c),
    ("RPC", lambda c: "rpc" in c),
    ("Wallet", lambda c: "wallet" in c),
    ("Node & network", lambda c: c.startswith(("kaspad", "kaspa-p2p", "kaspa-mining", "kaspa-index", "kaspa-utxoindex", "kaspa-database", "kaspa-notify", "kaspa-connectionmanager", "kaspa-addressmanager", "kaspa-perf-monitor", "kaspa-core", "kaspa-testing"))),
)


def area_of(suite_name):
    crate = re.split(r"::|/", suite_name, maxsplit=1)[0]
    for name, match in AREAS:
        if match(crate):
            return name
    return "Other"


def read_junit(path):
    """{area: [tests, failures]} and the names of failed tests."""
    root = ET.parse(path).getroot()
    suites = [root] if root.tag == "testsuite" else root.iter("testsuite")
    areas, failed = {}, []
    for suite in suites:
        area = areas.setdefault(area_of(suite.get("name", "")), [0, 0])
        for case in suite.iter("testcase"):
            if case.find("skipped") is not None:
                continue
            area[0] += 1
            if case.find("failure") is not None or case.find("error") is not None:
                area[1] += 1
                failed.append(f"{suite.get('name', '')} :: {case.get('name', '')}")
    return areas, failed


def readiness_tally(path):
    counts = {"PASS": 0, "PARTIAL": 0, "TODO": 0}
    with open(path, encoding="utf-8") as f:
        for line in f:
            # An item row has three cells (status | item | evidence); the legend's rows have two.
            m = re.match(r"\|\s*\*\*(PASS|PARTIAL|TODO)\*\*\s*\|", line)
            if m and line.count("|") >= 4:
                counts[m.group(1)] += 1
    return counts


def render(areas, failed, commit, rust, declared, readiness):
    total = sum(a[0] for a in areas.values())
    failures = sum(a[1] for a in areas.values())
    order = [n for n, _ in AREAS] + ["Other"]
    out = ["## MISAKA CI summary", "", "| | |", "|---|---|"]
    out.append(f"| Commit | `{commit}` |")
    out.append(f"| Rust | `{rust}` |")
    if declared:
        out.append(f"| Network (declared) | `{declared.get('network')}` |")
        out.append(f"| Consensus fingerprint (declared) | `{declared.get('consensus_params_fingerprint')}` |")
        out.append(f"| Consensus frozen | {'yes' if declared.get('consensus_frozen') else 'no'} |")
    if readiness:
        out.append(f"| Mainnet readiness | {readiness['PASS']} PASS · {readiness['PARTIAL']} PARTIAL · {readiness['TODO']} TODO |")
    out += ["", "| area | tests | failed |", "|---|---:|---:|"]
    for name in order:
        if name in areas:
            t, f = areas[name]
            out.append(f"| {name} | {t:,} | {f:,} |")
    out.append(f"| **Total** | **{total:,}** | **{failures:,}** |")
    if failed:
        out += ["", f"**Failed tests** ({len(failed)}{', first 50 shown' if len(failed) > 50 else ''}):", ""]
        out += [f"- `{name}`" for name in failed[:50]]
    return "\n".join(out) + "\n"


def selftest():
    import tempfile
    xml = """<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="5" failures="1" errors="0">
  <testsuite name="kaspa-consensus-core" tests="2" failures="0" errors="0">
    <testcase name="a" classname="x"/><testcase name="b" classname="x"/>
  </testsuite>
  <testsuite name="misaka-palw-base0::integration" tests="2" failures="1" errors="0">
    <testcase name="c" classname="x"/><testcase name="d" classname="x"><failure message="boom"/></testcase>
  </testsuite>
  <testsuite name="kaspa-wrpc-server" tests="1" failures="0" errors="0">
    <testcase name="e" classname="x"/><testcase name="f" classname="x"><skipped/></testcase>
  </testsuite>
</testsuites>"""
    with tempfile.NamedTemporaryFile("w", suffix=".xml", delete=False) as f:
        f.write(xml)
    areas, failed = read_junit(f.name)
    os.unlink(f.name)
    assert areas == {"Consensus": [2, 0], "PALW": [2, 1], "RPC": [1, 0]}, areas
    assert failed == ["misaka-palw-base0::integration :: d"], failed
    md = render(areas, failed, "abc", "rustc 1.93.0", {"network": "testnet-12", "consensus_params_fingerprint": "f" * 64}, {"PASS": 1, "PARTIAL": 2, "TODO": 3})
    assert "| **Total** | **5** | **1** |" in md, md
    assert "1 PASS · 2 PARTIAL · 3 TODO" in md
    assert area_of("kaspa-pq-validator") == "Crypto / PQ" and area_of("kaspad") == "Node & network" and area_of("kaspa-evm") == "EVM"
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--junit")
    ap.add_argument("--commit", default=os.environ.get("GITHUB_SHA", ""))
    ap.add_argument("--rust", default="")
    ap.add_argument("--release-json", default="release.json")
    ap.add_argument("--readiness", default="docs/mainnet-readiness.md")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        selftest()
        return 0
    if not args.junit or not os.path.exists(args.junit):
        print("## MISAKA CI summary\n\nNo JUnit report was written: the test step did not reach the test run.")
        return 0
    areas, failed = read_junit(args.junit)
    declared = None
    if os.path.exists(args.release_json):
        with open(args.release_json, encoding="utf-8") as f:
            declared = json.load(f)
    readiness = readiness_tally(args.readiness) if os.path.exists(args.readiness) else None
    sys.stdout.write(render(areas, failed, args.commit, args.rust, declared, readiness))
    return 0


if __name__ == "__main__":
    sys.exit(main())
