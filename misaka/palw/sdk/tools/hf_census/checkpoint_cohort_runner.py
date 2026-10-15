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


def resolve_original_member(member: dict, entry: dict, plan_root: Path) -> tuple[dict, dict | None]:
    """Resolve context/task/components from the SAME frozen source revision.

    A frozen inaccessible/null revision cannot be replaced by today's source.
    The resolution is a predeclared hashed publisher metadata artifact, retained
    verbatim in the run. Its source files are checked before any command runs.
    """
    if member.get("source_declared_context_max"):
        return member, None
    evidence = entry.get("original_source_resolution")
    if not isinstance(evidence, dict) or not member.get("revision") or not ac.evidence_ok(evidence, plan_root):
        return member, None
    path = relative_file(plan_root, evidence["path"])
    resolution = read_json(path)
    if (resolution.get("schema") != "misaka.rfc02.original-source-resolution.v1"
            or resolution.get("repository") != member["repository"] or resolution.get("revision") != member["revision"]
            or not resolution.get("advertised_task")
            or member.get("advertised_task") is not None and resolution["advertised_task"] != member["advertised_task"]
            or type(resolution.get("source_declared_context_max")) is not int or resolution["source_declared_context_max"] <= 0
            or not isinstance(resolution.get("required_task_components"), list) or not resolution["required_task_components"]
            or not resolution.get("publisher_metadata_files")
            or not all(ac.evidence_ok(e, plan_root) for e in resolution["publisher_metadata_files"])):
        raise ValueError("source resolution changes frozen revision or lacks original publisher metadata")
    for e in resolution["publisher_metadata_files"]:
        relative_file(plan_root, e["path"])
    fields = ("advertised_task", "source_declared_context_max", "required_task_components", "source_directory", "source_files")
    return {**member, **{k: resolution[k] for k in fields}}, {"path":str(path), "sha256":evidence["sha256"]}


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
    if plan.get("receipt_window_command") is not None:
        spec = plan["receipt_window_command"]
        validate_auditors(spec.get("independent_audits"), plan)
        contract = plan.get("receipt_window_contract", {})
        if (contract.get("cohort_sha256") != cohort_sha or type(contract.get("receipt_window_daa")) is not int
                or contract["receipt_window_daa"] <= 0 or contract.get("minimum_independent_operators", 0) < 2
                or not isinstance(contract.get("original_model_roots"), dict) or not contract["original_model_roots"]
                or set(contract["original_model_roots"]) - {m["id"] for m in cohort["members"]}
                or any(not isinstance(v, str) or len(v) != 128 for v in contract["original_model_roots"].values())):
            raise ValueError("two-window original cohort/model R/clock/operator contract must be fixed before measurement")
    if plan.get("receipt_window_command") is not None:
        from native_receipt_window_driver_v1 import validate_contract
        validate_contract(plan["receipt_window_contract"])
        from native_node_provenance_v1 import validate_specs
        validate_specs(plan["receipt_window_command"]["independent_audits"])
        from native_receipt_live_acquisition_v2 import validate_live_collectors
        validate_live_collectors(plan["receipt_window_command"]["independent_audits"])
        for audit in plan["receipt_window_command"]["independent_audits"]:
            validate_receipt_verifier(audit.get("receipt_proof_verifier"), plan["network"])
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
    anchors, operators, databases, local_profiles = [], set(), set(), []
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
                or any(anchor.get(k) in (None, "", False) for k in ("network", "genesis", "anchor_block", "active_kernel", "class", "source", "maximum_scope", "resource_contract"))
                or type(anchor.get("anchor_daa")) is not int or anchor["anchor_daa"] < 0
                or anchor["genesis"] != entry["genesis"] or anchor["network"] != entry["network"]
                or audit.get("anchor_from_validated_consensus_database") is not True
                or audit.get("qualified_native_profile") is not True or audit.get("isolated_calibration") is not False):
            return False, "MISSING_VALIDATED_DATABASE_ANCHOR"
        if stage in {"seat", "final", "redemption"} and not all(anchor[k] for k in ("job", "original_funding", "initial_consent")):
            return False, "ORIGINAL_FUNDING_CONSENT_OR_JOB_MISSING"
        if stage in {"final", "redemption"} and (audit.get("final_authenticated") is not True
                or audit.get("court_window_closed") is not True or not anchor["final"]):
            return False, "INDEPENDENT_FINAL_NOT_AUTHENTICATED"
        if stage == "redemption" and (audit.get("redemption_authenticated") is not True
                or audit.get("single_credit_verified") is not True or audit.get("utxo_spend_verified") is not True
                or not all(anchor[k] for k in ("single_credit", "redemption_transaction", "redemption_utxo"))):
            return False, "INDEPENDENT_UTXO_REDEMPTION_NOT_AUTHENTICATED"
        anchors.append({"ordinary": anchor, "complete_model_binding": audit.get("complete_model_binding"),
                        "master": audit.get("validated_master_anchor")})
        local_profiles.append(audit.get("local_qualified_validation_network") is True)
        operators.add(entry["operator_id"])
        databases.add(entry["node_database"])
    if len(operators) != 2 or len(databases) != 2 or anchors[0] != anchors[1]:
        return False, "INDEPENDENT_NATIVE_REPLAYS_DISAGREE"
    if local_profiles[0] != local_profiles[1]:
        return False, "INDEPENDENT_NATIVE_PROFILE_CLASSIFICATION_DISAGREES"
    if local_profiles[0]:
        return False, "LOCAL_QUALIFIED_VALIDATION_ONLY"
    return True, "PASS"


