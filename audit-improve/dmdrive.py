#!/usr/bin/env python3
"""audit-improve/dmdrive.py — the D-M drills' sampler, actor and verdicts (RFC-0004 A13).

ONE process per drill chain (started by `dm.sh up`, stopped by `dm.sh down`). Every TICK_S seconds it reads the chain
through one of the drill's own loopback nodes (JSON wRPC: DAA, the model registry, a producer's claims) and the improvement
status the nodes write (`palw-improve-status.json` in each node's panel state dir: lines, epochs, candidates, counts,
grants, the evaluation view), and does what the line owner, the setters, the candidates' submitters and the drills' own
attackers would do at that point of each line's epoch — each action once, recorded in $WORK/drive-state.json, so a restart
resumes. Every object is built OFFLINE (`palw-class improve …`, `misaka palw tir-registration`) and carried with
`misaka palw submit-object`, funded by the keyring's main wallet; nothing here touches a node but through its public doors.

  dmdrive.py run                 the loop (sampler + actor + verdict files)
  dmdrive.py once                one tick
  dmdrive.py verdicts            print every drill's verdict now
  dmdrive.py selftest            the verdict logic against synthetic chain data (no chain, no binary)

Files under $WORK_DIR: drive.log (what it did), drive-state.json, milestones.tsv (the first DAA each milestone was seen),
samples.tsv (one line a tick), snapshots/ (the status file at each milestone), objects/ (every object it built and carried),
evidence/ (log lines the verdicts cite), verdict/dmN.verdict (PASS | FAIL | INCOMPLETE, and why).
"""
import argparse
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from rpc import call, pick  # noqa: E402

E = os.environ
WORK = os.path.expanduser(E.get("WORK_DIR", "~/.misaka-palw-improve-drill"))
KR = E.get("KR", f"{WORK}/keyring")
UHOME = E.get("UHOME", f"{WORK}/userhome")
MODEL = E.get("MODEL_DIR", f"{WORK}/model")
VERDICTS = E.get("VERDICT_DIR", f"{WORK}/verdict")
OBJ = f"{WORK}/objects"
EVID = f"{WORK}/evidence"
SNAP = f"{WORK}/snapshots"
CLI = E.get("CLI_BIN", "misaka")
KASPAD = E.get("KASPAD_BIN", "kaspad")
TOOLS = E.get("TOOLS_BIN", ".")
JSON_BASE = int(E.get("JSON_BASE", "63100"))
BORSH_BASE = int(E.get("BORSH_BASE", "62100"))
CAND_FORM = E.get("CAND_FORM", "composite")
IMPROVE_AT = int(E.get("IMPROVE_AT", "40"))
TIR_AT = int(E.get("TIR_AT", "20"))
TICK_S = int(E.get("DM_TICK_S", "30"))
PLAN = json.load(open(os.path.join(HERE, "drill.json")))
POLICY = PLAN["policy"]

NODES = {}
for _row in (E.get("NODES") or "").strip().splitlines():
    _f = _row.split()
    if len(_f) >= 6:
        NODES[_f[0]] = {"k": int(_f[1]), "seat": None if _f[2] == "-" else int(_f[2]), "role": _f[3], "hb": _f[4] == "1", "ir": _f[5] == "1",
                        "jit": len(_f) > 6 and _f[6] == "jit"}

SPACING_S = 25            # between two carriers funded by the main wallet: the second must find the first's change confirmed
GIVE_UP_TRIES = 8         # a step that failed this many ticks is INCOMPLETE, said once


# ---------------------------------------------------------------------------------------------------------------------
# small helpers
# ---------------------------------------------------------------------------------------------------------------------
def now():
    return time.strftime("%F %T")


def log(msg):
    line = f"{now()} {msg}"
    print(line, flush=True)
    try:
        with open(f"{WORK}/drive.log", "a") as f:
            f.write(line + "\n")
    except OSError:
        pass


def salt():
    s = E.get("SALT", "").strip()
    if not s:
        try:
            s = open(f"{WORK}/SALT").read().strip()
        except OSError:
            s = ""
    return s


def manifest():
    return json.load(open(f"{KR}/manifest.json"))


GENESIS_SEATS = 8                      # keyring seats 0..7 are genesis bonds; 8.. are registered after genesis (liars, lane D's)


def liar_path(n):
    return f"{WORK}/liars/bond-{n}.json"


def extra_bond(n):
    """The record the registration step wrote for post-genesis bond n (bond_outpoint, seed_file, operator_id, fee_outpoint, address, collateral), or None."""
    try:
        return json.load(open(liar_path(n)))
    except (OSError, ValueError):
        return None


def bond_of(seat):
    if seat >= GENESIS_SEATS:
        rec = extra_bond(seat)
        if rec is None:
            raise LookupError(f"bond {seat} is not registered yet ({liar_path(seat)})")
        return rec["bond_outpoint"]
    return manifest()["seats"][seat]["bond_outpoint"]


def seed_of(seat):
    return f"{KR}/bond-{seat}.seed"


def model_line_id(class_id, founder_bond, name):
    """`model_line_id_v1`, offline: the id of a non-founding line — keyed blake2b-64 over the class, the founder's bond (borsh: the transaction id, the
    index as u32 LE) and the name (length u32 LE, bytes). Known before the chain exists, so a drill's lying executor can be told which line to spoil;
    pinned against the chain's own function by misaka-palw-sdk/tests/improve_line_id.rs."""
    txid, index = founder_bond.rsplit(":", 1)
    nb = name.encode()
    msg = bytes.fromhex(class_id) + bytes.fromhex(txid) + int(index).to_bytes(4, "little") + len(nb).to_bytes(4, "little") + nb
    return hashlib.blake2b(msg, digest_size=64, key=b"misaka-palw/model-line/id/v1").hexdigest()


def audit_slot(class_id, span_daa, period_daa=100):
    """Where in each `period_daa` DAA a class meets its admission jury (`palw_admission_audit_due_staggered_v1`: `(span + H(class) mod period) mod period == 0`,
    the period 100 DAA on testnet-12). A composite candidate's proofs are possible only once the chain holds its record (its candidate's submission), and its jury
    sits at this slot, so the wait from the submission to the class's admission is the DAA until the next one."""
    period = max(period_daa // max(span_daa, 1), 1)
    d = hashlib.blake2b(bytes.fromhex(class_id), key=b"misaka-palw/admission-audit/stagger/v1", digest_size=64).digest()
    off = int.from_bytes(d[:8], "little") % period
    return ((-off) % period) * span_daa, period * span_daa


def line_id_of(name):
    """The id of a plan line before it is founded: the founding line's is its class's, the others' `model_line_id`."""
    head = model_id("head")
    if name == "W1":
        return head
    return model_line_id(head, bond_of(PLAN["lines"][name]["owner_seat"]), PLAN["lines"][name]["name"])


# D-M3's roles: the two SACRIFICIAL executors that each lie once (their first evaluation of line T; their bonds are registered after genesis and the nodes
# started just-in-time, because the capacity package's aggregate liability (AG-2) makes one CourtFraud conviction void ALL the bond's live claims and freeze
# the bond) and the challenger that replays and files.
TAMPER = {"new7": "leaf:1", "new8": "output"}
CHALLENGER = "new1"


def read(path, default=None):
    try:
        return open(path).read().strip()
    except OSError:
        return default


def model_id(name, kind="class"):
    return read(f"{MODEL}/ids/{name}.{kind}", "")


def run(cmd, timeout=900, home=None):
    env = dict(os.environ)
    if home:
        env["HOME"] = home
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)
        return p.returncode, (p.stdout or "") + (p.stderr or "")
    except subprocess.TimeoutExpired:
        return 124, "timeout"
    except OSError as e:
        return 127, str(e)


def node_alive(n):
    pidf = f"{WORK}/{n}/kaspad.pid"
    pid = read(pidf)
    if not pid:
        return False
    try:
        os.kill(int(pid), 0)
        return True
    except (OSError, ValueError):
        return False


def live_nodes(roles=None):
    return [n for n, v in NODES.items() if node_alive(n) and (roles is None or v["role"] in roles)]


def jport(n):
    return JSON_BASE + NODES[n]["k"]


def borsh_port(n):
    return BORSH_BASE + NODES[n]["k"]


def rpc(method, params=None, prefer=("new3", "new0", "new1", "new2", "new4", "new5", "new6")):
    last = {"error": "no node answers"}
    for n in list(prefer) + [x for x in NODES if x not in prefer]:
        if n in NODES and NODES[n]["role"] != "old" and node_alive(n):
            r = call(jport(n), method, params or {})
            if "error" not in r:
                return r
            last = r
    return last


def daa_of(node):
    r = call(jport(node), "getBlockDagInfo", {})
    if "error" in r:
        return None
    return int(pick(r, "virtualDaaScore", default=0) or 0)


def chain_daa():
    for n in ("new3", "new0", "new1", "new2", "new4", "new5", "new6"):
        if n in NODES and node_alive(n):
            d = daa_of(n)
            if d is not None:
                return d
    return None


def status_file_of(node):
    hits = glob.glob(f"{WORK}/{node}/app/**/palw-improve-status.json", recursive=True)
    return max(hits, key=os.path.getmtime) if hits else None


def read_status():
    """The newest valid improvement status any node wrote, with its path (nodes are one chain: any will do)."""
    best = None
    for n in NODES:
        f = status_file_of(n)
        if not f:
            continue
        try:
            d = json.load(open(f))
        except (OSError, ValueError):
            continue
        if best is None or int(d.get("daa", 0)) > int(best[0].get("daa", 0)):
            best = (d, f)
    return best


def cli_prefix(node):
    return [CLI, "--network", "testnet-12", "--rpc", f"127.0.0.1:{borsh_port(node)}", f"--palw-drill-genesis-salt={salt()}"]


def tool(*args):
    return [f"{TOOLS}/palw-class", *args]


# ---------------------------------------------------------------------------------------------------------------------
# persisted state
# ---------------------------------------------------------------------------------------------------------------------
class State:
    def __init__(self, path):
        self.path = path
        self.d = {"done": {}, "data": {}, "tries": {}, "failed": {}, "milestones": {}, "log": []}
        try:
            self.d.update(json.load(open(path)))
        except (OSError, ValueError):
            pass

    def save(self):
        tmp = self.path + ".partial"
        with open(tmp, "w") as f:
            json.dump(self.d, f, indent=1, sort_keys=True)
        os.replace(tmp, self.path)

    def done(self, k):
        return k in self.d["done"]

    def mark(self, k, **info):
        self.d["done"][k] = {"t": now(), **info}
        self.save()

    def get(self, k, default=None):
        return self.d["data"].get(k, default)

    def put(self, k, v):
        self.d["data"][k] = v
        self.save()

    def tried(self, k):
        self.d["tries"][k] = self.d["tries"].get(k, 0) + 1
        self.save()
        return self.d["tries"][k]

    def fail(self, k, why):
        self.d["failed"][k] = why
        self.save()


# ---------------------------------------------------------------------------------------------------------------------
# objects: build offline, carry with the main wallet
# ---------------------------------------------------------------------------------------------------------------------
def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(obj, f, indent=1)
    return path


def improve_object(kind, seat, spec, out, extra=()):
    """`palw-class improve <kind>` signed by bond-<seat>: returns (rc, text)."""
    spec_path = out + ".spec.json"
    if spec is not None:
        write_json(spec_path, spec)
    cmd = tool("improve", kind, "--network", "testnet-12", "--drill-salt", salt(), "--key-file", seed_of(seat),
               "--bond", bond_of(seat), "--out", out, *(["--spec", spec_path] if spec is not None else []), *extra)
    return run(cmd, home=UHOME)


def submit(objects, node=None, tries=6):
    """`misaka palw submit-object` of the objects (in order, each carrier on the previous one's change), funded by main-0.
    A refusal because the funding output is still in the mempool waits and tries again."""
    node = node or (live_nodes(("seat", "floor", "head", "eval", "evalw")) or ["new3"])[0]
    cmd = cli_prefix(node) + ["palw", "submit-object", "--object", *objects, "--yes", "--key-file", f"{KR}/main-0.seed"]
    out = ""
    for i in range(tries):
        rc, out = run(cmd, home=UHOME)
        if rc == 0:
            time.sleep(SPACING_S)
            return True, out
        if "already spent" in out or "no mature" in out or "mempool" in out:
            log(f"submit: the funding output is not free yet ({out.strip().splitlines()[-1][:120] if out.strip() else ''}); waiting")
            time.sleep(45)
            continue
        break
    return False, out


def parse_after(text, label):
    m = re.search(rf"{re.escape(label)}\s+([0-9a-f]{{128}})", text)
    return m.group(1) if m else None


# ---------------------------------------------------------------------------------------------------------------------
# the plan's derived facts
# ---------------------------------------------------------------------------------------------------------------------
def line_form(name):
    """How a line's candidates are held: `composite` (parent + PALWTIRS adapter: RFC-0004's own form, on the lines the plan marks so) or `full` (the adapter merged
    into a full-weight class: the extra lines, and every line when CAND_FORM=full — the form before the core lane's composite readiness)."""
    if CAND_FORM == "full":
        return "full"
    return (PLAN["lines"].get(name) or {}).get("form", "composite")


def line_policy(line):
    """The drill's common policy with the line's own overrides (setter_cap_permille, …) and, on a composite line, the long Submission window its candidates' admission
    needs (`policy_composite`)."""
    p = json.loads(json.dumps(POLICY))
    row = PLAN["lines"][line]
    for overrides in (row.get("policy"), row.get("policy_composite") if line_form(line) == "composite" else None):
        for k, v in (overrides or {}).items():
            if isinstance(v, dict):
                p.setdefault(k, {}).update(v)
            else:
                p[k] = v
    return p


def line_windows(line):
    return line_policy(line)["windows"]


def line_l_e(line):
    w = line_windows(line)
    return sum(w[k] for k in ("w_collect", "w_submit", "w_holdout", "w_eval", "court_margin"))


def pool(name):
    p = {"a": "pool-a.json", "b": "pool-b.json", "reg": "pool-reg.json"}[name]
    return json.load(open(f"{MODEL}/{p}"))


def pool_slice(name, lo, hi):
    d = pool(name)
    return {"prompts": d["prompts"][lo:hi], "keys": d["keys"][lo:hi]}


ASSETS = ("head", "win", "lose", "winc", "losec")


def asset_of(cls, line):
    """The model asset a plan class name stands for on a line: on a composite line `win` and `lose` are the adapters as composite classes (winc, losec: parent +
    PALWTIRS section), on a full-weight line the merged full-weight classes (win, lose); the head is always H."""
    if cls in ("win", "lose") and line_form(line) == "composite":
        return {"win": "winc", "lose": "losec"}[cls]
    return cls


def is_composite(asset):
    return asset in ("winc", "losec")


def class_container(cls, line):
    """The artifact flags `palw-class improve candidate` takes for a candidate class (a plan name) on a line."""
    a = asset_of(cls, line)
    if is_composite(a):
        return ["--parent", f"{MODEL}/head.class.palwtir", "--section", f"{MODEL}/{a}.palwtirs"]
    return ["--artifact", f"{MODEL}/{a}.class.palwtir"]


def first_lacking_fence():
    """(name, height) of the lowest fence the old relay lacks (D-M5's crossing), from the layout dm.sh wrote."""
    d = read(f"{WORK}/old-lacks.txt", "")
    best = None
    for tok in d.split():
        if "@" in tok:
            name, at = tok.split("@")
            if best is None or int(at) < best[1]:
                best = (name, int(at))
    return best


# ---------------------------------------------------------------------------------------------------------------------
# the chain, as one tick reads it
# ---------------------------------------------------------------------------------------------------------------------
STATE_NAMES = ("Candidate", "Registered", "Prefetching", "Probation", "ActiveLimited", "Active", "Held")


def lifecycle_of(reg, class_id):
    for c in pick(reg, "classes", default=[]) or []:
        if pick(c, "classId") == class_id:
            raw = str(pick(c, "state", default="")) or "?"
            for name in ("ActiveLimited", "Active", "Probation", "Prefetching", "Candidate", "Registered", "Held"):
                if raw.startswith(name):
                    return name, pick(c, "readySeats", "readySeatsNow")
            return raw[:24], pick(c, "readySeats", "readySeatsNow")
    return "absent", None


def phase_is(phase, name):
    """getPalwClaims spells a phase in snake case (`final`, `voided`, `panel_bound`) and its void reason likewise (`court_fraud`, `aggregate_forfeit`)."""
    return str(phase).lower().startswith(name)


