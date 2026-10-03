#!/usr/bin/env python3
"""**The census report** (RFC-0002 §II.10.3–5): funnels over D_all / D_files / D_rights, estimates with one-sided 95 % lower
bounds, strata, and the blocker buckets — from the snapshot's listing-depth rows (every repository), the headers-depth rows of the
sampled repositories (phase 1) and the shape-depth rows of the random prefix of the eligible ones (phase 2). No network.

    report.py --snapshot DIR --headers-rows DIR/rows/runA.jsonl --shape-rows DIR/rows/run2.jsonl ... --seed SEED [--out DIR/report]

Estimators: `estimate.py`. The strict headline (registration-ready / D_all) is exact: 0 — the rights policy `none` confirms no
repository, and `pack` needs the weights. The technical view is reported beside it.
"""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from estimate import TwoPhase, shape_key, share, share_normal  # noqa: E402


def gate(row: dict, view: str, name: str) -> dict:
    return next(g for g in row[view] if g["gate"] == name)


# A gate row without an `arg` is labelled from its evidence for the buckets' "leading arguments" column only (the code is the row's).
_ARG_RULES = [
    (re.compile(r"GGUF architecture `([^`]+)` has no mapping"), "gguf-arch:{0}"),
    (re.compile(r"GGUF metadata `([^`]+)` is not mapped"), "gguf-meta:{0}"),
    (re.compile(r"GGUF rope scaling `([^`]+)` is not mapped"), "gguf-rope:{0}"),
    (re.compile(r"GGUF: no `([^`]+)`"), "gguf-missing:{0}"),
    (re.compile(r"(\w+): trust_remote_code module"), "remote-code:{0}"),
    (re.compile(r"(\w+): config key\(s\) this lowerer does not model: ([^—]+?) —"), "config-key:{1}"),
    (re.compile(r"config has no `([^`]+)`"), "config-no:{0}"),
    (re.compile(r"`([^`]+)` is an encoder–decoder no adapter"), "encdec-no-adapter:{0}"),
    (re.compile(r"NOT_LOWERABLE\((\w+):"), "{0}"),
]


def arg_of(g: dict) -> str:
    if g.get("arg"):
        return g["arg"]
    ev = " ".join(g.get("evidence") or [])
    for rx, fmt in _ARG_RULES:
        m = rx.search(ev)
        if m:
            return fmt.format(*m.groups())
    return ""


def stop_key(row: dict, view: str = "technical") -> tuple[str, str, str]:
    """Where the repository stops: the first gate that FAILs or is NOT_RUN — skipping `pack`'s NOT_RUN_NEEDS_WEIGHTS, which no
    header census runs (the admit gate after it is judged at the shape depth)."""
    for g in row[view]:
        if g["status"] == "PASS":
            continue
        if g["gate"] == "pack" and g.get("blocking") == "NOT_RUN_NEEDS_WEIGHTS":
            continue
        return g["gate"], g.get("blocking") or g["status"], arg_of(g)
    return "none", "PASS", ""


def eligible(row: dict) -> bool:
    """Phase 2's population: a decoder class whose source and lower gates passed at the headers depth."""
    return (
        gate(row, "technical", "source")["status"] == "PASS"
        and gate(row, "technical", "lower")["status"] == "PASS"
        and gate(row, "technical", "admit").get("blocking") == "NOT_RUN_DEPTH_HEADERS"
    )


def judged_shape(r: dict) -> bool:
    """A shape-depth row of a decoder class whose source and lower gates passed (an eligible repository, judged)."""
    a = gate(r, "technical", "admit")
    return (
        gate(r, "technical", "source")["status"] == "PASS"
        and gate(r, "technical", "lower")["status"] == "PASS"
        and a.get("blocking") not in ("NOT_RUN_DEPTH_HEADERS", "NOT_RUN_PIPELINE_ADMISSION")
    )


def load_rows(paths: list[str]) -> dict:
    out = {}
    for p in paths:
        for line in open(p):
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if "repo" in r:
                out[r["repo"]] = r
    return out


class Inputs:
    """The sample, its phase-1 rows, its phase-2 rows and the judged prefix of the eligible order."""

    def __init__(self, snap: Path, headers_rows: list[str], shape_rows: list[str], seed: str):
        self.design = json.loads((snap / "sample" / "design.json").read_text())
        self.sample = {}
        for line in open(snap / "sample" / "sample.jsonl"):
            s = json.loads(line)
            self.sample[s["id"]] = s
        self.h1 = load_rows(headers_rows)
        self.h2 = {k: v for k, v in load_rows(shape_rows).items() if v.get("context") is None or v["context"].get("judged_at")}
        # A repository judged only at the shape depth (before the headers pass existed) has its shape row stand for both phases.
        for repo, r in self.h2.items():
            self.h1.setdefault(repo, r)
        el = sorted((shape_key(seed, repo), repo) for repo, r in self.h1.items() if repo in self.sample and (eligible(r) or judged_shape(r)))
        self.order = [repo for _, repo in el]
        m = 0
        while m < len(self.order) and self.order[m] in self.h2:
            m += 1
        self.prefix = set(self.order[:m])

    def unit(self, repo: str):
        """(stratum, phase-1 row or None, eligible, judged, phase-2 row or None)."""
        h = self.sample[repo]["design_stratum"]
        r1 = self.h1.get(repo)
        if r1 is None:
            return h, None, False, False, None
        el = eligible(r1) or judged_shape(r1)
        judged = repo in self.prefix
        return h, r1, el, judged, (self.h2.get(repo) if judged else None)


