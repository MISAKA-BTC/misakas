#!/usr/bin/env python3
"""**The census report** (RFC-0002 §II.10.3–5): funnels over D_all / D_files / D_rights, estimates with one-sided 95 % lower
bounds, strata, and the blocker buckets — from the snapshot's listing-depth rows (every repository) and the gate rows of the sampled,
fetched ones. No network.

    report.py --snapshot DIR --rows DIR/rows/<run>.jsonl [--out DIR/report]

**Estimation.** D_all = decided ∪ undecided. A *decided* repository failed a gate on its listing (its outcome is exact: it passes
nothing past that gate). The *undecided* part is estimated from the stratified sample (`sample/design.json`): the certainty stratum
counts exactly, every other stratum h by its Horvitz–Thompson expansion N_h / n_h. A sampled repository without a gate row (not
fetched, or its row missing) counts as a **failure** at every gate past `source` (nonresponse is never dropped, §II.10.5.1).
For a share p̂ with design variance v̂, the one-sided 95 % lower bound is Korn–Graubard's: the Clopper–Pearson bound at the effective
sample size n* = p̂(1−p̂)/v̂ (`kg_lb`), and 0 when p̂ = 0. Download-weighted shares use the exact download total of the denominator and
a normal-approximation bound (said so wherever printed).
"""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import json
import math
import sys
from collections import Counter, defaultdict
from pathlib import Path

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
    """The regularized incomplete beta I_x(a, b)."""
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
    """One-sided Clopper–Pearson lower bound for x successes in n (x, n may be fractional effective counts)."""
    if n <= 0 or x <= 0:
        return 0.0
    if x >= n:
        return alpha ** (1.0 / n)
    return beta_ppf(alpha, x, n - x + 1)


def kg_lb(p: float, v: float, n_nominal: float) -> float:
    if p <= 0:
        return 0.0
    if v <= 0:
        # Every stratum's sample agreed: fall back to the nominal sample size.
        n_eff = n_nominal
    else:
        n_eff = p * (1 - p) / v
    return cp_lb(p * n_eff, n_eff)


# ---- estimation ----------------------------------------------------------------------------------------------------------------------
class Estimator:
    """A total Σ y over D_all (or a domain of it): exact over decided and certainty units, HT over the sampled strata."""

    def __init__(self, design: dict):
        self.Nh = {h: s["N"] for h, s in design["strata"].items()}
        self.nh = {h: s["n"] for h, s in design["strata"].items()}
        self.exact = 0.0
        self.y = defaultdict(list)  # stratum -> [y of each sampled unit]

    def add_exact(self, y: float) -> None:
        self.exact += y

    def add_sampled(self, h: str, y: float) -> None:
        if h == "certainty":
            self.exact += y
        else:
            self.y[h].append(y)

    def total(self) -> tuple[float, float]:
        t, v = self.exact, 0.0
        for h, ys in self.y.items():
            N, n = self.Nh[h], self.nh[h]
            if n == 0:
                continue
            # Units sampled but absent from `ys` are nonresponse: they were added as y = 0 by the caller.
            mean = sum(ys) / n
            t += N * mean
            if n > 1:
                s2 = (sum((yy - mean) ** 2 for yy in ys) + (n - len(ys)) * mean**2) / (n - 1)
                v += N * N * (1 - n / N) * s2 / n
        return t, v


def gate(row: dict, view: str, name: str) -> dict:
    return next(g for g in row[view] if g["gate"] == name)


