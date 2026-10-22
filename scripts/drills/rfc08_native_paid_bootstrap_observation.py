"""Explicit V6 sidecars: current source graph and receipt structure only.

These facts do not satisfy historical signature, full selected approval,
independent availability, actual payments, or economic/security review gates.
"""
import rfc08_native_paid_observation as base

KINDS = frozenset(('native-paid-bootstrap-observed', 'native-paid-bootstrap-quorum', 'native-paid-bootstrap-source-graph'))
UNRESOLVED = ('nativeSelectedApproval', 'historicalSignatureReplay', 'coinbaseOutputReplayed', 'oneTimeFeeSettlement')
HASH_FIELDS = ('enrollment', 'context', 'checkerSnapshot', 'policy', 'binding', 'bootstrapEpoch',
    'sourceContext', 'sourceRoot', 'sourceOutput')
FIELDS = frozenset(('version', 'snapshot', 'baseClaim', 'authorityPresent', 'explicitV6Binding',
    'requiredSources', 'currentSources', 'sourceStage', 'sourceLockedDaa', 'lock', 'nativeConsumerComplete',
    'nativeAssignedQuorum', 'sourceGraphRevalidated', 'finalProjection', 'releaseApproval',
    'securitySoundnessMeasured') + HASH_FIELDS + UNRESOLVED)


def directory(drill, role):
    return base.directory(drill, role, base.BOOTSTRAP_DIRECTORY)


def validate(envelope, claim, dag, node, import_stamp, params, genesis):
    need, hash64, counter = base.need, base.hash64, base.counter
    need(isinstance(envelope, dict) and type(envelope.get('schemaVersion')) is int and envelope['schemaVersion'] == 6,
        'V6 observation schema differs; a V5 lock cannot supply bootstrap provenance')
    need(envelope.get('kind') == 'rfc08-private-native-paid-bootstrap-observation-v6', 'V6 observation kind differs')
    need(envelope.get('baseClaim') == hash64(claim), 'V6 observation names another exact claim')
    need(envelope.get('consensusParamsId') == node['consensusParamsId'] == params and
        envelope.get('genesis') == node['genesisHash'] == genesis, 'V6 observation network/Params differs')
    need(envelope.get('ownProcessObservationOnly') is True and envelope.get('releaseApproval') is False and
        envelope.get('securitySoundnessMeasured') is False, 'V6 private observation advertises unsupported authority')
    if envelope.get('available') is False:
        need(envelope.get('observation') is None, 'unavailable V6 observation contains a positive payload')
        return None
    need(envelope.get('available') is True and envelope.get('error') is None, 'V6 observation is unavailable')
    value = envelope.get('observation')
    need(isinstance(value, dict) and set(value) == FIELDS and type(value.get('version')) is int and value['version'] == 6,
        'V6 observation payload field/version contract differs')
    need(value.get('baseClaim') == claim and value.get('releaseApproval') is False and value.get('securitySoundnessMeasured') is False,
        'V6 observation payload identity/authority differs')
    snapshot = value['snapshot']
    need(isinstance(snapshot, dict) and set(snapshot) == {'checkpoint', 'statePoint', 'stateRoot', 'daa', 'importEpoch'},
        'V6 snapshot schema differs')
    need(hash64(snapshot['checkpoint']) == hash64(snapshot['statePoint']) == dag['sink'] and
        hash64(snapshot['stateRoot']) != '0' * 128, 'V6 snapshot is not current own selected state')
    need(counter(snapshot['daa']) == counter(dag['virtualDaaScore']), 'V6 snapshot DAA differs')
    epoch, point, phase, stable = import_stamp(node)
    need(stable is True and point == '' and phase in ('idle', 'legacy-stable') and
        counter(snapshot['importEpoch']) == counter(epoch), 'V6 observation import incomplete/changed')
    for key in ('authorityPresent', 'explicitV6Binding'):
        need(type(value[key]) is bool, 'V6 boolean differs: ' + key)
    need(value['nativeConsumerComplete'] is None or type(value['nativeConsumerComplete']) is bool,
        'V6 native completeness differs')
    for key in HASH_FIELDS:
        if value[key] is not None:
            need(hash64(value[key]) != '0' * 128, 'V6 native identity is zero: ' + key)
    required, found = counter(value['requiredSources'], 32), counter(value['currentSources'], 32)
    need(found <= required <= 65_536, 'V6 source count exceeds native contract')
    stage = value['sourceStage']
    need(stage in ('MISSING', 'PENDING', 'UNAVAILABLE', 'LOCKED'), 'V6 source stage differs')
    if value['explicitV6Binding']:
        need(value['authorityPresent'] and required > 0 and all(value[k] is not None for k in HASH_FIELDS[:6]),
            'V6 binding lacks its exact native graph identities')
    else:
        need(stage == 'MISSING' and value['lock'] is None and required == found == 0,
            'legacy/missing binding cannot carry a V6 source or lock')
    if stage == 'LOCKED':
        need(found == required and required > 0 and all(value[k] is not None for k in HASH_FIELDS[6:]),
            'V6 locked source lacks its reconstructed context/root/output')
        need(counter(value['sourceLockedDaa']) <= snapshot['daa'], 'V6 source lock is from future DAA')
    else:
        need(value['sourceLockedDaa'] is None and all(value[k] is None for k in HASH_FIELDS[6:]),
            'unlocked source carries locked metadata')
    for key in UNRESOLVED + ('nativeAssignedQuorum', 'sourceGraphRevalidated', 'finalProjection'):
        need(value[key] in ('UNKNOWN', 'OBSERVED', 'INCOMPLETE'), 'V6 fact enum differs')
    lock = value['lock']
    if lock is not None:
        need(isinstance(lock, dict) and set(lock) == {'round', 'context', 'anchor', 'sampleSeed', 'assignment',
            'lockedDaa', 'seats', 'quorum', 'distinctCompleteReceipts', 'sourceValidNativeFlag', 'assignmentRecomputed'},
            'V6 paid lock schema differs')
        need(value['explicitV6Binding'] and lock['context'] == value['binding'], 'V6 lock substitutes legacy context')
        for key in ('context', 'anchor', 'sampleSeed', 'assignment'):
            hash64(lock[key])
        for key in ('round', 'seats', 'quorum', 'distinctCompleteReceipts'):
            counter(lock[key], 32)
        need(lock['round'] == 0 and lock['distinctCompleteReceipts'] <= lock['seats'] and
            counter(lock['lockedDaa']) <= snapshot['daa'], 'V6 lock round/count/clock differs')
        for key in ('sourceValidNativeFlag', 'assignmentRecomputed'):
            need(type(lock[key]) is bool, 'V6 lock boolean differs')
    return value


