#!/usr/bin/env python3
"""**RFC-0019 §9 — upgrade readiness, MEASURED and never voted.**

Before a fence switch (an annual catalog edition's `activation_daa`, or a flag day such as testnet-12's DAA 14,000) operators
want to know how much of the network already runs the new binary. This script measures it from public node reads and drill
records. **Nothing it prints is a consensus input**: the switch happens at the fence height whatever these numbers say, there is
no vote and no stake-weighted signal, and ~95 % is an OPERATIONAL target for the release owner's go / no-go, not a rule.

A version self-report can be faked (a node can announce any schedule id), so every "upgraded" reading here is labelled
SELF-REPORTED, and metric M3 — real interop runs of independent full nodes (sync, reorg, IBD) across the switch — is required
evidence, not an optional extra.

Metrics (numerator / denominator, or UNKNOWN when the input is missing):

  M1  share of recently adopted PALW compute (claims accepted in the last `window_daa`, not voided, weighted by work leaves)
      whose executor bond is declared by an operator whose node(s) report the new schedule
  M2  upgraded share of verifiable Panel capacity (fresh seat-readiness rows, bond × class, held by upgraded operators)
  M3  independent full-node interop: operators whose records show sync, reorg and IBD success on the new binary across the switch
  M4  agreement: polled nodes reporting the expected schedule id, the switch height on their fence schedule and (for a catalog
      switch) the expected catalog root / next activation
  M5  concentration of upgrade capability: the top operator's share of upgraded M1 compute, and its HHI (permille)

Plan (JSON):
  {"network": "testnet-12", "switch_daa": 14000, "update_deadline_daa": 13700, "window_daa": 720, "target_permille": 950,
   "expected": {"consensus_schedule_id": "<hex>", "catalog_root": "<hex, optional>", "catalog_next_activation_daa": <int, optional>},
   "nodes": [{"name": "a1", "wrpc": "127.0.0.1:17110", "operator": "op-a", "bonds": ["<txid>:<index>", ...]}, ...],
   "reference": "a1",                                  # the node whose claim set and readiness rows are measured (default: first)
   "interop_records": ["path/to/interop-*.json"]}      # schema misaka/interop-record/v1 (written by the RFC-0019 drill)

    misaka-release-readiness.py --plan plan.json [--json] [--require-target]
    misaka-release-readiness.py --plan plan.json --observations captured.json   # offline: node reads captured earlier
    misaka-release-readiness.py --self-test

Exit 0 with a report; 3 with --require-target when an operational target is not met (an ops gate, never a consensus one);
2 on unreadable input. Standard-library Python 3.8+; the wRPC-JSON client is scripts/drills/misaka_wrpc_json.py.
"""

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "drills"))

SELF_REPORTED = "SELF-REPORTED (a schedule id can be announced by anyone; M3 interop is the evidence)"
VOIDED = {"voided", "void", "convicted", "rejected"}


def norm(value):
    return value.removeprefix("0x").lower() if isinstance(value, str) else value


def field(row, *names, default=None):
    for name in names:
        if isinstance(row, dict) and row.get(name) is not None:
            return row[name]
    return default


def bond_key(txid, index=None):
    if index is None:
        txid = str(txid)
        return norm(txid) if ":" in txid else norm(txid) + ":0"
    return f"{norm(txid)}:{int(index)}"


def poll(node, timeout):
    """One node's public reads, or {'error': ...}."""
    from misaka_wrpc_json import WsRpc  # noqa: E402 — imported lazily so --self-test needs no network module

    host, port = node["wrpc"].rsplit(":", 1)
    try:
        rpc = WsRpc(host=host, port=int(port), timeout=timeout)
    except OSError as error:
        return {"error": repr(error)}
    try:
        out = {"status": rpc.call("getPalwNodeStatus", {}), "dag": rpc.call("getBlockDagInfo", {})}
        try:
            out["registry"] = rpc.call("getPalwModelRegistry", {})
        except Exception as error:  # noqa: BLE001 — a node without the op is recorded, not fatal
            out["registry_error"] = repr(error)
        try:
            out["claims"] = rpc.call("getPalwClaims", {})
        except Exception as error:  # noqa: BLE001
            out["claims_error"] = repr(error)
        return out
    except Exception as error:  # noqa: BLE001
        return {"error": repr(error)}
    finally:
        rpc.close()