def weight_of(design: dict, h: str) -> float:
    if h == "certainty":
        return 1.0
    return design["strata"][h]["N"] / max(1, design["strata"][h]["n"])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--headers-rows", nargs="+", required=True)
    ap.add_argument("--shape-rows", nargs="*", default=[])
    ap.add_argument("--seed", required=True)
    ap.add_argument("--out")
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = Path(a.out).expanduser() if a.out else snap / "report"
    out.mkdir(exist_ok=True)
    man = json.loads((snap / "MANIFEST.json").read_text())
    I = Inputs(snap, a.headers_rows, a.shape_rows, a.seed)
    design = I.design

    N = W = 0
    d_files = d_files_w = d_rights_p = d_rights_p_w = 0
    est = {k: TwoPhase(design) for k in ("source_h", "lower", "shape", "shape_retry")}
    est_w = {k: TwoPhase(design) for k in ("lower", "shape", "shape_retry")}
    est_rp = {k: TwoPhase(design) for k in ("lower", "shape")}
    est_rp_w = TwoPhase(design)
    shape_estimators = lambda: (est["shape"], est["shape_retry"], est_w["shape"], est_w["shape_retry"], est_rp["shape"], est_rp_w)  # noqa: E731
    buckets = Counter()
    buckets_w = Counter()
    bucket_args = defaultdict(Counter)
    by_group = defaultdict(Counter)
    nonresponse = 0

    def credit(k, weight, w):
        buckets[k[:2]] += weight
        buckets_w[k[:2]] += weight * w
        if k[2]:
            bucket_args[k[:2]][k[2]] += weight

    with gzip.open(snap / "classified.jsonl.gz", "rt") as f:
        for line in f:
            c = json.loads(line)
            N += 1
            if "error" in c:
                credit(("source", "LISTING_UNREADABLE", ""), 1, 0)
                continue
            r = c["row"]
            w = r["downloads"]
            W += w
            g, fmt = r["strata"]["task_group"], r["strata"]["format"]
            grp = by_group[(g, fmt)]
            grp["N"] += 1
            grp["downloads"] += w
            rp = bool(c["rights_by_policy"].get("permissive-card-v0"))
            if gate(r, "technical", "source")["status"] == "PASS":
                d_files += 1
                d_files_w += w
                grp["d_files"] += 1
                if rp:
                    d_rights_p += 1
                    d_rights_p_w += w
            if c["decided"]:
                credit(stop_key(r), 1, w)
                grp["decided"] += 1
                continue
            grp["undecided"] += 1
            if r["repo"] not in I.sample:
                continue
            h, r1, el, judged, r2 = I.unit(r["repo"])
            grp["sampled"] += 1
            wt = weight_of(design, h)
            if r1 is None:
                nonresponse += 1
                for e in list(est.values()) + list(est_w.values()) + list(est_rp.values()) + [est_rp_w]:
                    e.add_unit(h, 0.0)
                credit(("lower", "NOT_FETCHED", ""), wt, w)
                continue
            grp["fetched"] += 1
            y_src = 1.0 if gate(r1, "technical", "source")["status"] == "PASS" else 0.0
            y_lower = 1.0 if (y_src and gate(r1, "technical", "lower")["status"] == "PASS") else 0.0
            est["source_h"].add_unit(h, y_src)
            est["lower"].add_unit(h, y_lower)
            est_w["lower"].add_unit(h, y_lower * w)
            est_rp["lower"].add_unit(h, y_lower * rp)
            grp["lower"] += y_lower
            if not el:
                for e in shape_estimators():
                    e.add_unit(h, 0.0)
                credit(stop_key(r1), wt, w)
                continue
            grp["eligible"] += 1
            if not judged:
                for e in shape_estimators():
                    e.add_unit(h, 0.0, eligible=True, judged=False)
                continue
            grp["judged"] += 1
            ys = 1.0 if r2["shape_ready"] else 0.0
            yr = 1.0 if r2.get("shape_ready_at_retry") else 0.0
            grp["shape"] += ys
            grp["shape_2k_only"] += yr
            est["shape"].add_unit(h, ys, eligible=True, judged=True)
            est["shape_retry"].add_unit(h, yr, eligible=True, judged=True)
            est_w["shape"].add_unit(h, ys * w, eligible=True, judged=True)
            est_w["shape_retry"].add_unit(h, yr * w, eligible=True, judged=True)
            est_rp["shape"].add_unit(h, ys * rp, eligible=True, judged=True)
            est_rp_w.add_unit(h, ys * w * rp, eligible=True, judged=True)
    # The judged prefix's stops stand for every eligible repository of their stratum: weight (N_h/n_h)(L_h/m_h).
    L, M = Counter(), Counter()
    for repo in I.order:
        h = I.sample[repo]["design_stratum"]
        L[h] += 1
        M[h] += repo in I.prefix
    for repo in I.prefix:
        h = I.sample[repo]["design_stratum"]
        r2 = I.h2[repo]
        if r2["shape_ready"]:
            k = ("admit", "SHAPE_READY", "")
        elif r2.get("shape_ready_at_retry"):
            k = ("admit", "SHAPE_READY_AT_2048_ONLY", "")
        else:
            k = stop_key(r2)
        credit(k, weight_of(design, h) * L[h] / M[h], r2["downloads"])
    for h, cnt in L.items():
        if M[h] == 0:
            credit(("admit", "NOT_YET_JUDGED", ""), weight_of(design, h) * cnt, 0)

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
        "decided": design["decided"],
        "undecided": design["undecided"],
        "sample": {
            "n": design["n"],
            "seed": design["seed"],
            "phase1_rows": sum(1 for repo in I.sample if repo in I.h1),
            "nonresponse_as_failure": nonresponse,
            "eligible_for_shape": len(I.order),
            "shape_prefix_judged": len(I.prefix),
        },
        "strict": {
            "registration_ready": 0,
            "share": 0.0,
            "why": "rights policy `none` confirms no repository (D_rights = 0); `pack` needs the weights (NOT_RUN in a header census)",
        },
        "technical": {
            "d_all": {
                "source_pass_listing": {"total": d_files, "share": d_files / N},
                "lower": share(est["lower"], N, n_nom),
                "shape_ready": share(est["shape"], N, n_nom),
                "shape_ready_at_2048_only": share(est["shape_retry"], N, n_nom),
            },
            "d_files": {"lower": share(est["lower"], d_files, n_nom), "shape_ready": share(est["shape"], d_files, n_nom)},
            "d_rights_proposed": {"lower": share(est_rp["lower"], d_rights_p, n_nom), "shape_ready": share(est_rp["shape"], d_rights_p, n_nom)},
            "download_weighted": {
                "d_all_lower": share_normal(est_w["lower"], W),
                "d_all_shape_ready": share_normal(est_w["shape"], W),
                "d_all_shape_ready_at_2048_only": share_normal(est_w["shape_retry"], W),
                "d_rights_proposed_shape_ready": share_normal(est_rp_w, d_rights_p_w),
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
                "top_args": [[x, round(n, 1)] for x, n in bucket_args[k].most_common(8)],
            }
            for k, v in buckets.most_common()
        ],
        "strata": [
            {"task_group": g, "format": fmt, **{kk: round(vv, 1) for kk, vv in cnt.items()}}
            for (g, fmt), cnt in sorted(by_group.items(), key=lambda x: -x[1]["N"])
        ],
    }
    judged_rows = [I.h2[r] for r in I.prefix]
    spec = lambda r: (r.get("preflight") or {}).get("spec_digest")  # noqa: E731
    res["unique"] = {
        "judged_rows": len(judged_rows),
        "shape_ready_rows": sum(1 for r in judged_rows if r["shape_ready"]),
        "shape_ready_spec_digests": len({spec(r) for r in judged_rows if r["shape_ready"] and spec(r)}),
        "shape_ready_weight_sets": len({r["weights_identity"] for r in judged_rows if r["shape_ready"] and r.get("weights_identity")}),
        "lower_pass_rows": sum(1 for r in I.h1.values() if gate(r, "technical", "lower")["status"] == "PASS"),
        "lower_pass_spec_digests": len({spec(r) for r in I.h1.values() if spec(r) and gate(r, "technical", "lower")["status"] == "PASS"}),
    }
    (out / "report.json").write_text(json.dumps(res, indent=1))
    print(json.dumps({k: res[k] for k in ("d_all", "d_files_listing", "decided", "undecided", "sample")}, indent=1))
    t = res["technical"]["d_all"]
    print(
        "technical D_all: lower %.4f (LB %.4f), shape-ready %.4f (LB %.4f), 2k-only %.4f; unjudged eligible %.0f"
        % (
            t["lower"]["share"],
            t["lower"]["lb95"],
            t["shape_ready"]["share"],
            t["shape_ready"]["lb95"],
            t["shape_ready_at_2048_only"]["share"],
            t["shape_ready"]["unjudged_eligible"],
        )
    )
    for b in res["buckets"][:30]:
        print(f"  {b['gate']:7s} {b['code']:34s} {b['repos_est']:>12.1f} {100 * b['share_d_all']:6.2f}%  dl {100 * b['download_share']:6.2f}%  {b['top_args'][:3]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
