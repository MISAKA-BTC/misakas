#!/usr/bin/env python3
"""**The local-LLM frame** (RFC-0002 §II.12, R8/A8): `L_listed`, `L_files`, one primary route per checkpoint, the TIR rate against the
90 % target with its one-sided 95 % lower bound, and the residual queue.

    report_local.py --snapshot DIR --headers-rows DIR/rows/runA.jsonl --shape-rows DIR/rows/run2.jsonl ... --seed SEED [--out DIR/report]

**The frame** (named sources, dated):
* Hugging Face, the snapshot of `MANIFEST.json`: every repository whose declared task is text generation (`text-generation`,
  declared or the census's conservative inference) or a text-generating chat task with another input (`image-text-to-text`,
  `any-to-any`, … — counted for their full input task, §II.12.1), and every GGUF repository with no declared task whose Hub GGUF
  summary reports a context length (an LLM-shaped GGUF the Hub lists without a task).
* Ollama's library (`DIR/ollama/units.jsonl`, fetched separately, dated by its log): one unit per `<model>:<stem>` of a model whose
  card does not say `embedding`; its quantisations are the unit's format stratum.
The unit is the repository (HF) or the unit (Ollama). Exact mirrors collapse by digest only where digests were read, so the
population counts are an upper bound on distinct checkpoints.

**Routes.** `UNSUPPORTED` — the source fails (not in `L_files`) or the task has no canonical job; `GVM_FALLBACK` — the first TIR
blocker is a missing semantic (`FEATURE_C`): the RFC-0005 residual queue, NOT_RUN until RFC-0005's fallback exists (never inferred);
`TIR` — everything else, with its first TIR blocker or `shape-ready`. A TIR pass needs `Final` on chain: **0 measured**; the shape-ready
TIR share is the upper bound it is.
"""

from __future__ import annotations

import argparse
import gzip
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from estimate import TwoPhase, share  # noqa: E402
from report import Inputs, gate, stop_key, weight_of  # noqa: E402

TEXT_GROUPS = {"text-generation", "multimodal-text"}


