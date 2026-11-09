#!/usr/bin/env python3
"""Audit the EXEC / 1 BPS documentation addition against the prior RFC20 ZIP.

Preserves the complete previous revision, its checkers and recorded results.
Reuses its pinned common preservation, fragment, registry and link algorithms;
overrides only the new input, inventories, mapping and adoption registration.
PASS concerns documents only, never runtime safety, throughput or activation.

Run from any directory, optionally with --root PACKAGE and --json REPORT.
No input baseline or prior report is rewritten by this program.
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


REVISION = "2026-11-08-exec-1bps"
REVISION_DIR = f"docs/maintenance/revisions/{REVISION}"
INPUT_MANIFEST = f"{REVISION_DIR}/input-manifest.json"
INPUT_LINKS = f"{REVISION_DIR}/input-link-issues.json"
INPUT_MANIFEST_SHA256 = "957ac9a413f331bfc2cb297b375cc4b1b885cbbf3d31eaea98eb396eb73a86bd"
INPUT_LINKS_SHA256 = "868d3827fe2c71e9426981d73c3c03782dd65868aa684f983fb7526b2ad4f3f6"
INPUT_ARCHIVE = "misaka_docs_rfc20_20261108.zip"
INPUT_ARCHIVE_SHA256 = "cb8338e04f95fd0087bd39b5cacd2d2aeebaa545670d254dd478a5b116fd6963"
PREVIOUS_DIR = "docs/maintenance/revisions/2026-11-08"
COMMON_CHECKER = f"{PREVIOUS_DIR}/check_revision.py"
COMMON_CHECKER_SHA256 = "17e74faf7ac96d2d71821f3b7f87846f735ad8ca44f0e60317e33980c3fd9126"
OLD_ADOPTION_ADR = "docs/adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md"
NEW_RFC = "docs/rfc/0048-exec-credit-schedule-and-one-bps.md"
NEW_ADR = "docs/adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md"

# These canonical entry digests were derived from files whose complete bytes
# matched the pinned input manifest. They preserve all old row values without
# depending on another checkout or replacing any historical evidence file.
OLD_MAP_INPUT_FILE_SHA256 = "e4057a2b414c0e2d4e2e3c9b40784b64e23912082d43c26f05c8bacdf9be9d1a"
OLD_MAP_ENTRIES_SHA256 = "a07ab289c94a5f82538e47a5092e97e71878bd9c5c1b79527845c59ed6ed6bfa"
OLD_PARAMETER_INPUT_FILE_SHA256 = "ffcf109510ec4f05394a4c4ffc7bd991f1b8f38a5d32a3ffaeccbc430f33ee12"
OLD_PARAMETER_ENTRIES_SHA256 = "dd32a7dad8f0482407e3beb84308a2123cc16a960bf4ee1efd02858e583be3f2"
NEW_PARAMETER_VALUES = {
    "exec_tx_target_bps_v2": "1",
    "exec_tx_slot_period_ms_v2": "1000",
    "exec_tx_canonical_blocks_per_slot_v2": "1",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def entries_digest(entries, identity: str) -> str:
    ordered = sorted(entries, key=lambda item: item[identity])
    encoded = json.dumps(ordered, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return digest(encoded.encode("utf-8"))


def load_common(root: Path):
    path = root / COMMON_CHECKER
    if digest(path.read_bytes()) != COMMON_CHECKER_SHA256:
        raise ValueError("The previous revision checker is missing or changed; refusing to import it.")
    spec = importlib.util.spec_from_file_location("preserved_rfc20_revision_checker", path)
    if spec is None or spec.loader is None:
        raise ImportError("Cannot load the preserved common revision checker.")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def audit_class(common):
    class ExecRevisionAudit(common.RevisionAudit):
        def check_inputs(self):
            manifest_path, links_path = self.path(INPUT_MANIFEST), self.path(INPUT_LINKS)
            self.expect(digest(manifest_path.read_bytes()) == INPUT_MANIFEST_SHA256,
                        "The addition's input manifest differs from the pinned prior ZIP evidence.")
            self.expect(digest(links_path.read_bytes()) == INPUT_LINKS_SHA256,
                        "The addition's 974-row input-link evidence has changed.")
            manifest = common.load_json(manifest_path)
            self.files = manifest["files"]
            self.baseline_links = common.load_json(links_path)
            self.expect(manifest["input_archive"] == INPUT_ARCHIVE
                        and manifest["sha256"] == INPUT_ARCHIVE_SHA256,
                        "Unexpected prior RFC20 ZIP identity.")
            self.expect(len(self.files) == 1867, "The prior ZIP must contain exactly 1867 input files.")
            self.expect(isinstance(self.baseline_links, list) and len(self.baseline_links) == 974,
                        "Input-link evidence must contain exactly 974 issue rows.")
            changed, missing = [], []
            for relative, old_hash in sorted(self.files.items()):
                path = self.path(relative)
                self.expect(isinstance(old_hash, str) and common.SHA256.fullmatch(old_hash),
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
                "note": "General input-file edits are informational; protected bodies and evidence are checked separately.",
                "input_file_count": len(self.files),
                "present_input_file_count": len(self.hashes),
                "unchanged_input_file_count": len(self.hashes) - len(changed),
                "changed_input_file_count": len(changed),
                "missing_input_files": missing,
                "changed_input_files": changed,
            }
            return {key: value for key, value in self.input_changes.items()
                    if key not in {"note", "changed_input_files"}}

        def check_inventories(self):
            self.rfc_registry, self.owner_paths = self.check_registry(
                "docs/rfc-v2", "RFC-v2", 20, "active_count")
            self.adr_registry, _ = self.check_registry("docs/adr-v2", "ADR-v2", 50, "current_count")
            input_rfcs = common.numbered_paths(self.files, "docs/rfc")
            input_adrs = common.numbered_paths(self.files, "docs/adr")
            adr_books = common.numbered_paths(self.files, "docs/adr-v2")
            for paths, expected, label in ((input_rfcs, 48, "Input RFC"),
                                           (input_adrs, 211, "Input ADR including ADR-0215"),
                                           (adr_books, 50, "ADR-v2 book")):
                self.expect(len(paths) == expected, f"{label}: unexpected input count.")
                self.unchanged(paths, label)
            self.expect(OLD_ADOPTION_ADR in input_adrs, "ADR-0215 must belong to the preserved input.")
            self.expect(NEW_RFC not in self.files and NEW_ADR not in self.files,
                        "RFC-0048 and ADR-0216 must be new records, not rewritten input records.")
            self.expect(self.actual_numbered("docs/rfc") == input_rfcs | {NEW_RFC},
                        "Current legacy-number RFC inventory must be exactly 48 inputs plus RFC-0048.")
            self.expect(self.actual_numbered("docs/adr") == input_adrs | {NEW_ADR},
                        "Current legacy-number ADR inventory must be exactly 211 inputs plus ADR-0216.")
            self.expect(self.actual_numbered("docs/adr-v2") == adr_books,
                        "The 50 consolidated ADR paths must remain unchanged.")
            return {"active_rfc_books": len(self.owner_paths), "preserved_input_rfcs": len(input_rfcs),
                    "current_legacy_number_rfcs": len(input_rfcs) + 1,
                    "preserved_input_adrs": len(input_adrs), "current_legacy_number_adrs": len(input_adrs) + 1,
                    "preserved_adr_v2_books": len(adr_books)}

        def check_snapshot_ledgers(self):
            details = super().check_snapshot_ledgers()
            previous = {path for path in self.files if path.startswith(PREVIOUS_DIR + "/")}
            self.expect(len(previous) == 19, "The previous revision must retain all 19 evidence files.")
            self.unchanged(previous, "Previous revision evidence, checkers and recorded results")
            actual = {path.relative_to(self.root).as_posix()
                      for path in self.path(PREVIOUS_DIR).rglob("*") if path.is_file()}
            self.expect(actual == previous,
                        "Do not add, remove or replace files inside the frozen previous revision directory.")
            details["preserved_previous_revision_files"] = len(previous)
            details["previous_revision_directory_inventory_unchanged"] = actual == previous
            return details

        def check_adoption_map(self):
            path = "docs/rfc-v2/adoption-map.json"
            mapping = common.load_json(self.path(path))
            entries = mapping["sources"]
            input_paths = common.numbered_paths(self.files, "docs/rfc")
            expected_paths = input_paths | {NEW_RFC}
            self.expect(self.files.get(path) == OLD_MAP_INPUT_FILE_SHA256,
                        "The pinned old adoption-map entry digest has an unexpected input file identity.")
            self.expect(mapping["schema"] == "misaka.docs.rfc20-adoption-map.v2",
                        "The new-source mapping must explicitly use adoption-map schema v2.")
            self.expect(mapping["active_rfc_count"] == 20
                        and mapping["input_legacy_rfc_file_count"] == 48
                        and mapping["current_legacy_rfc_file_count"] == 49,
                        "Adoption-map metadata must distinguish 48 input sources from 49 current sources.")
            self.expect(Counter(entry["source_path"] for entry in entries) == Counter(expected_paths),
                        "Adoption map must cover the 49 complete source paths exactly once.")
            old_entries = [entry for entry in entries if entry["source_path"] in input_paths]
            self.expect(len(old_entries) == 48 and entries_digest(old_entries, "source_path")
                        == OLD_MAP_ENTRIES_SHA256,
                        "Values of the previous 48 adoption-map rows changed.")
            historical = {source["path"]: source for source in self.source_map["sources"]}
            primary_owners = {}
            for doc in self.rfc_registry["documents"]:
                for old_id in doc["primary_v1"]:
                    self.expect(old_id not in primary_owners, f"Duplicate historical primary owner: {old_id}")
                    primary_owners[old_id] = doc["id"]
            self.expect(set(primary_owners) == {source["old_id"] for source in historical.values()},
                        "The original 41 primary ownership registrations must remain complete.")
            for entry in entries:
                source = entry["source_path"]
                self.expect(entry["legacy_id"] == f"RFC-{PurePosixPath(source).name[:4]}",
                            f"Adoption-map legacy ID / complete path mismatch: {source}")
                self.expect(self.owner_paths.get(entry["current_owner_id"]) == entry["current_owner_path"],
                            f"Adoption-map owner is not the registered actual file: {source}")
                if source in input_paths:
                    self.expect(entry["input_sha256"] == self.files[source] == self.hashes.get(source),
                                f"Preserved source input hash / actual bytes mismatch: {source}")
                    self.expect(entry["source_bytes_preserved"] is True,
                                f"Existing source lost its preserved-bytes declaration: {source}")
                    old = historical.get(source)
                    self.expect(entry["historical_snapshot_included"] is (old is not None),
                                f"Historical snapshot membership differs: {source}")
                    old_owner = old["primary_new_id"] if old else None
                    self.expect(entry["historical_owner_id"] == old_owner,
                                f"Historical owner differs: {source}")
                    if old:
                        self.expect(entry["current_owner_id"] == old_owner
                                    == primary_owners.get(old["old_id"]),
                                    f"Historical primary owner changed: {source}")
                elif source == NEW_RFC:
                    self.expect(entry["input_sha256"] is None and entry["new_adoption_record"] is True,
                                "RFC-0048 must be explicitly new, with input_sha256:null.")
                    self.expect(entry["source_sha256"] == digest(self.path(NEW_RFC).read_bytes()),
                                "RFC-0048's current source digest does not match its actual bytes.")
                    self.expect(entry["current_owner_id"] == "RFC-v2-0007",
                                "RFC-0048's current owner must be RFC-v2-0007.")
                    self.expect(Counter(entry["related_new_profile_ids"])
                                == Counter(f"RFC-v2-{n:04d}" for n in (5, 10, 18, 19, 20)),
                                "RFC-0048 must reference exactly owners 05/10/18/19/20 as related books.")
                related_ids, related_paths = entry["related_new_profile_ids"], entry["related_new_profile_paths"]
                self.expect(len(related_ids) == len(related_paths) == len(set(related_ids)),
                            f"Related owner lists differ or contain duplicates: {source}")
                for book_id, book_path in zip(related_ids, related_paths):
                    self.expect(self.owner_paths.get(book_id) == book_path,
                                f"Related owner ID / actual file mismatch: {source}")

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
                if source in expected_paths:
                    table_owners.append((source, owner))
            self.expect(Counter(table_owners) == Counter(
                (entry["source_path"], entry["current_owner_path"]) for entry in entries),
                "The 49 MAPPING.md primary owners differ from adoption-map.json.")
            return {"source_entries": len(entries), "unchanged_prior_entries": len(old_entries),
                    "new_source_entries": len(entries) - len(old_entries),
                    "historical_primary_owners": len(primary_owners),
                    "human_readable_mapping_rows": len(table_owners)}

        def check_post_consolidation(self):
            historical_rfcs = {source["path"] for source in self.source_map["sources"]}
            historical_adrs = {source["path"] for source in self.adr_sources["sources"]}
            input_rfcs = common.numbered_paths(self.files, "docs/rfc") - historical_rfcs
            input_adrs = common.numbered_paths(self.files, "docs/adr") - historical_adrs
            self.expect(len(input_rfcs) == 7 and len(input_adrs) == 7 and OLD_ADOPTION_ADR in input_adrs,
                        "Input post-consolidation inventory must be seven RFCs and seven ADRs including ADR-0215.")
            for path, identifier in ((NEW_RFC, "RFC-0048"), (NEW_ADR, "ADR-0216")):
                actual = self.path(path)
                self.expect(path not in self.files and actual.is_file()
                            and actual.read_text(encoding="utf-8").startswith(f"# {identifier}"),
                            f"New adoption record missing or incorrectly identified: {path}")
            expected = Counter()
            for kind, paths in (("rfc", input_rfcs | {NEW_RFC}), ("adr", input_adrs | {NEW_ADR})):
                for path in paths:
                    expected[(kind, f"{kind.upper()}-{PurePosixPath(path).name[:4]}", path)] += 1

            def records(registry):
                return Counter((row["kind"], row["id"], row["path"])
                               for row in registry["post_consolidation_documents"])

            self.expect(sum(expected.values()) == 16 and records(self.rfc_registry) == expected,
                        "RFC registry must register exactly eight post-consolidation RFCs and eight ADRs.")
            adr_expected = expected.copy()
            for n in range(16, 21):
                book_id = f"RFC-v2-{n:04d}"
                adr_expected[("rfc", book_id, self.owner_paths[book_id])] += 1
            self.expect(records(self.adr_registry) == adr_expected,
                        "ADR registry must register the same 16 records plus the five adopted RFC-v2 books.")
            for doc in self.rfc_registry["documents"]:
                if doc["id"] in {f"RFC-v2-{n:04d}" for n in range(16, 21)}:
                    self.expect(doc["decision_record"] == OLD_ADOPTION_ADR,
                                f"The original ADR-0215 adoption identity changed: {doc['id']}")
            return {"post_snapshot_rfcs": len(input_rfcs) + 1,
                    "post_snapshot_adrs": len(input_adrs) + 1,
                    "rfc_registry_post_consolidation_records": sum(expected.values()),
                    "adr_registry_post_consolidation_records": sum(adr_expected.values())}

        def check_parameter_additions(self):
            path = "docs/governance/parameters.json"
            self.expect(self.files.get(path) == OLD_PARAMETER_INPUT_FILE_SHA256,
                        "The old parameter entry digest has an unexpected input file identity.")
            entries = common.load_json(self.path(path))["parameters"]
            keys = [entry["key"] for entry in entries]
            self.expect(len(keys) == len(set(keys)) == 55,
                        "Parameter registry must contain 55 distinct entries: 52 inputs plus three additions.")
            previous = [entry for entry in entries if entry["key"] not in NEW_PARAMETER_VALUES]
            added = {entry["key"]: entry for entry in entries if entry["key"] in NEW_PARAMETER_VALUES}
            self.expect(len(previous) == 52 and entries_digest(previous, "key") == OLD_PARAMETER_ENTRIES_SHA256,
                        "One or more values of the original 52 parameter entries changed.")
            self.expect(set(added) == set(NEW_PARAMETER_VALUES),
                        "Only the three explicitly adopted EXEC parameter keys may be added.")
            for key, value in NEW_PARAMETER_VALUES.items():
                if key not in added:
                    continue
                entry = added[key]
                self.expect(entry["baseline"] == value and entry["source"] == NEW_RFC
                            and entry["new_profile_only"] is True and entry["runtime_config_changed"] is False,
                            f"New parameter value, source or documentation-only scope differs: {key}")
            return {"preserved_input_parameter_entries": len(previous),
                    "new_parameter_entries": len(added),
                    "new_parameter_keys": sorted(added), "runtime_configuration_tested": False}

        def run(self):
            for name, action in (
                ("input_evidence_and_no_deletions", self.check_inputs),
                ("numbered_inventories_and_preserved_bodies", self.check_inventories),
                ("immutable_snapshot_ledgers_and_previous_revision", self.check_snapshot_ledgers),
                ("historical_rfc_fragment_digests", self.check_source_fragments),
                ("complete_adoption_map_v2", self.check_adoption_map),
                ("post_consolidation_registration", self.check_post_consolidation),
                ("parameter_additions_only", self.check_parameter_additions),
                ("local_link_regressions_against_input", self.check_link_regressions),
            ):
                self.run_check(name, action)
            return {
                "schema": "misaka.docs.revision-audit.v1", "revision": REVISION,
                "checked_at_utc": datetime.now(timezone.utc).isoformat(), "root": str(self.root),
                "status": "PASS" if not self.errors else "FAIL", "ok": not self.errors,
                "error_count": len(self.errors), "errors": self.errors,
                "scope": "Documentation preservation, registration, parameter-entry preservation and local-link regressions only.",
                "limitations": [
                    "No semantic keyword matching certifies the adopted design.",
                    "No runtime safety, scheduling feasibility, measured 1 BPS, court performance or activation is checked.",
                    "External URLs and targets outside the supplied docs package remain unchecked by doclib.",
                    "Differences from older historical snapshots belong to the separate unmodified auditors.",
                    "The supplied court-analysis text's source-attachment identity is verified separately.",
                ],
                "common_checker": {"path": COMMON_CHECKER, "sha256": COMMON_CHECKER_SHA256,
                                   "previous_revision_written_by_checker": False},
                "checks": self.checks, "input_changes": self.input_changes, "link_scan": self.link_scan,
            }

    return ExecRevisionAudit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4],
                        help="Package root containing docs/; inferred from this script by default.")
    parser.add_argument("--json", type=Path, help="Save the complete machine-readable result here.")
    args = parser.parse_args()
    root = args.root.resolve()
    sys.dont_write_bytecode = True
    input_paths = set()
    try:
        audit = audit_class(load_common(root))(root)
        report = audit.run()
        input_paths = set(audit.files)
    except (OSError, ValueError, ImportError) as exc:
        error = f"{type(exc).__name__}: {exc}"
        report = {"schema": "misaka.docs.revision-audit.v1", "revision": REVISION,
                  "root": str(root), "status": "FAIL", "ok": False, "error_count": 1,
                  "scope": "Documentation integrity only; common-checker bootstrap failed.",
                  "errors": [error], "checks": [{"name": "preserved_common_checker",
                                                "status": "FAIL", "details": {}, "errors": [error]}]}
    if args.json:
        output = args.json.resolve()
        if output == root / PREVIOUS_DIR or root / PREVIOUS_DIR in output.parents:
            parser.error("--json must not overwrite the frozen previous revision directory.")
        protected_paths = input_paths | {INPUT_MANIFEST, INPUT_LINKS, f"{REVISION_DIR}/check_revision.py"}
        if output.is_relative_to(root) and output.relative_to(root).as_posix() in protected_paths:
            parser.error("--json must not overwrite an input source.")
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{report['status']}: EXEC / 1 BPS documentation addition; {report['error_count']} errors.")
    for check in report["checks"]:
        print(f"  {check['status']}: {check['name']}")
        for error in check["errors"]:
            print(f"    {error}")
    print("Scope: documents only; runtime, throughput, court completion and activation are not certified.")
    if args.json:
        print(f"JSON report: {args.json.resolve()}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
