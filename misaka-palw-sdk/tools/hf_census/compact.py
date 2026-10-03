#!/usr/bin/env python3
"""**The listing records of D_all**, from a snapshot's saved pages (no network).

    compact.py --snapshot DIR            writes DIR/dall.listing.jsonl.gz and DIR/dall.summary.json

One `misaka_palw_sdk::census::ListingV1` per line: the fields of one `/api/models` entry, renamed, without the chat and processor
templates, and the base references looked up in the same snapshot (`base_resolved`: found, its sha, gated, disabled, license). Only
repositories whose ObjectId precedes the snapshot's `t0_object_id` are D_all; a repository listed twice (a page boundary moved) is
kept once, at its first listing. Field extraction only: every decision is the Rust census's.
"""

from __future__ import annotations

import argparse
import gzip
import json
import sys
from pathlib import Path

DROP_CONFIG_KEYS = {"tokenizer_config", "processor_config", "chat_template", "chat_template_jinja"}


def pages(snap: Path):
    for p in sorted((snap / "pages").glob("*.json.gz")):
        yield from json.loads(gzip.decompress(p.read_bytes()))


def license_of(m: dict) -> tuple[str | None, str | None, str | None]:
    cd = m.get("cardData") or {}
    lic = cd.get("license")
    if isinstance(lic, list):
        lic = lic[0] if lic else None
    if not isinstance(lic, str):
        lic = None
    if lic is None:
        for t in m.get("tags") or []:
            if t.startswith("license:"):
                lic = t[len("license:") :]
                break
    ln = cd.get("license_name") if isinstance(cd.get("license_name"), str) else None
    ll = cd.get("license_link") if isinstance(cd.get("license_link"), str) else None
    return lic, ln, ll


def base_refs(m: dict) -> tuple[str | None, list[str]]:
    bm = m.get("baseModels") or {}
    rel = bm.get("relation") if isinstance(bm, dict) else None
    ids = [x.get("id") for x in (bm.get("models") or [])] if isinstance(bm, dict) else []
    ids = [i for i in ids if isinstance(i, str)]
    if not ids:
        cd = m.get("cardData") or {}
        b = cd.get("base_model")
        if isinstance(b, str):
            ids = [b]
        elif isinstance(b, list):
            ids = [x for x in b if isinstance(x, str)]
    return rel, ids


def peft_base(m: dict) -> str | None:
    p = (m.get("config") or {}).get("peft") or {}
    b = p.get("base_model_name_or_path") if isinstance(p, dict) else None
    return b if isinstance(b, str) else None


def compact_config(c: dict | None) -> dict:
    out = {}
    for k, v in (c or {}).items():
        if k in DROP_CONFIG_KEYS:
            continue
        s = json.dumps(v)
        if len(s) > 4096:
            continue
        out[k] = v
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    man = json.loads((snap / "MANIFEST.json").read_text())
    state = json.loads((snap / "state.json").read_text())
    if not state.get("done"):
        print("the snapshot's enumeration is not complete", file=sys.stderr)
        return 2
    boundary = man["t0_object_id"]

    # Pass 1: the base ids referenced.
    wanted: set[str] = set()
    for m in pages(snap):
        if m.get("_id", "") >= boundary:
            continue
        _, ids = base_refs(m)
        wanted.update(ids)
        b = peft_base(m)
        if b:
            wanted.add(b)
    # Pass 2: their facts in the snapshot.
    facts: dict[str, dict] = {}
    for m in pages(snap):
        if m.get("_id", "") >= boundary or m["id"] not in wanted or m["id"] in facts:
            continue
        lic, _, _ = license_of(m)
        facts[m["id"]] = {
            "id": m["id"],
            "found": True,
            "sha": m.get("sha"),
            "gated": bool(m.get("gated")),
            "disabled": bool(m.get("disabled")),
            "license": lic,
        }
    # Pass 3: the records.
    seen: set[str] = set()
    n = dup = past = 0
    by_gated = {}
    out_path = snap / "dall.listing.jsonl.gz"
    tmp = out_path.with_suffix(".tmp")
    with gzip.open(tmp, "wt", compresslevel=6) as out:
        for m in pages(snap):
            if m.get("_id", "") >= boundary:
                past += 1
                continue
            if m["id"] in seen:
                dup += 1
                continue
            seen.add(m["id"])
            rel, ids = base_refs(m)
            pb = peft_base(m)
            refs = list(dict.fromkeys(ids + ([pb] if pb else [])))
            lic, ln, ll = license_of(m)
            st = m.get("safetensors") or {}
            gg = m.get("gguf") or {}
            rec = {
                "id": m["id"],
                "oid": m.get("_id"),
                "sha": m.get("sha"),
                "created": m.get("createdAt"),
                "modified": m.get("lastModified"),
                "pipeline_tag": m.get("pipeline_tag"),
                "library": m.get("library_name"),
                "gated": m.get("gated", False),
                "disabled": bool(m.get("disabled", False)),
                "private": bool(m.get("private", False)),
                "downloads": int(m.get("downloads") or 0),
                "downloads_all": int(m.get("downloadsAllTime") or 0),
                "likes": int(m.get("likes") or 0),
                "tags": m.get("tags") or [],
                "license": lic,
                "license_name": ln,
                "license_link": ll,
                "base_relation": rel,
                "base_ids": ids,
                "base_resolved": [facts.get(r, {"id": r, "found": False}) for r in refs],
                "config": compact_config(m.get("config")),
                "tinfo": m.get("transformersInfo") or {},
                "st_params": st.get("parameters"),
                "st_total": st.get("total"),
                "gguf_total": gg.get("total"),
                "gguf_arch": gg.get("architecture"),
                "gguf_ctx": gg.get("context_length"),
                "siblings": [s.get("rfilename") for s in (m.get("siblings") or []) if isinstance(s, dict)],
            }
            g = str(rec["gated"])
            by_gated[g] = by_gated.get(g, 0) + 1
            out.write(json.dumps(rec, separators=(",", ":")) + "\n")
            n += 1
    tmp.rename(out_path)
    summ = {"snapshot": man["snapshot"], "d_all": n, "duplicates_dropped": dup, "listed_past_t0": past, "bases_wanted": len(wanted), "bases_found": len(facts), "gated": by_gated}
    (snap / "dall.summary.json").write_text(json.dumps(summ, indent=1))
    print(json.dumps(summ))
    return 0


if __name__ == "__main__":
    sys.exit(main())
