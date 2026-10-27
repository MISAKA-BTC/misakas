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
HEX64 = re.compile(r'^([0-9a-f]{64}|[0-9a-f]{128})$')  # a 64-byte Hash64 genesis is 128 hex
WAIVER_KEY = b'misaka-palw/arming-waiver/v2'  # consensus/core/src/palw_arming_waiver_v1.rs PALW_ARMING_WAIVER_KEY_V1
WAIVER_REVIEW = 0xFFFF
# F1 (2026-10-27): the only kinds; the Rust form check also binds each to its slot class.
WAIVER_KINDS = {'drill-skipped': 1, 'e2e-not-run': 2, 'provisional-estimate': 3, 'agent-review': 4, 'user-decision': 5}
WAIVER_RUST = {'drill-skipped': 'DrillSkipped', 'e2e-not-run': 'E2ENotRun', 'provisional-estimate': 'ProvisionalEstimate',
               'agent-review': 'AgentReview', 'user-decision': 'UserDecision'}
WAIVER_NEEDS_VALUE = {'provisional-estimate', 'agent-review', 'user-decision'}


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


def _put(h, b):
    h.update(len(b).to_bytes(4, 'little'))
    h.update(b)


def waiver_digest(fence_name, genesis, at, slot, kind, value, reason, decided_by):
    """The same bytes as `PalwArmingWaiverV1::digest` (keyed blake2b-256)."""
    h = hashlib.blake2b(digest_size=32, key=WAIVER_KEY)
    _put(h, fence_name.encode())
    _put(h, bytes.fromhex(genesis))
    h.update(at.to_bytes(8, 'little'))
    h.update(slot.to_bytes(2, 'little'))
    h.update(bytes([WAIVER_KINDS[kind]]))
    _put(h, value.encode())
    _put(h, reason.encode())
    _put(h, decided_by.encode())
    return h.hexdigest()


def make_waiver(fence_name, genesis, at, slot, kind, value, reason, decided_by):
    """A typed waiver (F1 kinds only). It is never evidence that something ran or passed."""
    if not decided_by.startswith('user '):
        raise PinError('a waiver must be decided by the user (--decided-by "user <date>")')
    if not reason.strip():
        raise PinError('a waiver needs a reason (for an estimate: its basis)')
    if kind not in WAIVER_KINDS:
        raise PinError('waiver kind: one of %s' % ', '.join(WAIVER_KINDS))
    if kind in WAIVER_NEEDS_VALUE and not value.strip():
        raise PinError('a %s waiver carries its value / report / decision' % kind)
    if kind == 'agent-review' and slot != WAIVER_REVIEW and 'review' not in reason.lower() + value.lower():
        raise PinError('an agent-review waiver names a review')
    return {'fence': fence_name, 'genesis': genesis, 'at': at, 'slot': slot, 'kind': kind, 'value': value, 'reason': reason,
            'decidedBy': decided_by,
            'digest': waiver_digest(fence_name, genesis, at, slot, kind, value, reason, decided_by)}


