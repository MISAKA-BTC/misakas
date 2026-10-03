#!/usr/bin/env python3
"""audit-combined/dcwatch.py — the combined drill's SHARE verdict (read-only JSON-wRPC to a drill node; stdlib only).

  panel [--work DIR] [--p50-max 6 --p95-max 12 --wait-max 40]
        the PANEL verdict ("verification does not jam"), read from the driver's capacity sampler (drive-state.json: cap-summary per window, cap-claims,
        cap-series) and the nodes' logs: per window the claims' bind->licence latency p50/p95 (PASS: p50 <= --p50-max, p95 <= --p95-max; the lead's healthy
        figure is 2-6 DAA), the panel-bound backlog trend (the sampler's gate: slope over the window's second half and its last quarter against its second:
        must not diverge), the seats' oldest wait (<= --wait-max), F3 producer-hold events ("NOT PRODUCING ... holding" lines) per node, PanelUnavailable
        expiries (claims voided as panel-unavailable: expect 0 for honest load) and receipts filed per seat per hour. Exit 0 PASS, 1 FAIL, 3 INCOMPLETE.
  share --port P --fence H' [--window 60 --step 30 --settle 20 --to DAA]
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
    if not any(w["n"] and not w["idle"] for w in ws):
        print("INCOMPLETE: every window is idle (no REAL block) — a share over no real load proves nothing"); return 3
    bad = [w for w in ws if w["n"] and not w["idle"] and (w["share"] < 0.9 or w["hb"] > 0.1 or w["floor"] > 0)]
    if bad:
        print(f"FAIL: {len(bad)} of {len(ws)} windows break the share (REAL+EXEC >= 90 %, heartbeats <= 10 %, no floor outside idle windows): " + ", ".join(f"{w['lo']}-{w['hi']}" for w in bad[:8]))
        return 1
    print(f"PASS: {len(ws)} windows past the fence, min share {min(w['share'] for w in ws if w['share'] is not None)}, max heartbeat {max(w['hb'] for w in ws if w['hb'] is not None)}, "
          f"{sum(1 for w in ws if w['idle'])} idle")
    return 0


def panel(a):
    work = os.path.expanduser(a.work)
    d = json.load(open(os.path.join(work, "drive-state.json")))["data"]
    cap, claims = d.get("cap-summary") or {}, d.get("cap-claims") or {}
    if not cap:
        print("INCOMPLETE: no capacity window measured yet"); return 3
    bad, rows = [], []
    for name, sm in cap.items():
        why = []
        if sm.get("bind_latency_p50") is None:
            why.append("no bind->licence sample")
        else:
            if sm["bind_latency_p50"] > a.p50_max: why.append(f"p50 {sm['bind_latency_p50']} > {a.p50_max}")
            if sm["bind_latency_p95"] > a.p95_max: why.append(f"p95 {sm['bind_latency_p95']} > {a.p95_max}")
        if sm.get("diverges"): why.append("backlog diverges")
        if (sm.get("oldest_wait_max") or 0) > a.wait_max: why.append(f"oldest wait {sm['oldest_wait_max']} > {a.wait_max}")
        rows.append((name, sm, why))
        if why: bad.append(f"{name}: " + "; ".join(why))
    unavailable = [c for c, r in claims.items() if "unavail" in str(r.get("void", "")).lower()]
    f3, rec = {}, {}
    import glob, re, time
    for log in sorted(glob.glob(os.path.join(work, "new*", "kaspad.out"))):
        node = os.path.basename(os.path.dirname(log))
        n_hold = n_rec = 0
        first = last = None
        with open(log, errors="replace") as f:
            for line in f:
                if "NOT PRODUCING" in line and "holding" in line: n_hold += 1
                if "[palw-panel] filed a" in line and "receipt" in line:
                    n_rec += 1
                    m = re.match(r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)", line)
                    if m:
                        t = time.mktime(time.strptime(m.group(1), "%Y-%m-%d %H:%M:%S")); first = first or t; last = t
        f3[node] = n_hold
        rec[node] = round(n_rec / max((last - first) / 3600, 1 / 60), 1) if first and last and last > first else None
    print(f"{'window':>10} {'bindP50':>7} {'bindP95':>7} {'n':>4} {'backlog':>10} {'slope':>7} {'oldest':>6} {'diverges':>8}")
    for name, sm, why in rows:
        print(f"{name:>10} {str(sm.get('bind_latency_p50')):>7} {str(sm.get('bind_latency_p95')):>7} {sm.get('bind_latency_n'):>4} "
              f"{str(sm['backlog_first'])+'->'+str(sm['backlog_last']):>10} {str(sm['backlog_slope_per_daa_second_half']):>7} {str(sm.get('oldest_wait_max')):>6} {str(sm['diverges']):>8}  {'; '.join(why)}")
    print(f"F3 / producer-hold lines per node: {f3}")
    print(f"PanelUnavailable expiries (voided claims): {len(unavailable)}")
    print(f"receipts filed per hour per seat node (log span): {rec}")
    if unavailable: bad.append(f"{len(unavailable)} claims expired as panel-unavailable")
    if bad:
        print("FAIL: " + " | ".join(bad)); return 1
    print(f"PASS: {len(rows)} windows: nothing diverges, bind->licence within p50 {a.p50_max} / p95 {a.p95_max}"); return 0


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("share")
    s.add_argument("--port", type=int, required=True); s.add_argument("--fence", type=int, required=True)
    s.add_argument("--window", type=int, default=60); s.add_argument("--step", type=int, default=30); s.add_argument("--settle", type=int, default=20)
    s.add_argument("--to", type=int, default=None); s.add_argument("--idle-file", default=None)
    q = sub.add_parser("panel")
    q.add_argument("--work", default="~/.misaka-palw-improve-drill"); q.add_argument("--p50-max", type=int, default=6)
    q.add_argument("--p95-max", type=int, default=12); q.add_argument("--wait-max", type=int, default=40)
    a = p.parse_args()
    sys.exit(share(a) if a.cmd == "share" else panel(a))


if __name__ == "__main__":
    main()
