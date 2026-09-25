#!/usr/bin/env python3
"""chainstate.py — the drill's read-only view of a testnet-12 node, over its wRPC-JSON listener.

Every subcommand is a READ (getBlockDagInfo, getBlock, getBlocks, getPalw*). Nothing here signs,
submits or writes to a node. Standard library only (the drill host has no `websockets`).

  dag       --port P [--field F]                   getBlockDagInfo (one field, or the JSON)
  status    --port P                               getPalwNodeStatus (lane mix, alarm, memory ledger)
  registry  --port P [--class PREFIX]              getPalwModelRegistry rows
  facts     --port P --class C --bond TXID:IDX     getPalwProducerFacts (notReadyReason, fpCertified)
  claims    --port P --bond B [--role R] [--phase X] [--terminal]
  lanes     --port P --window-daa W --bonds B,...  every merged block of the last W DAA, by pow_algo_id,
                                                   and algo 6/7/10 attributed to classes
  gate      --port P --window-daa W --bonds B,... --floor C --model C [--alarm-ports P,...]
            [--want-fp CLASSPREFIX,...] [--expect-heartbeat-only] [--short]
                                                   --short: algo 10 / spent tickets are noted, not judged
                                                   (a Final's tickets mature 1,200 DAA after it)
  snap      --port P --bonds B,...                 the consensus-derived state, normalised
  compare   --ports P,P,... --bonds B,... [--out DIR]
  slashed   --port P --bonds B,...                 per bond: collateral, slashed (and the sum)
  genesis   --port P --drill-genesis H --forbidden H,H --drill-premine T --forbidden-premines T,T
                                                   exit 0 only on the drill chain: its genesis in the DAG at
                                                   DAA 0 (or pruned past), no forbidden genesis, a bond on the
                                                   drill premine txid and none on a forbidden one
  dns       --port P                               getDnsConfirmation: rolloutStage (1 = Bootstrap)
  licence   --port P --bonds B,... --class C [--window-daa W --grace-daa G --min-ratio R --min-bound N]
                                                   bound floor claims that licensed, and the stuck ones
  room      --port P --class C --bond B            one sample: op 186's panelRoom vs the gate's refusal
  classes   --port P                               one observer row: every class's state/inflight/room/ready

Why `lanes` walks the DAG and not `getPalwNodeStatus`: the node's lane watch counts the SELECTED
chain only. A round block (algo 10) is never a selected parent and a receipt block (algo 7) buys no
chain position, so both are structurally (10) or almost always (7) zero there. A launch gate that
read the node's mix would report the execution lane dead while it runs.
"""
import argparse
import base64
import json
import os
import socket
import struct
import sys
import time


