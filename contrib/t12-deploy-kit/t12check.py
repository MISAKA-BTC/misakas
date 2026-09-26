#!/usr/bin/env python3
"""deploy-t12/t12check.py — read-only post-start check of ONE local kaspad over its JSON wRPC.

Stdlib only (no websocket package on the fleet hosts). Asks the node:
  getInfo, getBlockDagInfo, getPalwNodeStatus, getConnectedPeerInfo, getBlock(<expected genesis>)
and, with --registry, getPalwModelRegistry. Prints one block of lines and exits
  0  fingerprint and genesis match
  1  a mismatch (wrong build / wrong chain)
  2  the node did not answer
Nothing it sends changes node state.

usage: t12check.py --port 26314 --expect-fp <64 hex> --expect-genesis <128 hex> [--registry] [--json] [--state-line]
                   [--expect-premine <128 hex>] [--expect-class <class id>[:<registry artifactBytes>]] [--expect-class-prefix <hex>]
       t12check.py --port 26994 --probe      # a FRESH isolated node: print its fingerprint and genesis
       t12check.py --port 26994 --premine    # the txid the genesis bonds sit on (getPalwPanelSeats)
       t12check.py --port 26994 --layout     # the genesis classes (id, artifact bytes, root) and bond outpoints

--expect-premine / --expect-class / --expect-class-prefix check the deploy kit's copies of chain facts
that no node checks at start (fleet.env PREMINE_TXID and CLASS_8K, lib.sh CLASS_2M_PREFIX): every genesis
seat's bond is <txid>:<card> for cards 0..7, the class is registered, and a registered class carries the
prefix. The registry's artifactBytes is the work derivation's figure (2,620,391,424 for both t12 dense rows),
NOT the artifact file's size (ART_8K_BYTES), so the optional :<bytes> compares with the former only.
--expect-class needs --registry (added when absent).

--state-line appends ONE machine-readable line for lib.sh's `upgrade` gates (the same run, nothing extra asked):
  STATE fp=OK|BAD|? genesis=OK|BAD|? facts=OK|BAD|?|- synced=true|false|? peers=<n> daa=<n>|? blocks=<n>|? ready=<now>/<required>|-
'?' = that call got no answer (or, except getBlock, an error) before --timeout — unknown, not a mismatch;
facts = the kit's chain-fact copies (--expect-premine / --expect-class / --expect-class-prefix; '-' when none was asked);
ready = the first --expect-class's readySeatsNow / requiredReadySeats. A node that does not answer prints
`STATE unreachable` (exit 2).
"""
import argparse, base64, json, os, socket, struct, sys, time


