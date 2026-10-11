#!/usr/bin/env python3
"""Run an actual signed native registration/binding over three private processes.

This finite probe runs the exact complete-v1 factory, the normal PALW floor
producer, the public Borsh CLI carrier path, and ordinary P2P. It never inserts
state or bypasses PoW, quorum, material windows, or conformance. Passing covers
registration/binding transport and cold persistence only; useful-work Final,
pruned join, 100k and security remain separate acceptance conditions.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

from rfc12_owned_resources import OwnedResourceSampler, check_completed_limits, read_limits

SPEC = importlib.util.spec_from_file_location('rfc12_construction_smoke',
    Path(__file__).with_name('misaka-rfc12-private-smoke.py'))
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)
transport = smoke.transport


def plan_for(binary, salt, fixture, bootstrap, timeout):
    reservations, endpoints = [], []
    try:
        for _ in range(15):
            listener = socket.socket()
            listener.bind(('127.0.0.1', 0))
            reservations.append(listener)
            endpoints.append('127.0.0.1:%d' % listener.getsockname()[1])
        nodes = {}
        for index, role in enumerate(('A', 'B', 'JOIN')):
            p2p, grpc, wrpc, borsh, eth = endpoints[index * 5:index * 5 + 5]
            seat = bootstrap['seats'][index]
            argv = [str(binary), '--private-probe-profile=complete-v1', '--private-probe-salt=' + salt,
                '--testnet', '--netsuffix=12', '--nodnsseed', '--outpeers=0', '--disable-upnp',
                '--listen=' + p2p, '--rpclisten=' + grpc, '--rpclisten-json=' + wrpc,
                '--rpclisten-borsh=' + borsh, '--evm-rpc-listen=' + eth,
                '--appdir={' + role + '_APPDIR}', '--ram-scale=0.1', '--utxoindex', '--unsaferpc',
                '--nologfiles', '--loglevel=info', '--enable-unsynced-mining',
                '--palw-producer-key=' + str(fixture / seat['keyFile']),
                '--palw-producer-bond=' + seat['bond'], '--palw-producer-pay-address=' + seat['payAddress'],
                '--palw-fee-outpoint=' + seat['feeOutpoint'],
                '--palw-class-artifact=' + str(fixture / bootstrap['artifact'])]
            if role == 'A':
                argv.extend(['--palw-produce', '--palw-producer-class=' + bootstrap['floorClass']])
            nodes[role] = dict(p2p=p2p, grpc=grpc, wrpc=wrpc, borsh=borsh, eth='http://' + eth, argv=argv)
        return dict(schemaVersion=1, kind='rfc12-private-registration-binding-only', nodes=nodes,
            scenario_timeout_s=timeout, probe_timeout_s=15, command_timeout_s=60,
            fullRfc12SecurityApproval=False, release100kMeasured=False)
    finally:
        for listener in reservations:
            listener.close()


def rpc_read(drill, role, method, payload=None):
    began = time.monotonic()
    rpc = drill.rpc(role)
    try:
        return rpc.call(method, payload or {})
    finally:
        rpc.close()
        drill.latencies.append(round((time.monotonic() - began) * 1000, 3))


def ready(drill, role, bootstrap):
    try:
        status = rpc_read(drill, role, 'getPalwNodeStatus')
        assert status['consensusParamsId'] == bootstrap['consensusParamsId'], 'private runtime factory ID mismatch'
        assert status['genesisHash'] == bootstrap['genesis'], 'private runtime genesis mismatch'
        assert transport.import_stamp(status)[3] is True, 'private import is incomplete'
        return status
    except (OSError, RuntimeError):
        return None


def onboarding(drill, role, bootstrap):
    # Recheck the known child's identity before using its public state.
    if not ready(drill, role, bootstrap):
        return None
    first = rpc_read(drill, role, 'getBlockDagInfo')
    row = rpc_read(drill, role, 'getPalwOnboarding', {'classId': bootstrap['class']})
    after = rpc_read(drill, role, 'getBlockDagInfo')
    if first['sink'] != after['sink'] or first['virtualDaaScore'] != after['virtualDaaScore']:
        return None
    if not row['available'] or not row['found']:
        return None
    assert row['artifactRoot'] == bootstrap['artifactRoot'], 'native artifact root substitution'
    assert row['registrant'] == bootstrap['bond'], 'native registrant substitution'
    bindings = [entry for entry in row['artifactBindings']
        if entry['kernelParamRoot'] == bootstrap['kernelParamRoot'] and entry['binder'] == bootstrap['bond']]
    if not bindings:
        return None
    assert len(bindings) == 1 and bindings[0]['state'] != 'Refuted', 'duplicate/refuted artifact binding'
    return dict(sink=after['sink'], virtualDaaScore=after['virtualDaaScore'], onboarding=row)


def agreement(drill, bootstrap):
    rows = {role: onboarding(drill, role, bootstrap) for role in ('A', 'B', 'JOIN')}
    if any(row is None for row in rows.values()):
        return None
    keys = [(row['sink'], row['virtualDaaScore'], row['onboarding']['ledgerRoot'], row['onboarding']['auxRoot'])
        for row in rows.values()]
    return rows if len(set(keys)) == 1 else None


def command(drill, sampler, label, argv, timeout):
    child = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    sampler.register(label, child)
    try:
        stdout, stderr = child.communicate(timeout=timeout)
        (drill.root / (label + '.stdout')).write_text(stdout)
        (drill.root / (label + '.stderr')).write_text(stderr)
        sampler.check()
        if child.returncode:
            raise transport.Blocked('%s exited %s: %s' % (label, child.returncode, stderr[-3000:]))
        return json.loads(stdout)
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate(timeout=5)


def run(drill, sampler, cli, salt, fixture, bootstrap, maximum_fee):
    for role in ('A', 'B', 'JOIN'):
        drill.start(role)
        drill.wait(lambda role=role: ready(drill, role, bootstrap), role + ' exact private process identity')
    # Validate actual carrier costs before permitting the first broadcast; the
    # capped CLI also repeats the check on its actual broadcast batch.
    argv = [str(cli), '--network=testnet-12', '--rpc=' + drill.plan['nodes']['A']['borsh'],
        '--palw-drill-genesis-salt=' + salt, '--output=json', 'palw', 'submit-object',
        '--key-file=' + str(fixture / bootstrap['fundingKeyFile']),
        '--maximum-total-fee-sompi=' + str(maximum_fee), '--object',
        str(fixture / bootstrap['signedRegistration']), str(fixture / bootstrap['artifactBinding'])]
    preview = command(drill, sampler, 'CARRIER_PREVIEW', argv, 60)
    assert preview['dry_run'] is True and preview['total_fee_sompi'] <= maximum_fee
    # Queue the factory-priced registration before enabling the producer's
    # first peer, so no earlier floor production can retarget the live price.
    sent = command(drill, sampler, 'CARRIER_SUBMIT', argv + ['--yes'], 60)
    assert sent['ok'] is True and sent['submitted'] is True and len(sent['carriers']) == 2
    for role in ('B', 'JOIN'):
        drill.join_link(role, 'A')
    included = drill.wait(lambda: agreement(drill, bootstrap), 'actual native registration/binding across ordinary P2P')
    # Freeze production by terminating only our actual producer child; reopen
    # its same DB as an observer with the identical consensus factory.
    drill.kill('A')
    drill.plan['nodes']['A']['argv'].remove('--palw-produce')
    drill.start('A')
    reopened = drill.wait(lambda: agreement(drill, bootstrap), 'actual cold reopen native registration/binding agreement')
    before = included['A']['onboarding']
    after = reopened['A']['onboarding']
    for field in ('classId', 'artifactRoot', 'registrant', 'artifactBindings'):
        if field == 'artifactBindings':
            # State (Pending/Matured/Final) is resolved at the current clock.
            columns = ('kernelParamRoot', 'binder', 'boundDaa', 'maturesDaa', 'finalDaa', 'reservedSompi')
            assert [tuple(row[k] for k in columns) for row in before[field]] == \
                [tuple(row[k] for k in columns) for row in after[field]], 'cold persistence changed binding terms'
        else:
            assert before[field] == after[field], 'cold persistence changed ' + field
    return dict(preview=preview, submission=sent, included=included, reopened=reopened)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--cli', required=True)
    parser.add_argument('--salt', required=True)
    parser.add_argument('--maximum-carrier-fee-sompi', required=True, type=int)
    parser.add_argument('--resource-limits', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--factory-timeout', type=int, default=60)
    args = parser.parse_args()
    binary, cli = Path(args.binary).resolve(strict=True), Path(args.cli).resolve(strict=True)
    assert all(path.is_file() and os.access(path, os.X_OK) for path in (binary, cli)), 'supply actual helper and CLI executables'
    assert len(args.salt) == 64 and int(args.salt, 16) != 0, 'an explicit nonzero private 32-byte salt is required'
    assert 0 < args.maximum_carrier_fee_sompi <= 2**64 - 1 and args.timeout > 0 and args.factory_timeout > 0
    limits = read_limits(args.resource_limits)
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc12-bootstrap-'))
    sampler = OwnedResourceSampler({}, limits)
    drill = None
    result = dict(status='FAIL', artifactDirectory=str(root), resourceLimits=limits, resourceLimitsMet=False,
        maximumCarrierFeeSompi=args.maximum_carrier_fee_sompi, nativeRegistrationBindingMeasured=False,
        completeCheckMeasured=False, matureUsefulWorkMeasured=False, prunedIbdMeasured=False,
        marketFillsMeasured=False, release100kMeasured=False, fullRfc12SecurityApproval=False)
    try:
        fixture = root / 'signed-public-fixture'
        factory = [str(binary), '--private-probe-profile=complete-v1', '--private-probe-salt=' + args.salt,
            '--testnet', '--netsuffix=12', '--private-probe-export-bootstrap=' + str(fixture)]
        # Factory creates files only, and needs no consensus process or DB.
        began = time.monotonic()
        bootstrap = command(type('FactoryLog', (), {'root': root})(), sampler, 'MANIFEST', factory, args.factory_timeout)
        result['factorySeconds'] = time.monotonic() - began
        assert bootstrap['skipProofOfWork'] is False and bootstrap['releaseApproval'] is False
        plan = plan_for(binary, args.salt, fixture, bootstrap, args.timeout)
        (root / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
        drill = smoke.ResourceDrill(plan, root, sampler, {})
        result['bootstrap'] = bootstrap
        result['evidence'] = run(drill, sampler, cli, args.salt, fixture, bootstrap, args.maximum_carrier_fee_sompi)
        result.update(status='REGISTRATION_BINDING_TRANSPORT_PASS', nativeRegistrationBindingMeasured=True)
    except Exception as error:
        result['reason'] = str(error)
        if drill:
            result['lastNodeStatuses'] = {}
            for role in drill.processes:
                try:
                    result['lastNodeStatuses'][role] = rpc_read(drill, role, 'getPalwNodeStatus')
                except Exception as status_error:
                    result['lastNodeStatuses'][role] = {'error': str(status_error)}
    finally:
        result['sampledOwnedResources'] = sampler.stop()
        if drill:
            drill.close()
        result['ownedDatabaseLogicalBytes'] = {role: sum(file.stat().st_size for file in (root / role).rglob('*') if file.is_file())
            for role in ('A', 'B', 'JOIN')}
        result['probeLatenciesMs'] = drill.latencies if drill else []
        if result['status'] == 'REGISTRATION_BINDING_TRANSPORT_PASS':
            try:
                check_completed_limits(limits, result)
                result['resourceLimitsMet'] = True
            except RuntimeError as error:
                result.update(status='FAIL', reason=str(error))
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key not in ('evidence', 'bootstrap')}, indent=2))
    return 0 if result['status'] == 'REGISTRATION_BINDING_TRANSPORT_PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
