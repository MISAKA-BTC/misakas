#!/usr/bin/env python3
"""panel_bias.py - read-only testnet-12 panel-seating monitor (panel-seed stopgap, lane C).

Who gets seated on which claim's panel, under which anchor producer, compared with what a fair
stake-weighted draw would give. Stdlib only, one file: it runs from the Mac or from cron on a
fleet host with nothing installed. It sends read calls only (getBlockDagInfo, getPalwNodeStatus,
getPalwModelRegistry, getPalwPanelSeats, getPalwClaims, getPalwPanelAssignments,
getVirtualChainFromBlock, getBlocks, getBlock). Nothing it sends changes node state.

What it records, per claim (kept in a state file so the evidence accumulates across runs and
survives the claims' retirement from the node's state):
  claim id, class, producer bond, anchor block, the anchor's producer bond, panel seats (genesis
  bonds named by host), each seat's receipt status, and the licence / void outcome.

The anchor. Past R-core+ (testnet-12 arms it at genesis) a claim's panel is drawn from its anchor
block: the FIRST selected-chain attempt block (algo 6 or 9) whose DAA is at or past
bind_base + anchor_delay, where bind_base = reboundDaa, else acceptedDaa (processor.rs
palw_v2_anchor_fact_of_candidate, palw_block_may_anchor_a_panel_v1). The panel binds IN that
block (SW-8), so the anchor's DAA equals the claim's boundDaa; the monitor checks that for every
bound claim and reports any mismatch. The anchor's producer is the executor bond in its header's
PAV2 attempt envelope.

The null model. A panel of k seats is a successive (exponential-race) sample without replacement
from the claim's population, weighted by each bond's posted collateral in whole MSK capped at
--weight-cap-msk (ADR-0152 SW-2/SW-3; on testnet-12 one operator is one bond). The population is
modelled at the first observation after the bind: bonds capable of the class, Active, not the
executor; for a class with seat rows only the rows that are ready; for a base class (the floor)
every capable bond at or above --seat-floor-msk; a non-genesis bond only once the maturity rule
admits it; every bond actually seated is in its own claim's population. The transient Valid-lock
filter (a seat whose free stake cannot post the bind's lock) is not visible over RPC after the
fact, so a saturated seat is modelled as eligible; the pooled row of the report shows such
population-wide effects, which hit every anchor producer alike.

The tests, and their false-alarm rate (both under H0 = "every panel is the fair draw above"):
  cells    for every (anchor producer, seat bond): the count of that producer's claims seating
           that bond is an exact Poisson-binomial variable (one Bernoulli per claim, probability
           the bond's inclusion probability in that claim's draw). Two-sided exact p-value;
           Holm-Bonferroni over every cell of the run at family-wise --alpha.
  omnibus  per anchor producer: Pearson X2 of observed vs expected seat counts, with the
           Rao-Scott second-order (Satterthwaite) correction for the without-replacement,
           unequal-population design; Bonferroni over anchor producers at --alpha. Evaluated
           only when every expected count is at least --min-expected.
  So one run raises a false bias alert with probability <= 2 * alpha (1e-4 each by default,
  <= 2e-4 per run; <= 0.5% per day hourly by the union bound, less in practice because
  consecutive runs share data).

Other alerts: a panel seating a non-genesis bond, and a claim anchored by a non-genesis producer
(each alerted once, when first seen; --realert repeats them). An anchor that does not match the
claim's boundDaa, or data the node would not give, makes the run DEGRADED.

Output: a human report on stdout, then ONE machine line `PANEL_BIAS {json}` (with --quiet only
that line). Exit 0 = OK, 1 = ALERT, 2 = DEGRADED / could not evaluate (an alert wins over
degraded). See README.md for running it hourly.
"""
import argparse
import base64
import bisect
import collections
import datetime
import heapq
import json
import math
import os
import random
import socket
import ssl
import struct
import sys
import time
from urllib.parse import urlparse

# ------------------------------------------------------------------ testnet-12 facts (release 0e8ec984e)
T12_GENESIS = ("a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f025"
               "42a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23")
T12_PREMINE = ("5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035"
               "099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669")
T12_FP = "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f"
# The 8 genesis bonds are premine outputs 0..7 = the operator's fleet (contrib/t12-deploy-kit PLAN.md).
DEFAULT_ROSTER = {
    0: ("ibm", "ibm node0 (8k producer)"),
    1: ("ibm", "ibm node1 (floor producer)"),
    2: ("5.104", "5.104.81.23 seat 2"),
    3: ("5.104", "5.104.81.23 seat 3"),
    4: ("5.104", "5.104.81.23 seat 4"),
    5: ("5.104", "5.104.81.23 seat 5"),
    6: (".113", ".113 explorer node (floor producer)"),
    7: ("5.104", "5.104.81.23 seat 7"),
}
CLASS_LABELS = {"ebf44d0a": "8k", "f1c5635c": "floor", "74c67e63": "2M"}
ATTEMPT_ALGOS = (6, 9)          # is_palw_attempt_algo_id: the only lanes that may anchor past R-core+
ANCHOR_DELAY = 20               # the RC lattice's anchor_delay (live check: acceptedDaa 19 -> bound 39)
WEIGHT_CAP_MSK = 1_000_000      # SW-2's weight_cap_msk
SEAT_FLOOR_MSK = 130_000        # the t12 seat floor (palw_draw_operator_weight_msk_v1's doc)
MATURITY_ACTIVATION = 1_000     # PALW_T12_BOND_MATURITY_WINDOW_DAA: activation and window
MATURITY_WINDOW = 1_000
SOMPI_PER_MSK = 100_000_000
TERMINAL_PHASES = ("final", "voided")
PAV2_MAGIC = b"PAV2"


# ------------------------------------------------------------------ statistics (pure; unit-tested)
def inclusion_probabilities(weights, k, mc_samples=40_000, seed=12345, max_states=200_000):
    """Successive sampling of k items without replacement with probability proportional to weight
    (the exponential race: the k smallest Exp(1)/w). Returns (pi, pij): pi[i] = P(i drawn),
    pij[i][j] = P(i and j both drawn), pij[i][i] = pi[i].

    Exact by a DP over how many items of each distinct weight have been drawn (items of equal
    weight are exchangeable), so eight equal genesis bonds cost nothing. Falls back to a seeded
    Monte Carlo only when the weights take so many distinct values that the DP would exceed
    max_states."""
    n = len(weights)
    if n == 0:
        return [], []
    if any(w <= 0 for w in weights):
        raise ValueError("weights must be positive")
    if k >= n:
        return [1.0] * n, [[1.0] * n for _ in range(n)]
    if k <= 0:
        return [0.0] * n, [[0.0] * n for _ in range(n)]
    levels = sorted(set(weights))
    G = len(levels)
    if math.comb(k + G, G) > max_states:
        return _inclusion_mc(weights, k, mc_samples, seed)
    gidx = {w: g for g, w in enumerate(levels)}
    size = [0] * G
    for w in weights:
        size[gidx[w]] += 1
    dist = {tuple([0] * G): 1.0}
    for _ in range(k):
        nxt = collections.defaultdict(float)
        for state, p in dist.items():
            total = sum((size[g] - state[g]) * levels[g] for g in range(G))
            for g in range(G):
                left = size[g] - state[g]
                if left <= 0:
                    continue
                s = list(state)
                s[g] += 1
                nxt[tuple(s)] += p * left * levels[g] / total
        dist = nxt
    e1 = [0.0] * G
    e2 = [[0.0] * G for _ in range(G)]
    for state, p in dist.items():
        for g in range(G):
            e1[g] += p * state[g]
            for h in range(G):
                e2[g][h] += p * (state[g] * (state[g] - 1) if g == h else state[g] * state[h])
    grp = [gidx[w] for w in weights]
    pi = [e1[grp[i]] / size[grp[i]] for i in range(n)]
    pij = [[0.0] * n for _ in range(n)]
    for i in range(n):
        for j in range(n):
            gi, gj = grp[i], grp[j]
            if i == j:
                pij[i][j] = pi[i]
            elif gi == gj:
                pij[i][j] = e2[gi][gi] / (size[gi] * (size[gi] - 1))
            else:
                pij[i][j] = e2[gi][gj] / (size[gi] * size[gj])
    return pi, pij


