#!/usr/bin/env python3
"""Execute a predeclared complete-checkpoint pipeline without changing its cohort.

Commands are argv arrays, never shell source. Each stage has a fresh bounded JSON
result and retained log. A successful process is not a successful stage: scope,
all task components, output hashes and a separately executed native-chain audit
must agree. Unknown/unavailable members stay in the denominator as NOT_RUN.
This runner orchestrates real producers/checkers; it never simulates receipts.
"""
from __future__ import annotations
import argparse
import datetime as dt
import json
import math
import os
from pathlib import Path
import signal
import select
import subprocess
import time

import acceptance_cohort as ac

SCHEMA = "misaka.rfc02.checkpoint-run-plan.v2"
RESULT_SCHEMA = "misaka.rfc02.checkpoint-stage-result.v2"
CHAIN_SCHEMA = "misaka.rfc02.independent-native-chain-audit.v3"
AUDIT_PAIR_SCHEMA = "misaka.rfc02.independent-native-chain-audit-pair.v3"
ANCHOR_FIELDS = ("network", "genesis", "anchor_block", "anchor_daa", "active_kernel", "class", "source", "job",
                 "maximum_scope", "actual_scope", "resource_contract", "original_funding", "initial_consent",
                 "claim", "final", "single_credit", "redemption_transaction", "redemption_utxo")
MAX_JSON = 16 << 20
MAX_LOG = 64 << 20
CHAIN_STAGES = {"register", "seat", "final", "redemption"}
FIDELITY_THRESHOLDS = {"teacher_forced_top1_min": .95, "mean_kl_max": .01,
                       "logit_correlation_min": .99, "generated_token_agreement_min": .95,
                       "exact_generated_job_fraction_min": 2 / 3,
                       "source_and_integer_task_success_min": 1.0}


