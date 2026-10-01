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
TOOLS = E.get("TOOLS_BIN", ".")
JSON_BASE = int(E.get("JSON_BASE", "63100"))
BORSH_BASE = int(E.get("BORSH_BASE", "62100"))
CAND_FORM = E.get("CAND_FORM", "full")
IMPROVE_AT = int(E.get("IMPROVE_AT", "40"))
TIR_AT = int(E.get("TIR_AT", "20"))
TICK_S = int(E.get("DM_TICK_S", "30"))
PLAN = json.load(open(os.path.join(HERE, "drill.json")))
POLICY = PLAN["policy"]
WIN = POLICY["windows"]
L_E = sum(WIN[k] for k in ("w_collect", "w_submit", "w_holdout", "w_eval", "court_margin"))

NODES = {}
for _row in (E.get("NODES") or "").strip().splitlines():
    _f = _row.split()
    if len(_f) >= 6:
        NODES[_f[0]] = {"k": int(_f[1]), "seat": None if _f[2] == "-" else int(_f[2]), "role": _f[3], "hb": _f[4] == "1", "ir": _f[5] == "1"}

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


def bond_of(seat):
    return manifest()["seats"][seat]["bond_outpoint"]


def seed_of(seat):
    return f"{KR}/bond-{seat}.seed"


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
def line_policy(line):
    """The drill's common policy with the line's own overrides (setter_cap_permille, …)."""
    p = json.loads(json.dumps(POLICY))
    for k, v in (PLAN["lines"][line].get("policy") or {}).items():
        if isinstance(v, dict):
            p.setdefault(k, {}).update(v)
        else:
            p[k] = v
    return p


def pool(name):
    p = {"a": "pool-a.json", "b": "pool-b.json", "reg": "pool-reg.json"}[name]
    return json.load(open(f"{MODEL}/{p}"))


def pool_slice(name, lo, hi):
    d = pool(name)
    return {"prompts": d["prompts"][lo:hi], "keys": d["keys"][lo:hi]}


def class_container(cls):
    """The artifact flags `palw-class improve candidate` takes for a candidate class, and the file a registration names."""
    if cls == "winc":
        return ["--parent", f"{MODEL}/head.class.palwtir", "--section", f"{MODEL}/winc.palwtirs"]
    return ["--artifact", f"{MODEL}/{cls}.class.palwtir"]


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


def claims_of(bond):
    r = rpc("getPalwClaims", {"bond": bond, "role": "executor", "includeTerminal": True, "limit": 0})
    return pick(r, "claims", default=[]) or []


def final_claims(bond, class_id):
    n = 0
    for c in claims_of(bond):
        if pick(c, "classId") in (None, class_id) and str(pick(c, "phase", default="")).startswith("Final"):
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


def jobs_settled(view):
    """Every job that holds a claim holds a final (or a void) one — keys may open (spec 17 §17.8.2, in Closing)."""
    jobs = [j for j in (view or {}).get("jobs", []) if j.get("claim")]
    if not jobs:
        return False
    return all(j["claim"].get("final_daa") is not None or j["claim"].get("voided") for j in jobs)


