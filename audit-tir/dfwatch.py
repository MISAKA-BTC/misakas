#!/usr/bin/env python3
"""audit-tir/dfwatch.py — the D-F drill's sampler (read-only RPC to the drill's own loopback nodes).

Every 30 s, through one node (DF_PORT, new3's JSON port by default), for each IR class of the drill — D-F1
(ir-class.id: the Qwen2.5-A16 class, new0 its registrant and producer) and the small class (small-class.id:
the tiny HF llama D-F2's live battery runs on, new4 its registrant and producer) — the class's registry row
(getPalwModelRegistry: lifecycle state, ready seats), its class-context row (getPalwClassContexts: source must
read chain_ir_registration), and the class's claims of the producers' bonds (getPalwClaims, executor, terminal
included) counted by phase: Final, convicted by a court (CourtFraud, CourtDefault, CourtHeldVerdict — a
proof, a default at the court's clock, a held dissection's verdict), otherwise Voided, open.
Appends one line to $WORK_DIR/df.tsv and rewrites $WORK_DIR/df-state.json (D-F1's fields at the top level,
the small class's under "small"). Prints the first sample at which each milestone is reached (Candidate,
Prefetching, Probation, ActiveLimited, Active, the first Final after Active; the small class's with a
"small-" prefix, and its first conviction) to $WORK_DIR/df-milestones.tsv.

  dfwatch.py [--once] [--until prefetching|active|final|small-active|small-convicted]
Exit (with --until): 0 when reached, 3 INCOMPLETE when --deadline-daa passes first, 4 GUARDED (--until
prefetching only) when D-F1 sits at Candidate while the IR holders' logs name the interim F7 guard's
READINESS refusal (armed only by a kaspad whose PALW_TIR_GUARD_REFUSES_READINESS_V1 was set; off in the
flag-day release, and gone with the F7 node update)."""
import argparse, json, os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rpc import call, pick

WORK = os.path.expanduser(os.environ.get("WORK_DIR", "~/.misaka-palw-tir-drill"))
PORT = int(os.environ.get("DF_PORT", str(int(os.environ.get("JSON_BASE", "58100")) + 3)))
MILESTONES = ["Candidate", "Prefetching", "Probation", "ActiveLimited", "Active"]
# A claim a court convicted: the reasons the fold writes for a proof, a default and a held verdict.
CONVICTED = ("CourtFraud", "CourtDefault", "CourtHeldVerdict")
# The interim F7 guard's words (kaspad's PALW_TIR_DISSECTION_GUARD_WORDS_V1, flag-day release only) and the
# nodes that hold the IR artifacts (lib-df.sh's ir column).
GUARD_WORDS = "IR class with dissected points: this node cannot play the dissection yet"
IR_HOLDERS = os.environ.get("IR_HOLDERS", "new0 new1 new2 new3 new4 new5 new6").split()


def read_id(name):
    try:
        return open(f"{WORK}/{name}").read().strip() or None
    except OSError:
        return None


def bonds():
    m = json.load(open(f"{WORK}/keyring/manifest.json"))
    return {"new0": m["seats"][3]["bond_outpoint"], "new4": m["seats"][4]["bond_outpoint"]}


def guarded_holders(cid):
    """The IR holders whose log names the guard's READINESS refusal for this class (the panel's
    `readiness for class <cid>: no proof — <guard words> ...`)."""
    logs = [f"{WORK}/{n}/kaspad.out" for n in IR_HOLDERS if os.path.exists(f"{WORK}/{n}/kaspad.out")]
    if not logs:
        return []
    r = subprocess.run(["grep", "-lF", f"readiness for class {cid}: no proof — {GUARD_WORDS}", *logs], capture_output=True, text=True)
    return [os.path.basename(os.path.dirname(path)) for path in r.stdout.split()]