def validate_receipt_verifier(spec: dict, network: str) -> None:
    if (not isinstance(spec, dict) or not isinstance(spec.get("argv"), list) or not spec["argv"]
            or not Path(spec["argv"][0]).is_absolute()
            or type(spec.get("timeout_seconds")) is not int or not 1 <= spec["timeout_seconds"] <= 43200
            or not isinstance(spec.get("executable_sha256"), str) or len(spec["executable_sha256"]) != 64):
        raise ValueError("fresh pinned offline receipt proof verifier required for each independent exporter")
    argv = spec["argv"]
    for flag, expected in (("--network", network), ("--contract", "{receipt_window_contract}"),
                           ("--points", "{receipt_points}"), ("--points-format", "stream-v2"), ("--evidence-root", "{evidence_root}"),
                           ("--out", "{receipt_proof_result}")):
        if argv.count(flag) != 1 or argv.index(flag)+1 >= len(argv) or argv[argv.index(flag)+1] != expected:
            raise ValueError("offline receipt verifier must consume original contract/own proof transcript and fresh output")


def recheck_receipt_proof_pair(pair: Path, root: Path, specs: list, contract_path: Path) -> list:
    """Runner executes the proof verifier itself: exported JSON cannot authorize PASS."""
    exports = read_json(pair).get("exports", [])
    if len(exports) != 2 or len(specs) != 2:
        raise ValueError("two independently collected receipt transcripts required")
    verified = []
    for index, (entry, owner) in enumerate(zip(exports, specs)):
        if not ac.evidence_ok(entry, root):
            raise ValueError("receipt exporter artifact mutated")
        audit = read_json(relative_file(root, entry["path"]))
        points = audit.get("receipt_points_evidence", {})
        if not ac.evidence_ok(points, root) or points not in audit.get("chain_objects", []):
            raise ValueError("complete independently collected native proof transcript missing")
        spec = owner["receipt_proof_verifier"]
        validate_receipt_verifier(spec, owner["network"])
        output = root / f"receipt-proof-rechecked-{index}.json"
        if output.exists():
            raise ValueError("producer pre-created independent proof recheck output")
        context = {"receipt_window_contract": str(contract_path.resolve()), "evidence_root": str(root.resolve()),
                   "receipt_points": str(relative_file(root, points["path"])), "receipt_proof_result": str(output.resolve())}
        witness = audit.get("receipt_master_witnesses_evidence")
        has_masters = any(w.get("masters") for w in read_json(contract_path)["windows"])
        if has_masters:
            if not ac.evidence_ok(witness, root) or witness not in audit.get("chain_objects", []):
                raise ValueError("complete Master32 public witnesses missing")
            # The path is a verifier input, never a producer-authored verdict.
            # Append ourselves so the command template cannot omit this binding.
            if "--master-witnesses" in spec["argv"]:
                raise ValueError("runner owns exact original Master32 witness binding")
            spec = {**spec, "argv": [*spec["argv"], "--master-witnesses", witness["path"]]}
        code, _, error = run_command(spec, context, root / f"receipt-proof-rechecked-{index}.log")
        if code or error or not output.is_file() or read_json(output) != audit.get("receipt_proof_result"):
            raise ValueError(error or "fresh offline native proof recheck differs from exporter JSON")
        verified.append({"path": str(output.relative_to(root)), "sha256": ac.digest(output)})
    return verified


