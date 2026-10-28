#!/usr/bin/env python3
"""**Base references under a repository's old name** (RFC-0002 §II.10.2: "resolve the repository and every referenced component").

    resolve_renames.py --snapshot DIR [--api-share 0.6]          → DIR/renames.json

A card names its base as it was called when the card was written; the Hub moved many repositories since (`gpt2` →
`openai-community/gpt2`, `bert-base-uncased` → `google-bert/bert-base-uncased`, `THUDM/*` → `zai-org/*`) and answers the old
name with a redirect. For every base id the compactor did not find in the snapshot, ask the API once (no redirect followed, no
body kept beyond the `Location`) and record where it points. Resumable: `renames.json` is rewritten every 200 answers, and an id
already answered is not asked again. Metadata only.
"""
import argparse
import gzip
import json
import sys
import time
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hfc_http  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--api-share", type=float, default=0.6)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    out_p = snap / "renames.json"
    done = json.loads(out_p.read_text()) if out_p.exists() else {}
    missing = set()
    with gzip.open(snap / "dall.listing.jsonl.gz", "rt") as f:
        for line in f:
            d = json.loads(line)
            for b in d.get("base_resolved") or []:
                if not b.get("found") and isinstance(b.get("id"), str):
                    missing.add(b["id"])
    todo = sorted(x for x in missing if x not in done)
    print(f"{len(missing)} base ids not in the snapshot, {len(todo)} to ask", flush=True)
    c = hfc_http.Client(api_share=a.api_share)
    t0 = time.time()
    try:
        for i, bid in enumerate(todo):
            if "/" in bid and bid.count("/") > 1 or bid.startswith((".", "/")) or " " in bid or not bid:
                done[bid] = {"status": "not_a_repo_id"}
                continue
            try:
                r = c.get_api(f"{hfc_http.ENDPOINT}/api/models/{quote(bid, safe='/')}")
                if r.status_code in (301, 302, 307, 308):
                    loc = r.headers.get("location", "")
                    new = loc.split("/api/models/", 1)[-1].split("?", 1)[0] if "/api/models/" in loc else None
                    done[bid] = {"status": "renamed", "to": new}
                elif r.status_code == 200:
                    done[bid] = {"status": "exists_not_listed"}
                else:
                    done[bid] = {"status": f"http_{r.status_code}"}
            except hfc_http.FetchError as e:
                done[bid] = {"status": e.kind}
            if (i + 1) % 200 == 0:
                out_p.write_text(json.dumps(done))
                print(f"{i + 1}/{len(todo)} {(i + 1) / (time.time() - t0) * 60:.0f}/min", flush=True)
    finally:
        out_p.write_text(json.dumps(done))
        c.close()
    st = {}
    for v in done.values():
        st[v["status"]] = st.get(v["status"], 0) + 1
    print(json.dumps(st))
    return 0


if __name__ == "__main__":
    sys.exit(main())
