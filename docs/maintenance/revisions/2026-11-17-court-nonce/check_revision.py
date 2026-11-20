#!/usr/bin/env python3
"""Offline CPN1 revision preservation checks, not native or activation evidence.

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

REVISION = "2026-11-17-court-nonce"
REV = f"docs/maintenance/revisions/{REVISION}"
SOURCE_COMMIT = "2d8212b634880b0967fbf24b00b41936c2322c14"
BASELINE_SHA256 = "f2ba0f3159facc201abd8db778072c517d010fe3e8c23cbc7da40359a2f060d5"
PREVIOUS_PACKAGE_SHA256 = "c5396aef09909bf98b5a40cecb56a66f51a87ec2e4b725e7f09b3e97a7951e7d"
BASELINE = f"{REV}/source-baseline.json"
INTEGRATION = f"{REV}/integration-baseline.json"
MANIFEST = f"{REV}/changes.json"
RESULT = f"{REV}/revision-results.json"
PROPOSED = "PROPOSED_DESIGN_NOT_ACTIVATED"
RFC = "docs/rfc/0057-court-nonce-and-panel-challenges.md"
ADR = "docs/adr/0225-court-nonce-and-panel-challenges.md"
MODEL_SCRIPT = f"{REV}/check_nonce_model.py"
MODEL_RESULT = f"{REV}/model-results.json"
SELF_EXCLUSIONS = {MANIFEST, RESULT, f"{REV}/validation-results.json"}

FRONT_POINTERS = {
    "docs/rfc-v2/0006-panels-and-probabilistic-verification.md",
    "docs/rfc-v2/0007-claims-exec-and-answer-accounting.md",
    "docs/rfc-v2/0012-authenticated-state-storage-and-ibd.md",
    "docs/rfc-v2/0014-court-master-contract.md",
    "docs/rfc-v2/0016-miner-execution-commit-and-material-supply.md",
    "docs/rfc-v2/0017-noninteractive-positive-verification.md",
    "docs/rfc-v2/0018-interactive-negative-court-and-defaults.md",
    "docs/rfc-v2/0019-model-free-node-and-bounded-evidence.md",
    "docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md",
    "docs/rfc-v2/README.md", "docs/adr-v2/README.md", "docs/rfc-v2/MIGRATION.md",
}
APPEND_MARKERS = {
    "docs/maintenance/numbering.md": "CPN1-NUMBERING",
    "docs/governance/parameter-register.md": "CPN1-PARAMETERS",
}
REGISTRIES = {
    "docs/rfc-v2/registry.json": ("active_count", 20, "docs/rfc-v2"),
    "docs/adr-v2/registry.json": ("current_count", 50, "docs/adr-v2"),
}
GENERATED_INDEXES = {
    "docs/rfc/INDEX.md", "docs/rfc/TOPICS.md",
    "docs/adr/INDEX.md", "docs/adr/TOPICS.md",
    "docs/maintenance/catalog.md", "docs/maintenance/decision-catalog.json",
}
PARAMETERS = "docs/governance/parameters.json"
NUMBER_REGISTRY = "docs/maintenance/number-registry.json"
INDEX_GENERATOR = "docs/maintenance/generate_indexes.py"
STRUCTURED_BASELINES = set(REGISTRIES) | {PARAMETERS, NUMBER_REGISTRY, INDEX_GENERATOR}
EDITABLE = (FRONT_POINTERS | set(APPEND_MARKERS) | set(REGISTRIES)
            | GENERATED_INDEXES | {PARAMETERS, NUMBER_REGISTRY, INDEX_GENERATOR})
NEW_DOCUMENTS = {
    RFC, ADR,
    "docs/design/palw/integration/court-nonce-implementation-plan-2026-11-17.md",
}
RECORDS = {"RFC-0057": (RFC, PROPOSED), "ADR-0225": (ADR, PROPOSED)}
NEW_PARAMETER_KEYS = {
    "cpn1_binding_schema", "cpn1_positive_seed_rule", "cpn1_samples_per_actor",
    "cpn1_range_attempt_limit", "cpn1_chunk_staging_limits", "cpn1_role_duty_limits",
    "cpn1_liability_terms", "cpn1_retention_contract",
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
        self.before: dict[str, str] = {}
        self.new_paths: set[str] = set()
        self.generator_authorized = False

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

    def check_integration_baseline(self, baseline: dict) -> None:
        data = self.load(INTEGRATION)
        before = data.get("before", {})
        self.require(data.get("schema_version") == 1 and isinstance(before, dict)
                     and set(before) == STRUCTURED_BASELINES,
                     "integration baseline must contain exactly five authorized preimages")
        self.require(isinstance(data.get("front_pointer_paths"), list)
                     and len(data["front_pointer_paths"]) == 12
                     and set(data["front_pointer_paths"]) == FRONT_POINTERS,
                     "integration baseline front-pointer inventory differs")
        self.require(data.get("append_markers") == APPEND_MARKERS,
                     "integration baseline append-marker inventory differs")
        self.require(isinstance(data.get("new_parameter_keys"), list)
                     and len(data["new_parameter_keys"]) == 8
                     and set(data["new_parameter_keys"]) == NEW_PARAMETER_KEYS,
                     "integration baseline parameter inventory differs")
        for path in sorted(STRUCTURED_BASELINES):
            value = before.get(path)
            if not isinstance(value, str):
                raise ValueError(f"missing original metadata preimage: {path}")
            encoded = value.encode("utf-8")
            pinned = baseline[path]
            if (git_blob(encoded) != pinned["git_blob_sha"]
                    or hashlib.sha256(encoded).hexdigest() != pinned["sha256"]
                    or len(encoded) != pinned["bytes"]):
                raise ValueError(f"unauthenticated original metadata preimage: {path}")
            self.before[path] = value
        self.stats["authenticated_integration_preimages"] = len(self.before)

    def old_object(self, path: str) -> dict:
        value = json.loads(self.before[path], object_pairs_hook=unique_object)
        if not isinstance(value, dict):
            raise ValueError(f"original object required: {path}")
        return value

    def check_parameters(self, data: dict, baseline: dict, expected: str) -> None:
        old = self.old_object(PARAMETERS)
        original = old.get("parameters", [])
        rows = data.get("parameters", [])
        self.require(len(original) == 70, "original parameter inventory is not 70")
        self.require(isinstance(rows, list) and len(rows) == 78,
                     "parameters must contain the original 70 plus exactly eight CPN1 records")
        if not isinstance(rows, list):
            return
        self.require(rows[:70] == original, "original 70 parameter records/order changed")
        keys = [r.get("key") for r in rows]
        self.require(len(keys) == len(set(keys)), "duplicate parameter keys")
        new = rows[70:]
        self.require({r.get("key") for r in new} == NEW_PARAMETER_KEYS and len(new) == 8,
                     "CPN1 parameter key set differs from the eight proposed records")
        for row in new:
            name = row.get("key")
            self.require(str(row.get("approval_state", "")).startswith(PROPOSED),
                         f"CPN1 parameter is not proposed: {name}")
            self.require(row.get("source") == RFC and row.get("decision") == ADR,
                         f"CPN1 parameter source/decision differs: {name}")
            self.require(row.get("runtime_key") is None
                         and row.get("runtime_config_changed") is False
                         and row.get("new_profile_only") is True
                         and row.get("changed_by_reorganization") is False,
                         f"CPN1 parameter changes runtime or scope: {name}")
        restored = copy.deepcopy(data)
        restored["parameters"] = original
        self.restore_equal(PARAMETERS, restored, expected)
        self.stats["original_parameters_preserved"] = sum(a == b for a, b in zip(rows[:70], original))
        self.stats["proposed_parameters"] = len(new)

    def check_registry(self, path: str, data: dict, expected: str) -> None:
        old = self.old_object(path)
        count_key, count, folder = REGISTRIES[path]
        files = sorted(p.name for p in (self.root / folder).glob("[0-9][0-9][0-9][0-9]-*.md"))
        documents = data.get("documents", [])
        self.require(data.get(count_key) == count and len(documents) == count
                     and len(files) == count, f"20/50 inventory changed: {path}")
        self.require(sorted(d.get("file", "") for d in documents) == files,
                     f"registry/current-book filenames differ: {path}")
        self.require(documents == old.get("documents"),
                     f"current document declarations/adoption history changed: {path}")
        self.require(len({d.get("id") for d in documents}) == count,
                     f"duplicate current-book registry IDs: {path}")
        post = data.get("post_consolidation_documents", [])
        previous = old.get("post_consolidation_documents", [])
        self.require(isinstance(post, list) and len(post) == len(previous) + 2
                     and post[:len(previous)] == previous,
                     f"previous post-consolidation records changed: {path}")
        additions = post[len(previous):]
        expected_added = [
            {"kind": kind, "id": identifier, "path": filename, "status": PROPOSED,
             "provenance": "current requested Court nonce RFC"}
            for kind, identifier, filename in [("rfc", "RFC-0057", RFC), ("adr", "ADR-0225", ADR)]
        ]
        self.require(additions == expected_added, f"unexpected new proposal records: {path}")
        latest = {"revision": REVISION, "rfc_record": RFC, "decision_record": ADR,
                  "status": PROPOSED, "runtime_changed": False, "activation_changed": False}
        self.require(data.get("latest_proposal") == latest,
                     f"latest_proposal must remain exactly proposed and non-runtime: {path}")
        restored = copy.deepcopy(data)
        restored["post_consolidation_documents"] = previous
        if "latest_proposal" in old:
            restored["latest_proposal"] = old["latest_proposal"]
        else:
            restored.pop("latest_proposal", None)
        self.restore_equal(path, restored, expected)
        self.stats[folder.rsplit("/", 1)[-1] + "_books"] = len(files)
        self.stats[folder.rsplit("/", 1)[-1] + "_previous_post_records_preserved"] = len(previous)

    def check_number_registry(self, data: dict, expected: str) -> None:
        old = self.old_object(NUMBER_REGISTRY)
        previous = old.get("subsequent_local_allocations", [])
        rows = data.get("subsequent_local_allocations", [])
        self.require(isinstance(rows, list) and len(rows) == len(previous) + 1
                     and rows[:len(previous)] == previous,
                     "number registry must append only one allocation without modifying previous data")
        if len(rows) == len(previous) + 1:
            new = rows[-1]
            self.require(new.get("date") == "2026-11-17" and new.get("rfc") == "0057"
                         and new.get("adr") == "0225" and new.get("status") == PROPOSED
                         and new.get("rfc_path") == RFC and new.get("adr_path") == ADR
                         and isinstance(new.get("audit"), str) and bool(new["audit"])
                         and set(new) == {"date", "rfc", "rfc_path", "adr", "adr_path", "status", "audit"},
                         "wrong CPN1 number allocation/status")
        restored = copy.deepcopy(data)
        restored["subsequent_local_allocations"] = previous
        self.restore_equal(NUMBER_REGISTRY, restored, expected)

    def check_index_generator(self, data: bytes, expected: str) -> None:
        wanted = {("rfc", "0057"), ("adr", "0225")}
        found, original = [], []
        errors_before = len(self.errors)
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
            if safe:
                self.require(nodes[0].value.value.startswith("Proposed design")
                             and "未有効化" in nodes[0].value.value,
                             "new decision index status must remain Proposed and inactive")
            found.append(key)
        self.require(set(found) == wanted and len(found) == 2,
                     "index generator must add exactly two scoped status mappings")
        self.require(git_blob(b"".join(original)) == expected,
                     "index generator changed beyond the two status mappings")
        self.generator_authorized = len(self.errors) == errors_before

    def check_generated_indexes(self) -> None:
        """Run only the authenticated generator with all six writes captured in memory."""
        if not self.generator_authorized:
            self.errors.append("cannot verify generated indexes using an unauthorized generator")
            return
        audit = self
        captured = {}

        class ReadAndCapturePath:
            def __init__(self, value):
                self.path = value.path if isinstance(value, ReadAndCapturePath) else Path(value)

            def __truediv__(self, other):
                return ReadAndCapturePath(self.path / other)

            def __lt__(self, other):
                return self.path < other.path

            def resolve(self):
                return ReadAndCapturePath(self.path.resolve())

            @property
            def parents(self):
                return tuple(ReadAndCapturePath(p) for p in self.path.parents)

            @property
            def parent(self):
                return ReadAndCapturePath(self.path.parent)

            @property
            def name(self):
                return self.path.name

            @property
            def stem(self):
                return self.path.stem

            def glob(self, pattern):
                return (ReadAndCapturePath(p) for p in self.path.glob(pattern))

            def rglob(self, pattern):
                return (ReadAndCapturePath(p) for p in self.path.rglob(pattern))

            def relative_to(self, other):
                raw = other.path if isinstance(other, ReadAndCapturePath) else other
                return self.path.relative_to(raw)

            def read_text(self):
                relative = self.path.relative_to(audit.root).as_posix()
                return audit.read(relative).decode("utf-8")

            def write_text(self, value):
                relative = self.path.relative_to(audit.root).as_posix()
                if relative not in GENERATED_INDEXES or relative in captured:
                    raise ValueError(f"unexpected generated destination: {relative}")
                captured[relative] = value.encode("utf-8")
                return len(value)

        tree = ast.parse(self.read(INDEX_GENERATOR).decode("utf-8"), filename=INDEX_GENERATOR)
        # The generator's bytes were authenticated above. Only Path's import is
        # replaced, so no file write from the generator reaches the filesystem.
        imports = [n for n in tree.body if isinstance(n, ast.ImportFrom) and n.module == "pathlib"]
        self.require(len(imports) == 1 and len(imports[0].names) == 1
                     and imports[0].names[0].name == "Path"
                     and imports[0].names[0].asname is None,
                     "unexpected authenticated generator Path import")
        tree.body = [n for n in tree.body if n not in imports]
        namespace = {"Path": ReadAndCapturePath, "__file__": str(self.root / INDEX_GENERATOR),
                     "__name__": "cpn1_captured_generator", "print": lambda *args, **kwargs: None}
        exec(compile(ast.fix_missing_locations(tree), INDEX_GENERATOR, "exec"), namespace)
        self.require(set(captured) == GENERATED_INDEXES, "generated destination inventory differs")
        for path, expected in captured.items():
            self.require(self.read(path) == expected, f"generated file is stale or modified: {path}")
            if path.endswith(".md"):
                self.new_text[path] = self.read(path).decode("utf-8")
        catalog = self.load("docs/maintenance/decision-catalog.json")
        records = catalog.get("records", [])
        for identifier, (path, _) in RECORDS.items():
            matched = [r for r in records if r.get("path") == path]
            self.require(len(matched) == 1 and matched[0].get("status", "").startswith("Proposed design"),
                         f"new decision catalog status differs: {identifier}")
        self.stats["generated_files_verified_in_memory"] = len(captured)

    def check_model_result(self) -> None:
        result = self.load(MODEL_RESULT)
        self.require(result.get("schema") == "misaka-cpn1-abstract-model-results-v1"
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
        spec = importlib.util.spec_from_file_location("cpn1_pinned_doclib", self.root / helper_path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        counts = {"checked_local": 0, "external_unchecked": 0,
                  "outside_docs_unchecked": 0, "deferred_self_output": 0}
        anchor_cache = {}
        for source, text in sorted(self.new_text.items()):
            for _, _, url in module.spans(text):
                kind, target, fragment = module.resolve(source, url)
                if source in GENERATED_INDEXES and target not in self.new_paths:
                    # Old generated links are covered by deterministic generation,
                    # not reclassified as new handwritten links.
                    continue
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
                         and source.get("previous_package_sha256") == PREVIOUS_PACKAGE_SHA256,
                         "pinned source commit/previous delivery mismatch")
            records = source.get("files", [])
            self.require(isinstance(records, list) and len(records) == 1975,
                         "pinned prior-delivery inventory must contain 1975 files")
            baseline = {r["path"]: r for r in records}
            self.require(len(baseline) == len(records)
                         and all(valid_doc_path(p) for p in baseline),
                         "invalid or duplicate pinned input paths")
            self.check_integration_baseline(baseline)
            actual = set()
            for p in (self.root / "docs").rglob("*"):
                if "__pycache__" in p.parts or p.suffix == ".pyc":
                    continue
                if p.is_symlink():
                    self.errors.append(f"symlink in documentation package: {p.relative_to(self.root)}")
                elif p.is_file():
                    actual.add(p.relative_to(self.root).as_posix())
            self.new_paths = actual - set(baseline)
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
                    self.require(len(data) == record["bytes"]
                                 and hashlib.sha256(data).hexdigest() == record["sha256"],
                                 f"baseline size/SHA256 mismatch: {path}")
                    preserved += 1
                    continue
                changed.add(path)
                try:
                    if path in FRONT_POINTERS:
                        _, self.new_text[path] = restore_marked(data, "CPN1-PROPOSAL", expected, "front")
                        self.require(PROPOSED in self.new_text[path],
                                     f"front pointer omits proposal-only state: {path}")
                    elif path in APPEND_MARKERS:
                        _, self.new_text[path] = restore_marked(data, APPEND_MARKERS[path], expected, "append")
                        self.require(PROPOSED in self.new_text[path],
                                     f"appendix omits proposal-only state: {path}")
                    elif path == PARAMETERS:
                        self.check_parameters(self.load(path), source, expected)
                    elif path in REGISTRIES:
                        self.check_registry(path, self.load(path), expected)
                    elif path == NUMBER_REGISTRY:
                        self.check_number_registry(self.load(path), expected)
                    elif path == INDEX_GENERATOR:
                        self.check_index_generator(data, expected)
                    elif path in GENERATED_INDEXES:
                        pass  # Exact generation is verified below, without writes.
                    else:
                        self.errors.append(f"immutable original source changed: {path}")
                except (ValueError, KeyError, TypeError, SyntaxError) as exc:
                    self.errors.append(f"preservation check failed for {path}: {exc}")
            self.require(changed == EDITABLE and len(changed) == 25,
                         f"only the 25 additive integration paths may change: {sorted(changed ^ EDITABLE)}")
            for path in self.new_paths:
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
            review = self.read(f"{REV}/REVIEW.md").decode("utf-8")
            self.require(hashlib.sha256(self.read(RFC)).hexdigest() in review,
                         "review does not identify the latest RFC hash")
            self.stats.update({
                "source_documents": len(baseline),
                "byte_identical_source_documents": preserved,
                "front_pointer_files": len(changed & FRONT_POINTERS),
                "historical_checkers_and_appendices_editable": False,
                "prior_revision_files_exact": sum(p.startswith("docs/maintenance/revisions/")
                                                  and p not in changed and p in actual for p in baseline),
                "deleted_source_files": len(set(baseline) - actual),
            })
            self.check_generated_indexes()
            self.check_manifest(baseline, actual, changed)
            self.require(self.load(MANIFEST).get("deleted") == [], "manifest must record no deleted files")
            self.check_model_result()
            self.check_links(baseline)
        except (OSError, ValueError, KeyError, TypeError, AttributeError, ImportError, SyntaxError) as exc:
            self.errors.append(f"audit could not complete: {type(exc).__name__}: {exc}")
        return {
            "schema": "misaka-cpn1-revision-preservation-v1",
            "generated_at_utc": datetime.now(timezone.utc).isoformat(),
            "scope": "DOCS-ONLY REVISION PRESERVATION; NOT native, E2E, performance, or activation evidence",
            "revision": REVISION,
            "source_commit": SOURCE_COMMIT,
            "source_baseline_sha256": BASELINE_SHA256,
            "comparison_basis": "previous delivered RFC-0056 overlay; pinned 1975-file inventory",
            "passed": not self.errors,
            "errors": self.errors,
            "stats": self.stats,
            "runtime_executed": False,
            "native_code_audited_by_this_checker": False,
            "limitations": [
                "The documentation package and change manifest are checked; unrelated native files and live runtime are not audited.",
                "Historical numbered RFC/ADR, appendices, prior revisions/checkers, and prior 0055/0056 deliverables are preserved, not re-certified.",
                "New/marked Markdown and newly generated target links use a pinned limited parser; external URLs and paths outside docs are not re-verified.",
                "Six generated files are compared with the authenticated generator's output captured in memory; no generated file is rewritten.",
                "Fresh passing abstract-model records are checked, not re-run. Native integration, hardware capacity, beacon security, independent labor, and activation are outside this checker.",
                "The pinned source and historical checker/link baselines are never rewritten.",
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
