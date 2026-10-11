#!/usr/bin/env python3
"""Small JSON helpers for the H1 runner (stdlib only).

    report.py model <models.json> <id>                         one model's row, as JSON
    report.py field <models.json> <id> <key>                   one field of it (empty when null)
    report.py preflight-summary <preflight.json> [<more.json>]  the L0/L1 verdict of preflight report(s), as JSON
    report.py merge <out.json> <key>=<file.json|literal> ...   write/extend a JSON object
"""
import json
import os
import sys


def model(path, mid):
    for m in json.load(open(path))["models"]:
        if m["id"] == mid:
            return m
    sys.exit(f"no model {mid} in {path}")


def preflight_summary(path):
    try:
        d = json.load(open(path))
    except Exception as e:  # an empty or partial report is a finding, not a pass
        return {"file": os.path.basename(path), "readable": False, "error": str(e)}
    v = d.get("verdict") or {}
    blockers = []
    for stage in ("convert", "register", "mine"):
        for b in (v.get(stage) or {}).get("blockers") or []:
            blockers.append({"stage": stage, **({k: b.get(k) for k in ("code", "arg", "what", "have", "need", "unit", "safe_paths") if k in b})})
    m = d.get("model") or {}
    feats = m.get("features") or []
    unsupported = [{"id": f.get("id"), "status": f.get("status"), "lowering": f.get("lowering"), "detail": f.get("detail")}
                   for f in feats if f.get("status") != "Supported" or f.get("lowering") != "Implemented"]
    adm = d.get("admission") or {}
    return {
        "file": os.path.basename(path), "readable": True,
        "input": (d.get("input") or {}).get("label"), "bytes_read": (d.get("input") or {}).get("bytes_read"),
        "depth": d.get("depth"), "network_daa": (d.get("network") or {}).get("daa"),
        "model_type": m.get("model_type"), "architectures": m.get("architectures"), "frontend_level": m.get("level"),
        "adapter": (m.get("adapter") or {}).get("id"), "features_used": len(feats), "features_not_supported": unsupported,
        "scope": d.get("scope"), "storage": [r.get("storage") for r in (d.get("storage") or {}).get("rows") or []],
        "weights_bytes": (d.get("source") or {}).get("weight_bytes"), "missing_shards": (d.get("source") or {}).get("missing_shards"),
        "artifact_estimate_bytes": (d.get("artifact") or {}).get("estimate_bytes"),
        "kernel": d.get("kernel"), "admission_verdict": adm.get("verdict"), "admission_gate": adm.get("gate"),
        "admission_gate_detail": adm.get("gate_detail"), "layout": adm.get("layout"),
        "seat_needed_bytes": (d.get("seat") or {}).get("needed_bytes"),
        "verdict": {k: (v.get(k) or {}).get("status") for k in ("convert", "register", "mine")},
        "unknown_because": {k: (v.get(k) or {}).get("unknown_because") for k in ("convert", "register", "mine") if (v.get(k) or {}).get("unknown_because")},
        "blockers": blockers, "notes": d.get("notes"),
    }


def main():
    op = sys.argv[1]
    if op == "model":
        print(json.dumps(model(sys.argv[2], sys.argv[3]), indent=1))
    elif op == "field":
        v = model(sys.argv[2], sys.argv[3]).get(sys.argv[4])
        print("" if v is None else v)
    elif op == "preflight-summary":
        out = [preflight_summary(p) for p in sys.argv[2:]]
        print(json.dumps(out if len(out) > 1 else out[0], indent=1))
    elif op == "merge":
        path = sys.argv[2]
        d = json.load(open(path)) if os.path.exists(path) else {}
        for kv in sys.argv[3:]:
            k, v = kv.split("=", 1)
            if os.path.isfile(v):
                try:
                    d[k] = json.load(open(v))
                except Exception:
                    d[k] = open(v, errors="replace").read()
            else:
                try:
                    d[k] = json.loads(v)
                except Exception:
                    d[k] = v
        with open(path + ".tmp", "w") as f:
            json.dump(d, f, indent=1)
            f.write("\n")
        os.replace(path + ".tmp", path)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
