"""Pinned RFC-0012 multi-process acceptance plan, schema version 1.

Every timeout, fee cap, scale floor, fixture command and expected identity is
fixed in one plan file before the first child process starts. A missing key or
an unknown key is a refusal, never a default. This module only parses: it does
not touch the filesystem, launch processes or decide whether a node is correct.

Values here are operator run inputs. The module invents no release threshold;
the only numeric floors it enforces are the ones the RFC-0012 drill already
fixes (100,000 accepted claims / population and the 5,400 DAA isolated span of
the drill-provisional maturity, user ruling 2026-10-11). Nothing parsed here is
an approval, and no verdict produced from it may be labelled PASS.
"""
import hashlib
import json
import re
import string

PLAN_KIND = 'rfc12-acceptance-plan'
PLAN_VERSION = 1
U32_MAX = 2**32 - 1
U64_MAX = 2**64 - 1
U128_MAX = 2**128 - 1

# Semantic plan role -> internal drill role. The internal names keep the existing
# bootstrap helpers (roles A, B, JOIN) usable unchanged.
ROLE_KEYS = {'producer': 'A', 'provider': 'B', 'checker': 'JOIN', 'fresh': 'FRESH', 'crashJoin': 'CRASHJOIN'}
ROLE_NAMES = {key: name for name, key in ROLE_KEYS.items()}
PLACEHOLDER_ROLES = {'producer': 'PRODUCER', 'provider': 'PROVIDER', 'checker': 'CHECKER', 'fresh': 'FRESH',
                     'crashJoin': 'CRASHJOIN'}
ENDPOINT_FIELDS = ('APPDIR', 'WRPC', 'GRPC', 'BORSH', 'ETH', 'P2P')
GLOBAL_PLACEHOLDERS = ('ROOT', 'SALT', 'FIXTURE', 'CLI', 'OLD_BLOCK', 'WORK_DIR', 'REQUEST_BLOCK')

# The kernel paid-work producer (--private-probe-kernel-paid-work) exists only on paid-v4.
ACCEPTANCE_PROFILES = ('paid-v4',)
OTHER_PRIVATE_PROFILES = ('native-v1', 'population-v1', 'complete-v1', 'paid-v3')
# A wrong profile that needs no extra private flag, so its refusal can only be the DB identity pin
# (population-v1 would first refuse for its missing --private-probe-bonds).
MISMATCH_PROFILES = ('native-v1', 'complete-v1', 'paid-v3')
# Drill floors already fixed by RFC-0012 records (not new thresholds).
MINIMUM_SCALE = 100_000
MINIMUM_ISOLATED_DAA_SPAN = 5400
# Seats 0..2 are the executor and the two providers, seat 3 is the dedicated ACK
# carrier payer (misaka-rfc12-private-bootstrap.native_provider_config_v4).
PROVIDER_PAYER_SEAT = 3
RESERVED_SEATS = (0, 1, 2, 3)
MAXIMUM_SEAT = 7

IMPORT_PHASES = ('sidecars', 'utxo-stream', 'awaiting-completion')
CLAIM_COUNTERS = ('capacityShadowLiveClaims', 'kernelFinalsTotal')
PARTICIPATION_SIGNALS = ('sink-follows', 'provider-store-grows', 'produced-blocks-increase')
# Phases at which the EVM/native consumer comparison may be taken.
CONSUMER_CHECKPOINTS = ('usefulWork', 'bridge', 'market', 'fork', 'partition', 'capacity', 'scale', 'prune',
                        'freshJoin', 'coldReopen', 'generationMismatch')
# Phases during which a crash point may be armed, and the trigger kinds each role can carry.
CRASH_PHASES = ('usefulWork', 'fork', 'partition', 'freshJoin', 'coldReopen')
IMPORT_WAL_STAGES = (
    'utxo-before-clear', 'utxo-after-clear', 'utxo-before-virtual-copy', 'utxo-after-virtual-clear',
    'utxo-mid-virtual-copy', 'utxo-after-virtual-copy', 'import-before-complete', 'import-after-complete',
)
TRIGGERS = {
    'import-wal': (('crashJoin',), ('freshJoin',)),
    'import-phase': (('crashJoin',), ('freshJoin',)),
    'kernel-outbox': (('producer',), ('usefulWork',)),
    'checker-outbox': (('producer', 'provider', 'checker'), ('usefulWork',)),
    'provider-store': (('provider',), ('usefulWork',)),
    'after-convergence': (('producer', 'provider', 'checker', 'fresh'), CRASH_PHASES),
}
OUTBOX_FILES = ('object.borsh', 'transaction.borsh')
CHECKER_OUTBOX_FILES = ('receipt.borsh', 'transaction.borsh')
CHECKER_HOSTS = ('checker', 'provider', 'producer')
CAPACITY_MODES = {'running-node': ('checker', 'provider'), 'import': ('crashJoin',)}
# Local capacity flags only: a constrained relaunch may not alter any consensus input.
CAPACITY_FLAGS = {
    '--palw-native-logical-admission-v1': False,
    '--palw-native-resource-admission-v1': False,
    '--palw-native-memory-budget': True,
    '--palw-native-database-budget': True,
    '--palw-resource-admission-v2': False,
    '--palw-host-memory-budget': True,
}
TOP_KEYS = ('schemaVersion', 'kind', 'commit', 'network', 'salt', 'binaries', 'identity', 'bounds',
            'observationBoundsFile', 'fees', 'scan', 'settlement', 'usefulWork', 'topology', 'scale', 'crash',
            'capacity', 'evm', 'bridge', 'market', 'accounting', 'reconciliation', 'coverage', 'dnsHistory')


