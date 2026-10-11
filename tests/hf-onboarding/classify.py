#!/usr/bin/env python3
"""H1's failure classifier: every failure of the HF onboarding closed loop as one machine-readable record.

    classify.py add  --failures FILE --stage S --category C --code X [--command CMD] [--expected E] [--observed O]
                     [--revision R] [--sha SHA] [--fixture F] [--owner O] [--security TEXT] [--repro CMD] [--log FILE]
    classify.py auto --failures FILE --stage S --log FILE [the same optional fields]   # infer category/code from the log text
    classify.py show --failures FILE

`failures.json` is a list (schema misaka.h1.failure.v1). `auto` reads the codes the tools print (census/preflight classes, admission
codes, pack-verify statuses, CLI exit text) and maps them to the H1 categories; an unrecognised failure is
TEST_INFRASTRUCTURE_FAILED with the last lines quoted, never a pass. Stdlib only.
"""
import argparse
import json
import os
import re
import sys
import time

CATEGORIES = [
    "HF_ACCESS_FAILED", "HF_REVISION_OR_SOURCE_MISMATCH", "FRONTEND_REQUIRED", "TASK_UNSUPPORTED", "SEMANTICS_UNSUPPORTED",
    "KERNEL_EXTENSION_REQUIRED", "TIR_ADMISSION_REFUSED", "LAYOUT_OR_RESOURCE_REFUSED", "CONVERSION_FAILED", "CONFORMANCE_FAILED",
    "PACK_INCOMPLETE", "REGISTRATION_QUOTE_INVALID", "FUNDING_INSUFFICIENT", "SIGNATURE_OR_RELAY_FAILED",
    "CONSENSUS_REGISTRATION_REFUSED", "REGISTRY_STATE_MISMATCH", "BEACON_UNAVAILABLE", "G14_INCOMPLETE", "DA_UNAVAILABLE",
    "PANEL_CAPACITY_UNAVAILABLE", "INFERENCE_FAILED", "FINAL_OR_SETTLEMENT_FAILED", "TEST_INFRASTRUCTURE_FAILED",
]

# The default owner of a category (the brief's ownership table); a record may name another.
OWNER = {
    "HF_ACCESS_FAILED": "external (source access) / A for the reader", "HF_REVISION_OR_SOURCE_MISMATCH": "A",
    "FRONTEND_REQUIRED": "A", "TASK_UNSUPPORTED": "A (task profile) -> B", "SEMANTICS_UNSUPPORTED": "B",
    "KERNEL_EXTENSION_REQUIRED": "B", "TIR_ADMISSION_REFUSED": "A (lowering) / B (admission rule)",
    "LAYOUT_OR_RESOURCE_REFUSED": "C", "CONVERSION_FAILED": "A", "CONFORMANCE_FAILED": "C", "PACK_INCOMPLETE": "C",
    "REGISTRATION_QUOTE_INVALID": "C1 / Lead", "FUNDING_INSUFFICIENT": "H1 (test funding) / Lead (economics)",
    "SIGNATURE_OR_RELAY_FAILED": "C1", "CONSENSUS_REGISTRATION_REFUSED": "D", "REGISTRY_STATE_MISMATCH": "D",
    "BEACON_UNAVAILABLE": "D / Lead", "G14_INCOMPLETE": "D (node wiring) / B (plan)", "DA_UNAVAILABLE": "D",
    "PANEL_CAPACITY_UNAVAILABLE": "D / H1 (devnet sizing)", "INFERENCE_FAILED": "C3", "FINAL_OR_SETTLEMENT_FAILED": "D",
    "TEST_INFRASTRUCTURE_FAILED": "H1",
}
# A code may name a better owner than its category's default.
OWNER_BY_CODE = {"HF_FIT_OUT_OF_TOLERANCE": "A (lowering quality) / H1 (calibration policy)"}

