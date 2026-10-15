#!/usr/bin/env python3
"""Independent native chain auditor for the ``register``, ``seat``, ``final`` and ``redemption`` stages.

Produces ``misaka.rfc02.independent-native-chain-audit.v3`` (the schema that
``checkpoint_cohort_runner.inspect_audit_pair`` requires) from ONE operator's own
independently replayed node. The producer's stage result supplies only pointers
(job, payment block, post block, spend); every fact is re-derived by:

1. ``palw-native-audit-fetch`` against the auditor's own node (selected-chain
   membership, readonly audit32 openings, coinbase33, post-payment payouts and
   the spend35 acceptance list), then
2. ``palw-class audit-native-final32`` offline, which re-derives the Final from
   pinned headers + authenticated rows, delivery33 and spend35.

A measurement Final (isolated calibration34 devnet) is written with
``qualified_native_profile = false`` / ``isolated_calibration = true``; the
runner and acceptance_cohort.py then keep it as a named failure for A6/A8.
Register and seat stages use ``palw-class audit-native-lifecycle32`` on a lifecycle-only
opening (class on a mature unrefuted source; job funded with the producer's consent).
This tool runs only in the consolidated validation phase.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

SCHEMA = "misaka.rfc02.independent-native-chain-audit.v3"
RESULT_SCHEMA = "misaka.rfc02.checkpoint-stage-result.v2"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def pinned_exe(path: str, digest: str) -> str:
    p = Path(path)
    if not p.is_absolute() or not p.is_file() or sha256(p) != digest:
        raise ValueError(f"executable {path} differs from its predeclared SHA256")
    return str(p)


def hex128(v) -> str:
    if not isinstance(v, str) or len(v) != 128 or any(c not in "0123456789abcdef" for c in v):
        raise ValueError("128-hex pointer required")
    return v


def run(argv: list[str], log: Path, timeout: int) -> None:
    with log.open("xb") as out:
        p = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT, timeout=timeout)
    if p.returncode:
        raise ValueError(f"{Path(argv[0]).name} refused (see {log.name})")


def owning_args(a) -> tuple[list[str], list[str], list[str]]:
    pins = (a.own_params_snapshot34, a.own_params_snapshot_root, a.own_params_sha256)
    if any(pins) and not all(pins):
        raise ValueError("own snapshot pins go together")
    if not all(pins):
        return [], [], []
    return (["--own-params-snapshot34", pins[0], "--own-params-snapshot-root", pins[1]],
            ["--own-params-sha256", pins[2]], ["--own-params-snapshot-sha256", pins[2]])


def lifecycle(a, scope: dict, ptr: dict, work: Path, out: Path) -> int:
    """Register (class on a mature unrefuted source) / seat (job funded with the
    producer's original consent), re-derived offline from the own node's opening."""
    master_mode=ptr.get("namespace")==32
    ident, child, parent, start = (hex128(ptr.get(k)) for k in
        (("master", "anchor_child", "anchor_selected_parent", "provenance_start") if master_mode else
         ("id", "anchor_child", "anchor_selected_parent", "provenance_start")))
    jobs=ptr.get("master_jobs", []) if master_mode else []
    if master_mode and (not isinstance(jobs,list) or len(jobs)>64 or len(set(jobs))!=len(jobs)
            or a.stage=="register" and jobs or a.stage=="seat" and not jobs):
        raise ValueError("Master32 original lifecycle stage-ordered jobs required")
    for job in jobs:
        hex128(job)
    fetch = pinned_exe(a.audit_fetch, a.audit_fetch_sha256)
    klass = pinned_exe(a.palw_class, a.palw_class_sha256)
    own, own_sha_fetch, own_sha_class = owning_args(a)
    collected = work / "collected"
    run([fetch, "--rpc", a.rpc, "--pinned-child", child, "--pinned-selected-parent", parent,
         "--pinned-provenance-start", start, "--job", ident, "--network", a.network, "--genesis", a.genesis,
         "--operator-id", a.operator_id, "--node-database-id", a.node_database, *own_sha_fetch, *own,
         "--lifecycle-only", "true", "--out", str(collected), "--timeout-seconds", "60"], work / "fetch.log", a.timeout_seconds)
    collection = json.loads((collected / "collection.json").read_text())
    if (collection.get("genesis") != a.genesis or collection.get("network") != a.network
            or collection.get("pinned_child_on_own_node_selected_chain") is not True):
        raise ValueError("own-node collection differs from the predeclared network/genesis/chain")
    report_path = work / "lifecycle-audit.json"
    run([klass, "audit-master-lifecycle32" if master_mode else "audit-native-lifecycle32", "--stage", a.stage, "--network", a.network, *own, *own_sha_class,
         "--child-header-json", str(collected / "child-header.json"),
         "--selected-parent-header-json", str(collected / "selected-parent-header.json"),
         "--pinned-child", child, "--pinned-selected-parent", parent, "--master" if master_mode else "--id", ident,
         *(["--jobs", ",".join(jobs)] if master_mode else []), "--aux-proof32", str(collected / "aux-proof32.borsh"), "--out", str(report_path)], work / "audit.log", a.timeout_seconds)
    report = json.loads(report_path.read_text())
    if master_mode:
        components=report.get("components", [])
        if (report.get("schema")!="misaka.palw.pinned-native-master-lifecycle-audit.v32" or report.get("master")!=ident
                or report.get("all_original_components_verified") is not True or not 1<=len(components)<=64
                or a.stage=="seat" and [c.get("job") for c in components]!=jobs):
            raise ValueError("complete original Master32 lifecycle proof refused")
        # Compatibility anchor remains one genuine component; aggregate bindings
        # have their own explicit names and never masquerade as an ordinary Fund.
        c=components[0]
        report={**report,"class":c["class"],"source":c["source"],"maximum_scope":c["maximum_scope"],
            "maximum_resource":c["maximum_resource"],"job":c["job"],"original_funding":c["original_funding"],
            "initial_consent":c["initial_consent"]}
    ok = report.get("pinned_headers_and_original_rows_verified") is True and report.get("stage") == a.stage
    seat_ok = a.stage == "register" or all(report.get(k) not in (None, "0" * 128) for k in ("job", "original_funding", "initial_consent"))
    qualified = report.get("qualified_native_profile") is True
    evidence_root = Path(a.evidence_root).resolve()
    if not work.resolve().is_relative_to(evidence_root) or not out.resolve().is_relative_to(evidence_root):
        raise ValueError("audit work directory and result must be inside the run's evidence root")
    objects = [{"path": str(p.resolve().relative_to(evidence_root)), "sha256": sha256(p)}
               for p in sorted(collected.iterdir()) + [report_path] if p.is_file()]
    child_header = json.loads((collected / "child-header.json").read_text())
    blank = lambda k: report.get(k) if a.stage == "seat" else ""
    doc = {
        "schema": SCHEMA, "scope": scope, "stage": a.stage, "run_plan_sha256": a.run_plan_sha256,
        "operator_id": a.operator_id, "node_database": a.node_database, "genesis": a.genesis, "network": a.network,
        "canonical_blocks_verified": collection.get("pinned_child_on_own_node_selected_chain") is True,
        # Rows exist only as native fold output; register = class bound to a mature unrefuted
        # source with its derived maximum scope/resources; seat = funded original consent.
        "native_carriers_verified": ok and seat_ok,
        "class_source_job_scope_verified": ok and seat_ok,
        "active_kernel_verified": ok,
        "resource_reservations_verified": ok and seat_ok,
        # Before Commit there is no seal/DA liability to verify; nothing is open by construction.
        "own_seal_and_public_da_verified": ok,
        "anchor_from_validated_consensus_database": True,
        "qualified_native_profile": qualified,
        "local_qualified_validation_network": report.get("local_qualified_validation_network") is True,
        "isolated_calibration": not qualified,
        "chain_objects": objects,
        "validated_anchor": {
            "network": a.network, "genesis": a.genesis, "anchor_block": child,
            "anchor_daa": int(child_header.get("daaScore", child_header.get("daa_score", -1))),
            "active_kernel": f"native/ns{report.get('namespace')}/{collection.get('node_consensus_params_id')}",
            "class": report.get("class"), "source": report.get("source"), "job": blank("job"),
            "maximum_scope": report.get("maximum_scope"), "actual_scope": "", "resource_contract": report.get("maximum_resource"),
            "original_funding": blank("original_funding"), "initial_consent": blank("initial_consent"),
            "claim": "", "final": "", "single_credit": "", "redemption_transaction": "", "redemption_utxo": "",
        },
    }
    if master_mode:
        doc["validated_master_anchor"]={k:report[k] for k in ("master","native_class_set","complete_source","manifest","complete_inventory","program_set","real_parameter_set","class_scope_set","class_resource_set","components")}
        doc["complete_model_lifecycle_evidence"]={"path":str(report_path.resolve().relative_to(evidence_root)),"sha256":sha256(report_path)}
    with out.open("x") as f:
        json.dump(doc, f, indent=2)
    return 0


def complete_model_identity(report: dict) -> dict | None:
    """Identity from the freshly executed offline proof verifier only.

    A Stage28 component never stands in for its complete original model. Master32
    needs its own checked composite report; until that adapter exists it remains
    explicitly unproven, including when one component has a genuine Final.
    """
    if (report.get("schema") != "misaka.palw.pinned-native-final-audit.v32"
            or report.get("namespace") != 29
            or report.get("pinned_headers_and_original_rows_verified") is not True):
        return None
    fields = {"native_class": "class", "native_source": "source", "program_root": "program_root",
              "inventory_root": "inventory", "real_parameter_root": "real_parameter_root"}
    identity = {}
    for key, source in fields.items():
        value = hex128(report.get(source))
        if value == "0" * 128:
            raise ValueError("offline complete-model identity has a zero original root")
        identity[key] = value
    return identity


def master(a, scope: dict, ptr: dict, work: Path, out: Path) -> int:
    """Every component is collected from this auditor's own DB, then matched to
    one independently verified original Master32 record. No new master credit.
    """
    from copy import copy
    evidence = Path(a.evidence_root).resolve()
    master_id, child, parent, start = (hex128(ptr.get(k)) for k in
        ("master", "master_child", "master_selected_parent", "provenance_start"))
    jobs = ptr.get("master_jobs")
    pointers = ptr.get("component_redemptions")
    if (not isinstance(jobs, list) or not 1 <= len(jobs) <= 64 or len(set(jobs)) != len(jobs)
            or not isinstance(pointers, list) or len(pointers) != len(jobs)):
        raise ValueError("every original Master32 component pointer required in stage order")
    for j in jobs:
        hex128(j)
    public = {}
    for k in ("master_outputs", "master_generated"):
        value = ptr.get(k)
        if not isinstance(value, str):
            raise ValueError("bounded original public Master32 witness path required")
        p = (Path(a.stage_result).parent / value).resolve()
        if not p.is_relative_to(evidence) or not p.is_file() or p.is_symlink():
            raise ValueError("Master32 witness escapes original evidence root")
        public[k] = p
    fetch = pinned_exe(a.audit_fetch, a.audit_fetch_sha256)
    klass = pinned_exe(a.palw_class, a.palw_class_sha256)
    own, own_fetch, own_class = owning_args(a)
    collected = work / "master-collected"
    run([fetch, "--rpc", a.rpc, "--pinned-child", child, "--pinned-selected-parent", parent,
        "--pinned-provenance-start", start, "--job", jobs[0], "--network", a.network, "--genesis", a.genesis,
        "--operator-id", a.operator_id, "--node-database-id", a.node_database, *own_fetch, *own,
        "--lifecycle-only", "true", "--out", str(collected), "--timeout-seconds", "60"],
        work / "master-fetch.log", a.timeout_seconds)
    collection = json.loads((collected / "collection.json").read_text())
    if (collection.get("network") != a.network or collection.get("genesis") != a.genesis
            or collection.get("pinned_child_on_own_node_selected_chain") is not True):
        raise ValueError("Master32 own validated chain pin changed")
    report_path = work / "master-proof-audit.json"
    run([klass, "audit-master-composite32", "--network", a.network, *own, *own_class,
        "--child-header-json", str(collected / "child-header.json"),
        "--selected-parent-header-json", str(collected / "selected-parent-header.json"),
        "--pinned-child", child, "--pinned-selected-parent", parent,
        "--aux-proof32", str(collected / "aux-proof32.borsh"), "--master", master_id,
        "--jobs", ",".join(jobs), "--outputs", str(public["master_outputs"]),
        "--generated-json", str(public["master_generated"]), "--out", str(report_path)],
        work / "master-proof-audit.log", a.timeout_seconds)
    report = json.loads(report_path.read_text())
    if (report.get("schema") != "misaka.palw.pinned-native-master-execution-audit.v32"
            or report.get("namespace") != 32 or report.get("master") != master_id
            or report.get("pinned_headers_and_original_rows_verified") is not True
            or report.get("all_original_component_finals_and_credits_verified") is not True
            or report.get("master_mints_extra_credit") is not False
            or [c.get("job") for c in report.get("components", [])] != jobs):
        raise ValueError("independent complete original Master32 proof refused")
    binding = report.get("complete_model_binding", {})
    required = {"native_class_set", "complete_source", "manifest", "complete_inventory", "program_set",
        "real_parameter_set", "maximum_scope_set", "maximum_resource_set"}
    if binding.get("namespace") != 32 or set(binding) != required | {"namespace"}:
        raise ValueError("explicit original Master32 set identity unavailable")
    for k in required:
        if hex128(binding[k]) == "0" * 128:
            raise ValueError("zero original Master32 binding")
    objects, component_evidence, docs = [], [], []
    # Cross-report equality is meaningful only after BOTH reports were freshly
    # produced by pinned proof checkers from the same own node's actual rows.
    fields = {"class":"class", "program":"program_root", "source":"source", "inventory":"inventory",
        "real_parameter":"real_parameter_root", "job":"job", "original_funding":"original_funding",
        "initial_consent":"initial_consent", "maximum_scope":"original_maximum_scope",
        "maximum_resource":"original_maximum_resource", "claim":"claim", "final_book":"final_book",
        "single_credit":"single_credit", "payout_queue":"payout_queue", "settlement":"settlement",
        "requester_operator":"requester_operator", "producer_operator":"producer_operator", "established_daa":"established_daa", "reward":"producer_credit"}
    for i, (job, pointer, original) in enumerate(zip(jobs, pointers, report["components"])):
        if not isinstance(pointer, dict) or pointer.get("job") != job:
            raise ValueError("original Master32 component job order substituted")
        folder = work / f"component-{i:03}"
        folder.mkdir()
        component_out = folder / "independent-final.json"
        sub = copy(a)
        # Final-only proof is useful before redemption; public launch requires
        # redemption stage and exact spend35 for EVERY component below.
        doc = ordinary_final(sub, scope, pointer, folder, component_out)
        proof_e = doc["complete_model_identity_evidence"]
        final_report = json.loads((evidence / proof_e["path"]).read_text())
        if (final_report.get("namespace") != 28 or final_report.get("network") != report.get("network")
                or final_report.get("genesis") != report.get("genesis")
                or doc.get("local_qualified_validation_network") != report.get("local_qualified_validation_network")
                or any(final_report.get(dst) != original.get(src) for src, dst in fields.items())):
            raise ValueError("component genuine Final/credit differs from original Master32 receipt")
        if a.stage == "redemption" and not all(doc.get(k) is True for k in
                ("redemption_authenticated", "single_credit_verified", "utxo_spend_verified")):
            raise ValueError("every original Master32 component must actually redeem its own credit")
        docs.append(doc); objects.extend(doc["chain_objects"]); component_evidence.append(proof_e)
    def artifact(p):
        return {"path": str(p.resolve().relative_to(evidence)), "sha256": sha256(p)}
    objects.extend(artifact(p) for p in sorted(collected.iterdir()) if p.is_file())
    objects.extend(artifact(p) for p in [report_path, *public.values()])
    qualified = report.get("qualified_native_profile") is True and report.get("final_receipt_class") == "qualified-final"
    if any(d.get("qualified_native_profile") != qualified for d in docs):
        raise ValueError("Master32 component owning qualification differs")
    doc = {**docs[0], "scope": scope, "complete_model_identity": report["complete_model_identity"],
        "complete_model_identity_status": "PROOF_DERIVED_COMPLETE_MASTER32",
        "complete_model_identity_evidence": artifact(report_path), "complete_model_binding": binding,
        "complete_model_binding_evidence": artifact(report_path), "component_redemption_evidence": component_evidence,
        "chain_objects": objects, "validated_anchor": docs[0]["validated_anchor"],
        "validated_master_anchor": {"namespace": 32, "master": master_id, "child": child,
            "selected_parent": parent, "composite": report["composite"],
            "single_credit_set": report["single_credit_set"], "complete_model_binding": binding,
            "components": report["components"]}}
    # Keep the ordinary compatibility anchor as a genuine component anchor.
    # Set roots live ONLY in explicit Master fields, never in single-job fields.
    with out.open("x") as f:
        json.dump(doc, f, indent=2)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("stage", "stage-result", "run-plan-sha256", "audit-result", "node-database", "operator-id",
                 "genesis", "network", "rpc", "palw-class", "palw-class-sha256", "audit-fetch", "audit-fetch-sha256",
                 "work-dir", "evidence-root"):
        ap.add_argument(f"--{name}", required=True)
    ap.add_argument("--scope")
    ap.add_argument("--receipt-audit")
    ap.add_argument("--receipt-audit-sha256")
    ap.add_argument("--receipt-collect")
    ap.add_argument("--receipt-collect-sha256")
    # Isolated calibration34 owning snapshot (all three) or none: the shipped preset.
    for name in ("own-params-snapshot34", "own-params-snapshot-root", "own-params-sha256"):
        ap.add_argument(f"--{name}")
    ap.add_argument("--timeout-seconds", type=int, default=600)
    a = ap.parse_args()
    if a.stage not in ("register", "seat", "final", "redemption", "receipt-windows"):
        return 2
    out = Path(a.audit_result)
    work = Path(a.work_dir)
    if out.exists() or work.exists():
        raise ValueError("fresh audit result and work directory required")
    work.mkdir(parents=True)
    result = json.loads(Path(a.stage_result).read_text())
    if a.stage == "receipt-windows":
        from native_receipt_window_audit_v1 import audit
        if not all((a.receipt_audit, a.receipt_audit_sha256, a.receipt_collect, a.receipt_collect_sha256)):
            raise ValueError("pinned independent receipt proof checker required")
        return audit(a, result, work, out)
    if not a.scope:
        raise ValueError("original lifecycle scope required")
    scope = json.loads(Path(a.scope).read_text())
    if result.get("schema") != RESULT_SCHEMA or result.get("stage") != a.stage:
        raise ValueError("stage result pointer document differs")
    ptr = result.get("native_pointers") or {}
    if a.stage in ("register", "seat"):
        return lifecycle(a, scope, ptr, work, out)
    if ptr.get("namespace") == 32:
        return master(a, scope, ptr, work, out)
    ordinary_final(a, scope, ptr, work, out)
    return 0


def ordinary_final(a, scope: dict, ptr: dict, work: Path, out: Path) -> dict:
    job, child, parent, start = (hex128(ptr.get(k)) for k in ("job", "final_child", "final_selected_parent", "provenance_start"))
    fetch = pinned_exe(a.audit_fetch, a.audit_fetch_sha256)
    klass = pinned_exe(a.palw_class, a.palw_class_sha256)
    pins = (a.own_params_snapshot34, a.own_params_snapshot_root, a.own_params_sha256)
    if any(pins) and not all(pins):
        raise ValueError("own snapshot pins go together")
    own = ["--own-params-snapshot34", pins[0], "--own-params-snapshot-root", pins[1]] if all(pins) else []
    own_sha_fetch = ["--own-params-sha256", pins[2]] if all(pins) else []
    own_sha_class = ["--own-params-snapshot-sha256", pins[2]] if all(pins) else []
    collected = work / "collected"
    argv = [fetch, "--rpc", a.rpc, "--pinned-child", child, "--pinned-selected-parent", parent,
            "--pinned-provenance-start", start, "--job", job, "--network", a.network, "--genesis", a.genesis,
            "--operator-id", a.operator_id, "--node-database-id", a.node_database, *own_sha_fetch,
            *own, "--out", str(collected), "--timeout-seconds", "60"]
    redemption = a.stage == "redemption"
    if redemption:
        post = hex128(ptr.get("post_header"))
        spend_chain = hex128(ptr.get("spend_chain_block"))
        spend_tx = Path(a.stage_result).parent / ptr["spend_tx_json"]
        argv += ["--redemption", "true", "--pinned-post-header", post, "--spend-chain-block", spend_chain,
                 "--spend-tx-json", str(spend_tx)]
    run(argv, work / "fetch.log", a.timeout_seconds)
    collection = json.loads((collected / "collection.json").read_text())
    if (collection.get("genesis") != a.genesis or collection.get("network") != a.network
            or collection.get("node_consensus_params_id") is None
            or collection.get("pinned_child_on_own_node_selected_chain") is not True):
        raise ValueError("own-node collection differs from the predeclared network/genesis/chain")
    report_path = work / "final-audit.json"
    argv = [klass, "audit-native-final32", "--network", a.network, *own, *own_sha_class,
            "--child-header-json", str(collected / "child-header.json"),
            "--selected-parent-header-json", str(collected / "selected-parent-header.json"),
            "--pinned-child", child, "--pinned-selected-parent", parent, "--job", job,
            "--aux-proof32", str(collected / "aux-proof32.borsh"), "--payout-proof32", str(collected / "payout-proof32.borsh"),
            "--raw-record-borsh", str(collected / "native-record.borsh"), "--out", str(report_path)]
    if redemption:
        index = ptr.get("payout_output_index")
        if type(index) is not int or index < 0:
            raise ValueError("payout output index pointer required")
        argv += ["--coinbase-proof33-borsh", str(collected / "native-coinbase-proof33.borsh"),
                 "--post-header-json", str(collected / "post-header.json"), "--pinned-post-header", post,
                 "--post-payout-proof32", str(collected / "post-payout-proof32.borsh"), "--payout-output-index", str(index),
                 "--accepted-spend35-json", str(collected / "native-accepted-spend35.json"),
                 "--spend-chain-header-json", str(collected / "spend-chain-header.json"), "--pinned-spend-chain", spend_chain,
                 "--spend-selected-parent-header-json", str(collected / "spend-selected-parent-header.json"),
                 "--pinned-spend-selected-parent", hex128(ptr.get("spend_selected_parent"))]
    run(argv, work / "audit.log", a.timeout_seconds)
    report = json.loads(report_path.read_text())
    identity = complete_model_identity(report)
    delivery = report.get("coinbase_delivery") or {}
    spend = delivery.get("accepted_spend") or {}
    qualified = report.get("qualified_native_profile") is True and report.get("final_receipt_class") == "qualified-final"
    final_ok = report.get("pinned_headers_and_original_rows_verified") is True and report.get("job") == job
    redeemed = (delivery.get("real_coinbase_inclusion_and_original_queue_consumption_verified") is True
                and spend.get("accepted_spend_of_original_outpoint_verified") is True
                and report.get("utxo_redemption_verified") is True)
    child_header = json.loads((collected / "child-header.json").read_text())
    evidence_root = Path(a.evidence_root).resolve()
    if not work.resolve().is_relative_to(evidence_root) or not out.resolve().is_relative_to(evidence_root):
        raise ValueError("audit work directory and result must be inside the run's evidence root")
    objects = [{"path": str(p.resolve().relative_to(evidence_root)), "sha256": sha256(p)}
               for p in sorted(collected.iterdir()) + [report_path] if p.is_file()]
    doc = {
        "schema": SCHEMA, "scope": scope, "stage": a.stage, "run_plan_sha256": a.run_plan_sha256,
        "operator_id": a.operator_id, "node_database": a.node_database, "genesis": a.genesis, "network": a.network,
        # Own independently replayed node; selected-chain membership checked by the fetch.
        "canonical_blocks_verified": collection.get("pinned_child_on_own_node_selected_chain") is True,
        # Rows exist only as native fold output under the owning fences re-derived by audit32.
        "native_carriers_verified": final_ok,
        "class_source_job_scope_verified": final_ok,
        "active_kernel_verified": final_ok,
        "resource_reservations_verified": final_ok,
        # An audited Final implies the original challenge window closed with no open own-seal/DA liability.
        "own_seal_and_public_da_verified": final_ok,
        "anchor_from_validated_consensus_database": True,
        "qualified_native_profile": qualified,
        "local_qualified_validation_network": report.get("local_qualified_validation_network") is True,
        "isolated_calibration": not qualified,
        "final_authenticated": final_ok,
        "court_window_closed": final_ok,
        "redemption_authenticated": redemption and redeemed,
        "single_credit_verified": redemption and redeemed,
        "utxo_spend_verified": redemption and redeemed,
        "complete_model_identity": identity,
        "complete_model_identity_status": "PROOF_DERIVED_TEXT29" if identity else "NOT_PROVEN_COMPLETE_MODEL",
        "complete_model_identity_evidence": {"path": str(report_path.resolve().relative_to(evidence_root)), "sha256": sha256(report_path)},
        "chain_objects": objects,
        "validated_anchor": {
            "network": a.network, "genesis": a.genesis, "anchor_block": child,
            "anchor_daa": int(child_header.get("daaScore", child_header.get("daa_score", -1))),
            "active_kernel": f"native-final27/ns{report.get('namespace')}/{collection.get('node_consensus_params_id')}",
            "class": report.get("class"), "source": report.get("source"), "job": report.get("job"),
            "maximum_scope": report.get("original_maximum_scope"), "actual_scope": report.get("actual_scope"),
            "resource_contract": report.get("original_maximum_resource"), "original_funding": report.get("original_funding"),
            "initial_consent": report.get("initial_consent"), "claim": report.get("claim"), "final": report.get("final_book"),
            "single_credit": report.get("single_credit"),
            "redemption_transaction": spend.get("spend_transaction", ""),
            "redemption_utxo": delivery.get("outpoint", ""),
        },
    }
    with out.open("x") as f:
        json.dump(doc, f, indent=2)
    return doc


if __name__ == "__main__":
    raise SystemExit(main())