class PlanError(ValueError):
    pass


def _keys(value, required, name):
    if not isinstance(value, dict):
        raise PlanError(name + ' must be an object')
    missing, unknown = set(required) - set(value), set(value) - set(required)
    if missing or unknown:
        raise PlanError('%s keys differ: missing %s, unknown %s' % (name, sorted(missing), sorted(unknown)))
    return value


def _int(value, name, minimum=0, maximum=U64_MAX):
    # bool is an int subclass in Python; a JSON true must not stand for 1.
    if type(value) is not int or not minimum <= value <= maximum:
        raise PlanError('%s must be an exact integer in [%d, %d]' % (name, minimum, maximum))
    return value


def _bool(value, name):
    if type(value) is not bool:
        raise PlanError(name + ' must be an explicit boolean')
    return value


def _hex(value, name, length, nonzero=False):
    if not isinstance(value, str) or len(value) != length or any(c not in '0123456789abcdef' for c in value):
        raise PlanError('%s must be %d lowercase hex characters' % (name, length))
    if nonzero and int(value, 16) == 0:
        raise PlanError(name + ' must be nonzero')
    return value


def _decimal_u128(value, name, positive=True):
    if not isinstance(value, str) or not value or any(c not in '0123456789' for c in value) \
            or (len(value) > 1 and value[0] == '0') or len(value) > 39 or int(value) > U128_MAX:
        raise PlanError(name + ' must be canonical u128 decimal text')
    if positive and int(value) == 0:
        raise PlanError(name + ' must be positive')
    return value


def _enum(value, name, choices):
    if value not in choices:
        raise PlanError('%s must be one of %s' % (name, ', '.join(map(str, choices))))
    return value


def _text(value, name, pattern=r'[A-Za-z0-9:._-]{1,256}'):
    if not isinstance(value, str) or not re.fullmatch(pattern, value):
        raise PlanError(name + ' must be a plain identifier')
    return value


def _absolute(value, name):
    if not isinstance(value, str) or not value.startswith('/') or '\0' in value or '/../' in value + '/':
        raise PlanError(name + ' must be an absolute path without parent traversal')
    return value


def _evm_address(value, name):
    if not isinstance(value, str) or not re.fullmatch(r'0x[0-9a-fA-F]{40}', value) or int(value[2:], 16) == 0:
        raise PlanError(name + ' must be a nonzero 0x-prefixed 20-byte EVM address')
    return value.lower()


def placeholders():
    names = set(GLOBAL_PLACEHOLDERS)
    for role in PLACEHOLDER_ROLES.values():
        names.update(role + '_' + field for field in ENDPOINT_FIELDS)
    return frozenset(names)


def _template(value, name, allowed):
    if not isinstance(value, str) or not value or '\0' in value:
        raise PlanError(name + ' must be a nonempty string')
    try:
        parsed = list(string.Formatter().parse(value))
    except ValueError as error:
        raise PlanError('%s is not a valid template: %s' % (name, error))
    for _, field, spec, conversion in parsed:
        if field is None:
            continue
        if field not in allowed or spec or conversion:
            raise PlanError('%s names an unknown or formatted placeholder {%s}' % (name, field))
    return value


def _argv(value, name, allowed):
    if not isinstance(value, list) or not value:
        raise PlanError(name + ' must be a nonempty argv list (no shell string)')
    return [_template(item, '%s[%d]' % (name, index), allowed) for index, item in enumerate(value)]


def _argv_list(value, name, allowed, minimum):
    if not isinstance(value, list) or len(value) < minimum:
        raise PlanError('%s must list at least %d argv command(s)' % (name, minimum))
    return [_argv(item, '%s[%d]' % (name, index), allowed) for index, item in enumerate(value)]


def _network(value):
    if not isinstance(value, dict):
        raise PlanError('network must be an object')
    kind = _keys(value, ('kind', 'profile' if value.get('kind') == 'private-profile' else 'name'), 'network')['kind']
    if kind == 'private-profile':
        return dict(kind=kind, profile=_enum(value['profile'], 'network.profile', ACCEPTANCE_PROFILES))
    if kind == 'preset':
        # Accepted only so the run can be recorded and judged BLOCKED: presets keep
        # palw_dns_retirement_v1 dormant, and this harness never alters a preset.
        return dict(kind=kind, name=_text(value['name'], 'network.name'))
    raise PlanError('network.kind must be private-profile or preset')


def _identity(value):
    _keys(value, ('genesis', 'consensusParamsId', 'retirementActivationDaa', 'retirementPolicyId'), 'identity')
    return dict(genesis=_hex(value['genesis'], 'identity.genesis', 128, True),
                consensusParamsId=_hex(value['consensusParamsId'], 'identity.consensusParamsId', 64, True),
                retirementActivationDaa=_int(value['retirementActivationDaa'], 'identity.retirementActivationDaa'),
                retirementPolicyId=_hex(value['retirementPolicyId'], 'identity.retirementPolicyId', 128, True))


def _bounds(value):
    fields = ('workflowTimeoutSeconds', 'phaseTimeoutSeconds', 'commandTimeoutSeconds', 'probeTimeoutSeconds',
              'factoryTimeoutSeconds')
    _keys(value, fields, 'bounds')
    out = {field: _int(value[field], 'bounds.' + field, 1) for field in fields}
    for field in fields[1:]:
        if out[field] > out['workflowTimeoutSeconds']:
            raise PlanError('bounds.%s cannot exceed the total workflow timeout' % field)
    if out['probeTimeoutSeconds'] > out['phaseTimeoutSeconds']:
        raise PlanError('bounds.probeTimeoutSeconds cannot exceed a phase timeout')
    return out


