"""Opt-in wiring of real private kernel producer/checker processes.

The node re-executes the exported TIR work and checks its own native authority.
Neither an exporter manifest, a WAL file, a Final lifecycle string nor this
driver's observations establish selected-verifier approval or settled fees.
Limits and fixture commands are supplied before launching children; the owned
bootstrap exports actual native-snapshot inputs before worker activation. No
fake work, implicit payer, state copying, model review or clock injection occurs.
"""
import hashlib
import json
from pathlib import Path
import stat

MAX_SPEC_BYTES = 1024 * 1024
# Same finite request envelope as the private native-work exporter. This is a
# driver input cap, not a whole-model/RSS or production resource approval.
MAX_WORK_INPUT_BYTES = 64 * 1024 * 1024
WORK_FLAG = '--private-probe-kernel-paid-work='
CHECK_FLAG = '--private-probe-kernel-paid-check='
BOND_DOMAIN = b'misaka-palw/kernel-route/bond-id/v1'
NETWORK_DOMAIN = b'misaka-palw/attempt-v2/network-domain/v1'
BOOTSTRAP_SOURCE_POLICY_FLAG = '--private-probe-paid-bootstrap-source-policy'
BOOTSTRAP_SOURCE_POLICY_BYTES = 182  # Borsh (u64 activation, fixed PolicyV6).


def bootstrap_source_policy_id(path):
    path = regular(path, BOOTSTRAP_SOURCE_POLICY_BYTES)
    with path.open('rb') as source:
        raw = source.read(BOOTSTRAP_SOURCE_POLICY_BYTES + 1)
    if len(raw) != BOOTSTRAP_SOURCE_POLICY_BYTES or raw[8:10] != b'\x06\x00' or int.from_bytes(raw[:8], 'little') == 0:
        raise ValueError('V6 source policy needs its exact fixed canonical tuple and explicit nonzero activation')
    policy = raw[8:]
    # Identity only. The native factory validates every value; this driver
    # neither selects nor approves any number, stake assumption or fee.
    return hashlib.blake2b(len(policy).to_bytes(8, 'little') + policy,
        key=b'MISAKA/PALW/PAID-BOOTSTRAP-SOURCE-POLICY/V6', digest_size=64).hexdigest()


def exact(value, name, maximum=2**64 - 1, minimum=1):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError(name + ' must be an exact bounded unsigned integer')
    return value


def hex_text(value, name, sizes=(128,)):
    if not isinstance(value, str) or len(value) not in sizes or any(c not in '0123456789abcdef' for c in value):
        raise ValueError(name + ' must be canonical lower-case hex')
    return value


def outpoint(value):
    if not isinstance(value, str) or value.count(':') != 1:
        raise ValueError('native outpoint must be canonical Hash64:u32')
    txid, index = value.split(':')
    hex_text(txid, 'outpoint hash')
    if not index or not index.isascii() or not index.isdecimal() or str(int(index)) != index or int(index) >= 2**32:
        raise ValueError('native outpoint index must be canonical u32')
    return value


def kernel_bond(value):
    txid, index = outpoint(value).split(':')
    return hashlib.blake2b(bytes.fromhex(txid) + int(index).to_bytes(4, 'little'), key=BOND_DOMAIN, digest_size=64).hexdigest()


def network_domain(genesis):
    # This private helper permits testnet-12 only. Match the native signature
    # domain's network-name length, exact name, and chain incarnation hash.
    network = b'testnet-12'
    return hashlib.blake2b(len(network).to_bytes(8, 'little') + network + bytes.fromhex(hex_text(genesis, 'genesis')),
        key=NETWORK_DOMAIN, digest_size=64).hexdigest()


def regular(value, maximum, private=False):
    path = Path(value)
    if not path.is_absolute() or any(parent.is_symlink() for parent in (path,) + tuple(path.parents)):
        raise ValueError('kernel consumer input must be an absolute non-symlink path')
    metadata = path.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > maximum:
        raise ValueError('kernel consumer input exceeds its regular-file bound')
    if private and metadata.st_mode & 0o077:
        raise ValueError('kernel consumer key must have private file permissions')
    return path


