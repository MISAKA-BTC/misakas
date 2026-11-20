#!/usr/bin/env python3
"""Offline CAD1 revision preservation checks, not native or activation evidence.

The source baseline is pinned inside this checker. This checker never rewrites
it, historical checkers, or link-issue baselines. By default only this revision's
revision-results.json is written; --stdout-only performs no writes.
"""
from __future__ import annotations

import argparse
import ast
import copy
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import sys

sys.dont_write_bytecode = True

REVISION = "2026-11-17-admission-difficulty"
REV = f"docs/maintenance/revisions/{REVISION}"
SOURCE_COMMIT = "2d8212b634880b0967fbf24b00b41936c2322c14"
SOURCE_TREE = "d99e8148554f04b7f53a19d4717a1f9b4796d3f3"
BASELINE_SHA256 = "c14469bd26ff6eb45daedd2ea78b8100a266be593497dae8ef85aaa40b0318d8"
BASELINE = f"{REV}/source-baseline.json"
MANIFEST = f"{REV}/changes.json"
RESULT = f"{REV}/revision-results.json"
PROPOSED = "PROPOSED_DESIGN_NOT_ACTIVATED"
ADOPTED = "ADOPTED_DESIGN_NOT_ACTIVATED"
RFC = "docs/rfc/0056-global-claim-admission-difficulty.md"
ADR = "docs/adr/0224-global-claim-admission-difficulty.md"
MODEL_SCRIPT = f"{REV}/check_difficulty_model.py"
MODEL_RESULT = f"{REV}/model-results.json"
SELF_EXCLUSIONS = {MANIFEST, RESULT, f"{REV}/validation-results.json"}

FRONT_POINTERS = {
    "docs/PRINCIPLES.md",
    "docs/architecture/palw-specification.md",
    "docs/rfc-v2/README.md",
    "docs/rfc-v2/MIGRATION.md",
    "docs/adr-v2/README.md",
    "docs/rfc-v2/0002-tensor-ir-and-shared-query-data.md",
    "docs/rfc-v2/0005-bounded-resources-and-long-context.md",
    "docs/rfc-v2/0007-claims-exec-and-answer-accounting.md",
    "docs/rfc-v2/0009-remote-client-and-sdk.md",
    "docs/rfc-v2/0010-dag-consensus-and-native-settlement.md",
    "docs/rfc-v2/0011-bond-budgets-fees-and-payments.md",
    "docs/rfc-v2/0012-authenticated-state-storage-and-ibd.md",
    "docs/rfc-v2/0013-permissionless-bootstrap-and-release.md",
    "docs/rfc-v2/0014-court-master-contract.md",
    "docs/rfc-v2/0015-freeze-deferred-rejected-and-governance.md",
    "docs/rfc-v2/0019-model-free-node-and-bounded-evidence.md",
    "docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md",
}
APPEND_MARKERS = {
    "docs/maintenance/numbering.md": "CAD1-NUMBERING",
    "docs/governance/parameter-register.md": "CAD1-PARAMETERS",
}
REGISTRIES = {
    "docs/rfc-v2/registry.json": ("2026-11-09", "active_count", 20, "docs/rfc-v2"),
    "docs/adr-v2/registry.json": ("2026-10-30", "current_count", 50, "docs/adr-v2"),
}
GENERATED_INDEXES = {
    "docs/rfc/INDEX.md", "docs/rfc/TOPICS.md",
    "docs/adr/INDEX.md", "docs/adr/TOPICS.md",
    "docs/maintenance/catalog.md", "docs/maintenance/decision-catalog.json",
}
PARAMETERS = "docs/governance/parameters.json"
NUMBER_REGISTRY = "docs/maintenance/number-registry.json"
INDEX_GENERATOR = "docs/maintenance/generate_indexes.py"
EDITABLE = (FRONT_POINTERS | set(APPEND_MARKERS) | set(REGISTRIES)
            | GENERATED_INDEXES | {PARAMETERS, NUMBER_REGISTRY, INDEX_GENERATOR})
