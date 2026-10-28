#!/usr/bin/env python3
"""**RFC-0011 §18 numbers 2 and 3, counted under ADR-0175** (registered models are permanently immutable; HFX, 2026-10-10).

The census's numerators are counts of evidence, never of verdicts: a repository counts toward number 2 only through an on-chain
registration of its complete advertised task at its complete advertised context (§18's evidence standard), and toward number 3 only
when that registration's class reached mining and a funded Final. This module reads the registrations as records and counts them
the way ADR-0175 says a registration relates to content:

* **a registration is its content.** Weights, graph, tokenizer, spec, canonical artifact root and the registration id never change;
  `model_registration_id_v1 = H64("misaka-palw/model-registration/id/v1", class_id || canonical_artifact_root || founder || name)`
  binds the root. A repository counts only through a record of ITS SNAPSHOT REVISION (the listing's `sha`): a record of another
  revision registered other content, and is excluded with that reason — it never "updates" the repository's count, in either
  direction (a newer upload is not covered by an older registration, and a registration of a newer upload does not cover the
  snapshot's);
* **an improvement is a new registration** (`CandidateSelected`): a record that names a parent counts for its own repository and
  revision only, never for the parent's;
* **one repository revision counts once**, however many records register it (two packagings, two contexts): the extra records are
  reported as `duplicate_records`;
* a record that is not the full task or not the full context (a short-context or text-only registration), or is not on the public
  chain, is excluded with every one of its reasons and never enters number 2. Registration is not availability: no record field reads whether
  the model is served or seeded (ADR-0177).

A record (one JSON object per line):

    {"repo": "org/name", "revision": "<sha>", "class_id": "<hex>", "canonical_artifact_root": "<hex>",
     "model_registration_id": "<hex>" | null, "parent_registration_id": "<hex>" | null,
     "network": "...", "public_chain": bool, "full_task": bool, "full_context": bool,
     "registered_positions": int, "declared_positions": int, "mined_funded_final": bool, "evidence": "..."}

Usage: imported by `coverage_report.py` (`--registrations FILE`); `registrations.py --self-test` runs the rule's own cases.
"""

from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

REQUIRED = ("repo", "revision", "class_id", "canonical_artifact_root", "full_task", "full_context", "public_chain", "mined_funded_final")


def load(path: str | Path) -> list[dict]:
    out = []
    for n, line in enumerate(open(path), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        r = json.loads(line)
        missing = [k for k in REQUIRED if k not in r]
        if missing:
            raise SystemExit(f"{path}:{n}: a registration record lacks {missing}")
        out.append(r)
    return out


def snapshot_revisions(listing_gz: str | Path, repos: set[str]) -> dict[str, str]:
    """The snapshot's revision (`sha`) of each repository named, from the census listing (streamed; only the named ones parsed)."""
    want = {f'"id":{json.dumps(r)}' for r in repos}
    out: dict[str, str] = {}
    if not repos:
        return out
    with gzip.open(listing_gz, "rt") as fh:
        for line in fh:
            if not any(w in line for w in want):
                continue
            row = json.loads(line)
            if row.get("id") in repos:
                out[row["id"]] = row.get("sha") or ""
                if len(out) == len(repos):
                    break
    return out


def count(records: list[dict], revisions: dict[str, str]) -> dict:
    """Numbers 2 and 3's numerators under ADR-0175, and every record that does not enter them with all of its reasons."""
    counted: dict[str, dict] = {}
    excluded: list[dict] = []
    duplicates: list[str] = []
    for r in records:
        key = f'{r["repo"]}@{r["revision"][:8]}'
        snap = revisions.get(r["repo"])
        why = []
        if snap is None:
            why.append("not in the snapshot")
        elif r["revision"] != snap:
            why.append(f"another revision (the snapshot's is {snap[:8]}): other content, never an update")
        if not r["public_chain"]:
            why.append(f'not on the public chain ({r.get("network", "unnamed")})')
        if not r["full_task"]:
            why.append("not the full task")
        if not r["full_context"]:
            why.append(f'context {r.get("registered_positions")} of {r.get("declared_positions")}')
        if why:
            excluded.append({"record": key, "reasons": why})
            continue
        if r["repo"] in counted:
            duplicates.append(key)
            counted[r["repo"]]["mined_funded_final"] |= bool(r["mined_funded_final"])
            continue
        counted[r["repo"]] = {"revision": r["revision"], "mined_funded_final": bool(r["mined_funded_final"])}
    return {
        "rule": "ADR-0175: a registration covers exactly the content it binds (model_registration_id_v1 binds the canonical artifact root); "
        "a repository counts through a record of its snapshot revision only, once, on the public chain, at its full task and full "
        "context; another revision's record and a child's record never count for it",
        "registered_full_task": len(counted),
        "mined_funded_final": sum(1 for v in counted.values() if v["mined_funded_final"]),
        "records": len(records),
        "excluded": excluded,
        "duplicate_records": duplicates,
    }


def self_test() -> None:
    base = {"class_id": "c", "canonical_artifact_root": "a", "full_task": True, "full_context": True, "public_chain": True, "mined_funded_final": False}
    rev = {"o/m": "1111111111", "o/n": "2222222222", "o/child": "3333333333"}
    rec = lambda **kw: {**base, **kw}  # noqa: E731
    reasons = lambda c: [r for e in c["excluded"] for r in e["reasons"]]  # noqa: E731
    # A registration of the snapshot revision counts; the same revision twice counts once.
    c = count([rec(repo="o/m", revision="1111111111"), rec(repo="o/m", revision="1111111111", canonical_artifact_root="b")], rev)
    assert c["registered_full_task"] == 1 and len(c["duplicate_records"]) == 1, c
    # Another revision's registration is other content: it counts for nothing, and is named.
    c = count([rec(repo="o/m", revision="0000000000")], rev)
    assert c["registered_full_task"] == 0 and any("another revision" in r for r in reasons(c)), c
    # A child registration (a fine-tune registered on its own) counts for its own repository only.
    c = count([rec(repo="o/child", revision="3333333333", parent_registration_id="p")], rev)
    assert c["registered_full_task"] == 1, c
    # Short context, not the full task, a private devnet, a repository outside the snapshot: excluded, every reason named.
    c = count(
        [
            rec(repo="o/n", revision="2222222222", full_context=False, registered_positions=512, declared_positions=32768, public_chain=False,
                network="a private devnet"),
            rec(repo="o/n", revision="2222222222", full_task=False),
            rec(repo="o/zzz", revision="9"),
        ],
        rev,
    )
    assert c["registered_full_task"] == 0 and len(c["excluded"]) == 3, c
    assert c["excluded"][0]["reasons"] == ["not on the public chain (a private devnet)", "context 512 of 32768"], c
    # Number 3 is a subset of number 2.
    c = count([rec(repo="o/m", revision="1111111111", mined_funded_final=True), rec(repo="o/n", revision="2222222222")], rev)
    assert (c["registered_full_task"], c["mined_funded_final"]) == (2, 1), c
    print("registrations.py self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        sys.exit(__doc__)
