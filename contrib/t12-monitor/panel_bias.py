#!/usr/bin/env python3
"""panel_bias.py - read-only testnet-12 panel-seating monitor (panel-seed stopgap, lane C).

Who gets seated on which claim's panel, under which anchor producer, compared first with what the
OTHER anchor producers' panels seat at the same time, and second with what a fair stake-weighted
draw would give. Stdlib only, one file: it runs from the Mac or from cron on a fleet host with
nothing installed. It sends read calls only (getBlockDagInfo, getPalwNodeStatus,
getPalwModelRegistry, getPalwPanelSeats, getPalwClaims, getPalwPanelAssignments,
getVirtualChainFromBlock, getBlocks, getBlock). Nothing it sends changes node state.

What it records, per claim (kept in a state file so the evidence accumulates across runs and
survives the claims' retirement from the node's state):
  claim id, class, producer bond, anchor block, the anchor's producer bond, panel seats (genesis
  bonds named by host), each seat's receipt status, and the licence / void outcome. A claim that is
  redrawn and binds a second panel keeps its first panel as a separate record ("<claim>#<boundDaa>").

The anchor. Past R-core+ (testnet-12 arms it at genesis) a claim's panel is drawn from its anchor
block: the FIRST selected-chain attempt block (algo 6 or 9) whose DAA is at or past
bind_base + anchor_delay, where bind_base = reboundDaa, else acceptedDaa (processor.rs
palw_v2_anchor_fact_of_candidate, palw_block_may_anchor_a_panel_v1). The panel binds IN that
block (SW-8), so a bound claim's anchor is the first such block at its boundDaa. The monitor
reads the anchor off boundDaa and checks that the slot rule leads there:
  - a redrawn claim (reboundDaa set) is checked from reboundDaa; the panel it still shows before it
    rebinds is its first panel, checked from acceptedDaa;
  - past the registry-resilience fence (--resilience-from) a claim no capable panel could take is
    re-based on its anchor block and retried one anchor_delay on, and binds with reboundDaa reset
    to None (rcore/f1-registry-resilience); the check follows those retries;
  - past the operator-anchor stopgap (--stopgap-from, lane A) only the genesis bonds' attempts may
    anchor; a bound claim whose boundDaa holds only other producers' attempts is an ALERT.
Anything else is a mismatch and makes the run DEGRADED. The anchor's producer is the executor bond
in its header's PAV2 attempt envelope.

The tests (H0 = "the panel draw does not depend on who produced the anchor block"):
  relative cells   for every (anchor producer A, seat bond b): the number of A's claims seating b,
           against the other anchor producers' claims of the same class and executor in the same
           --stratum-daa slice of anchor DAA. Given each stratum's seat totals this count is an
           exact sum of hypergeometric variables (Fisher's exact test, stratified); two-sided,
           exact while cheap, continuity-corrected normal beyond. Holm-Bonferroni over every cell
           of every window at family-wise --alpha.
  relative omnibus per anchor producer: the generalized Cochran-Mantel-Haenszel statistic of A's
           seat-count vector against the same strata (Q = D' V+ D, chi-square with rank(V) df).
           Bonferroni over anchor producers and windows at --alpha.
  windows  every test runs on all recorded claims AND on the claims anchored in each of the last
           --windows DAA (default 50, 100, 200, 400, 800: geometric, so the window that starts
           nearest a change of behaviour is within a factor 2 of it), so a producer that turns
           after a long honest history is still caught within hours; the corrections above span
           the windows. With exactly two producers sharing strata, A-vs-B and B-vs-A are one
           hypothesis and are counted once.
  So one run raises a false bias alert with probability <= 2 * alpha (1e-4 each by default).
  Because the comparison is between anchor producers at the same time, an effect that hits the
  whole population alike (a seat saturated by the Valid lock, a readiness row gone stale, a
  bond the model missed) does not raise a bias alert.
  model notes (not alerts)  the per-cell exact Poisson-binomial test and the Rao-Scott omnibus
           against the stake-weighted successive-sampling model of each claim's population
           (ADR-0152 SW-2/SW-3: collateral in whole MSK capped at --weight-cap-msk). They show a
           population-wide effect ("population note"), and in a bias alert they tell which side of
           the pair deviates from the model. Claims whose population is uncertain (a readiness row
           re-proved after the anchor) are left out of them.

Other alerts: the first panel seating each non-genesis bond (once per bond; the relative cells
catch over-seating), and past --stopgap-from any claim anchored by a non-genesis producer (before
that height they are reported, not alerted). DEGRADED: an anchor that does not follow the rule,
a bound claim the chain read does not cover, a chain read that stops short of the sink, a claim
list whose newest 500 rows no longer reach back to the last run, an unreadable node, or a
consensus fingerprint other than --expect-fp.

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
WINDOW_BIND = 600               # the RC lattice's window_bind: at most (600 - 20) / 20 = 29 NoCapablePanel retries
WEIGHT_CAP_MSK = 1_000_000      # SW-2's weight_cap_msk
SEAT_FLOOR_MSK = 130_000        # the t12 seat floor (palw_draw_operator_weight_msk_v1's doc)
MATURITY_ACTIVATION = 1_000     # PALW_T12_BOND_MATURITY_WINDOW_DAA: activation and window
MATURITY_WINDOW = 1_000
SOMPI_PER_MSK = 100_000_000
TERMINAL_PHASES = ("final", "voided")
PAV2_MAGIC = b"PAV2"
STATE_VERSION = 2
# anchorCheck values that need no further chain reading (re-checked only while within --recheck-daa).
SETTLED = ("ok", "mismatch", "unbound", "void-elsewhere")
# anchorCheck values that are final: the chain the node serves cannot resolve them.
UNRESOLVABLE = ("below-range", "too-old")


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


def _norm_sf(z):
    return 0.5 * math.erfc(z / math.sqrt(2.0))


def _skewed_sf(z, g):
    """P(Z >= z) for a standardized variable of skewness g: the Cornish-Fisher shift of the normal
    quantile, held monotone beyond z = 1.5 / |g| (the correction only matters within a few sd)."""
    if g:
        zc = min(z, 1.5 / abs(g))
        z = z - g * (zc * zc - 1.0) / 6.0
    return _norm_sf(z)


def normal_test(observed, mean, var, k3=0.0):
    """Two-sided test of an integer count by the continuity-corrected normal approximation with a
    Cornish-Fisher skewness correction (k3 = the third central moment). Returns (p_two_sided,
    p_upper = P(X >= obs), p_lower = P(X <= obs))."""
    if var <= 0.0:
        pu = 1.0 if observed <= mean + 1e-9 else 0.0
        pl = 1.0 if observed >= mean - 1e-9 else 0.0
    else:
        sd = math.sqrt(var)
        g = k3 / (var * sd)
        pu = min(1.0, _skewed_sf((observed - 0.5 - mean) / sd, g))
        pl = min(1.0, _skewed_sf((mean - observed - 0.5) / sd, -g))
    return min(1.0, 2.0 * min(pu, pl)), pu, pl


def poisson_binomial_pmf(ps):
    """P(sum of independent Bernoulli(p_i) = x) for x = 0..len(ps). Exact: equal p's are grouped
    into binomials (a claim population's inclusion probability takes few distinct values), which
    are then convolved."""
    groups = collections.Counter(round(float(p), 12) for p in ps)
    pmf = [1.0]
    for p, cnt in sorted(groups.items()):
        pmf = _convolve(pmf, _binomial_pmf(cnt, p))
    return pmf


def poisson_binomial_test(ps, observed, exact_max=0):
    """Exact two-sided test of `observed` successes against Poisson-binomial(ps):
    p = min(1, 2 * min(P(X >= obs), P(X <= obs))). Returns (p_two_sided, p_upper, p_lower).
    With exact_max > 0 and more than exact_max trials, the continuity-corrected normal
    approximation (the exact convolution is quadratic in the number of trials)."""
    if observed < 0 or observed > len(ps):
        raise ValueError(f"observed {observed} outside 0..{len(ps)}")
    if exact_max and len(ps) > exact_max:
        return normal_test(observed, sum(ps), sum(p * (1.0 - p) for p in ps), sum(p * (1.0 - p) * (1.0 - 2.0 * p) for p in ps))
    pmf = poisson_binomial_pmf(ps)
    upper = min(1.0, sum(pmf[observed:]))
    lower = min(1.0, sum(pmf[:observed + 1]))
    return min(1.0, 2.0 * min(upper, lower)), upper, lower


def _log_comb(n, k):
    return math.lgamma(n + 1) - math.lgamma(k + 1) - math.lgamma(n - k + 1)


def hypergeom_pmf(N, K, n):
    """X = successes in n draws without replacement from N items of which K are successes.
    Returns (lo, [P(X = lo), ..., P(X = hi)])."""
    lo, hi = max(0, n + K - N), min(n, K)
    base = _log_comb(N, n)
    return lo, [math.exp(_log_comb(K, x) + _log_comb(N - K, n - x) - base) for x in range(lo, hi + 1)]


def stratified_hypergeom_test(strata, observed, max_cost=250_000):
    """Two-sided test of `observed` = the sum over strata (N, K, n) of independent
    Hypergeometric(N, K, n) counts: the stratified Fisher exact test. Exact by convolution while
    that is cheap (it costs about width^2 / 2 for a total support width), the continuity- and
    skewness-corrected normal beyond. Returns (p_two_sided, p_upper, p_lower, mean, var)."""
    fixed, mean, var, k3, parts = 0, 0.0, 0.0, 0.0, []
    for N, K, n in strata:
        lo, hi = max(0, n + K - N), min(n, K)
        if hi <= lo:
            fixed += lo
            continue
        mean += n * K / N
        var += n * (K / N) * (1.0 - K / N) * (N - n) / (N - 1)
        if N > 2:
            k3 += n * K * (N - K) * (N - n) * (N - 2 * K) * (N - 2 * n) / (N ** 3 * (N - 1) * (N - 2))
        parts.append((N, K, n, hi - lo))
    mean += fixed
    width = sum(p[3] for p in parts)
    if width * width > 2 * max_cost:
        p2, pu, pl = normal_test(observed, mean, var, k3)
        return p2, pu, pl, mean, var
    pmf, offset = [1.0], fixed
    for N, K, n, _ in parts:
        lo, h = hypergeom_pmf(N, K, n)
        pmf = _convolve(pmf, h)
        offset += lo
    x = observed - offset
    if x < 0 or x >= len(pmf):
        raise ValueError(f"observed {observed} outside {offset}..{offset + len(pmf) - 1}")
    upper = min(1.0, sum(pmf[x:]))
    lower = min(1.0, sum(pmf[:x + 1]))
    return min(1.0, 2.0 * min(upper, lower)), upper, lower, mean, var


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


def sym_eig(a, sweeps=100):
    """Eigenvalues and eigenvectors (columns of the returned matrix) of a small symmetric matrix,
    by cyclic Jacobi rotations."""
    n = len(a)
    a = [list(map(float, row)) for row in a]
    v = [[1.0 if i == j else 0.0 for j in range(n)] for i in range(n)]
    for _ in range(sweeps):
        off = sum(a[i][j] ** 2 for i in range(n) for j in range(n) if i != j)
        diag = sum(a[i][i] ** 2 for i in range(n))
        if off <= 1e-26 * max(diag, 1e-300):
            break
        for p in range(n - 1):
            for q in range(p + 1, n):
                if a[p][q] == 0.0:
                    continue
                theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q])
                t = (1.0 if theta >= 0 else -1.0) / (abs(theta) + math.sqrt(theta * theta + 1.0))
                c = 1.0 / math.sqrt(t * t + 1.0)
                s = t * c
                for k in range(n):
                    akp, akq = a[k][p], a[k][q]
                    a[k][p], a[k][q] = c * akp - s * akq, s * akp + c * akq
                for k in range(n):
                    apk, aqk = a[p][k], a[q][k]
                    a[p][k], a[q][k] = c * apk - s * aqk, s * apk + c * aqk
                for k in range(n):
                    vkp, vkq = v[k][p], v[k][q]
                    v[k][p], v[k][q] = c * vkp - s * vkq, s * vkp + c * vkq
    return [a[i][i] for i in range(n)], v


def pinv_quadratic(d, cov, rel_tol=1e-9):
    """d' V+ d and rank(V) for a symmetric positive semi-definite V (the Moore-Penrose inverse
    over the eigenvalues above rel_tol x the largest)."""
    vals, vecs = sym_eig(cov)
    top = max((abs(x) for x in vals), default=0.0)
    q, rank = 0.0, 0
    for i, lam in enumerate(vals):
        if top > 0.0 and lam > rel_tol * top:
            proj = sum(vecs[j][i] * d[j] for j in range(len(d)))
            q += proj * proj / lam
            rank += 1
    return q, rank


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


def anchor_rule(names=None, stopgap_from=None):
    """The predicate "may this chain block anchor a panel": an attempt lane, and past the
    operator-anchor stopgap (lane A, keyed on the block's own DAA) a genesis bond's attempt."""
    def admits(hs):
        if hs["algo"] not in ATTEMPT_ALGOS:
            return False
        if stopgap_from is not None and hs["daa"] >= stopgap_from:
            return names is not None and names.is_genesis(hs["bond"])
        return True
    admits.stopgap_from = stopgap_from
    return admits


def _first_admitted(chain, daas, frm, admits):
    i = bisect.bisect_left(daas, frm)
    while i < len(chain) and not admits(chain[i][1]):
        i += 1
    return i


def check_base(rec, anchor_delay):
    """The DAA the slot rule starts from. A bound claim with a reboundDaa that its panel is past was
    redrawn and rebound: from reboundDaa. A bound claim whose reboundDaa lies past its panel still
    shows its first panel (redrawn, not yet rebound): from acceptedDaa. An unbound claim: its
    bind base (reboundDaa, else acceptedDaa)."""
    reb, acc, bound = rec.get("reb"), int(rec.get("acc") or 0), rec.get("bound")
    if bound is not None:
        return reb if reb is not None and reb + anchor_delay <= bound else acc
    return reb if reb is not None else acc


def resolve_anchor(rec, chain, daas, anchor_delay, admits=None, resilience_from=None,
                   max_retries=(WINDOW_BIND - ANCHOR_DELAY) // ANCHOR_DELAY + 1):
    """The claim's anchor block on `chain` ([(hash, header summary)] in chain order, DAA
    non-decreasing). Returns {check, anchor, hs, retries, slot, breach}.

    check: ok (bound, and the slot rule leads to the first admitted attempt at its boundDaa),
    mismatch (bound, and it does not), pending (the anchor lies past the chain read),
    below-range (the slot lies below the chain read), unbound (voided by step 4c at its anchor
    block without a panel), void-elsewhere (voided, but not at this block), unbound-pending (at its
    anchor, the bind not folded yet). breach: the claim bound at a DAA holding only attempts the
    operator-anchor stopgap does not admit (a consensus failure)."""
    admits = admits or anchor_rule()
    bound = rec.get("bound")
    base = check_base(rec, anchor_delay)
    slot = base + anchor_delay
    out = {"check": None, "anchor": None, "hs": None, "retries": 0, "slot": slot, "breach": False}
    if not chain or daas[0] >= slot:
        out["check"] = "below-range"
        return out
    i = _first_admitted(chain, daas, slot, admits)
    if bound is None:
        if i >= len(chain):
            out["check"] = "pending"
            return out
        hs = chain[i][1]
        if rec.get("phase") == "voided" and rec.get("phaseDaa") != hs["daa"]:
            out["check"] = "void-elsewhere"
            return out
        out.update(anchor=chain[i][0], hs=hs, check="unbound" if rec.get("phase") == "voided" else "unbound-pending")
        return out
    acc = int(rec.get("acc") or 0)
    while i < len(chain):
        hs = chain[i][1]
        if hs["daa"] == bound:
            out.update(check="ok", anchor=chain[i][0], hs=hs)
            return out
        # Not bound at this anchor: only a NoCapablePanel retry (past the registry-resilience fence,
        # for a claim that never held a panel) re-bases it on this block and draws one delay on.
        if not (hs["daa"] < bound and base == acc and resilience_from is not None
                and hs["daa"] >= resilience_from and out["retries"] < max_retries):
            break
        out["retries"] += 1
        i = _first_admitted(chain, daas, hs["daa"] + anchor_delay, admits)
    # The slot rule did not lead to boundDaa. Past the operator-anchor stopgap, a bind at a DAA that
    # holds only attempts the stopgap does not admit is the consensus failure the stopgap forbids.
    stopgap = getattr(admits, "stopgap_from", None)
    if stopgap is not None and bound >= stopgap:
        j, at = bisect.bisect_left(daas, bound), []
        while j < len(chain) and daas[j] == bound:
            at.append(j)
            j += 1
        if not any(admits(chain[k][1]) for k in at):
            ext = next((k for k in at if chain[k][1]["algo"] in ATTEMPT_ALGOS), None)
            if ext is not None:
                out.update(check="ok", anchor=chain[ext][0], hs=chain[ext][1], breach=True)
                return out
    if i >= len(chain):
        out["check"] = "pending"
        return out
    out.update(check="mismatch", anchor=chain[i][0], hs=chain[i][1])
    return out


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
    return {"version": STATE_VERSION, "claims": {}, "bonds": {}, "checkpoints": [], "lists": {},
            "alerted": {"external_seat_bonds": [], "external_anchor": []},
            "runs": 0, "last_run": None, "last_tip": None, "network": {}}


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
    if int(disk.get("version") or 1) < STATE_VERSION:
        # v1 kept checkpoints under their bucket key rather than their block's DAA, and alerted
        # external seats per claim: neither carries over.
        st["checkpoints"] = []
        st["alerted"] = new_state()["alerted"]
        st["version"] = STATE_VERSION
    for k, v in new_state()["alerted"].items():
        st["alerted"].setdefault(k, v)
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


def prune_state(st, tip, retain_daa, now=None):
    """Drop claims the node no longer shows (retired, or out of its newest-500 lists) anchored (or
    accepted) more than retain_daa below the tip; drop alerted ids no record carries any more."""
    if not retain_daa or tip is None:
        return 0
    floor = tip - retain_daa
    old = [cid for cid, r in st["claims"].items()
           if (r.get("gone") or (now is not None and r.get("last") != now)) and (r.get("anchorDaa") or r.get("acc") or 0) < floor]
    for cid in old:
        del st["claims"][cid]
    st["alerted"]["external_anchor"] = [c for c in st["alerted"].get("external_anchor", []) if c in st["claims"]]
    return len(old)


# ------------------------------------------------------------------ collection
def scan_slot(rec, anchor_delay):
    return check_base(rec, anchor_delay) + anchor_delay


def _key_daa(rec, anchor_delay):
    if rec.get("bound") is not None:
        return rec["bound"]
    return rec.get("anchorDaa") or scan_slot(rec, anchor_delay)


def collect(rpc, st, cfg, now):
    """Read the node and fold what it says into `st`. Returns run facts (tip, errors, counts)."""
    run = {"errors": [], "degraded": [], "notes": [], "new_claims": 0, "newly_bound": 0, "calls": 0}
    status = rpc.call("getPalwNodeStatus")
    dag = rpc.call("getBlockDagInfo")
    genesis = status.get("genesisHash", "")
    if cfg.expect_genesis and genesis != cfg.expect_genesis:
        raise RpcError(f"wrong network: genesis {genesis[:16]}... is not testnet-12's {cfg.expect_genesis[:16]}...")
    tip = int(dag.get("virtualDaaScore") or 0)
    run.update(tip=tip, sink=dag.get("sink", ""), network=dag.get("network", ""), fp=status.get("consensusParamsId", ""),
               genesis=genesis, pruning=dag.get("pruningPointHash", ""))
    if cfg.expect_fp and run["fp"] != cfg.expect_fp:
        run["degraded"].append(f"consensus params fp {run['fp'][:16]} is not the shipped {cfg.expect_fp[:16]} (a fence armed? "
                               f"check the population rules, pass --fence-daa, then --expect-fp)")
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

    claim_rows, vesting_rows = {}, {}
    cover = {}                 # (bond, role) -> None (the whole list) or the acceptedDaa its newest rows reach down to
    truncated_lists = []

    def fetch_claims(bond, role):
        try:
            r = rpc.call("getPalwClaims", {"bond": bond, "role": role, "includeTerminal": True, "limit": 0})
        except RpcError as e:
            run["errors"].append(str(e)[:200])
            return
        rows_ = r.get("claims") or []
        key = f"{role}:{bond}"
        if r.get("truncated") and rows_:
            # The node returns a bond's newest 500 rows (by acceptedDaa). That is the normal case on
            # testnet-12 (claims stay 3,000 DAA after they end); it is a gap only when those rows no
            # longer reach back to what the last run read.
            cutoff = min(int(c.get("acceptedDaa") or 0) for c in rows_)
            cover[(bond, role)] = cutoff
            truncated_lists.append(f"{names.short(bond)} ({role}) from DAA {cutoff}")
            prev = st["lists"].get(key)
            if prev is not None and cutoff >= prev - cfg.recheck_daa:
                run["degraded"].append(f"getPalwClaims for {names.short(bond)} ({role}): the newest {len(rows_)} rows reach "
                                       f"back only to DAA {cutoff}, the last run read to DAA {prev}: claims in between may be missing")
        else:
            cover[(bond, role)] = None
        st["lists"][key] = tip
        if r.get("bondKnown") is not None:
            st["bonds"][bond] = {"known": bool(r.get("bondKnown")), "coll": int(r.get("bondCollateral") or 0),
                                 "slashed": int(r.get("bondSlashed") or 0), "reg": int(r.get("bondRegisteredDaa") or 0),
                                 "retiring": r.get("bondRetiringSinceDaa"), "classes": r.get("bondCapableClasses") or [],
                                 "seen": st["bonds"].get(bond, {}).get("seen", now)}
        for c in rows_:
            claim_rows[c["claimId"]] = c
        for c in r.get("vestingOnlyRows") or []:
            if c.get("claimId") in st["claims"]:
                vesting_rows.setdefault(c["claimId"], c)

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
    run["truncated_lists"] = truncated_lists

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
    for cid, c in claim_rows.items():
        rec = st["claims"].get(cid)
        if rec is None:
            rec = st["claims"][cid] = {"first": now}
            run["new_claims"] += 1
        new_bound = c.get("boundDaa")
        if c.get("seats") and rec.get("seats") and rec.get("bound") is not None and new_bound is not None \
                and int(new_bound) != rec["bound"]:
            # Redrawn and bound again: the first panel is its own piece of evidence.
            arch = dict(rec)
            arch.update(gone=True, archived=True)
            st["claims"][f"{cid}#{rec['bound']}"] = arch
            for f in ("anchor", "anchorDaa", "anchorBond", "anchorAlgo", "anchorCheck", "retries", "breach",
                      "pop", "popUncertain", "popLag", "verdicts", "full", "lic"):
                rec.pop(f, None)
        was_bound = bool(rec.get("seats"))
        rec.update(cls=c.get("classId"), exe=c.get("executorBond"), fp=bool(c.get("isFreePrompt")), phase=c.get("phase"),
                   void=c.get("voidReason") or "", acc=int(c.get("acceptedDaa") or 0), accBlk=(c.get("acceptedBlock") or "")[:32],
                   reb=c.get("reboundDaa"), bound=new_bound, phaseDaa=c.get("phaseDaa"), last=now, gone=False)
        if c.get("seats"):
            rec["seats"] = list(c["seats"])
            if not was_bound:
                run["newly_bound"] += 1
        asg = assignments.get(cid)
        if asg:
            rec["lic"] = asg.get("licensedState")
            rec["full"] = asg.get("fullSeat")
            rec["verdicts"] = {s["seatId"]: s.get("receiptStatus") for s in asg.get("seats") or []}
    for cid, c in vesting_rows.items():
        if cid in claim_rows:
            continue
        # A retired claim whose reward still vests: the row carries its outcome, not its bind facts.
        rec = st["claims"][cid]
        rec.update(phase="final", phaseDaa=c.get("phaseDaa"), last=now, gone=False)
    visible = set(claim_rows) | set(vesting_rows)
    for cid, rec in st["claims"].items():
        if cid in visible or rec.get("gone"):
            continue
        # Gone only when a list that would show it was read whole down to its acceptedDaa; a claim
        # that merely fell out of a newest-500 list is still live, just not refreshed this run.
        keys = [(rec.get("exe"), "executor")] + [(s, "seat") for s in rec.get("seats") or []]
        if any(k in cover and (cover[k] is None or int(rec.get("acc") or 0) > cover[k]) for k in keys):
            rec["gone"] = True    # retired from the node's state (or on a reorged-out branch)

    # The chain, from a checkpoint below the lowest slot still to resolve. Only claims seen this run
    # whose anchor is still open (or was resolved within --recheck-daa of the tip, for reorgs) set it.
    horizon = tip - cfg.max_scan_daa
    need, too_old = [], 0
    for cid in visible:
        r = st["claims"][cid]
        chk = r.get("anchorCheck")
        if chk in UNRESOLVABLE or (cid in vesting_rows and cid not in claim_rows):
            continue
        if chk in SETTLED and _key_daa(r, cfg.anchor_delay) < tip - cfg.recheck_daa:
            continue
        if chk not in SETTLED and scan_slot(r, cfg.anchor_delay) < horizon:
            r["anchorCheck"] = "too-old"
            too_old += bool(r.get("seats"))
            continue
        need.append(r)
    if too_old:
        (run["degraded"] if st.get("runs") else run["notes"]).append(
            f"{too_old} bound claim(s) with a slot more than --max-scan-daa {cfg.max_scan_daa} below the tip were left unresolved"
            + ("" if st.get("runs") else " (first run: the monitor starts from here)"))
    admits = anchor_rule(names, cfg.stopgap_from)
    run["chain_blocks"] = 0
    if need:
        min_slot = min(scan_slot(r, cfg.anchor_delay) for r in need)
        chain = fetch_chain(rpc, st, run, min_slot, cfg)
        daas = [h["daa"] for _, h in chain]
        for _, h in chain:
            if h["bond"] and h["bond"] not in st["bonds"]:
                st["bonds"].setdefault(h["bond"], {"known": None, "coll": 0, "reg": 0, "retiring": None, "classes": [], "seen": now})
        for rec in need:
            res = resolve_anchor(rec, chain, daas, cfg.anchor_delay, admits, cfg.resilience_from)
            chk = res["check"]
            if chk == "below-range" and rec.get("anchor"):
                continue                  # keep what an earlier run resolved
            if res["anchor"] is None:
                rec.update(anchor=None, anchorDaa=None, anchorBond=None, anchorAlgo=None, anchorCheck=chk)
                continue
            hs = res["hs"]
            rec.update(anchor=res["anchor"], anchorDaa=hs["daa"], anchorBond=hs["bond"], anchorAlgo=hs["algo"], anchorCheck=chk)
            if res["retries"]:
                rec["retries"] = res["retries"]
            if res["breach"]:
                rec["breach"] = True
        for rec in need:
            if not rec.get("seats"):
                continue
            chk = rec.get("anchorCheck")
            if chk == "pending":
                run.setdefault("bound_pending", 0)
                run["bound_pending"] += 1
            elif chk == "below-range" and run.get("chain_from") == "checkpoint":
                run.setdefault("bound_below", 0)
                run["bound_below"] += 1
    if run.get("bound_pending"):
        run["degraded"].append(f"{run['bound_pending']} bound claim(s) whose anchor block lies past the chain read: their panels are not tested")
    if run.get("bound_below"):
        run["degraded"].append(f"{run['bound_below']} bound claim(s) whose slot lies below the scan's checkpoint start")
    now_visible = [st["claims"][cid] for cid in visible]
    mism = [r for r in now_visible if r.get("anchorCheck") == "mismatch"]
    if mism:
        run["degraded"].append(f"{len(mism)} bound claim(s) whose slot rule does not lead to the first admitted attempt at their "
                               f"boundDaa (e.g. bound {mism[0].get('bound')}, slot rule reached DAA {mism[0].get('anchorDaa')}): "
                               f"the anchor rule the monitor models does not hold (fences armed? pass --fence-daa)")
    blind = [r for r in now_visible if r.get("seats") and r.get("anchorCheck") == "ok" and not r.get("anchorBond")]
    if blind:
        run["degraded"].append(f"{len(blind)} bound claim(s) whose anchor header's attempt envelope does not decode: "
                               f"the anchor producer is unknown and the panels are not tested")
    # Populations are modelled once, at the first run that sees the claim bound with its anchor.
    for rec in now_visible:
        if rec.get("seats") and rec.get("anchorCheck") == "ok" and "pop" not in rec and base_classes:
            rec["pop"], rec["popUncertain"] = population_for(rec, st, rows, base_classes, cfg, tip)
            rec["popLag"] = tip - int(rec.get("anchorDaa") or 0)
    run["calls"] = rpc.calls
    return run


def fetch_chain(rpc, st, run, min_slot, cfg):
    """[(hash, header summary)] of the selected chain from the best checkpoint below min_slot to
    the sink, in chain order. getVirtualChainFromBlock answers at most 10 x mergeset_size_limit
    (1,800 on testnet-12) added chain blocks per call, so it is paged until it answers nothing new."""
    starts = [(d, h) for d, h in sorted(st["checkpoints"], reverse=True) if d < min_slot]
    starts.append((None, run["pruning"] or run["genesis"]))
    chain_hashes, last_added, start_errors = None, [], []
    for d, start in starts:
        try:
            vc = rpc.call("getVirtualChainFromBlock", {"startHash": start, "includeAcceptedTransactionIds": False})
        except RpcError as e:
            start_errors.append(str(e)[:200])            # e.g. a checkpoint below the pruning point
            continue
        if vc.get("removedChainBlockHashes"):
            continue                                          # the checkpoint left the chain: try an older one
        last_added = list(vc.get("addedChainBlockHashes") or [])
        chain_hashes = [start] + last_added
        run["chain_from"] = "checkpoint" if d is not None else "pruning point"
        break
    if chain_hashes is None:
        run["errors"].extend(start_errors[-2:])
        run["degraded"].append("could not read the selected chain")
        return []
    pages, reached = 1, not last_added
    while not reached and pages < cfg.max_chain_pages:
        try:
            vc = rpc.call("getVirtualChainFromBlock", {"startHash": chain_hashes[-1], "includeAcceptedTransactionIds": False})
        except RpcError as e:
            run["errors"].append(str(e)[:200])
            break
        pages += 1
        removed = vc.get("removedChainBlockHashes") or []
        added = vc.get("addedChainBlockHashes") or []
        if removed:                                           # the chain moved under the read: drop the tail it left
            left = set(removed)
            while len(chain_hashes) > 1 and chain_hashes[-1] in left:
                chain_hashes.pop()
            if chain_hashes[-1] in left:
                run["degraded"].append("the selected chain reorganised below the scan's start during the read")
                return []
        elif not added:
            reached = True
            break
        chain_hashes.extend(added)
    run["chain_pages"] = pages
    if not reached:
        run["degraded"].append(f"the chain read stopped after {pages} getVirtualChainFromBlock page(s) ({len(chain_hashes)} blocks) "
                               f"short of the sink: later anchors are not resolved (raise --max-chain-pages?)")
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
    # Checkpoints: the first chain block of every `checkpoint_every` DAA, kept under its OWN DAA.
    cps = {d - d % cfg.checkpoint_every: (d, h) for d, h in st["checkpoints"]}
    fresh_cps = {}
    for x, hs in chain:
        fresh_cps.setdefault(hs["daa"] - hs["daa"] % cfg.checkpoint_every, (hs["daa"], x))
    cps.update(fresh_cps)                                     # today's chain replaces a reorged-out checkpoint
    st["checkpoints"] = sorted([d, h] for d, h in cps.values())[-cfg.keep_checkpoints:]
    run["chain_blocks"] = len(chain)
    return chain


def ready_at(row, daa):
    """Was this readiness row's bond ready at `daa`? True/False when the row's current proof decides
    it, None when the row was re-proved after `daa` (the earlier proof is not visible)."""
    if "readinessProvedDaa" not in row and "readinessExpiresDaa" not in row:
        return None
    proved = int(row.get("readinessProvedDaa") or 0)
    expires = int(row.get("readinessExpiresDaa") or 0)
    if proved == 0 and expires == 0:
        return False
    if proved <= daa < expires:
        return True
    if expires <= daa:
        return False
    return None


def population_for(rec, st, rows, base_classes, cfg, tip=None):
    """({bond: weight_msk} the claim's panel is modelled as drawn from (executor excluded), whether
    that population is uncertain).

    palw_bond_may_judge_class_v4: a non-base class seats the bonds with a fresh readiness row at
    the anchor (getPalwPanelSeats readinessProvedDaa..readinessExpiresDaa); the base class seats
    the bonds that declared it. Both need an Active bond at the panel floor, and past the maturity
    fence a registration older than the window (genesis bonds are registered at 0). A readiness row
    re-proved after the anchor, or one read long after it without proof dates, makes the
    population uncertain; the model notes leave such claims out."""
    cls = rec["cls"]
    names = cfg.names
    anchor_daa = rec.get("anchorDaa") or 0
    lag = (tip - anchor_daa) if tip is not None else 0
    out, uncertain = {}, False
    for bond, info in st["bonds"].items():
        if bond == rec.get("exe") or not info.get("known") or info.get("retiring") is not None:
            continue
        coll_msk = int(info.get("coll") or 0) // SOMPI_PER_MSK
        if coll_msk < cfg.seat_floor_msk:
            continue
        if cls not in base_classes:
            row = rows.get(cls, {}).get(bond)
            if row is None:
                continue
            ready = ready_at(row, anchor_daa)
            if ready is None:
                ready = bool(row.get("ready"))
                uncertain = uncertain or "readinessProvedDaa" in row or lag > cfg.pop_fresh_daa
            if not ready:
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
    return out, uncertain


def draw_weight(coll_msk, cap):
    return max(1, min(int(coll_msk), int(cap)))


# ------------------------------------------------------------------ analysis
def relative_tests(used, stratum_daa, min_expected):
    """Each anchor producer against the other anchor producers: strata are (class, executor, panel
    size, anchor DAA // stratum_daa); only strata where some other producer anchored too carry
    information. Returns (anchors, cells, omnibus)."""
    strata = {}
    for _cid, r in used:
        seats = r["seats"]
        key = (r.get("cls"), r.get("exe"), len(seats), int(r.get("anchorDaa") or 0) // max(1, stratum_daa))
        s = strata.get(key)
        if s is None:
            s = strata[key] = {"N": 0, "T": collections.Counter(), "P": collections.Counter(), "A": {}}
        s["N"] += 1
        s["T"].update(seats)
        for b in seats:
            for c in seats:
                s["P"][(b, c)] += 1
        a = s["A"].get(r["anchorBond"])
        if a is None:
            a = s["A"][r["anchorBond"]] = [0, collections.Counter()]
        a[0] += 1
        a[1].update(seats)
    anchors = sorted({A for s in strata.values() for A in s["A"]})
    cells, omnibus = [], []
    for A in anchors:
        mine = [s for s in strata.values() if A in s["A"]]
        comp = [s for s in mine if s["A"][A][0] < s["N"]]
        n_all = sum(s["A"][A][0] for s in mine)
        n_comp = sum(s["A"][A][0] for s in comp)
        rest = sum(s["N"] - s["A"][A][0] for s in comp)
        bonds = sorted({b for s in comp for b in s["T"]})
        E = {b: 0.0 for b in bonds}
        R = {b: 0.0 for b in bonds}
        O = {b: 0 for b in bonds}
        cov = {b: {c: 0.0 for c in bonds} for b in bonds}
        for s in comp:
            N, n = s["N"], s["A"][A][0]
            f = n * (N - n) / (N - 1)
            items = list(s["T"].items())
            for b, t in items:
                E[b] += n * t / N
                R[b] += (N - n) * t / N
                O[b] += s["A"][A][1][b]
                for c, u in items:
                    cov[b][c] += f * (s["P"][(b, c)] / N - t * u / (N * N))
        for b in bonds:
            if cov[b][b] <= 1e-12:
                continue
            p2, pu, pl, _, _ = stratified_hypergeom_test([(s["N"], s["T"][b], s["A"][A][0]) for s in comp], O[b])
            cells.append({"anchor": A, "seat": b, "n": n_comp, "obs": O[b], "exp": E[b], "var": cov[b][b],
                          "p": p2, "p_upper": pu, "p_lower": pl})
        keep = [b for b in bonds if cov[b][b] > 1e-12 and min(E[b], R[b], n_comp - E[b], rest - R[b]) >= min_expected]
        row = {"anchor": A, "claims": n_all, "comparable": n_comp, "bonds": len(keep), "evaluated": False}
        if not comp:
            row["reason"] = "no stratum shared with another anchor producer"
        elif len(keep) < 2:
            row["reason"] = f"too few comparable claims ({n_comp}; expected counts < {min_expected:g})"
        else:
            q, rank = pinv_quadratic([O[b] - E[b] for b in keep], [[cov[b][c] for c in keep] for b in keep])
            if rank < 1:
                row["reason"] = "no variation"
            else:
                row.update(evaluated=True, q=q, df=rank, p=chi2_sf(q, rank))
        omnibus.append(row)
    return anchors, cells, omnibus


def model_tests(used, min_expected, probs, exact_max):
    """Each anchor producer (and all of them pooled, anchor "*") against the stake-weighted model of
    each claim's population: exact Poisson-binomial cells, Rao-Scott omnibus. Notes, not alerts."""
    per = collections.defaultdict(lambda: {"claims": 0, "obs": collections.Counter(), "ps": collections.defaultdict(list),
                                           "cov": collections.defaultdict(float)})
    for _cid, r in used:
        if not r.get("pop") or r.get("popUncertain"):
            continue
        pop = dict(r["pop"])
        pop.pop(r.get("exe"), None)
        for s in r["seats"]:
            pop.setdefault(s, 1)
        bonds, pi, pij = probs(pop, len(r["seats"]))
        for A in (r["anchorBond"], "*"):
            cell = per[A]
            cell["claims"] += 1
            cell["obs"].update(r["seats"])
            for b in bonds:
                cell["ps"][b].append(pi[b])
                for c in bonds:
                    cell["cov"][(b, c)] += pij[(b, c)] - pi[b] * pi[c]
    cells, omnibus = [], []
    for A in sorted(a for a in per if a != "*") + (["*"] if "*" in per else []):
        P = per[A]
        for b in sorted(P["ps"]):
            ps = P["ps"][b]
            p2, pu, pl = poisson_binomial_test(ps, P["obs"][b], exact_max)
            cells.append({"anchor": A, "seat": b, "n": len(ps), "obs": P["obs"][b], "exp": sum(ps), "p": p2, "p_upper": pu, "p_lower": pl})
        bonds = sorted(P["ps"])
        exp = [sum(P["ps"][b]) for b in bonds]
        obs = [P["obs"][b] for b in bonds]
        keep = [i for i, e in enumerate(exp) if e > 1e-12]
        row = {"anchor": A, "claims": P["claims"], "bonds": len(keep), "min_exp": min((exp[i] for i in keep), default=0.0), "evaluated": False}
        if len(keep) < 2 or row["min_exp"] < min_expected:
            row["reason"] = f"min expected {row['min_exp']:.1f} < {min_expected:g}" if len(keep) >= 2 else "fewer than 2 bonds"
        else:
            cov = [[P["cov"][(bonds[i], bonds[j])] for j in keep] for i in keep]
            x2, g, h, p = rao_scott_chi2([obs[i] for i in keep], [exp[i] for i in keep], cov)
            row.update(evaluated=True, x2=x2, g=g, h=h, p=p)
        omnibus.append(row)
    return cells, omnibus


def analyse(records, names, alpha=1e-4, min_expected=5.0, windows=(), tip=None, stratum_daa=25, exact_max=2000):
    """The seat draws per anchor producer against the other anchor producers (relative: the
    alerts) and against the stake-weighted model (notes), on all recorded claims and on each recent
    window. `records` are the state's claim records; returns a dict the report and the summary read."""
    cache = {}

    def probs(pop, k):
        key = (tuple(sorted(pop.items())), k)
        if key not in cache:
            bonds = [b for b, _ in key[0]]
            pi, pij = inclusion_probabilities([w for _, w in key[0]], k)
            cache[key] = (bonds, {b: pi[i] for i, b in enumerate(bonds)},
                          {(bonds[i], bonds[j]): pij[i][j] for i in range(len(bonds)) for j in range(len(bonds))})
        return cache[key]

    used_all = sorted(((cid, r) for cid, r in records.items()
                       if r.get("seats") and r.get("anchorCheck") == "ok" and r.get("anchorBond")), key=lambda kv: kv[0])
    ref = tip if tip is not None else max((int(r.get("anchorDaa") or 0) for _, r in used_all), default=0)
    out = []
    for w in [None] + sorted({int(x) for x in windows if x and int(x) > 0}, reverse=True):
        used = used_all if w is None else [(c, r) for c, r in used_all if int(r.get("anchorDaa") or 0) > ref - w]
        if w is not None and (len(used) == len(out[-1]["_n"]) or not used):
            continue                  # the window holds nothing the wider one did not: no second test of it
        anchors, rel_cells, rel_omni = relative_tests(used, stratum_daa, min_expected)
        # With exactly two producers sharing strata, B against A is A against B turned over: one
        # hypothesis, reported under both names but counted once in the corrections.
        active = [o["anchor"] for o in rel_omni if o["comparable"] > 0]
        twin = {active[0]: active[1], active[1]: active[0]} if len(active) == 2 else {}
        for row in rel_cells + rel_omni:
            row["mirror"] = bool(twin) and row["anchor"] == active[1]
        mod_cells, mod_omni = model_tests(used, min_expected, probs, exact_max)
        out.append({"name": "all" if w is None else f"last {w}", "from": None if w is None else ref - w, "claims": len(used),
                    "anchors": anchors, "rel_cells": rel_cells, "rel_omnibus": rel_omni, "twin": twin,
                    "model_cells": mod_cells, "model_omnibus": mod_omni, "_n": used})
    rel = [c for w in out for c in w["rel_cells"] if not c["mirror"]]
    for c, rej in zip(rel, holm([c["p"] for c in rel], alpha)):
        c["significant"] = rej
    omni = [o for w in out for o in w["rel_omnibus"] if o["evaluated"] and not o["mirror"]]
    for w in out:
        for o in w["rel_omnibus"]:
            if not o["mirror"]:
                o["significant"] = bool(o["evaluated"] and o["p"] <= alpha / max(1, len(omni)))
        orig = {(c["anchor"], c["seat"]): c for c in w["rel_cells"] if not c["mirror"]}
        orig_o = {o["anchor"]: o for o in w["rel_omnibus"] if not o["mirror"]}
        for c in w["rel_cells"]:
            if c["mirror"]:
                c["significant"] = bool(orig.get((w["twin"][c["anchor"]], c["seat"]), {}).get("significant"))
        for o in w["rel_omnibus"]:
            if o["mirror"]:
                o["significant"] = bool(orig_o.get(w["twin"][o["anchor"]], {}).get("significant"))
    mcells = [c for w in out for c in w["model_cells"]]
    for c, rej in zip(mcells, holm([c["p"] for c in mcells], alpha)):
        c["significant"] = rej
    momni = [o for w in out for o in w["model_omnibus"] if o["evaluated"]]
    for w in out:
        for o in w["model_omnibus"]:
            o["significant"] = bool(o["evaluated"] and o["p"] <= alpha / max(1, len(momni)))
    for w in out:
        del w["_n"]
    hosts = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0.0]))
    for c in (out[0]["model_cells"] if out else []):
        slot = hosts[c["anchor"]][names.host(c["seat"])]
        slot[0] += c["obs"]
        slot[1] += c["exp"]
    first = out[0] if out else {"anchors": [], "rel_omnibus": []}
    return {"claims_used": len(used_all), "windows": out, "anchors": first["anchors"], "ref": ref,
            "comparable": sum(o["comparable"] for o in first["rel_omnibus"]),
            "hosts": {a: {h: tuple(v) for h, v in hs.items()} for a, hs in hosts.items()},
            "alpha": alpha, "cells_tested": len(rel), "omnibus_tested": len(omni), "stratum_daa": stratum_daa}