def _inclusion_mc(weights, k, samples, seed):
    rng = random.Random(seed)
    n = len(weights)
    single = [0] * n
    pair = [[0] * n for _ in range(n)]
    idx = range(n)
    for _ in range(samples):
        drawn = heapq.nsmallest(k, idx, key=lambda i: rng.expovariate(1.0) / weights[i])
        for a in drawn:
            single[a] += 1
            for b in drawn:
                pair[a][b] += 1
    pi = [c / samples for c in single]
    return pi, [[c / samples for c in row] for row in pair]


def _binomial_pmf(n, p):
    if p <= 0.0:
        return [1.0] + [0.0] * n
    if p >= 1.0:
        return [0.0] * n + [1.0]
    lp, lq = math.log(p), math.log1p(-p)
    lg = math.lgamma
    base = lg(n + 1)
    return [math.exp(base - lg(x + 1) - lg(n - x + 1) + x * lp + (n - x) * lq) for x in range(n + 1)]


def _convolve(a, b):
    out = [0.0] * (len(a) + len(b) - 1)
    for i, x in enumerate(a):
        if x == 0.0:
            continue
        for j, y in enumerate(b):
            out[i + j] += x * y
    return out


def poisson_binomial_pmf(ps):
    """P(sum of independent Bernoulli(p_i) = x) for x = 0..len(ps). Exact: equal p's are grouped
    into binomials (a claim population's inclusion probability takes few distinct values), which
    are then convolved."""
    groups = collections.Counter(round(float(p), 12) for p in ps)
    pmf = [1.0]
    for p, cnt in sorted(groups.items()):
        pmf = _convolve(pmf, _binomial_pmf(cnt, p))
    return pmf


def poisson_binomial_test(ps, observed):
    """Exact two-sided test of `observed` successes against Poisson-binomial(ps):
    p = min(1, 2 * min(P(X >= obs), P(X <= obs))). Returns (p_two_sided, p_upper, p_lower)."""
    pmf = poisson_binomial_pmf(ps)
    if observed < 0 or observed >= len(pmf):
        raise ValueError(f"observed {observed} outside 0..{len(pmf) - 1}")
    upper = min(1.0, sum(pmf[observed:]))
    lower = min(1.0, sum(pmf[:observed + 1]))
    return min(1.0, 2.0 * min(upper, lower)), upper, lower


def holm(pvals, alpha):
    """Holm-Bonferroni step-down: which hypotheses are rejected at family-wise error alpha."""
    order = sorted(range(len(pvals)), key=lambda i: pvals[i])
    m = len(pvals)
    reject = [False] * m
    for rank, i in enumerate(order):
        if pvals[i] <= alpha / (m - rank):
            reject[i] = True
        else:
            break
    return reject


def _gammainc_upper_regularized(a, x):
    """Q(a, x) = Gamma(a, x) / Gamma(a), a > 0 (series below a+1, continued fraction above)."""
    if x <= 0:
        return 1.0
    gln = math.lgamma(a)
    if x < a + 1.0:
        term = 1.0 / a
        total = term
        ap = a
        for _ in range(10_000):
            ap += 1.0
            term *= x / ap
            total += term
            if abs(term) < abs(total) * 1e-16:
                break
        return max(0.0, 1.0 - total * math.exp(-x + a * math.log(x) - gln))
    tiny = 1e-300
    b = x + 1.0 - a
    c = 1.0 / tiny
    d = 1.0 / b
    h = d
    for i in range(1, 10_000):
        an = -i * (i - a)
        b += 2.0
        d = an * d + b
        if abs(d) < tiny:
            d = tiny
        c = b + an / c
        if abs(c) < tiny:
            c = tiny
        d = 1.0 / d
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 1e-16:
            break
    return min(1.0, math.exp(-x + a * math.log(x) - gln) * h)


def chi2_sf(x, df):
    """Survival function of the chi-square distribution; df may be non-integer (Satterthwaite)."""
    if df <= 0:
        raise ValueError("df must be positive")
    return _gammainc_upper_regularized(df / 2.0, x / 2.0)


def rao_scott_chi2(observed, expected, cov):
    """Pearson X2 = sum (O-E)^2/E with the Rao-Scott second-order correction: under H0,
    X2 ~ sum lambda_i chi2_1 (lambda = eigenvalues of D^-1 V, D = diag(E), V = Cov(O)); matched
    to g * chi2_h with g = tr(M^2)/tr(M), h = tr(M)^2/tr(M^2), M = D^-1 V.
    Returns (X2, g, h, p)."""
    m = len(observed)
    x2 = sum((observed[i] - expected[i]) ** 2 / expected[i] for i in range(m))
    tr1 = sum(cov[i][i] / expected[i] for i in range(m))
    tr2 = sum(cov[i][j] * cov[j][i] / (expected[i] * expected[j]) for i in range(m) for j in range(m))
    if tr1 <= 0 or tr2 <= 0:
        return x2, float("nan"), 0.0, 1.0
    g = tr2 / tr1
    h = tr1 * tr1 / tr2
    return x2, g, h, chi2_sf(x2 / g, h)


# ------------------------------------------------------------------ chain facts
def to_bytes(v):
    if isinstance(v, list):
        return bytes(v)
    if isinstance(v, str):
        try:
            return bytes.fromhex(v)
        except ValueError:
            return b""
    return b""


def decode_pav2(raw):
    """The PalwAttemptEnvelopeV2 in an attempt header's palwCommitment: "PAV2" + borsh
    (version u16, network_domain, challenge, class_id, executor_bond {txid Hash64, index u32},
    executor_pubkey Vec<u8>, operator_id, ...). Returns {class, bond, operator} or None."""
    b = to_bytes(raw)
    if b[:4] != PAV2_MAGIC:
        return None
    try:
        o = 4 + 2 + 64 + 64
        cls = b[o:o + 64].hex(); o += 64
        tx = b[o:o + 64].hex(); o += 64
        ix = struct.unpack_from("<I", b, o)[0]; o += 4
        plen = struct.unpack_from("<I", b, o)[0]; o += 4 + plen
        op = b[o:o + 64].hex()
        if len(cls) != 128 or len(tx) != 128 or len(op) != 128:
            return None
        return {"class": cls, "bond": f"{tx}:{ix}", "operator": op}
    except struct.error:
        return None


