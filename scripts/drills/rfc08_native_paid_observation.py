"""Bounded observations from the current owned private child, never remote authority.

The v5 exporter deliberately lacks complete source/signature/payment provenance.
Its structural quorum, Final projection, escrow totals and latest drain cannot
turn those missing facts into a selected approval or a one-time settlement.
"""
import json
import os
from pathlib import Path
import stat

FLAG = '--private-probe-paid-observe-claims='
DIRECTORY = 'rfc08-native-paid-observations-v5'
BOOTSTRAP_DIRECTORY = 'rfc08-native-paid-bootstrap-observations-v6'
MAX_CLAIMS = 64
MAX_BYTES = 64 * 1024
PAID_KINDS = frozenset(('native-paid-observed', 'native-paid-assigned-quorum',
    'selected-verifier-approval', 'native-fee-settlement', 'all-rewards-once'))
UNRESOLVED = ('nativeSelectedApproval', 'sourceProvenanceRevalidated',
    'historicalSignatureReplay', 'coinbaseOutputReplayed', 'oneTimeFeeSettlement')
SOURCE_POLICY_FLAG = '--private-probe-source-policy='
PAID_BOOTSTRAP_POLICY_FLAG = '--private-probe-paid-bootstrap-source-policy='
GENERATION_FLAGS = ('--private-probe-kernel-paid-material-generation-v5=',
    '--private-probe-kernel-paid-material-generation-v7=')


class Unavailable(ValueError):
    pass


def need(condition, reason):
    if not condition:
        raise Unavailable(reason)


def hash64(value):
    need(isinstance(value, str) and len(value) == 128 and
        all(c in '0123456789abcdef' for c in value), 'noncanonical paid observation Hash64')
    return value


def counter(value, bits=64):
    need(type(value) is int and 0 <= value < 2**bits, 'paid observation counter is not an exact u' + str(bits))
    return value


def claims(argv):
    flags = [arg for arg in argv if arg.startswith(FLAG)]
    need(len(flags) <= 1, 'paid observation flag repeated')
    if not flags:
        return ()
    raw = flags[0][len(FLAG):]
    need(1 <= len(raw) <= MAX_CLAIMS * 129 and raw.count(',') < MAX_CLAIMS,
        'paid observation claim list exceeds its finite cap')
    values = tuple(raw.split(','))
    for value in values:
        need(hash64(value) != '0' * 128, 'paid observation claim may not be zero')
    need(len(values) == len(set(values)), 'paid observation claims repeated')
    profiles = [arg for arg in argv if arg.startswith('--private-probe-profile=')]
    need(len(profiles) == 1 and profiles[0] in ('--private-probe-profile=paid-v4', '--private-probe-profile=source-paid-v1'),
        'paid observations require one explicit private paid capability profile')
    source_policy(argv)
    need('--print-private-probe-profile' not in argv and not any(arg.startswith(('--private-probe-export-', '--private-probe-print-')) for arg in argv),
        'paid observations require live node mode')
    return values


def source_policy(argv):
    if '--private-probe-profile=source-paid-v1' not in argv:
        return None
    flags = [arg for arg in argv if arg.startswith(SOURCE_POLICY_FLAG)]
    need(len(flags) == 1, 'source-paid-v1 requires one explicit source policy file')
    path = flags[0][len(SOURCE_POLICY_FLAG):]
    need(Path(path).is_absolute() and '{' not in path and '\x00' not in path,
        'source-paid-v1 policy must be an explicit absolute frozen operator input')
    # The native factory validates canonical Borsh, nonzero activation and all
    # bounds. This collector neither chooses nor approves policy numbers.
    return path


def paid_bootstrap_source_policy(argv):
    flags = [arg for arg in argv if arg.startswith(PAID_BOOTSTRAP_POLICY_FLAG)]
    need(len(flags) <= 1, 'paid bootstrap source policy flag repeated')
    if not flags:
        return None
    need('--private-probe-profile=source-paid-v1' in argv,
        'paid bootstrap source policy requires the explicit common source-paid profile')
    path = flags[0][len(PAID_BOOTSTRAP_POLICY_FLAG):]
    need(Path(path).is_absolute() and '{' not in path and '\x00' not in path,
        'paid bootstrap source policy must be an absolute frozen operator input')
    return path


