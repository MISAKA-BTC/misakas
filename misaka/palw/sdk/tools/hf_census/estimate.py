"""**The census's estimators** (no network): exact counts over the listing-decided repositories, Horvitz–Thompson over the
stratified header sample (phase 1), and a two-phase estimate for the shape depth (phase 2: a random prefix of the eligible
repositories, `select_shape.py`). One-sided 95 % lower bounds by Korn–Graubard.

Phase 1 (`lower` and everything the headers decide) covers every sampled repository; a sampled repository without a row is a
failure (nonresponse is never dropped). Phase 2 (`admit` at the shape depth) covers the longest prefix of the eligible repositories'
random order that is fully judged: within a design stratum h with L_h eligible sampled repositories and m_h of them in the prefix,
each prefix repository stands for (N_h / n_h)(L_h / m_h) repositories of the population. The variance is the two-phase
(double-sampling) sum: the phase-1 term over the stratum's sample with each eligible repository's value imputed by its stratum's
prefix mean, plus the phase-2 term (N_h / n_h)² L_h² (1 − m_h / L_h) s²_h / m_h. An eligible stratum with no judged repository
contributes 0 and is counted in `unjudged_eligible` (conservative).
"""

from __future__ import annotations

import hashlib
import math
from collections import defaultdict

Z95 = 1.6448536269514722


# ---- the incomplete beta and the Clopper–Pearson / Korn–Graubard bounds ------------------------------------------------------------
def _betacf(a: float, b: float, x: float) -> float:
    tiny = 1e-300
    qab, qap, qam = a + b, a + 1.0, a - 1.0
    c, d = 1.0, 1.0 - qab * x / qap
    d = 1.0 / (d if abs(d) > tiny else tiny)
    h = d
    for m in range(1, 400):
        m2 = 2 * m
        aa = m * (b - m) * x / ((qam + m2) * (a + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c
        c = c if abs(c) > tiny else tiny
        h *= d * c
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c
        c = c if abs(c) > tiny else tiny
        de = d * c
        h *= de
        if abs(de - 1.0) < 3e-14:
            break
    return h


def betainc(a: float, b: float, x: float) -> float:
    if x <= 0:
        return 0.0
    if x >= 1:
        return 1.0
    lbt = math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b) + a * math.log(x) + b * math.log1p(-x)
    if x < (a + 1) / (a + b + 2):
        return math.exp(lbt) * _betacf(a, b, x) / a
    return 1.0 - math.exp(lbt) * _betacf(b, a, 1.0 - x) / b


def beta_ppf(q: float, a: float, b: float) -> float:
    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if betainc(a, b, mid) < q:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def cp_lb(x: float, n: float, alpha: float = 0.05) -> float:
    if n <= 0 or x <= 0:
        return 0.0
    if x >= n:
        return alpha ** (1.0 / n)
    return beta_ppf(alpha, x, n - x + 1)


def kg_lb(p: float, v: float, n_nominal: float) -> float:
    """Korn–Graubard: Clopper–Pearson at the effective sample size p(1−p)/v (the nominal size when v = 0)."""
    if p <= 0:
        return 0.0
    n_eff = n_nominal if v <= 0 else p * (1 - p) / v
    return cp_lb(p * n_eff, n_eff)


def shape_key(seed: str, repo: str) -> str:
    return hashlib.blake2b(f"{seed}\0shape\0{repo}".encode(), digest_size=8).hexdigest()


# ---- the estimator ----------------------------------------------------------------------------------------------------------------
class TwoPhase:
    """A population total Σ y: exact units, phase-1 sampled units (y known), and phase-2 units (eligible; y known for the judged
    prefix only). Strata are the sample design's (`certainty` has N = n)."""

    def __init__(self, design: dict):
        self.Nh = {h: s["N"] for h, s in design["strata"].items()}
        self.nh = {h: s["n"] for h, s in design["strata"].items()}
        self.Nh["certainty"] = self.nh["certainty"] = design["certainty"]["n"]
        self.exact = 0.0
        # stratum -> list of (eligible, judged, y): y is the phase-1 value for a non-eligible unit, the phase-2 value for a judged one.
        self.units = defaultdict(list)

    def add_exact(self, y: float) -> None:
        self.exact += y

    def add_unit(self, h: str, y: float, eligible: bool = False, judged: bool = False) -> None:
        self.units[h].append((eligible, judged, y))

    def total(self) -> dict:
        t = self.exact
        v1 = v2 = 0.0
        unjudged = 0.0
        for h, us in self.units.items():
            N, n = self.Nh[h], self.nh[h]
            if n == 0:
                continue
            w = N / n
            L = sum(1 for e, _, _ in us if e)
            judged = [y for e, j, y in us if e and j]
            m = len(judged)
            ybar2 = sum(judged) / m if m else 0.0
            if L and not m:
                unjudged += w * L
            # Phase-1 values, eligible units imputed by the stratum's prefix mean.
            z = [(ybar2 if e else y) for e, _, y in us] + [0.0] * max(0, n - len(us))
            t += w * sum(z)
            if n > 1 and N > n:
                zbar = sum(z) / n
                s2 = sum((x - zbar) ** 2 for x in z) / (n - 1)
                v1 += N * N * (1 - n / N) * s2 / n
            if m > 1 and L > m:
                s2y = sum((y - ybar2) ** 2 for y in judged) / (m - 1)
                v2 += w * w * L * L * (1 - m / L) * s2y / m
        return {"total": t, "var": v1 + v2, "unjudged_eligible": unjudged}


def share(est: TwoPhase, denom: float, n_nominal: float) -> dict:
    r = est.total()
    p = r["total"] / denom if denom else 0.0
    vp = r["var"] / denom**2 if denom else 0.0
    return {
        "total": round(r["total"], 1),
        "share": p,
        "se": math.sqrt(vp),
        "lb95": kg_lb(p, vp, n_nominal),
        "denominator": denom,
        "unjudged_eligible": round(r["unjudged_eligible"], 1),
    }


def share_normal(est: TwoPhase, denom: float) -> dict:
    r = est.total()
    p = r["total"] / denom if denom else 0.0
    se = math.sqrt(r["var"]) / denom if denom else 0.0
    return {"total": round(r["total"]), "share": p, "se": se, "lb95_normal": max(0.0, p - Z95 * se), "denominator": denom}
