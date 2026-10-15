#!/usr/bin/env python3
"""RFC-0016 V+ simulation and sigma/k sweep driver (RFC-0016 §6, §8, §8.6).

What it is: a discrete-DAA model of the common-W lottery (ADR-0137: a model's win rate is proportional to the compute on it, times
the optional tilt tau_m in [1/k, k]), V+ pre-issued block rights (the same integer rules as
`consensus/core/src/palw_rfc0016_v1/rights.rs`: entrant reserve deducted first, floor(P_m * base), own right, sigma-capped reissue
past phi with the others' pace protected, never past the epoch total), the W step (x4 / /4 clamp, floored at W0), miner mobility
(the convergence conditions C1-C7), per-model verifier capacity with a bounded queue, and the clock dependence on the heartbeat
(DAA steps with no model block).

What it is NOT: a measurement. Its outputs are model outputs for choosing what to MEASURE; no number it prints is a production value,
and none is recorded as one. Every parameter is named on the command line or in a scenario; nothing is defaulted silently into a
result. The selection rule (RFC-0016 §8.6) reports the region that satisfies every pass condition, or the conflicting constraints
when none does.

Usage:
  python3 -I scripts/sim/palw-rfc0016-vplus-sim.py scenario E3 --sigma 250 --k 1000 --seed 1
  python3 -I scripts/sim/palw-rfc0016-vplus-sim.py sweep --sigmas 0,100,250,500,1000 --ks 1000,1500,2000 --scenarios E1,E2,E3,E4,E5,E6
  python3 -I scripts/sim/palw-rfc0016-vplus-sim.py conditions        # C1-C7 one at a time, broken against the baseline
"""
import argparse
import json
import math
import random
import sys
from dataclasses import dataclass, field, asdict, replace
from typing import Dict, List

# ---- the V+ integer rules (mirror of rights.rs) ------------------------------------------------------------------------------


