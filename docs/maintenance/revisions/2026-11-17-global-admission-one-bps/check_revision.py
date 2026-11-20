#!/usr/bin/env python3
"""Offline preservation/registration checks for the 2026-11-17 docs revision.

DOCUMENTATION CHECK ONLY. This neither executes native/Rust/Studio/Panel code
nor establishes protocol correctness, hardware capacity, E2E success or
activation. The saved abstract-model result is checked for hashes/counts/scope;
this checker does not rerun that model or turn it into native evidence.

The immutable source baseline is pinned below. This program NEVER rewrites a
baseline, change manifest, source document, old checker, or historical result.
Its only default write is this revision's excluded revision-results.json.
External URLs and targets outside the docs-only package remain unchecked.
"""
from __future__ import annotations

import argparse
import ast
from collections import Counter
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import sys

sys.dont_write_bytecode = True

REVISION = "2026-11-17-global-admission-one-bps"
REV_DIR = f"docs/maintenance/revisions/{REVISION}"
SOURCE_COMMIT = "c3da7c6eaf5bf87f8e032d5b122c603d720a8398"
SOURCE_TREE = "62971af5ea12db4ee07cbe547ad55180ef3a31e9"
BASELINE = f"{REV_DIR}/source-baseline.json"
BASELINE_SHA256 = "507087b7f3088252fae061a67ca3d51f8e559b3bc5b379356e33fb84859614c8"
CHANGES = f"{REV_DIR}/changes.json"
RESULT = f"{REV_DIR}/revision-results.json"
NEW_RFC = "docs/rfc/0055-global-admission-court-calendar-and-one-bps.md"
NEW_ADR = "docs/adr/0223-global-admission-court-calendar-and-one-bps.md"
PLAN = "docs/design/palw/integration/unified-admission-implementation-plan-2026-11-17.md"
MODEL = f"{REV_DIR}/check_protocol_model.py"
MODEL_RESULT = f"{REV_DIR}/model-results.json"
SELF_EXCLUSIONS = {CHANGES, RESULT, f"{REV_DIR}/validation-results.json"}
SUFFIXES = {".md", ".json", ".py"}

EXPECTED_PARAMETERS = {
    "unified_admission_planning_epoch_ms_v1": "120000",
    "unified_carrier_slot_period_ms_v1": "1000",
    "unified_carrier_permits_per_slot_v1": "1",
    "unified_admission_difficulty_range_v1": "[1, 2^120-1]",
    "unified_admission_utilization_scale_v1": "1024",
    "unified_admission_high_watermark_v1": "768/1024",
    "unified_admission_low_watermark_v1": "256/1024",
    "unified_admission_increase_ratio_v1": "9/8（ceil）",
    "unified_admission_decrease_ratio_v1": "8/9（floor）",
    "unified_admission_low_epochs_v1": "3",
    "unified_admission_publication_offset_v1": "2",
    "unified_court_count_rule_v1": "全flow/stock・全期限・全責任の残容量から導出",
    "unified_carrier_lifetime_slots_v1": "共通qualification済みprofileの有限必須欄",
    "unified_common_capacity_profile_v1": "版・機器・共通経路のqualificationで導出",
}