def generation_config(argv):
    flags = [(prefix, arg[len(prefix):]) for arg in argv for prefix in GENERATION_FLAGS if arg.startswith(prefix)]
    need(len(flags) <= 1, 'material generation worker versions/config flags are mutually exclusive')
    if not flags:
        return None
    prefix, path = flags[0]
    need(Path(path).is_absolute() and '{' not in path and '\x00' not in path,
        'material generation config must be an explicit absolute frozen input')
    profiles = [arg for arg in argv if arg.startswith('--private-probe-profile=')]
    need(len(profiles) == 1 and profiles[0] in ('--private-probe-profile=paid-v4', '--private-probe-profile=source-paid-v1'),
        'material generation needs an explicit paid capability profile')
    if prefix == GENERATION_FLAGS[1]:
        need(paid_bootstrap_source_policy(argv) is not None,
            'V7 generation requires common source-paid profile and explicit V6 policy')
        source_policy(argv)
    return path


def directory(drill, role, name=DIRECTORY):
    need(name in (DIRECTORY, BOOTSTRAP_DIRECTORY), 'unknown paid observation directory')
    path = Path(drill.values[role + '_APPDIR']) / 'misaka-testnet-12' / name
    need(path.is_absolute() and not any(part.is_symlink() for part in (path,) + tuple(path.parents)),
        'paid observation path traverses a symlink or escapes the owned appdir')
    return path


def invalidate(drill, role):
    """Before Popen, remove only this opt-in role's exact stale observation files.

    No WAL or native state is modified. A failed restart cannot re-use a previous
    process's positive file, even when sink/DAA/import happen to be unchanged.
    """
    values = claims(drill.plan['nodes'][role]['argv'])
    if not values:
        return
    need(role not in drill.processes, 'cannot invalidate observations of an existing owned child')
    names = [DIRECTORY]
    if paid_bootstrap_source_policy(drill.plan['nodes'][role]['argv']) is not None:
        names.append(BOOTSTRAP_DIRECTORY)
    for name in names:
        parent = directory(drill, role, name)
        if not parent.exists():
            continue
        need(parent.is_dir(), 'paid observation parent is not a directory')
        for value in values:
            for suffix in ('.json', '.pending'):
                path = parent / (value + suffix)
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    continue
                need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1,
                    'stale paid observation is not an owned ordinary file')
                path.unlink()
        descriptor = os.open(parent, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def read_owned(path):
    need(not any(part.is_symlink() for part in (path,) + tuple(path.parents)), 'paid observation file traverses a symlink')
    descriptor = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    with os.fdopen(descriptor, 'rb') as source:
        info = os.fstat(source.fileno())
        need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1,
            'paid observation is not an owned ordinary file')
        need(0 < info.st_size <= MAX_BYTES, 'paid observation file exceeds bounded framing')
        raw = source.read(MAX_BYTES + 1)
        after = os.fstat(source.fileno())
    current = path.lstat()
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns)
    need(identity(info) == identity(after) == identity(current) and len(raw) == info.st_size,
        'paid observation changed during its bounded read')
    def unique(pairs):
        value = {}
        for key, item in pairs:
            need(key not in value, 'paid observation repeats a JSON field')
            value[key] = item
        return value
    def constant(value):
        raise Unavailable('nonfinite paid observation number: ' + value)
    envelope = json.loads(raw, object_pairs_hook=unique,
        parse_constant=constant)
    return envelope, dict(device=info.st_dev, inode=info.st_ino, bytes=info.st_size, mtimeNs=info.st_mtime_ns)


