#!/usr/bin/env python3
"""Run precommitted full checkpoint, A6 and A8 protocols in one verification batch.

Each cohort and run plan is independently pinned before execution. Every member
is retained; an unavailable full L_files frame cannot be replaced by a local
engineering panel. This command runs only when the user starts verification.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import acceptance_cohort as ac
import checkpoint_cohort_runner as runner

SCHEMA = "misaka.rfc02.complete-verification-batch.v1"


def validate(batch: dict, base: Path) -> list[tuple[dict, Path, Path]]:
    if (batch.get("schema") != SCHEMA or batch.get("target_share") != .90
            or batch.get("failure_policy") != "all-frozen-members;unknown-and-nonresponse-fail;no-substitution"):
        raise ValueError("original frozen threshold and denominator policy required")
    entries = batch.get("cohorts")
    if not isinstance(entries, list) or not entries or len(entries) > 16:
        raise ValueError("bounded complete batch cohort list required")
    seen, prepared = set(), []
    for entry in entries:
        name = entry.get("name")
        if not isinstance(name, str) or not name or name in seen or not name.replace("-", "").replace("_", "").isalnum():
            raise ValueError("unique plain batch cohort name required")
        seen.add(name)
        cohort = runner.relative_file(base, entry["cohort"])
        plan = runner.relative_file(base, entry["plan"])
        if ac.digest(cohort) != entry.get("cohort_sha256") or ac.digest(plan) != entry.get("committed_plan_sha256"):
            raise ValueError("batch changes precommitted cohort or run plan bytes")
        doc = runner.read_json(cohort)
        runner.validate_plan(doc, runner.read_json(plan), entry["cohort_sha256"])
        gate = entry.get("completion_gate")
        if gate not in ("A6", "A8", "engineering"):
            raise ValueError("explicit completion gate required")
        if gate == "A6" and (doc.get("population") != "D_all" or doc.get("design") == "ENGINEERING_PANEL"):
            raise ValueError("A6 must retain original D_all sampling frame")
        if gate == "A8" and (doc.get("population") != "L_files" or doc.get("design") == "ENGINEERING_PANEL"
                or doc.get("frame_eligibility") != "complete_public_weights_and_full_task_metadata"
                or doc.get("digest_deduplication_complete") is not True
                or not {"huggingface", "ollama"} <= set(doc.get("catalogues", []))):
            raise ValueError("A8 complete readable-file/full-task frame with both catalogues required")
        prepared.append((entry, cohort, plan))
    return prepared


def make_plan(cohort: Path, commands: Path, out: Path, network: str, genesis: str) -> dict:
    """Freeze exact all-member commands, retaining unresolved members unchanged.

    Source details belong to the committed cohort. Command mappings cannot add
    members or change their source revision/context/task/components. An absent
    mapping becomes an explicit all-stage NOT_RUN, rather than substitution.
    """
    if out.parent.resolve() != commands.parent.resolve():
        raise ValueError("freeze-plan output must retain command/source-resolution base directory")
    doc, mappings = runner.read_json(cohort), runner.read_json(commands)
    ac.valid_design(doc)
    by_id = mappings.get("members", {})
    if not isinstance(by_id, dict) or set(by_id) - {m["id"] for m in doc["members"]}:
        raise ValueError("command mappings may only refer to frozen members")
    entries = []
    for member in doc["members"]:
        command = by_id.get(member["id"], {})
        resolved, _ = runner.resolve_original_member(member, command, commands.parent)
        scope = command.get("scope", {})
        if scope and (scope.get("repository") != member["repository"] or scope.get("revision") != member.get("revision")
                or scope.get("context_max") != resolved.get("source_declared_context_max")
                or scope.get("complete_task") != resolved.get("advertised_task")):
            raise ValueError("command scope changes original frozen member")
        entries.append({"id": member["id"], "scope": scope,
                        "task_components": resolved.get("required_task_components", []),
                        "stages": command.get("stages", {}),
                        **({"original_source_resolution": command["original_source_resolution"]} if command.get("original_source_resolution") else {})})
    plan = {"schema": runner.SCHEMA, "cohort_sha256": ac.digest(cohort), "target_share": .90,
            "required_stages": list(ac.STAGES), "fidelity_thresholds": runner.FIDELITY_THRESHOLDS,
            "execution_policy": "full-original-context-and-all-advertised-components;no-substitution;missing-fails",
            "network": network, "genesis": genesis, "members": entries}
    if mappings.get("receipt_window_command") is not None:
        plan["receipt_window_command"] = mappings["receipt_window_command"]
        plan["receipt_window_contract"] = mappings.get("receipt_window_contract", {})
    runner.validate_plan(doc, plan, ac.digest(cohort))
    ac.write(out, plan)
    return {"schema": "misaka.rfc02.frozen-run-plan-preparation.v1", "plan_sha256": ac.digest(out),
            "members": len(entries), "premeasurement_commit_required": True, "measurement_or_eligibility": False}


def run(batch_file: Path, committed_sha: str, out: Path) -> dict:
    if ac.digest(batch_file) != committed_sha:
        raise ValueError("verification batch changed after premeasurement commit")
    batch = runner.read_json(batch_file)
    prepared = validate(batch, batch_file.parent)
    out.mkdir(parents=True, exist_ok=False)
    results = []
    for entry, cohort, plan in prepared:
        folder = out / entry["name"]
        try:
            result = runner.run(cohort, plan, entry["committed_plan_sha256"], folder)
            results.append({"name": entry["name"], "gate": entry["completion_gate"], "status": "MEASURED",
                            "A6_satisfied": result["A6_satisfied"], "A8_satisfied": result["A8_satisfied"],
                            "report": str((folder / "acceptance.json").relative_to(out)),
                            "report_sha256": ac.digest(folder / "acceptance.json"),
                            "rates": result["rates"], "residual_table": result["residual_table"]})
        except (OSError, ValueError, KeyError, TypeError) as e:
            # A failed cohort never prevents independent remaining cohorts from
            # running, and never loses the original frozen denominator.
            results.append({"name": entry["name"], "gate": entry["completion_gate"], "status": "FAIL",
                            "A6_satisfied": False, "A8_satisfied": False, "blocking": str(e),
                            "cohort_sha256": entry["cohort_sha256"],
                            "all_original_members_count_as_failures": runner.read_json(cohort)["n"]})
    gates = {gate: [r for r in results if r["gate"] == gate] for gate in ("A6", "A8")}
    report = {"schema": "misaka.rfc02.complete-verification-batch-result.v1", "batch_sha256": committed_sha,
              "cohorts": results, "A6_satisfied": bool(gates["A6"]) and all(r["A6_satisfied"] for r in gates["A6"]),
              "A8_satisfied": bool(gates["A8"]) and all(r["A8_satisfied"] for r in gates["A8"]),
              "absent_gate_frame_status": {gate: "PRESENT" if entries else "NOT_RUN_MISSING_ORIGINAL_FRAME"
                                            for gate, entries in gates.items()},
              "RFC02_implementation_complete": False}
    # Other RFC02 implementation/economic/version gates require their own exact
    # native batch checks. This cohort driver cannot assert overall completion.
    ac.write(out / "batch-result.json", report)
    return report


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    make = sub.add_parser("freeze-plan")
    for name in ("cohort", "commands", "out"):
        make.add_argument(f"--{name}", type=Path, required=True)
    make.add_argument("--network", required=True)
    make.add_argument("--genesis", required=True)
    execute = sub.add_parser("run")
    execute.add_argument("--batch", type=Path, required=True)
    execute.add_argument("--committed-batch-sha256", required=True)
    execute.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    report = make_plan(a.cohort, a.commands, a.out, a.network, a.genesis) if a.command == "freeze-plan" else run(a.batch, a.committed_batch_sha256, a.out)
    print(json.dumps(report, allow_nan=False))


if __name__ == "__main__":
    main()
