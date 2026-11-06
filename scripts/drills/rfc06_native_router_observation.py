"""Actual router bindings and exact committed native court outcomes.

Op 245 is a chain binding read, not a court receipt. Op 204's courtConvicted
is intentionally not used as proof that this particular reader was adjudicated.
"""
import hashlib
import json
import struct

from rfc08_native_paid_observation import need, hash64, counter

MAX_BYTES = 1024 * 1024
EXPECTED = {'classId', 'producerBond', 'bindingSha256', 'table', 'reader', 'router'}


def validate_expected(expected):
    need(isinstance(expected, dict) and set(expected) in (EXPECTED, EXPECTED | {'court'}), 'router observation scope fields differ')
    hash64(expected['classId'])
    bond = expected['producerBond']
    need(isinstance(bond, str) and ':' in bond, 'router producer bond absent')
    txid, index = bond.split(':', 1)
    hash64(txid)
    need(index.isascii() and index.isdecimal() and str(int(index)) == index and int(index) < 2**32,
         'router producer bond index is not canonical u32')
    digest = expected['bindingSha256']
    need(isinstance(digest, str) and len(digest) == 64 and all(c in '0123456789abcdef' for c in digest),
         'router binding digest is not canonical SHA256')
    need(type(expected['table']) is int and expected['table'] in (0, 1), 'router table differs')
    counter(expected['reader'], 16)
    counter(expected['router'], 16)
    need(expected['router'] < expected['reader'], 'router must precede reader')
    if 'court' in expected:
        court = expected['court']
        need(isinstance(court, dict) and set(court) == {'blockHash', 'objectId', 'objectTag', 'route', 'leafIndex', 'verdict'},
             'exact court expectation fields differ')
        hash64(court['blockHash'])
        hash64(court['objectId'])
        need(type(court['objectTag']) is int and court['objectTag'] == 244 and court['route'] == 'RoutedStep',
             'reader/router court requires the routed tag 244 terminal')
        counter(court['leafIndex'])
        need(court['verdict'] in ('ExecutorGuilty', 'FalseAccusation'), 'court verdict differs')



def validate_payload(claim, expected, dispute, binding, daa):
    validate_expected(expected)
    counter(daa)
    need(dispute.get('available') is True and type(dispute.get('observationVersion')) is int and dispute['observationVersion'] == 1,
         'legacy claim observation unavailable/version differs')
    raw = dispute.get('json')
    need(isinstance(raw, str) and len(raw) <= MAX_BYTES, 'legacy claim framing exceeds bound')
    value = json.loads(raw)
    need(type(value.get('version')) is int and value['version'] == 1, 'legacy observation version differs')
    need(value.get('claimId') == hash64(claim) and value.get('classId') == expected['classId'] and
         value.get('producer') == expected['producerBond'], 'router observation names another claim/class/producer')
    accepted = counter(value.get('acceptedDaa'))
    need(accepted <= daa, 'claim clock is in the future')
    need(binding.get('available') is True and binding.get('armed') is True and binding.get('found') is True,
         'router binding unavailable, dormant, or absent')
    need(counter(binding.get('nowDaa')) == daa, 'binding DAA is outside bracket')
    bound = counter(binding.get('boundFromDaa'))
    need(bound <= accepted, 'binding was declared after the claim')
    wire = binding.get('rowHex')
    need(isinstance(wire, str) and 28 <= len(wire) <= 2 * MAX_BYTES and len(wire) % 2 == 0 and
         all(c in '0123456789abcdef' for c in wire), 'router row is not bounded canonical hex')
    data = bytes.fromhex(wire)
    version, row_daa, count = struct.unpack('<HQI', data[:14])
    need(version == 1 and row_daa == bound and len(data) == 14 + 5 * count,
         'router row version/clock/framing differs')
    readers = [struct.unpack('<BHH', data[i:i+5]) for i in range(14, len(data), 5)]
    need(len({(t, r) for t, r, _ in readers}) == len(readers) and
         all(t in (0, 1) and router < reader for t, reader, router in readers), 'router row has invalid/duplicate readers')
    need((expected['table'], expected['reader'], expected['router']) in readers, 'expected reader/router is not bound')
    need(hashlib.sha256(data).hexdigest() == expected['bindingSha256'], 'exact router row differs from frozen scope')
    return dict(claimId=claim, classId=expected['classId'], producerBond=expected['producerBond'],
                acceptedDaa=accepted, boundFromDaa=bound, rowHex=wire, bindingSha256=expected['bindingSha256'],
                table=expected['table'], reader=expected['reader'], router=expected['router'])


