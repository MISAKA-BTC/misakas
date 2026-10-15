#!/usr/bin/env python3
"""Independent offline owning39 two-window audit (A6) with full denominators.

Each auditor runs its own pinned `palw-native-receipt-audit39` over its own
contiguous live39 transcript. The Core accumulator re-authenticates every
state, fixes component jobs at their first authentic Fund84, and reports
every frozen Master by name. This wrapper only checks pins and copies the
proof-derived conclusions; it never upgrades NOT_MET and never writes PASS.
A8 keeps the census D_all / D_complete denominators: the per-member results
here are inputs to it, never a reduced denominator.
"""
from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

import acceptance_cohort as ac
import checkpoint_cohort_runner as runner
from native_receipt_window_driver_v39 import POINTER_SCHEMA, validate_contract

REPORT_SCHEMA = "misaka.rfc02.proof-derived-receipt-windows.v39"


def audit(a) -> int:
    evidence = Path(a.evidence_root).resolve()
    out = Path(a.out)
    if out.exists() or not out.resolve().is_relative_to(evidence):
        raise ValueError("fresh audit output inside the evidence root required")
    pointer = runner.read_json(Path(a.pointer))
    if pointer.get("schema") != POINTER_SCHEMA or pointer.get("run_plan_sha256") != a.run_plan_sha256:
        raise ValueError("owning39 pointer-only stage result required")
    contract_path = runner.relative_file(evidence, pointer["contract"])
    if ac.digest(contract_path) != pointer.get("contract_sha256"):
        raise ValueError("frozen receipt39 contract mutated")
    contract = runner.read_json(contract_path)
    validate_contract(contract)
    if ac.digest(Path(a.receipt_audit)) != a.receipt_audit_sha256:
        raise ValueError("pinned receipt39 checker executable differs")
    points = runner.relative_file(evidence, pointer["points_stream"])
    report_path = out.with_suffix(".proof.json")
    command = [a.receipt_audit, "--network", a.network, "--genesis", a.genesis,
               "--own-params-snapshot39", a.own_params_snapshot39, "--own-params-snapshot-root", a.own_params_snapshot_root,
               "--own-params-sha256", a.own_params_sha256, "--contract", str(contract_path),
               "--contract-sha256", pointer["contract_sha256"], "--evidence-root", str(evidence),
               "--points", str(points), "--out", str(report_path)]
    if a.archive_dir:
        command += ["--archive-dir", a.archive_dir]
    done = subprocess.run(command, capture_output=True, text=True, timeout=a.timeout_seconds)
    if done.returncode != 0:
        ac.write(out, {"schema": "misaka.rfc02.receipt-windows-v39-stage.v1", "status": "REFUSED",
                       "reason": done.stderr.strip()[-4096:], "a6_two_windows": "NOT_MET"})
        return 1
    report = runner.read_json(report_path)
    if report.get("schema") != REPORT_SCHEMA or report.get("network") != a.network or report.get("genesis") != a.genesis:
        raise ValueError("independent receipt39 proof report identity differs")
    denominators = {w["start_daa"]: w["intended_masters"] for w in report["windows"]}
    expected = {w["start_daa"]: len(w["intended"]) for w in contract["windows"]}
    if denominators != expected:
        raise ValueError("a frozen intended Master was dropped from a window denominator")
    ac.write(out, {"schema": "misaka.rfc02.receipt-windows-v39-stage.v1", "stage": "receipt-windows-v39",
        "run_plan_sha256": a.run_plan_sha256, "contract_sha256": pointer["contract_sha256"],
        "proof_report": {"path": str(report_path.resolve().relative_to(evidence)), "sha256": ac.digest(report_path)},
        "a6_two_windows": report["a6_two_windows"], "a8_members": report["a8_members"],
        "windows": [{k: w[k] for k in ("start_daa", "end_daa", "intended_masters", "final_within_lag", "a6_window",
                                       "independent_operators", "queue_non_growing")} for w in report["windows"]],
        "local_qualified_validation_network": report.get("local_qualified_validation_network") is True})
    return 0


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    for key in ("pointer", "run-plan-sha256", "evidence-root", "network", "genesis", "own-params-snapshot39",
                "own-params-snapshot-root", "own-params-sha256", "receipt-audit", "receipt-audit-sha256", "out"):
        p.add_argument(f"--{key}", required=True)
    p.add_argument("--archive-dir")
    p.add_argument("--timeout-seconds", type=int, default=43200)
    raise SystemExit(audit(p.parse_args()))


if __name__ == "__main__":
    main()
