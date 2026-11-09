#!/usr/bin/env python3
"""Check this documentation revision against the supplied immutable ZIP snapshot.

This is a preservation, inventory and local-link regression check, not a proof of
protocol semantics, runtime correctness, measured performance or activation.
No original manifest, historical checker or previous result is rewritten.
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

sys.dont_write_bytecode = True
REVISION = "2026-11-09-palw-execution-integrity"
REV_DIR = f"docs/maintenance/revisions/{REVISION}"
MANIFEST = f"{REV_DIR}/input-manifest.json"
LINKS = f"{REV_DIR}/input-link-issues.json"
MANIFEST_SHA = "4abf0f014d45e7865359a2cede2bab37fd45dc542d53ba712ac599bb7b4c59d2"
LINKS_SHA = "1a36579e7d37293b0758c70efa35a3fc0847e7725dfb3b0e76d46b2d100381f6"
NEW_RFC = "docs/rfc/0049-palw-execution-integrity-and-unilateral-refutation.md"
NEW_ADR = "docs/adr/0217-palw-execution-integrity-and-unilateral-refutation.md"
POLICY = "docs/design/palw/consensus/public-adjudication-policy-v0.3.md"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical_digest(value):
    return digest(json.dumps(value, ensure_ascii=False, sort_keys=True,
                             separators=(",", ":")).encode("utf-8"))


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def numbered(paths, directory):
    return {p for p in paths if re.fullmatch(re.escape(directory) + r"/\d{4}-[^/]+\.md", p)}


class Audit:
    def __init__(self, root):
        self.root = root
        self.errors = []
        self.checks = []
        self.files = {}
        self.hashes = {}
        self.changes = []

    def expect(self, condition, error):
        if not condition:
            self.errors.append(error)

    def path(self, rel):
        return self.root / rel

    def run_check(self, name, action):
        before = len(self.errors)
        try:
            details = action()
        except (OSError, ValueError, KeyError, TypeError, ImportError) as exc:
            self.errors.append(f"{type(exc).__name__}: {exc}")
            details = {}
        self.checks.append({"name": name, "status": "PASS" if len(self.errors) == before else "FAIL",
                            "details": details, "errors": self.errors[before:]})

    def check_input(self):
        self.expect(digest(self.path(MANIFEST).read_bytes()) == MANIFEST_SHA,
                    "The exact input manifest changed.")
        self.expect(digest(self.path(LINKS).read_bytes()) == LINKS_SHA,
                    "The input link-issue evidence changed.")
        self.manifest = read_json(self.path(MANIFEST))
        self.baseline_links = read_json(self.path(LINKS))
        self.files = self.manifest["files"]
        self.expect(len(self.files) == self.manifest["file_count"] == 1885,
                    "Expected 1885 files in the exact supplied archive.")
        self.expect(len(self.baseline_links) == 974, "Expected 974 input link issues.")
        self.expect(self.manifest["input_archive"] == "misaka_docs_rfc20_exec_1bps_20261108(2).zip",
                    "Unexpected input archive identity.")
        for rel, expected in self.files.items():
            p = self.path(rel)
            if not p.is_file() or p.is_symlink():
                self.errors.append(f"Input file missing or no longer regular: {rel}")
                continue
            current = digest(p.read_bytes())
            self.hashes[rel] = current
            if expected != current:
                self.changes.append({"path": rel, "input_sha256": expected,
                                     "current_sha256": current})
        return {"input_files": len(self.files), "present_input_files": len(self.hashes),
                "changed_input_files": len(self.changes),
                "unchanged_input_files": len(self.hashes) - len(self.changes)}

    def check_preservation(self):
        protected = self.manifest["protected_paths"]
        for rel in protected:
            self.expect(self.hashes.get(rel) == self.files[rel], f"Protected bytes changed: {rel}")
        old_revisions = {p for p in self.files if p.startswith("docs/maintenance/revisions/")}
        actual_old = {p.relative_to(self.root).as_posix()
                      for p in self.path("docs/maintenance/revisions").rglob("*")
                      if p.is_file() and not p.is_relative_to(self.path(REV_DIR))}
        self.expect(old_revisions == actual_old, "Past revision directory inventory changed.")
        for rel, expected in self.manifest["historical_rfc_appendices"].items():
            data = self.path(rel).read_bytes()
            marker = "## 原条文の統合収録".encode("utf-8")
            self.expect(marker in data and digest(data[data.index(marker):]) == expected,
                        f"Historical RFC appendix changed: {rel}")
        pr = self.path("docs/PRINCIPLES.md").read_bytes()
        start = pr.index(b'<a id="model-free-adversarial-verification"')
        end = pr.index(b'<a id="exec-credit-one-bps-and-court-capacity"', start)
        self.expect(digest(pr[:start]) == self.manifest["principles_unchanged_prefix_sha256"],
                    "PRINCIPLES before section 13 changed, including sections 8/9.")
        self.expect(digest(pr[end:]) == self.manifest["principles_unchanged_suffix_sha256"],
                    "PRINCIPLES section 14 or later changed.")
        return {"protected_input_files": len(protected), "previous_revision_files": len(old_revisions),
                "unchanged_historical_rfc_appendices": len(self.manifest["historical_rfc_appendices"]),
                "principles_edit_scope": "section 13 only", "parameters_changed": False}

    def actual_numbered(self, directory):
        return {p.relative_to(self.root).as_posix() for p in self.path(directory).glob("[0-9][0-9][0-9][0-9]-*.md")}

    def check_inventories(self):
        self.rfc = read_json(self.path("docs/rfc-v2/registry.json"))
        self.adr = read_json(self.path("docs/adr-v2/registry.json"))
        for registry, directory, count, key, ns in ((self.rfc,"docs/rfc-v2",20,"active_count","RFC-v2"),
                                                   (self.adr,"docs/adr-v2",50,"current_count","ADR-v2")):
            docs = registry["documents"]
            self.expect(registry[key] == len(docs) == count, f"{ns}: incorrect book count.")
            self.expect({d["id"] for d in docs} == {f"{ns}-{n:04d}" for n in range(1,count+1)},
                        f"{ns}: book IDs are missing or duplicated.")
            self.expect(self.actual_numbered(directory) == {directory+"/"+d["file"] for d in docs},
                        f"{ns}: registered file set differs from actual file set.")
            for d in docs:
                self.expect(self.path(directory+"/"+d["file"]).read_text().startswith("# "+d["id"]+" — "),
                            f"Incorrect current book heading: {d['file']}")
        self.expect(canonical_digest(self.rfc["documents"]) == self.manifest["baseline"]["rfc_documents_sha256"],
                    "Existing RFC ownership/document records changed.")
        self.expect(canonical_digest(self.adr["documents"]) == self.manifest["baseline"]["adr_documents_sha256"],
                    "Existing ADR book records or authored-section digests changed.")
        for directory, added, count in (("docs/rfc", NEW_RFC,49),("docs/adr",NEW_ADR,212)):
            old = numbered(self.files,directory)
            self.expect(len(old) == count and added not in old, f"Unexpected {directory} input inventory.")
            self.expect(self.actual_numbered(directory) == old | {added},
                        f"Only the one recorded adoption may be added to {directory}.")
        for rel,identifier in ((NEW_RFC,"RFC-0049"),(NEW_ADR,"ADR-0217")):
            self.expect(self.path(rel).read_text().startswith("# "+identifier+" — "),
                        f"Incorrect adoption identifier: {rel}")
        self.expect(self.path(POLICY).is_file(),"Public adjudication policy v0.3 is missing.")
        return {"rfc_books":20,"adr_books":50,"legacy_rfcs":50,"legacy_adrs":213,
                "new_numbered_adoption_records":[NEW_RFC,NEW_ADR]}

    def check_registration(self):
        added = [{"kind":"rfc","id":"RFC-0049","path":NEW_RFC},
                 {"kind":"adr","id":"ADR-0217","path":NEW_ADR}]
        def rows(v):return Counter(json.dumps(x,sort_keys=True) for x in v)
        self.expect(rows(self.rfc["post_consolidation_documents"]) == rows(self.manifest["baseline"]["rfc_post_records"]+added),
                    "RFC post-consolidation records must preserve all inputs and add exactly 0049/0217.")
        self.expect(rows(self.adr["post_consolidation_documents"]) == rows(self.manifest["baseline"]["adr_post_records"]+added),
                    "ADR post-consolidation records must preserve all inputs and add exactly 0049/0217.")
        mapping = read_json(self.path("docs/rfc-v2/adoption-map.json"))
        entries = mapping["sources"]
        oldpaths = numbered(self.files,"docs/rfc")
        prior = sorted([x for x in entries if x["source_path"] in oldpaths],key=lambda x:x["source_path"])
        self.expect(canonical_digest(prior) == self.manifest["baseline"]["adoption_rows_sha256"],
                    "One or more of the 49 existing adoption-map rows changed.")
        self.expect(Counter(x["source_path"] for x in entries) == Counter(oldpaths | {NEW_RFC}),
                    "Adoption-map must cover all 50 complete RFC source paths once.")
        self.expect(mapping["active_rfc_count"] == 20 and mapping["current_legacy_rfc_file_count"] == 50
                    and mapping["revision_input_legacy_rfc_file_count"] == 49,
                    "Adoption-map current and revision-input counts differ.")
        owner_paths = {x["id"]:"docs/rfc-v2/"+x["file"] for x in self.rfc["documents"]}
        for entry in entries:
            self.expect(entry["current_owner_path"] == owner_paths.get(entry["current_owner_id"]),
                        f"Incorrect owner mapping: {entry['source_path']}")
            for ident,path in zip(entry["related_new_profile_ids"],entry["related_new_profile_paths"]):
                self.expect(owner_paths.get(ident) == path,f"Incorrect related owner: {entry['source_path']}")
        new = next(x for x in entries if x["source_path"] == NEW_RFC)
        self.expect(new["input_sha256"] is None and new["new_adoption_record"] is True
                    and new["source_sha256"] == digest(self.path(NEW_RFC).read_bytes())
                    and new["current_owner_id"] == "RFC-v2-0020",
                    "RFC-0049 source digest, new-record identity or owner is wrong.")
        table=[];rx=re.compile(r"\[[^\]\n]*\]\(([^\s)]+)\)")
        for line in self.path("docs/rfc-v2/MAPPING.md").read_text().splitlines():
            cells=line.split("|")
            if len(cells)<4:continue
            src,dst=rx.findall(cells[1]),rx.findall(cells[2])
            if len(src)!=1 or len(dst)!=1:continue
            sp=posixpath.normpath(posixpath.join("docs/rfc-v2",src[0]))
            dp=posixpath.normpath(posixpath.join("docs/rfc-v2",dst[0]))
            if sp in oldpaths | {NEW_RFC}:table.append((sp,dp))
        self.expect(Counter(table) == Counter((x["source_path"],x["current_owner_path"]) for x in entries),
                    "Human-readable MAPPING rows differ from adoption-map.")
        catalog=read_json(self.path("docs/maintenance/decision-catalog.json"))["records"]
        for kind in ("rfc","adr"):
            self.expect({x["path"] for x in catalog if x["kind"]==kind} == self.actual_numbered("docs/"+kind),
                        f"The {kind} decision catalog is stale; generate indexes after authors finish.")
        return {"unchanged_mapping_rows":49,"new_mapping_rows":1,"rfc_post_records":18,"adr_post_records":23}

    def check_links(self):
        p=self.path("docs/maintenance/doclib.py")
        self.expect(digest(p.read_bytes()) == self.files["docs/maintenance/doclib.py"],
                    "The preserved link scanner changed.")
        spec=importlib.util.spec_from_file_location("preserved_palw_doclib",p)
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        current=[r for r in module.scan(self.root) if r["status"] in module.ERRORS]
        old=Counter(module.key(r) for r in self.baseline_links)
        actual=Counter(module.key(r) for r in current)
        new=actual-old
        for key,count in new.items():self.errors.append(f"New/increased local link error x{count}: {key}")
        return {"input_error_occurrences":sum(old.values()),"current_error_occurrences":sum(actual.values()),
                "new_or_increased":sum(new.values()),"fixed_occurrences":sum((old-actual).values()),
                "external_urls_and_outside_package_targets":"unchecked"}

    def run(self):
        for name,action in (("exact_input_and_no_deletions",self.check_input),
                            ("historical_evidence_and_parameter_preservation",self.check_preservation),
                            ("current_inventory_and_new_adoption_records",self.check_inventories),
                            ("mapping_registry_and_index_consistency",self.check_registration),
                            ("local_link_regression_against_exact_input",self.check_links)):
            self.run_check(name,action)
        actual={p.relative_to(self.root).as_posix() for p in self.path("docs").rglob("*") if p.is_file()}
        return {"schema":"misaka.docs.execution-integrity-audit.v1","revision":REVISION,
                "checked_at_utc":datetime.now(timezone.utc).isoformat(),
                "status":"PASS" if not self.errors else "FAIL","ok":not self.errors,
                "error_count":len(self.errors),"errors":self.errors,"checks":self.checks,
                "input_changes":self.changes,"new_files":sorted(actual-set(self.files)),
                "scope":"Exact-input documentation preservation, registration and local-link regressions only.",
                "limitations":["Does not prove protocol semantics, runtime safety, measured throughput or activation.",
                               "Historical snapshot checker failures are reported separately, never baseline-rewritten.",
                               "Does not validate external URLs or code outside the supplied docs tree."]}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,default=Path(__file__).resolve().parents[4])
    parser.add_argument("--json",type=Path)
    args=parser.parse_args();root=args.root.resolve();audit=Audit(root);report=audit.run()
    if args.json:
        output=args.json.resolve()
        if output.is_relative_to(root):
            rel=output.relative_to(root).as_posix()
            if rel in audit.files or rel in {MANIFEST,LINKS,f"{REV_DIR}/check_revision.py"}:
                parser.error("--json must not overwrite input sources or pinned evidence.")
        output.parent.mkdir(parents=True,exist_ok=True)
        output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
    print(f"{report['status']}: {REVISION}; {report['error_count']} documentation errors.")
    for check in report["checks"]:
        print(f"  {check['status']}: {check['name']}")
        for error in check["errors"]:print("    "+error)
    print("Scope: documentation only; runtime, protocol closure and activation are not certified here.")
    return 0 if report["ok"] else 1


if __name__=="__main__":
    raise SystemExit(main())
