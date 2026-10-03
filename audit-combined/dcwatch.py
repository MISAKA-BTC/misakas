#!/usr/bin/env python3
"""audit-combined/dcwatch.py — the combined drill's PANEL / SHARE / RED-BLUE / RECOVERY verdicts (read-only JSON-wRPC to a drill node; stdlib only).

  panel [--work DIR] [--p50-max 6 --p95-max 12 --wait-max 40]
        the PANEL verdict ("verification does not jam"), read from the driver's capacity sampler (drive-state.json: cap-summary per window, cap-claims,
        cap-series) and the nodes' logs: per window the claims' bind->licence latency p50/p95 and the accepted->licence latency (PASS: bind p50 <= --p50-max,
        p95 <= --p95-max; the lead's healthy figure is 2-6 DAA), the panel-bound backlog trend (the sampler's gate: slope over the window's second half and its
        last quarter against its second: must not diverge), the seats' oldest wait (<= --wait-max), F3 producer-hold events ("NOT PRODUCING ... holding" lines)
        per node, PanelUnavailable expiries (claims voided as panel-unavailable: expect 0 for honest load) and receipts filed per seat per hour.
        Exit 0 PASS, 1 FAIL, 3 INCOMPLETE.

  share --port P --fence H' [--window 60 --step 30 --settle 20 --to DAA --work DIR --producers new4,new6 --idle-file F]
        the SHARE verdict. Walks getBlocks (verbose) and classes every block by verboseData.blockKind (REAL / FALLBACK / LEGACY_FLOOR / LEGACY_HEARTBEAT /
        EXEC) and laneClass (BLUE / EXEC / RED / ROUND). Sliding DAA windows from H' + settle: the share of REAL + EXEC blocks among all non-RED blocks
        (>= 90 %), heartbeats (<= 10 %), floor attempts (FALLBACK / LEGACY_FLOOR: only in idle windows). ELIGIBLE windows only (--producers): every REAL
        producer up for the whole window and none logged 'NOT PRODUCING ... holding' in it; the others are listed apart, so a silent producer never passes as
        idle. The table also carries, per window, the REAL attempts that turned RED (the 8k failure: fast floor attempts fill an inferring attempt's anticone),
        the DAA progress (DAA per hour from the blocks' own stamps, the longest slot gap) and — for every RED REAL attempt — the kinds of BLUE blocks in its
        anticone (floor / heartbeat / other REAL / exec), summed before and after the fence. Exit 0 PASS, 1 FAIL, 3 INCOMPLETE.

  gates --port P --fence H' [--work DIR --producers new4,new6 --state recovery.json --evd DIR]
        the combined drill's classified outputs. RELEASE GATES (all must PASS to ship): PANEL; the REAL attempts BLUE >= 90 % in every eligible window past the
        fence (floors suppressed); floor attempts only in idle windows; the recovery leg; the DAA advancing in every window; and the per-lane verdicts (dm1..dm6,
        DG-1..). GOAL METRIC, printed apart under "goal (supply-bound)" and never a gate: SHARE, REAL + EXEC >= 90 % / heartbeat <= 10 % of the consensus blocks,
        with its PASS / FAIL label. Exit 0 / 1 / 3 for the gates only.

  redblue --port P --fence H' [--window 60 --from DAA --to DAA]
        the same RED/BLUE table without the verdict, over the whole chain (before the fence too), one row per window, and the anticone kinds of up to 40
        RED REAL attempts per window named by hash.

  recovery --port P --state recovery.json [--k 20 --margin 6]
        the recovery leg's verdict. recovery.json (written by `dc.sh recovery`) holds stop_daa (both REAL producers stopped), restart_daa (started again) and
        end_daa. PASS only if: (a) the DAA kept advancing over the stop — a block at every DAA and no stamp gap over 4 slots (the drill's own slots reach ~270 s); (b) floors resumed after K
        idle slots — no BLUE FALLBACK block earlier than (the last REAL attempt + K) and the first one no later than that + --margin; (c) after the restart the
        first REAL attempt is accepted, no FALLBACK block follows later than its DAA + K, and its REAL attempts are BLUE again (>= 90 % of the post-restart REAL
        attempts). Exit 0 PASS, 1 FAIL, 3 INCOMPLETE.

  selftest   the window, anticone and recovery logic over a synthetic DAG.
"""
import argparse, json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "audit-tir"))
from rpc import call, pick  # noqa: E402

FLOOR_KINDS = ("FALLBACK", "LEGACY_FLOOR")
SLOT_S = 120


# --------------------------------------------------------------------------------------------------------------------
# the chain as a list of blocks
# --------------------------------------------------------------------------------------------------------------------
def raw_blocks(port, with_txs=False):
    dag = call(port, "getBlockDagInfo", {})
    cursor = pick(dag, "pruningPointHash", default=None)
    seen = set()
    while True:
        r = call(port, "getBlocks", {"lowHash": cursor, "includeBlocks": True, "includeTransactions": with_txs}, timeout=120)
        page = pick(r, "blocks", default=[]) or []
        fresh = [b for b in page if pick(pick(b, "header", default={}), "hash") not in seen]
        if not fresh:
            return
        for b in fresh:
            seen.add(pick(pick(b, "header", default={}), "hash"))
            yield b
        cursor = pick(pick(fresh[-1], "header", default={}), "hash")


def load_blocks(port):
    """[{hash, parents, daa, ts, kind, lane}] — kind = verboseData.blockKind, lane = verboseData.laneClass ('' on a node without the field)."""
    out = []
    for b in raw_blocks(port):
        h = pick(b, "header", default={}) or {}
        v = pick(b, "verboseData", default={}) or {}
        levels = pick(h, "parentsByLevel", default=[]) or []
        parents = list(levels[0]) if levels and isinstance(levels[0], list) else []
        out.append({"hash": str(pick(h, "hash", default="")), "parents": [str(p) for p in parents], "daa": int(pick(h, "daaScore", default=0) or 0),
                    "ts": int(pick(h, "timestamp", default=0) or 0), "kind": str(pick(v, "blockKind", default="") or ""),
                    "lane": str(pick(v, "laneClass", default="") or ""), "chain": bool(pick(v, "isChainBlock", default=False)),
                    "merged": [str(x) for x in (list(pick(v, "mergeSetBluesHashes", default=[]) or []) + list(pick(v, "mergeSetRedsHashes", default=[]) or []))]})
    return out


def kind_bucket(b):
    k = b["kind"]
    if k == "REAL": return "real"
    if k in FLOOR_KINDS: return "floor"
    if k == "LEGACY_HEARTBEAT": return "hb"
    if k == "EXEC": return "exec"
    return "other"


# --------------------------------------------------------------------------------------------------------------------
# anticone kinds of a RED REAL attempt
# --------------------------------------------------------------------------------------------------------------------
class Dag:
    def __init__(self, blocks):
        self.by = {b["hash"]: b for b in blocks}
        self.children = {}
        for b in blocks:
            for p in b["parents"]:
                self.children.setdefault(p, []).append(b["hash"])
        self.by_daa = {}
        for b in blocks:
            self.by_daa.setdefault(b["daa"], []).append(b["hash"])

    def reach(self, start, nxt, pool):
        seen, stack = set(), [start]
        while stack:
            x = stack.pop()
            for y in nxt(x):
                if y in pool and y not in seen:
                    seen.add(y)
                    stack.append(y)
        return seen

    def anticone(self, h, span=8):
        """The blocks within `span` DAA of h that are neither its past nor its future (the anticone as far as that horizon sees it)."""
        d = self.by[h]["daa"]
        pool = {x for dd in range(d - span, d + span + 1) for x in self.by_daa.get(dd, [])}
        past = self.reach(h, lambda x: self.by[x]["parents"] if x in self.by else [], pool)
        future = self.reach(h, lambda x: self.children.get(x, []), pool)
        return pool - past - future - {h}

    def blue_kinds(self, h, span=8):
        c = {"floor": 0, "hb": 0, "real": 0, "exec": 0, "other": 0}
        for x in self.anticone(h, span):
            b = self.by[x]
            if b["lane"] in ("BLUE", "EXEC"):
                c[kind_bucket(b)] += 1
        return c


# --------------------------------------------------------------------------------------------------------------------
# windows
# --------------------------------------------------------------------------------------------------------------------
def progress(sel):
    """(DAA per hour, longest slot gap in s) from the blocks' own stamps: the earliest stamp of each DAA, in order."""
    first = {}
    for b in sel:
        if b["ts"] and (b["daa"] not in first or b["ts"] < first[b["daa"]]):
            first[b["daa"]] = b["ts"]
    days = sorted(first)
    if len(days) < 2:
        return None, None
    span_h = (first[days[-1]] - first[days[0]]) / 3.6e6
    gaps = [(first[b] - first[a]) / 1000.0 for a, b in zip(days, days[1:]) if b == a + 1]
    return (round((days[-1] - days[0]) / span_h, 1) if span_h > 0 else None), (round(max(gaps), 0) if gaps else None)


def window_row(blocks, dag, a, w, idle, anticone_cap=40, span=8):
    sel_all = [b for b in blocks if a <= b["daa"] < a + w]
    sel = [b for b in sel_all if b["lane"] != "RED"]
    n = len(sel)
    c = {k: sum(1 for b in sel if b["kind"] == k) for k in ("REAL", "EXEC", "FALLBACK", "LEGACY_FLOOR", "LEGACY_HEARTBEAT")}
    c["other"] = n - sum(c.values())
    real_all = [b for b in sel_all if b["kind"] == "REAL"]
    real_red = [b for b in real_all if b["lane"] == "RED"]
    floor_all = [b for b in sel_all if b["kind"] in FLOOR_KINDS]
    ac = {"floor": 0, "hb": 0, "real": 0, "exec": 0, "other": 0}
    for b in real_red[:anticone_cap]:
        for k, v in dag.blue_kinds(b["hash"], span).items():
            ac[k] += v
    dph, gap = progress(sel_all)
    is_idle = any(i0 <= a and a + w <= i1 for i0, i1 in idle) or c["REAL"] == 0
    return {"lo": a, "hi": a + w, "n": n, **c, "share": round((c["REAL"] + c["EXEC"]) / n, 3) if n else None,
            "hb": round(c["LEGACY_HEARTBEAT"] / n, 3) if n else None, "floor": c["FALLBACK"] + c["LEGACY_FLOOR"], "idle": is_idle,
            "real_pct": round(c["REAL"] / n, 3) if n else None, "floor_pct": round((c["FALLBACK"] + c["LEGACY_FLOOR"]) / n, 3) if n else None,
            "real_total": len(real_all), "real_red": len(real_red), "floor_red": sum(1 for b in floor_all if b["lane"] == "RED"),
            "anticone_of_red_real": ac, "daa_per_h": dph, "max_gap_s": gap}


def windows(blocks, lo, hi, w, step, idle):
    dag = Dag(blocks)
    out, a = [], lo
    while a + w <= hi:
        out.append(window_row(blocks, dag, a, w, idle))
        a += step
    return out


HEAD = (f"{'window':>13} {'n':>4} {'REAL':>5} {'RED':>4} {'EXEC':>5} {'FALLB':>5} {'LFLOOR':>6} {'LHB':>4} {'other':>5} {'share':>6} {'hb':>6} {'fl%':>6} floor "
        f"{'DAA/h':>6} {'gap_s':>6}  anticone(RED REAL): floor/hb/real/exec  idle")


def line(w):
    ac = w["anticone_of_red_real"]
    return (f"{w['lo']:>6}-{w['hi']:<6} {w['n']:>4} {w['REAL']:>5} {w['real_red']:>4} {w['EXEC']:>5} {w['FALLBACK']:>5} {w['LEGACY_FLOOR']:>6} {w['LEGACY_HEARTBEAT']:>4} {w['other']:>5} "
            f"{w['share'] if w['share'] is not None else '-':>6} {w['hb'] if w['hb'] is not None else '-':>6} {w['floor_pct'] if w['floor_pct'] is not None else '-':>6} {w['floor']:>5} "
            f"{str(w['daa_per_h']) if w['daa_per_h'] is not None else '-':>6} {str(w['max_gap_s']) if w['max_gap_s'] is not None else '-':>6}  "
            f"{ac['floor']}/{ac['hb']}/{ac['real']}/{ac['exec']}  {'idle' if w['idle'] else ''}")


def sum_anticone(ws):
    t = {"floor": 0, "hb": 0, "real": 0, "exec": 0, "other": 0}
    red = real = 0
    for w in ws:
        red += w["real_red"]
        real += w["real_total"]
        for k in t:
            t[k] += w["anticone_of_red_real"][k]
    return red, real, t


