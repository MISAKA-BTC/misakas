#!/usr/bin/env python3
"""Render the corpus report (`tests/corpus_v2.rs` JSON) into markdown tables and summary numbers.

    python report.py REPORT.json                       summary + tables on stdout
    python report.py REPORT.json --md OUT.md           the generated sections, between markers, into OUT.md
    python report.py REPORT.json --json SUMMARY.json   the summary numbers as JSON
    python report.py REPORT.json --md OUT.md --census census_report.json   also the census section

Reads `corpus_v2.json` (what each entry is) and `blockers.json` (this lane's classification of
every Level C entry: which kind of gap it is, which feature request names it).
"""

import argparse
import json
import os
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
BEGIN = "<!-- BEGIN GENERATED: {} -->"
END = "<!-- END GENERATED: {} -->"

# blocker classes (this lane's reading of a Level C entry)
CLASSES = {
    "feature": "a generic FEATURE is missing; lowerable in principle with the existing primitives",
    "route": "no data route exists for this kind of model yet (the lowering is Rust per family, or absent); existing primitives suffice",
    "primitive": "a new general PRIMITIVE would be needed",
    "protocol": "a protocol capability beyond the IR is needed (an input binding, a second history, a stage kind)",
    "remote": "the reference semantics are remote code outside transformers (cannot be pinned)",
}


def load(path):
    with open(path) as f:
        return json.load(f)


def blockers():
    p = os.path.join(HERE, "blockers.json")
    return load(p) if os.path.exists(p) else {}


def manifest():
    return {e["id"]: e for e in load(os.path.join(HERE, "corpus_v2.json"))["entries"]}


def pct(n, d):
    return f"{100.0 * n / d:.0f} %" if d else "-"


def feature_short(f):
    return f.replace("_V1", "")


def stage_ok(e, name):
    s = e.get("stages", {}).get(name)
    return None if s is None else bool(s.get("ok"))


def summarise(rep, man, blk):
    ents = rep["entries"]
    w = rep.get("usage_weight") or {"vh": 8, "h": 4, "m": 2, "l": 1}
    n = len(ents)
    lv = Counter(e["level"] for e in ents)
    wt = defaultdict(float)
    for e in ents:
        wt[e["level"]] += w[man[e["id"]]["usage"]]
    tot = sum(wt.values())
    # data-expressible = Level A or B (and the later stages did not disprove it)
    proven = [e for e in ents if e["level"] in ("A", "B") and e.get("failed_stage") is None]
    refuted = [e for e in ents if e["level"] in ("A", "B") and e.get("failed_stage") is not None]
    # an A/B entry whose reference is remote code has no weights to run: it is Level B at the READ stage only
    read_only = [e["id"] for e in proven if "lower" not in e.get("stages", {})]
    cls = Counter()
    core_routes = []
    for e in ents:
        if e["level"] == "C":
            cls[blk.get(e["id"], {}).get("class", "unclassified")] += 1
            if e.get("core", {}).get("core_route"):
                core_routes.append(e["id"])
    by_cat = defaultdict(Counter)
    for e in ents:
        by_cat[e["category"]][e["level"]] += 1
    # court
    court = [e for e in ents if stage_ok(e, "court")]
    commits = sum(e["stages"]["court"].get("commit_points_replayed", 0) for e in court)
    exec_commits = sum(e["stages"]["court"].get("replayed_on_exec", 0) for e in court)
    roles, prims = Counter(), Counter()
    for e in ents:
        low = e.get("stages", {}).get("lower")
        if low and low.get("ok"):
            for k, v in (low.get("commit_roles") or {}).items():
                roles[k] += v
            for k, v in (low.get("prims") or {}).items():
                prims[k] += 1
    court_fail = [e["id"] for e in ents if stage_ok(e, "court") is False]
    three_way = [e for e in ents if stage_ok(e, "three_way")]
    tw_fail = [e["id"] for e in ents if stage_ok(e, "three_way") is False]
    return {
        "entries": n, "levels": dict(lv),
        "level_pct": {k: 100.0 * v / n for k, v in lv.items()},
        "usage_weighted_pct": {k: 100.0 * v / tot for k, v in wt.items()},
        "data_expressible_proven": len(proven), "data_expressible_refuted": [e["id"] for e in refuted],
        "read_only": read_only,
        "level_c_by_class": dict(cls),
        "core_rust_routes": core_routes,
        "by_category": {k: dict(v) for k, v in by_cat.items()},
        "court": {
            "entries_replayed": len(court), "commit_points_replayed": commits, "replayed_on_exec": exec_commits,
            "failures": court_fail, "commit_roles": dict(roles), "primitive_kinds_reached": len(prims),
            "primitives": sorted(prims),
        },
        "three_way": {"entries": len(three_way), "failures": tw_fail},
    }


