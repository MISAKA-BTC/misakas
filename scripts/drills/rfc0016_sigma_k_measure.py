#!/usr/bin/env python3
"""rfc0016_sigma_k_measure.py — RFC-0016 §8.6: the real-node σ / k measurement harness (devnet on an isolated calibration preset).

What it does (and what it does not):

  plan       Build the run matrix: every (σ, k, φ, pace margin) point of the sweep × every scenario (E1–E6, the adversarial set
             A1–A12 where a node run can express it). For each point it prints the BUILD value string
             (MISAKA_RFC0016_CALIBRATION_VALUES — 18 integers, the order of PalwRfc0016ValuesV1) every node of that run MUST be built
             with (the values are not in the params fingerprint: nodes of one run must share one build), and a scenario manifest.
  run        On the devnet host: start the scenario's miners at their DAA marks through the operator's launch command (this harness
             does not start models itself), poll `getPalwRfc0016Report` (RPC op 255) every --poll-secs, and record every report
             (events.jsonl). Stops the miners at their marks. Nothing is decided here.
  summarize  From events.jsonl: per model the PHYSICAL blocks, the actual payout, the per-existing-miner revenue, the unused (unminted)
             rights, queue wait and the heartbeat-dependent DAA — outcomes only; targets are listed apart — and the pass conditions
             of RFC-0016 §8.6 with the bounds given on the command line (no default bound: a bound is a decision).
  select     Over the summaries of every run: the region where EVERY scenario passes every condition. Where none does, it prints the
             conflicting constraints and exits non-zero. It never proposes a value outside the measured points and never invents one.

Prerequisites the operator owns (see docs/design/palw/rfc-0016-implementation-status-2026-10-15.md §7):
  * an isolated calibration network (Params::has_isolated_native_calibration_v34) with palw_probabilistic_constraints_v1,
    palw_bond_budget_v1 and — for V+ — palw_model_bond_allocation_v1 armed (the last is the ADR-0177 lane's to make armable in
    calibration; without it ΣA = 0 and V+ issues no right, which is scenario H1, not a σ/k measurement);
  * the reference model: Qwen2.5-1.5B A16, fixed profile, reference hardware, minimum producer bond; plus at least one model of a
    different CCU;
  * a launch command template that starts / stops one miner: --launch-cmd "… {miner} {bond} {model} {action}".

Usage (python -I):
  rfc0016_sigma_k_measure.py plan [--sigmas 0,100,250,500,1000,2000] [--ks 1000] [--phis 250,500,750] [--paces 1000,1200,1500] \
      [--base-values "<18 ints>"  (default: the 2026-10-15 calibration starting set)] --scenarios E1,E2,E3,E4,E5,E6 --out plan.json
  rfc0016_sigma_k_measure.py run --manifest run.json --duration-daa 4320 --out DIR [--port 26314] [--poll-secs 30]
      --launch-cmd "scripts/ops/start-miner.sh {miner} {bond} {model} {action}"
  rfc0016_sigma_k_measure.py summarize --events DIR/events.jsonl --manifest run.json --track-tol-ppm N \
      --unminted-max-permille N --hb-max-permille N --surge-min-permille N --out DIR/summary.json
  rfc0016_sigma_k_measure.py select --summaries DIR1/summary.json DIR2/summary.json ...
  rfc0016_sigma_k_measure.py --self-test
"""
import argparse
import importlib.util
import itertools
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

FIELDS = [
    "sigma_permille", "tilt_k_permille", "reissue_checkpoint_permille", "pace_margin_permille", "entrant_reserve_permille",
    "entrant_max_per_model_permille", "entrant_min_capital_sompi", "entrant_age_daa", "capacity_safety_permille",
    "c7_measured_concurrency", "queue_len_per_class", "queue_ttl_daa", "quote_margin_h", "verifier_share_base_permille",
    "verifier_slope_gamma_permille", "verifier_util_target_permille", "opv_checks_per_claim_permille",
    "opv_checks_per_verifier_epoch",
]