# ---------------------------------------------------------------------------------------------
# minimal RFC 6455 client: text frames, masked client frames, continuation frames reassembled
class WsRpc:
    def __init__(self, port, host="127.0.0.1", timeout=60.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.settimeout(timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (f"GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
             f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        self.buf = b""
        while b"\r\n\r\n" not in self.buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise RuntimeError("closed during the handshake")
            self.buf += chunk
        head, self.buf = self.buf.split(b"\r\n\r\n", 1)
        if b" 101" not in head.split(b"\r\n")[0]:
            raise RuntimeError(f"handshake refused on port {port}")
        self.next_id = 1

    def _need(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(1 << 20)
            if not chunk:
                raise RuntimeError("the server closed the connection")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _send(self, text):
        data = text.encode()
        mask = os.urandom(4)
        n = len(data)
        if n < 126:
            hdr = struct.pack("!BB", 0x81, 0x80 | n)
        elif n < 65536:
            hdr = struct.pack("!BBH", 0x81, 0x80 | 126, n)
        else:
            hdr = struct.pack("!BBQ", 0x81, 0x80 | 127, n)
        self.sock.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _message(self):
        msg = b""
        while True:
            b0, b1 = self._need(2)
            op, n = b0 & 0x0F, b1 & 0x7F
            if n == 126:
                n = struct.unpack("!H", self._need(2))[0]
            elif n == 127:
                n = struct.unpack("!Q", self._need(8))[0]
            mask = self._need(4) if b1 & 0x80 else None
            payload = self._need(n) if n else b""
            if mask:
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            if op == 0x8:
                raise RuntimeError("the server sent a close frame")
            if op in (0x9, 0xA):
                continue
            msg += payload
            if b0 & 0x80:
                return msg

    def call(self, method, params=None):
        rid = self.next_id
        self.next_id += 1
        self._send(json.dumps({"id": rid, "method": method, "params": params or {}}))
        while True:
            m = json.loads(self._message().decode())
            if m.get("id") != rid:
                continue
            if m.get("error") is not None:
                raise RuntimeError(f"{method}: {m['error']}")
            return m.get("params", m.get("result"))

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def split_bond(b):
    txid, idx = b.rsplit(":", 1)
    return txid, int(idx)


# ---------------------------------------------------------------------------------------------
# reads
def dag(c):
    return c.call("getBlockDagInfo", {})


def header(c, h):
    b = c.call("getBlock", {"hash": h, "includeTransactions": False})
    b = b.get("block", b)
    return b.get("header", {}), b.get("verboseData") or {}


def claims(c, bond, role="executor", terminal=True):
    return c.call("getPalwClaims", {"bond": bond, "role": role, "includeTerminal": terminal, "limit": 0})


def registry(c):
    return c.call("getPalwModelRegistry", {})


def round_lane(c):
    return c.call("getPalwRoundLane", {})


def node_status(c):
    return c.call("getPalwNodeStatus", {})


def facts(c, class_id, bond):
    txid, idx = split_bond(bond)
    return c.call("getPalwProducerFacts", {"classId": class_id, "bondTransactionId": txid, "bondIndex": idx, "withBond": True})


# ---------------------------------------------------------------------------------------------
# lanes: every block merged by the selected chain over the last W DAA, by algo
def window_blocks(c, window_daa, max_chain=200000):
    info = dag(c)
    sink = info["sink"]
    sh, _ = header(c, sink)
    top = int(sh["daaScore"])
    floor_daa = max(0, top - window_daa)
    # walk the selected chain back to the first chain block below the window
    h, low, walked = sink, None, 0
    while h and walked < max_chain:
        hh, vd = header(c, h)
        walked += 1
        if int(hh["daaScore"]) < floor_daa or not vd.get("selectedParentHash"):
            low = h
            break
        low = h
        h = vd.get("selectedParentHash")
    # page forward with getBlocks from `low`: the chain's blocks with their mergesets, reds included
    seen, blocks, cursor = set(), [], low
    for _ in range(100000):
        r = c.call("getBlocks", {"lowHash": cursor, "includeBlocks": True, "includeTransactions": False})
        page = r.get("blocks") or []
        progressed = False
        next_cursor = cursor
        for b in page:
            hd = b.get("header", {})
            vd = b.get("verboseData") or {}
            bh = vd.get("hash") or hd.get("hash")
            if not bh or bh in seen:
                continue
            seen.add(bh)
            progressed = True
            blocks.append({"hash": bh, "algo": hd.get("powAlgoId"), "daa": int(hd.get("daaScore", 0)),
                           "chain": bool(vd.get("isChainBlock"))})
            if vd.get("isChainBlock"):
                next_cursor = bh
        if not progressed or next_cursor == cursor or sink in seen:
            break
        cursor = next_cursor
    return {"sink": sink, "sinkDaa": top, "fromDaa": floor_daa, "chainBlocksWalked": walked, "blocks": blocks}


def attribution(c, bonds):
    """accepted attempt block -> class, and per class: FP quanta spent, execution tickets spent."""
    by_block, per_class = {}, {}
    for b in bonds:
        try:
            r = claims(c, b, "executor", True)
        except Exception as e:  # a bond the chain does not know answers bondKnown=false, not an error
            print(f"# claims({b}): {e}", file=sys.stderr)
            continue
        for row in r.get("claims") or []:
            cls = row.get("classId", "")
            pc = per_class.setdefault(cls, {"claims": 0, "final": 0, "fpClaims": 0, "fpQuantaSpent": 0,
                                            "execTickets": 0, "execTicketsSpent": 0, "voided": 0})
            pc["claims"] += 1
            pc["final"] += row.get("phase") == "final"
            pc["voided"] += row.get("phase") == "voided"
            if row.get("isFreePrompt"):
                pc["fpClaims"] += 1
                pc["fpQuantaSpent"] += int(row.get("quantaSpent") or 0)
            pc["execTickets"] += int(row.get("execTickets") or 0)
            pc["execTicketsSpent"] += int(row.get("execTicketsSpent") or 0)
            if row.get("acceptedBlock"):
                by_block[row["acceptedBlock"]] = cls
    return by_block, per_class


def lanes(c, window_daa, bonds):
    w = window_blocks(c, window_daa)
    by_block, per_class = attribution(c, bonds)
    counts, chain_counts, attempt_by_class = {}, {}, {}
    for b in w["blocks"]:
        a = str(b["algo"])
        counts[a] = counts.get(a, 0) + 1
        if b["chain"]:
            chain_counts[a] = chain_counts.get(a, 0) + 1
        if b["algo"] == 6:
            cls = by_block.get(b["hash"], "unattributed")
            attempt_by_class[cls] = attempt_by_class.get(cls, 0) + 1
    return {"sink": w["sink"], "sinkDaa": w["sinkDaa"], "fromDaa": w["fromDaa"], "blocks": len(w["blocks"]),
            "byAlgo": counts, "selectedChainByAlgo": chain_counts, "attemptBlocksByClass": attempt_by_class,
            "classes": per_class}


def cls_match(cls, prefix):
    return cls.lower().startswith(prefix.lower())


def gate(c, a):
    L = lanes(c, a.window_daa, a.bonds.split(","))
    reg = registry(c)
    problems, notes = [], []
    by = {int(k): v for k, v in L["byAlgo"].items() if k not in ("None", "null")}
    total = sum(by.values()) or 1
    work = sum(by.get(k, 0) for k in (6, 7, 9, 10))
    if work == 0:
        problems.append(f"heartbeat-only: {by.get(8, 0)} of {total} merged blocks in DAA {L['fromDaa']}..{L['sinkDaa']} are heartbeats, no work block")
    if by.get(9, 0):
        problems.append(f"algo 9 appeared ({by[9]}×) — testnet-12 does not arm palw_attempt_activation; an unexpected lane")

    def need(cls_prefix, label, want_fp):
        att = sum(v for k, v in L["attemptBlocksByClass"].items() if cls_match(k, cls_prefix))
        pc = {}
        for k, v in L["classes"].items():
            if cls_match(k, cls_prefix):
                for kk, vv in v.items():
                    pc[kk] = pc.get(kk, 0) + vv
        row = {"attempt(6)": att, "roundTicketsSpent(10)": pc.get("execTicketsSpent", 0),
               "fpQuantaSpent(7)": pc.get("fpQuantaSpent", 0), "final": pc.get("final", 0)}
        if att == 0:
            problems.append(f"{label}: no algo-6 attempt block in the window")
        if pc.get("execTicketsSpent", 0) == 0:
            problems.append(f"{label}: no execution ticket of its Finals spent (no algo-10 block traced to it)")
        if want_fp and pc.get("fpQuantaSpent", 0) == 0:
            problems.append(f"{label}: free-prompt lane certified and on, and no quantum spent (no algo-7 block traced to it)")
        return row

    want_fp = [p for p in (a.want_fp or "").split(",") if p]
    per = {"floor": need(a.floor, "floor", any(cls_match(a.floor, p) for p in want_fp)),
           "model": need(a.model, "model", any(cls_match(a.model, p) for p in want_fp))}
    if by.get(10, 0) == 0:
        problems.append("no algo-10 round block merged in the window")
    if a.short:
        # the in-window gate: a Final's tickets mature 1,200 DAA after the Final (window_challenge, not the
        # 120-DAA short window), so no algo-10 block and no spent ticket can exist yet — noted, not failed
        moved = [p for p in problems if "algo-10" in p or "execution ticket" in p]
        problems = [p for p in problems if p not in moved]
        notes += [f"SHORT gate, not judged: {p}" for p in moved]
    for row in reg.get("classes") or []:
        if a.two_m and cls_match(row.get("classId", ""), a.two_m):
            notes.append(f"2M row {row['classId'][:8]} is {row.get('state')} (readySeatsNow {row.get('readySeatsNow')}): expected absent, not a failure")
    alarms = {}
    for p in [x for x in (a.alarm_ports or "").split(",") if x]:
        try:
            s = node_status(WsRpc(int(p)))
            alarms[p] = s.get("laneAlarm", "")
            if s.get("laneAlarm"):
                problems.append(f"node on json port {p} raises the lane alarm: {s['laneAlarm'][:160]}")
        except Exception as e:
            problems.append(f"node on json port {p} did not answer getPalwNodeStatus: {e}")
    verdict = {"pass": not problems, "problems": problems, "notes": notes, "perClass": per,
               "byAlgo": L["byAlgo"], "selectedChainByAlgo": L["selectedChainByAlgo"],
               "window": [L["fromDaa"], L["sinkDaa"]], "sink": L["sink"], "alarms": alarms}
    return verdict


# ---------------------------------------------------------------------------------------------
# snapshots: what every node at the same sink must agree on
VOLATILE_LANE = {"round", "permits", "nextRoundPermits", "virtualDaa"}


def snap(c, bonds):
    d = dag(c)
    out = {"dag": {"sink": d["sink"], "virtualDaaScore": d["virtualDaaScore"], "tips": sorted(d.get("tipHashes") or [])}}
    reg = dict(registry(c))
    reg.pop("tipDaa", None)
    reg["classes"] = sorted(reg.get("classes") or [], key=lambda r: r.get("classId", ""))
    reg["readiness"] = sorted(reg.get("readiness") or [], key=lambda r: (r.get("classId", ""), r.get("bondTxid", ""), r.get("bondIndex", 0)))
    out["registry"] = reg
    out["bonds"] = {}
    for b in bonds:
        r = dict(claims(c, b, "executor", True))
        r.pop("tipDaa", None)
        r["claims"] = sorted(r.get("claims") or [], key=lambda x: x.get("claimId", ""))
        out["bonds"][b] = r
    rl = {k: v for k, v in round_lane(c).items() if k not in VOLATILE_LANE}
    out["roundLane"] = rl
    return out


def diff_paths(a, b, path="", out=None, limit=40):
    out = [] if out is None else out
    if len(out) >= limit:
        return out
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            diff_paths(a.get(k), b.get(k), f"{path}.{k}", out, limit)
    elif isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
        for i, (x, y) in enumerate(zip(a, b)):
            diff_paths(x, y, f"{path}[{i}]", out, limit)
    elif a != b:
        out.append(f"{path}: {json.dumps(a)[:120]} != {json.dumps(b)[:120]}")
    return out


def compare(ports, bonds, out_dir=None, tries=60):
    last = None
    for t in range(tries):
        conns = [WsRpc(p) for p in ports]
        try:
            heads = [dag(c) for c in conns]
            key = lambda d: (d["sink"], d["virtualDaaScore"], tuple(sorted(d.get("tipHashes") or [])))
            if len({key(h) for h in heads}) != 1:
                last = {"stage": "tips differ", "sinks": [h["sink"][:16] for h in heads]}
                time.sleep(2)
                continue
            snaps = [snap(c, bonds) for c in conns]
            after = [dag(c) for c in conns]
            if len({key(h) for h in heads + after}) != 1:
                last = {"stage": "the DAG moved during the read"}
                continue
            ref = json.dumps(snaps[0], sort_keys=True)
            bad = {}
            for p, s in zip(ports[1:], snaps[1:]):
                if json.dumps(s, sort_keys=True) != ref:
                    bad[p] = diff_paths(snaps[0], s)
            if out_dir:
                os.makedirs(out_dir, exist_ok=True)
                for p, s in zip(ports, snaps):
                    with open(os.path.join(out_dir, f"snap-{p}.json"), "w") as f:
                        json.dump(s, f, sort_keys=True, indent=1)
            return {"agree": not bad, "sink": heads[0]["sink"], "virtualDaaScore": heads[0]["virtualDaaScore"],
                    "tries": t + 1, "differences": bad}
        finally:
            for c in conns:
                c.close()
    return {"agree": None, "inconclusive": last, "tries": tries}


def slashed(c, bonds):
    rows, total = {}, 0
    for b in bonds:
        r = claims(c, b, "executor", True)
        rows[b] = {"known": r.get("bondKnown"), "collateral": r.get("bondCollateral"), "slashed": r.get("bondSlashed"),
                   "retiringSince": r.get("bondRetiringSinceDaa")}
        total += int(r.get("bondSlashed") or 0)
    d = dag(c)
    return {"sink": d["sink"], "virtualDaaScore": d["virtualDaaScore"], "bonds": rows, "slashedTotal": total}


# ---------------------------------------------------------------------------------------------
# identity: is this node on the DRILL chain (its genesis, its premine txid) and on nothing public?
def block_present(c, h):
    try:
        b = c.call("getBlock", {"hash": h, "includeTransactions": False})
        b = b.get("block", b)
        return True, int((b.get("header") or {}).get("daaScore", -1))
    except RuntimeError as e:  # an unknown hash is an RPC error, which is the answer for a forbidden one
        return False, str(e)[:160]


def genesis(c, drill, forbidden, drill_premine, forbidden_premines, forbidden_prefixes):
    d = dag(c)
    drill_present, drill_daa = block_present(c, drill)
    forb = {h[:16]: block_present(c, h)[0] for h in forbidden}
    pp, sink = d.get("pruningPointHash", ""), d.get("sink", "")
    # a bond on premine index 1 exists on exactly one premine txid: the one this chain's genesis minted
    bond_known = {"drill": bool(claims(c, f"{drill_premine}:1", "executor", False).get("bondKnown"))}
    for p in forbidden_premines:
        bond_known[p[:16]] = bool(claims(c, f"{p}:1", "executor", False).get("bondKnown"))
    bad_prefix = [x for x in (pp, sink) for pre in forbidden_prefixes if pre and x.lower().startswith(pre.lower())]
    pruned_past_genesis = (not drill_present) and pp and pp != drill
    problems = []
    if not drill_present and not pruned_past_genesis:
        problems.append(f"the drill genesis {drill[:16]}… is not in this node's DAG and the pruning point is still genesis-era ({pp[:16]}…)")
    if drill_present and drill_daa != 0:
        problems.append(f"the drill genesis answers with daaScore {drill_daa}, not 0")
    if any(forb.values()):
        problems.append(f"a forbidden (public/retired) genesis is in this node's DAG: {[k for k, v in forb.items() if v]}")
    if not bond_known["drill"]:
        problems.append(f"no bond at the drill premine {drill_premine[:16]}…:1 — this chain's premine is not the drill's")
    if any(v for k, v in bond_known.items() if k != "drill"):
        problems.append(f"a bond exists on a forbidden premine txid: {[k for k, v in bond_known.items() if k != 'drill' and v]}")
    if bad_prefix:
        problems.append(f"the pruning point or sink carries a forbidden genesis prefix: {bad_prefix}")
    if str(d.get("network", "")).lower() not in ("testnet-12", "misaka-testnet-12", "kaspa-testnet-12"):
        # the network id spelling differs by serializer; only a different suffix is a finding
        if "12" not in str(d.get("network", "")):
            problems.append(f"network {d.get('network')} is not testnet-12")
    return {"ok": not problems, "problems": problems, "network": d.get("network"), "drillGenesisPresent": drill_present,
            "drillGenesisDaa": drill_daa, "pruningPoint": pp, "prunedPastGenesis": bool(pruned_past_genesis),
            "forbiddenGenesisPresent": forb, "bondOnPremineIndex1Known": bond_known, "sink": sink,
            "virtualDaaScore": d.get("virtualDaaScore")}


def dns(c):
    """getDnsConfirmation: testnet-12's overlay (PALW_T12_DNS_PARAMS: 6 × 20M MSK validators) stays in
    Bootstrap (rolloutStage 1: the reorg gate is not enforced) while no such validator set exists."""
    r = c.call("getDnsConfirmation", {"blockHash": ""})
    keep = ("available", "rolloutStage", "dnsConfirmed", "powConfirmed", "health", "lastDnsConfirmedAnchor",
            "lastDnsConfirmedAnchorDaaScore", "requiredStakeDepth", "note")
    out = {k: r.get(k) for k in keep}
    out["stageName"] = {0: "Launch", 1: "Bootstrap", 2: "Active"}.get(r.get("rolloutStage"), str(r.get("rolloutStage")))
    return out


# ---------------------------------------------------------------------------------------------
# C13: does every bound floor claim license? (the live defect before c8652a97: about half of the floor claims
# never did; the licence-stall fix a4dfe903 + d94d3a1b is in the drill build, so this is expected to pass)
LICENSED = ("receipt_licensed", "final")


def licence(c, bonds, cls, window, grace, min_ratio, min_bound):
    rows, tip = [], 0
    for b in bonds:
        try:
            r = claims(c, b, "executor", True)
        except Exception as e:
            print(f"# claims({b}): {e}", file=sys.stderr)
            continue
        tip = max(tip, int(r.get("tipDaa") or 0))
        rows += [x for x in r.get("claims") or [] if cls_match(x.get("classId", ""), cls)]
    lo, hi = max(0, tip - window), tip - grace
    bound = [x for x in rows if x.get("boundDaa") is not None and lo <= int(x["boundDaa"]) <= hi]
    lic = [x for x in bound if x.get("phase") in LICENSED]
    stuck = [x for x in bound if x.get("phase") == "panel_bound"]
    other = [x for x in bound if x.get("phase") not in LICENSED + ("panel_bound",)]
    ratio = (len(lic) / len(bound)) if bound else None
    # the observed windows, from the rows themselves (evidence, not configuration)
    windows = sorted({int(x["deadlineDaa"]) - int(x["boundDaa"]) for x in rows
                      if x.get("phase") == "panel_bound" and x.get("deadlineDaa") and x.get("boundDaa") is not None})
    challenge = sorted({int(x["deadlineDaa"]) - int(x["phaseDaa"]) for x in rows
                        if x.get("phase") == "receipt_licensed" and x.get("deadlineDaa") and x.get("phaseDaa")})
    brief = lambda x: {k: x.get(k) for k in ("claimId", "classId", "executorBond", "isFreePrompt", "phase", "voidReason", "acceptedDaa",
                                             "boundDaa", "reboundDaa", "phaseDaa", "deadlineDaa", "seats", "openCourts")}
    ok = ratio is not None and ratio >= min_ratio and len(bound) >= min_bound
    # every bound claim in the window (drill.sh counts each one's Valid receipts from the seats' logs and
    # takes the verdict over those with a quorum of Valid — item 10a's population)
    return {"pass": ok, "tipDaa": tip, "boundWindow": [lo, hi], "bound": len(bound), "licensed": len(lic),
            "ratio": ratio, "minRatio": min_ratio, "minBound": min_bound, "stuckPanelBound": [brief(x) for x in stuck],
            "boundThenOther": [brief(x) for x in other], "boundClaims": [brief(x) for x in bound],
            "observedReceiptWindowDaa": windows, "observedChallengeWindowDaa": challenge, "claimsSeen": len(rows)}


# ---------------------------------------------------------------------------------------------
# C14: op 186's room is the gate's room (c8652a97), and no Held reason is a utilization reading.
# Two sentences refuse for lack of room at c8652a97: the rate rule's PanelRoomExhausted, and — for a HELD
# class (class_is_held_v1: t12's 8k and 2M rows), held to its static cap past the fence (ADR-0152 T-2(b),
# checked FIRST) — ClassInflightCapped. op 186's panelRoom for a held class is min(rate room, cap − owed)
# (palw_panel_room_read_v1), so panelRoom == 0 ⇔ the gate refuses with one of the two.
ADMITTING = ("Probation", "ActiveLimited", "Active")
GATE_ROOM = "the panel has no room for a claim of class"
GATE_CAP = "claims in flight against the registry's cap of"
NOT_ADMITTING = "the model registry admits no new claim of this class now"
GATE_ROOM_RE = __import__("re").compile(r"the panel has no room for a claim of class [0-9a-f]+: (\d+) of replay in flight against a budget of (\d+) over (\d+) spans")


def variant(state):
    """getPalwModelRegistry renders `state` with {:?} (rpc/service/src/service.rs:1891): Probation and
    ActiveLimited carry fields ("Probation { probes_passed: 0 }"). Compare on the variant name only."""
    return str(state or "").split(" ", 1)[0]


def room(c, cls, bond):
    s1 = dag(c)["sink"]
    reg = registry(c)
    f = facts(c, cls, bond)
    s2 = dag(c)["sink"]
    rows = reg.get("classes") or []
    row = next((r for r in rows if cls_match(r.get("classId", ""), cls)), {})
    stable = s1 == s2
    state, tip, grace_until = variant(row.get("state")), int(reg.get("tipDaa") or 0), int(reg.get("graceUntilDaa") or 0)
    nrr = f.get("notReadyReason") or ""
    # the facts name the class gate only as "<NOT_ADMITTING sentence> [<gate's words>]" and only when no
    # earlier reason (bond, budget …) comes first (rpc/service/src/service.rs:2861): another reason masks it
    masked = bool(nrr) and not nrr.startswith(NOT_ADMITTING)
    applicable = stable and state in ADMITTING and tip >= grace_until and not row.get("isBaseClass") and not masked
    fdaa = f.get("daaScore")
    offset = (int(fdaa) - tip) if fdaa is not None else None
    refusal = "room" if GATE_ROOM in nrr else ("cap" if GATE_CAP in nrr else None)
    agree = None
    if applicable:
        agree = (int(row.get("panelRoom") or 0) == 0) == (refusal is not None)
    # when the gate refuses on the rate, its sentence carries the fold's own demand as replay over THIS
    # class's window W ("… over W spans": ceil(D·W/S), PalwPanelRateV1::inflight_replay), while op 186 prints
    # the demand A SPAN over a horizon of one span (ceil(D/S), panelHorizonSpans = 1). Read at the same DAA
    # (offset 0) they agree when W·(c−1) < gate ≤ W·c, c = op 186's panelInflightReplay (gate = 0 when c = 0).
    m = GATE_ROOM_RE.search(nrr)
    gate_inflight = int(m.group(1)) if m else None
    gate_window = max(int(m.group(3)), 1) if m else None
    replay_agree = None
    if m and offset == 0 and str(reg.get("panelInflightReplay") or "").isdigit():
        c = int(reg["panelInflightReplay"])
        replay_agree = (gate_inflight == 0) if c == 0 else (gate_window * (c - 1) < gate_inflight <= gate_window * c)
    overloaded = [{"class": r.get("classId", "")[:16], "state": r.get("state"), "reason": r.get("reason")} for r in rows
                  if "overloaded (" in (r.get("reason") or "") and "utilization" in (r.get("reason") or "")]
    return {"utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "sinkStable": stable, "sink": s2,
            "registryTipDaa": tip, "factsDaa": fdaa, "factsMinusTipDaa": offset, "class": cls[:16], "state": state,
            "stateRaw": row.get("state"), "probesPassed": row.get("probesPassed"),
            "panelRoom": row.get("panelRoom"), "inflightNow": row.get("inflightNow"), "readySeatsNow": row.get("readySeatsNow"),
            "reason": row.get("reason"), "notReadyReason": nrr, "maskedByAnotherReason": masked,
            "applicable": applicable, "roomAgreesWithGate": agree, "gateRefusal": refusal,
            "panelInflightReplay": reg.get("panelInflightReplay"), "gateInflightReplay": gate_inflight,
            "gateWindowSpans": gate_window,
            "inflightReplayAgrees": replay_agree, "panelHorizonSpans": reg.get("panelHorizonSpans"),
            "workTargetShadow": reg.get("workTargetShadow"), "overloadedReasons": overloaded}


def classes_line(c):
    """one observer row: every class's VARIANT / inflight / room / ready (/B for the base class) — no
    spaces inside a cell, so `drill.sh second` can split it — then the probes each class passed, and
    the Held reasons."""
    reg = registry(c)
    cells, probes = [], []
    for r in sorted(reg.get("classes") or [], key=lambda r: r.get("classId", "")):
        cells.append(f"{r.get('classId', '')[:8]}={variant(r.get('state'))}/{r.get('inflightNow')}/{r.get('panelRoom')}/{r.get('readySeatsNow')}"
                     + ("/B" if r.get("isBaseClass") else ""))
        probes.append(f"{r.get('classId', '')[:8]}={r.get('probesPassed')}")
    held = [f"{r.get('classId', '')[:8]}:{r.get('reason')}" for r in reg.get("classes") or [] if variant(r.get("state")) == "Held"]
    return f"{reg.get('tipDaa')}\t{' '.join(cells)}\t{' | '.join(held)}\t{' '.join(probes)}"


# ---------------------------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("dag", "status", "registry", "facts", "claims", "lanes", "gate", "snap", "slashed",
                 "genesis", "dns", "licence", "room", "classes"):
        s = sub.add_parser(name)
        s.add_argument("--port", type=int, required=True)
        s.add_argument("--field")
        s.add_argument("--class", dest="class_id")
        s.add_argument("--bond")
        s.add_argument("--bonds", default="")
        s.add_argument("--role", default="executor")
        s.add_argument("--phase")
        s.add_argument("--terminal", action="store_true")
        s.add_argument("--window-daa", type=int, default=600)
        s.add_argument("--floor")
        s.add_argument("--model")
        s.add_argument("--two-m")
        s.add_argument("--want-fp")
        s.add_argument("--alarm-ports")
        s.add_argument("--expect-heartbeat-only", action="store_true")
        s.add_argument("--short", action="store_true")
        s.add_argument("--drill-genesis")
        s.add_argument("--forbidden", default="")
        s.add_argument("--forbidden-prefixes", default="a27f8f44,d73dbf44,f6cc9576,fb1074b0,a8cabac4,1eaa6c0f")
        s.add_argument("--drill-premine")
        s.add_argument("--forbidden-premines", default="")
        s.add_argument("--grace-daa", type=int, default=20)
        s.add_argument("--min-ratio", type=float, default=0.95)
        s.add_argument("--min-bound", type=int, default=10)
    s = sub.add_parser("compare")
    s.add_argument("--ports", required=True)
    s.add_argument("--bonds", default="")
    s.add_argument("--out")
    s.add_argument("--tries", type=int, default=60)
    a = ap.parse_args()

    if a.cmd == "compare":
        r = compare([int(p) for p in a.ports.split(",")], [b for b in a.bonds.split(",") if b], a.out, a.tries)
        print(json.dumps(r, indent=1))
        sys.exit(0 if r.get("agree") else (2 if r.get("agree") is None else 1))

    c = WsRpc(a.port)
    bonds = [b for b in a.bonds.split(",") if b]
    if a.cmd == "dag":
        d = dag(c)
        print(d.get(a.field) if a.field else json.dumps(d, indent=1))
    elif a.cmd == "status":
        print(json.dumps(node_status(c), indent=1))
    elif a.cmd == "registry":
        r = registry(c)
        if a.class_id:
            r = [x for x in r.get("classes") or [] if cls_match(x.get("classId", ""), a.class_id)]
        print(json.dumps(r, indent=1))
    elif a.cmd == "facts":
        print(json.dumps(facts(c, a.class_id, a.bond), indent=1))
    elif a.cmd == "claims":
        r = claims(c, a.bond, a.role, a.terminal)
        rows = r.get("claims") or []
        if a.phase:
            rows = [x for x in rows if x.get("phase") in a.phase.split(",")]
        if a.class_id:
            rows = [x for x in rows if cls_match(x.get("classId", ""), a.class_id)]
        print(json.dumps({"tipDaa": r.get("tipDaa"), "bondKnown": r.get("bondKnown"), "bondCollateral": r.get("bondCollateral"),
                          "bondSlashed": r.get("bondSlashed"), "bondCapableClasses": r.get("bondCapableClasses"),
                          "claims": rows}, indent=1))
    elif a.cmd == "lanes":
        print(json.dumps(lanes(c, a.window_daa, bonds), indent=1))
    elif a.cmd == "gate":
        v = gate(c, a)
        print(json.dumps(v, indent=1))
        if a.expect_heartbeat_only:
            ok = (not v["pass"]) and any(p.startswith("heartbeat-only") for p in v["problems"]) and any(v["alarms"].values())
            sys.exit(0 if ok else 1)
        sys.exit(0 if v["pass"] else 1)
    elif a.cmd == "snap":
        print(json.dumps(snap(c, bonds), indent=1, sort_keys=True))
    elif a.cmd == "slashed":
        print(json.dumps(slashed(c, bonds), indent=1))
    elif a.cmd == "genesis":
        g = genesis(c, a.drill_genesis, [h for h in a.forbidden.split(",") if h], a.drill_premine,
                    [p for p in a.forbidden_premines.split(",") if p], [p for p in a.forbidden_prefixes.split(",") if p])
        print(json.dumps(g, indent=1))
        c.close()
        sys.exit(0 if g["ok"] else 1)
    elif a.cmd == "dns":
        print(json.dumps(dns(c), indent=1))
    elif a.cmd == "licence":
        v = licence(c, bonds, a.class_id, a.window_daa, a.grace_daa, a.min_ratio, a.min_bound)
        print(json.dumps(v, indent=1))
        c.close()
        sys.exit(0 if v["pass"] else 1)
    elif a.cmd == "room":
        print(json.dumps(room(c, a.class_id, a.bond)))
    elif a.cmd == "classes":
        print(classes_line(c))
    c.close()


if __name__ == "__main__":
    main()