def bias_findings(res):
    """Significant relative cells, one per (anchor, seat) at its smallest p over the windows, each
    with the same window's model cell; significant relative omnibus rows."""
    best = {}
    for w in res["windows"]:
        model = {(c["anchor"], c["seat"]): c for c in w["model_cells"]}
        for c in w["rel_cells"]:
            if c.get("significant"):
                key = (c["anchor"], c["seat"])
                if key not in best or c["p"] < best[key][0]["p"]:
                    best[key] = (c, w["name"], model.get(key))
    cells = sorted(best.values(), key=lambda t: ((t[2] or {}).get("p", 1.0), t[0]["p"]))
    omni = {}
    for w in res["windows"]:
        for o in w["rel_omnibus"]:
            if o.get("significant") and (o["anchor"] not in omni or o["p"] < omni[o["anchor"]][0]["p"]):
                omni[o["anchor"]] = (o, w["name"])
    return cells, sorted(omni.values(), key=lambda t: t[0]["p"])


def find_external(records, names, alerted, realert, stopgap_from=None):
    """Non-genesis bonds seated (alerted once per bond) and claims anchored by a non-genesis
    producer (alerted, once per claim, only at or past the stopgap height)."""
    seat_claims = collections.defaultdict(list)
    pre, post = [], []
    for cid, r in records.items():
        for s in r.get("seats") or []:
            if not names.is_genesis(s):
                seat_claims[s].append(cid)
        if r.get("anchorBond") and r.get("anchorCheck") in ("ok", "unbound") and not names.is_genesis(r["anchorBond"]):
            if stopgap_from is not None and int(r.get("anchorDaa") or 0) >= stopgap_from:
                post.append(cid)
            else:
                pre.append(cid)
    known = set(alerted.get("external_seat_bonds", []))
    done = set(alerted.get("external_anchor", []))
    return {"seat_claims": dict(seat_claims),
            "new_bonds": sorted(b for b in seat_claims if realert or b not in known),
            "pre": sorted(pre), "post": sorted(post),
            "new_post": sorted(c for c in post if realert or c not in done)}


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
    if r.get("archived"):
        outcome += " [first panel, redrawn]"
    anchor = f"{(r.get('anchor') or '')[:12]}@{r.get('anchorDaa')}" if r.get("anchor") else (r.get("anchorCheck") or "-")
    if r.get("anchorCheck") == "mismatch":
        anchor += " MISMATCH"
    if r.get("retries"):
        anchor += f" r{r['retries']}"
    if r.get("breach"):
        anchor += " BREACH"
    return (f"{cid[:16]}  {class_label(r.get('cls')):<5}  {names.short(r.get('exe')):<5} {r.get('acc', ''):>5}  "
            f"{anchor:<20} {names.short(r.get('anchorBond')) if r.get('anchorBond') else '-':<5}  "
            f"{' '.join(seats) if seats else '-':<28}  {outcome}")


