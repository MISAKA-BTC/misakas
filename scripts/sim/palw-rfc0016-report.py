#!/usr/bin/env python3
"""RFC-0016 §11 outcome-first model report (user ruling R6, 2026-10-15).

Input: two JSON snapshots of the per-model report (`palw_rfc0016_model_report_v1`'s rows and per-miner rows, as dumped by a node
reader or a test harness) — `{"epoch": n, "rows": [...], "miners": [...]}` with the field names of `PalwModelReportRowV1` /
`PalwMinerRevenueRowV1`; `null` is UNKNOWN.

Output: OUTCOMES (physical blocks, rights consumed/unused, Finals, coinbase payout, escrow payout, per-miner revenue, refusals, queue
wait, REAL gap / heartbeat dependence) as before/after deltas, and the TARGET (`P_m`, issued rights) in a separate column labelled as a
target. The verdict column follows the one rule (the same as `palw_report_outcome_v1`): an improvement is claimed only when physical
blocks or payout rose; a target change alone prints `TARGET ONLY`, never `IMPROVED`. UNKNOWN never counts as a change.

Usage:
  python3 -I scripts/sim/palw-rfc0016-report.py before.json after.json
  python3 -I scripts/sim/palw-rfc0016-report.py --self-test
"""
import json
import sys

OUTCOME_FIELDS = [
    "physical_blocks", "rights_consumed", "rights_unused", "finals", "payout_coinbase_sompi", "payout_escrow_sompi",
    "refused_for_capacity", "refused_for_rights", "queue_wait_p50_daa", "queue_wait_p95_daa", "real_gap_daa",
    "heartbeat_dependent_daa", "capacity_shortfall_per_span",
]


def delta(b, a):
    if b is None or a is None:
        return None
    return a - b


def verdict(before, after):
    phys = after.get("physical_blocks", 0) - before.get("physical_blocks", 0)
    pay = delta(before.get("payout_coinbase_sompi"), after.get("payout_coinbase_sompi")) or 0
    if phys < 0 or pay < 0:
        return "WORSENED"
    if phys > 0 or pay > 0:
        return "IMPROVED"
    tgt = delta(before.get("target_p_ppm"), after.get("target_p_ppm")) or 0
    return "TARGET ONLY (not an outcome)" if tgt else "UNCHANGED"


def report(before, after):
    b = {r["model"]: r for r in before["rows"]}
    a = {r["model"]: r for r in after["rows"]}
    lines = []
    for m in sorted(set(b) | set(a)):
        rb, ra = b.get(m, {}), a.get(m, {})
        out = {f: delta(rb.get(f), ra.get(f)) for f in OUTCOME_FIELDS}
        lines.append({
            "model": m,
            "verdict": verdict(rb, ra),
            "outcomes_delta": out,
            "target (not an outcome)": {"P_m_ppm": [rb.get("target_p_ppm"), ra.get("target_p_ppm")],
                                         "rights_issued": [rb.get("rights_issued"), ra.get("rights_issued")]},
        })
    mb = {(r["bond"], r["model"]): r["payout_coinbase_sompi"] for r in before.get("miners", [])}
    ma = {(r["bond"], r["model"]): r["payout_coinbase_sompi"] for r in after.get("miners", [])}
    miners = [{"bond": k[0], "model": k[1], "revenue_delta_sompi": delta(mb.get(k), ma.get(k))} for k in sorted(set(mb) | set(ma))]
    return {"models": lines, "existing_miners": miners}


def self_test():
    b = {"rows": [{"model": "A", "target_p_ppm": 100, "physical_blocks": 5, "payout_coinbase_sompi": 50}], "miners": []}
    t = {"rows": [{"model": "A", "target_p_ppm": 300, "physical_blocks": 5, "payout_coinbase_sompi": 50}], "miners": []}
    p = {"rows": [{"model": "A", "target_p_ppm": 300, "physical_blocks": 8, "payout_coinbase_sompi": 50}], "miners": []}
    u = {"rows": [{"model": "A", "target_p_ppm": 300, "physical_blocks": 5, "payout_coinbase_sompi": None}], "miners": []}
    assert report(b, t)["models"][0]["verdict"].startswith("TARGET ONLY")
    assert report(b, p)["models"][0]["verdict"] == "IMPROVED"
    assert report(b, u)["models"][0]["verdict"].startswith("TARGET ONLY"), "UNKNOWN payout is not a change"
    print("self-test: the target-only rule holds (no IMPROVED from a target alone)")


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        return
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    with open(sys.argv[1]) as f:
        before = json.load(f)
    with open(sys.argv[2]) as f:
        after = json.load(f)
    print(json.dumps(report(before, after), indent=1))


if __name__ == "__main__":
    main()