def header_summary(h):
    att = decode_pav2(h.get("palwCommitment"))
    return {"daa": int(h.get("daaScore") or 0), "algo": int(h.get("powAlgoId") or 0),
            "bond": att["bond"] if att else None, "cls": att["class"] if att else None}


def resolve_anchor(rec, chain, daas, anchor_delay):
    """The first chain block at or past the claim's slot whose lane may anchor. Returns
    (hash, header-summary) or (None, reason)."""
    base = rec["reb"] if rec.get("reb") is not None else rec["acc"]
    slot = base + anchor_delay
    if not chain or daas[0] >= slot:
        return None, "below-range"
    i = bisect.bisect_left(daas, slot)
    while i < len(chain) and chain[i][1]["algo"] not in ATTEMPT_ALGOS:
        i += 1
    if i >= len(chain):
        return None, "pending"
    return chain[i][0], chain[i][1]


# ------------------------------------------------------------------ JSON wRPC (ws:// and wss://)
class RpcError(Exception):
    pass


class WsRpc:
    """One WebSocket, JSON wRPC framing ({id, method, params}), reconnect-once on a dropped socket.
    Read calls only; unknown ops would drop the socket on an old node, which the retry absorbs."""

    def __init__(self, url, timeout=60):
        self.url, self.timeout = url, timeout
        self.sock, self.buf, self.next_id, self.calls = None, b"", 0, 0

    def _connect(self):
        u = urlparse(self.url)
        host = u.hostname
        port = u.port or (443 if u.scheme == "wss" else 80)
        s = socket.create_connection((host, port), timeout=self.timeout)
        if u.scheme == "wss":
            s = ssl.create_default_context().wrap_socket(s, server_hostname=host)
        key = base64.b64encode(os.urandom(16)).decode()
        path = (u.path or "/") + (("?" + u.query) if u.query else "")
        s.sendall((f"GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                   f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            chunk = s.recv(4096)
            if not chunk:
                raise ConnectionError("closed during the WebSocket handshake")
            buf += chunk
        head, rest = buf.split(b"\r\n\r\n", 1)
        status = head.split(b"\r\n")[0]
        if b" 101 " not in status:
            raise ConnectionError("WebSocket refused: " + status.decode(errors="replace"))
        self.sock, self.buf = s, rest

    def close(self):
        if self.sock:
            try:
                self.sock.close()
            except OSError:
                pass
        self.sock, self.buf = None, b""

    def _send(self, data, opcode=0x1):
        mask = os.urandom(4)
        n = len(data)
        hdr = bytes([0x80 | opcode])
        if n < 126:
            hdr += bytes([0x80 | n])
        elif n < 65536:
            hdr += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            hdr += bytes([0x80 | 127]) + struct.pack(">Q", n)
        self.sock.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _need(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(1 << 20)
            if not chunk:
                raise EOFError("connection closed")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _message(self):
        msg = b""
        while True:
            b0, b1 = self._need(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._need(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._need(8))[0]
            mask = self._need(4) if b1 & 0x80 else None
            payload = self._need(n)
            if mask:
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            op = b0 & 0x0F
            if op == 0x8:
                raise EOFError("server closed the WebSocket")
            if op == 0x9:
                self._send(payload, 0xA)
                continue
            if op == 0xA:
                continue
            msg += payload
            if b0 & 0x80:
                return msg

    def call(self, method, params=None):
        for attempt in (0, 1):
            try:
                if self.sock is None:
                    self._connect()
                self.next_id += 1
                rid = self.next_id
                self._send(json.dumps({"id": rid, "method": method, "params": params or {}}).encode())
                deadline = time.time() + self.timeout
                while time.time() < deadline:
                    m = json.loads(self._message())
                    if m.get("id") == rid:
                        self.calls += 1
                        if m.get("error"):
                            raise RpcError(f"{method}: {m['error']}")
                        return m.get("params") if m.get("params") is not None else m.get("result", {})
                raise TimeoutError(f"{method}: no answer in {self.timeout}s")
            except (OSError, EOFError, ConnectionError, TimeoutError, ValueError) as e:
                self.close()
                if attempt:
                    raise RpcError(f"{method}: {type(e).__name__}: {e}") from e
        raise RpcError(method)


# ------------------------------------------------------------------ labels
class Names:
    def __init__(self, genesis_txid, roster):
        self.genesis_txid = genesis_txid.lower()
        self.roster = roster          # index -> (host, name)

    def genesis_index(self, bond):
        if not bond or ":" not in bond:
            return None
        tx, ix = bond.rsplit(":", 1)
        if tx.lower() != self.genesis_txid:
            return None
        try:
            i = int(ix)
        except ValueError:
            return None
        return i if i in self.roster else None

    def is_genesis(self, bond):
        return self.genesis_index(bond) is not None

    def short(self, bond):
        i = self.genesis_index(bond)
        if i is not None:
            return f"g{i}"
        if not bond:
            return "?"
        tx, _, ix = bond.rpartition(":")
        return f"x{tx[:8]}:{ix}"

    def host(self, bond):
        i = self.genesis_index(bond)
        return self.roster[i][0] if i is not None else "external"

    def long(self, bond):
        i = self.genesis_index(bond)
        return f"g{i} {self.roster[i][1]}" if i is not None else f"external {bond}"


def load_roster(path):
    roster = dict(DEFAULT_ROSTER)
    if path:
        with open(path) as f:
            for k, v in json.load(f).items():
                roster[int(k)] = (v["host"], v["name"])
    return roster


def class_label(cls):
    return CLASS_LABELS.get((cls or "")[:8], (cls or "?")[:8])


# ------------------------------------------------------------------ state
def new_state():
    return {"version": 1, "claims": {}, "bonds": {}, "checkpoints": [], "alerted": {"external_seat": [], "external_anchor": []},
            "runs": 0, "last_run": None, "network": {}}


# On disk a genesis bond is "g<i>" and a class id an index into `class_table`: a claim record is
# then ~0.6 KB instead of ~2 KB (testnet-12 makes ~2,000 claims a day).
_BOND_FIELDS = ("exe", "anchorBond", "full")


def _map_record(rec, bond, cls):
    out = dict(rec)
    for f in _BOND_FIELDS:
        if out.get(f):
            out[f] = bond(out[f])
    if out.get("seats"):
        out["seats"] = [bond(s) for s in out["seats"]]
    for f in ("verdicts", "pop"):
        if out.get(f):
            out[f] = {bond(k): v for k, v in out[f].items()}
    if out.get("cls") is not None:
        out["cls"] = cls(out["cls"])
    return out


def load_state(path):
    if not path or not os.path.exists(path):
        return new_state()
    with open(path) as f:
        disk = json.load(f)
    st = new_state()
    st.update(disk)
    txid = disk.get("genesis_txid", "")
    table = disk.get("class_table", [])

    def bond(s):
        return f"{txid}:{s[1:]}" if txid and s[:1] == "g" and s[1:].isdigit() else s

    def cls(c):
        return table[c] if isinstance(c, int) and 0 <= c < len(table) else c
    st["claims"] = {cid: _map_record(r, bond, cls) for cid, r in disk.get("claims", {}).items()}
    st.pop("class_table", None)
    return st


def save_state(path, st, genesis_txid):
    if not path:
        return
    prefix = genesis_txid.lower() + ":"
    table = []
    index = {}

    def bond(b):
        return "g" + b[len(prefix):] if b.lower().startswith(prefix) else b

    def cls(c):
        if c not in index:
            index[c] = len(table)
            table.append(c)
        return index[c]
    disk = dict(st)
    disk["claims"] = {cid: _map_record(r, bond, cls) for cid, r in st["claims"].items()}
    disk["class_table"] = table
    disk["genesis_txid"] = genesis_txid.lower()
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(disk, f, separators=(",", ":"))
    os.replace(tmp, path)


def prune_state(st, tip, retain_daa):
    """Drop retired claims anchored (or accepted) more than retain_daa below the tip."""
    if not retain_daa or tip is None:
        return 0
    floor = tip - retain_daa
    old = [cid for cid, r in st["claims"].items() if r.get("gone") and (r.get("anchorDaa") or r.get("acc") or 0) < floor]
    for cid in old:
        del st["claims"][cid]
    return len(old)


# ------------------------------------------------------------------ collection
def collect(rpc, st, cfg, now):
    """Read the node and fold what it says into `st`. Returns run facts (tip, errors, counts)."""
    run = {"errors": [], "degraded": [], "new_claims": 0, "newly_bound": 0, "calls": 0}
    status = rpc.call("getPalwNodeStatus")
    dag = rpc.call("getBlockDagInfo")
    genesis = status.get("genesisHash", "")
    if cfg.expect_genesis and genesis != cfg.expect_genesis:
        raise RpcError(f"wrong network: genesis {genesis[:16]}... is not testnet-12's {cfg.expect_genesis[:16]}...")
    tip = int(dag.get("virtualDaaScore") or 0)
    run.update(tip=tip, sink=dag.get("sink", ""), network=dag.get("network", ""), fp=status.get("consensusParamsId", ""),
               genesis=genesis, pruning=dag.get("pruningPointHash", ""))
    if cfg.expect_fp and run["fp"] != cfg.expect_fp:
        run["degraded"].append(f"consensus params fp {run['fp'][:16]} is not the shipped {cfg.expect_fp[:16]} (a fence armed? check the population rules)")
    st["network"] = {"genesis": genesis, "fp": run["fp"], "network": run["network"]}

    base_classes, seats = set(), []
    try:
        registry = rpc.call("getPalwModelRegistry")
        base_classes = {c["classId"] for c in registry.get("classes") or [] if c.get("isBaseClass")}
        seats = rpc.call("getPalwPanelSeats", {"classId": ""}).get("seats") or []
    except RpcError as e:
        run["errors"].append(str(e)[:200])
    if not base_classes:
        run["degraded"].append("no base class from getPalwModelRegistry: populations cannot be modelled this run")
    rows = collections.defaultdict(dict)              # class -> bond -> row
    for r in seats:
        rows[r.get("classId", "")][r.get("bondOutpoint", "")] = r

    names = cfg.names
    bonds = set(st["bonds"])
    bonds.update(f"{names.genesis_txid}:{i}" for i in names.roster)
    for r in seats:
        bonds.add(r.get("bondOutpoint", ""))
    for rec in st["claims"].values():
        bonds.add(rec.get("exe") or "")
    bonds.discard("")

    claim_rows = {}

    def fetch_claims(bond, role):
        try:
            r = rpc.call("getPalwClaims", {"bond": bond, "role": role, "includeTerminal": True, "limit": 0})
        except RpcError as e:
            run["errors"].append(str(e)[:200])
            return
        if r.get("truncated"):
            run["degraded"].append(f"getPalwClaims truncated for {names.short(bond)} ({role})")
        if r.get("bondKnown") is not None:
            st["bonds"][bond] = {"known": bool(r.get("bondKnown")), "coll": int(r.get("bondCollateral") or 0),
                                 "slashed": int(r.get("bondSlashed") or 0), "reg": int(r.get("bondRegisteredDaa") or 0),
                                 "retiring": r.get("bondRetiringSinceDaa"), "classes": r.get("bondCapableClasses") or [],
                                 "seen": st["bonds"].get(bond, {}).get("seen", now)}
        for c in r.get("claims") or []:
            claim_rows[c["claimId"]] = c
        for c in r.get("vestingOnlyRows") or []:
            if c.get("claimId") in st["claims"]:
                claim_rows.setdefault(c["claimId"], c)

    queried = set()
    seated = {s for rec in st["claims"].values() for s in rec.get("seats") or []}
    for _round in range(3):   # new bonds show up as seats and executors of claims just read
        todo = sorted(b for b in bonds if b not in queried)
        if not todo:
            break
        for b in todo:
            queried.add(b)
            fetch_claims(b, "executor")
            if names.is_genesis(b) or b in seated or any(b in rows[c] for c in rows):
                fetch_claims(b, "seat")
        for c in claim_rows.values():
            bonds.add(c.get("executorBond") or "")
            bonds.update(c.get("seats") or [])
            seated.update(c.get("seats") or [])
        bonds.discard("")

    # Assignments: the receipt status of each seat.
    assignments = {}
    try:
        a = rpc.call("getPalwPanelAssignments", {"claimId": "", "seatId": ""})
        for x in a.get("assignments") or []:
            assignments[x["claimId"]] = x
        if a.get("truncated"):
            missing = [cid for cid, c in claim_rows.items() if c.get("seats") and cid not in assignments
                       and c.get("phase") not in TERMINAL_PHASES][:cfg.max_assignment_calls]
            for cid in missing:
                for x in rpc.call("getPalwPanelAssignments", {"claimId": cid, "seatId": ""}).get("assignments") or []:
                    assignments[x["claimId"]] = x
    except RpcError as e:
        run["errors"].append(str(e)[:200])

    # Fold claim rows into records.
    visible = set(claim_rows)
    for cid, c in claim_rows.items():
        rec = st["claims"].get(cid)
        if rec is None:
            rec = st["claims"][cid] = {"first": now}
            run["new_claims"] += 1
        was_bound = bool(rec.get("seats"))
        rec.update(cls=c.get("classId"), exe=c.get("executorBond"), fp=bool(c.get("isFreePrompt")), phase=c.get("phase"),
                   void=c.get("voidReason") or "", acc=int(c.get("acceptedDaa") or 0), accBlk=(c.get("acceptedBlock") or "")[:32],
                   reb=c.get("reboundDaa"), bound=c.get("boundDaa"), phaseDaa=c.get("phaseDaa"), last=now, gone=False)
        if c.get("seats"):
            rec["seats"] = list(c["seats"])
            if not was_bound:
                run["newly_bound"] += 1
        asg = assignments.get(cid)
        if asg:
            rec["lic"] = asg.get("licensedState")
            rec["full"] = asg.get("fullSeat")
            rec["verdicts"] = {s["seatId"]: s.get("receiptStatus") for s in asg.get("seats") or []}
    for cid, rec in st["claims"].items():
        if cid not in visible and not rec.get("gone"):
            rec["gone"] = True    # retired from the node's state (or on a reorged-out branch)

    # The chain, from a checkpoint below the lowest slot still to resolve.
    settled = ("ok", "unbound", "void-elsewhere", "below-range")
    need = [r for r in st["claims"].values() if not r.get("gone") and
            not (r.get("phase") in TERMINAL_PHASES and r.get("anchorCheck") in settled)]
    # A slot below the oldest block the node still serves stays unresolved; it must not drag every
    # later run back to the pruning point.
    slots = [(r["reb"] if r.get("reb") is not None else r["acc"]) + cfg.anchor_delay for r in need if r.get("anchorCheck") != "below-range"]
    if need and slots:
        min_slot = min(slots)
        chain = fetch_chain(rpc, st, run, min_slot, cfg)
        daas = [h["daa"] for _, h in chain]
        for _, h in chain:
            if h["bond"]:
                if h["bond"] not in st["bonds"]:
                    st["bonds"].setdefault(h["bond"], {"known": None, "coll": 0, "reg": 0, "retiring": None, "classes": [], "seen": now})
        for rec in need:
            anchor, hs = resolve_anchor(rec, chain, daas, cfg.anchor_delay)
            if anchor is None:
                if hs == "below-range" and rec.get("anchor"):
                    continue                  # keep what an earlier run resolved
                rec["anchorCheck"] = hs
                continue
            if rec.get("bound") is None and rec.get("phase") == "voided" and rec.get("phaseDaa") != hs["daa"]:
                # Voided without a panel, but not by step 4c at this block: the block is not its anchor.
                rec.update(anchor=None, anchorDaa=None, anchorBond=None, anchorAlgo=None, anchorCheck="void-elsewhere")
                continue
            rec.update(anchor=anchor, anchorDaa=hs["daa"], anchorBond=hs["bond"], anchorAlgo=hs["algo"])
            if rec.get("bound") is not None:
                rec["anchorCheck"] = "ok" if hs["daa"] == rec["bound"] else "mismatch"
            elif rec.get("phase") == "voided":
                rec["anchorCheck"] = "unbound"   # voided by step 4c at its anchor block, without a panel
            else:
                rec["anchorCheck"] = "unbound-pending"   # at its slot, the chain has not folded the bind yet
    # Populations are modelled once, at the first run that sees the claim bound with its anchor.
    for rec in st["claims"].values():
        if rec.get("seats") and rec.get("anchorCheck") == "ok" and "pop" not in rec and base_classes:
            rec["pop"] = population_for(rec, st, rows, base_classes, cfg)
    run["calls"] = rpc.calls
    return run


def fetch_chain(rpc, st, run, min_slot, cfg):
    """[(hash, header summary)] of the selected chain from the best checkpoint below min_slot to
    the sink, in chain order."""
    starts = [h for d, h in sorted(st["checkpoints"], reverse=True) if d < min_slot]
    starts.append(run["pruning"] or run["genesis"])
    chain_hashes = None
    start_errors = []
    for start in starts:
        try:
            vc = rpc.call("getVirtualChainFromBlock", {"startHash": start, "includeAcceptedTransactionIds": False})
        except RpcError as e:
            start_errors.append(str(e)[:200])            # e.g. a checkpoint below the pruning point
            continue
        if vc.get("removedChainBlockHashes"):
            continue                                          # the checkpoint left the chain: try an older one
        chain_hashes = [start] + list(vc.get("addedChainBlockHashes") or [])
        break
    if chain_hashes is None:
        run["errors"].extend(start_errors[-2:])
        run["degraded"].append("could not read the selected chain")
        return []
    headers = {}
    chain_set = set(chain_hashes)
    low = chain_hashes[0]
    for _ in range(cfg.max_block_pages):
        try:
            gb = rpc.call("getBlocks", {"lowHash": low, "includeBlocks": True, "includeTransactions": False})
        except RpcError as e:
            run["errors"].append(str(e)[:200])
            break
        fresh = 0
        for b in gb.get("blocks") or []:
            h = b.get("header") or {}
            hh = h.get("hash") or (b.get("verboseData") or {}).get("hash")
            if hh and hh not in headers:
                headers[hh] = header_summary(h)
                fresh += 1
        on_chain = [x for x in gb.get("blockHashes") or [] if x in chain_set]
        if not fresh or not on_chain or on_chain[-1] == low:
            break
        low = on_chain[-1]
    missing = [x for x in chain_hashes if x not in headers]
    for x in missing[:cfg.max_block_calls]:
        try:
            blk = rpc.call("getBlock", {"hash": x, "includeTransactions": False}).get("block") or {}
            headers[x] = header_summary(blk.get("header") or {})
        except RpcError as e:
            run["errors"].append(str(e)[:200])
            break
    chain = [(x, headers[x]) for x in chain_hashes if x in headers]
    if len(chain) < len(chain_hashes):
        run["degraded"].append(f"{len(chain_hashes) - len(chain)} chain headers unread")
        # A gap would move "the first attempt block at or past the slot": keep only the unbroken prefix.
        first_gap = next(i for i, x in enumerate(chain_hashes) if x not in headers)
        chain = [(x, headers[x]) for x in chain_hashes[:first_gap]]
    # Checkpoints: one chain block per `checkpoint_every` DAA, kept for the next run.
    fresh_cps = {}
    for x, hs in chain:
        fresh_cps.setdefault(hs["daa"] - hs["daa"] % cfg.checkpoint_every, x)   # the bucket's first chain block
    cps = {d: h for d, h in st["checkpoints"]}
    cps.update(fresh_cps)                                     # today's chain replaces a reorged-out checkpoint
    st["checkpoints"] = sorted([d, h] for d, h in cps.items())[-cfg.keep_checkpoints:]
    run["chain_blocks"] = len(chain)
    return chain


def population_for(rec, st, rows, base_classes, cfg):
    """{bond: weight_msk} the claim's panel is modelled as drawn from (executor excluded).

    palw_bond_may_judge_class_v4: a non-base class seats the bonds with a fresh readiness row
    (getPalwPanelSeats `ready`); the base class seats the bonds that declared it. Both need an
    Active bond at the panel floor, and past the maturity fence a registration older than the
    window (genesis bonds are registered at 0)."""
    cls = rec["cls"]
    names = cfg.names
    anchor_daa = rec.get("anchorDaa") or 0
    out = {}
    for bond, info in st["bonds"].items():
        if bond == rec.get("exe") or not info.get("known") or info.get("retiring") is not None:
            continue
        coll_msk = int(info.get("coll") or 0) // SOMPI_PER_MSK
        if coll_msk < cfg.seat_floor_msk:
            continue
        if cls not in base_classes:
            row = rows.get(cls, {}).get(bond)
            if row is None or not row.get("ready"):
                continue
        elif cls not in (info.get("classes") or []):
            continue
        if not names.is_genesis(bond) and anchor_daa >= cfg.maturity_activation and \
                int(info.get("reg") or 0) + cfg.maturity_window > anchor_daa:
            continue
        out[bond] = draw_weight(coll_msk, cfg.weight_cap_msk)
    for bond in rec.get("seats") or []:
        if bond not in out:
            info = st["bonds"].get(bond, {})
            out[bond] = draw_weight(int(info.get("coll") or 0) // SOMPI_PER_MSK, cfg.weight_cap_msk)
    return out


def draw_weight(coll_msk, cap):
    return max(1, min(int(coll_msk), int(cap)))


# ------------------------------------------------------------------ analysis
def analyse(records, names, alpha=1e-4, min_expected=5.0, window_from=None):
    """The seat-draw distribution per anchor producer against the stake-weighted expectation.
    `records` are the state's claim records; returns a dict the report and the summary read."""
    cache = {}

    def probs(pop, k):
        key = (tuple(sorted(pop.items())), k)
        if key not in cache:
            bonds = [b for b, _ in key[0]]
            pi, pij = inclusion_probabilities([w for _, w in key[0]], k)
            cache[key] = (bonds, {b: pi[i] for i, b in enumerate(bonds)},
                          {(bonds[i], bonds[j]): pij[i][j] for i in range(len(bonds)) for j in range(len(bonds))})
        return cache[key]

    used = []
    for cid, r in records.items():
        if not r.get("seats") or r.get("anchorCheck") != "ok" or not r.get("pop") or not r.get("anchorBond"):
            continue
        if window_from is not None and (r.get("anchorDaa") or 0) < window_from:
            continue
        used.append((cid, r))

    per = collections.defaultdict(lambda: {"claims": 0, "obs": collections.Counter(), "ps": collections.defaultdict(list),
                                           "cov": collections.defaultdict(float)})
    for cid, r in used:
        pop = dict(r["pop"])
        pop.pop(r.get("exe"), None)
        for s in r["seats"]:
            pop.setdefault(s, 1)
        k = len(r["seats"])
        bonds, pi, pij = probs(pop, k)
        for A in (r["anchorBond"], "*"):
            cell = per[A]
            cell["claims"] += 1
            for s in r["seats"]:
                cell["obs"][s] += 1
            for b in bonds:
                cell["ps"][b].append(pi[b])
                for c in bonds:
                    cell["cov"][(b, c)] += pij[(b, c)] - pi[b] * pi[c]

    anchors = sorted(a for a in per if a != "*")
    cells = []
    for A in anchors + ["*"]:
        P = per[A]
        for b in sorted(P["ps"]):
            ps = P["ps"][b]
            o = P["obs"][b]
            e = sum(ps)
            p2, pu, pl = poisson_binomial_test(ps, o)
            cells.append({"anchor": A, "seat": b, "n": len(ps), "obs": o, "exp": e, "p": p2, "p_upper": pu, "p_lower": pl})
    tested = [c for c in cells if c["anchor"] != "*"]
    for c, rej in zip(tested, holm([c["p"] for c in tested], alpha)):
        c["significant"] = rej
    pooled = [c for c in cells if c["anchor"] == "*"]
    for c, rej in zip(pooled, holm([c["p"] for c in pooled], alpha)):
        c["significant"] = rej

    omnibus = []
    for A in anchors:
        P = per[A]
        bonds = sorted(P["ps"])
        exp = [sum(P["ps"][b]) for b in bonds]
        obs = [P["obs"][b] for b in bonds]
        keep = [i for i, e in enumerate(exp) if e > 1e-12]
        row = {"anchor": A, "claims": P["claims"], "bonds": len(keep), "min_exp": min((exp[i] for i in keep), default=0.0)}
        if len(keep) < 2 or row["min_exp"] < min_expected:
            row.update(evaluated=False, reason=f"min expected {row['min_exp']:.1f} < {min_expected:g}" if len(keep) >= 2 else "fewer than 2 bonds")
        else:
            cov = [[P["cov"][(bonds[i], bonds[j])] for j in keep] for i in keep]
            x2, g, h, p = rao_scott_chi2([obs[i] for i in keep], [exp[i] for i in keep], cov)
            row.update(evaluated=True, x2=x2, g=g, h=h, p=p)
        omnibus.append(row)
    n_omni = sum(1 for r in omnibus if r["evaluated"])
    for r in omnibus:
        r["significant"] = bool(r["evaluated"] and r["p"] <= alpha / max(1, n_omni))

    hosts = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0.0]))
    for c in cells:
        slot = hosts[c["anchor"]][names.host(c["seat"])]
        slot[0] += c["obs"]
        slot[1] += c["exp"]
    return {"claims_used": len(used), "anchors": anchors, "cells": cells, "omnibus": omnibus,
            "hosts": {a: {h: tuple(v) for h, v in hs.items()} for a, hs in hosts.items()},
            "alpha": alpha, "cells_tested": len(tested), "omnibus_tested": n_omni}


def find_external(records, names, alerted, realert):
    ext_seat, ext_anchor = [], []
    for cid, r in records.items():
        if any(not names.is_genesis(s) for s in r.get("seats") or []):
            ext_seat.append(cid)
        if r.get("anchorBond") and r.get("anchorCheck") in ("ok", "unbound") and not names.is_genesis(r["anchorBond"]):
            ext_anchor.append(cid)
    new_seat = [c for c in ext_seat if realert or c not in set(alerted.get("external_seat", []))]
    new_anchor = [c for c in ext_anchor if realert or c not in set(alerted.get("external_anchor", []))]
    return ext_seat, ext_anchor, new_seat, new_anchor


# ------------------------------------------------------------------ report
def fmt_p(p):
    return f"{p:.2g}" if p >= 1e-3 else f"{p:.1e}"


VERDICT = {"valid": "V", "pending": ".", "none": "-"}


def claim_line(cid, r, names):
    seats = []
    for s in r.get("seats") or []:
        v = VERDICT.get((r.get("verdicts") or {}).get(s), "?")
        seats.append(f"{names.short(s)}{'*' if s == r.get('full') else ''}{v}")
    outcome = r.get("phase") or "?"
    if r.get("void"):
        outcome += f" ({r['void']})"
    anchor = f"{(r.get('anchor') or '')[:12]}@{r.get('anchorDaa')}" if r.get("anchor") else (r.get("anchorCheck") or "-")
    if r.get("anchorCheck") == "mismatch":
        anchor += " MISMATCH"
    return (f"{cid[:16]}  {class_label(r.get('cls')):<5}  {names.short(r.get('exe')):<5} {r.get('acc', ''):>5}  "
            f"{anchor:<20} {names.short(r.get('anchorBond')) if r.get('anchorBond') else '-':<5}  "
            f"{' '.join(seats) if seats else '-':<28}  {outcome}")


def report(st, run, res, alerts, cfg, names, out):
    w = out.write
    ts = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).strftime("%Y-%m-%d %H:%M:%S JST")
    recs = st["claims"]
    phases = collections.Counter(r.get("phase") for r in recs.values())
    w(f"testnet-12 panel-bias monitor - {ts}\n")
    if cfg.offline:
        w(f"offline analysis of {cfg.state}\n")
    elif run.get("tip") is not None:
        w(f"endpoint {cfg.url} | network {run.get('network')} | fp {run.get('fp', '')[:12]} | genesis {run.get('genesis', '')[:12]} | "
          f"tip DAA {run.get('tip')} | sink {run.get('sink', '')[:12]} | {run.get('calls')} read calls\n")
    checks = collections.Counter(r.get("anchorCheck") for r in recs.values() if r.get("seats"))
    w(f"records: {len(recs)} claims ({', '.join(f'{k} {v}' for k, v in sorted(phases.items(), key=lambda kv: str(kv[0])))}); "
      f"bound {sum(1 for r in recs.values() if r.get('seats'))}, anchors {dict(checks)}; "
      f"this run +{run.get('new_claims', 0) if run else 0} new, +{run.get('newly_bound', 0) if run else 0} newly bound\n\n")

    anchored = [(cid, r) for cid, r in recs.items() if r.get("seats") or r.get("anchorCheck") == "unbound"]
    newest = sorted(anchored, key=lambda kv: (kv[1].get("anchorDaa") or kv[1].get("bound") or 0, kv[1].get("acc") or 0, kv[0]),
                    reverse=True)[:cfg.show]
    w(f"Recent panels (newest {len(newest)} of {len(anchored)} anchored claims; seat verdict V valid . pending - none ? unknown; "
      f"* full seat)\n")
    w(f"{'claim':<16}  {'class':<5}  {'prod':<5} {'accDAA':>5}  {'anchor@DAA':<20} {'by':<5}  {'seats':<28}  outcome\n")
    for cid, r in newest:
        w(claim_line(cid, r, names) + "\n")
    waiting = [r for r in recs.values() if not r.get("seats") and r.get("phase") == "provisional" and not r.get("gone")]
    if waiting:
        by_cls = collections.Counter(class_label(r.get("cls")) for r in waiting)
        accs = [r.get("acc") or 0 for r in waiting]
        w(f"  + {len(waiting)} provisional claim(s) awaiting their anchor ({', '.join(f'{k} {v}' for k, v in sorted(by_cls.items()))}; "
          f"accepted DAA {min(accs)}..{max(accs)}, slots from DAA {min(accs) + cfg.anchor_delay})\n")
    voids = collections.Counter(r.get("void") or "?" for r in recs.values() if not r.get("seats") and r.get("phase") == "voided")
    if voids:
        w(f"  + voided without a panel: {', '.join(f'{k} {v}' for k, v in sorted(voids.items()))}\n")

    w(f"\nSeat draws per anchor producer: observed/expected (stake-weighted), {res['claims_used']} bound claims with a resolved anchor\n")
    seats = sorted({c["seat"] for c in res["cells"]}, key=lambda b: (not names.is_genesis(b), names.short(b)))
    w(f"{'anchor':<18}{'claims':>7}  " + "".join(f"{names.short(b):>11}" for b in seats) + "\n")
    by = {(c["anchor"], c["seat"]): c for c in res["cells"]}
    for A in res["anchors"] + ["*"]:
        n = next((o["claims"] for o in res["omnibus"] if o["anchor"] == A), None)
        if A == "*":
            n = res["claims_used"]
        label = "all (pooled)" if A == "*" else f"{names.short(A)} {names.host(A)}"
        cells = []
        for b in seats:
            c = by.get((A, b))
            cells.append(f"{c['obs']}/{c['exp']:.1f}{'!' if c.get('significant') else ''}" if c else "-")
        w(f"{label:<18}{n:>7}  " + "".join(f"{x:>11}" for x in cells) + "\n")
    w("\nBy host: observed/expected seats\n")
    hosts = sorted({h for hs in res["hosts"].values() for h in hs})
    w(f"{'anchor':<18}" + "".join(f"{h:>14}" for h in hosts) + "\n")
    for A in res["anchors"] + ["*"]:
        label = "all (pooled)" if A == "*" else f"{names.short(A)} {names.host(A)}"
        hs = res["hosts"].get(A, {})
        w(f"{label:<18}" + "".join(f"{(str(hs[h][0]) + '/' + format(hs[h][1], '.1f')) if h in hs else '-':>14}" for h in hosts) + "\n")

    w(f"\nTests (H0: each panel is a stake-weighted successive sample of its modelled population; family-wise alpha {res['alpha']:g} each)\n")
    tested = [c for c in res["cells"] if c["anchor"] != "*"]
    sig = [c for c in tested if c.get("significant")]
    if tested:
        low = min(tested, key=lambda c: c["p"])
        w(f"  cells: exact two-sided Poisson-binomial, Holm over {len(tested)} cells -> {len(sig)} significant; "
          f"smallest p {fmt_p(low['p'])} ({names.short(low['anchor'])} anchors -> {names.short(low['seat'])} seated {low['obs']}/{low['exp']:.1f})\n")
    else:
        w("  cells: nothing to test yet (no bound claim with a resolved anchor and population)\n")
    for c in sig:
        direction = "OVER" if c["obs"] > c["exp"] else "UNDER"
        w(f"    ! {names.short(c['anchor'])} anchors seat {names.long(c['seat'])} {direction}: {c['obs']}/{c['exp']:.1f} over {c['n']} claims, p {fmt_p(c['p'])}\n")
    for o in res["omnibus"]:
        if o["evaluated"]:
            w(f"  omnibus {names.short(o['anchor'])}: Rao-Scott X2 {o['x2']:.2f} (g {o['g']:.2f}, df {o['h']:.2f}) p {fmt_p(o['p'])}"
              f" over {o['claims']} claims, {o['bonds']} bonds{' SIGNIFICANT' if o['significant'] else ''}\n")
        else:
            w(f"  omnibus {names.short(o['anchor'])}: not evaluated ({o['reason']}) over {o['claims']} claims\n")
    pooled_sig = [c for c in res["cells"] if c["anchor"] == "*" and c.get("significant")]
    for c in pooled_sig:
        w(f"  population note: {names.short(c['seat'])} is seated {c['obs']}/{c['exp']:.1f} over all anchors (p {fmt_p(c['p'])}) - "
          f"a population-wide effect (eligibility, saturation, readiness), not by itself an anchor bias\n")

    w("\nAlerts\n")
    if not alerts and not (run or {}).get("degraded") and not (run or {}).get("errors"):
        w("  none\n")
    for a in alerts:
        w(f"  ALERT {a}\n")
    for d in (run or {}).get("degraded", []):
        w(f"  DEGRADED {d}\n")
    for e in (run or {}).get("errors", [])[:10]:
        w(f"  ERROR {e}\n")


# ------------------------------------------------------------------ main
def parse_args(argv):
    ap = argparse.ArgumentParser(description="read-only testnet-12 panel-seating monitor (see the module doc)")
    ap.add_argument("--url", default="wss://misakascan.com/kaspa", help="JSON wRPC endpoint (ws:// or wss://)")
    ap.add_argument("--state", default=os.path.expanduser("~/.t12-panel-bias/state.json"),
                    help="state file (claim records, checkpoints, alerted ids); '' = none")
    ap.add_argument("--offline", action="store_true", help="analyse the state file only; no RPC")
    ap.add_argument("--alpha", type=float, default=1e-4, help="family-wise false-alarm rate of each test family per run")
    ap.add_argument("--min-expected", type=float, default=5.0, help="omnibus needs every expected count >= this")
    ap.add_argument("--window-daa", type=int, default=0, help="test only claims anchored in the last N DAA (0 = all recorded)")
    ap.add_argument("--anchor-delay", type=int, default=ANCHOR_DELAY)
    ap.add_argument("--weight-cap-msk", type=int, default=WEIGHT_CAP_MSK)
    ap.add_argument("--seat-floor-msk", type=int, default=SEAT_FLOOR_MSK)
    ap.add_argument("--maturity-activation", type=int, default=MATURITY_ACTIVATION)
    ap.add_argument("--maturity-window", type=int, default=MATURITY_WINDOW)
    ap.add_argument("--genesis-txid", default=T12_PREMINE, help="the premine txid whose outputs are the genesis bonds")
    ap.add_argument("--roster", default="", help="JSON {index: {host, name}} overriding the genesis-bond names")
    ap.add_argument("--expect-genesis", default=T12_GENESIS, help="refuse another network ('' = any)")
    ap.add_argument("--expect-fp", default=T12_FP, help="note a different consensus params fp ('' = do not check)")
    ap.add_argument("--realert", action="store_true", help="repeat external-seat/anchor alerts already raised")
    ap.add_argument("--show", type=int, default=25, help="recent panels listed in the report")
    ap.add_argument("--retain-daa", type=int, default=20_000,
                    help="forget retired claims anchored more than this many DAA below the tip (0 = keep all)")
    ap.add_argument("--json", default="", help="also write the full result (records + analysis) to this file")
    ap.add_argument("--quiet", action="store_true", help="print only the machine summary line")
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--checkpoint-every", type=int, default=25)
    ap.add_argument("--keep-checkpoints", type=int, default=400)
    ap.add_argument("--max-block-pages", type=int, default=400)
    ap.add_argument("--max-block-calls", type=int, default=300)
    ap.add_argument("--max-assignment-calls", type=int, default=200)
    return ap.parse_args(argv)


def main(argv=None):
    cfg = parse_args(argv)
    cfg.names = Names(cfg.genesis_txid, load_roster(cfg.roster))
    names = cfg.names
    now = int(time.time())
    st = load_state(cfg.state)
    run = {"errors": [], "degraded": []}
    fatal = None
    if not cfg.offline:
        rpc = WsRpc(cfg.url, cfg.timeout)
        try:
            run = collect(rpc, st, cfg, now)
        except Exception as e:                      # noqa: BLE001 - an unreadable node is DEGRADED, never a crash
            fatal = f"{type(e).__name__}: {e}"[:300]
            run = {"errors": [fatal], "degraded": [], "calls": rpc.calls}
        finally:
            rpc.close()
    records = st["claims"]
    window_from = None
    if cfg.window_daa and run.get("tip"):
        window_from = run["tip"] - cfg.window_daa
    res = analyse(records, names, cfg.alpha, cfg.min_expected, window_from)
    ext_seat, ext_anchor, new_seat, new_anchor = find_external(records, names, st["alerted"], cfg.realert)

    alerts = []
    for c in res["cells"]:
        if c["anchor"] != "*" and c.get("significant"):
            alerts.append(f"bias: {names.short(c['anchor'])} anchors seat {names.short(c['seat'])} "
                          f"{'over' if c['obs'] > c['exp'] else 'under'} ({c['obs']}/{c['exp']:.1f}, n {c['n']}, p {fmt_p(c['p'])})")
    for o in res["omnibus"]:
        if o.get("significant"):
            alerts.append(f"bias: {names.short(o['anchor'])}'s panels differ from the stake-weighted draw (Rao-Scott p {fmt_p(o['p'])}, {o['claims']} claims)")
    for cid in new_seat:
        r = records[cid]
        ext = [names.short(s) for s in r["seats"] if not names.is_genesis(s)]
        alerts.append(f"external seat: claim {cid[:16]} ({class_label(r.get('cls'))}, anchor by {names.short(r.get('anchorBond'))}) seats {' '.join(ext)}")
    for cid in new_anchor:
        r = records[cid]
        alerts.append(f"external anchor: claim {cid[:16]} ({class_label(r.get('cls'))}) anchored at DAA {r.get('anchorDaa')} by {names.long(r.get('anchorBond'))}")
    mismatches = [cid for cid, r in records.items() if r.get("anchorCheck") == "mismatch" and not r.get("gone")]
    if mismatches:
        run.setdefault("degraded", []).append(f"{len(mismatches)} bound claim(s) whose resolved anchor DAA != boundDaa "
                                              f"(e.g. {mismatches[0][:16]}): the anchor rule the monitor models no longer holds")
    if not cfg.offline and fatal is None:
        st["alerted"]["external_seat"] = sorted(set(st["alerted"].get("external_seat", [])) | set(ext_seat))
        st["alerted"]["external_anchor"] = sorted(set(st["alerted"].get("external_anchor", [])) | set(ext_anchor))
        st["runs"] = st.get("runs", 0) + 1
        st["last_run"] = now
        run["pruned"] = prune_state(st, run.get("tip"), cfg.retain_daa)
        save_state(cfg.state, st, cfg.genesis_txid)

    degraded = bool(fatal or run.get("degraded") or run.get("errors"))
    status = "ALERT" if alerts else ("DEGRADED" if degraded else "OK")
    by_anchor = collections.Counter(names.short(r["anchorBond"]) for r in records.values()
                                    if r.get("seats") and r.get("anchorCheck") == "ok" and r.get("anchorBond"))
    min_cell = min((c["p"] for c in res["cells"] if c["anchor"] != "*"), default=None)
    min_omni = min((o["p"] for o in res["omnibus"] if o["evaluated"]), default=None)
    summary = {"status": status, "time": now, "tip": run.get("tip"), "fp": (run.get("fp") or "")[:16],
               "claims": len(records), "bound": sum(1 for r in records.values() if r.get("seats")),
               "tested": res["claims_used"], "anchors": dict(sorted(by_anchor.items())),
               "ext_seat": len(ext_seat), "ext_anchor": len(ext_anchor), "bias_cells": sum(1 for c in res["cells"] if c["anchor"] != "*" and c.get("significant")),
               "min_cell_p": None if min_cell is None else float(f"{min_cell:.3g}"),
               "min_omnibus_p": None if min_omni is None else float(f"{min_omni:.3g}"),
               "alpha": cfg.alpha, "alerts": alerts, "degraded": (run.get("degraded") or []) + (run.get("errors") or [])[:5]}
    if not cfg.quiet:
        report(st, run, res, alerts, cfg, names, sys.stdout)
        sys.stdout.write("\n")
    sys.stdout.write("PANEL_BIAS " + json.dumps(summary, separators=(",", ":")) + "\n")
    if cfg.json:
        with open(cfg.json, "w") as f:
            json.dump({"summary": summary, "run": run, "analysis": res, "records": records}, f, indent=1, default=str)
    return 1 if alerts else (2 if degraded else 0)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 - Python's own crash exit (1) would read as ALERT to cron
        import traceback
        traceback.print_exc()
        print("PANEL_BIAS " + json.dumps({"status": "DEGRADED", "time": int(time.time()), "alerts": [],
                                          "degraded": [f"monitor crashed: {type(exc).__name__}: {exc}"[:300]]}))
        sys.exit(2)