def _fees(value):
    positive = ('maximumWorkflowCarrierFeeSompi', 'maximumProviderAckFeePerCarrierSompi',
                'maximumProviderCarrierTotalSompi', 'maximumKernelClaimFeeSompi', 'maximumKernelCarrierTotalSompi',
                'maximumCheckerReceiptFeeSompi', 'maximumCheckerCarrierTotalSompi',
                'maximumDepositLockFeeSompi', 'maximumWorkflowTotalCostSompi')
    unsigned = ('maximumBindingReservationSompi', 'maximumCompleteCheckFeeSompi')
    _keys(value, positive + unsigned, 'fees')
    out = {field: _int(value[field], 'fees.' + field, 1) for field in positive}
    out.update({field: _int(value[field], 'fees.' + field) for field in unsigned})
    if out['maximumProviderAckFeePerCarrierSompi'] > out['maximumProviderCarrierTotalSompi']:
        raise PlanError('one provider carrier cannot exceed the provider carrier total budget')
    if out['maximumKernelClaimFeeSompi'] > out['maximumKernelCarrierTotalSompi']:
        raise PlanError('one kernel claim carrier cannot exceed the kernel carrier total budget')
    if out['maximumCheckerReceiptFeeSompi'] > out['maximumCheckerCarrierTotalSompi']:
        raise PlanError('one checker receipt carrier cannot exceed the checker carrier total budget')
    return out


def _scan(value):
    fields = ('maximumScannedChainBlocks', 'maximumMergesetBlockReads', 'maximumScannedEvmBlocks',
              'maximumUtxoRowsPerAddress', 'kernelFinalsLimit')
    _keys(value, fields, 'scan')
    out = {field: _int(value[field], 'scan.' + field, 1) for field in fields}
    _int(out['kernelFinalsLimit'], 'scan.kernelFinalsLimit', 1, U32_MAX)
    return out


def _settlement(value):
    _keys(value, ('minimumDepth', 'minimumWork'), 'settlement')
    return dict(minimumDepth=_int(value['minimumDepth'], 'settlement.minimumDepth', 1),
                minimumWork=_decimal_u128(value['minimumWork'], 'settlement.minimumWork'))


def _checker(value, kernel_payer):
    fields = ('hosts', 'round', 'providerSeat', 'payerSeats', 'maxUnitBytes', 'maxTotalFetchBytes',
              'maxWorkspaceBytes', 'maxRequests', 'challengeValidUntilDaaSpan')
    _keys(value, fields, 'usefulWork.checker')
    # null = no funded material-challenge permission after a classified fetch failure; the node
    # infers none from a local timeout. A number is a span from the DAA observed when the spec is written.
    span = value['challengeValidUntilDaaSpan']
    if span is not None:
        _int(span, 'usefulWork.checker.challengeValidUntilDaaSpan', 1)
    hosts = value['hosts']
    if not isinstance(hosts, list) or not hosts or len(set(hosts)) != len(hosts) \
            or any(host not in CHECKER_HOSTS for host in hosts):
        raise PlanError('usefulWork.checker.hosts must name distinct paid-v4 roles that may host a selected checker')
    # One dedicated funded payer per checker host: two checkers sharing one fee float would race for it.
    payers = value['payerSeats']
    if not isinstance(payers, list) or len(payers) != len(hosts) or len(set(payers)) != len(payers):
        raise PlanError('usefulWork.checker.payerSeats must name one distinct payer seat per checker host')
    for payer in payers:
        _int(payer, 'usefulWork.checker.payerSeats[]', 0, MAXIMUM_SEAT)
        if payer in RESERVED_SEATS or payer == kernel_payer:
            raise PlanError('a checker receipt payer must be a dedicated seat (not 0..3, not the kernel payer)')
    return dict(hosts=list(hosts), round=_int(value['round'], 'usefulWork.checker.round', 0, U32_MAX),
                providerSeat=_enum(value['providerSeat'], 'usefulWork.checker.providerSeat', (1, 2)),
                payerSeats=list(payers),
                maxUnitBytes=_int(value['maxUnitBytes'], 'usefulWork.checker.maxUnitBytes', 1),
                maxTotalFetchBytes=_int(value['maxTotalFetchBytes'], 'usefulWork.checker.maxTotalFetchBytes', 1),
                maxWorkspaceBytes=_int(value['maxWorkspaceBytes'], 'usefulWork.checker.maxWorkspaceBytes', 1),
                maxRequests=_int(value['maxRequests'], 'usefulWork.checker.maxRequests', 1, U32_MAX),
                challengeValidUntilDaaSpan=span)


