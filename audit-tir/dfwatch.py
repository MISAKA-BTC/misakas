#!/usr/bin/env python3
"""audit-tir/dfwatch.py — the D-F drill's sampler (read-only RPC to the drill's own loopback nodes).

Every 30 s, through one node (DF_PORT, new3's JSON port by default), for each IR class of the drill — D-F1
(ir-class.id: the Qwen2.5-A16 class, new0 its registrant and producer) and the small class (small-class.id:
the tiny HF llama D-F2's live battery runs on, new4 its registrant and producer) — the class's registry row
(getPalwModelRegistry: lifecycle state, ready seats), its class-context row (getPalwClassContexts: source must
read chain_ir_registration), and the class's claims of the producers' bonds (getPalwClaims, executor, terminal
included) counted by phase: Final, convicted by a court (CourtFraud, CourtDefault, CourtHeldVerdict — a
proof, a default at the court's clock, a held dissection's verdict), defaulted on a data-availability demand
(ProducerWithholding — the B/D/C piece's silent executor, counted apart as "withheld"), otherwise Voided, open.
Appends one line to $WORK_DIR/df.tsv and rewrites $WORK_DIR/df-state.json (D-F1's fields at the top level,
the small class's under "small"). Prints the first sample at which each milestone is reached (Candidate,
Prefetching, Probation, ActiveLimited, Active, the first Final after Active; the small class's with a
"small-" prefix, and its first conviction) to $WORK_DIR/df-milestones.tsv.

  dfwatch.py [--once] [--until prefetching|active|final|small-active|small-convicted]
Exit (with --until): 0 when reached, 3 INCOMPLETE when --deadline-daa passes first, 4 when the memory tripwire stopped the drill.

Every sample also appends the drill nodes' resident memory and the machine's free memory to $WORK_DIR/mem.tsv (time, DAA,
summed RSS in GiB, free %, each node's MiB) — the measurement the quiet window for the drill is sized from — and trips a wire
(DF_MEM_FLOOR_PCT, default 10; 0 = off) that stops the drill's nodes when the machine runs out of memory."""
import argparse, json, os, re, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rpc import call, pick

WORK = os.path.expanduser(os.environ.get("WORK_DIR", "~/.misaka-palw-tir-drill"))
PORT = int(os.environ.get("DF_PORT", str(int(os.environ.get("JSON_BASE", "58100")) + 3)))
MILESTONES = ["Candidate", "Prefetching", "Probation", "ActiveLimited", "Active"]
# A claim a court convicted: the reasons the fold writes for a proof, a default and a held verdict.
CONVICTED = ("CourtFraud", "CourtDefault", "CourtHeldVerdict")
# A claim voided because its executor did not answer a data-availability demand in time (R-core+'s DA default,
# evidence transport C's silent executor): a slash, counted apart from the court's convictions.
WITHHELD = ("ProducerWithholding",)


def node_rss_kib():
    """The resident set of every running drill node (its kaspad.pid), in KiB, by node name."""
    out = {}
    try:
        names = sorted(os.listdir(WORK))
    except OSError:
        return out
    for name in names:
        try:
            pid = int(open(f"{WORK}/{name}/kaspad.pid").read().strip())
            rss = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
            if rss.isdigit():
                out[name] = int(rss)
        except (OSError, ValueError):
            continue
    return out


def free_memory_pct():
    """macOS `memory_pressure`'s system-wide free percentage (reclaimable memory included), None when unreadable."""
    try:
        text = subprocess.run(["memory_pressure"], capture_output=True, text=True, timeout=20).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    m = re.search(r"System-wide memory free percentage: (\d+)%", text)
    return int(m.group(1)) if m else None


# The drill's memory tripwire (the 2026-10-01 incident: a lane's process took the machine to swap and the swap filled the disk):
# when the machine's free memory stays below DF_MEM_FLOOR_PCT for two samples in a row, the drill's nodes are stopped
# (SIGINT, never SIGKILL: `nodes.sh stop`) and MEM-TRIPWIRE is written beside the logs. 0 switches it off.
MEM_FLOOR_PCT = int(os.environ.get("DF_MEM_FLOOR_PCT", "10"))