# User decisions 2026-10-15 (RFC-0016 §14.1): the CALIBRATION STARTING SET — mirrors `PALW_RFC0016_CALIBRATION_START_V1`
# (consensus/core/src/palw_rfc0016_v1/values.rs). V2: σ = 0.25, φ = 0.5, pace margin 1.2, k = 1 (the tilt stays off). The terms the
# user left to measurement (queue length / TTL, h, γ, u*, OPV checks) are PROBES the sweep moves. Never production values.
CALIBRATION_START = {
    "sigma_permille": 250, "tilt_k_permille": 1000, "reissue_checkpoint_permille": 500, "pace_margin_permille": 1200,
    "entrant_reserve_permille": 20, "entrant_max_per_model_permille": 2, "entrant_min_capital_sompi": 0, "entrant_age_daa": 21600,
    "capacity_safety_permille": 1000, "c7_measured_concurrency": 0, "queue_len_per_class": 8, "queue_ttl_daa": 720,
    "quote_margin_h": 2, "verifier_share_base_permille": 200, "verifier_slope_gamma_permille": 100,
    "verifier_util_target_permille": 700, "opv_checks_per_claim_permille": 500, "opv_checks_per_verifier_epoch": 10,
}
# V2: the sweep still measures every §8.6 candidate; k is pinned to 1 (a calibration build refuses another k).
DEFAULT_SWEEP = {"sigmas": "0,100,250,500,1000,2000", "ks": "1000", "phis": "250,500,750", "paces": "1000,1200,1500"}

# Scenario templates: which miners run where, in relative terms. `ref` is the reference model (Qwen2.5-1.5B A16), `alt` a model of a
# different CCU. `capital` is the bond weight of the model's miners (relative), `compute` the miners' count (each at the reference
# hardware), `mobile` whether those miners may be switched by the operator at `switch_at` (a fraction of the run), `surge_at` a fraction
# of the run at which `surge_x` × miners join.
SCENARIOS = {
    "E1": {"note": "miners cannot switch models", "models": {"ref": {"capital": 8, "compute": 1, "mobile": False},
                                                             "alt": {"capital": 2, "compute": 4, "mobile": False}}},
    "E2": {"note": "demand surge on a low-bond model", "models": {"ref": {"capital": 8, "compute": 2, "mobile": True},
                                                                  "alt": {"capital": 2, "compute": 2, "mobile": True,
                                                                          "surge_at": 0.3, "surge_x": 3}}},
    "E3": {"note": "much capital, little compute", "models": {"ref": {"capital": 8, "compute": 1, "mobile": True},
                                                              "alt": {"capital": 2, "compute": 4, "mobile": True}}},
    "E4": {"note": "much compute, little capital", "models": {"ref": {"capital": 2, "compute": 4, "mobile": True},
                                                              "alt": {"capital": 8, "compute": 1, "mobile": True}}},
    "E5": {"note": "unpaid griefing wins", "models": {"ref": {"capital": 8, "compute": 2, "mobile": True},
                                                      "alt": {"capital": 0, "compute": 4, "mobile": False, "griefer": True}}},
    "E6": {"note": "W0 regime (supply below the cadence)", "models": {"ref": {"capital": 8, "compute": 1, "mobile": True},
                                                                      "alt": {"capital": 2, "compute": 1, "mobile": True}},
           "low_supply": True},
    # The adversarial set as node runs (the rest are unit / E2E sources; see the status doc's test map).
    "A1": {"note": "capital moved across the 30-day average edge", "models": {"ref": {"capital": 5, "compute": 2, "mobile": True},
                                                                              "alt": {"capital": 5, "compute": 2, "mobile": True,
                                                                                      "capital_shift_at": 0.5}}},
    "A5": {"note": "queue squatting by one bond", "models": {"ref": {"capital": 8, "compute": 4, "mobile": False,
                                                                     "one_bond": True}}},
    "A7": {"note": "W-work griefing", "models": {"ref": {"capital": 8, "compute": 2, "mobile": True},
                                                 "alt": {"capital": 0, "compute": 8, "mobile": False, "griefer": True}}},
    "A10": {"note": "post-quote race (parallel winners)", "models": {"ref": {"capital": 8, "compute": 6, "mobile": False}}},
}


def values_string(values):
    return ",".join(str(values[f]) for f in FIELDS)


def parse_values(text):
    parts = [int(x) for x in text.split(",")]
    if len(parts) != len(FIELDS):
        raise SystemExit(f"--base-values needs {len(FIELDS)} integers (the order of PalwRfc0016ValuesV1)")
    return dict(zip(FIELDS, parts))