def _useful_work(value, allowed):
    fields = ('requestCommand', 'requestFile', 'claimSalt', 'commitDaaOffset', 'expiresDaaSpan', 'kernelPayerSeat',
              'maxInputBytes', 'maxMaterialBytes', 'maxWorkspaceBytes', 'maxUnits', 'checker')
    _keys(value, fields, 'usefulWork')
    request_file = _template(value['requestFile'], 'usefulWork.requestFile', allowed)
    if not request_file.startswith('{ROOT}/') or '..' in request_file:
        raise PlanError('usefulWork.requestFile must name a path under the owned {ROOT}')
    seat = _int(value['kernelPayerSeat'], 'usefulWork.kernelPayerSeat', 0, MAXIMUM_SEAT)
    if seat in RESERVED_SEATS:
        raise PlanError('the kernel claim payer must be a dedicated seat, not executor/provider/ACK payer seats 0..3')
    return dict(requestCommand=_argv(value['requestCommand'], 'usefulWork.requestCommand', allowed),
                requestFile=request_file,
                claimSalt=_hex(value['claimSalt'], 'usefulWork.claimSalt', 128, True),
                commitDaaOffset=_int(value['commitDaaOffset'], 'usefulWork.commitDaaOffset'),
                expiresDaaSpan=_int(value['expiresDaaSpan'], 'usefulWork.expiresDaaSpan', 1),
                kernelPayerSeat=seat,
                maxInputBytes=_int(value['maxInputBytes'], 'usefulWork.maxInputBytes', 1),
                maxMaterialBytes=_int(value['maxMaterialBytes'], 'usefulWork.maxMaterialBytes', 1),
                maxWorkspaceBytes=_int(value['maxWorkspaceBytes'], 'usefulWork.maxWorkspaceBytes', 1),
                maxUnits=_int(value['maxUnits'], 'usefulWork.maxUnits', 1, U32_MAX),
                checker=_checker(value['checker'], seat))


def _topology(value, allowed):
    fields = ('forkFixture', 'partitionFixture', 'pruneFixture', 'forkWinner', 'partitionWinner',
              'minimumIsolatedDaaSpan', 'freshJoinSource')
    _keys(value, fields, 'topology')
    return dict(forkFixture=_argv_list(value['forkFixture'], 'topology.forkFixture', allowed, 1),
                partitionFixture=_argv_list(value['partitionFixture'], 'topology.partitionFixture', allowed, 1),
                pruneFixture=_argv_list(value['pruneFixture'], 'topology.pruneFixture', allowed, 1),
                forkWinner=_enum(value['forkWinner'], 'topology.forkWinner', ('producer', 'provider')),
                partitionWinner=_enum(value['partitionWinner'], 'topology.partitionWinner', ('producer', 'provider')),
                minimumIsolatedDaaSpan=_int(value['minimumIsolatedDaaSpan'], 'topology.minimumIsolatedDaaSpan',
                                            MINIMUM_ISOLATED_DAA_SPAN),
                freshJoinSource=_enum(value['freshJoinSource'], 'topology.freshJoinSource',
                                      ('producer', 'provider', 'checker')))


def _scale(value, allowed):
    _keys(value, ('claimFixture', 'claimCounter', 'minimumClaims', 'registryPopulation'), 'scale')
    population = _keys(value['registryPopulation'], ('enabled', 'bonds', 'timeoutSeconds'), 'scale.registryPopulation')
    return dict(claimFixture=_argv_list(value['claimFixture'], 'scale.claimFixture', allowed, 0),
                claimCounter=_enum(value['claimCounter'], 'scale.claimCounter', CLAIM_COUNTERS),
                minimumClaims=_int(value['minimumClaims'], 'scale.minimumClaims', MINIMUM_SCALE),
                registryPopulation=dict(
                    enabled=_bool(population['enabled'], 'scale.registryPopulation.enabled'),
                    bonds=_int(population['bonds'], 'scale.registryPopulation.bonds', MINIMUM_SCALE, U32_MAX),
                    timeoutSeconds=_int(population['timeoutSeconds'], 'scale.registryPopulation.timeoutSeconds', 1)))


def _trigger(value, name, role, phase):
    kind = value.get('kind') if isinstance(value, dict) else None
    if kind not in TRIGGERS:
        raise PlanError('%s.kind must be one of %s' % (name, ', '.join(TRIGGERS)))
    roles, phases = TRIGGERS[kind]
    if role not in roles or phase not in phases:
        raise PlanError('%s %s cannot be armed for role %s in phase %s' % (name, kind, role, phase))
    if kind == 'import-wal':
        _keys(value, ('kind', 'stage'), name)
        return dict(kind=kind, stage=_enum(value['stage'], name + '.stage', IMPORT_WAL_STAGES))
    if kind == 'import-phase':
        _keys(value, ('kind', 'phases'), name)
        chosen = value['phases']
        if not isinstance(chosen, list) or not chosen or len(set(chosen)) != len(chosen) \
                or any(item not in IMPORT_PHASES for item in chosen):
            raise PlanError(name + '.phases must be distinct active import phases')
        return dict(kind=kind, phases=list(chosen))
    if kind in ('kernel-outbox', 'checker-outbox'):
        files = OUTBOX_FILES if kind == 'kernel-outbox' else CHECKER_OUTBOX_FILES
        _keys(value, ('kind', 'present', 'absent'), name)
        present = _enum(value['present'], name + '.present', files)
        absent = value['absent']
        if absent is not None:
            _enum(absent, name + '.absent', files)
            if absent == present:
                raise PlanError(name + ' present and absent files must differ')
        return dict(kind=kind, present=present, absent=absent)
    _keys(value, ('kind',), name)
    return dict(kind=kind)