def table_results(rep, man, blk):
    rows = ["| id | category | level | route / adapter | features | missing / why not | failed stage |", "| --- | --- | :-: | --- | ---: | --- | --- |"]
    for e in rep["entries"]:
        r = e.get("read", {}).get("report", {})
        feats = r.get("features") or []
        fcount = len(feats)
        missing = [m["what"] for m in (r.get("missing") or [])]
        b = blk.get(e["id"], {})
        why = ", ".join(feature_short(m) for m in missing) if missing else ""
        if e["level"] == "C":
            why = why or b.get("short", "")
            if b.get("frs"):
                why += " → " + ", ".join(b["frs"])
        via = e.get("via", "-")
        if e.get("core", {}).get("core_route"):
            via = (via if via != "-" else "") + (" ; " if via != "-" else "") + "core Rust route"
        fs = e.get("failed_stage") or ""
        rows.append(f"| `{e['id']}` | {e['category']} | **{e['level']}** | {via} | {fcount or ''} | {why} | {fs} |")
    return "\n".join(rows)


def table_corpus(man):
    rows = ["| # | id | category | `architectures[0]` | usage | share | why it is in the corpus |", "| ---: | --- | --- | --- | :-: | ---: | --- |"]
    for i, e in enumerate(man.values(), 1):
        sh = f"{e['share']:g} %" if e.get("share") is not None else "unknown"
        rows.append(f"| {i} | `{e['id']}` | {e['category']} | `{e['hf_arch']}` | {e['usage']} | {sh} | {e['why']} |")
    return "\n".join(rows)


def summary_md(s):
    n = s["entries"]
    lv = s["levels"]
    lines = [
        "| | entries | share of corpus | usage-weighted |",
        "| --- | ---: | ---: | ---: |",
    ]
    for k, name in (("A", "Level A — the config alone"), ("B", "Level B — a thin data adapter"), ("C", "Level C — a capability is missing")):
        lines.append(f"| {name} | {lv.get(k, 0)} | {pct(lv.get(k, 0), n)} | {s['usage_weighted_pct'].get(k, 0):.0f} % |")
    ab = lv.get("A", 0) + lv.get("B", 0)
    lines.append(f"| **A + B (expressible with existing features)** | **{ab}** | **{pct(ab, n)}** | **{s['usage_weighted_pct'].get('A', 0) + s['usage_weighted_pct'].get('B', 0):.0f} %** |")
    if s.get("read_only"):
        k = len(s["read_only"])
        lines.append(f"| *of the A + B, read-level only (the reference is remote code, no weights to run): {', '.join('`'+i+'`' for i in s['read_only'])}* | {k} | {pct(k, n)} | |")
        lines.append(f"| *A + B with every stage proven on weights* | {ab - k} | {pct(ab - k, n)} | |")
    if s.get("core_rust_routes"):
        k = len(s["core_rust_routes"])
        lines.append(f"| *Level C, but lowered today by a per-family Rust route (not data): {', '.join('`'+i+'`' for i in s['core_rust_routes'])}* | {k} | {pct(k, n)} | |")
        lines.append(f"| *supported today by any route (A + B + Rust routes)* | {ab + k} | {pct(ab + k, n)} | |")
    lines.append("")
    lines.append("Level C by kind of gap (this lane's classification, `tools/corpus/blockers.json`):")
    lines.append("")
    lines.append("| kind | entries | meaning |")
    lines.append("| --- | ---: | --- |")
    for k, v in sorted(s["level_c_by_class"].items(), key=lambda kv: -kv[1]):
        lines.append(f"| {k} | {v} | {CLASSES.get(k, 'not yet classified')} |")
    lines.append("")
    c = s["court"]
    lines.append(
        f"Court coverage: {c['commit_points_replayed']} commit points over {c['entries_replayed']} lowered models reproduced by the cone evaluator from their opened leaves "
        f"(reference evaluator and typed backend; {len(c['failures'])} failures); {c['primitive_kinds_reached']} of the 25 primitives and {len(c['commit_roles'])} commit-point roles reached."
    )
    return "\n".join(lines)


