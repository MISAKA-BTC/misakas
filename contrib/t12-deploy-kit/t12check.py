#!/usr/bin/env python3
"""deploy-t12/t12check.py — read-only post-start check of ONE local kaspad over its JSON wRPC.

Stdlib only (no websocket package on the fleet hosts). Asks the node:
  getInfo, getBlockDagInfo, getPalwNodeStatus, getConnectedPeerInfo, getBlock(<expected genesis>)
and, with --registry, getPalwModelRegistry. Prints one block of lines and exits
  0  fingerprint and genesis match
  1  a mismatch (wrong build / wrong chain)
  2  the node did not answer
Nothing it sends changes node state.

usage: t12check.py --port 26314 --expect-fp <64 hex> --expect-genesis <128 hex> [--registry] [--json]
       t12check.py --port 26994 --probe      # a FRESH isolated node: print its fingerprint and genesis
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
    ap.add_argument("--registry", action="store_true")
    ap.add_argument("--json", action="store_true")
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
    if not (a.expect_fp and a.expect_genesis):
        ap.error("--expect-fp and --expect-genesis are required unless --probe")

    calls = [("getInfo", {}), ("getBlockDagInfo", {}), ("getPalwNodeStatus", {}), ("getConnectedPeerInfo", {}),
             ("getBlock", {"hash": a.expect_genesis, "includeTransactions": False})]
    if a.registry:
        calls.append(("getPalwModelRegistry", {}))
    try:
        res = call_all(a.port, calls, a.timeout)
    except Exception as e:  # noqa: BLE001 — any transport failure is "did not answer"
        print(f"  UNREACHABLE json wRPC 127.0.0.1:{a.port}: {e}")
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
        print(f"  memory share {share / gib:.2f} GiB, reserved {(pick(st, 'memoryReservedBytes', default=0) or 0) / gib:.2f}, "
              f"headroom {(pick(st, 'memoryHeadroomBytes', default=0) or 0) / gib:.2f}, bounded={pick(st, 'memoryBounded', default='?')}")
        holders = pick(st, "memoryHolders", default="")
        if holders:
            print(f"  memory holders: {str(holders)[:200]}")
    else:
        print(f"  getPalwNodeStatus: {st.get('error') if isinstance(st, dict) else st}")
    if a.registry:
        reg = res.get("getPalwModelRegistry") or {}
        for c in pick(reg, "classes", default=[]) or []:
            cid = str(pick(c, "classId", default="?"))
            print(f"  class {cid[:8]} state={pick(c, 'state', default='?')} ready={pick(c, 'readySeats', default='?')}"
                  f"/now {pick(c, 'readySeatsNow', default='?')} of {pick(c, 'requiredReadySeats', default='?')} "
                  f"inflight={pick(c, 'inflightNow', default='?')} — {str(pick(c, 'reason', default=''))[:110]}")
    if bad:
        print(f"  RESULT: MISMATCH ({', '.join(bad)})")
        return 1
    print("  RESULT: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
