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


def reverse_pointer_stream(source: Path, output: Path) -> int:
    """Reverse a potentially multi-day JSONL index with one bounded line resident.

    The collector emits child-to-parent. The offline verifier consumes the exact
    parent-to-child stream; no state is sampled or omitted to fit an array cap.
    """
    count = 0
    with source.open("rb") as src, output.open("xb") as dst:
        dst.write(b'{"schema":"misaka.rfc02.receipt-state-stream.v2"}\n')
        src.seek(0, 2)
        end = src.tell()
        if end == 0:
            raise ValueError("empty original receipt transcript")
        src.seek(end - 1)
        if src.read(1) != b"\n":
            raise ValueError("partial original receipt index line")
        pos, suffix = end - 1, b""
        while pos > 0:
            size = min(65536, pos)
            pos -= size
            src.seek(pos)
            block = src.read(size) + suffix
            lines = block.split(b"\n")
            suffix = lines[0]
            for line in reversed(lines[1:]):
                if not line or len(line) + 1 > 4096:
                    raise ValueError("original receipt pointer line bound")
                point = json.loads(line)
                if set(point) != {"child", "selected_parent", "collected"}:
                    raise ValueError("original pointer grammar differs")
                common.hex128(point["child"]); common.hex128(point["selected_parent"])
                dst.write(line + b"\n"); count += 1
            if len(suffix) > 4096:
                raise ValueError("original receipt pointer line bound")
        if not suffix or len(suffix) + 1 > 4096:
            raise ValueError("original receipt first pointer missing")
        point = json.loads(suffix)
        if set(point) != {"child", "selected_parent", "collected"}:
            raise ValueError("original pointer grammar differs")
        common.hex128(point["child"]); common.hex128(point["selected_parent"])
        dst.write(suffix + b"\n"); count += 1
    return count


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
    collector = common.pinned_exe(a.receipt_collect, a.receipt_collect_sha256)
    checker = common.pinned_exe(a.receipt_audit, a.receipt_audit_sha256)
    own, own_sha_fetch, own_sha_checker = common.owning_args(a)
    maximum = result.get("maximum_evidence_bytes")
    if type(maximum) is not int or not 1 <= maximum <= 1 << 40:
        raise ValueError("frozen complete transcript evidence reservation required")
    live = getattr(a, "receipt_live_collected", None)
    if live:
        boundary = Path(live).resolve()
        if not boundary.is_relative_to(evidence) or not boundary.is_dir():
            raise ValueError("independent live collection must remain inside original evidence root")
    else:
        boundary = work / "collected"
        common.run([collector, "--rpc", a.rpc, "--network", a.network, "--genesis", a.genesis,
            "--operator-id", a.operator_id, "--node-database-id", a.node_database, *own_sha_fetch, *own,
            "--start-child", start, "--end-child", end, "--end-selected-parent", parent,
            "--evidence-root", str(evidence), "--maximum-evidence-bytes", str(maximum),
            "--timeout-seconds", str(min(a.timeout_seconds, 43200)), "--out", str(boundary)],
            work / "collect.log", a.timeout_seconds)
    collection = runner.read_json(boundary / "collection.json")
    expected = {"schema": "misaka.rfc02.native-receipt-cas-collection.v1", "network": a.network,
        "genesis": a.genesis, "operator_id": a.operator_id, "node_database": a.node_database}
    if live:
        expected["acquisition_mode"] = "live-forward-v2"
        expected["stop_daa"] = max(w["end_daa"] + w["maximum_final_lag_daa"] for w in contract["windows"])
    else:
        expected.update(start_child=start, end_child=end, end_selected_parent=parent)
    if any(collection.get(k) != v for k, v in expected.items()):
        raise ValueError("own collector original transcript identity changed")
    # Transport metadata is not authority. Every CAS object and every consecutive
    # state in the canonical full-window interval is independently authenticated.
    transcript = work / "points.jsonl"
    if live:
        from native_receipt_live_acquisition_v2 import canonical_window_stream
        index = boundary / "points-stream-v2.jsonl"
        count = canonical_window_stream(index, transcript, evidence, contract)
        if type(collection.get("states")) is not int or count > collection["states"]:
            raise ValueError("independent canonical window interval exceeds full live transcript")
    else:
        index = boundary / "reverse-index.jsonl"
        count = reverse_pointer_stream(index, transcript)
        if count != collection.get("states"):
            raise ValueError("own original transcript was truncated")
    if count < 3:
        raise ValueError("own original transcript was truncated")
    objects = [{"path": str(p.resolve().relative_to(evidence)), "sha256": ac.digest(p)} for p in
        (boundary / "collection.json", boundary / "evidence-index.jsonl", index)]
    witness_args = []
    witness_evidence = result.get("master_witnesses")
    if any(w.get("masters") for w in contract["windows"]):
        if not ac.evidence_ok(witness_evidence, evidence):
            raise ValueError("frozen complete Master32 witnesses unavailable or mutated")
        witness_args = ["--master-witnesses", witness_evidence["path"]]
        objects.append(witness_evidence)
        for d in runner.read_json(runner.relative_file(evidence, witness_evidence["path"])):
            for key in ("outputs", "generated"):
                f = runner.relative_file(evidence, d[key])
                objects.append({"path": d[key], "sha256": ac.digest(f)})
    report_path = work / "proof-audit.json"
    common.run([checker, "--network", a.network, *own, *own_sha_checker,
        "--contract", str(contract_path), "--evidence-root", str(evidence),
        "--points", str(transcript), "--points-format", "stream-v2", *witness_args, "--out", str(report_path)], work / "proof-audit.log", a.timeout_seconds)
    report = runner.read_json(report_path)
    if (report.get("schema") != "misaka.rfc02.proof-derived-receipt-windows.v2"
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
        "receipt_master_witnesses_evidence": witness_evidence, "receipt_points_format": "stream-v2",
        "receipt_points_evidence": {"path": str(transcript.resolve().relative_to(evidence)), "sha256": ac.digest(transcript)}, "chain_objects": objects,
        "node_consensus_params_id": collection["node_consensus_params_id"]}
    ac.write(out, doc)
    return 0
