#!/usr/bin/env python3
"""Frozen real-checkpoint evidence, separate from the header/shape census.

``freeze-hub`` selects from *every* D_all row, including inaccessible and unknown
models, before any execution. ``report`` retains every frozen member as failure
until all required, identically scoped stages have evidence. File hashes certify
the documentary inputs, not canonical-chain authentication or their truth.
Engineering panels cannot be promoted into population coverage. A supplied
L_files design must independently establish its full-task, readable-file frame
and digest de-duplication, including the named Ollama and HF catalogues.
"""
from __future__ import annotations

import argparse
from collections import Counter
import datetime as dt
import gzip
import hashlib
import heapq
import json
from pathlib import Path
import sys

from estimate import cp_lb

SCHEMA = "misaka.rfc02.real-checkpoint-cohort.v1"
STAGES = ("source", "lower", "weight_import", "fidelity", "execution",
          "ordinary_verification", "public_court", "register", "seat", "final", "redemption")
SCOPE = ("repository", "revision", "complete_task", "context_max", "storage",
         "arithmetic", "kernel", "verification_plan", "dispute_plan")


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def write(path: Path, doc: dict) -> None:
    with path.open("x") as f:
        json.dump(doc, f, indent=2, allow_nan=False)
        f.write("\n")


def freeze_hub(snapshot: Path, out: Path, n: int, seed: str) -> dict:
    if not 1 <= n <= 10000 or not seed:
        raise ValueError("positive bounded sample size and nonempty seed required")
    listing = snapshot / "dall.listing-v2.jsonl.gz"
    heap, N = [], 0
    with gzip.open(listing, "rt") as f:
        for line in f:
            row = json.loads(line)
            repo = row["id"]
            key = int.from_bytes(hashlib.blake2b(f"{seed}\0D_all\0{repo}".encode(), digest_size=16).digest(), "big")
            entry = (-key, -N, row)
            N += 1
            if len(heap) < n:
                heapq.heappush(heap, entry)
            elif entry[:2] > heap[0][:2]:
                heapq.heapreplace(heap, entry)
    expected = json.loads((snapshot / "dall.summary-v2.json").read_text())["repos"]
    if N != expected:
        raise ValueError("truncated/inconsistent census frame")
    members = [{"id": r["id"], "repository": r["id"], "revision": r.get("sha"),
                "advertised_task": r.get("pipeline_tag"), "listing": r}
               for _, _, r in sorted(heap, reverse=True)]
    doc = {"schema": SCHEMA, "predeclared_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
           "population": "D_all", "design": "SRS_WITHOUT_REPLACEMENT", "N": N,
           "n": len(members), "seed": seed, "inclusion_probability": len(members) / N,
           "selection": "smallest BLAKE2b-128(seed || NUL || D_all || NUL || repository); every listing row eligible",
           "snapshot": snapshot.name, "frame_complete": True,
           "frame_sources": [{"path": "MANIFEST.json", "sha256": digest(snapshot / "MANIFEST.json")},
                             {"path": listing.name, "sha256": digest(listing)}],
           "target_share": 0.90, "confidence": "one-sided95% Clopper-Pearson (conservative SRS without FPC)",
           "failure_policy": "unknown, inaccessible, unsupported, nonresponding and NOT_RUN remain failures; no substitution",
           "task_resolution_policy": "if the listing has no task, use complete task metadata from the same pinned source revision with hashed evidence; unresolved semantics fail, without removing the member",
           "required_stages": list(STAGES), "members": members}
    write(out, doc)
    return {"N": N, "n": len(members), "sha256": digest(out)}


