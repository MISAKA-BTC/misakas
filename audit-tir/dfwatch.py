#!/usr/bin/env python3
"""audit-tir/dfwatch.py — the D-F drill's sampler (read-only RPC to the drill's own loopback nodes).

Every 30 s, through one node (DF_PORT, new3's JSON port by default):
  the IR class's registry row (getPalwModelRegistry: lifecycle state, ready seats), its class-context row
  (getPalwClassContexts: source must read chain_ir_registration), and the IR claims of the IR producers'
  bonds (getPalwClaims, executor, terminal included) counted by phase — Final, Voided (and why), open.
Appends one line to $WORK_DIR/df.tsv and rewrites $WORK_DIR/df-state.json. Prints the first sample at which
each milestone is reached (Candidate, Prefetching, Probation, ActiveLimited, Active, the first Final after
Active, and for D-F2 the first CourtFraud of the tampering producer) to $WORK_DIR/df-milestones.tsv.

  dfwatch.py [--once] [--until active|final|df2]
Exit (with --until): 0 when reached, 3 INCOMPLETE when --deadline-daa passes first."""
import argparse, json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rpc import call, pick

WORK = os.path.expanduser(os.environ.get("WORK_DIR", "~/.misaka-palw-tir-drill"))
PORT = int(os.environ.get("DF_PORT", str(int(os.environ.get("JSON_BASE", "58100")) + 3)))
MILESTONES = ["Candidate", "Prefetching", "Probation", "ActiveLimited", "Active"]


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
    ap.add_argument("--until", choices=["active", "final", "df2"])
    ap.add_argument("--deadline-daa", type=int, default=0)
    a = ap.parse_args()
    seen = set()
    if os.path.exists(f"{WORK}/df-milestones.tsv"):
        seen = {l.split("\t")[0] for l in open(f"{WORK}/df-milestones.tsv")}
    active_at = None
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
        for m in marks:
            if m not in seen:
                seen.add(m)
                with open(f"{WORK}/df-milestones.tsv", "a") as f:
                    f.write(f"{m}\t{s['daa']}\t{s['t']}\n")
                print(f"{s['t']} DAA {s['daa']}: {m}", flush=True)
        goal = {"active": "Active", "final": "first-final-after-active", "df2": "court-fraud"}.get(a.until or "")
        if a.once or (goal and goal in seen):
            return 0
        if a.deadline_daa and s["daa"] >= a.deadline_daa:
            print(f"INCOMPLETE: DAA {s['daa']} passed {a.deadline_daa} before {goal}", flush=True)
            return 3
        time.sleep(30)


if __name__ == "__main__":
    sys.exit(main())
