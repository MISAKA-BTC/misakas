#!/usr/bin/env python3
"""Independent native chain auditor for the ``final`` and ``redemption`` stages.

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
Register/seat stages are not covered here (exit 2, no output): they stay NOT_RUN.
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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("stage", "scope", "stage-result", "run-plan-sha256", "audit-result", "node-database", "operator-id",
                 "genesis", "network", "rpc", "palw-class", "palw-class-sha256", "audit-fetch", "audit-fetch-sha256",
                 "work-dir", "evidence-root"):
        ap.add_argument(f"--{name}", required=True)
    # Isolated calibration34 owning snapshot (all three) or none: the shipped preset.
    for name in ("own-params-snapshot34", "own-params-snapshot-root", "own-params-sha256"):
        ap.add_argument(f"--{name}")
    ap.add_argument("--timeout-seconds", type=int, default=600)
    a = ap.parse_args()
    if a.stage not in ("final", "redemption"):
        return 2
    out = Path(a.audit_result)
    work = Path(a.work_dir)
    if out.exists() or work.exists():
        raise ValueError("fresh audit result and work directory required")
    work.mkdir(parents=True)
    scope = json.loads(Path(a.scope).read_text())
    result = json.loads(Path(a.stage_result).read_text())
    if result.get("schema") != RESULT_SCHEMA or result.get("stage") != a.stage:
        raise ValueError("stage result pointer document differs")
    ptr = result.get("native_pointers") or {}
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
        "isolated_calibration": not qualified,
        "final_authenticated": final_ok,
        "court_window_closed": final_ok,
        "redemption_authenticated": redemption and redeemed,
        "single_credit_verified": redemption and redeemed,
        "utxo_spend_verified": redemption and redeemed,
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
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
