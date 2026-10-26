#!/usr/bin/env python3
"""Lane TNODE (G14 17,000 slice): T01..T15 on real node processes for the kernel slice — a thin driver over the EXISTING runners.

The slice (coordinator plan 2026-10-26): kernel route with Panel=1, card 1's genesis class, a one-prompt decode job;
`PALW_T12_KERNEL_SLICE_FENCES_V1` armed at a low height H on a PRIVATE SALTED testnet-12 drill chain through the g14-v1 factory's
`--private-probe-g14-set=kernel-slice` (drill records for the G14 AND the kernel-route gated fences, never pinned; the shipped
validation still refuses the ruleset). No new runtime: every node step is `g14_t12_16000_fence_drill.run` (F0..F7: partition / reorg
across H, restart, fresh IBD relay, old claims keep old rules) or `g14_collusion_drill` (positive family: honest producer + hosted
Panel + a bonded outsider, observed over public wRPC).

Outputs (`--out-dir`, NEVER under docs/ — `scripts/release/pin_arming_record.py` refuses documents):
  ledger.json                     one row per T item: scenario, roles, status PASS / FAIL / BLOCKED / WAITING-INTEGRATION /
                                  NOT-IMPLEMENTED / WAIVED, the raw result it was read from;
  req-<family>-<fence>-<i>.json   one run-result per arming requirement this lane can speak to, in the pin script's form
                                  (`exitCode`, `passed`, `sourceCommit`, binaries' sha256, the exact command). `passed` is true only
                                  when every T row the requirement rests on was OBSERVED PASS. A waiver is `passed: false` with
                                  `waiver` filled (the user's 2026-10-26 waiver of the RFC-0011 §15.7 shadow comparison is NOT a run).

Statuses are never upgraded: a missing observation is BLOCKED, a step this binary cannot drive is NOT-IMPLEMENTED with what is missing.
No cryptographic computation proof of any kind is used or claimed (PRINCIPLES §8). Fixture keys of one host are NOT independence or
drawn-approval evidence: every seat here is a drill key in one process, recorded as such.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
REPO = Path(__file__).resolve().parents[2]
KIND = 'misaka-g14-tnode-slice/v1'

# T items -> (scenario, roles, runner row(s) it is read from, what a real process adds over fold/in-process tests).
# runner rows: 'fence:F<n>' = g14_t12_16000_fence_drill row; 'pos:<key>' = collusion-drill positive observation; None = no runner.
T_ITEMS = [
    ('T01', 'honest miner + wrong challenger through shared-original duties and funded outcomes',
     'producer P (card 0) + hosted Panel=1 seat (card 1) + bonded outsider O as challenger', None,
     'needs a node producer that emits KernelClaim for the slice class under g14-v1 and a node-side wrong-challenge filer; '
     'in-process: canonical/spine.rs, cert.rs'),
    ('T02', 'permanent miner silence, supplied opportunity, duty authority, eligible default',
     'P stopped for good after acceptance; O/relay observe the default', None,
     'needs kernel-slice claims on the node and the CLOCK lane deadlines (palw_provider_court_v1 / claim hard deadline)'),
    ('T03', 'forged signatures and fake Panel completion reports rejected before any monetary effect',
     'P submits forged seat signature via wRPC; all nodes refuse', None,
     'no node-side injection flag for a forged kernel-route seat report; in-process: canonical/cert.rs'),
    ('T04', 'normal-tx / paid-court / invalid-codec floods; bounded native intake',
     'flood driver against P and relay F', None, 'no flood driver exists for the native intake; needs V-values (intake bounds)'),
    ('T05', 'every owner occupied by colluders, one external verifier finishes through paid public ingress',
     'colluding P + Panel=1 seat, outsider O', None,
     'g14_collusion_drill convicts on the LEGACY route only (tamper leaf on V2 attempts); the kernel route has no node tamper flag'),
    ('T06', 'last inclusive notice and latest lawful response fit the original hard deadline', 'P, O', None,
     'WAITING-INTEGRATION: CLOCK lane (kernel route deadline/clock)'),
    ('T07', '-1 / = / +1 at every connected native deadline', 'P, O', None, 'WAITING-INTEGRATION: CLOCK lane'),
    ('T08', 'miner PC offline, distinct court signer, third-party relay with current-duty authority', 'P offline, signer S, relay F', None,
     'needs kernel-slice claims on the node; no separate court-signer process flag for the kernel route'),
    ('T09', 'shortest signed dependency leases, multiple claims sharing one bond, aggregate reservation', 'P, O', None,
     'WAITING-INTEGRATION: CLOCK lane (palw_provider_court_v1 leases); settlement once needs palw_bond_budget_v1 values'),
    ('T10', 'internal numerical fold-only lie rejected without dropping fold arithmetic', 'P lying, all nodes', None,
     'in-process only (fold); no node fault flag for the kernel route'),
    ('T11', '1..10,000 concurrent pursuits, last stage alone false', 'many claims, O', None,
     'needs kernel-slice claim volume on the node; F02 lane aux table 150'),
    ('T12', 'hard-boundary concentration with every accepted duty funded', 'P, O', None, 'WAITING-INTEGRATION: CLOCK + F02'),
    ('T13', 'oldest claim loses service, independent claim still progresses, no false slash', 'P, O', None,
     'needs kernel-slice claims on the node'),
    ('T14', 'shared path across reorg / restart / pruned IBD', 'P, Q (partition), restart P, fresh relay F',
     ['fence:F2', 'fence:F3', 'fence:F4', 'fence:F7'],
     'real processes: partition+merge across H, restart over own app dir, fresh relay IBD from genesis (pruned IBD: not forced at this '
     'chain length — recorded as not shown)'),
    ('T15', 'old/new fence boundary, old claims keep old rules, shipping identity unchanged, activation refusal remains',
     'P, Q, F; shipped binaries hashed before/after', ['fence:F0', 'fence:F1', 'fence:F5', 'fence:F6', 'profile:shipped-refuses'],
     'real processes cross H on the shipped release build'),
]

# Arming requirements this lane can speak to: (family, fence, index, T rows it rests on, waiver text or None).
REQUIREMENTS = [
    ('kernel-route', 'palw_probabilistic_constraints_v1', 1, ['T01', 'T03', 'T05', 'T14'], None),
    ('kernel-route', 'palw_probabilistic_constraints_v1', 4, [], 'user decision 2026-10-26: RFC-0011 §15.7 shadow comparison treated as '
     'complete TEMPORARILY; not measured; to be replaced by a measurement'),
    ('kernel-route', 'palw_probabilistic_constraints_v1', 7, ['T14', 'T15'], None),
    ('g14', 'palw_g14_route_required_v1', 2, ['T15'], None),
    ('g14', 'palw_g14_route_required_v1', 3, ['T14', 'T15'], None),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def git_head():
    return subprocess.run(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], capture_output=True, text=True, check=True).stdout.strip()


def tree_dirty():
    out = subprocess.run(['git', '-C', str(REPO), 'status', '--porcelain', '--untracked-files=no'], capture_output=True, text=True).stdout
    return bool(out.strip())


def rows_by_key(fence_result, profile_check):
    got = {}
    for r in (fence_result or {}).get('rows', []):
        got['fence:' + r['phase']] = r
    if profile_check is not None:
        got['profile:shipped-refuses'] = profile_check
    return got


def ledger(fence_result, profile_check, blocked_reason=None):
    """T rows from the runner rows (pure; unit-tested). A T item with no runner is NOT-IMPLEMENTED / WAITING-INTEGRATION, never PASS."""
    got = rows_by_key(fence_result, profile_check)
    out = []
    for tid, what, roles, keys, note in T_ITEMS:
        if keys is None:
            status = 'WAITING-INTEGRATION' if note.startswith('WAITING-INTEGRATION') else 'NOT-IMPLEMENTED'
            out.append({'t': tid, 'what': what, 'roles': roles, 'status': status, 'note': note, 'rows': {}})
            continue
        rows = {k: got.get(k) for k in keys}
        sts = [(r or {}).get('status') for r in rows.values()]
        if all(s == 'PASS' for s in sts):
            status = 'PASS'
        elif any(s == 'FAIL' for s in sts):
            status = 'FAIL'
        else:
            status = 'BLOCKED'
        n = note if status == 'PASS' else (note + (' | blocked: %s' % blocked_reason if blocked_reason else ''))
        out.append({'t': tid, 'what': what, 'roles': roles, 'status': status, 'note': n, 'rows': rows})
    return out


def requirement_results(led, ident, command):
    by = {r['t']: r for r in led}
    out = []
    for family, fence, idx, ts, waiver in REQUIREMENTS:
        passed = waiver is None and bool(ts) and all(by[t]['status'] == 'PASS' for t in ts)
        out.append({
            'kind': KIND + '/requirement', 'family': family, 'fence': fence, 'requirementIndex': idx,
            'tItems': {t: by[t]['status'] for t in ts}, 'passed': passed, 'exitCode': 0 if passed else 1,
            'waiver': waiver, 'estimate': False, 'testOnlyValues': ['drill salt', 'drill records (labelled, never pinned)',
                                                                    'drill keys of one host (not independence evidence)'],
            **ident, 'command': command,
        })
    return out


def profile_refusal_check(private_node, salt, at, seats):
    """The factory's kernel-slice ruleset prints (it validates only through the drill record path; the factory itself asserts the
    shipped validation refuses it). PASS when it prints and names the kernel-route records; FAIL/BLOCKED otherwise."""
    p = subprocess.run([str(private_node), '--private-probe-profile=g14-v1', '--private-probe-salt=%s' % salt,
                        '--private-probe-g14-at=%d' % at, '--private-probe-g14-set=kernel-slice',
                        '--private-probe-panel-seats=%d' % seats, '--print-private-probe-profile', '--testnet', '--netsuffix=12'],
                       capture_output=True, text=True)
    if p.returncode:
        return {'status': 'BLOCKED', 'evidence': {'exit': p.returncode}, 'note': (p.stderr or p.stdout)[-3000:]}
    try:
        body = json.loads(p.stdout)
    except ValueError:
        return {'status': 'FAIL', 'evidence': {}, 'note': 'profile output is not JSON: %s' % p.stdout[-500:]}
    return {'status': 'PASS', 'evidence': {'g14Drill': body.get('g14Drill'), 'genesis': body.get('genesis')}}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--at', type=int, default=60)
    ap.add_argument('--salt', required=True)
    ap.add_argument('--work', required=True, help='fresh scratch dir (removed by the caller afterwards)')
    ap.add_argument('--out-dir', required=True, help='run results (not under docs/)')
    ap.add_argument('--seats', type=int, default=3)
    ap.add_argument('--port-base', type=int, default=36500)
    ap.add_argument('--step-timeout', type=int, default=2 * 3600)
    ap.add_argument('--stall-s', type=int, default=1200)
    ap.add_argument('--private-node', default=str(REPO / 'target/release/misaka-rfc12-private-node'))
    ap.add_argument('--kaspad', default=str(REPO / 'target/release/kaspad'))
    ap.add_argument('--cli', default=str(REPO / 'target/release/misaka'))
    ap.add_argument('--build-command', default='cargo build --release -p kaspad -p misaka-cli --features kaspad/private-integration-config')
    ap.add_argument('--profile-only', action='store_true', help='only the ruleset check (no node is started)')
    a = ap.parse_args()
    if 'docs' in Path(a.out_dir).resolve().parts:
        sys.exit('--out-dir must not be under docs/ (the pin script refuses documents)')
    out = Path(a.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    bins = [a.private_node, a.kaspad, a.cli]
    ident = {'sourceCommit': git_head(), 'treeDirty': tree_dirty(), 'buildCommand': a.build_command,
             'binaries': {Path(b).name: sha256_file(b) for b in bins if Path(b).is_file()}, 'startedAt': time.time()}
    command = ' '.join([sys.executable] + sys.argv)
    profile = profile_refusal_check(a.private_node, a.salt, a.at, a.seats)
    (out / 'profile.json').write_text(json.dumps(profile, indent=2) + '\n')
    fence_result, blocked = None, None
    if profile['status'] != 'PASS':
        blocked = 'the kernel-slice ruleset does not build: ' + profile['note'][-600:]
    elif not a.profile_only:
        import g14_t12_16000_fence_drill as FD
        ns = argparse.Namespace(at=a.at, public_target=FD.TESTNET12_FULL_ACTIVATION_DAA, salt=a.salt, work=a.work,
                                out=str(out / 'fence-drill.json'), seats=a.seats, pre_claims=2, post_claims=2, partition_lead=6,
                                partition_tail=6, restart_after=10, extra_node_flag=[], crossing='kernel-slice',
                                private_node=a.private_node, kaspad=a.kaspad, cli=a.cli, port_base=a.port_base,
                                stall_s=a.stall_s, step_timeout=a.step_timeout)
        fence_result = FD.run(ns)
        blocked = fence_result.get('blocked')
    elif a.profile_only:
        blocked = 'profile-only run: no node was started'
    led = ledger(fence_result, profile, blocked)
    ident['binariesEnd'] = {Path(b).name: sha256_file(b) for b in bins if Path(b).is_file()}
    (out / 'ledger.json').write_text(json.dumps({'kind': KIND, **ident, 'command': command, 'at': a.at, 'ledger': led,
                                                 'blocked': blocked}, indent=2, default=str) + '\n')
    for r in requirement_results(led, ident, command):
        name = 'req-%s-%s-%d.json' % (r['family'], r['fence'], r['requirementIndex'])
        (out / name).write_text(json.dumps(r, indent=2, default=str) + '\n')
    for r in led:
        print('%s %-20s %s' % (r['t'], r['status'], r['what']))
    return 0


if __name__ == '__main__':
    sys.exit(main())
