#!/usr/bin/env python3
"""Render the corpus report (`tests/corpus_v2.rs` JSON) into markdown tables and summary numbers.

    python report.py REPORT.json                       summary + tables on stdout
    python report.py REPORT.json --md OUT.md           the generated sections, between markers, into OUT.md
    python report.py REPORT.json --json SUMMARY.json   the summary numbers as JSON

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
    cls = Counter()
    for e in ents:
        if e["level"] == "C":
            cls[blk.get(e["id"], {}).get("class", "unclassified")] += 1
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
        "level_c_by_class": dict(cls),
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
    else:
        print(summary_md(s))
        print()
        print(by_category_md(s))
        print()
        print(table_results(rep, man, blk))


if __name__ == "__main__":
    main()
