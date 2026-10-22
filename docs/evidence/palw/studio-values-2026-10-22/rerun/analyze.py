#!/usr/bin/env python3
"""values/p1-measure (2026-10-22) — summarise the batch JSONs measure.py wrote.

    python3 docs/evidence/palw/studio-values-2026-10-22/rerun/analyze.py <out-dir>

Writes <out-dir>/summary.json and prints Markdown tables. Pure post-processing: no value is decided here.
Statistics: every per-op line already excludes its warm-up item (stats_exclude_warmup=true); across repeated
processes the table reports the median of the process medians, the max of the p95s and the max of the maxima.
Peak RSS is the process's (setup included); `rss_over_setup` subtracts the median N=0 process of the same op/scale.
"""
import json
import os
import re
import statistics
import sys


def kv(line):
    return dict(x.split("=", 1) for x in line.split() if "=" in x)


def pct(sorted_v, q):
    return sorted_v[round((len(sorted_v) - 1) * q)]


def court_verify(b):
    groups = {}
    for r in b["runs"]:
        if r["rc"] != 0:
            continue
        env = r["env"]
        key = (env["MEASURE_OP"], int(env["MEASURE_N"]), env.get("MEASURE_SCALE", ""))
        m = [kv(l) for l in r["lines"] if l.startswith("MEASURE ")]
        if not m:
            continue
        groups.setdefault(key, []).append((m[0], r))
    base = {}
    for (op, n, sc), v in groups.items():
        if n == 0:
            base[(op, sc)] = statistics.median([r["peak_rss_bytes"] for _, r in v])
    setup_only = base.get(("fixture_setup_only", ""))
    rows = []
    for (op, n, sc), v in sorted(groups.items()):
        if n == 0 or op == "fixture_setup_only":
            continue
        meds = [int(m["median_wall_ns"]) for m, _ in v]
        row = {
            "op": op, "n": n, "scale": sc, "processes": len(v),
            "warmup_wall_ns": [int(m["warmup_wall_ns"]) for m, _ in v],
            "median_wall_ns": int(statistics.median(meds)),
            "p95_wall_ns": max(int(m["p95_wall_ns"]) for m, _ in v),
            "p99_wall_ns": max(int(m["p99_wall_ns"]) for m, _ in v),
            "max_wall_ns": max(int(m["max_wall_ns"]) for m, _ in v),
            "distinct": int(v[0][0]["distinct"]),
            "exemplar_body_bytes": int(v[0][0]["exemplar_body_bytes"]),
            "peak_rss_bytes": max(r["peak_rss_bytes"] for _, r in v),
            "setup_rss_bytes": base.get((op, sc)),
            "under_load_processes": sum(1 for _, r in v if r.get("under_load")),
            "extra": {k: x for k, x in v[0][0].items() if k not in (
                "fixture_only", "clock", "stats_exclude_warmup", "op", "n", "distinct", "exemplar_body_bytes", "warmup_wall_ns",
                "median_wall_ns", "p95_wall_ns", "p99_wall_ns", "max_wall_ns", "total_wall_ns", "observed_expected_outcomes")},
        }
        if row["setup_rss_bytes"] is not None:
            row["rss_over_setup_bytes"] = row["peak_rss_bytes"] - row["setup_rss_bytes"]
        rows.append(row)
    return {"rows": rows, "setup_only_rss_bytes": setup_only}


def in_crate(b):
    rows = []
    for r in b["runs"]:
        for l in r["lines"]:
            if l.startswith("MEASURE ") or l.startswith("MEASURE_DA "):
                d = kv(l)
                d["kind"] = l.split()[0]
                rows.append(d)
    return {"rows": rows, "processes": [{"rc": r["rc"], "peak_rss_bytes": r["peak_rss_bytes"], "process_wall_s": r["process_wall_s"],
                                          "under_load": r.get("under_load"), "cmd": r["cmd"][1]} for r in b["runs"]]}


LABELS = ["empty_block_t12", "dispute_reserved_x42", "empty_block_opening", "one_move_opening_x1", "one_move_opening_x42",
          "default_sweep_42_sessions"]


def court_op(b):
    out = []
    r = b["runs"][0]
    i = 0
    for l in r["lines"]:
        m = re.search(r"raw objects=(\d+) block_fold_ns=\[([^\]]*)\]", l)
        if not m:
            continue
        v = [int(x) for x in m.group(2).split(",") if x.strip()]
        rest = sorted(v[1:])
        out.append({"label": LABELS[i] if i < len(LABELS) else f"call{i}", "objects": int(m.group(1)), "runs": len(v),
                    "warmup_ns": v[0], "median_ns": int(statistics.median(rest)), "p95_ns": pct(rest, 0.95), "max_ns": rest[-1],
                    "min_ns": rest[0]})
        i += 1
    other = [l for l in r["lines"] if "raw objects" not in l]
    return {"blocks": out, "lines": other, "peak_rss_bytes": r["peak_rss_bytes"], "rc": r["rc"]}