def stop_key(row: dict, view: str = "technical") -> tuple[str, str, str]:
    """The first gate that is not PASS, its code and argument."""
    for g in row[view]:
        if g["status"] != "PASS":
            return g["gate"], g.get("blocking") or g["status"], g.get("arg") or ""
    return "none", "PASS", ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--rows", required=True, nargs="+")
    ap.add_argument("--out")
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = Path(a.out).expanduser() if a.out else snap / "report"
    out.mkdir(exist_ok=True)
    man = json.loads((snap / "MANIFEST.json").read_text())
    design = json.loads((snap / "sample" / "design.json").read_text())
    sample = {}
    for line in open(snap / "sample" / "sample.jsonl"):
        s = json.loads(line)
        sample[s["id"]] = s
    rows = {}
    for p in a.rows:
        for line in open(p):
            r = json.loads(line)
            rows[r["repo"]] = r

    # ---- pass over every repository at the listing depth -----------------------------------------------------------------------
    N = 0
    W = 0  # downloads, all
    d_files = d_files_w = 0
    d_rights_p = d_rights_p_w = 0
    decided_stop = Counter()
    decided_stop_w = Counter()
    decided_stop_arg = defaultdict(Counter)
    by_group = defaultdict(lambda: Counter())
    listing_source_pass = 0
    rights_flag = {}
    rights_flag_w = {}
    est = {k: Estimator(design) for k in ("lower", "shape", "shape_retry", "source_h")}
    est_w = {k: Estimator(design) for k in ("lower", "shape", "shape_retry")}
    est_files = {k: Estimator(design) for k in ("lower", "shape")}
    est_rights = {k: Estimator(design) for k in ("lower", "shape")}
    est_rights_w = {k: Estimator(design) for k in ("shape",)}
    buckets = Counter()
    buckets_w = Counter()
    bucket_args = defaultdict(Counter)
    nonresponse = 0
    sampled_seen = 0
    with gzip.open(snap / "classified.jsonl.gz", "rt") as f:
        for line in f:
            c = json.loads(line)
            N += 1
            if "error" in c:
                # A listing record that did not read: a failure at source, kept in D_all.
                k = ("source", "LISTING_UNREADABLE")
                decided_stop[k] += 1
                buckets[k] += 1
                continue
            r = c["row"]
            w = r["downloads"]
            W += w
            g = r["strata"]["task_group"]
            fmt = r["strata"]["format"]
            src_pass = gate(r, "technical", "source")["status"] == "PASS"
            rp = c["rights_by_policy"].get("permissive-card-v0", False)
            by_group[(g, fmt)]["N"] += 1
            by_group[(g, fmt)]["W"] += w
            if src_pass:
                listing_source_pass += 1
                d_files += 1
                d_files_w += w
                by_group[(g, fmt)]["files"] += 1
                if rp:
                    d_rights_p += 1
                    d_rights_p_w += w
            if c["decided"]:
                k = stop_key(r)
                decided_stop[k[:2]] += 1
                decided_stop_w[k[:2]] += w
                decided_stop_arg[k[:2]][k[2]] += 1
                buckets[k[:2]] += 1
                buckets_w[k[:2]] += w
                bucket_args[k[:2]][k[2]] += 1
                by_group[(g, fmt)]["decided"] += 1
                continue
            by_group[(g, fmt)]["undecided"] += 1
            s = sample.get(r["repo"])
            if s is None:
                continue
            sampled_seen += 1
            h = s["design_stratum"]
            row = rows.get(r["repo"])
            by_group[(g, fmt)]["sampled"] += 1
            inv = 1.0 if h == "certainty" else design["strata"][h]["N"] / max(1, design["strata"][h]["n"])
            if row is None:
                nonresponse += 1
                y_lower = y_shape = y_retry = 0.0
                k = ("lower", "NOT_FETCHED", "")
                y_src = 0.0
            else:
                by_group[(g, fmt)]["fetched"] += 1
                y_src = 1.0 if gate(row, "technical", "source")["status"] == "PASS" else 0.0
                y_lower = 1.0 if (y_src and gate(row, "technical", "lower")["status"] == "PASS") else 0.0
                y_shape = 1.0 if row["shape_ready"] else 0.0
                y_retry = 1.0 if row.get("shape_ready_at_retry") else 0.0
                k = stop_key(row)
                if k[0] == "pack" and y_shape:
                    k = ("none", "SHAPE_READY", "")
                by_group[(g, fmt)]["shape"] += y_shape
                by_group[(g, fmt)]["shape_w"] += y_shape * w
                by_group[(g, fmt)]["lower"] += y_lower
            for name, y in (("lower", y_lower), ("shape", y_shape), ("shape_retry", y_retry), ("source_h", y_src)):
                est[name].add_sampled(h, y)
            for name, y in (("lower", y_lower), ("shape", y_shape), ("shape_retry", y_retry)):
                est_w[name].add_sampled(h, y * w)
            for name, y in (("lower", y_lower), ("shape", y_shape)):
                est_files[name].add_sampled(h, y)
                est_rights[name].add_sampled(h, y * (1.0 if rp else 0.0))
            est_rights_w["shape"].add_sampled(h, y_shape * w * (1.0 if rp else 0.0))
            buckets[k[:2]] += inv
            buckets_w[k[:2]] += inv * w
            bucket_args[k[:2]][k[2]] += inv
            if row is not None:
                by_group[(g, fmt)]["stop:" + "|".join(k[:2])] += 1

    def share(e: Estimator, denom: float, n_nom: float) -> dict:
        t, v = e.total()
        p = t / denom if denom else 0.0
        vp = v / denom**2 if denom else 0.0
        return {"total": t, "share": p, "se": math.sqrt(vp), "lb95": kg_lb(p, vp, n_nom), "denominator": denom}

    def share_w(e: Estimator, denom: float) -> dict:
        t, v = e.total()
        p = t / denom if denom else 0.0
        se = math.sqrt(v) / denom if denom else 0.0
        return {"total": t, "share": p, "se": se, "lb95_normal": max(0.0, p - Z95 * se), "denominator": denom}

    n_nom = design["n"]
    res = {
        "schema": "misaka.palw.hf-census-report.v1",
        "snapshot": man["snapshot"],
        "t0_utc": man["t0_utc"],
        "generated_utc": dt.datetime.now(dt.UTC).isoformat(timespec="seconds"),
        "d_all": N,
        "d_files_listing": d_files,
        "d_rights": {"none": 0, "permissive-card-v0 (PROPOSED, not adopted)": d_rights_p},
        "downloads": {"d_all": W, "d_files": d_files_w, "d_rights_proposed": d_rights_p_w},
        "decided": sum(decided_stop.values()),
        "undecided": design["undecided"],
        "sample": {"n": design["n"], "seen": sampled_seen, "fetched_rows": len(rows), "nonresponse_as_failure": nonresponse, "seed": design["seed"]},
        "strict": {"registration_ready": 0, "why": "the rights policy `none` confirms no repository (D_rights = 0) and `pack` needs the weights (NOT_RUN in a header census)"},
        "technical": {
            "d_all": {
                "source_pass_listing": {"total": listing_source_pass, "share": listing_source_pass / N},
                "lower": share(est["lower"], N, n_nom),
                "shape_ready": share(est["shape"], N, n_nom),
                "shape_ready_at_retry_only": share(est["shape_retry"], N, n_nom),
            },
            "d_files": {"lower": share(est_files["lower"], d_files, n_nom), "shape_ready": share(est_files["shape"], d_files, n_nom)},
            "d_rights_proposed": {
                "lower": share(est_rights["lower"], d_rights_p, n_nom),
                "shape_ready": share(est_rights["shape"], d_rights_p, n_nom),
            },
            "download_weighted": {
                "d_all_lower": share_w(est_w["lower"], W),
                "d_all_shape_ready": share_w(est_w["shape"], W),
                "d_all_shape_ready_at_retry_only": share_w(est_w["shape_retry"], W),
                "d_rights_proposed_shape_ready": share_w(est_rights_w["shape"], d_rights_p_w),
            },
        },
        "buckets": [
            {
                "gate": k[0],
                "code": k[1],
                "repos_est": round(v, 1),
                "share_d_all": v / N,
                "downloads_est": round(buckets_w[k]),
                "download_share": buckets_w[k] / W if W else 0,
                "top_args": [[a, round(n, 1)] for a, n in bucket_args[k].most_common(8) if a],
            }
            for k, v in buckets.most_common()
        ],
        "strata": [
            {"task_group": g, "format": fmt, **{kk: (round(vv, 1) if isinstance(vv, float) else vv) for kk, vv in cnt.items() if not kk.startswith("stop:")}}
            for (g, fmt), cnt in sorted(by_group.items(), key=lambda x: -x[1]["N"])
        ],
    }
    # Unique feature sets and weights among sampled rows that passed lower / were shape-ready.
    specs_lower = {r["preflight"]["spec_digest"] for r in rows.values() if r.get("preflight") and r["preflight"].get("spec_digest") and gate(r, "technical", "lower")["status"] == "PASS"}
    specs_shape = {r["preflight"]["spec_digest"] for r in rows.values() if r.get("preflight") and r["preflight"].get("spec_digest") and r["shape_ready"]}
    weights_shape = {r["weights_identity"] for r in rows.values() if r.get("weights_identity") and r["shape_ready"]}
    res["unique"] = {
        "sampled_rows": len(rows),
        "lower_pass_rows": sum(1 for r in rows.values() if gate(r, "technical", "lower")["status"] == "PASS"),
        "lower_pass_spec_digests": len(specs_lower),
        "shape_ready_rows": sum(1 for r in rows.values() if r["shape_ready"]),
        "shape_ready_spec_digests": len(specs_shape),
        "shape_ready_weight_sets": len(weights_shape),
        "context_sources": Counter((r.get("context") or {}).get("source", "-").split(".")[0] for r in rows.values()),
    }
    (out / "report.json").write_text(json.dumps(res, indent=1))
    print(json.dumps({k: res[k] for k in ("d_all", "d_files_listing", "decided", "undecided", "sample")}, indent=1))
    t = res["technical"]["d_all"]
    print("technical D_all: lower %.4f (LB %.4f), shape-ready %.4f (LB %.4f), at-retry-only %.4f" % (t["lower"]["share"], t["lower"]["lb95"], t["shape_ready"]["share"], t["shape_ready"]["lb95"], t["shape_ready_at_retry_only"]["share"]))
    for b in res["buckets"][:25]:
        print(f"  {b['gate']:7s} {b['code']:32s} {b['repos_est']:>12.1f} {100 * b['share_d_all']:6.2f}%  dl {100 * b['download_share']:6.2f}%  {b['top_args'][:3]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
