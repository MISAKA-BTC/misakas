#!/usr/bin/env python3
"""Compare recorded baseline/post checker failures without rerunning or editing them.

Counter comparisons preserve duplicate occurrences; unchanged totals alone are
not a no-regression verdict. Exit 0 means no NEW reported failures, not that the
underlying documentation checkers passed.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def difference(before: Counter, after: Counter) -> dict:
    added, removed = after - before, before - after
    items = lambda counter: [
        {"item": key, "occurrences": count} for key, count in sorted(counter.items())
    ]
    return {
        "baseline_occurrences": sum(before.values()),
        "post_occurrences": sum(after.values()),
        "baseline_unique_items": len(before),
        "post_unique_items": len(after),
        "added_occurrences": sum(added.values()),
        "removed_occurrences": sum(removed.values()),
        "added": items(added),
        "removed": items(removed),
    }


def link_failures(failures: list[str]) -> Counter:
    counts = Counter()
    for value in failures:
        match = re.fullmatch(r"new link issue x(\d+): (.+)", value)
        if match:
            counts[match.group(2)] += int(match.group(1))
    return counts


def rfc_failure_lines(path: Path) -> Counter:
    return Counter(
        line.strip() for line in path.read_text(encoding="utf-8").splitlines()
        if line.startswith("FAIL:")
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, default=HERE / "baseline")
    parser.add_argument("--post", type=Path, default=HERE / "post")
    parser.add_argument("--output", type=Path, default=HERE / "document_check_delta.json")
    args = parser.parse_args()
    baseline, post = args.baseline.resolve(), args.post.resolve()
    before_docs = read_json(baseline / "check_docs.json")
    after_docs = read_json(post / "check_docs.json")
    before_adr = read_json(baseline / "adr_v2_integrity.json")
    after_adr = read_json(post / "adr_v2_integrity.json")
    before_links = link_failures(before_docs["failures"])
    after_links = link_failures(after_docs["failures"])

    comparisons = {
        "docs_failure_entries": difference(
            Counter(before_docs["failures"]), Counter(after_docs["failures"])
        ),
        "docs_new_link_occurrences": difference(before_links, after_links),
        "rfc_v2_failure_lines": difference(
            rfc_failure_lines(baseline / "rfc_v2_integrity.log"),
            rfc_failure_lines(post / "rfc_v2_integrity.log"),
        ),
        "adr_v2_errors": difference(
            Counter(before_adr["errors"]), Counter(after_adr["errors"])
        ),
    }

    count_consistency = {
        "baseline_link_weights_match_summary": (
            sum(before_links.values()) == before_docs["new_issue_occurrences"]
        ),
        "post_link_weights_match_summary": (
            sum(after_links.values()) == after_docs["new_issue_occurrences"]
        ),
    }
    # The three official checker files must still be the same programs as the
    # baseline run. This does not alter historical source-digest ledgers.
    current_root = HERE.parents[4]
    checker_sources = {
        "check_docs": current_root / "docs/maintenance/check_docs.py",
        "rfc_v2_integrity": current_root / "docs/rfc-v2/check_integrity.py",
        "adr_v2_integrity": current_root / "docs/adr-v2/check_integrity.py",
    }
    baseline_summary = read_json(baseline / "summary.json")
    expected_hash = {row["name"]: row["checker_sha256"] for row in baseline_summary["checks"]}
    checker_identity = {}
    for name, path in checker_sources.items():
        actual = sha(path) if path.is_file() else None
        checker_identity[name] = {
            "baseline_sha256": expected_hash.get(name),
            "current_sha256": actual,
            "unchanged": actual == expected_hash.get(name),
        }

    inputs = {}
    for label, directory in [("baseline", baseline), ("post", post)]:
        inputs[label] = {
            filename: {"bytes": (directory / filename).stat().st_size,
                       "sha256": sha(directory / filename)}
            for filename in ["check_docs.json", "rfc_v2_integrity.log", "adr_v2_integrity.json"]
        }
    no_new = (
        all(result["added_occurrences"] == 0 for result in comparisons.values())
        and all(count_consistency.values())
        and all(result["unchanged"] for result in checker_identity.values())
    )
    result = {
        "status": "NO_NEW_REPORTED_FAILURES" if no_new else "REGRESSION_OR_INCONSISTENCY",
        "no_new_reported_failures": no_new,
        "scope": "Multiset comparison of saved official checker outputs; not protocol/runtime verification.",
        "baseline_official_checks_passed": {
            "check_docs": before_docs["passed"],
            "rfc_v2_integrity": not rfc_failure_lines(baseline / "rfc_v2_integrity.log"),
            "adr_v2_integrity": before_adr["passed"],
        },
        "post_official_checks_passed": {
            "check_docs": after_docs["passed"],
            "rfc_v2_integrity": not rfc_failure_lines(post / "rfc_v2_integrity.log"),
            "adr_v2_integrity": after_adr["passed"],
        },
        "comparisons": comparisons,
        "count_consistency": count_consistency,
        "checker_identity": checker_identity,
        "document_metrics": {
            "baseline": {key: before_docs[key] for key in [
                "markdown_files", "known_issue_occurrences", "new_issue_occurrences", "targets"
            ]},
            "post": {key: after_docs[key] for key in [
                "markdown_files", "known_issue_occurrences", "new_issue_occurrences", "targets"
            ]},
        },
        "inputs": inputs,
        "limitations": [
            "Existing failures remain; zero added failures is not an overall PASS.",
            "Link occurrence keys come from reported 'new link issue xN' failure entries. Known-issue summaries are retained as aggregate counts.",
            "The official checker does not validate external URLs, runtime correctness, consensus, or network service.",
            "Rerun the official checks after further document edits, then rerun this comparison.",
        ],
        "script_sha256": sha(Path(__file__).resolve()),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({
        "status": result["status"],
        "added": {key: value["added_occurrences"] for key, value in comparisons.items()},
        "output": str(args.output),
    }, ensure_ascii=False))
    return 0 if no_new else 1


if __name__ == "__main__":
    raise SystemExit(main())
