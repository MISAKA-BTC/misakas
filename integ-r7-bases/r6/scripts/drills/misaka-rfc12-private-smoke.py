#!/usr/bin/env python3
"""Exercise the unpublished private constructor over three real processes.

This finite boot/handshake/cut/heal/kill/reopen probe checks real public wRPC and
genesis monetary rows. It never claims mature useful work, pruned IBD, market
fills, release100k, retirement activation or security approval.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from rfc12_owned_resources import OwnedResourceSampler, check_completed_limits, read_limits


MODULE = Path(__file__).with_name('misaka-rfc12-multiprocess.py')
SPEC = importlib.util.spec_from_file_location('rfc12_private_transport', MODULE)
transport = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(transport)


def monetary_rows(entries):
    rows = []
    for row in entries:
        entry, outpoint = row['utxoEntry'], row['outpoint']
        rows.append({'transactionId': outpoint['transactionId'], 'index': outpoint['index'],
                     'amount': str(entry['amount']), 'blockDaaScore': entry['blockDaaScore'],
                     'isCoinbase': entry['isCoinbase']})
    return sorted(rows, key=lambda row: (row['transactionId'], row['index']))


def identity_and_money(drill, role, manifest):
    row = drill.probe(role)
    node, dag = row['node'], row['dag']
    assert node['genesisHash'] == manifest['genesis'], 'RPC genesis differs from the constructor manifest'
    assert node['drillSaltId'] == manifest['saltId'], 'RPC salt identity differs'
    assert node['consensusParamsId'] == manifest['consensusParamsId'], 'RPC rules differ from the pinned private Config'
    assert dag['sink'] == manifest['genesis'], 'this construction probe must never substitute manufactured work'
    epoch, point, phase, stable = transport.import_stamp(node)
    assert epoch == 0 and point == '' and phase == 'legacy-stable' and stable, 'fresh/reopened genesis import is incomplete'
    assert node['producerState'] == 'off', 'background production would make the finite probe unpinned'
    probe = manifest['genesisMoneyProbe']
    assert probe and probe['utxos'], 'the fixture must pin actual funded genesis rows'
    rpc = drill.rpc(role)
    try:
        entries = rpc.call('getUtxosByAddresses', {'addresses': [probe['address']]})['entries']
        supply = rpc.call('getCoinSupply') if manifest.get('population') else None
        after = rpc.call('getPalwNodeStatus')
    finally:
        rpc.close()
    assert transport.import_stamp(after) == transport.import_stamp(node), 'monetary read crossed an import epoch'
    actual = monetary_rows(entries)
    assert actual == probe['utxos'], 'real indexed monetary rows differ from the salted genesis'
    row['genesisMoneyRows'] = actual
    if manifest.get('population'):
        assert row['shadow'] and row['shadow']['available'], 'actual registry capacity read is unavailable'
        assert row['shadow']['bondsTotal'] == manifest['population']['totalBonds'], 'native registry did not load the pinned funded population'
        assert str(supply['circulatingSompi']) == manifest['population']['premineSupplySompi'], 'actual indexed supply differs from the conserved factory'
        row['coinSupply'] = supply
    return row


def peer_count(drill, role):
    rpc = drill.rpc(role)
    try:
        return len(rpc.call('getConnectedPeerInfo')['peerInfo'])
    finally:
        rpc.close()


def ready(drill, role, manifest):
    try:
        return identity_and_money(drill, role, manifest)
    except transport.Blocked:
        raise
    except RuntimeError:
        return None  # a starting wRPC service may close its first connection


def plan_for(binary, salt, manifest, timeout, profile='native-v1', bonds=None):
    # Reserve all listeners before choosing the finite plan. Release them before
    # spawning; any port collision fails normally and never kills another process.
    reservations, endpoints = [], []
    try:
        for _ in range(12):
            listener = socket.socket()
            listener.bind(('127.0.0.1', 0))
            reservations.append(listener)
            endpoints.append('127.0.0.1:%d' % listener.getsockname()[1])
        nodes = {}
        for index, role in enumerate(('A', 'B', 'JOIN')):
            p2p, grpc, wrpc, eth = endpoints[index * 4:index * 4 + 4]
            nodes[role] = dict(p2p=p2p, grpc=grpc, wrpc=wrpc, eth='http://' + eth,
                argv=[str(binary), '--private-probe-profile=' + profile, '--private-probe-salt=' + salt,
                      '--testnet', '--netsuffix=12', '--nodnsseed', '--outpeers=0', '--disable-upnp',
                      '--listen=' + p2p, '--rpclisten=' + grpc, '--rpclisten-json=' + wrpc,
                      '--evm-rpc-listen=' + eth, '--appdir={' + role + '_APPDIR}',
                      '--ram-scale=0.1', '--utxoindex', '--unsaferpc', '--nologfiles', '--loglevel=warn'])
            if bonds is not None:
                nodes[role]['argv'].append('--private-probe-bonds=' + str(bonds))
        return dict(schemaVersion=1, kind='rfc12-construction-transport-only', nodes=nodes,
                    manifest=manifest, scenario_timeout_s=timeout, probe_timeout_s=15,
                    fullRfc12SecurityApproval=False, release100kMeasured=False)
    finally:
        for listener in reservations:
            listener.close()


def run(drill, manifest):
    before = {}
    for role in ('A', 'B', 'JOIN'):
        drill.start(role)
        before[role] = drill.wait(lambda role=role: ready(drill, role, manifest), role + ' actual RPC/money startup')
    headers = [row['header'] for row in before.values()]
    assert headers[0] == headers[1] == headers[2], 'separate DBs disagree on the actual L1 genesis commitments'
    link = drill.join_link('B', 'A')
    drill.wait(lambda: peer_count(drill, 'A') > 0 and peer_count(drill, 'B') > 0, 'ordinary P2P handshake')
    link.cut()
    drill.wait(lambda: peer_count(drill, 'A') == 0 and peer_count(drill, 'B') == 0, 'actual owned socket partition')
    link.enabled.set()
    drill.wait(lambda: peer_count(drill, 'A') > 0 and peer_count(drill, 'B') > 0, 'P2P reconnection after healing')
    drill.join_link('JOIN', 'A')
    drill.wait(lambda: peer_count(drill, 'JOIN') > 0, 'third separate process handshake')
    for role in ('A', 'B', 'JOIN'):
        assert identity_and_money(drill, role, manifest)['header'] == headers[0]
    drill.kill('JOIN')  # SIGKILL of this driver's actual child, no destructor shutdown
    drill.start('JOIN')  # identical owned DB and private fingerprint
    after = drill.wait(lambda: ready(drill, 'JOIN', manifest), 'actual cold reopen monetary and root agreement')
    assert after['header'] == headers[0]
    assert transport.import_stamp(after['node']) == transport.import_stamp(before['JOIN']['node'])
    drill.join_link('JOIN', 'A')
    drill.wait(lambda: peer_count(drill, 'JOIN') > 0, 'cold-reopened process P2P handshake')


class ResourceDrill(transport.Drill):
    def __init__(self, plan, root, sampler, owned_children):
        super().__init__(plan, root)
        self.sampler, self.owned_children = sampler, owned_children

    def start(self, role):
        self.sampler.check()
        super().start(role)
        self.sampler.register(role, self.processes[role])

    def wait(self, predicate, label):
        def checked():
            self.sampler.check()
            value = predicate()
            self.sampler.check()
            return value
        return super().wait(checked, label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--salt', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--profile', choices=('native-v1', 'population-v1', 'complete-v1'), default='native-v1')
    parser.add_argument('--bonds', type=int)
    parser.add_argument('--manifest-timeout', type=int, default=30)
    parser.add_argument('--resource-limits', help='optional predeclared construction-only resource limits JSON')
    args = parser.parse_args()
    binary = Path(args.binary).resolve(strict=True)
    assert binary.is_file() and os.access(binary, os.X_OK), 'supply the compiled opt-in private helper'
    assert len(args.salt) == 64 and int(args.salt, 16) != 0, 'an explicit nonzero 32-byte private salt is required'
    assert (args.profile == 'population-v1') == (args.bonds is not None), 'population-v1 needs an explicit size; native-v1/complete-v1 use their existing genesis'
    limits = read_limits(args.resource_limits)  # fixed before the first factory child starts
    manifest_argv = [str(binary), '--private-probe-profile=' + args.profile, '--private-probe-salt=' + args.salt,
                     '--testnet', '--netsuffix=12', '--print-private-probe-profile']
    if args.bonds is not None:
        manifest_argv.append('--private-probe-bonds=' + str(args.bonds))
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc12-construction-'))
    owned_children = {}
    sampler = OwnedResourceSampler(owned_children, limits)
    factory, drill, plan = None, None, None
    factory_stdout, factory_stderr = '', ''
    result = dict(status='FAIL', artifactDirectory=str(root), resourceLimits=limits, resourceLimitsMet=None,
                  fullRfc12SecurityApproval=False, release100kMeasured=False, prunedIbdMeasured=False,
                  matureUsefulWorkMeasured=False, marketFillsMeasured=False)
    try:
        factory_began = time.monotonic()
        factory = subprocess.Popen(manifest_argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        sampler.register('MANIFEST', factory)
        sampler.sample_now()
        factory_timeout = min(args.manifest_timeout, (limits or {}).get('maximumFactorySeconds', args.manifest_timeout))
        factory_stdout, factory_stderr = factory.communicate(timeout=factory_timeout)
        result['factorySeconds'] = time.monotonic() - factory_began
        assert factory.returncode == 0, 'private manifest factory exited %d; inspect its preserved stderr' % factory.returncode
        sampler.check()
        manifest = json.loads(factory_stdout)
        assert manifest['kind'] == 'rfc12-private-integration-probe'
        assert manifest['skipProofOfWork'] is False and manifest['releaseApproval'] is False
        plan = plan_for(binary, args.salt, manifest, args.timeout, args.profile, args.bonds)
        plan['resourceLimits'] = limits
        plan_bytes = (json.dumps(plan, sort_keys=True, indent=2) + '\n').encode()
        (root / 'plan.json').write_bytes(plan_bytes)  # fixed before the first node starts
        result['planSha256'] = hashlib.sha256(plan_bytes).hexdigest()
        drill = ResourceDrill(plan, root, sampler, owned_children)
        run(drill, manifest)
        sampler.check()
        result['status'] = 'CONSTRUCTION_TRANSPORT_PASS'
    except transport.Blocked as error:
        result.update(status='BLOCKED', reason=str(error))
    except Exception as error:
        result['reason'] = repr(error)
    finally:
        if factory is not None and factory.poll() is None:
            factory.kill()  # only this driver's manifest child
            factory_stdout, factory_stderr = factory.communicate(timeout=5)
            result['factorySeconds'] = time.monotonic() - factory_began
        (root / 'manifest.stdout').write_text(factory_stdout)
        (root / 'manifest.stderr').write_text(factory_stderr)
        result['sampledOwnedResources'] = sampler.stop()
        # Snapshot only owned child RSS, before shutdown; KiB from the platform's
        # ps RSS column. This is an observation, not a declared release threshold.
        result['ownedProcessRssKiB'] = {}
        for role, process in (drill.processes if drill else {}).items():
            try:
                rss = subprocess.run(['ps', '-o', 'rss=', '-p', str(process.pid)], check=True,
                                     capture_output=True, text=True, timeout=5).stdout.strip()
                result['ownedProcessRssKiB'][role] = int(rss)
            except (ValueError, OSError, subprocess.SubprocessError):
                result['ownedProcessRssKiB'][role] = None
        if drill:
            drill.close()
        result['ownedDatabaseLogicalBytes'] = {role: sum(file.stat().st_size for file in (root / role).rglob('*') if file.is_file())
                                              for role in (plan['nodes'] if plan else {})}
        result.update(observations=drill.observations if drill else [], probeLatenciesMs=drill.latencies if drill else [])
        if limits:
            result['resourceLimitsMet'] = False
            if result['status'] == 'CONSTRUCTION_TRANSPORT_PASS':
                try:
                    check_completed_limits(limits, result)
                    result['resourceLimitsMet'] = True
                except RuntimeError as error:
                    result.update(status='FAIL', reason=str(error))
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'observations'}, indent=2))
    return 0 if result['status'] == 'CONSTRUCTION_TRANSPORT_PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
