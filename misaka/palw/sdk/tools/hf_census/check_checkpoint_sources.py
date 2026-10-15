#!/usr/bin/env python3
"""Stream-check a predeclared checkpoint panel without loading tensor payloads.

Weights are fully hashed; safetensors headers are bounded and extents validated.
This establishes local byte integrity against the frozen documentary pins only.
It does not authenticate a publisher, execute a model, confirm use rights, or
populate any downstream acceptance gate. Missing projection/adapter files stay
visible even when the decoder's GGUF file is present.
"""
import argparse
import datetime as dt
import json
from pathlib import Path
import sys
import time

import acceptance_cohort as ac

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tir/lower/tools"))
import hf_checkpoint_reference as reference


def check(cohort: Path) -> dict:
    doc = json.loads(cohort.read_text())
    ac.valid_design(doc)
    out = []
    started = time.monotonic()
    for m in doc["members"]:
        source = Path(m["source_directory"])
        files = []
        for pin in m["source_files"]:
            relative = Path(pin["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("unique relative source path required")
            path = source / relative
            r = {"path": pin["path"], "bytes": pin["bytes"], "expected_sha256": pin["sha256"]}
            if not path.is_file():
                r.update(status="FAIL", blocking="MISSING_SOURCE_FILE")
            elif path.stat().st_size != pin["bytes"]:
                r.update(status="FAIL", blocking="SOURCE_LENGTH_MISMATCH", actual_bytes=path.stat().st_size)
            else:
                sha = ac.digest(path)
                r.update(actual_sha256=sha, status="PASS" if sha == pin["sha256"] else "FAIL")
                if r["status"] != "PASS":
                    r["blocking"] = "SOURCE_DIGEST_MISMATCH"
            files.append(r)
        parameter_bytes = None
        header_verdict = "NOT_RUN"
        pins = [p for p in m["source_files"] if p["path"].endswith(".safetensors")]
        if pins and all(r["status"] == "PASS" for r in files):
            try:
                parameter_bytes = reference.model_bytes(source, pins, 128 << 30)
                header_verdict = "PASS"
            except ValueError as e:
                header_verdict = f"FAIL:{e}"
        out.append({"id": m["id"], "repository": m["repository"], "revision": m["revision"],
                    "source_files": files, "local_byte_integrity": "PASS" if all(r["status"] == "PASS" for r in files) else "FAIL",
                    "bounded_safetensors_headers": header_verdict, "float32_parameter_payload_bytes": parameter_bytes,
                    "complete_advertised_task": m["advertised_task"], "source_declared_context_max": m["source_declared_context_max"],
                    "publisher_revision_independently_authenticated": False,
                    "weight_import_execution_fidelity_final_or_a6_a8_credit": False})
        print(f"source integrity {m['id']}: {out[-1]['local_byte_integrity']}", file=sys.stderr, flush=True)
    return {"schema": "misaka.rfc02.checkpoint-source-integrity.v1", "observed_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
            "cohort_sha256": ac.digest(cohort), "elapsed_seconds": time.monotonic() - started,
            "source_bytes_fully_hashed": sum(r["bytes"] for m in out for r in m["source_files"] if "actual_sha256" in r),
            "records": out, "coverage_credit": False, "full_rfc02_complete": False}


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cohort", type=Path)
    ap.add_argument("out", type=Path)
    args = ap.parse_args()
    if args.out.exists():
        raise ValueError("fresh output required")
    ac.write(args.out, check(args.cohort))
