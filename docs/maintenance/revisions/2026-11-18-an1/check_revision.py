#!/usr/bin/env python3
"""Verify AN1 document preservation and link deltas; not protocol safety."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
sys.path.insert(0, str(ROOT / 'docs/maintenance'))
from doclib import scan, key, ERRORS


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    baseline = json.loads((HERE / 'source-baseline.json').read_text())
    allowed = set(json.loads((HERE / 'edit-scope.json').read_text())['allowed_modified_paths'])
    deleted = []
    changed = []
    unexpected = []
    for entry in baseline['files']:
        path = ROOT / entry['path']
        if not path.is_file():
            deleted.append(entry['path'])
        elif digest(path) != entry['sha256']:
            changed.append(entry['path'])
            if entry['path'] not in allowed:
                unexpected.append(entry['path'])
    now = Counter(key(row) for row in scan(ROOT) if row['status'] in ERRORS)
    previous = Counter(baseline['link_issue_counts'])
    new_links = now - previous
    rfc_count = len(list((ROOT / 'docs/rfc-v2').glob('[0-9][0-9][0-9][0-9]-*.md')))
    adr_count = len(list((ROOT / 'docs/adr-v2').glob('[0-9][0-9][0-9][0-9]-*.md')))
    expected = [
        'docs/rfc/0059-palw-an1-input-binding-and-panel-court.md',
        'docs/adr/0227-palw-an1-input-binding-and-panel-court.md',
    ]
    registry_errors = []
    for location in ['docs/rfc-v2/registry.json', 'docs/adr-v2/registry.json']:
        data = json.loads((ROOT / location).read_text())
        records = {entry['path']: entry for entry in data['post_consolidation_documents']}
        for path in expected:
            if records.get(path, {}).get('status') != 'DRAFT_NOT_ACTIVATED':
                registry_errors.append(location + ': missing/wrong state for ' + path)
    result = {
        'status': 'PASS' if not (deleted or unexpected or new_links or registry_errors) and rfc_count == 20 and adr_count == 50 else 'FAIL',
        'baseline_file_count': len(baseline['files']),
        'deleted_baseline_files': deleted,
        'modified_baseline_files': changed,
        'out_of_scope_modifications': unexpected,
        'new_link_issue_occurrences': sum(new_links.values()),
        'new_link_issues': dict(new_links),
        'retained_baseline_link_issue_occurrences': sum((now & previous).values()),
        'current_rfc_book_count': rfc_count,
        'current_adr_book_count': adr_count,
        'registry_errors': registry_errors,
        'limitations': ['Document preservation and offline link delta only.', 'No validation of protocol security, native runtime, cryptography, network availability, or LLM cost lower bound.'],
    }
    (HERE / 'revision-results.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
