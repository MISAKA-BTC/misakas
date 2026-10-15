"""Owned private-child native observations; never public certificates or GC authority."""
import json

FILE = 'rfc08-native-recovery-observation-v4.json'
MAX_BYTES = 16 * 1024 * 1024


def hash64(value):
    assert isinstance(value, str) and len(value) == 128 and all(c in '0123456789abcdef' for c in value), 'noncanonical native Hash64'
    return value


def outpoint(value):
    assert isinstance(value, dict) and set(value) == {'transactionId', 'index'}, 'native outpoint shape differs'
    index = value['index']
    assert type(index) is int and 0 <= index < 2**32, 'native outpoint index is not u32'
    return hash64(value['transactionId']) + ':' + str(index)


def validate(envelope, dag, node, bootstrap, expected_providers, import_stamp, require_bound=True, require_finalized=False, expected_root=None, expected_claim=None):
    assert type(envelope.get('schemaVersion')) is int and envelope['schemaVersion'] == 4
    assert envelope.get('kind') == 'rfc08-private-native-recovery-observation-v4'
    assert envelope.get('releaseApproval') is False and envelope.get('securitySoundnessMeasured') is False
    assert envelope.get('consensusParamsId') == node['consensusParamsId'] == bootstrap['consensusParamsId']
    assert envelope.get('genesis') == node['genesisHash'] == bootstrap['genesis']
    if envelope.get('available') is False:
        return None
    assert envelope.get('available') is True and envelope.get('error') is None
    observation = envelope['observation']
    assert type(observation['version']) is int and observation['version'] == 4
    assert observation['releaseApproval'] is False and observation['securitySoundnessMeasured'] is False
    epoch, point, phase, stable = import_stamp(node)
    assert stable is True and point == '' and phase in ('idle', 'legacy-stable'), 'native import incomplete'
    assert type(observation['importEpoch']) is int and observation['importEpoch'] == epoch
    assert hash64(observation['checkpoint']) == dag['sink'], 'native observation names a stale branch/generation'
    assert type(observation['daa']) is int and 0 <= observation['daa'] < 2**64 and observation['daa'] == dag['virtualDaaScore']
    assert isinstance(observation['rows'], list)
    maximum = bootstrap['privateProbeProfile']['releaseRecoveryPolicy']['maxReservedContracts']
    assert type(maximum) is int and 0 < maximum < 2**32 and len(observation['rows']) <= maximum
    seen, candidates = set(), []
    expected = sorted(expected_providers, key=lambda value: (bytes.fromhex(value.split(':')[0]), int(value.split(':')[1])))
    for row in observation['rows']:
        root = hash64(row['root'])
        assert root not in seen, 'native observation repeats a contract'
        seen.add(root)
        hash64(row['consent'])
        hash64(row['recovery'])
        assert type(row['enrollmentFinalized']) is bool
        if row['enrollmentFinalized']:
            assert hash64(observation['finalizedCheckpoint']) != bootstrap['genesis'], 'enrollment finality still names genesis'
        assert type(row['acceptedDaa']) is int and 0 <= row['acceptedDaa'] <= observation['daa']
        assert type(row['enrollmentUntilDaa']) is int and 0 <= row['enrollmentUntilDaa'] < 2**64
        assert isinstance(row['providers'], list) and len(row['providers']) == 8
        providers = [outpoint(value) for value in row['providers'] if value is not None]
        assert row['providers'] == row['providers'][:len(providers)] + [None] * (8 - len(providers)), 'provider roster has holes'
        if row['class'] != bootstrap['class'] or row['artifactRoot'] != bootstrap['artifactRoot'] \
                or outpoint(row['executor']) != bootstrap['seats'][0]['bond'] or providers != expected:
            continue
        if expected_root is not None and root != expected_root:
            continue
        if row['boundClaim'] is not None:
            hash64(row['boundClaim'])
        if require_bound and row['boundClaim'] is None:
            continue
        if expected_claim is not None and row['boundClaim'] != expected_claim:
            continue
        if require_finalized:
            if not row['enrollmentFinalized']:
                continue
            assert hash64(observation['finalizedCheckpoint']) != bootstrap['genesis'], 'enrollment finality still names genesis'
        candidates.append(row)
    if not candidates:
        return None
    row = min(candidates, key=lambda value: value['root'])
    return dict(checkpoint=observation['checkpoint'], daa=observation['daa'], importEpoch=epoch,
        finalizedCheckpoint=observation['finalizedCheckpoint'], row=row,
        acceptedNativeRowObserved=True, nativeBindingObserved=row['boundClaim'] is not None,
        enrollmentFinalizedObserved=row['enrollmentFinalized'], publicCertificate=False, releaseApproval=False)


def observe(drill, role, bootstrap, rpc_read, import_stamp, require_bound=True, require_finalized=False, expected_root=None, expected_claim=None):
    # This exact file is written by this owned child's native Consensus API.
    # Bracket it with public reads so an old file after reorg/import/restart
    # cannot satisfy acceptance for the current native generation.
    path = drill.root / role / 'misaka-testnet-12' / FILE
    before = rpc_read(drill, role, 'getBlockDagInfo')
    node = rpc_read(drill, role, 'getPalwNodeStatus')
    try:
        with path.open('rb') as source:
            raw = source.read(MAX_BYTES + 1)
    except FileNotFoundError:
        return None
    assert len(raw) <= MAX_BYTES, 'owned native observation exceeds its bounded framing'
    envelope = json.loads(raw)
    result = validate(envelope, before, node, bootstrap, drill.plan['nativeCustodyProvider']['providerBonds'],
        import_stamp, require_bound, require_finalized, expected_root, expected_claim)
    after = rpc_read(drill, role, 'getBlockDagInfo')
    after_node = rpc_read(drill, role, 'getPalwNodeStatus')
    assert (before['sink'], before['pruningPointHash'], before['virtualDaaScore']) == \
        (after['sink'], after['pruningPointHash'], after['virtualDaaScore']), 'native observation changed generation'
    assert import_stamp(node) == import_stamp(after_node), 'native observation crossed an import epoch'
    return result


def agreement(drill, bootstrap, rpc_read, import_stamp, roles=('A', 'B', 'JOIN'), expected_root=None, expected_claim=None):
    rows = {role: observe(drill, role, bootstrap, rpc_read, import_stamp, expected_root=expected_root, expected_claim=expected_claim) for role in roles}
    if any(row is None for row in rows.values()):
        return None
    keys = [(row['checkpoint'], row['importEpoch'], row['row']['root'], row['row']['consent'],
        row['row']['recovery'], row['row']['boundClaim']) for row in rows.values()]
    # Import epochs are local counters, so their values need not agree across
    # nodes. Each observe already brackets its own exact epoch.
    identities = [(key[0],) + key[2:] for key in keys]
    return rows if len(set(identities)) == 1 else None
