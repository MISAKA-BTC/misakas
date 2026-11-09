#!/usr/bin/env python3
"""Audit the 2026-11-08 documentation revision against its attached input.

This checks preservation, inventories, registrations, and local-link regressions.
It does not certify protocol semantics, implementation, tests, or activation.
The older RFC/ADR snapshot auditors remain separate and may report differences
that were already present in the attachment. No baseline is rewritten here.

Usage from any directory:
  python3 /path/to/docs/maintenance/revisions/2026-11-08/check_revision.py
  python3 check_revision.py --root /path/to/package --json /path/to/report.json
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import sys


REVISION = "2026-11-08"
REVISION_DIR = f"docs/maintenance/revisions/{REVISION}"
INPUT_MANIFEST = f"{REVISION_DIR}/input-manifest.json"
INPUT_LINKS = f"{REVISION_DIR}/input-link-issues.json"
# Pin the evidence supplied for this particular revision, not a rolling baseline.
INPUT_MANIFEST_SHA256 = "638a24a961cca2b81f5f3542d949ba71ba595acc776d7bd4193aa0e22e21f8b1"
INPUT_LINKS_SHA256 = "525b830422c93122c46e3339b4fffb6c433dbd914efc8d076b768bd7b4e4cf35"
INPUT_ARCHIVE_SHA256 = "7b97e539976711c835995da6f5e49fc4d471517d142eddc57f6653c2100271b2"
ADOPTION_ADR = "docs/adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md"
NUMBERED = re.compile(r"([0-9]{4})-[^/]+\.md\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
FRAGMENT = re.compile(
    r'<a id="(source-v1-\d{4}-L\d{5})"></a>\n\n'
    r'### [^\n]+\n\n出典: [^\n]+\n\n(.*?)'
    r'(?=\n---\n\n<a id="source-v1-|\Z)', re.S,
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def numbered_paths(paths, directory: str) -> set[str]:
    """Only immediate children count; archived subdirectories are not books."""
    return {
        path for path in paths
        if str(PurePosixPath(path).parent) == directory
        and NUMBERED.fullmatch(PurePosixPath(path).name)
    }


class RevisionAudit:
    def __init__(self, root: Path):
        self.root = root
        self.files: dict[str, str] = {}
        self.hashes: dict[str, str] = {}
        self.rfc_registry = {}
        self.adr_registry = {}
        self.source_map = {}
        self.adr_sources = {}
        self.owner_paths: dict[str, str] = {}
        self.baseline_links = []
        self.checks = []
        self.errors: list[str] = []
        self.input_changes = {}
        self.link_scan = {}

    def path(self, relative: str) -> Path:
        p = PurePosixPath(relative)
        if (not isinstance(relative, str) or p.is_absolute()
                or p.as_posix() != relative or ".." in p.parts
                or not p.parts or p.parts[0] != "docs"):
            raise ValueError(f"Invalid package-relative documentation path: {relative!r}")
        return self.root.joinpath(*p.parts)

    def expect(self, condition, message: str):
        if not condition:
            self.errors.append(message)

    def run_check(self, name: str, action):
        start = len(self.errors)
        details = {}
        try:
            details = action() or {}
        except (OSError, ValueError, KeyError, TypeError, AttributeError, ImportError) as exc:
            self.errors.append(f"{type(exc).__name__}: {exc}")
        self.checks.append({
            "name": name,
            "status": "PASS" if len(self.errors) == start else "FAIL",
            "details": details,
            "errors": self.errors[start:],
        })

    def actual_numbered(self, directory: str) -> set[str]:
        return {
            p.relative_to(self.root).as_posix()
            for p in self.path(directory).iterdir()
            if p.is_file() and NUMBERED.fullmatch(p.name)
        }

    def unchanged(self, paths, label: str):
        for relative in sorted(paths):
            self.expect(relative in self.files, f"{label}: absent from input manifest: {relative}")
            self.expect(self.hashes.get(relative) == self.files.get(relative)
                        and relative in self.hashes,
                        f"{label}: input bytes missing or changed: {relative}")

    def check_inputs(self):
        manifest_path = self.path(INPUT_MANIFEST)
        links_path = self.path(INPUT_LINKS)
        self.expect(digest(manifest_path.read_bytes()) == INPUT_MANIFEST_SHA256,
                    "The revision input manifest differs from the pinned supplied evidence.")
        self.expect(digest(links_path.read_bytes()) == INPUT_LINKS_SHA256,
                    "The input-link issue list differs from its original 1540-row evidence.")
        manifest = load_json(manifest_path)
        self.files = manifest["files"]
        self.baseline_links = load_json(links_path)
        self.expect(manifest["input_archive_sha256"] == INPUT_ARCHIVE_SHA256,
                    "Unexpected input archive identity.")
        self.expect(len(self.files) == 1840, "Input inventory must contain 1840 files.")
        self.expect(isinstance(self.baseline_links, list) and len(self.baseline_links) == 1540,
                    "Input-link evidence must contain exactly 1540 issue rows.")
        changed, missing = [], []
        for relative, old_hash in sorted(self.files.items()):
            path = self.path(relative)
            self.expect(isinstance(old_hash, str) and SHA256.fullmatch(old_hash),
                        f"Invalid input SHA-256: {relative}")
            if not path.is_file() or path.is_symlink():
                missing.append(relative)
                self.errors.append(f"Input file removed or replaced by a non-regular file: {relative}")
                continue
            current_hash = digest(path.read_bytes())
            self.hashes[relative] = current_hash
            if current_hash != old_hash:
                changed.append({"path": relative, "input_sha256": old_hash,
                                "current_sha256": current_hash})
        self.input_changes = {
            "note": "General input-file changes are informational; specifically protected files are checked separately.",
            "input_file_count": len(self.files),
            "present_input_file_count": len(self.hashes),
            "unchanged_input_file_count": len(self.hashes) - len(changed),
            "changed_input_file_count": len(changed),
            "missing_input_files": missing,
            "changed_input_files": changed,
        }
        return {k: v for k, v in self.input_changes.items()
                if k not in {"changed_input_files", "note"}}

    def check_registry(self, directory: str, namespace: str, count: int, count_field: str):
        registry = load_json(self.path(f"{directory}/registry.json"))
        documents = registry["documents"]
        expected_ids = {f"{namespace}-{n:04d}" for n in range(1, count + 1)}
        ids = [doc["id"] for doc in documents]
        self.expect(registry[count_field] == count and len(documents) == count,
                    f"{namespace}: registry must contain exactly {count} books.")
        self.expect(set(ids) == expected_ids and len(ids) == len(set(ids)),
                    f"{namespace}: book identifiers must cover the complete sequence exactly once.")
        paths = []
        for doc in documents:
            filename = doc["file"]
            self.expect(bool(NUMBERED.fullmatch(filename)),
                        f"{namespace}: registry file must be a numbered direct child: {filename}")
            self.expect(filename[:4] == doc["id"].rsplit("-", 1)[-1],
                        f"{namespace}: file and ID disagree: {doc['id']} / {filename}")
            relative = f"{directory}/{filename}"
            paths.append(relative)
            path = self.path(relative)
            self.expect(path.is_file(), f"{namespace}: registered book missing: {relative}")
            if path.is_file():
                self.expect(path.read_text(encoding="utf-8").startswith(f"# {doc['id']} — "),
                            f"{namespace}: top-level book identifier differs: {relative}")
        self.expect(Counter(paths) == Counter(self.actual_numbered(directory)),
                    f"{namespace}: actual numbered files and registry differ, or registration is duplicated.")
        return registry, dict(zip(ids, paths))

    def check_inventories(self):
        self.rfc_registry, self.owner_paths = self.check_registry(
            "docs/rfc-v2", "RFC-v2", 20, "active_count")
        self.adr_registry, _ = self.check_registry("docs/adr-v2", "ADR-v2", 50, "current_count")
        legacy_rfc = numbered_paths(self.files, "docs/rfc")
        legacy_adr = numbered_paths(self.files, "docs/adr")
        adr_books = numbered_paths(self.files, "docs/adr-v2")
        for paths, expected, label in ((legacy_rfc, 48, "Legacy RFC"),
                                       (legacy_adr, 210, "Legacy ADR"),
                                       (adr_books, 50, "ADR-v2 book")):
            self.expect(len(paths) == expected, f"{label}: unexpected input inventory count.")
            self.unchanged(paths, label)
        self.expect(self.actual_numbered("docs/rfc") == legacy_rfc,
                    "Legacy RFC inventory must retain exactly the 48 input paths.")
        self.expect(self.actual_numbered("docs/adr") == legacy_adr | {ADOPTION_ADR},
                    "Legacy ADR inventory must be the 210 input paths plus the new ADR-0215.")
        self.expect(self.actual_numbered("docs/adr-v2") == adr_books,
                    "The 50 consolidated ADR book paths must remain unchanged.")
        return {"active_rfc_books": len(self.owner_paths), "legacy_rfc_files": len(legacy_rfc),
                "preserved_legacy_adr_files": len(legacy_adr), "preserved_adr_v2_books": len(adr_books)}

    def check_snapshot_ledgers(self):
        protected = {
            "docs/rfc-v2/source-map.json", "docs/rfc-v2/section-owners.json",
            "docs/adr-v2/source-manifest.json",
            "docs/adr-v2/post-integration-changes.json",
            "docs/adr-v2/post-integration-parameter-changes.json",
            "docs/maintenance/known-link-issues.json",
            "docs/maintenance/doclib.py",
        }
        protected.update(path for path in self.files
                         if path.startswith(("docs/history/documentation/", "docs/adr-v2/validation/")))
        self.unchanged(protected, "Historical ledger / snapshot / link-scanner baseline")
        self.source_map = load_json(self.path("docs/rfc-v2/source-map.json"))
        self.adr_sources = load_json(self.path("docs/adr-v2/source-manifest.json"))
        return {"protected_ledger_and_snapshot_files": len(protected),
                "historical_source_hashes_reinterpreted": False,
                "known_link_baseline_updated": False}

    def check_source_fragments(self):
        sources = self.source_map["sources"]
        self.expect(len(sources) == 41 and {s["old_id"] for s in sources}
                    == {f"RFC-{n:04d}" for n in range(1, 42)},
                    "Historical RFC source map must cover its original 41 IDs.")
        fragments = {}
        for relative in self.owner_paths.values():
            text = self.path(relative).read_text(encoding="utf-8")
            for anchor, body in FRAGMENT.findall(text):
                key = (relative, anchor)
                self.expect(key not in fragments, f"Duplicated source fragment anchor: {key}")
                fragments[key] = digest(body.rstrip().encode("utf-8"))
        expected_keys = []
        for source in sources:
            cursor = 1
            for segment in sorted(source["segments"], key=lambda item: item["start_line"]):
                key = (segment["path"], segment["anchor"])
                expected_keys.append(key)
                self.expect(segment["start_line"] == cursor and segment["end_line"] >= cursor,
                            f"Historical line allocation has a gap or overlap: {source['path']}")
                cursor = segment["end_line"] + 1
                self.expect(self.owner_paths.get(segment["new_id"]) == segment["path"],
                            f"Fragment owner ID / file mismatch: {key}")
                self.expect(fragments.get(key) == segment["fragment_sha256"],
                            f"Historical source fragment missing or changed: {key}")
            self.expect(cursor == source["lines"] + 1,
                        f"Historical source line coverage incomplete: {source['path']}")
        self.expect(len(expected_keys) == 89 and len(set(expected_keys)) == 89,
                    "The immutable source map must register exactly 89 distinct fragments.")
        self.expect(Counter(expected_keys) == Counter(fragments.keys()),
                    "Actual source fragments differ from the 89 registered locations.")
        return {"historical_rfc_sources": len(sources), "registered_fragments": len(expected_keys),
                "actual_fragments": len(fragments)}

    def check_adoption_map(self):
        mapping = load_json(self.path("docs/rfc-v2/adoption-map.json"))
        entries = mapping["sources"]
        legacy = numbered_paths(self.files, "docs/rfc")
        self.expect(mapping["active_rfc_count"] == 20 and mapping["input_legacy_rfc_file_count"] == 48,
                    "Adoption-map inventory metadata must describe 20 books and 48 source files.")
        self.expect(mapping["decision_record"] == ADOPTION_ADR,
                    "Adoption map points to a different decision record.")
        self.expect(Counter(entry["source_path"] for entry in entries) == Counter(legacy),
                    "Adoption map must cover all 48 complete source paths exactly once.")
        historical = {source["path"]: source for source in self.source_map["sources"]}
        primary_owners = {}
        for doc in self.rfc_registry["documents"]:
            for old_id in doc["primary_v1"]:
                self.expect(old_id not in primary_owners, f"Duplicate registered historical owner: {old_id}")
                primary_owners[old_id] = doc["id"]
        self.expect(set(primary_owners) == {s["old_id"] for s in historical.values()},
                    "Registered historical primary ownership is incomplete or contains extra IDs.")
        for entry in entries:
            source = entry["source_path"]
            self.expect(entry["input_sha256"] == self.files.get(source),
                        f"Adoption-map input hash differs: {source}")
            self.expect(entry["input_sha256"] == self.hashes.get(source),
                        f"Adoption-map source bytes differ: {source}")
            self.expect(entry["legacy_id"] == f"RFC-{PurePosixPath(source).name[:4]}",
                        f"Adoption-map legacy ID / full path mismatch: {source}")
            self.expect(self.owner_paths.get(entry["current_owner_id"]) == entry["current_owner_path"],
                        f"Adoption-map current owner is not the registered actual file: {source}")
            self.expect(entry["source_bytes_preserved"] is True,
                        f"Adoption map does not declare preserved original bytes: {source}")
            old = historical.get(source)
            self.expect(entry["historical_snapshot_included"] is (old is not None),
                        f"Adoption-map historical snapshot membership differs: {source}")
            expected_old_owner = old["primary_new_id"] if old else None
            self.expect(entry["historical_owner_id"] == expected_old_owner,
                        f"Adoption-map historical owner differs: {source}")
            if old:
                self.expect(entry["current_owner_id"] == expected_old_owner
                            == primary_owners.get(old["old_id"]),
                            f"Historical primary owner changed in the current mapping: {source}")
            related_ids = entry["related_new_profile_ids"]
            related_paths = entry["related_new_profile_paths"]
            self.expect(len(related_ids) == len(related_paths) == len(set(related_ids)),
                        f"Adoption-map related-owner lists differ or duplicate IDs: {source}")
            for related_id, related_path in zip(related_ids, related_paths):
                self.expect(self.owner_paths.get(related_id) == related_path,
                            f"Adoption-map related owner ID / actual file mismatch: {source}")
        # Check the human-readable ownership table as well as the JSON registry.
        # This is a structural link comparison, not a judgment about design prose.
        table_owners = []
        link = re.compile(r"\[[^\]\n]*\]\(([^\s)]+)\)")
        for line in self.path("docs/rfc-v2/MAPPING.md").read_text(encoding="utf-8").splitlines():
            cells = line.split("|")
            if len(cells) < 4:
                continue
            source_links, owner_links = link.findall(cells[1]), link.findall(cells[2])
            if len(source_links) != 1 or len(owner_links) != 1:
                continue
            source = posixpath.normpath(posixpath.join("docs/rfc-v2", source_links[0]))
            owner = posixpath.normpath(posixpath.join("docs/rfc-v2", owner_links[0]))
            if source in legacy:
                table_owners.append((source, owner))
        self.expect(Counter(table_owners) == Counter(
            (entry["source_path"], entry["current_owner_path"]) for entry in entries),
            "The 48 human-readable MAPPING.md primary owners differ from adoption-map.json.")
        return {"source_entries": len(entries), "distinct_complete_paths": len(legacy),
                "historical_primary_owners": len(primary_owners),
                "human_readable_mapping_rows": len(table_owners)}

    def check_post_consolidation(self):
        old_rfc_paths = {s["path"] for s in self.source_map["sources"]}
        old_adr_paths = {s["path"] for s in self.adr_sources["sources"]}
        post_rfc = numbered_paths(self.files, "docs/rfc") - old_rfc_paths
        post_adr = numbered_paths(self.files, "docs/adr") - old_adr_paths
        self.expect(len(post_rfc) == 7 and len(post_adr) == 6,
                    "Post-consolidation input inventory must contain seven RFCs and six ADRs.")
        self.expect(ADOPTION_ADR not in self.files, "ADR-0215 must be a new adoption record.")
        decision = self.path(ADOPTION_ADR)
        self.expect(decision.is_file() and decision.read_text(encoding="utf-8").startswith("# ADR-0215"),
                    "The new ADR-0215 adoption record is missing or incorrectly identified.")
        expected = Counter()
        for kind, paths in (("rfc", post_rfc), ("adr", post_adr | {ADOPTION_ADR})):
            for path in paths:
                expected[(kind, f"{kind.upper()}-{PurePosixPath(path).name[:4]}", path)] += 1

        def records(registry):
            return Counter((row["kind"], row["id"], row["path"])
                           for row in registry["post_consolidation_documents"])

        self.expect(records(self.rfc_registry) == expected,
                    "RFC registry must register the exact 14 post-consolidation records once each.")
        adr_expected = expected.copy()
        for n in range(16, 21):
            book_id = f"RFC-v2-{n:04d}"
            adr_expected[("rfc", book_id, self.owner_paths[book_id])] += 1
        self.expect(records(self.adr_registry) == adr_expected,
                    "ADR registry must register the same 14 records plus the five adopted RFC-v2 books.")
        for doc in self.rfc_registry["documents"]:
            if doc["id"] in {f"RFC-v2-{n:04d}" for n in range(16, 21)}:
                self.expect(doc["decision_record"] == ADOPTION_ADR,
                            f"New RFC book has a different adoption record: {doc['id']}")
        return {"preserved_post_snapshot_rfcs": len(post_rfc),
                "preserved_post_snapshot_adrs": len(post_adr), "new_adoption_adrs": 1,
                "rfc_registry_post_consolidation_records": sum(expected.values()),
                "adr_registry_post_consolidation_records": sum(adr_expected.values())}

    def check_link_regressions(self):
        # The scanner is itself protected against changes relative to this input.
        spec = importlib.util.spec_from_file_location("revision_doclib", self.path("docs/maintenance/doclib.py"))
        if spec is None or spec.loader is None:
            raise ImportError("Cannot load docs/maintenance/doclib.py")
        doclib = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(doclib)
        self.expect(all(row["status"] in doclib.ERRORS for row in self.baseline_links),
                    "Input-link evidence contains rows outside doclib.ERRORS.")
        before = Counter(doclib.key(row) for row in self.baseline_links)
        rows = doclib.scan(self.root)
        current_issues = [row for row in rows if row["status"] in doclib.ERRORS]
        after = Counter(doclib.key(row) for row in current_issues)
        increased = after - before
        decreased = before - after
        added = []
        for key, extra in sorted(increased.items()):
            added.append({"key": key, "input_count": before[key], "current_count": after[key],
                          "increase": extra,
                          "current_rows": [row for row in current_issues if doclib.key(row) == key]})
        self.link_scan = {
            "scanner": "docs/maintenance/doclib.py: scan, ERRORS, key",
            "input_issue_rows": len(self.baseline_links),
            "current_issue_rows": len(current_issues),
            "input_distinct_issue_keys": len(before),
            "current_distinct_issue_keys": len(after),
            "scanned_link_rows": len(rows),
            "status_counts": dict(sorted(Counter(row["status"] for row in rows).items())),
            "new_or_increased_issue_rows": sum(increased.values()),
            "new_or_increased_issue_keys": len(increased),
            "resolved_or_reduced_issue_rows": sum(decreased.values()),
            "added_or_increased_issues": added,
        }
        for issue in added:
            self.errors.append(f"Local-link regression (+{issue['increase']}): {issue['key']}")
        return {key: value for key, value in self.link_scan.items() if key != "added_or_increased_issues"}

    def run(self):
        for name, action in (
            ("input_evidence_and_no_deletions", self.check_inputs),
            ("numbered_inventories_and_preserved_bodies", self.check_inventories),
            ("immutable_snapshot_ledgers", self.check_snapshot_ledgers),
            ("historical_rfc_fragment_digests", self.check_source_fragments),
            ("complete_adoption_map", self.check_adoption_map),
            ("post_consolidation_registration", self.check_post_consolidation),
            ("local_link_regressions_against_input", self.check_link_regressions),
        ):
            self.run_check(name, action)
        return {
            "schema": "misaka.docs.revision-audit.v1", "revision": REVISION,
            "checked_at_utc": datetime.now(timezone.utc).isoformat(),
            "root": str(self.root), "status": "PASS" if not self.errors else "FAIL",
            "ok": not self.errors, "error_count": len(self.errors), "errors": self.errors,
            "scope": "Documentation preservation, registration, and local-link regression checks only.",
            "limitations": [
                "No semantic keyword search is used to certify the adopted design.",
                "No runtime build, protocol-safety proof, performance test, or activation is checked.",
                "External URLs and links outside the supplied docs package remain unchecked by doclib.",
                "Pre-existing differences from older historical package hashes require the separate RFC/ADR auditors.",
            ],
            "checks": self.checks, "input_changes": self.input_changes, "link_scan": self.link_scan,
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4],
                        help="Package root containing docs/ (default: inferred from this script).")
    parser.add_argument("--json", type=Path, help="Write the complete machine-readable audit report here.")
    args = parser.parse_args()
    sys.dont_write_bytecode = True
    report = RevisionAudit(args.root.resolve()).run()
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{report['status']}: 2026-11-08 documentation revision audit; {report['error_count']} errors.")
    for check in report["checks"]:
        print(f"  {check['status']}: {check['name']}")
        for error in check["errors"]:
            print(f"    {error}")
    print("Scope: documentation integrity only; runtime, protocol safety, and activation are not certified.")
    if args.json:
        print(f"JSON report: {args.json.resolve()}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