def observe(drill, requirement, probe_context):
    expected = requirement['expected']
    validate_expected(expected)
    rpc = drill.rpc(requirement['role'])
    try:
        before, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        need(drill.assert_network(dict(dag=before, node=node)), 'native import incomplete')
        stamp = probe_context(before, node)
        dispute = rpc.call('getPalwLegacyDispute', {'claimId': requirement['claimId']})
        binding = rpc.call('getPalwRouterBindingV2', {'classHex': expected['classId']})
        court = (rpc.call('getPalwMemoryRoutingOutcomeV1', {'blockHash': expected['court']['blockHash'],
                  'claimId': requirement['claimId']}) if 'court' in expected else None)
        after, after_node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        need(drill.assert_network(dict(dag=after, node=after_node)) and stamp == probe_context(after, after_node),
             'router observation crossed a changing sink/import context')
        value = validate_payload(requirement['claimId'], expected, dispute, binding, before['virtualDaaScore'])
        outcome = validate_outcome(court, requirement, before) if court is not None else None
        return dict(context=stamp, binding=value, courtOutcome=outcome)
    finally:
        rpc.close()


def validate_outcome(response, requirement, dag):
    if response.get('available') is False:
        return None
    need(response.get('available') is True, 'court outcome availability differs')
    wire = response.get('observationJson')
    need(isinstance(wire, str) and len(wire) <= MAX_BYTES, 'court observation exceeds bounded framing')
    envelope = json.loads(wire)
    need(type(envelope.get('version')) is int and envelope['version'] == 1 and
         envelope.get('kind') == 'nativeCommittedMemoryRoutingOutcomeV1', 'court observation version/kind differs')
    need(envelope.get('nodeLocalObservationOnly') is True and envelope.get('releaseApproval') is False and
         envelope.get('securitySoundnessMeasured') is False, 'court observation substitutes broader authority')
    need(envelope.get('sink') == dag['sink'], 'court observation selected sink differs from bracket')
    row = envelope.get('observation')
    expected = requirement['expected']
    court = expected['court']
    need(isinstance(row, dict) and row.get('block') == court['blockHash'], 'court observation block differs')
    need(hash64(row.get('postStateRoot')) != '0' * 128 and counter(row.get('daa')) <= counter(dag['virtualDaaScore']),
         'court observation state/clock differs')
    outcomes = row.get('outcomes')
    need(isinstance(outcomes, list) and 1 <= len(outcomes) <= 64, 'court outcome list exceeds bound')
    matches = [item for item in outcomes if isinstance(item, dict) and item.get('objectId') == court['objectId']]
    need(len(matches) == 1, 'exact court object absent or ambiguous')
    item = matches[0]
    need(item.get('claimId') == requirement['claimId'] and item.get('classId') == expected['classId'],
         'court outcome names another claim/class')
    for key in ('objectTag', 'route', 'leafIndex', 'verdict'):
        need(type(item.get(key)) is type(court[key]) and item[key] == court[key], 'court outcome differs: ' + key)
    elapsed = item.get('adjudicationAndSettlementMicros')
    if elapsed is not None:
        counter(elapsed)
    return envelope


def fact(requirement, observation):
    outcome = observation.get('courtOutcome')
    return dict(id=requirement['id'], kind=requirement['kind'], role=requirement['role'], claimId=requirement['claimId'],
                status='PASS' if outcome is not None else 'UNKNOWN', nativeAuthorityEstablished=outcome is not None,
                nativeRouterBinding=observation, routerBindingObserved=True,
                routeAdjudication='OBSERVED' if outcome is not None else 'UNKNOWN', releaseApproval=False,
                securitySoundnessMeasured=False,
                reason=('exact committed native routed court outcome and immutable reader binding observed' if outcome is not None else
                        'exact native reader binding observed; exact committed route outcome unavailable/unrequested'))
