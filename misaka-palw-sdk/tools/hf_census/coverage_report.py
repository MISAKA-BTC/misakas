#!/usr/bin/env python3
"""**The coverage census** (COV-P4, 2026-10-08; RFC-0011 §7 and §16.4): the achievement rate over all public repositories (`D_all`) and
over the repositories that are weight-accessible and form a valid target model (`D_b`), blockers in the Model Onboarding buckets
(`buckets.py`), at the rulesets the rows were judged at. No network.

    coverage_report.py --snapshot DIR --compact-base DIR/p4/compact-classified.jsonl --compact-new DIR/p4/compact-v3.jsonl \
        --rows a=DIR/p4/rows/a --rows b=DIR/p4/rows/b --seed SEED --out DIR/p4/report

The design is `report.py`'s (hf-census-v1.md §4, §4b): the repositories the baseline listing classification (`classified.jsonl.gz`)
decided are counted exactly — by the current tree's listing verdict (`compact-v3`), and a decided repository the current tree leaves
undecided is counted as not passing (`UNDECIDED_UNMEASURED`, a `NOT_RUN` code: UNTESTED); the undecided part is estimated from the
stratified header sample (Horvitz–Thompson), the baseline's three bucket cohorts (`ADAPTER_UNCHECKED`, `TASK_UNKNOWN`,
`BASE_UNPINNED`: a seeded SRS of n = 150 each, expanded by N / n) replace those buckets' baseline counts, and one-sided 95 % lower
bounds are Korn–Graubard (`estimate.kg_lb`). Every sampled repository without a row is a failure (UNTESTED).

`PARTIAL_TASK_ONLY` is read from the baseline's exact count and its own cohort is used only for the text-only column.

Denominator (b), `D_b`, is `D_all` minus every repository whose primary blocker is external (MISSING_WEIGHTS, GATED,
ADAPTER_BASE_MISSING, NO_MODEL_TASK — see `buckets.py`): weights present and complete, public and not gated, an adapter's base pinned
in the snapshot, a model task or a configuration naming a model class. Its listing-decided part is exact; the header-decided part
(an unreachable repository, an unreadable header, an incomplete shard, a missing configuration) is estimated from the sample and
reported as such. The rate over `D_b` is a ratio estimator (linearised variance).
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from buckets import ALL_BUCKETS, CLOSABLE, EXTERNAL, bucket_of, class_agrees, is_unmapped  # noqa: E402
from estimate import TwoPhase, kg_lb, Z95  # noqa: E402
from report import arg_of, load_rows  # noqa: E402

COHORT_KEYS = {("lower", "ADAPTER_UNCHECKED"): "adapters", ("lower", "TASK_UNKNOWN"): "task_unknown", ("source", "BASE_UNPINNED"): "base_unpinned"}


def stop_of(row: dict) -> tuple[str, str, str, str | None]:
    """The first technical gate that is not PASS (`pack`'s NOT_RUN_NEEDS_WEIGHTS skipped): (gate, code, arg, class)."""
    for g in row["technical"]:
        if g["status"] == "PASS":
            continue
        if g["gate"] == "pack" and g.get("blocking") == "NOT_RUN_NEEDS_WEIGHTS":
            continue
        return g["gate"], g.get("blocking") or g["status"], arg_of(g), g.get("class")
    return "none", "PASS", "", None


def norm_arg(code: str, arg: str) -> str:
    """The argument grouped for counting: file names and long evidence collapse to their kind."""
    if not arg:
        return ""
    if code in ("WEIGHTS_INCOMPLETE", "HEADER_INVALID", "FETCH_FAILED"):
        return arg.rsplit("/", 1)[-1].split("-0000")[0][:40]
    if code == "TENSOR_MISSING" or code == "TENSOR_SHAPE":
        return arg[:40]
    return arg[:80]


def outcome_of(r: dict | None) -> dict:
    """One sampled/cohort unit's outcome: success flag and primary blocker."""
    if r is None:
        return {"y": 0, "bucket": "UNTESTED", "dim": "untested", "gate": "lower", "code": "NOT_FETCHED", "arg": "", "short": 0, "unmapped": 0, "capped": 0}
    if "error" in r:
        return {"y": 0, "bucket": "UNTESTED", "dim": "untested", "gate": "lower", "code": "CENSUS_ROW_FAILED", "arg": "", "short": 0, "unmapped": 0, "capped": 0}
    if r["shape_ready"]:
        cx = r.get("context") or {}
        capped = 1 if (cx.get("declared") or 0) > (cx.get("primary") or 0) else 0
        seat = next((g for g in r["technical"] if g["gate"] == "seat"), {})
        seat_short = 1 if seat.get("status") == "FAIL" and seat.get("blocking") == "SEAT_MEMORY" else 0
        return {"y": 1, "bucket": "SHAPE_READY", "dim": "success", "gate": "admit", "code": "SHAPE_READY", "arg": "", "short": 0, "unmapped": 0, "capped": capped, "seat_short": seat_short, "lower_pass": 1,
                "encdec": 1 if any("encdec class" in n for n in ((r.get("preflight") or {}).get("notes") or [])) else 0}
    gate, code, arg, cls = stop_of(r)
    b, dim = bucket_of(gate, code, arg, cls)
    tech = {g["gate"]: g for g in r["technical"]}
    encdec = 1 if any("encdec class" in n for n in ((r.get("preflight") or {}).get("notes") or [])) else 0
    lower_pass = 1 if tech["source"]["status"] == "PASS" and tech["lower"]["status"] == "PASS" else 0
    return {
        "encdec": encdec,
        "lower_pass": lower_pass,
        "y": 0,
        "bucket": b,
        "dim": dim,
        "gate": gate,
        "code": code,
        "arg": norm_arg(code, arg),
        "short": 1 if r.get("shape_ready_at_retry") else 0,
        "unmapped": 1 if is_unmapped(code) else 0,
        "class_ok": class_agrees(b, cls),
    }


class Tally:
    """Counts of repositories by (bucket, gate, code, arg): exact, estimated, and downloads."""

    def __init__(self):
        self.n = Counter()
        self.dl = Counter()
        self.dim = Counter()

    def add(self, key, w, dl=0, dim=None):
        self.n[key] += w
        self.dl[key] += w * dl
        if dim:
            self.dim[dim] += w


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--compact-base", required=True)
    ap.add_argument("--compact-new", required=True)
    ap.add_argument("--rows", action="append", required=True, help="RULESET=PREFIX: PREFIX.sample.jsonl and PREFIX.cohort_NAME.jsonl")
    ap.add_argument("--seed", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--tree", default="unstated")
    ap.add_argument(
        "--partial-override",
        action="append",
        default=[],
        help="RULESET=FILE: the partial-task cohort's rows for that ruleset, judged with PALW_CENSUS_GEN_RANGE_TWIN=1 (the range twin that is in force at DAA 9,000 on testnet-12); the text-only column reads them",
    )
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = Path(a.out).expanduser()
    out.mkdir(parents=True, exist_ok=True)
    design = json.loads((snap / "sample" / "design.json").read_text())
    sample = {}
    for line in open(snap / "sample" / "sample.jsonl"):
        s = json.loads(line)
        sample[s["id"]] = s["design_stratum"]
    cohort_design = {}
    for name in COHORT_KEYS.values():
        cohort_design[name] = json.loads((snap / "sample" / f"cohort_{name}_design.json").read_text())
    cohort_members = {name: [json.loads(x)["id"] for x in open(snap / "sample" / f"cohort_{name}.jsonl")] for name in list(COHORT_KEYS.values()) + ["partial"]}
    partial_design = json.loads((snap / "sample" / "cohort_partial_design.json").read_text())
    for name, cd in cohort_design.items():
        design["strata"][f"cohort-{name}"] = {"N": cd["N"], "n": cd["n"]}
    n_nominal = design["n"]

    def stratum_weight(h: str) -> float:
        if h == "certainty":
            return 1.0
        return design["strata"][h]["N"] / max(1, design["strata"][h]["n"])

    # ---- the exact part (ruleset-independent): the baseline-decided repositories, counted by the current tree's listing verdict -------
    exact = Tally()
    cohort_decided = {name: Tally() for name in COHORT_KEYS.values()}  # frame members the current listing decides: exact
    cell_n = Counter()  # (cohort, gate, code) -> frame members the current listing leaves undecided
    cohort_ids = {i: name for name in COHORT_KEYS.values() for i in cohort_members[name]}
    cell_of = {}  # cohort member -> (gate, code) of the current listing when it is undecided there
    unmeasured = Counter()
    n_all = 0
    dl_all = 0
    d_files = 0
    d_files_dl = 0
    frame_ids_seen = set()
    cohort_frame_n = Counter()
    base_transitions = Counter()
    with open(a.compact_base) as fb, open(a.compact_new) as fn:
        for lb, ln in zip(fb, fn):
            b = json.loads(lb)
            c = json.loads(ln)
            n_all += 1
            if "err" in b or "err" in c:
                exact.add(("MISSING_WEIGHTS", "source", "LISTING_UNREADABLE", ""), 1)
                continue
            dl_all += c["dl"]
            if c["sp"]:
                d_files += 1
                d_files_dl += c["dl"]
            if not b["d"]:
                # In the sampling frame: the sample represents it (the repository itself is in `sample` or not).
                if b["r"] in sample:
                    frame_ids_seen.add(b["r"])
                continue
            kb = (b["k"][0], b["k"][1]) if b["k"] else None
            if kb in COHORT_KEYS:
                name = COHORT_KEYS[kb]
                cohort_frame_n[name] += 1
                gate3, code3, arg3, cls3 = c["k"] if c["k"] else ("none", "PASS", "", None)
                if c["d"]:
                    bkt3, dim3 = bucket_of(gate3, code3, arg3, cls3)
                    cohort_decided[name].add((bkt3, gate3, code3, norm_arg(code3, arg3 or "")), 1, c["dl"], dim3)
                else:
                    cell_n[(name, gate3, code3)] += 1
                    if b["r"] in cohort_ids:
                        cell_of[b["r"]] = (gate3, code3)
                continue
            # Decided in the baseline, outside every cohort: the current tree's listing verdict.
            gate, code, arg, cls = c["k"] if c["k"] else ("none", "PASS", "", None)
            if not c["d"]:
                unmeasured[(kb, (gate, code))] += 1
            bkt, dim = bucket_of(gate, code, arg, cls)
            exact.add((bkt, gate, code, norm_arg(code, arg or "")), 1, c["dl"], dim)
    assert len(frame_ids_seen) == len(sample), (len(frame_ids_seen), len(sample))
    for name, cd in cohort_design.items():
        assert cohort_frame_n[name] == cd["N"], (name, cohort_frame_n[name], cd["N"])

    results = {}
    for spec in a.rows:
        ruleset, prefix = spec.split("=", 1)
        sample_rows = load_rows([f"{prefix}.sample.jsonl"])
        coh_rows = {name: load_rows([f"{prefix}.cohort_{name}.jsonl"]) for name in list(COHORT_KEYS.values()) + ["partial"]}
        for ov in a.partial_override:
            rs_, f_ = ov.split("=", 1)
            if rs_ == ruleset:
                coh_rows["partial"] = load_rows([f_])
        height = next((r["ruleset"]["height"] for r in sample_rows.values() if "ruleset" in r), None)
        trees = sorted({r["ruleset"]["tree"] for r in sample_rows.values() if "ruleset" in r})

        for variant in ("ps", "plain"):
            post_strat = variant == "ps"
            # The baseline-decided repositories outside every cohort are exact. With post-stratification (the report's estimator) the
            # cohort frames' members that the current listing decides are exact too, and a cohort estimates only the members the
            # listing leaves undecided, by the cell of the listing verdict (gate, code); the plain method expands the whole frame.
            base = Tally()
            parts = [exact] + ([cohort_decided[nm] for nm in COHORT_KEYS.values()] if post_strat else [])
            for t in parts:
                for k, v in t.n.items():
                    base.n[k] += v
                    base.dl[k] += t.dl[k]
                base.dim.update(t.dim)
            tally = Tally()
            tally.n.update(base.n)
            tally.dl.update(base.dl)
            tally.dim.update(base.dim)
            units = []  # (stratum, y_success, d_b, external, short, downloads, weight)
            agree_bad = Counter()
            unmapped = Counter()
            encdec_out = Counter()
            unit_n = Counter()  # key -> sampled units (header-read repositories) behind the estimate
            nonresponse = 0
            text_only = Counter()

            def credit(o: dict, w: float, dl: int, h: str):
                nonlocal nonresponse
                key = (o["bucket"], o["gate"], o["code"], o["arg"])
                tally.add(key, w, dl, o["dim"])
                unit_n[key] += 1
                ext = 1.0 if o["bucket"] in EXTERNAL else 0.0
                units.append((h, float(o["y"]), 1.0 - ext, ext, float(o["short"]), dl, w, float(o.get("capped", 0)), float(o.get("seat_short", 0)), float(o.get("lower_pass", 0))))
                if o.get("encdec"):
                    encdec_out[o["code"] + ("(" + o["arg"] + ")" if o["arg"] else "")] += w
                if o.get("unmapped"):
                    unmapped[(o["gate"], o["code"])] += w
                if o.get("class_ok") is False:
                    agree_bad[(o["bucket"], o["code"])] += 1

            for repo, h in sample.items():
                r = sample_rows.get(repo)
                if r is None:
                    nonresponse += 1
                o = outcome_of(r)
                credit(o, stratum_weight(h), (r or {}).get("downloads", 0) if r and "error" not in r else 0, h)
            cell_unmeasured = Counter()
            for name in COHORT_KEYS.values():
                cd = cohort_design[name]
                if not post_strat:
                    w = cd["N"] / cd["n"]
                    for repo in cohort_members[name]:
                        r = coh_rows[name].get(repo)
                        o = outcome_of(r)
                        credit(o, w, (r or {}).get("downloads", 0) if r and "error" not in r else 0, f"cohort-{name}")
                    continue
                # post-stratified: the members the current listing leaves undecided, by cell
                members = defaultdict(list)
                for repo in cohort_members[name]:
                    if repo in cell_of:
                        members[cell_of[repo]].append(repo)
                for (nm, gate3, code3), big_n in sorted(cell_n.items()):
                    if nm != name:
                        continue
                    ms = members.get((gate3, code3), [])
                    if not ms:
                        # a cell with frame members and no cohort member: not measured, counted as not passing
                        for t in (base, tally):
                            t.add(("UNTESTED", gate3, "NOT_SAMPLED_CELL", f"{name}/{code3}"), big_n, 0, "untested")
                        cell_unmeasured[(name, gate3, code3)] = big_n
                        continue
                    h = f"cohort-{name}|{gate3}|{code3}"
                    design["strata"][h] = {"N": big_n, "n": len(ms)}
                    w = big_n / len(ms)
                    for repo in ms:
                        r = coh_rows[name].get(repo)
                        o = outcome_of(r)
                        credit(o, w, (r or {}).get("downloads", 0) if r and "error" not in r else 0, h)

            # ---- the partial-task cohort (the text-only column; not an estimator of anything else) -------------------------------------
            pw = partial_design["N"] / partial_design["n"]
            for repo in cohort_members["partial"]:
                r = coh_rows["partial"].get(repo)
                if r is None or "error" in r:
                    text_only["no_row"] += 1
                    continue
                pf = r.get("preflight") or {}
                blockers = pf.get("blockers") or []
                stage_ok = pf.get("depth_reached") == "shape" and not blockers
                vision_tower = (r.get("image_stage_probe") or {}).get("ok") is True
                text_only["members"] += 1
                text_only["text_stage_admitted"] += 1 if stage_ok else 0
                text_only["vision_class_admitted_dormant_fences"] += 1 if vision_tower else 0
                text_only["has_image_probe"] += 1 if r.get("image_stage_probe") else 0

            # the exact part and the estimated units together stand for exactly D_all
            total_weight = sum(base.n.values()) + sum(u[6] for u in units)
            assert abs(total_weight - n_all) < 1.0, (total_weight, n_all)

            # ---- estimators --------------------------------------------------------------------------------------------------------------
            def estimator(idx_value):
                tp = TwoPhase(design)
                for u in units:
                    tp.add_unit(u[0], idx_value(u))
                return tp

            D_all = n_all
            # exact contributions to the totals the estimators see: the baseline-decided repositories (none is shape-ready)
            exact_n = sum(base.n.values())
            exact_ext = sum(v for k, v in base.n.items() if k[0] in EXTERNAL)
            exact_db = exact_n - exact_ext
            S = estimator(lambda u: u[1])
            S.add_exact(0.0)
            E = estimator(lambda u: u[3])
            E.add_exact(exact_ext)
            Dd = estimator(lambda u: u[2])
            Dd.add_exact(exact_db)
            Sh = estimator(lambda u: u[4])  # short-context only
            Sc = estimator(lambda u: u[7])  # shape-ready at the census cap (8,192) while the model declares a wider context
            Sf = estimator(lambda u: u[1] - u[7])  # shape-ready at its declared context (declared <= the cap)
            Lp = estimator(lambda u: u[9])  # source and lower gates pass (before the admission)
            Ss = estimator(lambda u: u[8])  # shape-ready whose class no seat tier of the fleet holds (SEAT_MEMORY)
            S_t, E_t, D_t, Sh_t = S.total(), E.total(), Dd.total(), Sh.total()
            Sc_t, Sf_t, Ss_t, Lp_t = Sc.total(), Sf.total(), Ss.total(), Lp.total()
            by_bucket_pre = Counter()
            for k, v in tally.n.items():
                by_bucket_pre[k[0]] += v

            def share(tot: dict, denom: float) -> dict:
                p = tot["total"] / denom
                v = tot["var"] / denom**2
                return {"total": round(tot["total"], 1), "share": p, "se": math.sqrt(v), "lb95": kg_lb(p, v, n_nominal)}

            # ratio S / D_b: linearised variance of z = y - p d
            p_b = S_t["total"] / D_t["total"]
            Z = TwoPhase(design)
            for u in units:
                Z.add_unit(u[0], u[1] - p_b * u[2])
            Z.add_exact(-p_b * exact_db)
            Z_t = Z.total()
            v_b = Z_t["var"] / D_t["total"] ** 2
            s_over_db = {"total": round(S_t["total"], 1), "denominator": round(D_t["total"], 1), "share": p_b, "se": math.sqrt(v_b), "lb95": kg_lb(p_b, v_b, n_nominal)}
            # the ceiling: 1 - E / D_all
            ceil_p = 1.0 - E_t["total"] / D_all
            ceiling = {"external_total": round(E_t["total"], 1), "share_external": E_t["total"] / D_all, "ceiling": ceil_p, "se": math.sqrt(E_t["var"]) / D_all}
            ceiling["ceiling_lb95"] = kg_lb(ceil_p, E_t["var"] / D_all**2, n_nominal)
            ceiling["ceiling_ub95_normal"] = min(1.0, ceil_p + Z95 * ceiling["se"])
            # Even with the repositories that declare no task and name no model class counted as closable, the ceiling is this:
            ext3 = by_bucket_pre.get("MISSING_WEIGHTS", 0.0) + by_bucket_pre.get("GATED", 0.0) + by_bucket_pre.get("ADAPTER_BASE_MISSING", 0.0)
            ceiling["ceiling_if_no_model_task_were_closable"] = 1.0 - ext3 / D_all
            ceiling["no_model_task_est"] = round(by_bucket_pre.get("NO_MODEL_TASK", 0.0), 1)

            # download-weighted shape-ready (normal approximation, as report.py)
            tpw = TwoPhase(design)
            for u in units:
                tpw.add_unit(u[0], u[1] * u[5])
            wt = tpw.total()
            dl_share = {"share": wt["total"] / dl_all, "lb95_normal": max(0.0, (wt["total"] - Z95 * math.sqrt(wt["var"])) / dl_all)}

            # ---- the bucket table ---------------------------------------------------------------------------------------------------------
            by_bucket = Counter()
            by_bucket_dl = Counter()
            for k, v in tally.n.items():
                by_bucket[k[0]] += v
                by_bucket_dl[k[0]] += tally.dl[k]
            db_total = D_t["total"]
            table = []
            for b in ["SHAPE_READY"] + list(ALL_BUCKETS):
                v = by_bucket.get(b, 0.0)
                table.append(
                    {
                        "bucket": b,
                        "family": "success(shape)" if b == "SHAPE_READY" else ("external" if b in EXTERNAL else "software-closable"),
                        "repos_est": round(v, 1),
                        "share_d_all": v / D_all,
                        "share_d_b": (v / db_total) if (b not in EXTERNAL) else None,
                        "downloads_share": by_bucket_dl.get(b, 0.0) / dl_all,
                    }
                )
            closable_total = sum(by_bucket.get(b, 0.0) for b in CLOSABLE)
            by_code = [
                {"bucket": k[0], "gate": k[1], "code": k[2], "arg": k[3], "repos_est": round(v, 1), "share_d_all": v / D_all, "sampled_units": unit_n.get(k, 0)}
                for k, v in sorted(tally.n.items(), key=lambda x: -x[1])
            ]
            # (bucket, code) with the leading arguments
            codes = defaultdict(lambda: {"n": 0.0, "args": Counter(), "dl": 0.0})
            for k, v in tally.n.items():
                c = codes[(k[0], k[1], k[2])]
                c["n"] += v
                c["dl"] += tally.dl[k]
                if k[3]:
                    c["args"][k[3]] += v
            code_table = [
                {
                    "bucket": k[0],
                    "gate": k[1],
                    "code": k[2],
                    "repos_est": round(c["n"], 1),
                    "share_d_all": c["n"] / D_all,
                    "downloads_share": c["dl"] / dl_all,
                    "top_args": [[x, round(n, 1)] for x, n in c["args"].most_common(8)],
                }
                for k, c in sorted(codes.items(), key=lambda x: -x[1]["n"])
            ]

            res_v = {
                "variant": "post-stratified cohorts (the report's estimator)" if post_strat else "plain cohort expansion (hf-census-v1 §4b)",
                "cell_unmeasured": {f"{k[0]}/{k[1]}/{k[2]}": v for k, v in cell_unmeasured.items()},
                "ruleset": {"network": "testnet-12", "height": height if height is not None else "schedule end (9000)", "trees": trees},
                "sample": {"n": n_nominal, "nonresponse_as_failure": nonresponse, "units": len(units)},
                "d_all": D_all,
                "evidence_depth": {
                    "listing_only_exact": round(exact_n, 1),
                    "represented_by_header_read_units_est": round(D_all - exact_n, 1),
                    "header_read_units": len(units),
                },
                "d_files_listing": d_files,
                "d_b": {"total": round(db_total, 1), "share_of_d_all": db_total / D_all, "exact_part": exact_db, "external_est": round(E_t["total"], 1)},
                "shape_ready_over_d_all": share(S_t, D_all),
                "shape_ready_over_d_files": share(S_t, d_files),
                "lower_pass_over_d_all": share(Lp_t, D_all),
                "shape_ready_over_d_b": s_over_db,
                "short_context_only_over_d_all": share(Sh_t, D_all),
                "shape_ready_no_fleet_seat_tier_holds_it_over_d_all": share(Ss_t, D_all),
                "shape_ready_context_split_over_d_all": {
                    "at_declared_context": share(Sf_t, D_all),
                    "at_cap_8192_declared_wider": share(Sc_t, D_all),
                },
                "ceiling_if_every_software_closable_fixed": ceiling,
                "software_closable_failures": {"repos_est": round(closable_total, 1), "share_d_all": closable_total / D_all, "share_d_b": closable_total / db_total},
                "external_failures": {"repos_est": round(E_t["total"], 1), "share_d_all": E_t["total"] / D_all},
                "download_weighted_shape_ready": dl_share,
                "buckets": table,
                "dimension": {k: {"repos_est": round(v, 1), "share_d_all": v / D_all} for k, v in sorted(tally.dim.items(), key=lambda x: -x[1])},
                "codes": code_table,
                "by_key": by_code[:400],
                "encoder_decoder_classes": {k: round(v, 1) for k, v in sorted(encdec_out.items(), key=lambda x: -x[1])},
                "text_only_partial_cohort": {"N": partial_design["N"], "n": partial_design["n"], **dict(text_only)},
                "unmapped": {f"{k[0]}:{k[1]}": round(v, 1) for k, v in unmapped.items()},
                "class_disagreements": {f"{k[0]}:{k[1]}": v for k, v in agree_bad.items()},
            }
            if post_strat:
                results[ruleset] = res_v
            else:
                results[ruleset]["plain_method"] = {
                    k: res_v[k]
                    for k in ("d_b", "shape_ready_over_d_all", "shape_ready_over_d_b", "ceiling_if_every_software_closable_fixed", "external_failures", "software_closable_failures")
                }
            print(
                f"ruleset {ruleset} [{variant}] (height {height}): D_b {db_total:,.0f} ({db_total / D_all:.2%} of D_all); shape-ready {S_t['total']:,.0f} = "
                f"{S_t['total'] / D_all:.2%} of D_all (LB {res_v['shape_ready_over_d_all']['lb95']:.2%}), {p_b:.2%} of D_b (LB {s_over_db['lb95']:.2%}); "
                f"external {E_t['total'] / D_all:.2%}; ceiling {ceil_p:.2%}"
            )

    res = {
        "schema": "misaka.palw.hf-coverage-report.v1",
        "snapshot": "2026-10-03T131904Z",
        "generated_utc": dt.datetime.now(dt.UTC).isoformat(timespec="seconds"),
        "tree": a.tree,
        "d_all": n_all,
        "downloads_d_all": dl_all,
        "d_files_listing": d_files,
        "baseline_decided": design["decided"],
        "baseline_undecided": design["undecided"],
        "unmeasured_baseline_decided": {f"{k[0][0]}/{k[0][1]} -> {k[1][0]}/{k[1][1]}": v for k, v in sorted(unmeasured.items(), key=lambda x: -x[1])},
        "cohort_frames": dict(cohort_frame_n),
        "rulesets": results,
    }
    (out / "coverage.json").write_text(json.dumps(res, indent=1))
    print(json.dumps({"d_all": n_all, "d_files": d_files, "cohort_frames": dict(cohort_frame_n)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
