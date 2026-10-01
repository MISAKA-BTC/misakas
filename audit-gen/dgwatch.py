#!/usr/bin/env python3
"""audit-gen/dgwatch.py — the generative drill's read-only RPC helper (RFC-0003 step 8). Stdlib only; one JSON-wRPC
call per request through audit-tir/rpc.py to a drill node's loopback JSON port. Nothing here changes node state.

  dgwatch.py --port P anchor                       print "<sink hash> <its DAA>": the anchor a gen-claim request names
  dgwatch.py --port P tip                          print the virtual DAA score
  dgwatch.py --port P claims --bond TXID:I [--class CLASSID]
                                                   one JSON line: the bond's executor claims by phase (final, convicted,
                                                   voided, open) and the void reasons seen
  dgwatch.py --port P wait --daa N [--bond B --class C --final K | --convicted K] [--deadline-daa D]
                                                   poll every 20 s until the DAA reaches N, or the bond's claims of the
                                                   class reach K Final / K convicted; exit 0 reached, 3 deadline passed
  dgwatch.py --port P class --class CLASSID        print whether the chain's class table lists the class (and its status)

A claim a court convicted reads CourtFraud / CourtDefault / CourtHeldVerdict in its phase or void reason."""
import argparse, json, os, sys, time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "audit-tir"))
from rpc import call, pick  # noqa: E402

CONVICTED = ("CourtFraud", "CourtDefault", "CourtHeldVerdict")


def tip(port):
    return int(pick(call(port, "getBlockDagInfo", {}), "virtualDaaScore", default=0) or 0)


def anchor(port):
    dag = call(port, "getBlockDagInfo", {})
    sink = str(pick(dag, "sink", default="") or "")
    daa = tip(port)
    blk = call(port, "getBlock", {"hash": sink, "includeTransactions": False})
    header = pick(pick(blk, "block", default={}), "header", default={}) or {}
    own = pick(header, "daaScore", "daa_score")
    return sink, int(own) if own is not None else daa


def claims_of(port, bond, class_id=None):
    r = call(port, "getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0})
    counts = {"final": 0, "convicted": 0, "voided": 0, "open": 0}
    reasons, rows = {}, []
    for c in pick(r, "claims", default=[]) or []:
        if class_id and pick(c, "classId") not in (None, class_id):
            continue
        phase, void = str(pick(c, "phase", default="")), str(pick(c, "voidReason", default="") or "")
        if phase.startswith("Final"):
            counts["final"] += 1
        elif any(x in phase or x in void for x in CONVICTED):
            counts["convicted"] += 1
        elif phase.startswith("Voided"):
            counts["voided"] += 1
        else:
            counts["open"] += 1
        if void:
            reasons[void] = reasons.get(void, 0) + 1
        rows.append({"claim": str(pick(c, "claimId", default=""))[:16], "phase": phase, "void": void, "daa": pick(c, "acceptedDaa")})
    return {"tip": pick(r, "tipDaa"), "collateral": pick(r, "bondCollateral"), "slashed": pick(r, "bondSlashed"),
            "counts": counts, "void_reasons": reasons, "claims": rows}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    sp = ap.add_subparsers(dest="cmd", required=True)
    sp.add_parser("anchor")
    sp.add_parser("tip")
    c = sp.add_parser("claims")
    c.add_argument("--bond", required=True)
    c.add_argument("--class", dest="cls", default=None)
    w = sp.add_parser("wait")
    w.add_argument("--daa", type=int, default=0)
    w.add_argument("--bond")
    w.add_argument("--class", dest="cls", default=None)
    w.add_argument("--final", type=int, default=0)
    w.add_argument("--convicted", type=int, default=0)
    w.add_argument("--deadline-daa", type=int, default=0)
    k = sp.add_parser("class")
    k.add_argument("--class", dest="cls", required=True)
    a = ap.parse_args()
    if a.cmd == "tip":
        print(tip(a.port))
    elif a.cmd == "anchor":
        sink, daa = anchor(a.port)
        print(sink, daa)
    elif a.cmd == "claims":
        print(json.dumps(claims_of(a.port, a.bond, a.cls)))
    elif a.cmd == "class":
        reg = call(a.port, "getPalwClassContexts", {})
        row = next((x for x in (pick(reg, "classes", default=[]) or []) if pick(x, "classId") == a.cls), None)
        print(json.dumps({"listed": row is not None, "row": row}))
        sys.exit(0 if row is not None else 1)
    elif a.cmd == "wait":
        while True:
            daa = tip(a.port)
            done = daa >= a.daa if (a.daa and not (a.final or a.convicted)) else False
            if a.bond and (a.final or a.convicted):
                counts = claims_of(a.port, a.bond, a.cls)["counts"]
                done = (a.final and counts["final"] >= a.final) or (a.convicted and counts["convicted"] >= a.convicted)
            if done:
                print(f"reached at DAA {daa}")
                return 0
            if a.deadline_daa and daa >= a.deadline_daa:
                print(f"INCOMPLETE: DAA {daa} passed {a.deadline_daa}")
                return 3
            time.sleep(20)


if __name__ == "__main__":
    sys.exit(main())