FRAME_KEYS = [("execute", "ns"), ("answer_cold", "ns"), ("answer_cold", "slice_max_ns"), ("answer_cold", "authenticate_ns"),
              ("answer_cold", "open_ns"), ("filing", "court_check_ns"), ("filing", "challenger_build_ns")]
FRAME_FIXED = [("execute", "capture_bytes"), ("answer_cold", "requests"), ("answer_cold", "slices"), ("answer_cold", "answer_bytes_max"),
               ("filing", "witness_bytes"), ("preparation", "retained_bytes"), ("preparation", "publish_slices"),
               ("geometry", "leaves")]


def frame(out_dir):
    d = os.path.join(out_dir, "frame")
    if not os.path.isdir(d):
        return {}
    recs = [json.load(open(os.path.join(d, f))) for f in sorted(os.listdir(d)) if f.endswith(".json")]
    classes = {}
    for k, rec in enumerate(recs):
        for c in rec["classes"]:
            classes.setdefault(c["class"], []).append((k, c))
    res = {}
    for name, v in classes.items():
        row = {"runs": len(v)}
        for a, b in FRAME_KEYS:
            vals = [c[a][b] for _, c in v]
            rest = sorted(vals[1:]) if len(vals) > 1 else vals
            row[f"{a}.{b}"] = {"warmup": vals[0], "median": int(statistics.median(rest)), "p95": pct(rest, 0.95), "max": rest[-1]}
        sc = [c["step_check"] for _, c in v]
        if sc and isinstance(sc[0], dict):
            for key in sc[0]:
                if isinstance(sc[0][key], (int, float)) and key.endswith("ns"):
                    vals = [s[key] for s in sc]
                    rest = sorted(vals[1:]) if len(vals) > 1 else vals
                    row[f"step_check.{key}"] = {"warmup": vals[0], "median": int(statistics.median(rest)), "p95": pct(rest, 0.95),
                                                "max": rest[-1]}
        for a, b in FRAME_FIXED:
            vals = {c[a][b] for _, c in v}
            row[f"{a}.{b}"] = sorted(vals) if len(vals) > 1 else vals.pop()
        row["frame_bytes_upper6"] = v[0][1]["frame"][0]["frame_bytes"]
        row["frame_bytes_upper257"] = v[0][1]["frame"][1]["frame_bytes"]
        row["door_daa_upper6"] = v[0][1]["plans"][0]["door_daa"]
        row["owner_turns_upper6"] = v[0][1]["plans"][0]["owner_turns"]
        res[name] = row
    return res


def main():
    out = sys.argv[1]
    summary = {}
    for name, fn in [("court_verify", court_verify), ("in_crate", in_crate), ("court_op_harness", court_op)]:
        p = os.path.join(out, f"{name}.json")
        if os.path.exists(p):
            b = json.load(open(p))
            summary[name] = fn(b)
            summary[name]["under_load"] = b["under_load"]
            summary[name]["load_before"] = b["load_before"]
            summary[name]["load_after"] = b["load_after"]
            summary["host"] = b["host"]
            summary["source_commit"] = b.get("source_commit")
    p = os.path.join(out, "frame_measure.json")
    if os.path.exists(p):
        b = json.load(open(p))
        summary["frame_measure"] = {"classes": frame(out), "under_load": b["under_load"], "load_before": b["load_before"],
                                    "load_after": b["load_after"], "failed": sum(1 for r in b["runs"] if r["rc"] != 0)}
    with open(os.path.join(out, "summary.json"), "w") as f:
        json.dump(summary, f, indent=1, sort_keys=True)
    cv = summary.get("court_verify")
    if cv:
        print("| op | N | scale | procs | median ns | p95 ns | max ns | warm-up ns | bytes | peak RSS MiB | over setup MiB | loaded |")
        print("|---|---|---|---|---|---|---|---|---|---|---|---|")
        for r in cv["rows"]:
            over = r.get("rss_over_setup_bytes")
            print(f"| {r['op']} | {r['n']} | {r['scale']} | {r['processes']} | {r['median_wall_ns']} | {r['p95_wall_ns']} | "
                  f"{r['max_wall_ns']} | {max(r['warmup_wall_ns'])} | {r['exemplar_body_bytes']} | {r['peak_rss_bytes']/2**20:.1f} | "
                  f"{'' if over is None else f'{over/2**20:.1f}'} | {r['under_load_processes']}/{r['processes']} |")


if __name__ == "__main__":
    main()