def by_category_md(s):
    rows = ["| category | A | B | C | total |", "| --- | ---: | ---: | ---: | ---: |"]
    for cat, c in sorted(s["by_category"].items()):
        rows.append(f"| {cat} | {c.get('A', 0)} | {c.get('B', 0)} | {c.get('C', 0)} | {sum(c.values())} |")
    return "\n".join(rows)



def fr_table(blk):
    """FR -> entries it names, ranked by how many entries it names."""
    titles = blk.get("_frs", {})
    by = defaultdict(list)
    for k, v in blk.items():
        if k.startswith("_"):
            continue
        for fr in v.get("frs", []):
            by[fr].append(k)
    rows = ["| FR | feature | size | entries it names | entries |", "| --- | --- | :-: | ---: | --- |"]
    for fr, ids in sorted(by.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        t = titles.get(fr, {})
        rows.append(f"| {fr} | {t.get('title', '')} | {t.get('size', '')} | {len(ids)} | {', '.join('`'+i+'`' for i in ids)} |")
    return "\n".join(rows)


UPLIFT_GROUPS = [
    ("MoE hyper-parameters and layer pattern", {"num_experts_per_tok", "num_local_experts", "num_experts", "moe_intermediate_size", "norm_topk_prob",
        "first_k_dense_replace", "n_group", "topk_group", "routed_scaling_factor", "n_routed_experts", "n_shared_experts", "decoder_sparse_step",
        "mlp_only_layers", "moe_topk", "router_jitter_noise", "num_nextn_predict_layers", "shared_expert_intermediate_size"}),
    ("MLA dimensions", {"kv_lora_rank", "q_lora_rank", "qk_head_dim", "qk_nope_head_dim", "qk_rope_head_dim", "v_head_dim"}),
    ("nested VLM wrapper (text_config, vision_config, token ids)", {"text_config", "vision_config", "image_token_id", "image_token_index", "video_token_id",
        "vision_start_token_id", "vision_end_token_id", "boi_token_index", "eoi_token_index", "mm_tokens_per_image"}),
    ("bias switches", {"use_bias", "attention_out_bias", "bias", "enable_bias", "use_qkv_bias", "qkv_bias", "add_bias_linear"}),
    ("norm epsilon / norm kind aliases", {"layer_norm_eps", "layer_norm_epsilon", "norm_epsilon", "norm_eps", "layer_norm_elementwise_affine", "do_layer_norm_before", "_remove_final_layer_norm"}),
    ("activation aliases", {"activation_function", "hidden_activation", "activation", "mlp_hidden_act"}),
    ("dimension aliases (n_embd, n_head, n_layer, ...)", {"n_embd", "n_head", "n_layer", "n_positions", "n_inner", "d_model", "n_heads", "n_layers", "ffn_dim", "word_embed_proj_dim", "ffn_hidden_size", "max_seq_len"}),
    ("rope / position keys", {"original_max_position_embeddings", "rotary_pct", "no_rope_layer_interval", "alibi", "use_parallel_residual", "multi_query", "new_decoder_architecture"}),
]


def uplift_md(rep):
    keys = Counter()
    who = defaultdict(set)
    n_read = 0
    n_dec = 0
    for e in rep["entries"]:
        if e["route"] not in ("decoder", "vlm"):
            continue
        n_dec += 1
        n = e["read"]["none"]
        if n.get("ok"):
            n_read += 1
            continue
        for k in (n["failure"].get("unmapped_config_keys") or []):
            keys[k] += 1
            who[k].add(e["id"])
    rows = ["| convention the template lacks | entries it blocks | the keys |", "| --- | ---: | --- |"]
    seen = set()
    for name, ks in UPLIFT_GROUPS:
        ids = set()
        used = []
        for k in ks:
            if k in who:
                ids |= who[k]
                used.append(k)
                seen.add(k)
        if ids:
            rows.append(f"| {name} | {len(ids)} | {', '.join('`'+k+'`' for k in sorted(used))} |")
    rest = sorted(set(who) - seen)
    other_ids = set()
    for k in rest:
        other_ids |= who[k]
    rows.append(f"| everything else (family-specific keys) | {len(other_ids)} | {len(rest)} distinct keys |")
    head = f"The standard template (no adapter) reads {n_read} of the {n_dec} decoder-route entries without refusing; for the rest it names the keys it does not model:\n\n"
    return head + "\n".join(rows)


def census_reason(e):
    """Why a census family is Level C, from the read stage: a refusal (named by its message) or the groups of unmodelled keys."""
    f = ((e.get("read") or {}).get("auto") or {}).get("failure") or {}
    if e.get("failed_stage") != "read":
        return ("later stage: " + str(e.get("failed_stage")), [])
    ks = f.get("unmapped_config_keys") or []
    if ks:
        return ("config keys the template does not model", ks)
    msg = f.get("error") or ""
    msg = msg.replace("NOT_LOWERABLE(", "")
    # drop the leading class name
    if ":" in msg and msg.split(":", 1)[0].replace("`", "").replace(" ", "").isalnum():
        msg = msg.split(":", 1)[1].strip()
    for pat, name in (
        ("is not a causal language model class", "not a causal language model class (no adapter)"),
        ("rope_parameters is keyed by layer type", "rope parameters keyed by layer type, layer has none"),
        ("deepseek_sparse_attention", "layer type `deepseek_sparse_attention` (FR-09)"),
        ("linear_attention", "layer type `linear_attention` not modelled"),
        ("cross-attention", "cross-attention (decoder half of an encoder-decoder)"),
        ("mp_num", "weights layout the binder cannot express (FR-01)"),
        ("MLA + muP remote code", "MiniCPM3 (MLA + muP) refused by name"),
        ("multi_query", "bad config in the tiny fixture (multi_query)"),
    ):
        if pat in msg:
            return (name, [])
    return (msg[:80], [])


def census_md(rep, cman):
    ents = rep["entries"]
    n = len(ents)
    cen = cman.get("census", {})
    lv = Counter(e["level"] for e in ents)
    via = Counter()
    for e in ents:
        if e["level"] == "B":
            v = e.get("via", "")
            via["synthesized" if (e.get("synthesized") or {}).get("adapter") else ("built-in" if "built-in" in v else "third-party")] += 1
    total = n + len(cen.get("not_derivable", [])) + len(cen.get("excluded", []))
    rows = ["| outcome | families | share of the " + str(total) + " |", "| --- | ---: | ---: |"]
    rows.append(f"| Level A (the standard template alone) | {lv.get('A', 0)} | {pct(lv.get('A', 0), total)} |")
    rows.append(f"| Level B through the built-in adapter pack (nobody wrote anything for this run) | {via.get('built-in', 0)} | {pct(via.get('built-in', 0), total)} |")
    rows.append(f"| Level B through a third-party adapter (written by this lane for this table, data only: `tools/corpus/census-adapters/`) | {via.get('third-party', 0)} | {pct(via.get('third-party', 0), total)} |")
    rows.append(f"| Level B through a synthesised adapter (convention search) | {via.get('synthesized', 0)} | {pct(via.get('synthesized', 0), total)} |")
    ab = lv.get("A", 0) + lv.get("B", 0)
    rows.append(f"| **A + B** | **{ab}** | **{pct(ab, total)}** (of the {n} buildable: {pct(ab, n)}) |")
    rows.append(f"| Level C (read refused or a later stage failed) | {lv.get('C', 0)} | {pct(lv.get('C', 0), total)} |")
    rows.append(f"| no automatic tiny config (listed below) | {len(cen.get('not_derivable', []))} | {pct(len(cen.get('not_derivable', [])), total)} |")
    rows.append(f"| excluded: not a standalone text decoder (listed below, with the reason) | {len(cen.get('excluded', []))} | {pct(len(cen.get('excluded', [])), total)} |")
    out = ["\n".join(rows), ""]
    groups = defaultdict(list)
    keyed = defaultdict(set)
    for e in ents:
        if e["level"] != "C":
            continue
        why, ks = census_reason(e)
        groups[why].append(e["id"])
        for k in ks:
            keyed[k].add(e["id"])
    out.append("Why the Level C families are Level C (read-stage evidence, `census_report.json`):")
    out.append("")
    out.append("| reason | families | which |")
    out.append("| --- | ---: | --- |")
    for why, ids in sorted(groups.items(), key=lambda kv: -len(kv[1])):
        out.append(f"| {why} | {len(ids)} | {', '.join('`'+i+'`' for i in sorted(ids))} |")
    out.append("")
    # the unmodelled-key groups
    seen = set()
    rows = ["| convention the template lacks | families it blocks | the keys |", "| --- | ---: | --- |"]
    for name, ks in UPLIFT_GROUPS:
        ids, used = set(), []
        for k in ks:
            if k in keyed:
                ids |= keyed[k]
                used.append(k)
                seen.add(k)
        if ids:
            rows.append(f"| {name} | {len(ids)} | {', '.join('`'+k+'`' for k in sorted(used))} |")
    rest = sorted(set(keyed) - seen)
    other = set()
    for k in rest:
        other |= keyed[k]
    rows.append(f"| everything else (family-specific keys) | {len(other)} | {len(rest)} distinct keys |")
    out.append("\n".join(rows))
    nd = cen.get("not_derivable", [])
    if nd:
        out.append("")
        out.append("No automatic tiny config (the family's defaults are over the size guard or its tiny build fails; recorded, not dropped): " + ", ".join(f"`{r['model_type']}`" for r in nd) + ".")
    ex = cen.get("excluded", [])
    if ex:
        out.append("")
        out.append("Excluded, with the reason: " + "; ".join(f"`{r['model_type']}` ({r['why']})" for r in ex) + ".")
    return "\n".join(out)


def splice(path, name, body):
    b, e = BEGIN.format(name), END.format(name)
    text = open(path).read() if os.path.exists(path) else ""
    block = f"{b}\n{body}\n{e}"
    if b in text and e in text:
        i, j = text.index(b), text.index(e) + len(e)
        text = text[:i] + block + text[j:]
    else:
        text = text.rstrip("\n") + "\n\n" + block + "\n"
    with open(path, "w") as f:
        f.write(text)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("report")
    ap.add_argument("--md")
    ap.add_argument("--json")
    ap.add_argument("--census", help="census_report.json: also render the census section (needs census_v2.json next to this script)")
    a = ap.parse_args()
    rep, man, blk = load(a.report), manifest(), blockers()
    s = summarise(rep, man, blk)
    if a.json:
        with open(a.json, "w") as f:
            json.dump(s, f, indent=1)
    if a.md:
        splice(a.md, "corpus", table_corpus(man))
        splice(a.md, "summary", summary_md(s) + "\n\n" + by_category_md(s))
        splice(a.md, "results", table_results(rep, man, blk))
        splice(a.md, "frs", fr_table(blk))
        splice(a.md, "uplift", uplift_md(rep))
        if a.census:
            splice(a.md, "census", census_md(load(a.census), load(os.path.join(HERE, "census_v2.json"))))
    else:
        print(summary_md(s))
        print()
        print(by_category_md(s))
        print()
        print(table_results(rep, man, blk))
        print()
        print(fr_table(blk))
        print()
        print(uplift_md(rep))


if __name__ == "__main__":
    main()
