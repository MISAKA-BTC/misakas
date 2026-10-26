#!/usr/bin/env python3
"""Summarize per-process court fixture results without pooling unavailable item timings."""

import argparse
import json
from pathlib import Path
import statistics


def fields(line):
    return dict(part.split("=", 1) for part in line.split() if "=" in part)


def middle(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("batch", type=Path)
    args = parser.parse_args()
    batch = json.loads((args.batch / "batch.json").read_text())
    rows = []
    for run in batch["operations"]:
        path = Path(run["command"][run["command"].index("--out") + 1])
        raw = json.loads((path / "raw.json").read_text())
        samples = [r for r in raw["runs"] if r.get("env", {}).get("MEASURE_N") != "0" and r["status"] == "PASS"]
        baseline = [r for r in raw["runs"] if r.get("env", {}).get("MEASURE_N") == "0" and r["status"] == "PASS"]
        metrics = [fields(r["measure"]) for r in samples]
        rss = middle([r["peak_rss_bytes"] for r in samples])
        base_rss = middle([r["peak_rss_bytes"] for r in baseline])
        rows.append({
            "operation": raw["operation"], "scale": raw["scale"], "status": raw["status"],
            "under_load": any(r.get("under_load", False) for r in raw["runs"]),
            "sample_processes": len(samples), "baseline_processes": len(baseline), "n_each": raw["n"],
            "median_of_process_medians_wall_ns": middle([int(m["median_wall_ns"]) for m in metrics]),
            "worst_process_p95_wall_ns": max((int(m["p95_wall_ns"]) for m in metrics), default=None),
            "worst_item_wall_ns": max((int(m["max_wall_ns"]) for m in metrics), default=None),
            "warmup_wall_ns": [int(m["warmup_wall_ns"]) for m in metrics],
            "exemplar_body_bytes": [int(m["exemplar_body_bytes"]) for m in metrics],
            "distinct": [int(m["distinct"]) for m in metrics],
            "median_process_peak_rss_bytes": rss,
            "median_setup_peak_rss_bytes": base_rss,
            "descriptive_rss_delta_bytes": rss - base_rss if rss is not None and base_rss is not None else None,
            "max_load_before_or_after": max((max(r[k]["load"][0] for k in ("before", "after"))
                                              for r in raw["runs"] if "after" in r), default=None),
        })
    summary = {"batch_status": batch["status"], "aggregation": "median of process medians; largest process p95; largest item maximum; independent-process RSS delta is descriptive only", "rows": rows}
    (args.batch / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({"batch_status": batch["status"], "rows": len(rows)}))


if __name__ == "__main__":
    main()