def read_json(value):
    path = regular(value, MAX_SPEC_BYTES)
    with path.open('rb') as stream:
        raw = stream.read(MAX_SPEC_BYTES + 1)
    if len(raw) > MAX_SPEC_BYTES:
        raise ValueError('kernel consumer JSON changed beyond its bound')
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def file_digest(value, maximum):
    path = regular(value, maximum)
    digest, count = hashlib.sha256(), 0
    with path.open('rb') as stream:
        while chunk := stream.read(64 * 1024):
            count += len(chunk)
            if count > maximum:
                raise ValueError('kernel consumer input changed beyond its bound')
            digest.update(chunk)
    return digest.hexdigest()


def flag(argv, name):
    values = [value[len(name) + 1:] for value in argv if value.startswith(name + '=')]
    if len(values) != 1 or not values[0]:
        raise ValueError('kernel consumer node needs one explicit ' + name)
    return values[0]


def funding(spec, used):
    points = spec.get('funding_outpoints')
    if not isinstance(points, list) or not 1 <= len(points) <= 64:
        raise ValueError('kernel consumer needs one to 64 explicit funding inputs')
    for point in points:
        outpoint(point)
        if point in used:
            raise ValueError('kernel consumer funding may not be reused across workers')
        used.add(point)
    fee = exact(spec.get('maximum_fee_sompi'), 'worker maximum_fee_sompi')
    regular(spec.get('payer_key'), 256, private=True)
    return fee * len(points)


def validate_request(plan):
    """Freeze schema and exposure limits before bootstrap starts any child."""
    supplied = plan.get('nativeKernelConsumers')
    if supplied is None:
        return None
    required = {'version', 'producer', 'checkers', 'claim', 'kernelClass', 'networkDomain', 'genesis',
        'consensusParamsId', 'maximumDirectCarrierExposureSompi', 'maximumPublicRecordBytes'}
    if not isinstance(supplied, dict) or set(supplied) != required or type(supplied['version']) is not int or supplied['version'] != 4:
        raise ValueError('nativeKernelConsumers needs its exact explicit version-4 schema')
    for name in ('claim', 'kernelClass', 'networkDomain', 'genesis'):
        hex_text(supplied[name], name)
    if supplied['networkDomain'] != network_domain(supplied['genesis']):
        raise ValueError('kernel network signature domain differs from the pinned chain incarnation')
    hex_text(supplied['consensusParamsId'], 'consensusParamsId', (64, 128))
    if plan.get('ruleset_id') is not None and plan['ruleset_id'].removeprefix('0x') != supplied['consensusParamsId']:
        raise ValueError('kernel work ruleset differs from the pinned scenario ruleset')
    maximum = exact(supplied['maximumDirectCarrierExposureSompi'], 'maximumDirectCarrierExposureSompi')
    exact(supplied['maximumPublicRecordBytes'], 'maximumPublicRecordBytes', 64 * 1024 * 1024)
    checks = supplied['checkers']
    if not isinstance(checks, list) or not 1 <= len(checks) <= 2:
        raise ValueError('one to two separate checker processes must be explicitly configured')
    producer = supplied['producer']
    if not isinstance(producer, dict) or set(producer) != {'role', 'specification', 'workManifest'}:
        raise ValueError('kernel producer needs explicit role, specification and exporter workManifest')
    if producer['role'] == 'JOIN':
        raise ValueError('kernel producer must run before the later fresh JOIN import')
    roles = set()
    for entry, is_producer in [(producer, True)] + [(row, False) for row in checks]:
        if not isinstance(entry, dict) or (not is_producer and set(entry) != {'role', 'specification'}):
            raise ValueError('kernel checker needs an explicit role and specification')
        if entry['role'] not in plan['nodes'] or entry['role'] in roles:
            raise ValueError('kernel workers must run in distinct known processes')
        roles.add(entry['role'])
        path_fields = ('specification', 'workManifest') if is_producer else ('specification',)
        for key in path_fields:
            if not isinstance(entry[key], str) or not entry[key]:
                raise ValueError('kernel consumer input paths must be explicit strings')
    return supplied


