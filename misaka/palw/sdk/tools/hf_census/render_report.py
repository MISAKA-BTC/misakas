#!/usr/bin/env python3
"""Render the census's JSON results as the tables of a dated report (markdown on stdout).

    render_report.py --report DIR/report/report.json --local DIR/report/local_llm.json [--verify DIR/report/verify_implied.json]
"""
import argparse
import json


def pct(x: float, d: int = 2) -> str:
    return f"{100 * x:.{d}f} %"


def n(x) -> str:
    return f"{x:,.0f}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", required=True)
    ap.add_argument("--local", required=True)
    ap.add_argument("--verify")
    a = ap.parse_args()
    r = json.load(open(a.report))
    lo = json.load(open(a.local))
    t = r["technical"]
    out = []
    w = out.append
    w("### Denominators\n")
    w("| | Repositories | Downloads (30 days) |")
    w("| --- | ---: | ---: |")
    w(f"| `D_all` | {n(r['d_all'])} | {n(r['downloads']['d_all'])} |")
    w(f"| `D_files` (source PASS on the listing) | {n(r['d_files_listing'])} ({pct(r['d_files_listing'] / r['d_all'])}) | {n(r['downloads']['d_files'])} |")
    w(f"| `D_rights`, policy `none` (the headline's) | 0 | 0 |")
    rp = r["d_rights"]["permissive-card-v0 (PROPOSED, not adopted)"]
    w(f"| `D_rights`, policy `permissive-card-v0` (**proposal, not adopted**) | {n(rp)} ({pct(rp / r['d_all'])}) | {n(r['downloads']['d_rights_proposed'])} |")
    w("")
    w("### The funnel (estimates with one-sided 95 % lower bounds)\n")
    w("| Stage | Strict (rights `none`) | Technical, of `D_all` | Technical, of `D_files` | Technical, of `D_rights` (proposal) |")
    w("| --- | ---: | ---: | ---: | ---: |")
    d = t["d_all"]
    w(f"| `source` PASS | 0 | {pct(d['source_pass_listing']['share'])} (exact) | 100 % | 100 % |")
    w(f"| `lower` PASS | 0 | {pct(d['lower']['share'])} (LB {pct(d['lower']['lb95'])}) | {pct(t['d_files']['lower']['share'])} (LB {pct(t['d_files']['lower']['lb95'])}) | {pct(t['d_rights_proposed']['lower']['share'])} (LB {pct(t['d_rights_proposed']['lower']['lb95'])}) |")
    w(f"| `admit` PASS at the primary context (**shape-ready**) | 0 | {pct(d['shape_ready']['share'])} (LB {pct(d['shape_ready']['lb95'])}) | {pct(t['d_files']['shape_ready']['share'])} (LB {pct(t['d_files']['shape_ready']['lb95'])}) | {pct(t['d_rights_proposed']['shape_ready']['share'])} (LB {pct(t['d_rights_proposed']['shape_ready']['lb95'])}) |")
    w(f"| admitted at 2,048 only (its own stratum) | 0 | {pct(d['shape_ready_at_2048_only']['share'], 3)} | | |")
    w("| `pack`, `seat`, `final` | 0 | not run (weights / a chain) | | |")
    w(f"| **registration-ready** | **0** | **0 measured** (`pack` not run) | | |")
    w("")
    dw = t["download_weighted"]
    w(f"Download-weighted (normal-approximation bounds): `lower` {pct(dw['d_all_lower']['share'])} (LB {pct(dw['d_all_lower']['lb95_normal'])}), shape-ready {pct(dw['d_all_shape_ready']['share'])} (LB {pct(dw['d_all_shape_ready']['lb95_normal'])}).\n")
    s = r["sample"]
    w(f"Sample: n = {n(s['n'])} (seed `{s['seed']}`), phase-1 rows {n(s['phase1_rows'])}, nonresponse {s['nonresponse_as_failure']}; eligible for the shape depth {n(s['eligible_for_shape'])}, judged prefix {n(s['shape_prefix_judged'])} (unjudged eligible estimated at {n(d['shape_ready']['unjudged_eligible'])} repositories, counted as not passing).\n")
    w("### Where repositories stop (technical view, estimated repositories; the first gate that fails or cannot be run)\n")
    w("| Gate | Code | Repositories | Share of `D_all` | Share of downloads | Leading arguments |")
    w("| --- | --- | ---: | ---: | ---: | --- |")
    for b in r["buckets"][:32]:
        args = ", ".join(f"`{x}` {n(c)}" for x, c in b["top_args"][:3] if x)
        w(f"| {b['gate']} | `{b['code']}` | {n(b['repos_est'])} | {pct(b['share_d_all'])} | {pct(b['download_share'])} | {args} |")
    w("")
    u = r["unique"]
    w(f"Unique feature sets: {n(u['lower_pass_spec_digests'])} spec digests among the {n(u['lower_pass_rows'])} sampled repositories that pass `lower`; among the judged shape-ready repositories, {n(u['shape_ready_spec_digests'])} spec digests and {n(u['shape_ready_weight_sets'])} distinct weight sets for {n(u['shape_ready_rows'])} repositories.\n")
    w("### The local-LLM frame (§II.12)\n")
    f = lo["frame"]
    w("| | Units |")
    w("| --- | ---: |")
    w(f"| Hugging Face, listed (text generation, chat with other inputs, untasked LLM-shaped GGUF) | {n(f['hf_listed'])} |")
    w(f"| Hugging Face, in `L_files` | {n(f['hf_files'])} |")
    w(f"| … of which untasked GGUF (supplementary sample) | {n(f['hf_untasked_gguf_in_files'])} |")
    w(f"| Ollama library units (text models; embedding models excluded: {f['ollama_embedding_units_excluded']}) | {n(f['ollama_units'])} |")
    w(f"| Ollama units in `L_files` (a manifest naming the weights) | {n(f['ollama_units_in_files'])} |")
    w(f"| **`L_listed`** / **`L_files`** | **{n(f['L_listed'])}** / **{n(f['L_files'])}** |")
    w("")
    tr = lo["tir"]
    w(f"TIR `Final`: **0 measured**. TIR shape-ready over `L_files`: **{pct(tr['shape_ready_share_of_L_files'])}** (one-sided 95 % LB {pct(tr['lb95'])}) against the 90 % target — **{'met' if tr['meets_target'] else 'not met'}**.\n")
    w("| Route | Gate | First TIR blocker | Units (est.) | Share of `L_files` | Leading arguments |")
    w("| --- | --- | --- | ---: | ---: | --- |")
    for q in lo["residual_queue"][:28]:
        args = ", ".join(f"`{x}` {n(c)}" for x, c in q["top_args"][:2] if x)
        w(f"| {q['route']} | {q['gate']} | `{q['code']}` | {n(q['units_est'])} | {pct(q['share_L_files'])} | {args} |")
    w("")
    w("| Ollama units | |")
    w("| --- | ---: |")
    for k, v in lo["routes_ollama"].items():
        w(f"| {k} | {v} |")
    w("")
    if a.verify:
        v = json.load(open(a.verify))
        w(f"### The 2,048 → primary implication, checked\n\n{v['checked']} implied refusals (seeded, at most 4 B parameters) judged at their primary context for real: {v['agree']} refused there too ({v['same_blocking']} with the same blocking code).\n")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