def validate(envelope, claim, dag, node, import_stamp, params, genesis):
    need(isinstance(envelope, dict) and type(envelope.get('schemaVersion')) is int and envelope['schemaVersion'] == 5,
        'paid observation schema differs')
    need(envelope.get('kind') == 'rfc08-private-native-paid-claim-observation-v5', 'paid observation kind differs')
    need(envelope.get('baseClaim') == hash64(claim), 'paid observation names another exact claim')
    need(envelope.get('consensusParamsId') == node['consensusParamsId'] == params and
        envelope.get('genesis') == node['genesisHash'] == genesis, 'paid observation network/Params differs')
    need(envelope.get('ownProcessObservationOnly') is True and envelope.get('releaseApproval') is False and
        envelope.get('securitySoundnessMeasured') is False, 'private paid observation advertises unsupported authority')
    if envelope.get('available') is False:
        need(envelope.get('observation') is None, 'unavailable paid observation contains a positive payload')
        return None
    need(envelope.get('available') is True and envelope.get('error') is None, 'paid observation is unavailable')
    value = envelope.get('observation')
    need(isinstance(value, dict) and type(value.get('version')) is int and value['version'] == 5,
        'paid observation payload version differs')
    need(set(value) == {'version', 'snapshot', 'baseClaim', 'authorityPresent', 'enrollment', 'context', 'checkerSnapshot',
        'expectedRounds', 'checkedRounds', 'lastLock', 'nativeConsumerComplete', 'nativeAssignedQuorum',
        'nativeSelectedApproval', 'sourceProvenanceRevalidated', 'historicalSignatureReplay', 'finalProjection',
        'observationLimitReached', 'sourceContextsObserved', 'sourcePromisesObserved', 'escrowHeldSompi',
        'escrowPaidSompi', 'escrowRefundedSompi', 'latestMatchingNativeDrain', 'coinbaseOutputReplayed',
        'oneTimeFeeSettlement', 'releaseApproval', 'securitySoundnessMeasured'}, 'paid observation field contract differs')
    need(value.get('baseClaim') == claim and value.get('releaseApproval') is False and
        value.get('securitySoundnessMeasured') is False, 'paid observation payload identity/authority differs')
    snapshot = value['snapshot']
    need(isinstance(snapshot, dict) and set(snapshot) == {'checkpoint', 'statePoint', 'stateRoot', 'daa', 'importEpoch'},
        'paid observation snapshot schema differs')
    need(hash64(snapshot['checkpoint']) == hash64(snapshot['statePoint']) == dag['sink'],
        'paid observation is not the current own selected-parent state')
    need(hash64(snapshot['stateRoot']) != '0' * 128, 'paid observation committed root absent')
    need(counter(snapshot['daa']) == counter(dag['virtualDaaScore']), 'paid observation DAA differs')
    epoch, point, phase, stable = import_stamp(node)
    need(stable is True and point == '' and phase in ('idle', 'legacy-stable') and
        counter(snapshot['importEpoch']) == counter(epoch), 'paid observation import is incomplete or changed')
    for key in ('authorityPresent', 'observationLimitReached'):
        need(type(value.get(key)) is bool, 'paid observation boolean differs: ' + key)
    need(value.get('nativeConsumerComplete') is None or type(value['nativeConsumerComplete']) is bool,
        'paid native completeness is not a boolean or unavailable')
    for key in ('enrollment', 'context', 'checkerSnapshot'):
        if value.get(key) is not None:
            hash64(value[key])
    for key in ('expectedRounds', 'checkedRounds', 'sourceContextsObserved', 'sourcePromisesObserved'):
        counter(value[key], 32)
    need(value['checkedRounds'] <= value['expectedRounds'] and value['sourceContextsObserved'] <= 64 and
        value['sourcePromisesObserved'] <= 4096, 'paid observation bounded row counts differ')
    for key in ('escrowHeldSompi', 'escrowPaidSompi', 'escrowRefundedSompi'):
        counter(value[key], 128)
    for key in UNRESOLVED + ('nativeAssignedQuorum', 'finalProjection'):
        need(value.get(key) in ('UNKNOWN', 'OBSERVED', 'INCOMPLETE'), 'paid observation fact enum differs')
    if value['lastLock'] is not None:
        lock = value['lastLock']
        need(isinstance(lock, dict) and set(lock) == {'round', 'context', 'anchor', 'sampleSeed', 'assignment',
            'lockedDaa', 'seats', 'quorum', 'distinctCompleteReceipts', 'sourceValidNativeFlag', 'assignmentRecomputed'},
            'paid observation lock schema differs')
        for key in ('context', 'anchor', 'sampleSeed', 'assignment'):
            hash64(lock[key])
        for key in ('round', 'seats', 'quorum', 'distinctCompleteReceipts'):
            counter(lock[key], 32)
        need(counter(lock['lockedDaa']) <= snapshot['daa'], 'paid observation lock is from a future DAA')
        for key in ('sourceValidNativeFlag', 'assignmentRecomputed'):
            need(type(lock[key]) is bool, 'paid observation lock boolean differs')
    if value.get('latestMatchingNativeDrain') is not None:
        drain = value['latestMatchingNativeDrain']
        need(isinstance(drain, dict) and set(drain) == {'sourceClaim', 'sourceContext', 'recipient', 'entitlement', 'income',
            'queue', 'amount', 'coinbaseQueueAmount', 'paidBlock', 'paidDaa', 'materialReceipt'}, 'paid latest drain schema differs')
        for key in ('sourceClaim', 'sourceContext', 'entitlement', 'income', 'queue', 'paidBlock', 'materialReceipt'):
            hash64(drain[key])
        for key in ('amount', 'coinbaseQueueAmount', 'paidDaa'):
            counter(drain[key])
        need(drain['paidDaa'] <= snapshot['daa'], 'paid latest drain is from a future DAA')
        recipient = drain['recipient']
        need(isinstance(recipient, dict) and set(recipient) == {'transactionId', 'index'}, 'paid recipient schema differs')
        hash64(recipient['transactionId'])
        counter(recipient['index'], 32)
    # Even a hand-written OBSERVED in these fields cannot extend this reviewed
    # version's capabilities. Only the own structural facts below are consumed.
    return value


