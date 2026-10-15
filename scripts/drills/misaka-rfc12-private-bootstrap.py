#!/usr/bin/env python3
"""Run an actual signed native registration/binding over three private processes.

This finite probe runs an exact named private factory, the normal PALW floor
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
import rfc12_complete_bootstrap as complete

SPEC = importlib.util.spec_from_file_location('rfc12_construction_smoke',
    Path(__file__).with_name('misaka-rfc12-private-smoke.py'))
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)
transport = smoke.transport


class FiniteBootstrapDrill(smoke.ResourceDrill):
    def wait(self, predicate, label):
        def bounded():
            if time.monotonic() >= self.workflow_deadline:
                raise transport.Blocked('the total predeclared workflow deadline expired: ' + label)
            return predicate()
        return super().wait(bounded, label)


def validate_factory_manifest(bootstrap, profile):
    if profile not in ('complete-v1', 'paid-v3'):
        raise ValueError('unallocated private bootstrap profile')
    if bootstrap.get('skipProofOfWork') is not False or bootstrap.get('releaseApproval') is not False:
        raise ValueError('the private factory must retain real PoW and no release approval')
    if profile == 'paid-v3':
        if bootstrap.get('schema') != 'misaka.private-paid-bootstrap.v3':
            raise ValueError('paid-v3 requires its own authenticated bootstrap manifest')
        if bootstrap.get('baseArtifactsAreControlReceipts') is not False:
            raise ValueError('old base artifacts cannot supply paid-v3 control receipts')
        if bootstrap.get('paidClaimIncluded') is not False or bootstrap.get('nativeFinalObserved') is not False:
            raise ValueError('offline paid-v3 bootstrap cannot assert native claim or Final')
    elif bootstrap.get('kind') != 'rfc12-private-complete-bootstrap-file-fixture':
        raise ValueError('complete-v1 requires its exact original bootstrap manifest')


NATIVE_PROVIDER_KIND = 'rfc08-pav3-loopback-native-provider-v1'
NATIVE_PROVIDER_HOST, NATIVE_EXCHANGE_CLIENT = 'B', 'A'
NATIVE_EXECUTOR_SEAT, NATIVE_PROVIDER_SEATS = 0, (1, 2)


def native_provider_config(fixture, bootstrap, listen):
    """RFC-0008 PAV3 loopback provider wiring (2026-10-15; unverified source, unapproved mechanics).

    Node B hosts a provider service over fixture seats 1 and 2 (distinct card operators, neither the executor's seat 0)
    on one loopback socket; node A's producer seals PAV3 custody carriages through it. The node binary refuses any
    other seat/key/operator relation itself; this only names the wiring. It applies to non-floor classes past
    palw_monetary_material_v1 with an enrolled native session: the BASE floor producer this drill runs stays PAV2,
    so requesting it measures nothing about native custody by itself.
    """
    seats = bootstrap.get('seats') or []
    if len(seats) <= max(NATIVE_PROVIDER_SEATS):
        raise ValueError('the PAV3 loopback provider needs two provider seats beside the executor seat')
    keys, bonds = [], []
    for index in NATIVE_PROVIDER_SEATS:
        seat = seats[index]
        key = str(fixture / seat['keyFile'])
        if ',' in key or '@' in key or not str(seat['bond']).count(':') == 1:
            raise ValueError('a provider seat key path or bond cannot be named unambiguously')
        keys.append('%s@%s' % (key, seat['bond']))
        bonds.append(seat['bond'])
    if seats[NATIVE_EXECUTOR_SEAT]['bond'] in bonds or len(set(bonds)) != len(bonds):
        raise ValueError('provider seats must be distinct from each other and from the executor seat')
    host_argv = ['--private-probe-native-provider-keys=' + ','.join(keys), '--private-probe-native-provider-listen=' + listen]
    client_argv = ['--private-probe-native-exchange=' + listen]
    return dict(kind=NATIVE_PROVIDER_KIND, host=NATIVE_PROVIDER_HOST, exchangeClient=NATIVE_EXCHANGE_CLIENT,
        listen=listen, executorSeat=NATIVE_EXECUTOR_SEAT, providerSeats=list(NATIVE_PROVIDER_SEATS), providerBonds=bonds,
        hostArgv=host_argv, clientArgv=client_argv,
        appliesTo='non-floor classes past palw_monetary_material_v1; the BASE floor producer stays PAV2',
        unverifiedSource=True, nativeCustodyMeasured=False, releaseApproval=False)


NATIVE_SESSION_KIND = 'rfc08-pav3-prospective-native-session-v1'
# Enrollment and admission span from the observed DAA; capped by the factory's maxEnrollmentDaa.
NATIVE_SESSION_SPAN_DAA = 2000
MONETARY_MATERIAL_TABLE = 120
NATIVE_PROVIDER_STORE_DIR = 'palw-native-provider-v3'


def native_session_spec(bootstrap, daa, span=NATIVE_SESSION_SPAN_DAA):
    """RFC-0008 PAV3 prospective native session (2026-10-15 round 3; unverified source, unapproved mechanics).

    The JSON `misaka palw material-session draft --spec` takes: the registered NON-floor class (`class`), executed by
    seat 0 (A's producer) and custodied by seats 1 and 2 (B's provider service), sponsored by seat 0, at the paid
    factory's own monetary-material policy terms. Every value comes from the authenticated bootstrap manifest and the
    observed DAA; the node's fold re-checks every bound and refuses anything else.
    """
    policy = bootstrap.get('monetaryMaterialPolicy')
    seats = bootstrap.get('seats') or []
    if not isinstance(policy, dict) or policy.get('approval') is not False:
        raise ValueError('the paid-v3 bootstrap carries no unapproved monetary material policy terms')
    if len(seats) <= max(NATIVE_PROVIDER_SEATS):
        raise ValueError('a native session needs the executor seat and two provider seats')
    if bootstrap.get('class') == bootstrap.get('floorClass'):
        raise ValueError('a native session is for a non-floor class; the BASE floor is exempt from PAV3 custody')
    if any('operatorId' not in seats[index] for index in (NATIVE_EXECUTOR_SEAT,) + NATIVE_PROVIDER_SEATS):
        raise ValueError('the bootstrap seats do not name their card operators')
    if not isinstance(daa, int) or daa < 0 or span <= 0:
        raise ValueError('an observed DAA and a positive span are required')
    until = daa + min(span, int(policy['maxEnrollmentDaa']))
    executor = seats[NATIVE_EXECUTOR_SEAT]
    if len({seats[index]['operatorId'] for index in (NATIVE_EXECUTOR_SEAT,) + NATIVE_PROVIDER_SEATS}) != 3:
        raise ValueError('the executor and both providers need three distinct card operators')
    return dict(networkDomain=bootstrap['networkDomain'], sponsor=executor['bond'], policyId=policy['id'],
        windowDaa=int(policy['windowDaa']), maxMaterialBytes=int(policy['maxBytesPerContract']),
        enrollmentUntilDaa=until,
        nativeSession=dict(classId=bootstrap['class'], artifactRoot=bootstrap['artifactRoot'], executor=executor['bond'],
            operator=executor['operatorId'], maxUnits=int(policy['nativeSessionMaxUnits']), admitFromDaa=daa,
            admitUntilDaa=until),
        offers=[dict(provider=seats[index]['bond'], collateralSompi=int(policy['minimumCollateralSompi']),
            prepaidFeeSompi=int(policy['minimumFeeSompi'])) for index in NATIVE_PROVIDER_SEATS])


def material_session_commands(prefix, fixture, bootstrap, folder):
    """The offline CLI steps: one draft, one signature per party (sponsor = executor seat, then each provider seat with its
    own fixture key), one seal. Returns (label, argv) pairs and the object path."""
    seats = bootstrap['seats']
    draft, sealed = folder / 'native-session-draft.json', folder / 'native-session.borsh'
    steps = [('NATIVE_SESSION_DRAFT', prefix + ['palw', 'material-session', 'draft',
        '--spec=' + str(folder / 'native-session-spec.json'), '--out=' + str(draft)])]
    signatures = []
    for index in (NATIVE_EXECUTOR_SEAT,) + NATIVE_PROVIDER_SEATS:
        signature = folder / ('native-session-sig-%d.json' % index)
        signatures.append(signature)
        steps.append(('NATIVE_SESSION_SIGN_%d' % index, prefix + ['palw', 'material-session', 'sign',
            '--key-file=' + str(fixture / seats[index]['keyFile']), '--bond=' + seats[index]['bond'],
            '--draft=' + str(draft), '--out=' + str(signature)]))
    steps.append(('NATIVE_SESSION_SEAL', prefix + ['palw', 'material-session', 'seal', '--draft=' + str(draft),
        '--out=' + str(sealed), '--signature'] + [str(path) for path in signatures]))
    return steps, sealed


def _key_bytes(key):
    if isinstance(key, str):
        return bytes.fromhex(key)
    return bytes(key)


def monetary_rows_observed(pages):
    """Root keys (hex) of every complete table-120 row in an op-242 page sequence. An OBSERVATION for the drill's wait, never
    an authenticated proof (that is `MonetaryMaterialProofPagerV1`'s); incomplete or unavailable pages observe nothing."""
    roots = set()
    if not pages or any(not page.get('available') for page in pages):
        return None
    last = pages[-1]
    if (last.get('nextRow'), last.get('nextOffset')) != (last.get('totalRows'), 0):
        return None
    for page in pages:
        for fragment in page.get('fragments') or []:
            if fragment.get('table') == MONETARY_MATERIAL_TABLE and fragment.get('offset') == 0:
                roots.add(_key_bytes(fragment['key']).hex())
    return roots


def monetary_pages(drill, role, limit=1 << 20, maximum_pages=4096):
    status = rpc_read(drill, role, 'getBlockDagInfo')
    pages, row, offset = [], 0, 0
    for _ in range(maximum_pages):
        page = rpc_read(drill, role, 'getPalwMonetaryMaterial',
            {'blockHash': status['sink'], 'row': row, 'offset': offset, 'limitBytes': limit})
        pages.append(page)
        if not page.get('available'):
            return pages
        row, offset = page['nextRow'], page['nextOffset']
        if (row, offset) == (page['totalRows'], 0):
            return pages
    raise transport.Blocked('the monetary page walk exceeded its bound')


def custody_store_observed(appdir):
    """A provider published a bundle durably: some `<bond>.store` under the provider service directory is nonempty."""
    return sorted(str(path) for path in Path(appdir).rglob(NATIVE_PROVIDER_STORE_DIR + '/*.store')
        if path.is_file() and path.stat().st_size > 0)


def enroll_native_session(drill, sampler, cli, salt, fixture, bootstrap, budget):
    """Draft, sign (sponsor + both provider seats), seal and file the prospective native session, then wait until every
    node observes its row. Fees come out of the one workflow carrier budget."""
    folder = drill.root / 'native-session'
    folder.mkdir(exist_ok=True)
    daa = rpc_read(drill, 'A', 'getBlockDagInfo')['virtualDaaScore']
    spec = native_session_spec(bootstrap, daa)
    (folder / 'native-session-spec.json').write_text(json.dumps(spec, indent=2) + '\n')
    steps, sealed = material_session_commands(complete.cli_prefix(drill, cli, salt), fixture, bootstrap, folder)
    outputs = {label: command(drill, sampler, label, argv, 60) for label, argv in steps}
    draft = outputs['NATIVE_SESSION_DRAFT']
    assert draft['ok'] is True and draft['submitted'] is False and len(draft['signers']) == 3
    assert outputs['NATIVE_SESSION_SEAL']['consent'] == draft['consent']
    filed = complete.submit(drill, sampler, cli, salt, fixture, bootstrap, sealed, 'NATIVE_SESSION', budget, command)
    root = draft['root']

    def everywhere():
        for role in ('A', 'B', 'JOIN'):
            try:
                roots = monetary_rows_observed(monetary_pages(drill, role))
            except (OSError, RuntimeError, KeyError):
                return None
            if roots is None or root not in roots:
                return None
        return True
    drill.wait(everywhere, 'the prospective native session row on every node')
    return dict(kind=NATIVE_SESSION_KIND, spec=spec, consent=draft['consent'], root=root, observedDaa=daa,
        filed=filed, rowObservedEverywhere=True, rowObservationAuthenticated=False, releaseApproval=False)


def exercise_native_custody(drill, bootstrap):
    """Restart A's producer on the enrolled non-floor class (same DB, same exchange) and wait for B's provider service to
    durably publish a bundle. Observing the store is evidence the PAV3 exchange ran end to end on this fixture; it is not a
    Final, a court outcome or a GC observation."""
    argv = drill.plan['nodes']['A']['argv']
    floor = '--palw-producer-class=' + bootstrap['floorClass']
    if floor not in argv:
        raise AssertionError('A is not the floor producer this step replaces')
    drill.kill('A')
    argv[argv.index(floor)] = '--palw-producer-class=' + bootstrap['class']
    drill.start('A')
    drill.wait(lambda: ready(drill, 'A', bootstrap), 'A producing the enrolled non-floor class')
    for role in ('B', 'JOIN'):
        drill.join_link(role, 'A')
    stores = drill.wait(lambda: custody_store_observed(drill.root / NATIVE_PROVIDER_HOST) or None,
        'a provider bundle durably published by B for A\'s PAV3 attempt')
    return dict(producerClass=bootstrap['class'], providerStores=stores, custodyStoreObserved=True,
        finalObserved=False, gcObserved=False, releaseApproval=False)


def plan_for(binary, salt, fixture, bootstrap, timeout, profile='complete-v1', native_provider=False):
    validate_factory_manifest(bootstrap, profile)
    if native_provider and profile != 'paid-v3':
        raise ValueError('the PAV3 loopback native provider is a paid-v3 profile option only')
    reservations, endpoints = [], []
    try:
        for _ in range(16 if native_provider else 15):
            listener = socket.socket()
            listener.bind(('127.0.0.1', 0))
            reservations.append(listener)
            endpoints.append('127.0.0.1:%d' % listener.getsockname()[1])
        nodes = {}
        for index, role in enumerate(('A', 'B', 'JOIN')):
            p2p, grpc, wrpc, borsh, eth = endpoints[index * 5:index * 5 + 5]
            seat = bootstrap['seats'][index]
            argv = [str(binary), '--private-probe-profile=' + profile, '--private-probe-salt=' + salt,
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
        plan = dict(schemaVersion=1, kind='rfc12-private-registration-binding-only', nodes=nodes, privateProfile=profile,
            scenario_timeout_s=timeout, probe_timeout_s=15, command_timeout_s=60,
            fullRfc12SecurityApproval=False, release100kMeasured=False)
        if profile == 'paid-v3':
            # Named for every paid-v3 plan; None unless explicitly requested, so the default paid-v3 run is unchanged.
            provider = native_provider_config(fixture, bootstrap, endpoints[15]) if native_provider else None
            if provider:
                nodes[provider['host']]['argv'].extend(provider['hostArgv'])
                nodes[provider['exchangeClient']]['argv'].extend(provider['clientArgv'])
            plan['nativeCustodyProvider'] = provider
        return plan
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
    if row['tipDaa'] != after['virtualDaaScore']:
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
    remaining = getattr(drill, 'workflow_deadline', float('inf')) - time.monotonic()
    if remaining <= 0:
        raise transport.Blocked('the total predeclared workflow deadline expired before ' + label)
    timeout = min(timeout, remaining)
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


def registration_window(bootstrap, daa):
    start, end = bootstrap['envelopeValidFromDaa'], bootstrap['envelopeValidUntilDaa']
    if not start <= daa <= end:
        raise transport.Blocked('no carrier submitted: registration envelope DAA window %d..%d excludes actual DAA %d' % (start, end, daa))
    return dict(validFromDaa=start, validUntilDaa=end, observedDaa=daa)


def activate_complete_clock(drill, bootstrap):
    # The shipped salted policy's attempt lane does not advance DAA at genesis.
    # Enable the ordinary bondless recovery miner only AFTER the DAA-0 signed
    # registration is included. Starting it earlier can expire that envelope.
    # Restart an observer, preserving A's live producer fee-outpoint rollover.
    role = 'B'
    address = bootstrap['seats'][1]['payAddress']
    if not isinstance(address, str) or not address:
        raise ValueError('the exact factory must supply the owned heartbeat pay address')
    flag = '--palw-heartbeat-miner-address=' + address
    if any(argument.startswith('--palw-heartbeat-miner-address')
            for node in drill.plan['nodes'].values() for argument in node['argv']):
        raise AssertionError('the registration phase must not have a recovery miner enabled')
    drill.kill(role)
    drill.plan['nodes'][role]['argv'].append(flag)
    drill.start(role)
    drill.wait(lambda: ready(drill, role, bootstrap), 'cold observer with ordinary heartbeat miner and exact private identity')
    drill.join_link(role, 'A')
    rows = drill.wait(lambda: agreement(drill, bootstrap), 'native registration/binding retained after enabling recovery mining')
    return dict(role=role, minerAddress=address, consensusClockInjected=False, agreement=rows)


def run(drill, sampler, cli, salt, fixture, bootstrap, maximum_fee, complete_check=False,
        maximum_binding=None, maximum_check=None, progress=None, native_session=False):
    if native_session and complete_check:
        raise ValueError('the native session step replaces the floor producer; run it without --complete-check')
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
    window = registration_window(bootstrap, rpc_read(drill, 'A', 'getBlockDagInfo')['virtualDaaScore'])
    preview = command(drill, sampler, 'CARRIER_PREVIEW', argv, 60)
    assert preview['dry_run'] is True and preview['total_fee_sompi'] <= maximum_fee
    # Queue the factory-priced registration before enabling the producer's
    # first peer, so no earlier floor production can retarget the live price.
    registration_window(bootstrap, rpc_read(drill, 'A', 'getBlockDagInfo')['virtualDaaScore'])
    sent = command(drill, sampler, 'CARRIER_SUBMIT', argv + ['--yes'], 60)
    assert sent['ok'] is True and sent['submitted'] is True and len(sent['carriers']) == 2
    budget = dict(maximum=maximum_fee, spent=0)
    complete.cap_and_charge(sent, budget)
    for role in ('B', 'JOIN'):
        drill.join_link(role, 'A')
    included = drill.wait(lambda: agreement(drill, bootstrap), 'actual native registration/binding across ordinary P2P')
    evidence = dict(registrationWindow=window, preview=preview, submission=sent, included=included,
        cumulativeCarrierFeeSompi=budget['spent'], carrierBudget=budget, completeCheck=None)
    # Retain completed observations even if a later maturity/check/reopen fails.
    # The live budget records deductions made by later batches before a refusal;
    # neither this checkpoint nor carrier inclusion is a complete-check PASS.
    if progress:
        progress(evidence, False)
    if native_session:
        evidence['nativeSession'] = enroll_native_session(drill, sampler, cli, salt, fixture, bootstrap, budget)
        evidence['cumulativeCarrierFeeSompi'] = budget['spent']
        if progress:
            progress(evidence, False)
        evidence['nativeCustody'] = exercise_native_custody(drill, bootstrap)
        if progress:
            progress(evidence, False)
    complete_evidence = None
    if complete_check:
        evidence['recoveryClock'] = activate_complete_clock(drill, bootstrap)
        complete_evidence = complete.run(drill, sampler, cli, salt, fixture, bootstrap, budget,
            maximum_binding, maximum_check, agreement, rpc_read, command)
        evidence['completeCheck'] = complete_evidence
    # Freeze production by terminating only our actual producer child; reopen
    # its same DB as an observer with the identical consensus factory.
    drill.kill('A')
    drill.plan['nodes']['A']['argv'].remove('--palw-produce')
    drill.start('A')
    reopened = drill.wait(lambda: agreement(drill, bootstrap), 'actual cold reopen native registration/binding agreement')
    if complete_check:
        complete_evidence['cold'] = complete.verify_after_cold_reopen(drill, sampler, cli, salt, fixture,
            bootstrap, agreement, rpc_read, command)
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
    evidence.update(reopened=reopened, cumulativeCarrierFeeSompi=budget['spent'], completeCheck=complete_evidence)
    if progress:
        progress(evidence, True)
    return evidence


def record_observed_progress(result, evidence, cold_reopened):
    result.update(nativeRegistrationBindingMeasured=True, coldReopenMeasured=cold_reopened, evidence=evidence,
        nativeSessionRowObserved=bool(evidence.get('nativeSession')),
        nativeCustodyStoreObserved=bool((evidence.get('nativeCustody') or {}).get('custodyStoreObserved')))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--cli', required=True)
    parser.add_argument('--salt', required=True)
    parser.add_argument('--private-profile', choices=('complete-v1', 'paid-v3'), default='complete-v1',
        help='exact private policy; paid-v3 prepares new scope and never promotes old fixture receipts')
    parser.add_argument('--maximum-carrier-fee-sompi', required=True, type=int)
    parser.add_argument('--resource-limits', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--factory-timeout', type=int, default=60)
    parser.add_argument('--native-loopback-provider', action='store_true',
        help='paid-v3 only: host the RFC-0008 PAV3 loopback provider (seats 1,2) on B for A\'s producer; unverified source')
    parser.add_argument('--native-session', action='store_true',
        help='with --native-loopback-provider: enroll a prospective native session for the registered non-floor class '
        '(sponsor seat 0, providers seats 1,2), then switch A to that class and wait for a provider bundle; unverified source')
    parser.add_argument('--complete-check', action='store_true', help='also wait actual binding maturity and carry/verify the exhaustive code-fixture check')
    parser.add_argument('--maximum-binding-reservation-sompi', type=int, help='required with --complete-check; fixed before node launch')
    parser.add_argument('--maximum-complete-check-fee-sompi', type=int, help='required with --complete-check; fixed before node launch')
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
        coldReopenMeasured=False, privateProfile=args.private_profile,
        nativeLoopbackProviderRequested=args.native_loopback_provider, nativeCustodyMeasured=False,
        nativeSessionRequested=args.native_session, nativeSessionRowObserved=False, nativeCustodyStoreObserved=False,
        completeCheckRequested=args.complete_check, maximumBindingReservationSompi=args.maximum_binding_reservation_sompi,
        maximumCompleteCheckFeeSompi=args.maximum_complete_check_fee_sompi, totalWorkflowTimeoutSeconds=args.timeout,
        completeCheckMeasured=False, matureUsefulWorkMeasured=False, prunedIbdMeasured=False,
        marketFillsMeasured=False, release100kMeasured=False, fullRfc12SecurityApproval=False)
    try:
        if args.native_session and not (args.native_loopback_provider and args.private_profile == 'paid-v3'):
            raise ValueError('--native-session needs --native-loopback-provider on the paid-v3 profile')
        fixture = root / 'signed-public-fixture'
        factory = [str(binary), '--private-probe-profile=' + args.private_profile, '--private-probe-salt=' + args.salt,
            '--testnet', '--netsuffix=12', '--private-probe-export-bootstrap=' + str(fixture)]
        # Factory creates files only, and needs no consensus process or DB.
        began = time.monotonic()
        bootstrap = command(type('FactoryLog', (), {'root': root})(), sampler, 'MANIFEST', factory, args.factory_timeout)
        result['factorySeconds'] = time.monotonic() - began
        validate_factory_manifest(bootstrap, args.private_profile)
        if args.complete_check:
            complete.costs_before_nodes(bootstrap, args.maximum_binding_reservation_sompi, args.maximum_complete_check_fee_sompi)
        elif args.maximum_binding_reservation_sompi is not None or args.maximum_complete_check_fee_sompi is not None:
            raise ValueError('separate complete-check bond caps require --complete-check')
        plan = plan_for(binary, args.salt, fixture, bootstrap, args.timeout, args.private_profile,
            args.native_loopback_provider)
        (root / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
        drill = FiniteBootstrapDrill(plan, root, sampler, {})
        drill.workflow_deadline = time.monotonic() + args.timeout
        result['bootstrap'] = bootstrap
        result['evidence'] = run(drill, sampler, cli, args.salt, fixture, bootstrap, args.maximum_carrier_fee_sompi,
            args.complete_check, args.maximum_binding_reservation_sompi, args.maximum_complete_check_fee_sompi,
            lambda evidence, cold: record_observed_progress(result, evidence, cold), args.native_session)
        result.update(status='REGISTRATION_BINDING_TRANSPORT_PASS', nativeRegistrationBindingMeasured=True)
        if args.complete_check:
            result.update(status='COMPLETE_CHECK_TRANSPORT_PASS', completeCheckMeasured=True)
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
        if 'evidence' in result:
            result['evidence']['cumulativeCarrierFeeSompi'] = result['evidence']['carrierBudget']['spent']
        result['sampledOwnedResources'] = sampler.stop()
        if drill:
            drill.close()
        result['ownedDatabaseLogicalBytes'] = {role: sum(file.stat().st_size for file in (root / role).rglob('*') if file.is_file())
            for role in ('A', 'B', 'JOIN')}
        result['probeLatenciesMs'] = drill.latencies if drill else []
        if result['status'] in ('REGISTRATION_BINDING_TRANSPORT_PASS', 'COMPLETE_CHECK_TRANSPORT_PASS'):
            try:
                check_completed_limits(limits, result)
                result['resourceLimitsMet'] = True
            except RuntimeError as error:
                result.update(status='FAIL', reason=str(error))
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key not in ('evidence', 'bootstrap')}, indent=2))
    return 0 if result['status'] in ('REGISTRATION_BINDING_TRANSPORT_PASS', 'COMPLETE_CHECK_TRANSPORT_PASS') else 1


if __name__ == '__main__':
    raise SystemExit(main())