def read_json(path: Path) -> dict:
    if not path.is_file() or path.stat().st_size > MAX_JSON:
        raise ValueError("missing/oversized bounded JSON artifact")
    with path.open() as f:
        doc = json.load(f, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    if not isinstance(doc, dict):
        raise ValueError("JSON object required")
    return doc


def relative_file(root: Path, name: str) -> Path:
    p = Path(name)
    if p.is_absolute() or not p.parts or ".." in p.parts:
        raise ValueError("relative evidence path required")
    full = (root / p).resolve()
    if not full.is_relative_to(root.resolve()):
        raise ValueError("evidence symlink escapes run directory")
    return full


def source_pins_ok(member: dict) -> bool:
    source = member.get("source_directory")
    pins = member.get("source_files")
    if not isinstance(source, str) or not isinstance(pins, list) or not pins:
        return False
    names = set()
    for pin in pins:
        try:
            p = relative_file(Path(source), pin["path"])
            if pin["path"] in names or type(pin["bytes"]) is not int or pin["bytes"] < 0:
                return False
            names.add(pin["path"])
            if not p.is_file() or p.stat().st_size != pin["bytes"] or ac.digest(p) != pin["sha256"]:
                return False
        except (KeyError, ValueError, OSError):
            return False
    return True


def validate_plan(cohort: dict, plan: dict, cohort_sha: str) -> None:
    ac.valid_design(cohort)
    if plan.get("schema") != SCHEMA or plan.get("cohort_sha256") != cohort_sha:
        raise ValueError("run plan is not pinned to the frozen cohort")
    if cohort.get("fidelity_thresholds", FIDELITY_THRESHOLDS) != FIDELITY_THRESHOLDS:
        raise ValueError("unknown fidelity thresholds require a separately reviewed protocol")
    if plan.get("fidelity_thresholds", FIDELITY_THRESHOLDS) != FIDELITY_THRESHOLDS:
        raise ValueError("run plan weakens original fidelity/task thresholds")
    if plan.get("required_stages") != list(ac.STAGES) or plan.get("target_share") != .90:
        raise ValueError("run plan weakens frozen completion requirements")
    members = plan.get("members")
    if not isinstance(members, list) or {m.get("id") for m in members} != {m["id"] for m in cohort["members"]} or len(members) != cohort["n"]:
        raise ValueError("plan must retain every frozen member exactly once")
    # A plan must itself have been committed before measurement. The driver records
    # its hash; the caller supplies the committed hash rather than a mutable flag.
    if plan.get("execution_policy") != "full-original-context-and-all-advertised-components;no-substitution;missing-fails":
        raise ValueError("explicit full-scope execution policy required")
    for entry in members:
        stages = entry.get("stages", {})
        if set(stages) - set(ac.STAGES):
            raise ValueError("unknown pipeline stage")
        for stage, spec in stages.items():
            if stage in CHAIN_STAGES:
                validate_auditors(spec.get("independent_audits"), plan)
            argv = spec.get("argv")
            if not isinstance(argv, list) or not argv or any(not isinstance(x, str) or not x or "\0" in x for x in argv):
                raise ValueError("nonempty literal argv required")
            timeout = spec.get("timeout_seconds")
            if type(timeout) is not int or not 1 <= timeout <= 43200:
                raise ValueError("explicit bounded command timeout required")
            if not isinstance(spec.get("executable_sha256"), str) or len(spec["executable_sha256"]) != 64:
                raise ValueError("predeclared executable SHA-256 required")
            if spec.get("maximum_result_bytes", MAX_JSON) > MAX_JSON:
                raise ValueError("stage JSON cap exceeds runner cap")


def validate_auditors(auditors: list, plan: dict) -> None:
    if not isinstance(auditors, list) or len(auditors) != 2:
        raise ValueError("two independently replayed, pinned native node exporters required")
    if len({a.get("operator_id") for a in auditors}) != 2 or len({a.get("node_database") for a in auditors}) != 2:
        raise ValueError("auditors must use distinct operators and independent node databases")
    for audit in auditors:
        if (not isinstance(audit.get("argv"), list) or not audit["argv"]
                or any(not isinstance(v, str) or not v or "\0" in v for v in audit["argv"])
                or type(audit.get("timeout_seconds")) is not int or not 1 <= audit["timeout_seconds"] <= 43200
                or not isinstance(audit.get("executable_sha256"), str) or len(audit["executable_sha256"]) != 64
                or not audit.get("operator_id") or not audit.get("node_database")
                or audit.get("genesis") != plan.get("genesis") or not plan.get("genesis")
                or audit.get("network") != plan.get("network") or not plan.get("network")):
            raise ValueError("predeclared network/genesis, executable, node database and operator required")


def inspect_audit_pair(path: Path, root: Path, scope: dict, stage: str, run_sha: str) -> tuple[bool, str]:
    pair = read_json(path)
    if pair.get("schema") != AUDIT_PAIR_SCHEMA or len(pair.get("exports", [])) != 2:
        return False, "TWO_INDEPENDENT_EXPORTS_REQUIRED"
    anchors, operators, databases = [], set(), set()
    for entry in pair["exports"]:
        if not ac.evidence_ok(entry, root):
            return False, "NATIVE_EXPORT_MISSING_OR_MUTATED"
        audit = read_json(relative_file(root, entry["path"]))
        checks = ("canonical_blocks_verified", "native_carriers_verified", "class_source_job_scope_verified",
                  "active_kernel_verified", "resource_reservations_verified", "own_seal_and_public_da_verified")
        if (audit.get("schema") != CHAIN_SCHEMA or audit.get("scope") != scope or audit.get("stage") != stage
                or audit.get("run_plan_sha256") != run_sha or any(audit.get(k) is not True for k in checks)
                or audit.get("operator_id") != entry.get("operator_id")
                or audit.get("node_database") != entry.get("node_database")
                or audit.get("genesis") != entry.get("genesis") or audit.get("network") != entry.get("network")
                or not audit.get("chain_objects") or not all(ac.evidence_ok(e, root) for e in audit["chain_objects"])):
            return False, "NATIVE_CHAIN_AUDIT_INCOMPLETE"
        for evidence in audit["chain_objects"]:
            relative_file(root, evidence["path"])
        anchor = audit.get("validated_anchor")
        if (not isinstance(anchor, dict) or set(anchor) != set(ANCHOR_FIELDS)
                or any(anchor.get(k) in (None, "", False) for k in ANCHOR_FIELDS[:13])
                or anchor["genesis"] != entry["genesis"] or anchor["network"] != entry["network"]
                or audit.get("anchor_from_validated_consensus_database") is not True):
            return False, "MISSING_VALIDATED_DATABASE_ANCHOR"
        if stage in {"final", "redemption"} and (audit.get("final_authenticated") is not True
                or audit.get("court_window_closed") is not True or not anchor["final"]):
            return False, "INDEPENDENT_FINAL_NOT_AUTHENTICATED"
        if stage == "redemption" and (audit.get("redemption_authenticated") is not True
                or audit.get("single_credit_verified") is not True or audit.get("utxo_spend_verified") is not True
                or not all(anchor[k] for k in ("single_credit", "redemption_transaction", "redemption_utxo"))):
            return False, "INDEPENDENT_UTXO_REDEMPTION_NOT_AUTHENTICATED"
        anchors.append(anchor)
        operators.add(entry["operator_id"])
        databases.add(entry["node_database"])
    if len(operators) != 2 or len(databases) != 2 or anchors[0] != anchors[1]:
        return False, "INDEPENDENT_NATIVE_REPLAYS_DISAGREE"
    return True, "PASS"


def execute_audit_pair(specs: list, context: dict, folder: Path, stage: str, out: Path) -> tuple[Path, float]:
    exports, elapsed = [], 0.0
    for index, spec in enumerate(specs):
        path = folder / f"{stage}-independent-audit-{index}.json"
        if path.exists():
            raise ValueError("producer pre-created independent auditor output")
        ctx = {**context, "audit_result": str(path.resolve()), "node_database": spec["node_database"],
               "operator_id": spec["operator_id"], "genesis": spec["genesis"], "network": spec["network"]}
        code, seconds, error = run_command(spec, ctx, folder / f"{stage}-independent-audit-{index}.log")
        elapsed += seconds
        if code or error or not path.is_file() or path.stat().st_size > MAX_JSON:
            raise ValueError(error or "independent native exporter failed")
        exports.append({"path": str(path.relative_to(out)), "sha256": ac.digest(path),
                        **{k: spec[k] for k in ("operator_id", "node_database", "genesis", "network")}})
    pair = folder / f"{stage}-independent-audit-pair.json"
    ac.write(pair, {"schema": AUDIT_PAIR_SCHEMA, "exports": exports})
    return pair, elapsed


def run_command(spec: dict, context: dict[str, str], log: Path) -> tuple[int, float, str | None]:
    argv = []
    for value in spec["argv"]:
        # Expand whole-token placeholders only. Model text is never executable syntax.
        if value.startswith("{") and value.endswith("}"):
            key = value[1:-1]
            if key not in context:
                raise ValueError("unknown command placeholder")
            value = context[key]
        argv.append(value)
    exe = Path(argv[0])
    if not exe.is_absolute() or not exe.is_file() or ac.digest(exe) != spec["executable_sha256"]:
        raise ValueError("executable differs from predeclared implementation")
    start = time.monotonic()
    reason = None
    with log.open("xb") as out:
        p = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             start_new_session=True, env={**os.environ, "CARGO_INCREMENTAL": "0"})
        written = 0
        pipe_open = True
        while p.poll() is None or pipe_open:
            if time.monotonic() - start > spec["timeout_seconds"]:
                reason = "COMMAND_TIMEOUT"
            if pipe_open:
                ready, _, _ = select.select([p.stdout], [], [], .1)
                if ready:
                    data = os.read(p.stdout.fileno(), 65536)
                    if data:
                        keep = min(len(data), MAX_LOG - written)
                        out.write(data[:keep])
                        written += keep
                        if keep < len(data):
                            reason = "COMMAND_LOG_LIMIT"
                    else:
                        pipe_open = False
            if reason:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                if p.poll() is None:
                    try:
                        p.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(p.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        p.wait()
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                break
            if not pipe_open and p.poll() is None:
                time.sleep(.1)
        p.stdout.close()
        p.wait()
    if ac.digest(exe) != spec["executable_sha256"]:
        reason = "EXECUTABLE_MUTATED_DURING_RUN"
    return p.returncode, time.monotonic() - start, reason


def inspect_result(path: Path, root: Path, member: dict, scope: dict, stage: str, run_sha: str, audit_path: Path | None = None) -> tuple[bool, str]:
    try:
        r = read_json(path)
        if (r.get("schema") != RESULT_SCHEMA or r.get("status") != "PASS" or r.get("stage") != stage
                or r.get("member_id") != member["id"] or r.get("run_plan_sha256") != run_sha or r.get("scope") != scope
                or r.get("actual_context_max") != member.get("source_declared_context_max")
                or r.get("full_advertised_task") is not True
                or set(r.get("completed_task_components", [])) != set(member.get("required_task_components", []))):
            return False, r.get("blocking") or "STAGE_SCOPE_OR_COMPLETENESS_MISMATCH"
        evidence = r.get("artifacts")
        if not isinstance(evidence, list) or not evidence or not all(ac.evidence_ok(e, root) for e in evidence):
            return False, "UNPINNED_OR_MISSING_STAGE_ARTIFACTS"
        for e in evidence:
            relative_file(root, e["path"])
        if stage == "fidelity":
            metrics = r.get("metrics", {})
            expected = {"teacher_forced_top1": (.95, False), "mean_kl": (.01, True),
                        "logit_correlation": (.99, False), "generated_token_agreement": (.95, False),
                        "exact_generated_job_fraction": (2 / 3, False), "source_task_success": (1.0, False),
                        "integer_task_success": (1.0, False)}
            if any(type(metrics.get(k)) not in (int, float) or not math.isfinite(metrics[k])
                   or (metrics[k] > v if upper else metrics[k] < v) for k, (v, upper) in expected.items()):
                return False, "ORIGINAL_FIDELITY_OR_TASK_THRESHOLDS_FAILED"
            if any(type(metrics.get(k)) is not int or metrics[k] <= 0 for k in ("teacher_forced_positions", "generated_tokens", "complete_jobs")):
                return False, "NO_COMPLETE_FIDELITY_MEASUREMENT"
            if metrics["teacher_forced_positions"] < member["source_declared_context_max"]:
                return False, "FULL_ORIGINAL_CONTEXT_FIDELITY_UNMEASURED"
            if any(not 0 <= metrics[k] <= 1 for k in expected if k != "mean_kl") or metrics["mean_kl"] < 0:
                return False, "MALFORMED_FIDELITY_RANGES"
        if stage in CHAIN_STAGES:
            # No PASS without a separate audit process and exact retained chain objects.
            if audit_path is None:
                return False, "MISSING_INDEPENDENT_NATIVE_AUDIT"
            good, reason = inspect_audit_pair(audit_path, root, scope, stage, run_sha)
            if not good:
                return good, reason
        return True, "PASS"
    except (ValueError, OSError, KeyError, TypeError):
        return False, "MALFORMED_STAGE_RESULT"


def run(cohort_path: Path, plan_path: Path, committed_plan_sha: str, out: Path) -> dict:
    cohort, plan = read_json(cohort_path), read_json(plan_path)
    cohort_sha, run_sha = ac.digest(cohort_path), ac.digest(plan_path)
    if run_sha != committed_plan_sha:
        raise ValueError("plan bytes differ from the precommitted measurement plan")
    validate_plan(cohort, plan, cohort_sha)
    out.mkdir(parents=True, exist_ok=False)
    entries = {m["id"]: m for m in plan["members"]}
    rows = []
    for index, member in enumerate(cohort["members"]):
        entry = entries[member["id"]]
        scope = entry.get("scope", {})
        row = {"id": member["id"], "cohort_sha256": cohort_sha, "run_plan_sha256": run_sha,
               "scope": scope, "source_declared_context_max": member.get("source_declared_context_max"),
               "full_advertised_task": False, "real_size": False, "active_shipping_kernel": False,
               "public_chain": False, "stages": {}}
        valid_scope = (all(scope.get(k) not in (None, "", False) for k in ac.SCOPE)
                       and scope.get("repository") == member["repository"] and scope.get("revision") == member["revision"]
                       and scope.get("context_max") == member.get("source_declared_context_max")
                       and scope.get("complete_task") == member.get("advertised_task")
                       and set(entry.get("task_components", [])) == set(member.get("required_task_components", [])))
        folder = out / f"member-{index:05d}"
        folder.mkdir()
        ac.write(folder / "member.json", member)
        ac.write(folder / "scope.json", scope)
        pins = valid_scope and source_pins_ok(member)
        blocked = not pins
        for stage in ac.STAGES:
            result = folder / f"{stage}.json"
            spec = entry.get("stages", {}).get(stage)
            record = {"status": "NOT_RUN", "scope": scope, "blocking": "UPSTREAM_FAILURE"}
            if not valid_scope:
                record["blocking"] = "FULL_SCOPE_PLAN_UNRESOLVED"
            elif not pins:
                record["blocking"] = "MISSING_OR_MISMATCHED_ORIGINAL_SOURCE"
            elif not spec:
                record["blocking"] = "NO_IMPLEMENTED_STAGE_COMMAND"
                blocked = True
            elif not blocked:
                ctx = {"member": str((folder / "member.json").resolve()), "scope": str((folder / "scope.json").resolve()),
                       "result": str(result.resolve()), "evidence_root": str(out.resolve()), "run_plan_sha256": run_sha,
                       "cohort": str(cohort_path.resolve())}
                log = folder / f"{stage}.log"
                try:
                    code, elapsed, error = run_command(spec, ctx, log)
                    audit_path = None
                    if not code and not error and stage in CHAIN_STAGES:
                        audit_path, audit_elapsed = execute_audit_pair(spec["independent_audits"],
                            {**ctx, "stage_result": str(result.resolve())}, folder, stage, out)
                        elapsed += audit_elapsed
                    good, why = inspect_result(result, out, member, scope, stage, run_sha, audit_path) if not code and not error else (False, error or "COMMAND_FAILED")
                except (OSError, ValueError) as e:
                    code, elapsed, good, why = -1, 0, False, f"COMMAND_START_FAILED:{type(e).__name__}"
                record.update(status="PASS" if good else "FAIL", blocking=why, returncode=code, elapsed_seconds=elapsed)
                if result.is_file() and result.stat().st_size <= MAX_JSON:
                    record["evidence"] = {"path": str(result.relative_to(out)), "sha256": ac.digest(result)}
                blocked = not good
            row["stages"][stage] = record
        # Detect source mutation even if a successful tool ignored input pins.
        if pins and not source_pins_ok(member):
            row["stages"]["source"].update(status="FAIL", blocking="SOURCE_MUTATED_DURING_RUN")
            blocked = True
        if all(row["stages"][s]["status"] == "PASS" for s in ac.STAGES[:ac.STAGES.index("register") + 1]):
            row.update(full_advertised_task=True, real_size=True, active_shipping_kernel=True, public_chain=True)
        rows.append(row)
        with (out / "observations.jsonl").open("a") as f:
            f.write(json.dumps(row, allow_nan=False) + "\n")
    panel = None
    window_status = {"status": "NOT_RUN", "blocking": "NO_TWO_WINDOW_NATIVE_DRIVER"}
    if plan.get("receipt_window_command") is not None:
        candidate = out / "receipt-windows.json"
        try:
            spec = plan["receipt_window_command"]
            validate_auditors(spec.get("independent_audits"), plan)
            ctx = {"evidence_root": str(out.resolve()), "result": str(candidate.resolve()),
                   "run_plan_sha256": run_sha, "cohort": str(cohort_path.resolve())}
            code, _, error = run_command(spec, ctx, out / "receipt-windows.log")
            if code or error or not ac.panel_ok(read_json(candidate), out):
                raise ValueError(error or "original two-window intended receipt conditions failed")
            pair, _ = execute_audit_pair(spec["independent_audits"],
                {**ctx, "stage_result": str(candidate.resolve())}, out, "receipt-windows", out)
            exports = read_json(pair)["exports"]
            for entry in exports:
                audit = read_json(relative_file(out, entry["path"]))
                if (audit.get("schema") != CHAIN_SCHEMA or audit.get("stage") != "receipt-windows"
                        or audit.get("run_plan_sha256") != run_sha or audit.get("genesis") != entry["genesis"]
                        or audit.get("network") != entry["network"] or audit.get("operator_id") != entry["operator_id"]
                        or audit.get("node_database") != entry["node_database"]
                        or audit.get("anchor_from_validated_consensus_database") is not True
                        or audit.get("canonical_blocks_verified") is not True
                        or audit.get("intended_claims_and_queue_accounting_verified") is not True
                        or audit.get("receipt_windows") != read_json(candidate)
                        or not audit.get("chain_objects") or not all(ac.evidence_ok(e, out) for e in audit["chain_objects"])):
                    raise ValueError("independent original receipt-window/queue replay differs")
                for evidence in audit["chain_objects"]:
                    relative_file(out, evidence["path"])
            panel = candidate
            window_status = {"status": "PASS", "audit_pair_sha256": ac.digest(pair)}
        except (OSError, ValueError, KeyError, TypeError) as e:
            window_status = {"status": "FAIL", "blocking": str(e)}
    report = ac.report(cohort_path, out / "observations.jsonl", out, panel)
    report.update(receipt_window_run=window_status, run_plan_sha256=run_sha, observed_utc=dt.datetime.now(dt.timezone.utc).isoformat())
    ac.write(out / "acceptance.json", report)
    return report


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--cohort", type=Path, required=True)
    ap.add_argument("--plan", type=Path, required=True)
    ap.add_argument("--committed-plan-sha256", required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    r = run(a.cohort, a.plan, a.committed_plan_sha256, a.out)
    print(json.dumps({k: v for k, v in r.items() if k != "records"}, allow_nan=False))


if __name__ == "__main__":
    main()
