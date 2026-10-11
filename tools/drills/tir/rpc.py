#!/usr/bin/env python3
"""audit-lifecycle/rpc.py — read-only JSON-wRPC client + the lifecycle ledger snapshot. Stdlib only.
Transport is pret12rpc.py's (one connection per call). Nothing here changes node state.

  call --port P METHOD JSONPARAMS            raw call, JSON to stdout
  snap --port P --bond TXID:I [--addr A] [--label L] [--class C]
        one ledger row (JSON): DAA, wallet spendable/total (utxo index, mature vs immature),
        bond collateral / slashed / retiring / reserved exposure / ceiling / available,
        claim count by phase, per-claim (phase, reserved, escrow, payout, vesting), seat role counts
  claims --port P --bond B [--role executor|seat]   table of a bond's claims
"""
import argparse, base64, json, os, socket, struct, sys, time

def ws_connect(port, timeout):
    s = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    key = base64.b64encode(os.urandom(16)).decode()
    s.sendall((f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
               f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
    buf = b""
    while b"\r\n\r\n" not in buf:
        c = s.recv(4096)
        if not c:
            raise ConnectionError("closed during handshake")
        buf += c
    head, rest = buf.split(b"\r\n\r\n", 1)
    if b" 101 " not in head.split(b"\r\n")[0]:
        raise ConnectionError("websocket refused")
    return s, rest

def ws_send(s, text):
    data = text.encode(); mask = os.urandom(4); n = len(data); hdr = bytes([0x81])
    if n < 126: hdr += bytes([0x80 | n])
    elif n < 65536: hdr += bytes([0x80 | 126]) + struct.pack(">H", n)
    else: hdr += bytes([0x80 | 127]) + struct.pack(">Q", n)
    s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

def ws_frame(s, buf):
    def need(n):
        nonlocal buf
        while len(buf) < n:
            c = s.recv(1 << 20)
            if not c:
                raise EOFError("closed")
            buf += c
        out, buf = buf[:n], buf[n:]
        return out
    msg = b""
    while True:
        b0, b1 = need(2); n = b1 & 0x7F
        if n == 126: n = struct.unpack(">H", need(2))[0]
        elif n == 127: n = struct.unpack(">Q", need(8))[0]
        if b1 & 0x80: need(4)
        payload = need(n); op = b0 & 0x0F
        if op == 0x8: raise EOFError("server closed")
        if op in (0x9, 0xA): continue
        msg += payload
        if b0 & 0x80: return msg

def call(port, method, params, timeout=30):
    try:
        s, rest = ws_connect(port, timeout)
        ws_send(s, json.dumps({"id": 1, "method": method, "params": params}))
        deadline = time.time() + timeout
        while time.time() < deadline:
            m = json.loads(ws_frame(s, rest)); rest = b""
            if m.get("id") == 1:
                s.close()
                return {"error": m["error"]} if m.get("error") else (m.get("params") or {})
        s.close()
        return {"error": "timeout"}
    except Exception as e:  # noqa: BLE001
        return {"error": f"{type(e).__name__}: {e}"}

def call_url(url, method, params, timeout=30):
    """One JSON-wRPC call to ws://host:port/path or wss://host/path (read-only use: e.g. misakascan)."""
    import ssl, urllib.parse
    try:
        u = urllib.parse.urlparse(url); host = u.hostname; secure = u.scheme == "wss"
        port = u.port or (443 if secure else 80); path = u.path or "/"
        raw = socket.create_connection((host, port), timeout=timeout)
        s = ssl.create_default_context().wrap_socket(raw, server_hostname=host) if secure else raw
        key = base64.b64encode(os.urandom(16)).decode()
        s.sendall((f"GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                   f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            c = s.recv(4096)
            if not c:
                raise ConnectionError("closed during handshake")
            buf += c
        head, rest = buf.split(b"\r\n\r\n", 1)
        if b" 101 " not in head.split(b"\r\n")[0]:
            raise ConnectionError("websocket refused: " + head.split(b"\r\n")[0].decode(errors="replace"))
        ws_send(s, json.dumps({"id": 1, "method": method, "params": params}))
        deadline = time.time() + timeout
        while time.time() < deadline:
            m = json.loads(ws_frame(s, rest)); rest = b""
            if m.get("id") == 1:
                s.close()
                return {"error": m["error"]} if m.get("error") else (m.get("params") or {})
        s.close()
        return {"error": "timeout"}
    except Exception as e:  # noqa: BLE001
        return {"error": f"{type(e).__name__}: {e}"}

def pick(d, *names, default=None):
    if not isinstance(d, dict): return default
    norm = {k.replace("_", "").lower(): v for k, v in d.items()}
    for n in names:
        v = norm.get(n.replace("_", "").lower())
        if v is not None: return v
    return default

def split_op(b):
    t, i = b.rsplit(":", 1); return t, int(i)

def wallet(port, addr, daa):
    r = call(port, "getUtxosByAddresses", {"addresses": [addr]})
    if "error" in r: return {"error": r["error"]}
    ents = pick(r, "entries", default=[]) or []
    tot = spend = imm = cb = 0; n = 0; outs = []
    for e in ents:
        u = pick(e, "utxoEntry", default={}) or {}
        op = pick(e, "outpoint", default={}) or {}
        amt = int(pick(u, "amount", default=0) or 0); iscb = bool(pick(u, "isCoinbase", default=False))
        bd = int(pick(u, "blockDaaScore", default=0) or 0)
        tot += amt; n += 1
        outs.append({"op": f"{pick(op,'transactionId')}:{pick(op,'index')}", "sompi": amt, "cb": iscb, "daa": bd})
        if iscb: cb += amt
    return {"total_sompi": tot, "utxos": n, "coinbase_sompi": cb, "outs": sorted(outs, key=lambda x: -x["sompi"])[:12]}

def snap(a):
    port = a.port
    dag = call(port, "getBlockDagInfo", {})
    daa = int(pick(dag, "virtualDaaScore", default=0) or 0)
    row = {"t": time.strftime("%Y-%m-%d %H:%M:%S"), "label": a.label, "daa": daa,
           "sink": str(pick(dag, "sink", default=""))[:16]}
    if a.addr:
        row["wallet"] = wallet(port, a.addr, daa)
    if a.bond:
        t, i = split_op(a.bond)
        cl = call(port, "getPalwClaims", {"bond": a.bond, "role": "executor", "includeTerminal": True, "limit": 0})
        row["bond"] = {k: pick(cl, k) for k in ("bondKnown", "bondCollateral", "bondSlashed", "bondRetiringSinceDaa",
                                                "bondRegisteredDaa", "bondCapableClasses", "tipDaa")}
        if "error" in cl: row["bond"]["error"] = cl["error"]
        facts = call(port, "getPalwProducerFacts", {"classId": a.cls or "", "bondTransactionId": t, "bondIndex": i, "withBond": True})
        row["facts"] = {k: pick(facts, k) for k in ("bondKnown", "bondCollateral", "bondReservedExposure", "bondExposureCeiling",
                                                   "bondClaimExposure", "notReadyReason", "classId")}
        if "error" in facts: row["facts"]["error"] = facts["error"]
        try:
            col = int(row["bond"]["bondCollateral"] or 0); res = int(row["facts"]["bondReservedExposure"] or 0)
            ceil = int(row["facts"]["bondExposureCeiling"] or 0); sl = int(row["bond"]["bondSlashed"] or 0)
            row["bond"]["reserved"] = res; row["bond"]["ceiling"] = ceil; row["bond"]["available"] = ceil - res
        except Exception:  # noqa: BLE001
            pass
        claims = []; phases = {}; rsum = 0
        for c in (pick(cl, "claims", default=[]) or []):
            ph = pick(c, "phase", default="?"); phases[ph] = phases.get(ph, 0) + 1
            r = int(pick(c, "reservedSompi", default="0") or 0); rsum += r
            claims.append({"id": str(pick(c, "claimId", default=""))[:16], "phase": ph, "void": pick(c, "voidReason", default=""),
                           "acc": pick(c, "acceptedDaa"), "bound": pick(c, "boundDaa"), "phase_daa": pick(c, "phaseDaa"),
                           "deadline": pick(c, "deadlineDaa"), "reserved": r, "escrow": pick(c, "escrowSompi"),
                           "payout_pending": pick(c, "payoutPendingSompi"), "seats": len(pick(c, "seats", default=[]) or []),
                           "courts": pick(c, "openCourts"), "vest": pick(c, "vestingStage", default=""),
                           "vest_sompi": pick(c, "vestingSompi"), "vest_payee": pick(c, "vestingPayeeSompi"),
                           "vest_expiry": pick(c, "vestingExpiryDaa"), "vest_lic": [pick(c, "vestingLicencesSinceFinal"), pick(c, "vestingLicencesNeeded")],
                           "vest_eta": pick(c, "vestingEtaDaa"), "exec": pick(c, "execStage", default="")})
        row["claims"] = {"count": len(claims), "phases": phases, "reserved_sum": rsum, "rows": claims,
                         "vesting_only": len(pick(cl, "vestingOnlyRows", default=[]) or [])}
        vo = []
        for c in (pick(cl, "vestingOnlyRows", default=[]) or []):
            vo.append({"id": str(pick(c, "claimId", default=""))[:16], "vest": pick(c, "vestingStage"), "vest_payee": pick(c, "vestingPayeeSompi"),
                       "payout_pending": pick(c, "payoutPendingSompi")})
        row["claims"]["vesting_only_rows"] = vo
        sc = call(port, "getPalwClaims", {"bond": a.bond, "role": "seat", "includeTerminal": True, "limit": 0})
        sp = {}
        for c in (pick(sc, "claims", default=[]) or []):
            ph = pick(c, "phase", default="?"); sp[ph] = sp.get(ph, 0) + 1
        row["seat_role"] = sp
        v = call(port, "getPalwVesting", {"bond": a.bond, "payoutAddress": "", "claimId": "", "limit": 0, "after": ""})
        row["vesting"] = {k: v.get(k) for k in v if not isinstance(v.get(k), list)} if isinstance(v, dict) else v
    print(json.dumps(row))

def cmd_claims(a):
    r = call(a.port, "getPalwClaims", {"bond": a.bond, "role": a.role, "includeTerminal": True, "limit": 0})
    if "error" in r:
        print(f"ERROR {r['error']}"); return 2
    print(f"tip={pick(r,'tipDaa')} collateral={pick(r,'bondCollateral')} slashed={pick(r,'bondSlashed')} retiring={pick(r,'bondRetiringSinceDaa')} capable={len(pick(r,'bondCapableClasses',default=[]) or [])}")
    for c in pick(r, "claims", default=[]) or []:
        print(f"{str(pick(c, 'claimId', default='?'))[:16]}\t{pick(c, 'phase', default='?')}\tvoid={pick(c,'voidReason',default='')}\tacc={pick(c, 'acceptedDaa', default='?')}"
              f"\tbound={pick(c, 'boundDaa', default='-')}\tphase_daa={pick(c,'phaseDaa')}\tdl={pick(c,'deadlineDaa')}\tres={pick(c,'reservedSompi')}\tvest={pick(c,'vestingStage',default='')}\tseats={len(pick(c, 'seats', default=[]) or [])}")
    for c in pick(r, "vestingOnlyRows", default=[]) or []:
        print(f"VESTONLY {str(pick(c, 'claimId', default='?'))[:16]}\t{pick(c,'vestingStage')}\tpayee={pick(c,'vestingPayeeSompi')}\tpending={pick(c,'payoutPendingSompi')}")
    return 0

def main():
    ap = argparse.ArgumentParser(); sp = ap.add_subparsers(dest="cmd", required=True)
    c = sp.add_parser("call"); c.add_argument("--port", type=int, default=0); c.add_argument("--url", default=""); c.add_argument("method"); c.add_argument("params", nargs="?", default="{}")
    s = sp.add_parser("snap"); s.add_argument("--port", type=int, required=True); s.add_argument("--bond", default="")
    s.add_argument("--addr", default=""); s.add_argument("--label", default=""); s.add_argument("--cls", default="")
    k = sp.add_parser("claims"); k.add_argument("--port", type=int, required=True); k.add_argument("--bond", required=True); k.add_argument("--role", default="executor")
    a = ap.parse_args()
    if a.cmd == "call":
        r = call_url(a.url, a.method, json.loads(a.params)) if a.url else call(a.port, a.method, json.loads(a.params))
        print(json.dumps(r, indent=1))
    elif a.cmd == "snap":
        snap(a)
    elif a.cmd == "claims":
        sys.exit(cmd_claims(a))

if __name__ == "__main__":
    main()