def _table(w, rows, seats, names, head):
    w(f"{'anchor':<18}{head:>12}  " + "".join(f"{names.short(b):>11}" for b in seats) + "\n")
    for label, n, cells in rows:
        w(f"{label:<18}{n:>12}  " + "".join(f"{cells.get(b, '-'):>11}" for b in seats) + "\n")


def report(st, run, res, alerts, ext, cfg, names, out):
    w = out.write
    ts = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).strftime("%Y-%m-%d %H:%M:%S JST")
    recs = st["claims"]
    phases = collections.Counter(r.get("phase") for r in recs.values())
    w(f"testnet-12 panel-bias monitor - {ts}\n")
    if cfg.offline:
        w(f"offline analysis of {cfg.state}\n")
    elif run.get("tip") is not None:
        w(f"endpoint {cfg.url} | network {run.get('network')} | fp {run.get('fp', '')[:12]} | genesis {run.get('genesis', '')[:12]} | "
          f"tip DAA {run.get('tip')} | sink {run.get('sink', '')[:12]} | {run.get('calls')} read calls | "
          f"chain {run.get('chain_blocks', 0)} blocks in {run.get('chain_pages', 0)} page(s) from the {run.get('chain_from', '-')}\n")
    fences = []
    if cfg.stopgap_from is not None:
        fences.append(f"operator-anchor stopgap from DAA {cfg.stopgap_from}")
    if cfg.resilience_from is not None:
        fences.append(f"registry resilience from DAA {cfg.resilience_from}")
    w(f"fences modelled: {', '.join(fences) if fences else 'none (the shipped t12 rules)'}\n")
    checks = collections.Counter(r.get("anchorCheck") for r in recs.values() if r.get("seats"))
    w(f"records: {len(recs)} claims ({', '.join(f'{k} {v}' for k, v in sorted(phases.items(), key=lambda kv: str(kv[0])))}); "
      f"bound {sum(1 for r in recs.values() if r.get('seats'))}, anchors {dict(checks)}; "
      f"this run +{run.get('new_claims', 0) if run else 0} new, +{run.get('newly_bound', 0) if run else 0} newly bound\n")
    if run.get("truncated_lists"):
        w(f"claim lists capped at the newest 500 rows: {len(run['truncated_lists'])} (normal on testnet-12; a gap would be DEGRADED)\n")
    w("\n")

    anchored = [(cid, r) for cid, r in recs.items() if r.get("seats") or r.get("anchorCheck") == "unbound"]
    newest = sorted(anchored, key=lambda kv: (kv[1].get("anchorDaa") or kv[1].get("bound") or 0, kv[1].get("acc") or 0, kv[0]),
                    reverse=True)[:cfg.show]
    w(f"Recent panels (newest {len(newest)} of {len(anchored)} anchored claims; seat verdict V valid . pending - none ? unknown; "
      f"* full seat; rN = N NoCapablePanel retries)\n")
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

    wins = res["windows"]
    first = wins[0] if wins else {"rel_cells": [], "rel_omnibus": [], "model_cells": [], "anchors": []}
    sig_any = {(c["anchor"], c["seat"]) for win in wins for c in win["rel_cells"] if c.get("significant")}
    seats = sorted({c["seat"] for c in first["rel_cells"]} | {c["seat"] for c in first["model_cells"]},
                   key=lambda b: (not names.is_genesis(b), names.short(b)))
    w(f"\nSeat draws per anchor producer against the OTHER anchor producers (all {res['claims_used']} tested claims; "
      f"strata: class x executor x {res['stratum_daa']}-DAA slice; ! = significant in some window)\n")
    rows = []
    for o in first["rel_omnibus"]:
        cells = {c["seat"]: f"{c['obs']}/{c['exp']:.1f}{'!' if (c['anchor'], c['seat']) in sig_any else ''}"
                 for c in first["rel_cells"] if c["anchor"] == o["anchor"]}
        rows.append((f"{names.short(o['anchor'])} {names.host(o['anchor'])}", f"{o['comparable']}/{o['claims']}", cells))
    _table(w, rows, seats, names, "comp/claims")
    w("\nSeat draws against the stake-weighted model (population notes, not alerts; ~ = significant)\n")
    rows = []
    for o in first.get("model_omnibus", []):
        cells = {c["seat"]: f"{c['obs']}/{c['exp']:.1f}{'~' if c.get('significant') else ''}"
                 for c in first["model_cells"] if c["anchor"] == o["anchor"]}
        label = "all (pooled)" if o["anchor"] == "*" else f"{names.short(o['anchor'])} {names.host(o['anchor'])}"
        rows.append((label, str(o["claims"]), cells))
    _table(w, rows, seats, names, "claims")
    w("\nBy host, against the model: observed/expected seats\n")
    hosts = sorted({h for hs in res["hosts"].values() for h in hs})
    w(f"{'anchor':<18}" + "".join(f"{h:>14}" for h in hosts) + "\n")
    for A in sorted(a for a in res["hosts"] if a != "*") + (["*"] if "*" in res["hosts"] else []):
        label = "all (pooled)" if A == "*" else f"{names.short(A)} {names.host(A)}"
        hs = res["hosts"].get(A, {})
        w(f"{label:<18}" + "".join(f"{(str(hs[h][0]) + '/' + format(hs[h][1], '.1f')) if h in hs else '-':>14}" for h in hosts) + "\n")

    w(f"\nTests (H0: the draw does not depend on the anchor producer; family-wise alpha {res['alpha']:g} for the cells and "
      f"for the omnibus, Holm/Bonferroni over {res['cells_tested']} cells and {res['omnibus_tested']} omnibus rows in {len(wins)} window(s))\n")
    for win in wins:
        cells = win["rel_cells"]
        low = min(cells, key=lambda c: c["p"]) if cells else None
        w(f"  window {win['name']} ({win['claims']} claims"
          + (f", anchored after DAA {win['from']}" if win["from"] is not None else "") + "): "
          + (f"{len(cells)} cells, {sum(1 for c in cells if c.get('significant'))} significant, smallest p {fmt_p(low['p'])} "
             f"({names.short(low['anchor'])} -> {names.short(low['seat'])} {low['obs']}/{low['exp']:.1f})" if low else "no comparable claims")
          + "\n")
        for o in win["rel_omnibus"]:
            if o["evaluated"]:
                w(f"    omnibus {names.short(o['anchor'])}: CMH Q {o['q']:.2f} df {o['df']} p {fmt_p(o['p'])} over {o['comparable']} "
                  f"comparable claims{' SIGNIFICANT' if o['significant'] else ''}\n")
            else:
                w(f"    omnibus {names.short(o['anchor'])}: not evaluated ({o['reason']})\n")
        for c in win["model_cells"]:
            if c.get("significant"):
                who = "all anchors" if c["anchor"] == "*" else f"{names.short(c['anchor'])}'s panels"
                w(f"    population note: {names.short(c['seat'])} seated {c['obs']}/{c['exp']:.1f} on {who} against the model "
                  f"(p {fmt_p(c['p'])}): eligibility, saturation, readiness or a model gap, not by itself an anchor bias\n")
    w(f"\nExternal bonds: {len(ext['seat_claims'])} seated ("
      + ", ".join(f"{names.short(b)} on {len(v)}" for b, v in sorted(ext["seat_claims"].items())) + "); "
      f"external anchors {len(ext['pre'])} before the stopgap, {len(ext['post'])} past it\n")

    w("\nAlerts\n")
    if not alerts and not (run or {}).get("degraded") and not (run or {}).get("errors"):
        w("  none\n")
    for a in alerts:
        w(f"  ALERT {a}\n")
    for d in (run or {}).get("degraded", []):
        w(f"  DEGRADED {d}\n")
    for e in (run or {}).get("errors", [])[:10]:
        w(f"  ERROR {e}\n")
    for n in (run or {}).get("notes", []):
        w(f"  note {n}\n")


