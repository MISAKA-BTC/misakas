#!/usr/bin/env python3
"""Render `coverage_report.py`'s `coverage.json` as the markdown tables of the dated report (no network, no estimation).

    render_coverage.py COVERAGE.json [--rulesets a b] [--labels "a=DAA 6,900" "b=DAA 9,000"] [--top 14] > tables.md
"""
import argparse
import json
import sys


def pct(x, d=2):
    return f"{100 * x:.{d}f} %"


def n0(x):
    return f"{x:,.0f}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("coverage")
    ap.add_argument("--rulesets", nargs="*", default=["a", "b"])
    ap.add_argument("--labels", nargs="*", default=["a=DAA 6,900 (today)", "b=DAA 9,000 (every scheduled fence)"])
    ap.add_argument("--top", type=int, default=16)
    a = ap.parse_args()
    c = json.load(open(a.coverage))
    labels = dict(x.split("=", 1) for x in a.labels)
    R = c["rulesets"]
    rs = [r for r in a.rulesets if r in R]
    out = []
    w = out.append

    w("### Headline (technical view; the rights policy is not applied)\n")
    w("| | " + " | ".join(labels[r] for r in rs) + " |")
    w("| --- | " + " | ".join("---:" for _ in rs) + " |")

    def row(name, f):
        w(f"| {name} | " + " | ".join(f(R[r]) for r in rs) + " |")

    row("`D_all` (all public repositories)", lambda x: n0(x["d_all"]))
    row("`D_b` (weight-accessible, valid target model), estimated", lambda x: f"{n0(x['d_b']['total'])} ({pct(x['d_b']['share_of_d_all'])} of `D_all`)")
    row(
        "**shape-ready / `D_all`** (point; one-sided 95 % LB)",
        lambda x: f"{n0(x['shape_ready_over_d_all']['total'])} = **{pct(x['shape_ready_over_d_all']['share'])}** (LB {pct(x['shape_ready_over_d_all']['lb95'])})",
    )
    row(
        "**shape-ready / `D_b`** (point; one-sided 95 % LB)",
        lambda x: f"**{pct(x['shape_ready_over_d_b']['share'])}** (LB {pct(x['shape_ready_over_d_b']['lb95'])})",
    )
    row("shape-ready / `D_files` (listing-level)", lambda x: f"{pct(x['shape_ready_over_d_files']['share'])} (LB {pct(x['shape_ready_over_d_files']['lb95'])})")
    row("admitted at 2,048 only (short context; **not counted**)", lambda x: f"{n0(x['short_context_only_over_d_all']['total'])} ({pct(x['short_context_only_over_d_all']['share'])})")
    row("download-weighted shape-ready (normal-approx. LB)", lambda x: f"{pct(x['download_weighted_shape_ready']['share'])} (LB {pct(x['download_weighted_shape_ready']['lb95_normal'])})")
    w("")

    w("### Blockers by bucket, both denominators\n")
    hdr = "| Bucket | family | " + " | ".join(f"{labels[r]}: repos | % `D_all` | % `D_b`" for r in rs) + " |"
    w(hdr)
    w("| --- | --- | " + " | ".join("---: | ---: | ---:" for _ in rs) + " |")
    order = [b["bucket"] for b in R[rs[0]]["buckets"]]
    for b in order:
        cells = []
        fam = ""
        for r in rs:
            x = next(t for t in R[r]["buckets"] if t["bucket"] == b)
            fam = x["family"]
            cells += [n0(x["repos_est"]), pct(x["share_d_all"]), pct(x["share_d_b"]) if x["share_d_b"] is not None else "outside `D_b`"]
        w(f"| `{b}` | {fam} | " + " | ".join(cells) + " |")
    w("")

    w("### External versus software-closable\n")
    w("| | " + " | ".join(labels[r] for r in rs) + " |")
    w("| --- | " + " | ".join("---:" for _ in rs) + " |")
    row("external failures (missing / gated / unresolvable source, no model task)", lambda x: f"{n0(x['external_failures']['repos_est'])} ({pct(x['external_failures']['share_d_all'])})")
    row("software-closable failures (`FRONTEND` + `NEW_KERNEL` + `QUANT_FORMAT` + `RESOURCE` + `UNTESTED`)", lambda x: f"{n0(x['software_closable_failures']['repos_est'])} ({pct(x['software_closable_failures']['share_d_all'])} of `D_all`, {pct(x['software_closable_failures']['share_d_b'])} of `D_b`)")
    row("**maximum achievable rate over `D_all`** if every software-closable failure were fixed", lambda x: f"**{pct(x['ceiling_if_every_software_closable_fixed']['ceiling'])}** (SE {pct(x['ceiling_if_every_software_closable_fixed']['se'])})")
    row("… even if every repository with no model task were also closable", lambda x: pct(x["ceiling_if_every_software_closable_fixed"]["ceiling_if_no_model_task_were_closable"]))
    w("")

    w("### The earlier analysis's dimension\n")
    dims = ["success", "external", "frontend-only", "feature-only", "protocol-envelope", "untested"]
    w("| Dimension | " + " | ".join(f"{labels[r]}: repos | % `D_all`" for r in rs) + " |")
    w("| --- | " + " | ".join("---: | ---:" for _ in rs) + " |")
    for d in dims:
        cells = []
        for r in rs:
            x = R[r]["dimension"].get(d, {"repos_est": 0, "share_d_all": 0})
            cells += [n0(x["repos_est"]), pct(x["share_d_all"])]
        w(f"| `{d}` | " + " | ".join(cells) + " |")
    w("")

    last = rs[-1]
    w(f"### Where repositories stop, by code ({labels[last]}; estimated repositories)\n")
    w("| Bucket | Gate | Code | Repositories | % `D_all` | Leading arguments |")
    w("| --- | --- | --- | ---: | ---: | --- |")
    for x in R[last]["codes"][: a.top * 3]:
        if x["repos_est"] < 1:
            continue
        args = ", ".join(f"`{k}` {n0(v)}" for k, v in x["top_args"][:3])
        w(f"| `{x['bucket']}` | {x['gate']} | `{x['code']}` | {n0(x['repos_est'])} | {pct(x['share_d_all'])} | {args} |")
    w("")
    sys.stdout.write("\n".join(out) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
