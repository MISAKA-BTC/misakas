#!/usr/bin/env python3
"""**D_all: every public model repository at one instant** (RFC-0002 §II.10.1–2), enumerated through the Hub API.

    enumerate_snapshot.py --store ~/Downloads/MISAKA-wt-b/hf-census [--snapshot ID] [--api-share 0.5]

* The snapshot instant `T0` is the UTC second the snapshot was opened (recorded in `MANIFEST.json`). Its id is `T0` spelled
  `YYYY-MM-DDTHHMMSSZ`.
* Pages are `GET /api/models?limit=1000&sort=createdAt&direction=1&expand[]=...` followed by the `Link: rel="next"` cursor, which
  the Hub keys by the repository's ObjectId (`{"_id": {"$gt": ...}}`), so the walk is in creation order and stable. The walk stops
  at the first repository whose ObjectId is at or past `T0`: **D_all is the public repositories whose ObjectId precedes `T0`**,
  each at the commit (`sha`) the page reported. A repository deleted or made private between `T0` and its page is not seen (it is
  not enumerable); a repository updated in that interval is pinned at the newer commit. Both are stated in the manifest.
* Every page is saved as received (decoded JSON bytes, gzip) with its URL, status, rate-limit header, first and last id, count and
  SHA-256, in `pages.log.jsonl`. `state.json` holds the next URL, so a killed run resumes where it stopped and never refetches a
  saved page. The census is reproducible offline from the pages.
"""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import json
import os
import sys
import time
from pathlib import Path
from urllib.parse import urlencode

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hfc_http  # noqa: E402

EXPAND = [
    "author",
    "baseModels",
    "cardData",
    "config",
    "createdAt",
    "disabled",
    "downloads",
    "downloadsAllTime",
    "gated",
    "gguf",
    "lastModified",
    "library_name",
    "likes",
    "pipeline_tag",
    "private",
    "safetensors",
    "sha",
    "siblings",
    "tags",
    "transformersInfo",
]


def first_url() -> str:
    q = [("limit", "1000"), ("sort", "createdAt"), ("direction", "1")] + [("expand[]", e) for e in EXPAND]
    return f"{hfc_http.ENDPOINT}/api/models?{urlencode(q)}"


def next_link(link: str | None) -> str | None:
    if not link:
        return None
    for part in link.split(","):
        seg = part.strip()
        if 'rel="next"' in seg and seg.startswith("<"):
            return seg[1 : seg.index(">")]
    return None


def oid_of_instant(t0: int) -> str:
    return f"{t0:08x}" + "0" * 16


def atomic_write(path: Path, data: bytes) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_bytes(data)
    os.replace(tmp, path)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--store", required=True)
    ap.add_argument("--snapshot", help="resume this snapshot id (default: open a new one now)")
    ap.add_argument("--api-share", type=float, default=0.5, help="the share of the anonymous API quota this walk may use")
    ap.add_argument("--max-pages", type=int, default=0, help="stop after this many pages this run (0: no limit)")
    ap.add_argument("--disk-floor-gb", type=float, default=15.0)
    a = ap.parse_args()

    store = Path(a.store).expanduser()
    if a.snapshot:
        snap = store / "snapshots" / a.snapshot
        man = json.loads((snap / "MANIFEST.json").read_text())
    else:
        t0 = int(time.time())
        sid = dt.datetime.fromtimestamp(t0, dt.UTC).strftime("%Y-%m-%dT%H%M%SZ")
        snap = store / "snapshots" / sid
        (snap / "pages").mkdir(parents=True, exist_ok=False)
        man = {
            "schema": "misaka.palw.hf-census-snapshot.v1",
            "snapshot": sid,
            "t0_unix": t0,
            "t0_utc": dt.datetime.fromtimestamp(t0, dt.UTC).isoformat(),
            "t0_object_id": oid_of_instant(t0),
            "endpoint": hfc_http.ENDPOINT,
            "query": first_url(),
            "expand": EXPAND,
            "user_agent": hfc_http.USER_AGENT,
            "authenticated": False,
            "d_all_rule": "public model repositories listed by /api/models whose ObjectId precedes t0_object_id, each at the sha its page reported",
            "not_seen": "repositories deleted or made private between t0 and their page; private repositories (not enumerable)",
        }
        atomic_write(snap / "MANIFEST.json", json.dumps(man, indent=1).encode())
        atomic_write(snap / "state.json", json.dumps({"next_url": first_url(), "next_page": 0, "done": False}).encode())
    state = json.loads((snap / "state.json").read_text())
    if state.get("done"):
        print(f"{snap.name}: already complete ({state['next_page']} pages)")
        return 0
    boundary = man["t0_object_id"]
    client = hfc_http.Client(api_share=a.api_share)
    log = open(snap / "pages.log.jsonl", "a")
    pages_this_run = 0
    t_start = time.time()
    try:
        while state["next_url"]:
            st = os.statvfs(str(store))
            free_gb = st.f_bavail * st.f_frsize / 2**30
            if free_gb < a.disk_floor_gb:
                print(f"stopping: {free_gb:.1f} GB free is under the floor of {a.disk_floor_gb} GB", file=sys.stderr)
                return 3
            idx = state["next_page"]
            url = state["next_url"]
            r = client.get_api(url)
            if r.status_code != 200:
                print(f"page {idx}: HTTP {r.status_code}: {r.text[:300]}", file=sys.stderr)
                return 2
            body = r.content
            rows = json.loads(body)
            atomic_write(snap / "pages" / f"{idx:06d}.json.gz", gzip.compress(body, compresslevel=6))
            nxt = next_link(r.headers.get("link"))
            past = [m for m in rows if m.get("_id", "") >= boundary]
            entry = {
                "page": idx,
                "url": url,
                "status": r.status_code,
                "fetched_at": dt.datetime.now(dt.UTC).isoformat(),
                "n": len(rows),
                "first_id": rows[0]["_id"] if rows else None,
                "last_id": rows[-1]["_id"] if rows else None,
                "last_created": rows[-1].get("createdAt") if rows else None,
                "past_t0": len(past),
                "sha256": hashlib.sha256(body).hexdigest(),
                "ratelimit": r.headers.get("ratelimit"),
            }
            log.write(json.dumps(entry) + "\n")
            log.flush()
            done = (not rows) or (nxt is None) or bool(past)
            state = {"next_url": None if done else nxt, "next_page": idx + 1, "done": done}
            atomic_write(snap / "state.json", json.dumps(state).encode())
            pages_this_run += 1
            if pages_this_run % 25 == 0:
                el = time.time() - t_start
                print(
                    f"{dt.datetime.now().strftime('%H:%M:%S')} page {idx} n={len(rows)} last={entry['last_created']} "
                    f"rl={entry['ratelimit']} {pages_this_run / el * 60:.1f} pages/min in={client.bytes_in / 2**20:.0f} MiB",
                    flush=True,
                )
            if a.max_pages and pages_this_run >= a.max_pages:
                break
        if state.get("done"):
            man["completed_at"] = dt.datetime.now(dt.UTC).isoformat()
            man["pages"] = state["next_page"]
            atomic_write(snap / "MANIFEST.json", json.dumps(man, indent=1).encode())
            print(f"{snap.name}: complete, {state['next_page']} pages")
    finally:
        log.close()
        client.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