# ------------------------------------------------------------------ main
def _windows(s):
    return [int(x) for x in str(s).replace(" ", "").split(",") if x and int(x) > 0]


def parse_args(argv):
    ap = argparse.ArgumentParser(description="read-only testnet-12 panel-seating monitor (see the module doc)")
    ap.add_argument("--url", default="wss://misakascan.com/kaspa", help="JSON wRPC endpoint (ws:// or wss://)")
    ap.add_argument("--state", default=os.path.expanduser("~/.t12-panel-bias/state.json"),
                    help="state file (claim records, checkpoints, alerted ids); '' = none")
    ap.add_argument("--offline", action="store_true", help="analyse the state file only; no RPC")
    ap.add_argument("--alpha", type=float, default=1e-4, help="family-wise false-alarm rate of each test family per run")
    ap.add_argument("--min-expected", type=float, default=5.0, help="an omnibus seat needs expected counts >= this")
    ap.add_argument("--windows", type=_windows, default=[50, 100, 200, 400, 800],
                    help="recent windows (DAA, comma-separated) tested besides all recorded claims ('' = none)")
    ap.add_argument("--stratum-daa", type=int, default=25, help="anchor-DAA slice the relative tests compare producers within")
    ap.add_argument("--fence-daa", type=int, default=None,
                    help="the common post-launch fence height once armed: sets --stopgap-from and --resilience-from")
    ap.add_argument("--stopgap-from", type=int, default=None, help="operator-anchor stopgap (lane A) height, if armed")
    ap.add_argument("--resilience-from", type=int, default=None, help="registry-resilience (NoCapablePanel retry) height, if armed")
    ap.add_argument("--anchor-delay", type=int, default=ANCHOR_DELAY)
    ap.add_argument("--weight-cap-msk", type=int, default=WEIGHT_CAP_MSK)
    ap.add_argument("--seat-floor-msk", type=int, default=SEAT_FLOOR_MSK)
    ap.add_argument("--maturity-activation", type=int, default=MATURITY_ACTIVATION)
    ap.add_argument("--maturity-window", type=int, default=MATURITY_WINDOW)
    ap.add_argument("--pop-fresh-daa", type=int, default=24,
                    help="a readiness row read more than this after the anchor without proof dates makes the population uncertain")
    ap.add_argument("--genesis-txid", default=T12_PREMINE, help="the premine txid whose outputs are the genesis bonds")
    ap.add_argument("--roster", default="", help="JSON {index: {host, name}} overriding the genesis-bond names")
    ap.add_argument("--expect-genesis", default=T12_GENESIS, help="refuse another network ('' = any)")
    ap.add_argument("--expect-fp", default=T12_FP, help="note a different consensus params fp ('' = do not check)")
    ap.add_argument("--realert", action="store_true", help="repeat external-seat/anchor alerts already raised")
    ap.add_argument("--show", type=int, default=25, help="recent panels listed in the report")
    ap.add_argument("--retain-daa", type=int, default=20_000,
                    help="forget claims the node no longer shows anchored more than this many DAA below the tip (0 = keep all)")
    ap.add_argument("--recheck-daa", type=int, default=30, help="re-resolve anchors this close to the tip (reorgs)")
    ap.add_argument("--max-scan-daa", type=int, default=2_000, help="never read the chain further back than this below the tip")
    ap.add_argument("--json", default="", help="also write the full result (records + analysis) to this file")
    ap.add_argument("--quiet", action="store_true", help="print only the machine summary line")
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--checkpoint-every", type=int, default=25)
    ap.add_argument("--keep-checkpoints", type=int, default=400)
    ap.add_argument("--max-chain-pages", type=int, default=60, help="getVirtualChainFromBlock pages (1,800 chain blocks each)")
    ap.add_argument("--max-block-pages", type=int, default=400)
    ap.add_argument("--max-block-calls", type=int, default=300)
    ap.add_argument("--max-assignment-calls", type=int, default=200)
    cfg = ap.parse_args(argv)
    if cfg.stopgap_from is None:
        cfg.stopgap_from = cfg.fence_daa
    if cfg.resilience_from is None:
        cfg.resilience_from = cfg.fence_daa
    return cfg


