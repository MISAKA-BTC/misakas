#!/usr/bin/env python3
"""Regression tests for the documentation validator, not protocol/runtime tests.

Uses only the Python standard library. Each mutation occurs in a temporary copy.
"""
from __future__ import annotations
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import check_integrity


class IntegrityTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix='misaka-adr50-check-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)/'package'
        source_root = Path(__file__).resolve().parents[2]
        self.root.mkdir()
        shutil.copy2(source_root/'README.md', self.root/'README.md')
        manifest = json.loads((source_root/'docs/adr-v2/source-manifest.json').read_text())
        paths = {entry['path'] for field in ('sources', 'preserved_rfc_files', 'unchanged_baseline_files')
                 for entry in manifest[field]}
        for path in paths:
            src = source_root/path
            if src.is_file():
                dst = self.root/path
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        shutil.copytree(source_root/'docs/adr-v2', self.root/'docs/adr-v2',
                        ignore=shutil.ignore_patterns('__pycache__'), dirs_exist_ok=True)
        # Link validation checks existence; only files pinned above need content.
        for src in (source_root/'docs').rglob('*.md'):
            if '.git' in src.parts or 'target' in src.parts:
                continue
            dst = self.root/src.relative_to(source_root)
            if not dst.exists():
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.touch()
        for name in ('scripts/README.md', 'LICENSE', 'CONTRIBUTING.md', 'SECURITY.md'):
            dst = self.root/name
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_root/name, dst)
        self.active = self.root/'docs/adr-v2'
        self.registry = json.loads((self.active/'registry.json').read_text())
        self.manifest = json.loads((self.active/'source-manifest.json').read_text())

    def save_registry(self) -> None:
        (self.active/'registry.json').write_text(json.dumps(self.registry,ensure_ascii=False))

    def save_manifest(self) -> None:
        (self.active/'source-manifest.json').write_text(json.dumps(self.manifest,ensure_ascii=False))

    def assert_rejected(self, contains: str) -> None:
        result=check_integrity.check(self.root)
        self.assertFalse(result['passed'],result)
        self.assertTrue(any(contains in e for e in result['errors']),result['errors'])

    def test_complete_package_passes(self) -> None:
        result = check_integrity.check(self.root)
        self.assertTrue(result['passed'], result['errors'])

    def test_original_source_edit_is_rejected(self) -> None:
        path=self.root/self.manifest['sources'][0]['path']
        path.write_bytes(path.read_bytes()+b'\nchanged\n')
        self.assert_rejected('Original source bytes changed')

    def test_parameter_ledger_change_needs_the_exact_logged_digest(self) -> None:
        path = self.root/'docs/governance/parameter-register.md'
        path.write_bytes(path.read_bytes() + b'\nunrecorded change\n')
        self.assert_rejected('Unchanged baseline files modified or missing: docs/governance/parameter-register.md')

    def test_missing_original_is_rejected(self) -> None:
        (self.root/self.manifest['sources'][0]['path']).unlink()
        self.assert_rejected('Missing original source')

    def test_missing_current_document_is_rejected(self) -> None:
        (self.active/self.registry['documents'][0]['file']).unlink()
        self.assert_rejected('Missing current document')

    def test_missing_owner_is_rejected(self) -> None:
        self.registry['documents'][0]['primary_sources'].pop()
        self.save_registry()
        self.assert_rejected('exactly one primary owner')

    def test_duplicate_owner_is_rejected(self) -> None:
        source=self.registry['documents'][0]['primary_sources'][0]
        self.registry['documents'][1]['primary_sources'].append(source)
        self.save_registry()
        self.assert_rejected('exactly one primary owner')

    def test_changed_appendix_is_rejected(self) -> None:
        src=self.manifest['sources'][0]
        path=self.root/src['owner_path']
        marker=(src['begin_marker']+'\n'+src['fence']+'markdown\n').encode()
        data=path.read_bytes().replace(marker,marker+b'changed\n',1)
        path.write_bytes(data)
        self.assert_rejected('Original appendix bytes changed')

    def test_changed_status_quote_is_rejected(self) -> None:
        src=next(s for s in self.manifest['sources'] if s['declarations'])
        src['declarations'][0]['raw']='Invented adoption'
        self.save_manifest()
        self.assert_rejected('Status declaration mismatch')

    def test_changed_synthesis_is_rejected(self) -> None:
        path=self.active/self.registry['documents'][0]['file']
        text=path.read_text().replace('## 1.','Changed\n\n## 1.',1)
        path.write_text(text)
        self.assert_rejected('Authored-section checksum mismatch')

    def test_missing_rfc_is_rejected(self) -> None:
        path=next((self.root/'docs/rfc-v2').glob('0001-*.md'))
        path.unlink()
        self.assert_rejected('Existing RFC files modified or missing')

    def test_broken_new_link_is_rejected(self) -> None:
        path=self.active/'README.md'
        path.write_text(path.read_text()+'\n[bad](does-not-exist.md)\n')
        self.assert_rejected('Broken new link')

    def test_colliding_numbers_cannot_be_merged(self) -> None:
        self.manifest['collisions']['0173']=self.manifest['collisions']['0173'][:1]
        self.save_manifest()
        self.assert_rejected('Legacy collisions are not preserved')


if __name__=='__main__':
    unittest.main(verbosity=2)