# Independent of changes.json: a manifest cannot authorize editing old evidence
# or a historical checker by merely listing it as modified.
ALLOWED_MODIFIED_EXACT = {
    "docs/PRINCIPLES.md", "docs/README.md", "docs/architecture/palw-specification.md",
    "docs/governance/parameter-register.md", "docs/governance/parameters.json",
    "docs/rfc/README.md", "docs/rfc/INDEX.md", "docs/rfc/TOPICS.md",
    "docs/adr/README.md", "docs/adr/INDEX.md", "docs/adr/TOPICS.md",
    "docs/rfc-v2/README.md", "docs/rfc-v2/MIGRATION.md", "docs/rfc-v2/registry.json",
    "docs/adr-v2/README.md", "docs/adr-v2/registry.json",
    "docs/maintenance/catalog.md", "docs/maintenance/decision-catalog.json",
    "docs/maintenance/number-registry.json", "docs/maintenance/numbering.md",
    "docs/maintenance/generate_indexes.py",
}
REQUIRED_ADDED = {
    NEW_RFC, NEW_ADR, PLAN, BASELINE, MODEL, MODEL_RESULT,
    f"{REV_DIR}/README.md", f"{REV_DIR}/REVIEW.md", f"{REV_DIR}/VALIDATION.md",
    f"{REV_DIR}/check_revision.py",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_blob(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()


def canonical_digest(value) -> str:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return sha256(encoded)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON object key: {key}")
        result[key] = value
    return result


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def numbered(path: str, directory: str) -> bool:
    return bool(re.fullmatch(re.escape(directory) + r"/\d{4}-[^/]+\.md", path))


def protected(path: str) -> bool:
    name = PurePosixPath(path).name
    if numbered(path, "docs/rfc") or numbered(path, "docs/adr"):
        return True
    if path.startswith(("docs/history/", "docs/evidence/", "docs/audit/")):
        return True
    if path.startswith("docs/maintenance/revisions/") and not path.startswith(REV_DIR + "/"):
        return True
    if "source-map" in name or "source_map" in name or "manifest" in name:
        return True
    return path.endswith(".py") and (name.startswith("check") or name == "doclib.py")


def allowed_modified(path: str) -> bool:
    return path in ALLOWED_MODIFIED_EXACT or numbered(path, "docs/rfc-v2") or numbered(path, "docs/adr-v2")


class Audit:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.errors = []
        self.checks = []
        self.baseline = {}
        self.modified = {}
        self.added = {}
        self.actual_hashes = {}
        self.baseline_data = {}
        self.changes_data = {}

    def expect(self, condition: bool, message: str) -> None:
        if not condition:
            self.errors.append(message)

    def path(self, relative: str) -> Path:
        if not isinstance(relative, str):
            raise ValueError("Manifest path is not a string")
        pure = PurePosixPath(relative)
        if (not relative.startswith("docs/") or pure.is_absolute() or ".." in pure.parts
                or "\\" in relative or str(pure) != relative):
            raise ValueError(f"Noncanonical/out-of-scope manifest path: {relative}")
        result = self.root / relative
        if result.resolve() != result:
            raise ValueError(f"Symlink or noncanonical file path: {relative}")
        return result

    def run_check(self, name: str, action) -> None:
        start = len(self.errors)
        try:
            detail = action()
        except (OSError, ValueError, KeyError, TypeError, ImportError, SyntaxError) as error:
            self.errors.append(f"{type(error).__name__}: {error}")
            detail = {}
        errors = self.errors[start:]
        self.checks.append(dict(name=name, status="FAIL" if errors else "PASS", details=detail, errors=errors))

    def rows(self, rows: list, hash_field: str) -> dict:
        if not isinstance(rows, list):
            raise ValueError("Manifest entries must be a list")
        result = {}
        for row in rows:
            path = row["path"]
            self.path(path)
            if path in result:
                raise ValueError(f"Duplicate manifest path: {path}")
            if PurePosixPath(path).suffix not in SUFFIXES:
                raise ValueError(f"Unexpected manifest scope: {path}")
            if not re.fullmatch(r"[0-9a-f]{40}", row[hash_field]):
                raise ValueError(f"Invalid Git blob hash: {path}")
            result[path] = row
        return result

    def check_baseline_and_manifest(self):
        data = self.path(BASELINE).read_bytes()
        if sha256(data) != BASELINE_SHA256:
            raise ValueError("Pinned source-baseline bytes changed; do not regenerate a baseline to hide edits")
        self.baseline_data = read_json(self.path(BASELINE))
        self.expect(self.baseline_data["source_commit"] == SOURCE_COMMIT, "Incorrect source commit")
        self.expect(self.baseline_data["source_tree"] == SOURCE_TREE, "Incorrect source tree")
        self.baseline = self.rows(self.baseline_data["documents"], "git_blob_sha")
        self.expect(len(self.baseline) == 1367, "Expected the pinned 1367 source text blobs")
        self.changes_data = read_json(self.path(CHANGES))
        changes = self.changes_data
        self.expect(changes["source_commit"] == SOURCE_COMMIT and changes["revision"] == REVISION,
                    "Change manifest source/revision mismatch")
        self.expect(changes.get("runtime_changed") is False and changes.get("activation_changed") is False,
                    "A documentation revision must not assert runtime/activation changes")
        self.modified = self.rows(changes["modified"], "updated_git_blob_sha")
        self.added = self.rows(changes["added"], "git_blob_sha")
        self.expect(not (self.modified.keys() & self.added.keys()), "Path appears as both modified and added")
        excluded = changes["self_referential_hash_exclusions"]
        self.expect(len(excluded) == len(set(excluded)) and set(excluded) == SELF_EXCLUSIONS,
                    "Only the three fixed self-referential output/manifest exclusions are allowed")
        self.expect(not ((self.modified.keys() | self.added.keys()) & SELF_EXCLUSIONS),
                    "Self-referential exclusions must not appear as hashed change entries")
        for path, row in self.modified.items():
            self.expect(path in self.baseline, f"Modified path is absent from the source baseline: {path}")
            self.expect(row.get("baseline_git_blob_sha") == self.baseline.get(path, {}).get("git_blob_sha"),
                        f"Modified path has a different source blob: {path}")
            self.expect(bool(row.get("reason", "").strip()), f"Modified path has no reason: {path}")
            self.expect(not protected(path), f"Protected history/checker may not enter the modification allowlist: {path}")
            self.expect(allowed_modified(path), f"Path is outside this revision's independent edit scope: {path}")
        for path, row in self.added.items():
            self.expect(path not in self.baseline, f"Source path disguised as an addition: {path}")
            self.expect(path in REQUIRED_ADDED, f"Unexpected addition outside this revision's deliverables: {path}")
            self.expect(bool(row.get("reason", "").strip()), f"Added path has no reason: {path}")
        self.expect(set(self.added) == REQUIRED_ADDED,
                    f"Added-file manifest differs from deliverables: missing={sorted(REQUIRED_ADDED-set(self.added))}, "
                    f"extra={sorted(set(self.added)-REQUIRED_ADDED)}")
        return dict(source_commit=SOURCE_COMMIT, source_tree=SOURCE_TREE,
                    pinned_baseline_sha256=BASELINE_SHA256, baseline_blobs=len(self.baseline),
                    allowed_modified=len(self.modified), added=len(self.added),
                    excluded_self_referential=sorted(SELF_EXCLUSIONS))

    def check_all_source_and_current_bytes(self):
        if not self.baseline:
            raise ValueError("No verified source baseline is available")
        unchanged = changed = protected_count = 0
        for path, row in self.baseline.items():
            file = self.path(path)
            self.expect(file.is_file(), f"Source text file is missing: {path}")
            if not file.is_file():
                continue
            actual = git_blob(file.read_bytes())
            self.actual_hashes[path] = actual
            original = row["git_blob_sha"]
            expected = self.modified[path]["updated_git_blob_sha"] if path in self.modified else original
            self.expect(actual == expected, f"Source/current blob mismatch: {path}")
            if actual == original:
                unchanged += 1
                self.expect(path not in self.modified, f"Unchanged file unnecessarily authorized as modified: {path}")
            else:
                changed += 1
                self.expect(path in self.modified, f"Unlisted modification of source text: {path}")
            if protected(path):
                protected_count += 1
                self.expect(actual == original, f"Protected original bytes changed: {path}")
        for path, row in self.added.items():
            file = self.path(path)
            self.expect(file.is_file(), f"Added file is missing: {path}")
            if file.is_file():
                self.expect(git_blob(file.read_bytes()) == row["git_blob_sha"], f"Added-file blob mismatch: {path}")
        actual_paths = {path.relative_to(self.root).as_posix() for path in (self.root / "docs").rglob("*")
                        if path.is_file() and path.suffix in SUFFIXES}
        expected_paths = set(self.baseline) | set(self.added)
        self.expect(actual_paths - SELF_EXCLUSIONS == expected_paths,
                    f"Text inventory mismatch: unlisted={sorted(actual_paths-SELF_EXCLUSIONS-expected_paths)}, "
                    f"missing={sorted(expected_paths-actual_paths)}")
        return dict(source_blobs=len(self.baseline), unchanged=unchanged, modified=changed,
                    protected_unchanged=protected_count, actual_text_files=len(actual_paths),
                    scope="docs .md/.json/.py only; no binary evidence/runtime audit")

    def check_current_front_pointers_preserve_old_bytes(self):
        checked = []
        for path in sorted(self.modified):
            if not (numbered(path, "docs/rfc-v2") or numbered(path, "docs/adr-v2")):
                continue
            data = self.path(path).read_bytes()
            header_end = data.index(b"\n") + 1
            self.expect(data[header_end:].lstrip(b"\n").startswith(b"## 2026-11-17 "),
                        f"Current book modification is not a dated front pointer: {path}")
            # Remove a single added block directly after the unchanged title.
            # Finding a matching OLD Git blob proves that every remaining byte,
            # including all historical source fragments, is unchanged.
            restored = False
            limit = min(len(data), header_end + 16_384)
            for match in re.finditer(b"\n\n", data[header_end:limit]):
                tail_start = header_end + match.end()
                inserted = data[header_end:tail_start]
                if b"0055-global-admission" not in inserted or b"0223-global-admission" not in inserted:
                    continue
                for separator in (b"", b"\n", b"\n\n"):
                    old = data[:header_end] + separator + data[tail_start:]
                    if git_blob(old) == self.baseline[path]["git_blob_sha"]:
                        restored = True
                        break
                if restored:
                    break
            self.expect(restored, f"Cannot recover exact source book by removing only its front pointer: {path}")
            checked.append(path)
        appended = []
        numbering = "docs/maintenance/numbering.md"
        if numbering in self.modified:
            data = self.path(numbering).read_bytes()
            marker = "## 2026-11-17 全モデル共通受付とNode負荷保護".encode("utf-8")
            self.expect(data.count(marker) == 1, "Numbering note must have exactly one dated revision section")
            if data.count(marker) == 1:
                prefix, addition = data.split(marker, 1)
                # New blank-line separation is allowed; the original bytes,
                # including their exact trailing newline count, must recover.
                restored = any(git_blob(prefix.rstrip(b"\n") + b"\n" * count)
                               == self.baseline[numbering]["git_blob_sha"] for count in range(5))
                self.expect(restored, "Old numbering.md content changed; only the dated final note is authorized")
                self.expect(b"0055-global-admission" in addition and b"0223-global-admission" in addition
                            and b"ADOPTED_DESIGN_NOT_ACTIVATED" in addition,
                            "Numbering append must identify both new records and the unactivated status")
                appended.append(numbering)
        return dict(current_books_with_byte_preserving_front_pointers=len(checked), paths=checked,
                    append_only_documents_with_exact_old_blobs=appended)

    def check_inventories_and_registration(self):
        records = {}
        result = {}
        for directory, namespace, count, key in (
                ("docs/rfc-v2", "RFC-v2", 20, "active_count"),
                ("docs/adr-v2", "ADR-v2", 50, "current_count")):
            registry = read_json(self.path(f"{directory}/registry.json"))
            documents = registry["documents"]
            self.expect(registry[key] == len(documents) == count, f"Incorrect {namespace} inventory count")
            expected_ids = {f"{namespace}-{n:04d}" for n in range(1, count + 1)}
            self.expect({doc["id"] for doc in documents} == expected_ids, f"Incorrect {namespace} IDs")
            actual = {p.relative_to(self.root).as_posix() for p in self.path(directory).glob("[0-9][0-9][0-9][0-9]-*.md")}
            listed = {f"{directory}/{doc['file']}" for doc in documents}
            self.expect(actual == listed, f"{namespace} registered files differ from actual numbered books")
            for doc in documents:
                text = self.path(f"{directory}/{doc['file']}").read_text(encoding="utf-8")
                self.expect(text.startswith(f"# {doc['id']} — "), f"Incorrect current book heading: {doc['file']}")
            post = registry["post_consolidation_documents"]
            for kind, identifier, path in (("rfc", "RFC-0055", NEW_RFC), ("adr", "ADR-0223", NEW_ADR)):
                found = [row for row in post if row.get("path") == path or row.get("id") == identifier]
                self.expect(len(found) == 1 and found[0].get("path") == path
                            and found[0].get("id") == identifier and found[0].get("kind") == kind
                            and found[0].get("status") == "ADOPTED_DESIGN_NOT_ACTIVATED",
                            f"{namespace} post-consolidation registration mismatch: {identifier}")
            records[namespace] = registry
            result[namespace] = count
        for directory, new_path, identifier in (("docs/rfc", NEW_RFC, "RFC-0055"),
                                                ("docs/adr", NEW_ADR, "ADR-0223")):
            old = {path for path in self.baseline if numbered(path, directory)}
            current = {p.relative_to(self.root).as_posix() for p in self.path(directory).glob("[0-9][0-9][0-9][0-9]-*.md")}
            self.expect(current == old | {new_path}, f"Unexpected numbered historical addition/removal in {directory}")
            text = self.path(new_path).read_text(encoding="utf-8")
            self.expect(text.startswith(f"# {identifier} — ") and "ADOPTED_DESIGN_NOT_ACTIVATED" in text[:2_000],
                        f"New adoption has wrong heading/status: {new_path}")
        catalog = read_json(self.path("docs/maintenance/decision-catalog.json"))["records"]
        for kind, number, path in (("rfc", "0055", NEW_RFC), ("adr", "0223", NEW_ADR)):
            found = [row for row in catalog if row.get("path") == path]
            self.expect(len(found) == 1 and found[0].get("kind") == kind and found[0].get("number") == number,
                        f"New adoption absent/duplicated in decision catalog: {path}")
        numbers = read_json(self.path("docs/maintenance/number-registry.json"))
        found = [row for row in numbers.get("subsequent_local_allocations", [])
                 if row.get("rfc") == "0055" or row.get("adr") == "0223"]
        self.expect(len(found) == 1 and found[0].get("rfc") == "0055" and found[0].get("adr") == "0223"
                    and found[0].get("rfc_path") == NEW_RFC and found[0].get("adr_path") == NEW_ADR
                    and found[0].get("status") == "ADOPTED_DESIGN_NOT_ACTIVATED",
                    "Number registry must append one complete 0055/0223 allocation without reusing old numbers")
        result["new_records"] = [NEW_RFC, NEW_ADR]
        return result

    def check_parameters(self):
        prior_rows = self.baseline_data["baseline_parameters"]
        prior = {row["key"]: row["canonical_sha256"] for row in prior_rows}
        self.expect(len(prior_rows) == len(prior) == 56, "Expected 56 unique source parameter records")
        current_rows = read_json(self.path("docs/governance/parameters.json"))["parameters"]
        current = {row["key"]: row for row in current_rows}
        self.expect(len(current_rows) == len(current), "Duplicate parameter key")
        self.expect(set(current) == set(prior) | set(EXPECTED_PARAMETERS),
                    "Parameter inventory must preserve old keys and add exactly the 14 unified-profile keys")
        for key, expected in prior.items():
            self.expect(key in current and canonical_digest(current[key]) == expected,
                        f"Existing parameter content changed or missing: {key}")
        table = self.path("docs/governance/parameter-register.md").read_text(encoding="utf-8")
        for key, value in EXPECTED_PARAMETERS.items():
            record = current.get(key, {})
            self.expect(record.get("baseline") == value, f"New parameter value is incorrect: {key}")
            self.expect(record.get("source") == NEW_RFC and record.get("decision") == NEW_ADR,
                        f"New parameter source/decision mismatch: {key}")
            self.expect(record.get("new_profile_only") is True and record.get("runtime_config_changed") is False,
                        f"New parameter must remain new-profile-only and non-runtime: {key}")
            self.expect("ADOPTED_DESIGN_NOT_ACTIVATED" in record.get("approval_state", ""),
                        f"New parameter falsely implies activation: {key}")
            self.expect(len(re.findall(re.escape(f"`{key}`"), table)) == 1,
                        f"New parameter missing/duplicated in the human register: {key}")
        return dict(existing_records_preserved=len(prior), new_keys=len(EXPECTED_PARAMETERS),
                    current_records=len(current), comparison="canonical JSON SHA-256 per existing record")

    def check_new_document_links(self):
        helper_path = "docs/maintenance/doclib.py"
        helper = self.path(helper_path)
        if git_blob(helper.read_bytes()) != self.baseline[helper_path]["git_blob_sha"]:
            raise ValueError("Refusing to execute a modified historical link helper")
        spec = importlib.util.spec_from_file_location("preserved_revision_doclib", helper)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        rows, anchors = [], {}
        for source in sorted(path for path in self.added if path.endswith(".md")):
            text = self.path(source).read_text(encoding="utf-8")
            for begin, _, url in module.spans(text):
                kind, target, fragment = module.resolve(source, url)
                status = "ok"
                if kind == "external":
                    status = "external-unchecked"
                elif kind == "malformed":
                    status = "malformed-target"
                elif target != "docs" and not target.startswith("docs/"):
                    status = "outside-package-unchecked"
                else:
                    file = self.root / target
                    if not file.exists():
                        status = "missing-doc-file"
                    elif fragment:
                        if file.is_dir():
                            file = file / "README.md"
                        if not file.is_file() or file.suffix != ".md":
                            status = "fragment-unchecked"
                        else:
                            key = str(file)
                            if key not in anchors:
                                anchors[key] = module.anchors(file.read_text(encoding="utf-8"))
                            if fragment not in anchors[key]:
                                status = "missing-heading"
                row = dict(source=source, line=text.count("\n", 0, begin) + 1,
                           target=target, fragment=fragment, status=status)
                rows.append(row)
                self.expect(status not in module.ERRORS,
                            f"New document link error {status}: {source}:{row['line']} -> {url}")
        return dict(new_markdown_files=sum(path.endswith(".md") for path in self.added),
                    links=len(rows), statuses=dict(Counter(row["status"] for row in rows)),
                    limitations="Existing-document/global link regressions are checked separately; external/code targets unchecked")

    def check_model_record(self):
        result = read_json(self.path(MODEL_RESULT))
        model_data = self.path(MODEL).read_bytes()
        self.expect(result.get("rfc_path") == NEW_RFC and result.get("base_source_sha") == SOURCE_COMMIT,
                    "Saved model result targets a different RFC/source")
        self.expect(result.get("rfc_sha256") == sha256(self.path(NEW_RFC).read_bytes()), "Saved model RFC hash is stale")
        self.expect(result.get("script_sha256") == sha256(model_data), "Saved model script hash is stale")
        methods = []
        for node in ast.parse(model_data.decode("utf-8")).body:
            if isinstance(node, ast.ClassDef) and node.name == "ContractTests":
                methods = [child.name for child in node.body
                           if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)) and child.name.startswith("test_")]
        tests = result["tests"]
        self.expect(len(methods) == len(set(methods)) == len(tests) == 30, "Expected exactly 30 recorded abstract-model tests")
        self.expect(Counter(row["id"] for row in tests) == Counter(methods), "Saved model cases differ from script test names")
        self.expect(all(row.get("outcome") == "passed" for row in tests), "Saved model record has a nonpassing case")
        self.expect(result.get("counts") == dict(run=30, passed=30, failed=0, errors=0, skipped=0)
                    and result.get("all_passed") is True, "Saved model count/summary mismatch")
        scope = result.get("scope", "").lower()
        self.expect("abstract model only" in scope and "not implementation" in scope and "e2e" in scope
                    and "hardware" in scope and "formal proof" in scope, "Model scope does not clearly state its limitations")
        self.expect(bool(result.get("assumptions")) and bool(result.get("not_demonstrated")), "Model record omits assumptions/untested scope")
        return dict(recorded_abstract_tests=30, recorded_passed=30,
                    rfc_sha256=result.get("rfc_sha256"), script_sha256=result.get("script_sha256"),
                    verification="Stored result names/counts/scope and file hashes; model NOT rerun by this checker")

    def run(self) -> dict:
        for name, method in (
                ("pinned_baseline_and_change_scope", self.check_baseline_and_manifest),
                ("source_bytes_and_exact_text_inventory", self.check_all_source_and_current_bytes),
                ("current_front_pointers_preserve_old_bytes", self.check_current_front_pointers_preserve_old_bytes),
                ("20_50_inventory_and_new_registrations", self.check_inventories_and_registration),
                ("56_existing_and_14_new_parameters", self.check_parameters),
                ("new_document_local_links", self.check_new_document_links),
                ("saved_abstract_model_record", self.check_model_record)):
            self.run_check(name, method)
        return dict(revision=REVISION, source_commit=SOURCE_COMMIT, source_tree=SOURCE_TREE,
                    scope="DOCUMENTATION/PRESERVATION ONLY; not native/E2E/performance/protocol proof/activation",
                    generated_at_utc=datetime.now(timezone.utc).isoformat(),
                    baseline_sha256=BASELINE_SHA256, checks=self.checks,
                    counts=dict(checks=len(self.checks), passed=sum(c["status"] == "PASS" for c in self.checks),
                                failed=sum(c["status"] == "FAIL" for c in self.checks), errors=len(self.errors)),
                    all_passed=not self.errors, errors=self.errors,
                    limitations=[
                        "Pinned source-baseline is trusted provenance; this offline checker does not contact GitHub or verify a remote Git commit signature.",
                        "Scope is docs .md/.json/.py text. Non-text assets and runtime code are outside this package check.",
                        "Historical source/checker/manifest bytes cannot be allowlisted. Current book front pointers are removed in memory to recover exact old blobs.",
                        "External URLs and out-of-package code links are not fetched; global historical link regressions use the separate before/after checks.",
                        "Model results are saved abstract evidence, not native or real-model execution, hardware qualification, or public activation."
                    ])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--stdout-only", action="store_true", help="Print JSON without writing revision-results.json")
    args = parser.parse_args()
    audit = Audit(args.root)
    result = audit.run()
    if args.stdout_only:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        output = audit.path(RESULT)
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        for check in result["checks"]:
            print(f"{check['status']}: {check['name']}")
        for error in result["errors"]:
            print(f"ERROR: {error}")
        print(json.dumps(dict(documentation_only=True, **result["counts"],
                              all_passed=result["all_passed"], result=str(output)), ensure_ascii=False))
    return 0 if result["all_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