def prepare(plan, values=None):
    """Attach real workers AFTER owned bootstrap exported their native inputs.

    Schema/exposure limits are predeclared before any process starts. Actual
    work/spec files may be generated by the bounded bootstrap fixture, which
    reads its own native snapshot. Validate/fingerprint these files before
    enabling workers through a same-DB child restart. No state is injected.
    """
    original = validate_request(plan)
    if original is None:
        return None
    supplied = dict(original)
    def resolved(entry):
        copy = dict(entry)
        for key in ('specification', 'workManifest'):
            if key in copy:
                copy[key] = copy[key].format_map(values or {})
        return copy
    supplied['producer'] = resolved(original['producer'])
    supplied['checkers'] = [resolved(row) for row in original['checkers']]
    producer, checks = supplied['producer'], supplied['checkers']
    maximum = supplied['maximumDirectCarrierExposureSompi']
    used, native_bonds, worker_roles, actions, specs = set(), set(), set(), [], []
    for node in plan['nodes'].values():
        for argument in node['argv']:
            if argument.startswith('--private-probe-recovery-ack-funding='):
                for value in argument.split('=', 1)[1].split(','):
                    used.add(outpoint(value))
            if argument.startswith('--private-probe-native-provider-keys='):
                for value in argument.split('=', 1)[1].split(','):
                    native_bonds.add(outpoint(value.rsplit('@', 1)[-1]))
    expected_work, exporter = None, None
    exposure = 0
    # Validate the complete configuration before mutating any node argv.
    for entry, is_producer in [(producer, True)] + [(row, False) for row in checks]:
        if not isinstance(entry, dict) or (not is_producer and set(entry) != {'role', 'specification'}):
            raise ValueError('kernel checker needs an explicit role and specification')
        role = entry['role']
        if role not in plan['nodes'] or role in worker_roles:
            raise ValueError('kernel workers must run in distinct known processes')
        worker_roles.add(role)
        argv = plan['nodes'][role]['argv']
        if flag(argv, '--private-probe-profile') not in ('paid-v4', 'source-paid-v1') or any(value.startswith((WORK_FLAG, CHECK_FLAG)) for value in argv):
            raise ValueError('kernel workers need the paid-v4 private factory without preexisting consumer flags')
        spec, digest = read_json(entry['specification'])
        common = {'version', 'work_path', 'payer_key', 'funding_outpoints', 'maximum_fee_sompi', 'max_input_bytes', 'max_workspace_bytes'}
        required_spec = common | ({'checkpoint', 'paid_class', 'paid_job', 'salt', 'commit_daa', 'expires_daa',
            'max_material_bytes', 'max_units'} if is_producer else {'enrollment', 'round', 'checker_bond', 'checker_key',
            'provider_bond', 'provider_address', 'max_unit_bytes', 'max_total_fetch_bytes', 'max_requests'})
        optional_spec = {'source_lock_funding_outpoint', 'maximum_source_lock_fee_sompi'} if is_producer else {'compact_record_path', 'challenge_valid_until_daa'}
        if not isinstance(spec, dict) or not required_spec <= set(spec) or set(spec) - required_spec - optional_spec:
            raise ValueError('kernel specification differs from the actual node JSON schema')
        if type(spec.get('version')) is not int or spec['version'] != 4:
            raise ValueError('kernel node specification version must be integer four')
        exposure += funding(spec, used)
        exact(spec.get('max_input_bytes'), 'max_input_bytes', MAX_WORK_INPUT_BYTES)
        exact(spec.get('max_workspace_bytes'), 'max_workspace_bytes')
        work = regular(spec.get('work_path'), spec['max_input_bytes'])
        files = [dict(path=entry['specification'], maximumBytes=MAX_SPEC_BYTES, sha256=digest),
            dict(path=str(work), maximumBytes=spec['max_input_bytes'], sha256=file_digest(work, spec['max_input_bytes']))]
        if is_producer:
            if '--palw-produce' not in argv:
                raise ValueError('kernel paid producer needs its actual native producer context')
            exchange = flag(argv, '--private-probe-native-exchange')
            hosts = [(host_role, node) for host_role, node in plan['nodes'].items()
                if '--private-probe-native-provider-listen=' + exchange in node['argv']]
            if len(hosts) != 1 or hosts[0][0] == 'JOIN':
                raise ValueError('producer upload needs a live initial owned provider, not the later fresh JOIN')
            hosted = {outpoint(value.rsplit('@', 1)[-1]) for argument in hosts[0][1]['argv']
                if argument.startswith('--private-probe-native-provider-keys=') for value in argument.split('=', 1)[1].split(',')}
            roster = [outpoint(value) for value in flag(argv, '--private-probe-native-recovery-providers').split(',')]
            order = lambda value: (bytes.fromhex(value.split(':')[0]), int(value.split(':')[1]))
            if not 2 <= len(roster) <= 8 or len(set(roster)) != len(roster) or roster != sorted(roster, key=order) or not set(roster) <= hosted:
                raise ValueError('producer needs the exact sorted 2..8 roster hosted by its native exchange')
            native_bond = outpoint(flag(argv, '--palw-producer-bond'))
            native_bonds.add(native_bond)
            expected_work = work
            exporter, export_digest = read_json(entry['workManifest'])
            files.append(dict(path=entry['workManifest'], maximumBytes=MAX_SPEC_BYTES, sha256=export_digest))
            if exporter.get('schema') not in ('misaka.private-paid-native-work.v4', 'misaka.private-paid-native-work.v6') or exporter.get('realTirExecution') is not True:
                raise ValueError('kernel producer needs the real TIR exporter manifest')
            v6 = exporter['schema'] == 'misaka.private-paid-native-work.v6'
            explicit_v6 = [value for value in argv if value.startswith(BOOTSTRAP_SOURCE_POLICY_FLAG + '=')]
            if v6:
                policy_path = flag(argv, BOOTSTRAP_SOURCE_POLICY_FLAG)
                if flag(argv, '--private-probe-profile') != 'source-paid-v1' or exporter.get('sourceNamespace') != 'approved-bootstrap-v6':
                    raise ValueError('V6 export needs the exact explicit source-paid native namespace')
                for node in plan['nodes'].values():
                    if flag(node['argv'], '--private-probe-profile') != 'source-paid-v1' or flag(node['argv'], BOOTSTRAP_SOURCE_POLICY_FLAG) != policy_path:
                        raise ValueError('every V6 role including JOIN needs the same explicit policy bytes')
                if hex_text(exporter.get('sourcePolicyId'), 'V6 exported policy') != bootstrap_source_policy_id(policy_path):
                    raise ValueError('V6 export is bound to another exact source policy')
                files.append(dict(path=policy_path, maximumBytes=BOOTSTRAP_SOURCE_POLICY_BYTES,
                    sha256=file_digest(policy_path, BOOTSTRAP_SOURCE_POLICY_BYTES)))
                if exporter.get('signedEnrollment') != 'signed-paid-enrollment-v6.borsh':
                    raise ValueError('V6 exporter must name its dedicated signed enrollment artifact')
                signed = Path(entry['workManifest']).parent / exporter['signedEnrollment']
                files.append(dict(path=str(signed), maximumBytes=MAX_WORK_INPUT_BYTES,
                    sha256=file_digest(signed, MAX_WORK_INPUT_BYTES)))
                point = outpoint(spec.get('source_lock_funding_outpoint'))
                if point in used:
                    raise ValueError('V6 source lock cannot reuse any declared worker/ACK funding input')
                used.add(point)
                exposure += exact(spec.get('maximum_source_lock_fee_sompi'), 'V6 maximum_source_lock_fee_sompi')
            elif explicit_v6 or any(spec.get(key) is not None for key in ('source_lock_funding_outpoint', 'maximum_source_lock_fee_sompi')):
                raise ValueError('legacy V4 export cannot silently use V6 source policy or funding')
            # Provenance/identity only: the actual node validates the bytes and
            # own native authority. No exporter assertion becomes acceptance.
            if exporter.get('work') != work.name or Path(entry['workManifest']).parent != work.parent:
                raise ValueError('exporter manifest must name the exact adjacent work input')
            if exporter.get('producer') != kernel_bond(native_bond):
                raise ValueError('exported work belongs to another native producer')
            for key, field in (('paid_class', 'paidClass'), ('paid_job', 'paidJob')):
                if hex_text(spec.get(key), key) != hex_text(exporter.get(field), field):
                    raise ValueError('producer paid scope differs from exporter manifest')
            hex_text(exporter.get('baseJob'), 'baseJob')
            for key in ('checkpoint', 'salt'):
                hex_text(spec.get(key), key)
            for key in ('max_material_bytes', 'max_units', 'expires_daa'):
                exact(spec.get(key), key, 2**32 - 1 if key == 'max_units' else 2**64 - 1)
            exact(spec.get('commit_daa'), 'commit_daa', minimum=0)
            if spec['commit_daa'] > spec['expires_daa']:
                raise ValueError('kernel commit DAA exceeds its signed expiry')
            actions.append((role, WORK_FLAG + str(Path(entry['specification']))))
            specs.append(dict(role=role, kind='producer', specificationSha256=digest, workManifestSha256=export_digest,
                nativeProducer=native_bond, kernelProducer=exporter['producer'], baseJob=exporter['baseJob'],
                checkpoint=spec['checkpoint'], providerHost=hosts[0][0], inputs=files))
        else:
            if work != expected_work:
                raise ValueError('checker must re-execute the exact producer work input')
            checker, provider = outpoint(spec.get('checker_bond')), outpoint(spec.get('provider_bond'))
            native_bonds.update((checker, provider))
            if checker == provider:
                raise ValueError('checker and provider must have distinct native identities')
            regular(spec.get('checker_key'), 256, private=True)
            exact(spec.get('round'), 'checker round', 2**32 - 1, 0)
            hex_text(spec.get('enrollment'), 'checker enrollment')
            address = spec.get('provider_address')
            if not isinstance(address, str) or not address.startswith('127.0.0.1:'):
                raise ValueError('checker query endpoint must be an explicit loopback provider')
            port = address.removeprefix('127.0.0.1:')
            if not port.isascii() or not port.isdecimal() or str(int(port)) != port or not 0 < int(port) < 65536:
                raise ValueError('provider query port must be canonical and finite')
            # Require a real independently owned host for the query socket;
            # the driver never accepts a mock result or launches a fake server.
            hosts = [node for node in plan['nodes'].values() if '--private-probe-native-provider-listen=' + address in node['argv']]
            hosted = [value.rsplit('@', 1)[-1] for node in hosts for argument in node['argv']
                if argument.startswith('--private-probe-native-provider-keys=') for value in argument.split('=', 1)[1].split(',')]
            if len(hosts) != 1 or provider not in hosted:
                raise ValueError('checker provider must be hosted by exactly one configured native service')
            for key in ('max_unit_bytes', 'max_total_fetch_bytes', 'max_requests'):
                exact(spec.get(key), key, 2**32 - 1 if key == 'max_requests' else 2**64 - 1)
            if spec.get('compact_record_path') is not None:
                regular(spec['compact_record_path'], spec['max_input_bytes'])
                files.append(dict(path=spec['compact_record_path'], maximumBytes=spec['max_input_bytes'],
                    sha256=file_digest(spec['compact_record_path'], spec['max_input_bytes'])))
            if spec.get('challenge_valid_until_daa') is not None:
                exact(spec['challenge_valid_until_daa'], 'challenge_valid_until_daa')
            actions.append((role, CHECK_FLAG + str(Path(entry['specification']))))
            specs.append(dict(role=role, kind='checker', specificationSha256=digest, checker=checker, provider=provider,
                round=spec['round'], enrollment=spec['enrollment'], maximumInputBytes=spec['max_input_bytes'],
                actionSlots=len(spec['funding_outpoints']), inputs=files))
    if used & native_bonds:
        raise ValueError('kernel workers may not spend any configured native bond as a carrier input')
    if exposure > maximum or exposure >= 2**64:
        raise ValueError('kernel worker carrier exposure exceeds its predeclared exact total')
    for role, argument in actions:
        plan['nodes'][role]['argv'].append(argument)
    return dict(configuration=supplied, workers=specs, maximumDirectCarrierExposureSompi=maximum,
        directCarrierExposureUpperSompi=exposure, providerCarrierFeesMeasured=False,
        selectedVerifierApprovalStatus='UNKNOWN', securitySoundnessMeasured=False)


