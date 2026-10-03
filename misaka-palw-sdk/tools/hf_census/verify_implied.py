#!/usr/bin/env python3
"""**Check the 2k → primary implication, not assume it** (the lead's guard, 2026-10-03).

    verify_implied.py --snapshot DIR --rows ROWS.jsonl --palw-class BIN --seed SEED [--n 30] [--max-params 4e9] [--threads 2]

A row whose admit gate is `primary_implied` was refused at 2,048 positions on a code whose limit only grows with the context, and its
primary context (min(declared, 8,192)) was not run. This draws a seeded random subset of those rows among the cheap ones (at most
`--max-params` parameters), judges each at its primary context for real (`palw-class census gates --max-context <primary>`), and
reports agreement: the implication holds when the real primary is refused too. Writes `DIR/report/verify_implied.json`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fetch import repo_dir  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--rows", required=True)
    ap.add_argument("--palw-class", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--n", type=int, default=30)
    ap.add_argument("--max-params", type=float, default=4e9)
    ap.add_argument("--threads", type=int, default=2)
    ap.add_argument("--tree", default="unstated")
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    implied = []
    for line in open(a.rows):
        r = json.loads(line)
        cx = r.get("context") or {}
        if cx.get("primary_implied") and (r.get("params") or 0) <= a.max_params:
            implied.append(r)
    key = lambda r: hashlib.blake2b((a.seed + "\0" + r["repo"]).encode(), digest_size=8).hexdigest()  # noqa: E731
    implied.sort(key=key)
    chosen = implied[: a.n]
    by_ctx = defaultdict(list)
    for r in chosen:
        by_ctx[r["context"]["primary"]].append(r)
    results = []
    for ctx, rs in sorted(by_ctx.items()):
        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
            for r in rs:
                f.write(str(repo_dir(snap / "repos", r["repo"])) + "\n")
            dirs = f.name
        out = subprocess.run(
            [a.palw_class, "census", "gates", "--snapshot", snap.name, "--tree", a.tree, "--threads", str(a.threads), "--max-context", str(ctx), "--dirs", dirs],
            capture_output=True,
            text=True,
            check=True,
        )
        real = {json.loads(x)["repo"]: json.loads(x) for x in out.stdout.splitlines() if x.strip()}
        for r in rs:
            rr = real[r["repo"]]
            a_imp = next(g for g in r["technical"] if g["gate"] == "admit")
            a_real = next(g for g in rr["technical"] if g["gate"] == "admit")
            results.append(
                {
                    "repo": r["repo"],
                    "params": r.get("params"),
                    "primary": ctx,
                    "implied": {"status": a_imp["status"], "blocking": a_imp.get("blocking"), "codes": a_imp.get("codes")},
                    "real": {"status": a_real["status"], "blocking": a_real.get("blocking"), "codes": a_real.get("codes")},
                    "agrees": a_real["status"] == "FAIL",
                    "same_blocking": a_real.get("blocking") == a_imp.get("blocking"),
                    "elapsed_ms": rr.get("elapsed_ms"),
                }
            )
    summ = {
        "seed": a.seed,
        "eligible": len(implied),
        "checked": len(results),
        "agree": sum(1 for x in results if x["agrees"]),
        "same_blocking": sum(1 for x in results if x["same_blocking"]),
        "results": results,
    }
    (snap / "report").mkdir(exist_ok=True)
    (snap / "report" / "verify_implied.json").write_text(json.dumps(summ, indent=1))
    print(json.dumps({k: summ[k] for k in ("eligible", "checked", "agree", "same_blocking")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
