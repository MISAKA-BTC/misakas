#!/usr/bin/env python3
"""Read-only rehash of retained sources; write only a new local audit report."""
import datetime
import hashlib
import json
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = "4753ee4beae5e245095954c6ef6e9b96fa05f361"


def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def identity(path):
    s = path.stat()
    return (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def main():
    audit_head = git("rev-parse", "HEAD").decode().strip()
    source = ROOT / "docs/evidence/models/trained-llm-cohort-2026-10-31"
    manifest = source / "evidence-sha256.json"
    original_manifest_sha = "905da715403dbb361920ff69e4a8edd8f1f55e0542422e7cd117dc51677eb951"
    assert sha(manifest) == original_manifest_sha
    raw = []
    for expected in json.loads(manifest.read_text())["files"]:
        p = source / expected["path"]
        raw.append({**expected, "matches": p.stat().st_size == expected["bytes"] and sha(p) == expected["sha256"]})
    declared = {}
    agents = []
    expected_missing = []
    for name in ["smollm2-360m.json", "smollm2-135m.json", "pythia-70m.json"]:
        p = HERE / name
        d = json.loads(p.read_text())
        agents.append({"path": name, "bytes": p.stat().st_size, "sha256": sha(p)})
        for record in d["files"]:
            path = record["path"]
            if not record.get("exists", True):
                expected_missing.append(path)
                continue
            value = {"bytes": record["bytes"], "sha256": record["sha256"]}
            assert path not in declared or declared[path] == value, path
            declared[path] = value
    checked = []
    for name, expected in sorted(declared.items()):
        p = pathlib.Path(name)
        before, real_before = identity(p), str(p.resolve())
        actual = sha(p)
        after = identity(p)
        checked.append({"path": name, **expected, "matches": actual == expected["sha256"] and before[2] == expected["bytes"], "stable_during_parent_read": before == after and real_before == str(p.resolve())})
    expected_missing = sorted(set(expected_missing))
    assert all(not pathlib.Path(p).exists() for p in expected_missing)
    old_commit = "8b5f5d5e317ecd1f8aff3060c5de181309cb0b63"
    old_path = "testing/positive-court-a7/evidence/a0-79c3-fixture-fixed-2026-11-01/run.json"
    old_sha = hashlib.sha256(git("show", old_commit + ":" + old_path)).hexdigest()
    assert old_sha == "4c0e05c233b337e4c0f7cb87bdeef5b76fd7e0f7ae20228c04fe3e399789672c"
    assert git("rev-parse", "codex/a7-79c3-fixture-fixed").decode().strip() == old_commit
    api_paths = [
        "misaka/palw/tir/exec/src/node/run_wide_q24_v2.rs",
        "misaka/palw/tir/exec/src/node/wide_capture_v2.rs",
        "misaka/palw/tir/exec/src/node/wide_capture_v2/budget.rs",
        "misaka/palw/tir/exec/src/node/run.rs",
        "misaka/palw/tir/exec/src/node/backend.rs",
        "consensus/core/src/palw_tir_profile_resolver_v2.rs",
        "consensus/core/src/palw_tir_row_profile_v2.rs",
        "consensus/core/src/palw_tir_step_v1/wide_v2.rs",
        "consensus/core/src/palw_logits_q24_v2.rs",
        "misaka/palw/tir/artifact/src/metadata_v16.rs",
        "misaka/palw/sdk/src/tir_layout.rs",
        "docs/design/palw/positive-court/WIDE-Q24-INTEGRATION.md",
    ]
    apis = [{"path": p, "bytes": (ROOT / p).stat().st_size, "sha256": sha(ROOT / p)} for p in api_paths]
    assert all(x["sha256"] == hashlib.sha256(git("show", BASE + ":" + x["path"])).hexdigest() for x in apis)
    ok = len(raw) == 73 and all(x["matches"] for x in raw) and all(x["matches"] and x["stable_during_parent_read"] for x in checked)
    report = {"schema": "misaka.a7.parent-inventory-audit.v1", "source_commit": BASE, "audit_head": audit_head, "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "scope": "Data identity only; no inference, new native V2 registration, court verdict, credit, build, conversion or download", "agent_outputs": agents, "retained_manifest_sha256": original_manifest_sha, "retained_raw_files": raw, "unique_existing_files_rehashed": len(checked), "existing_files": checked, "expected_missing_original_failed_narrow_outputs": expected_missing, "old_667_evidence": {"commit": old_commit, "branch": "codex/a7-79c3-fixture-fixed", "run_json_sha256": old_sha}, "proposal_source_api_files": apis, "passed": ok}
    (HERE / "parent-audit.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"passed": ok, "retained_raw": len(raw), "unique_existing_files": len(checked), "expected_missing": len(expected_missing)}))
    assert ok


if __name__ == "__main__":
    main()
