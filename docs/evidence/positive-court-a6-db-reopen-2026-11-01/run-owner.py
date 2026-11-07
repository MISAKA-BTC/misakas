#!/usr/bin/env python3
"""Plan by default. Execute only after the separately coordinated heavy slot.

Preserves old owner binaries with an actual gzip -> file -> SHA256 round trip.
Never raises test stack limits, compiles C++, changes global Rust flags, retries
failed tests, restores over a new artifact, or deletes unrelated build products.
"""
import argparse
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

SOURCE = "355408e5a3426926d863193b76ffaa810704bef0"
WORKSPACE = Path("/Users/wata/.codex/worktrees/rfc42-a6-recovery/MISAKA-testnet")
TARGET = Path("/Users/wata/Downloads/MISAKA-testnet/target")
EXTERNAL = Path("/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out")
ARCHIVES = {
    "librocksdb.a": (23048496, "6eae541967b2cb5345ace55eadb34b2c490e91acfd8a4e2da5822a60f91af3a2"),
    "libsnappy.a": (46784, "f45a4731a8dc330e5723b70dabb4be96167a71044fcdeb0ce4d78f837f9538e5"),
}
NATIVE_INPUTS = dict(ARCHIVES, **{
    "bindings.rs": (208430, "cbe5eae46ca4209c1bfb020945fd86fb9a401015979112ae70933a2ac5621e62"),
})
LINKS_CONFIG = Path(__file__).with_name("native-links-r2.toml").resolve()
OLD = (
    "kaspa_consensus-cdd71196065befc6",
    "kaspa_consensus_core-b2df9d40a9b2c4d1",
    "positive_court_a4_integration-a21d9f9a1e95c479",
)
GIB = 1 << 30
CONFIG = [
    "--config", "profile.dev.package.kaspa-consensus-core.opt-level=0",
    "--config", "profile.dev.package.kaspa-consensus-core.debug=0",
    "--config", "profile.dev.package.kaspa-consensus.opt-level=0",
    "--config", "profile.dev.package.kaspa-consensus.debug=0",
]


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def unchanged_source():
    subprocess.run(
        ["git", "diff", "--exit-code", SOURCE, "--", ".",
         ":(exclude)docs/**", ":(exclude)handoff/**"],
        cwd=WORKSPACE, check=True, stdout=subprocess.DEVNULL,
    )
    unknown = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=WORKSPACE, text=True,
    ).splitlines()
    if any(not p.startswith(("docs/", "handoff/")) for p in unknown):
        raise RuntimeError("untracked source outside documentation; refuse an unfrozen run")
    frozen = json.loads((Path(__file__).parent / "source-freeze.json").read_text())
    for name, expected in frozen["files"].items():
        if digest(WORKSPACE / name) != expected:
            raise RuntimeError(f"source freeze changed: {name}")


def verify_native_inputs():
    for name, (size, expected) in NATIVE_INPUTS.items():
        path = EXTERNAL / name
        if path.is_symlink() or path.stat().st_size != size or digest(path) != expected:
            raise RuntimeError(f"external input identity mismatch: {path}")


