#!/usr/bin/env python3
"""Combine what every platform's binary printed into one RELEASE-INFO.json and a summary.

    misaka-release-info.py --release-json release.json --tag <tag> --commit <sha> \
        --rust <rustc -V> --out RELEASE-INFO.json [--summary $GITHUB_STEP_SUMMARY] \
        release-identity-*.json

Every field of the release's identity comes from a file, never from a hand-typed workflow line:
the network, the declared genesis and `consensus_frozen` from `release.json`, and the
fingerprint, fence schedule and schedule id from what each platform's `kaspad` printed at startup
(`misaka-release-smoke.py`). It fails if any platform failed its smoke, or if two platforms
printed different identities — a consensus fingerprint that depends on the platform is a fork.

    misaka-release-info.py --selftest
"""

import argparse
import json
import sys


def combine(declared, identities, tag, commit, rust):
    problems = []
    if not identities:
        problems.append("no platform identities were given")
    for ident in identities:
        if not ident.get("matches_release_json"):
            problems.append(f"{ident.get('platform') or '?'} failed its smoke: {'; '.join(ident.get('problems') or ['no detail'])}")
    keys = ("network", "consensus_params_fingerprint", "fence_schedule", "consensus_schedule_id")
    seen = {tuple(json.dumps(i.get(k)) for k in keys) for i in identities}
    if len(seen) > 1:
        problems.append("platforms printed different identities: " + ", ".join(f"{i.get('platform')}={i.get('consensus_params_fingerprint')}" for i in identities))
    first = identities[0] if identities else {}
    info = {
        "schema": "misaka/release-info/v1",
        "release_tag": tag,
        "git_commit": commit,
        "rust_toolchain": rust,
        "network": declared["network"],
        "consensus_frozen": bool(declared.get("consensus_frozen", False)),
        "consensus_params_fingerprint": first.get("consensus_params_fingerprint"),
        "consensus_schedule_id": first.get("consensus_schedule_id"),
        "fence_schedule": first.get("fence_schedule"),
        "genesis_declared": declared.get("genesis"),
        "platforms": sorted(i.get("platform") or "?" for i in identities),
        "identity_verified_on_every_platform": not problems,
    }
    return info, problems


def summary_markdown(info, problems):
    ok = "PASS" if not problems else "FAIL"
    rows = [
        ("Release", f"`{info['release_tag']}`"),
        ("Git commit", f"`{info['git_commit']}`"),
        ("Rust", f"`{info['rust_toolchain']}`"),
        ("Network", f"`{info['network']}`"),
        ("Consensus frozen", "yes" if info["consensus_frozen"] else "no"),
        ("Consensus params fingerprint", f"`{info['consensus_params_fingerprint']}`"),
        ("Fence schedule", "`" + ", ".join(info["fence_schedule"] or []) + "`"),
        ("Schedule id", f"`{info['consensus_schedule_id']}`"),
        ("Genesis (declared)", f"`{info['genesis_declared']}`"),
        ("Platforms smoke-started", ", ".join(info["platforms"])),
        ("Identity identical on every platform", ok),
    ]
    out = ["## MISAKA release identity", "", "| | |", "|---|---|"]
    out += [f"| {k} | {v} |" for k, v in rows]
    if problems:
        out += ["", "**Problems:**", ""] + [f"- {p}" for p in problems]
    return "\n".join(out) + "\n"


def selftest():
    declared = {"network": "testnet-12", "genesis": "g" * 128, "consensus_frozen": False}
    a = {"platform": "x86_64-unknown-linux-musl", "network": "testnet-12", "consensus_params_fingerprint": "a" * 64,
         "fence_schedule": ["1000"], "consensus_schedule_id": "b" * 64, "matches_release_json": True, "problems": []}
    b = dict(a, platform="aarch64-unknown-linux-gnu")
    info, problems = combine(declared, [a, b], "t", "c", "rustc 1.93.0")
    assert not problems and info["identity_verified_on_every_platform"], problems
    assert info["platforms"] == ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]
    _, problems = combine(declared, [a, dict(b, consensus_params_fingerprint="f" * 64)], "t", "c", "r")
    assert any("different identities" in p for p in problems), problems
    _, problems = combine(declared, [a, dict(b, matches_release_json=False, problems=["x"])], "t", "c", "r")
    assert any("failed its smoke" in p for p in problems), problems
    _, problems = combine(declared, [], "t", "c", "r")
    assert problems
    assert "FAIL" in summary_markdown(*combine(declared, [], "t", "c", "r"))
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--release-json", default="release.json")
    ap.add_argument("--tag")
    ap.add_argument("--commit")
    ap.add_argument("--rust", default="")
    ap.add_argument("--out")
    ap.add_argument("--summary")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("identities", nargs="*")
    args = ap.parse_args()
    if args.selftest:
        selftest()
        return 0
    if not (args.tag and args.commit and args.out):
        ap.error("--tag, --commit and --out are required")
    with open(args.release_json, encoding="utf-8") as f:
        declared = json.load(f)
    identities = []
    for path in args.identities:
        with open(path, encoding="utf-8") as f:
            identities.append(json.load(f))
    info, problems = combine(declared, identities, args.tag, args.commit, args.rust)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(info, f, indent=2)
        f.write("\n")
    md = summary_markdown(info, problems)
    print(md)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as f:
            f.write(md)
    if problems:
        for p in problems:
            print("RELEASE INFO FAILED:", p, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