def _crash(value, allowed):
    _keys(value, ('points', 'generationMismatch', 'conflictFixture'), 'crash')
    points = value['points']
    if not isinstance(points, list) or not points:
        raise PlanError('crash.points must list at least one injected point')
    parsed, names = [], set()
    for index, point in enumerate(points):
        name = 'crash.points[%d]' % index
        _keys(point, ('name', 'role', 'phase', 'trigger', 'maximumWaitSeconds'), name)
        label = _text(point['name'], name + '.name', r'[a-z0-9][a-z0-9-]{0,63}')
        if label in names:
            raise PlanError('crash point names must be unique')
        names.add(label)
        role = _enum(point['role'], name + '.role', tuple(ROLE_KEYS))
        phase = _enum(point['phase'], name + '.phase', CRASH_PHASES)
        parsed.append(dict(name=label, role=role, phase=phase, trigger=_trigger(point['trigger'], name + '.trigger', role, phase),
                           maximumWaitSeconds=_int(point['maximumWaitSeconds'], name + '.maximumWaitSeconds', 1)))
    if sum(p['trigger']['kind'] == 'import-wal' for p in parsed) > 1:
        raise PlanError('one exact WAL rendezvous per owned crash-join launch; use a separate bounded plan for each stage')
    if any(p['trigger']['kind'] == 'import-wal' for p in parsed) and any(
            p['trigger']['kind'] == 'import-phase' for p in parsed):
        raise PlanError('an exact WAL run cannot also kill its crash-join process at a coarse import phase')
    mismatch = _keys(value['generationMismatch'], ('role', 'wrongProfile', 'exitWithinSeconds'), 'crash.generationMismatch')
    return dict(points=parsed,
                generationMismatch=dict(
                    role=_enum(mismatch['role'], 'crash.generationMismatch.role', ('checker', 'fresh')),
                    wrongProfile=_enum(mismatch['wrongProfile'], 'crash.generationMismatch.wrongProfile',
                                       MISMATCH_PROFILES),
                    exitWithinSeconds=_int(mismatch['exitWithinSeconds'], 'crash.generationMismatch.exitWithinSeconds', 1)),
                conflictFixture=_argv_list(value['conflictFixture'], 'crash.conflictFixture', allowed, 0))


def _capacity(value):
    _keys(value, ('mode', 'role', 'constrainedArgs', 'refusalWindowSeconds', 'participationSignal'), 'capacity')
    mode = _enum(value['mode'], 'capacity.mode', tuple(CAPACITY_MODES))
    args = value['constrainedArgs']
    if not isinstance(args, list) or not args:
        raise PlanError('capacity.constrainedArgs must name at least one local capacity flag')
    seen = set()
    for arg in args:
        if not isinstance(arg, str):
            raise PlanError('capacity.constrainedArgs must be strings')
        flag, _, number = arg.partition('=')
        if flag not in CAPACITY_FLAGS or flag in seen:
            raise PlanError('capacity flag %r is not an allowlisted local capacity flag (or repeats)' % flag)
        seen.add(flag)
        if CAPACITY_FLAGS[flag]:
            if not number.isdigit() or str(int(number)) != number or not 0 < int(number) <= U64_MAX:
                raise PlanError(flag + ' needs an explicit positive u64 value')
        elif number or '=' in arg:
            raise PlanError(flag + ' takes no value')
    role = _enum(value['role'], 'capacity.role', CAPACITY_MODES[mode])
    signal = _enum(value['participationSignal'], 'capacity.participationSignal', PARTICIPATION_SIGNALS)
    if signal == 'provider-store-grows' and role != 'provider':
        raise PlanError('provider-store-grows can only witness the provider role')
    if signal == 'produced-blocks-increase' and role not in ('provider', 'producer'):
        raise PlanError('produced-blocks-increase needs a producing role')
    if mode == 'import' and signal != 'sink-follows':
        # The joiner produces nothing: after its import is retried it can only follow the network sink.
        raise PlanError('an import-mode capacity control is witnessed by sink-follows')
    return dict(mode=mode, role=role, constrainedArgs=list(args),
                refusalWindowSeconds=_int(value['refusalWindowSeconds'], 'capacity.refusalWindowSeconds', 1),
                participationSignal=signal)


def _evm(value):
    _keys(value, ('consumerCheckpoints', 'roles'), 'evm')
    checkpoints, roles = value['consumerCheckpoints'], value['roles']
    if not isinstance(checkpoints, list) or not checkpoints or len(set(checkpoints)) != len(checkpoints) \
            or any(item not in CONSUMER_CHECKPOINTS for item in checkpoints):
        raise PlanError('evm.consumerCheckpoints must be distinct named phases')
    if not isinstance(roles, list) or len(roles) < 2 or len(set(roles)) != len(roles) \
            or any(item not in ('producer', 'provider', 'checker', 'fresh') for item in roles):
        raise PlanError('evm.roles must name at least two distinct observing roles')
    return dict(consumerCheckpoints=list(checkpoints), roles=list(roles))


def _native_address(value, name):
    return _text(value, name, r'[a-z]+:[a-z0-9]{8,200}')