# (regex over the log, category, code group or literal). First match wins; ordered from specific to generic.
RULES = [
    (r"\b(401|403)\b.*(huggingface|hf://)|GATED_ACCESS|gated repo", "HF_ACCESS_FAILED", "GATED_ACCESS"),
    (r"Revision Not Found|revision .* not found|RevisionNotFound|404.*resolve", "HF_REVISION_OR_SOURCE_MISMATCH", "REVISION_NOT_FOUND"),
    (r"(sha256|digest) mismatch|SOURCE_HASH_MISMATCH|source file .* does not match", "HF_REVISION_OR_SOURCE_MISMATCH", "SOURCE_HASH_MISMATCH"),
    (r"\b(MISSING_WEIGHTS|BASE_UNPINNED|CONFIG_MISSING)\b", "HF_ACCESS_FAILED", None),
    (r"\b(PARTIAL_TASK_ONLY|MODALITY_PROFILE_MISSING|TASK_MISMATCH|PROFILE_REQUIRED)\b", "TASK_UNSUPPORTED", None),
    (r"\b(TASK_UNKNOWN)\b", "TASK_UNSUPPORTED", None),
    (r"\b(ARCH_NEEDS_FEATURE)\b.*an adapter for", "FRONTEND_REQUIRED", "ARCH_NEEDS_FEATURE(adapter)"),
    (r"\b(NOT_RUN_PIPELINE_ADMISSION)\b", "TASK_UNSUPPORTED", None),
    (r"\b(ARCH_NEEDS_PRIMITIVE|ARCH_NEEDS_FEATURE|FEATURE_C|SEMANTICS_UNSUPPORTED)\b", "SEMANTICS_UNSUPPORTED", None),
    (r"\bKERNEL_EXTENSION_REQUIRED\b", "KERNEL_EXTENSION_REQUIRED", "KERNEL_EXTENSION_REQUIRED"),
    (r"\b(TENSOR_MISSING|TENSOR_SHAPE|CONFIG_KEY_UNREAD|TOKENIZER_MISSING|ARCH_REFUSED|ADAPTER_REFUSED|ADAPTER_UNCHECKED|"
     r"QUANT_DESCRIPTOR_MISSING|QUANT_NO_DESCRIPTOR|QUANT_REFUSED|FORMAT_UNSUPPORTED|CUSTOM_CODE_UNMODELLED|FRONTEND_REQUIRED)\b", "FRONTEND_REQUIRED", None),
    (r"\b(CLOSE_SIZE_OVER_CAP|COURT_COST_OVER_CEILING|COURT_COST_EXCEEDS_CEILING|TIR_EXCEEDS_CEILING|COURT_BUDGET|CLOSE_TOO_LARGE|CONTEXT_BOUND|SEAT_MEMORY_SHORT|SEAT_MEMORY|HEADER_TOO_LARGE|ADMISSION_EXCEEDS|BOUNDS_EXCEEDED|LAYOUT_REQUIRED|"
     r"RESOURCE_REFUSED|NoAdmissibleLayout|no layout)\b", "LAYOUT_OR_RESOURCE_REFUSED", None),
    (r"\b(TIR_PROGRAM_REFUSED|TIR_CLASS_REFUSED|TIR_CLASS_ID_IS_NOT_DERIVED|CLASS_NOT_ATTRIBUTABLE|TirNeedsItsFence|FAMILY_FENCE_CLOSED|"
     r"NOT_END_TO_END_CERTIFIED)\b", "TIR_ADMISSION_REFUSED", None),
    (r"\bPUBLIC_PROSECUTION_INCOMPLETE\b|\bKERNEL_NOT_ACTIVE\b", "G14_INCOMPLETE", None),
    (r"\bBEACON_UNAVAILABLE\b|WAITING_RANDOMNESS", "BEACON_UNAVAILABLE", None),
    # The pack builder's fit gate: the integer program held to the float reference's units, order and KL (corpus-v1 §9). A fit that
    # misses the tolerance is a measured property of the lowering, never an infrastructure failure.
    (r"outside the tolerance of its reference", "CONFORMANCE_FAILED", "HF_FIT_OUT_OF_TOLERANCE"),
    (r"EVIDENCE_FORGED|EVIDENCE_NOT_REPRODUCED|COMMITMENT_STALE|conformance .*FAIL|check .* FAILED", "CONFORMANCE_FAILED", None),
    (r"\bSKIPPED\b.*(strict|verified)|not VERIFIED|PACK_NOT_VERIFIED", "PACK_INCOMPLETE", None),
    (r"\b(REGISTRATION_DROPPED|DuplicateClass|not signed by the bond|difficulty is not a registrant's)\b", "CONSENSUS_REGISTRATION_REFUSED", None),
    (r"insufficient (mature )?funds|not enough funds|no spendable|no mature|InsufficientFunds|E-FUNDS", "FUNDING_INSUFFICIENT", None),
    (r"QuoteInconsistent|FeeAboveCap|terms changed|quote expired", "REGISTRATION_QUOTE_INVALID", None),
    (r"ChangeNotPayer|PayerIsNotThisKey|OwnerKeyMismatch|relay .* refused|E-OBJECT-REFUSED|signature", "SIGNATURE_OR_RELAY_FAILED", None),
    (r"(convert|lower|lowering|calibrat).*(error|failed|panicked)|ConvertError|LowerError", "CONVERSION_FAILED", None),
    (r"panicked at|thread '.*' panicked", "TEST_INFRASTRUCTURE_FAILED", "PANIC"),
]


