#!/usr/bin/env python3
"""audit-vertex/dvmeasure.py — RFC-0007 Part I drill measurements (read-only JSON-wRPC to the drill's own loopback nodes). Stdlib only.

  census   --port P --fence H     every executor bond's claims (getPalwClaims, terminal included) by the path they licensed on:
                                  panel bound BELOW the fence (the receipt path), AT/ABOVE it (licence by tally), and the CROSS claims
                                  (bound below the fence, licensed at or above it: the old path after the fence)
  carriage --port P --fence H [--from DAA] [--to DAA]
                                  walks the chain's blocks (getBlocks) and sums the lifecycle payload bytes by object tag: the receipt
                                  licences (tags 6 / 49 / 52 / 59) below the fence, the vertices (91) and equivocations (92) above,
                                  per claim licensed in the same window
  status   --ports P,P,…          each node's getPalwNodeStatus `verification` line (the seat's vertex counters)
  bond     --port P --bond TXID:I the bond's collateral / slashed / status (rpc.py snap's row)
"""
import argparse, json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "audit-tir"))
from rpc import call, pick  # noqa: E402

LICENCE_TAGS = {6: "ReceiptLicensed", 49: "ReceiptLicensedV2", 52: "OptimisticLicensed", 59: "ReceiptLicensedBatchV1"}
VERTEX_TAG, EQUIVOCATION_TAG = 91, 92


def manifest(kr):
    return json.load(open(os.path.join(os.path.expanduser(kr), "manifest.json")))


def claims_of(port, bonds):
    seen, rows = set(), []
    for bond in bonds:
        for role in ("executor",):
            r = call(port, "getPalwClaims", {"bond": bond, "role": role, "includeTerminal": True, "limit": 0})
            for c in (pick(r, "claims", default=[]) or []):
                cid = pick(c, "claimId")
                if cid and cid not in seen:
                    seen.add(cid)
                    rows.append(c)
    return rows


def census(a):
    m = manifest(a.keyring)
    bonds = [s["bond_outpoint"] for s in m["seats"]]
    rows = claims_of(a.port, bonds)
    below = above = cross = 0
    licensed_below = licensed_above = 0
    open_below = open_above = 0
    detail = []
    for c in rows:
        bound = pick(c, "boundDaa")
        phase = str(pick(c, "phase", default=""))
        phase_daa = int(pick(c, "phaseDaa", default=0) or 0)
        if bound is None:
            continue
        bound = int(bound)
        licensed = phase in ("receipt_licensed", "final") or phase.startswith("Final")
        if bound < a.fence:
            below += 1
            if licensed:
                licensed_below += 1
            else:
                open_below += 1
            # `phase_daa` is when the phase began: for receipt_licensed, the licence DAA (a `final` claim's phase DAA is its final DAA).
            if phase == "receipt_licensed" and phase_daa >= a.fence:
                cross += 1
                detail.append({"claim": pick(c, "claimId")[:16], "bound": bound, "licensed_at": phase_daa})
        else:
            above += 1
            if licensed:
                licensed_above += 1
            else:
                open_above += 1
    out = {"fence": a.fence, "claims_seen": len(rows), "bound_below": below, "licensed_or_final_below": licensed_below,
           "not_yet_licensed_below": open_below, "bound_at_or_above": above, "licensed_or_final_above": licensed_above,
           "not_yet_licensed_above": open_above, "cross_claims_licensed_at_or_after_fence": cross, "cross_detail": detail[:20]}
    print(json.dumps(out, indent=1))


def blocks(port, low=None):
    """Every block from the DAG's start, in pages, with its transactions."""
    dag = call(port, "getBlockDagInfo", {})
    low_hash = low or pick(dag, "pruningPointHash") or pick(dag, "sink")
    low_hash = pick(dag, "pruningPointHash", default=None) or low_hash
    seen = set()
    cursor = low_hash
    while True:
        r = call(port, "getBlocks", {"lowHash": cursor, "includeBlocks": True, "includeTransactions": True}, timeout=120)
        page = pick(r, "blocks", default=[]) or []
        fresh = [b for b in page if pick(pick(b, "header", default={}), "hash") not in seen]
        if not fresh:
            return
        for b in fresh:
            h = pick(pick(b, "header", default={}), "hash")
            seen.add(h)
            yield b
        cursor = pick(pick(fresh[-1], "header", default={}), "hash")


def carriage(a):
    by_tag = {}
    first = last = None
    for b in blocks(a.port):
        header = pick(b, "header", default={}) or {}
        daa = int(pick(header, "daaScore", default=0) or 0)
        if (a.frm is not None and daa < a.frm) or (a.to is not None and daa > a.to):
            continue
        for tx in (pick(b, "transactions", default=[]) or []):
            sub = str(pick(tx, "subnetworkId", default=""))
            payload = str(pick(tx, "payload", default=""))
            if not sub.startswith("4b") or len(payload) < 6:
                continue
            raw = bytes.fromhex(payload)
            if len(raw) < 3:
                continue
            tag = raw[2]  # PalwLifecycleTxPayloadV2 { version: u16, object }: the object's borsh tag follows the version
            side = "below" if daa < a.fence else "above"
            row = by_tag.setdefault((side, tag), {"txs": 0, "payload_bytes": 0})
            row["txs"] += 1
            row["payload_bytes"] += len(raw)
            first = daa if first is None else min(first, daa)
            last = daa if last is None else max(last, daa)
    summary = {}
    for (side, tag), row in sorted(by_tag.items()):
        name = LICENCE_TAGS.get(tag) or ("VerificationVertexV1" if tag == VERTEX_TAG else "VertexEquivocationV1" if tag == EQUIVOCATION_TAG else f"tag{tag}")
        if tag in LICENCE_TAGS or tag in (VERTEX_TAG, EQUIVOCATION_TAG):
            summary[f"{side}:{name}"] = row
    print(json.dumps({"fence": a.fence, "daa_range": [first, last], "lifecycle_payload_bytes_by_object": summary}, indent=1))


def status(a):
    for port in a.ports.split(","):
        r = call(int(port), "getPalwNodeStatus", {})
        print(port, pick(r, "verification", default="?"))


def bond(a):
    r = call(a.port, "getPalwPanelSeats", {})
    print(json.dumps(r)[:2000])


def main():
    p = argparse.ArgumentParser()
    sp = p.add_subparsers(dest="cmd", required=True)
    c = sp.add_parser("census"); c.add_argument("--port", type=int, required=True); c.add_argument("--fence", type=int, required=True)
    c.add_argument("--keyring", default="~/.misaka-palw-int10-drill/keyring"); c.set_defaults(f=census)
    k = sp.add_parser("carriage"); k.add_argument("--port", type=int, required=True); k.add_argument("--fence", type=int, required=True)
    k.add_argument("--from", dest="frm", type=int); k.add_argument("--to", type=int); k.set_defaults(f=carriage)
    s = sp.add_parser("status"); s.add_argument("--ports", required=True); s.set_defaults(f=status)
    b = sp.add_parser("bond"); b.add_argument("--port", type=int, required=True); b.add_argument("--bond"); b.set_defaults(f=bond)
    a = p.parse_args()
    a.f(a)


if __name__ == "__main__":
    main()