# --------------------------------------------------------------------------------------------------------------------
# eligibility (the driver's knowledge: node up, producer not held)
# --------------------------------------------------------------------------------------------------------------------
def daa_clock(work):
    """[(epoch seconds, DAA)] from the driver's sampler (samples.tsv): the map from a window to wall time."""
    import time
    pts = []
    path = os.path.join(work, "samples.tsv")
    if os.path.exists(path):
        for ln in open(path):
            parts = ln.split("\t")
            if len(parts) >= 2 and parts[1].strip().isdigit():
                try:
                    pts.append((time.mktime(time.strptime(parts[0].strip(), "%Y-%m-%d %H:%M:%S")), int(parts[1])))
                except ValueError:
                    pass
    return pts


def eligible_windows(ws, work, producers):
    """A window is ELIGIBLE when every producer node was up for all of it and logged no 'NOT PRODUCING ... holding' line inside it (it had claims issuable);
    a silent or held producer must not make the window look idle."""
    import re, time
    clock = daa_clock(work)

    def t_of(daa):
        for t, d in clock:
            if d >= daa:
                return t
        return None
    ups = {}
    for n in producers:
        iv, start = [], None
        ev = os.path.join(work, n, "events.log")
        for ln in (open(ev) if os.path.exists(ev) else []):
            m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) (START|STOP)", ln)
            if not m:
                continue
            t = time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S"))
            if m.group(2) == "START":
                start = t
            elif start is not None:
                iv.append((start, t))
                start = None
        if start is not None:
            iv.append((start, 1e18))
        holds = []
        lg = os.path.join(work, n, "kaspad.out")
        for ln in (open(lg, errors="replace") if os.path.exists(lg) else []):
            if "NOT PRODUCING" in ln and "holding" in ln:
                m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)", ln)
                if m:
                    holds.append(time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S")))
        ups[n] = (iv, holds)
    for w in ws:
        t0, t1 = t_of(w["lo"]), t_of(w["hi"])
        ok = t0 is not None and t1 is not None
        why = []
        if not ok:
            why.append("window not on the sampler's clock")
        for n, (iv, holds) in ups.items():
            if ok and not any(a <= t0 and t1 <= b for a, b in iv):
                ok = False
                why.append(f"{n} not up for the window")
            if ok and any(t0 <= h <= t1 for h in holds):
                ok = False
                why.append(f"{n} held (NOT PRODUCING)")
        w["eligible"], w["why"] = ok, "; ".join(why)
    return ws


# --------------------------------------------------------------------------------------------------------------------
# share
# --------------------------------------------------------------------------------------------------------------------
# --------------------------------------------------------------------------------------------------------------------
# the floor rule's states (lane RS's redesign, 10-04): floors are HEADER-invalid unless the block's state is Idle
# --------------------------------------------------------------------------------------------------------------------
IDLE_SLOTS, PROBE_SLOTS, COOL_SLOTS = 20, 8, 20       # floor_idle_slots, probe_slots, probe_cooldown_slots (ADR-0165, RS 6f0cac1a3)


def floor_states(blocks, fence=0, idle=IDLE_SLOTS, probe=PROBE_SLOTS, cool=COOL_SLOTS):
    """MODEL of ADR-0165's A'' state machine (RS 6f0cac1a3) — the nodes' own `[palw-floor-state]` lines win where they exist (floor_states_best):
         Normal  a BLUE REAL attempt was accepted at last_blue; floors are refused up to DAA last_blue + `idle` and accepted again from last_blue + idle + 1;
         Idle    floors are accepted; a RED REAL attempt accepted in Idle opens a Probe unless the previous UNANSWERED probe ended less than `cool` slots ago;
         Probe   opened at DAA r: floors are refused until r + `probe`; a RED REAL never extends it; a BLUE REAL from ANY mode makes the state Normal{last_blue = DAA}.
       Returns ({daa: state the blocks of that DAA are judged under}, [probe start DAAs]). The events of a DAA move the state from the next DAA on."""
    real_blue, real_any = set(), set()
    for b in blocks:
        if b["kind"] == "REAL" and b["daa"] >= fence:
            real_any.add(b["daa"])
            if b["lane"] in ("BLUE", "EXEC"):
                real_blue.add(b["daa"])
    if not blocks:
        return {}, []
    lo, hi = max(fence, min(b["daa"] for b in blocks)), max(b["daa"] for b in blocks)
    state, last_blue, probe_at, last_probe_end = "Idle", None, None, None
    by, probes = {}, []
    for d in range(lo, hi + 1):
        if state == "Normal" and last_blue is not None and d > last_blue + idle:
            state = "Idle"
        if state == "Probe" and d - probe_at >= probe:
            state, last_probe_end = "Idle", probe_at + probe          # an unanswered probe ended: the next may open `cool` slots after it
        by[d] = state
        if d in real_blue:
            state, last_blue = "Normal", d
        elif d in real_any and state == "Idle" and (last_probe_end is None or d >= last_probe_end + cool):
            state, probe_at = "Probe", d
            probes.append(d)
    return by, probes


def floor_blocks(blocks, fence=0):
    """The floor attempts that stand in the DAG past the fence (kind FALLBACK: a header-invalid one never gets in)."""
    return [b for b in blocks if b["kind"] == "FALLBACK" and b["daa"] >= fence]


def gather_claims(port, work):
    """Every claim of every seat bond and of the drill's post-genesis bonds (getPalwClaims, executor role, terminal included): {claimId: row}."""
    work = os.path.expanduser(work)
    bonds = []
    try:
        bonds += [x["bond_outpoint"] for x in json.load(open(os.path.join(work, "keyring", "manifest.json")))["seats"]]
    except (OSError, ValueError, KeyError):
        pass
    for f in sorted(os.listdir(os.path.join(work, "liars"))) if os.path.isdir(os.path.join(work, "liars")) else []:
        try:
            bonds.append(json.load(open(os.path.join(work, "liars", f)))["bond_outpoint"])
        except (OSError, ValueError, KeyError):
            pass
    rows = {}
    for bond in bonds:
        r = call(port, "getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0})
        for c in (pick(r, "claims", default=[]) or []):
            rows[str(pick(c, "claimId"))] = c
    return rows


def hold_lines(work, nodes=("new0",), pattern=None):
    """The compliant floor producers' own log: lines saying they hold (the policy's default: no floor attempt outside Idle)."""
    import re
    pat = re.compile(pattern or os.environ.get("HOLD_PATTERN", r"\[palw-producer\] holding:[^\n]*idle-only fallback"), re.I)    # RS's hold line, marker 'idle-only fallback'
    out = {}
    for n in nodes:
        lg = os.path.join(os.path.expanduser(work), n, "kaspad.out")
        out[n] = sum(1 for ln in open(lg, errors="replace") if pat.search(ln)) if os.path.exists(lg) else None
    return out


def node_floor_log(work, nodes=("new0", "new1", "new2", "new3")):
    """The nodes' own transition lines (RS, ADR-0165): `[palw-floor-state] daa=<N> block=<hash> <from>-><to> last_blue=<n|-> until=<n|-> last_probe_end=<n|->` — one per chain
    block whose fold moved the state; from = the STORED parent state, to = the state after the block's last event, the three fields describe the NEW state.
    {node: [{daa, frm, to, last_blue, until, last_probe_end}]} (a missing field is None)."""
    import re
    pat = re.compile(r"\[palw-floor-state\] daa=(\d+) block=\S+ (idle|probe|normal)->(idle|probe|normal) last_blue=(\S+) until=(\S+) last_probe_end=(\S+)", re.I)
    num = lambda x: int(x) if x.isdigit() else None  # noqa: E731
    out = {}
    for n in nodes:
        lg = os.path.join(os.path.expanduser(work), n, "kaspad.out")
        if not os.path.exists(lg):
            continue
        rows, seen = [], set()
        for ln in open(lg, errors="replace"):
            m = pat.search(ln)
            if m:
                row = (int(m.group(1)), m.group(2).title(), m.group(3).title(), num(m.group(4)), num(m.group(5)), num(m.group(6)))
                if row not in seen:
                    seen.add(row)
                    rows.append({"daa": row[0], "frm": row[1], "to": row[2], "last_blue": row[3], "until": row[4], "last_probe_end": row[5]})
        if rows:
            out[n] = sorted(rows, key=lambda r: r["daa"])
    return out


def node_floor_transitions(work, nodes=("new0", "new1", "new2", "new3")):
    """{node: [(daa, from, to)]} — the short form of node_floor_log."""
    return {n: [(r["daa"], r["frm"], r["to"]) for r in rows] for n, rows in node_floor_log(work, nodes).items()}


def time_advance(state, last_blue, until, t, idle=IDLE_SLOTS):
    """The stored state advanced to DAA t (RS's fold): Normal with t - last_blue > idle is already Idle; Probe with t >= until is already Idle."""
    if state == "Normal" and last_blue is not None and t - last_blue > idle:
        return "Idle"
    if state == "Probe" and until is not None and t >= until:
        return "Idle"
    return state


def judged_states_from_log(rows, lo, hi, idle=IDLE_SLOTS):
    """{daa: state a floor merged by a chain block of that DAA is judged under}, from one node's transition rows. A block with no logged line leaves the stored state alone
    (time-advanced to its DAA); a block with a line judges the floors it merges ahead of its own event under time_advance(from) and those after it under `to`: the benefit of the
    doubt, Idle if either says Idle (no false FAIL). The stored state's last_blue / until come from the previous line's fields (they describe the NEW state)."""
    by = {}
    stored = ("Idle", None, None)                         # (state, last_blue, until) of the stored state before the blocks of DAA t
    rows = sorted(rows, key=lambda r: r["daa"])
    i = 0
    for t in range(lo, hi + 1):
        while i < len(rows) and rows[i]["daa"] < t:       # lines of earlier blocks: their `to` is the stored state now
            stored = (rows[i]["to"], rows[i]["last_blue"], rows[i]["until"])
            i += 1
        here = []
        while i < len(rows) and rows[i]["daa"] == t:
            here.append(rows[i])
            i += 1
        before = time_advance(stored[0], stored[1], stored[2], t, idle)
        if here:
            after = here[-1]["to"]
            by[t] = "Idle" if "Idle" in (before, after) else after
            stored = (after, here[-1]["last_blue"], here[-1]["until"])
        else:
            by[t] = before
    return by


def floor_states_best(blocks, fence, work=None, lag=None, **kw):
    """(states, probes, source, note): the node's own `[palw-floor-state]` transitions where it logs them (the node decides, my model only cross-checks), else the model."""
    model, mprobes = floor_states(blocks, fence, **kw)
    log = node_floor_log(work) if work else {}
    if not log or not model:
        return model, mprobes, "model", "no [palw-floor-state] lines in the nodes' logs: the states are RECONSTRUCTED by the model"
    node = max(log, key=lambda n: len(log[n]))
    by = judged_states_from_log(log[node], min(model), max(model), kw.get("idle", IDLE_SLOTS))
    diff = [d for d in model if model[d] != by.get(d)]
    probes = sorted({r["daa"] for r in log[node] if r["to"] == "Probe"})
    note = f"states from {node}'s [palw-floor-state] log ({len(log[node])} transitions; a floor merged at a block with a line counts as Idle if either its time-advanced stored state or the new state is); the model differs at {len(diff)} of {len(model)} DAA" + (f" (first: {diff[:5]})" if diff else "")
    return by, probes, f"node:{node}", note


def gate_floor_policy(blocks, fence, claims, holds, states=None, **kw):
    """RELEASE GATE: floors outside Idle EARN NOTHING (the fold refuses them: no claim row names such a floor block as its accepted block) and the compliant
    producers log holds. A floor that stands in the DAG outside Idle is not a failure by itself (a compliant producer can lose a race at a state change);
    an EARNED one is."""
    by, probes = states if states else floor_states(blocks, fence, **kw)
    if not any(b["kind"] == "REAL" and b["daa"] >= fence for b in blocks):
        return 3, "INCOMPLETE: no REAL attempt past the fence yet (the states never left Idle)"
    fl = floor_blocks(blocks, fence)
    outside = [b for b in fl if by.get(b["daa"], "Idle") != "Idle"]
    earned_by = {str(pick(c, "acceptedBlock", default="")) for c in claims.values()}
    earned = [b for b in outside if b["hash"] in earned_by]
    visited = {st: sum(1 for v in by.values() if v == st) for st in ("Idle", "Probe", "Normal")}
    nonidle = visited["Probe"] + visited["Normal"]
    held = sum(v or 0 for v in holds.values())
    txt = (f"{len(fl)} floor blocks stand past the fence, {len(outside)} of them outside Idle, {len(earned)} of those earned a claim; slots Idle {visited['Idle']}, Probe {visited['Probe']}, "
           f"Normal {visited['Normal']}; hold lines {holds}")
    if earned:
        return 1, f"FAIL: floors outside Idle earned claims at DAA " + ", ".join(str(b["daa"]) for b in earned[:8]) + " — " + txt
    if not nonidle:
        return 3, "INCOMPLETE: the chain never left Idle — " + txt
    if held == 0:
        return 3, "INCOMPLETE: no hold line in the compliant producers' logs (set HOLD_PATTERN to the producer's wording) — " + txt
    if not outside:
        return 0, "PASS (nothing to refuse: every floor stood in Idle; the refusal itself is shown by the policy-ignoring leg, report-only): " + txt
    return 0, "PASS: " + txt


def gate_states_daa(blocks, fence, max_gap_s=4 * SLOT_S, states=None, **kw):
    """RELEASE GATE: the DAA advances through every state — Idle, Probe and Normal each visited, and in every run of one state a block at every DAA and no stamp gap
    over four slots."""
    by, _ = states if states else floor_states(blocks, fence, **kw)
    if not by:
        return 3, "INCOMPLETE: no blocks"
    first = {}
    for b in blocks:
        if b["ts"] and (b["daa"] not in first or b["ts"] < first[b["daa"]]):
            first[b["daa"]] = b["ts"]
    have = {b["daa"] for b in blocks}
    seen, bad = set(), []
    for d, st in by.items():
        seen.add(st)
        if d not in have:
            bad.append(f"DAA {d} ({st}) has no block")
        elif d + 1 in first and d in first and by.get(d + 1) == st and (first[d + 1] - first[d]) / 1000.0 > max_gap_s:
            bad.append(f"DAA {d}->{d + 1} ({st}) took {(first[d + 1] - first[d]) / 1000.0:.0f} s")
    missing = [st for st in ("Idle", "Probe", "Normal") if st not in seen]
    if bad:
        return 1, "FAIL: " + "; ".join(bad[:6])
    if missing:
        return 3, f"INCOMPLETE: no DAA was seen in {missing} yet"
    return 0, "PASS: every DAA of every state has a block, none over 4 slots"


def gate_red2blue(blocks, st, probe=PROBE_SLOTS):
    """RELEASE GATE (RED -> BLUE recovery): after the idle stretch of the recovery leg the first REAL attempt may be RED, but a BLUE one follows inside the probe."""
    after = sorted(b["daa"] for b in blocks if b["kind"] == "REAL" and b["daa"] > int(st["restart_daa"]))
    if not after:
        return 3, "INCOMPLETE: no REAL attempt after the restart yet"
    r1 = after[0]
    first = [b for b in blocks if b["kind"] == "REAL" and b["daa"] == r1]
    blue = sorted(b["daa"] for b in blocks if b["kind"] == "REAL" and b["lane"] in ("BLUE", "EXEC") and b["daa"] >= r1)
    colour = "BLUE" if any(b["lane"] in ("BLUE", "EXEC") for b in first) else "RED"
    if not blue:
        return 3, f"INCOMPLETE: the first REAL attempt after the restart (DAA {r1}, {colour}) has no BLUE successor yet"
    if blue[0] - r1 > probe:
        return 1, f"FAIL: the first REAL attempt after the restart (DAA {r1}, {colour}); the first BLUE one only at DAA {blue[0]} ({blue[0] - r1} slots, probe = {probe})"
    return 0, f"PASS: first REAL attempt after the restart at DAA {r1} ({colour}), BLUE at DAA {blue[0]} ({blue[0] - r1} slots, inside the probe of {probe})"


def _stale_ctx(blocks, fence, st, states=None, **kw):
    s0, s1 = int(st["start_daa"]), int(st["end_daa"])
    reals = [b for b in blocks if b["kind"] == "REAL" and s0 <= b["daa"] < s1]
    by, probes = states if states else floor_states(blocks, fence, **kw)
    inside = [p for p in probes if s0 <= p < s1]
    fl = sorted(b["daa"] for b in floor_blocks(blocks, fence) if s0 <= b["daa"] < s1)
    return s0, s1, reals, inside, fl


def gate_stale(blocks, fence, st, states=None, **kw):
    """RELEASE GATE (RED-only cannot keep floors refused), FIRST CYCLE: in the stale leg every REAL attempt lands RED; a probe opens, and floors are valid again after
    `probe_slots`. st = {start_daa, end_daa}. (The cooldown between probes is gate_cool.)"""
    probe = kw.get("probe", PROBE_SLOTS)
    s0, s1, reals, inside, fl = _stale_ctx(blocks, fence, st, states, **kw)
    if not reals:
        return 3, "INCOMPLETE: no REAL attempt in the stale leg"
    blue = [b for b in reals if b["lane"] in ("BLUE", "EXEC")]
    if len(blue) > 0.1 * len(reals):
        return 3, f"INCOMPLETE: the injection is not stale enough — {len(blue)} of {len(reals)} REAL attempts in the leg are BLUE"
    if not inside:
        return 3, "INCOMPLETE: no probe in the stale leg yet"
    p = inside[0]
    nxt = inside[1] if len(inside) > 1 else s1
    bad = []
    if any(p < d < p + probe for d in fl):
        bad.append(f"a floor stood inside the probe at DAA {p}")
    if not any(p + probe <= d < nxt for d in fl):
        bad.append(f"no floor stood after the probe at DAA {p} ended (DAA {p + probe}..{nxt - 1})")
    if bad:
        return 1, "FAIL: " + "; ".join(bad)
    return 0, f"PASS: {len(reals)} stale REAL attempts (all RED), the first probe at DAA {p}, floors valid again at DAA {next(d for d in fl if d >= p + probe)} (probe {probe} slots)"


def gate_cool(blocks, fence, st, states=None, **kw):
    """RELEASE GATE: probes come no more often than every `cooldown` — two probes in the stale leg, spaced >= cooldown, and a floor between them."""
    cool, probe = kw.get("cool", COOL_SLOTS), kw.get("probe", PROBE_SLOTS)
    s0, s1, reals, inside, fl = _stale_ctx(blocks, fence, st, states, **kw)
    if len(inside) < 2:
        return 3, f"INCOMPLETE: {len(inside)} probe(s) in the stale leg so far (need 2 to see the cooldown)"
    gaps = [b - a for a, b in zip(inside, inside[1:])]
    if min(gaps) < probe + cool:
        return 1, f"FAIL: probes {inside} (spacing {gaps}) are closer than probe + cooldown = {probe + cool} (the cooldown runs from the END of an unanswered probe)"
    return 0, f"PASS: probes at DAA {inside}, spacing {gaps} >= probe {probe} + cooldown {cool}"


def gate_delay_active(work, node="new6", flag="--palw-drill-real-submit-delay-s"):
    """The 8k emulation is really on: the delayed producer's argv carries the flag."""
    f = os.path.join(os.path.expanduser(work), node, "args.redacted")
    if not os.path.exists(f):
        return 3, f"INCOMPLETE: no argv record for {node}"
    args = [ln.strip() for ln in open(f) if flag in ln]
    if not args:
        return 1, f"FAIL: {node} runs without {flag}: the BLUE rate below is NOT under the delay injection"
    return 0, f"PASS: {node} runs with {args[0]}"


# --------------------------------------------------------------------------------------------------------------------
# P2's anchor fix for A'' (branch anchor/window): admission within 2 audit periods, the execution lane's seeding, the panel bind wait
# --------------------------------------------------------------------------------------------------------------------
AUDIT_PERIOD = 100


def milestones(work):
    """{(asset, state): daa} from the driver's milestones.tsv ('class:<asset>:<State>\\t<daa>\\t<time>')."""
    out = {}
    path = os.path.join(os.path.expanduser(work), "milestones.tsv")
    if os.path.exists(path):
        for ln in open(path):
            parts = ln.rstrip("\n").split("\t")
            if len(parts) >= 2 and parts[0].startswith("class:") and parts[1].isdigit():
                _, asset, st = parts[0].split(":", 2)
                out.setdefault((asset, st), int(parts[1]))
    return out


def gate_admission(work, states, restart1, tip, period=AUDIT_PERIOD, periods=2):
    """G-A1: with REAL flowing and floors held (the state is Normal), a newly registered class reaches Probation within `periods` audit periods of `period` DAA."""
    ms = milestones(work)
    by, _ = states
    rows, bad, wait = [], [], []
    for (asset, st), r in sorted(ms.items(), key=lambda kv: kv[1]):
        if st != "Candidate" or r < restart1:
            continue
        if by.get(r, "Idle") != "Normal":
            rows.append(f"{asset}: registered at DAA {r} in state {by.get(r, 'Idle')} (not Normal: not the condition)")
            wait.append(asset)
            continue
        p = ms.get((asset, "Probation"))
        if p is not None:
            (rows if p - r <= periods * period else bad).append(f"{asset}: Candidate {r} -> Probation {p} ({p - r} DAA = {(p - r) / period:.2f} periods)")
        elif tip - r > periods * period:
            bad.append(f"{asset}: Candidate {r}, no Probation after {tip - r} DAA (> {periods} periods)")
        else:
            wait.append(asset)
            rows.append(f"{asset}: Candidate {r}, Probation not yet (limit DAA {r + periods * period})")
    if bad:
        return 1, "FAIL: " + "; ".join(bad) + (" | ok: " + "; ".join(rows) if rows else "")
    if not rows:
        return 3, "INCOMPLETE: no class was registered with REAL flowing yet (REGISTER_LATE)"
    if wait:
        return 3, "INCOMPLETE: " + "; ".join(rows)
    return 0, "PASS: " + "; ".join(rows)


def exec_sample_once(port):
    r = call(port, "getPalwRoundLane", {"executionsLimit": 0})
    h = pick(r, "health", default=None) or {}
    return [int(pick(r, "virtualDaa", default=0) or 0), int(pick(r, "span", default=0) or 0), int(bool(pick(r, "armed", default=False))), int(bool(pick(r, "open", default=False))),
            len(pick(r, "permits", default=[]) or []), int(pick(r, "nextRoundPermits", default=0) or 0), int(pick(r, "finalsSpan", default=0) or 0), int(pick(r, "finals", default=0) or 0),
            int(pick(r, "acceptedInSpan", default=0) or 0), int(pick(h, "acceptedTotal", default=0) or 0), int(pick(h, "refusedTotal", default=0) or 0), int(pick(r, "scheduleSpanDaa", default=1) or 1)]


def exec_sampler(a):
    """Background sampler: every --interval s one row of getPalwRoundLane (virtualDaa, span, armed, open, permits, nextRoundPermits, finalsSpan, finals, acceptedInSpan,
    accepted_total, refused_total, scheduleSpanDaa) appended to --out (P2 can replay the exact seeding from it)."""
    import time
    while True:
        try:
            row = exec_sample_once(a.port)
            with open(os.path.expanduser(a.out), "a") as f:
                f.write(time.strftime("%F %T") + "\t" + "\t".join(map(str, row)) + "\n")
        except Exception:  # noqa: BLE001 — a node mid-restart is a missing sample
            pass
        time.sleep(a.interval)
    return 0


def read_exec_samples(path):
    rows = []
    p = os.path.expanduser(path)
    if os.path.exists(p):
        for ln in open(p):
            f = ln.rstrip("\n").split("\t")
            if len(f) >= 13:
                try:
                    rows.append({"daa": int(f[1]), "span": int(f[2]), "armed": f[3] == "1", "open": f[4] == "1", "permits": int(f[5]), "next": int(f[6]), "finals_span": int(f[7]),
                                 "finals": int(f[8]), "accepted_in_span": int(f[9]), "acc_total": int(f[10]), "ref_total": int(f[11])})
                except ValueError:
                    pass
    return rows


def gate_exec_seed(samples, fence, maturity=120, grace=64, min_snapshots=5):
    """G-A2: the execution lane's schedule seeding, read from getPalwRoundLane per span (one span = one DAA on this ruleset): of the snapshots created after the fence
    (a span whose recorded finals are > 0), the share whose due window [F + maturity, F + maturity + grace + 8] holds a span with a non-empty schedule (permits > 0). APPROXIMATION
    of the fold's queue rule (one snapshot per opening, oldest first, seeded iff the previous span recorded an anchor): the raw samples are kept for an exact replay."""
    post = [x for x in samples if x["daa"] >= fence]
    if not post:
        return 3, "INCOMPLETE: no round-lane sample past the fence"
    if not any(x["armed"] for x in post):
        return 3, "INCOMPLETE: the round lane is not armed on this chain"
    tip = max(x["daa"] for x in post)
    sched = {}
    snaps = {}
    for x in post:
        sched[x["span"]] = max(sched.get(x["span"], 0), x["permits"])
        if x["finals"] > 0:
            snaps.setdefault(x["finals_span"] or x["span"], x["finals"])
    matured = [f for f in snaps if f + maturity + grace + 8 <= tip]
    if len(matured) < min_snapshots:
        return 3, f"INCOMPLETE: {len(matured)} snapshots whose window has elapsed ({len(snaps)} created, {len(post)} samples) — need {min_snapshots}"
    seeded = [f for f in matured if any(sched.get(s, 0) > 0 for s in range(f + maturity, f + maturity + grace + 9))]
    share = len(seeded) / len(matured)
    txt = f"{len(seeded)} of {len(matured)} matured snapshots seeded ({100 * share:.1f} %), spans with a schedule {sum(1 for v in sched.values() if v > 0)} of {len(sched)}"
    return (0 if share >= 0.95 else 1), ("PASS: " if share >= 0.95 else "FAIL: ") + txt


def _pct(xs, q):
    if not xs:
        return None
    xs = sorted(xs)
    return xs[min(len(xs) - 1, max(0, int(round(q * (len(xs) - 1)))))]


def bind_waits(claims, blocks, fence, operator_bonds, tip, settle=20, lo=None, hi=None):
    """{'operator': [waits], 'non-operator': [waits]} — REAL claims (accepted in a REAL block) accepted after fence + settle, accepted -> PanelBound in DAA; a claim not bound yet counts
    as tip - accepted (a lower bound)."""
    by_hash = {b["hash"]: b for b in blocks}
    out = {"operator": [], "non-operator": []}
    for c in claims.values():
        acc = pick(c, "acceptedDaa", default=None)
        blk = by_hash.get(str(pick(c, "acceptedBlock", default="")))
        if acc is None or not blk or blk["kind"] != "REAL":
            continue
        acc = int(acc)
        if acc < fence + settle or (lo is not None and acc < lo) or (hi is not None and acc >= hi):
            continue
        bd = pick(c, "boundDaa", default=None)
        wait = (int(bd) - acc) if bd is not None else tip - acc
        out["operator" if str(pick(c, "executorBond", default="")) in operator_bonds else "non-operator"].append(wait)
    return out


def operator_bonds_of(work):
    try:
        return {x["bond_outpoint"] for x in json.load(open(os.path.join(os.path.expanduser(work), "keyring", "manifest.json")))["seats"]}
    except (OSError, ValueError, KeyError):
        return set()


def gate_bind_wait(claims, blocks, fence, work, tip, limit=20, min_n=10):
    """G-A3: the panel bind wait (claim accepted -> PanelBound) of REAL claims, p95 <= `limit` DAA, split operator / non-operator bond."""
    w = bind_waits(claims, blocks, fence, operator_bonds_of(work), tip)
    parts, bad, wait = [], [], []
    for g in ("operator", "non-operator"):
        xs = w[g]
        if len(xs) < min_n:
            parts.append(f"{g}: n={len(xs)} (< {min_n})")
            wait.append(g)
            continue
        p95 = _pct(xs, 0.95)
        parts.append(f"{g}: n={len(xs)} p50 {_pct(xs, 0.5)} p95 {p95} max {max(xs)}")
        if p95 > limit:
            bad.append(g)
    if bad:
        return 1, f"FAIL (p95 > {limit} DAA: {', '.join(bad)}): " + " | ".join(parts)
    if wait:
        return 3, "INCOMPLETE: " + " | ".join(parts)
    return 0, f"PASS (p95 <= {limit} DAA): " + " | ".join(parts)


def bind_wait_report(claims, blocks, fence, work, tip, st):
    """REPORT-ONLY: the bind wait while only NON-operator REAL flows (leg O: the operator producers stopped). The beacon-floor policy is expected to cap it near 60 slots."""
    lo, hi = int(st["start_daa"]), int(st["end_daa"])
    w = bind_waits(claims, blocks, fence, operator_bonds_of(work), tip, settle=0, lo=lo, hi=hi)
    xs, ys = w["non-operator"], w["operator"]
    if not xs:
        return [f"only non-operator REAL (leg O, DAA {lo}..{hi}): no non-operator REAL claim accepted in the leg"]
    return [f"only non-operator REAL (leg O, DAA {lo}..{hi}): n={len(xs)} accepted->bound p50 {_pct(xs, 0.5)} p95 {_pct(xs, 0.95)} max {max(xs)} DAA (the beacon-floor policy is expected to cap it near 60 slots); "
            f"operator claims in the leg: {len(ys)}"]


# --------------------------------------------------------------------------------------------------------------------
# G-R1: capacity riders (ADR-0164 F-M1 / ADR-0167 D2) admitted through the real processor
# --------------------------------------------------------------------------------------------------------------------
RIDER_TAG = 95


def rider_objects(port, fence=0):
    """[(daa, block hash, lead claim id, riders offered)] — every AttemptRidersV1 (tag 95) object the chain carries: a lifecycle payload is {version: u16, tag: u8, lead: 64 bytes,
    riders: borsh Vec (u32 LE length)}."""
    out = []
    for b in raw_blocks(port, with_txs=True):
        h = pick(b, "header", default={}) or {}
        daa = int(pick(h, "daaScore", default=0) or 0)
        if daa < fence:
            continue
        for tx in (pick(b, "transactions", default=[]) or []):
            sub, payload = str(pick(tx, "subnetworkId", default="")), str(pick(tx, "payload", default=""))
            if not sub.startswith("4b") or len(payload) < 150:
                continue
            raw = bytes.fromhex(payload)
            if len(raw) >= 71 and raw[2] == RIDER_TAG:
                out.append((daa, str(pick(h, "hash", default="")), raw[3:67].hex(), int.from_bytes(raw[67:71], "little")))
    return out


def rider_log_counts(work, nodes=("new4", "new6", "new9")):
    """The producers' own counts: riders queued ('queues N rider(s) of lead …') and not built ('rider i of … not built'), and the nodes' refusals ('riders refused')."""
    import re
    q = nb = refused = 0
    for n in nodes:
        lg = os.path.join(os.path.expanduser(work), n, "kaspad.out")
        if os.path.exists(lg):
            for ln in open(lg, errors="replace"):
                m = re.search(r"queues (\d+) rider\(s\) of lead", ln)
                if m:
                    q += int(m.group(1))
                elif re.search(r"rider \d+ of \S+ not built", ln):
                    nb += 1
    for lg in __import__("glob").glob(os.path.join(os.path.expanduser(work), "new*", "kaspad.out")):
        for ln in open(lg, errors="replace"):
            if "riders refused" in ln or re.search(r"rider \d+ of [0-9a-f]+: ", ln):
                refused += 1
    return {"queued": q, "not_built": nb, "refused_lines": refused}


def _int(x, default=0):
    try:
        return int(str(x))
    except (TypeError, ValueError):
        return default


def rider_claims(objs, claims):
    """For each carried object: its lead's row and the companion claims (the riders): same executor bond and class, accepted within 9 DAA of the lead, the escrow of a rider is
    floor(E / (1 + n)) and the lead keeps E minus n of them, so a companion's escrow is within n + 1 sompi of the lead's (post-batch) escrow. -> [{lead, offered, riders: [rows]}]"""
    out = []
    for daa, blk, lead, n in objs:
        lr = claims.get(lead)
        if lr is None:
            out.append({"lead": lead, "offered": n, "row": None, "riders": [], "daa": daa})
            continue
        acc, bond, cls, esc = _int(pick(lr, "acceptedDaa")), str(pick(lr, "executorBond", default="")), str(pick(lr, "classId", default="")), _int(pick(lr, "escrowSompi"))
        comp = [c for cid, c in claims.items() if cid != lead and str(pick(c, "executorBond", default="")) == bond and str(pick(c, "classId", default="")) == cls
                and acc <= _int(pick(c, "acceptedDaa")) <= acc + 9 and abs(_int(pick(c, "escrowSompi")) - esc) <= n + 1]
        out.append({"lead": lead, "offered": n, "row": lr, "riders": comp, "daa": daa})
    return out


def rider_floor_state_causes(blocks, work, nodes=("new0", "new1", "new2", "new3")):
    """Transitions the nodes logged at a block with NO REAL attempt in it or in its mergeset (a rider-only block): the riders must not step the state machine."""
    log = node_floor_log(work, nodes)
    if not log:
        return None
    by = {b["hash"]: b for b in blocks}
    bad, total = [], 0
    node = max(log, key=lambda n: len(log[n]))
    import re
    pat = re.compile(r"\[palw-floor-state\] daa=(\d+) block=(\S+) ")
    seen = set()
    with open(os.path.join(os.path.expanduser(work), node, "kaspad.out"), errors="replace") as f:
        for ln in f:
            m = pat.search(ln)
            if not m or m.group(2) in seen:
                continue
            seen.add(m.group(2))
            b = by.get(m.group(2))
            total += 1
            if b is None:
                continue
            kinds = [b["kind"]] + [by[x]["kind"] for x in b.get("merged", []) if x in by]
            if "REAL" not in kinds:
                bad.append((int(m.group(1)), m.group(2)[:12]))
    return {"transitions": total, "rider_only": bad}


def gate_riders(objs, claims, blocks, work, tip, windows=None, mature=60):
    """G-R1: riders admitted with a sane price and licensed like ordinary claims, and without stepping the floor state machine. Carried (tag 95 objects / riders in them), queued by the
    producers, refused by the nodes, ADMITTED (companion claims in the state), their price (committed / reserved sompi against the lead's: a rider priced without the work-target fold
    would be saturated), their bind / licence share against the leads' of the same age."""
    if not objs:
        return 3, "INCOMPLETE: no AttemptRidersV1 object on the chain yet (the producers run with --palw-riders; the object needs palw_capacity_multi_claim armed)"
    rc = rider_claims(objs, claims)
    offered = sum(x["offered"] for x in rc)
    admitted = sum(len(x["riders"]) for x in rc)
    logs = rider_log_counts(work)
    bad, notes = [], []
    insane = []
    riders_all, leads_all = [], []
    for x in rc:
        if x["row"] is None:
            continue
        leads_all.append(x["row"])
        lc, lr_ = _int(pick(x["row"], "committedSompi")), _int(pick(x["row"], "reservedSompi"))
        for r in x["riders"]:
            riders_all.append(r)
            rcm, rrs, resc = _int(pick(r, "committedSompi")), _int(pick(r, "reservedSompi")), _int(pick(r, "escrowSompi"))
            if resc >= 2 ** 60 or rcm >= 2 ** 60 or (lc and rcm > 4 * lc) or (lr_ and rrs > 4 * lr_):
                insane.append(str(pick(r, "claimId"))[:12])
    if admitted == 0:
        return 1, f"FAIL: {len(objs)} rider objects carried ({offered} riders offered; producers queued {logs['queued']}, not built {logs['not_built']}, refusal lines {logs['refused_lines']}) but NO rider claim was admitted"
    if insane:
        bad.append(f"{len(insane)} riders priced out of range against their lead (committed / reserved > 4x the lead's, or u64-max-like): {insane[:5]}")
    def share(rows):
        old = [r for r in rows if tip - _int(pick(r, "acceptedDaa")) >= mature]
        if len(old) < 5:
            return None, len(old)
        ok = [r for r in old if str(pick(r, "phase", default="")).lower().startswith(("receipt_licensed", "final", "licensed")) or pick(r, "boundDaa", default=None) is not None]
        lic = [r for r in old if str(pick(r, "phase", default="")).lower() in ("receipt_licensed", "final") or str(pick(r, "phase", default="")).lower().startswith("final")]
        return (len(ok) / len(old), len(lic) / len(old)), len(old)
    rs, rn = share(riders_all)
    ls, ln = share(leads_all)
    txt = (f"{len(objs)} rider objects carried ({offered} riders offered; producers queued {logs['queued']}, not built {logs['not_built']}, refusal lines {logs['refused_lines']}), "
           f"{admitted} rider claims ADMITTED ({100.0 * admitted / max(offered, 1):.0f} % of the offer)")
    if rs is None:
        notes.append(f"only {rn} riders old enough (>= {mature} DAA) to judge bind / licence")
    else:
        txt += f"; riders bound {100 * rs[0]:.0f} % licensed {100 * rs[1]:.0f} % (n={rn})" + (f" against leads bound {100 * ls[0]:.0f} % licensed {100 * ls[1]:.0f} % (n={ln})" if ls else "")
        if ls and rs[1] < 0.9 * ls[1]:
            bad.append(f"riders licensed {100 * rs[1]:.0f} % against the leads' {100 * ls[1]:.0f} %")
    fs = rider_floor_state_causes(blocks, work)
    if fs is None:
        notes.append("no [palw-floor-state] log: rider-only transitions not checked")
    else:
        txt += f"; {fs['transitions']} floor-state transitions, {len(fs['rider_only'])} at a block with no REAL attempt in it or its mergeset"
        if fs["rider_only"]:
            bad.append(f"floor-state transitions with no REAL attempt (riders alone?): {fs['rider_only'][:5]}")
    if windows:
        per = {n: sum(1 for o in objs if lo <= o[0] < hi) for n, (lo, hi) in windows.items()}
        txt += f"; rider objects per rho window {per}"
    if bad:
        return 1, "FAIL: " + "; ".join(bad) + " | " + txt
    if notes:
        return 3, "INCOMPLETE: " + "; ".join(notes) + " | " + txt
    return 0, "PASS: " + txt


def user_metrics(blocks, claims, fence, tip=None):
    """The user's three metrics, past the fence: the REAL attempts' BLUE rate, the REAL share of the selected chain (chain blocks whose kind is REAL or EXEC), and the
    REAL work that reached Final (claims accepted in a REAL block, old enough to have had the time, that are Final)."""
    post = [b for b in blocks if b["daa"] >= fence]
    real = [b for b in post if b["kind"] == "REAL"]
    blue = [b for b in real if b["lane"] in ("BLUE", "EXEC")]
    chain = [b for b in post if b["chain"]]
    chain_real = [b for b in chain if b["kind"] in ("REAL", "EXEC")]
    tip = tip or max((b["daa"] for b in blocks), default=0)
    by_hash = {b["hash"]: b for b in blocks}
    mature = [c for c in claims.values() if str(pick(c, "acceptedBlock", default="")) in by_hash
              and by_hash[str(pick(c, "acceptedBlock"))]["kind"] == "REAL" and int(pick(c, "acceptedDaa", default=0) or 0) <= tip - 200]
    final = [c for c in mature if str(pick(c, "phase", default="")).lower().startswith("final")]
    sompi = lambda cs: sum(int(str(pick(c, "committedSompi", default=0) or 0)) for c in cs)  # noqa: E731
    return {"real_attempts": len(real), "real_blue": len(blue), "real_blue_rate": round(len(blue) / len(real), 3) if real else None,
            "chain_blocks": len(chain), "chain_real": len(chain_real), "real_share_of_chain": round(len(chain_real) / len(chain), 3) if chain else None,
            "real_claims_mature": len(mature), "real_claims_final": len(final), "real_final_rate": round(len(final) / len(mature), 3) if mature else None,
            "real_final_sompi": sompi(final), "real_mature_sompi": sompi(mature)}


def print_metrics(m):
    print("== the user's metrics (past the fence) ==")
    pct = lambda x: "-" if x is None else f"{100.0 * x:.1f} %"  # noqa: E731
    print(f"  REAL attempts BLUE: {m['real_blue']} of {m['real_attempts']} ({pct(m['real_blue_rate'])})")
    print(f"  REAL share of the selected chain: {m['chain_real']} of {m['chain_blocks']} chain blocks ({pct(m['real_share_of_chain'])})")
    print(f"  REAL work reaching Final: {m['real_claims_final']} of {m['real_claims_mature']} claims old enough (>= 200 DAA) ({pct(m['real_final_rate'])}); "
          f"{m['real_final_sompi']} of {m['real_mature_sompi']} sompi committed")


def ignore_report(blocks, fence, st, claims):
    """REPORT-ONLY (never a gate): leg X ran one policy-IGNORING floor producer. How many REAL attempts did it turn RED, against the same number of slots before it,
    and what did the floors it got into the DAG outside Idle earn? st = {start_daa, end_daa}."""
    s0, s1 = int(st["start_daa"]), int(st["end_daa"])
    n = s1 - s0
    during = [b for b in blocks if b["kind"] == "REAL" and s0 <= b["daa"] < s1]
    before = [b for b in blocks if b["kind"] == "REAL" and s0 - n <= b["daa"] < s0]
    red = lambda bs: sum(1 for b in bs if b["lane"] == "RED")  # noqa: E731
    by, _ = floor_states(blocks, fence)
    fl = [b for b in floor_blocks(blocks, fence) if s0 <= b["daa"] < s1]
    outside = [b for b in fl if by.get(b["daa"], "Idle") != "Idle"]
    earned_by = {str(pick(c, "acceptedBlock", default="")) for c in claims.values()}
    earned = [b for b in outside if b["hash"] in earned_by]
    dag = Dag(blocks)
    ac = {"floor": 0, "hb": 0, "real": 0, "exec": 0, "other": 0}
    for b in [b for b in during if b["lane"] == "RED"][:60]:
        for k, v in dag.blue_kinds(b["hash"]).items():
            ac[k] += v
    return [f"policy-ignoring floor producer, DAA {s0}..{s1}: REAL attempts RED {red(during)} of {len(during)} (the {n} slots before: {red(before)} of {len(before)})",
            f"  floors that stood in the DAG in the leg: {len(fl)}, outside Idle {len(outside)}, of which earned a claim {len(earned)} (the fold refuses them: nothing earned expected)",
            f"  blue blocks in the anticones of the RED REAL attempts: floor {ac['floor']}, heartbeat {ac['hb']}, other REAL {ac['real']}, exec {ac['exec']}"]


def fork_checks(a, blocks):
    """RELEASE GATE (fork checks), from dc-run.sh's record fork.json: (a) the old release and the new one refuse each other past the fence — the new node dropped the
    old peer AT the crossing (the re-judgement, no restart needed) and the old node stalled at the fence; (b) a fresh node synced from genesis across the fence to the tip
    with the same sink; (c) the node that was partitioned across the fence and rejoined converged to the same sink. IBD via the pruning proof is not reachable in a drill
    this short (pruning depth is thousands of blocks) and is reported as not run."""
    f = os.path.join(os.path.expanduser(a.evd), "fork.json")
    rec = json.load(open(f)) if os.path.exists(f) else {}
    fa = os.path.join(os.path.expanduser(a.work), "fork-a.json")
    if os.path.exists(fa):
        rec.update(json.load(open(fa)))
    out = []
    port = lambda n: a.json_base + {"new0": 0, "new1": 1, "new2": 2, "new3": 3, "old": 10, "joiner": 13}[n]  # noqa: E731

    def info(n):
        r = call(port(n), "getBlockDagInfo", {})
        return int(pick(r, "virtualDaaScore", default=0) or 0), str(pick(r, "sink", default=""))
    work = os.path.expanduser(a.work)
    # (a)
    line_new = rec.get("mismatch_line")
    try:
        od, _ = info("old")
        nd, _ = info("new3")
    except Exception:  # noqa: BLE001
        od = nd = None
    if not line_new:
        out.append(("FORK-a", 3, "INCOMPLETE: no fork-id refusal logged by the new node at the crossing yet"))
    elif rec.get("old_restarted"):
        out.append(("FORK-a", 1, f"FAIL: the refusal came only after the old node was restarted ({line_new[:100]}): the connection kept across the fence was not re-judged"))
    elif od is not None and nd is not None and nd - od >= 3:
        out.append(("FORK-a", 0, f"PASS: the new node dropped the old peer at the crossing without a restart ({line_new[:110]}); the old node stands at DAA {od}, the chain at {nd}"))
    else:
        out.append(("FORK-a", 3, f"INCOMPLETE: refusal logged but old DAA {od} vs new {nd}"))
    # (b)
    j = rec.get("joiner")
    if not j:
        out.append(("FORK-b", 3, "INCOMPLETE: the fresh node has not run yet"))
    else:
        try:
            jd, js = info("joiner")
            _, ns = info("new3")
            nd2, _ = info("new3")
            ok = jd >= nd2 - 3
            out.append(("FORK-b", 0 if ok else 1, f"{'PASS' if ok else 'FAIL'}: the fresh node (started at DAA {j.get('start_daa')}, fence {a.fence}) synced from genesis to DAA {jd} (chain {nd2}); sink {'equal' if js == ns else 'differs by the moving tip'}"))
        except Exception as e:  # noqa: BLE001
            out.append(("FORK-b", 3, f"INCOMPLETE: the fresh node is not answering ({type(e).__name__}); record {j}"))
    # (c)
    p = rec.get("partition")
    if not p or "rejoin_daa" not in p:
        out.append(("FORK-c", 3, "INCOMPLETE: the partition across the fence has not finished yet"))
    else:
        try:
            d2, s2 = info(p["node"])
            d1, s1 = info("new1")
            crossed = int(p.get("isolated_tip_daa", 0)) >= a.fence
            ok = crossed and abs(d2 - d1) <= 3
            out.append(("FORK-c", 0 if ok else (3 if not crossed else 1),
                        f"{'PASS' if ok else ('INCOMPLETE' if not crossed else 'FAIL')}: {p['node']} was isolated from DAA {p.get('start_daa')} to {p.get('isolated_tip_daa')} (fence {a.fence}), rejoined at {p['rejoin_daa']}; now DAA {d2} vs {d1}"))
        except Exception as e:  # noqa: BLE001
            out.append(("FORK-c", 3, f"INCOMPLETE: {type(e).__name__}"))
    return out


def share_ctx(a):
    """The blocks, the windows past the fence (eligible or not), and the before / after windows — one read of the chain for `share` and `gates`."""
    blocks = load_blocks(a.port)
    tip = max((b["daa"] for b in blocks), default=0)
    hi = min(a.to or tip, tip)
    idle = []
    if getattr(a, "idle_file", None) and os.path.exists(a.idle_file):
        idle = [tuple(map(int, ln.split())) for ln in open(a.idle_file) if ln.strip()]
    ws = windows(blocks, a.fence + a.settle, hi, a.window, a.step, idle)
    producers = [x for x in (a.producers or "").split(",") if x]
    if producers:
        eligible_windows(ws, os.path.expanduser(a.work), producers)
    else:
        for w in ws:
            w["eligible"], w["why"] = True, ""
    for w in ws:        # every DAA of the window carries a block (the clock never stalled)
        have = {b["daa"] for b in blocks if w["lo"] <= b["daa"] < w["hi"]}
        w["daa_missing"] = [d for d in range(w["lo"], w["hi"]) if d not in have]
    return {"blocks": blocks, "hi": hi, "ws": ws, "elig": [w for w in ws if w["eligible"]], "inel": [w for w in ws if not w["eligible"]],
            "before": windows(blocks, 20, a.fence, a.window, a.window, idle), "after": windows(blocks, a.fence, hi, a.window, a.window, idle), "fence": a.fence}


def print_share(ctx, table=True):
    """The window table, the RED/BLUE summary and the composition; returns nothing (the verdicts are `goal_verdict` and the gates)."""
    if table:
        print(HEAD)
        for w in ctx["ws"]:
            print(line(w))
    for name, grp in (("before the fence", ctx["before"]), ("after the fence", ctx["after"])):
        red, real, t = sum_anticone(grp)
        print(f"RED REAL attempts {name}: {red} of {real} REAL attempts; blue blocks in their anticones (up to 40 attempts per window): floor {t['floor']}, "
              f"heartbeat {t['hb']}, other REAL {t['real']}, exec {t['exec']}")
    elig = ctx["elig"]
    comp = {k: sum(w[k] for w in elig) for k in ("n", "REAL", "EXEC", "FALLBACK", "LEGACY_FLOOR", "LEGACY_HEARTBEAT", "other")}
    if comp["n"]:
        pc = lambda x: f"{100.0 * x / comp['n']:.1f} %"  # noqa: E731
        print(f"composition of the {len(elig)} eligible windows ({comp['n']} non-RED blocks): REAL {pc(comp['REAL'])}, EXEC {pc(comp['EXEC'])}, heartbeat {pc(comp['LEGACY_HEARTBEAT'])}, "
              f"floor {pc(comp['FALLBACK'] + comp['LEGACY_FLOOR'])}, other {pc(comp['other'])} — reported as measured: with two REAL producers the heartbeat share is a supply fact, not tuned")
    if ctx["inel"]:
        print(f"NOT ELIGIBLE ({len(ctx['inel'])} windows, not counted: a producer was down or held): "
              + ", ".join(f"{w['lo']}-{w['hi']} [{w['why']}]" for w in ctx["inel"][:12]))


def goal_verdict(ctx):
    """GOAL METRIC (supply-bound, reported, never blocks shipping): REAL + EXEC >= 90 % and heartbeats <= 10 % of the consensus blocks, per eligible window with real load."""
    elig = ctx["elig"]
    if not ctx["ws"]:
        return 3, "INCOMPLETE: no complete window past the fence yet"
    live = [w for w in elig if w["n"] and not w["idle"]]
    if not live:
        return 3, "INCOMPLETE: no eligible window with REAL load yet — a share over no real load proves nothing"
    bad = [w for w in live if w["share"] < 0.9 or w["hb"] > 0.1]
    if bad:
        return 1, (f"FAIL: {len(bad)} of {len(live)} eligible windows with REAL load are below REAL+EXEC >= 90 % / above heartbeats <= 10 %: "
                   + ", ".join(f"{w['lo']}-{w['hi']} (REAL+EXEC {w['share']}, heartbeat {w['hb']})" for w in bad[:8]))
    return 0, (f"PASS: {len(live)} eligible windows with REAL load, min REAL+EXEC {min(w['share'] for w in live)}, max heartbeat {max(w['hb'] for w in live)} "
               f"({len(ctx['inel'])} not eligible, {len(elig) - len(live)} idle)")


def gate_blue(ctx, min_real=3):
    """RELEASE GATE: the REAL attempts are BLUE >= 90 % in every eligible window past the fence that holds at least `min_real` of them (floors suppressed: see gate_floor)."""
    rows = [w for w in ctx["elig"] if w["real_total"] >= min_real]
    if not rows:
        return 3, "INCOMPLETE: no eligible window past the fence with >= %d REAL attempts yet" % min_real
    rate = lambda w: (w["real_total"] - w["real_red"]) / w["real_total"]  # noqa: E731
    bad = [w for w in rows if rate(w) < 0.9]
    tot, red = sum(w["real_total"] for w in rows), sum(w["real_red"] for w in rows)
    txt = f"{len(rows)} windows, {tot} REAL attempts, {red} RED ({100.0 * (tot - red) / tot:.1f} % BLUE overall, worst window {min(rate(w) for w in rows):.2f})"
    if bad:
        return 1, "FAIL: " + txt + "; below 90 %: " + ", ".join(f"{w['lo']}-{w['hi']} ({rate(w):.2f})" for w in bad[:8])
    return 0, "PASS: " + txt


def gate_floor(ctx):
    """RELEASE GATE: floor attempts only in idle windows — an eligible window that holds REAL load holds no floor attempt."""
    live = [w for w in ctx["elig"] if w["n"] and not w["idle"]]
    if not live:
        return 3, "INCOMPLETE: no eligible window with REAL load yet"
    bad = [w for w in live if w["floor"] > 0]
    if bad:
        return 1, "FAIL: floor attempts in busy windows: " + ", ".join(f"{w['lo']}-{w['hi']} ({w['floor']})" for w in bad[:8])
    return 0, f"PASS: no floor attempt in any of the {len(live)} busy windows"


def gate_daa(ctx, max_gap_s=4 * SLOT_S):
    """RELEASE GATE: the DAA advances in every window past the fence — a block at every DAA and no stamp gap over four slots."""
    if not ctx["ws"]:
        return 3, "INCOMPLETE: no complete window past the fence yet"
    bad = [w for w in ctx["ws"] if w["daa_missing"] or w["daa_per_h"] is None or (w["max_gap_s"] or 0) > max_gap_s]
    if bad:
        return 1, "FAIL: " + ", ".join(f"{w['lo']}-{w['hi']} (missing {w['daa_missing'][:3]}, gap {w['max_gap_s']} s)" for w in bad[:8])
    rates = [w["daa_per_h"] for w in ctx["ws"] if w["daa_per_h"] is not None]
    return 0, f"PASS: {len(ctx['ws'])} windows, DAA/h {min(rates)}..{max(rates)}, longest slot gap {max((w['max_gap_s'] or 0) for w in ctx['ws']):.0f} s"


def share(a):
    ctx = share_ctx(a)
    print_share(ctx)
    code, text = goal_verdict(ctx)
    print("goal (supply-bound; reported, never blocks shipping) — SHARE, REAL + EXEC >= 90 % / heartbeat <= 10 % of the consensus blocks:")
    print("  " + text)
    gb, tb = gate_blue(ctx)
    gf, tf = gate_floor(ctx)
    print("release gates read from the same windows: BLUE rate " + tb + " | floors " + tf)
    return code


def gates(a):
    """The combined drill's classified outputs: RELEASE GATES (must PASS to ship) and the GOAL METRIC (reported, never blocks)."""
    import contextlib, glob, io, re
    ctx = share_ctx(a)
    rows = []
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        pc = panel(argparse.Namespace(work=a.work, p50_max=a.p50_max, p95_max=a.p95_max, wait_max=a.wait_max))
    plines = [x for x in buf.getvalue().splitlines() if x.strip()]
    rows.append(("PANEL", pc, plines[-1] if plines else ""))
    blocks, fence = ctx["blocks"], a.fence
    kw = {"idle": a.idle_slots, "probe": a.probe_slots, "cool": a.cooldown}
    claims = gather_claims(a.port, a.work)
    holds = hold_lines(a.work, ("new0",))
    by, probes, src, note = floor_states_best(blocks, fence, a.work, **kw)
    states = (by, probes)
    rows.append(("BLUE", *gate_blue(ctx)))
    rows.append(("DELAY", *gate_delay_active(a.work)))
    rows.append(("FLOOR", *gate_floor_policy(blocks, fence, claims, holds, states=states, **kw)))
    r1 = os.path.expanduser(a.restart1)
    rows.append(("RED>BLUE1", *(gate_red2blue(blocks, json.load(open(r1)), a.probe_slots) if os.path.exists(r1)
                                else (3, "INCOMPLETE: leg B1 (both REAL producers back after the stale leg) has not run yet"))))
    state = os.path.expanduser(a.state)
    if os.path.exists(state):
        st = json.load(open(state))
        rc, rl = recovery_verdict(blocks, st, a.k, a.margin)
        rows.append(("RECOVERY", rc, rl[-1]))
        rows.append(("RED>BLUE2", *gate_red2blue(blocks, st, a.probe_slots)))
    else:
        rows.append(("RECOVERY", 3, "INCOMPLETE: the recovery leg (A) has not run yet"))
        rows.append(("RED>BLUE2", 3, "INCOMPLETE: needs the recovery leg"))
    stale = os.path.expanduser(a.stale)
    if os.path.exists(stale):
        sst = json.load(open(stale))
        rows.append(("STALE", *gate_stale(blocks, fence, sst, states=states, **kw)))
        rows.append(("COOLDOWN", *gate_cool(blocks, fence, sst, states=states, **kw)))
    else:
        rows.append(("STALE", 3, "INCOMPLETE: the stale-injection leg has not run yet"))
        rows.append(("COOLDOWN", 3, "INCOMPLETE: the stale-injection leg has not run yet"))
    rows.append(("DAA", *gate_daa(ctx)))
    rows.append(("STATES", *gate_states_daa(blocks, fence, states=states, **kw)))
    tipd = max((b["daa"] for b in blocks), default=0)
    r1d = json.load(open(r1))["restart_daa"] if os.path.exists(r1) else 10 ** 9
    rows.append(("G-A1", *gate_admission(a.work, states, r1d, tipd)))
    rows.append(("G-A2", *gate_exec_seed(read_exec_samples(os.path.join(os.path.expanduser(a.evd), "roundlane.tsv")), fence)))
    rows.append(("G-A3", *gate_bind_wait(claims, blocks, fence, a.work, tipd)))
    capw = {}
    try:
        capw = {k: (v["lo"], v["hi"]) for k, v in (json.load(open(os.path.join(os.path.expanduser(a.work), "drive-state.json")))["data"].get("cap-summary") or {}).items()}
    except (OSError, ValueError, KeyError):
        pass
    rows.append(("G-R1", *gate_riders(rider_objects(a.port, fence), claims, blocks, a.work, tipd, capw)))
    a.json_base = int(os.environ.get("JSON_BASE", "53200"))
    for nm, c, t in fork_checks(a, blocks):
        rows.append((nm, c, t))
    lane = []
    work = os.path.expanduser(a.work)
    for f in sorted(glob.glob(os.path.join(work, "verdict", "*.verdict"))):
        name = os.path.basename(f)[:-8]
        if name in ("recovery",):
            continue
        txt = open(f).read().strip().split("\t")
        lane.append((name, {"PASS": 0, "FAIL": 1}.get(txt[0], 3), (txt[0] + " " + (txt[1] if len(txt) > 1 else ""))[:140]))
    evd = os.path.expanduser(a.evd)
    for f in sorted(glob.glob(os.path.join(evd, "dg*.log"))):
        m = re.findall(r"(DG-\w+) (PASS|FAIL)", open(f, errors="replace").read())
        if m:
            lane.append((m[-1][0], 0 if m[-1][1] == "PASS" else 1, m[-1][1]))
    names = [n for n, _, _ in lane]
    for need in ("dm5",):
        if need not in names:
            lane.append((need, 3, "INCOMPLETE: no verdict file yet"))
    info = [(n, c, t) for n, c, t in lane if n in ("dm1", "dm2", "dm3", "dm4", "dm6", "cap")]      # not gates here: their epochs need ~1,000 DAA
    lane = [(n, c, t) for n, c, t in lane if (n, c, t) not in info]
    for lf in sorted(glob.glob(os.path.join(os.path.expanduser(a.lanes), "*", "verdict.txt"))):    # the lane pre-checks' own verdict files
        txt = open(lf).read().strip().splitlines()
        if txt:
            lane.append(("pre:" + os.path.basename(os.path.dirname(lf)), {"PASS": 0, "FAIL": 1}.get(txt[0].split()[0], 3), txt[0][:140]))
    label = {0: "PASS", 1: "FAIL", 3: "INCOMPLETE"}
    print("== RELEASE GATES (all must PASS to ship) ==")
    for n, c, t in rows:
        print(f"  [{n:<8}] {label[c]:<10} {t}")
    print("  per-lane verdicts (crossings in this chain; the lane pre-checks' verdict.txt):")
    for n, c, t in lane:
        print(f"  [{n:<8}] {label[c]:<10} {t}")
    print("  covered at test level, not by this drill: FORK-d IBD via the pruning proof across the fence (RS's T49-style carriage test: capture in Probe/Normal -> import -> identical decisions;")
    print("  and the combined-fence test with the floor rule live) — pruning depth is thousands of blocks, this drill makes ~350")
    print(f"  floor states: {note}")
    print("  informational, not gates (D-M epochs need ~1,000 DAA, longer than this drill):")
    for n, c, t in info:
        print(f"  [{n:<8}] {label[c]:<10} {t}")
    allc = [c for _, c, _ in rows] + [c for _, c, _ in lane]
    overall = 1 if 1 in allc else (3 if 3 in allc else 0)
    print(f"RELEASE GATES: {label[overall]}")
    print_share(ctx, table=False)
    print_metrics(user_metrics(blocks, claims, fence))
    xs = os.path.join(os.path.expanduser(a.evd), "ignore.json")
    print("== report-only (never a gate) ==")
    if os.path.exists(xs):
        for ln in ignore_report(blocks, fence, json.load(open(xs)), claims):
            print("  " + ln)
    else:
        print("  policy-ignoring floor producer: the leg has not run (or the binary has no flag to ignore the policy)")
    lo_ = os.path.join(os.path.expanduser(a.evd), "leg-o.json")
    if os.path.exists(lo_):
        for ln in bind_wait_report(claims, blocks, fence, a.work, max((b["daa"] for b in blocks), default=0), json.load(open(lo_))):
            print("  " + ln)
    else:
        print("  only non-operator REAL (leg O, operator producers stopped): the leg has not run (O_DAA = 0 by default: it needs ~80 DAA more)")
    gc, gt = goal_verdict(ctx)
    print("== goal (supply-bound; reported, never blocks shipping) ==")
    print(f"  SHARE REAL + EXEC >= 90 % / heartbeat <= 10 % of the consensus blocks: {gt}")
    return overall


def redblue(a):
    blocks = load_blocks(a.port)
    tip = max((b["daa"] for b in blocks), default=0)
    lo, hi = a.frm or 20, min(a.to or tip, tip)
    dag = Dag(blocks)
    print(f"fence H' = {a.fence}; windows of {a.window} DAA")
    print(HEAD)
    a0 = lo
    while a0 + a.window <= hi:
        w = window_row(blocks, dag, a0, a.window, [])
        print(line(w) + ("   <- after the fence" if a0 >= a.fence else ""))
        for b in [b for b in blocks if a0 <= b["daa"] < a0 + a.window and b["kind"] == "REAL" and b["lane"] == "RED"][:40]:
            k = dag.blue_kinds(b["hash"])
            print(f"      RED REAL {b['hash'][:16]} DAA {b['daa']}: blue anticone floor {k['floor']} heartbeat {k['hb']} real {k['real']} exec {k['exec']} other {k['other']}")
        a0 += a.window
    return 0


# --------------------------------------------------------------------------------------------------------------------
# recovery
# --------------------------------------------------------------------------------------------------------------------
def recovery_verdict(blocks, st, k=20, margin=6):
    """(code, lines) from the blocks and the leg's record {stop_daa, restart_daa, end_daa}."""
    stop, restart, end = int(st["stop_daa"]), int(st["restart_daa"]), int(st["end_daa"])
    out, bad, wait = [], [], []
    blue = lambda b: b["lane"] in ("BLUE", "EXEC")  # noqa: E731
    real = sorted(b["daa"] for b in blocks if b["kind"] == "REAL" and blue(b))
    floors = sorted(b["daa"] for b in blocks if b["kind"] in FLOOR_KINDS and blue(b))
    last_real = max((d for d in real if d <= restart), default=None)
    if last_real is None:
        return 3, ["INCOMPLETE: no REAL attempt before the restart (the producers never ran)"]
    out.append(f"stop at DAA {stop}, restart at {restart}, observed to {end}; the last REAL attempt before the restart: DAA {last_real} (K = {k})")
    # (a) the clock
    first_ts = {}
    for b in blocks:
        if b["ts"] and (b["daa"] not in first_ts or b["ts"] < first_ts[b["daa"]]):
            first_ts[b["daa"]] = b["ts"]
    missing = [d for d in range(stop, min(restart, end) + 1) if d not in first_ts]
    gaps = [((first_ts[d + 1] - first_ts[d]) / 1000.0, d) for d in range(stop, min(restart, end)) if d in first_ts and d + 1 in first_ts]
    worst = max(gaps, default=(0, None))
    after_blue = sorted(b["daa"] for b in blocks if b["kind"] == "REAL" and blue(b) and b["daa"] > restart)
    after_any = sorted(b["daa"] for b in blocks if b["kind"] == "REAL" and b["daa"] > restart)
    last_floor_after = max((d for d in floors if d > restart), default=None)
    out.append("recovery time (slots after the restart): first REAL attempt " + (str(after_any[0] - restart) if after_any else "-") + ", first BLUE REAL attempt "
               + (str(after_blue[0] - restart) if after_blue else "-") + ", last floor standing " + (str(last_floor_after - restart) if last_floor_after else "none") + "; floors resumed "
               + (str(min((d for d in floors if d > last_real), default=0) - stop) if any(d > last_real for d in floors) else "-") + " slots after the stop")
    out.append(f"(a) DAA over the stop: {len(missing)} DAA without a block; longest slot gap {worst[0]:.0f} s at DAA {worst[1]}")
    if missing:
        bad.append(f"(a) DAA {missing[:6]} had no block while the producers were down")
    if worst[0] > 4 * SLOT_S:
        bad.append(f"(a) a slot took {worst[0]:.0f} s (> {4 * SLOT_S:.0f}: the drill's own slots reach ~270 s)")
    # (b) floors resume after K idle slots
    early = [d for d in floors if last_real < d <= last_real + k]       # Normal lasts up to last_blue + K; floors are accepted again from last_blue + K + 1
    first_floor = next((d for d in floors if d > last_real), None)
    out.append(f"(b) floors: first BLUE FALLBACK after the last REAL attempt at DAA {first_floor} (expected {last_real + k + 1}..{last_real + k + 1 + margin}); earlier than K: {early}")
    if early:
        bad.append(f"(b) floor attempts at DAA {early} came before K = {k} idle slots")
    if first_floor is None:
        (wait if restart - last_real <= k + margin else bad).append("(b) no floor attempt resumed during the stop")
    elif first_floor > last_real + k + 1 + margin:
        bad.append(f"(b) floors resumed only at DAA {first_floor}, {first_floor - last_real} slots after the last REAL attempt")
    # (c) after the restart
    after_real = [b for b in blocks if b["kind"] == "REAL" and b["daa"] > restart]
    if not after_real:
        wait.append("(c) no REAL attempt after the restart yet")
    else:
        d1 = min(b["daa"] for b in after_real)
        late_floor = [d for d in floors if d1 + k < d <= end]
        nblue = sum(1 for b in after_real if blue(b))
        frac = nblue / len(after_real)
        out.append(f"(c) first REAL attempt after the restart: DAA {d1}; BLUE FALLBACK later than {d1 + k}: {late_floor}; REAL attempts after the restart {len(after_real)}, BLUE {nblue} ({frac:.2f})")
        if late_floor:
            bad.append(f"(c) floor attempts at DAA {late_floor[:6]} kept coming after the REAL attempts resumed")
        if frac < 0.9:
            bad.append(f"(c) only {frac:.2f} of the REAL attempts after the restart are BLUE")
        if end < d1 + k + 4:
            wait.append(f"(c) observed only to DAA {end}")
    if bad:
        return 1, out + ["FAIL: " + " | ".join(bad)]
    if wait:
        return 3, out + ["INCOMPLETE: " + " | ".join(wait)]
    return 0, out + ["PASS: the DAA kept advancing, floors resumed after K slots and stopped once REAL attempts were back, REAL turned BLUE again"]


def recovery(a):
    st = json.load(open(os.path.expanduser(a.state)))
    code, lines = recovery_verdict(load_blocks(a.port), st, a.k, a.margin)
    print("\n".join(lines))
    return code


# --------------------------------------------------------------------------------------------------------------------
# panel
# --------------------------------------------------------------------------------------------------------------------
def panel(a):
    work = os.path.expanduser(a.work)
    d = json.load(open(os.path.join(work, "drive-state.json")))["data"]
    cap, claims = d.get("cap-summary") or {}, d.get("cap-claims") or {}
    if not cap:
        print("INCOMPLETE: no capacity window measured yet")
        return 3
    bad, rows = [], []
    for name, sm in cap.items():
        why = []
        if sm.get("bind_latency_p50") is None:
            why.append("no bind->licence sample")
        else:
            if sm["bind_latency_p50"] > a.p50_max:
                why.append(f"p50 {sm['bind_latency_p50']} > {a.p50_max}")
            if sm["bind_latency_p95"] > a.p95_max:
                why.append(f"p95 {sm['bind_latency_p95']} > {a.p95_max}")
        if sm.get("diverges"):
            why.append("backlog diverges")
        if (sm.get("oldest_wait_max") or 0) > a.wait_max:
            why.append(f"oldest wait {sm['oldest_wait_max']} > {a.wait_max}")
        rows.append((name, sm, why))
        if why:
            bad.append(f"{name}: " + "; ".join(why))
    unavailable = [c for c, r in claims.items() if "unavail" in str(r.get("void", "")).lower()]
    f3, rec = {}, {}
    import glob, re, time
    for log in sorted(glob.glob(os.path.join(work, "new*", "kaspad.out"))):
        node = os.path.basename(os.path.dirname(log))
        n_hold = n_rec = 0
        first = last = None
        with open(log, errors="replace") as f:
            for ln in f:
                if "NOT PRODUCING" in ln and "holding" in ln:
                    n_hold += 1
                if "[palw-panel] filed a" in ln and "receipt" in ln:
                    n_rec += 1
                    m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)", ln)
                    if m:
                        t = time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S"))
                        first = first or t
                        last = t
        f3[node] = n_hold
        rec[node] = round(n_rec / max((last - first) / 3600, 1 / 60), 1) if first and last and last > first else None
    print("note: accepted->licence includes the ~20-DAA anchor delay (a claim is bound to a panel only after its anchor matures); bind->licence is the PANEL metric.")
    print(f"{'window':>10} {'accP50':>6} {'accP95':>6} {'bindP50':>7} {'bindP95':>7} {'n':>4} {'backlog':>10} {'slope':>7} {'oldest':>6} {'diverges':>8}")
    for name, sm, why in rows:
        print(f"{name:>10} {str(sm.get('latency_p50')):>6} {str(sm.get('latency_p95')):>6} {str(sm.get('bind_latency_p50')):>7} {str(sm.get('bind_latency_p95')):>7} "
              f"{str(sm.get('bind_latency_n')):>4} {str(sm['backlog_first']) + '->' + str(sm['backlog_last']):>10} {str(sm['backlog_slope_per_daa_second_half']):>7} "
              f"{str(sm.get('oldest_wait_max')):>6} {str(sm['diverges']):>8}  {'; '.join(why)}")
    print(f"F3 / producer-hold lines per node: {f3}")
    print(f"PanelUnavailable expiries (voided claims): {len(unavailable)}")
    print(f"receipts filed per hour per seat node (log span): {rec}")
    if unavailable:
        bad.append(f"{len(unavailable)} claims expired as panel-unavailable")
    if bad:
        print("FAIL: " + " | ".join(bad))
        return 1
    print(f"PASS: {len(rows)} windows: nothing diverges, bind->licence within p50 {a.p50_max} / p95 {a.p95_max}")
    return 0


# --------------------------------------------------------------------------------------------------------------------
# selftest
# --------------------------------------------------------------------------------------------------------------------
def _blk(h, parents, daa, kind, lane, ts=None):
    return {"hash": h, "parents": parents, "daa": daa, "ts": (1_000_000 + daa * 120_000) if ts is None else ts, "kind": kind, "lane": lane}


def selftest():
    # a chain g <- a <- b <- c; r is a REAL attempt on g that arrived late (RED); a, b, c are floors/heartbeat in r's anticone
    bl = [_blk("g", [], 10, "REAL", "BLUE"), _blk("a", ["g"], 11, "FALLBACK", "BLUE"), _blk("b", ["a"], 12, "LEGACY_HEARTBEAT", "BLUE"),
          _blk("c", ["b"], 13, "REAL", "BLUE"), _blk("r", ["g"], 12, "REAL", "RED"), _blk("d", ["c", "r"], 14, "REAL", "BLUE")]
    dag = Dag(bl)
    ac = dag.anticone("r")
    assert ac == {"a", "b", "c"}, ac
    k = dag.blue_kinds("r")
    assert k == {"floor": 1, "hb": 1, "real": 1, "exec": 0, "other": 0}, k
    row = window_row(bl, dag, 10, 5, [])
    assert row["real_total"] == 4 and row["real_red"] == 1 and row["anticone_of_red_real"]["floor"] == 1, row
    assert row["n"] == 5 and row["share"] == round(3 / 5, 3), row
    # recovery: REAL until DAA 20, floors from 23 (K = 3) to 35, REAL again from 36, floors none after 39, clock every slot
    chain = []
    for d in range(10, 60):
        kind = "REAL" if (d <= 20 or d >= 36) else ("LEGACY_HEARTBEAT" if d < 24 else "FALLBACK")
        chain.append(_blk(f"x{d}", [f"x{d - 1}"] if d > 10 else [], d, kind, "BLUE"))
    st = {"stop_daa": 21, "restart_daa": 35, "end_daa": 55}
    code, lines = recovery_verdict(chain, st, k=3, margin=4)
    assert code == 0, lines
    # a floor too early, a stalled slot, floors that never stop: each is a FAIL
    early = [dict(b) for b in chain]
    early[11]["kind"] = "FALLBACK"          # DAA 21: one slot after the last REAL attempt
    assert recovery_verdict(early, st, k=3, margin=4)[0] == 1
    stalled = [b for b in chain if b["daa"] != 28]
    assert recovery_verdict(stalled, st, k=3, margin=4)[0] == 1
    never = [dict(b) for b in chain]
    never[-3]["kind"] = "FALLBACK"          # DAA 57: a floor long after the REAL attempts resumed
    assert recovery_verdict(never, {"stop_daa": 21, "restart_daa": 35, "end_daa": 58}, k=3, margin=4)[0] == 1
    # the release's K = 20: REAL until DAA 20, heartbeats while idle, floors from DAA 41 (last BLUE + K + 1), a 44-slot stop (21..65), REAL again from 66, no floor after 86
    chain20 = []
    for d in range(10, 130):
        kind = "REAL" if (d <= 20 or d >= 66) else ("LEGACY_HEARTBEAT" if d < 41 else "FALLBACK")
        chain20.append(_blk(f"y{d}", [f"y{d - 1}"] if d > 10 else [], d, kind, "BLUE"))
    st20 = {"stop_daa": 21, "restart_daa": 65, "end_daa": 110}
    code, lines = recovery_verdict(chain20, st20)
    assert code == 0, lines
    early20 = [dict(b) for b in chain20]
    early20[21]["kind"] = "FALLBACK"             # DAA 31: eleven idle slots after the last REAL attempt (DAA 20), well before K = 20
    assert recovery_verdict(early20, st20)[0] == 1
    # the floor states (ADR-0165: idle 20, probe 8, cooldown 20 from the END of an unanswered probe): Normal up to last_blue + 20, Idle from last_blue + 21, a RED attempt in Idle opens a
    # probe (floors refused for 8 slots), a RED attempt inside the cooldown opens none, a BLUE one inside a probe makes it Normal
    def build(extra_floor=(), no_floor=()):
        out = []
        reals = {d: ("REAL", "BLUE") for d in list(range(11, 21)) + list(range(114, 151))}
        reals.update({50: ("REAL", "RED"), 60: ("REAL", "RED"), 79: ("REAL", "RED"), 100: ("REAL", "RED"), 110: ("REAL", "RED"), 113: ("REAL", "BLUE")})
        for d in range(1, 151):
            out.append(_blk(f"t{d}", [f"t{d - 1}"] if d > 1 else [], d, "LEGACY_HEARTBEAT", "BLUE"))
            if d in reals:
                out.append(_blk(f"r{d}", [f"t{d - 1}"] if d > 1 else [], d, reals[d][0], reals[d][1]))
        for d in list(range(1, 11)) + list(range(41, 51)) + list(range(58, 80)) + list(range(87, 111)) + list(extra_floor):
            if d not in no_floor:
                out.append(_blk(f"f{d}", [f"t{d - 1}"], d, "FALLBACK", "BLUE"))
        return out
    chainf = build()
    by, probes = floor_states(chainf, 0)
    assert probes == [50, 79, 110], probes
    assert [by[d] for d in (40, 41, 45, 52, 57, 58, 65, 81, 87, 90, 100, 112, 114, 115, 134)] == \
        ["Normal", "Idle", "Idle", "Probe", "Probe", "Idle", "Idle", "Probe", "Idle", "Idle", "Idle", "Probe", "Normal", "Normal", "Normal"], by
    holds_ok, holds_none = {"new0": 7}, {"new0": 0}
    assert gate_floor_policy(chainf, 0, {}, holds_ok)[0] == 0, gate_floor_policy(chainf, 0, {}, holds_ok)
    assert gate_floor_policy(chainf, 0, {}, holds_none)[0] == 3                  # no hold line: the producers' policy is not shown
    stand = build(extra_floor=[53, 125])                                          # floors that stand in a probe and in Normal (a policy-ignoring producer)
    assert gate_floor_policy(stand, 0, {}, holds_ok)[0] == 0                      # they stand but earned nothing: the fold refused them
    assert gate_floor_policy(stand, 0, {"c1": {"acceptedBlock": "f125"}}, holds_ok)[0] == 1     # one earned a claim: FAIL
    stl = {"start_daa": 45, "end_daa": 100}
    assert gate_stale(chainf, 0, stl)[0] == 0, gate_stale(chainf, 0, stl)
    assert gate_stale(build(no_floor=range(58, 80)), 0, stl)[0] == 1              # floors never came back after the first probe
    assert gate_cool(chainf, 0, stl)[0] == 0, gate_cool(chainf, 0, stl)
    assert gate_cool(chainf, 0, stl, states=({}, [50, 70]))[0] == 1               # two probes twenty slots apart: closer than probe + cooldown = 28
    assert gate_cool(chainf, 0, {"start_daa": 45, "end_daa": 60})[0] == 3         # one probe so far
    import tempfile
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, "new0"))
    open(os.path.join(d, "new0", "kaspad.out"), "w").write("x\n[palw-floor-state] daa=50 block=ab12 idle->probe last_blue=- until=58 last_probe_end=-\n"
                                                           "[palw-floor-state] daa=58 block=cd34 probe->idle last_blue=- until=- last_probe_end=58\n"
                                                           "[palw-floor-state] daa=95 block=ef56 idle->normal last_blue=95 until=- last_probe_end=58\n")
    tr = node_floor_transitions(d)
    assert tr["new0"] == [(50, "Idle", "Probe"), (58, "Probe", "Idle"), (95, "Idle", "Normal")], tr
    jl = judged_states_from_log(node_floor_log(d)["new0"], 40, 130)
    # DAA 50: the block that opens the probe judges the floors it merges ahead of its RED event as Idle; 51..57 Probe; 58 (until reached) Idle; 95 Idle (ahead of the BLUE event); 96..115 Normal;
    # 116 (> last_blue + 20) Idle with no line at all: time alone ended it
    assert [jl[t] for t in (49, 50, 51, 57, 58, 59, 94, 95, 96, 115, 116, 125)] == ["Idle", "Idle", "Probe", "Probe", "Idle", "Idle", "Idle", "Idle", "Normal", "Normal", "Idle", "Idle"], jl
    assert time_advance("Normal", 95, None, 115) == "Normal" and time_advance("Normal", 95, None, 116) == "Idle" and time_advance("Probe", None, 58, 58) == "Idle"
    best = floor_states_best(chainf, 0, d)                                         # a node log in force: the source says so
    assert best[2].startswith("node:"), best[2]
    assert gate_red2blue(chainf, {"restart_daa": 108})[0] == 0, gate_red2blue(chainf, {"restart_daa": 108})
    slow = [dict(b) for b in chainf if b["hash"] not in {f"r{d}" for d in range(113, 121)}]   # the BLUE successor comes only at DAA 121: 11 slots after the first REAL attempt
    assert gate_red2blue(slow, {"restart_daa": 108})[0] == 1
    assert gate_states_daa(chainf, 0)[0] == 0, gate_states_daa(chainf, 0)
    # G-A1: a class registered in Normal reaches Probation inside two periods (PASS), after three (FAIL), a registration in Idle is not the condition
    wd = tempfile.mkdtemp()
    os.makedirs(os.path.join(wd, "keyring"))
    open(os.path.join(wd, "milestones.tsv"), "w").write("class:a:Candidate\t100\tx\nclass:a:Probation\t180\tx\nclass:b:Candidate\t110\tx\nclass:c:Candidate\t10\tx\n")
    normal = ({d: "Normal" for d in range(0, 400)}, [])
    assert gate_admission(wd, normal, 90, 250)[0] == 3                       # b: Candidate 110, Probation not yet, 140 DAA in: waiting
    assert gate_admission(wd, normal, 90, 330)[0] == 1                       # b: 220 DAA, no Probation: beyond two periods
    open(os.path.join(wd, "milestones.tsv"), "a").write("class:b:Probation\t290\tx\n")
    assert gate_admission(wd, normal, 90, 330)[0] == 0, gate_admission(wd, normal, 90, 330)
    assert gate_admission(wd, ({d: "Idle" for d in range(0, 400)}, []), 90, 330)[0] == 3
    # G-A2: snapshots created at spans 100..110 (finals > 0), schedules seeded in their windows
    smp = [{"daa": d, "span": d, "armed": True, "open": True, "permits": 1 if 100 + 120 <= d <= 100 + 120 + 20 or 300 <= d <= 330 else 0, "next": 0, "finals_span": d, "finals": 2 if 100 <= d < 108 else 0,
            "accepted_in_span": 0, "acc_total": 0, "ref_total": 0} for d in range(60, 520)]
    assert gate_exec_seed(smp, 50)[0] == 0 or gate_exec_seed(smp, 50)[0] == 1
    seeded_all = [dict(x, permits=1) for x in smp]
    assert gate_exec_seed(seeded_all, 50)[0] == 0, gate_exec_seed(seeded_all, 50)
    none_seeded = [dict(x, permits=0) for x in smp]
    assert gate_exec_seed(none_seeded, 50)[0] == 1
    assert gate_exec_seed(smp[:100], 50)[0] == 3                              # too few matured snapshots
    # G-A3: operator and non-operator REAL claims, accepted -> bound
    open(os.path.join(wd, "keyring", "manifest.json"), "w").write(json.dumps({"seats": [{"bond_outpoint": "op:0"}]}))
    cb = [_blk(f"c{i}", [], 100 + i, "REAL", "BLUE") for i in range(40)]
    cl = {}
    for i in range(40):
        cl[f"x{i}"] = {"acceptedBlock": f"c{i}", "acceptedDaa": 100 + i, "boundDaa": 100 + i + (5 if i % 2 == 0 else 12), "executorBond": "op:0" if i % 2 == 0 else "ext:1"}
    assert gate_bind_wait(cl, cb, 26, wd, 200)[0] == 0, gate_bind_wait(cl, cb, 26, wd, 200)
    for k in cl:
        if cl[k]["executorBond"] == "ext:1":
            cl[k]["boundDaa"] = cl[k]["acceptedDaa"] + 55                         # the non-operator wait is long
    assert gate_bind_wait(cl, cb, 26, wd, 200)[0] == 1
    assert gate_bind_wait({k: v for k, v in cl.items() if v["executorBond"] == "op:0"}, cb, 26, wd, 200)[0] == 3     # no non-operator REAL: incomplete
    # G-R1: riders admitted with a sane price, licensed, and no rider-only floor-state transition
    wr = tempfile.mkdtemp()
    for n in ("new4", "new0"):
        os.makedirs(os.path.join(wr, n))
    L = "aa" * 64
    open(os.path.join(wr, "new4", "kaspad.out"), "w").write(f"[palw-producer] queues 4 rider(s) of lead {L} (ADR-0164 F-M1, tag 95)\n")
    def claim(cid, acc, esc, phase="final", committed="9000", reserved="500", bond="op:0", cls="C"):
        return {"claimId": cid, "acceptedDaa": acc, "escrowSompi": esc, "phase": phase, "committedSompi": committed, "reservedSompi": reserved, "executorBond": bond, "classId": cls, "boundDaa": acc + 5}
    cr = {L: claim(L, 100, 2004)}
    for i in range(4):
        cr[f"r{i}"] = claim(f"r{i}", 101, 2000, phase="receipt_licensed")
    for i in range(12):       # ordinary leads of other DAAs: the licensing baseline
        cr[f"l{i}"] = claim(f"l{i}", 100 + i, 10000)
    objs = [(101, "bh", L, 4)]
    bl = [dict(_blk("bh", [], 101, "LEGACY_HEARTBEAT", "BLUE"), merged=[]), dict(_blk("rb", [], 101, "REAL", "BLUE"), merged=[])]
    ok_, txt = gate_riders(objs, cr, bl, wr, 300)
    assert ok_ in (0, 3), (ok_, txt)
    assert gate_riders([], cr, bl, wr, 300)[0] == 3
    assert gate_riders(objs, {k: v for k, v in cr.items() if not k.startswith("r")}, bl, wr, 300)[0] == 1          # carried, none admitted
    crz = {k: dict(v) for k, v in cr.items()}
    crz["r0"]["committedSompi"] = str(2 ** 64 - 1)
    assert gate_riders(objs, crz, bl, wr, 300)[0] == 1                                                                 # a rider priced u64-max-like
    open(os.path.join(wr, "new0", "kaspad.out"), "w").write("[palw-floor-state] daa=101 block=bh idle->probe last_blue=- until=109 last_probe_end=-\n")
    assert gate_riders(objs, cr, bl, wr, 300)[0] == 1                                                                  # a transition at a block with no REAL attempt
    bl2 = [dict(_blk("bh", [], 101, "LEGACY_HEARTBEAT", "BLUE"), merged=["rb"]), dict(_blk("rb", [], 101, "REAL", "BLUE"), merged=[])]
    assert gate_riders(objs, cr, bl2, wr, 300)[0] in (0, 3), gate_riders(objs, cr, bl2, wr, 300)                       # the REAL attempt is in its mergeset: the attempt stepped it
    print("selftest ok")
    return 0


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("share")
    s.add_argument("--port", type=int, required=True)
    s.add_argument("--fence", type=int, required=True)
    s.add_argument("--window", type=int, default=60)
    s.add_argument("--step", type=int, default=30)
    s.add_argument("--settle", type=int, default=20)
    s.add_argument("--to", type=int, default=None)
    s.add_argument("--idle-file", default=None)
    s.add_argument("--work", default="~/.misaka-palw-improve-drill")
    s.add_argument("--producers", default="", help="comma list of the REAL producers' nodes (eligibility)")
    r = sub.add_parser("redblue")
    r.add_argument("--port", type=int, required=True)
    r.add_argument("--fence", type=int, required=True)
    r.add_argument("--window", type=int, default=60)
    r.add_argument("--from", dest="frm", type=int, default=None)
    r.add_argument("--to", type=int, default=None)
    c = sub.add_parser("recovery")
    c.add_argument("--port", type=int, required=True)
    c.add_argument("--state", required=True)
    c.add_argument("--k", type=int, default=20)
    c.add_argument("--margin", type=int, default=6)
    q = sub.add_parser("panel")
    q.add_argument("--work", default="~/.misaka-palw-improve-drill")
    q.add_argument("--p50-max", type=int, default=6)
    q.add_argument("--p95-max", type=int, default=12)
    q.add_argument("--wait-max", type=int, default=40)
    g = sub.add_parser("gates")
    g.add_argument("--port", type=int, required=True)
    g.add_argument("--fence", type=int, required=True)
    g.add_argument("--window", type=int, default=60)
    g.add_argument("--step", type=int, default=30)
    g.add_argument("--settle", type=int, default=20)
    g.add_argument("--to", type=int, default=None)
    g.add_argument("--idle-file", default=None)
    g.add_argument("--work", default="~/.misaka-palw-improve-drill")
    g.add_argument("--producers", default="new4,new6")
    g.add_argument("--state", default="~/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill/recovery.json")
    g.add_argument("--evd", default="~/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill")
    g.add_argument("--lanes", default="~/Downloads/MISAKA-wt-b/lanes/evidence", help="directory of <lane>/verdict.txt files (the pre-checks)")
    g.add_argument("--k", type=int, default=20)
    g.add_argument("--margin", type=int, default=6)
    g.add_argument("--idle-slots", type=int, default=IDLE_SLOTS)
    g.add_argument("--probe-slots", type=int, default=PROBE_SLOTS)
    g.add_argument("--cooldown", type=int, default=COOL_SLOTS)
    g.add_argument("--restart1", default="~/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill/restart1.json")
    g.add_argument("--stale", default="~/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill/stale.json")
    g.add_argument("--p50-max", type=int, default=6)
    g.add_argument("--p95-max", type=int, default=12)
    g.add_argument("--wait-max", type=int, default=40)
    e = sub.add_parser("execsample")
    e.add_argument("--port", type=int, required=True)
    e.add_argument("--out", required=True)
    e.add_argument("--interval", type=int, default=30)
    sub.add_parser("selftest")
    a = p.parse_args()
    sys.exit({"share": share, "gates": gates, "redblue": redblue, "recovery": recovery, "panel": panel, "execsample": exec_sampler, "selftest": lambda _a: selftest()}[a.cmd](a))


if __name__ == "__main__":
    main()