def infer(text):
    for pat, cat, code in RULES:
        m = re.search(pat, text, re.IGNORECASE if cat.startswith("HF_") else 0)
        if m:
            return cat, code or (m.group(1) if m.groups() else m.group(0))
    return "TEST_INFRASTRUCTURE_FAILED", "UNCLASSIFIED"


def load(path):
    if os.path.exists(path):
        with open(path) as f:
            return json.load(f)
    return []


def save(path, rows):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(rows, f, indent=1)
        f.write("\n")
    os.replace(tmp, path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("op", choices=["add", "auto", "show"])
    ap.add_argument("--failures", required=True)
    for k in ("stage", "category", "code", "command", "expected", "observed", "revision", "sha", "fixture", "owner", "security", "repro",
              "log", "model"):
        ap.add_argument("--" + k)
    a = ap.parse_args()
    rows = load(a.failures)
    if a.op == "show":
        for r in rows:
            print(f"{r['category']:<32} {r['code']:<36} {r['stage']:<14} owner {r['owner']}")
        return 0
    text = ""
    if a.log and os.path.exists(a.log):
        with open(a.log, errors="replace") as f:
            text = f.read()
    if a.op == "auto":
        cat, code = infer(text)
        cat = a.category or cat
        code = a.code or code
    else:
        cat, code = a.category, a.code
    if cat not in CATEGORIES:
        print(f"unknown category {cat}", file=sys.stderr)
        return 2
    lines = [ln for ln in text.splitlines() if ln.strip()]
    keys = [ln for ln in lines if re.search(r"against the reference|outside the tolerance|error|failed|refused|panicked", ln, re.IGNORECASE)][-6:]
    tail = "\n".join(keys + ["--- tail ---"] + lines[-12:]) if keys else "\n".join(lines[-12:])
    rec = {
        "schema": "misaka.h1.failure.v1", "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "model": a.model, "stage": a.stage,
        "category": cat, "code": code, "command": a.command, "expected": a.expected, "observed": a.observed or tail,
        "model_revision": a.revision, "integration_sha": a.sha, "failing_fixture": a.fixture, "owner": a.owner or OWNER_BY_CODE.get(code) or OWNER[cat],
        "security_consequence": a.security, "reproduction": a.repro, "log": a.log,
    }
    rows.append(rec)
    save(a.failures, rows)
    print(json.dumps({"category": cat, "code": code, "owner": rec["owner"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
