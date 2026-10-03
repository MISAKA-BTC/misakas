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
def raw_blocks(port):
    dag = call(port, "getBlockDagInfo", {})
    cursor = pick(dag, "pruningPointHash", default=None)
    seen = set()
    while True:
        r = call(port, "getBlocks", {"lowHash": cursor, "includeBlocks": True, "includeTransactions": False}, timeout=120)
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
                    "lane": str(pick(v, "laneClass", default="") or "")})
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
def share(a):
    blocks = load_blocks(a.port)
    tip = max((b["daa"] for b in blocks), default=0)
    hi = min(a.to or tip, tip)
    idle = []
    if a.idle_file and os.path.exists(a.idle_file):
        idle = [tuple(map(int, ln.split())) for ln in open(a.idle_file) if ln.strip()]
    ws = windows(blocks, a.fence + a.settle, hi, a.window, a.step, idle)
    print(HEAD)
    for w in ws:
        print(line(w))
    if not ws:
        print("INCOMPLETE: no complete window past the fence yet")
        return 3
    producers = [x for x in (a.producers or "").split(",") if x]
    if producers:
        eligible_windows(ws, os.path.expanduser(a.work), producers)
    else:
        for w in ws:
            w["eligible"], w["why"] = True, ""
    elig = [w for w in ws if w["eligible"]]
    inel = [w for w in ws if not w["eligible"]]
    # the 8k failure, before and after the fence (whole-chain windows of the same width, no overlap)
    before = windows(blocks, 20, a.fence, a.window, a.window, idle)
    after = windows(blocks, a.fence, hi, a.window, a.window, idle)
    for name, grp in (("before the fence", before), ("after the fence", after)):
        red, real, t = sum_anticone(grp)
        print(f"RED REAL attempts {name}: {red} of {real} REAL attempts; blue blocks in their anticones (up to 40 attempts per window): floor {t['floor']}, "
              f"heartbeat {t['hb']}, other REAL {t['real']}, exec {t['exec']}")
    comp = {k: sum(w[k] for w in elig) for k in ("n", "REAL", "EXEC", "FALLBACK", "LEGACY_FLOOR", "LEGACY_HEARTBEAT", "other")}
    if comp["n"]:
        pc = lambda x: f"{100.0 * x / comp['n']:.1f} %"  # noqa: E731
        print(f"composition of the {len(elig)} eligible windows ({comp['n']} non-RED blocks): REAL {pc(comp['REAL'])}, EXEC {pc(comp['EXEC'])}, heartbeat {pc(comp['LEGACY_HEARTBEAT'])}, "
              f"floor {pc(comp['FALLBACK'] + comp['LEGACY_FLOOR'])}, other {pc(comp['other'])} — reported as measured: with two REAL producers the heartbeat share is a supply fact, not tuned")
    if inel:
        print(f"NOT ELIGIBLE ({len(inel)} windows, not counted: a producer was down or held): " + ", ".join(f"{w['lo']}-{w['hi']} [{w['why']}]" for w in inel[:12]))
    if not any(w["n"] and not w["idle"] for w in elig):
        print("INCOMPLETE: no eligible window with REAL load yet — a share over no real load proves nothing")
        return 3
    bad = [w for w in elig if w["n"] and not w["idle"] and (w["share"] < 0.9 or w["hb"] > 0.1 or w["floor"] > 0)]
    if bad:
        print(f"FAIL: {len(bad)} of {len(elig)} eligible windows break the share (REAL+EXEC >= 90 %, heartbeats <= 10 %, no floor outside idle windows): "
              + ", ".join(f"{w['lo']}-{w['hi']}" for w in bad[:8]))
        return 1
    print(f"PASS: {len(elig)} eligible windows past the fence ({len(inel)} not eligible, reported above), min share {min(w['share'] for w in elig if w['share'] is not None)}, "
          f"max heartbeat {max(w['hb'] for w in elig if w['hb'] is not None)}, {sum(1 for w in elig if w['idle'])} idle")
    return 0


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
    out.append(f"(a) DAA over the stop: {len(missing)} DAA without a block; longest slot gap {worst[0]:.0f} s at DAA {worst[1]}")
    if missing:
        bad.append(f"(a) DAA {missing[:6]} had no block while the producers were down")
    if worst[0] > 4 * SLOT_S:
        bad.append(f"(a) a slot took {worst[0]:.0f} s (> {4 * SLOT_S:.0f}: the drill's own slots reach ~270 s)")
    # (b) floors resume after K idle slots
    early = [d for d in floors if last_real < d < last_real + k]
    first_floor = next((d for d in floors if d > last_real), None)
    out.append(f"(b) floors: first BLUE FALLBACK after the last REAL attempt at DAA {first_floor} (expected {last_real + k}..{last_real + k + margin}); earlier than K: {early}")
    if early:
        bad.append(f"(b) floor attempts at DAA {early} came before K = {k} idle slots")
    if first_floor is None:
        (wait if restart - last_real <= k + margin else bad).append("(b) no floor attempt resumed during the stop")
    elif first_floor > last_real + k + margin:
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
              f"{sm.get('bind_latency_n'):>4} {str(sm['backlog_first']) + '->' + str(sm['backlog_last']):>10} {str(sm['backlog_slope_per_daa_second_half']):>7} "
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
        kind = "REAL" if (d <= 20 or d >= 36) else ("LEGACY_HEARTBEAT" if d < 23 else "FALLBACK")
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
    # the release's K = 20: REAL until DAA 20, heartbeats while idle, floors from DAA 40, a 44-slot stop (21..65), REAL again from 66, no floor after 86
    chain20 = []
    for d in range(10, 130):
        kind = "REAL" if (d <= 20 or d >= 66) else ("LEGACY_HEARTBEAT" if d < 40 else "FALLBACK")
        chain20.append(_blk(f"y{d}", [f"y{d - 1}"] if d > 10 else [], d, kind, "BLUE"))
    st20 = {"stop_daa": 21, "restart_daa": 65, "end_daa": 110}
    code, lines = recovery_verdict(chain20, st20)
    assert code == 0, lines
    early20 = [dict(b) for b in chain20]
    early20[21]["kind"] = "FALLBACK"             # DAA 31: eleven idle slots after the last REAL attempt (DAA 20), well before K = 20
    assert recovery_verdict(early20, st20)[0] == 1
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
    sub.add_parser("selftest")
    a = p.parse_args()
    sys.exit({"share": share, "redblue": redblue, "recovery": recovery, "panel": panel, "selftest": lambda _a: selftest()}[a.cmd](a))


if __name__ == "__main__":
    main()
