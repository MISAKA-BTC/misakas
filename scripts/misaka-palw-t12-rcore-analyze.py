#!/usr/bin/env python3
"""misaka-palw-t12-rcore-analyze.py — the T10 analyzer (ADR-0152 §8.4 O-1 and O-2; phase2-plan.md §4, T10 step 11).

Three reports over a drill's (or public testnet-12's) snapshots, which
`scripts/misaka-palw-t12-rcore-drill.sh snapshot` appends to WORK_DIR/snapshots.jsonl — one JSON object a
line: {"format": "misaka-palw-drill-snapshot/v1", "wall_ms", "host", "dag": misaka node dag-info,
"vesting": getPalwVesting (P2-10) through `misaka palw vesting --json` (P2-11), or null}:

1. **The door and withheld histogram (p)** — `getPalwVesting.licence_histogram`, door → (released, held,
   withheld_seen), from the latest snapshot that carries one: each door's count and share, and
   p = withheld_seen / (released + held), the withheld share the ADR says T10 must measure (O-2).
2. **Measured seconds per DAA** — the wall clock against the virtual DAA across snapshots: the fitted slope
   and the per-interval spread, beside the node's own `measured_secs_per_daa` where it reports one.
3. **Licence cadence** — cumulative licences (the histogram's released + held) against the DAA: licences per
   1,000 DAA, and O-1's live criterion, ≥ 30 licences within 3,000 DAA of the first Final (the first
   snapshot whose `totals.vesting_created` is positive): PASS, FAIL or PENDING.

The exit status is the drill's verdict, which `misaka-palw-t12-rcore-drill.sh report` refuses to pass on
anything but 0: 0 every report available and O-1 PASS; 3 INCOMPLETE (a report not available — e.g. a build
without getPalwVesting — or O-1 still PENDING); 1 O-1 FAIL. An absent report is never a pass.

BUILT BEFORE THE LAUNCH, RUN AFTER IT: nothing here has read a live host. `--self-test` runs the three
reports over a synthetic snapshot set and checks their arithmetic; that is the only execution it has had.

The wire shape of getPalwVesting is P2-10's and may land in snake_case (RPC model) or camelCase (JSON RPC);
every field is read under both spellings, and a histogram entry may be an object or a [released, held,
withheld_seen] triple. A field a snapshot does not carry is reported as absent, never as zero.
"""
import argparse
import json
import os
import statistics
import sys
import tempfile

O1_LICENCES = 30
O1_WINDOW_DAA = 3_000


def pick(d, *names, default=None):
    """The first of `names` present in dict `d` (snake_case and camelCase spellings of one field)."""
    if not isinstance(d, dict):
        return default
    for name in names:
        if name in d:
            return d[name]
    return default


def vesting_of(snapshot):
    """The getPalwVesting body of a snapshot, whatever envelope the CLI wraps it in."""
    v = snapshot.get("vesting")
    if isinstance(v, dict):
        for key in ("vesting", "response", "result"):
            if isinstance(v.get(key), dict):
                return v[key]
    return v if isinstance(v, dict) else None


def histogram_of(vesting):
    """door -> (released, held, withheld_seen), or None when the snapshot carries no histogram."""
    raw = pick(vesting, "licence_histogram", "licenceHistogram", "license_histogram", "licenseHistogram")
    if raw is None:
        return None
    rows = raw.items() if isinstance(raw, dict) else ((pick(r, "door"), r) for r in raw)
    out = {}
    for door, entry in rows:
        if isinstance(entry, (list, tuple)) and len(entry) >= 3:
            released, held, withheld = entry[0], entry[1], entry[2]
        else:
            released = pick(entry, "released", default=0)
            held = pick(entry, "held", default=0)
            withheld = pick(entry, "withheld_seen", "withheldSeen", default=0)
        out[str(door)] = (int(released), int(held), int(withheld))
    return out