def _bridge(value, allowed):
    fields = ('depositorKey', 'evmAddress', 'depositAmountSompi', 'claimTipSompi', 'depositLockFeeSompi',
              'claimTimeoutDaaDelta', 'postClaimEvmBlocks', 'refund', 'withdrawal')
    _keys(value, fields, 'bridge')
    amount = _int(value['depositAmountSompi'], 'bridge.depositAmountSompi', 1)
    tip = _int(value['claimTipSompi'], 'bridge.claimTipSompi', 0, amount)
    refund = _keys(value['refund'], ('evmAddress', 'amountSompi', 'lockFeeSompi', 'timeoutDaaDelta', 'commands',
                                     'destinationAddress', 'maximumRefundFeeSompi'), 'bridge.refund')
    withdrawal = _keys(value['withdrawal'], ('commands', 'sourceEvmAddress', 'destinationAddress', 'amountSompi',
                                             'maximumGasWei'), 'bridge.withdrawal')
    out = dict(depositorKey=_enum(value['depositorKey'], 'bridge.depositorKey', ('funding',)),
               evmAddress=_evm_address(value['evmAddress'], 'bridge.evmAddress'),
               depositAmountSompi=amount, claimTipSompi=tip,
               depositLockFeeSompi=_int(value['depositLockFeeSompi'], 'bridge.depositLockFeeSompi', 1),
               claimTimeoutDaaDelta=_int(value['claimTimeoutDaaDelta'], 'bridge.claimTimeoutDaaDelta', 1),
               postClaimEvmBlocks=_int(value['postClaimEvmBlocks'], 'bridge.postClaimEvmBlocks', 1),
               refund=dict(
                   evmAddress=_evm_address(refund['evmAddress'], 'bridge.refund.evmAddress'),
                   amountSompi=_int(refund['amountSompi'], 'bridge.refund.amountSompi', 1),
                   lockFeeSompi=_int(refund['lockFeeSompi'], 'bridge.refund.lockFeeSompi', 1),
                   timeoutDaaDelta=_int(refund['timeoutDaaDelta'], 'bridge.refund.timeoutDaaDelta', 1),
                   commands=_argv_list(refund['commands'], 'bridge.refund.commands', allowed, 0),
                   destinationAddress=_native_address(refund['destinationAddress'], 'bridge.refund.destinationAddress'),
                   maximumRefundFeeSompi=_int(refund['maximumRefundFeeSompi'], 'bridge.refund.maximumRefundFeeSompi')),
               withdrawal=dict(
                   commands=_argv_list(withdrawal['commands'], 'bridge.withdrawal.commands', allowed, 0),
                   sourceEvmAddress=_evm_address(withdrawal['sourceEvmAddress'], 'bridge.withdrawal.sourceEvmAddress'),
                   destinationAddress=_native_address(withdrawal['destinationAddress'],
                                                      'bridge.withdrawal.destinationAddress'),
                   amountSompi=_int(withdrawal['amountSompi'], 'bridge.withdrawal.amountSompi', 1),
                   maximumGasWei=_decimal_u128(withdrawal['maximumGasWei'], 'bridge.withdrawal.maximumGasWei', False)))
    if out['refund']['evmAddress'] == out['evmAddress']:
        raise PlanError('the refund lock must name a separate EVM address, so any credit of it is visible')
    if out['withdrawal']['sourceEvmAddress'] != out['evmAddress']:
        raise PlanError('the withdrawal must spend the credited deposit account so backing reconciles on one account')
    if out['withdrawal']['destinationAddress'] == out['refund']['destinationAddress']:
        raise PlanError('withdrawal and refund destinations must be separate dedicated native addresses')
    if out['refund']['maximumRefundFeeSompi'] >= out['refund']['amountSompi']:
        raise PlanError('the refund fee bound must leave a positive refunded amount')
    if out['refund']['timeoutDaaDelta'] >= out['claimTimeoutDaaDelta']:
        # The refund lock must expire while the claimed deposit is still inside its claim window.
        raise PlanError('the refund lock timeout must be shorter than the claimed deposit timeout')
    return out


def _market(value, allowed):
    fields = ('enabled', 'openCommands', 'buyCommands', 'sellCommands', 'holderId', 'lineId', 'buyUnits', 'sellUnits')
    _keys(value, fields, 'market')
    buy = _int(value['buyUnits'], 'market.buyUnits', 1)
    return dict(enabled=_bool(value['enabled'], 'market.enabled'),
                openCommands=_argv_list(value['openCommands'], 'market.openCommands', allowed, 0),
                buyCommands=_argv_list(value['buyCommands'], 'market.buyCommands', allowed, 0),
                sellCommands=_argv_list(value['sellCommands'], 'market.sellCommands', allowed, 0),
                holderId=_hex(value['holderId'], 'market.holderId', 128, True),
                lineId=_hex(value['lineId'], 'market.lineId', 128, True),
                buyUnits=buy, sellUnits=_int(value['sellUnits'], 'market.sellUnits', 1, buy))


def _accounting(value):
    _keys(value, ('incomeSeats', 'providerPayerSeat'), 'accounting')
    seats = value['incomeSeats']
    if not isinstance(seats, list) or not seats or len(set(seats)) != len(seats):
        raise PlanError('accounting.incomeSeats must be distinct seats')
    for seat in seats:
        _int(seat, 'accounting.incomeSeats[]', 0, MAXIMUM_SEAT)
    if _int(value['providerPayerSeat'], 'accounting.providerPayerSeat', 0, MAXIMUM_SEAT) != PROVIDER_PAYER_SEAT:
        raise PlanError('the provider ACK carrier payer is fixed at seat 3 by the paid-v4 provider wiring')
    if PROVIDER_PAYER_SEAT in seats:
        raise PlanError('the dedicated carrier payer cannot also be an income seat')
    return dict(incomeSeats=list(seats), providerPayerSeat=PROVIDER_PAYER_SEAT)


def _reconciliation(value):
    _keys(value, ('bondPageLimit', 'maximumBondPages'), 'reconciliation')
    return dict(bondPageLimit=_int(value['bondPageLimit'], 'reconciliation.bondPageLimit', 1, 1000),
                maximumBondPages=_int(value['maximumBondPages'], 'reconciliation.maximumBondPages', 1, 100_000))


