"""Independent own-node collection + offline proof-based two-window audit.

Each auditor fetches the complete contiguous selected-parent transcript itself.
The producer supplies only boundary pointers. No producer window counts, queue
sizes, operators or intended-claim success flags enter the offline checker.
"""
from pathlib import Path
import json
import native_chain_audit_v3 as common
import checkpoint_cohort_runner as runner
import acceptance_cohort as ac
from native_receipt_window_driver_v1 import POINTER_SCHEMA, validate_contract


def audit(a, result: dict, work: Path, out: Path) -> int:
    evidence = Path(a.evidence_root).resolve()
    if not work.resolve().is_relative_to(evidence) or not out.resolve().is_relative_to(evidence):
        raise ValueError("fresh independent audit work and output must remain in evidence root")
    if (result.get("schema") != POINTER_SCHEMA or result.get("stage") != "receipt-windows"
            or result.get("run_plan_sha256") != a.run_plan_sha256):
        raise ValueError("original pointer-only window stage required")
    contract_path = runner.relative_file(evidence, result["contract"])
    if ac.digest(contract_path) != result.get("contract_sha256"):
        raise ValueError("original receipt-window intent mutated")
    contract = runner.read_json(contract_path)
    validate_contract(contract)
    if contract["cohort_sha256"] != result.get("cohort_sha256"):
        raise ValueError("receipt denominator changed")
    ptr = result["native_pointers"]
    start, end, parent, provenance = (common.hex128(ptr.get(k)) for k in
        ("start_child", "end_child", "end_selected_parent", "provenance_start"))
    if provenance != start:
        raise ValueError("complete transcript must start at original first boundary")
    fetch = common.pinned_exe(a.audit_fetch, a.audit_fetch_sha256)
    checker = common.pinned_exe(a.receipt_audit, a.receipt_audit_sha256)
    own, own_sha_fetch, own_sha_checker = common.owning_args(a)
    ident = contract["windows"][0]["intended"][0]["job"]

    def collect(child, selected_parent, from_hash, folder, log, maximum):
        common.run([fetch, "--rpc", a.rpc, "--pinned-child", child,
            "--pinned-selected-parent", selected_parent, "--pinned-provenance-start", from_hash,
            "--job", ident, "--network", a.network, "--genesis", a.genesis,
            "--operator-id", a.operator_id, "--node-database-id", a.node_database,
            *own_sha_fetch, *own, "--lifecycle-only", "true", "--max-blocks", str(maximum),
            "--out", str(folder), "--timeout-seconds", "60"], log, a.timeout_seconds)
        c = runner.read_json(folder / "collection.json")
        if (c.get("network") != a.network or c.get("genesis") != a.genesis
                or c.get("operator_id") != a.operator_id or c.get("node_database") != a.node_database
                or c.get("pinned_child") != child or c.get("pinned_selected_parent") != selected_parent
                or c.get("pinned_child_on_own_node_selected_chain") is not True
                or c.get("complete_requested_provenance_retained") is not True
                or c.get("qualified_native_profile") is not True or c.get("measurement_final_only") is not False):
            raise ValueError("own validated node lacks original contiguous qualified chain/state")
        for artifact in c.get("artifacts", []):
            if not ac.evidence_ok(artifact, folder):
                raise ValueError("independent collected artifact mutated")
        return c

    boundary = work / "boundary"
    collection = collect(end, parent, start, boundary, work / "boundary-fetch.log", 65536)
    blocks = sorted(boundary.glob("block-*.json"), reverse=True)
    if not 3 <= len(blocks) <= 65536:
        raise ValueError("complete sustained chain transcript unavailable/pruned/too large")
    points, objects, retained = [], [], 0
    for index, block_path in enumerate(blocks):
        block = runner.read_json(block_path)
        child = common.hex128(block["header"]["hash"])
        parent_pin = common.hex128(block["verboseData"]["selectedParentHash"])
        folder = boundary if child == end else work / f"point-{index:05}"
        if folder != boundary:
            collect(child, parent_pin, child, folder, work / f"point-{index:05}-fetch.log", 1)
        points.append({"child": child, "selected_parent": parent_pin,
                       "collected": str(folder.resolve().relative_to(evidence))})
        for p in sorted(folder.iterdir()):
            if p.is_file():
                retained += p.stat().st_size
                if retained > 32 << 30:
                    raise ValueError("independent complete receipt transcript exceeds 32 GiB cap")
                objects.append({"path": str(p.resolve().relative_to(evidence)), "sha256": ac.digest(p)})
    if points[0]["child"] != start or points[-1]["child"] != end:
        raise ValueError("own transcript does not cover original boundary pins")
    transcript = work / "points.json"
    ac.write(transcript, {"schema": "misaka.rfc02.receipt-state-transcript.v1", "points": points})
    report_path = work / "proof-audit.json"
    common.run([checker, "--network", a.network, *own, *own_sha_checker,
        "--contract", str(contract_path), "--evidence-root", str(evidence),
        "--points", str(transcript), "--out", str(report_path)], work / "proof-audit.log", a.timeout_seconds)
    report = runner.read_json(report_path)
    if (report.get("schema") != "misaka.rfc02.proof-derived-receipt-windows.v1"
            or report.get("network") != a.network or report.get("genesis") != a.genesis
            or report.get("qualified_native_profile") is not True
            or report.get("intended_claims_and_queue_accounting_verified") is not True):
        raise ValueError("independent proof checker refused original claims/queue/opportunity")
    # The same immutable proof-derived evidence is referenced by each public panel
    # window; neither a self-reference nor a producer-authored receipt is accepted.
    proof_evidence = {"path": str(report_path.resolve().relative_to(evidence)), "sha256": ac.digest(report_path)}
    panel = {k: report[k] for k in ("cohort_sha256", "original_model_roots", "receipt_window_daa", "minimum_independent_operators")}
    panel.update(schema="misaka.rfc02.proof-derived-receipt-panel.v1",
        local_qualified_validation_network=report.get("local_qualified_validation_network") is True, contract_root=report["contract_root"],
        contract_sha256=ac.digest(contract_path), windows=[{**w, "evidence": proof_evidence} for w in report["windows"]])
    objects.extend([proof_evidence, {"path": str(transcript.resolve().relative_to(evidence)), "sha256": ac.digest(transcript)},
                    {"path": result["contract"], "sha256": ac.digest(contract_path)}])
    doc = {"schema": common.SCHEMA, "stage": "receipt-windows", "run_plan_sha256": a.run_plan_sha256,
        "operator_id": a.operator_id, "node_database": a.node_database, "genesis": a.genesis, "network": a.network,
        "canonical_blocks_verified": True, "anchor_from_validated_consensus_database": True,
        "qualified_native_profile": True, "isolated_calibration": False,
        "local_qualified_validation_network": report.get("local_qualified_validation_network") is True,
        "intended_claims_and_queue_accounting_verified": True, "receipt_windows": panel,
        # Compare proof results without per-auditor filenames; all state roots,
        # operators, accepted original claims and cumulative counters must agree.
        "receipt_proof_result": report, "receipt_proof_evidence": proof_evidence,
        "receipt_points_evidence": {"path": str(transcript.resolve().relative_to(evidence)), "sha256": ac.digest(transcript)}, "chain_objects": objects,
        "node_consensus_params_id": collection["node_consensus_params_id"]}
    ac.write(out, doc)
    return 0
