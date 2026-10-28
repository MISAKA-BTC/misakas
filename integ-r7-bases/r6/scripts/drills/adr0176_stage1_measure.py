#!/usr/bin/env python3
"""adr0176_stage1_measure.py — ADR-0176 cap derivation, Stage 1: measure the honest minimal configuration.

ユーザー方針 2026-10-15 item 8 / ユーザー判断 2026-10-15 item 7. Runs ON the node host against its local wRPC-JSON listener
(misaka_wrpc_json.WsRpc), for ONE reference miner bond (Qwen2.5-1.5B A16, fixed artifact/context/job, reference hardware, the
target ruleset's minimum producer bond), under Panel=1 or Panel=0. It records, per claim, the stages

    compute + lottery win -> claim acceptance (REAL block) -> verification (bound / licensed) -> Final -> settlement (vesting)
    -> bond capacity recovery (the claim leaves its window: d + W, read from the reservation split where the node reports it)

and, from the node log when given, every attempt the fold refused for the bond budget (REAL stopped only for lack of cap), so the
SUSTAINED accepted / REAL / Final rate is measured — not the peak. Nothing here decides a value.

Outputs (in --out):
  events.jsonl   one line per observed change (claim phase, DAA, wall time)
  summary.json   intervals between accepted claims, acceptance->Final delays, rates per day-equivalent, the longest REAL gap,
                 budget refusals, heartbeat-only spans, reservation splits seen, and ADR0176_PROFILE — the Stage 2 input of
                 `cargo test -p kaspa-consensus-core --test adr0176_cap_sweep -- --ignored stage2_sweep_from_a_measured_profile`

Usage (python -I):
  adr0176_stage1_measure.py --bond <txid>:<index> --mode 1 --capital-bili 13000 --duration-daa 2160 --out DIR
                            [--port 26314] [--poll-secs 20] [--node-log /path/kaspad.log]

Self-test (no node): adr0176_stage1_measure.py --self-test
"""
import argparse
import importlib.util
import json
import re
import sys
import time
from pathlib import Path

DAA_PER_DAY = 720  # PALW_BOND_HOLD_DAA_PER_DAY_V1: 86 400 s / the 120 s clock slot
REFUSAL = re.compile(r"(BondBudgetExhausted|BondBudgetRefused|bond budget|budget window has no room|burst cap)", re.I)


def load_client():
    here = Path(__file__).resolve().parent / "misaka_wrpc_json.py"
    spec = importlib.util.spec_from_file_location("misaka_wrpc_json", here)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.WsRpc


def phase_of(row):
    return (row.get("phase") or "").lower()