def virtual_daa_of(snapshot):
    daa = pick(snapshot.get("dag"), "virtual_daa", "virtualDaa", "virtual_daa_score", "virtualDaaScore")
    if daa is None:
        daa = pick(vesting_of(snapshot), "tip_daa", "tipDaa")
    return int(daa) if daa is not None else None


def load(work_dir):
    path = os.path.join(work_dir, "snapshots.jsonl")
    rows = []
    with open(path) as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as e:
                print(f"snapshots.jsonl line {n}: skipped, not JSON ({e})", file=sys.stderr)
    rows.sort(key=lambda r: r.get("wall_ms", 0))
    return rows


def report_histogram(rows):
    latest = next((h for h in (histogram_of(vesting_of(r)) for r in reversed(rows)) if h is not None), None)
    if latest is None:
        return {"available": False, "why": "no snapshot carries getPalwVesting.licence_histogram"}
    total = sum(r + h for r, h, _ in latest.values())
    doors = {}
    for door, (released, held, withheld) in sorted(latest.items()):
        n = released + held
        doors[door] = {
            "released": released,
            "held": held,
            "withheld_seen": withheld,
            "share": (n / total) if total else None,
            "p_withheld": (withheld / n) if n else None,
        }
    withheld_all = sum(w for _, _, w in latest.values())
    return {"available": True, "licences": total, "doors": doors, "p_withheld": (withheld_all / total) if total else None}


def report_secs_per_daa(rows):
    points = [(r["wall_ms"] / 1000.0, virtual_daa_of(r)) for r in rows if r.get("wall_ms") is not None]
    points = [(t, d) for t, d in points if d is not None]
    reported = [pick(vesting_of(r), "measured_secs_per_daa", "measuredSecsPerDaa") for r in rows]
    reported = [float(x) for x in reported if x is not None]
    out = {"points": len(points), "node_reported_latest": reported[-1] if reported else None}
    if len(points) < 2 or points[-1][1] == points[0][1]:
        out.update(available=False, why="fewer than two snapshots at different DAA")
        return out
    # Least-squares slope of wall seconds on DAA: seconds per DAA over the whole run.
    ts, ds = [p[0] for p in points], [p[1] for p in points]
    mean_t, mean_d = statistics.fmean(ts), statistics.fmean(ds)
    var_d = sum((d - mean_d) ** 2 for d in ds)
    slope = sum((d - mean_d) * (t - mean_t) for t, d in points) / var_d if var_d else None
    intervals = [(t1 - t0) / (d1 - d0) for (t0, d0), (t1, d1) in zip(points, points[1:]) if d1 > d0]
    out.update(
        available=True,
        daa_span=(ds[0], ds[-1]),
        secs_per_daa_fit=slope,
        secs_per_daa_overall=(ts[-1] - ts[0]) / (ds[-1] - ds[0]),
        interval_min=min(intervals) if intervals else None,
        interval_median=statistics.median(intervals) if intervals else None,
        interval_max=max(intervals) if intervals else None,
    )
    return out


def licences_at(series, daa):
    """Cumulative licences at `daa`, linearly interpolated between snapshots; None outside the series."""
    if not series or daa < series[0][0] or daa > series[-1][0]:
        return None
    for (d0, n0), (d1, n1) in zip(series, series[1:]):
        if d0 <= daa <= d1:
            return n0 if d1 == d0 else n0 + (n1 - n0) * (daa - d0) / (d1 - d0)
    return series[-1][1]


