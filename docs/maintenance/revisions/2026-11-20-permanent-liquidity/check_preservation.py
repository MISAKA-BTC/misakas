#!/usr/bin/env python3
"""Check preservation only; do not replace the existing documentation checkers.

Run with Python's standard library:
  python check_preservation.py --base /path/to/base/docs --target /path/to/new/docs

The base must be the previous, independently retained docs snapshot. Target-only
files, including this revision's before/*.md.txt archives, are outside this check.
Existing before archives within earlier revisions ARE compared byte-for-byte.
The expected MOE1 baseline contains exactly 204 ADR-v2 source blocks.
An old check_docs/check_integrity failure is not resolved by this check passing.
Only the report is written; source documents and old checkers are never edited.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import defaultdict
from pathlib import Path


CURRENT_REVISION = "2026-11-20-permanent-liquidity"
EXPECTED_ADR_SOURCE_BLOCKS = 204
OLD_CHECKERS = (
    "maintenance/check_docs.py",
    "rfc-v2/check_integrity.py",
    "adr-v2/check_integrity.py",
)
KNOWN_LINK_BASELINE = "maintenance/known-link-issues.json"
RFC_HEADING = re.compile(
    rb"(?m)^" + re.escape("## 原条文の統合収録".encode()) + rb"[^\r\n]*(?:\r?\n|$)"
)
ADR_MARKER = re.compile(
    rb"(?m)^[ \t]*<!-- ADR_SOURCE_(BEGIN|END):([^ \r\n]+) -->[ \t]*(?:\r?\n|$)"
)
NAMED_MARKER = re.compile(
    rb"(?m)^[ \t]*<!-- ([A-Za-z0-9_-]+)-(BEGIN|END) -->[ \t]*(?:\r?\n|$)"
)
SINGLE_PROTECTED_MARKER = re.compile(
    rb"(?m)^[ \t]*<!-- ([A-Za-z0-9_-]*(?:TB1|FOW1|MOE1)[A-Za-z0-9_-]*) -->[ \t]*(?:\r?\n|$)"
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_regular(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise OSError(f"not a regular file: {path}")
    return path.read_bytes()


def extract_blocks(data: bytes, marker: re.Pattern, adr: bool = False,
                   families=(b"TB1", b"FOW1")):
    """Return keyed, ordered blocks and malformed-marker errors, including markers."""
    blocks = defaultdict(list)
    errors = []
    opened = {}
    for match in marker.finditer(data):
        if adr:
            kind, key = match.group(1), match.group(2)
        else:
            key, kind = match.group(1), match.group(2)
            if not any(family in key for family in families):
                continue
        if kind == b"BEGIN":
            if key in opened:
                errors.append(f"nested BEGIN for {key.decode()}")
            opened[key] = match.start()
        elif key not in opened:
            errors.append(f"END without BEGIN for {key.decode()}")
        else:
            start = opened.pop(key)
            blocks[key.decode()].append(data[start:match.end()])
    errors.extend(f"BEGIN without END for {key.decode()}" for key in opened)
    return blocks, errors


def protected_categories(relative: Path):
    categories = []
    posix = relative.as_posix()
    if set(relative.parts[:-1]) & {"history", "evidence", "audit", "audits"}:
        categories.append("history_evidence_audit_files")
    if relative.parts[:2] == ("maintenance", "revisions"):
        if len(relative.parts) > 2 and relative.parts[2] != CURRENT_REVISION:
            categories.append("previous_revision_files")
    if posix in OLD_CHECKERS:
        categories.append("old_checker_sources")
    normalized = relative.name.lower().replace("_", "-")
    if "source" in normalized and "manifest" in normalized:
        categories.append("source_manifests")
    if posix == KNOWN_LINK_BASELINE:
        categories.append("known_link_baseline")
    return categories


def check(base: Path, target: Path):
    groups = defaultdict(list)
    failures = []
    file_reads = {}
    target_reads = {}

    def target_bytes(relative: Path):
        key = relative.as_posix()
        if key not in target_reads:
            try:
                target_reads[key] = read_regular(target / relative)
            except OSError:
                target_reads[key] = None
        return target_reads[key]

    def record(group, row):
        groups[group].append(row)
        if not row["preserved"]:
            failures.append({"group": group, **row})

    def block_checks(group, relative, original, current, pattern, adr=False,
                     families=(b"TB1", b"FOW1")):
        old, old_errors = extract_blocks(original, pattern, adr, families)
        if not old and not old_errors:
            return
        new, new_errors = extract_blocks(current or b"", pattern, adr, families)
        for origin, errors in (("base", old_errors), ("target", new_errors)):
            for error in errors:
                record(group, {"path": relative.as_posix(), "origin": origin,
                               "error": error, "preserved": False})
        for key, values in sorted(old.items()):
            for index, value in enumerate(values):
                candidates = new.get(key, [])
                found = candidates[index] if index < len(candidates) else None
                record(group, {
                    "path": relative.as_posix(), "marker": key,
                    "occurrence": index + 1, "bytes": len(value),
                    "base_sha256": digest(value),
                    "target_sha256": digest(found) if found is not None else None,
                    "preserved": found == value,
                })

    base_files = sorted(p for p in base.rglob("*") if p.is_file() or p.is_symlink())
    for path in base_files:
        relative = path.relative_to(base)
        try:
            original = read_regular(path)
        except OSError as exc:
            record("base_read_errors", {"path": relative.as_posix(),
                                        "error": str(exc), "preserved": False})
            continue
        file_reads[relative.as_posix()] = len(original)
        categories = protected_categories(relative)
        if categories:
            current = target_bytes(relative)
            row = {
                "path": relative.as_posix(), "bytes": len(original),
                "base_sha256": digest(original),
                "target_sha256": digest(current) if current is not None else None,
                "preserved": current == original,
            }
            for category in categories:
                record(category, dict(row))

        if relative.parts[0] == "rfc-v2" and path.suffix == ".md":
            heading = RFC_HEADING.search(original)
            if heading:
                suffix = original[heading.start():]
                current = target_bytes(relative)
                target_heading = RFC_HEADING.search(current or b"")
                offset = (current or b"").find(suffix)
                preserved = bool(target_heading and offset == target_heading.start())
                record("rfc_v2_original_suffixes", {
                    "path": relative.as_posix(), "bytes": len(suffix),
                    "base_sha256": digest(suffix),
                    "base_offset": heading.start(),
                    "target_offset": offset if offset >= 0 else None,
                    "appended_bytes_after_original_suffix": (
                        len(current) - offset - len(suffix) if preserved else None),
                    "preserved": preserved,
                })

        if relative.parts[0] == "adr-v2" and b"ADR_SOURCE_BEGIN:" in original:
            block_checks("adr_v2_source_blocks", relative, original,
                         target_bytes(relative), ADR_MARKER, adr=True)

        for group_prefix, families in (
            ("tb1_fow1", (b"TB1", b"FOW1")),
            ("moe1", (b"MOE1",)),
        ):
            if not any(family in original for family in families):
                continue
            block_checks(group_prefix + "_blocks", relative, original,
                         target_bytes(relative), NAMED_MARKER,
                         families=families)
            markers = defaultdict(list)
            for match in SINGLE_PROTECTED_MARKER.finditer(original):
                key = match.group(1)
                if (any(family in key for family in families)
                        and not key.endswith((b"-BEGIN", b"-END"))):
                    markers[key.decode()].append(match.group(0))
            if markers:
                current = target_bytes(relative) or b""
                for key, values in sorted(markers.items()):
                    for value in sorted(set(values)):
                        record(group_prefix + "_single_markers", {
                            "path": relative.as_posix(), "marker": key,
                            "base_sha256": digest(value),
                            "base_count": values.count(value),
                            "target_count": current.count(value),
                            "preserved": current.count(value) >= values.count(value),
                        })

    for required in (*OLD_CHECKERS, KNOWN_LINK_BASELINE):
        if required not in file_reads:
            record("required_base_files", {"path": required,
                                          "error": "missing from supplied base",
                                          "preserved": False})

    required_groups = ("rfc_v2_original_suffixes", "adr_v2_source_blocks",
                       "tb1_fow1_blocks", "moe1_blocks", "history_evidence_audit_files",
                       "previous_revision_files", "source_manifests")
    for group in required_groups:
        if not groups[group]:
            record("required_coverage", {"group_name": group,
                                         "error": "no baseline entries found",
                                         "preserved": False})

    actual_adr_blocks = sum("marker" in row
                            for row in groups["adr_v2_source_blocks"])
    if actual_adr_blocks != EXPECTED_ADR_SOURCE_BLOCKS:
        record("required_coverage", {
            "group_name": "adr_v2_source_blocks",
            "expected": EXPECTED_ADR_SOURCE_BLOCKS,
            "actual": actual_adr_blocks,
            "error": "The supplied MOE1 snapshot is not the expected 204-block baseline.",
            "preserved": False,
        })

    summary = {
        name: {"checked": len(rows),
               "passed": sum(row["preserved"] for row in rows),
               "failed": sum(not row["preserved"] for row in rows)}
        for name, rows in sorted(groups.items())
    }
    return {
        "check": "MOE2 independent preservation from MOE1 only", "schema_version": 2,
        "base_docs": str(base), "target_docs": str(target),
        "base_file_count": len(base_files),
        "expected_adr_source_blocks": EXPECTED_ADR_SOURCE_BLOCKS,
        "status": "PASS" if not failures else "FAIL",
        "summary": summary, "failures": failures,
        "scope": {
            "comparison": "Raw bytes; no newline or whitespace normalization.",
            "rfc_rule": "The entire baseline original-source suffix must remain contiguous at the target's original-source heading; new trailing appendices are allowed.",
            "adr_rule": "Every existing keyed ADR_SOURCE block, including marker lines, must retain its bytes and occurrence order for that key.",
            "beacon_rule": "Every existing paired TB1/FOW1 comment block must retain its bytes. Unpaired numbering markers are counted separately; their following mutable index text is not treated as a protected block.",
            "moe1_rule": "Every existing paired MOE1 block must retain its exact bytes and per-key occurrence order. New MOE2 sections may supersede these historical decisions without rewriting their protected records.",
            "archival_rule": "Existing files under any history/evidence/audit/audits directory and previous maintenance/revisions directories are compared whole.",
            "target_only_files": "Ignored, including this revision and before/*.md.txt; archived-original completeness is audited separately.",
        },
        "legacy_checker_status": {
            "execution": "NOT_RUN_BY_THIS_SCRIPT",
            "claim": "This PASS means preservation only. It does not change, clear, or supersede any FAIL from the old documentation or integrity checkers.",
            "limitations": "No link-validity, current-source-manifest acceptance, protocol safety, native implementation, or economic correctness claim is made.",
        },
        "details": dict(sorted(groups.items())),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, type=Path,
                        help="Independent previous docs snapshot")
    parser.add_argument("--target", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", type=Path,
                        default=Path(__file__).with_name("preservation-check.json"))
    args = parser.parse_args()
    base, target = args.base.resolve(), args.target.resolve()
    if not base.is_dir() or not target.is_dir() or base == target:
        parser.error("base and target must be distinct existing docs directories")
    report = check(base, target)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                           encoding="utf-8")
    print(json.dumps({"status": report["status"], "summary": report["summary"],
                      "failure_count": len(report["failures"]),
                      "report": str(args.output.resolve()),
                      "legacy_checkers": "not run; existing FAIL results are unchanged"},
                     ensure_ascii=False))
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