def _coverage(value):
    # RFC-0006 EXEC origin header coverage (rfc06-exec-origin-coverage-v1.json): written only by custody/kernel-consumer
    # paid-v4 nodes. The archive role is launched with --archival; the pruned role is the producer, the GC source.
    _keys(value, ('archiveRole', 'maximumWaitSeconds'), 'coverage')
    return dict(archiveRole=_enum(value['archiveRole'], 'coverage.archiveRole', ('provider', 'checker')),
                prunedRole='producer',
                maximumWaitSeconds=_int(value['maximumWaitSeconds'], 'coverage.maximumWaitSeconds', 1))


VALIDATOR_ROLES = ('provider', 'checker')
VALIDATOR_MODES = ('active', 'standby')
PALW_PAYOUT_SOURCES = ('factory-seats',)


def _owned_template(value, name, allowed):
    template = _template(value, name, allowed)
    if not template.startswith('{ROOT}/') or '..' in template:
        raise PlanError(name + ' must name a path under the owned {ROOT}')
    return template


def dns_cadence(value, name):
    """The node's DnsParams the legacy settlement window is derived from (rfc12_retirement_reconciliation, rule note)."""
    _keys(value, ('epochLengthBlocks', 'rewardUniquenessWindowBlocks', 'maxReorgHorizonBlocks',
                  'attestationEpochLengthBlueScore', 'attestationLagBlueScore'), name)
    return dict(epochLengthBlocks=_int(value['epochLengthBlocks'], name + '.epochLengthBlocks', 1),
                rewardUniquenessWindowBlocks=_int(value['rewardUniquenessWindowBlocks'], name + '.rewardUniquenessWindowBlocks'),
                maxReorgHorizonBlocks=_int(value['maxReorgHorizonBlocks'], name + '.maxReorgHorizonBlocks'),
                attestationEpochLengthBlueScore=_int(value['attestationEpochLengthBlueScore'],
                                                     name + '.attestationEpochLengthBlueScore', 1),
                attestationLagBlueScore=_int(value['attestationLagBlueScore'], name + '.attestationLagBlueScore'))


def check_legacy_window(fence, window, observe, cadence, name):
    """A non-zero legacy settlement window must be exactly the one the node rule derives; the scan must cover it."""
    import rfc12_retirement_reconciliation as R
    if window == 0:
        return
    derived = R.legacy_settlement_window(fence, cadence)['windowDaa']
    if window != derived:
        raise PlanError('%s.legacySettlementWindowDaa %d is not the window the node rule derives (%d) for fence %d'
                        % (name, window, derived, fence))
    if observe <= window:
        raise PlanError('%s.observeAfterFenceDaa must exceed the legacy settlement window so its last crossing is scanned' % name)


def _dns_history(value, allowed):
    """RFC-0012 §4.4: real DNS-validator history before a later private retirement fence, then its reconciliation."""
    fields = ('keyringCommands', 'validators', 'bondWaitSeconds', 'exit', 'legacySettlementWindowDaa', 'dnsCadence',
              'observeAfterFenceDaa', 'scanFromDaa', 'maximumBlockReads', 'maximumStatusReads', 'palwPayoutSource')
    _keys(value, fields, 'dnsHistory')
    validators, roles = [], set()
    if not isinstance(value['validators'], list):
        raise PlanError('dnsHistory.validators must be a list')
    for index, item in enumerate(value['validators']):
        name = 'dnsHistory.validators[%d]' % index
        _keys(item, ('role', 'keyFile', 'mode', 'bondCommands'), name)
        role = _enum(item['role'], name + '.role', VALIDATOR_ROLES)
        if role in roles:
            raise PlanError('one DNS validator per node role')
        roles.add(role)
        validators.append(dict(role=role, keyFile=_owned_template(item['keyFile'], name + '.keyFile', allowed),
                               mode=_enum(item['mode'], name + '.mode', VALIDATOR_MODES),
                               bondCommands=_argv_list(item['bondCommands'], name + '.bondCommands', allowed, 1)))
    exit_ = _keys(value['exit'], ('unbondCommands', 'withdrawCommands', 'destinationAddress', 'maximumFeeSompi'),
                  'dnsHistory.exit')
    unbond = _argv_list(exit_['unbondCommands'], 'dnsHistory.exit.unbondCommands', allowed, 0)
    withdraw = _argv_list(exit_['withdrawCommands'], 'dnsHistory.exit.withdrawCommands', allowed, 0)
    destination = exit_['destinationAddress']
    if unbond or withdraw:
        if not unbond or not withdraw or destination is None or not validators:
            raise PlanError('a legacy exit needs unbond and withdraw commands, a destination and a bonded validator')
        destination = _native_address(destination, 'dnsHistory.exit.destinationAddress')
    elif destination is not None:
        raise PlanError('dnsHistory.exit.destinationAddress without exit commands')
    return dict(keyringCommands=_argv_list(value['keyringCommands'], 'dnsHistory.keyringCommands', allowed, 0),
                validators=validators,
                bondWaitSeconds=_int(value['bondWaitSeconds'], 'dnsHistory.bondWaitSeconds', 1),
                exit=dict(unbondCommands=unbond, withdrawCommands=withdraw, destinationAddress=destination,
                          maximumFeeSompi=_int(exit_['maximumFeeSompi'], 'dnsHistory.exit.maximumFeeSompi')),
                legacySettlementWindowDaa=_int(value['legacySettlementWindowDaa'], 'dnsHistory.legacySettlementWindowDaa'),
                dnsCadence=dns_cadence(value['dnsCadence'], 'dnsHistory.dnsCadence'),
                observeAfterFenceDaa=_int(value['observeAfterFenceDaa'], 'dnsHistory.observeAfterFenceDaa', 1),
                scanFromDaa=_int(value['scanFromDaa'], 'dnsHistory.scanFromDaa'),
                maximumBlockReads=_int(value['maximumBlockReads'], 'dnsHistory.maximumBlockReads', 1),
                maximumStatusReads=_int(value['maximumStatusReads'], 'dnsHistory.maximumStatusReads', 1),
                palwPayoutSource=_enum(value['palwPayoutSource'], 'dnsHistory.palwPayoutSource', PALW_PAYOUT_SOURCES))