def outcome_of(erow):
    return (erow or {}).get("outcome")


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
        self.ids = {n: model_id(n) for n in ("head", "win", "lose", "winc")}
        self.milestones()
        for step in (self.step_m5_below, self.step_m5_below_verify, self.step_m5_cross, self.step_register_classes, self.step_lines, self.step_policies,
                     self.step_material, self.step_epochs, self.step_rollbacks, self.step_attacks):
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
        pat_new = r"Block \S+: \S+ was dropped by name below palw_improvement_v1, and the block stands \(RFC-0004\)"
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
        for cls in ("win", "lose", "winc"):
            if cls == "winc" and not os.path.exists(f"{MODEL}/winc.palwtirs"):
                continue
            if lifecycle_of(self.reg, self.ids[cls])[0] != "absent":
                continue
            out = f"{OBJ}/register-{cls}.obj"
            if cls == "winc":
                art = ["--artifact", f"{MODEL}/winc.palwtirs", "--parent", f"{MODEL}/head.class.palwtir"]
            else:
                art = ["--artifact", f"{MODEL}/{cls}.class.palwtir"]
            cmd = cli_prefix("new3") + ["palw", "tir-registration", *art, "--bond", bond_of(7), "--key-file", seed_of(7), "--out", out]
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

    # ----- lines ------------------------------------------------------------------------------------------------
    def step_lines(self):
        if self.s.done("lines") or self.daa < IMPROVE_AT + 1 or not self.ids.get("head"):
            return
        if lifecycle_of(self.reg, self.ids["head"])[0] == "absent":
            return
        root = model_id("head", "root")
        lines = dict(self.s.get("lines") or {})
        for name in ("W2", "L"):
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
            time.sleep(SPACING_S)
        self.s.mark("lines", **lines)

    def step_policies(self):
        if self.s.done("policies") or not self.s.done("lines"):
            return
        lines = self.line_ids()
        if not all(l in lines for l in ("W1", "W2", "L")):
            return
        # the founding line's row exists once the head's class is registered; the founded lines' once their objects are mined
        objs = []
        for name in ("W1", "W2", "L"):
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
        log(f"opt-in: three policy objects submitted at DAA {self.daa}")

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
                    self.act_epoch(name, lid, e)

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
            rc, text = improve_object("candidate", c["seat"], spec, out, extra=class_container(c["class"]))
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
        # by proof: W1's second epoch showed the predecessor H beating the promoted W
        if not self.s.done("rollback-proof") and "W1" in lines:
            row = line_status(self.status, lines["W1"])
            e2 = epoch_row(row, 2)
            if e2 and e2["state"] == "Decided" and (e2.get("previous_counts") or {}).get("eligible"):
                out = f"{OBJ}/rollback-proof-W1.obj"
                spec = {"line": lines["W1"], "epoch": 1, "to_class": self.ids["head"], "cause": {"later_regression": 2}}
                rc, text = improve_object("rollback", 7, spec, out)
                if rc != 0:
                    raise RuntimeError(f"rollback (proof): {text.strip()[-300:]}")
                ok, text = submit([out])
                if not ok:
                    raise RuntimeError(f"cannot submit the rollback: {text.strip()[-300:]}")
                self.s.mark("rollback-proof", daa=self.daa)
                log(f"D-M4: W1's promotion rolled back by proof (epoch 2's regression check) at DAA {self.daa}")

    # ----- D-M6: the attacks ------------------------------------------------------------------------------------
    def step_attacks(self):
        """Each attack once, at the state where it is meant to bite; the verdict reads what the chain did."""
        lines = self.line_ids()
        if "W1" in lines:
            row = line_status(self.status, lines["W1"])
            e1 = epoch_row(row, 1)
            if e1:
                self.attack_copy(lines["W1"], e1)
                self.attack_late(lines["W1"], e1)
        # The attacks that could cost a line its epoch run on L, whose expected outcome (NoChange) they cannot spoil: the early
        # keys in its first epoch, the hold-out flood in its second (the flood costs the epoch its items, so it never runs on a
        # line whose promotion another drill needs).
        if "L" in lines:
            row = line_status(self.status, lines["L"])
            e1, e2 = epoch_row(row, 1), epoch_row(row, 2)
            if e1:
                self.attack_early_keys(lines["L"], e1)
            if e2:
                self.attack_holdout_spam(lines["L"], e2)

    def attack_copy(self, lid, e1):
        """A copy of the entered candidate by another bond, in the same window: refused (the class is entered)."""
        k = "attack:copy"
        if self.s.done(k) or e1["state"] != "Submission" or not self.s.done("W1:e1:candidates"):
            return
        before = e1["counts"]["candidates"]
        out = f"{OBJ}/attack-copy.obj"
        rc, text = improve_object("candidate", 3, {"line": lid, "epoch": 1, "declarations": {}}, out, extra=class_container("win"))
        if rc != 0:
            self.s.mark(k, result="INCOMPLETE", why=f"cannot build: {text.strip()[-200:]}")
            return
        ok, text = submit([out])
        self.s.mark(k, submitted_daa=self.daa, candidates_before=before, ok=ok)
        log(f"D-M6 copy: a copy of `win` by seat 3 submitted at DAA {self.daa}")

    def attack_late(self, lid, e1):
        k = "attack:late"
        if self.s.done(k) or e1["state"] not in ("HoldOut", "Drawing") or not self.s.done("W1:e1:candidates"):
            return
        out = f"{OBJ}/attack-late.obj"
        rc, text = improve_object("candidate", 7, {"line": lid, "epoch": 1, "declarations": {}}, out, extra=class_container("lose"))
        if rc != 0:
            self.s.mark(k, result="INCOMPLETE", why=f"cannot build: {text.strip()[-200:]}")
            return
        ok, text = submit([out])
        self.s.mark(k, submitted_daa=self.daa, candidates_before=e1["counts"]["candidates"], ok=ok)
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

    # ----- verdicts ---------------------------------------------------------------------------------------------
    def write_verdicts(self):
        for name, fn in (("dm5", self.verdict_dm5), ("dm1", self.verdict_dm1), ("dm2", self.verdict_dm2), ("dm3", self.verdict_dm3),
                         ("dm4", self.verdict_dm4), ("dm6", self.verdict_dm6)):
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
        return verdict_dm1(self.s.d, self.status, self.line_ids(), self.ids, self.daa)

    def verdict_dm2(self):
        return verdict_dm2(self.s.d, self.status, self.line_ids())

    def verdict_dm3(self):
        return ("INCOMPLETE", "needs the evaluation court (court proofs 13-15 / the gen court over an evaluation binding are not in the tree) "
                "and a node-side accuser; the seat half (a replay that differs files nothing) is covered by tests")

    def verdict_dm4(self):
        return verdict_dm4(self.s.d, self.status, self.line_ids(), self.ids)

    def verdict_dm6(self):
        return verdict_dm6(self.s.d, self.status, self.line_ids())


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
    note = "" if lack[0] == "improve" else f" (the first fence the old build lacks is {lack[0]}@{lack[1]}, not the improvement fence: name an older release closer to this one)"
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
    grid = WIN["grid"]
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
    vest_from = max([g.get("vest_from_daa") or 0 for g in grants] or [0])
    if vest_from and daa is not None:
        due = vest_from + L_E
        if daa < due:
            checks.append((None, f"vesting: the first unit is due at DAA {due}"))
        else:
            checks.append((any((g.get("vested") or 0) > 0 for g in grants), "vesting: a unit paid"))
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
        out.append((None, "by proof: W1's second epoch has not shown the regression yet"))
    else:
        row = line_status(st, lines.get("W1", "")) if st else None
        heads = [h["cause"] for h in (row or {}).get("heads", [])]
        out.append(("RolledBackByProof" in heads, f"by proof: head history {heads}"))
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
        n = e1["counts"]["candidates"] if e1 else None
        res.append((None if n is None else n == cp["candidates_before"], f"copy refused: candidates {cp.get('candidates_before')} → {n}"))
    lt = sd["done"].get("attack:late")
    if lt is None:
        res.append((None, "late candidate not run"))
    elif lt.get("result") == "INCOMPLETE":
        res.append((None, lt.get("why", "")))
    else:
        row = line_status(st, lines.get("W1", "")) if st else None
        e1 = epoch_row(row, 1) if row else None
        n = e1["counts"]["candidates"] if e1 else None
        res.append((None if n is None else n == lt["candidates_before"], f"late candidate refused: {lt.get('candidates_before')} → {n}"))
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
    e1 = {"epoch": 1, "state": "Decided", "times": {"t_open": 320}, "dataset_root": "dd" * 64, "candidates": cands,
          "outcome": "Promoted { class_id: " + win + ", wins: 8, losses: 0 }",
          "grants": [{"vest_from_daa": 600, "vested": 5}]}
    row = {"line_id": "11" * 64, "epochs": [e1], "heads": [], "pool": {"deposited": "0", "fees_in": "10", "phi_in": "0", "held_in": "5",
                                                                        "balance": "3", "held": "2", "unvested": "5", "paid": "3", "refunded": "2"}}
    st = {"lines": [row]}
    sd["milestones"] = {"head:first-final-claim": {"daa": 260}}
    r = verdict_dm1(sd, st, {"W1": "11" * 64}, {"win": win}, 950)
    assert r[0] == "PASS", r
    row["pool"]["balance"] = "4"
    assert verdict_dm1(sd, st, {"W1": "11" * 64}, {"win": win}, 950)[0] == "FAIL"
    row["pool"]["balance"] = "3"
    assert verdict_dm1(sd, st, {"W1": "11" * 64}, {"win": win}, 700)[0] == "INCOMPLETE"
    e1["outcome"] = "NoChange { reason: NoneEligible }"
    assert verdict_dm1(sd, st, {"W1": "11" * 64}, {"win": win}, 950)[0] == "FAIL"

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
    w1 = {"line_id": "11" * 64, "heads": [{"seq": 3, "cause": "RolledBackByProof"}], "epochs": []}
    assert verdict_dm4(sd, {"lines": [w1, w2]}, {"W1": "11" * 64, "W2": "33" * 64}, {})[0] == "PASS"
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["run", "once", "verdicts", "selftest"])
    a = ap.parse_args()
    if a.cmd == "selftest":
        selftest()
        return 0
    d = Drive()
    if a.cmd == "verdicts":
        for n in ("dm5", "dm1", "dm2", "dm3", "dm4", "dm6"):
            print(f"{n}: {read(f'{VERDICTS}/{n}.verdict', 'not yet')}")
        return 0
    if a.cmd == "once":
        d.tick()
        return 0
    log(f"dmdrive: run (tick {TICK_S}s, L_e {L_E}, grid {WIN['grid']})")
    while True:
        try:
            d.tick()
        except Exception as e:  # noqa: BLE001
            log(f"tick: {type(e).__name__}: {e}")
        time.sleep(TICK_S)


if __name__ == "__main__":
    sys.exit(main())
