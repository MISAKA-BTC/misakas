#!/usr/bin/env python3
"""**A supplementary sample for the local-LLM frame** (RFC-0002 §II.12): the GGUF repositories the Hub lists with no task but with a
GGUF context length are in the frame as text generation; the Hub-wide census stopped them at TASK_UNKNOWN on the listing, so they
were not in its sample. This draws a seeded simple random sample of them among `L_files` (source PASS on the listing).

    sample_frame.py --snapshot DIR --seed SEED --n 150     → DIR/sample/frame_gguf.jsonl, DIR/sample/frame_gguf_design.json
"""
import argparse, gzip, hashlib, json, sys
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--snapshot", required=True)
ap.add_argument("--seed", required=True)
ap.add_argument("--n", type=int, default=150)
a = ap.parse_args()
snap = Path(a.snapshot).expanduser()
key = lambda repo: hashlib.blake2b(f"{a.seed}\0frame-gguf\0{repo}".encode(), digest_size=8).hexdigest()  # noqa: E731
pop = []
with gzip.open(snap / "classified.jsonl.gz", "rt") as fc, gzip.open(snap / "dall.listing.jsonl.gz", "rt") as fl:
    for i, (lc, ll) in enumerate(zip(fc, fl)):
        c = json.loads(lc)
        if "error" in c:
            continue
        r = c["row"]
        if r["strata"]["format"] != "gguf" or r["task"]["task"] != "unknown":
            continue
        if next(g for g in r["technical"] if g["gate"] == "source")["status"] != "PASS":
            continue
        l = json.loads(ll)
        if not l.get("gguf_ctx"):
            continue
        pop.append((key(l["id"]), i, l, c["plan"]))
pop.sort(key=lambda x: x[0])
chosen = pop[: a.n]
with open(snap / "sample" / "frame_gguf.jsonl", "w") as f:
    for _, i, l, plan in chosen:
        f.write(json.dumps({"id": l["id"], "index": i, "stratum": "frame-gguf-untasked", "design_stratum": "frame-gguf-untasked", "pi": len(chosen) / len(pop), "certainty": False, "listing": l, "plan": plan}) + "\n")
d = {"schema": "misaka.palw.hf-census-sample.v1", "seed": a.seed, "key": "BLAKE2b-64(seed || 0 || 'frame-gguf' || 0 || repo_id)", "population": "GGUF repositories with no declared task, a GGUF context length, source PASS on the listing", "N": len(pop), "n": len(chosen)}
(snap / "sample" / "frame_gguf_design.json").write_text(json.dumps(d, indent=1))
print(json.dumps(d))