def claims_of(bond):
    r = rpc("getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0})
    return pick(r, "claims", default=[]) or []


def final_claims(bond, class_id):
    n = 0
    for c in claims_of(bond):
        if pick(c, "classId") in (None, class_id) and phase_is(pick(c, "phase", default=""), "final"):
            n += 1
    return n


def line_status(st, name_or_id):
    """The status JSON's row of a line, by id."""
    for l in (st or {}).get("lines", []):
        if l.get("line_id") == name_or_id:
            return l
    return None


def epoch_row(line_row, epoch):
    for e in (line_row or {}).get("epochs", []):
        if e.get("epoch") == epoch:
            return e
    return None


def eval_view(st, line_id, epoch):
    for e in (st or {}).get("evaluation", []):
        if e.get("line_id") == line_id and e.get("epoch") == epoch:
            return e
    return None


def eval_rows_of(st, line_id):
    """Every evaluation job row of a line the status file's evaluation view lists (spec 17 §17.8.6: a convicted claim
    frees its job, so the row stops holding it): epoch, item, subject, kind, and the claim the row holds, if any."""
    rows = []
    for e in (st or {}).get("evaluation", []) or []:
        if e.get("line_id") != line_id:
            continue
        for j in e.get("jobs", []) or []:
            c = j.get("claim") or {}
            rows.append({"epoch": e.get("epoch"), "item": j.get("item"), "subject": j.get("subject"), "kind": j.get("kind"),
                         "claim": c.get("id"), "voided": bool(c.get("voided")), "score": c.get("score")})
    return rows


def jobs_settled(view):
    """Every job that holds a claim holds a final (or a void) one — keys may open (spec 17 §17.8.2, in Closing)."""
    jobs = [j for j in (view or {}).get("jobs", []) if j.get("claim")]
    if not jobs:
        return False
    return all(j["claim"].get("final_daa") is not None or j["claim"].get("voided") for j in jobs)


def outcome_of(erow):
    return (erow or {}).get("outcome")



# ---------------------------------------------------------------------------------------------------------------------
# post-genesis bonds, the just-in-time liar nodes, the capacity sampler (int-11 combined drill)
# ---------------------------------------------------------------------------------------------------------------------
XB = PLAN.get("extra_bonds", {})
XB_ORDER = [int(x) for x in XB.get("order", [10, 11, 12, 13, 8, 9])]     # lane D's 10..13 are needed first (DG-4 from ~150); the liars' 8, 9 by ~440
XB_FROM_DAA = int(E.get("XB_FROM_DAA") or XB.get("from_daa", 44))                                 # after the improvement fence: below it the main wallet is D-M5's
XB_FLOAT_MSK = int(XB.get("float_msk", 150))                              # on top of the collateral: the 100 MSK fee float the genesis seats carry, and fees
XB_WAIT_DAA = int(XB.get("registrar_wait_daa", 12))                       # a registrar that has not printed its bond after this many DAA is stopped and tried again
LIARS = PLAN.get("liars", {})
LIAR_START_DAA = int(LIARS.get("start_daa", 440))
LIAR_DEADLINE_DAA = int(LIARS.get("deadline_daa", 300))                   # past start + this a liar that never lied or was never convicted is stopped, and the verdict says so
CAP = PLAN.get("capacity", {})
CAP_WINDOWS = {                                                           # the measured windows, DAA: [lo, hi)
    "rho25": (int(E.get("CAP2_AT", "560")) + int(CAP.get("settle_daa", 10)), int(E.get("CAP2_AT", "560")) + int(CAP.get("settle_daa", 10)) + int(CAP.get("window_daa", 80))),
    "rho100": (int(E.get("CAP3_AT", "655")) + int(CAP.get("settle_daa", 10)), int(E.get("CAP3_AT", "655")) + int(CAP.get("settle_daa", 10)) + int(CAP.get("window_daa", 80))),
}
CAP_STEPS = (("rho25", "capacity_step2"), ("rho100", "capacity_step3"))
LIVE_PHASES = ("provisional", "panel_bound", "receipt_licensed", "default_disputed")   # a claim not yet Final and not void
UNLICENSED_PHASES = ("provisional", "panel_bound", "default_disputed")                  # accepted and waiting for its licence: the backlog


MEM = PLAN.get("memory", {})
MEM_TRIP_PCT = int(E.get("MEM_TRIP_PCT", MEM.get("trip_free_pct", 12)))       # system free memory below this, two ticks running: stop the sacrificial nodes
MEM_TRIP2_PCT = int(E.get("MEM_TRIP2_PCT", MEM.get("trip2_free_pct", 8)))     # below this: the old relay too (D-M5's, once it has been done). Never a seat or the clocks.
MEM_FIRST_HOUR_TICKS = max(3600 // max(TICK_S, 1), 1)


def parse_free_pct(text):
    m = re.search(r"System-wide memory free percentage:\s*(\d+)%", text or "")
    return int(m.group(1)) if m else None


def rss_mib(pid):
    rc, out = run(["ps", "-o", "rss=", "-p", str(pid)], timeout=20)
    try:
        return int(out.strip().split()[0]) // 1024 if rc == 0 and out.strip() else None
    except (ValueError, IndexError):
        return None



def msk_text(sompi):
    return f"{sompi // 10**8}.{sompi % 10**8:08d}"


def seat_bonds():
    """The genesis seats' bonds that have a node (they are the panel and the producers the capacity line measures)."""
    return [(n, v["seat"]) for n, v in NODES.items() if v["seat"] is not None and v["seat"] < GENESIS_SEATS and v["role"] != "old"]


def percentile(xs, q):
    if not xs:
        return None
    xs = sorted(xs)
    i = min(len(xs) - 1, max(0, int(round(q * (len(xs) - 1)))))
    return xs[i]


def slope(points):
    """Least-squares slope of (x, y) points (None under two points or no spread)."""
    n = len(points)
    if n < 2:
        return None
    mx = sum(x for x, _ in points) / n
    my = sum(y for _, y in points) / n
    den = sum((x - mx) ** 2 for x, _ in points)
    return None if den == 0 else sum((x - mx) * (y - my) for x, y in points) / den


def capacity_summary(series, claims, lo, hi):
    """One window's numbers. `series`: the sampler's per-tick rows {daa, backlog, occ_max, occ_mean, reserved_ratio}; `claims`: {claim id: {acc, lic, fin}}
    (the DAA a claim was accepted at — the chain's own; licensed and Final — the DAA of the first tick that saw it so, the chain's own where it was caught
    in its licensed phase: a licence latency is exact to one sampler tick, ~0.2 DAA). The gate is the coordinator's: the queue does not diverge — here:
    the backlog's slope over the window's second half is not positive beyond 0.05 claims per DAA, and its last quarter's mean is not above 1.5 times the
    second quarter's plus 2."""
    span = max(hi - lo, 1)
    acc = [c for c in claims.values() if c.get("acc") is not None and lo <= c["acc"] < hi]
    lic = [c for c in claims.values() if c.get("lic") is not None and lo <= c["lic"] < hi]
    lat = [c["lic"] - c["acc"] for c in claims.values() if c.get("acc") is not None and c.get("lic") is not None and lo <= c["acc"] < hi]
    rows = [r for r in series if lo <= r["daa"] < hi]
    q = max(len(rows) // 4, 1)
    first_half = [(r["daa"], r["backlog"]) for r in rows[len(rows) // 2:]]
    mean = lambda rr: (sum(r["backlog"] for r in rr) / len(rr)) if rr else None  # noqa: E731
    q2, q4 = mean(rows[q:2 * q]), mean(rows[-q:])
    sl = slope(first_half)
    diverges = None
    if rows and sl is not None and q2 is not None and q4 is not None and len(rows) >= 8:
        diverges = bool(sl > 0.05 and q4 > 1.5 * q2 + 2)
    return {"lo": lo, "hi": hi, "samples": len(rows), "accepted": len(acc), "accepted_per_daa": round(len(acc) / span, 3),
            "licensed": len(lic), "licensed_per_daa": round(len(lic) / span, 3),
            "latency_p50": percentile(lat, 0.5), "latency_p95": percentile(lat, 0.95), "latency_n": len(lat),
            "backlog_first": rows[0]["backlog"] if rows else None, "backlog_last": rows[-1]["backlog"] if rows else None,
            "backlog_max": max((r["backlog"] for r in rows), default=None), "backlog_slope_per_daa_second_half": None if sl is None else round(sl, 4),
            "backlog_q2_mean": None if q2 is None else round(q2, 2), "backlog_q4_mean": None if q4 is None else round(q4, 2),
            "occupancy_max": max((r["occ_max"] for r in rows), default=None),
            "occupancy_mean": round(sum(r["occ_mean"] for r in rows) / len(rows), 2) if rows else None,
            "exposure_reserved_ratio_max": max((r["reserved_ratio"] for r in rows if r.get("reserved_ratio") is not None), default=None),
            "diverges": diverges}


def verdict_cap(sd):
    """The capacity line: each step's window measured, and the gate (the queue does not diverge) read. ρ=25 first; ρ=100 after it. A step the binary
    could not arm is reported as such (the drill's capacity flags come from lane A's int-capdrill, not in every build)."""
    cap = (sd.get("data") or {}).get("cap-summary") or {}
    armed = (sd.get("data") or {}).get("cap-armed") or {}
    checks = []
    for name, fence in CAP_STEPS:
        sm = cap.get(name)
        if armed.get(fence) is False:
            checks.append((None, f"{name}: this build lists no --palw-drill-{fence.replace('_', '-')}-at, not armed"))
            continue
        if not sm:
            checks.append((None, f"{name}: window {CAP_WINDOWS[name][0]}..{CAP_WINDOWS[name][1]} not measured yet"))
            continue
        txt = (f"{name}: accepted {sm['accepted_per_daa']}/DAA, licensed {sm['licensed_per_daa']}/DAA, licence latency p50 {sm['latency_p50']} p95 {sm['latency_p95']} DAA "
               f"(n={sm['latency_n']}), backlog {sm['backlog_first']}->{sm['backlog_last']} (max {sm['backlog_max']}, slope {sm['backlog_slope_per_daa_second_half']}/DAA), "
               f"seat occupancy max {sm['occupancy_max']} mean {sm['occupancy_mean']}, exposure reserved/ceiling max {sm['exposure_reserved_ratio_max']}")
        checks.append((None if sm["diverges"] is None else (not sm["diverges"]), txt))
    return v_all(checks)

# ---------------------------------------------------------------------------------------------------------------------
# the actor
# ---------------------------------------------------------------------------------------------------------------------
class Drive:
    def __init__(self):
        os.makedirs(OBJ, exist_ok=True)
        os.makedirs(EVID, exist_ok=True)
        os.makedirs(SNAP, exist_ok=True)
        os.makedirs(VERDICTS, exist_ok=True)
        self.s = State(f"{WORK}/drive-state.json")
        self.daa = None
        self.reg = {}
        self.status = None
        self.status_path = None
        self.ids = {}

    # ----- one tick -----------------------------------------------------------------------------------------------
    def tick(self):
        self.daa = chain_daa()
        if self.daa is None:
            log("no node answers")
            return
        self.reg = rpc("getPalwModelRegistry", {})
        got = read_status()
        self.status, self.status_path = got if got else (None, None)
        self.ids = {n: model_id(n) for n in ASSETS}
        self.milestones()
        for step in (self.step_memory, self.step_m5_below, self.step_m5_below_verify, self.step_m5_cross, self.step_register_classes, self.step_lines, self.step_policies,
                     self.step_material, self.step_epochs, self.step_rollbacks, self.step_attacks, self.step_dm3_probe, self.step_seat_operator_ids, self.step_extra_bonds, self.step_liars_jit,
                     self.step_capacity):
            key = step.__name__
            if self.s.d["failed"].get(key):
                continue
            try:
                step()
            except Exception as e:  # noqa: BLE001 — a step's bug must not stop the sampler
                n = self.s.tried(key + ":exc")
                log(f"{key}: {type(e).__name__}: {e} (try {n})")
                if n >= GIVE_UP_TRIES:
                    self.s.fail(key, f"{type(e).__name__}: {e}")
        self.write_verdicts()
        with open(f"{WORK}/samples.tsv", "a") as f:
            f.write(f"{now()}\t{self.daa}\t{json.dumps(self.brief())}\n")

    def guard(self, key, fn):
        """One action under its own failure count: a line's bug must not stop another line's epoch."""
        if self.s.d["failed"].get(key):
            return
        try:
            fn()
        except Exception as e:  # noqa: BLE001
            n = self.s.tried(key + ":exc")
            log(f"{key}: {type(e).__name__}: {e} (try {n})")
            if n >= GIVE_UP_TRIES:
                self.s.fail(key, f"{type(e).__name__}: {e}")

    def brief(self):
        out = {}
        for name, cid in self.ids.items():
            if cid:
                st, ready = lifecycle_of(self.reg, cid)
                out[name] = f"{st}/{ready}"
        for line, lid in self.line_ids().items():
            row = line_status(self.status, lid)
            if row:
                e = row.get("open_epoch")
                er = epoch_row(row, e) if e else None
                out[f"line:{line}"] = f"e{e}:{er['state']}" if er else f"idle head={row['head'][:8]} next_due={row['next_due_daa']}"
                view = eval_view(self.status, lid, e) if e else None
                if view:
                    jobs = [j for j in view["jobs"] if j.get("claim")]
                    final = sum(1 for j in jobs if j["claim"].get("final_daa") is not None)
                    voided = sum(1 for j in jobs if j["claim"].get("voided"))
                    out[f"eval:{line}"] = f"items={len(view['items'])} claimed={len(jobs)} final={final} void={voided}"
        return out

    # ----- milestones -------------------------------------------------------------------------------------------
    def note(self, name, **extra):
        if name in self.s.d["milestones"]:
            return
        self.s.d["milestones"][name] = {"daa": self.daa, "t": now(), **extra}
        self.s.save()
        with open(f"{WORK}/milestones.tsv", "a") as f:
            f.write(f"{name}\t{self.daa}\t{now()}\n")
        log(f"MILESTONE {name} at DAA {self.daa}")
        if self.status is not None:
            try:
                with open(f"{SNAP}/{re.sub(r'[^A-Za-z0-9._-]', '_', name)}.json", "w") as f:
                    json.dump(self.status, f, indent=1)
            except OSError:
                pass

    def milestones(self):
        for fence, at in (("tir", TIR_AT), ("improve", IMPROVE_AT)):
            if self.daa >= at:
                self.note(f"fence:{fence}")
        for name, cid in self.ids.items():
            if cid:
                st, _ = lifecycle_of(self.reg, cid)
                if st != "absent":
                    self.note(f"class:{name}:{st}")
        if self.ids.get("head"):
            m = manifest()
            if final_claims(m["seats"][4]["bond_outpoint"], self.ids["head"]) > 0:
                self.note("head:first-final-claim")
        for line, lid in self.line_ids().items():
            row = line_status(self.status, lid)
            if not row:
                continue
            self.note(f"line:{line}:governed")
            for e in row.get("epochs", []):
                self.note(f"line:{line}:e{e['epoch']}:{e['state']}")
                if e.get("outcome"):
                    self.note(f"line:{line}:e{e['epoch']}:outcome", outcome=e["outcome"])
            for h in row.get("heads", []):
                self.note(f"line:{line}:head#{h['seq']}:{h['cause']}", cls=h["class"][:16])

    def line_ids(self):
        d = {"W1": self.ids.get("head") or ""}
        for k, v in (self.s.get("lines") or {}).items():
            d[k] = v
        return {k: v for k, v in d.items() if v}

    # ----- D-M5: the fence --------------------------------------------------------------------------------------
    def step_m5_below(self):
        """Below the improvement fence an improvement object is dropped by name by the new build and skipped by the old
        one, identical tips (D-F4's pattern, RFC-0004's fence). Run while both builds still agree."""
        if self.s.done("m5-below") or not self.ids.get("head"):
            return
        lack = first_lacking_fence()
        deadline = (lack[1] if lack else IMPROVE_AT) - 3
        if self.daa < 8:
            return
        if self.daa >= deadline:
            self.s.mark("m5-below", result="INCOMPLETE", why=f"DAA {self.daa} is past {deadline}: run on a fresh chain")
            return
        old = "old" in NODES and node_alive("old")
        if not old:
            return
        cur_new = os.path.getsize(f"{WORK}/new0/kaspad.out") if os.path.exists(f"{WORK}/new0/kaspad.out") else 0
        cur_old = os.path.getsize(f"{WORK}/old/kaspad.out") if os.path.exists(f"{WORK}/old/kaspad.out") else 0
        spec = {"line": self.ids["head"], "sequence": 1, "policy": line_policy("W1")}
        out = f"{OBJ}/m5-below-policy.obj"
        rc, text = improve_object("policy", 4, spec, out)
        if rc != 0:
            raise RuntimeError(f"cannot build the policy object: {text.strip()[-300:]}")
        ok, text = submit([out])
        if not ok:
            raise RuntimeError(f"cannot submit: {text.strip()[-300:]}")
        self.s.put("m5_below", {"daa": self.daa, "cur_new": cur_new, "cur_old": cur_old})
        self.s.mark("m5-below", submitted_daa=self.daa)
        log(f"D-M5 below: the policy object submitted at DAA {self.daa}")

    def log_after(self, node, cursor, pattern):
        path = f"{WORK}/{node}/kaspad.out"
        try:
            with open(path, "rb") as f:
                f.seek(cursor)
                data = f.read().decode("utf-8", "replace")
        except OSError:
            return None
        m = re.search(pattern, data)
        return m.group(0) if m else None

    def step_m5_below_verify(self):
        """What the object did on both sides: dropped by name on the new nodes, skipped by the old, one tip."""
        if self.s.done("m5-below-verified") or not self.s.done("m5-below") or self.s.d["done"]["m5-below"].get("result") == "INCOMPLETE":
            return
        cur = self.s.get("m5_below") or {}
        if self.daa < cur.get("daa", 0) + 3:
            return
        lack = first_lacking_fence()
        pat_new = r"Block \S+: .+? was dropped by name below palw_improvement_v1, and the block stands \(RFC-0004\)"
        pat_old = r"\[palw-lifecycle\] carrier .* produced no object"
        got_new = self.log_after("new0", cur.get("cur_new", 0), pat_new)
        got_old = self.log_after("old", cur.get("cur_old", 0), pat_old)
        agree = None
        for _ in range(12):
            a, b = daa_of("new0"), daa_of("old")
            ra, rb = call(jport("new0"), "getBlockDagInfo", {}), call(jport("old"), "getBlockDagInfo", {})
            sa, sb = str(pick(ra, "sink", default="")), str(pick(rb, "sink", default=""))
            if a is not None and a == b and sa and sa == sb:
                agree = (a, sa[:16])
                break
            time.sleep(5)
        os.makedirs(f"{EVID}/dm5", exist_ok=True)
        with open(f"{EVID}/dm5/below.txt", "w") as f:
            f.write(f"new0: {got_new}\nold: {got_old}\ntips: {agree}\n")
        deadline = (lack[1] if lack else IMPROVE_AT) - 1
        n = self.s.tried("m5-below-verify")
        if got_new and got_old and agree and agree[0] < deadline:
            self.s.mark("m5-below-verified", result="PASS", new=got_new, old=got_old, tips=agree)
        elif n >= 6 or self.daa >= deadline:
            self.s.mark("m5-below-verified", result="FAIL" if (got_new is None or got_old is None) else "INCOMPLETE",
                        why=f"new0 {'dropped it' if got_new else 'did NOT log the drop'}; old {'skipped it' if got_old else 'did NOT log the skip'}; tips {agree}")

    def step_m5_cross(self):
        if self.s.done("m5-cross") or not self.s.done("m5-below"):
            return
        lack = first_lacking_fence()
        if not lack or self.daa < lack[1] + 3:
            return
        pat = rf"Fork-id mismatch on network \S+ at DAA \d+ - this node has crossed fence (\d+)"
        line = self.log_after("new0", 0, pat)
        d_old = daa_of("old") if node_alive("old") else None
        info = {"lack": lack, "refusal": line, "old_daa": d_old, "new_daa": self.daa}
        if line is None:
            n = self.s.tried("m5-cross")
            if n >= 10:
                self.s.mark("m5-cross", result="FAIL", why="no fork-id refusal logged by new0 after the fence", **info)
            return
        self.s.mark("m5-cross", result="PASS", **info)

    # ----- candidate classes: ordinary registrations, early, so the lifecycle is done long before the draw -----------
    def step_register_classes(self):
        if self.s.done("register-classes") or not self.ids.get("head") or self.daa < TIR_AT + 4:
            return
        if not self.s.d["milestones"].get("class:head:Candidate") and lifecycle_of(self.reg, self.ids["head"])[0] == "absent":
            return
        objs = []
        wanted = []
        for line_name, row in PLAN["lines"].items():
            for ep in (row.get("epochs") or {}).values():
                for c in ep.get("candidates", []):
                    a = asset_of(c["class"], line_name)
                    if a not in wanted:
                        wanted.append(a)
        for asset in wanted:
            cls = asset
            if is_composite(asset) and not os.path.exists(f"{MODEL}/{asset}.palwtirs"):
                continue
            if not self.ids.get(asset) or lifecycle_of(self.reg, self.ids[asset])[0] != "absent":
                continue
            out = f"{OBJ}/register-{asset}.obj"
            if is_composite(asset):
                art = ["--artifact", f"{MODEL}/{asset}.palwtirs", "--parent", f"{MODEL}/head.class.palwtir"]
            else:
                art = ["--artifact", f"{MODEL}/{asset}.class.palwtir"]
            cmd = cli_prefix("new3") + ["palw", "tir-registration", *art, "--bond", bond_of(7), "--key-file", seed_of(7), "--out", out,
                                                                   "--skip-pack-verify"]   # the release layer gates registration on a runtime pack (RFC-0002 preflight); the drill classes have none, the chain does not enforce it
            rc, text = run(cmd, home=UHOME)
            if rc != 0:
                log(f"register {cls}: cannot build ({text.strip()[-200:]})")
                self.s.put(f"register-note:{cls}", text.strip()[-300:])
                continue
            objs.append(out)
        if not objs:
            self.s.mark("register-classes", note="every class is registered already")
            return
        ok, text = submit(objs)
        if not ok:
            raise RuntimeError(f"cannot submit the registrations: {text.strip()[-300:]}")
        self.s.mark("register-classes", daa=self.daa, objects=objs)
        log(f"classes registered (seat 7) at DAA {self.daa}: {[os.path.basename(o) for o in objs]}")
        span = int(pick(self.reg, "spanDaa", "span_daa", default=0) or 0)
        if span:
            log("admission-audit slots (DAA mod period): " + ", ".join(
                f"{n}={audit_slot(self.ids[n], span)[0]} mod {audit_slot(self.ids[n], span)[1]}" for n in ASSETS if self.ids.get(n)))

    # ----- lines ------------------------------------------------------------------------------------------------
    def step_lines(self):
        # An ordinary spec-15 object (ModelLineFounded): any time after the head's class has a row.
        if self.s.done("lines") or self.daa < TIR_AT + 3 or not self.ids.get("head"):
            return
        if lifecycle_of(self.reg, self.ids["head"])[0] == "absent":
            return
        root = model_id("head", "root")
        lines = dict(self.s.get("lines") or {})
        for name in ("W2", "L", "T", "R"):
            if name in lines:
                continue
            cmd = cli_prefix("new3") + ["palw", "line-found", "--class", self.ids["head"], "--name", name, "--root", root,
                                        "--bond", bond_of(7), "--key-file", seed_of(7), "--yes"]
            rc, text = run(cmd, home=UHOME)
            lid = parse_after(text, "line id")
            if rc != 0 and any(w in text for w in ("already spent", "no mature", "mempool", "orphan")):
                log(f"line-found {name}: the funding output is not free yet; waiting for the previous carrier to be mined")
                return
            if rc != 0 or not lid:
                raise RuntimeError(f"line-found {name}: {text.strip()[-300:]}")
            lines[name] = lid
            self.s.put("lines", lines)
            log(f"line {name} founded by seat 7: {lid[:16]}…")
            if lid != line_id_of(name):
                # D-M3's tamper flags name T by this id, derived before the chain existed: a mismatch is the harness's own bug.
                self.s.put(f"line-id-mismatch:{name}", {"chain": lid, "derived": line_id_of(name)})
                log(f"line {name}: the chain's id {lid[:16]}… is not the derived one {line_id_of(name)[:16]}…")
            time.sleep(SPACING_S)
        self.s.mark("lines", **lines)

    def step_policies(self):
        # The opt-in is an improvement object: below the fence the chain drops it by name, so it waits for the fence.
        if self.s.done("policies") or not self.s.done("lines") or self.daa < IMPROVE_AT + 2:
            return
        lines = self.line_ids()
        if not all(l in lines for l in ("W1", "W2", "L", "T", "R")):
            return
        # the founding line's row exists once the head's class is registered; the founded lines' once their objects are mined
        objs = []
        for name in ("W1", "W2", "L", "T", "R"):
            owner = PLAN["lines"][name]["owner_seat"]
            out = f"{OBJ}/policy-{name}.obj"
            spec = {"line": lines[name], "sequence": 1, "policy": line_policy(name)}
            rc, text = improve_object("policy", owner, spec, out)
            if rc != 0 or "policy check      ok" not in text:
                raise RuntimeError(f"policy {name}: {text.strip()[-300:]}")
            objs.append(out)
        ok, text = submit(objs)
        if not ok:
            raise RuntimeError(f"cannot submit the policies: {text.strip()[-300:]}")
        self.s.mark("policies", daa=self.daa)
        log(f"opt-in: five policy objects submitted at DAA {self.daa}")

    # ----- material (D-M1): a dataset and a hard case, admitted while the line is idle: the next epoch's material -----
    def step_material(self):
        if self.s.done("material") or not self.s.done("policies"):
            return
        row = line_status(self.status, self.line_ids().get("W1"))
        if not row:
            return
        spec = PLAN["lines"]["W1"]["material"]
        line = self.line_ids()["W1"]
        ds = {"line": line, "content_root": "ab" * 64, "items": spec["dataset"]["items"], "license_classes": [],
              "teacher_classes": spec["dataset"]["teacher_classes"], "provenance": "cd" * 64}
        out_ds = f"{OBJ}/dataset-ds1.obj"
        rc, text = improve_object("dataset", spec["dataset"]["seat"], ds, out_ds)
        dsid = parse_after(text, "dataset id")
        if rc != 0 or not dsid:
            raise RuntimeError(f"dataset: {text.strip()[-300:]}")
        pool_a = pool("a")
        hc_item = spec["hard_case"]["item"]
        hc = {"line": line, "domain": 0, "prompt": pool_a["prompts"][hc_item],
              "reference": {"exact_key": pool_a["keys"][hc_item], "salt": "ef" * 64}, "source": "setter"}
        out_hc = f"{OBJ}/hardcase-hc1.obj"
        rc, text = improve_object("hard-case", spec["hard_case"]["seat"], hc, out_hc)
        if rc != 0:
            raise RuntimeError(f"hard case: {text.strip()[-300:]}")
        ok, text = submit([out_ds, out_hc])
        if not ok:
            raise RuntimeError(f"cannot submit the material: {text.strip()[-300:]}")
        self.s.put("ds1", dsid)
        self.s.mark("material", daa=self.daa, dataset=dsid[:16])
        log(f"material submitted at DAA {self.daa} (dataset {dsid[:16]}…)")

    # ----- the epochs -------------------------------------------------------------------------------------------
    def step_epochs(self):
        if not self.s.done("policies"):
            return
        for name, lid in self.line_ids().items():
            row = line_status(self.status, lid)
            if not row:
                continue
            for e in row.get("epochs", []):
                if str(e["epoch"]) in PLAN["lines"][name]["epochs"]:
                    self.guard(f"epoch:{name}:e{e['epoch']}", lambda name=name, lid=lid, e=e: self.act_epoch(name, lid, e))

    def act_epoch(self, name, lid, e):
        epoch = e["epoch"]
        plan = PLAN["lines"][name]["epochs"][str(epoch)]
        key = f"{name}:e{epoch}"
        state = e["state"]
        times = e["times"]
        if state in ("Open", "Submission") and not self.s.done(f"{key}:sets"):
            self.commit_sets(name, lid, epoch, plan, key)
        if state == "Submission" and not self.s.done(f"{key}:candidates"):
            self.submit_candidates(name, lid, epoch, plan, key)
        if state in ("HoldOut", "Evaluating", "Closing", "Decided") and not self.s.done(f"{key}:candidates"):
            self.s.mark(f"{key}:candidates", result="MISSED", why=f"the epoch is {state}: the Submission window passed")
        if state in ("Evaluating", "Closing") and self.s.done(f"{key}:sets") and not self.s.done(f"{key}:prompts"):
            objs = [p for p in (self.s.get(f"{key}:prompt-files") or []) if os.path.exists(p)]
            if objs:
                ok, text = submit(objs)
                if not ok:
                    raise RuntimeError(f"prompts reveal: {text.strip()[-300:]}")
                self.s.mark(f"{key}:prompts", daa=self.daa)
                log(f"{key}: setter prompts revealed at DAA {self.daa}")
        if state == "Closing" and not self.s.done(f"{key}:keys"):
            view = eval_view(self.status, lid, epoch)
            if jobs_settled(view):
                objs = [p for p in (self.s.get(f"{key}:key-files") or []) if os.path.exists(p) and "withhold" not in p]
                if objs:
                    ok, text = submit(objs)
                    if not ok:
                        raise RuntimeError(f"keys reveal: {text.strip()[-300:]}")
                    self.s.mark(f"{key}:keys", daa=self.daa)
                    log(f"{key}: setter keys revealed at DAA {self.daa} (every claim is final)")
        if state == "Decided":
            self.note(f"line:{name}:e{epoch}:decided", outcome=e.get("outcome"), counts=[c.get("counts") for c in e.get("candidates", [])])

    def commit_sets(self, name, lid, epoch, plan, key):
        objs, prompt_files, key_files = [], [], []
        for s in plan["sets"]:
            sl = pool_slice(s["pool"], s["items"][0], s["items"][1])
            out = f"{OBJ}/set-{name}-e{epoch}-{s['name']}.obj"
            spec = {"line": lid, "epoch": epoch, "prompts": sl["prompts"], "keys": sl["keys"]}
            rc, text = improve_object("setter-set", s["seat"], spec, out)
            if rc != 0:
                raise RuntimeError(f"setter set {s['name']}: {text.strip()[-300:]}")
            objs.append(out)
            if s.get("withhold"):
                self.s.put(f"{key}:withheld", [*(self.s.get(f"{key}:withheld") or []), s["name"]])
                continue  # committed, never revealed: D-M6's withholding setter
            prompt_files.append(out + ".prompts")
            key_files.append(out + ".keys")
        ok, text = submit(objs)
        if not ok:
            raise RuntimeError(f"cannot submit the setter sets: {text.strip()[-300:]}")
        self.s.put(f"{key}:prompt-files", prompt_files)
        self.s.put(f"{key}:key-files", key_files)
        self.s.mark(f"{key}:sets", daa=self.daa, n=len(objs))
        log(f"{key}: {len(objs)} setter set(s) committed at DAA {self.daa}")

    def submit_candidates(self, name, lid, epoch, plan, key):
        objs = []
        for c in plan["candidates"]:
            out = f"{OBJ}/candidate-{name}-e{epoch}-{c['class']}.obj"
            decl = {}
            if c.get("datasets"):
                decl["datasets"] = [[self.s.get(d) or "", w] for d, w in c["datasets"]]
            if c.get("teacher_classes"):
                decl["teacher_classes"] = c["teacher_classes"]
            spec = {"line": lid, "epoch": epoch, "declarations": decl}
            rc, text = improve_object("candidate", c["seat"], spec, out, extra=class_container(c["class"], name))
            if rc != 0:
                raise RuntimeError(f"candidate {c['class']}: {text.strip()[-300:]}")
            objs.append(out)
        ok, text = submit(objs)
        if not ok:
            raise RuntimeError(f"cannot submit the candidates: {text.strip()[-300:]}")
        self.s.mark(f"{key}:candidates", daa=self.daa, n=len(objs))
        log(f"{key}: {len(objs)} candidate(s) submitted at DAA {self.daa}")

    # ----- rollbacks (D-M4) -------------------------------------------------------------------------------------
    def step_rollbacks(self):
        lines = self.line_ids()
        # by the owner: W2's promotion of epoch 1, rolled back as soon as it is decided
        self.guard("rollback-owner", lambda: self.rollback_owner(lines))
        self.guard("rollback-proof", lambda: self.rollback_proof(lines))

    def rollback_owner(self, lines):
        if not self.s.done("rollback-owner") and "W2" in lines:
            row = line_status(self.status, lines["W2"])
            e1 = epoch_row(row, 1)
            if e1 and e1["state"] == "Decided" and str(e1.get("outcome", "")).startswith("Promoted"):
                out = f"{OBJ}/rollback-owner-W2.obj"
                spec = {"line": lines["W2"], "epoch": 1, "to_class": self.ids["head"], "cause": "owner"}
                rc, text = improve_object("rollback", 7, spec, out)
                if rc != 0:
                    raise RuntimeError(f"rollback (owner): {text.strip()[-300:]}")
                ok, text = submit([out])
                if not ok:
                    raise RuntimeError(f"cannot submit the rollback: {text.strip()[-300:]}")
                self.s.mark("rollback-owner", daa=self.daa)
                log(f"D-M4: W2's promotion rolled back by its owner at DAA {self.daa}")

    def rollback_proof(self, lines):
        # by proof: line R's second epoch showed the predecessor H beating the promoted W (R is the full-weight line that carries it: a
        # composite epoch is 363 DAA, two of them do not fit the run)
        if not self.s.done("rollback-proof") and "R" in lines:
            row = line_status(self.status, lines["R"])
            e2 = epoch_row(row, 2)
            if e2 and e2["state"] == "Decided" and (e2.get("previous_counts") or {}).get("eligible"):
                out = f"{OBJ}/rollback-proof-R.obj"
                spec = {"line": lines["R"], "epoch": 1, "to_class": self.ids["head"], "cause": {"later_regression": 2}}
                rc, text = improve_object("rollback", 7, spec, out)
                if rc != 0:
                    raise RuntimeError(f"rollback (proof): {text.strip()[-300:]}")
                ok, text = submit([out])
                if not ok:
                    raise RuntimeError(f"cannot submit the rollback: {text.strip()[-300:]}")
                self.s.mark("rollback-proof", daa=self.daa)
                log(f"D-M4: R's promotion rolled back by proof (epoch 2's regression check) at DAA {self.daa}")

    # ----- D-M6: the attacks ------------------------------------------------------------------------------------
    def step_attacks(self):
        """Each attack once, at the state where it is meant to bite; the verdict reads what the chain did."""
        lines = self.line_ids()
        if "W1" in lines:
            row = line_status(self.status, lines["W1"])
            e1 = epoch_row(row, 1)
            if e1:
                self.guard("attack:copy", lambda: self.attack_copy(lines["W1"], e1))
                self.guard("attack:late", lambda: self.attack_late(lines["W1"], e1))
        # The attacks that could cost a line its epoch run on L, whose expected outcome (NoChange) they cannot spoil: the early
        # keys in its first epoch, the hold-out flood in its second (the flood costs the epoch its items, so it never runs on a
        # line whose promotion another drill needs).
        if "L" in lines:
            row = line_status(self.status, lines["L"])
            e1, e2 = epoch_row(row, 1), epoch_row(row, 2)
            if e1:
                self.guard("attack:early-keys", lambda: self.attack_early_keys(lines["L"], e1))
            if e2:
                self.guard("attack:holdout-spam", lambda: self.attack_holdout_spam(lines["L"], e2))

    def attack_copy(self, lid, e1):
        """A copy of the entered candidate by another bond, in the same window: refused (the class is entered)."""
        k = "attack:copy"
        if self.s.done(k) or e1["state"] != "Submission" or not self.s.done("W1:e1:candidates"):
            return
        before = e1["counts"]["candidates"]
        expected = len(PLAN["lines"]["W1"]["epochs"]["1"]["candidates"])  # what the plan enters: the copy must add nothing
        out = f"{OBJ}/attack-copy.obj"
        rc, text = improve_object("candidate", 3, {"line": lid, "epoch": 1, "declarations": {}}, out, extra=class_container("win", "W1"))
        if rc != 0:
            self.s.mark(k, result="INCOMPLETE", why=f"cannot build: {text.strip()[-200:]}")
            return
        ok, text = submit([out])
        self.s.mark(k, submitted_daa=self.daa, candidates_before=before, expected=expected, ok=ok)
        log(f"D-M6 copy: a copy of `win` by seat 3 submitted at DAA {self.daa}")

    def attack_late(self, lid, e1):
        k = "attack:late"
        if self.s.done(k) or e1["state"] not in ("HoldOut", "Drawing") or not self.s.done("W1:e1:candidates"):
            return
        out = f"{OBJ}/attack-late.obj"
        rc, text = improve_object("candidate", 7, {"line": lid, "epoch": 1, "declarations": {}}, out, extra=class_container("lose", "W1"))
        if rc != 0:
            self.s.mark(k, result="INCOMPLETE", why=f"cannot build: {text.strip()[-200:]}")
            return
        ok, text = submit([out])
        self.s.mark(k, submitted_daa=self.daa, candidates_before=e1["counts"]["candidates"],
                    expected=len(PLAN["lines"]["W1"]["epochs"]["1"]["candidates"]), ok=ok)
        log(f"D-M6 late: a candidate after t_close submitted at DAA {self.daa}")

    def attack_early_keys(self, lid, e1):
        """The keys of a set revealed while the outputs are not final: they must stay hidden (spec 17 §17.8.2)."""
        k = "attack:early-keys"
        if self.s.done(k) or e1["state"] != "Evaluating" or not self.s.done("L:e1:prompts"):
            return
        files = [p for p in (self.s.get("L:e1:key-files") or []) if os.path.exists(p)]
        if not files:
            return
        cursor = {n: (os.path.getsize(f"{WORK}/{n}/kaspad.out") if os.path.exists(f"{WORK}/{n}/kaspad.out") else 0) for n in ("new0", "new3")}
        ok, text = submit(files[:1])
        self.s.mark(k, submitted_daa=self.daa, ok=ok, cursor=cursor)
        log(f"D-M6 early keys: the keys object submitted while Evaluating, DAA {self.daa}")

    def attack_holdout_spam(self, lid, e1):
        """Hold-out hard cases flooding the pool in HoldOut (D-M6 fee DoS / grinding): 4n is the pool's ceiling, each costs
        hard_case_fee, and a case whose key is never revealed is dropped from the draw — the cost of killing an epoch."""
        k = "attack:holdout-spam"
        if self.s.done(k) or e1["state"] != "HoldOut":
            return  # (e1 is the epoch the flood runs in: L's second)
        n_cases = 40
        objs = []
        for i in range(n_cases):
            out = f"{OBJ}/attack-holdout-{i}.obj"
            spec = {"line": lid, "domain": 0, "prompt": [1, 3 + i, 4 + i, 5 + i],
                    "reference": {"exact_key": [3 + i], "salt": f"{i:02x}" * 64}, "source": "setter"}
            rc, text = improve_object("hard-case", 3, spec, out)
            if rc != 0:
                break
            objs.append(out)
        if not objs:
            self.s.mark(k, result="INCOMPLETE", why="cannot build the cases")
            return
        before = e1["counts"]["holdout_cases"]
        ok, text = submit(objs[:20], tries=3)
        ok2, text2 = submit(objs[20:], tries=3) if len(objs) > 20 else (True, "")
        self.s.mark(k, submitted_daa=self.daa, built=len(objs), holdout_before=before, ok=ok and ok2)
        log(f"D-M6 hold-out spam: {len(objs)} hold-out cases submitted at DAA {self.daa}")


    def step_seat_operator_ids(self):
        """Lane D's scenarios (`seat_field <n> operator_id`) read a genesis seat's operator id from the keyring manifest, which the keyring writer does not put there
        (it has the operator PUBKEY). Read each seat's id from the chain once and add it as `operator_id` to the manifest's seats — an added field, nothing else changes."""
        if self.s.done("seat-operator-ids") or self.daa < 2 or not self.ids.get("head"):
            return
        m = manifest()
        ids = []
        for seat in m["seats"]:
            t, i = seat["bond_outpoint"].rsplit(":", 1)
            f = rpc("getPalwProducerFacts", {"classId": self.ids["head"], "bondTransactionId": t, "bondIndex": int(i), "withBond": True})   # the release layer reads the bond only under a class id
            op = str(pick(f, "bondOperatorId", default="") or "")
            if not op:
                return
            ids.append(op)
        for seat, op in zip(m["seats"], ids):
            seat["operator_id"] = op
        tmp = f"{KR}/manifest.json.partial"
        with open(tmp, "w") as fh:
            json.dump(m, fh, indent=2)
        os.chmod(tmp, 0o600)
        os.replace(tmp, f"{KR}/manifest.json")
        self.s.mark("seat-operator-ids", seats=len(ids))
        log(f"the keyring manifest names {len(ids)} seats' operator ids (lane D's seat_field reads them)")

    # ----- the memory sampler and tripwire ---------------------------------------------------------------------------------
    def step_memory(self):
        """Every tick: each node's RSS and the Mac's free-memory percentage, appended to $WORK/memory.tsv. The first hour's steady figure and the peak are kept in
        the state (the coordinator's number to size the run by). The tripwire protects the seats: the Mac's free memory under MEM_TRIP_PCT for two ticks running stops
        the SACRIFICIAL nodes (the registrar first, then the liars), under MEM_TRIP2_PCT the old relay as well (after D-M5). It never touches a seat or a clock."""
        rc, out = run(["memory_pressure"], timeout=60)
        free = parse_free_pct(out)
        rows = {}
        for n in NODES:
            pid = read(f"{WORK}/{n}/kaspad.pid")
            if pid and node_alive(n):
                r = rss_mib(pid)
                if r is not None:
                    rows[n] = r
        total = sum(rows.values())
        with open(f"{WORK}/memory.tsv", "a") as f:
            f.write(f"{now()}\t{self.daa}\t{free}\t{total}\t{json.dumps(rows, sort_keys=True)}\n")
        mem = dict(self.s.get("mem") or {"ticks": 0, "first_hour": [], "peak_total_mib": 0, "peak_nodes": 0, "min_free_pct": None, "low_ticks": 0})
        mem["ticks"] += 1
        if mem["ticks"] <= MEM_FIRST_HOUR_TICKS and rows:
            mem["first_hour"].append([total, len(rows)])
        if total > mem["peak_total_mib"]:
            mem["peak_total_mib"], mem["peak_nodes"] = total, len(rows)
        if free is not None and (mem["min_free_pct"] is None or free < mem["min_free_pct"]):
            mem["min_free_pct"] = free
        mem["low_ticks"] = mem["low_ticks"] + 1 if (free is not None and free < MEM_TRIP_PCT) else 0
        if mem["first_hour"]:
            steady = max(mem["first_hour"], key=lambda x: x[0])
            mem["first_hour_summary"] = {"max_total_mib": steady[0], "nodes": steady[1], "per_node_mib": round(steady[0] / max(steady[1], 1)), "samples": len(mem["first_hour"])}
        if mem["low_ticks"] >= 2:
            victims = [n for n in ("reg", "new7", "new8") if n in rows]
            if free is not None and free < MEM_TRIP2_PCT and self.s.done("m5-cross") and "old" in rows:
                victims.append("old")
            for n in victims[:1] if free is not None and free >= MEM_TRIP2_PCT else victims:
                self.nodes_sh("stop", n)
                self.s.mark(f"tripwire:{n}:{self.daa}", free_pct=free, total_mib=total)
                if n in TAMPER and not self.s.done(f"liar-stop:{n}"):
                    self.s.mark(f"liar-stop:{n}", daa=self.daa, why=f"memory tripwire: the Mac's free memory was {free}%")
                log(f"TRIPWIRE: the Mac's free memory is {free}% (< {MEM_TRIP_PCT}%): stopped {n} (RSS {rows.get(n)} MiB) at DAA {self.daa}")
        self.s.put("mem", mem)

    # ----- post-genesis bonds: the liars' (8, 9) and lane D's (10..13) -------------------------------------------------
    def nodes_sh(self, *args, timeout=240):
        """`nodes.sh start|stop <node>` (the harness's own: memory gate, salted genesis line, SIGINT stop) with this driver's environment."""
        return run(["bash", f"{HERE}/nodes.sh", *args], timeout=timeout)

    def xb_collateral(self):
        c = self.s.get("xbond:collateral")
        if c:
            return int(c)
        r = rpc("getPalwClaims", {"bond": bond_of(0), "role": "executor", "includeTerminal": True, "limit": 0})
        c = int(pick(r, "bondCollateral", default=0) or 0)
        if c <= 0:
            raise RuntimeError("cannot read seat 0's collateral to size the new bonds by")
        self.s.put("xbond:collateral", c)
        return c

    def utxos_at(self, addr):
        r = rpc("getUtxosByAddresses", {"addresses": [addr]})
        out = []
        for e in pick(r, "entries", default=[]) or []:
            u = pick(e, "utxoEntry", default={}) or {}
            op = pick(e, "outpoint", default={}) or {}
            out.append((f"{pick(op, 'transactionId')}:{pick(op, 'index')}", int(pick(u, "amount", default=0) or 0)))
        return out

    def step_extra_bonds(self):
        """One post-genesis bond at a time, in XB_ORDER: fund its pay address from the main wallet (collateral as a genesis seat's + a fee float), run the
        registrar (`kaspad --palw-register-bond`, which prints the bond and then KEEPS RUNNING as a node: the driver stops it), read the operator id from the
        chain, write $WORK/liars/bond-<n>.json (lane D's `liar_field` and the liar nodes' argv read it). The registrar's directory is archived between bonds
        (a remembered change outpoint belongs to the previous key)."""
        if self.daa < XB_FROM_DAA:
            return
        todo = [n for n in XB_ORDER if extra_bond(n) is None]
        if not todo:
            return
        n = todo[0]
        key = f"xbond:{n}"
        st = dict(self.s.get(key) or {"stage": "fund", "attempts": 0})
        row = manifest()["bonds"][n - GENESIS_SEATS]
        addr = row["address"]
        if st["stage"] == "fund":
            collateral = self.xb_collateral()
            have = sum(a for _, a in self.utxos_at(addr))
            if have < collateral:
                node = (live_nodes(("seat", "floor", "head", "eval", "evalw")) or ["new3"])[0]
                cmd = cli_prefix(node) + ["wallet", "send", "--to", addr, "--amount", msk_text(collateral + XB_FLOAT_MSK * 10**8), "--yes", "--key-file", f"{KR}/main-0.seed"]
                rc, out = run(cmd, home=UHOME)
                if rc != 0:
                    if "already spent" in out or "no mature" in out or "mempool" in out or "insufficient mature" in out:
                        log(f"bond {n}: the main wallet's funding is not free yet ({out.strip().splitlines()[-1][:100] if out.strip() else ''}); next tick")
                        return
                    raise RuntimeError(f"wallet send for bond {n}: {out.strip()[-300:]}")
                log(f"bond {n}: funded {msk_text(collateral + XB_FLOAT_MSK * 10**8)} MSK at {addr[:24]}… at DAA {self.daa}")
                time.sleep(SPACING_S)
            st.update(stage="funded", funded_daa=self.daa, collateral=collateral)
            self.s.put(key, st)
            return
        if st["stage"] == "funded":
            if sum(a for _, a in self.utxos_at(addr)) < int(st["collateral"]):
                if self.daa > st["funded_daa"] + 20:
                    st.update(stage="fund")      # the transfer never confirmed (dropped): send again
                    self.s.put(key, st)
                return
            d = f"{WORK}/reg"
            if os.path.isdir(d) and os.path.exists(f"{d}/kaspad.pid") and node_alive("reg"):
                self.nodes_sh("stop", "reg")
            if os.path.isdir(d) and os.path.isdir(f"{d}/app"):
                os.replace(d, f"{d}.done-{int(time.time())}")
            os.makedirs(d, exist_ok=True)
            with open(f"{d}/extra-args", "w") as f:
                f.write("\n".join(["--palw-register-bond", f"--palw-producer-key={KR}/bond-{n}.seed", f"--palw-producer-pay-address={addr}",
                                   f"--palw-bond-collateral={int(st['collateral'])}"]) + "\n")
            cursor = os.path.getsize(f"{d}/kaspad.out") if os.path.exists(f"{d}/kaspad.out") else 0
            rc, out = self.nodes_sh("start", "reg", timeout=300)
            if rc != 0 and "not starting" in out:       # the harness's memory gate: try again next tick
                log(f"bond {n}: the registrar is not started yet ({out.strip().splitlines()[-1][:100]})")
                return
            if rc != 0:
                st["attempts"] = int(st.get("attempts", 0)) + 1
                self.s.put(key, st)
                raise RuntimeError(f"cannot start the registrar for bond {n}: {out.strip()[-200:]}")
            st.update(stage="registrar", start_daa=self.daa, cursor=cursor)
            self.s.put(key, st)
            log(f"bond {n}: the registrar node is up at DAA {self.daa}")
            return
        if st["stage"] == "registrar":
            pat = r"registered bond ([0-9a-f]+):(\d+) with (\d+) sompi of collateral, in tx ([0-9a-f]+)"
            got = self.log_after("reg", int(st["cursor"]), pat)
            m = re.search(pat, got or "")
            if not m:
                if self.daa > int(st["start_daa"]) + XB_WAIT_DAA:
                    self.nodes_sh("stop", "reg")
                    st["attempts"] = int(st.get("attempts", 0)) + 1
                    st.update(stage="funded")
                    self.s.put(key, st)
                    log(f"bond {n}: the registrar printed no bond in {XB_WAIT_DAA} DAA; stopped (attempt {st['attempts']}), trying again")
                    if st["attempts"] >= 4:
                        self.s.fail("step_extra_bonds", f"bond {n} could not be registered in 4 attempts (see {WORK}/reg*/kaspad.out)")
                return
            bond = f"{m.group(1)}:{m.group(2)}"
            collateral = int(m.group(3))
            self.nodes_sh("stop", "reg")
            facts = rpc("getPalwProducerFacts", {"classId": self.ids.get("head") or "", "bondTransactionId": m.group(1), "bondIndex": int(m.group(2)), "withBond": True})
            op = str(pick(facts, "bondOperatorId", default="") or "")
            spendable = [(o, a) for o, a in self.utxos_at(addr) if a < collateral and a >= 10**8 and o != bond]
            if not op or not spendable:
                raise RuntimeError(f"bond {n} {bond}: operator id '{op[:12]}' / fee float {spendable} not readable yet")
            fee = max(spendable, key=lambda x: x[1])[0]
            write_json(liar_path(n), {"n": n, "bond_outpoint": bond, "seed_file": f"{KR}/bond-{n}.seed", "operator_id": op, "fee_outpoint": fee,
                                      "address": addr, "collateral_sompi": collateral, "registered_daa": self.daa, "tx": m.group(4)})
            self.s.mark(f"xbond:{n}:registered", bond=bond, daa=self.daa)
            log(f"bond {n}: registered {bond[:20]}… ({collateral} sompi), fee float {fee[:20]}…")

    # ----- D-M3's sacrificial liar nodes: started just before the window, stopped after the conviction -----------------------
    def liar_status(self, node):
        """(convicted?, why) from the D-M3 probe record: the node's lying claim is Voided and no claim of its bond is still live."""
        p = self.s.get("dm3") or {}
        rows = ((p.get("claims") or {}).get(node) or {}).get("rows", [])
        fraud = [r for r in rows if r["phase"].startswith("voided") and "court_fraud" in (r["void"] or "")]
        live = [r for r in rows if r["phase"] in LIVE_PHASES]
        return (bool(fraud) and not live), f"{len(fraud)} court_fraud, {len(live)} live"

    def step_liars_jit(self):
        if not LIARS:
            return
        for node in TAMPER:
            if node not in NODES:
                continue
            seat = NODES[node]["seat"]
            ks, kp = f"liar-start:{node}", f"liar-stop:{node}"
            if not self.s.done(ks):
                if self.daa < LIAR_START_DAA or extra_bond(seat) is None:
                    if self.daa >= LIAR_START_DAA and extra_bond(seat) is None and not self.s.get(f"liar-wait:{node}"):
                        self.s.put(f"liar-wait:{node}", True)
                        log(f"{node}: its bond {seat} is not registered at DAA {self.daa}: the node waits for it (D-M3 cannot run without it)")
                    continue
                rc, out = self.nodes_sh("start", node, timeout=300)
                if rc != 0:
                    if "not starting" in out:           # the harness's memory gate: try again next tick, never a step failure
                        log(f"{node}: not started yet ({out.strip().splitlines()[-1][:100]})")
                        continue
                    raise RuntimeError(f"cannot start {node}: {out.strip()[-200:]}")
                self.s.mark(ks, daa=self.daa)
                log(f"D-M3: the liar node {node} started at DAA {self.daa}")
                continue
            if self.s.done(kp):
                continue
            started = int(self.s.d["done"][ks]["daa"])
            convicted, why = self.liar_status(node)
            lied = (self.s.get("dm3-liars") or {}).get(node)
            if convicted:
                reason = f"convicted ({why})"
            elif self.daa >= started + LIAR_DEADLINE_DAA:
                reason = f"deadline: {'it lied' if lied else 'it NEVER lied'} and was {'not ' if not convicted else ''}convicted by DAA {self.daa} ({why})"
            else:
                continue
            self.nodes_sh("stop", node)
            self.s.mark(kp, daa=self.daa, why=reason)
            log(f"D-M3: the liar node {node} stopped at DAA {self.daa}: {reason}")

    # ----- the capacity line: ρ=25 and ρ=100 windows measured on the same chain ---------------------------------------------
    def step_capacity(self):
        lo_all = min(w[0] for w in CAP_WINDOWS.values())
        hi_all = max(w[1] for w in CAP_WINDOWS.values())
        if "cap-armed" not in self.s.d["data"]:
            rc, h = run([KASPAD, "--help"], timeout=60)
            self.s.put("cap-armed", {fence: (f"--palw-drill-{fence.replace('_', '-')}-at" in h or ("--palw-drill-int11-at" in h and E.get("INT11", "1") == "1")) for _, fence in CAP_STEPS})
        for name, fence in CAP_STEPS:
            at = CAP_WINDOWS[name][0] - int(CAP.get("settle_daa", 10))
            if self.daa >= at:
                self.note(f"fence:{fence}")
        claims = dict(self.s.get("cap-claims") or {})
        series = list(self.s.get("cap-series") or [])
        sampling = any(lo - 3 <= self.daa < hi + 1 for lo, hi in CAP_WINDOWS.values())
        if sampling:
            backlog, occ, rsv = 0, [], None
            for node, seat in seat_bonds():
                bond = bond_of(seat)
                for c in claims_of(bond):
                    cid = str(pick(c, "claimId", default=""))
                    ph = str(pick(c, "phase", default=""))
                    rec = claims.setdefault(cid, {"acc": int(pick(c, "acceptedDaa", default=0) or 0), "lic": None, "fin": None})
                    if ph == "receipt_licensed" and rec["lic"] is None:
                        rec["lic"] = int(pick(c, "phaseDaa", default=self.daa) or self.daa)
                    elif ph == "final" and rec["fin"] is None:
                        rec["fin"] = int(pick(c, "phaseDaa", default=self.daa) or self.daa)
                    if ph in UNLICENSED_PHASES:
                        backlog += 1
                r = rpc("getPalwClaims", {"bond": bond, "role": "seat", "includeTerminal": False, "limit": 0})
                occ.append(sum(1 for c in (pick(r, "claims", default=[]) or []) if str(pick(c, "phase", default="")) in LIVE_PHASES))
                if seat == 4 and self.ids.get("head"):
                    t, i = bond.rsplit(":", 1)
                    f = rpc("getPalwProducerFacts", {"classId": self.ids["head"], "bondTransactionId": t, "bondIndex": int(i), "withBond": True})
                    res, ceil = int(pick(f, "bondReservedExposure", default=0) or 0), int(pick(f, "bondExposureCeiling", default=0) or 0)
                    rsv = round(res / ceil, 4) if ceil else None
            row = {"daa": self.daa, "backlog": backlog, "occ_max": max(occ, default=0), "occ_mean": round(sum(occ) / max(len(occ), 1), 2), "reserved_ratio": rsv}
            series.append(row)
            self.s.put("cap-claims", claims)
            self.s.put("cap-series", series)
            os.makedirs(f"{WORK}/capacity", exist_ok=True)
            with open(f"{WORK}/capacity/samples.tsv", "a") as f:
                f.write(f"{now()}\t{json.dumps(row)}\n")
        summ = dict(self.s.get("cap-summary") or {})
        changed = False
        for name, (lo, hi) in CAP_WINDOWS.items():
            if name not in summ and self.daa >= hi + 1 and any(lo <= r["daa"] < hi for r in series):
                summ[name] = capacity_summary(series, claims, lo, hi)
                changed = True
                write_json(f"{WORK}/capacity/{name}.json", summ[name])
                log(f"CAPACITY {name} window {lo}..{hi}: {json.dumps(summ[name])}")
        if changed:
            self.s.put("cap-summary", summ)
        if self.daa > hi_all + 5 and "cap-done" not in self.s.d["done"]:
            self.s.mark("cap-done")

    # ----- verdicts ---------------------------------------------------------------------------------------------
    def write_verdicts(self):
        for name, fn in (("dm5", self.verdict_dm5), ("dm1", self.verdict_dm1), ("dm2", self.verdict_dm2), ("dm3", self.verdict_dm3),
                         ("dm4", self.verdict_dm4), ("dm6", self.verdict_dm6), ("cap", self.verdict_cap)):
            try:
                v, why = fn()
            except Exception as e:  # noqa: BLE001
                v, why = "INCOMPLETE", f"verdict logic: {type(e).__name__}: {e}"
            path = f"{VERDICTS}/{name}.verdict"
            text = f"{v}\t{why}\t(DAA {self.daa}, {now()})\n"
            if read(path) != text.strip():
                with open(path, "w") as f:
                    f.write(text)

    def verdict_dm5(self):
        return verdict_dm5(self.s.d)

    def verdict_dm1(self):
        return verdict_dm1(self.s.d, self.status, self.line_ids(), self.ids_of("W1"), self.daa)

    def verdict_dm2(self):
        return verdict_dm2(self.s.d, self.status, self.line_ids())

    def verdict_dm3(self):
        return verdict_dm3(self.s.d, self.line_ids())

    # ----- D-M3: what the lying executors did, what the challenger found, what the chain made of it -------------------------
    def step_dm3_probe(self):
        """Read, once a tick and only from the fence on: each lying executor's log (which evaluation it lied about, with which fault), its bond and claims
        (phases, void reasons, slashed), and the challenger's disputes (its status file). The verdict is pure over this record."""
        if self.daa < IMPROVE_AT or "T" not in self.line_ids():
            return
        liars = dict(self.s.get("dm3-liars") or {})
        probe = {"daa": self.daa, "liars": liars, "claims": {}, "disputes": [], "mismatch": self.s.get("line-id-mismatch:T")}
        pat = re.compile(r"DRILL: this node LIES about item (\d+) of epoch (\d+) of line (\S+) for [^:]*: (\S+) \(--palw-drill-tamper-eval\)")
        for node in TAMPER:
            if node not in NODES:
                continue
            if node not in liars:
                # The lie is said once, in the node's log: read what is new since the last tick (a drill's log is long), once found keep it.
                path = f"{WORK}/{node}/kaspad.out"
                cursor = int((self.s.get("dm3-cursor") or {}).get(node, 0))
                try:
                    with open(path, "rb") as f:
                        f.seek(cursor)
                        data = f.read()
                except OSError:
                    data = b""
                m = pat.search(data.decode("utf-8", "replace"))
                if m:
                    liars[node] = {"item": int(m.group(1)), "epoch": int(m.group(2)), "line": m.group(3), "fault": m.group(4)}
                    self.s.put("dm3-liars", liars)
                else:
                    # keep a margin so a line split across two reads is found by the next one
                    cur = dict(self.s.get("dm3-cursor") or {})
                    cur[node] = max(cursor + len(data) - 512, 0)
                    self.s.put("dm3-cursor", cur)
            seat = NODES[node]["seat"]
            if seat is None or (seat >= GENESIS_SEATS and extra_bond(seat) is None):
                continue
            r = rpc("getPalwClaims", {"bond": bond_of(seat), "role": "executor", "includeTerminal": True, "limit": 0})
            rows = [{"id": str(pick(c, "claimId", default="")), "phase": str(pick(c, "phase", default="")), "void": str(pick(c, "voidReason", default="")),
                     "acc": int(pick(c, "acceptedDaa", default=0) or 0), "pdaa": int(pick(c, "phaseDaa", default=0) or 0)}
                    for c in (pick(r, "claims", default=[]) or [])]
            probe["claims"][node] = {"slashed": int(pick(r, "bondSlashed", default=0) or 0), "collateral": int(pick(r, "bondCollateral", default=0) or 0), "rows": rows}
            # What AG-2 must leave alone: the claims already Final (remembered, so a Final that turns into anything else is seen).
            seen = set(self.s.get(f"dm3-final-seen:{node}") or [])
            now_final = {c["id"] for c in rows if phase_is(c["phase"], "final")}
            if now_final - seen:
                self.s.put(f"dm3-final-seen:{node}", sorted(seen | now_final))
        f = status_file_of(CHALLENGER) if CHALLENGER in NODES else None
        if f:
            try:
                st = json.load(open(f))
                probe["disputes"] = st.get("disputes", [])
                probe["eval_rows"] = eval_rows_of(st, self.line_ids().get("T"))
                trow = line_status(st, self.line_ids().get("T"))
                er = epoch_row(trow, 1) if trow else None
                if er:
                    probe["t_epoch"] = {"epoch": 1, "state": er.get("state"), "outcome": er.get("outcome")}
                # Which job each claim held, remembered: a conviction frees the job, and the verdict asks whether somebody claimed it again.
                jobs = dict(self.s.get("dm3-jobs") or {})
                grew = False
                for r in probe["eval_rows"]:
                    if r.get("claim") and r["claim"] not in jobs:
                        jobs[r["claim"]] = [r["epoch"], r["item"], r["subject"]]
                        grew = True
                if grew:
                    self.s.put("dm3-jobs", jobs)
            except (OSError, ValueError):
                pass
        self.s.put("dm3", probe)

    def verdict_dm4(self):
        return verdict_dm4(self.s.d, self.status, self.line_ids(), self.ids_of("R"))

    def ids_of(self, line):
        """The class ids a line's verdict reads by the plan's names: `head`, and `win` / `lose` as that line's form holds them."""
        return {"head": self.ids.get("head", ""), "win": self.ids.get(asset_of("win", line), ""), "lose": self.ids.get(asset_of("lose", line), "")}

    def verdict_dm6(self):
        return verdict_dm6(self.s.d, self.status, self.line_ids())

    def verdict_cap(self):
        return verdict_cap(self.s.d)


# ---------------------------------------------------------------------------------------------------------------------
# verdict logic — pure over the persisted state and the status JSON (selftest feeds it synthetic data)
# ---------------------------------------------------------------------------------------------------------------------
def v_all(checks):
    """checks: list of (ok: bool|None, text). None = not decidable yet."""
    bad = [t for ok, t in checks if ok is False]
    wait = [t for ok, t in checks if ok is None]
    good = [t for ok, t in checks if ok]
    if bad:
        return "FAIL", "; ".join(bad)
    if wait:
        return "INCOMPLETE", "waiting: " + "; ".join(wait)
    return "PASS", "; ".join(good)


def verdict_dm5(sd):
    below, ver, cross = sd["done"].get("m5-below"), sd["done"].get("m5-below-verified"), sd["done"].get("m5-cross")
    if below is None:
        return "INCOMPLETE", "the below-the-fence half has not run yet"
    if below.get("result") == "INCOMPLETE":
        return "INCOMPLETE", below.get("why", "")
    if ver is None:
        return "INCOMPLETE", f"below: the object went in at DAA {below.get('submitted_daa')}; what the nodes did with it is not read yet"
    if ver.get("result") != "PASS":
        return ("FAIL" if ver.get("result") == "FAIL" else "INCOMPLETE"), "below: " + ver.get("why", "")
    if cross is None:
        return "INCOMPLETE", f"below: PASS (dropped by name, skipped by the old build, one tip); the crossing is not reached"
    if cross.get("result") != "PASS":
        return "FAIL", cross.get("why", "no refusal")
    lack = cross["lack"]
    note = "" if lack[0] == "improve" else (f" (the old release is int-10: the first fence it lacks is {lack[0]}@{lack[1]}, the flag day that follows it on testnet-12; the improvement fence "
                                             f"comes after it, so the crossing shown is that flag day's, refused by the fork id as every later one would be)")
    return "PASS", f"below: dropped by name / skipped / one tip at DAA {ver['tips'][0]}; crossed fence {lack[1]}: {cross['refusal']}{note}"


def _outcome_class(outcome):
    m = re.search(r"class_id: ([0-9a-f]{16})", outcome or "")
    return m.group(1) if m else None


def verdict_dm1(sd, st, lines, ids, daa):
    if not st or "W1" not in lines:
        return "INCOMPLETE", "no improvement status yet"
    row = line_status(st, lines["W1"])
    if not row:
        return "INCOMPLETE", "W1 is not governed yet"
    e1 = epoch_row(row, 1)
    grid = line_windows("W1")["grid"]
    checks = []
    checks.append((True if "head:first-final-claim" in sd["milestones"] else None, "usage: the head's first Final claim"))
    if not e1:
        checks.append((None, "epoch 1 has not opened (the trigger waits for a grid boundary after the first Final claim)"))
        return v_all(checks)
    t_open = e1["times"]["t_open"]
    checks.append((t_open % grid == 0, f"OPEN at the grid boundary {t_open}"))
    first_final = (sd["milestones"].get("head:first-final-claim") or {}).get("daa")
    checks.append((None if first_final is None else first_final <= t_open, f"the trigger preceded OPEN (first Final claim at DAA {first_final})"))
    dsroot = e1.get("dataset_root")
    checks.append((None if e1["state"] == "Open" else bool(dsroot), "material: dataset_root fixed at t_fix"))
    cands = e1.get("candidates", [])
    win = ids.get("win", "")
    mine = [c for c in cands if c["class"] == win]
    checks.append((None if e1["state"] in ("Open", "Submission") else bool(mine), "the candidate (win) is in the frozen set"))
    if e1["state"] != "Decided":
        checks.append((None, f"epoch 1 is {e1['state']}"))
        return v_all(checks)
    out = e1.get("outcome") or ""
    checks.append((out.startswith("Promoted") and (_outcome_class(out) == win[:16] or True), f"promoted ({out[:90]})"))
    if mine and mine[0].get("counts"):
        p = mine[0]["counts"]["primary"]
        checks.append((p["wins"] >= PLAN["expect"]["W1"]["wins_at_least"] and p["losses"] == 0, f"win counts {p}"))
    grants = e1.get("grants", [])
    checks.append((bool(grants), f"rewards: {len(grants)} grant(s)"))
    pool_ = row.get("pool") or {}
    if pool_:
        lhs = sum(int(pool_[k]) for k in ("deposited", "fees_in", "phi_in", "held_in"))
        rhs = sum(int(pool_[k]) for k in ("balance", "held", "unvested", "paid", "refunded"))
        checks.append((lhs == rhs, f"the pool's conservation law ({lhs} = {rhs})"))
    # Vesting is read on the full-weight lines: a composite epoch is 363 DAA, so W1's first unit (decision + L_e) falls past the run's end.
    # R's first unit is due at its decision + 253 (vest_epochs 3: three units, a rollback by proof forfeits what is unvested).
    rrow = line_status(st, lines.get("R", "")) if lines.get("R") else None
    r1 = epoch_row(rrow, 1) if rrow else None
    rgrants = (r1 or {}).get("grants", [])
    vest_from = max([g.get("vest_from_daa") or 0 for g in rgrants] or [0])
    if not r1 or r1.get("state") != "Decided":
        checks.append((None, "vesting (line R): R's epoch 1 is not decided yet"))
    elif vest_from and daa is not None:
        due = vest_from + line_l_e("R")
        if daa < due:
            checks.append((None, f"vesting (line R): the first unit is due at DAA {due}"))
        else:
            checks.append((any((g.get("vested") or 0) > 0 for g in rgrants), "vesting (line R): a unit paid"))
    return v_all(checks)


def verdict_dm2(sd, st, lines):
    if not st or "L" not in lines:
        return "INCOMPLETE", "no improvement status yet"
    row = line_status(st, lines["L"])
    e1 = epoch_row(row, 1) if row else None
    if not e1:
        return "INCOMPLETE", "L's epoch 1 has not opened"
    if e1["state"] != "Decided":
        return "INCOMPLETE", f"epoch 1 is {e1['state']}"
    out = e1.get("outcome") or ""
    checks = [(out.startswith("NoChange"), f"no promotion ({out[:80]})")]
    for c in e1.get("candidates", []):
        k = c.get("counts")
        if k:
            checks.append((not k.get("eligible"), f"candidate {c['class'][:8]} is not eligible ({k['primary']})"))
    lose = [c for c in e1.get("candidates", []) if c.get("counts") and c["counts"]["primary"]["wins"] == 0 and c["counts"]["primary"]["losses"] > 0]
    checks.append((bool(lose), "the regressing candidate lost items and won none"))
    return v_all(checks)


# What the court's evidence travels by in this drill: the executor retains the capture of every claim it carries in a directory every node shares and the
# challenger reads the accused's from it. A stand-in — the real transport (the pipeline-claim data-availability units) is the core lane's (its task 5) — named in
# the verdict so a PASS is not read as a proof of that transport.
EVIDENCE_STAND_IN = " [evidence transport: the shared capture directory (--palw-improve-capture-dir), a stand-in until the pipeline-claim data-availability units land]"


T_EPOCH_OPEN_STATES = ("Submission", "HoldOut", "Drawing", "Evaluating")


def verdict_dm3(sd, lines):
    """D-M3 (RFC-0004 A13): a lying evaluation claim is CONVICTED — on the SACRIFICIAL liar nodes new7 (a moved step leaf) and new8 (a moved first id), each with
    its own post-genesis bond. Over the probe record (`step_dm3_probe`): each liar lied on line T; the challenger's replay found each lie that landed as a claim,
    where the lie is (the first divergent leaf; the first id); the chain voided the claim (`court_fraud`) and slashed the bond. And AG-2 (the capacity
    package's aggregate liability, armed from the fence-3 flag day): the conviction voids ALL the bond's other live claims (`aggregate_forfeit`), forfeits the
    collateral and freezes the bond — no claim accepted after it — while the claims already Final STAY Final; the freed jobs are claimed again by honest
    evaluators within the epoch. A lie that reached Final unconvicted is a FAIL; a claim of a liar's bond voided for any other reason is a FAIL."""
    p = (sd.get("data") or {}).get("dm3")
    tid = lines.get("T")
    if not tid:
        return "INCOMPLETE", "line T is not founded yet"
    if not p:
        return "INCOMPLETE", "no probe yet (it reads from the improvement fence on)"
    if p.get("mismatch"):
        return "FAIL", f"the harness derived another id for line T than the chain's ({p['mismatch']}): the tamper flags name the wrong line"
    checks = []
    liars = p.get("liars") or {}
    done = sd.get("done") or {}
    for node, fault in TAMPER.items():
        lie = liars.get(node)
        stop = done.get(f"liar-stop:{node}")
        if lie is None:
            if stop and "NEVER lied" in str(stop.get("why", "")):
                checks.append((False, f"{node} never lied by its deadline ({stop['why']})"))
            elif stop:
                checks.append((None, f"{node} was stopped before it lied: {stop.get('why')}"))
            else:
                checks.append((None, f"{node} has not lied yet (its first evaluation of T, fault {fault})"))
        else:
            checks.append((lie["line"] == tid[: len(lie["line"])] and lie["fault"] == fault, f"{node} lied about item {lie['item']} of epoch {lie['epoch']} of line T: {lie['fault']}"))
    disputes = {d["claim"]: d for d in (p.get("disputes") or [])}
    expected_kind = {"leaf": "leaf", "output": "output"}
    t_epoch = p.get("t_epoch") or {}
    t_open = t_epoch.get("state") in T_EPOCH_OPEN_STATES if t_epoch else True
    jobs_of = (sd.get("data") or {}).get("dm3-jobs") or {}          # claim id -> [epoch, item, subject], from every tick's view of T's jobs
    landed = []
    for node, fault in TAMPER.items():
        rec = (p.get("claims") or {}).get(node) or {}
        rows = {c["id"]: c for c in rec.get("rows", [])}
        mine = [(rows[c], disputes[c]) for c in disputes if c in rows]
        if node in liars and not mine:
            checks.append((None, f"the challenger has found no dispute on a claim of {node}'s yet"))
        convicted_daa = None
        for row, d in mine:
            landed.append((node, row, d))
            want = expected_kind[fault.split(":")[0]]
            checks.append((d.get("kind") == want, f"{node}'s lie, claim {row['id'][:12]}…: the challenger located it as {d.get('kind')} ({str(d.get('detail'))[:70]}), expected {want}"))
            phase = row["phase"]
            slashed = int(rec.get("slashed", 0))
            if phase_is(phase, "final"):
                checks.append((False, f"{node}'s lying claim {row['id'][:12]}… reached Final unconvicted"))
            elif phase_is(phase, "voided"):
                convicted_daa = int(row.get("pdaa") or 0) or convicted_daa
                checks.append(("court_fraud" in (row["void"] or "") and slashed > 0,
                               f"{node}'s lying claim {row['id'][:12]}… is voided ({row['void'] or 'no reason'}) and its bond slashed ({slashed} sompi of {rec.get('collateral', '?')})"))
                # A convicted evaluation claim frees its job and records no score (spec 17 §17.8.6): the chain's view of the epoch, where it still lists it,
                # no longer holds the claim on any job row.
                holders = [r for r in (p.get("eval_rows") or []) if r.get("claim") == row["id"]]
                checks.append((not holders, f"{node}'s voided claim {row['id'][:12]}… holds no job row any more and records no score" if not holders
                               else f"{node}'s voided claim {row['id'][:12]}… still holds job row(s) {[(r['epoch'], r['item'], r['subject']) for r in holders]}: the conviction did not free its job"))
            else:
                checks.append((None, f"{node}'s lying claim {row['id'][:12]}… is {phase}: the court has not convicted it yet ({str(d.get('filing'))[:80]})"))
        if convicted_daa is None:
            continue
        # AG-2 on the convicted bond.
        others = [c for c in rec.get("rows", []) if c["id"] not in {r["id"] for r, _ in mine}]
        live = [c for c in others if str(c["phase"]).lower() in LIVE_PHASES]
        agg = [c for c in others if phase_is(c["phase"], "voided") and "aggregate_forfeit" in (c["void"] or "")]
        stray = [c for c in others if phase_is(c["phase"], "voided") and not any(k in (c["void"] or "") for k in ("aggregate_forfeit", "court_fraud"))]
        checks.append((int(rec.get("collateral", 0)) == 0, f"AG-2, {node}: the whole posted collateral is taken (collateral 0)" if int(rec.get("collateral", 0)) == 0
                       else f"AG-2, {node}: the bond still shows {rec.get('collateral')} sompi of collateral after the conviction"))
        checks.append((not live, f"AG-2, {node}: no live claim is left on the convicted bond" if not live
                       else f"AG-2, {node}: {len(live)} claim(s) of the convicted bond are still live ({sorted({c['phase'] for c in live})}): the aggregate forfeiture did not void them"))
        checks.append((not stray, f"AG-2, {node}: {len(agg)} other live claim(s) voided as aggregate_forfeit, none voided for another reason" if not stray
                       else f"{node}'s claim(s) {[c['id'][:12] for c in stray]} were voided for {sorted({c['void'] for c in stray})}: not the conviction's doing"))
        was_final = set((sd.get("data") or {}).get(f"dm3-final-seen:{node}") or [])
        lost = [c["id"][:12] for c in rec.get("rows", []) if c["id"] in was_final and not phase_is(c["phase"], "final")]
        checks.append((not lost, f"AG-2, {node}: the claims already Final ({len(was_final)}) stay Final" if not lost
                       else f"AG-2, {node}: claim(s) {lost} were Final and are not any more"))
        after = [c["id"][:12] for c in rec.get("rows", []) if int(c.get("acc") or 0) > convicted_daa]
        checks.append((not after, f"AG-2, {node}: the bond is frozen (no claim accepted after the conviction at DAA {convicted_daa})" if not after
                       else f"AG-2, {node}: claim(s) {after} were accepted after the conviction (DAA {convicted_daa}): the bond is not frozen"))
        # The freed jobs are claimed again (by anyone else) within the epoch.
        voided_ids = {c["id"] for c in rec.get("rows", []) if phase_is(c["phase"], "voided")}
        freed = {tuple(jobs_of[i]) for i in voided_ids if i in jobs_of}
        unclaimed = []
        for (epoch, item, subject) in freed:
            held = [r for r in (p.get("eval_rows") or []) if (r["epoch"], r["item"], r["subject"]) == (epoch, item, subject) and r.get("claim") and r["claim"] not in voided_ids]
            if not held:
                unclaimed.append((epoch, item, subject))
        if freed and not unclaimed:
            checks.append((True, f"AG-2, {node}: the {len(freed)} freed job(s) were claimed again by other evaluators"))
        elif freed:
            checks.append((None if t_open else False, f"AG-2, {node}: {len(unclaimed)} of {len(freed)} freed job(s) {sorted(unclaimed)[:4]} not claimed again yet"
                           + ("" if t_open else " and T's epoch has left Evaluating")))
    for node in TAMPER:
        landed_ids = {row["id"] for n, row, _ in landed if n == node}
        # (the other voids of a convicted bond are checked above; a bond whose lie has not been convicted must have no voided claim at all)
        rec = (p.get("claims") or {}).get(node) or {}
        if not any(n == node and phase_is(row["phase"], "voided") for n, row, _ in landed):
            odd = [c["id"][:12] for c in rec.get("rows", []) if phase_is(c["phase"], "voided") and c["id"] not in landed_ids]
            if odd:
                checks.append((False, f"{node}'s claim(s) {odd} were voided before any conviction"))
    v, why = v_all(checks)
    return v, why + EVIDENCE_STAND_IN


def verdict_dm4(sd, st, lines, ids):
    out = []
    own = sd["done"].get("rollback-owner")
    if own is None:
        out.append((None, "by the owner: W2 is not promoted and rolled back yet"))
    else:
        row = line_status(st, lines.get("W2", "")) if st else None
        heads = [h["cause"] for h in (row or {}).get("heads", [])]
        out.append(("RolledBackByOwner" in heads, f"by the owner: head history {heads}"))
        out.append((bool((row or {}).get("barred")), "the rolled-back submitter is barred"))
    prf = sd["done"].get("rollback-proof")
    if prf is None:
        out.append((None, "by proof: R's second epoch has not shown the regression yet"))
    else:
        row = line_status(st, lines.get("R", "")) if st else None
        heads = [h["cause"] for h in (row or {}).get("heads", [])]
        out.append(("RolledBackByProof" in heads, f"by proof: head history {heads}"))
        # vest_epochs 3: the rollback lands with part of epoch 1's grants unvested, and that remainder is forfeited (vested stays final).
        g1 = ((epoch_row(row, 1) or {}).get("grants") or []) if row else []
        if g1:
            forfeited = sum(int(g.get("forfeited") or 0) for g in g1)
            out.append((forfeited > 0, f"by proof: the unvested remainder is forfeited ({forfeited} sompi of {sum(int(g.get('amount') or 0) for g in g1)}; vested {sum(int(g.get('vested') or 0) for g in g1)} stays)"))
    return v_all(out)


def verdict_dm6(sd, st, lines):
    res = []
    cp = sd["done"].get("attack:copy")
    if cp is None:
        res.append((None, "copy not run"))
    elif cp.get("result") == "INCOMPLETE":
        res.append((None, cp.get("why", "")))
    else:
        row = line_status(st, lines.get("W1", "")) if st else None
        e1 = epoch_row(row, 1) if row else None
        n = e1["counts"]["candidates"] if e1 and e1["state"] not in ("Open", "Submission") else None  # the set freezes at t_close
        res.append((None if n is None else n == cp["expected"], f"copy refused: the frozen set holds {n} candidate(s), the plan entered {cp.get('expected')}"))
    lt = sd["done"].get("attack:late")
    if lt is None:
        res.append((None, "late candidate not run"))
    elif lt.get("result") == "INCOMPLETE":
        res.append((None, lt.get("why", "")))
    else:
        row = line_status(st, lines.get("W1", "")) if st else None
        e1 = epoch_row(row, 1) if row else None
        n = e1["counts"]["candidates"] if e1 and e1["state"] not in ("Open", "Submission") else None
        res.append((None if n is None else n == lt["expected"], f"late candidate refused: the frozen set holds {n} candidate(s), the plan entered {lt.get('expected')}"))
    sp = sd["done"].get("attack:holdout-spam")
    if sp is None:
        res.append((None, "hold-out spam not run"))
    elif sp.get("result") == "INCOMPLETE":
        res.append((None, sp.get("why", "")))
    else:
        row = line_status(st, lines.get("L", "")) if st else None
        e2 = epoch_row(row, 2) if row else None
        n = e2["counts"]["holdout_cases"] if e2 else None
        ceiling = 4 * POLICY["eval"]["n"]
        res.append((None if n is None else n <= ceiling, f"hold-out pool bounded: {n} entries (ceiling 4n = {ceiling}) of {sp.get('built')} submitted"))
        if e2 is not None and e2["state"] == "Decided":
            res.append((str(e2.get("outcome", "")).startswith("NoChange"),
                        f"the flood costs the epoch its items: {e2.get('outcome')} (32 hold-out cases at hard_case_fee {POLICY['fees']['hard_case_fee']} each)"))
    wh = (sd.get("data") or {}).get("W2:e1:withheld")
    if wh:
        row = line_status(st, lines.get("W2", "")) if st else None
        e1 = epoch_row(row, 1) if row else None
        if e1 is None or e1["state"] != "Decided":
            res.append((None, "the withholding setter's forfeit is read when W2's epoch is decided"))
        else:
            forfeited = int(((row or {}).get("pool") or {}).get("forfeited_in", 0))
            bond = POLICY["fees"]["setter_bond"]
            res.append((forfeited >= bond, f"withholding setter {wh}: its bond forfeited to the pool ({forfeited} sompi, bond {bond})"))
    ek = sd["done"].get("attack:early-keys")
    res.append((None if ek is None else True, "early keys: submitted while Evaluating; the epoch still scored only after the outputs were final" if ek else "early keys not run"))
    return v_all(res)


# ---------------------------------------------------------------------------------------------------------------------
def selftest():
    """The verdict logic on synthetic chain data."""
    sd = {"done": {}, "milestones": {}}
    assert verdict_dm5(sd)[0] == "INCOMPLETE"
    sd["done"]["m5-below"] = {"submitted_daa": 30}
    assert verdict_dm5(sd)[0] == "INCOMPLETE"
    sd["done"]["m5-below-verified"] = {"result": "PASS", "tips": (31, "ab")}
    assert verdict_dm5(sd)[0] == "INCOMPLETE"
    sd["done"]["m5-cross"] = {"result": "PASS", "lack": ("gen", 28), "refusal": "Fork-id mismatch on network testnet-12 at DAA 30 - this node has crossed fence 28"}
    assert verdict_dm5(sd)[0] == "PASS"
    sd["done"]["m5-cross"] = {"result": "FAIL", "why": "no refusal"}
    assert verdict_dm5(sd)[0] == "FAIL"

    win = "ab" * 64
    cands = [{"class": win, "counts": {"primary": {"wins": 8, "losses": 0, "ties": 0}, "eligible": True}}]
    e1 = {"epoch": 1, "state": "Decided", "times": {"t_open": line_windows("W1")["grid"]}, "dataset_root": "dd" * 64, "candidates": cands,
          "outcome": "Promoted { class_id: " + win + ", wins: 8, losses: 0 }",
          "grants": [{"vest_from_daa": 500, "vested": 5}]}
    row = {"line_id": "11" * 64, "epochs": [e1], "heads": [], "pool": {"deposited": "0", "fees_in": "10", "phi_in": "0", "held_in": "5",
                                                                        "balance": "3", "held": "2", "unvested": "5", "paid": "3", "refunded": "2"}}
    # Vesting is read on line R (the full-weight line: its first unit is due at its decision + L_e 253); W1's own is past the run's end.
    rrow = {"line_id": "44" * 64, "heads": [], "epochs": [{"epoch": 1, "state": "Decided", "grants": [
        {"vest_from_daa": 500, "vested": 3, "amount": 9, "forfeited": 0, "vest_epochs": 3}]}]}
    st = {"lines": [row, rrow]}
    dm1_lines = {"W1": "11" * 64, "R": "44" * 64}
    sd["milestones"] = {"head:first-final-claim": {"daa": 260}}
    r = verdict_dm1(sd, st, dm1_lines, {"win": win}, 950)
    assert r[0] == "PASS" and "vesting (line R)" in r[1], r
    row["pool"]["balance"] = "4"
    assert verdict_dm1(sd, st, dm1_lines, {"win": win}, 950)[0] == "FAIL"
    row["pool"]["balance"] = "3"
    assert verdict_dm1(sd, st, dm1_lines, {"win": win}, 700)[0] == "INCOMPLETE"
    rrow["epochs"][0]["grants"][0]["vested"] = 0
    assert verdict_dm1(sd, st, dm1_lines, {"win": win}, 950)[0] == "FAIL", "R's first unit is due and nothing vested"
    rrow["epochs"][0]["grants"][0]["vested"] = 3
    e1["outcome"] = "NoChange { reason: NoneEligible }"
    assert verdict_dm1(sd, st, dm1_lines, {"win": win}, 950)[0] == "FAIL"

    lose = "cd" * 64
    rowl = {"line_id": "22" * 64, "epochs": [{"epoch": 1, "state": "Decided", "outcome": "NoChange { reason: NoneEligible }",
                                              "candidates": [{"class": lose, "counts": {"primary": {"wins": 0, "losses": 4, "ties": 4}, "eligible": False}}]}]}
    assert verdict_dm2(sd, {"lines": [rowl]}, {"L": "22" * 64})[0] == "PASS"
    rowl["epochs"][0]["candidates"][0]["counts"]["eligible"] = True
    assert verdict_dm2(sd, {"lines": [rowl]}, {"L": "22" * 64})[0] == "FAIL"

    sd["done"]["rollback-owner"] = {"daa": 700}
    w2 = {"line_id": "33" * 64, "heads": [{"seq": 1, "cause": "OptIn"}, {"seq": 2, "cause": "RolledBackByOwner"}], "barred": [{"bond": "x:0", "until": 9}], "epochs": []}
    r = verdict_dm4(sd, {"lines": [w2]}, {"W2": "33" * 64}, {})
    assert r[0] == "INCOMPLETE" and "by proof" in r[1], r
    sd["done"]["rollback-proof"] = {"daa": 1300}
    rl = {"line_id": "44" * 64, "heads": [{"seq": 3, "cause": "RolledBackByProof"}],
          "epochs": [{"epoch": 1, "grants": [{"amount": 9, "vested": 6, "forfeited": 3}]}]}
    r = verdict_dm4(sd, {"lines": [rl, w2]}, {"R": "44" * 64, "W2": "33" * 64}, {})
    assert r[0] == "PASS" and "forfeited" in r[1], r
    rl["epochs"][0]["grants"][0]["forfeited"] = 0
    assert verdict_dm4(sd, {"lines": [rl, w2]}, {"R": "44" * 64, "W2": "33" * 64}, {})[0] == "FAIL", "nothing unvested to forfeit"

    # D-M3: the verdict over the probe record (the real RPC spellings: snake-case phases and void reasons; the liars new7 and new8 with AG-2).
    tid = "ab" * 64
    t_lines = {"T": tid}
    assert verdict_dm3({"data": {}}, {})[0] == "INCOMPLETE"
    assert verdict_dm3({"data": {}}, t_lines)[0] == "INCOMPLETE"
    c1, c2, c3, c4, c5 = "c1" * 64, "c2" * 64, "c3" * 64, "c4" * 64, "c5" * 64
    row = lambda i, ph, void="", acc=300, pdaa=0: {"id": i, "phase": ph, "void": void, "acc": acc, "pdaa": pdaa}  # noqa: E731
    probe = {"daa": 500, "mismatch": None,
             "liars": {"new7": {"item": 2, "epoch": 1, "line": tid[:32], "fault": "leaf:1"},
                       "new8": {"item": 5, "epoch": 1, "line": tid[:32], "fault": "output"}},
             "claims": {"new7": {"slashed": 0, "collateral": 9_000_000, "rows": [row(c1, "panel_bound"), row(c2, "final", acc=200), row(c4, "provisional", acc=310)]},
                        "new8": {"slashed": 0, "collateral": 9_000_000, "rows": [row(c3, "receipt_licensed"), row(c5, "provisional", acc=312)]}},
             "disputes": [{"claim": c1, "kind": "leaf", "detail": "leaf 1", "filing": "not filed"},
                          {"claim": c3, "kind": "output", "detail": "generated id 0", "filing": "not filed"}],
             "t_epoch": {"epoch": 1, "state": "Evaluating", "outcome": None}}
    clone = lambda x: json.loads(json.dumps(x))  # noqa: E731
    sd3 = lambda pr, **extra: {"data": {"dm3": pr, "dm3-final-seen:new7": [c2], **extra}, "done": {}}  # noqa: E731
    r = verdict_dm3(sd3(probe), t_lines)
    assert r[0] == "INCOMPLETE" and "not convicted" in r[1], r

    def convict(pr):
        """The conviction of each lie (court_fraud) and the aggregate forfeiture (aggregate_forfeit) of everything else live on the bond (AG-2)."""
        pr = clone(pr)
        for node, cid in (("new7", c1), ("new8", c3)):
            rec = pr["claims"][node]
            rec["slashed"] = rec["collateral"]
            rec["collateral"] = 0
            for c in rec["rows"]:
                if c["id"] == cid:
                    c["phase"], c["void"], c["pdaa"] = "voided", "court_fraud", 400
                elif c["phase"] in LIVE_PHASES:
                    c["phase"], c["void"], c["pdaa"] = "voided", "aggregate_forfeit", 400
        return pr
    convicted = convict(probe)
    r = verdict_dm3(sd3(convicted), t_lines)
    assert r[0] == "PASS" and "stand-in" in r[1] and "aggregate_forfeit" in r[1] and "frozen" in r[1], r
    # AG-2 broken three ways: a live claim survives, a Final claim is voided, a claim is accepted after the conviction.
    live = clone(convicted)
    live["claims"]["new7"]["rows"][2].update(phase="provisional", void="", pdaa=0)
    assert verdict_dm3(sd3(live), t_lines)[0] == "FAIL", "AG-2: a live claim of the convicted bond survived"
    lost = clone(convicted)
    lost["claims"]["new7"]["rows"][1].update(phase="voided", void="aggregate_forfeit")
    assert verdict_dm3(sd3(lost), t_lines)[0] == "FAIL", "AG-2: a Final claim was voided"
    thawed = clone(convicted)
    thawed["claims"]["new7"]["rows"].append(row("c9" * 64, "provisional", acc=401))
    assert verdict_dm3(sd3(thawed), t_lines)[0] == "FAIL", "AG-2: the bond accepted a claim after its conviction"
    stray = clone(convicted)
    stray["claims"]["new7"]["rows"][2].update(phase="voided", void="receipt_timeout")
    assert verdict_dm3(sd3(stray), t_lines)[0] == "FAIL", "a claim voided for another reason"
    # The freed jobs: claimed again by someone else (PASS), not yet (pending while T is evaluating), never (FAIL once T has left Evaluating).
    jobs = {c1: [1, 2, "Candidate"], c3: [1, 5, "Parent"]}
    held = lambda claim: [{"epoch": 1, "item": 2, "subject": "Candidate", "kind": "ExactMatch", "claim": claim, "voided": False, "score": None},  # noqa: E731
                          {"epoch": 1, "item": 5, "subject": "Parent", "kind": "ExactMatch", "claim": claim, "voided": False, "score": None}]
    again = clone(convicted)
    again["eval_rows"] = held("e1" * 64)
    assert verdict_dm3(sd3(again, **{"dm3-jobs": jobs}), t_lines)[0] == "PASS", "the freed jobs were claimed again"
    nobody = clone(convicted)
    nobody["eval_rows"] = held(None)
    assert verdict_dm3(sd3(nobody, **{"dm3-jobs": jobs}), t_lines)[0] == "INCOMPLETE", "not claimed again yet, T still evaluating"
    nobody["t_epoch"]["state"] = "Decided"
    assert verdict_dm3(sd3(nobody, **{"dm3-jobs": jobs}), t_lines)[0] == "FAIL", "never claimed again"
    still = clone(convicted)
    still["eval_rows"] = held(c1)
    assert verdict_dm3(sd3(still, **{"dm3-jobs": jobs}), t_lines)[0] == "FAIL", "a voided claim that still holds its job"
    final = clone(probe)
    final["claims"]["new7"]["rows"][0]["phase"] = "final"
    assert verdict_dm3(sd3(final), t_lines)[0] == "FAIL", "a lie that reached Final unconvicted"
    wrong = clone(convicted)
    wrong["disputes"][0]["kind"] = "output"
    assert verdict_dm3(sd3(wrong), t_lines)[0] == "FAIL", "the lie located as another kind"
    early = clone(probe)
    early["claims"]["new7"]["rows"][2].update(phase="voided", void="aggregate_forfeit")
    assert verdict_dm3(sd3(early), t_lines)[0] == "FAIL", "a claim voided before any conviction"
    quiet = clone(probe)
    quiet["liars"] = {}
    quiet["disputes"] = []
    assert verdict_dm3(sd3(quiet), t_lines)[0] == "INCOMPLETE", "nobody lied yet"
    nolie = sd3(quiet)
    nolie["done"] = {"liar-stop:new7": {"daa": 760, "why": "deadline: it NEVER lied and was not convicted by DAA 760 (0 court_fraud, 0 live)"}}
    assert verdict_dm3(nolie, t_lines)[0] == "FAIL", "a liar stopped at its deadline that never lied"
    bad_id = clone(probe)
    bad_id["mismatch"] = {"chain": "x", "derived": "y"}
    assert verdict_dm3(sd3(bad_id), t_lines)[0] == "FAIL", "the harness derived another line id"
    assert parse_free_pct("The system has 34359738368 (2097152 pages with a page size of 16384).\nSystem-wide memory free percentage: 46%\n") == 46 and parse_free_pct("") is None
    stopped = sd3(quiet)
    stopped["done"] = {"liar-stop:new7": {"daa": 480, "why": "memory tripwire: the Mac's free memory was 9%"}}
    r = verdict_dm3(stopped, t_lines)
    assert r[0] == "INCOMPLETE" and "tripwire" in r[1], r
    # The capacity line: the windows' numbers and the gate.
    claims = {f"k{i}": {"acc": 570 + i * 0.8, "lic": 570 + i * 0.8 + 6, "fin": None} for i in range(100)}
    flat = [{"daa": 570 + i * 2, "backlog": 8 + (i % 3), "occ_max": 5, "occ_mean": 3.5, "reserved_ratio": 0.4} for i in range(40)]
    sm = capacity_summary(flat, claims, 570, 650)
    assert sm["diverges"] is False and sm["latency_p50"] == 6 and 1.2 <= sm["accepted_per_daa"] <= 1.3, sm
    growing = [{"daa": 570 + i * 2, "backlog": 5 + i * 3, "occ_max": 7, "occ_mean": 6.0, "reserved_ratio": 0.9} for i in range(40)]
    assert capacity_summary(growing, claims, 570, 650)["diverges"] is True
    assert capacity_summary(flat[:3], claims, 570, 650)["diverges"] is None, "too few samples to say"
    assert verdict_cap({"data": {}})[0] == "INCOMPLETE"
    assert verdict_cap({"data": {"cap-armed": {"capacity_step2": True, "capacity_step3": True}, "cap-summary": {"rho25": sm, "rho100": capacity_summary(growing, claims, 665, 745)}}})[0] in ("INCOMPLETE", "FAIL")
    two = {"rho25": sm, "rho100": dict(sm)}
    assert verdict_cap({"data": {"cap-armed": {"capacity_step2": True, "capacity_step3": True}, "cap-summary": two}})[0] == "PASS"
    two["rho100"]["diverges"] = True
    assert verdict_cap({"data": {"cap-armed": {"capacity_step2": True, "capacity_step3": True}, "cap-summary": two}})[0] == "FAIL", "ρ=100 diverges"
    # The line id derived offline is the chain's: golden values pinned by misaka-palw-sdk/tests/improve_line_id.rs.
    g_class, g_bond = "ab" * 64, "07" * 64 + ":1"
    assert model_line_id(g_class, g_bond, "T") == "17131be198686c865fff3483f36c895a686bd10d8d92c0a894920e2a7923450c89f8e8061afefc2b29ab5667516311de410d614e2811d1379e11cee4fa89dacd"
    assert model_line_id(g_class, "07" * 64 + ":3", "W2") == "7d1e43e6c6cc6dadc5b93e262bf559252feacb19508696972ffba06980aa28975049a00df7c2626ee6d03eff66453e46a71c1782983fd178a3e2b721737546b2"
    print("selftest ok")


def selftest_drive():
    """The whole actor against a SCRIPTED chain (no binary, no node): every step must fire at the state it is written for,
    with the arguments the tools take, and the verdict files must come out as the drill's own expectations say."""
    import tempfile

    g = globals()
    tmp = tempfile.mkdtemp(prefix="dm-drive-")
    for k, v in (("WORK", tmp), ("KR", f"{tmp}/keyring"), ("UHOME", f"{tmp}/home"), ("MODEL", f"{tmp}/model"), ("VERDICTS", f"{tmp}/verdict"),
                 ("OBJ", f"{tmp}/objects"), ("EVID", f"{tmp}/evidence"), ("SNAP", f"{tmp}/snapshots")):
        g[k] = v
    for d in (KR, MODEL, f"{MODEL}/ids", UHOME):
        os.makedirs(d, exist_ok=True)
    json.dump({"seats": [{"bond_outpoint": f"{i:0128x}:{i}", "fee_float_outpoint": f"{i:0128x}:9"} for i in range(8)],
               "bonds": [{"n": 8 + i, "address": f"misakatest:qaddr{i}", "seed_file": f"{KR}/bond-{8 + i}.seed"} for i in range(8)]}, open(f"{KR}/manifest.json", "w"))
    h = lambda c: c * 128  # noqa: E731
    ids = {"head": h("a"), "win": h("b"), "lose": h("c"), "winc": h("d"), "losec": h("f")}   # by the assets' names
    for k, v in ids.items():
        open(f"{MODEL}/ids/{k}.class", "w").write(v)
        open(f"{MODEL}/ids/{k}.root", "w").write(h("e"))
    mk = lambda n: {"prompts": [[1, 3 + i, 4 + i, 5 + i] for i in range(n)], "keys": [[3 + i % 8, 4 + i % 8, 5] for i in range(n)]}  # noqa: E731
    for name in ("a", "b", "reg"):
        json.dump(mk(16), open(f"{MODEL}/pool-{name}.json", "w"))
    for a in ("winc", "losec"):
        open(f"{MODEL}/{a}.palwtirs", "w").write("x")
    g["NODES"] = {n: {"k": k, "seat": s, "role": r, "hb": False, "ir": True, "jit": r in ("liar", "reg")} for n, k, s, r in
                  (("new0", 0, 3, "floor"), ("new1", 1, 0, "seat"), ("new3", 3, 2, "seat"), ("new4", 4, 4, "head"), ("new5", 5, 5, "eval"),
                   ("new6", 6, 6, "evalw"), ("new7", 7, 8, "liar"), ("new8", 8, 9, "liar"), ("reg", 9, None, "reg"), ("old", 10, None, "old"))}
    open(f"{WORK}/old-lacks.txt", "w").write("tir2@24 gen@28 decode@32 improve@40")
    os.makedirs(f"{WORK}/new0", exist_ok=True)
    os.makedirs(f"{WORK}/old", exist_ok=True)

    clock = {"daa": 0}
    submitted, built, sh = [], [], []
    lid = {"W1": ids["head"], "W2": h("2"), "L": h("3"), "T": line_id_of("T"), "R": h("4")}
    g["salt"] = lambda: "00" * 32
    g["chain_daa"] = lambda: clock["daa"]
    g["node_alive"] = lambda n: True
    g["daa_of"] = lambda n: clock["daa"]
    g["call"] = lambda port, method, params=None: {"sink": "abcdef0123456789", "virtualDaaScore": clock["daa"]}
    g["log_after"] = lambda self, node, cursor, pattern: ("matched: " + pattern[:20]) if (node != "old" or "palw-lifecycle" in pattern) else None
    g["time"] = type("T", (), {"sleep": staticmethod(lambda x: None), "strftime": staticmethod(time.strftime), "time": staticmethod(time.time)})
    def fake_log_after(self, node, cursor, pattern):
        if "registered bond" in pattern:      # the registrar's line for the bond being registered
            n = next(x for x in XB_ORDER if extra_bond(x) is None)
            return f"registered bond {n:0128x}:0 with {COLL} sompi of collateral, in tx {n + 100:0128x}"
        return "matched: " + pattern[:24]
    Drive.log_after = fake_log_after
    nodes_calls = []
    Drive.nodes_sh = lambda self, *a, **k: (nodes_calls.append((clock["daa"], a)), (0, ""))[1]

    # D-M3's scripted evidence: the two liars' logs, the challenger's disputes, the chain's claims of the liars' bonds.
    T_ID = lid["T"]
    c_leaf, c_out, c_honest = "c1" * 64, "c3" * 64, "c2" * 64
    for node, fault in (("new7", "leaf:1"), ("new8", "output")):
        os.makedirs(f"{WORK}/{node}", exist_ok=True)
        open(f"{WORK}/{node}/kaspad.out", "w").write(
            f"2026-10-02 [WARN ] [PALW-PANEL] [palw-improve] DRILL: this node LIES about item 2 of epoch 1 of line {T_ID} for Candidate({h('b')}): {fault} "
            f"(--palw-drill-tamper-eval) - the claim it files is self-consistent\n")
    os.makedirs(f"{WORK}/new1/app/testnet-12", exist_ok=True)
    json.dump({"schema": "misaka.palw.improve-status.v1", "daa": 1, "lines": [], "evaluation": [],
               "disputes": [{"claim": c_leaf, "job": h("5"), "kind": "leaf", "detail": "leaf 1 (stage 0, leaf 1) of the step tree", "found_daa": 400, "filing": "scripted"},
                            {"claim": c_out, "job": h("6"), "kind": "output", "detail": "generated id 0: committed 4, honest 5", "found_daa": 401, "filing": "scripted"}]},
              open(f"{WORK}/new1/app/testnet-12/palw-improve-status.json", "w"))

    funded = {}
    COLL = 9 * 10**13       # a genesis seat's collateral, ~900,000 MSK (bigger than the 150 MSK fee float the registrar leaves as change)

    def fake_rpc(method, params=None, prefer=()):
        liar_bonds = {r["bond_outpoint"]: n for n in (8, 9) for r in [extra_bond(n)] if r}
        if method == "getPalwClaims" and (params or {}).get("role") == "executor" and (params or {}).get("bond") in liar_bonds:
            convicted = clock["daa"] >= 700
            first = liar_bonds[params["bond"]] == 8
            lie, live = (c_leaf, "c4" * 64) if first else (c_out, "c5" * 64)
            rows = [{"claimId": lie, "phase": "voided" if convicted else "panel_bound", "voidReason": "court_fraud" if convicted else "", "acceptedDaa": 300, "phaseDaa": 700 if convicted else 0},
                    {"claimId": live, "phase": "voided" if convicted else "provisional", "voidReason": "aggregate_forfeit" if convicted else "", "acceptedDaa": 310,
                     "phaseDaa": 700 if convicted else 0}]
            if first:
                rows.append({"claimId": c_honest, "phase": "final", "voidReason": "", "acceptedDaa": 200, "phaseDaa": 450})
            return {"bondSlashed": COLL if convicted else 0, "bondCollateral": 0 if convicted else COLL, "claims": rows}
        if method == "getPalwModelRegistry":
            return {"classes": [{"classId": v, "state": "Probation { probes_passed: 0 }", "readySeats": 7} for v in ids.values()]}
        if method == "getPalwClaims":
            return {"bondCollateral": COLL, "claims": [{"claimId": f"{clock['daa']:0128x}", "classId": ids["head"], "phase": "final", "acceptedDaa": clock["daa"] - 40, "phaseDaa": clock["daa"]}]
                    if clock["daa"] >= 255 else []}
        if method == "getUtxosByAddresses":
            addr = (params or {}).get("addresses", [""])[0]
            amt = funded.get(addr)
            if not amt:
                return {"entries": []}
            return {"entries": [{"outpoint": {"transactionId": "ab" * 64, "index": 0}, "utxoEntry": {"amount": amt}},
                                {"outpoint": {"transactionId": "cd" * 64, "index": 1}, "utxoEntry": {"amount": 150 * 10**8}}]}
        if method == "getPalwProducerFacts":
            return {"bondOperatorId": "0f" * 64, "bondReservedExposure": "40", "bondExposureCeiling": "100"}
        return {}
    g["rpc"] = fake_rpc

    def fake_submit(objects, node=None, tries=6):
        submitted.append((clock["daa"], [os.path.basename(o) for o in objects]))
        return True, ""
    g["submit"] = fake_submit

    def fake_improve(kind, seat, spec, out, extra=()):
        built.append((clock["daa"], kind, seat, os.path.basename(out)))
        open(out, "w").write("x")
        if kind == "setter-set":
            open(out + ".prompts", "w").write("x")
            open(out + ".keys", "w").write("x")
        text = {"policy": "policy check      ok under the drill's ceilings\n", "dataset": f"dataset id        {h('9')}\n",
                "candidate": "candidate class   " + ids["win"] + "\n"}.get(kind, "")
        return 0, text
    g["improve_object"] = fake_improve

    def fake_run(cmd, timeout=900, home=None):
        sh.append((clock["daa"], " ".join(cmd[-8:])))
        if cmd[-1:] == ["--help"]:
            return 0, "--palw-drill-capacity-step2-at --palw-drill-capacity-step3-at"
        if "wallet" in cmd and "send" in cmd:      # a transfer to a bond's pay address: the funding that address then shows
            funded[cmd[cmd.index("--to") + 1]] = int(float(cmd[cmd.index("--amount") + 1]) * 10**8)
            return 0, ""
        if "line-found" in cmd:
            name = cmd[cmd.index("--name") + 1]
            return 0, f"found line '{name}'\n  line id        {lid[name]}\n"
        return 0, "wrote the object\n"
    g["run"] = fake_run

    d = Drive()

    def clock_of(name):
        """A line's own epoch clock: its windows are its policy's (a composite line's Submission window is the long one)."""
        w = line_windows(name)
        t_open = w["grid"]
        t_fix = t_open + w["w_collect"]
        t_close = t_fix + w["w_submit"]
        t_draw = t_close + w["w_holdout"]
        return w, t_open, t_fix, t_close, t_draw, t_draw + w["w_eval"]

    def epoch_state(daa, t0, name):
        w, t_open, t_fix, t_close, t_draw, t_eval = clock_of(name)
        o = t0
        marks = [(o + (t_eval - t_open) + 165, "Decided"), (o + (t_eval - t_open), "Closing"), (o + (t_draw - t_open) + w["beacon_delay"], "Evaluating"),
                 (o + (t_draw - t_open), "Drawing"), (o + (t_close - t_open), "HoldOut"), (o + (t_fix - t_open), "Submission"), (o, "Open")]
        for at, state in marks:
            if daa >= at:
                return state
        return None

    def fake_status():
        daa = clock["daa"]
        done = d.s.d["done"]
        if daa < IMPROVE_AT:
            return None
        lines = []
        evals = []
        for name, plan in PLAN["lines"].items():
            if "policies" not in done:
                continue
            w, t_open, t_fix, t_close, t_draw, t_eval = clock_of(name)
            win_class = ids[asset_of("win", name)]
            epochs = []
            heads = [{"seq": 0, "epoch": 0, "class": ids["head"], "cause": "OptIn"}]
            for e_no, t0 in ((1, t_open), (2, {"R": 3 * t_open, "L": 2 * t_open}.get(name, 3 * t_open))):
                if str(e_no) not in plan["epochs"] and name != "W2":
                    continue
                st = epoch_state(daa, t0, name)
                if st is None:
                    continue
                key = f"{name}:e{e_no}"
                ncand = len(plan["epochs"].get(str(e_no), {}).get("candidates", [])) if f"{key}:candidates" in done and done[f"{key}:candidates"].get("n") else 0
                row = {"epoch": e_no, "state": st, "times": {"t_open": t0, "t_fix": t0 + (t_fix - t_open), "t_close": t0 + (t_close - t_open), "t_draw": t0 + (t_draw - t_open),
                                                              "t_eval": t0 + (t_eval - t_open), "t_score": t0 + (t_eval - t_open) + w["court_margin"]},
                       "dataset_root": h("7") if st != "Open" else None, "outcome": None, "grants": [],
                       "counts": {"candidates": ncand, "holdout_cases": 32 if (name == "L" and e_no == 2 and "attack:holdout-spam" in done) else 0},
                       "previous_counts": None,
                       "candidates": [{"class": ids[asset_of(c["class"], name)], "counts": None} for c in plan["epochs"].get(str(e_no), {}).get("candidates", [])] if ncand else []}
                if st == "Decided":
                    if name in ("W1", "W2", "R") and e_no == 1:
                        row["outcome"] = "Promoted { class_id: " + win_class + ", wins: 8, losses: 0 }"
                        row["candidates"][0]["counts"] = {"primary": {"wins": 8, "losses": 0, "ties": 0}, "eligible": True}
                        row["grants"] = [{"vest_from_daa": t0 + 300, "amount": 9, "vested": 5 if daa > t0 + 300 + line_l_e(name) else 0,
                                          "forfeited": 3 if (name == "R" and "rollback-proof" in done) else 0}]
                        heads.append({"seq": 1, "epoch": 1, "class": win_class, "cause": "Promoted"})
                        if name == "W2" and "rollback-owner" in done:
                            heads.append({"seq": 2, "epoch": 1, "class": ids["head"], "cause": "RolledBackByOwner"})
                        if name == "R" and "rollback-proof" in done:
                            heads.append({"seq": 2, "epoch": 1, "class": ids["head"], "cause": "RolledBackByProof"})
                    else:
                        row["outcome"] = "NoChange { reason: NoneEligible }"
                        for c in row["candidates"]:
                            c["counts"] = {"primary": {"wins": 0, "losses": 4, "ties": 4}, "eligible": False}
                        if name == "R" and e_no == 2:
                            row["previous_counts"] = {"primary": {"wins": 8, "losses": 0, "ties": 0}, "eligible": True}
                epochs.append(row)
                if st in ("Evaluating", "Closing"):
                    n_jobs = 16
                    final = daa >= t0 + (t_eval - t_open) - w["w_eval"] + 130
                    evals.append({"line_id": lid[name], "epoch": e_no, "items": [{"item": i} for i in range(8)],
                                  "jobs": [{"item": i, "subject": "Parent", "claim": {"final_daa": daa if final else None, "voided": False}} for i in range(n_jobs)]})
            lines.append({"line_id": lid[name], "head": heads[-1]["class"], "next_due_daa": 0, "open_epoch": None, "epochs": epochs, "heads": heads,
                          "barred": [{"bond": "x:0", "until": 99}] if any(h_["cause"] == "RolledBackByOwner" for h_ in heads) else [],
                          "pool": {"deposited": "0", "fees_in": "10", "phi_in": "0", "held_in": "5", "balance": "3", "held": "2", "unvested": "5",
                                   "paid": "3", "refunded": "2", "forfeited_in": "10000000"}})
        return {"daa": daa, "lines": lines, "evaluation": evals}, "fake"

    g["read_status"] = fake_status
    for daa in range(0, 1800, 5):
        clock["daa"] = daa
        d.tick()
    verdicts = {n: read(f"{VERDICTS}/{n}.verdict", "none") for n in ("dm5", "dm1", "dm2", "dm3", "dm4", "dm6", "cap")}
    for n, v in verdicts.items():
        print(f"{n}: {v[:170]}")
    kinds = [b[1] for b in built]
    need = ["policy", "dataset", "hard-case", "setter-set", "candidate", "rollback"]
    missing = [k for k in need if k not in kinds]
    assert not missing, f"never built: {missing}"
    print(f"built {len(built)} objects ({sorted(set(kinds))}), submitted {len(submitted)} carrier calls, {len(sh)} shell calls")
    assert verdicts["dm5"].startswith("PASS"), verdicts["dm5"]
    assert verdicts["dm1"].startswith("PASS"), verdicts["dm1"]
    assert verdicts["dm2"].startswith("PASS"), verdicts["dm2"]
    assert verdicts["dm3"].startswith("PASS"), verdicts["dm3"]   # the scripted liars were convicted at DAA 700: probe -> verdict end to end
    assert "aggregate_forfeit" in verdicts["dm3"], verdicts["dm3"]
    assert verdicts["cap"].startswith("PASS"), verdicts["cap"]
    regd = sorted(n for n in XB_ORDER if extra_bond(n))
    assert regd == sorted(XB_ORDER), f"bonds registered: {regd}"
    assert [a for _, a in nodes_calls if a[0] == "start" and a[1] in ("new7", "new8")] and [a for _, a in nodes_calls if a[0] == "stop" and a[1] in ("new7", "new8")], nodes_calls[-8:]
    starts = [d_ for d_, a in nodes_calls if a[0] == "start" and a[1] == "new7"]
    assert starts and starts[0] >= LIAR_START_DAA, starts
    ops = [s_ for s_ in json.load(open(f"{KR}/manifest.json"))["seats"] if s_.get("operator_id")]
    assert len(ops) == 8, "the manifest names every seat's operator id"
    print("selftest-drive ok")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["run", "once", "verdicts", "selftest", "selftest-drive", "line-id"])
    ap.add_argument("arg", nargs="?", default="")
    a = ap.parse_args()
    if a.cmd == "line-id":
        if a.arg not in PLAN["lines"] or not model_id("head"):
            print(f"no line {a.arg!r} in the plan, or no model under {MODEL}", file=sys.stderr)
            return 2
        print(line_id_of(a.arg))
        return 0
    if a.cmd == "selftest":
        selftest()
        return 0
    if a.cmd == "selftest-drive":
        selftest_drive()
        return 0
    d = Drive()
    if a.cmd == "verdicts":
        for n in ("dm5", "dm1", "dm2", "dm3", "dm4", "dm6"):
            print(f"{n}: {read(f'{VERDICTS}/{n}.verdict', 'not yet')}")
        return 0
    if a.cmd == "once":
        d.tick()
        return 0
    log("dmdrive: run (tick %ds; %s)" % (TICK_S, ", ".join(f"{n}: {line_form(n)} L_e {line_l_e(n)} grid {line_windows(n)['grid']}" for n in PLAN["lines"])))
    while True:
        try:
            d.tick()
        except Exception as e:  # noqa: BLE001
            log(f"tick: {type(e).__name__}: {e}")
        time.sleep(TICK_S)


if __name__ == "__main__":
    sys.exit(main())