def observe(drill, role, claim, import_stamp, probe_context, params, genesis):
    base.need(base.paid_bootstrap_source_policy(drill.plan['nodes'][role]['argv']) is not None,
        'V6 observation requires an explicit frozen V6 source policy')
    return base.observe(drill, role, claim, import_stamp, probe_context, params, genesis,
        directory_fn=directory, validate_fn=validate)


def fact(requirement, observed):
    result = dict(id=requirement['id'], kind=requirement['kind'], role=requirement['role'], claimId=requirement['claimId'],
        status='UNKNOWN', nativeAuthorityEstablished=False, nativePaidBootstrapObservation=observed,
        selectedVerifierApproval='UNKNOWN', historicalSignatures='UNKNOWN', oneTimeFeeSettlement='UNKNOWN',
        releaseApproval=False, securitySoundnessMeasured=False, reason='current V6 native observation unavailable')
    if observed is None or not observed['available']:
        return result
    value = observed['observation']
    for key, expected in requirement['expected'].items():
        base.need(value.get(key) == expected, 'V6 observation differs from frozen expected ' + key)
    if not value['authorityPresent'] or not value['explicitV6Binding']:
        result['reason'] = 'no exact explicit V6 native admission binding'
        return result
    if requirement['kind'] == 'native-paid-bootstrap-observed':
        result.update(status='PASS', reason='exact own V6 native binding observed; full approval/payment remain UNKNOWN')
    lock = value['lock']
    graph = (value['sourceStage'] == 'LOCKED' and value['sourceGraphRevalidated'] == 'OBSERVED' and
        value['currentSources'] == value['requiredSources'] > 0 and lock is not None and
        lock['assignmentRecomputed'] is True and lock['sourceValidNativeFlag'] is True)
    if requirement['kind'] == 'native-paid-bootstrap-source-graph' and graph:
        result.update(status='PASS', reason='current own source/work/material/settlement graph matches V6 lock; historical signatures remain UNKNOWN')
    if requirement['kind'] == 'native-paid-bootstrap-quorum' and graph and value['nativeAssignedQuorum'] == 'OBSERVED' and value['nativeConsumerComplete'] is True:
        base.need(0 < lock['quorum'] <= lock['distinctCompleteReceipts'] <= lock['seats'], 'V6 distinct quorum bounds differ')
        result.update(status='PASS', reason='own V6 source graph and assigned receipt structure observed; full approval and all payments remain UNKNOWN')
    return result