def class_sample(reg, ctx, claims_by_bond, cid):
    row = next((c for c in (pick(reg, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    crow = next((c for c in (pick(ctx, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    counts = {}
    for who, claims in claims_by_bond.items():
        c = {"final": 0, "convicted": 0, "voided": 0, "open": 0}
        for cl in claims:
            if pick(cl, "classId") not in (None, cid):
                continue
            phase = str(pick(cl, "phase", default=""))
            if phase.startswith("Final"):
                c["final"] += 1
            elif any(reason in phase for reason in CONVICTED):
                c["convicted"] += 1
            elif phase.startswith("Voided"):
                c["voided"] += 1
            else:
                c["open"] += 1
        counts[who] = c
    return {
        "state": str(pick(row, "state", default="absent")) if row else "absent",
        "ready": pick(row, "readySeats", "readySeatsNow", default=None) if row else None,
        "context_source": pick(crow, "source", default="absent") if crow else "absent",
        "claims": counts,
    }


def sample():
    dag = call(PORT, "getBlockDagInfo", {})
    daa = int(pick(dag, "virtualDaaScore", default=0) or 0)
    reg = call(PORT, "getPalwModelRegistry", {})
    ctx = call(PORT, "getPalwClassContexts", {})
    claims_by_bond = {
        who: pick(call(PORT, "getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0}), "claims", default=[])
        or []
        for who, bond in bonds().items()
    }
    s = {"t": time.strftime("%F %T"), "daa": daa}
    s.update(class_sample(reg, ctx, claims_by_bond, read_id("ir-class.id")))
    small = read_id("small-class.id")
    s["small"] = class_sample(reg, ctx, claims_by_bond, small) if small else None
    return s


def marks_of(state, prefix=""):
    marks = [prefix + m for m in MILESTONES if state.startswith(m)]
    if state.startswith("Active") and not state.startswith("ActiveLimited"):
        marks = [prefix + "Active"]
    return marks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--until", choices=["prefetching", "active", "final", "small-active", "small-convicted"])
    ap.add_argument("--deadline-daa", type=int, default=0)
    a = ap.parse_args()
    seen = set()
    if os.path.exists(f"{WORK}/df-milestones.tsv"):
        seen = {l.split("\t")[0] for l in open(f"{WORK}/df-milestones.tsv")}
    guard_samples = 0
    while True:
        s = sample()
        with open(f"{WORK}/df.tsv", "a") as f:
            small = s["small"] or {}
            f.write(
                f"{s['t']}\t{s['daa']}\t{s['state']}\t{s['ready']}\t{s['context_source']}\t{json.dumps(s['claims'])}"
                f"\t{small.get('state', '-')}\t{small.get('ready', '-')}\t{json.dumps(small.get('claims', {}))}\n"
            )
        json.dump(s, open(f"{WORK}/df-state.json", "w"), indent=1)
        marks = marks_of(s["state"])
        if s["claims"]["new0"]["final"] and "Active" in seen:
            marks.append("first-final-after-active")
        # Stage 1's registration milestone: Prefetching (the admission jury seated) with seats proving
        # readiness (ready > 0 on the row). A class already past it (Probation's gate is ready seats
        # enough) has met it too. Marked before the marks are recorded, or the goal is never seen.
        if (s["state"].startswith("Prefetching") and (s["ready"] or 0) > 0) or s["state"].startswith(("Probation", "Active")):
            marks.append("prefetching-ready")
        if s["small"]:
            marks += marks_of(s["small"]["state"], "small-")
            if s["small"]["claims"]["new4"]["convicted"]:
                marks.append("small-convicted")
        for m in marks:
            if m not in seen:
                seen.add(m)
                with open(f"{WORK}/df-milestones.tsv", "a") as f:
                    f.write(f"{m}\t{s['daa']}\t{s['t']}\n")
                print(f"{s['t']} DAA {s['daa']}: {m}", flush=True)
        goal = {
            "prefetching": "prefetching-ready",
            "active": "Active",
            "final": "first-final-after-active",
            "small-active": "small-Active",
            "small-convicted": "small-convicted",
        }.get(a.until or "")
        if a.once or (goal and goal in seen):
            return 0
        # The interim F7 guard: at Candidate with the holders refusing readiness by name, three samples
        # running, the jury cannot seat — say so rather than wait out the deadline.
        if goal == "prefetching-ready" and s["state"].startswith("Candidate"):
            held = guarded_holders(read_id("ir-class.id"))
            guard_samples = guard_samples + 1 if held else 0
            if guard_samples >= 3:
                print(f"GUARDED: DAA {s['daa']}: the class sits at Candidate; {len(held)} of {len(IR_HOLDERS)} IR holders "
                      f"refuse readiness by name ({' '.join(held)}): '{GUARD_WORDS}'", flush=True)
                with open(f"{WORK}/df-milestones.tsv", "a") as f:
                    f.write(f"guarded-at-candidate\t{s['daa']}\t{s['t']}\n")
                return 4
        if a.deadline_daa and s["daa"] >= a.deadline_daa:
            print(f"INCOMPLETE: DAA {s['daa']} passed {a.deadline_daa} before {goal}", flush=True)
            return 3
        time.sleep(30)


if __name__ == "__main__":
    sys.exit(main())