def valid_design(doc: dict) -> None:
    if doc.get("schema") != SCHEMA or doc.get("design") not in ("SRS_WITHOUT_REPLACEMENT", "CENSUS", "ENGINEERING_PANEL"):
        raise ValueError("unsupported frozen cohort schema/design")
    members = doc.get("members")
    if not isinstance(members, list) or not members or len(members) > 10000:
        raise ValueError("bounded nonempty frozen members required")
    if type(doc.get("N")) is not int or type(doc.get("n")) is not int or not 0 < doc["n"] <= doc["N"] or doc["n"] != len(members):
        raise ValueError("frozen population/sample size mismatch")
    ids = [m.get("id") for m in members]
    if any(not isinstance(i, str) or not i for i in ids) or len(set(ids)) != len(ids):
        raise ValueError("unique frozen identities required")
    if doc["design"] == "CENSUS" and doc["n"] != doc["N"]:
        raise ValueError("a census cannot omit members")
    if doc.get("target_share") != 0.90 or doc.get("required_stages") != list(STAGES):
        raise ValueError("RFC02 current goal threshold/stages cannot be weakened")


def evidence_ok(e: dict, base: Path) -> bool:
    name, sha = e.get("path"), e.get("sha256")
    if not isinstance(name, str) or not isinstance(sha, str) or len(sha) != 64:
        return False
    path = Path(name)
    if path.is_absolute() or ".." in path.parts:
        return False
    path = base / path
    return path.is_file() and digest(path) == sha


def panel_ok(panel: dict, base: Path) -> bool:
    windows = panel.get("windows")
    rule = panel.get("minimum_independent_operators")
    if type(rule) is not int or rule < 2 or not isinstance(windows, list) or len(windows) < 2:
        return False
    prior_end = None
    for w in windows:
        start, end, duration = w.get("start_daa"), w.get("end_daa"), panel.get("receipt_window_daa")
        if (any(type(v) is not int for v in (start, end, duration)) or duration <= 0
                or start < 0 or end - start < duration or prior_end is not None and start < prior_end
                or w.get("independent_operators", 0) < rule
                or w.get("intended_claims", 0) <= 0 or w.get("issued_claims", -1) < w["intended_claims"]
                or w.get("finalized_claims", -1) < w["intended_claims"]
                or w.get("queue_end", -1) < 0 or w.get("queue_start", -1) < 0
                or w["queue_end"] > w["queue_start"] or w.get("unaccounted_drops") != 0
                or not evidence_ok(w.get("evidence", {}), base)):
            return False
        prior_end = end
    return True


def outcome(member: dict, row: dict | None, base: Path, cohort_sha: str) -> dict:
    key = member["id"]
    result = {"id": key, "registration_ready": False, "final_capable": False, "first_blocker": "NOT_RUN", "stages": {}}
    if row is None:
        return result
    scope = row.get("scope", {})
    if (row.get("cohort_sha256") != cohort_sha or any(scope.get(k) in (None, "", False) for k in SCOPE)
            or scope["repository"] != member["repository"] or scope["revision"] != member["revision"]
            or member.get("advertised_task") is not None and scope["complete_task"] != member["advertised_task"]
            or type(scope["context_max"]) is not int or scope["context_max"] <= 0
            or scope["context_max"] != row.get("source_declared_context_max")
            or row.get("full_advertised_task") is not True or row.get("real_size") is not True
            or row.get("active_shipping_kernel") is not True or row.get("public_chain") is not True):
        result["first_blocker"] = "SCOPE_OR_ACTIVE_ROUTE_UNPROVEN"
        return result
    if member.get("advertised_task") is None and not evidence_ok(row.get("pinned_source_task_resolution_evidence", {}), base):
        result["first_blocker"] = "UNRESOLVED_PINNED_SOURCE_TASK"
        return result
    first = None
    for stage in STAGES:
        e = row.get("stages", {}).get(stage, {})
        status = e.get("status", "NOT_RUN")
        if status not in ("PASS", "FAIL", "NOT_RUN"):
            raise ValueError(f"unknown stage status for {key}/{stage}")
        if status == "PASS" and (e.get("scope") != scope or not evidence_ok(e.get("evidence", {}), base)):
            status = "FAIL"
            e = {"blocking": "MISSING_OR_MISMATCHED_EVIDENCE"}
        result["stages"][stage] = status
        if status != "PASS" and first is None:
            first = e.get("blocking") or f"{stage}:{status}"
        if stage == "register":
            result["registration_ready"] = first is None
    result["final_capable"] = first is None
    result["first_blocker"] = first or "PASS"
    return result


