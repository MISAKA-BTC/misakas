#!/usr/bin/env python3
"""Check the editorial RFC migration; never execute project or runtime code.

Run from any directory: python3 docs/rfc-v2/check_integrity.py
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    active = Path(__file__).resolve().parent
    root = active.parents[1]
    registry = json.loads((active / 'registry.json').read_text(encoding='utf-8'))
    manifest = json.loads((active / 'source-map.json').read_text(encoding='utf-8'))
    errors: list[str] = []
    docs = registry['documents']
    actual = sorted(p.name for p in active.glob('[0-9][0-9][0-9][0-9]-*.md'))
    expected = sorted(d['file'] for d in docs)
    if len(docs) != 15 or actual != expected:
        errors.append('The active RFC inventory does not contain exactly the expected 15 books.')
    expected_ids = {f'RFC-{n:04d}' for n in range(1, 42)}
    sources = manifest['sources']
    if len(sources) != 41 or {s['old_id'] for s in sources} != expected_ids:
        errors.append('The v1 source manifest must cover exactly RFC-0001 through RFC-0041.')
    primary_ids = [v1 for doc in docs for v1 in doc['primary_v1']]
    if len(primary_ids) != 41 or set(primary_ids) != expected_ids:
        errors.append('Every v1 RFC must have exactly one primary v2 owner.')
    total_lines = 0
    segments = 0
    fragment_pattern = re.compile(
        r'<a id="(source-v1-\d{4}-L\d{5})"></a>\n\n'
        r'### [^\n]+\n\n出典: [^\n]+\n\n(.*?)'
        r'(?=\n---\n\n<a id="source-v1-|\Z)', re.S)
    fragments = {}
    for d in docs:
        path = active / d['file']
        if not path.exists():
            errors.append(f'Missing book: {path.name}')
            continue
        text = path.read_text(encoding='utf-8')
        if not text.startswith('# ' + d['id'] + ' — '):
            errors.append(f'Incorrect top-level identifier: {path.name}')
        for anchor, body in fragment_pattern.findall(text):
            key = (str(path.relative_to(root)), anchor)
            if key in fragments:
                errors.append(f'Duplicate anchor: {key}')
            fragments[key] = body.rstrip()
    for source in sources:
        path = root / source['path']
        if not path.is_file() or digest(path.read_bytes()) != source['sha256']:
            errors.append(f'Original bytes changed or missing: {source["old_id"]}')
        cursor = 1
        for seg in sorted(source['segments'], key=lambda x: x['start_line']):
            segments += 1
            if seg['start_line'] != cursor or seg['end_line'] < seg['start_line']:
                errors.append(f'Gap/overlap in source line allocation: {source["old_id"]}')
            cursor = seg['end_line'] + 1
            key = (seg['path'], seg['anchor'])
            if key not in fragments:
                errors.append(f'Missing source fragment: {key}')
            elif 'fragment_sha256' in seg and digest(fragments[key].encode('utf-8')) != seg['fragment_sha256']:
                errors.append(f'Changed source fragment: {key}')
        if cursor != source['lines'] + 1:
            errors.append(f'Incomplete source coverage: {source["old_id"]}')
        total_lines += source['lines']
    # Check newly authored links only. Historical source excerpts retain old source links,
    # which may require the original full repository or carry pre-existing broken anchors.
    checked_links = 0
    own_files = [active / d['file'] for d in docs] + [active / n for n in ('README.md', 'MAPPING.md', 'MIGRATION.md')]
    rx = re.compile(r'\[[^\]\n]*\]\(([^\s)]+)\)')
    for path in own_files:
        text = path.read_text(encoding='utf-8').split('## 原条文の統合収録', 1)[0]
        for target in rx.findall(text):
            url = urlsplit(target)
            if url.scheme or target.startswith('//'):
                continue
            if not url.path:
                continue
            dst = path.parent / unquote(url.path)
            checked_links += 1
            if not dst.exists():
                errors.append(f'Broken newly authored link: {path.name}: {target}')
    if errors:
        for err in errors:
            print('FAIL:', err, file=sys.stderr)
        return 1
    print(f'PASS: {len(docs)} active RFC books; {len(sources)} unchanged v1 sources; '
          f'{total_lines} source lines assigned exactly once; {segments} source fragments; '
          f'{checked_links} newly authored links exist.')
    print('Scope: documentation integrity only. No Rust build, node execution, or runtime activation was performed.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
