#!/usr/bin/env python3
"""Walk the selected chain back N blocks and count the PoW lane of each.

**Which lanes actually pace this chain, read from the chain and not from the design.** The 2026-09-18
release was held because a document claimed testnet-11's selected chain was 300/300 hash-anchor
blocks and the chain said 300/300 were the model lane — a claim nobody had checked against the thing
it described. The launch runbook's §5c release gate reads a network's lane profile with this script
and holds the drill to it.

Run it ON a node (the JSON wRPC listener binds to loopback), with that listener's port:

    python3 scripts/misaka-t11-lane-walk.py [blocks] [json-wrpc-port]

A node exposes it with `--rpclisten-json=127.0.0.1:<port>`; without that flag there is nothing here
to talk to, whatever else the node is listening on.
"""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from misaka_wrpc_json import WsRpc  # noqa: E402  (a sibling file, not a package)

N = int(sys.argv[1]) if len(sys.argv) > 1 else 300
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 26314
c = WsRpc(port=PORT)
info = c.call("getBlockDagInfo", {})
print("virtualDaaScore", info.get("virtualDaaScore"), "blockCount", info.get("blockCount"), "sink", str(info.get("sink"))[:16])
h = info["sink"]
lanes = collections.Counter()
rows = []
seen = 0
while seen < N and h:
    b = c.call("getBlock", {"hash": h, "includeTransactions": False})
    blk = b.get("block") or b
    hdr = blk.get("header") or {}
    vd = blk.get("verboseData") or {}
    algo = hdr.get("powAlgoId", hdr.get("pow_algo_id"))
    if seen == 0:
        print("HEADER KEYS:", sorted(hdr.keys()))
    lanes[algo] += 1
    rows.append((hdr.get("daaScore"), hdr.get("blueScore"), hdr.get("timestamp"), algo))
    seen += 1
    h = (hdr.get("parentsByLevel") or [[None]])[0][0] if not vd.get("selectedParentHash") else vd.get("selectedParentHash")
print("walked", seen, "selected-chain blocks")
for k, v in sorted(lanes.items(), key=lambda kv: -kv[1]):
    print(f"  algo {k}: {v}  ({100.0*v/max(seen,1):.1f} %)")
if len(rows) >= 2:
    d0, b0, t0, _ = rows[-1]
    d1, b1, t1, _ = rows[0]
    span_s = (int(t1) - int(t0)) / 1000.0
    print(f"span {span_s/3600:.2f} h  |  DAA {d0} -> {d1} = {int(d1)-int(d0)}  -> {span_s/max(int(d1)-int(d0),1):.1f} s/DAA")
    print(f"blue {b0} -> {b1} = {int(b1)-int(b0)}  -> {span_s/max(int(b1)-int(b0),1):.1f} s/blue")
    print(f"chain blocks {seen} over {span_s/3600:.2f} h = {seen/max(span_s/3600,0.001):.2f} /h")