def revalidate(prepared, role):
    """Every cold relaunch keeps the original explicit bytes, never new work."""
    if prepared is None:
        return
    for worker in prepared['workers']:
        if worker['role'] != role:
            continue
        for item in worker['inputs']:
            if file_digest(item['path'], item['maximumBytes']) != item['sha256']:
                raise ValueError('kernel consumer input changed after predeclared launch configuration')


def bootstrap_checkpoint(drill, prepared, context):
    """Wait for the exact exported checkpoint in each initial OWN chain.

    A connected socket alone does not mean the provider imported the producer's
    pinned authority before its first upload. This diagnostic gate neither
    injects state nor replaces the node's own native ingress authentication.
    """
    config = prepared['configuration']
    producer = next(row for row in prepared['workers'] if row['kind'] == 'producer')
    rows = {}
    for role in ('A', 'B'):
        rpc = drill.rpc(role)
        try:
            before, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
            if node.get('genesisHash') != config['genesis'] or node.get('consensusParamsId') != config['consensusParamsId']:
                raise AssertionError('kernel bootstrap checkpoint came from another private network')
            if node.get('pruningImportStable') is not True or node.get('importSessionPoint') != '' \
                    or node.get('importSessionPhase') not in ('idle', 'legacy-stable'):
                return None
            stamp = context(before, node)
            block = rpc.call('getBlock', {'hash': producer['checkpoint'], 'includeTransactions': False})['block']
            verbose = block.get('verboseData') or {}
            if verbose.get('hash') != producer['checkpoint'] or verbose.get('isChainBlock') is not True:
                return None
            after, after_node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
            if context(after, after_node) != stamp:
                return None
            rows[role] = dict(checkpoint=producer['checkpoint'], importContext=stamp)
        except RuntimeError:
            # A header not yet imported is a pending prefix, never acceptance.
            return None
        finally:
            rpc.close()
    return rows