def observe(drill, role, claim, import_stamp, probe_context, params, genesis, *, directory_fn=directory, validate_fn=validate):
    need(claim in claims(drill.plan['nodes'][role]['argv']), 'exact claim not enabled in this role\'s frozen paid observation flag')
    process = drill.processes.get(role)
    need(process is not None and process.poll() is None, 'paid observation requires the current owned live child')
    need(getattr(drill, 'paid_observation_processes', {}).get(role) is process,
        'paid observation has no driver-recorded invalidated launch')
    path = directory_fn(drill, role) / (claim + '.json')
    rpc = drill.rpc(role)
    try:
        before, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        envelope, file_identity = read_owned(path)
        value = validate_fn(envelope, claim, before, node, import_stamp, params, genesis)
        after, after_node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        need(probe_context(before, node) == probe_context(after, after_node), 'paid observation crossed sink/DAA/import/Params')
        # An atomic publication during RPC bracketing must retain the exact own
        # committed root at the same state point. Never compare sink.header's
        # root: that header commits its parent's post-state, not this state.
        end_envelope, end_file = read_owned(path)
        end_value = validate_fn(end_envelope, claim, after, after_node, import_stamp, params, genesis)
        need(value == end_value, 'paid observation changed its root or facts during the bracket')
        need(drill.processes.get(role) is process and process.poll() is None and
            drill.paid_observation_processes.get(role) is process, 'paid observation crossed a process exit/restart')
    finally:
        rpc.close()
    return dict(available=value is not None, observation=value, role=role, claimId=claim, pid=process.pid,
        file=file_identity, endFile=end_file, currentOwnApiSnapshotObserved=value is not None,
        independentlyVerifiedCommittedRoot=False, remoteCertificate=False, releaseApproval=False,
        selectedVerifierApproval='UNKNOWN', sourceProvenance='UNKNOWN', historicalSignatures='UNKNOWN',
        coinbaseOutputReplay='UNKNOWN', oneTimeFeeSettlement='UNKNOWN')


def fact(requirement, observed):
    result = dict(id=requirement['id'], kind=requirement['kind'], role=requirement['role'], claimId=requirement['claimId'],
        status='UNKNOWN', nativeAuthorityEstablished=False, nativePaidObservation=observed,
        selectedVerifierApproval='UNKNOWN', oneTimeFeeSettlement='UNKNOWN', releaseApproval=False,
        securitySoundnessMeasured=False,
        reason='v5 structural native rows do not establish selected approval/source lineage/historical signatures/all payments')
    if observed is None or not observed['available']:
        result['reason'] = 'current own paid observation is not available'
        return result
    value = observed['observation']
    for key, expected in requirement['expected'].items():
        need(value.get(key) == expected, 'paid observation differs from frozen expected ' + key)
    if requirement['kind'] == 'native-paid-observed' and value['authorityPresent'] and all(
            value.get(key) is not None for key in ('enrollment', 'context', 'checkerSnapshot')):
        result.update(status='PASS', reason='exact own native authority identity observed; approval and settlement remain UNKNOWN')
    if requirement['kind'] == 'native-paid-assigned-quorum' and value['authorityPresent'] and not value['observationLimitReached'] and \
            value['nativeAssignedQuorum'] == 'OBSERVED' and value['nativeConsumerComplete'] is True and \
            0 < value['expectedRounds'] == value['checkedRounds'] <= 64:
        lock = value.get('lastLock')
        need(isinstance(lock, dict) and lock.get('assignmentRecomputed') is True, 'native quorum assignment not recomputed')
        seats, quorum = counter(lock['seats'], 32), counter(lock['quorum'], 32)
        receipts = counter(lock['distinctCompleteReceipts'], 32)
        need(0 < quorum <= receipts <= seats, 'native quorum distinct receipt bounds differ')
        result.update(status='PASS', reason='own assigned operator/receipt structure observed; full selected approval remains UNKNOWN')
    return result
