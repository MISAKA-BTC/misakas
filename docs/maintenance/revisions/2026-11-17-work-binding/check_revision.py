#!/usr/bin/env python3
"""Check SWC1 input preservation and integration, not runtime or PoW safety.

The factual source snapshot is pinned here. Official legacy checker baselines
are never edited. The uploaded package already failed its historical checks;
those failures remain explicit in baseline-checks.json and in this report.
Only revision-results.json is written, unless --stdout-only is supplied.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import sys

sys.dont_write_bytecode = True

REV = "docs/maintenance/revisions/2026-11-17-work-binding"
BASELINE = f"{REV}/source-baseline.json"
BASELINE_SHA256 = "19f63c1fcbbd9fc0a9f503dd962a59c4c64bd1746e55c3a2b35c2bf405431b42"
BASELINE_CHECKS = f"{REV}/baseline-checks.json"
BASELINE_CHECKS_SHA256 = "d0658df6aed4de59c007a482018446355e4105e5d93c1bc00b2138311cdb82c5"
ARCHIVE_SHA256 = "4d6ee065a148aa2212dd650830bad79cba6894c9b849e314bfd56d9af50d7e40"
PROPOSED = "PROPOSED_DESIGN_NOT_ACTIVATED"
INHERITED_ADOPTED = "ADOPTED_DESIGN_NOT_ACTIVATED"
RFC_PATHS = {
    55: "docs/rfc/0055-global-admission-court-calendar-and-one-bps.md",
    56: "docs/rfc/0056-global-claim-admission-difficulty.md",
    57: "docs/rfc/0057-court-nonce-and-panel-challenges.md",
    58: "docs/rfc/0058-staged-work-commitments-and-consensus-eligibility.md",
}
ADR_PATHS = {
    223: "docs/adr/0223-global-admission-court-calendar-and-one-bps.md",
    224: "docs/adr/0224-global-claim-admission-difficulty.md",
    225: "docs/adr/0225-court-nonce-and-panel-challenges.md",
    226: "docs/adr/0226-staged-work-commitments-and-consensus-eligibility.md",
}
REGISTRIES = {
    "docs/rfc-v2/registry.json": ("active_count", 20, "docs/rfc-v2", "RFC-v2"),
    "docs/adr-v2/registry.json": ("current_count", 50, "docs/adr-v2", "ADR-v2"),
}
PARAMETERS = "docs/governance/parameters.json"
NUMBER_REGISTRY = "docs/maintenance/number-registry.json"
FULL_AMENDMENTS = {RFC_PATHS[56], RFC_PATHS[57]}
APPEND_AMENDMENTS = {ADR_PATHS[224], ADR_PATHS[225]}
MARKED_POINTER_FILES = {
    "docs/PRINCIPLES.md", "docs/README.md", "docs/STATUS.md",
    "docs/rfc/README.md", "docs/adr/README.md",
    "docs/rfc-v2/README.md", "docs/rfc-v2/MIGRATION.md", "docs/adr-v2/README.md",
    "docs/governance/parameter-register.md", "docs/maintenance/numbering.md",
    "docs/design/palw/integration/public-one-honest-implementation-ledger.md",
    "docs/architecture/palw-specification.md",
}
GENERATED_EDITABLE = {
    "docs/rfc/INDEX.md", "docs/rfc/TOPICS.md", "docs/adr/INDEX.md", "docs/adr/TOPICS.md",
    "docs/maintenance/catalog.md", "docs/maintenance/decision-catalog.json",
    "docs/maintenance/generate_indexes.py",
}
NEW_DOCUMENTS = {
    RFC_PATHS[58], ADR_PATHS[226],
    "docs/design/palw/integration/staged-work-implementation-plan-2026-11-17.md",
}
MODEL_SCRIPT = f"{REV}/check_work_model.py"
MODEL_RESULT = f"{REV}/model-results.json"
MARKER = re.compile(rb"(?m)^<!-- ((?:CAD1|CPN1|SWC1)-[A-Z0-9_-]+)-BEGIN -->\r?$")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def valid_path(value: object) -> bool:
    if not isinstance(value, str) or "\\" in value:
        return False
    path = PurePosixPath(value)
    return (value.startswith("docs/") and not path.is_absolute()
            and ".." not in path.parts and "." not in path.parts
            and path.as_posix() == value)


def restore_marked(data: bytes, expected: str) -> bool:
    """Remove only added named blocks and adjacent insertion blank lines.

    Input sources contain no CAD1/CPN1/SWC1 markers. Hash matching requires
    every original byte, including historical appendices, to be restored.
    No global whitespace normalization or original baseline rewrite is used.
    """
    tags = MARKER.findall(data)
    if not tags or len(tags) > 8 or len(set(tags)) != len(tags):
        return False
    candidates = {data}
    for tag in tags:
        updated = set()
        begin = b"<!-- " + tag + b"-BEGIN -->"
        end = b"<!-- " + tag + b"-END -->"
        for candidate in candidates:
            lines = candidate.splitlines(keepends=True)
            starts = [i for i, line in enumerate(lines) if line.rstrip(b"\r\n") == begin]
            ends = [i for i, line in enumerate(lines) if line.rstrip(b"\r\n") == end]
            if len(starts) != 1 or len(ends) != 1 or starts[0] >= ends[0]:
                continue
            first, last = starts[0], ends[0]
            if any(MARKER.search(line) for line in lines[first + 1:last]):
                continue
            before, after = lines[:first], lines[last + 1:]
            for left in (0, 1):
                if left and (not before or before[-1] not in (b"\n", b"\r\n")):
                    continue
                for right in (0, 1):
                    if right and (not after or after[0] not in (b"\n", b"\r\n")):
                        continue
                    updated.add(b"".join(before[:-left] if left else before) + b"".join(after[right:]))
        if len(updated) > 4096:
            return False
        candidates = updated
    return any(sha(candidate) == expected for candidate in candidates)


class Audit:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.errors: list[str] = []
        self.stats: dict = {}
        self.baseline: dict = {}
        self.files: dict = {}
        self.changed: set[str] = set()
        self.added: set[str] = set()
        self.legacy: dict = {}

    def require(self, condition: object, message: str) -> None:
        if not condition:
            self.errors.append(message)

    def read(self, relative: str) -> bytes:
        if not valid_path(relative):
            raise ValueError(f"invalid documentation path: {relative}")
        target = self.root / relative
        if target.is_symlink() or not target.resolve().is_relative_to(self.root):
            raise ValueError(f"symlink or escaping path: {relative}")
        return target.read_bytes()

    def load(self, relative: str) -> dict:
        return json.loads(self.read(relative), object_pairs_hook=unique_object)

    def source_snapshot(self) -> None:
        data = self.read(BASELINE)
        if sha(data) != BASELINE_SHA256:
            raise ValueError("source snapshot bytes changed; do not regenerate the pinned snapshot")
        self.baseline = json.loads(data, object_pairs_hook=unique_object)
        records = self.baseline["files"]
        self.files = {row["path"]: row for row in records}
        self.require(len(records) == len(self.files) == 2527, "source snapshot must have 2,527 unique files")
        self.require(all(valid_path(p) for p in self.files), "invalid path in source snapshot")
        origins = Counter(row["origin"] for row in records)
        self.require(origins == {"uploaded_docs": 2483, "previous_only_import": 44}, "source provenance counts differ")
        self.require(self.baseline["source_archive"]["sha256"] == ARCHIVE_SHA256,
                     "uploaded archive hash differs")
        self.require(set(self.baseline["previous_only_import"]["paths"]) ==
                     {p for p, row in self.files.items() if row["origin"] == "previous_only_import"},
                     "previous-only import inventory differs")
        actual = set()
        for target in (self.root / "docs").rglob("*"):
            if target.is_symlink():
                self.errors.append(f"symlink in documentation package: {target.relative_to(self.root)}")
            elif target.is_file():
                actual.add(target.relative_to(self.root).as_posix())
        self.added = actual - set(self.files)
        for path in sorted(self.added):
            self.require(path in NEW_DOCUMENTS or path.startswith(REV + "/"),
                         f"unlisted new documentation path: {path}")
        missing = set(self.files) - actual
        for path in sorted(missing):
            self.errors.append(f"source file missing: {path}")
        exact, marked, appended = 0, 0, 0
        for path, row in sorted(self.files.items()):
            if path in missing:
                continue
            data = self.read(path)
            if sha(data) == row["sha256"] and len(data) == row["size"]:
                exact += 1
                continue
            self.changed.add(path)
            if path in FULL_AMENDMENTS:
                self.require(b"SWC1" in data and b"0058" in data and PROPOSED.encode() in data,
                             f"revised proposal lacks SWC1/version/status boundary: {path}")
            elif path in APPEND_AMENDMENTS:
                preserved = (restore_marked(data, row["sha256"])
                             and data.count(b"<!-- SWC1-AMENDMENT-BEGIN -->") == 1
                             and data.count(b"<!-- SWC1-AMENDMENT-END -->") == 1)
                self.require(preserved, f"ADR amendment does not preserve its complete input bytes: {path}")
                appended += int(preserved)
            elif path in MARKED_POINTER_FILES or re.fullmatch(r"docs/(rfc|adr)-v2/\d{4}-.+\.md", path):
                preserved = restore_marked(data, row["sha256"])
                self.require(preserved, f"proposal blocks do not restore the exact uploaded bytes: {path}")
                marked += int(preserved)
            elif path not in GENERATED_EDITABLE and path not in REGISTRIES and path not in {PARAMETERS, NUMBER_REGISTRY}:
                self.errors.append(f"immutable input/history/evidence changed: {path}")
        for path in (RFC_PATHS[55], ADR_PATHS[223]):
            self.require(path not in self.changed and path not in missing, f"protected RFC-0055/ADR-0223 changed: {path}")
        old_revisions = ("2026-11-17-global-admission-one-bps", "2026-11-17-admission-difficulty", "2026-11-17-court-nonce")
        protected = [p for p in self.files if any(p.startswith(f"docs/maintenance/revisions/{name}/") for name in old_revisions)]
        self.require(all(p not in self.changed and p not in missing for p in protected),
                     "one of the three inherited revision records was modified")
        self.stats.update({"uploaded_files": 2483, "previous_only_import_files": 44,
                           "source_files": len(records), "current_files": len(actual),
                           "exact_source_files": exact, "source_files_with_only_marked_additions": marked,
                           "adrs_with_only_marked_amendments": appended, "changed_source_files": sorted(self.changed),
                           "added_files": sorted(self.added), "deleted_source_files": len(missing),
                           "protected_inherited_revision_files": len(protected)})
        for number in (56, 57):
            prior = f"{REV}/prior-rfc-{number:04d}.source.txt"
            original = self.files[RFC_PATHS[number]]
            saved = self.read(prior)
            self.require(sha(saved) == original["sha256"] and len(saved) == original["size"],
                         f"original proposal preimage was not preserved: {prior}")

    def registries(self) -> None:
        required = {f"RFC-{n:04d}": ("rfc", path) for n, path in RFC_PATHS.items()}
        required.update({f"ADR-{n:04d}": ("adr", path) for n, path in ADR_PATHS.items()})
        status_of = lambda identifier: INHERITED_ADOPTED if identifier in {"RFC-0055", "ADR-0223"} else PROPOSED
        for identifier, (kind, path) in required.items():
            matches = list((self.root / "docs" / kind).glob(identifier[-4:] + "-*.md"))
            self.require(len(matches) == 1 and matches[0].relative_to(self.root).as_posix() == path,
                         f"missing or duplicate numbered proposal: {identifier}")
            self.require(status_of(identifier) in self.read(path).decode("utf-8"), f"inherited/proposed status missing: {identifier}")
        for path, (count_field, expected, directory, namespace) in REGISTRIES.items():
            data = self.load(path)
            old = self.baseline["structured_preimages"][path]
            docs = data["documents"]
            actual = {p.name for p in (self.root / directory).glob("[0-9][0-9][0-9][0-9]-*.md")}
            expected_ids = {f"{namespace}-{i:04d}" for i in range(1, expected + 1)}
            self.require(data[count_field] == len(docs) == len(actual) == expected,
                         f"current-book count changed: {path}")
            self.require({row["file"] for row in docs} == actual and {row["id"] for row in docs} == expected_ids,
                         f"current-book identities differ: {path}")
            self.require(docs == old["documents"], f"historical current-book registry rows changed: {path}")
            post = data.get("post_consolidation_documents", [])
            old_rows = Counter(canonical(row) for row in old.get("post_consolidation_documents", []))
            self.require(not old_rows - Counter(canonical(row) for row in post),
                         f"existing post-consolidation records changed: {path}")
            for identifier, (kind, target) in required.items():
                rows = [row for row in post if row.get("id") == identifier]
                self.require(len(rows) == 1 and rows[0].get("kind") == kind
                             and rows[0].get("path") == target and rows[0].get("status") == status_of(identifier),
                             f"missing/duplicate/unadopted-status mismatch: {path}: {identifier}")
            for key, value in old.items():
                if key not in {"documents", "post_consolidation_documents", "latest_revision", "current_revision", "editorial_date"}:
                    self.require(data.get(key) == value, f"historical registry metadata changed: {path}: {key}")
            self.stats["current_rfc_books" if namespace == "RFC-v2" else "current_adr_books"] = expected
        catalog = self.load("docs/maintenance/decision-catalog.json")["records"]
        for identifier, (kind, path) in required.items():
            rows = [row for row in catalog if row.get("kind") == kind and row.get("number") == identifier[-4:]]
            self.require(len(rows) == 1 and rows[0].get("path") == path,
                         f"catalog registration missing or duplicated: {identifier}")
        numbers = self.load(NUMBER_REGISTRY)
        old = self.baseline["structured_preimages"][NUMBER_REGISTRY]
        old_rows = old.get("subsequent_local_allocations", [])
        rows = numbers.get("subsequent_local_allocations", [])
        self.require(rows[:len(old_rows)] == old_rows, "old numbering allocations changed")
        for key, value in old.items():
            if key not in {"next_local", "subsequent_local_allocations"}:
                self.require(numbers.get(key) == value, f"historical number-registry field changed: {key}")
        for rn, an in zip(range(55, 59), range(223, 227)):
            found = [row for row in rows if str(row.get("rfc")) == f"{rn:04d}" or str(row.get("adr")) == f"{an:04d}"]
            self.require(len(found) == 1 and found[0].get("rfc_path") == RFC_PATHS[rn]
                         and found[0].get("adr_path") == ADR_PATHS[an] and found[0].get("status") == status_of(f"RFC-{rn:04d}"),
                         f"number allocation missing/duplicated/incorrect: RFC-{rn:04d}/ADR-{an:04d}")
        params = self.load(PARAMETERS)
        old = self.baseline["structured_preimages"][PARAMETERS]
        keys = [row["key"] for row in params["parameters"]]
        self.require(len(keys) == len(set(keys)), "duplicate parameter key")
        current = {row["key"]: row for row in params["parameters"]}
        for row in old["parameters"]:
            self.require(current.get(row["key"]) == row, f"existing parameter changed: {row['key']}")
        for key, value in old.items():
            if key != "parameters":
                self.require(params.get(key) == value, f"parameter metadata changed: {key}")
        self.stats["preserved_input_parameters"] = len(old["parameters"])

    def links(self) -> None:
        parser = "docs/maintenance/doclib.py"
        if sha(self.read(parser)) != self.baseline["link_snapshot"]["parser_sha256"]:
            raise ValueError("the input-pinned Markdown parser changed")
        spec = importlib.util.spec_from_file_location("swc1_revision_doclib", self.root / parser)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        rows = module.scan(self.root)
        errors = Counter(module.key(row) for row in rows if row["status"] in module.ERRORS)
        old = Counter({row["key"]: row["count"] for row in self.baseline["link_snapshot"]["error_keys"]})
        added = errors - old
        for key, count in sorted(added.items()):
            self.errors.append(f"new local link issue x{count}: {key}")
        modified_md = {path for path in self.changed | self.added if path.endswith(".md")}
        self.stats["links"] = {
            "scope": "offline docs-local targets and Markdown fragments; compared with factual combined input",
            "changed_or_added_markdown_files": len(modified_md),
            "input_error_occurrences": sum(old.values()),
            "current_error_occurrences": sum(errors.values()),
            "new_error_occurrences": sum(added.values()),
            "resolved_input_error_occurrences": sum((old - errors).values()),
            "remaining_input_error_occurrences": sum((old & errors).values()),
            "status_counts": dict(sorted(Counter(row["status"] for row in rows).items())),
            "new_issues": [{"key": key, "count": count} for key, count in sorted(added.items())],
        }

    def model_result(self) -> None:
        result = self.load(MODEL_RESULT)
        expected = {path: sha(self.read(path)) for path in (RFC_PATHS[56], RFC_PATHS[57], RFC_PATHS[58])}
        self.require(result.get("source_hashes") == expected, "abstract model result is not bound to the current RFC-0056/57/58 bytes")
        self.require(result.get("model_script_sha256") == sha(self.read(MODEL_SCRIPT)), "abstract model script hash is stale")
        self.require(result.get("scope") == "abstract-model-only", "abstract model scope is not explicit")
        self.require(result.get("sources_unchanged_during_run") is True, "model input changed during its run")
        tests = result.get("results", [])
        count = result.get("case_count")
        self.require(type(count) is int and count > 0 and count == len(tests)
                     and result.get("cases_passed") == count and result.get("passed") is True
                     and all(row.get("passed") is True for row in tests), "abstract model cases are incomplete or failed")
        self.require(len({row.get("id") for row in tests}) == len(tests), "duplicate abstract model case id")
        self.require(bool(result.get("not_verified")), "abstract model limitations are missing")
        self.stats["abstract_model"] = {"scope": result.get("scope"), "case_count": count,
                                        "passed": result.get("passed"), "source_hashes": expected,
                                        "model_script_sha256": sha(self.read(MODEL_SCRIPT))}

    def legacy_reports(self) -> None:
        data = self.read(BASELINE_CHECKS)
        if sha(data) != BASELINE_CHECKS_SHA256:
            raise ValueError("input legacy-check report snapshot changed")
        report = json.loads(data, object_pairs_hook=unique_object)
        self.require(report.get("input_archive_sha256") == ARCHIVE_SHA256
                     and report.get("uploaded_files") == 2483 and report.get("previous_only_import_files") == 44,
                     "legacy report input provenance differs")
        checks = report.get("checks", [])
        self.require(report.get("all_reported_passed") is False and len(checks) == 3
                     and all(row.get("reported_passed") is False for row in checks),
                     "input legacy failures were reclassified as passing")
        for row in checks:
            raw = row["verbatim_log"].encode("utf-8")
            self.require(sha(raw) == row["log_sha256"] and len(raw) == row["log_bytes"],
                         f"embedded legacy log hash differs: {row.get('id')}")
        self.legacy = {
            "input_all_reported_passed": False,
            "input_checks": [{"id": row["id"], "reported_passed": row["reported_passed"],
                              "log_sha256": row["log_sha256"]} for row in checks],
            "current_official_checks": "NOT_RUN_BY_THIS_CHECKER; run and report them separately",
            "baseline_rewritten": False,
        }

    def run(self) -> dict:
        try:
            self.source_snapshot()
        except Exception as exc:
            self.errors.append(f"source snapshot check failed: {type(exc).__name__}: {exc}")
        if self.files:
            for name, operation in (("registries", self.registries), ("links", self.links),
                                    ("model result", self.model_result), ("legacy reports", self.legacy_reports)):
                try:
                    operation()
                except Exception as exc:
                    self.errors.append(f"{name} check failed: {type(exc).__name__}: {exc}")
        return {
            "schema": "misaka-swc1-input-preservation-and-integration-v1",
            "scope": "INPUT-PRESERVATION-AND-INTEGRATION-ONLY",
            "checked_at_utc": datetime.now(timezone.utc).isoformat(),
            "passed": not self.errors, "integration_passed": not self.errors,
            "source_baseline_sha256": BASELINE_SHA256,
            "baseline_checks_sha256": BASELINE_CHECKS_SHA256,
            "stats": self.stats, "legacy_input_check_status": self.legacy,
            "errors": self.errors,
            "runtime_executed": False, "activation_changed": False,
            "limitations": [
                "Passing means this revision preserves its pinned inputs and metadata contracts; it does not make the legacy integrity checks pass.",
                "The factual input contains missing references. They remain visible and are not added to the official known-link-issues baseline.",
                "External URLs and targets outside docs are not fetched or validated.",
                "Model results are bound to current documents and script; this checker does not prove native execution, node capacity, cost lower bounds, data availability, or DAG consensus safety.",
            ],
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--stdout-only", action="store_true", help="do not write revision-results.json")
    args = parser.parse_args()
    root = args.root.resolve()
    if not (root / "docs").is_dir():
        parser.error("--root must contain docs/")
    result = Audit(root).run()
    if not args.stdout_only:
        destination = root / REV / "revision-results.json"
        if destination.is_symlink() or not destination.resolve().is_relative_to(root):
            parser.error("revision result destination is a symlink or escapes root")
        destination.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
