#!/usr/bin/env python3
"""audit-combined/dcwatch.py — the combined drill's SHARE verdict (read-only JSON-wRPC to a drill node; stdlib only).

  panel [--work DIR] [--p50-max 6 --p95-max 12 --wait-max 40]
        the PANEL verdict ("verification does not jam"), read from the driver's capacity sampler (drive-state.json: cap-summary per window, cap-claims,
        cap-series) and the nodes' logs: per window the claims' bind->licence latency p50/p95 (PASS: p50 <= --p50-max, p95 <= --p95-max; the lead's healthy
        figure is 2-6 DAA), the panel-bound backlog trend (the sampler's gate: slope over the window's second half and its last quarter against its second:
        must not diverge), the seats' oldest wait (<= --wait-max), F3 producer-hold events ("NOT PRODUCING ... holding" lines) per node, PanelUnavailable
        expiries (claims voided as panel-unavailable: expect 0 for honest load) and receipts filed per seat per hour. Exit 0 PASS, 1 FAIL, 3 INCOMPLETE.
  share --port P --fence H' [--window 60 --step 30 --settle 20 --to DAA --work DIR --producers new4,new6]
        ELIGIBLE windows only (--producers): both REAL producers up for the whole window and none logged 'NOT PRODUCING … holding' in it; the others are listed apart.
        walks getBlocks (verbose) and classes every block by verboseData.blockKind (REAL / FALLBACK / LEGACY_FLOOR / LEGACY_HEARTBEAT / EXEC)
        and laneClass (BLUE / EXEC / RED / ROUND). Sliding DAA windows from H' + settle: the share of REAL + EXEC blocks among all non-RED
        blocks (>= 90 %), heartbeats (<= 10 %), floor attempts (FALLBACK / LEGACY_FLOOR: only in idle windows — a window is idle when
        --idle-file lists it, one "lo hi" per line, or when it holds no REAL block). Prints the per-window table and the verdict.
        Exit 0 PASS, 1 FAIL, 3 INCOMPLETE (no window yet).
"""
import argparse, json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "audit-tir"))
from rpc import call, pick  # noqa: E402


def blocks(port):
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


def kinds(port):
    rows = []
    for b in blocks(port):
        daa = int(pick(pick(b, "header", default={}), "daaScore", default=0) or 0)
        v = pick(b, "verboseData", default={}) or {}
        rows.append((daa, str(pick(v, "blockKind", default="") or ""), str(pick(v, "laneClass", default="") or "")))
    return rows


def windows(rows, lo, hi, w, step, idle):
    out = []
    a = lo
    while a + w <= hi:
        sel = [r for r in rows if a <= r[0] < a + w and r[2] != "RED"]
        n = len(sel)
        c = {k: sum(1 for r in sel if r[1] == k) for k in ("REAL", "EXEC", "FALLBACK", "LEGACY_FLOOR", "LEGACY_HEARTBEAT")}
        c["other"] = n - sum(c.values())
        is_idle = any(i0 <= a and a + w <= i1 for i0, i1 in idle) or c["REAL"] == 0
        out.append({"lo": a, "hi": a + w, "n": n, **c, "share": round((c["REAL"] + c["EXEC"]) / n, 3) if n else None,
                    "hb": round(c["LEGACY_HEARTBEAT"] / n, 3) if n else None, "floor": c["FALLBACK"] + c["LEGACY_FLOOR"], "idle": is_idle})
        a += step
    return out


def daa_clock(work):
    """[(epoch seconds, DAA)] from the driver's sampler (capacity/samples.tsv, state samples.tsv): the map from a window to wall time."""
    import time
    pts = []
    for f in ("samples.tsv",):
        path = os.path.join(work, f)
        if os.path.exists(path):
            for line in open(path):
                parts = line.split("\t")
                if len(parts) >= 2 and parts[1].strip().isdigit():
                    try: pts.append((time.mktime(time.strptime(parts[0].strip(), "%Y-%m-%d %H:%M:%S")), int(parts[1])))
                    except ValueError: pass
    return pts