def summarize(events, refusals, mode, capital_bili):
    """Pure: the Stage 1 summary from the recorded events (also what --self-test checks)."""
    accepted = sorted({e["acceptedDaa"] for e in events if e.get("acceptedDaa") is not None and e.get("first")})
    finals = {}
    accepted_of = {}
    for e in events:
        cid = e["claimId"]
        if e.get("acceptedDaa") is not None:
            accepted_of.setdefault(cid, e["acceptedDaa"])
        if e.get("phase") == "final" and cid not in finals:
            finals[cid] = e["daa"]
    intervals = [b - a for a, b in zip(accepted, accepted[1:]) if b > a]
    delays = sorted(finals[c] - accepted_of[c] for c in finals if c in accepted_of and finals[c] >= accepted_of[c])
    span = (max(accepted) - min(accepted)) if len(accepted) > 1 else 0
    days = span / DAA_PER_DAY if span else 0
    refused_daa = sorted({r["daa"] for r in refusals})
    # Heartbeat-only spans: DAA ranges with no accepted claim of this bond longer than the honest median interval.
    median = sorted(intervals)[len(intervals) // 2] if intervals else 0
    heartbeat_only = [(a, b) for a, b in zip(accepted, accepted[1:]) if median and b - a > 2 * median]
    # REAL stopped ONLY for lack of cap: refusals inside a gap (compute + submission were present).
    cap_stall = [(a, b) for (a, b) in zip(accepted, accepted[1:]) if any(a < d < b for d in refused_daa)]
    return {
        "mode": "PanelZero" if mode == 0 else "PanelOne",
        "capitalBili": capital_bili,
        "acceptedClaims": len(accepted),
        "finals": len(finals),
        "spanDaa": span,
        "acceptedPerDay": (len(accepted) / days) if days else None,
        "finalsPerDay": (len(finals) / days) if days else None,
        "intervalsDaa": intervals,
        "finalDelaysDaa": delays,
        "maxRealGapDaa": max(intervals) if intervals else None,
        "budgetRefusals": len(refusals),
        "capStallSpans": cap_stall,
        "heartbeatOnlySpans": heartbeat_only,
        "ADR0176_PROFILE": "{};{};{};{}".format(
            ",".join(str(i) for i in intervals) or "240", ",".join(str(d) for d in delays) or "60", capital_bili, mode
        ),
        "note": "sustained rates over the whole span; no value here is a production cap (Stage 2 decides nothing either)",
    }


def run(args):
    WsRpc = load_client()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    events_path = out / "events.jsonl"
    seen = {}
    events = []
    refusals = []
    log_pos = 0
    c = WsRpc(port=args.port)
    start_daa = None
    try:
        while True:
            info = c.call("getBlockDagInfo") or {}
            daa = int(info.get("virtualDaaScore") or 0)
            start_daa = daa if start_daa is None else start_daa
            claims = c.call("getPalwClaims", {"bond": args.bond, "role": "executor", "includeTerminal": True, "limit": 500}) or {}
            for row in claims.get("claims", []):
                cid = row.get("claimId")
                ph = phase_of(row)
                if seen.get(cid) == ph:
                    continue
                ev = {
                    "claimId": cid,
                    "phase": ph,
                    "daa": daa,
                    "wall": time.time(),
                    "acceptedDaa": row.get("acceptedDaa"),
                    "first": cid not in seen,
                    "committedSompi": row.get("committedSompi"),
                    "reservationSplit": row.get("reservationSplit") or None,
                    "vestingStage": row.get("vestingStage"),
                }
                seen[cid] = ph
                events.append(ev)
                with events_path.open("a") as f:
                    f.write(json.dumps(ev) + "\n")
            if args.node_log:
                p = Path(args.node_log)
                if p.exists():
                    with p.open("r", errors="replace") as f:
                        f.seek(log_pos)
                        for line in f:
                            if REFUSAL.search(line) and args.bond.split(":")[0] in line:
                                refusals.append({"daa": daa, "line": line.strip()[:300]})
                        log_pos = f.tell()
            if daa - start_daa >= args.duration_daa:
                break
            time.sleep(args.poll_secs)
    finally:
        c.close()
    summary = summarize(events, refusals, args.mode, args.capital_bili)
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: summary[k] for k in ("acceptedClaims", "finals", "acceptedPerDay", "maxRealGapDaa", "budgetRefusals")}))


def self_test():
    ev = []
    for k, a in enumerate([240, 480, 720, 1_200, 1_440]):
        cid = f"c{k}"
        ev.append({"claimId": cid, "phase": "provisional", "daa": a, "acceptedDaa": a, "first": True})
        ev.append({"claimId": cid, "phase": "final", "daa": a + 60, "acceptedDaa": a, "first": False})
    s = summarize(ev, [{"daa": 900, "line": "BondBudgetExhausted"}], 1, 13_000)
    assert s["acceptedClaims"] == 5 and s["finals"] == 5
    assert s["intervalsDaa"] == [240, 240, 480, 240]
    assert s["finalDelaysDaa"] == [60] * 5
    assert s["capStallSpans"] == [(720, 1_200)], s["capStallSpans"]
    assert s["ADR0176_PROFILE"].startswith("240,240,480,240;60,60,60,60,60;13000;1")
    print("self-test ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bond")
    ap.add_argument("--mode", type=int, default=1, help="1 = Panel=1, 0 = Panel=0")
    ap.add_argument("--capital-bili", type=int, default=13_000)
    ap.add_argument("--duration-daa", type=int, default=3 * DAA_PER_DAY)
    ap.add_argument("--poll-secs", type=float, default=20.0)
    ap.add_argument("--port", type=int, default=26314)
    ap.add_argument("--node-log")
    ap.add_argument("--out", default="adr0176-stage1")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.bond:
        ap.error("--bond is required")
    run(args)


if __name__ == "__main__":
    main()