def report(cohort: Path, observations: Path, base: Path, panel: Path | None = None) -> dict:
    doc = json.loads(cohort.read_text())
    valid_design(doc)
    ids = {m["id"] for m in doc["members"]}
    rows = {}
    with observations.open() as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            if r.get("id") not in ids or r["id"] in rows:
                raise ValueError("extraneous/duplicate observation; frozen membership cannot change")
            rows[r["id"]] = r
    sha = digest(cohort)
    results = [outcome(m, rows.get(m["id"]), base, sha) for m in doc["members"]]
    n = doc["n"]
    counts = {k: sum(r[k] for r in results) for k in ("registration_ready", "final_capable")}
    rates = {k: {"successes": x, "sample_denominator": n, "population_denominator": doc["N"],
                 "share": x / n, "estimated_population_successes": doc["N"] * x / n,
                 "lb95": x / n if doc["design"] == "CENSUS" else cp_lb(x, n)} for k, x in counts.items()}
    population_design = doc["design"] in ("SRS_WITHOUT_REPLACEMENT", "CENSUS")
    frame = doc.get("frame_complete") is True
    sustained = panel is not None and panel_ok(json.loads(panel.read_text()), base)
    common = population_design and frame and sustained and all(r["share"] >= .90 for r in rates.values())
    a6_confidence = all(r["lb95"] > .50 for r in rates.values())
    a8_confidence = rates["final_capable"]["lb95"] >= .90
    # L_files must come from readable complete weights/full-task metadata, not
    # mere advertised files. Rights-unknown units stay in that frame and fail source.
    local_frame = (doc.get("frame_eligibility") == "complete_public_weights_and_full_task_metadata"
                   and doc.get("digest_deduplication_complete") is True
                   and {"huggingface", "ollama"} <= set(doc.get("catalogues", [])))
    return {"schema": "misaka.rfc02.real-checkpoint-acceptance.v1", "cohort_sha256": sha,
            "observations_sha256": digest(observations), "population": doc.get("population"),
            "design": doc["design"], "rates": rates, "two_receipt_windows_passed": sustained,
            "A6_satisfied": common and a6_confidence and doc.get("population") == "D_all",
            "A8_satisfied": common and a8_confidence and doc.get("population") == "L_files" and local_frame,
            "thresholds": {"current_goal_share": .90, "A6_original_lb95_strictly_above": .50, "A8_lb95_minimum": .90},
            "missing_observations_count_as_failures": n - len(rows),
            "residual_table": dict(sorted(Counter(r["first_blocker"] for r in results).items())),
            "records": results,
            "assurance": "documentary scope and SHA-256 checks only; supplied chain/seat/window evidence still requires independent authentication"}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    commands = ap.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("freeze-hub")
    freeze.add_argument("--snapshot", type=Path, required=True)
    freeze.add_argument("--out", type=Path, required=True)
    freeze.add_argument("--n", type=int, default=128)
    freeze.add_argument("--seed", required=True)
    calc = commands.add_parser("report")
    for name in ("cohort", "observations", "evidence-root", "out"):
        calc.add_argument(f"--{name}", type=Path, required=True)
    calc.add_argument("--panel", type=Path)
    a = ap.parse_args()
    if a.command == "freeze-hub":
        print(json.dumps(freeze_hub(a.snapshot, a.out, a.n, a.seed)))
    else:
        r = report(a.cohort, a.observations, a.evidence_root, a.panel)
        write(a.out, r)
        print(json.dumps({k: v for k, v in r.items() if k != "records"}))


if __name__ == "__main__":
    main()
