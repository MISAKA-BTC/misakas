#!/usr/bin/env python3
"""**The local-LLM frame** (RFC-0002 §II.12, R8/A8): `L_listed`, `L_files`, one primary route per checkpoint, the TIR rate against the
90 % target with its one-sided 95 % lower bound, and the residual queue.

    report_local.py --snapshot DIR --rows DIR/rows/<run>.jsonl [...] [--out DIR/report]

**The frame** (named sources, dated):
* Hugging Face, the snapshot of `MANIFEST.json`: every repository whose declared task is text generation (`text-generation`,
  declared or the census's conservative inference) or a text-generating chat task with another input (`image-text-to-text`,
  `any-to-any`, … — counted for their full input task, §II.12.1), and every GGUF repository with no declared task whose Hub GGUF
  summary reports a context length (an LLM-shaped GGUF the Hub lists without a task).
* Ollama's library (`DIR/ollama/units.jsonl`, fetched separately and dated by its log): one unit per `<model>:<stem>`, its
  quantisations the format stratum.
The unit is the repository (HF) or the unit (Ollama). Exact mirrors collapse by digest only where digests were read (the fetched
sample: LFS SHA-256 sets; Ollama: model-layer digests), so the population counts are an upper bound on distinct checkpoints.

**Routes.** `UNSUPPORTED` — the source fails (not in `L_files`) or the task has no canonical job; `GVM_FALLBACK` — the first TIR
blocker is a missing semantic (`FEATURE_C`): the RFC-0005 residual queue, **NOT_RUN** until RFC-0005's fallback exists (never
inferred); `TIR` — everything else, with its first TIR blocker (format, quantisation, configuration keys, adapters, real-size
admission, seats) or `shape-ready`. A `TIR` pass needs `Final` on chain: **0 measured** here; the census reports the shape-ready
TIR share as the upper bound it is.
"""

from __future__ import annotations

import argparse
import gzip
import json
import math
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report import Estimator, gate, kg_lb, stop_key  # noqa: E402

TEXT_GROUPS = {"text-generation", "multimodal-text"}


