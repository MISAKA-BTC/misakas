#!/usr/bin/env python3
"""Run the all-required initial seven-model gate using the native cohort runner.

This is a release preparation tool, not consensus activation. It cannot turn a
catalogue entry or old runtime receipt into an Active class. The fresh runner
executes each pinned producer/checker and both independent native audit commands.
Run only in the consolidated verification phase, after committing all inputs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import acceptance_cohort as ac
import checkpoint_cohort_runner as runner

SCHEMA = "misaka.rfc02.initial-model-launch.v1"
MODELS = {
    "dense-1p5b": ("Misakachain/Qwen2.5-1.5B-PALW-A16-runtime", "Qwen/Qwen2.5-1.5B-Instruct"),
    "dense-8b": ("meta-llama/Llama-3.1-8B-Instruct",) * 2,
    "dense-14b": ("Qwen/Qwen3-14B",) * 2,
    "dense-24b": ("mistralai/Mistral-Small-3.2-24B-Instruct-2506",) * 2,
    "dense-32b": ("Qwen/Qwen3-32B",) * 2,
    "dense-70b": ("meta-llama/Llama-3.3-70B-Instruct",) * 2,
    "moe-35b-a3b": ("Misakachain/Qwen3.6-35B-A3B-PALW-runtime",) * 2,
}
RIGHTS = ["block_production", "coinbase", "final_weight"]


def hex_string(value: object, digits: int) -> bool:
    return isinstance(value, str) and len(value) == digits and all(c in "0123456789abcdef" for c in value)


def validate_target(doc: dict) -> dict[str, dict]:
    if (doc.get("schema") != SCHEMA or doc.get("required_models") != 7
            or doc.get("required_success_share") != 1.0
            or doc.get("initial_enablement_policy") != "all-seven-pass-before-launch;missing-fails;no-substitution;no-forced-Active"
            or doc.get("population_coverage_claim") is not False
            or doc.get("existing_A6_A8_denominators_unchanged") is not True):
        raise ValueError("original all-seven launch scope required")
    models = doc.get("models", [])
    if len(models) != 7 or {m.get("id") for m in models} != set(MODELS):
        raise ValueError("seven original models required; 35B MoE cannot replace 32B dense")
    repos = doc.get("publisher_repositories", [])
    if len(repos) != 8 or len({r.get("repository") for r in repos}) != 8:
        raise ValueError("complete publisher revision inventories required")
    repos = {r["repository"]: r for r in repos}
    result = {}
    for model in models:
        key = model["id"]
        runtime, source = MODELS[key]
        if (model.get("runtime_repository") != runtime or model.get("repository") != source
                or model.get("required_initial_rights") != RIGHTS):
            raise ValueError("launch changes a selected model or drops initial rights")
        for repo, revision in ((runtime, model.get("runtime_revision")), (source, model.get("revision"))):
            if repo not in repos or not hex_string(revision, 40) or repos[repo].get("revision") != revision:
                raise ValueError("launch source/runtime differs from original publisher revision")
        if not isinstance(model.get("required_components"), list) or not model["required_components"]:
            raise ValueError("complete model components required")
        if key == "dense-24b" and not {"vision_encoder", "vision_projector", "decoder"} <= set(model["required_components"]):
            raise ValueError("Mistral vision cannot be replaced with decoder-only coverage")
        if key == "moe-35b-a3b" and not {"decoder", "router", "experts", "recurrent_state"} <= set(model["required_components"]):
            raise ValueError("MoE routing/experts/state cannot be omitted")
        if key != "moe-35b-a3b":
            names = {f["path"] for f in repos[source]["files"]}
            shards = {n for n in names if n.startswith("model-") and n.endswith(".safetensors")}
            expected = shards or {"model.safetensors"}
            if set(model.get("source_weight_files") or []) != expected or not expected <= names:
                raise ValueError("launch cannot omit an original safetensors shard")
        result[key] = model
    return result


def validate_inputs(target: dict, cohort: dict, plan: dict, cohort_sha: str) -> None:
    models = validate_target(target)
    runner.validate_plan(cohort, plan, cohort_sha)
    if (cohort.get("design") != "ENGINEERING_PANEL" or cohort.get("population") != "INITIAL_SEVEN"
            or cohort.get("n") != 7 or cohort.get("N") != 7
            or {m["id"] for m in cohort["members"]} != set(models)):
        raise ValueError("initial models are a separate exact engineering panel, not A6/A8")
    if not plan.get("receipt_window_command"):
        raise ValueError("the launch run requires two independently audited sustained receipt windows")
    for member in cohort["members"]:
        model = models[member["id"]]
        if (member.get("repository") != model["repository"] or member.get("revision") != model["revision"]
                or set(member.get("required_task_components", [])) != set(model["required_components"])
                or type(model.get("source_declared_context_max")) is not int
                or model["source_declared_context_max"] <= 0
                or member.get("source_declared_context_max") != model["source_declared_context_max"]):
            raise ValueError("original full task/context/components unresolved or changed")
        publisher = next(r for r in target["publisher_repositories"] if r["repository"] == model["repository"])
        inventory = {f["path"]: f for f in publisher["files"]}
        weights = model.get("source_weight_files")
        required = set(model.get("publisher_metadata_files", []))
        if not isinstance(weights, list) or not weights or len(weights) != len(set(weights)):
            raise ValueError("original weight format/complete shard set must be selected before measurement")
        required.update(weights)
        pins = member.get("source_files", [])
        by_path = {p["path"]: p for p in pins}
        if len(by_path) != len(pins) or not required <= set(by_path):
            raise ValueError("missing original weight shard, tokenizer or publisher metadata")
        source_dir = member.get("source_directory")
        if not isinstance(source_dir, str) or not Path(source_dir).is_absolute():
            raise ValueError("exact local original source directory required")
        for name, pin in by_path.items():
            original = inventory.get(name)
            if original is None or original.get("bytes") != pin.get("bytes"):
                raise ValueError("source file was not in the original publisher revision")
            if original.get("sha256"):
                if pin.get("sha256") != original["sha256"]:
                    raise ValueError("source bytes differ from pinned publisher LFS weights")
            else:
                path = runner.relative_file(Path(source_dir), name)
                if not path.is_file() or path.stat().st_size != original["bytes"] or original["bytes"] > 128 << 20:
                    raise ValueError("missing or oversized publisher metadata Git blob")
                sha = hashlib.sha1(f"blob {original['bytes']}\0".encode())
                with path.open("rb") as stream:
                    for part in iter(lambda: stream.read(1 << 20), b""):
                        sha.update(part)
                if sha.hexdigest() != original.get("git_blob"):
                    raise ValueError("publisher configuration/tokenizer Git blob differs")
        artifact = model.get("complete_artifact_pin")
        if (not isinstance(model.get("runtime_variant"), str) or not model["runtime_variant"]
                or not isinstance(artifact, dict) or not hex_string(artifact.get("sha256"), 64)
                or any(not hex_string(artifact.get(k), 128)
                       for k in ("inventory_root", "real_parameter_root", "program_root", "native_class", "native_source"))):
            raise ValueError("exact runtime variant and canonical artifact/native identity must be committed before measurement")
        # The generated canonical artifact is local; publisher LFS metadata alone
        # neither authenticates it nor establishes conversion equivalence.
        path = Path(artifact.get("path", ""))
        if (not path.is_absolute() or not path.is_file() or type(artifact.get("bytes")) is not int
                or path.stat().st_size != artifact["bytes"] or ac.digest(path) != artifact["sha256"]):
            raise ValueError("missing or changed original complete canonical artifact")


def launch_outcome(target: dict, records: list[dict], observations: list[dict], windows: bool) -> dict:
    models = validate_target(target)
    by_id = {r["id"]: r for r in records}
    observed = {r["id"]: r for r in observations}
    if len(by_id) != len(records) or len(observed) != len(observations) or set(by_id) != set(models) or set(observed) != set(models):
        raise ValueError("all seven original observations must remain in launch results")
    members = []
    for key, model in models.items():
        record, row = by_id[key], observed[key]
        artifact = model.get("complete_artifact_pin") or {}
        native = row.get("launch_native_anchor", {})
        # Filled below from the two fresh independent native replay exports;
        # producer-supplied stage status is never the anchor source.
        identities = ("native_class", "native_source", "program_root", "inventory_root", "real_parameter_root")
        exact = all(hex_string(artifact.get(k), 128) and native.get(k) == artifact[k] for k in identities)
        stages = record.get("stages", {})
        passed = (record.get("final_capable") is True and exact
                  and all(stages.get(s) == "PASS" for s in ac.STAGES))
        members.append({"id": key, "passed": passed, "initial_rights": RIGHTS,
                        "blocking": record.get("first_blocker") if not record.get("final_capable")
                        else "NATIVE_ARTIFACT_IDENTITY_UNPROVEN" if not exact else "PASS"})
    return {"schema": "misaka.rfc02.initial-seven-launch-result.v1", "models": members,
            "required_models": 7, "passed_models": sum(m["passed"] for m in members),
            "all_seven_evidence_passed": windows and all(m["passed"] for m in members),
            "two_receipt_windows_passed": windows, "consensus_activation_performed": False,
            "A6_or_A8_proven_by_launch_set": False}


def proof_derived_identity(audit: dict, root: Path) -> dict | None:
    evidence = audit.get("complete_model_identity_evidence", {})
    if not ac.evidence_ok(evidence, root) or evidence not in audit.get("chain_objects", []):
        return None
    report = runner.read_json(runner.relative_file(root, evidence["path"]))
    anchor = audit.get("validated_anchor", {})
    # Native28 is a component, not a complete-model Final. A new complete
    # Master32 exporter needs its own explicit verifier/schema before admission.
    if (report.get("schema") != ac.NATIVE_AUDIT_SCHEMA or report.get("namespace") != 29
            or report.get("pinned_headers_and_original_rows_verified") is not True
            or report.get("qualified_native_profile") is not True
            or report.get("final_receipt_class") != "qualified-final"
            or report.get("utxo_redemption_verified") is not True
            or any(report.get(k) != anchor.get(k) for k in ("class", "source", "job"))):
        return None
    identity = {"native_class": report.get("class"), "native_source": report.get("source"),
                "program_root": report.get("program_root"), "inventory_root": report.get("inventory"),
                "real_parameter_root": report.get("real_parameter_root")}
    if any(not hex_string(v, 128) for v in identity.values()) or audit.get("complete_model_identity") != identity:
        return None
    return identity


def run(target_file: Path, target_sha: str, cohort_file: Path, plan_file: Path, plan_sha: str, out: Path) -> dict:
    if ac.digest(target_file) != target_sha:
        raise ValueError("target differs from the precommitted launch manifest")
    target, cohort, plan = (runner.read_json(p) for p in (target_file, cohort_file, plan_file))
    validate_inputs(target, cohort, plan, ac.digest(cohort_file))
    report = runner.run(cohort_file, plan_file, plan_sha, out)
    observations = [json.loads(s) for s in (out / "observations.jsonl").read_text().splitlines() if s.strip()]
    # Obtain identities from the actual offline audit32 report carried inside the
    # independently replayed redemption exports. Missing fields remain a failure;
    # a legacy or component-only report is never upgraded by inference here.
    for index, row in enumerate(observations):
        folder = out / f"member-{index:05d}"
        pair_file = folder / "redemption-independent-audit-pair.json"
        anchors = []
        if pair_file.is_file():
            pair = runner.read_json(pair_file)
            for export in pair.get("exports", []):
                if ac.evidence_ok(export, out):
                    audit = runner.read_json(runner.relative_file(out, export["path"]))
                    anchors.append(proof_derived_identity(audit, out))
        if len(anchors) == 2 and anchors[0] is not None and anchors[0] == anchors[1]:
            row["launch_native_anchor"] = anchors[0]
    result = launch_outcome(target, report["records"], observations, report["two_receipt_windows_passed"])
    result.update(target_sha256=target_sha, cohort_sha256=ac.digest(cohort_file), plan_sha256=plan_sha)
    ac.write(out / "initial-seven-launch.json", result)
    return result


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("target", "cohort", "plan", "out"):
        ap.add_argument(f"--{name}", required=True, type=Path)
    ap.add_argument("--committed-target-sha256", required=True)
    ap.add_argument("--committed-plan-sha256", required=True)
    args = ap.parse_args()
    result = run(args.target, args.committed_target_sha256, args.cohort, args.plan, args.committed_plan_sha256, args.out)
    print(json.dumps(result, allow_nan=False))
    raise SystemExit(0 if result["all_seven_evidence_passed"] else 1)


if __name__ == "__main__":
    main()