def build_record_with_waivers(family, fence, fence_name, genesis, at, source_commit, requirements, evidence, waivers,
                              review=None, review_waiver=None):
    """One record whose every requirement index 0..requirements-1 is EITHER a passing run (`evidence`: [(index, path)]) OR a
    waiver (`waivers`: [(index, kind, value, reason)]) — never both, never neither. The review is a file or a waiver
    (`review_waiver`: (kind, value, reason), index 0xFFFF)."""
    decided_by = 'user 2026-10-27'
    if family not in FAMILIES:
        raise PinError('family: one of %s' % ', '.join(FAMILIES))
    if not HEX40.match(source_commit):
        raise PinError('--source-commit: 40 lowercase hex')
    if not HEX64.match(genesis) or set(genesis) == {'0'}:
        raise PinError('--genesis: hex, not zero')
    slots = {}
    for idx, path in evidence:
        slots.setdefault(idx, []).append(('run', check_evidence(str(idx), path, source_commit)))
    out_waivers = []
    for idx, kind, value, reason in waivers:
        w = make_waiver(fence_name, genesis, at, idx, kind, value, reason, decided_by)
        out_waivers.append(w)
        slots.setdefault(idx, []).append(('waiver', w['digest']))
    if sorted(slots) != list(range(requirements)) or any(len(v) != 1 for v in slots.values()):
        raise PinError('%s: every requirement 0..%d needs exactly one run or one waiver (got %s)' % (fence, requirements - 1,
                                                                                                    sorted(slots)))
    if (review is None) == (review_waiver is None):
        raise PinError('a review file or a review waiver, exactly one')
    if review is not None:
        _refuse_doc(review)
        rd = _digest(review)
    else:
        kind, value, reason = review_waiver
        w = make_waiver(fence_name, genesis, at, WAIVER_REVIEW, kind, value, reason, decided_by)
        out_waivers.append(w)
        rd = w['digest']
    digests = [(('%d %s' % (i, slots[i][0][0])), slots[i][0][1]) for i in range(requirements)]
    if len({d for _, d in digests} | {rd}) != requirements + 1:
        raise PinError('%s: two slots carry the same digest' % fence)
    return {'family': family, 'fence': fence, 'genesis': genesis, 'at': at, 'sourceCommit': source_commit,
            'evidence': digests, 'review': rd, 'waivers': out_waivers}


def render_waiver_rust(w, genesis_expr):
    slot = 'PALW_ARMING_WAIVER_REVIEW_V1' if w['slot'] == WAIVER_REVIEW else str(w['slot'])
    return ('    PalwArmingWaiverV1 {\n        fence: %s,\n        genesis: %s,\n        activation_daa: %d,\n        requirement: %s,\n'
            '        kind: PalwArmingWaiverKindV1::%s,\n        value: %s,\n        reason: %s,\n        decided_by: %s,\n    },\n') % (
        _lit(w['fence']), genesis_expr, w['at'], slot, WAIVER_RUST[w['kind']], _lit(w['value']), _lit(w['reason']),
        _lit(w['decidedBy']))


def _lit(text):
    """A Rust string literal with the same UTF-8 bytes as `text` (json's \\uXXXX escapes are not Rust escapes)."""
    return json.dumps(text, ensure_ascii=False)


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
    ap.add_argument('--review', help='the review output (or --waive-review)')
    ap.add_argument('--fence-name', help='the Params field name (needed with waivers)')
    ap.add_argument('--requirements', type=int, help='the fence\'s requirement count (needed with waivers)')
    ap.add_argument('--waiver', action='append', default=[], metavar='IDX|REVIEW:KIND:VALUE:REASON',
                    help='a typed waiver (F1 kinds: %s)' % ', '.join(WAIVER_KINDS))
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
        if a.waiver:
            if not a.fence_name or not a.requirements:
                raise PinError('waivers need --fence-name and --requirements')
            ws, review_waiver = [], None
            for item in a.waiver:
                parts = item.split(':', 3)
                if len(parts) != 4:
                    raise PinError('--waiver IDX|REVIEW:KIND:VALUE:REASON')
                idx, kind, value, reason = parts
                if idx == 'REVIEW':
                    review_waiver = (kind, value, reason)
                elif idx.isdigit():
                    ws.append((int(idx), kind, value, reason))
                else:
                    raise PinError('--waiver IDX|REVIEW:KIND:VALUE:REASON')
            ev_idx = []
            for req, path in ev:
                if not req.isdigit():
                    raise PinError('with waivers, --evidence takes IDX=PATH')
                ev_idx.append((int(req), path))
            rec = build_record_with_waivers(a.family, a.fence, a.fence_name, a.genesis.lower(), a.at, a.source_commit,
                                            a.requirements, ev_idx, ws, a.review, review_waiver)
            print(''.join(render_waiver_rust(w, 'GENESIS') for w in rec['waivers']))
        else:
            if not a.review:
                raise PinError('--review')
            rec = build_record(a.family, a.fence, a.genesis.lower(), a.at, a.source_commit, ev, a.review)
    except PinError as e:
        print('refused: %s' % e, file=sys.stderr)
        return 2
    print(json.dumps(rec, indent=2) if a.json else render_rust(rec))
    return 0


if __name__ == '__main__':
    sys.exit(main())