def route_of(stop: tuple[str, str, str]) -> str:
    g, code, _ = stop
    if g == "source":
        return "UNSUPPORTED"
    if code in ("MODALITY_PROFILE_MISSING",):
        return "UNSUPPORTED"
    if code == "FEATURE_C":
        return "GVM_FALLBACK"
    return "TIR"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--rows", required=True, nargs="+")
    ap.add_argument("--out")
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out = Path(a.out).expanduser() if a.out else snap / "report"
    out.mkdir(exist_ok=True)
    design = json.loads((snap / "sample" / "design.json").read_text())
    sample = {json.loads(x)["id"]: json.loads(x) for x in open(snap / "sample" / "sample.jsonl")}
    rows = {}
    for p in a.rows:
        for line in open(p):
            r = json.loads(line)
            rows[r["repo"]] = r

    listed = files = 0
    listed_by = Counter()
    files_w = 0
    e_shape = Estimator(design)
    e_lower = Estimator(design)
    routes = Counter()  # route -> estimated repos (decided exact + sampled HT)
    blockers = Counter()  # (route, gate, code) -> estimated repos
    blocker_args = defaultdict(Counter)
    unsampled_gguf = 0
    n_nom = 0
    with gzip.open(snap / "classified.jsonl.gz", "rt") as fc, gzip.open(snap / "dall.listing.jsonl.gz", "rt") as fl:
        for lc, ll in zip(fc, fl):
            c = json.loads(lc)
            if "error" in c:
                continue
            r = c["row"]
            g = r["strata"]["task_group"]
            in_frame = g in TEXT_GROUPS
            gguf_untasked = False
            if not in_frame and r["strata"]["format"] == "gguf" and r["task"]["task"] == "unknown":
                l = json.loads(ll)
                if l.get("gguf_ctx"):
                    in_frame = True
                    gguf_untasked = True
            if not in_frame:
                continue
            listed += 1
            listed_by[(g if not gguf_untasked else "gguf-untasked", r["strata"]["format"])] += 1
            src_pass = gate(r, "technical", "source")["status"] == "PASS"
            if src_pass:
                files += 1
                files_w += r["downloads"]
            if gguf_untasked:
                # In the frame by its GGUF summary, but its declared task is unknown, so the census's row stopped at TASK_UNKNOWN
                # and it was not sampled: its TIR outcome is not measured here (counted as not passing).
                if src_pass:
                    unsampled_gguf += 1
                    routes["TIR"] += 1
                    blockers[("TIR", "lower", "NOT_SAMPLED_UNTASKED_GGUF")] += 1
                else:
                    k = stop_key(r)
                    routes["UNSUPPORTED"] += 1
                    blockers[("UNSUPPORTED",) + k[:2]] += 1
                continue
            if c["decided"]:
                k = stop_key(r)
                rt = route_of(k)
                routes[rt] += 1
                blockers[(rt,) + k[:2]] += 1
                blocker_args[(rt,) + k[:2]][k[2]] += 1
                continue
            s = sample.get(r["repo"])
            if s is None:
                continue
            h = s["design_stratum"]
            inv = 1.0 if h == "certainty" else design["strata"][h]["N"] / max(1, design["strata"][h]["n"])
            row = rows.get(r["repo"])
            n_nom += 1
            if row is None:
                y_shape = y_lower = 0.0
                k = ("lower", "NOT_FETCHED", "")
            else:
                y_shape = 1.0 if row["shape_ready"] else 0.0
                y_lower = 1.0 if gate(row, "technical", "lower")["status"] == "PASS" else 0.0
                k = stop_key(row)
                if y_shape:
                    k = ("admit", "SHAPE_READY", "")
            e_shape.add_sampled(h, y_shape)
            e_lower.add_sampled(h, y_lower)
            rt = route_of(k)
            routes[rt] += inv
            blockers[(rt,) + k[:2]] += inv
            blocker_args[(rt,) + k[:2]][k[2]] += inv
    # Undecided units outside the sample are represented by the HT weights above; the decided and untasked are exact.

    # Ollama units: the library's text-generating models (a model whose card says `embedding` is not one).
    ol = []
    olp = snap / "ollama" / "units.jsonl"
    cards = {}
    if olp.exists():
        from ollama import library_cards

        cards = library_cards(gzip.decompress((snap / "ollama" / "library.html.gz").read_bytes()).decode("utf-8", "replace"))
        ol = [json.loads(x) for x in open(olp)]
    ol_embedding = [u for u in ol if "embedding" in cards.get(u["model"], {}).get("capabilities", [])]
    ol = [u for u in ol if u not in ol_embedding]
    # In L_files: a manifest that names the weights (a GGUF `model` layer, or the newer per-tensor layers).
    ol_files = [u for u in ol if u.get("manifest_status") == "ok" and any(l["type"] in ("model", "tensor") for l in u.get("layers", []))]
    is_vision = lambda u: "vision" in cards.get(u["model"], {}).get("capabilities", []) or any(l["type"] == "projector" for l in u.get("layers", []))  # noqa: E731
    ol_vision = sum(1 for u in ol_files if is_vision(u))
    L_listed = listed + len(ol)
    L_files = files + len(ol_files)
    t_shape, v_shape = e_shape.total()
    t_lower, _ = e_lower.total()
    p = t_shape / L_files if L_files else 0.0
    vp = v_shape / L_files**2 if L_files else 0.0
    res = {
        "schema": "misaka.palw.local-llm-frame.v1",
        "snapshot": design.get("seed"),
        "frame": {
            "hf_listed": listed,
            "hf_files": files,
            "hf_listed_by": {f"{k[0]}|{k[1]}": v for k, v in listed_by.most_common()},
            "hf_untasked_gguf_in_files_not_sampled": unsampled_gguf,
            "ollama_units": len(ol),
            "ollama_units_with_manifest": len(ol_files),
            "ollama_vision_units": ol_vision,
            "L_listed": L_listed,
            "L_files": L_files,
        },
        "tir": {
            "final_measured": 0,
            "shape_ready_est": t_shape,
            "shape_ready_share_of_L_files": p,
            "lb95": kg_lb(p, vp, max(1, n_nom)),
            "target": 0.90,
            "lower_pass_est": t_lower,
        },
        "routes_est": {k: round(v, 1) for k, v in routes.most_common()},
        "routes_ollama": {
            "TIR, PARTIAL_TASK_ONLY (a vision chat model: the image stage is not computed by the class the preflight produces)": ol_vision,
            "TIR, not judged (the weights' header is inside a registry blob; this census reads no blob)": len(ol_files) - ol_vision,
            "UNSUPPORTED (no manifest naming the weights)": len(ol) - len(ol_files),
            "excluded (an embedding model, not text generation)": len(ol_embedding),
        },
        "residual_queue": [
            {"route": k[0], "gate": k[1], "code": k[2], "units_est": round(v, 1), "share_L_files": v / L_files if L_files else 0, "top_args": [[x, round(n, 1)] for x, n in blocker_args[k].most_common(6) if x]}
            for k, v in blockers.most_common(30)
        ],
    }
    (out / "local_llm.json").write_text(json.dumps(res, indent=1))
    print(json.dumps({"frame": res["frame"], "tir": res["tir"], "routes_est": res["routes_est"]}, indent=1))
    for q in res["residual_queue"][:20]:
        print(f"  {q['route']:13s} {q['gate']:7s} {q['code']:32s} {q['units_est']:>11.1f} {100 * q['share_L_files']:6.2f}%  {q['top_args'][:2]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
