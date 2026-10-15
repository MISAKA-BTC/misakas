#!/usr/bin/env python3
"""**RFC-0019 §8 — the signed release manifest: what an operator checks BEFORE installing, and nothing it runs.**

A staged rollout (RFC-0019) ships the new rules dormant behind a fence; operators install the new binary at a time they choose
while consensus stays on the old rules, and at the fence height (a catalog edition's `activation_daa`, or a flag day such as
testnet-12's DAA 14,000) the new rules apply to everyone. This script is the operator's half of that: it verifies a release
manifest (schema `misaka/release-manifest/v1`) and never installs, copies or executes a binary — there is no auto-update path.

What a manifest binds, and what `verify` checks:

  * **signatures** — k-of-n OpenSSH signatures (`ssh-keygen -Y sign -n misaka-release`) over the canonical payload (the manifest
    without `signatures`, JSON with sorted keys and no whitespace, UTF-8). An unsigned or under-signed manifest FAILS; there is
    no "warn and continue".
  * **checksums** — every artifact's sha256 and size (against files in `--dir`, when given).
  * **reproducible builds** — every artifact reproduced by at least `--min-builders` independent builders reporting the same sha256.
  * **the consensus fingerprint** — `consensus_params_fingerprint`, `consensus_schedule_id` and `fence_schedule` equal to the
    `RELEASE-INFO.json` the release workflow wrote from what each platform's binary printed at startup
    (`misaka-release-smoke.py` / `misaka-release-info.py`), and the switch height is on that schedule.
  * **the annual catalog** — the chain is recomputed from its models (the same canonical bytes and keyed blake2b-512 as
    `consensus/core/src/palw_annual_catalog_v1.rs`), validated (edition 0 at DAA 0, previous-root chaining, strictly increasing
    heights, add-only over the whole chain, kernel version ≤ `--kernel-version-max`), each edition's root must equal the
    manifest's, and only LOCKED_IN / ACTIVE editions may ship.
  * **add-only across releases** — with `--previous`, the previous manifest's chain must be a prefix of this one (same network,
    same genesis); an edition rewritten, dropped or re-rooted fails.
  * **readiness is NOT verified here** — it is an operational measurement (`misaka-release-readiness.py`), never a consensus rule
    and never a vote. The manifest may carry the operational target for the record.

Modes:

    # assemble an UNSIGNED manifest from the workflow's outputs (never signs anything)
    misaka-release-manifest.py write --release-info RELEASE-INFO.json --catalog catalog.json --switch-daa 14000 \
        --lifecycle LOCKED_IN --artifacts-dir bin/ [--builder-report builder-a.json ...] --out manifest.json

    # the bytes a signer signs:  ssh-keygen -Y sign -f <key> -n misaka-release payload.bin
    misaka-release-manifest.py payload manifest.json --out payload.bin

    # attach a signature produced by a signer
    misaka-release-manifest.py attach manifest.json --signer <principal> --sig payload.bin.sig

    # the operator's check (exit 0 only when every check holds)
    misaka-release-manifest.py verify manifest.json --allowed-signers allowed_signers --min-signers 2 \
        [--dir bin/] [--release-info RELEASE-INFO.json] [--previous previous-manifest.json] [--node-status status.json]

    # one catalog's root (the cross-implementation vector in palw_annual_catalog_v1/tests.rs)
    misaka-release-manifest.py catalog-root catalog.json

    misaka-release-manifest.py self-test

Standard-library Python 3.8+ only (hashlib.blake2b); signature checks call the system `ssh-keygen`. Exit 0: written / verified.
Exit 1: a named check failed. Exit 2: usage or an unreadable input.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile

SCHEMA = "misaka/release-manifest/v1"
SIG_NAMESPACE = "misaka-release"
CATALOG_DOMAIN = b"misaka-palw/annual-model-catalog/v1"
CATALOG_ENCODING_VERSION = 1
CATALOG_MAX_MODELS = 1024
CATALOG_MAX_EDITIONS = 64
LIFECYCLES = ("PROPOSED", "TESTED", "PREPARED", "LOCKED_IN", "ACTIVE", "ABORTED")
SHIPPABLE = ("LOCKED_IN", "ACTIVE")
ZERO64 = "00" * 64
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX128 = re.compile(r"^[0-9a-f]{128}$")

TOP_REQUIRED = ("schema", "release", "network", "git_commit", "genesis", "consensus", "annual_catalog", "lifecycle", "builds")
TOP_OPTIONAL = ("rust_toolchain", "timeline", "readiness", "components_manifest_sha256", "signatures", "notes")


class Fail(Exception):
    pass


# ---- the annual catalog (must match consensus/core/src/palw_annual_catalog_v1.rs byte for byte) ------------------------------

def h64(value, what):
    if not isinstance(value, str) or not HEX128.match(value):
        raise Fail(f"{what} is not 128 lowercase hex characters: {value!r}")
    return bytes.fromhex(value)


def catalog_bytes(edition):
    models = edition["models"]
    out = struct.pack("<H", CATALOG_ENCODING_VERSION)
    out += struct.pack("<I", edition["edition"])
    out += struct.pack("<Q", edition["activation_daa"])
    out += h64(edition["previous_catalog_root"], "previous_catalog_root")
    out += struct.pack("<I", len(models))
    for m in models:
        out += h64(m["class_id"], "class_id") + h64(m["artifact_root"], "artifact_root")
        out += struct.pack("<H", m.get("kernel_version", 0))
        out += h64(m.get("verification_plan_id", ZERO64), "verification_plan_id")
    return out


def catalog_root(edition):
    return hashlib.blake2b(catalog_bytes(edition), digest_size=64, key=CATALOG_DOMAIN).hexdigest()


def validate_catalog_chain(editions, fence_daa, kernel_version_max):
    """RFC-0019 §5 — the same refusals as `PalwAnnualCatalogChainV1::validate` (plus the manifest's own roots and lifecycles)."""
    if not isinstance(editions, list) or not editions:
        raise Fail("the annual catalog starts with the genesis catalog (edition 0)")
    if len(editions) > CATALOG_MAX_EDITIONS:
        raise Fail("too many annual catalog editions")
    seen = set()
    previous = None
    for i, e in enumerate(editions):
        for key in ("edition", "activation_daa", "previous_catalog_root", "models", "root", "lifecycle"):
            if key not in e:
                raise Fail(f"edition {i}: missing {key}")
        if e["edition"] != i:
            raise Fail(f"edition {i}: editions are numbered 0, 1, 2, … without gaps (found {e['edition']})")
        if e["lifecycle"] not in SHIPPABLE:
            raise Fail(f"edition {i}: lifecycle {e['lifecycle']} cannot ship in a binary (only LOCKED_IN / ACTIVE)")
        models = e["models"]
        if len(models) > CATALOG_MAX_MODELS:
            raise Fail(f"edition {i}: more than {CATALOG_MAX_MODELS} models")
        if i == 0:
            if e["activation_daa"] != 0 or e["previous_catalog_root"] != ZERO64:
                raise Fail("the genesis catalog is edition 0 at DAA 0 with a zero previous root")
        else:
            if e["previous_catalog_root"] != previous["root"]:
                raise Fail(f"edition {i}: does not chain to the root of edition {i - 1}")
            if not models:
                raise Fail(f"edition {i}: an annual edition adds at least one model")
            daa = e["activation_daa"]
            if not isinstance(daa, int) or daa >= 2 ** 64 - 1 or daa <= previous["activation_daa"]:
                raise Fail(f"edition {i}: activation heights are real and strictly increasing")
            if fence_daa is not None and daa < fence_daa:
                raise Fail(f"edition {i}: activates below the tier fence ({daa} < {fence_daa})")
        last = None
        for m in models:
            cid = h64(m["class_id"], "class_id")
            if cid == bytes(64) or h64(m["artifact_root"], "artifact_root") == bytes(64):
                raise Fail(f"edition {i}: a zero class id or artifact root")
            if last is not None and last >= cid:
                raise Fail(f"edition {i}: models must be strictly ascending by class id (one canonical encoding)")
            last = cid
            if m.get("kernel_version", 0) > kernel_version_max:
                raise Fail(f"edition {i}: kernel version {m.get('kernel_version')} is above this binary's {kernel_version_max} (ADR-0172 §3)")
            if m["class_id"] in seen:
                raise Fail(f"edition {i}: class {m['class_id'][:16]}… is listed again — the catalog only ADDS (ADR-0175)")
            seen.add(m["class_id"])
        recomputed = catalog_root(e)
        if recomputed != e["root"]:
            raise Fail(f"edition {i}: root {e['root'][:16]}… does not match the recomputed {recomputed[:16]}…")
        previous = e


def sort_and_root(editions):
    """Fill `previous_catalog_root` / `root` and sort models (for `write`, never for `verify`)."""
    previous = None
    for i, e in enumerate(editions):
        e["edition"] = i
        e.setdefault("lifecycle", "LOCKED_IN")
        e["models"] = sorted(e.get("models", []), key=lambda m: m["class_id"])
        for m in e["models"]:
            m.setdefault("kernel_version", 0)
            m.setdefault("verification_plan_id", ZERO64)
        if i == 0:
            e["activation_daa"] = e.get("activation_daa", 0)
            e["previous_catalog_root"] = ZERO64
        else:
            e["previous_catalog_root"] = previous["root"]
        e["root"] = catalog_root(e)
        previous = e
    return editions


# ---- the manifest ----------------------------------------------------------------------------------------------------------

def canonical_payload(manifest):
    body = {k: v for k, v in manifest.items() if k != "signatures"}
    return json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def check_shape(m):
    missing = [k for k in TOP_REQUIRED if k not in m]
    if missing:
        raise Fail(f"missing top-level fields: {missing}")
    unknown = [k for k in m if k not in TOP_REQUIRED and k not in TOP_OPTIONAL]
    if unknown:
        raise Fail(f"unknown top-level fields: {unknown}")
    if m["schema"] != SCHEMA:
        raise Fail(f"schema is {m['schema']!r}, not {SCHEMA}")
    if m["lifecycle"] not in LIFECYCLES:
        raise Fail(f"lifecycle {m['lifecycle']!r} is not one of {LIFECYCLES}")
    if m["lifecycle"] == "ABORTED":
        raise Fail("this release is ABORTED: do not install it")
    if not HEX128.match(m["genesis"]):
        raise Fail("genesis is a 64-byte hash in hex")
    c = m["consensus"]
    for key in ("consensus_params_fingerprint", "consensus_schedule_id"):
        if not HEX64.match(c.get(key, "")):
            raise Fail(f"consensus.{key} is a 32-byte digest in hex")
    if not isinstance(c.get("fence_schedule"), list):
        raise Fail("consensus.fence_schedule is a list of heights")
    switch = c.get("switch_daa")
    if not isinstance(switch, int) or switch <= 0:
        raise Fail("consensus.switch_daa is the height this release switches at")
    if str(switch) not in [str(x) for x in c["fence_schedule"]]:
        raise Fail(f"the switch height {switch} is not on the release's fence schedule — the binary would not switch there")
    readiness = m.get("readiness")
    if readiness is not None and readiness.get("consensus_rule", False):
        raise Fail("readiness is operational only; a manifest may not declare it a consensus rule (RFC-0019 §9)")
    for build in m["builds"]:
        for a in build.get("artifacts", []):
            if not HEX64.match(a.get("sha256", "")) or not isinstance(a.get("size"), int) or not a.get("name"):
                raise Fail(f"{build.get('platform')}: an artifact row needs name, sha256 and size")


def verify_signatures(m, allowed_signers, min_signers, ssh_keygen):
    sigs = m.get("signatures") or []
    if not allowed_signers:
        raise Fail("no --allowed-signers: an unverified manifest is never installable")
    if not shutil.which(ssh_keygen):
        raise Fail(f"{ssh_keygen} not found: cannot verify signatures (install OpenSSH ≥ 8.1)")
    payload = canonical_payload(m)
    good = set()
    with tempfile.TemporaryDirectory() as tmp:
        for i, s in enumerate(sigs):
            signer, armored = s.get("signer"), s.get("sshsig")
            if s.get("namespace", SIG_NAMESPACE) != SIG_NAMESPACE or not signer or not armored:
                continue
            path = os.path.join(tmp, f"sig{i}")
            with open(path, "w") as f:
                f.write(armored)
            done = subprocess.run(
                [ssh_keygen, "-Y", "verify", "-f", allowed_signers, "-I", signer, "-n", SIG_NAMESPACE, "-s", path],
                input=payload, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30,
            )
            if done.returncode == 0:
                good.add(signer)
    if len(good) < min_signers:
        raise Fail(f"{len(good)} valid signer(s) of {min_signers} required ({sorted(good)})")
    return sorted(good)


def verify_artifacts(m, folder, min_builders):
    for build in m["builds"]:
        for a in build.get("artifacts", []):
            builders = {r.get("builder") for r in a.get("reproduced_by", []) if r.get("sha256") == a["sha256"] and r.get("builder")}
            if len(builders) < min_builders:
                raise Fail(f"{build.get('platform')}/{a['name']}: {len(builders)} independent reproduction(s), {min_builders} required")
            if folder:
                path = os.path.join(folder, a["name"])
                if not os.path.isfile(path):
                    raise Fail(f"{a['name']}: not in {folder}")
                if os.path.getsize(path) != a["size"] or sha256_file(path) != a["sha256"]:
                    raise Fail(f"{a['name']}: the file's sha256/size do not match the manifest")


def verify_release_info(m, info):
    c = m["consensus"]
    if info.get("network") != m["network"]:
        raise Fail(f"RELEASE-INFO network {info.get('network')} ≠ manifest {m['network']}")
    if not info.get("identity_verified_on_every_platform"):
        raise Fail("RELEASE-INFO does not record the identity verified on every platform")
    for key in ("consensus_params_fingerprint", "consensus_schedule_id"):
        if info.get(key) != c[key]:
            raise Fail(f"{key}: the binaries printed {info.get(key)}, the manifest says {c[key]}")
    if [str(x) for x in info.get("fence_schedule") or []] != [str(x) for x in c["fence_schedule"]]:
        raise Fail("the fence schedule the binaries printed differs from the manifest's")


def verify_node_status(m, status):
    """An operator's AFTER-install check against the node's own getPalwNodeStatus / getPalwModelRegistry JSON."""
    c = m["consensus"]
    if status.get("consensusParamsId") and status["consensusParamsId"] != c["consensus_params_fingerprint"]:
        raise Fail("the installed node reports a different consensus params fingerprint")
    if status.get("consensusScheduleId") and status["consensusScheduleId"] != c["consensus_schedule_id"]:
        raise Fail("the installed node reports a different schedule id")
    cat = status.get("annualCatalog")
    if cat and cat.get("scheduled"):
        roots = {e["edition"]: e["root"] for e in m["annual_catalog"]["editions"]}
        for e in cat.get("editions", []):
            if roots.get(e.get("edition")) != e.get("root"):
                raise Fail(f"the installed node carries a different root for edition {e.get('edition')}")


def verify(m, args):
    check_shape(m)
    cat = m["annual_catalog"]
    validate_catalog_chain(cat.get("editions"), cat.get("fence_daa"), args.kernel_version_max)
    report = {"catalog_root_in_binary": cat["editions"][-1]["root"], "editions": len(cat["editions"])}
    if args.previous:
        prev = load(args.previous)
        if prev["network"] != m["network"] or prev["genesis"] != m["genesis"]:
            raise Fail("the previous manifest is for another network or genesis")
        old = prev["annual_catalog"]["editions"]
        new = cat["editions"]
        if len(old) > len(new):
            raise Fail("this release drops an annual catalog edition the previous release carried")
        for a, b in zip(old, new):
            strip = lambda e: {k: v for k, v in e.items() if k != "lifecycle"}  # noqa: E731 — LOCKED_IN → ACTIVE is not a rewrite
            if strip(a) != strip(b):
                raise Fail(f"this release rewrites annual catalog edition {a['edition']}")
    verify_artifacts(m, args.dir, args.min_builders)
    if args.release_info:
        verify_release_info(m, load(args.release_info))
    if args.node_status:
        verify_node_status(m, load(args.node_status))
    report["signers"] = verify_signatures(m, args.allowed_signers, args.min_signers, args.ssh_keygen)
    return report


def load(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError) as error:
        print(f"cannot read {path}: {error}", file=sys.stderr)
        sys.exit(2)


def write(args):
    info = load(args.release_info)
    catalog = load(args.catalog)
    editions = sort_and_root(catalog["editions"])
    for e in editions:
        if args.lifecycle in SHIPPABLE and e.get("lifecycle") not in SHIPPABLE:
            e["lifecycle"] = "LOCKED_IN"
    validate_catalog_chain(editions, catalog.get("fence_daa"), args.kernel_version_max)
    artifacts = []
    if args.artifacts_dir:
        for name in sorted(os.listdir(args.artifacts_dir)):
            path = os.path.join(args.artifacts_dir, name)
            if os.path.isfile(path):
                artifacts.append({"name": name, "sha256": sha256_file(path), "size": os.path.getsize(path), "reproduced_by": []})
    for report_path in args.builder_report or []:
        report = load(report_path)
        for a in artifacts:
            digest = report.get("artifacts", {}).get(a["name"])
            if digest:
                a["reproduced_by"].append({"builder": report["builder"], "sha256": digest})
    manifest = {
        "schema": SCHEMA,
        "release": info.get("release_tag"),
        "network": info["network"],
        "git_commit": info.get("git_commit"),
        "rust_toolchain": info.get("rust_toolchain"),
        "genesis": info.get("genesis_declared"),
        "consensus": {
            "consensus_params_fingerprint": info["consensus_params_fingerprint"],
            "consensus_schedule_id": info["consensus_schedule_id"],
            "fence_schedule": info["fence_schedule"],
            "switch_daa": args.switch_daa,
        },
        "annual_catalog": {"fence_daa": catalog.get("fence_daa"), "editions": editions},
        "lifecycle": args.lifecycle,
        "builds": [{"platform": args.platform or "any", "artifacts": artifacts}],
        "readiness": {"operational_target_permille": args.readiness_target, "consensus_rule": False,
                      "note": "measured by misaka-release-readiness.py; never a consensus rule, never a vote"},
        "signatures": [],
    }
    check_shape(manifest)
    with open(args.out, "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
        f.write("\n")
    print(f"wrote UNSIGNED {args.out}: catalog root {editions[-1]['root'][:16]}…, switch DAA {args.switch_daa}")


# ---- self-test -------------------------------------------------------------------------------------------------------------

def self_test():
    # The cross-implementation vectors (palw_annual_catalog_v1/tests.rs R0 / R1 / EMPTY).
    e0 = {"edition": 0, "activation_daa": 0, "previous_catalog_root": ZERO64,
          "models": [{"class_id": "01" * 64, "artifact_root": "02" * 64, "kernel_version": 1, "verification_plan_id": "03" * 64}]}
    r0 = catalog_root(e0)
    assert r0.startswith("3da5390791b75561"), r0
    e1 = {"edition": 1, "activation_daa": 1000, "previous_catalog_root": r0,
          "models": [{"class_id": "04" * 64, "artifact_root": "05" * 64, "kernel_version": 0, "verification_plan_id": ZERO64}]}
    assert catalog_root(e1).startswith("6cb437ad38163cdb"), catalog_root(e1)
    empty = {"edition": 0, "activation_daa": 0, "previous_catalog_root": ZERO64, "models": []}
    assert catalog_root(empty).startswith("c94db3542b1e0be5"), catalog_root(empty)
    # A valid chain, then each refusal.
    chain = sort_and_root([dict(e0, lifecycle="ACTIVE"), dict(e1, lifecycle="LOCKED_IN")])
    validate_catalog_chain(chain, 0, 1)

    def refused(mutate, why):
        bad = json.loads(json.dumps(chain))
        mutate(bad)
        try:
            validate_catalog_chain(bad, 0, 1)
        except Fail:
            return
        raise AssertionError("not refused: " + why)

    refused(lambda c: c[1].update(previous_catalog_root="ff" * 64), "broken chain")
    refused(lambda c: c[1]["models"][0].update(class_id="01" * 64), "re-listed class (add-only)")
    refused(lambda c: c[1].update(activation_daa=0), "non-increasing height")
    refused(lambda c: c[1].update(models=[]), "empty annual edition")
    refused(lambda c: c[1].update(lifecycle="TESTED"), "a TESTED edition in a binary")
    refused(lambda c: c[0]["models"][0].update(kernel_version=2), "kernel version above the binary's")
    refused(lambda c: c[0].update(root="00" * 64), "a wrong root")
    # The manifest shape: the switch must be on the schedule; ABORTED and a readiness 'consensus rule' are refused.
    m = {"schema": SCHEMA, "release": "t", "network": "testnet-12", "git_commit": "c" * 40, "genesis": "a" * 128,
         "consensus": {"consensus_params_fingerprint": "b" * 64, "consensus_schedule_id": "d" * 64,
                       "fence_schedule": ["14000"], "switch_daa": 14000},
         "annual_catalog": {"fence_daa": 14000, "editions": sort_and_root([dict(empty, lifecycle="ACTIVE")])},
         "lifecycle": "LOCKED_IN", "builds": [{"platform": "x", "artifacts": [{"name": "kaspad", "sha256": "e" * 64, "size": 1,
                                                                             "reproduced_by": [{"builder": "a", "sha256": "e" * 64},
                                                                                               {"builder": "b", "sha256": "e" * 64}]}]}]}
    check_shape(m)
    verify_artifacts(m, None, 2)
    for mutate, why in [
        (lambda x: x["consensus"].update(switch_daa=12000), "switch off the schedule"),
        (lambda x: x.update(lifecycle="ABORTED"), "aborted"),
        (lambda x: x.update(readiness={"consensus_rule": True}), "readiness as a consensus rule"),
        (lambda x: x.update(vote={"yes": 1}), "an unknown field (no vote)"),
    ]:
        bad = json.loads(json.dumps(m))
        mutate(bad)
        try:
            check_shape(bad)
        except Fail:
            continue
        raise AssertionError("not refused: " + why)
    try:
        verify_artifacts(m, None, 3)
        raise AssertionError("not refused: too few reproductions")
    except Fail:
        pass
    try:
        verify_signatures(m, None, 1, "ssh-keygen")
        raise AssertionError("not refused: unsigned")
    except Fail:
        pass
    # The canonical payload ignores signatures and key order.
    a = dict(m, signatures=[{"signer": "x"}])
    assert canonical_payload(a) == canonical_payload(json.loads(json.dumps(m, sort_keys=False)))
    print("self-test: ok (catalog vectors, chain refusals, manifest shape, reproductions, unsigned refusal, payload)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="mode", required=True)
    w = sub.add_parser("write")
    w.add_argument("--release-info", required=True)
    w.add_argument("--catalog", required=True, help='{"fence_daa": N, "editions": [{"activation_daa", "models": [...]}, ...]}')
    w.add_argument("--switch-daa", type=int, required=True)
    w.add_argument("--lifecycle", choices=LIFECYCLES, required=True)
    w.add_argument("--artifacts-dir")
    w.add_argument("--platform")
    w.add_argument("--builder-report", action="append", help='{"builder": id, "artifacts": {name: sha256}}')
    w.add_argument("--readiness-target", type=int, default=950, help="operational target, permille (never a consensus rule)")
    w.add_argument("--kernel-version-max", type=int, default=1)
    w.add_argument("--out", required=True)
    p = sub.add_parser("payload")
    p.add_argument("manifest")
    p.add_argument("--out", required=True)
    a = sub.add_parser("attach")
    a.add_argument("manifest")
    a.add_argument("--signer", required=True)
    a.add_argument("--sig", required=True)
    v = sub.add_parser("verify")
    v.add_argument("manifest")
    v.add_argument("--allowed-signers")
    v.add_argument("--min-signers", type=int, default=2)
    v.add_argument("--min-builders", type=int, default=2)
    v.add_argument("--dir")
    v.add_argument("--release-info")
    v.add_argument("--previous")
    v.add_argument("--node-status")
    v.add_argument("--kernel-version-max", type=int, default=1)
    v.add_argument("--ssh-keygen", default="ssh-keygen")
    c = sub.add_parser("catalog-root")
    c.add_argument("catalog")
    sub.add_parser("self-test")
    args = parser.parse_args()
    try:
        if args.mode == "self-test":
            self_test()
        elif args.mode == "write":
            write(args)
        elif args.mode == "payload":
            with open(args.out, "wb") as f:
                f.write(canonical_payload(load(args.manifest)))
            print(f"wrote {args.out}: sign it with  ssh-keygen -Y sign -f <key> -n {SIG_NAMESPACE} {args.out}")
        elif args.mode == "attach":
            m = load(args.manifest)
            with open(args.sig) as f:
                m.setdefault("signatures", []).append({"signer": args.signer, "namespace": SIG_NAMESPACE, "sshsig": f.read()})
            with open(args.manifest, "w") as f:
                json.dump(m, f, indent=2, sort_keys=True)
                f.write("\n")
            print(f"attached {args.signer}'s signature ({len(m['signatures'])} total)")
        elif args.mode == "verify":
            report = verify(load(args.manifest), args)
            print(json.dumps({"verified": True, **report, "installed": False,
                              "note": "nothing was installed or executed; the operator chooses when to install"}, indent=2))
        elif args.mode == "catalog-root":
            cat = load(args.catalog)
            for e in sort_and_root(cat["editions"]):
                print(e["edition"], e["activation_daa"], e["root"])
    except Fail as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
