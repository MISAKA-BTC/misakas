#!/usr/bin/env python3
"""The level a run reached, from its evidence alone (nothing is assumed): writes g14-gates.json, summary.md and reproduction.md.

    summarize.py <evidence dir> <run id> <model id>

Levels (each needs every lower one):
  L0 source      the pinned revision is readable on the hub (and a local checkpoint matches it, when one is used)
  L1 shape       header-only preflight at the declared context: convert and register ok
  L2 artifact    full checkpoint -> artifact -> pack verify --strict --rebuild VERIFIED (nothing failed, nothing skipped)
  L3 registered  the client U's own signed registration folded on the devnet
  L4 replayed    A, B and C agree on the class row, the registry and the state proof at one pinned block, after a restart of B and on a
                 fresh node Z that joined by IBD
  L5 eligible    conformance + DA + G14 hold ON THE CHAIN — this build has no chain state for any of the three (see g14-gates.json)
  L6 useful work a real claim of the class executed, verified, Final, reward attributed
"""
import json
import os
import sys

EV, RUN, MID = sys.argv[1:4]


def rd(name):
    p = os.path.join(EV, name)
    try:
        return json.load(open(p))
    except Exception:
        return None


model = rd("model.json") or {}
env = rd("environment.json") or {}
pf = rd("preflight.json") or {}
conv = rd("conversion.json") or {}
pv = rd("pack-verification.json") or {}
reg = rd("registration.json") or {}
cs = rd("consensus-state.json") or {}
fails = rd("failures.json") or []
src = model.get("source") or {}

lv = {}
lv["L0"] = bool(src) and src.get("resolved_sha") == model.get("revision") and (not model.get("local") or (src.get("local") or {}).get("verdict") == "MATCH")
dec = (pf.get("declared") or {}).get("verdict") or {}
lv["L1"] = lv["L0"] and dec.get("convert") == "ok" and dec.get("register") == "ok"
lv["L2"] = lv["L1"] and pv.get("exit") == 0 and pv.get("verified") is True and not pv.get("skipped") and not pv.get("failed")
lv["L3"] = lv["L2"] and bool(((reg.get("status_at_fold") or {}).get("registration") or {}).get("folded"))
checks = cs.get("checks") or {}
lv["L4"] = lv["L3"] and bool(cs.get("all_agree")) and all(checks.get(k) is True for k in ("restart_B_agrees", "fresh_Z_ibd_agrees"))
bc = pv.get("beacon_conformance") or {}
gates = {
    "schema": "misaka.h1.g14-gates.v1",
    "statement": "What the chain can say about this class's eligibility. A gate the chain has no state for is GAP, never PASS.",
    "static_admission": {"status": "PASS" if lv["L3"] else ("NOT_RUN" if not lv["L2"] else "FAIL"),
                         "source": "the node's own gate admitted the IR registration (palw_tir_registration_preflight_at_v1) and the fold kept it"},
    "kernel_route": {"status": "KERNEL_NOT_ACTIVE", "detail": ((pf.get("declared") or {}).get("kernel") or {}),
                     "source": "preflight: K2-TIR-v1 shipped KERNEL_NOT_ACTIVE; the kernel route is not folded by this build's node"},
    "beacon_conformance": {"status": "SYNTHETIC_ONLY" if bc.get("label") == "SYNTHETIC_BEACON_CONFORMANCE_PASS" else ("NOT_RUN" if not bc else "FAIL"),
                           "label": bc.get("label"), "chain_state": "GAP: no on-chain ConformanceCommitmentV1, no beacon-facts RPC (BEACON_UNAVAILABLE)"},
    "da": {"status": "GAP", "detail": "no artifact-holding seats were run for this class; readiness/possession is lifecycle, not G14 DA"},
    "public_prosecution_complete": {"status": "GAP", "detail": "G14_INCOMPLETE: the lifecycle step into Prefetching/Probation/Active consults seats/readiness/panel only (registration-e2e-record GAP-6); no plan/kernel binding in the class id (GAP-7)"},
    "registry_lifecycle": {"state": ((cs.get("nodes") or {}).get("A") or {}).get("registry_row", {}) and ((cs.get("nodes") or {}).get("A") or {}).get("registry_row", {}).get("state"),
                           "class_status": (((cs.get("nodes") or {}).get("A") or {}).get("class_row") or {}).get("status"),
                           "note": "a registered class is not eligible; RegisteredDormant/Candidate is never L5"},
}
lv["L5"] = False
lv["L6"] = False
json.dump(gates, open(os.path.join(EV, "g14-gates.json"), "w"), indent=1)
reached = max([k for k, v in lv.items() if v], default="none", key=lambda k: int(k[1:]))
blockers = [f"{f['category']} {f['code']} ({f['stage']}, owner {f['owner']})" for f in fails]
next_gate = {"none": "L0 source", "L0": "L1 shape", "L1": "L2 artifact", "L2": "L3 registration", "L3": "L4 replay",
             "L4": "L5 (G14_INCOMPLETE + BEACON_UNAVAILABLE + DA: no chain state)"}.get(reached, "-")