def parse_plan(raw):
    """Parse exact plan bytes. Returns a normalized dict carrying the bytes' SHA-256."""
    if not isinstance(raw, (bytes, bytearray)):
        raise PlanError('the plan must be read as exact bytes')
    try:
        plan = json.loads(raw)
    except ValueError as error:
        raise PlanError('plan is not JSON: %s' % error)
    _keys(plan, TOP_KEYS, 'plan')
    if type(plan['schemaVersion']) is not int or plan['schemaVersion'] != PLAN_VERSION:
        raise PlanError('unsupported acceptance plan schema version')
    if plan['kind'] != PLAN_KIND:
        raise PlanError('plan kind must be ' + PLAN_KIND)
    allowed = placeholders()
    binaries = _keys(plan['binaries'], ('node', 'cli'), 'binaries')
    out = dict(
        schemaVersion=PLAN_VERSION, kind=PLAN_KIND, sha256=hashlib.sha256(bytes(raw)).hexdigest(),
        commit=_hex(plan['commit'], 'commit', 40, True),
        network=_network(plan['network']),
        salt=_hex(plan['salt'], 'salt', 64, True),
        binaries=dict(node=_absolute(binaries['node'], 'binaries.node'), cli=_absolute(binaries['cli'], 'binaries.cli')),
        identity=_identity(plan['identity']),
        bounds=_bounds(plan['bounds']),
        observationBoundsFile=_absolute(plan['observationBoundsFile'], 'observationBoundsFile'),
        fees=_fees(plan['fees']),
        scan=_scan(plan['scan']),
        settlement=_settlement(plan['settlement']),
        usefulWork=_useful_work(plan['usefulWork'], allowed),
        topology=_topology(plan['topology'], allowed),
        scale=_scale(plan['scale'], allowed),
        crash=_crash(plan['crash'], allowed),
        capacity=_capacity(plan['capacity']),
        evm=_evm(plan['evm']),
        bridge=_bridge(plan['bridge'], allowed),
        market=_market(plan['market'], allowed),
        accounting=_accounting(plan['accounting']),
        reconciliation=_reconciliation(plan['reconciliation']),
        coverage=_coverage(plan['coverage']),
        dnsHistory=_dns_history(plan['dnsHistory'], allowed),
    )
    payers = [out['usefulWork']['kernelPayerSeat']] + out['usefulWork']['checker']['payerSeats']
    if set(payers) & set(out['accounting']['incomeSeats']):
        raise PlanError('a dedicated carrier payer (kernel claim or checker receipt) cannot also be an income seat')
    for name, fee in (('bridge.depositLockFeeSompi', out['bridge']['depositLockFeeSompi']),
                      ('bridge.refund.lockFeeSompi', out['bridge']['refund']['lockFeeSompi'])):
        if fee > out['fees']['maximumDepositLockFeeSompi']:
            raise PlanError(name + ' exceeds fees.maximumDepositLockFeeSompi')
    if out['coverage']['archiveRole'] == 'checker' and 'checker' not in out['usefulWork']['checker']['hosts']:
        raise PlanError('an archive coverage role must be a custody node; the checker role hosts no selected checker')
    fence = out['identity']['retirementActivationDaa']
    if out['dnsHistory']['validators'] and fence < 2:
        raise PlanError('DNS-validator history needs the later private retirement (identity.retirementActivationDaa >= 2)')
    if out['dnsHistory']['scanFromDaa'] >= fence and fence >= 2:
        raise PlanError('dnsHistory.scanFromDaa must start below the fence so pre-fence history is scanned')
    if fence >= 2:
        check_legacy_window(fence, out['dnsHistory']['legacySettlementWindowDaa'], out['dnsHistory']['observeAfterFenceDaa'],
                            out['dnsHistory']['dnsCadence'], 'dnsHistory')
    if out['capacity']['mode'] == 'import' and any(p['trigger']['kind'] == 'import-wal' for p in out['crash']['points']):
        raise PlanError('exact WAL runs need running-node capacity mode; constrained import must be a separate run')
    for point in out['crash']['points']:
        if point['trigger']['kind'] == 'checker-outbox' and point['role'] not in out['usefulWork']['checker']['hosts']:
            raise PlanError('crash point %s watches a checker outbox on a role that hosts no checker' % point['name'])
    return out


def fill(template, values):
    """Expand one already validated template; an unbound placeholder is a refusal, not empty text."""
    try:
        return template.format_map(_Strict(values))
    except KeyError as error:
        raise PlanError('placeholder %s has no value at this phase' % error)


class _Strict(dict):
    def __missing__(self, key):
        raise KeyError(key)


def fill_argv(argv, values):
    return [fill(item, values) for item in argv]
