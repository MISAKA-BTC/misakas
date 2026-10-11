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
        after = rpc.call('getPalwNodeStatus')
    finally:
        rpc.close()
    assert transport.import_stamp(after) == transport.import_stamp(node), 'monetary read crossed an import epoch'
    actual = monetary_rows(entries)
    assert actual == probe['utxos'], 'real indexed monetary rows differ from the salted genesis'
    row['genesisMoneyRows'] = actual
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


def plan_for(binary, salt, manifest, timeout):
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
                argv=[str(binary), '--private-probe-profile=native-v1', '--private-probe-salt=' + salt,
                      '--testnet', '--netsuffix=12', '--nodnsseed', '--outpeers=0', '--disable-upnp',
                      '--listen=' + p2p, '--rpclisten=' + grpc, '--rpclisten-json=' + wrpc,
                      '--evm-rpc-listen=' + eth, '--appdir={' + role + '_APPDIR}',
                      '--ram-scale=0.1', '--utxoindex', '--unsaferpc', '--nologfiles', '--loglevel=warn'])
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--salt', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--timeout', type=int, default=120)
    args = parser.parse_args()
    binary = Path(args.binary).resolve(strict=True)
    assert binary.is_file() and os.access(binary, os.X_OK), 'supply the compiled opt-in private helper'
    assert len(args.salt) == 64 and int(args.salt, 16) != 0, 'an explicit nonzero 32-byte private salt is required'
    raw = subprocess.run([str(binary), '--private-probe-profile=native-v1', '--private-probe-salt=' + args.salt,
                          '--testnet', '--netsuffix=12', '--print-private-probe-profile'],
                         check=True, capture_output=True, text=True, timeout=30).stdout
    manifest = json.loads(raw)
    assert manifest['kind'] == 'rfc12-private-integration-probe'
    assert manifest['skipProofOfWork'] is False and manifest['releaseApproval'] is False
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc12-construction-'))
    plan = plan_for(binary, args.salt, manifest, args.timeout)
    plan_bytes = (json.dumps(plan, sort_keys=True, indent=2) + '\n').encode()
    (root / 'plan.json').write_bytes(plan_bytes)  # fixed before the first process starts
    drill = transport.Drill(plan, root)
    result = dict(status='FAIL', artifactDirectory=str(root), planSha256=hashlib.sha256(plan_bytes).hexdigest(),
                  fullRfc12SecurityApproval=False, release100kMeasured=False, prunedIbdMeasured=False,
                  matureUsefulWorkMeasured=False, marketFillsMeasured=False)
    try:
        run(drill, manifest)
        result['status'] = 'CONSTRUCTION_TRANSPORT_PASS'
    except transport.Blocked as error:
        result.update(status='BLOCKED', reason=str(error))
    except Exception as error:
        result['reason'] = repr(error)
    finally:
        drill.close()
        result.update(observations=drill.observations, probeLatenciesMs=drill.latencies)
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'observations'}, indent=2))
    return 0 if result['status'] == 'CONSTRUCTION_TRANSPORT_PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
