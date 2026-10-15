#!/usr/bin/env python3
"""Execute frozen native receipt work and retain pointer-only measurement output.

No counts or PASS flags are accepted from producers. The two independent auditors
fetch every own-chain audit32 state, then derive accepted originals/claims/Finals
and cumulative queue accounting. This driver never generates synthetic receipts.
Run only after the coordinated code-first phase has ended.
"""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import checkpoint_cohort_runner as runner
import acceptance_cohort as ac

SCHEMA = "misaka.rfc02.native-receipt-driver.v1"
POINTER_SCHEMA = "misaka.rfc02.native-receipt-window-pointers.v1"


def execute(plan_path: Path, contract_path: Path, committed_sha: str, run_sha: str,
            evidence_root: Path, result: Path) -> None:
    if ac.digest(plan_path) != committed_sha:
        raise ValueError("native receipt driver plan differs from pre-measurement committed SHA256")
    plan, contract = runner.read_json(plan_path), runner.read_json(contract_path)
    validate_contract(contract)
    if (plan.get("schema") != SCHEMA or plan.get("contract_sha256") != ac.digest(contract_path)
            or result.exists() or not result.resolve().is_relative_to(evidence_root.resolve())):
        raise ValueError("fresh native pointer result and original contract pins required")
    actions = plan.get("actions")
    intended = {j["job"] for w in contract["windows"] for j in w["intended"]}
    if (not isinstance(actions, list) or not actions or len(actions) > 8192
            or {s.get("intended_job") for s in actions} != intended):
        raise ValueError("native driver must retain every frozen intended original job")
    folder = evidence_root / "native-receipt-driver"
    folder.mkdir()  # fresh, including logs and immutable individual command pointers
    anchors = folder / "anchors.json"
    anchor_ctx = {"result": str(anchors.resolve()), "evidence_root": str(evidence_root.resolve()),
                  "run_plan_sha256": run_sha, "receipt_window_contract": str(contract_path.resolve())}
    # Start the real own-node clock observer BEFORE any producer can issue work.
    with ThreadPoolExecutor(max_workers=1) as pool:
        waiting = pool.submit(runner.run_command, plan["anchor_command"], anchor_ctx, folder / "anchors.log")
        for i, spec in enumerate(actions):
            output = folder / f"action-{i:05}.json"
            ctx = {"result": str(output.resolve()), "evidence_root": str(evidence_root.resolve()),
                   "run_plan_sha256": run_sha, "receipt_window_contract": str(contract_path.resolve()),
                   "intended_job": spec["intended_job"]}
            code, _, error = runner.run_command(spec, ctx, folder / f"action-{i:05}.log")
            if code or error:
                raise ValueError(error or f"original native action {i} failed")
        code, _, error = waiting.result()
    ptr = runner.read_json(anchors) if anchors.is_file() else {}
    if code or error or set(ptr) != {"start_child", "end_child", "end_selected_parent", "provenance_start"}:
        raise ValueError(error or "original sustained window boundary pointers missing")
    for value in ptr.values():
        if not isinstance(value, str) or len(value) != 128 or any(c not in "0123456789abcdef" for c in value):
            raise ValueError("exact native header pointers required")
    ac.write(result, {"schema": POINTER_SCHEMA, "stage": "receipt-windows", "run_plan_sha256": run_sha,
        "cohort_sha256": contract["cohort_sha256"], "contract_sha256": ac.digest(contract_path),
        "contract": str(contract_path.resolve().relative_to(evidence_root.resolve())), "native_pointers": ptr})


def validate_contract(c: dict) -> None:
    windows = c.get("windows")
    if (c.get("schema") != "misaka.rfc02.native-receipt-window-contract.v1" or not isinstance(windows, list)
            or len(windows) != 2 or type(c.get("receipt_window_daa")) is not int or c["receipt_window_daa"] <= 0
            or type(c.get("minimum_independent_operators")) is not int or not 2 <= c["minimum_independent_operators"] <= 128
            or not isinstance(c.get("original_model_roots"), dict) or not c["original_model_roots"]):
        raise ValueError("two original receipt windows and independent participation must be frozen")
    all_jobs, prior = set(), None
    keys = {"member", "job", "class", "source", "inventory", "real_parameter", "funding", "consent", "maximum_contract",
            "maximum_scope", "maximum_resource", "queue_plan", "queue_policy", "source_plan", "physical_service",
            "requester_operator", "producer_operator", "nonce", "issuance_not_before", "issuance_deadline"}
    for w in windows:
        if (set(w) != {"start_daa", "end_daa", "issuance_bin_daa", "minimum_commits_per_bin", "maximum_final_lag_daa", "intended"} or type(w["start_daa"]) is not int
                or type(w["end_daa"]) is not int or w["start_daa"] < 0
                or w["end_daa"] - w["start_daa"] < c["receipt_window_daa"]
                or prior is not None and w["start_daa"] < prior or not isinstance(w["intended"], list)
                or not 1 <= len(w["intended"]) <= 512
                or type(w["issuance_bin_daa"]) is not int or w["issuance_bin_daa"] <= 0
                or w["issuance_bin_daa"] > (w["end_daa"] - w["start_daa"]) // 2
                or (w["end_daa"] - w["start_daa"]) % w["issuance_bin_daa"]
                or type(w["minimum_commits_per_bin"]) is not int or w["minimum_commits_per_bin"] <= 0
                or type(w["maximum_final_lag_daa"]) is not int
                or not 0 < w["maximum_final_lag_daa"] <= w["issuance_bin_daa"]):
            raise ValueError("sustained original receipt window clocks/intended denominator differ")
        prior = w["end_daa"]
        for j in w["intended"]:
            if (set(j) != keys or j["job"] in all_jobs or c["original_model_roots"].get(j["member"]) != j["inventory"]
                    or j["requester_operator"] == j["producer_operator"]):
                raise ValueError("intended job denominator or original model/operator binding differs")
            if (type(j["issuance_not_before"]) is not int or type(j["issuance_deadline"]) is not int
                    or not w["start_daa"] <= j["issuance_not_before"] < j["issuance_deadline"] <= w["end_daa"]):
                raise ValueError("frozen original issuance opportunity/deadline required")
            for key in keys - {"member", "issuance_not_before", "issuance_deadline"}:
                value = j[key]
                if key == "funding" and value is None:
                    continue  # future native Fund is fixed by first authenticated observation
                if not isinstance(value, str) or len(value) != 128 or key != "nonce" and value == "0" * 128 or any(x not in "0123456789abcdef" for x in value):
                    raise ValueError("exact nonzero original native roots/operators/nonce required")
            all_jobs.add(j["job"])
        bins = (w["end_daa"] - w["start_daa"]) // w["issuance_bin_daa"]
        if bins > 1024:
            raise ValueError("receipt cadence bin cap")
        for i in range(bins):
            start = w["start_daa"] + i * w["issuance_bin_daa"]
            if sum(start <= j["issuance_not_before"] < j["issuance_deadline"] <= start + w["issuance_bin_daa"]
                   for j in w["intended"]) < w["minimum_commits_per_bin"]:
                raise ValueError("frozen intended work omits an issuance cadence bin")
    if len(all_jobs) > 512:
        raise ValueError("receipt window total intended job cap")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    for key in ("plan", "contract", "committed-plan-sha256", "run-plan-sha256", "evidence-root", "result"):
        p.add_argument(f"--{key}", required=True)
    a = p.parse_args()
    execute(Path(a.plan), Path(a.contract), a.committed_plan_sha256, a.run_plan_sha256, Path(a.evidence_root), Path(a.result))


if __name__ == "__main__":
    main()