def plan(preserved_from=None):
    unchanged_source()
    verify_native_inputs()
    archives = {}
    for name, (size, expected) in NATIVE_INPUTS.items():
        path = EXTERNAL / name
        if path.is_symlink() or path.stat().st_size != size or digest(path) != expected:
            raise RuntimeError(f"external archive identity mismatch: {path}")
        archives[name] = {"path": str(path), "bytes": size, "sha256": expected}
    if preserved_from is None:
        old = []
        for name in OLD:
            path = TARGET / "debug/deps" / name
            if path.is_symlink() or not path.is_file():
                raise RuntimeError(f"expected owner executable is missing: {path}")
            old.append({"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)})
    else:
        # Reuse only the fully verified original backups, not any failed test
        # result, in a NEW run directory/configuration. Never rewrite run 1.
        anchor = json.loads((Path(__file__).parent / "owner-run-plan-r2.json").read_text())
        old = anchor["old_binaries"]
        verified_saved_backups(preserved_from, old)
    return {
        "source": SOURCE, "runner_sha256": digest(__file__),
        "workspace": str(WORKSPACE), "target": str(TARGET),
        "archives": archives, "old_binaries": old,
        "preserved_from": str(preserved_from.resolve()) if preserved_from else None,
        "links_override": {"path": str(LINKS_CONFIG), "sha256": digest(LINKS_CONFIG)},
        "free_bytes_now": shutil.disk_usage(TARGET).free,
        "disk_plan": {
            "old_binary_bytes": sum(x["bytes"] for x in old),
            "gzip_roundtrip_peak_extra_bytes": max(x["bytes"] for x in old) * 2,
            "estimated_new_rust_and_link_peak_gib": [1.5, 3.5],
            "estimate_is_not_a_capacity_guarantee": True,
            "default_min_free_before_build_gib": 4,
            "stop_free_space_floor_mib": 512,
        },
        "tests": [
            {"package": "kaspa-consensus", "target": "kaspa_consensus",
             "select": ["--lib"], "filter": "positive_journal", "expected": 14},
            {"package": "misaka-palw-tir-exec", "target": "positive_court_a4_integration",
             "select": ["--features", "node", "--test", "positive_court_a4_integration"],
             "filter": "a6_journal_replay", "expected": 17},
        ],
        "executed": False,
    }


def no_active_old_binary(paths):
    names = {Path(p).name for p in paths}
    for line in subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True).splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and Path(parts[1]).name in names:
            raise RuntimeError(f"an old owner binary is running: {line.strip()}")


def preserve_old(run_dir, expected):
    backups = run_dir / "old-binaries"
    backups.mkdir()
    no_active_old_binary([v["path"] for v in expected])
    records = []
    for entry in expected:
        src = Path(entry["path"])
        if src.is_symlink() or src.stat().st_size != entry["bytes"] or digest(src) != entry["sha256"]:
            raise RuntimeError(f"owner executable changed since plan: {src}")
        # No build has started. Verify a real restored file before unlinking
        # ONLY this exact, checksum-checked old owner executable.
        archive = backups / (src.name + ".gz")
        with src.open("rb") as original, gzip.open(archive, "wb", compresslevel=1) as compressed:
            shutil.copyfileobj(original, compressed, 1 << 20)
        with tempfile.TemporaryDirectory(prefix="restore-check-", dir=backups) as td:
            restored = Path(td) / src.name
            with gzip.open(archive, "rb") as compressed, restored.open("wb") as output:
                shutil.copyfileobj(compressed, output, 1 << 20)
            if restored.stat().st_size != entry["bytes"] or digest(restored) != entry["sha256"]:
                raise RuntimeError(f"gzip restore verification failed: {src}")
        record = dict(entry, gzip=str(archive), gzip_bytes=archive.stat().st_size,
                      gzip_sha256=digest(archive), restored_file_sha256_verified=True)
        records.append(record)
        # Save recovery instructions before removing any old cache executable.
        write_json(backups / "manifest.json", records)
        no_active_old_binary([src])
        if digest(src) != entry["sha256"]:
            raise RuntimeError(f"old executable changed during backup: {src}")
        src.unlink()
    return records


def verified_saved_backups(run_dir, planned):
    entries = json.loads((run_dir / "old-binaries/manifest.json").read_text())
    expected = {(x["path"], x["bytes"], x["sha256"]) for x in planned}
    actual = {(x["path"], x["bytes"], x["sha256"]) for x in entries}
    if actual != expected or len(entries) != len(planned):
        raise RuntimeError("incomplete or changed old-binary preservation; manual recovery required")
    for entry in entries:
        backup = Path(entry["gzip"])
        if (not entry["restored_file_sha256_verified"]
                or backup.stat().st_size != entry["gzip_bytes"]
                or digest(backup) != entry["gzip_sha256"]):
            raise RuntimeError("a preserved executable archive changed")
        restored_hash = hashlib.sha256()
        restored_bytes = 0
        with gzip.open(backup, "rb") as compressed:
            for chunk in iter(lambda: compressed.read(1 << 20), b""):
                restored_bytes += len(chunk)
                restored_hash.update(chunk)
        if restored_bytes != entry["bytes"] or restored_hash.hexdigest() != entry["sha256"]:
            raise RuntimeError("preserved gzip CRC/uncompressed identity check failed")
        entry["reuse_gzip_crc_and_uncompressed_sha256_verified"] = True
    return entries


def group_exists(pgid):
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    return True


def stop_owned_group(proc, grace=15):
    # Cargo can exit before its rustc/linker child. Reap/check the leader and
    # the process group separately; a leader exit is not group completion.
    for sig, timeout in ((signal.SIGTERM, grace), (signal.SIGKILL, 5)):
        proc.poll()
        if not group_exists(proc.pid):
            break
        try:
            os.killpg(proc.pid, sig)
        except ProcessLookupError:
            break
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            proc.poll()
            if not group_exists(proc.pid):
                break
            time.sleep(0.1)
    proc.wait(timeout=5)
    if group_exists(proc.pid):
        raise RuntimeError("own process group remains after SIGKILL; preserve logs")


def monitored(command, logfile, env, floor):
    with logfile.open("wb") as log:
        proc = subprocess.Popen(command, cwd=WORKSPACE, env=env,
                                stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while proc.poll() is None:
                if shutil.disk_usage(TARGET).free < floor:
                    raise RuntimeError("free-space floor reached; stop own process group")
                time.sleep(1)
            return proc.returncode
        finally:
            stop_owned_group(proc)


def execute(plan_file, run_dir, min_free_gib, preserve_only=False):
    if not math.isfinite(min_free_gib) or min_free_gib < 4:
        raise RuntimeError("keep the reviewed minimum build headroom at four GiB or higher")
    data = json.loads(plan_file.read_text())
    if data["source"] != SOURCE or data["runner_sha256"] != digest(__file__):
        raise RuntimeError("runner/source plan mismatch")
    unchanged_source()
    # Hidden global flags would defeat cache/profile correspondence. Do not
    # print unrelated environment values or silently change the build graph.
    for key, value in os.environ.items():
        if value and (
            key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTFLAGS", "RUSTC",
                    "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTC_WRAPPER",
                    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET",
                    "CXX", "HOST_CXX", "TARGET_CXX", "HOST_CXXFLAGS", "TARGET_CXXFLAGS",
                    "CXXFLAGS")
            or key.startswith(("CXX_", "CXXFLAGS_", "CARGO_PROFILE_"))
            or (key.startswith("CARGO_TARGET_") and key.endswith(("_RUSTFLAGS", "_LINKER")))
        ):
            raise RuntimeError(f"{key} is set; a separately reviewed plan is required")
    verify_native_inputs()
    if (data["links_override"]["path"] != str(LINKS_CONFIG)
            or data["links_override"]["sha256"] != digest(LINKS_CONFIG)):
        raise RuntimeError("native links override changed")
    resumed_backups = None
    if run_dir.exists():
        # A capacity stop BEFORE any compiler/test attempt can be resumed
        # explicitly after headroom is provided, without restoring/deleting
        # cache files again. Never retry a compiler/test failure or overwrite
        # an earlier build/test log.
        previous = json.loads((run_dir / "status.json").read_text())
        if previous.get("executed_suites"):
            raise RuntimeError("a compiler/test was already attempted; no retry in this run directory")
        prior_plan = json.loads((run_dir / "plan.json").read_text())
        if prior_plan != data:
            raise RuntimeError("pre-build resume requires the identical frozen plan")
        resumed_backups = verified_saved_backups(run_dir, data["old_binaries"])
        index = 1
        while (run_dir / f"pre-build-stop-{index}.json").exists():
            index += 1
        write_json(run_dir / f"pre-build-stop-{index}.json", previous)
    else:
        run_dir.mkdir(parents=True, exist_ok=False)
        write_json(run_dir / "plan.json", data)
    status = {"source": SOURCE, "executed_suites": [], "status": "preparing"}
    try:
        if resumed_backups is None:
            if data.get("preserved_from"):
                status["old_binaries"] = verified_saved_backups(Path(data["preserved_from"]), data["old_binaries"])
                status["preserved_from"] = data["preserved_from"]
                # A new manifest references the original gzip paths. No old
                # cache path is deleted again and no compressed bytes copied.
                (run_dir / "old-binaries").mkdir()
                write_json(run_dir / "old-binaries/manifest.json", status["old_binaries"])
            else:
                if shutil.disk_usage(TARGET).free < data["disk_plan"]["gzip_roundtrip_peak_extra_bytes"] + (256 << 20):
                    raise RuntimeError("insufficient space even for verified old-binary preservation")
                status["old_binaries"] = preserve_old(run_dir, data["old_binaries"])
        else:
            status["old_binaries"] = resumed_backups
        if preserve_only:
            status["status"] = "PRESERVED only; no compiler or tests started"
            return
        if shutil.disk_usage(TARGET).free < min_free_gib * GIB:
            raise RuntimeError("insufficient post-backup build headroom; no compiler started")
        cxx = run_dir / "forbid-cxx.py"
        cxx.write_text(
            "#!" + sys.executable + "\nimport json,os,sys\n"
            "with open(os.environ['MISAKA_A6_CXX_ATTEMPT_LOG'],'a') as f:\n"
            " f.write(json.dumps(sys.argv)+'\\n')\n"
            "sys.stderr.write('C++ compilation/probing forbidden in this owner run\\n')\n"
            "sys.exit(95)\n"
        )
        cxx.chmod(0o755)
        env = os.environ.copy()
        env.pop("RUST_MIN_STACK", None)
        env.update({
            "CARGO_TARGET_DIR": str(TARGET), "CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": "2",
            "ROCKSDB_LIB_DIR": str(EXTERNAL), "ROCKSDB_STATIC": "1", "ROCKSDB_COMPILE": "0",
            "SNAPPY_LIB_DIR": str(EXTERNAL), "SNAPPY_STATIC": "1", "SNAPPY_COMPILE": "0",
            "CXX": str(cxx), "MISAKA_A6_CXX_ATTEMPT_LOG": str(run_dir / "forbidden-cxx.jsonl"),
        })
        status["build_environment"] = {k: env[k] for k in (
            "CARGO_TARGET_DIR", "CARGO_INCREMENTAL", "CARGO_BUILD_JOBS",
            "ROCKSDB_LIB_DIR", "ROCKSDB_STATIC", "ROCKSDB_COMPILE",
            "SNAPPY_LIB_DIR", "SNAPPY_STATIC", "SNAPPY_COMPILE", "CXX",
        )}
        status["RUST_MIN_STACK"] = "unset; default test stack only"
        for number, suite in enumerate(data["tests"], 1):
            unchanged_source()
            command = ["cargo", "rustc", "--offline", "--locked", "-p", suite["package"],
                       *suite["select"], "--profile", "test",
                       "--message-format=json-render-diagnostics",
                       "--config", str(LINKS_CONFIG), *CONFIG, "--", "-l", "c++"]
            prefix = f"{number}-{suite['package']}"
            buildlog = run_dir / (prefix + "-build.log")
            result = dict(suite, build_command=command, build_log=str(buildlog))
            status["executed_suites"].append(result)
            write_json(run_dir / "status.json", status)
            result["build_exit"] = monitored(command, buildlog, env, 512 << 20)
            if result["build_exit"]:
                raise RuntimeError("build failed; preserve raw log and stop without retry")
            artifacts = []
            for line in buildlog.read_text(errors="replace").splitlines():
                try:
                    item = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if (item.get("reason") == "compiler-artifact"
                        and item.get("target", {}).get("name") == suite["target"]
                        and item.get("profile", {}).get("test")
                        and item.get("executable")):
                    artifacts.append(item["executable"])
            artifacts = list(dict.fromkeys(artifacts))
            if len(artifacts) != 1:
                raise RuntimeError(f"expected one explicit test executable, got {artifacts}")
            binary = artifacts[0]
            result.update(binary=binary, binary_sha256=digest(binary))
            testlog = run_dir / (prefix + "-tests.log")
            result["test_log"] = str(testlog)
            result["test_command"] = [binary, suite["filter"], "--test-threads=2"]
            result["test_exit"] = monitored(result["test_command"], testlog, env, 512 << 20)
            if result["test_exit"]:
                raise RuntimeError("test failed/stopped; preserve raw log, no stack increase or retry")
            expected = f"test result: ok. {suite['expected']} passed; 0 failed; 0 ignored;"
            if expected not in testlog.read_text(errors="replace"):
                raise RuntimeError("unexpected test count; never count a zero-test filter as success")
        if (run_dir / "forbidden-cxx.jsonl").exists():
            raise RuntimeError("unexpected C++ invocation was blocked; do not claim C++-free success")
        unchanged_source()
        verify_native_inputs()
        if digest(LINKS_CONFIG) != data["links_override"]["sha256"]:
            raise RuntimeError("native links override changed during the run")
        status["status"] = "PASS owner scope only; not Root/native authority/IBD qualification"
    except BaseException as exc:
        status["status"] = "STOPPED; no automatic retry"
        status["error"] = str(exc)
        raise
    finally:
        status["external_inputs_after"] = {
            name: digest(EXTERNAL / name) if (EXTERNAL / name).is_file() else None
            for name in NATIVE_INPUTS
        }
        status["links_override_after_sha256"] = digest(LINKS_CONFIG) if LINKS_CONFIG.is_file() else None
        status["free_bytes_final"] = shutil.disk_usage(TARGET).free
        write_json(run_dir / "status.json", status)


def restore(manifest, destination):
    """Recover files to a NEW directory, never overwrite target cache outputs."""
    entries = json.loads(manifest.read_text())
    destination.mkdir(parents=True, exist_ok=False)
    for entry in entries:
        src = Path(entry["gzip"])
        if digest(src) != entry["gzip_sha256"]:
            raise RuntimeError("backup checksum mismatch")
        dst = destination / Path(entry["path"]).name
        with gzip.open(src, "rb") as compressed, dst.open("xb") as output:
            shutil.copyfileobj(compressed, output, 1 << 20)
        if digest(dst) != entry["sha256"] or dst.stat().st_size != entry["bytes"]:
            raise RuntimeError("restored executable checksum mismatch")
        dst.chmod(0o755)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--execute", action="store_true")
    mode.add_argument("--preserve-only", action="store_true")
    mode.add_argument("--restore-manifest", type=Path)
    parser.add_argument("--plan-file", type=Path, default=Path("/tmp/misaka-a6-db-reopen-owner-links-r2-plan.json"))
    parser.add_argument("--run-dir", type=Path, default=Path("/tmp/misaka-a6-db-reopen-owner-links-r2"))
    parser.add_argument("--restore-dir", type=Path)
    parser.add_argument("--preserved-from", type=Path,
                        help="plan a new run using original verified backups, not previous runtime results")
    parser.add_argument("--min-free-gib", type=float, default=4)
    args = parser.parse_args()
    if args.restore_manifest:
        if args.restore_dir is None:
            parser.error("--restore-dir is required and must not exist")
        restore(args.restore_manifest, args.restore_dir)
    elif args.execute or args.preserve_only:
        execute(args.plan_file, args.run_dir, args.min_free_gib, args.preserve_only)
    else:
        result = plan(args.preserved_from)
        write_json(args.plan_file, result)
        print(f"Read-only plan saved: {args.plan_file}; no compression, unlink, compiler or test executed")