lines = [
    f"# {RUN} — {model.get('repo')}@{(model.get('revision') or '')[:12]}",
    "",
    f"* model: `{model.get('repo')}` revision `{model.get('revision')}`, family {model.get('family')}, task {model.get('task')}, context {model.get('context')}",
    f"* tested integration SHA: `{env.get('integration_sha')}` (branch `{env.get('branch')}`, dirty={env.get('dirty')})",
    f"* binaries: " + ", ".join(f"{k} `{v['sha256'][:16]}…`" for k, v in (env.get("binaries") or {}).items()),
    f"* devnet: salted testnet-12 drill genesis `{(env.get('genesis_hash') or '')[:16]}…`, params `{(env.get('consensus_params_id') or '')[:16]}…`, fences {env.get('fences')}",
    "",
    "| level | reached |", "|---|---|",
] + [f"| {k} | {'PASS' if v else ('GAP' if k in ('L5', 'L6') else 'no')} |" for k, v in lv.items()] + [
    "",
    f"**Highest level: {reached}.** Next gate: {next_gate}.",
    "",
    "## Numbers",
    f"* artifact: {conv.get('artifact_bytes')} B, sha256 `{(conv.get('artifact_sha256') or '')[:16]}…`, pack `{(conv.get('pack_digest') or '')[:16]}…`, cache key `{conv.get('cache_key')}` (clean-source run: {conv.get('clean_source')})",
    f"* pack verify (strict, rebuild): exit {pv.get('exit')}, verified {pv.get('verified')}, failed {[c.get('check') for c in pv.get('failed') or []]}, skipped {[c.get('check') for c in pv.get('skipped') or []]}",
    f"* beacon conformance: {bc.get('label', 'NOT_RUN')} (facts SYNTHETIC; policy UNAPPROVED)",
    f"* registration: class `{(reg.get('class_id') or '')[:16]}…`, root `{(reg.get('artifact_root') or '')[:16]}…`, owner `{(reg.get('owner_bond') or '')[:20]}…`, carrier `{(reg.get('carrier_txid') or '')[:16]}…`, U spent {(reg.get('u_balance_sompi') or {}).get('spent')} sompi",
    f"* consensus state: all agree {cs.get('all_agree')}, checks {checks}",
    "",
    "## Blockers / failures (failures.json)",
] + ([f"* {b}" for b in blockers] or ["* none recorded"]) + [""]
open(os.path.join(EV, "summary.md"), "w").write("\n".join(lines))
repro = f"""# Reproduce {RUN}

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN={RUN} RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {env.get('fences')}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source {MID}
bash tests/hf-onboarding/onboard.sh preflight {MID}
bash tests/hf-onboarding/onboard.sh artifact {MID}        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance {MID}
bash tests/hf-onboarding/onboard.sh register {MID}
bash tests/hf-onboarding/onboard.sh observe {MID}
bash tests/hf-onboarding/onboard.sh summary {MID}
```
Model pin: `{model.get('repo')}@{model.get('revision')}` (models.json). Binary hashes: environment.json.
"""
open(os.path.join(EV, "reproduction.md"), "w").write(repro)
print(json.dumps({"reached": reached, "levels": lv}))