NEW_DOCUMENTS = {
    RFC, ADR,
    "docs/design/palw/integration/claim-admission-difficulty-implementation-plan-2026-11-17.md",
    "docs/design/palw/integration/public-one-honest-implementation-ledger.md",
}
RECORDS = {
    "RFC-0055": ("docs/rfc/0055-global-admission-court-calendar-and-one-bps.md", ADOPTED),
    "ADR-0223": ("docs/adr/0223-global-admission-court-calendar-and-one-bps.md", ADOPTED),
    "RFC-0056": (RFC, PROPOSED),
    "ADR-0224": (ADR, PROPOSED),
}
NEW_PARAMETER_KEYS = {
    "cad1_planning_epoch_ms", "cad1_difficulty_range", "cad1_utilization_scale",
    "cad1_high_watermark", "cad1_low_watermark", "cad1_increase_ratio",
    "cad1_decrease_ratio", "cad1_low_epochs", "cad1_publication_offset",
    "cad1_seed_ready_guard_ms", "cad1_initial_difficulty",
    "cad1_pressure_aggregation", "cad1_incomplete_observation", "cad1_trial_identity",
}


def git_blob(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()


def canonical_json(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode("utf-8")


def pretty_json(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError(f"duplicate JSON key: {key}")
        out[key] = value
    return out


def valid_doc_path(value: object) -> bool:
    if not isinstance(value, str) or "\\" in value:
        return False
    p = PurePosixPath(value)
    return (value.startswith("docs/") and not p.is_absolute()
            and ".." not in p.parts and "." not in p.parts
            and p.as_posix() == value)


def restore_marked(data: bytes, marker: str, expected: str,
                   placement: str) -> tuple[bytes, str]:
    """Remove one permitted block and at most one adjacent empty line per side.

    Every candidate restoration must match the pinned original Git blob exactly;
    whitespace elsewhere, old appendices, and all original content remain bound.
    """
    lines = data.splitlines(keepends=True)
    begin = f"<!-- {marker}-BEGIN -->".encode()
    end = f"<!-- {marker}-END -->".encode()
    starts = [i for i, line in enumerate(lines) if line.rstrip(b"\r\n") == begin]
    ends = [i for i, line in enumerate(lines) if line.rstrip(b"\r\n") == end]
    if len(starts) != 1 or len(ends) != 1 or starts[0] >= ends[0]:
        raise ValueError(f"expected exactly one ordered {marker} block")
    first, last = starts[0], ends[0]
    before, after = lines[:first], lines[last + 1:]
    if placement == "front":
        nonempty = [line for line in before if line.strip()]
        if len(nonempty) != 1 or not nonempty[0].startswith(b"# "):
            raise ValueError("proposal pointer is not directly after the title")
    elif placement == "append":
        if any(line.strip() for line in after):
            raise ValueError("numbering/parameter addition is not at EOF")
    else:
        raise ValueError("invalid marker placement")
    blank = lambda line: line in (b"\n", b"\r\n")
    for left in (0, 1):
        if left and (not before or not blank(before[-1])):
            continue
        for right in (0, 1):
            if right and (not after or not blank(after[0])):
                continue
            candidate = b"".join(before[:-left] if left else before)
            candidate += b"".join(after[right:])
            if git_blob(candidate) == expected:
                return candidate, b"".join(lines[first + 1:last]).decode("utf-8")
    raise ValueError("removing the allowed block does not restore the original bytes")


class Audit:
    def __init__(self, root: Path, stdout_only: bool):
        self.root = root
        self.stdout_only = stdout_only
        self.errors: list[str] = []
        self.stats: dict[str, object] = {}
        self.cache: dict[str, bytes] = {}
        self.new_text: dict[str, str] = {}

    def require(self, condition: object, message: str) -> None:
        if not condition:
            self.errors.append(message)

    def read(self, path: str) -> bytes:
        if not valid_doc_path(path):
            raise ValueError(f"noncanonical or non-doc path: {path}")
        p = self.root / path
        if p.is_symlink() or not p.resolve().is_relative_to(self.root):
            raise ValueError(f"symlink or escaping path: {path}")
        if path not in self.cache:
            self.cache[path] = p.read_bytes()
        return self.cache[path]

    def load(self, path: str) -> dict:
        value = json.loads(self.read(path), object_pairs_hook=unique_object)
        if not isinstance(value, dict):
            raise ValueError(f"JSON object required: {path}")
        return value

    def restore_equal(self, path: str, value: dict, expected: str) -> None:
        self.require(git_blob(pretty_json(value)) == expected,
                     f"unapproved changes to original structured data: {path}")

    def check_parameters(self, data: dict, baseline: dict, expected: str) -> None:
        rows = data.get("parameters", [])
        pins = baseline["baseline_parameters"]
        self.require(isinstance(rows, list) and len(rows) == 70,
                     "parameters must contain the original 56 plus exactly 14 CAD1 records")
        if not isinstance(rows, list):
            return
        old = rows[:56]
        self.require([r.get("key") for r in old] == [p["key"] for p in pins],
                     "original 56 parameter keys/order changed")
        exact_old = 0
        for row, pin in zip(old, pins):
            matches = hashlib.sha256(canonical_json(row)).hexdigest() == pin["canonical_sha256"]
            exact_old += int(matches)
            self.require(matches,
                         f"original parameter record changed: {pin['key']}")
        keys = [r.get("key") for r in rows]
        self.require(len(keys) == len(set(keys)), "duplicate parameter keys")
        new = rows[56:]
        self.require({r.get("key") for r in new} == NEW_PARAMETER_KEYS,
                     "CAD1 parameter key set differs from the 14 proposed records")
        for row in new:
            name = row.get("key")
            self.require(str(row.get("approval_state", "")).startswith(PROPOSED),
                         f"CAD1 parameter is not proposed: {name}")
            self.require(row.get("source") == RFC and row.get("decision") == ADR,
                         f"CAD1 parameter source/decision differs: {name}")
            self.require(row.get("runtime_key") is None
                         and row.get("runtime_config_changed") is False
                         and row.get("new_profile_only") is True,
                         f"CAD1 parameter changes runtime or scope: {name}")
        restored = copy.deepcopy(data)
        restored["parameters"] = old
        self.restore_equal(PARAMETERS, restored, expected)
        self.stats["original_parameters_preserved"] = exact_old
        self.stats["proposed_parameters"] = len(new)

    def check_registry(self, path: str, data: dict, expected: str) -> None:
        old_date, count_key, count, folder = REGISTRIES[path]
        files = sorted(p.name for p in (self.root / folder).glob("[0-9][0-9][0-9][0-9]-*.md"))
        documents = data.get("documents", [])
        self.require(data.get(count_key) == count and len(documents) == count
                     and len(files) == count, f"20/50 inventory changed: {path}")
        self.require(sorted(d.get("file", "") for d in documents) == files,
                     f"registry/current-book filenames differ: {path}")
        self.require(len({d.get("id") for d in documents}) == count,
                     f"duplicate current-book registry IDs: {path}")
        post = data.get("post_consolidation_documents", [])
        for identifier, (record_path, status) in RECORDS.items():
            matches = [r for r in post if r.get("id") == identifier]
            self.require(len(matches) == 1 and matches[0].get("path") == record_path
                         and matches[0].get("status") == status,
                         f"wrong/missing 0055/0223/0056/0224 status in {path}: {identifier}")
        latest = data.get("latest_proposal", {})
        self.require(latest.get("revision") == REVISION and latest.get("rfc_record") == RFC
                     and latest.get("decision_record") == ADR and latest.get("status") == PROPOSED
                     and latest.get("runtime_changed") is False
                     and latest.get("activation_changed") is False,
                     f"latest_proposal does not preserve proposed/non-runtime status: {path}")
        self.require(data.get("editorial_date") == "2026-11-17",
                     f"unexpected registry editorial date: {path}")
        restored = copy.deepcopy(data)
        restored.pop("latest_proposal", None)
        restored["editorial_date"] = old_date
        restored["post_consolidation_documents"] = [r for r in post if r.get("id") not in RECORDS]
        self.restore_equal(path, restored, expected)
        self.stats[folder.rsplit("/", 1)[-1] + "_books"] = len(files)

    def check_number_registry(self, data: dict, expected: str) -> None:
        rows = data.get("subsequent_local_allocations", [])
        self.require(isinstance(rows, list) and len(rows) == 2,
                     "number registry must add only the prior and current in-conversation allocations")
        for rfc, adr, status in [("0055", "0223", ADOPTED), ("0056", "0224", PROPOSED)]:
            selected = [r for r in rows if r.get("rfc") == rfc and r.get("adr") == adr]
            self.require(len(selected) == 1 and selected[0].get("status") == status
                         and selected[0].get("rfc_path") == RECORDS[f"RFC-{rfc}"][0]
                         and selected[0].get("adr_path") == RECORDS[f"ADR-{adr}"][0],
                         f"wrong number allocation/status: RFC-{rfc}/ADR-{adr}")
        restored = copy.deepcopy(data)
        restored.pop("subsequent_local_allocations", None)
        self.restore_equal(NUMBER_REGISTRY, restored, expected)

    def check_index_generator(self, data: bytes, expected: str) -> None:
        wanted = {("rfc", "0055"), ("adr", "0223"), ("rfc", "0056"), ("adr", "0224")}
        found = []
        original = []
        for line in data.splitlines(keepends=True):
            match = re.match(rb"^status\[\('(rfc|adr)','(\d{4})'\)\]", line)
            key = tuple(part.decode("ascii") for part in match.groups()) if match else None
            if key not in wanted:
                original.append(line)
                continue
            nodes = ast.parse(line.decode("utf-8")).body
            safe = (len(nodes) == 1 and isinstance(nodes[0], ast.Assign)
                    and len(nodes[0].targets) == 1 and isinstance(nodes[0].targets[0], ast.Subscript)
                    and isinstance(nodes[0].targets[0].value, ast.Name)
                    and nodes[0].targets[0].value.id == "status"
                    and ast.literal_eval(nodes[0].targets[0].slice) == key
                    and isinstance(nodes[0].value, ast.Constant)
                    and isinstance(nodes[0].value.value, str))
            self.require(safe, "index generator addition must be a literal status assignment only")
            if key in {("rfc", "0056"), ("adr", "0224")} and safe:
                self.require("Proposed" in nodes[0].value.value,
                             "new decision index status must remain Proposed")
            found.append(key)
        self.require(set(found) == wanted and len(found) == 4,
                     "index generator must add exactly four scoped status mappings")
        self.require(git_blob(b"".join(original)) == expected,
                     "index generator changed beyond the four status mappings")

    def check_model_result(self) -> None:
        result = self.load(MODEL_RESULT)
        self.require(result.get("schema") == "misaka-cad1-abstract-model-results-v1"
                     and result.get("source_commit") == SOURCE_COMMIT,
                     "abstract model result schema/source mismatch")
        for path_key, hash_key, path in [("rfc_path", "rfc_sha256", RFC),
                                         ("script_path", "script_sha256", MODEL_SCRIPT)]:
            self.require(result.get(path_key) == path
                         and result.get(hash_key) == hashlib.sha256(self.read(path)).hexdigest(),
                         f"stale abstract model result: {path}")
        counts = result.get("counts", result)
        tests = result.get("tests", [])
        run = counts.get("tests_run")
        self.require(type(run) is int and run > 0 and run == len(tests)
                     and counts.get("passed") == run
                     and all(counts.get(k) == 0 for k in ("failures", "errors", "skipped"))
                     and result.get("success") is True
                     and all(t.get("result") == "PASS" for t in tests),
                     "abstract model result is incomplete, failed, skipped, or inconsistent")
        self.require(len({t.get("name") for t in tests}) == len(tests),
                     "duplicate abstract model test names")
        self.require("ABSTRACT" in result.get("scope", "")
                     and bool(result.get("not_verified")),
                     "abstract-model limitations are missing")
        self.stats["abstract_model_tests"] = run
        self.stats["abstract_model_success"] = result.get("success")

    def manifest_entries(self, rows: object, label: str) -> dict:
        if not isinstance(rows, list):
            self.errors.append(f"manifest {label} must be a list")
            return {}
        out = {}
        for row in rows:
            if not isinstance(row, dict) or not valid_doc_path(row.get("path")):
                self.errors.append(f"manifest {label} contains an invalid/non-doc path")
                continue
            path = row["path"]
            self.require(path not in out, f"duplicate manifest {label} path: {path}")
            self.require(isinstance(row.get("reason"), str) and bool(row["reason"].strip()),
                         f"missing manifest reason: {path}")
            out[path] = row
        return out

    def check_manifest(self, baseline: dict, actual: set[str], changed: set[str]) -> None:
        data = self.load(MANIFEST)
        self.require(data.get("source_commit") == SOURCE_COMMIT and data.get("revision") == REVISION,
                     "manifest source/revision mismatch")
        self.require(data.get("runtime_changed") is False and data.get("activation_changed") is False,
                     "manifest must explicitly remain docs-only and not activated")
        exclusions = data.get("self_referential_hash_exclusions", [])
        self.require(isinstance(exclusions, list) and len(exclusions) == 3
                     and set(exclusions) == SELF_EXCLUSIONS,
                     "only the three declared current self-referential reports may be hash-excluded")
        modified = self.manifest_entries(data.get("modified"), "modified")
        added = self.manifest_entries(data.get("added"), "added")
        self.require(set(modified) == changed,
                     f"manifest modified inventory mismatch: {sorted(set(modified) ^ changed)}")
        expected_added = actual - set(baseline) - SELF_EXCLUSIONS
        self.require(set(added) == expected_added,
                     f"manifest added inventory mismatch: {sorted(set(added) ^ expected_added)}")
        for path, row in modified.items():
            self.require(path in EDITABLE and path in baseline,
                         f"manifest attempts to authorize an immutable/non-source edit: {path}")
            if path in baseline:
                self.require(row.get("baseline_git_blob_sha") == baseline[path]["git_blob_sha"],
                             f"manifest baseline hash mismatch: {path}")
            if path in actual:
                self.require(row.get("updated_git_blob_sha") == git_blob(self.read(path)),
                             f"manifest updated hash mismatch: {path}")
        for path, row in added.items():
            if path in actual:
                self.require(row.get("git_blob_sha") == git_blob(self.read(path)),
                             f"manifest added hash mismatch: {path}")
        self.stats["modified_files"] = len(changed)
        self.stats["added_files_in_manifest"] = len(added)
        self.stats["self_referential_hash_exclusions"] = sorted(SELF_EXCLUSIONS)

    def check_links(self, baseline: dict) -> None:
        helper_path = "docs/maintenance/doclib.py"
        self.require(git_blob(self.read(helper_path)) == baseline[helper_path]["git_blob_sha"],
                     "refusing to import a changed historical Markdown helper")
        if git_blob(self.read(helper_path)) != baseline[helper_path]["git_blob_sha"]:
            return
        spec = importlib.util.spec_from_file_location("cad1_pinned_doclib", self.root / helper_path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        counts = {"checked_local": 0, "external_unchecked": 0,
                  "outside_docs_unchecked": 0, "deferred_self_output": 0}
        anchor_cache = {}
        for source, text in sorted(self.new_text.items()):
            for _, _, url in module.spans(text):
                kind, target, fragment = module.resolve(source, url)
                if kind == "external":
                    counts["external_unchecked"] += 1
                    continue
                if kind == "malformed":
                    self.errors.append(f"malformed new Markdown link in {source}: {url}")
                    continue
                if target != "docs" and not target.startswith("docs/"):
                    counts["outside_docs_unchecked"] += 1
                    continue
                counts["checked_local"] += 1
                p = self.root / target
                if not p.exists() and target == RESULT and not self.stdout_only:
                    counts["deferred_self_output"] += 1
                    continue
                if not p.exists():
                    self.errors.append(f"missing new Markdown target: {source} -> {target}")
                    continue
                if fragment:
                    q = p / "README.md" if p.is_dir() else p
                    if q.is_file() and q.suffix == ".md":
                        if q not in anchor_cache:
                            anchor_cache[q] = module.anchors(q.read_text(encoding="utf-8"))
                        self.require(fragment in anchor_cache[q],
                                     f"missing new Markdown anchor: {source} -> {target}#{fragment}")
        self.stats["new_markdown_links"] = counts

    def run(self) -> dict:
        try:
            raw = self.read(BASELINE)
            if hashlib.sha256(raw).hexdigest() != BASELINE_SHA256:
                raise ValueError("pinned source-baseline.json bytes changed; do not regenerate the baseline")
            source = self.load(BASELINE)
            self.require(source.get("source_commit") == SOURCE_COMMIT
                         and source.get("source_tree") == SOURCE_TREE,
                         "pinned source commit/tree mismatch")
            records = source.get("documents", [])
            companions = source.get("previous_conversation_companions", [])
            self.require(len(records) == 1946 and len(companions) == 13
                         and len(source.get("baseline_parameters", [])) == 56,
                         "pinned source inventory counts differ")
            baseline = {r["path"]: r for r in records}
            prior = {r["path"]: r for r in companions}
            self.require(len(baseline) == len(records) and len(prior) == len(companions)
                         and not (set(baseline) & set(prior)), "duplicate/overlapping baseline paths")
            actual = set()
            for p in (self.root / "docs").rglob("*"):
                if "__pycache__" in p.parts or p.suffix == ".pyc":
                    continue
                if p.is_symlink():
                    self.errors.append(f"symlink in documentation package: {p.relative_to(self.root)}")
                elif p.is_file():
                    actual.add(p.relative_to(self.root).as_posix())
            changed = set()
            preserved = 0
            for path, record in baseline.items():
                if path not in actual:
                    self.errors.append(f"original source file missing: {path}")
                    continue
                data = self.read(path)
                digest = git_blob(data)
                expected = record["git_blob_sha"]
                if digest == expected:
                    self.require(len(data) == record["bytes"], f"baseline length mismatch: {path}")
                    preserved += 1
                    continue
                changed.add(path)
                try:
                    if path in FRONT_POINTERS:
                        _, self.new_text[path] = restore_marked(data, "CAD1-PROPOSAL", expected, "front")
                    elif path in APPEND_MARKERS:
                        _, self.new_text[path] = restore_marked(data, APPEND_MARKERS[path], expected, "append")
                    elif path == PARAMETERS:
                        self.check_parameters(self.load(path), source, expected)
                    elif path in REGISTRIES:
                        self.check_registry(path, self.load(path), expected)
                    elif path == NUMBER_REGISTRY:
                        self.check_number_registry(self.load(path), expected)
                    elif path == INDEX_GENERATOR:
                        self.check_index_generator(data, expected)
                    elif path in GENERATED_INDEXES:
                        if path.endswith(".md"):
                            self.new_text[path] = data.decode("utf-8")
                    else:
                        self.errors.append(f"immutable original source changed: {path}")
                except (ValueError, KeyError, TypeError, SyntaxError) as exc:
                    self.errors.append(f"preservation check failed for {path}: {exc}")
            # Required integration records cannot be omitted by leaving baseline files unchanged.
            for path in {PARAMETERS, NUMBER_REGISTRY, INDEX_GENERATOR} | set(REGISTRIES):
                self.require(path in changed, f"required CAD1 integration is absent: {path}")
            exact_prior = 0
            for path, record in prior.items():
                matches = path in actual and git_blob(self.read(path)) == record["git_blob_sha"]
                exact_prior += int(matches)
                self.require(matches,
                             f"previous 0055 companion changed or missing: {path}")
            for path in actual - set(baseline) - set(prior):
                current_revision = path.startswith(REV + "/")
                allowed_revision = (current_revision and
                                    (Path(path).suffix in {".md", ".json"}
                                     or path in {MODEL_SCRIPT, f"{REV}/check_revision.py"}))
                self.require(path in NEW_DOCUMENTS or allowed_revision,
                             f"new file outside the scoped proposal/records: {path}")
                if path.endswith(".md"):
                    self.new_text[path] = self.read(path).decode("utf-8")
            for identifier, (path, status) in RECORDS.items():
                text = self.read(path).decode("utf-8")
                match = re.search(r"(?m)^- 状態:\s*(?:\*\*)?([A-Z_]+)", text)
                self.require(match is not None and match[1] == status,
                             f"primary status mismatch: {identifier}")
            self.stats.update({"source_documents": len(baseline),
                               "byte_identical_source_documents": preserved,
                               "previous_companions_exact": exact_prior,
                               "front_pointer_files": len(changed & FRONT_POINTERS),
                               "historical_checkers_and_appendices_editable": False,
                               "deleted_source_files": len(set(baseline) - actual)})
            self.check_manifest(baseline, actual, changed)
            self.check_model_result()
            self.check_links(baseline)
        except (OSError, ValueError, KeyError, TypeError, AttributeError, ImportError) as exc:
            self.errors.append(f"audit could not complete: {type(exc).__name__}: {exc}")
        return {
            "schema": "misaka-cad1-revision-preservation-v1",
            "generated_at_utc": datetime.now(timezone.utc).isoformat(),
            "scope": "DOCS-ONLY REVISION PRESERVATION; NOT native, E2E, performance, or activation evidence",
            "revision": REVISION,
            "source_commit": SOURCE_COMMIT,
            "source_baseline_sha256": BASELINE_SHA256,
            "passed": not self.errors,
            "errors": self.errors,
            "stats": self.stats,
            "runtime_executed": False,
            "native_code_audited_by_this_checker": False,
            "limitations": [
                "The change manifest and docs package are checked; unrelated working-tree/native changes and live runtime are not audited.",
                "Historical numbered RFC/ADR, appendices, prior revisions/checkers and prior 0055 companions are byte-preserved, not re-certified.",
                "New authored Markdown and marked additions are link-checked with the pinned limited parser; frozen companion links, external URLs and paths outside docs are not re-verified.",
                "Fresh hashes and passing abstract-model records are checked; this script does not run or prove native integration, hardware capacity, closed-loop stability, or activation.",
                "The immutable source baseline and global historical checker/link baselines are never rewritten.",
            ],
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--stdout-only", action="store_true", help="print JSON without writing revision-results.json")
    args = parser.parse_args()
    root = args.root.resolve()
    if not (root / "docs").is_dir():
        parser.error("root must contain docs/")
    result = Audit(root, args.stdout_only).run()
    encoded = pretty_json(result)
    if not args.stdout_only:
        try:
            destination = root / RESULT
            if destination.is_symlink() or not destination.resolve().is_relative_to(root):
                raise OSError("result path is a symlink or escapes the selected root")
            destination.write_bytes(encoded)
        except OSError as exc:
            result["passed"] = False
            result["errors"].append(f"could not write revision-results.json: {exc}")
            encoded = pretty_json(result)
    sys.stdout.write(encoded.decode("utf-8"))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
