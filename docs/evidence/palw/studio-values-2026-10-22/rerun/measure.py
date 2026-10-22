#!/usr/bin/env python3
"""values/p1-measure (2026-10-22) — rerun driver for the measurements of
docs/design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md §3 that need no real artifact
(M-C, M-X3 / M-ENV / M-RHO / M-DA inputs, R6).

It builds the test binaries once (release, `nice -n 10`), then runs each measurement in its own process under
`/usr/bin/time` (macOS `-l`, Linux `-v`) WITHOUT nice, gating every batch on the 1-minute load average
(<= cores / 2; waits up to --max-wait seconds, then runs and marks the batch `under_load`). It writes one JSON
document per batch into --out and a combined `results.json`.

Nothing here touches a live node; nothing here decides a value. Usage (from the repository root):

    python3 docs/evidence/palw/studio-values-2026-10-22/rerun/measure.py --out /tmp/p1-out [--batches all]

Batches: court_verify, in_crate, court_op_harness, frame_measure (or `all`).
Environment: CARGO_TARGET_DIR is honoured (default: <repo>/target).
"""
import argparse
import json
import os
import platform
import re
import subprocess
import sys
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../.."))
IS_MAC = platform.system() == "Darwin"

COURT_OPS_N = ["close_cone_acquit", "close_cone_forged_last_stage", "root_claim_admit", "dissect_round_apply",
               "dissect_bottom_honest", "lookahead_expand", "mldsa87_verify"]
# ops whose growth order is varied by MEASURE_SCALE (N fixed at 1000 for per-item ops, 21 for the whole-registry sweep)
COURT_OPS_SCALED = {
    "position_close_honest": [4, 8, 16],
    "position_close_forged": [4, 8, 16],
    "d2_demand_open": [16, 256, 4096],
    "d2_answer_disclosed": [16, 256, 4096],
    "d2_answer_convicted": [16, 256, 4096],
    "d2_sweep": [16, 256, 4096],
}
N_LEVELS = [10, 100, 1000]


def host_info():
    info = {"platform": platform.platform(), "machine": platform.machine(), "python": platform.python_version()}
    if IS_MAC:
        for k in ["hw.model", "hw.memsize", "hw.ncpu", "hw.perflevel0.physicalcpu", "hw.perflevel1.physicalcpu",
                  "machdep.cpu.brand_string"]:
            r = subprocess.run(["sysctl", "-n", k], capture_output=True, text=True)
            info[k] = r.stdout.strip()
    else:
        info["nproc"] = subprocess.run(["nproc"], capture_output=True, text=True).stdout.strip()
        try:
            with open("/proc/meminfo") as f:
                info["MemTotal"] = f.readline().strip()
            with open("/proc/cpuinfo") as f:
                names = [l.split(":", 1)[1].strip() for l in f if l.startswith("model name")]
            info["cpu"] = names[0] if names else ""
        except OSError:
            pass
    info["ncpu"] = os.cpu_count()
    return info


def load_snapshot():
    up = subprocess.run(["uptime"], capture_output=True, text=True).stdout.strip()
    return {"t": time.time(), "uptime": up, "loadavg": os.getloadavg()}


def gate(max_wait):
    """Wait until the 1-min load <= ncpu/2. Returns (snapshot, under_load)."""
    limit = (os.cpu_count() or 2) / 2
    start = time.time()
    while True:
        s = load_snapshot()
        if s["loadavg"][0] <= limit:
            return s, False
        if time.time() - start >= max_wait:
            return s, True
        time.sleep(30)


def cargo_build(args, log):
    cmd = ["nice", "-n", "10", "cargo"] + args + ["--message-format=json"]
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", os.path.join(REPO, "target"))
    p = subprocess.run(cmd, cwd=REPO, env=env, capture_output=True, text=True)
    with open(log, "a") as f:
        f.write(" ".join(cmd) + "\n" + p.stderr[-20000:] + "\n")
    if p.returncode != 0:
        sys.exit(f"build failed: {' '.join(cmd)} (see {log})")
    exes = []
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("reason") == "compiler-artifact" and m.get("executable"):
            exes.append((m["target"]["name"], m["target"]["kind"], m["executable"]))
    return exes


def timed(cmd, env_extra, cwd):
    tcmd = ["/usr/bin/time", "-l" if IS_MAC else "-v"] + cmd
    env = dict(os.environ)
    env.update(env_extra)
    load = os.getloadavg()
    t0 = time.time()
    p = subprocess.run(tcmd, cwd=cwd, env=env, capture_output=True, text=True)
    wall = time.time() - t0
    err = p.stderr
    rss = None
    if IS_MAC:
        m = re.search(r"(\d+)\s+maximum resident set size", err)
        rss = int(m.group(1)) if m else None
    else:
        m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", err)
        rss = int(m.group(1)) * 1024 if m else None
    m_user = re.search(r"([\d.]+)\s+user", err) if IS_MAC else re.search(r"User time \(seconds\):\s*([\d.]+)", err)
    m_sys = re.search(r"([\d.]+)\s+sys", err) if IS_MAC else re.search(r"System time \(seconds\):\s*([\d.]+)", err)
    lines = [l for l in (p.stdout + "\n" + err).splitlines() if l.startswith("MEASURE") or l.startswith("[palw_court_op_timing_harness]")
             or l.startswith("[tir-frame-measure]")]
    return {
        "cmd": cmd, "env": env_extra, "rc": p.returncode, "process_wall_s": round(wall, 3), "peak_rss_bytes": rss,
        "loadavg_at_start": load, "under_load": load[0] > (os.cpu_count() or 2) / 2,
        "user_s": float(m_user.group(1)) if m_user else None, "sys_s": float(m_sys.group(1)) if m_sys else None,
        "lines": lines, "stderr_tail": err[-3000:] if p.returncode != 0 else "",
    }