def ws_connect(port, timeout):
    s = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    key = base64.b64encode(os.urandom(16)).decode()
    s.sendall((f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
               f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = s.recv(4096)
        if not chunk:
            raise ConnectionError("closed during handshake")
        buf += chunk
    head, rest = buf.split(b"\r\n\r\n", 1)
    if b" 101 " not in head.split(b"\r\n")[0]:
        raise ConnectionError("websocket handshake refused: " + head.split(b"\r\n")[0].decode(errors="replace"))
    return s, rest


def ws_send(s, text):
    data = text.encode()
    mask = os.urandom(4)
    n = len(data)
    hdr = bytes([0x81])
    if n < 126:
        hdr += bytes([0x80 | n])
    elif n < 65536:
        hdr += bytes([0x80 | 126]) + struct.pack(">H", n)
    else:
        hdr += bytes([0x80 | 127]) + struct.pack(">Q", n)
    s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


class Reader:
    def __init__(self, sock, rest):
        self.sock, self.buf = sock, rest

    def need(self, n):
        while len(self.buf) < n:
            c = self.sock.recv(1 << 20)
            if not c:
                raise EOFError("connection closed")
            self.buf += c
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def frame(self):
        msg = b""
        while True:
            b0, b1 = self.need(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self.need(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self.need(8))[0]
            if b1 & 0x80:
                self.need(4)
            payload = self.need(n)
            op = b0 & 0x0F
            if op == 0x8:
                raise EOFError("server closed the websocket")
            if op in (0x9, 0xA):
                continue
            msg += payload
            if b0 & 0x80:
                return msg


def call_all(port, calls, timeout):
    """calls: list of (method, params). Returns {method: result-or-{'error':…}}."""
    sock, rest = ws_connect(port, timeout)
    r = Reader(sock, rest)
    for i, (m, p) in enumerate(calls):
        ws_send(sock, json.dumps({"id": i, "method": m, "params": p}))
    out, left, deadline = {}, len(calls), time.time() + timeout
    while left and time.time() < deadline:
        msg = json.loads(r.frame())
        if msg.get("id") is None:
            continue
        m = calls[msg["id"]][0]
        out[m] = {"error": msg["error"]} if msg.get("error") else msg.get("params")
        left -= 1
    sock.close()
    return out


def pick(d, *names, default=None):
    """case/underscore-insensitive field lookup (the JSON is camelCase; be robust anyway)."""
    if not isinstance(d, dict):
        return default
    norm = {k.replace("_", "").lower(): v for k, v in d.items()}
    for n in names:
        v = norm.get(n.replace("_", "").lower())
        if v is not None:
            return v
    return default


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--expect-fp", default="")
    ap.add_argument("--expect-genesis", default="")
    ap.add_argument("--probe", action="store_true",
                    help="fresh isolated node: print FP=… and GENESIS=… (its pruning point is the genesis)")
    ap.add_argument("--premine", action="store_true",
                    help="print PREMINE_TXID=… — the one txid every genesis seat's bond outpoint names (getPalwPanelSeats)")
    ap.add_argument("--layout", action="store_true",
                    help="print CLASS <id> bytes=… root=… base=… and BOND <outpoint> lines (getPalwModelRegistry, getPalwPanelSeats)")
    ap.add_argument("--expect-premine", default="")
    ap.add_argument("--expect-class", action="append", default=[], help="<class id>[:<registry artifactBytes>]")
    ap.add_argument("--expect-class-prefix", action="append", default=[])
    ap.add_argument("--registry", action="store_true")
    ap.add_argument("--expect-panel", action="store_true",
                    help="the node holds a bond: its panel must be RUNNING (getPalwNodeStatus.panelRunning), not just planned")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--state-line", action="store_true",
                    help="append one 'STATE fp=… genesis=… facts=… synced=… peers=… daa=… blocks=… ready=…' line (lib.sh upgrade)")
    ap.add_argument("--timeout", type=float, default=20)
    a = ap.parse_args()
    if a.probe:
        try:
            res = call_all(a.port, [("getPalwNodeStatus", {}), ("getBlockDagInfo", {})], a.timeout)
        except Exception as e:  # noqa: BLE001
            print(f"UNREACHABLE {e}")
            return 2
        st, dag = res.get("getPalwNodeStatus") or {}, res.get("getBlockDagInfo") or {}
        print(f"FP={pick(st, 'consensusParamsId', default='')}")
        print(f"GENESIS={pick(dag, 'pruningPointHash', default='')}")
        print(f"BLOCKS={pick(dag, 'blockCount', default='?')} NETWORK={pick(dag, 'network', default='?')}")
        return 0
    if a.premine:
        try:
            res = call_all(a.port, [("getPalwPanelSeats", {"classId": ""})], a.timeout)
        except Exception as e:  # noqa: BLE001
            print(f"UNREACHABLE {e}")
            return 2
        seats = pick(res.get("getPalwPanelSeats") or {}, "seats", default=[]) or []
        txids = sorted({str(pick(s, "bondOutpoint", default="")).split(":")[0] for s in seats if pick(s, "bondOutpoint")})
        print(f"SEATS={len(seats)} TXIDS={len(txids)}")
        if len(txids) == 1:
            print(f"PREMINE_TXID={txids[0]}")
            return 0
        print("PREMINE_TXID=")  # none, or the genesis seats do not share one txid: the operator reads it by hand
        return 1
    if a.layout:
        try:
            res = call_all(a.port, [("getPalwModelRegistry", {}), ("getPalwPanelSeats", {"classId": ""})], a.timeout)
        except Exception as e:  # noqa: BLE001
            print(f"UNREACHABLE {e}")
            return 2
        reg, seats = res.get("getPalwModelRegistry") or {}, res.get("getPalwPanelSeats") or {}
        for c in pick(reg, "classes", default=[]) or []:
            print(f"CLASS {pick(c, 'classId', default='?')} bytes={pick(c, 'artifactBytes', default='?')} "
                  f"root={pick(c, 'artifactRoot', default='?')} base={pick(c, 'isBaseClass', default='?')} state={pick(c, 'state', default='?')}")
        for o in sorted({str(pick(x, "bondOutpoint", default="")) for x in pick(seats, "seats", default=[]) or []}):
            print(f"BOND {o}")
        return 0
    if not (a.expect_fp and a.expect_genesis):
        ap.error("--expect-fp and --expect-genesis are required unless --probe / --premine / --layout")
    if a.expect_class or a.expect_class_prefix:
        a.registry = True

    calls = [("getInfo", {}), ("getBlockDagInfo", {}), ("getPalwNodeStatus", {}), ("getConnectedPeerInfo", {}),
             ("getBlock", {"hash": a.expect_genesis, "includeTransactions": False})]
    if a.registry:
        calls.append(("getPalwModelRegistry", {}))
    if a.expect_premine:
        calls.append(("getPalwPanelSeats", {"classId": ""}))
    try:
        res = call_all(a.port, calls, a.timeout)
    except Exception as e:  # noqa: BLE001 — any transport failure is "did not answer"
        print(f"  UNREACHABLE json wRPC 127.0.0.1:{a.port}: {e}")
        if a.state_line:
            print("STATE unreachable")
        return 2
    if a.json:
        print(json.dumps(res, indent=1))

    bad = []
    info, dag, st, peers, gblk = (res.get(m) or {} for m in
                                  ("getInfo", "getBlockDagInfo", "getPalwNodeStatus", "getConnectedPeerInfo", "getBlock"))
    fp = pick(st, "consensusParamsId", default="")
    fp_ok = fp == a.expect_fp
    if not fp_ok:
        bad.append("fingerprint")
    genesis_known = isinstance(gblk, dict) and "error" not in gblk and bool(gblk)
    pp = pick(dag, "pruningPointHash", default="")
    if not genesis_known:
        bad.append("genesis")
    net = pick(dag, "network", "networkId", default="?")
    daa = pick(dag, "virtualDaaScore", default="?")
    blocks = pick(dag, "blockCount", default="?")
    peer_list = pick(peers, "peerInfo", default=[]) or []
    addrs = sorted({str(pick(p, "address", default="?")) for p in peer_list})
    print(f"  node {pick(info, 'serverVersion', default='?')} net={net} synced={pick(info, 'isSynced', default='?')} "
          f"utxoindex={pick(info, 'isUtxoIndexed', default='?')} daa={daa} blocks={blocks}")
    print(f"  fingerprint {fp[:16] or '<none>'}… {'OK' if fp_ok else 'MISMATCH (expected ' + a.expect_fp[:16] + '…)'}")
    print(f"  genesis {a.expect_genesis[:16]}… {'present' if genesis_known else 'NOT FOUND on this node'}"
          f"; pruning point {str(pp)[:16]}…{' (= genesis)' if pp == a.expect_genesis else ''}")
    print(f"  peers {len(peer_list)}: {', '.join(addrs[:8])}{' …' if len(addrs) > 8 else ''}")
    if not peer_list:
        print("  WARNING: no peers — the heartbeat miner holds with no peers, and nothing relays")
    if st and "error" not in st:
        print(f"  lanes: {pick(st, 'laneMix', default='(no mix yet)')}")
        alarm = pick(st, "laneAlarm", default="")
        if alarm:
            print(f"  LANE ALARM: {alarm[:200]}")
        print(f"  producer {pick(st, 'producerState', default='?')}: {str(pick(st, 'producerReason', default=''))[:160]}")
        print(f"  producer class {str(pick(st, 'producerClass', default=''))[:16]} bond {str(pick(st, 'producerBond', default=''))[-6:]} "
              f"produced={pick(st, 'producedBlocks', default='?')} receipts={pick(st, 'receiptBlocks', default='?')} "
              f"panel={pick(st, 'panelRunning', default='?')} submitter={pick(st, 'panelSubmitter', default='?')}")
        gib = 1 << 30
        share = pick(st, "memoryShareBytes", default=0) or 0
        headroom = pick(st, "memoryHeadroomBytes", default=0) or 0
        print(f"  memory share {share / gib:.2f} GiB, reserved {(pick(st, 'memoryReservedBytes', default=0) or 0) / gib:.2f}, "
              f"live {headroom / gib:.2f} (min(MemAvailable, cgroup max − current) − 1 GiB), "
              f"available {(pick(st, 'memoryAvailableBytes', default=0) or 0) / gib:.2f}, bounded={pick(st, 'memoryBounded', default='?')}")
        if share and headroom and headroom < share:
            print("  WARNING: the ledger's live bound is below the share — duties the share admits are refused "
                  "(host MemAvailable or the unit's MemoryMax minus its page cache; PLAN.md §2)")
        holders = pick(st, "memoryHolders", default="")
        if holders:
            print(f"  memory holders: {str(holders)[:200]}")
        # 09-25: the seat duties are always on for a bonded node — the startup 'PALW duties' line is the
        # PLAN; this is whether the panel worker actually started (its key and bond loaded, the gossip
        # inbox was free).
        if a.expect_panel:
            if pick(st, "panelRunning", default=None) is not True:
                print("  PANEL NOT RUNNING — this node holds a bond, so its seat duties must run (journal: "
                      "'panel service disabled' / 'PALW duties NOT as planned' say why)")
                bad.append("panel")
            elif pick(st, "panelSubmitter", default=None) is not True:
                print("  WARNING: the panel runs receipts only (no --palw-fee-outpoint): it carries nothing on chain")
    else:
        print(f"  getPalwNodeStatus: {st.get('error') if isinstance(st, dict) else st}")
        if a.expect_panel:
            bad.append("panel")
    if a.expect_premine:
        seats = pick(res.get("getPalwPanelSeats") or {}, "seats", default=[]) or []
        outs = {str(pick(x, "bondOutpoint", default="")) for x in seats}
        want = {f"{a.expect_premine}:{n}" for n in range(8)}
        missing = sorted(want - outs)
        print(f"  genesis bonds: {len(outs)} seats; {'all eight on ' + a.expect_premine[:16] + '…:0..7' if not missing else 'MISSING ' + ', '.join(m[-4:] for m in missing)}")
        if missing:
            bad.append("premine")
    if a.registry:
        reg = res.get("getPalwModelRegistry") or {}
        rows = {str(pick(c, "classId", default="")): c for c in pick(reg, "classes", default=[]) or []}
        for spec in a.expect_class:
            cid, _, size = spec.partition(":")
            row = rows.get(cid)
            got = pick(row, "artifactBytes", default=None) if row else None
            ok = row is not None and (not size or str(got) == size)
            print(f"  class {cid[:16]}… {'registered (' + str(pick(row, 'state', default='?')) + ')' if ok else ('NOT REGISTERED' if row is None else 'registry artifactBytes ' + str(got) + ', expected ' + size)}")
            if not ok:
                bad.append("class " + cid[:8])
        for prefix in a.expect_class_prefix:
            if not any(k.startswith(prefix) for k in rows):
                print(f"  class prefix {prefix}: NO registered class (the kit's refusal would match nothing)")
                bad.append("class prefix " + prefix)
        for c in pick(reg, "classes", default=[]) or []:
            cid = str(pick(c, "classId", default="?"))
            print(f"  class {cid[:8]} state={pick(c, 'state', default='?')} ready={pick(c, 'readySeats', default='?')}"
                  f"/now {pick(c, 'readySeatsNow', default='?')} of {pick(c, 'requiredReadySeats', default='?')} "
                  f"inflight={pick(c, 'inflightNow', default='?')} — {str(pick(c, 'reason', default=''))[:110]}")
    if a.state_line:
        facts_asked = bool(a.expect_premine or a.expect_class or a.expect_class_prefix)
        facts_bad = any(b == "premine" or b.startswith("class") for b in bad)
        ready = "-"
        if a.registry and a.expect_class:
            rows = {str(pick(c, "classId", default="")): c for c in pick(res.get("getPalwModelRegistry") or {}, "classes", default=[]) or []}
            row = rows.get(a.expect_class[0].partition(":")[0])
            if row is not None:
                ready = f"{pick(row, 'readySeatsNow', default='?')}/{pick(row, 'requiredReadySeats', default='?')}"
        synced = pick(info, "isSynced", default=None)

        def num(v):
            return str(v) if isinstance(v, int) and not isinstance(v, bool) else (v if isinstance(v, str) and v.isdigit() else "?")
        # '?' = the call got no answer before --timeout (call_all returns what arrived): not a mismatch, and
        # lib.sh must not act on it (it stops a node only on a definite BAD)
        def verdict(ok, *methods):
            if ok:
                return "OK"
            return "?" if any(m not in res or (isinstance(res[m], dict) and "error" in res[m] and m != "getBlock") for m in methods) else "BAD"
        facts_methods = (["getPalwPanelSeats"] if a.expect_premine else []) + (["getPalwModelRegistry"] if a.registry else [])
        print(f"STATE fp={verdict(fp_ok, 'getPalwNodeStatus')} genesis={verdict(genesis_known, 'getBlock')} "
              f"facts={verdict(not facts_bad, *facts_methods) if facts_asked else '-'} "
              f"synced={'true' if synced is True else ('false' if synced is False else '?')} peers={len(peer_list)} "
              f"daa={num(daa)} blocks={num(blocks)} ready={ready}")
    if bad:
        print(f"  RESULT: MISMATCH ({', '.join(bad)})")
        return 1
    print("  RESULT: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