def main(argv=None):
    cfg = parse_args(argv)
    cfg.names = Names(cfg.genesis_txid, load_roster(cfg.roster))
    names = cfg.names
    now = int(time.time())
    st = load_state(cfg.state)
    run = {"errors": [], "degraded": [], "notes": []}
    fatal = None
    if not cfg.offline:
        rpc = WsRpc(cfg.url, cfg.timeout)
        try:
            run = collect(rpc, st, cfg, now)
        except Exception as e:                      # noqa: BLE001 - an unreadable node is DEGRADED, never a crash
            fatal = f"{type(e).__name__}: {e}"[:300]
            run = {"errors": [fatal], "degraded": [], "notes": [], "calls": rpc.calls}
        finally:
            rpc.close()
    records = st["claims"]
    res = analyse(records, names, cfg.alpha, cfg.min_expected, cfg.windows, run.get("tip"), cfg.stratum_daa)
    ext = find_external(records, names, st["alerted"], cfg.realert, cfg.stopgap_from)

    alerts = []
    cells, omni = bias_findings(res)
    for c, win, m in cells:
        model = f"; against the model {m['obs']}/{m['exp']:.1f} p {fmt_p(m['p'])}" if m else "; no model"
        alerts.append(f"bias: {names.short(c['anchor'])} seats {names.short(c['seat'])} {'OVER' if c['obs'] > c['exp'] else 'UNDER'} "
                      f"the other anchor producers ({c['obs']}/{c['exp']:.1f} over {c['n']} comparable claims, p {fmt_p(c['p'])}, "
                      f"window {win}){model}")
    for o, win in omni:
        alerts.append(f"bias: {names.short(o['anchor'])}'s panels differ from the other anchor producers' "
                      f"(CMH Q {o['q']:.1f} df {o['df']} p {fmt_p(o['p'])}, {o['comparable']} comparable claims, window {win})")
    for b in ext["new_bonds"]:
        first = min(ext["seat_claims"][b], key=lambda cid: (records[cid].get("anchorDaa") or 0, cid))
        r = records[first]
        alerts.append(f"external seat: {names.long(b)} is seated for the first time (claim {first[:16]}, {class_label(r.get('cls'))}, "
                      f"anchor by {names.short(r.get('anchorBond'))} at DAA {r.get('anchorDaa')}; {len(ext['seat_claims'][b])} panel(s) so far)")
    for cid in ext["new_post"]:
        r = records[cid]
        alerts.append(f"external anchor past the stopgap: claim {cid[:16]} ({class_label(r.get('cls'))}) anchored at DAA {r.get('anchorDaa')} "
                      f"by {names.long(r.get('anchorBond'))}{' (bound in a block the stopgap does not admit)' if r.get('breach') else ''}")
    if not cfg.offline and fatal is None:
        st["alerted"]["external_seat_bonds"] = sorted(set(st["alerted"].get("external_seat_bonds", [])) | set(ext["seat_claims"]))
        st["alerted"]["external_anchor"] = sorted(set(st["alerted"].get("external_anchor", [])) | set(ext["post"]))
        st["runs"] = st.get("runs", 0) + 1
        st["last_run"] = now
        st["last_tip"] = run.get("tip")
        run["pruned"] = prune_state(st, run.get("tip"), cfg.retain_daa, now)
        save_state(cfg.state, st, cfg.genesis_txid)

    degraded = bool(fatal or run.get("degraded") or run.get("errors"))
    status = "ALERT" if alerts else ("DEGRADED" if degraded else "OK")
    tested = [r for r in records.values() if r.get("seats") and r.get("anchorCheck") == "ok" and r.get("anchorBond")]
    by_anchor = collections.Counter(names.short(r["anchorBond"]) for r in tested)
    rel = [c for w in res["windows"] for c in w["rel_cells"]]
    min_cell = min((c["p"] for c in rel), default=None)
    min_omni = min((o["p"] for w in res["windows"] for o in w["rel_omnibus"] if o["evaluated"]), default=None)
    unresolved = collections.Counter(r.get("anchorCheck") for r in records.values()
                                     if r.get("seats") and r.get("anchorCheck") in ("pending", "below-range", "too-old", "mismatch", None))
    summary = {"status": status, "time": now, "tip": run.get("tip"), "fp": (run.get("fp") or "")[:16],
               "claims": len(records), "bound": sum(1 for r in records.values() if r.get("seats")),
               "tested": res["claims_used"], "comparable": res["comparable"], "anchors": dict(sorted(by_anchor.items())),
               "windows": [w["name"] for w in res["windows"]],
               "ext_seat": sum(len(v) for v in ext["seat_claims"].values()), "ext_seat_bonds": len(ext["seat_claims"]),
               "ext_anchor": len(ext["pre"]) + len(ext["post"]), "ext_anchor_post_stopgap": len(ext["post"]),
               "bias_cells": len(cells), "min_cell_p": None if min_cell is None else float(f"{min_cell:.3g}"),
               "min_omnibus_p": None if min_omni is None else float(f"{min_omni:.3g}"),
               "model_notes": sum(1 for w in res["windows"] for c in w["model_cells"] if c.get("significant")),
               "unresolved": {str(k): v for k, v in sorted(unresolved.items(), key=lambda kv: str(kv[0]))},
               "alpha": cfg.alpha, "alerts": alerts, "degraded": (run.get("degraded") or []) + (run.get("errors") or [])[:5]}
    if not cfg.quiet:
        report(st, run, res, alerts, ext, cfg, names, sys.stdout)
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