def parse_kv(line):
    return dict(kv.split("=", 1) for kv in line.split() if "=" in kv)


def find(exes, name, kind=None):
    for n, k, e in exes:
        if n == name and (kind is None or kind in k):
            return e
    sys.exit(f"no executable {name} {kind} in build output")


def batch_court_verify(out, log, max_wait):
    exes = cargo_build(["test", "--release", "-p", "kaspa-consensus-core", "--test", "palw_tir_court_verify_cpu", "--no-run"], log)
    exe = find(exes, "palw_tir_court_verify_cpu")
    base = [exe, "court_verify_cpu_measure", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    runs = []
    before, under = gate(max_wait)
    for _ in range(3):
        runs.append(timed(base, {"MEASURE_OP": "fixture_setup_only", "MEASURE_N": "0"}, REPO))
    for op in COURT_OPS_N:
        for _ in range(3):
            runs.append(timed(base, {"MEASURE_OP": op, "MEASURE_N": "0"}, REPO))
        for n in N_LEVELS:
            reps = 3 if n == 1000 else 1
            for _ in range(reps):
                runs.append(timed(base, {"MEASURE_OP": op, "MEASURE_N": str(n)}, REPO))
    for op, scales in COURT_OPS_SCALED.items():
        for sc in scales:
            n = "21" if op == "d2_sweep" else "1000"
            runs.append(timed(base, {"MEASURE_OP": op, "MEASURE_N": "0", "MEASURE_SCALE": str(sc)}, REPO))
            # position cases take ~80 s of setup per process on an M1 Max: one timed process per scale (999 in-process samples)
            for _ in range(1 if op.startswith("position") else 3):
                runs.append(timed(base, {"MEASURE_OP": op, "MEASURE_N": n, "MEASURE_SCALE": str(sc)}, REPO))
    after = load_snapshot()
    return {"batch": "court_verify", "executable": exe, "load_before": before, "under_load": under, "load_after": after, "runs": runs}


def batch_in_crate(out, log, max_wait):
    exes = cargo_build(["test", "--release", "-p", "kaspa-consensus-core", "--lib", "--no-run"], log)
    exe = find(exes, "kaspa_consensus_core", "lib")
    runs = []
    before, under = gate(max_wait)
    for name, scales in [("material_intake_measure_wall_time_and_retained_bytes", "0,1000,10000"),
                         ("r6_d4_measure_post_final_duty_wall_time", "10,100,1000")]:
        runs.append(timed([exe, name, "--ignored", "--nocapture", "--test-threads=1"],
                          {"MEASURE_RUNS": "21", "MEASURE_SCALES": scales}, REPO))
    after = load_snapshot()
    return {"batch": "in_crate", "executable": exe, "load_before": before, "under_load": under, "load_after": after, "runs": runs}


def batch_court_op_harness(out, log, max_wait):
    exes = cargo_build(["test", "--release", "-p", "kaspa-consensus-core", "--test", "g14_court_capacity", "--no-run"], log)
    exe = find(exes, "g14_court_capacity")
    before, under = gate(max_wait)
    runs = [timed([exe, "palw_court_op_timing_harness", "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                  {"MEASURE_RUNS": "21"}, REPO)]
    after = load_snapshot()
    return {"batch": "court_op_harness", "executable": exe, "load_before": before, "under_load": under, "load_after": after, "runs": runs}


def batch_frame_measure(out, log, max_wait):
    exes = cargo_build(["test", "--release", "-p", "misaka-palw-tir-exec", "--features", "node", "--test", "frame_measure",
                        "--no-run"], log)
    exe = find(exes, "frame_measure")
    before, under = gate(max_wait)
    runs = []
    os.makedirs(os.path.join(out, "frame"), exist_ok=True)
    for i in range(21):
        rec = os.path.join(out, "frame", f"frame-{i:02d}.json")
        runs.append(timed([exe, "tir_frame_operations_are_measured_on_every_corpus_class", "--exact", "--nocapture",
                           "--test-threads=1"], {"MISAKA_TIR_FRAME_MEASURE_OUT": rec, "MISAKA_MEASURE_HOST": platform.node()},
                          os.path.join(REPO, "misaka/palw/tir/exec")))
    after = load_snapshot()
    return {"batch": "frame_measure", "executable": exe, "load_before": before, "under_load": under, "load_after": after, "runs": runs}


BATCHES = {"court_verify": batch_court_verify, "in_crate": batch_in_crate, "court_op_harness": batch_court_op_harness,
           "frame_measure": batch_frame_measure}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--batches", default="all")
    ap.add_argument("--max-wait", type=int, default=1200, help="seconds to wait for load <= cores/2 before a batch")
    ap.add_argument("--source-commit", default="")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    names = list(BATCHES) if a.batches == "all" else a.batches.split(",")
    log = os.path.join(a.out, "build.log")
    host = host_info()
    for name in names:
        res = BATCHES[name](a.out, log, a.max_wait)
        res["host"] = host
        res["source_commit"] = a.source_commit
        with open(os.path.join(a.out, f"{name}.json"), "w") as f:
            json.dump(res, f, indent=1, sort_keys=True)
        bad = [r for r in res["runs"] if r["rc"] != 0]
        print(f"[measure] {name}: {len(res['runs'])} processes, {len(bad)} failed, under_load={res['under_load']}", flush=True)


if __name__ == "__main__":
    main()
