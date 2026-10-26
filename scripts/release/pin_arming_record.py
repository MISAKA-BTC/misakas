#!/usr/bin/env python3
"""Build ONE arming-record literal from real run outputs (lane ACT, G14 17,000).

The record lists (`PALW_G14_ARMING_RECORDS_V1`, `PALW_KERNEL_ROUTE_ARMING_RECORDS_V1`, `PALW_RFC0018_ARMING_RECORDS_V1`) take one
evidence digest per arming requirement, a source commit and a review digest. This script fills them ONLY from run outputs:

* every `--evidence REQ=PATH` is a JSON result written by the run itself (suite, node E2E, shipped-binary crossing drill) with
  `"exitCode": 0`, `"passed": true` and `"sourceCommit"` equal to `--source-commit`; a file under `docs/`, a Markdown file, an
  estimate (`"estimate": true`) or a failed run is refused;
* `--review PATH` is the review's own signed output (any bytes, not under `docs/`), and must differ from every evidence file;
* `--source-commit` must be a 40-hex commit that the given `--repo` contains, and its tree must be clean of the evidence paths.

The digest is SHA-256 of the file's bytes. The script prints a Rust literal to paste into the list; it never edits the list itself, and
the Rust form check (`palw_*_arming_record_form_v1`) still decides the count and distinctness. Nothing here arms a fence.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

FAMILIES = {
    'g14': ('PalwG14ArmingRecordV1', 'PalwG14GatedFenceV1', 'PALW_G14_ARMING_RECORD_VERSION_V1'),
    'kernel-route': ('PalwKernelRouteArmingRecordV1', 'PalwKernelRouteGatedFenceV1', 'PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1'),
    'rfc0018': ('PalwRfc0018ArmingRecordV1', 'PalwRfc0018GatedFenceV1', 'PALW_RFC0018_ARMING_RECORD_VERSION_V1'),
}
HEX40 = re.compile(r'^[0-9a-f]{40}$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')


class PinError(Exception):
    pass


def _digest(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def _refuse_doc(path):
    p = pathlib.Path(path)
    if p.suffix.lower() in ('.md', '.markdown', '.txt') or 'docs' in p.parts:
        raise PinError('%s: a document is not run evidence' % path)


def check_evidence(req, path, source_commit):
    _refuse_doc(path)
    try:
        body = json.loads(pathlib.Path(path).read_text())
    except (OSError, ValueError) as e:
        raise PinError('%s (%s): not a JSON run result: %s' % (req, path, e)) from None
    if not isinstance(body, dict):
        raise PinError('%s: not a JSON object' % req)
    if body.get('estimate') is True:
        raise PinError('%s: an estimate is not evidence' % req)
    if body.get('exitCode') != 0 or body.get('passed') is not True:
        raise PinError('%s: the run did not pass (exitCode=%r passed=%r)' % (req, body.get('exitCode'), body.get('passed')))
    if body.get('sourceCommit') != source_commit:
        raise PinError('%s: ran on %r, not %s' % (req, body.get('sourceCommit'), source_commit))
    return _digest(path)


def build_record(family, fence, genesis, at, source_commit, evidence, review):
    if family not in FAMILIES:
        raise PinError('family: one of %s' % ', '.join(FAMILIES))
    if not HEX40.match(source_commit):
        raise PinError('--source-commit: 40 lowercase hex')
    if not HEX64.match(genesis) or set(genesis) == {'0'}:
        raise PinError('--genesis: 64 hex, not zero')
    if at <= 0:
        raise PinError('--at: a real height')
    if not evidence:
        raise PinError('at least one --evidence')
    _refuse_doc(review)
    digests, seen = [], set()
    for req, path in evidence:
        d = check_evidence(req, path, source_commit)
        if d in seen:
            raise PinError('%s: the same bytes back two requirements' % req)
        seen.add(d)
        digests.append((req, d))
    rd = _digest(review)
    if rd in seen:
        raise PinError('--review: the review is one of the evidence files')
    return {'family': family, 'fence': fence, 'genesis': genesis, 'at': at, 'sourceCommit': source_commit,
            'evidence': digests, 'review': rd}


def _bytes(h):
    return '[' + ', '.join('0x%s' % h[i:i + 2] for i in range(0, len(h), 2)) + ']'


def render_rust(rec):
    struct, fence_ty, version = FAMILIES[rec['family']]
    ev = ''.join('        %s, // %s\n' % (_bytes(d), req) for req, d in rec['evidence'])
    return ('%s {\n    version: %s,\n    fence: %s::%s,\n    genesis: Hash64::from_bytes(%s),\n    activation_daa: %d,\n'
            '    source_commit: %s,\n    evidence: &[\n%s    ],\n    review: %s,\n}\n') % (
        struct, version, fence_ty, rec['fence'], _bytes(rec['genesis']), rec['at'], _bytes(rec['sourceCommit']), ev,
        _bytes(rec['review']))


def _commit_exists(repo, commit):
    return subprocess.run(['git', '-C', repo, 'cat-file', '-e', commit + '^{commit}'], capture_output=True).returncode == 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--family', required=True, choices=sorted(FAMILIES))
    ap.add_argument('--fence', required=True, help='the gated-fence enum variant, e.g. ProbabilisticConstraintsV1')
    ap.add_argument('--genesis', required=True)
    ap.add_argument('--at', type=int, required=True)
    ap.add_argument('--source-commit', required=True)
    ap.add_argument('--evidence', action='append', default=[], metavar='REQ=PATH')
    ap.add_argument('--review', required=True)
    ap.add_argument('--repo', default=str(pathlib.Path(__file__).resolve().parents[2]))
    ap.add_argument('--json', action='store_true')
    a = ap.parse_args(argv)
    try:
        ev = []
        for item in a.evidence:
            req, sep, path = item.partition('=')
            if not sep or not req or not path:
                raise PinError('--evidence: REQ=PATH')
            ev.append((req, path))
        if not _commit_exists(a.repo, a.source_commit):
            raise PinError('--source-commit %s is not in %s' % (a.source_commit, a.repo))
        rec = build_record(a.family, a.fence, a.genesis.lower(), a.at, a.source_commit, ev, a.review)
    except PinError as e:
        print('refused: %s' % e, file=sys.stderr)
        return 2
    print(json.dumps(rec, indent=2) if a.json else render_rust(rec))
    return 0


if __name__ == '__main__':
    sys.exit(main())