def cmd_plan(a):
    base = parse_values(a.base_values) if a.base_values else dict(CALIBRATION_START)
    if any(int(k) != 1000 for k in a.ks.split(",")):
        raise SystemExit("RFC-0016 V2 (2026-10-15): k = 1 — the auxiliary tilt is not wired; a calibration build refuses another k")
    runs = []
    for s, k, phi, pace in itertools.product(*(map(int, x.split(",")) for x in (a.sigmas, a.ks, a.phis, a.paces))):
        v = dict(base, sigma_permille=s, tilt_k_permille=k, reissue_checkpoint_permille=phi, pace_margin_permille=pace)
        for name in a.scenarios.split(","):
            runs.append({"build_env": {"MISAKA_RFC0016_CALIBRATION_VALUES": values_string(v)}, "values": v,
                         "scenario": name, "template": SCENARIOS[name]})
    Path(a.out).write_text(json.dumps({"runs": runs, "note": "every node of one run is built with its build_env"}, indent=1))
    print(f"{len(runs)} runs written to {a.out}")


def load_client():
    here = Path(__file__).resolve().parent / "misaka_wrpc_json.py"
    spec = importlib.util.spec_from_file_location("misaka_wrpc_json", here)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.WsRpc


def launch(template, miner, action):
    cmd = template.format(miner=miner["name"], bond=miner["bond"], model=miner["model"], action=action)
    subprocess.run(shlex.split(cmd), check=False)


def cmd_run(a):
    manifest = json.loads(Path(a.manifest).read_text())
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    events = (out / "events.jsonl").open("a")
    WsRpc = load_client()
    c = WsRpc(port=a.port)
    started, stopped = set(), set()
    start_daa = None
    while True:
        info = c.call("getBlockDagInfo") or {}
        daa = int(info.get("virtualDaaScore", 0))
        start_daa = start_daa if start_daa is not None else daa
        for m in manifest["miners"]:
            if m["name"] not in started and daa >= start_daa + int(m.get("start_daa", 0)):
                launch(a.launch_cmd, m, "start")
                started.add(m["name"])
            if m.get("stop_daa") is not None and m["name"] not in stopped and daa >= start_daa + int(m["stop_daa"]):
                launch(a.launch_cmd, m, "stop")
                stopped.add(m["name"])
        report = c.call("getPalwRfc0016Report", {"classHex": manifest.get("quote_class", "")}) or {}
        events.write(json.dumps({"wall": time.time(), "daa": daa, "report": report}) + "\n")
        events.flush()
        if daa >= start_daa + a.duration_daa:
            break
        time.sleep(a.poll_secs)
    for m in manifest["miners"]:
        if m["name"] not in stopped:
            launch(a.launch_cmd, m, "stop")


# V5 (user decision 2026-10-15): one calibration epoch = 720 DAA; the 30-day window = 21,600 DAA. An epoch is a DAA count — its
# wall-clock span is MEASURED here (DAA vs wall clock, and DAA vs the chain's own timestamps), never labelled "24 h".
EPOCH_DAA = 720
AVERAGE_WINDOW_DAA = 30 * EPOCH_DAA
NOMINAL_MS_PER_DAA = 120_000


def epoch_clock(lines):
    """Pure: DAA vs wall-clock drift over the recorded polls (and vs the sink timestamps the report carries, where present)."""
    if len(lines) < 2 or lines[-1]["daa"] <= lines[0]["daa"] or "wall" not in lines[0] or "wall" not in lines[-1]:
        return {"epoch_daa": EPOCH_DAA, "note": "too few timed polls to measure the epoch's wall-clock span"}
    ddaa = lines[-1]["daa"] - lines[0]["daa"]
    wall_ms_per_daa = (lines[-1]["wall"] - lines[0]["wall"]) * 1000.0 / ddaa
    out = {
        "epoch_daa": EPOCH_DAA,
        "average_window_daa": AVERAGE_WINDOW_DAA,
        "nominal_ms_per_daa": NOMINAL_MS_PER_DAA,
        "observed_wall_ms_per_daa": round(wall_ms_per_daa, 1),
        "observed_epoch_wall_hours": round(EPOCH_DAA * wall_ms_per_daa / 3_600_000.0, 3),
        "drift_permille_vs_nominal": round((wall_ms_per_daa - NOMINAL_MS_PER_DAA) * 1000.0 / NOMINAL_MS_PER_DAA, 1),
        "label": f"epoch = {EPOCH_DAA} DAA (wall-clock span measured; never assumed)",
    }

    def sink(line):
        try:
            clock = json.loads(line["report"].get("reportJson") or "{}").get("epochClock") or {}
            value = clock.get("sinkTimestampMs")
            return int(value) if value not in (None, "null") else None
        except (AttributeError, KeyError, ValueError, TypeError):
            return None

    t0, t1 = sink(lines[0]), sink(lines[-1])
    if t0 is not None and t1 is not None:
        out["observed_chain_ms_per_daa"] = round((t1 - t0) / ddaa, 1)
    return out