def route_of(stop: tuple[str, str, str]) -> str:
    g, code, _ = stop
    if g == "source" or code == "MODALITY_PROFILE_MISSING":
        return "UNSUPPORTED"
    if code == "FEATURE_C":
        return "GVM_FALLBACK"
    return "TIR"


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
    I = Inputs(snap, a.headers_rows, a.shape_rows, a.seed)
    design = I.design

    listed = files = files_w = 0
    listed_by = Counter()
    e_shape, e_lower = TwoPhase(design), TwoPhase(design)
    routes = Counter()
    queue = Counter()
    queue_args = defaultdict(Counter)
    untasked_gguf = 0
    in_frame_sampled = 0
    L, M = Counter(), Counter()
    for repo in I.order:
        h = I.sample[repo]["design_stratum"]
        L[h] += 1
        M[h] += repo in I.prefix

    def credit(rt, k, wt):
        routes[rt] += wt
        queue[(rt,) + k[:2]] += wt
        if k[2]:
            queue_args[(rt,) + k[:2]][k[2]] += wt

    with gzip.open(snap / "classified.jsonl.gz", "rt") as fc, gzip.open(snap / "dall.listing.jsonl.gz", "rt") as fl:
        for lc, ll in zip(fc, fl):
            c = json.loads(lc)
            if "error" in c:
                continue
            r = c["row"]
            g = r["strata"]["task_group"]
            in_frame = g in TEXT_GROUPS
            untasked = False
            if not in_frame and r["strata"]["format"] == "gguf" and r["task"]["task"] == "unknown" and json.loads(ll).get("gguf_ctx"):
                in_frame = untasked = True
            sampled = (not c["decided"]) and r["repo"] in I.sample
            if sampled and not in_frame:
                # Outside the frame, inside the sample: a zero of the domain estimate, in its place in the two phases.
                h, r1, el, judged, _ = I.unit(r["repo"])
                e_shape.add_unit(h, 0.0, eligible=el, judged=judged)
                e_lower.add_unit(h, 0.0)
                continue
            if not in_frame:
                continue
            listed += 1
            listed_by[("gguf-untasked" if untasked else g, r["strata"]["format"])] += 1
            src_pass = gate(r, "technical", "source")["status"] == "PASS"
            if src_pass:
                files += 1
                files_w += r["downloads"]
            if untasked:
                # In the frame by its GGUF summary; its declared task is unknown, so the census's row stopped at TASK_UNKNOWN and it was
                # not sampled: its TIR outcome is not measured (counted as not passing).
                if src_pass:
                    untasked_gguf += 1
                    credit("TIR", ("lower", "NOT_SAMPLED_UNTASKED_GGUF", ""), 1)
                else:
                    credit("UNSUPPORTED", stop_key(r), 1)
                continue
            if c["decided"]:
                k = stop_key(r)
                credit(route_of(k), k, 1)
                continue
            if not sampled:
                continue
            in_frame_sampled += 1
            h, r1, el, judged, r2 = I.unit(r["repo"])
            wt = weight_of(design, h)
            if r1 is None:
                e_shape.add_unit(h, 0.0)
                e_lower.add_unit(h, 0.0)
                credit("TIR", ("lower", "NOT_FETCHED", ""), wt)
                continue
            y_lower = 1.0 if gate(r1, "technical", "lower")["status"] == "PASS" else 0.0
            e_lower.add_unit(h, y_lower)
            if not el:
                e_shape.add_unit(h, 0.0)
                k = stop_key(r1)
                credit(route_of(k), k, wt)
                continue
            if not judged:
                e_shape.add_unit(h, 0.0, eligible=True, judged=False)
                continue
            ys = 1.0 if r2["shape_ready"] else 0.0
            e_shape.add_unit(h, ys, eligible=True, judged=True)
            k = ("admit", "SHAPE_READY", "") if ys else stop_key(r2)
            credit(route_of(k), k, wt * L[h] / M[h])
    for h, cnt in L.items():
        if M[h] == 0:
            credit("TIR", ("admit", "NOT_YET_JUDGED", ""), weight_of(design, h) * cnt)

    # Ollama.
    ol, cards = [], {}
    olp = snap / "ollama" / "units.jsonl"
    if olp.exists():
        from ollama import library_cards, pulls_number

        cards = library_cards(gzip.decompress((snap / "ollama" / "library.html.gz").read_bytes()).decode("utf-8", "replace"))
        ol = [json.loads(x) for x in open(olp)]
    ol_embedding = [u for u in ol if "embedding" in cards.get(u["model"], {}).get("capabilities", [])]
    ol = [u for u in ol if u not in ol_embedding]
    ol_files = [u for u in ol if u.get("manifest_status") == "ok" and any(l["type"] in ("model", "tensor") for l in u.get("layers", []))]
    vision = lambda u: "vision" in cards.get(u["model"], {}).get("capabilities", []) or any(l["type"] == "projector" for l in u.get("layers", []))  # noqa: E731
    ol_vision = sum(1 for u in ol_files if vision(u))
    ol_pulls = sum(pulls_number(v.get("pulls")) for k, v in cards.items() if "embedding" not in v.get("capabilities", [])) if cards else 0

    L_listed = listed + len(ol)
    L_files = files + len(ol_files)
    sh = share(e_shape, L_files, max(1, in_frame_sampled))
    res = {
        "schema": "misaka.palw.local-llm-frame.v1",
        "frame": {
            "sources": ["huggingface: the census snapshot", "ollama: library pages and registry manifests (ollama/fetch.log.jsonl)"],
            "hf_listed": listed,
            "hf_files": files,
            "hf_listed_by": {f"{k[0]}|{k[1]}": v for k, v in listed_by.most_common()},
            "hf_untasked_gguf_in_files_not_sampled": untasked_gguf,
            "ollama_units": len(ol),
            "ollama_units_in_files": len(ol_files),
            "ollama_vision_units": ol_vision,
            "ollama_embedding_units_excluded": len(ol_embedding),
            "ollama_pulls_text_models": ol_pulls,
            "L_listed": L_listed,
            "L_files": L_files,
        },
        "tir": {
            "final_measured": 0,
            "shape_ready_est": sh["total"],
            "shape_ready_share_of_L_files": sh["share"],
            "lb95": sh["lb95"],
            "target": 0.90,
            "meets_target": sh["lb95"] >= 0.90,
            "unjudged_eligible": sh["unjudged_eligible"],
            "lower_pass_est": round(e_lower.total()["total"], 1),
        },
        "routes_est_hf": {k: round(v, 1) for k, v in routes.most_common()},
        "routes_ollama": {
            "TIR, PARTIAL_TASK_ONLY (vision chat: the image stage is not computed by the class the preflight produces)": ol_vision,
            "TIR, not judged (the weights' header is inside a registry blob; this census reads no blob)": len(ol_files) - ol_vision,
            "UNSUPPORTED (no manifest naming the weights)": len(ol) - len(ol_files),
        },
        "residual_queue": [
            {"route": k[0], "gate": k[1], "code": k[2], "units_est": round(v, 1), "share_L_files": v / L_files if L_files else 0, "top_args": [[x, round(n, 1)] for x, n in queue_args[k].most_common(6)]}
            for k, v in queue.most_common(40)
        ],
    }
    (out / "local_llm.json").write_text(json.dumps(res, indent=1))
    print(json.dumps({"frame": res["frame"], "tir": res["tir"], "routes_est_hf": res["routes_est_hf"]}, indent=1))
    for q in res["residual_queue"][:25]:
        print(f"  {q['route']:13s} {q['gate']:7s} {q['code']:34s} {q['units_est']:>11.1f} {100 * q['share_L_files']:6.2f}%  {q['top_args'][:2]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