def inspect_receipt_audit_pair(path: Path, root: Path, contract: dict, run_sha: str) -> dict:
    """Accept only matching proof-checker output from two own replay databases.

    Producer pointer documents and a pair of bool-only JSON reports are insufficient.
    Every window names a hashed offline report; exact original jobs, native clocks,
    per-bin throughput, whole-transcript roots and counters must agree across nodes.
    """
    pair = read_json(path)
    if pair.get("schema") != AUDIT_PAIR_SCHEMA or len(pair.get("exports", [])) != 2:
        raise ValueError("TWO_INDEPENDENT_RECEIPT_PROOF_EXPORTS_REQUIRED")
    results, panels, operators, databases = [], [], set(), set()
    for entry in pair["exports"]:
        if not ac.evidence_ok(entry, root):
            raise ValueError("NATIVE_RECEIPT_EXPORT_MISSING_OR_MUTATED")
        audit = read_json(relative_file(root, entry["path"]))
        if (audit.get("schema") != CHAIN_SCHEMA or audit.get("stage") != "receipt-windows"
                or audit.get("run_plan_sha256") != run_sha
                or any(audit.get(k) != entry.get(k) for k in ("genesis", "network", "operator_id", "node_database"))
                or any(audit.get(k) is not True for k in ("anchor_from_validated_consensus_database", "canonical_blocks_verified",
                    "qualified_native_profile", "intended_claims_and_queue_accounting_verified"))
                or audit.get("isolated_calibration") is not False or not audit.get("chain_objects")
                or not all(ac.evidence_ok(e, root) for e in audit["chain_objects"])):
            raise ValueError("INDEPENDENT_NATIVE_RECEIPT_PROOFS_INCOMPLETE")
        for e in audit["chain_objects"]:
            relative_file(root, e["path"])
        evidence = audit.get("receipt_proof_evidence", {})
        if not ac.evidence_ok(evidence, root) or evidence not in audit["chain_objects"]:
            raise ValueError("OFFLINE_RECEIPT_PROOF_REPORT_MISSING")
        report = read_json(relative_file(root, evidence["path"]))
        if (report != audit.get("receipt_proof_result") or report.get("schema") != "misaka.rfc02.proof-derived-receipt-windows.v2"
                or report.get("qualified_native_profile") is not True
                or report.get("intended_claims_and_queue_accounting_verified") is not True
                or len(report.get("windows", [])) != 2 or len(report.get("cumulative_accounting", [])) < 3
                or type(report.get("transcript_points")) is not int or report["transcript_points"] < 3
                or report.get("retention_profile") != "previous-and-frozen-endpoints;every-point-verified;no-sampling"
                or not isinstance(report.get("consecutive_transcript_root"), str) or len(report["consecutive_transcript_root"]) != 128):
            raise ValueError("BOOL_ONLY_RECEIPT_REPORT_IS_NOT_PROOF")
        panel = audit.get("receipt_windows", {})
        if (not ac.panel_ok(panel, root, allow_local_validation=True) or panel.get("schema") != "misaka.rfc02.proof-derived-receipt-panel.v1"
                or panel.get("contract_root") != report.get("contract_root")
                or any(panel.get(k) != contract.get(k) or report.get(k) != contract.get(k)
                       for k in ("cohort_sha256", "original_model_roots", "receipt_window_daa", "minimum_independent_operators"))):
            raise ValueError("ORIGINAL_RECEIPT_DENOMINATOR_OR_CONTRACT_CHANGED")
        for frozen, observed, public in zip(contract["windows"], report["windows"], panel["windows"]):
            expected_ids = {j["job"] for j in frozen["intended"]} | {m["master"] for m in frozen.get("masters", [])}
            expected_count = len(frozen["intended"]) + len(frozen.get("masters", []))
            if (public != {**observed, "evidence": evidence}
                    or any(observed.get(k) != frozen[k] for k in ("start_daa", "end_daa", "issuance_bin_daa", "minimum_commits_per_bin", "maximum_final_lag_daa"))
                    or observed.get("intended_claims") != expected_count
                    or {j.get("job") for j in observed.get("original_claims", [])} != expected_ids
                    or len(observed.get("original_claims", [])) != expected_count):
                raise ValueError("ACCEPTED_ORIGINAL_RECEIPT_SET_OR_CADENCE_CHANGED")
            bins = (frozen["end_daa"] - frozen["start_daa"]) // frozen["issuance_bin_daa"]
            counts = observed.get("accepted_commits_per_bin", [])
            if (len(counts) != bins or any(type(n) is not int or n < frozen["minimum_commits_per_bin"] for n in counts)
                    or counts != observed.get("bounded_finals_per_bin") or sum(counts) != expected_count):
                raise ValueError("SUSTAINED_RECEIPT_ISSUANCE_OR_FINAL_THROUGHPUT_FAILED")
        results.append(report)
        panels.append(panel)
        operators.add(entry["operator_id"])
        databases.add(entry["node_database"])
    if len(operators) != 2 or len(databases) != 2 or results[0] != results[1]:
        raise ValueError("INDEPENDENT_RECEIPT_REPLAY_OR_CUMULATIVE_QUEUE_DISAGREES")
    return panels[0]


