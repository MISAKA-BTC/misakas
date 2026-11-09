#!/usr/bin/env python3
"""Validate only the editorial ADR/RFC consolidation, without runtime execution.

Usage: python3 docs/adr-v2/check_integrity.py [--root /path/to/package] [--json /path/to/result.json]
Exit status 0 means the documentation integrity checks passed, not that a design
is approved, implemented, activated, or safe for a live network.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from collections import Counter, defaultdict
from pathlib import Path
from urllib.parse import unquote, urlsplit


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def outside_fences(text: str) -> str:
    """Remove fenced historical payloads before checking newly authored links."""
    result: list[str] = []
    active: tuple[str, int] | None = None
    for line in text.splitlines():
        if active is None:
            start = re.match(r'^\s*(`{3,}|~{3,})', line)
            if start:
                run = start.group(1)
                active = (run[0], len(run))
            else:
                result.append(line)
        else:
            char, length = active
            if re.match(r'^\s*' + re.escape(char) + '{' + str(length) + r',}\s*$', line):
                active = None
    return '\n'.join(result)


# An accepted post-import RFC may update navigation/registry metadata. Keep every
# original digest in source-manifest.json; authorize only exact, separately logged
# replacements of these metadata files. Numbered RFC/ADR payloads are never waivable.
POST_INTEGRATION_METADATA = frozenset({
    'docs/rfc/INDEX.md', 'docs/rfc/TOPICS.md', 'docs/rfc-v2/README.md',
    'docs/rfc-v2/registry.json', 'docs/maintenance/numbering.md',
    'docs/governance/parameter-register.md',
})

def metadata_change_matches(path: str, baseline: str, actual: str, change: dict) -> bool:
    return (isinstance(actual,str) and re.fullmatch(r'[0-9a-f]{64}',actual) is not None
            and path in POST_INTEGRATION_METADATA and change.get('path') == path
            and change.get('baseline_sha256') == baseline
            and change.get('updated_sha256') == actual
            and bool(change.get('reason')))


def check(root: Path) -> dict:
    root = root.resolve()
    current = root / 'docs/adr-v2'
    errors: list[str] = []
    stats: dict[str, object] = {}
    try:
        registry = json.loads((current / 'registry.json').read_text(encoding='utf-8'))
        manifest = json.loads((current / 'source-manifest.json').read_text(encoding='utf-8'))
    except (OSError, ValueError) as exc:
        return {'passed': False, 'errors': [f'Cannot read registry/manifest: {exc}'], 'stats': stats}

    docs = registry.get('documents', [])
    expected_names = [d['file'] for d in docs]
    actual_names = sorted(p.name for p in current.glob('[0-9][0-9][0-9][0-9]-*.md'))
    expected_ids = {f'ADR-v2-{n:04d}' for n in range(1, 51)}
    if len(docs) != 50 or len(set(expected_names)) != 50 or set(d['id'] for d in docs) != expected_ids or sorted(expected_names) != actual_names:
        errors.append('Current ADR inventory must contain exactly ADR-v2-0001 through ADR-v2-0050.')
    stats['current_adr_documents'] = len(actual_names)

    sources = manifest.get('sources', [])
    source_keys = [s['source_key'] for s in sources]
    legacy_actual = sorted(p.name for p in (root/'docs/adr').glob('[0-9][0-9][0-9][0-9]-*.md'))
    assigned = [s for d in docs for s in d.get('primary_sources', [])]
    later = registry.get('post_consolidation_documents', [])
    later_adr = sorted(Path(d['path']).name for d in later if d['kind'] == 'adr')
    if len(sources) != 204 or len(set(source_keys)) != 204 or sorted(source_keys + later_adr) != legacy_actual:
        errors.append('Source inventory must include all 204 legacy files by complete filename.')
    for d in later:
        if d['kind'] not in {'adr', 'rfc'} or not (root/d['path']).is_file():
            errors.append(f'Missing post-consolidation document: {d["path"]}')
    stats['post_consolidation_documents'] = len(later)
    if len(assigned) != 204 or Counter(assigned) != Counter(source_keys):
        errors.append('Each legacy source must have exactly one primary owner, without gaps or duplicates.')
    actual_owner: dict[str, list[dict]] = defaultdict(list)
    for d in docs:
        for key in d.get('primary_sources', []):
            actual_owner[key].append(d)
    by_number: dict[str, list[str]] = defaultdict(list)
    for key in source_keys:
        by_number[key[:4]].append(key)
    collisions = {k: sorted(v) for k,v in by_number.items() if len(v)>1}
    if collisions != {k: sorted(v) for k,v in manifest.get('collisions', {}).items()} or set(collisions) != {'0026','0173','0193'}:
        errors.append('Legacy collisions are not preserved as distinct complete filenames.')
    missing = [f'{n:04d}' for n in range(1,210) if f'{n:04d}' not in by_number]
    if missing != manifest.get('missing_numbers_in_0001_0209'):
        errors.append('Legacy missing-number inventory changed.')
    stats['legacy_source_files'] = len(sources)
    stats['legacy_distinct_numbers'] = len(by_number)
    stats['legacy_duplicate_numbers'] = sorted(collisions)
    stats['legacy_missing_numbers'] = missing

    book_data: dict[str, bytes] = {}
    appendix_start = '## 原条文の完全収録（履歴）'.encode('utf-8')
    for d in docs:
        p = current/d['file']
        if not p.is_file():
            errors.append(f'Missing current document: {d["file"]}')
            continue
        data = p.read_bytes()
        book_data['docs/adr-v2/'+d['file']] = data
        if not data.startswith(('# '+d['id']+' — ').encode()):
            errors.append(f'Incorrect document identifier: {d["file"]}')
        if data.count(appendix_start) != 1:
            errors.append(f'Missing or repeated appendix separator: {d["file"]}')
        authored = data.split(appendix_start,1)[0]
        if digest(authored) != d['authored_section_sha256']:
            errors.append(f'Authored-section checksum mismatch: {d["file"]}')
        if re.search(rb'\{[ARS][0-9]+[a-z]?\}', authored):
            errors.append(f'Unresolved editorial link placeholder: {d["file"]}')

    total_lines = 0
    total_bytes = 0
    appendix_verified = 0
    declarations_verified = 0
    for s in sources:
        p = root/s['path']
        if not p.is_file():
            errors.append(f'Missing original source: {s["source_key"]}')
            continue
        data = p.read_bytes()
        lines = data.decode('utf-8').splitlines()
        total_lines += len(lines)
        total_bytes += len(data)
        if digest(data) != s['sha256'] or len(data) != s['bytes'] or len(lines) != s['lines']:
            errors.append(f'Original source bytes changed: {s["source_key"]}')
        if s['source_line_range'] != [1,len(lines)]:
            errors.append(f'Incomplete source line range: {s["source_key"]}')
        owners = actual_owner.get(s['source_key'], [])
        if len(owners)!=1 or owners[0]['id'] != s['owner'] or 'docs/adr-v2/'+owners[0]['file'] != s['owner_path']:
            errors.append(f'Owner mismatch: {s["source_key"]}')
        for decl in s.get('declarations', []):
            start,end = decl['start_line'],decl['end_line']
            if not (1<=start<=end<=len(lines)) or '\n'.join(lines[start-1:end]) != decl['raw']:
                errors.append(f'Status declaration mismatch: {s["source_key"]}')
            else:
                declarations_verified += 1
        owner_bytes = book_data.get(s['owner_path'], b'')
        begin = (s['begin_marker']+'\n'+s['fence']+'markdown\n').encode()
        end = (s['fence']+'\n'+s['end_marker']).encode()
        if owner_bytes.count(begin)!=1 or owner_bytes.count(end)!=1:
            errors.append(f'Missing/repeated original appendix: {s["source_key"]}')
        else:
            payload = owner_bytes.split(begin,1)[1].split(end,1)[0]
            if payload != data or digest(payload) != s['sha256']:
                errors.append(f'Original appendix bytes changed: {s["source_key"]}')
            else:
                appendix_verified += 1
    if total_lines != manifest['line_count'] or total_lines != 48236:
        errors.append('The 48,236 legacy lines are not completely preserved.')
    if total_bytes != manifest['byte_count']:
        errors.append('Legacy source byte total changed.')
    if sum(data.count(b'<!-- ADR_SOURCE_BEGIN:') for data in book_data.values()) != 204:
        errors.append('Appendix payload count must be exactly 204.')
    stats.update({'legacy_lines':total_lines,'legacy_bytes':total_bytes,'verified_full_appendices':appendix_verified,
                  'verified_status_declarations':declarations_verified})

    updates = {}
    # Separate adopted changes keep their own RFC/ADR authority and exact hashes.
    # The immutable import manifest and numbered source payloads stay unchanged.
    for update_path in (current/'post-integration-changes.json', current/'post-integration-parameter-changes.json'):
        if not update_path.exists():
            continue
        record = json.loads(update_path.read_text())
        if record.get('schema') != 'misaka.docs.post-integration-metadata-changes.v1' or not record.get('adoption'):
            errors.append('Post-integration metadata changes lack their schema/adoption record.')
        for document in record.get('documents', []):
            if document not in {d['path'] for d in later} or not (root/document).is_file():
                errors.append(f'Post-integration adoption document is not registered: {document}')
        document_kinds = {d['kind'] for d in later if d['path'] in record.get('documents', [])}
        if len(record.get('documents', [])) != 2 or document_kinds != {'rfc','adr'}:
            errors.append('Post-integration metadata changes require both RFC and ADR records.')
        for change in record.get('changes', []):
            path = change.get('path')
            if path not in POST_INTEGRATION_METADATA or path in updates:
                errors.append(f'Unwaivable or repeated post-integration metadata path: {path}')
            else:
                updates[path] = change
    applied_updates = set()
    for label, entries in [('Existing RFC files',manifest.get('preserved_rfc_files',[])),
                           ('Unchanged baseline files',manifest.get('unchanged_baseline_files',[]))]:
        good=0
        for entry in entries:
            p=root/entry['path']
            actual = digest(p.read_bytes()) if p.is_file() else None
            if actual == entry['sha256']:
                good+=1
            elif metadata_change_matches(entry['path'],entry['sha256'],actual,updates.get(entry['path'],{})):
                applied_updates.add(entry['path'])
            else:
                errors.append(f'{label} modified or missing: {entry["path"]}')
        stats['verified_'+label.lower().replace(' ','_')]=good
    stats['verified_post_integration_metadata_changes']=len(applied_updates)
    stats['current_rfc_documents']=len(list((root/'docs/rfc-v2').glob('[0-9][0-9][0-9][0-9]-*.md')))
    stats['legacy_rfc_documents']=len(list((root/'docs/rfc').glob('[0-9][0-9][0-9][0-9]-*.md')))
    later_rfc = [d for d in later if d['kind'] == 'rfc' and d['path'].startswith('docs/rfc/')]
    rfc_registry = json.loads((root/'docs/rfc-v2/registry.json').read_text(encoding='utf-8'))
    if (stats['current_rfc_documents'] != rfc_registry['active_count']
            or stats['legacy_rfc_documents'] != 41 + len(later_rfc)):
        errors.append('Existing RFC document counts changed.')

    # New links only; source payloads and quoted source declarations are inside fences.
    links=0
    fragments=0
    paths=list(current.glob('*.md'))
    if (root/'README.md').is_file():
        paths.append(root/'README.md')
    else:
        stats['repository_readme_scope'] = 'Not supplied in docs-only overlay; not checked.'
    rx=re.compile(r'\[[^\]\n]*\]\(([^\s)]+)\)')
    explicit_anchor_cache: dict[Path,set[str]]={}
    for path in paths:
        for target in rx.findall(outside_fences(path.read_text(encoding='utf-8'))):
            u=urlsplit(target)
            if u.scheme or target.startswith('//'): continue
            dest=(path.parent/unquote(u.path)).resolve() if u.path else path.resolve()
            links+=1
            if not dest.is_relative_to(root):
                errors.append(f'New link leaves package: {path.name} -> {target}')
            elif not dest.exists():
                errors.append(f'Broken new link: {path.name} -> {target}')
            elif u.fragment:
                if dest not in explicit_anchor_cache:
                    explicit_anchor_cache[dest]=set(re.findall(r'<a id="([^"]+)"></a>',dest.read_text(encoding='utf-8')))
                fragments+=1
                if unquote(u.fragment) not in explicit_anchor_cache[dest]:
                    errors.append(f'Broken explicit fragment: {path.name} -> {target}')
    stats['checked_new_local_link_occurrences']=links
    stats['checked_explicit_fragment_links']=fragments
    stats['runtime_executed']=False
    return {'passed':not errors,'scope':'documentation-integrity-only','errors':errors,'stats':stats}


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2])
    parser.add_argument('--json',type=Path,help='Optional output file; does not change source documents.')
    args=parser.parse_args()
    result=check(args.root)
    text=json.dumps(result,ensure_ascii=False,indent=2)+'\n'
    if args.json:
        args.json.parent.mkdir(parents=True,exist_ok=True)
        args.json.write_text(text,encoding='utf-8')
    print(text,end='')
    return 0 if result['passed'] else 1


if __name__=='__main__':
    raise SystemExit(main())