def summarize(lines, manifest, bounds):
    """Pure: outcomes per model from the last report and the pass conditions (also what --self-test checks)."""
    reports = [json.loads(l["report"].get("reportJson") or "{}") if isinstance(l.get("report"), dict) else {} for l in lines]
    reports = [r for r in reports if r.get("models")]
    if not reports:
        return {"error": "no armed report recorded (was the calibration fence armed?)"}
    first, last = reports[0], reports[-1]
    by = lambda r: {m["model"]: m for m in r["models"]}
    f, l = by(first), by(last)
    models = {}
    for mid, row in l.items():
        o = row["outcomes"]
        o0 = f.get(mid, {}).get("outcomes", {})
        num = lambda d, k: int(d.get(k) or 0)
        models[mid] = {
            "target (not an outcome)": row["targets"],
            "physical_blocks": num(o, "physicalBlocks") - num(o0, "physicalBlocks"),
            "payout_coinbase_sompi": num(o, "payoutCoinbaseSompi") - num(o0, "payoutCoinbaseSompi"),
            "payout_escrow_sompi": num(o, "payoutEscrowSompi") - num(o0, "payoutEscrowSompi"),
            "rights_unused": num(o, "rightsUnused"),
            "rights_issued": int(row["targets"].get("rightsIssued") or 0),
            "queue_wait_max_daa": o.get("queueWaitMaxDaa"),
            "heartbeat_dependent_daa": o.get("heartbeatDependentDaa"),
            "node_local (not consensus)": row.get("nodeLocal"),
        }
    tot = sum(m["physical_blocks"] for m in models.values()) or 1
    fails = []
    for mid, m in models.items():
        share_ppm = 1_000_000 * m["physical_blocks"] // tot
        target = int(m["target (not an outcome)"].get("pPpm") or 0)
        role = manifest.get("roles", {}).get(mid, {})
        if role.get("bond_heavy_with_compute") and abs(share_ppm - target) > bounds["track_tol_ppm"]:
            fails.append(f"{mid}: physical share {share_ppm} ppm vs target {target} ppm")
        if role.get("surging") and m["rights_issued"] and m["physical_blocks"] * 1000 < m["rights_issued"] * bounds["surge_min_permille"]:
            fails.append(f"{mid}: surging model stalled ({m['physical_blocks']} of {m['rights_issued']} rights)")
    issued = sum(m["rights_issued"] for m in models.values())
    unused = sum(m["rights_unused"] for m in models.values())
    if issued and unused * 1000 > issued * bounds["unminted_max_permille"]:
        fails.append(f"unminted rights {unused} of {issued}")
    hb = max((int(m["heartbeat_dependent_daa"] or 0) for m in models.values()), default=0)
    span = max(1, (lines[-1]["daa"] - lines[0]["daa"]))
    if hb * 1000 > span * bounds["hb_max_permille"]:
        fails.append(f"heartbeat-dependent {hb} of {span} DAA")
    return {"models": models, "fails": fails, "values": manifest.get("values"), "scenario": manifest.get("scenario"),
            "epoch_clock (V5: measured, not assumed)": epoch_clock(lines)}


def cmd_summarize(a):
    lines = [json.loads(x) for x in Path(a.events).read_text().splitlines() if x.strip()]
    manifest = json.loads(Path(a.manifest).read_text())
    bounds = {"track_tol_ppm": a.track_tol_ppm, "unminted_max_permille": a.unminted_max_permille,
              "hb_max_permille": a.hb_max_permille, "surge_min_permille": a.surge_min_permille}
    s = summarize(lines, manifest, bounds)
    s["bounds"] = bounds
    Path(a.out).write_text(json.dumps(s, indent=1))
    print(json.dumps({"fails": s.get("fails"), "error": s.get("error")}, indent=1))