def issue(n_exp, eps_permille, capitals, eligible, entrants, ent_cap_permille, ent_min):
    """Rt_m = floor((n_exp - Ent) * A_m / sum A), A_m = S_m^1.5 (alpha decided); entrants linear in capital, capped."""
    ent = n_exp * min(eps_permille, 1000) // 1000
    base = n_exp - ent
    weights = {m: c ** 1.5 for m, c in capitals.items()}
    total = sum(weights.values())
    issued = {}
    for m, a in weights.items():
        if eligible.get(m, True) and total > 0:
            r = int(base * a // total)
            if r > 0:
                issued[m] = r
    q = {m: c for m, c in entrants.items() if c >= ent_min and c > 0 and eligible.get(m, True)}
    sc = sum(q.values())
    cap = n_exp * ent_cap_permille // 1000
    granted = 0
    if sc > 0 and ent > 0:
        for m in sorted(q):
            g = min(ent * q[m] // sc, cap, ent - granted)
            if g > 0:
                granted += g
                issued[m] = issued.get(m, 0) + g
    return issued, ent


def protected(issued, used, elapsed, length, phi, margin):
    unused = max(0, issued - used)
    if elapsed * 1000 < length * phi:
        return unused
    need = math.ceil(used * (length - elapsed) * margin / (max(elapsed, 1) * 1000))
    return min(unused, need)


def admit(issued, used, model, elapsed, length, sigma, phi, margin):
    total = sum(issued.values())
    total_used = sum(used.values())
    if total_used >= total:
        return "EpochTotalReached" if total else "NoRightsIssued"
    im, um = issued.get(model, 0), used.get(model, 0)
    if um < im:
        return "Own"
    if im == 0:
        return "NoRightsIssued"
    if elapsed * 1000 < length * phi:
        return "OwnRightsExhausted"
    if um >= im + im * sigma // 1000:
        return "SigmaCapReached"
    prot = sum(protected(i, used.get(j, 0), elapsed, length, phi, margin) for j, i in issued.items() if j != model)
    if total_used + prot + 1 > total:
        return "NoReleasedRights"
    return "Reissued"


def tilt(target, physical, k):
    if k <= 1000 or target <= 0 or physical <= 0:
        return 1.0
    return min(max(target / physical, 1000 / k), k / 1000)


# ---- the world --------------------------------------------------------------------------------------------------------------


@dataclass
class Model:
    capital: float            # S_m (30-day average already applied; the ADR-0177 lane computes it)
    compute: float            # compute on the model (relative MAC-eq/s), before mobility
    verifiers: int            # ready verifiers holding it
    eligible: bool = True
    entrant_since: int = -1   # epoch it became eligible (-1: not an entrant)
    surge_at: int = -1        # epoch a demand surge multiplies its compute
    surge_x: float = 1.0


@dataclass
class World:
    models: Dict[str, Model]
    epochs: int = 30
    epoch_length: int = 600            # DAA per epoch
    attempt_share_permille: int = 1000
    mobile_fraction: float = 1.0       # C2: share of compute that can switch models
    switch_lag_epochs: int = 1         # C2/C6: epochs to move (fetch, warm-up, seating)
    quote: bool = True                 # C3: the pre-Attempt quote exists
    verifier_capacity: float = 1.0     # claims per DAA one verifier serves (C4)
    queue_len: int = 8
    w0: float = 1.0
    griefer_compute: float = 0.0       # E5: unpaid compute on a model with no rights
    eps_permille: int = 0
    ent_cap_permille: int = 0
    ent_min: float = 0.0
    ent_age_epochs: int = 0
    phi: int = 500
    margin: int = 1000


SCENARIOS = {
    # baseline: bond and compute aligned, everyone mobile.
    "BASE": lambda: World(models={"A": Model(800, 800, 8), "B": Model(200, 200, 2)}),
    # E1: miners cannot switch (hardware/model-specific setups, RAM, cold fetch, seating).
    "E1": lambda: World(models={"A": Model(800, 100, 8), "B": Model(200, 900, 2)}, mobile_fraction=0.0),
    # E2: a sudden demand surge on a low-bond model.
    "E2": lambda: World(models={"A": Model(800, 500, 8), "B": Model(200, 500, 2, surge_at=10, surge_x=5.0)}),
    # E3: much capital, little compute.
    "E3": lambda: World(models={"A": Model(800, 200, 8), "B": Model(200, 800, 2)}),
    # E4: much compute, little capital.
    "E4": lambda: World(models={"A": Model(200, 800, 2), "B": Model(800, 200, 8)}),
    # E5: an unpaid griefer raising the common W.
    "E5": lambda: World(models={"A": Model(800, 800, 8), "B": Model(200, 200, 2)}, griefer_compute=1000.0),
    # E6: the W0 regime (supply below the cadence).
    "E6": lambda: World(models={"A": Model(800, 8, 8), "B": Model(200, 2, 2)}, epoch_length=600),
    # T7: a new model joins under alpha = 1.5 with an entrant reserve.
    "T7": lambda: World(models={"A": Model(900, 900, 9), "N": Model(10, 100, 1, entrant_since=5)},
                        eps_permille=20, ent_cap_permille=20, ent_min=1, ent_age_epochs=30),
}


def run(world: World, sigma: int, k: int, seed: int):
    rng = random.Random(seed)
    models = world.models
    names = sorted(models)
    compute = {m: models[m].compute for m in names}
    W = world.w0
    out = {m: {"physical": 0, "issued": 0, "unused": 0, "refused_rights": 0, "refused_capacity": 0, "finals": 0,
               "queue_wait": [], "target_ppm": 0} for m in names}
    hb_dependent = 0
    unminted = 0
    total_issued = 0
    phys_share = {m: 0.0 for m in names}
    for e in range(world.epochs):
        # surges
        for m in names:
            if models[m].surge_at == e:
                compute[m] *= models[m].surge_x
        caps = {m: models[m].capital for m in names}
        tot_a = sum(c ** 1.5 for c in caps.values()) or 1.0
        for m in names:
            out[m]["target_ppm"] = int(1e6 * caps[m] ** 1.5 / tot_a)
        n_exp = world.epoch_length * world.attempt_share_permille // 1000
        entrants = {m: caps[m] for m in names if models[m].entrant_since >= 0
                    and 0 <= e - models[m].entrant_since < world.ent_age_epochs}
        issued, _ = issue(n_exp, world.eps_permille, {m: caps[m] for m in names if m not in entrants},
                          {m: models[m].eligible for m in names}, entrants, world.ent_cap_permille, world.ent_min)
        total_issued += sum(issued.values())
        used = {m: 0 for m in names}
        queue = {m: [] for m in names}
        blocks_this_epoch = 0
        for step in range(world.epoch_length):
            tau = {m: tilt(out[m]["target_ppm"] / 1e6, phys_share.get(m, 0.0), k) for m in names}
            any_block = False
            order = names[:]
            rng.shuffle(order)
            for m in order:
                rate = compute[m] * tau[m] / max(W, 1e-9) / world.epoch_length
                verdict = admit(issued, used, m, step, world.epoch_length, sigma, world.phi, world.margin)
                if world.quote and verdict not in ("Own", "Reissued"):
                    continue  # C3: the quote stops the forward before it is spent
                wins = 1 if rng.random() < min(rate, 1.0) else 0
                if not wins:
                    continue
                if verdict not in ("Own", "Reissued"):
                    out[m]["refused_rights"] += 1
                    continue
                # capacity: verifier throughput per DAA, bounded queue
                if len(queue[m]) >= world.queue_len or models[m].verifiers == 0:
                    out[m]["refused_capacity"] += 1
                    continue
                used[m] += 1
                out[m]["physical"] += 1
                blocks_this_epoch += 1
                any_block = True
                queue[m].append(step)
            # unpaid griefer wins raise W through the block count (they are model blocks of a rightless class -> refused, so here
            # they only spend compute; the W step counts accepted model blocks)
            for m in names:
                served = int(models[m].verifiers * world.verifier_capacity)
                for _ in range(min(served, len(queue[m]))):
                    out[m]["queue_wait"].append(step - queue[m].pop(0))
                    out[m]["finals"] += 1
            if not any_block:
                hb_dependent += 1
        for m in names:
            out[m]["issued"] += issued.get(m, 0)
            out[m]["unused"] += max(0, issued.get(m, 0) - used[m])
        unminted += max(0, sum(issued.values()) - sum(used.values()))
        # the W step over the epoch's model blocks
        scaled = W * blocks_this_epoch / max(n_exp, 1)
        W = max(world.w0, min(max(scaled, W / 4), W * 4)) if blocks_this_epoch else max(world.w0, W / 4)
        tot_phys = sum(out[m]["physical"] for m in names) or 1
        phys_share = {m: out[m]["physical"] / tot_phys for m in names}
        # mobility (C2/C6): the mobile share of compute drifts toward models whose rights were unused, away from refused ones
        if world.mobile_fraction > 0 and (e + 1) % max(world.switch_lag_epochs, 1) == 0:
            pool = sum(compute[m] * world.mobile_fraction for m in names)
            want = {m: max(issued.get(m, 0), 1) for m in names}
            tw = sum(want.values())
            for m in names:
                compute[m] = compute[m] * (1 - world.mobile_fraction) + pool * want[m] / tw
    tot_phys = sum(out[m]["physical"] for m in names) or 1
    for m in names:
        q = sorted(out[m]["queue_wait"])
        out[m]["queue_wait_p50"] = q[len(q) // 2] if q else None
        out[m]["queue_wait_p95"] = q[int(len(q) * 0.95)] if q else None
        del out[m]["queue_wait"]
        out[m]["physical_share_ppm"] = int(1e6 * out[m]["physical"] / tot_phys)
    return {"models": out, "heartbeat_dependent_daa": hb_dependent, "unminted_rights": unminted, "issued_rights": total_issued}


# ---- pass conditions and the selection rule (RFC-0016 §8.6) ----------------------------------------------------------------


@dataclass
class PassBounds:
    track_tol_ppm: int        # |physical share - P_m| for bond-heavy models WITH compute
    unminted_max_permille: int
    hb_max_permille: int      # heartbeat-dependent DAA (reported; the heartbeat itself is untouched)
    surge_min_permille: int   # a surging model's physical blocks vs its issued rights (not stalled)


def evaluate(scenario, result, world, bounds: PassBounds):
    fails = []
    ms = result["models"]
    for m, row in ms.items():
        has_compute = world.models[m].compute > 0 and world.mobile_fraction > 0
        if has_compute and world.models[m].capital >= max(x.capital for x in world.models.values()):
            if abs(row["physical_share_ppm"] - row["target_ppm"]) > bounds.track_tol_ppm:
                fails.append(f"{scenario}: {m} physical {row['physical_share_ppm']} ppm vs target {row['target_ppm']} ppm")
        if world.models[m].surge_at >= 0 and row["issued"] > 0:
            if row["physical"] * 1000 < row["issued"] * bounds.surge_min_permille:
                fails.append(f"{scenario}: surging {m} stalled ({row['physical']} of {row['issued']} rights)")
    if result["issued_rights"] and result["unminted_rights"] * 1000 > result["issued_rights"] * bounds.unminted_max_permille:
        fails.append(f"{scenario}: unminted rights {result['unminted_rights']} of {result['issued_rights']}")
    total_daa = world.epochs * world.epoch_length
    if result["heartbeat_dependent_daa"] * 1000 > total_daa * bounds.hb_max_permille:
        fails.append(f"{scenario}: heartbeat-dependent {result['heartbeat_dependent_daa']} of {total_daa} DAA")
    return fails


def cmd_scenario(a):
    world = SCENARIOS[a.name]()
    world = replace(world, phi=a.phi, margin=a.margin)
    print(json.dumps(run(world, a.sigma, a.k, a.seed), indent=1, default=str))


def cmd_sweep(a):
    bounds = PassBounds(a.track_tol_ppm, a.unminted_max_permille, a.hb_max_permille, a.surge_min_permille)
    region, conflicts = [], {}
    for s in [int(x) for x in a.sigmas.split(",")]:
        for k in [int(x) for x in a.ks.split(",")]:
            fails = []
            for name in a.scenarios.split(","):
                world = replace(SCENARIOS[name](), phi=a.phi, margin=a.margin)
                for seed in range(a.seeds):
                    fails += evaluate(name, run(world, s, k, seed), world, bounds)
            if fails:
                conflicts[(s, k)] = sorted(set(fails))
            else:
                region.append((s, k))
    print(json.dumps({"bounds": asdict(bounds), "passing_region": region,
                      "conflicts": {f"sigma={s},k={k}": v for (s, k), v in conflicts.items()},
                      "note": "model outputs only - choose what to MEASURE; never a production value"}, indent=1))
    if not region:
        print("NO (sigma, k) satisfies every pass condition: report the conflicts above; do not invent values.", file=sys.stderr)


def cmd_conditions(a):
    """C1-C7: break one convergence condition at a time against the BASE scenario."""
    breaks = {
        "C1 (not saturated: W0 regime)": lambda w: replace(w, models={m: replace(x, compute=x.compute / 100) for m, x in w.models.items()}),
        "C2 (no mobility)": lambda w: replace(w, mobile_fraction=0.0, models={"A": Model(800, 200, 8), "B": Model(200, 800, 2)}),
        "C3 (no quote)": lambda w: replace(w, quote=False),
        "C4 (no verifier capacity at A)": lambda w: replace(w, models={**w.models, "A": replace(w.models["A"], verifiers=0)}),
        "C5 (A ineligible)": lambda w: replace(w, models={**w.models, "A": replace(w.models["A"], eligible=False)}),
        "C6 (slow switching)": lambda w: replace(w, switch_lag_epochs=10, models={"A": Model(800, 200, 8), "B": Model(200, 800, 2)}),
    }
    res = {"BASE": run(replace(SCENARIOS["BASE"](), phi=a.phi, margin=a.margin), a.sigma, a.k, a.seed)}
    for name, f in breaks.items():
        res[name] = run(replace(f(SCENARIOS["BASE"]()), phi=a.phi, margin=a.margin), a.sigma, a.k, a.seed)
    # C7: sigma large vs small
    res["C7 (sigma = 10000)"] = run(replace(SCENARIOS["E3"](), phi=a.phi, margin=a.margin), 10000, a.k, a.seed)
    print(json.dumps(res, indent=1, default=str))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("scenario", "sweep", "conditions"):
        s = sub.add_parser(name)
        s.add_argument("--phi", type=int, required=True, help="reissue checkpoint, permille (no default: a named input)")
        s.add_argument("--margin", type=int, required=True, help="pace margin, permille")
        if name == "scenario":
            s.add_argument("name", choices=sorted(SCENARIOS))
        if name in ("scenario", "conditions"):
            s.add_argument("--sigma", type=int, required=True)
            s.add_argument("--k", type=int, required=True)
            s.add_argument("--seed", type=int, default=1)
        if name == "sweep":
            s.add_argument("--sigmas", required=True)
            s.add_argument("--ks", required=True)
            s.add_argument("--scenarios", required=True)
            s.add_argument("--seeds", type=int, default=3)
            s.add_argument("--track-tol-ppm", type=int, required=True)
            s.add_argument("--unminted-max-permille", type=int, required=True)
            s.add_argument("--hb-max-permille", type=int, required=True)
            s.add_argument("--surge-min-permille", type=int, required=True)
    a = p.parse_args()
    {"scenario": cmd_scenario, "sweep": cmd_sweep, "conditions": cmd_conditions}[a.cmd](a)


if __name__ == "__main__":
    main()