def execute_audit_pair(specs: list, context: dict, folder: Path, stage: str, out: Path) -> tuple[Path, float]:
    exports, elapsed = [], 0.0
    for index, spec in enumerate(specs):
        path = folder / f"{stage}-independent-audit-{index}.json"
        if path.exists():
            raise ValueError("producer pre-created independent auditor output")
        ctx = {**context, "audit_result": str(path.resolve()), "node_database": spec["node_database"],
               "operator_id": spec["operator_id"], "genesis": spec["genesis"], "network": spec["network"],
               "audit_work_dir": str((folder / f"{stage}-independent-audit-{index}-work").resolve()),
               "live_collected": str((folder / f"receipt-live-{index}" / "collected").resolve())}
        code, seconds, error = run_command(spec, ctx, folder / f"{stage}-independent-audit-{index}.log")
        elapsed += seconds
        if code or error or not path.is_file() or path.stat().st_size > MAX_JSON:
            raise ValueError(error or "independent native exporter failed")
        exports.append({"path": str(path.relative_to(out)), "sha256": ac.digest(path),
                        **{k: spec[k] for k in ("operator_id", "node_database", "genesis", "network")}})
    pair = folder / f"{stage}-independent-audit-pair.json"
    ac.write(pair, {"schema": AUDIT_PAIR_SCHEMA, "exports": exports})
    return pair, elapsed


