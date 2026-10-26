#!/usr/bin/env python3
"""G14 full-activation fence-crossing drill (RFC-0026 T15 / T14 on real processes) — the testnet-12 DAA-16,000 rehearsal.

testnet-12 will arm every G14 / full-activation fence at DAA 16,000 (central decision; nothing here arms the public network). This
drill rehearses that crossing on a PRIVATE SALTED devnet built by the SHIPPED binaries, with the whole fence list moved to a low
height H the same way the existing drills move flag days:

  * `misaka-rfc12-private-node --private-probe-profile=g14-v1 --private-probe-g14-at=H --private-probe-g14-set=full` (the G14 drill
    factory: testnet-12's shipped rules below H, the G14 candidate set from H; `PRIVATE_G14_DRILL_FLAG_DAYS_V1` must lie below H, so
    H > 28), every arming record naming THIS salted genesis — the shipped ruleset is never edited;
  * `--crossing=full-activation-flag` (the default): additionally `--palw-drill-full-activation-at=H` — kaspad's own crossing of
    consensus-core's `PALW_T12_FULL_ACTIVATION_FENCES_V1`, the exact list the release will arm at 16,000 (`PALW_T12_FULL_ACTIVATION_DAA`,
    `None` until the records are pinned and the user approves). The node refuses it (by name, untouched) while any entry's validation
    lacks its record for this genesis/height: the node then exits at startup naming the refusal. `--crossing=factory` keeps the factory-only
    crossing (the G14 candidate set alone);
  * optionally `--extra-node-flag=--palw-drill-int13-at=<H>` etc. for flag days the factory does not own (kaspad's own
    `--palw-drill-*-at` family, each refused unless the drill genesis salt is set).

Phases (each one a row of the JSON verdict):

  F0 identity      the drill ruleset prints and validates (`--print-private-probe-profile`); its genesis is the keyring's salted
                   drill genesis; printed again at the end, byte-identical (the shipped identity never moves; the drill identity is
                   stable across restarts). The shipped binaries' sha256 are recorded before and after.
  F1 pre-claims    the honest producer P (with the Panel hosted in its process) runs until at least --pre-claims claims are
                   accepted BELOW H.
  F2 partition     at H - --partition-lead both producers are restarted WITHOUT peers (restart #1, below H) and mine apart;
                   both cross H on their own branches.
  F3 reorg         when both sides are past H + --partition-tail, Q is peered again: one side reorganizes ACROSS H (observed:
                   the losing side's pre-merge sink is not the merged sink, and its pre-merge DAA was >= H).
  F4 restart       P is stopped and restarted past H over its own app dir (restart #2, across the fence's rules).
  F5 post-claims   the chain runs until at least --post-claims claims are accepted at or past H.
  F6 old-rules     every pre-H claim reaches a terminal phase without a void reason naming the new rules (old claims keep the
                   rules they were admitted with); no pre-H claim was refused/voided by a post-H rule.
  F7 ibd           a fresh bondless relay F syncs from genesis across H; P, Q, (O) and F agree on the sink and on every
                   claim/bond row read over public wRPC.

The verdict is PASS only when every row was OBSERVED as asserted; FAIL names what was observed; BLOCKED names the missing binary,
flag or step. Output --out (JSON). Loopback only, no DNS seeders, a private network id; never a public port.

No cryptographic computation proof of any kind is used or claimed (PRINCIPLES §8). This is an observation record of a private
drill chain, not a release approval.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

REPO = Path(__file__).resolve().parents[2]
KIND = 'misaka-g14-t12-16000-fence-drill/v1'
# kaspad::private_integration::g14_drill_factory::PRIVATE_G14_DRILL_FLAG_DAYS_V1's last entry: H must lie above it.
PRIVATE_G14_LAST_FLAG_DAY = 28
# The public testnet-12 target (user goal 2026-10-26: DAA 17,000). Pinned to the core's
# PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1; override with --public-target, never read by a node.
TESTNET12_FULL_ACTIVATION_DAA = 17_000
# kaspad's flag that moves consensus-core's PALW_T12_FULL_ACTIVATION_FENCES_V1 to H on a salted drill chain.
FULL_ACTIVATION_FLAG = '--palw-drill-full-activation-at'
CROSSINGS = ('full-activation-flag', 'factory', 'kernel-slice')
PHASES = ('F0', 'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7')
TERMINAL_PHASES = ('final', 'settled', 'convicted', 'defaulted', 'void', 'expired', 'unavailable', 'released')


class PlanError(ValueError):
    pass


# =====================================================================================================================================
# Plan (pure): heights, node argv, and the judgement of observations — unit-tested in tests/test_g14_t12_16000_fence_drill.py
# =====================================================================================================================================

def validate_salt(salt):
    if not isinstance(salt, str) or len(salt) != 64:
        raise PlanError('--salt: 64 hex characters')
    try:
        int(salt, 16)
    except ValueError:
        raise PlanError('--salt: not hex') from None
    if set(salt) == {'0'}:
        raise PlanError('--salt: not all zero')
    return salt.lower()


def build_plan(at, partition_lead=6, partition_tail=6, restart_after=10, pre_claims=2, post_claims=2,
               public_target=TESTNET12_FULL_ACTIVATION_DAA):
    """The DAA schedule of the crossing. Every height is derived from H; nothing is pinned to the public target."""
    if public_target <= PRIVATE_G14_LAST_FLAG_DAY:
        raise PlanError('--public-target %d is not a public height' % public_target)
    if at <= PRIVATE_G14_LAST_FLAG_DAY:
        raise PlanError('H=%d must be above the drill factory\'s last flag day %d' % (at, PRIVATE_G14_LAST_FLAG_DAY))
    if at == public_target:
        raise PlanError('H must be a LOW drill height, not the public %d' % public_target)
    if partition_lead < 2 or partition_tail < 2:
        raise PlanError('the partition must start at least 2 below H and end at least 2 past it')
    if at - partition_lead <= PRIVATE_G14_LAST_FLAG_DAY:
        raise PlanError('the partition would start at or below the factory\'s flag days')
    if restart_after <= partition_tail:
        raise PlanError('the restart must come after the reorg (restart_after > partition_tail)')
    if pre_claims < 1 or post_claims < 1:
        raise PlanError('at least one claim on each side of the fence')
    return {
        'at': at,
        'publicTarget': public_target,
        'preClaims': pre_claims,
        'postClaims': post_claims,
        'partitionStartDaa': at - partition_lead,
        'partitionEndDaa': at + partition_tail,
        'restartDaa': at + restart_after,
        'phases': list(PHASES),
        'roles': {
            'P': {'card': 0, 'hostsPanel': True, 'producer': True},
            'Q': {'card': None, 'hostsPanel': False, 'producer': True},  # card = seats + 1, set by roles_for()
            'F': {'card': None, 'hostsPanel': False, 'producer': False, 'fresh': True},
        },
    }


def roles_for(plan, seats):
    roles = json.loads(json.dumps(plan['roles']))
    roles['Q']['card'] = seats + 1
    return roles


def factory_set(crossing):
    """The g14-v1 factory set a crossing runs: the kernel slice (lane TNODE, PALW_T12_KERNEL_SLICE_FENCES_V1 with drill records for
    the G14 and kernel-route gated fences) or the full candidate set."""
    return 'kernel-slice' if crossing == 'kernel-slice' else 'full'


def node_argv(private_node, salt, at, seats, appdir, ports, extra=(), g14_set='full'):
    """The shipped private node on the g14-v1 drill profile with the set moved to H (the crossing under test)."""
    p2p, grpc, borsh, js, evm = ports
    argv = [
        str(private_node),
        '--private-probe-profile=g14-v1', '--private-probe-salt=%s' % salt,
        '--private-probe-g14-at=%d' % at, '--private-probe-g14-set=%s' % g14_set,
        '--private-probe-panel-seats=%d' % seats,
        '--testnet', '--netsuffix=12', '--appdir=%s' % appdir,
        '--nodnsseed', '--outpeers=0', '--disable-upnp',
        '--listen=127.0.0.1:%d' % p2p, '--rpclisten=127.0.0.1:%d' % grpc,
        '--rpclisten-borsh=127.0.0.1:%d' % borsh, '--rpclisten-json=127.0.0.1:%d' % js,
        '--evm-rpc-listen=127.0.0.1:%d' % evm,
        '--utxoindex', '--enable-unsynced-mining', '--unsaferpc', '--yes',
    ]
    for f in extra:
        if not f.startswith('--palw-drill-') and not f.startswith('--private-probe-'):
            raise PlanError('extra node flag %r is not a drill flag' % f)
        argv.append(f)
    return argv


def crossing_flags(crossing, at):
    """The node flags the chosen crossing adds on top of the factory's: the full-activation list at H, or nothing (the factory and
    the kernel slice, which the factory itself arms)."""
    if crossing not in CROSSINGS:
        raise PlanError('--crossing: one of %s' % ', '.join(CROSSINGS))
    if crossing == 'full-activation-flag':
        return ['%s=%d' % (FULL_ACTIVATION_FLAG, at)]
    return []


def split_claims(rows, at):
    """Claims accepted below H (old rules) and at/after H (new rules), by their public `acceptedDaa`."""
    pre = [r for r in rows if r.get('acceptedDaa') is not None and r['acceptedDaa'] < at]
    post = [r for r in rows if r.get('acceptedDaa') is not None and r['acceptedDaa'] >= at]
    return pre, post


def is_terminal(row):
    phase = str(row.get('phase') or '').lower()
    return any(t in phase for t in TERMINAL_PHASES)


def voided_by_new_rules(row, at):
    """A pre-H claim voided at/after H with a reason naming a G14 rule: the old claim did NOT keep its old rules."""
    reason = str(row.get('voidReason') or '').lower()
    if not reason:
        return False
    phase_daa = row.get('phaseDaa') or 0
    return phase_daa >= at and any(k in reason for k in ('g14', 'route required', 'route-required', 'fence', 'court policy',
                                                          'original admission', 'not admitted'))


def reorg_observed(loser_before, merged_sink, at):
    """The losing side reorganized across H: its pre-merge sink is gone from the merged tip and it had already crossed H."""
    return (loser_before is not None and merged_sink is not None and loser_before['sink'] != merged_sink
            and loser_before['daa'] >= at)


def judge(plan, obs):
    """Rows of the verdict from the observations (a dict filled by the runner). Missing observation -> BLOCKED, never PASS."""
    at = plan['at']
    rows = []

    def row(phase, status, evidence, note=''):
        rows.append({'phase': phase, 'status': status, 'evidence': evidence, 'note': note})

    # F0 identity
    ident = obs.get('identity')
    if not ident:
        row('F0', 'BLOCKED', {}, 'no identity observation')
    else:
        same_profile = ident.get('profileStart') is not None and ident.get('profileStart') == ident.get('profileEnd')
        same_bins = ident.get('binariesStart') is not None and ident.get('binariesStart') == ident.get('binariesEnd')
        genesis_ok = ident.get('genesisMatchesKeyring') is True
        ok = same_profile and same_bins and genesis_ok
        row('F0', 'PASS' if ok else 'FAIL', {'sameProfile': same_profile, 'sameBinaries': same_bins, 'genesisOk': genesis_ok})

    claims = obs.get('claims')
    if claims is None:
        for p in ('F1', 'F5', 'F6'):
            row(p, 'BLOCKED', {}, 'no claim observation')
    else:
        pre, post = split_claims(claims, at)
        row('F1', 'PASS' if len(pre) >= plan['preClaims'] else 'FAIL', {'preClaims': len(pre), 'want': plan['preClaims']})
        row('F5', 'PASS' if len(post) >= plan['postClaims'] else 'FAIL', {'postClaims': len(post), 'want': plan['postClaims']})
        bad = [r.get('claimId') for r in pre if voided_by_new_rules(r, at)]
        unfinished = [r.get('claimId') for r in pre if not is_terminal(r)]
        if not pre:
            row('F6', 'BLOCKED', {}, 'no pre-H claim to follow')
        elif bad:
            row('F6', 'FAIL', {'voidedByNewRules': bad}, 'an old claim was judged by a post-H rule')
        elif unfinished:
            row('F6', 'FAIL', {'notTerminal': unfinished}, 'an old claim never reached a terminal phase')
        else:
            row('F6', 'PASS', {'preClaims': len(pre)})

    part = obs.get('partition')
    if not part:
        row('F2', 'BLOCKED', {}, 'no partition observation')
        row('F3', 'BLOCKED', {}, 'no reorg observation')
    else:
        both_crossed = all(part.get(k, {}).get('daa', -1) >= at for k in ('P', 'Q'))
        row('F2', 'PASS' if both_crossed and part.get('qRestartedBelowH') else 'FAIL',
            {'P': part.get('P'), 'Q': part.get('Q'), 'qRestartedBelowH': part.get('qRestartedBelowH')})
        seen = reorg_observed(part.get('loserBefore'), part.get('mergedSink'), at)
        row('F3', 'PASS' if seen and part.get('mergedSame') else 'FAIL',
            {'reorgAcrossH': seen, 'mergedSame': part.get('mergedSame'), 'loser': part.get('loser')})

    rs = obs.get('restart')
    if not rs:
        row('F4', 'BLOCKED', {}, 'no restart observation')
    else:
        ok = rs.get('stoppedAtDaa', -1) >= at and rs.get('resumed') is True and rs.get('same') is True
        row('F4', 'PASS' if ok else 'FAIL', rs)

    ibd = obs.get('ibd')
    if not ibd:
        row('F7', 'BLOCKED', {}, 'no IBD observation')
    else:
        row('F7', 'PASS' if ibd.get('same') and ibd.get('fromGenesis') else 'FAIL', {'same': ibd.get('same'), 'sink': ibd.get('sink')})

    order = {p: i for i, p in enumerate(PHASES)}
    rows.sort(key=lambda r: order[r['phase']])
    statuses = {r['status'] for r in rows}
    verdict = 'PASS' if statuses == {'PASS'} else ('FAIL' if 'FAIL' in statuses else 'BLOCKED')
    return {'kind': KIND, 'verdict': verdict, 'plan': plan, 'rows': rows, 'releaseApproval': False,
            'note': 'an observation record of a private salted drill chain, not an approval'}


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


# =====================================================================================================================================
# Runner (real processes). Reuses the G14 collusion drill's node plumbing (keyring, producer / seat flags, wRPC, waits).
# =====================================================================================================================================

def run(args):  # pragma: no cover - drives real processes
    import g14_collusion_drill as G
    import shutil

    plan = build_plan(args.at, args.partition_lead, args.partition_tail, args.restart_after, args.pre_claims, args.post_claims,
                      args.public_target)
    ns = argparse.Namespace(
        mode='cross', family='positive', at=args.at, set=factory_set(args.crossing), seats=args.seats, salt=args.salt, work=args.work, out=args.out + '.raw',
        private_node=args.private_node, kaspad=args.kaspad, cli=args.cli, port_base=args.port_base, tamper_leaf=None,
        canonical_interval=20, stall_s=args.stall_s, outsider_funding_bili=2_000_000, outsider_collateral_bili=0,
        positive_finals=0, positive_every=180, liveness_span=15, after_daa=0, frozen_timeout=3600, d11=False, d16=False, inject=[],
        inject_claims=0, lie_from_daa=None, skip_d02=True, lie_claims=0, inject_timeout=0, run_acceptance=False,
        acceptance_only=False, acceptance_timeout=0, step_timeout=args.step_timeout, conviction_timeout=0, filing_timeout=0,
        settle_timeout=0)
    d = G.Drill(ns)
    d.work.mkdir(parents=True, exist_ok=True)
    obs = {}
    bins = [args.private_node, args.kaspad, args.cli]

    def write(result):
        Path(args.out).write_text(json.dumps(result, indent=2, default=str) + '\n')

    def profile():
        import subprocess
        p = subprocess.run([args.private_node, '--private-probe-profile=g14-v1', '--private-probe-salt=%s' % args.salt,
                            '--private-probe-g14-at=%d' % args.at, '--private-probe-g14-set=%s' % factory_set(args.crossing),
                            '--private-probe-panel-seats=%d' % args.seats, '--print-private-probe-profile',
                            '--testnet', '--netsuffix=12'], capture_output=True, text=True)
        if p.returncode:
            raise G.Blocked('the g14-v1 ruleset at H=%d does not build/validate: %s' % (args.at, (p.stderr or p.stdout)[-2000:]))
        return json.loads(p.stdout)

    def all_claims(nodes_bonds):
        rows = {}
        for bond in nodes_bonds:
            for r in d.claims(bond).get('claims', []):
                rows[G.norm(r['claimId'])] = r
        return list(rows.values())

    try:
        for b in bins:
            if not Path(b).is_file():
                raise G.Blocked('missing shipped binary %s (cargo build --release -p kaspad -p misaka-cli --features '
                                'kaspad/private-integration-config)' % b)
        obs['identity'] = {'binariesStart': {b: sha256_file(b) for b in bins}}
        d.write_keyring()
        p0 = profile()
        obs['identity'].update({'profileStart': p0, 'genesisMatchesKeyring': G.norm(p0['genesis']) == G.norm(d.keyring['genesis_hash'])})
        roles = roles_for(plan, args.seats)
        extra = crossing_flags(args.crossing, args.at) + list(args.extra_node_flag)
        P = d.add('P', 0, d.producer_flags(0, host_panel=True, honest=True) + extra)
        Q = d.add('Q', 1, d.producer_flags(roles['Q']['card'], honest=True) + extra)
        P.start(); Q.start()
        d.wait_rpc(P); d.wait_rpc(Q); d.peer_all()
        bonds = [d.seat(0)['bond_outpoint'], d.seat(roles['Q']['card'])['bond_outpoint']]

        # F1: pre-H claims, stopping before the partition height.
        def pre_ready():
            pre, _ = split_claims(all_claims(bonds), args.at)
            return len(pre) >= plan['preClaims'] and pre
        d.wait('%d claims accepted below H=%d' % (plan['preClaims'], args.at), pre_ready, args.step_timeout)
        if d.daa_quiet() >= plan['partitionStartDaa']:
            raise G.Blocked('the chain passed the partition height %d before %d pre-H claims were accepted; raise --at'
                            % (plan['partitionStartDaa'], plan['preClaims']))
        d.wait_for_chain([P], plan['partitionStartDaa'], args.step_timeout)

        # F2: partition — Q restarts below H without peers (restart #1) and mines alone.
        # Both restart without peers: `addPeer … isPermanent` lives in the running process only, and --outpeers=0 keeps either side
        # from dialling the other, so the two producers mine apart. If they nevertheless reconnect, F3 observes no reorg and FAILs.
        q_restart_daa = P.rpc('getBlockDagInfo')['virtualDaaScore']
        Q.stop(); P.stop(); time.sleep(3); P.start(); Q.start(); d.wait_rpc(P); d.wait_rpc(Q)
        part = {'qRestartedBelowH': q_restart_daa < args.at}
        d.wait_for_chain([P], plan['partitionEndDaa'], args.step_timeout)
        d.wait('Q alone past H + tail', lambda: Q.rpc('getBlockDagInfo')['virtualDaaScore'] >= plan['partitionEndDaa'],
               args.step_timeout)
        before = {n.role: {'sink': n.rpc('getBlockDagInfo')['sink'], 'daa': n.rpc('getBlockDagInfo')['virtualDaaScore']}
                  for n in (P, Q)}
        part.update(before)
        # F3: merge — one side reorganizes across H.
        d.peer_all()
        merged = d.wait_synced([P, Q])
        loser = 'P' if before['P']['sink'] != merged else 'Q'
        part.update({'mergedSink': merged, 'loser': loser, 'loserBefore': before[loser],
                     'mergedSame': d.compare_nodes('after the merge', [P, Q], bonds, [])['same']})
        obs['partition'] = part

        # F4: restart P past H over its own app dir.
        d.wait_for_chain([P], plan['restartDaa'], args.step_timeout)
        stopped_at = P.rpc('getBlockDagInfo')['virtualDaaScore']
        P.stop(); time.sleep(3); P.start(); d.wait_rpc(P); d.peer_all()
        cmp = d.compare_nodes('after the restart of P', [P, Q], bonds, [])
        obs['restart'] = {'stoppedAtDaa': stopped_at, 'resumed': True, 'same': cmp['same']}

        # F5 / F6: post-H claims; every pre-H claim followed to a terminal phase.
        def post_ready():
            pre, post = split_claims(all_claims(bonds), args.at)
            return len(post) >= plan['postClaims'] and all(is_terminal(r) for r in pre) and post
        try:
            d.wait('%d claims at/after H and every pre-H claim terminal' % plan['postClaims'], post_ready, args.step_timeout)
        except G.Blocked as e:
            G.log('post-H wait: %s' % e)
        obs['claims'] = all_claims(bonds)

        # F7: a fresh bondless relay syncs from genesis across H.
        F = d.add('F', 9, [], bonded=False)
        if F.appdir.exists():
            shutil.rmtree(F.appdir)
        F.start(); d.wait_rpc(F); d.peer_all()
        claim_pairs = [(r.get('bond') or bonds[0], r['claimId']) for r in obs['claims'] if r.get('bond')]
        cmp = d.compare_nodes('fresh IBD relay F', [P, Q, F], bonds, claim_pairs)
        obs['ibd'] = {'same': cmp['same'], 'sink': cmp['sink'], 'fromGenesis': True}
        F.stop()

        obs['identity'].update({'profileEnd': profile(), 'binariesEnd': {b: sha256_file(b) for b in bins}})
    except G.Blocked as e:
        obs['blocked'] = str(e)
        G.log('BLOCKED: %s' % e)
    finally:
        for n in list(d.nodes.values()):
            n.stop()
        result = judge(plan, obs)
        if 'blocked' in obs:
            result['blocked'] = obs['blocked']
        result['observations'] = obs
        write(result)
        G.log('verdict %s -> %s' % (result['verdict'], args.out))
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--at', type=int, default=60, help='the moved full-activation height H (low; > %d)' % PRIVATE_G14_LAST_FLAG_DAY)
    ap.add_argument('--public-target', type=int, default=TESTNET12_FULL_ACTIVATION_DAA,
                    help='the public testnet-12 full-activation height the drill stands in for (default %(default)s)')
    ap.add_argument('--salt', required=True, help='64 hex, this drill only (openssl rand -hex 32)')
    ap.add_argument('--work', required=True, help='fresh scratch directory (never ~/.misaka)')
    ap.add_argument('--out', required=True)
    ap.add_argument('--seats', type=int, default=3)
    ap.add_argument('--pre-claims', type=int, default=2)
    ap.add_argument('--post-claims', type=int, default=2)
    ap.add_argument('--partition-lead', type=int, default=6)
    ap.add_argument('--partition-tail', type=int, default=6)
    ap.add_argument('--restart-after', type=int, default=10)
    ap.add_argument('--extra-node-flag', action='append', default=[], help='an extra --palw-drill-* / --private-probe-* node flag')
    ap.add_argument('--crossing', choices=CROSSINGS, default='full-activation-flag',
                    help='full-activation-flag: also pass %s=H (the release list); factory: the G14 factory set only' % FULL_ACTIVATION_FLAG)
    ap.add_argument('--private-node', default=str(REPO / 'target/release/misaka-rfc12-private-node'))
    ap.add_argument('--kaspad', default=str(REPO / 'target/release/kaspad'))
    ap.add_argument('--cli', default=str(REPO / 'target/release/misaka'))
    ap.add_argument('--port-base', type=int, default=34500)
    ap.add_argument('--stall-s', type=int, default=1800)
    ap.add_argument('--step-timeout', type=int, default=4 * 3600)
    ap.add_argument('--plan-only', action='store_true', help='print the plan (heights, argv of P) and exit; starts nothing')
    args = ap.parse_args()
    try:
        args.salt = validate_salt(args.salt)
        plan = build_plan(args.at, args.partition_lead, args.partition_tail, args.restart_after, args.pre_claims, args.post_claims,
                      args.public_target)
    except PlanError as e:
        ap.error(str(e))
    if args.plan_only:
        base = args.port_base
        print(json.dumps({'plan': plan, 'roles': roles_for(plan, args.seats),
                          'argvP': node_argv(args.private_node, args.salt, args.at, args.seats, Path(args.work) / 'P' / 'app',
                                             (base, base + 1, base + 2, base + 3, base + 4),
                                             crossing_flags(args.crossing, args.at) + list(args.extra_node_flag))}, indent=2))
        return 0
    result = run(args)
    return 0 if result['verdict'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