def report_cadence(rows):
    series, first_final = [], None
    for r in rows:
        daa, v = virtual_daa_of(r), vesting_of(r)
        hist = histogram_of(v)
        if daa is None or hist is None:
            continue
        series.append((daa, sum(rel + held for rel, held, _ in hist.values())))
        created = pick(pick(v, "totals", default={}), "vesting_created", "vestingCreated")
        if first_final is None and created is not None and int(created) > 0:
            first_final = daa
    series.sort()
    out = {"points": len(series), "first_final_daa": first_final}
    if len(series) < 2:
        out.update(available=False, why="fewer than two snapshots carrying the licence histogram")
        return out
    span = series[-1][0] - series[0][0]
    out.update(
        available=True,
        licences=series[-1][1],
        per_1000_daa=(1000 * (series[-1][1] - series[0][1]) / span) if span else None,
    )
    if first_final is None:
        out["o1"] = "PENDING (no Final yet: totals.vesting_created is 0 in every snapshot)"
        return out
    start = licences_at(series, first_final)
    end_daa = first_final + O1_WINDOW_DAA
    end = licences_at(series, end_daa)
    if end is None:
        so_far = series[-1][1] - (start or 0)
        verdict = "PASS" if so_far >= O1_LICENCES else "PENDING"
        out["o1"] = f"{verdict} ({so_far:.0f} licences since the first Final at DAA {first_final}; the window closes at DAA {end_daa})"
    else:
        n = end - (start or 0)
        out["o1"] = f"{'PASS' if n >= O1_LICENCES else 'FAIL'} ({n:.0f} licences in the {O1_WINDOW_DAA} DAA after the first Final at DAA {first_final}; need {O1_LICENCES})"
    return out


def fmt(x, digits=1):
    return "absent" if x is None else (f"{x:.{digits}f}" if isinstance(x, float) else str(x))


def print_reports(reports):
    h, s, c = reports["histogram"], reports["secs_per_daa"], reports["cadence"]
    print("== 1. door and withheld histogram (getPalwVesting.licence_histogram; O-2's p) ==")
    if not h["available"]:
        print(f"   not available: {h['why']}")
    else:
        print(f"   {h['licences']} licences; p (withheld seen / licences) = {fmt(h['p_withheld'], 4)}")
        for door, row in h["doors"].items():
            print(
                f"   {door:<24} released {row['released']:>6}  held {row['held']:>6}  withheld seen {row['withheld_seen']:>6}"
                f"  share {fmt(row['share'], 3)}  p {fmt(row['p_withheld'], 4)}"
            )
    print("== 2. measured seconds per DAA ==")
    if not s["available"]:
        print(f"   not available: {s['why']} ({s['points']} usable snapshot(s))")
    else:
        lo, hi = s["daa_span"]
        print(f"   DAA {lo} → {hi} over {s['points']} snapshots: fit {fmt(s['secs_per_daa_fit'])} s/DAA, overall {fmt(s['secs_per_daa_overall'])} s/DAA")
        print(f"   per interval: min {fmt(s['interval_min'])}, median {fmt(s['interval_median'])}, max {fmt(s['interval_max'])} s/DAA")
    print(f"   the node's own measured_secs_per_daa (latest): {fmt(s['node_reported_latest'])}")
    print("== 3. licence cadence ==")
    if not c["available"]:
        print(f"   not available: {c['why']}")
    else:
        print(f"   {c['licences']} licences; {fmt(c['per_1000_daa'], 2)} per 1,000 DAA; first Final at DAA {fmt(c['first_final_daa'])}")
        print(f"   O-1 (≥ {O1_LICENCES} licences within {O1_WINDOW_DAA} DAA of the first Final): {c['o1']}")


def analyze(work_dir):
    rows = load(work_dir)
    return {"snapshots": len(rows), "histogram": report_histogram(rows), "secs_per_daa": report_secs_per_daa(rows), "cadence": report_cadence(rows)}


EXIT_PASS, EXIT_FAIL, EXIT_INCOMPLETE = 0, 1, 3