def command_inputs_ok(spec: dict, argv: list[str]) -> bool:
    pins = spec.get("command_artifact_pins", [])
    if not isinstance(pins, list) or len(pins) > 1024:
        return False
    expected = {}
    for pin in pins:
        if not isinstance(pin, dict) or not isinstance(pin.get("path"), str) or not Path(pin["path"]).is_absolute():
            return False
        path = Path(pin["path"]).resolve()
        if (path in expected or not path.is_file() or type(pin.get("bytes")) is not int
                or path.stat().st_size != pin["bytes"] or ac.digest(path) != pin.get("sha256")):
            return False
        expected[path] = pin
    # Pin scripts/configs handed to a generic interpreter or binary. Hashing
    # /usr/bin/python alone does not bind the verification implementation.
    original = spec["argv"]
    for literal, argument in zip(original[1:], argv[1:]):
        if literal.startswith("{") and literal.endswith("}"):
            continue  # Fresh internally pinned scope/results are dynamic inputs.
        path = Path(argument)
        if path.is_file() and path.resolve() not in expected:
            return False
    return True


def run_command(spec: dict, context: dict[str, str], log: Path, cancel_event=None) -> tuple[int, float, str | None]:
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
    if not command_inputs_ok(spec, argv):
        raise ValueError("command script/config/dependency differs from predeclared pins")
    start = time.monotonic()
    reason = None
    with log.open("xb") as out:
        p = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             start_new_session=True, env={**os.environ, "CARGO_INCREMENTAL": "0"})
        written = 0
        pipe_open = True
        while p.poll() is None or pipe_open:
            if cancel_event is not None and cancel_event.is_set():
                reason = "COMMAND_CANCELLED"
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
    if ac.digest(exe) != spec["executable_sha256"] or not command_inputs_ok(spec, argv):
        reason = "EXECUTABLE_OR_DEPENDENCY_MUTATED_DURING_RUN"
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
        resolution_error = None
        try:
            member, source_resolution = resolve_original_member(member, entry, plan_path.parent)
        except (OSError, ValueError, KeyError, TypeError) as e:
            source_resolution, resolution_error = None, f"ORIGINAL_SOURCE_RESOLUTION_FAILED:{type(e).__name__}"
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
        if source_resolution is not None:
            resolution = folder / "original-source-resolution.json"
            resolution.write_bytes(Path(source_resolution["path"]).read_bytes())
            if ac.digest(resolution) != source_resolution["sha256"]:
                resolution_error = "ORIGINAL_SOURCE_RESOLUTION_MUTATED"
            row["pinned_source_task_resolution_evidence"] = {"path": str(resolution.relative_to(out)), "sha256": ac.digest(resolution)}
        ac.write(folder / "scope.json", scope)
        pins = valid_scope and resolution_error is None and source_pins_ok(member)
        blocked = not pins
        for stage in ac.STAGES:
            result = folder / f"{stage}.json"
            spec = entry.get("stages", {}).get(stage)
            record = {"status": "NOT_RUN", "scope": scope, "blocking": "UPSTREAM_FAILURE"}
            if not valid_scope:
                record["blocking"] = "FULL_SCOPE_PLAN_UNRESOLVED"
            elif not pins:
                record["blocking"] = resolution_error or "MISSING_OR_MISMATCHED_ORIGINAL_SOURCE"
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
                local_only = why == "LOCAL_QUALIFIED_VALIDATION_ONLY"
                if local_only:
                    row["local_qualified_validation_network"] = True
                record.update(status="LOCAL_VALIDATION" if local_only else "PASS" if good else "FAIL", blocking=why, returncode=code, elapsed_seconds=elapsed)
                if result.is_file() and result.stat().st_size <= MAX_JSON:
                    record["evidence"] = {"path": str(result.relative_to(out)), "sha256": ac.digest(result)}
                blocked = not (good or local_only)
            row["stages"][stage] = record
        # Detect source mutation even if a successful tool ignored input pins.
        if pins and not source_pins_ok(member):
            row["stages"]["source"].update(status="FAIL", blocking="SOURCE_MUTATED_DURING_RUN")
            blocked = True
        if all(row["stages"][s]["status"] in {"PASS", "LOCAL_VALIDATION"} for s in ac.STAGES[:ac.STAGES.index("register") + 1]):
            public = row.get("local_qualified_validation_network") is not True
            row.update(full_advertised_task=True, real_size=True, active_shipping_kernel=public, public_chain=public)
        rows.append(row)
        with (out / "observations.jsonl").open("a") as f:
            f.write(json.dumps(row, allow_nan=False) + "\n")
    panel = None
    window_status = {"status": "NOT_RUN", "blocking": "NO_TWO_WINDOW_NATIVE_DRIVER"}
    if plan.get("receipt_window_command") is not None:
        candidate = out / "receipt-window-pointers.json"
        try:
            spec = plan["receipt_window_command"]
            validate_auditors(spec.get("independent_audits"), plan)
            contract = plan["receipt_window_contract"]
            from native_node_provenance_v1 import collect, same_processes
            provenance_before_path = out / "receipt-node-provenance-before.json"
            provenance_before = collect(spec["independent_audits"], provenance_before_path)
            contract_path = out / "receipt-window-contract.json"
            ac.write(contract_path, contract)
            ctx = {"evidence_root": str(out.resolve()), "result": str(candidate.resolve()),
                   "run_plan_sha256": run_sha, "cohort": str(cohort_path.resolve()),
                   "receipt_window_contract": str(contract_path.resolve())}
            from native_receipt_live_acquisition_v2 import LiveReceiptCollectors
            with LiveReceiptCollectors(spec["independent_audits"], contract, ctx, out):
                code, _, error = run_command(spec, ctx, out / "receipt-windows.log")
                if code or error:
                    raise ValueError(error or "original receipt producer failed; cancel independent live acquisition")
            pointers = read_json(candidate) if candidate.is_file() else {}
            if (code or error or pointers.get("schema") != "misaka.rfc02.native-receipt-window-pointers.v1"
                    or pointers.get("stage") != "receipt-windows" or pointers.get("run_plan_sha256") != run_sha
                    or pointers.get("contract_sha256") != ac.digest(contract_path)
                    or pointers.get("cohort_sha256") != cohort_sha
                    or pointers.get("contract") != str(contract_path.relative_to(out))):
                raise ValueError(error or "original pointer-only native receipt driver failed")
            pair, _ = execute_audit_pair(spec["independent_audits"],
                {**ctx, "stage_result": str(candidate.resolve())}, out, "receipt-windows", out)
            rechecks = recheck_receipt_proof_pair(pair, out, spec["independent_audits"], contract_path)
            panel_doc = inspect_receipt_audit_pair(pair, out, contract, run_sha)
            provenance_after_path = out / "receipt-node-provenance-after.json"
            provenance_after = collect(spec["independent_audits"], provenance_after_path)
            if not same_processes(provenance_before, provenance_after):
                raise ValueError("independent local replay processes/databases changed during original receipt measurement")
            panel_doc["local_node_provenance"] = [{"path": str(p.relative_to(out)), "sha256": ac.digest(p)}
                                                   for p in (provenance_before_path, provenance_after_path)]
            panel_doc["local_qualified_validation_only"] = True
            panel_doc["public_operator_independence_proven"] = False
            panel_doc["fresh_independent_proof_rechecks"] = rechecks
            panel = out / "receipt-windows.json"
            ac.write(panel, panel_doc)
            local_only = (panel_doc.get("local_qualified_validation_network") is True or panel_doc.get("local_qualified_validation_only") is True)
            window_status = {"status": "LOCAL_VALIDATION" if local_only else "PASS",
                             "blocking": "LOCAL_QUALIFIED_VALIDATION_ONLY" if local_only else "PASS",
                             "audit_pair_sha256": ac.digest(pair), "proof_rechecks": rechecks}
            if local_only:
                panel = None  # Retain measured local panel; never count it as public A6/A8.
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
