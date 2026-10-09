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

# ── the clock ───────────────────────────────────────────────────────────────────────────────────────────────────────────
clock = {
    "s_per_daa_lower_bound": d(DAA_MIN_S, "target_time_per_block of the 2-minute network (rfc-0012-policy-proposal.md): a DAA step needs one heartbeat slot. Used for every DAA conversion (a lower bound makes the DAA budget conservative)"),
    "s_per_daa_cited": a("125-150", "t12-daa9000-drill-plan.md (~125 measured on a Mac drill) and operational notes (~150): NOT measured by this lane; fleet plan item F1"),
}

doc = {
    "schema": "opv-measurements/v1",
    "generated_by": "tools/opv-meas/scripts/summarize.py",
    "legend": {"measured": "read off a harness run on this host", "derived": "computed from measured values by the stated formula", "assumed": "no run established it"},
    "host": host,
    "clock": clock,
    "unit_costs": unit,
    "classes": classes,
    "not_measured": [
        "link bandwidth, latency, loss and provider egress (every fetch here is a local file or the localhost HTTP provider)",
        "chain inclusion of a filing: carrier latency and reorg depth (opv.rs carrier_daa / reorg_slack_daa)",
        "the number of independent honest verifiers that actually check, and the probability each is present (q, n, P_run)",
        "any class beyond 0.5B end to end: the reference verifier caches 16 bytes per parameter (0.8B needs >= 12.3 GB, 1B >= 19.9 GB, 1.7B >= 27.6 GB; the host is shared)",
        "the 9B-8k real artifact (H1's pack is not built)",
    ],
}
out_path.write_text(json.dumps(doc, indent=1, sort_keys=False) + "\n")
print(f"wrote {out_path} ({out_path.stat().st_size} bytes)")