def verdict(reports):
    """(exit status, why): INCOMPLETE while any report is unavailable or O-1 is PENDING, FAIL on O-1 FAIL."""
    missing = [name for name in ("histogram", "secs_per_daa", "cadence") if not reports[name]["available"]]
    if missing:
        return EXIT_INCOMPLETE, "INCOMPLETE: not available: " + ", ".join(missing)
    o1 = reports["cadence"].get("o1", "PENDING (no verdict)")
    if o1.startswith("FAIL"):
        return EXIT_FAIL, "FAIL: O-1 " + o1
    if not o1.startswith("PASS"):
        return EXIT_INCOMPLETE, "INCOMPLETE: O-1 " + o1
    return EXIT_PASS, "PASS: O-1 " + o1


def self_test():
    """The three reports over a synthetic run whose answers are known: 200 s/DAA, one licence every 50 DAA
    from DAA 100, the first Final at DAA 300, two doors, a camelCase histogram in one snapshot."""
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "snapshots.jsonl"), "w") as f:
            for i, daa in enumerate(range(100, 3_800, 100)):
                licences = (daa - 100) // 50
                hist = {"Quorum": {"released": licences - licences // 4, "held": 0, "withheld_seen": licences // 10},
                        "Coverage": [licences // 4, 0, 0]}
                vesting = {"tip_daa": daa, "licence_histogram": hist, "totals": {"vesting_created": 1 if daa >= 300 else 0},
                           "measured_secs_per_daa": 200.0}
                if i == 5:
                    vesting = {"tipDaa": daa, "licenceHistogram": hist, "totals": {"vestingCreated": 1}, "measuredSecsPerDaa": 200.0}
                row = {"format": "misaka-palw-drill-snapshot/v1", "wall_ms": 1_000_000 + daa * 200_000, "host": "t",
                       "dag": {"virtual_daa": daa}, "vesting": vesting}
                f.write(json.dumps(row) + "\n")
            f.write("not json\n")
        r = analyze(d)
    assert r["histogram"]["available"] and r["histogram"]["licences"] == 72, r["histogram"]
    assert set(r["histogram"]["doors"]) == {"Quorum", "Coverage"}
    assert abs(r["secs_per_daa"]["secs_per_daa_fit"] - 200.0) < 1e-6, r["secs_per_daa"]
    assert abs(r["secs_per_daa"]["interval_median"] - 200.0) < 1e-6
    assert r["cadence"]["first_final_daa"] == 300, r["cadence"]
    assert abs(r["cadence"]["per_1000_daa"] - 20.0) < 1e-6, r["cadence"]
    assert r["cadence"]["o1"].startswith("PASS (60 licences"), r["cadence"]["o1"]
    assert verdict(r)[0] == EXIT_PASS, verdict(r)
    # A run whose snapshots carry no getPalwVesting (a build without P2-10) is INCOMPLETE, never a pass.
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "snapshots.jsonl"), "w") as f:
            for daa in (100, 200):
                f.write(json.dumps({"wall_ms": daa * 200_000, "dag": {"virtual_daa": daa}, "vesting": None}) + "\n")
        bare = analyze(d)
    assert verdict(bare)[0] == EXIT_INCOMPLETE and "histogram" in verdict(bare)[1], verdict(bare)
    assert bare["secs_per_daa"]["available"], "the clock report needs no vesting"
    print("self-test: the three reports reproduce a synthetic run's known answers; a run without vesting is INCOMPLETE")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("work_dir", nargs="?", help="the drill's WORK_DIR (holds snapshots.jsonl)")
    parser.add_argument("--json", action="store_true", help="print the reports as JSON")
    parser.add_argument("--self-test", action="store_true", help="check the arithmetic on synthetic data and exit")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.work_dir:
        parser.error("work_dir is required")
    reports = analyze(args.work_dir)
    code, why = verdict(reports)
    if args.json:
        reports["verdict"] = why
        print(json.dumps(reports, indent=2, sort_keys=True, default=str))
    else:
        print(f"{reports['snapshots']} snapshot(s) in {args.work_dir}")
        print_reports(reports)
        print(f"== verdict: {why}")
    sys.exit(code)


if __name__ == "__main__":
    main()