def upgraded(plan, reading):
    """Whether a node's own reads say it runs the new schedule (and, for a catalog switch, the expected catalog). SELF-REPORTED."""
    if not reading or "error" in reading:
        return None
    exp = plan["expected"]
    status = reading.get("status", {})
    if norm(field(status, "consensusScheduleId", "consensus_schedule_id")) != norm(exp["consensus_schedule_id"]):
        return False
    schedule = [int(x) for x in field(status, "fenceSchedule", "fence_schedule", default=[]) or []]
    if int(plan["switch_daa"]) not in schedule and not exp.get("switch_not_on_schedule_ok"):
        return False
    if exp.get("catalog_root"):
        cat = field(reading.get("registry", {}), "annualCatalog", "annual_catalog", default={}) or {}
        roots = {norm(field(e, "root")) for e in field(cat, "editions", default=[]) or []}
        if norm(exp["catalog_root"]) not in roots:
            return False
        if exp.get("catalog_next_activation_daa") is not None:
            tip = int(field(reading.get("dag", {}), "virtualDaaScore", default=0) or 0)
            if tip < int(exp["catalog_next_activation_daa"]) and int(field(cat, "nextActivationDaa", default=0) or 0) != int(exp["catalog_next_activation_daa"]):
                return False
    return True


def ratio(num, den):
    return {"numerator": num, "denominator": den, "permille": (num * 1000 // den) if den else None,
            "value": f"{num}/{den}" if den else "UNKNOWN"}


def measure(plan, readings, interop):
    nodes = plan["nodes"]
    window = int(plan.get("window_daa", 720))
    target = int(plan.get("target_permille", 950))
    up = {n["name"]: upgraded(plan, readings.get(n["name"])) for n in nodes}
    # An operator is upgraded when every one of its reachable nodes reports the new schedule (one stale node of a producer
    # that runs several is enough to fork its blocks at the switch).
    operators = {}
    for n in nodes:
        state = up[n["name"]]
        if state is None:
            continue
        operators.setdefault(n["operator"], []).append(state)
    op_up = {op: all(states) for op, states in operators.items()}
    bond_op = {}
    for n in nodes:
        for b in n.get("bonds", []):
            bond_op[bond_key(b)] = n["operator"]
    ref_name = plan.get("reference") or next((n["name"] for n in nodes if readings.get(n["name"], {}).get("claims")), None)
    ref = readings.get(ref_name, {}) if ref_name else {}
    tip = int(field(ref.get("dag", {}), "virtualDaaScore", default=0) or 0)
    # M1: recent adopted PALW compute.
    rows = field(ref.get("claims", {}), "claims", "rows", default=[]) or []
    up_work = total = unattributed = 0
    per_op = {}
    for r in rows:
        accepted = int(field(r, "acceptedDaa", "accepted_daa", default=0) or 0)
        if accepted + window < tip or str(field(r, "phase", default="")).lower() in VOIDED:
            continue
        w = int(field(r, "workLeaves", "work_leaves", default=1) or 1)
        total += w
        op = bond_op.get(bond_key(field(r, "executorBond", "executor_bond", default="")))
        if op is None:
            unattributed += w
        elif op_up.get(op):
            up_work += w
            per_op[op] = per_op.get(op, 0) + w
    m1 = ratio(up_work, total)
    m1["unattributed_work"] = unattributed
    # M2: verifiable Panel capacity (fresh readiness rows).
    seats = field(ref.get("registry", {}), "readiness", default=[]) or []
    fresh = [s for s in seats if field(s, "fresh", default=False)]
    seat_up = sum(1 for s in fresh if op_up.get(bond_op.get(bond_key(field(s, "bondTxid", "bond_txid", default=""), field(s, "bondIndex", "bond_index", default=0)))))
    m2 = ratio(seat_up, len(fresh))
    # M3: interop records of independent full nodes.
    by_op = {}
    for rec in interop:
        if rec.get("schema") != "misaka/interop-record/v1":
            continue
        checks = rec.get("checks", {})
        ok = all(bool(checks.get(k)) for k in ("sync", "reorg", "ibd")) and rec.get("crossed_switch_daa") is not None
        by_op[rec.get("operator")] = by_op.get(rec.get("operator"), True) and ok
    m3 = ratio(sum(1 for ok in by_op.values() if ok), len(by_op))
    # M4: agreement among reachable nodes.
    reachable = [n["name"] for n in nodes if up[n["name"]] is not None]
    m4 = ratio(sum(1 for name in reachable if up[name]), len(reachable))
    m4["unreachable"] = [n["name"] for n in nodes if up[n["name"]] is None]
    # M5: concentration of the upgraded compute.
    shares = sorted((w * 1000 // up_work for w in per_op.values()), reverse=True) if up_work else []
    m5 = {"upgraded_operators": len(per_op), "top_operator_permille": shares[0] if shares else None,
          "hhi_permille": sum(s * s for s in shares) // 1000 if shares else None}
    met = {
        "M1": m1["permille"] is not None and m1["permille"] >= target,
        "M2": m2["permille"] is None or m2["permille"] >= target,
        "M3": m3["denominator"] > 0 and m3["numerator"] == m3["denominator"],
        "M4": m4["permille"] is not None and m4["permille"] >= target,
    }
    return {
        "schema": "misaka/release-readiness/v1",
        "network": plan.get("network"),
        "switch_daa": plan["switch_daa"],
        "update_deadline_daa": plan.get("update_deadline_daa"),
        "tip_daa": tip,
        "reference_node": ref_name,
        "target_permille": target,
        "upgrade_evidence": SELF_REPORTED,
        "nodes": {name: ("upgraded" if s else "old" if s is False else "unreachable") for name, s in up.items()},
        "M1_recent_palw_compute_upgraded": m1,
        "M2_panel_capacity_upgraded": m2,
        "M3_independent_interop": m3,
        "M4_schedule_and_catalog_agreement": m4,
        "M5_concentration": m5,
        "operational_target_met": met,
        "consensus_effect": "none — the switch happens at switch_daa regardless; this is not a vote",
    }


def human(report):
    out = [f"readiness for the switch at DAA {report['switch_daa']} ({report['network']}) — tip {report['tip_daa']}, "
           f"target {report['target_permille']} ‰ (operational only)", "  " + report["upgrade_evidence"]]
    for key in ("M1_recent_palw_compute_upgraded", "M2_panel_capacity_upgraded", "M3_independent_interop", "M4_schedule_and_catalog_agreement"):
        m = report[key]
        out.append(f"  {key}: {m['value']}" + (f" = {m['permille']} ‰" if m["permille"] is not None else ""))
    m5 = report["M5_concentration"]
    out.append(f"  M5_concentration: {m5['upgraded_operators']} upgraded operator(s), top {m5['top_operator_permille']} ‰, HHI {m5['hhi_permille']} ‰")
    out.append("  target met: " + ", ".join(f"{k} {'yes' if v else 'NO'}" for k, v in report["operational_target_met"].items()))
    out.append("  " + report["consensus_effect"])
    return "\n".join(out)


def self_test():
    sid, other = "aa" * 32, "bb" * 32
    plan = {"network": "testnet-12", "switch_daa": 14000, "window_daa": 100, "target_permille": 950,
            "expected": {"consensus_schedule_id": sid},
            "nodes": [{"name": "a", "wrpc": "x:1", "operator": "A", "bonds": ["01:0"]},
                      {"name": "b", "wrpc": "x:2", "operator": "B", "bonds": ["02:0"]},
                      {"name": "c", "wrpc": "x:3", "operator": "C", "bonds": ["03:0"]}]}

    def reading(schedule_id, fences):
        return {"status": {"consensusScheduleId": schedule_id, "fenceSchedule": fences}, "dag": {"virtualDaaScore": 13500},
                "registry": {"readiness": [{"bondTxid": "01", "bondIndex": 0, "fresh": True}, {"bondTxid": "03", "bondIndex": 0, "fresh": True}]},
                "claims": {"claims": [{"acceptedDaa": 13450, "workLeaves": 90, "executorBond": "01:0", "phase": "Final"},
                                      {"acceptedDaa": 13460, "workLeaves": 10, "executorBond": "03:0", "phase": "Pending"},
                                      {"acceptedDaa": 13470, "workLeaves": 50, "executorBond": "02:0", "phase": "Voided"},
                                      {"acceptedDaa": 13000, "workLeaves": 500, "executorBond": "03:0", "phase": "Final"}]}}

    readings = {"a": reading(sid, [14000]), "b": reading(sid, [14000]), "c": reading(other, [])}
    interop = [{"schema": "misaka/interop-record/v1", "operator": "A", "crossed_switch_daa": 14000, "checks": {"sync": True, "reorg": True, "ibd": True}},
               {"schema": "misaka/interop-record/v1", "operator": "B", "crossed_switch_daa": 14000, "checks": {"sync": True, "reorg": False, "ibd": True}}]
    r = measure(plan, readings, interop)
    assert r["M1_recent_palw_compute_upgraded"]["value"] == "90/100", r["M1_recent_palw_compute_upgraded"]
    assert r["M2_panel_capacity_upgraded"]["value"] == "1/2"
    assert r["M3_independent_interop"]["value"] == "1/2"
    assert r["M4_schedule_and_catalog_agreement"]["value"] == "2/3"
    assert r["M5_concentration"]["top_operator_permille"] == 1000 and r["M5_concentration"]["upgraded_operators"] == 1
    assert not any(r["operational_target_met"].values()), r["operational_target_met"]
    # All upgraded and all interop green: every target met — still no consensus effect.
    readings["c"] = reading(sid, [14000])
    interop[1]["checks"]["reorg"] = True
    r = measure(plan, readings, interop)
    assert all(r["operational_target_met"].values()), r
    assert "not a vote" in r["consensus_effect"]
    # A catalog switch: the root must be carried too.
    plan["expected"]["catalog_root"] = "cc" * 64
    for name in readings:
        readings[name]["registry"]["annualCatalog"] = {"editions": [{"edition": 0, "root": "cc" * 64}]}
    readings["c"]["registry"]["annualCatalog"] = {"editions": [{"edition": 0, "root": "dd" * 64}]}
    r = measure(plan, readings, interop)
    assert r["nodes"]["c"] == "old", r["nodes"]
    # Unreachable nodes are reported apart, never counted as agreeing.
    readings["b"] = {"error": "refused"}
    r = measure(plan, readings, interop)
    assert r["M4_schedule_and_catalog_agreement"]["unreachable"] == ["b"]
    print("self-test: ok (M1–M5, voided/old-window exclusion, catalog root, unreachable, no consensus effect)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--plan")
    parser.add_argument("--observations", help="node reads captured earlier ({name: reading}); no network access")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--require-target", action="store_true")
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.plan:
        parser.error("--plan is required")
    try:
        with open(args.plan) as f:
            plan = json.load(f)
        if args.observations:
            with open(args.observations) as f:
                readings = json.load(f)
        else:
            readings = {n["name"]: poll(n, args.timeout) for n in plan["nodes"]}
        interop = []
        for path in plan.get("interop_records", []):
            with open(path) as f:
                loaded = json.load(f)
            interop.extend(loaded if isinstance(loaded, list) else [loaded])
    except (OSError, ValueError, KeyError) as error:
        print(f"cannot read the inputs: {error}", file=sys.stderr)
        sys.exit(2)
    report = measure(plan, readings, interop)
    print(json.dumps(report, indent=2) if args.json else human(report))
    if args.require_target and not all(report["operational_target_met"].values()):
        sys.exit(3)


if __name__ == "__main__":
    main()
