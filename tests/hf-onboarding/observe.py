#!/usr/bin/env python3
"""H1's observer: what several nodes of the devnet say about one class, and whether they agree.

    observe.py --class C --nodes A=PORT,B=PORT,C=PORT [--pin BLOCK] [--wait-pin 300] --out FILE

Per node (JSON wRPC, read-only): the DAG tip, the class-table row, the registry row, `getPalwModel`, the registration status, and
`getPalwStateProof` of the `classes` collection at ONE block every node holds (the pin: --pin, else the first node's sink once every
node has it). Agreement is asserted on what is chain state (class row, the registry's static fields, the state proof's whole answer at
the pin), never on node-local timing fields. Exit 0 all agree, 3 a disagreement (REGISTRY_STATE_MISMATCH), 2 a node did not answer.
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.join(os.environ.get("H1_WT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."), "audit-tir"))
import rpc  # noqa: E402

# Registry-row fields that move with the tip or with node-local measurement — not compared.
VOLATILE = {"readySeatsNow", "inflightNow", "panelRoom", "capUtilizationPermille", "admissionMilli", "expectedForwardsQ32", "reason",
            "noCapablePanelVoids", "utilizationPermille", "inflightClaims", "tipDaa", "readySeats"}


def compact_proof(sp):
    import hashlib
    if not isinstance(sp, dict) or sp.get("_error"):
        return sp
    return {"available": sp.get("available"), "reason": sp.get("reason"), "blockHash": sp.get("blockHash"), "collection": sp.get("collection"),
            "rows": len(sp.get("rows") or []), "answer_sha256": hashlib.sha256(json.dumps(sp, sort_keys=True).encode()).hexdigest()}


def ask(port, method, params):
    try:
        return rpc.call(port, method, params, timeout=60)
    except Exception as e:  # a node that does not answer is a finding of its own
        return {"_error": str(e)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--class", dest="cid", required=True)
    ap.add_argument("--nodes", required=True)
    ap.add_argument("--pin")
    ap.add_argument("--wait-pin", type=int, default=300)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    nodes = [(n.split("=")[0], int(n.split("=")[1])) for n in a.nodes.split(",")]
    pin = a.pin or ask(nodes[0][1], "getBlockDagInfo", {}).get("sink")
    # Every node must hold the pin before the state at it is compared.
    t0 = time.time()
    while True:
        have = [n for n, p in nodes if not ask(p, "getBlock", {"hash": pin, "includeTransactions": False}).get("_error")]
        if len(have) == len(nodes) or time.time() - t0 > a.wait_pin:
            break
        time.sleep(5)
    out = {"schema": "misaka.h1.consensus-state.v1", "class_id": a.cid, "pin": pin, "nodes_holding_pin": have, "observed_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "nodes": {}}
    for n, p in nodes:
        dag = ask(p, "getBlockDagInfo", {})
        classes = ask(p, "getPalwClasses", {})
        reg = ask(p, "getPalwModelRegistry", {})
        row = next((c for c in classes.get("classes", []) if c.get("classId") == a.cid), None)
        rrow = next((c for c in reg.get("classes", []) if c.get("classId") == a.cid), None)
        out["nodes"][n] = {
            "port": p, "virtual_daa": dag.get("virtualDaaScore"), "sink": dag.get("sink"), "error": dag.get("_error"),
            "class_row": row, "registry_row": rrow,
            "model": ask(p, "getPalwModel", {"classId": a.cid}),
            "registration_status": ask(p, "getPalwModelRegistrationStatus", {"classId": a.cid, "objectId": "", "transactionId": ""}),
            "state_proof_classes_at_pin": ask(p, "getPalwStateProof", {"blockHash": pin, "collection": "classes"}),
        }
    names = [n for n, _ in nodes]
    ref = out["nodes"][names[0]]
    agree = {}
    for n in names[1:]:
        o = out["nodes"][n]
        stat = lambda r: {k: v for k, v in (r or {}).items() if k not in VOLATILE}
        agree[f"{names[0]}~{n}"] = {
            "class_row": ref["class_row"] == o["class_row"],
            "registry_static": stat(ref["registry_row"]) == stat(o["registry_row"]),
            "state_proof_at_pin": ref["state_proof_classes_at_pin"] == o["state_proof_classes_at_pin"] and not ref["state_proof_classes_at_pin"].get("_error"),
        }
    out["agreement"] = agree
    # The raw proof is compared above; the evidence keeps its digest and shape (the rows are kilobytes of bytes per collection).
    for n in names:
        sp = out["nodes"][n]["state_proof_classes_at_pin"]
        out["nodes"][n]["state_proof_classes_at_pin"] = compact_proof(sp)
    out["class_present_everywhere"] = all(out["nodes"][n]["class_row"] is not None for n in names)
    out["all_agree"] = out["class_present_everywhere"] and all(all(v.values()) for v in agree.values())
    with open(a.out, "w") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
    print(json.dumps({"pin": (pin or "")[:16], "present": out["class_present_everywhere"], "agreement": agree, "all_agree": out["all_agree"]}))
    if any(out["nodes"][n]["error"] for n in names):
        return 2
    return 0 if out["all_agree"] else 3


if __name__ == "__main__":
    sys.exit(main())