def observe(drill, role, prepared, context):
    """Bracket the exact actual claim with sink/network/import-generation reads."""
    config = prepared['configuration']
    rpc = drill.rpc(role)
    try:
        before, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        if node.get('genesisHash') != config['genesis'] or node.get('consensusParamsId') != config['consensusParamsId']:
            raise AssertionError('kernel work observation came from another private network')
        if node.get('pruningImportStable') is not True or node.get('importSessionPoint') != '' \
                or node.get('importSessionPhase') not in ('idle', 'legacy-stable'):
            raise AssertionError('kernel work observation is not a stable completed native import')
        stamp = context(before, node)
        claim = rpc.call('getPalwKernelClaim', {'claimId': config['claim']})
        after, after_node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        if context(after, after_node) != stamp:
            raise AssertionError('kernel work observation crossed a native generation/import epoch')
    finally:
        rpc.close()
    if claim.get('available') is not True or claim.get('found') is not True:
        return None
    producer = next(row for row in prepared['workers'] if row['kind'] == 'producer')
    if (claim.get('claimId'), claim.get('classId'), claim.get('jobId'), claim.get('producerBond')) != \
            (config['claim'], config['kernelClass'], producer['baseJob'], producer['kernelProducer']):
        raise AssertionError('native kernel claim differs from the explicit executed work scope')
    if exact(claim.get('tipDaa'), 'native claim DAA', minimum=0) != exact(before['virtualDaaScore'], 'native DAA', minimum=0):
        raise AssertionError('kernel claim RPC used another native DAA')
    records = {}
    for key in ('publicRecord', 'recordHeader'):
        encoded = claim.get(key)
        if not isinstance(encoded, str) or not encoded or len(encoded) % 2 or len(encoded) > 2 * config['maximumPublicRecordBytes'] \
                or any(c not in '0123456789abcdef' for c in encoded):
            raise AssertionError('native claim omitted its bounded actual verification record/header')
        digest = hashlib.sha256()
        for offset in range(0, len(encoded), 64 * 1024):
            digest.update(bytes.fromhex(encoded[offset:offset + 64 * 1024]))
        records[key] = dict(bytes=len(encoded) // 2, sha256=digest.hexdigest())
    if claim.get('convicted') is not False or claim.get('settlement') in ('convicted', 'defaulted'):
        raise AssertionError('configured actual kernel work was convicted/defaulted')
    for key in ('ledgerRoot', 'auxRoot'):
        hex_text(claim.get(key), key)
    # Retain fixed metadata/digests, not a deep public material copy for every
    # role/milestone. The bounded RPC record itself is released after this call.
    projection = {key: claim[key] for key in ('claimId', 'classId', 'jobId', 'producerBond', 'tipDaa', 'ledgerRoot', 'auxRoot')}
    for key in ('state', 'settlement'):
        value = claim.get(key)
        if not isinstance(value, str) or len(value) > 128:
            raise AssertionError('kernel claim lifecycle label exceeds its observation bound')
        projection[key] = value
    projection.update(records)
    # Public claim RPC reports the older Panel assignment/receipt count, not
    # typed paid-v2/v3 approval. In particular state=Final is insufficient.
    return dict(checkpoint=before['sink'], importContext=stamp, claim=projection,
        actualNativeClaimObserved=True, selectedVerifierApprovalStatus='UNKNOWN',
        settledFeeStatus='UNKNOWN', publicCertificate=False, releaseApproval=False)


def checker_wal(drill, worker):
    """Hash only the bounded named records of this exact owned checker job.

    Record presence is not native acceptance. Comparison after OS kill/reopen
    measures immutable file persistence only, independently of receipt approval.
    """
    data = Path(drill.values[worker['role'] + '_APPDIR']) / 'misaka-testnet-12'
    folder = data / 'kernel-paid-checker-v4' / kernel_bond(worker['checker']) / (worker['enrollment'] + '-' + str(worker['round']))
    paths = [('challenge-base.borsh', folder / 'challenge-base.borsh')]
    for slot in range(worker['actionSlots']):
        prefix = '' if slot == 0 else 'action-%02d/' % slot
        for name in ('receipt.borsh', 'transaction.borsh'):
            paths.append((prefix + name, folder / (prefix + name)))
    records = {}
    for name, path in paths:
        if not path.exists():
            continue
        regular(path, worker['maximumInputBytes'])
        digest, count = hashlib.sha256(), 0
        with path.open('rb') as stream:
            while chunk := stream.read(64 * 1024):
                count += len(chunk)
                if count > worker['maximumInputBytes']:
                    raise AssertionError('checker WAL changed beyond its explicit input bound')
                digest.update(chunk)
        records[name] = dict(bytes=count, sha256=digest.hexdigest())
    return records
