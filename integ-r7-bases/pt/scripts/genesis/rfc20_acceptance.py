#!/usr/bin/env python3
"""RFC-0020 audit 20-A: the long-context acceptance runs of a REAL converted TIR model, recorded — never a PASS.

For each context length (default 8192, 16384, 32768) and each position-run width (default 1 and 32, `PALW_FLOAT_POSITION_BLOCK`),
run `palw-tir-fidelity <src> --calib … --stats-in … --eval <long tokens> --eval-seqs 1 --positions L --exec --drift-windows W
--json --ram-accounting FILE` under `/usr/bin/time -l`, twice when `--repeat` (the first run is labelled `first` — the page cache
is NOT purged, a true cold run needs `sudo purge`, which this script never runs — the second `warm`). Records per run: wall time,
peak resident set, block I/O, the fidelity metrics and drift curve, the program digest, the RAM-accounting peak by kind. Then,
per length, whether every run produced the SAME program digest, metrics and drift (the run width and the repeat must not change
a bit: RFC-0020 §5.5 / §7). A configuration this host cannot hold (`--max-rss-gb`) is skipped before it is started and recorded
with the reason.

    python3 scripts/genesis/rfc20_acceptance.py --src DIR --calib calib-tokens.json --stats-in calib-stats.json \\
        --eval eval-long-tokens.json --out rfc20.json [--lengths 8192,16384] [--runs 1,32] [--repeat] [--bin target/release]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path


def time_l(argv: list[str], env: dict) -> tuple[int, str, str, dict]:
    started = time.time()
    p = subprocess.run(["/usr/bin/time", "-l", *argv], env=env, capture_output=True, text=True)
    wall = time.time() - started
    tl = {"wall_seconds": round(wall, 3)}
    for key, pat in (("max_rss_bytes", r"(\d+)\s+maximum resident set size"), ("block_input_ops", r"(\d+)\s+block input operations"),
                     ("block_output_ops", r"(\d+)\s+block output operations"), ("peak_memory_footprint_bytes", r"(\d+)\s+peak memory footprint")):
        m = re.search(pat, p.stderr)
        if m:
            tl[key] = int(m.group(1))
    return p.returncode, p.stdout, p.stderr, tl


def ram_peak(path: Path) -> dict:
    peak: dict = {}
    if not path.exists():
        return peak
    for line in path.read_text().splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        for k, v in row.items():
            if isinstance(v, (int, float)) and ("peak" in k or "rss" in k or "live" in k):
                peak[k] = max(peak.get(k, 0), v)
    return peak


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True)
    ap.add_argument("--calib", required=True)
    ap.add_argument("--stats-in", required=True)
    ap.add_argument("--eval", required=True, help="a JSON token file whose first sequence is at least the longest length")
    ap.add_argument("--out", required=True)
    ap.add_argument("--lengths", default="8192,16384,32768")
    ap.add_argument("--runs", default="1,32")
    ap.add_argument("--repeat", action="store_true")
    ap.add_argument("--drift-windows", type=int, default=8)
    ap.add_argument("--bin", default=str(Path(__file__).resolve().parents[2] / "target" / "release"))
    ap.add_argument("--work", default=None, help="where RAM-accounting files go (default: next to --out)")
    ap.add_argument("--max-rss-gb", type=float, default=24.0, help="skip a configuration estimated past this resident set")
    ap.add_argument("--estimate-rss-gb", default="", help="L:GB,… estimates; a length over --max-rss-gb is skipped, not run")
    a = ap.parse_args()
    out = Path(a.out)
    work = Path(a.work) if a.work else out.parent / (out.stem + "-runs")
    work.mkdir(parents=True, exist_ok=True)
    estimates = {int(k): float(v) for k, v in (kv.split(":") for kv in a.estimate_rss_gb.split(",") if kv)}
    fid = str(Path(a.bin) / "palw-tir-fidelity")
    runs = []
    for length in [int(x) for x in a.lengths.split(",")]:
        if estimates.get(length, 0.0) > a.max_rss_gb:
            runs.append({"length": length, "skipped": f"estimated {estimates[length]} GB resident > --max-rss-gb {a.max_rss_gb}"})
            continue
        for width in [int(x) for x in a.runs.split(",")]:
            for label in (["first", "warm"] if a.repeat else ["first"]):
                ram = work / f"ram-L{length}-run{width}-{label}.jsonl"
                ram.unlink(missing_ok=True)
                argv = [fid, a.src, "--calib", a.calib, "--stats-in", a.stats_in, "--eval", a.eval, "--eval-seqs", "1",
                        "--positions", str(length), "--exec", "--drift-windows", str(a.drift_windows), "--json",
                        "--ram-accounting", str(ram)]
                env = dict(os.environ, PALW_FLOAT_POSITION_BLOCK=str(width))
                print(f"[rfc20] L={length} run={width} {label}", file=sys.stderr, flush=True)
                code, stdout, stderr, tl = time_l(argv, env)
                row = {"length": length, "run": width, "label": label, "exit": code, **tl, "ram_accounting_peak": ram_peak(ram)}
                try:
                    res = json.loads(stdout[stdout.index("{"):])
                    row.update({"program_digest": res.get("program_digest"), "metrics": res.get("metrics"), "drift": res.get("drift"),
                                "fidelity_seconds": res.get("seconds")})
                except (ValueError, json.JSONDecodeError):
                    row["stderr_tail"] = stderr[-2000:]
                runs.append(row)
                out.write_text(json.dumps({"runs": runs}, indent=1))
    identity = {}
    for length in sorted({r["length"] for r in runs if "exit" in r}):
        rows = [r for r in runs if r.get("length") == length and r.get("exit") == 0]
        keys = {json.dumps([r.get("program_digest"), r.get("metrics"), r.get("drift")], sort_keys=True) for r in rows}
        identity[str(length)] = {"runs": len(rows), "identical": len(keys) == 1 and len(rows) > 1 if rows else None}
    out.write_text(json.dumps({"tool": "rfc20_acceptance.py", "note": "a measurement, not a PASS; page cache not purged",
                               "runs": runs, "identity_by_length": identity}, indent=1))
    print(json.dumps(identity))
    return 0


if __name__ == "__main__":
    sys.exit(main())
