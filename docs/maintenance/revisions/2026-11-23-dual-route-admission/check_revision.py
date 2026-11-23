#!/usr/bin/env python3
"""Check the MAD2 document revision against its attached-source manifest.

This is a preservation/contract check, not a native or cryptographic test.
The input manifest is a provenance record, not a digital signature.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()
    revision = Path(__file__).resolve().parent
    root = revision.parents[3]
    manifest = json.loads((revision / "change-manifest.json").read_text())
    errors: list[str] = []
    counts = {"original_files": 0, "changed_files": 0, "unchanged_non_document_files": 0,
              "preserved_originals": 0, "historical_payloads": 0, "new_design_files": 0}

    def require(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    for item in manifest["original_files"]:
        path = item["path"]
        file = root / path
        counts["original_files"] += 1
        if not file.is_file():
            errors.append(f"Original file missing: {path}")
            continue
        current = file.read_bytes()
        require(sha(current) == item["after_sha256"], f"Revised digest mismatch: {path}")
        if item["changed"]:
            counts["changed_files"] += 1
            require(path.startswith("docs/") or path in {"AGENTS.md", "CLAUDE.md"},
                    f"Out-of-scope source edit: {path}")
            require(not path.startswith(("docs/history/", "docs/evidence/")),
                    f"Historical/evidence file edited: {path}")
            saved = revision / item["before_path"]
            if not saved.is_file():
                errors.append(f"Missing saved original: {path}")
                continue
            original = saved.read_bytes()
            require(sha(original) == item["before_sha256"], f"Saved original mismatch: {path}")
            counts["preserved_originals"] += 1
            if path.startswith("docs/adr-v2/"):
                marker = "## 原条文の完全収録（履歴）".encode()
            elif path.startswith("docs/rfc-v2/"):
                marker = "## 原条文の統合収録".encode()
            else:
                marker = b""
            if marker and marker in original:
                require(marker in current and current.split(marker, 1)[1] == original.split(marker, 1)[1],
                        f"Historical payload changed: {path}")
                counts["historical_payloads"] += 1
        else:
            require(item["before_sha256"] == item["after_sha256"], f"Invalid unchanged manifest: {path}")
            if not path.startswith("docs/") and path not in {"AGENTS.md", "CLAUDE.md"}:
                counts["unchanged_non_document_files"] += 1

    for item in manifest["new_design_files"]:
        file = root / item["path"]
        require(item["path"].startswith("docs/"), f"New file outside docs: {item['path']}")
        require(file.is_file() and sha(file.read_bytes()) == item["sha256"],
                f"New design file changed/missing: {item['path']}")
        counts["new_design_files"] += 1

    policy = json.loads((root / "docs/design/palw/models/mad2-admission-policy.json").read_text())
    require(policy["routes"] == ["KERNEL_AUTO", "BFT_DECISION"], "Unexpected admission routes")
    for key in ("runtime_enabled", "wire_ids_assigned", "native_implemented_here",
                "actual_bft_vote_performed", "qwen_approved_here"):
        require(policy[key] is False, f"Unperformed activation/action claimed: {key}")
    for key in ("mathematical_proof_required", "collision_theorem_required", "cost_proof_required",
                "cost_measurement_evidence_required", "utility_evaluation_evidence_required",
                "reason_or_evidence_attachment_required", "theorem_rule_field_required"):
        require(policy["bft"][key] is False, f"BFT proof/evidence requirement reintroduced: {key}")
    require(policy["bft"]["unknown_admission_assessments_allowed"] is True, "BFT cannot admit UNKNOWN")
    require(policy["bft"]["hard_technical_failure_may_be_overridden"] is False, "Technical failure bypass")
    require(policy["assurance"]["claims_bypass_panel_court_or_gate_c"] is False, "Claim gate bypass")
    require(policy["control"]["same_batch_stop_wins"] is True, "Stop priority absent")
    require(policy["control"]["yes_clears_stop"] is False, "YES clears independent stop")
    require(policy["compatibility"]["cryptographic_execution_validity_proof"] is False,
            "Cryptographic execution proof introduced")
    classes = policy["model_classes"]
    require(set(classes) == {"GENESIS", "ADDITIONAL", "FORK_DESIGNATED"}, "Model classifications changed")
    for key in ("bft_add", "bft_modify", "bft_remove", "bft_stop", "bft_resolve", "bft_rule_stop_applies"):
        require(classes["GENESIS"][key] is False, f"BFT can manage original genesis: {key}")
    require(policy["genesis_origin_assignable_after_chain_creation"] is False, "Post-genesis immunity assignment")
    require(classes["GENESIS"]["claim_validation_exempt"] is False, "Genesis claim verification waived")
    for name in ("ADDITIONAL", "FORK_DESIGNATED"):
        require(classes[name]["bft_stop"] is True and classes[name]["bft_resolve"] is True,
                f"Non-genesis model outside BFT management: {name}")
    require(classes["ADDITIONAL"]["automatic_origin_is_bft_managed"] is True, "Automatic model avoids BFT control")
    require(classes["FORK_DESIGNATED"]["bft_add"] is True, "BFT cannot add fork model")
    require(classes["FORK_DESIGNATED"]["option_market"] is False, "Fork model creates Option market")
    require(classes["FORK_DESIGNATED"]["miner_permille_of_worker_R"] == 1000, "Fork model loses core economics")
    require(classes["FORK_DESIGNATED"]["automatic_fork_designation"] is False, "Automatic fork designation")
    require("requested_model_class" in policy["bft"]["signed_context_fields"], "Economic class not signed")

    for kind, expected in (("rfc", 20), ("adr", 50)):
        registry = json.loads((root / f"docs/{kind}-v2/registry.json").read_text())
        require(len(registry["documents"]) == expected, f"Current {kind} inventory changed")
    updates = json.loads((revision / "editorial-checksum-updates.json").read_text())["updates"]
    registry = json.loads((root / "docs/adr-v2/registry.json").read_text())
    current_hashes = {"docs/adr-v2/" + row["file"]: row["authored_section_sha256"]
                      for row in registry["documents"]}
    for update in updates:
        require(current_hashes[update["path"]] == update["registered_after"],
                f"Current editorial metadata mismatch: {update['path']}")

    result = {"passed": not errors, "scope": "document-preservation-and-declared-policy-only",
              "base_commit": manifest["base_commit"], "counts": counts, "errors": errors,
              "not_verified": ["native implementation", "signature verification", "quorum execution",
                               "Qwen performance or hardness", "three-peer consensus", "public activation"]}
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
