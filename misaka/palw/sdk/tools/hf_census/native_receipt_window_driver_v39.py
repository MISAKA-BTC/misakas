#!/usr/bin/env python3
"""Owning39 receipt-window contract validation and pointer-only stage result.

The frozen contract fixes, BEFORE any producer work, every original Master
(complete source, manifest, pipeline, request, all component class/source/
profile/program/inventory/real-parameter/scope/resource sets and producer
operators) for both original windows. A future component job id is not
frozen: the offline Core accumulator fixes it at its first authentic Fund84
observation and never lets it change. No counts or success flags come from
producers; this driver records only own-node collection pointers.
Run only after the code-first phase has ended.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import acceptance_cohort as ac
import checkpoint_cohort_runner as runner

CONTRACT_SCHEMA = "misaka.rfc02.native-receipt-window-contract.v39"
POINTER_SCHEMA = "misaka.rfc02.native-receipt-window-pointers.v39"
HEX128 = set("0123456789abcdef")


def hex128(value, nonzero=True) -> str:
    if not isinstance(value, str) or len(value) != 128 or any(c not in HEX128 for c in value) or (nonzero and value == "0" * 128):
        raise ValueError("exact nonzero 64-byte original identity required")
    return value


def validate_contract(c: dict) -> None:
    windows = c.get("windows")
    sets = c.get("original_model_inventory_sets")
    if (c.get("schema") != CONTRACT_SCHEMA or not isinstance(windows, list) or len(windows) != 2
            or type(c.get("receipt_window_daa")) is not int or c["receipt_window_daa"] <= 0
            or type(c.get("minimum_independent_operators")) is not int or not 2 <= c["minimum_independent_operators"] <= 128
            or not isinstance(sets, dict) or not sets or len(sets) > 512):
        raise ValueError("two original receipt windows, independent participation and model denominators must be frozen")
    for key in ("original_native_policy", "original_final_policy", "original_queue_policy", "original_physical_service"):
        hex128(c.get(key))
    for name, root in sets.items():
        if not name or len(name) > 256:
            raise ValueError("original model member name")
        hex128(root)
    masters, jobs, prior = set(), set(), None
    for w in windows:
        span = w.get("end_daa", 0) - w.get("start_daa", 0)
        if (type(w.get("start_daa")) is not int or type(w.get("end_daa")) is not int or span < c["receipt_window_daa"]
                or prior is not None and w["start_daa"] < prior
                or type(w.get("issuance_bin_daa")) is not int or not 0 < w["issuance_bin_daa"] <= span // 2
                or span % w["issuance_bin_daa"] or span // w["issuance_bin_daa"] > 1024
                or type(w.get("minimum_commits_per_bin")) is not int or w["minimum_commits_per_bin"] <= 0
                or type(w.get("maximum_final_lag_daa")) is not int or not 0 < w["maximum_final_lag_daa"] <= w["issuance_bin_daa"]
                or not isinstance(w.get("intended"), list) or not w["intended"]):
            raise ValueError("sustained nonoverlapping original windows/bins/Final tail required")
        prior = w["end_daa"]
        members = set()
        for m in w["intended"]:
            if m.get("master") in masters or sets.get(m.get("member")) != m.get("inventory_set"):
                raise ValueError("an original Master repeats or changes its model's inventory set")
            for key in ("master", "requester_operator", "complete_source", "manifest", "pipeline", "request", "class_set",
                        "profile_set", "program_set", "inventory_set", "real_parameter_set", "maximum_scope_set",
                        "maximum_resource_set"):
                hex128(m.get(key))
            if (type(m.get("issuance_not_before")) is not int or type(m.get("issuance_deadline")) is not int
                    or not w["start_daa"] <= m["issuance_not_before"] < m["issuance_deadline"] <= w["end_daa"]
                    or not isinstance(m.get("components"), list) or not 1 <= len(m["components"]) <= 64):
                raise ValueError("original Master cadence/component denominator")
            masters.add(m["master"])
            members.add(m["member"])
            for i, comp in enumerate(m["components"]):
                if comp.get("stage") != i or comp.get("requester_operator") != m["requester_operator"] \
                        or comp.get("requester_operator") == comp.get("producer_operator"):
                    raise ValueError("ordered original stage/operator binding")
                known = comp.get("job_if_already_known")
                if known is not None:
                    hex128(known)
                    if known in jobs:
                        raise ValueError("a known original job repeats")
                    jobs.add(known)
        if members != set(sets):
            raise ValueError("every original model must remain in both window denominators")
        for b in range((w["end_daa"] - w["start_daa"]) // w["issuance_bin_daa"]):
            start = w["start_daa"] + b * w["issuance_bin_daa"]
            if sum(start <= m["issuance_not_before"] and m["issuance_deadline"] <= start + w["issuance_bin_daa"]
                   for m in w["intended"]) < w["minimum_commits_per_bin"]:
                raise ValueError("frozen intended Masters omit an issuance cadence bin")


def record(contract_path: Path, run_sha: str, evidence_root: Path, live_collection: Path, result: Path) -> None:
    contract = runner.read_json(contract_path)
    validate_contract(contract)
    evidence_root = evidence_root.resolve()
    if (result.exists() or not result.resolve().is_relative_to(evidence_root)
            or not live_collection.resolve().is_relative_to(evidence_root) or not live_collection.is_dir()):
        raise ValueError("fresh pointer result and own live39 collection inside the evidence root required")
    collection = runner.read_json(live_collection / "collection.json")
    if collection.get("schema") != "misaka.rfc02.native-live-receipt-collection.v39" or collection.get("raw_transport_only") is not True:
        raise ValueError("own live39 raw collection required")
    ac.write(result, {"schema": POINTER_SCHEMA, "stage": "receipt-windows-v39", "run_plan_sha256": run_sha,
        "cohort_sha256": contract["cohort_sha256"], "contract_sha256": ac.digest(contract_path),
        "contract": str(contract_path.resolve().relative_to(evidence_root)),
        "live_collection": str(live_collection.resolve().relative_to(evidence_root)),
        "points_stream": str((live_collection / "points-stream-v39.jsonl").resolve().relative_to(evidence_root)),
        "status": "NOT_RUN"})


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    for key in ("contract", "run-plan-sha256", "evidence-root", "live-collection", "result"):
        p.add_argument(f"--{key}", required=True)
    a = p.parse_args()
    record(Path(a.contract), a.run_plan_sha256, Path(a.evidence_root), Path(a.live_collection), Path(a.result))


if __name__ == "__main__":
    main()