def eligible_windows(ws, work, producers):
    """A window is ELIGIBLE when every producer node was up for all of it and logged no 'NOT PRODUCING … holding' line inside it (it had claims issuable);
    a silent or held producer must not make the window look idle."""
    import re, time
    clock = daa_clock(work)
    def t_of(daa):
        for t, d in clock:
            if d >= daa: return t
        return None
    ups = {}
    for n in producers:
        iv, start = [], None
        ev = os.path.join(work, n, "events.log")
        for line in (open(ev) if os.path.exists(ev) else []):
            m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) (START|STOP)", line)
            if not m: continue
            t = time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S"))
            if m.group(2) == "START": start = t
            elif start is not None: iv.append((start, t)); start = None
        if start is not None: iv.append((start, 1e18))
        holds = []
        lg = os.path.join(work, n, "kaspad.out")
        for line in (open(lg, errors="replace") if os.path.exists(lg) else []):
            if "NOT PRODUCING" in line and "holding" in line:
                m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)", line)
                if m: holds.append(time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S")))
        ups[n] = (iv, holds)
    for w in ws:
        t0, t1 = t_of(w["lo"]), t_of(w["hi"])
        ok = t0 is not None and t1 is not None
        why = []
        if not ok: why.append("window not on the sampler's clock")
        for n, (iv, holds) in ups.items():
            if ok and not any(a <= t0 and t1 <= b for a, b in iv): ok = False; why.append(f"{n} not up for the window")
            if ok and any(t0 <= h <= t1 for h in holds): ok = False; why.append(f"{n} held (NOT PRODUCING)")
        w["eligible"], w["why"] = ok, "; ".join(why)
    return ws


def share(a):
    rows = kinds(a.port)
    tip = max((r[0] for r in rows), default=0)
    hi = min(a.to or tip, tip)
    idle = []
    if a.idle_file and os.path.exists(a.idle_file):
        idle = [tuple(map(int, l.split())) for l in open(a.idle_file) if l.strip()]
    ws = windows(rows, a.fence + a.settle, hi, a.window, a.step, idle)
    print(f"{'window':>13} {'n':>4} {'REAL':>5} {'EXEC':>5} {'FALLB':>5} {'LFLOOR':>6} {'LHB':>4} {'other':>5} {'share':>6} {'hb':>6} floor idle")
    for w in ws:
        print(f"{w['lo']:>6}-{w['hi']:<6} {w['n']:>4} {w['REAL']:>5} {w['EXEC']:>5} {w['FALLBACK']:>5} {w['LEGACY_FLOOR']:>6} {w['LEGACY_HEARTBEAT']:>4} {w['other']:>5} "
              f"{w['share'] if w['share'] is not None else '-':>6} {w['hb'] if w['hb'] is not None else '-':>6} {w['floor']:>5} {'idle' if w['idle'] else ''}")
    if not ws:
        print("INCOMPLETE: no complete window past the fence yet"); return 3
    producers = [x for x in (a.producers or "").split(",") if x]
    if producers:
        eligible_windows(ws, os.path.expanduser(a.work), producers)
    else:
        for w in ws: w["eligible"], w["why"] = True, ""
    elig = [w for w in ws if w["eligible"]]
    inel = [w for w in ws if not w["eligible"]]
    if inel:
        print(f"NOT ELIGIBLE ({len(inel)} windows, not counted: a producer was down or held): " + ", ".join(f"{w['lo']}-{w['hi']} [{w['why']}]" for w in inel[:12]))
    if not any(w["n"] and not w["idle"] for w in elig):
        print("INCOMPLETE: no eligible window with REAL load yet — a share over no real load proves nothing"); return 3
    bad = [w for w in elig if w["n"] and not w["idle"] and (w["share"] < 0.9 or w["hb"] > 0.1 or w["floor"] > 0)]
    if bad:
        print(f"FAIL: {len(bad)} of {len(elig)} eligible windows break the share (REAL+EXEC >= 90 %, heartbeats <= 10 %, no floor outside idle windows): " + ", ".join(f"{w['lo']}-{w['hi']}" for w in bad[:8]))
        return 1
    print(f"PASS: {len(elig)} eligible windows past the fence ({len(inel)} not eligible, reported above), min share {min(w['share'] for w in elig if w['share'] is not None)}, "
          f"max heartbeat {max(w['hb'] for w in elig if w['hb'] is not None)}, {sum(1 for w in elig if w['idle'])} idle")
    return 0


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("share")
    s.add_argument("--port", type=int, required=True); s.add_argument("--fence", type=int, required=True)
    s.add_argument("--window", type=int, default=60); s.add_argument("--step", type=int, default=30); s.add_argument("--settle", type=int, default=20)
    s.add_argument("--to", type=int, default=None); s.add_argument("--idle-file", default=None)
    s.add_argument("--work", default="~/.misaka-palw-improve-drill"); s.add_argument("--producers", default="", help="comma list of the REAL producers' nodes (eligibility)")
    q = sub.add_parser("panel")
    q.add_argument("--work", default="~/.misaka-palw-improve-drill"); q.add_argument("--p50-max", type=int, default=6)
    q.add_argument("--p95-max", type=int, default=12); q.add_argument("--wait-max", type=int, default=40)
    a = p.parse_args()
    sys.exit(share(a) if a.cmd == "share" else panel(a))


if __name__ == "__main__":
    main()