def memory_sample(daa, state):
    """Append one line to $WORK/mem.tsv (time, DAA, the nodes' summed RSS in GiB, the machine's free %, each node's RSS in MiB)
    and trip the wire when the machine is out of memory twice running. Returns True when the drill was stopped."""
    rss = node_rss_kib()
    free = free_memory_pct()
    total = sum(rss.values()) / (1 << 20)
    with open(f"{WORK}/mem.tsv", "a") as f:
        f.write(f"{time.strftime('%F %T')}\t{daa}\t{total:.2f}\t{free if free is not None else '-'}\t"
                f"{json.dumps({n: round(k / 1024) for n, k in rss.items()})}\n")
    if MEM_FLOOR_PCT and free is not None and free < MEM_FLOOR_PCT:
        state["low"] = state.get("low", 0) + 1
        if state["low"] >= 2:
            msg = (f"{time.strftime('%F %T')} the machine's free memory is {free}% (< {MEM_FLOOR_PCT}%) twice running; the drill's nodes "
                   f"hold {total:.1f} GiB — stopping them (SIGINT) so the machine does not swap its disk full")
            print("MEM TRIPWIRE:", msg, flush=True)
            with open(f"{WORK}/MEM-TRIPWIRE", "a") as f:
                f.write(msg + "\n")
            subprocess.run(["bash", os.path.join(os.path.dirname(os.path.abspath(__file__)), "nodes.sh"), "stop"], timeout=900)
            return True
    else:
        state["low"] = 0
    return False


def read_id(name):
    try:
        return open(f"{WORK}/{name}").read().strip() or None
    except OSError:
        return None


def bonds():
    m = json.load(open(f"{WORK}/keyring/manifest.json"))
    return {"new0": m["seats"][3]["bond_outpoint"], "new4": m["seats"][4]["bond_outpoint"]}


def class_sample(reg, ctx, claims_by_bond, cid):
    row = next((c for c in (pick(reg, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    crow = next((c for c in (pick(ctx, "classes", default=[]) or []) if pick(c, "classId") == cid), None)
    counts = {}
    for who, claims in claims_by_bond.items():
        c = {"final": 0, "convicted": 0, "withheld": 0, "voided": 0, "open": 0}
        for cl in claims:
            if pick(cl, "classId") not in (None, cid):
                continue
            phase = str(pick(cl, "phase", default=""))
            if phase.startswith("Final"):
                c["final"] += 1
            elif any(reason in phase for reason in CONVICTED):
                c["convicted"] += 1
            elif any(reason in phase for reason in WITHHELD):
                c["withheld"] += 1
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
    """The lifecycle milestone `state` is. `ActiveLimited` starts with `Active` and is NOT Active: matching it
    as a prefix marked Active at ActiveLimited, so a Final under limited activation passed D-F1 early."""
    if state.startswith("ActiveLimited"):
        names = ["ActiveLimited"]
    elif state.startswith("Active"):
        names = ["Active"]
    else:
        names = [m for m in MILESTONES if state.startswith(m)]
    return [prefix + m for m in names]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--until", choices=["prefetching", "active", "final", "small-active", "small-convicted"])
    ap.add_argument("--deadline-daa", type=int, default=0)
    a = ap.parse_args()
    seen = set()
    if os.path.exists(f"{WORK}/df-milestones.tsv"):
        seen = {l.split("\t")[0] for l in open(f"{WORK}/df-milestones.tsv")}
    mem_state = {}
    while True:
        s = sample()
        if not a.once and memory_sample(s["daa"], mem_state):
            return 4
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
        if a.deadline_daa and s["daa"] >= a.deadline_daa:
            print(f"INCOMPLETE: DAA {s['daa']} passed {a.deadline_daa} before {goal}", flush=True)
            return 3
        time.sleep(30)


if __name__ == "__main__":
    sys.exit(main())
