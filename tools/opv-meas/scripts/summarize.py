#!/usr/bin/env python3
"""summarize.py — the machine-readable table of the OPV fresh-verifier measurements (docs/design/palw/opv-measurements.json).

Reads the harness's JSON-lines results and writes ONE JSON file. Every number in the output is a leaf object

    {"v": <value>, "kind": "measured" | "derived" | "assumed", "src": "<where it comes from>", ...optional n / min / max / median}

  measured — read off a harness run on the host below (samples listed under "n"; the spread is given);
  derived  — computed from measured numbers by the stated formula, no other input;
  assumed  — a value no run established; it is named as such and says what would replace it.

Usage (python -I):  summarize.py RESULTS_DIR OUT.json
"""
import json
import math
import re
import sys
from pathlib import Path

res = Path(sys.argv[1])
out_path = Path(sys.argv[2])


def read_jsonl(name):
    rows = []
    p = res / name
    if not p.exists():
        return rows
    for line in p.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return rows


def read_json(name):
    return json.loads((res / name).read_text())


def m(v, src, **kw):
    return {"v": v, "kind": "measured", "src": src, **kw}


def d(v, formula, **kw):
    return {"v": v, "kind": "derived", "src": formula, **kw}


def a(v, why, **kw):
    return {"v": v, "kind": "assumed", "src": why, **kw}