def cmd_select(a):
    points = {}
    for p in a.summaries:
        s = json.loads(Path(p).read_text())
        key = json.dumps(s.get("values"), sort_keys=True)
        points.setdefault(key, []).append(s)
    region, conflicts = [], {}
    for key, runs in points.items():
        fails = [f"{r.get('scenario')}: {x}" for r in runs for x in (r.get("fails") or [])] + \
                [f"{r.get('scenario')}: {r['error']}" for r in runs if r.get("error")]
        (conflicts.__setitem__(key, fails) if fails else region.append(json.loads(key)))
    print(json.dumps({"passing_region": region, "conflicts": conflicts}, indent=1))
    if not region:
        print("NO measured point satisfies every pass condition: report the conflicts; do not invent values.", file=sys.stderr)
        sys.exit(1)


def self_test():
    v = dict(zip(FIELDS, range(18)))
    assert parse_values(values_string(v)) == v
    assert set(CALIBRATION_START) == set(FIELDS)
    assert (CALIBRATION_START["sigma_permille"], CALIBRATION_START["reissue_checkpoint_permille"],
            CALIBRATION_START["pace_margin_permille"], CALIBRATION_START["tilt_k_permille"]) == (250, 500, 1200, 1000)
    lines = [
        {"daa": 0, "report": {"reportJson": json.dumps({"models": [
            {"model": "A", "targets": {"pPpm": "800000", "rightsIssued": "100"}, "outcomes": {"physicalBlocks": "0"}}]})}},
        {"daa": 100, "report": {"reportJson": json.dumps({"models": [
            {"model": "A", "targets": {"pPpm": "800000", "rightsIssued": "100"},
             "outcomes": {"physicalBlocks": "80", "rightsUnused": "20", "heartbeatDependentDaa": "5"}}]})}},
    ]
    s = summarize(lines, {"roles": {"A": {"bond_heavy_with_compute": True}}},
                  {"track_tol_ppm": 300_000, "unminted_max_permille": 100, "hb_max_permille": 100, "surge_min_permille": 0})
    assert s["models"]["A"]["physical_blocks"] == 80
    clock = epoch_clock([{"daa": 0, "wall": 0.0, "report": {}}, {"daa": 720, "wall": 720 * 150.0, "report": {}}])
    assert clock["observed_epoch_wall_hours"] == 30.0 and clock["drift_permille_vs_nominal"] == 250.0, clock
    assert "24 h" not in clock["label"]
    assert any("unminted" in f for f in s["fails"]), s["fails"]
    print("self-test: plan/summarize logic holds (no node contacted, no value decided)")


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
        return
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    pl = sub.add_parser("plan")
    for name in ("sigmas", "ks", "phis", "paces"):
        pl.add_argument("--" + name, dest=name, default=DEFAULT_SWEEP[name])
    pl.add_argument("--base-values", dest="base_values", default=None, help="18 ints; default: the calibration starting set")
    for name in ("scenarios", "out"):
        pl.add_argument("--" + name, dest=name, required=True)
    r = sub.add_parser("run")
    r.add_argument("--manifest", required=True)
    r.add_argument("--duration-daa", type=int, required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--port", type=int, default=26314)
    r.add_argument("--poll-secs", type=float, default=30.0)
    r.add_argument("--launch-cmd", required=True)
    s = sub.add_parser("summarize")
    s.add_argument("--events", required=True)
    s.add_argument("--manifest", required=True)
    s.add_argument("--out", required=True)
    for b in ("track_tol_ppm", "unminted_max_permille", "hb_max_permille", "surge_min_permille"):
        s.add_argument("--" + b.replace("_", "-"), dest=b, type=int, required=True)
    sel = sub.add_parser("select")
    sel.add_argument("--summaries", nargs="+", required=True)
    a = p.parse_args()
    {"plan": cmd_plan, "run": cmd_run, "summarize": cmd_summarize, "select": cmd_select}[a.cmd](a)


if __name__ == "__main__":
    main()
