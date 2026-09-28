#!/usr/bin/env python3
"""audit-tir/dfwatch.py — the D-F drill's sampler (read-only RPC to the drill's own loopback nodes).

Every 30 s, through one node (DF_PORT, new3's JSON port by default):
  the IR class's registry row (getPalwModelRegistry: lifecycle state, ready seats), its class-context row
  (getPalwClassContexts: source must read chain_ir_registration), and the IR claims of the IR producers'
  bonds (getPalwClaims, executor, terminal included) counted by phase — Final, Voided (and why), open.
Appends one line to $WORK_DIR/df.tsv and rewrites $WORK_DIR/df-state.json. Prints the first sample at which
each milestone is reached (Candidate, Prefetching, Probation, ActiveLimited, Active, the first Final after
Active, and for D-F2 the first CourtFraud of the tampering producer) to $WORK_DIR/df-milestones.tsv.

  dfwatch.py [--once] [--until prefetching|active|final|df2]
Exit (with --until): 0 when reached, 3 INCOMPLETE when --deadline-daa passes first, 4 GUARDED (--until
prefetching only) when the class sits at Candidate while the IR holders' logs name the interim F7 guard's
READINESS refusal (armed only by kaspad's PALW_TIR_GUARD_REFUSES_READINESS_V1, off in the flag-day release)."""
import argparse, json, os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rpc import call, pick

WORK = os.path.expanduser(os.environ.get("WORK_DIR", "~/.misaka-palw-tir-drill"))
PORT = int(os.environ.get("DF_PORT", str(int(os.environ.get("JSON_BASE", "58100")) + 3)))
MILESTONES = ["Candidate", "Prefetching", "Probation", "ActiveLimited", "Active"]
# The interim F7 guard's words (kaspad's PALW_TIR_DISSECTION_GUARD_WORDS_V1) and the nodes that hold the IR
# artifact (lib-df.sh's ir column).
GUARD_WORDS = "IR class with dissected points: this node cannot play the dissection yet"
IR_HOLDERS = os.environ.get("IR_HOLDERS", "new0 new1 new2 new3 new4 new5 new6").split()


def guarded_holders(cid):
    """The IR holders whose log names the guard's READINESS refusal for this class (the panel's
    `readiness for class <cid>: no proof — <guard words> ...`). The production half's hold (new0's
    producer, always on until the F7 node update) is not this: it does not keep the class at Candidate."""
    logs = [f"{WORK}/{n}/kaspad.out" for n in IR_HOLDERS if os.path.exists(f"{WORK}/{n}/kaspad.out")]
    if not logs:
        return []
    r = subprocess.run(["grep", "-lF", f"readiness for class {cid}: no proof — {GUARD_WORDS}", *logs], capture_output=True, text=True)
    return [os.path.basename(os.path.dirname(path)) for path in r.stdout.split()]


def class_id():
    return open(f"{WORK}/ir-class.id").read().strip()


def bonds():
    m = json.load(open(f"{WORK}/keyring/manifest.json"))
    return {"new0": m["seats"][3]["bond_outpoint"], "new4": m["seats"][4]["bond_outpoint"]}


def sample(cid):
    dag = call(PORT, "getBlockDagInfo", {})
    daa = int(pick(dag, "virtualDaaScore", default=0) or 0)
    reg = call(PORT, "getPalwModelRegistry", {})
    row = next((c for c in (pick(reg, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    state = str(pick(row, "state", default="absent")) if row else "absent"
    ready = pick(row, "readySeats", "readySeatsNow", default=None) if row else None
    ctx = call(PORT, "getPalwClassContexts", {})
    crow = next((c for c in (pick(ctx, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    source = pick(crow, "source", default="absent") if crow else "absent"
    counts = {}
    for who, bond in bonds().items():
        r = call(PORT, "getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0})
        c = {"final": 0, "court_fraud": 0, "voided": 0, "open": 0}
        for cl in pick(r, "claims", default=[]) or []:
            if pick(cl, "classId") not in (None, cid):
                continue
            phase = str(pick(cl, "phase", default=""))
            if phase.startswith("Final"):
                c["final"] += 1
            elif "CourtFraud" in phase:
                c["court_fraud"] += 1
            elif phase.startswith("Voided"):
                c["voided"] += 1
            else:
                c["open"] += 1
        counts[who] = c
    return {"t": time.strftime("%F %T"), "daa": daa, "state": state, "ready": ready, "context_source": source, "claims": counts}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--until", choices=["prefetching", "active", "final", "df2"])
    ap.add_argument("--deadline-daa", type=int, default=0)
    a = ap.parse_args()
    seen = set()
    if os.path.exists(f"{WORK}/df-milestones.tsv"):
        seen = {l.split("\t")[0] for l in open(f"{WORK}/df-milestones.tsv")}
    active_at = None
    guard_samples = 0
    while True:
        s = sample(class_id())
        with open(f"{WORK}/df.tsv", "a") as f:
            f.write(f"{s['t']}\t{s['daa']}\t{s['state']}\t{s['ready']}\t{s['context_source']}\t{json.dumps(s['claims'])}\n")
        json.dump(s, open(f"{WORK}/df-state.json", "w"), indent=1)
        marks = [m for m in MILESTONES if s["state"].startswith(m)]
        if s["state"].startswith("Active") and not s["state"].startswith("ActiveLimited"):
            marks = ["Active"]
            active_at = active_at or s["daa"]
        if s["claims"]["new0"]["final"] and "Active" in seen:
            marks.append("first-final-after-active")
        if s["claims"]["new0"]["court_fraud"]:
            marks.append("court-fraud")
        # Stage 1's registration milestone: Prefetching (the admission jury seated) with seats proving
        # readiness (ready > 0 on the row). A class already past it (Probation's gate is ready seats
        # enough) has met it too. Marked before the marks are recorded, or the goal is never seen.
        if (s["state"].startswith("Prefetching") and (s["ready"] or 0) > 0) or s["state"].startswith(("Probation", "Active")):
            marks.append("prefetching-ready")
        for m in marks:
            if m not in seen:
                seen.add(m)
                with open(f"{WORK}/df-milestones.tsv", "a") as f:
                    f.write(f"{m}\t{s['daa']}\t{s['t']}\n")
                print(f"{s['t']} DAA {s['daa']}: {m}", flush=True)
        goal = {"prefetching": "prefetching-ready", "active": "Active", "final": "first-final-after-active", "df2": "court-fraud"}.get(
            a.until or ""
        )
        if a.once or (goal and goal in seen):
            return 0
        # The interim F7 guard: at Candidate with the holders refusing readiness by name, three samples
        # running, the jury cannot seat — say so rather than wait out the deadline.
        if goal == "prefetching-ready" and s["state"].startswith("Candidate"):
            held = guarded_holders(class_id())
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