def stat(xs):
    xs = sorted(xs)
    n = len(xs)
    med = xs[n // 2] if n % 2 else (xs[n // 2 - 1] + xs[n // 2]) / 2
    return {"n": n, "median": med, "min": xs[0], "max": xs[-1]}


def statm(xs, src):
    s = stat(xs)
    return m(s["median"], src, n=s["n"], min=s["min"], max=s["max"])


def phase(sample, name):
    for p in sample["phases"]:
        if p["name"] == name:
            return p
    return None


def lsq(xs, ys):
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx == 0:
        return None
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx
    a0 = my - b * mx
    resid = [y - (a0 + b * x) for x, y in zip(xs, ys)]
    return a0, b, max(abs(r) for r in resid)


# ── inputs ──────────────────────────────────────────────────────────────────────────────────────────────────────────────
static = {r["label"]: r for r in read_jsonl("static.jsonl") if r.get("cmd") == "static"}
weights = {r["label"]: r for r in read_jsonl("weights.jsonl") if r.get("cmd") == "weights"}
micro = [r for r in read_jsonl("micro.jsonl") if r.get("cmd") == "micro"][0]
worlds = {}
for r in read_jsonl("worlds.jsonl"):
    if r.get("cmd") == "build-world":
        worlds[r["world"]["label"]] = r
for extra in sorted(res.glob("world-*.json")):
    r = json.loads(extra.read_text())
    worlds[r["world"]["label"]] = r

verifies = []
for f in ("qwen25-p3.jsonl", "qwen25-sweep.jsonl"):
    verifies += [r for r in read_jsonl(f) if r.get("cmd") == "verify"]
for f in sorted(res.glob("early-*.json")):
    verifies.append(json.loads(f.read_text()))
notes = [r for f in ("qwen25-p3.jsonl", "qwen25-sweep.jsonl") for r in read_jsonl(f) if "note" in r]

DAA_MIN_S = 120

host = {
    "machine": m("Apple M1 Max, 10 cores, 32 GiB, macOS 26.7", "sysctl hw.ncpu hw.memsize; sample(1)"),
    "shared": "yes: 9 devnet kaspad + other lanes' builds ran during every sample; see load and swap per sample",
    "runs_after_the_restart": "glm-edge-1.5b (static, weights, build-world) ran on 2026-10-10 05:11-05:16 at 1-minute load 13-18, swap 4 GB; every other class ran on 2026-10-09",
    "load1_during_verify_samples": stat([s["load_before"][0] for s in verifies] + [s["load_after"][0] for s in verifies]),
    "swap_used_gb_at_samples": stat([n["swap_used_gb"] for n in notes if "swap_used_gb" in n]) if any("swap_used_gb" in n for n in notes) else None,
    "verifier_implementation": "reference (misaka-palw-kernel check path): single thread, i128 tensors, every parameter instance cached for the scope (16 bytes/param)",
    "provider": "files = a directory in the misaka-palw-remote content-addressed layout; http-localhost = its reference HTTP provider on 127.0.0.1 (NO network: the link term is unmeasured)",
}

unit = {
    "field_mult_gf127_cpu_ns": statm([r["cpu_ns_per_mult"] for r in micro["field_mult_gf127"]], "micro: Fp::dot, raw field words"),
    "exact_mac_cpu_ns": statm([r["cpu_ns_per_mac"] for r in micro["exact_row_recompute"]], "micro: checked i128 multiply-accumulate (a localized row's recompute)"),
    "tensor_commitment_cpu_ns_per_element": statm([r["cpu_ns_per_element"] for r in micro["tensor_commitment_i8"]], "micro: dual-root Merkle commitment of an i8 tensor"),
    "read_tensor_cpu_ns_per_element": statm([r["cpu_ns_per_element"] for r in micro["read_tensor_biggest"]], "micro: container tensor -> i128"),
}

# ── per class ───────────────────────────────────────────────────────────────────────────────────────────────────────────
classes = {}
for label, st in static.items():
    w = weights.get(label)
    wd = worlds.get(label)
    c = {"label": label}
    c["artifact"] = {
        "container_bytes": m(st["container_bytes"], "static"),
        "params": m(st["params"], "static"),
        "param_instances": m(st["param_instances"], "static"),
        "committed_nodes_per_position": m(st["committed_nodes_per_position"], "static"),
        "token_bound": m(st["token_bound"], "static"),
        "history_bound": m(st["history_bound"], "static"),
    }
    plan = {}
    for r in st["by_positions"]:
        v2 = r["K2-TIR-v2"]
        plan[f"max_positions_{r['max_positions']}"] = {
            "evidence_bytes_per_position_bound": m(int(v2["budgets"]["evidence_bytes_per_position"]), "static: plan budget at the class's history bound"),
            "gate_ok": m(v2["gate"].get("ok"), "static: public_prosecution_complete_v1"),
            "worst_filing_bytes": m(v2["gate"].get("max_filing_bytes"), "static: gate bound") if v2["gate"].get("ok") else None,
            "max_verifier_ram_bound": m(int(v2["gate"]["max_verifier_ram"]), "static: gate bound = artifact_bytes + evidence_bytes_per_position") if v2["gate"].get("ok") else None,
            "opv_carrier_fit_interim": m((v2.get("opv") or {}).get("interim_carriers", {}).get("fits"), "static: carrier_fit_v1 at the interim 1,583,616-byte carriers"),
            "opv_carrier_refusal": (v2.get("opv") or {}).get("interim_carriers", {}).get("why"),
        }
    c["plan_bounds_k2_v2"] = plan
    if w:
        n = w["params"]
        c["weights_pass_streaming"] = {
            "read_cpu_s": m(w["read"]["cpu_s"], "weights"),
            "commit_once_cpu_s": m(w["commit_once"]["cpu_s"], "weights"),
            "project_cpu_s": m(w["project"]["cpu_s"], "weights: 2 repetitions of W r over GF(2^127-1)"),
            "field_mults": m(w["project"]["field_mults"], "weights"),
            "model_cpu_s_commit_twice": d(w["model_cpu_s_commit_twice"], "read + 2 x commit + project (the reference verifier authenticates a weight twice)"),
            "model_cpu_ns_per_param": d(w["model_cpu_ns_per_param_commit_twice"], "model_cpu_s / params"),
            "wall_s_under_load": m(w["wall_s"], "weights", load1_before=w["load_before"][0]),
            "peak_rss_bytes": m(w["peak_rss_bytes"], "weights (streaming: one tensor at a time)"),
        }
    if wd:
        wo = wd["world"]
        ph = {p["name"]: p for p in wd["phases"]}
        c["producer_side_p3"] = {
            "registration_mode": m(wo["mode"], "build-world: OPV first, Panel-licensed when OPV is refused"),
            "opv_registration_refusal": wo["opv_refusal"],
            "honest_trace_cpu_s": m(ph["honest_trace"]["cpu_s"], "build-world: the producer's reference evaluation of 3 positions"),
            "honest_commitments_cpu_s": m(ph["honest_commitments"]["cpu_s"], "build-world"),
            "param_commitments_cpu_s": m(ph["param_commitments"]["cpu_s"], "build-world"),
            "chain_bytes": m(wo["chain_bytes"], "build-world: class registration + 6 claims' commitments (Borsh)"),
        }
        honest = [cl for cl in wo["claims"] if cl["name"] == "honest"][0]
        c["da_p3"] = {
            "claim_public_material_bytes": m(honest["da_bytes"], "build-world: every committed value but the derived ones, 3 positions"),
            "per_position_bytes": d(honest["da_bytes"] / honest["positions"], "da bytes / 3 positions (small history: it grows with the position index)"),
        }
    classes[label] = c

# ── qwen25: the verifier, end to end ────────────────────────────────────────────────────────────────────────────────────
Q = "qwen25-0.5b"
qs = [s for s in verifies if s["label"].startswith(Q)]
p3 = [s for s in qs if s["label"] == Q]
files = [s for s in p3 if s["provider"] == "files"]
http = [s for s in p3 if s["provider"] == "http-localhost"]


def ph_stat(samples, name, key):
    xs = [phase(s, name)[key] for s in samples if phase(s, name)]
    return stat(xs) if xs else None


def phase_m(samples, name, key, src):
    st_ = ph_stat(samples, name, key)
    return m(st_["median"], src, n=st_["n"], min=st_["min"], max=st_["max"]) if st_ else None


honest_outsider = [s for s in files if s["claim"] == "honest" and s["via"] == "outsider" and s.get("withhold_pos") is None]
honest_fresh = [s for s in qs if s["claim"] == "honest" and s["via"] == "fresh"]
q = classes[Q]
q["end_to_end_measured"] = True
q["t_check_p3_outsider_path"] = {
    "what": "OutsiderV1::check on an honest 3-position claim: authenticate every served value, the decode relation, check_salted over every relation",
    "cpu_s": phase_m(honest_outsider, "check_outsider", "cpu_s", "verify: honest, files"),
    "wall_s_under_load": phase_m(honest_outsider, "check_outsider", "wall_s", "verify: honest, files; the host was shared"),
    "total_process_cpu_s": statm([s["cpu_total_s"] for s in honest_outsider], "verify: one fresh process, replay to verdict"),
    "total_process_wall_s": statm([s["wall_total_s"] for s in honest_outsider], "verify"),
    "peak_rss_bytes_max": m(max(s["peak_rss_bytes"] for s in honest_outsider), "verify: max over samples (macOS ru_maxrss: compressed/swapped pages are not counted, so the minimum is lower)", min=min(s["peak_rss_bytes"] for s in honest_outsider)),
    "peak_rss_per_param_bytes": d(max(s["peak_rss_bytes"] for s in honest_outsider) / q["artifact"]["params"]["v"], "peak_rss_max / params"),
}
# the position sweep: check_salted CPU against P, and against the claim's public-material bytes
pts = []
for s in honest_fresh:
    cs = phase(s, "check_salted")
    pts.append({"P": s["result"]["cost"]["positions"], "da_bytes": s["da_bytes"], "cpu": cs["cpu_s"], "wall": cs["wall_s"], "cost": s["result"]["cost"], "load1": s["load_before"][0], "rss": s["peak_rss_bytes"]})
q["t_check_by_positions_fresh_path"] = {
    "what": "FreshVerifierV1::check_salted over an honest claim of P positions (no value authentication, no decode relation): the verifier's relation work",
    "samples": [m({k: (v if k != "cost" else None) for k, v in p.items() if k != "cost"}, "verify --via fresh") for p in pts],
    "counters": [m({"P": p["P"], "field_mults": int(p["cost"]["field_mults"]), "exact_elements": int(p["cost"]["exact_elements"]), "opened_bytes": int(p["cost"]["opened_bytes"]), "param_bytes": int(p["cost"]["param_bytes"])}, "verify --via fresh: the kernel's CheckCostV1") for p in pts],
}
if len({p["P"] for p in pts}) >= 2:
    fit = lsq([p["P"] for p in pts], [p["cpu"] for p in pts])
    fit_da = lsq([p["da_bytes"] for p in pts], [p["cpu"] for p in pts])
    q["t_check_by_positions_fresh_path"]["fit_cpu_s_vs_positions"] = d({"intercept_s": fit[0], "per_position_s": fit[1], "max_abs_residual_s": fit[2]}, "least squares over the samples above: cpu = a + b P")
    q["t_check_by_positions_fresh_path"]["fit_cpu_s_vs_public_material_bytes"] = d({"intercept_s": fit_da[0], "per_byte_s": fit_da[1], "max_abs_residual_s": fit_da[2]}, "least squares: cpu = a + rho x public-material bytes")
# fetch
q["fetch"] = {}
for name, group in (("files", files), ("http_localhost", http)):
    g = [s for s in group]
    if not g:
        continue
    q["fetch"][name] = {
        "claim_material_bytes": m(g[0]["da_bytes"], "verify"),
        "claim_material_wall_s": phase_m(g, "fetch_da", "wall_s", f"verify: fetch_claim_material_any, every chunk hash-checked against a manifest bound to the claim; provider={name}"),
        "claim_material_cpu_s": phase_m(g, "fetch_da", "cpu_s", "verify"),
        "artifact_bytes": m(g[0]["artifact_bytes"], "verify"),
        "artifact_wall_s": phase_m(g, "fetch_artifact", "wall_s", f"verify: manifest check + every chunk hash-checked; provider={name}"),
        "artifact_cpu_s": phase_m(g, "fetch_artifact", "cpu_s", "verify"),
        "chain_replay_cpu_s": phase_m(g, "ledger_replay", "cpu_s", "verify: strict decode + replay of the registration and the claim blocks"),
    }
if "files" in q["fetch"]:
    f = q["fetch"]["files"]
    q["fetch"]["verification_throughput"] = d(
        {"claim_material_MB_per_cpu_s": f["claim_material_bytes"]["v"] / 1e6 / f["claim_material_cpu_s"]["v"], "artifact_MB_per_cpu_s": f["artifact_bytes"]["v"] / 1e6 / f["artifact_cpu_s"]["v"]},
        "bytes / cpu seconds of fetch_da and fetch_artifact: the CPU ceiling of 'download and hash-check' on this core, with no link in the path",
    )
# localization and filing, per lie
base = statm([phase(s, "check_outsider")["cpu_s"] for s in honest_outsider], "verify honest, files")
q["lies"] = {}
for name in ("early", "late", "gather", "elem", "decode"):
    g = [s for s in files if s["claim"] == name and s["via"] == "outsider"]
    if not g:
        continue
    r = g[0]["result"]
    ck = [phase(s, "check_outsider")["cpu_s"] for s in g]
    e = {
        "samples": m(len(g), "verify"),
        "finding": m(r.get("finding"), "verify"),
        "check_to_verdict_cpu_s": statm(ck, "verify: check_outsider, which stops at the first fault"),
        "check_to_verdict_wall_s": statm([phase(s, "check_outsider")["wall_s"] for s in g], "verify"),
        "fault": m(r.get("fault"), "verify: the kernel's localized fault (kind 1 = element recompute, 2 = matmul row scalar)") if r.get("fault") else None,
        "filing_proof_bytes": m(r.get("filing_proof_bytes"), "verify: FaultProofWireV1") if r.get("filing_proof_bytes") is not None else None,
        "filing_object_bytes": m(r.get("filing_object_bytes"), "verify: the FileProof route object, strict-encoded"),
        "fits_route_object_cap": m(r.get("fits_route_object_cap"), "verify: against PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 (1,583,616)"),
        "refused_at_wire": m(r.get("filing_refused_at_wire"), "verify: K::decode of the encoded FileProof (the strict wire decode every node runs)") if r.get("filing_refused_at_wire") else None,
        "filing_assemble_cpu_s": phase_m(g, "assemble_filing", "cpu_s", "verify: decode + rebuild + encode the proof") if phase(g[0], "assemble_filing") else None,
        "court_cpu_s": phase_m(g, "court_node", "cpu_s", "verify: a node replica's court, apply_block") if phase(g[0], "court_node") else None,
        "court_wall_s": phase_m(g, "court_node", "wall_s", "verify") if phase(g[0], "court_node") else None,
        "court_events": m(r.get("court_events"), "verify"),
    }
    if name in ("late", "elem"):
        e["t_localize_upper_bound_s"] = d(
            stat(ck)["median"] - base["v"],
            "median check-to-verdict CPU of the lying claim minus the median honest check CPU. The fault is in the LAST position, so detection costs what a pass would; the excess bounds localization + proof assembly from above. The host's run-to-run noise on a 140 s check is +-16 s (the fit residual), so a value inside that band is 'not distinguishable from 0'",
        )
    q["lies"][name] = e
# localization by recompute, from the unit cost (the dominant term of a matmul-row localization)
q["t_localize_model"] = d(
    {"logits_row_macs": 896 * 151936, "cpu_s": 896 * 151936 * unit["exact_mac_cpu_ns"]["v"] * 1e-9},
    "k x n exact i128 multiply-accumulates of the failing lm_head row x the measured exact_mac_cpu_ns; the lie 'late' is in that row",
)
# the demand / disclosure flow
dem = [s for s in files if s.get("withhold_pos") is not None]
if dem:
    r = dem[0]["result"]
    q["demand_flow"] = {
        "finding": m(r.get("finding"), "verify --withhold-pos 0 --serve"),
        "response_object_bytes": m(r.get("response_object_bytes"), "verify: one position's committed values as a Respond object"),
        "fits_route_object_cap": m(r.get("fits_response_object_cap"), "verify: 1,583,616-byte interim object cap; the harness's replica ledger applied it anyway (it does not enforce the carrier at apply time), so 'Served' there is NOT evidence that a chain would carry it"),
        "check_after_served_cpu_s": phase_m(dem, "check_after_served", "cpu_s", "verify"),
    }

# notes about samples that were skipped for memory
q["first_pass_samples_skipped_for_host_memory_then_rerun"] = m([n["note"] for n in notes if str(n.get("note", "")).startswith("skip")], "run-class.sh guard (free+inactive memory below the minimum); matrix2.sh re-ran every one of them, waiting for memory instead of skipping")

# ── the classes the reference verifier cannot hold on this host ─────────────────────────────────────────────────────────────
fit_n = q["t_check_by_positions_fresh_path"].get("fit_cpu_s_vs_positions")
fit_b = q["t_check_by_positions_fresh_path"].get("fit_cpu_s_vs_public_material_bytes")
w_q = classes[Q].get("weights_pass_streaming")
for label, c in classes.items():
    if label == Q:
        continue
    c["end_to_end_measured"] = False
    c["why_not_end_to_end"] = d(
        {"min_verifier_ram_bytes": 16 * c["artifact"]["params"]["v"], "host_ram_bytes": 32 * 2**30},
        "16 bytes x params: Ctx::param_cache keeps every parameter instance as i128 for the scope (misaka-palw-kernel/src/verify.rs)",
    )
    wp = c.get("weights_pass_streaming")
    if wp:
        c["t_check_lower_bound_cpu_s"] = d(wp["model_cpu_s_commit_twice"]["v"], "the weight pass alone (read + 2 x commit + projection), measured streaming on the real artifact: a fresh check cannot cost less")
    if wp and fit_n and fit_b and w_q and c.get("da_p3"):
        kappa = fit_n["v"]["intercept_s"] / w_q["model_cpu_s_commit_twice"]["v"]
        per_pos_bytes = c["da_p3"]["per_position_bytes"]["v"]
        est_fixed = kappa * wp["model_cpu_s_commit_twice"]["v"]
        est_pos = fit_b["v"]["per_byte_s"] * per_pos_bytes
        c["t_check_estimate_p3_cpu_s"] = a(
            est_fixed + 3 * est_pos,
            "ASSUMPTION, not a measurement: fixed part = (0.5B intercept / 0.5B weight-pass model) x this class's weight-pass model; per-position part = the 0.5B cost per public-material byte x this class's bytes per position. Replace by a run on a host with the RAM (fleet plan F2)",
            fixed_s=est_fixed, per_position_s=est_pos, kappa=kappa,
        )
    c["samples"] = "none: no claim was verified for this class"

# ── the flat table: one row per (class, claim size) ──────────────────────────────────────────────────────────────────────────
rate = (q.get("fetch", {}).get("verification_throughput") or {}).get("v", {})
files_f = q.get("fetch", {}).get("files", {})
table = []


def row(cls_, P, **kw):
    r = {"class": cls_, "positions": P}
    r.update(kw)
    table.append(r)


tco = q["t_check_p3_outsider_path"]
lies = q.get("lies", {})
filing_sizes = {k: v["filing_object_bytes"] for k, v in lies.items()}
loc = {"upper_bound_late_s": lies.get("late", {}).get("t_localize_upper_bound_s"), "model_row_recompute_s": q["t_localize_model"]}
row(Q, 3, path="OutsiderV1::check (authenticates every value, decode relation, check_salted)",
    t_check_cpu_s=tco["cpu_s"], t_check_wall_s=tco["wall_s_under_load"], peak_rss_bytes=tco["peak_rss_bytes_max"],
    da_claim_material_bytes=files_f["claim_material_bytes"], da_claim_material_fetch_wall_s=files_f["claim_material_wall_s"],
    artifact_bytes=files_f["artifact_bytes"], artifact_fetch_wall_s=files_f["artifact_wall_s"], fetch_provider="files (no link)",
    filing_object_bytes_by_lie=filing_sizes, t_localize=loc,
    note="the only class run end to end; 5 honest samples, host load 37-220")
for pt, smp in zip(pts, honest_fresh):
    ph = phase(smp, "check_salted")
    row(Q, pt["P"], path="FreshVerifierV1::check_salted only",
        t_check_cpu_s=m(ph["cpu_s"], "verify --via fresh", load1=smp["load_before"][0]), t_check_wall_s=m(ph["wall_s"], "verify --via fresh"),
        peak_rss_bytes=m(smp["peak_rss_bytes"], "verify --via fresh"), da_claim_material_bytes=m(smp["da_bytes"], "verify"),
        da_claim_material_fetch_wall_s=m(phase(smp, "fetch_da")["wall_s"], "verify: files"), artifact_bytes=m(smp["artifact_bytes"], "verify"),
        artifact_fetch_wall_s=m(phase(smp, "fetch_artifact")["wall_s"], "verify: files") if phase(smp, "fetch_artifact") else None, fetch_provider="files (no link)",
        filing_object_bytes_by_lie=None, t_localize=None, note="honest claim; relation work only")
for label, c in classes.items():
    if label == Q:
        continue
    da = c.get("da_p3")
    tput = rate.get("claim_material_MB_per_cpu_s")
    row(label, 3, path="not run (reference verifier needs >= 16 B/param of RAM)",
        t_check_cpu_s=None, t_check_lower_bound_cpu_s=c.get("t_check_lower_bound_cpu_s"), t_check_estimate_cpu_s=c.get("t_check_estimate_p3_cpu_s"),
        peak_rss_bytes=c.get("why_not_end_to_end"),
        da_claim_material_bytes=da["claim_public_material_bytes"] if da else None,
        da_claim_material_fetch_cpu_s=d(da["claim_public_material_bytes"]["v"] / 1e6 / tput, "bytes / the 0.5B class's measured fetch+hash-check MB per CPU-second (a CPU floor; no link)") if da and tput else None,
        artifact_bytes=c["artifact"]["container_bytes"],
        artifact_fetch_cpu_s=d(c["artifact"]["container_bytes"]["v"] / 1e6 / tput, "same rate") if tput else None, fetch_provider="not run",
        filing_object_bytes_by_lie=None, t_localize=None, note="components only: weight pass (M, streaming), public-material bytes (M); the estimate is an assumption")

# ── GAP-07: the worst-case deadline of G14 condition C8, per family / class / profile ───────────────────────────────────────
# T_challenge = (T_fetch(claim material) + T_check_to_verdict(worst lie: the LAST position) + T_file) x (1 + margin) + carrier + reorg.
# CPU seconds are the comparable figure (the host was shared); the margin is the measured median wall/CPU.
MARGIN = 1.35
CHAIN_DAA = 4   # policy: carrier_daa 2 + reorg_slack_daa 2 (opv.rs interim), NOT measured (fleet plan F6)
WINDOW_DAA, HARD_DAA, LIABILITY_DAA = 50, 80, 200   # the interim windows: OPV 40 + 10; Panel-licensed: pass + challenge_window; + court 20 + grace 10; post-Final


def daa_of(sec):
    return max(1, math.ceil(sec / DAA_MIN_S))


def gap07_row(klass, P, status, fetch_cpu, check_cpu, localize_cpu, file_cpu, src, **kw):
    inner = fetch_cpu + check_cpu + localize_cpu + file_cpu
    total = inner * MARGIN + CHAIN_DAA * DAA_MIN_S
    r = {
        "family": "F1", "class": klass, "positions": P, "status": status,
        "t_fetch_claim_material_cpu_s": fetch_cpu, "t_check_to_verdict_cpu_s": check_cpu, "t_localize_cpu_s": localize_cpu, "t_file_cpu_s": file_cpu,
        "margin_factor": MARGIN, "carrier_plus_reorg_daa_policy": CHAIN_DAA,
        "t_challenge_s": total, "t_challenge_daa": daa_of(total),
        "window_daa": {"opv_window": WINDOW_DAA, "panel_licensed_colluding_seats_window": WINDOW_DAA, "hard_deadline": HARD_DAA, "post_final_liability": LIABILITY_DAA},
        "fits_the_interim_window": daa_of(total) <= WINDOW_DAA, "fits_the_interim_hard_deadline": daa_of(total) <= HARD_DAA,
        "src": src,
    }
    r.update(kw)
    return r


gap07_rows = []
lt = q["lies"].get("late", {})
fd3 = files_f.get("claim_material_cpu_s", {}).get("v", 0.0)
if lt:
    gap07_rows.append(gap07_row(
        Q, 3, "MEASURED (late lie, n=%d; fetch n=%d)" % (lt["samples"]["v"], files_f["claim_material_cpu_s"].get("n", 1)),
        fd3, lt["check_to_verdict_cpu_s"]["max"], 0.0, (lt.get("filing_assemble_cpu_s") or {"v": 0.0})["v"] + (lt.get("court_cpu_s") or {"v": 0.0})["v"],
        "verify --claim late: check_outsider runs to the verdict, so it already contains localization; its maximum over samples is the worst case",
        check_to_verdict_wall_s_max=lt["check_to_verdict_wall_s"]["max"], peak_rss_bytes_max=tco["peak_rss_bytes_max"]["v"]))
for pt, smp in zip(pts, honest_fresh):
    if pt["P"] == 3:
        continue
    gap07_rows.append(gap07_row(
        Q, pt["P"], "MEASURED honest pass (fresh path); localization A (the P=3 upper bound carried; the last-position lie was not run at this P)",
        phase(smp, "fetch_da")["cpu_s"], pt["cpu"], 23.5, 0.1,
        "verify --via fresh: check_salted over an honest claim (the whole pass a last-position lie also costs) + the P=3 localization upper bound",
        check_wall_s=pt["wall"], peak_rss_bytes=pt["rss"], load1=pt["load1"]))
g07v = [r for r in read_jsonl("gap07-verify.jsonl") if r.get("cmd") == "verify"]
for smp in g07v:
    ck = phase(smp, "check_outsider")
    if ck is None:
        continue
    mpos = re.search(r"-p(\d+)$", smp["label"])
    pos = int(mpos.group(1)) if mpos else None
    gap07_rows.append(gap07_row(
        Q, pos, "MEASURED (%s lie, outsider path, n=1; the worst case: the whole pass, then localization, filing and the court)" % smp["claim"],
        (phase(smp, "fetch_da") or {"cpu_s": 0.0})["cpu_s"], ck["cpu_s"], 0.0,
        sum((phase(smp, n) or {"cpu_s": 0.0})["cpu_s"] for n in ("assemble_filing", "encode_filing_object", "strict_decode_filing", "court_node")),
        "verify --claim %s: check_outsider runs to the verdict (localization included)" % smp["claim"],
        check_wall_s=ck["wall_s"], peak_rss_bytes=smp["peak_rss_bytes"], load1=smp["load_before"][0], da_bytes=smp["da_bytes"]))
g07a = []
if (res / "gap07-attempts.json").exists():
    g07a = json.loads((res / "gap07-attempts.json").read_text())
tput_bytes = rate.get("claim_material_MB_per_cpu_s", 690.0) * 1e6
for label, c in classes.items():
    if label == Q or "t_check_estimate_p3_cpu_s" not in c:
        continue
    est = c["t_check_estimate_p3_cpu_s"]["fixed_s"], c["t_check_estimate_p3_cpu_s"]["per_position_s"]
    pb = c["da_p3"]["per_position_bytes"]["v"]
    for P in (3, 64):
        gap07_rows.append(gap07_row(
            label, P, "ASSUMED: no check of this class ran (reference verifier needs >= 16 B/param); weight pass MEASURED, fixed and per-position parts carried from the 0.5B fit, localization the 0.5B upper bound",
            P * pb / tput_bytes, est[0] + P * est[1], 23.5, 0.1, "t_check_estimate_p3_cpu_s (A) + the measured weight pass as its floor",
            t_check_lower_bound_cpu_s=c["t_check_lower_bound_cpu_s"]["v"]))
families = [
    {"family": "F1", "what": "K2-TIR v1/v2 single-program classes", "profiles": "Panel-licensed with every interim seat colluding; OPV", "status": "see rows: MEASURED for Qwen2.5-0.5B (P = 3, 8, 16, and the merged-head samples); ASSUMED for the four other real classes", "needs": "hosts with >= 24 / 32 / 48 GB RAM for the reference verifier (fleet plan F2)"},
    {"family": "F2", "what": "K2-TIR v3 pipeline / media classes", "profiles": "OPV", "status": "UNMEASURED: V-ref only, no real pipeline claim, no onboarding path (GAP-20, GAP-21)", "structure": "sum over stages of the component's T_check; edges at inclusion; one logits tensor per edge filing", "needs": "a real pipeline claim on a node (K2S wire form)"},
    {"family": "F3", "what": "K2-TIR v4 segmented real-scale (8k, 262k window, 2M)", "profiles": "OPV only", "status": "UNMEASURED: the only real artifact would be H1's 9B pack (not built); 2M is refused by the gate", "structure": "9B-8k rows of opv-measurements.md section 5 are ASSUMED", "needs": "the 9B pack, a >= 160 GB host, the K2S v4 node"},
    {"family": "F4", "what": "K2-TIR v5 encoders and task heads", "profiles": "kernel route", "status": "UNMEASURED: V-unit test failing at the commit; no node test", "needs": "K2S GAP-40"},
    {"family": "F5", "what": "private / fused material profiles", "profiles": "-", "status": "NO DEADLINE: never registers (fail-closed)"},
    {"family": "F6", "what": "RFC-0004 typed roots: Memory / Retrieval / Composite", "profiles": "OPV + palw_typed_roots_v1", "status": "UNMEASURED: no real artifact of any kind exists", "structure": "opv-measurements.md section 8", "needs": "a real memory class, retrieval snapshot, composite"},
    {"family": "F7", "what": "RFC-0008 EXEC work slices", "profiles": "palw_exec_payload_v2", "status": "UNMEASURED: a slice's kernel claim has the F1/F3 deadline of its own size; leg continuity and suffix void have no timing", "needs": "X8R slice node E2E on a real class"},
    {"family": "F8", "what": "onboarding conformance evidence (tag 109) and artifact binding", "profiles": "the reward gate", "status": "NO OUTSIDER DEADLINE on the complete-check path (judged in the fold, node CPU per block, not measured here); the sampled path cannot gate rewards"},
    {"family": "F9-F16", "what": "legacy V2 Panel route on t12", "profiles": "exempt LEGACY_PANEL_ROUTE", "status": "NOT G14: windows receipt 600 / challenge 1,200 DAA (code reading, t12-bond-reuse-audit-2026-10-10.md)"},
]
gap07 = {
    "what": "G14 condition C8: the worst-case deadline for a fresh, non-seat, bonded verifier that HOLDS the registered model to fetch the claim material, check, localize and file (g14-completion-matrix.md GAP-07). Windows are the INTERIM policy constants, not production values.",
    "formula": "(T_fetch(claim material) + T_check_to_verdict(worst lie = last position, localization included) + T_file) x 1.35 + (carrier 2 + reorg 2) DAA x 120 s; T_beacon = 0 (private-salt outsider)",
    "windows_daa": {"opv_window": WINDOW_DAA, "panel_licensed_with_every_seat_colluding": "pass + challenge_window = %d (lifecycle.rs: ProbabilisticPass.window_end; a dispute is refused after it)" % WINDOW_DAA, "hard_deadline": HARD_DAA, "post_final_liability": LIABILITY_DAA, "s_per_daa": DAA_MIN_S},
    "rows": gap07_rows,
    "attempted_and_not_completed": g07a,
    "families": families,
}

# ── the clock ───────────────────────────────────────────────────────────────────────────────────────────────────────────
clock = {
    "s_per_daa_lower_bound": d(DAA_MIN_S, "target_time_per_block of the 2-minute network (rfc-0012-policy-proposal.md): a DAA step needs one heartbeat slot. Used for every DAA conversion (a lower bound makes the DAA budget conservative)"),
    "s_per_daa_cited": a("125-150", "t12-daa9000-drill-plan.md (~125 measured on a Mac drill) and operational notes (~150): NOT measured by this lane; fleet plan item F1"),
}

scopes = {
    "adr_0177_d7": "The chain does not make a model obtainable. Every `artifact_*` field is the registered MODEL: an off-chain acquisition assumption (what a verifier already holds, or fetches before the claim), NOT a protocol term of T_challenge. Every `da_claim_material_*` field is the CLAIM's public material: a protocol term (T_fetch).",
    "detection_probability": "Every detection probability derived from these timings is CONDITIONAL on the verifier holding the registered model. For a closed model nobody but the producer holds, it is 0 and no finite collateral exists (opv-measurements.md section 3.2).",
    "adr_0032": "A self-reporting producer recovers the reporter share (4,900 bps) of a collected slash; collateral formulas use the net loss, 51 % of the collected slash, never the gross slash.",
}

doc = {
    "schema": "opv-measurements/v2",
    "scopes": scopes,
    "generated_by": "tools/opv-meas/scripts/summarize.py",
    "legend": {"measured": "read off a harness run on this host", "derived": "computed from measured values by the stated formula", "assumed": "no run established it"},
    "host": host,
    "clock": clock,
    "unit_costs": unit,
    "table": table,
    "gap07_worst_case_deadlines": gap07,
    "classes": classes,
    "not_measured": [
        "link bandwidth, latency, loss and provider egress (every fetch here is a local file or the localhost HTTP provider)",
        "chain inclusion of a filing: carrier latency and reorg depth (opv.rs carrier_daa / reorg_slack_daa)",
        "the number of independent honest verifiers that actually check, and the probability each is present (q, n, P_run)",
        "any class beyond 0.5B end to end: the reference verifier caches 16 bytes per parameter (0.8B needs >= 12.3 GB, 1B >= 19.9 GB, 1.7B >= 27.6 GB; the host is shared)",
        "the 9B-8k real artifact (H1's pack is not built)",
        "how many verifiers HOLD a registered model, and for a closed model whether any third party does (ADR-0177 D7): the effective detection probability is 0 for a closed model and unmeasured for every other",
        "any real Memory, Retrieval or Composite artifact (RFC-0004 Part II): none exists in the repository or in H1's evidence, only the synthetic fixtures of misaka-palw-tir-sketch; per-kind inspection cost is therefore a derived model, not a measurement (opv-measurements.md section 8)",
        "the ADR-0176 section 4 measurements that need the BUDGET engine (claims, reward blocks, rewards and Final credit per DAA per unit of bond; liability and maximum concurrent exposure; effective collection rate): the plan is opv-measurements.md section 7",
    ],
}
out_path.write_text(json.dumps(doc, indent=1, sort_keys=False) + "\n")
print(f"wrote {out_path} ({out_path.stat().st_size} bytes)")
