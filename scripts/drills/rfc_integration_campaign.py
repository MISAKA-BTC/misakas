"""One frozen candidate, one owned testnet, actual evidence only.

Source controls are not execution evidence. No native approval/settlement or
attack outcome is inferred from fixture stdout, a manifest, WAL or Final label.
"""
from dataclasses import dataclass
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import time
import rfc08_native_paid_observation as paid_observations

SCHEMA = 1
KIND = 'misaka-rfc-integration-campaign-v1'
ROLES = frozenset(('A', 'B', 'JOIN'))
STAGES = frozenset(
    ['run.before', 'run.after'] +
    ['fixture.' + name + '.' + edge for name in ('bootstrap', 'fork', 'partition', 'prune') for edge in ('before', 'after')] +
    ['milestone.' + name for name in ('shared-bootstrap', 'fork-convergence', 'partition-healed',
        'source-body-pruned', 'fresh-pruned-join', 'cold-pruned-join')])
# These are evidence contracts, not endpoints invented by the plan. The current
# public RPC has only the claim identity collector implemented below.
NATIVE_KINDS = frozenset(('native-claim-observed', 'native-paid-observed', 'native-paid-assigned-quorum', 'selected-verifier-approval', 'native-fee-settlement',
    'reader-router-court', 'all-model-cost-boundaries', 'independent-source-availability',
    'cumulative-soundness-review', '128-bit-review', 'adapter-interoperability',
    'all-rewards-once', 'material-all-path-bounds', 'restart-reorg-atomicity', 'market-accounting',
    'attack-submission', 'attack-import', 'attack-adjudication'))
GATES = {
    'rfc06-model-boundaries-cost': 'all-model-cost-boundaries',
    'rfc06-routing-court': 'reader-router-court',
    'rfc06-100k-pruned-p2p': 'base-drill-mechanics',
    'rfc07-independent-sources': 'independent-source-availability',
    'rfc07-income-cost': 'native-fee-settlement',
    'rfc07-cumulative-soundness': 'cumulative-soundness-review',
    'rfc07-128bit-review': '128-bit-review',
    'rfc07-adapters': 'adapter-interoperability',
    'rfc07-selected-approval': 'selected-verifier-approval',
    'rfc08-all-rewards-once': 'all-rewards-once',
    'rfc08-material-bounds': 'material-all-path-bounds',
    'rfc08-restart-reorg': 'restart-reorg-atomicity',
    'rfc12-fork-ibd-prune-dns': 'base-drill-mechanics',
    'rfc12-crash-atomicity': 'restart-reorg-atomicity',
    'rfc12-market': 'market-accounting',
}
SOURCE_LEAVES = ('misaka-rfc-integration-campaign.py', 'rfc_integration_campaign.py', 'misaka-rfc12-multiprocess.py',
    'rfc12_owned_resources.py', 'misaka_wrpc_json.py', 'rfc12_kernel_consumers.py', 'rfc12_native_recovery_observation.py',
    'rfc08_native_paid_observation.py')
BUILD_TOOLS = frozenset(('cargo', 'rustc', 'rustup', 'make', 'cmake', 'ninja', 'meson', 'git', 'rebase', 'reset'))
IDENTIFIER = re.compile(r'[a-z0-9][a-z0-9._-]{0,95}\Z')


class CampaignError(RuntimeError):
    pass


class CampaignIdentityError(CampaignError):
    pass


class MissingNativeFact(CampaignError):
    pass


class NativeMismatch(CampaignError):
    pass


def require(condition, message):
    if not condition:
        raise CampaignError(message)


def exact_keys(value, fields, label):
    require(isinstance(value, dict) and set(value) == set(fields), label + ' fields differ from the frozen schema')


def hash_text(value, width, label):
    require(isinstance(value, str) and len(value) == width and all(c in '0123456789abcdef' for c in value),
        label + ' is not canonical lowercase hex')
    return value


def u64(value, label, minimum=0):
    require(type(value) is int and minimum <= value < 2**64, label + ' is not an exact bounded unsigned counter')
    return value


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()


def read_json(path, maximum=32 * 1024 * 1024):
    with Path(path).open('rb') as source:
        raw = source.read(maximum + 1)
    require(len(raw) <= maximum, 'plan/source framing exceeds its finite input cap')
    return raw, json.loads(raw)


@dataclass(frozen=True)
class FilePin:
    path: str
    sha256: str
    device: int
    inode: int
    size: int
    mtime_ns: int

    @classmethod
    def capture(cls, path, expected=None, maximum_bytes=None, on_read=None):
        path = Path(path)
        require(path.is_absolute(), 'pinned executables and inputs must use absolute paths')
        require(not any(parent.is_symlink() for parent in (path,) + tuple(path.parents)), 'pinned path traverses a symlink')
        resolved = path.resolve(strict=True)
        require(str(path) == str(resolved), 'pinned paths must already be canonical absolute paths')
        path = resolved
        before = path.stat()
        require(path.is_file(), 'pin is not a regular file')
        if maximum_bytes is not None:
            require(before.st_size <= maximum_bytes, 'pinned file exceeds its predeclared finite read bound')
        digest = hashlib.sha256()
        count = 0
        with path.open('rb') as source:
            while count <= before.st_size:
                chunk = source.read(min(1024 * 1024, before.st_size + 1 - count))
                if not chunk:
                    break
                count += len(chunk)
                if on_read is not None:
                    on_read(len(chunk))
                digest.update(chunk)
        require(count == before.st_size, 'pinned file changed extent during bounded hashing')
        after = path.stat()
        identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns)
        require(identity(before) == identity(after), 'pinned file changed during hashing')
        actual = digest.hexdigest()
        if expected is not None:
            require(actual == hash_text(expected, 64, 'expected SHA256'), 'pinned file SHA256 differs: ' + str(path))
        return cls(str(path), actual, *identity(after))

    def revalidate(self, on_read=None):
        self.observe_metadata()
        require(FilePin.capture(self.path, maximum_bytes=self.size, on_read=on_read) == self, 'frozen file replaced/modified during campaign: ' + self.path)

    def observe_metadata(self):
        path = Path(self.path)
        require(not any(part.is_symlink() for part in (path,) + tuple(path.parents)), 'frozen path changed into a symlink')
        actual = path.stat()
        require((actual.st_dev, actual.st_ino, actual.st_size, actual.st_mtime_ns) ==
            (self.device, self.inode, self.size, self.mtime_ns), 'frozen file metadata changed: ' + self.path)

    def report(self):
        return dict(path=self.path, sha256=self.sha256, bytes=self.size, device=self.device, inode=self.inode,
            mtimeNs=self.mtime_ns)


def validate_argv(argv):
    require(isinstance(argv, list) and argv and all(isinstance(arg, str) and arg and '\x00' not in arg for arg in argv),
        'commands must be nonempty explicit argv arrays')
    require(Path(argv[0]).is_absolute() and '{' not in argv[0], 'command executable must be a frozen absolute path')
    require(Path(argv[0]).name not in BUILD_TOOLS, 'building or rewriting Git state is forbidden inside a campaign')
    require(Path(argv[0]).name not in ('sh', 'bash', 'zsh', 'fish', 'env'), 'campaign commands cannot hide argv behind a shell/env')
    require(not any(Path(arg).name in BUILD_TOOLS for arg in argv), 'command contains a forbidden build/Git operation')
    require(not any(arg in ('-c', '-e', '--eval', '-m') for arg in argv[1:]), 'inline/module command dispatch cannot replace a frozen helper script')


def validate_campaign(plan, drill):
    exact_keys(plan, ('schemaVersion', 'kind', 'campaignId', 'candidate', 'sourceClaims', 'network', 'drillPlan',
        'resourceLimits', 'integrityPolicy', 'pinnedInputs', 'scenarios', 'gates'), 'campaign')
    require(type(plan['schemaVersion']) is int and plan['schemaVersion'] == SCHEMA and plan['kind'] == KIND, 'campaign schema/version differs')
    require(isinstance(plan['campaignId'], str) and IDENTIFIER.fullmatch(plan['campaignId']) is not None, 'campaignId is not a stable identifier')
    exact_keys(plan['candidate'], ('path', 'sha256'), 'candidate')
    exact_keys(plan['integrityPolicy'], ('fullDigestIntervalSeconds', 'maximumLogicalFileReadBytes', 'maximumSingleCheckMilliseconds'), 'integrity policy')
    for key, value in plan['integrityPolicy'].items():
        u64(value, 'integrity policy ' + key, 1)
    for key in ('drillPlan', 'resourceLimits'):
        exact_keys(plan[key], ('path', 'sha256'), key)
    hash_text(plan['candidate']['sha256'], 64, 'candidate SHA256')
    exact_keys(plan['sourceClaims'], ('revision', 'buildInputs'), 'source claims')
    require(isinstance(plan['sourceClaims']['revision'], str) and 1 <= len(plan['sourceClaims']['revision']) <= 128, 'source revision claim missing')
    require(isinstance(plan['sourceClaims']['buildInputs'], dict), 'build input claims must be an explicit manifest object')
    exact_keys(plan['network'], ('network', 'genesisHash', 'consensusParamsId'), 'network')
    require(plan['network']['network'] == 'misaka-testnet-12', 'this campaign uses the same explicit private testnet-12 only')
    hash_text(plan['network']['genesisHash'], 128, 'genesis')
    hash_text(plan['network']['consensusParamsId'], 128, 'consensus params')
    require(drill['ruleset_id'] == plan['network']['consensusParamsId'], 'base drill ruleset differs from campaign network')
    require(set(drill['nodes']) == ROLES, 'one campaign requires exactly the same A/B/JOIN node inventory')
    for role, node in drill['nodes'].items():
        validate_argv(node['argv'])
        paid_observations.claims(node['argv'])
        require(node['argv'][0] == plan['candidate']['path'], 'every node/restart must launch the exact same candidate path')
        require(node['wrpc'].startswith('127.0.0.1:') and node['p2p'].startswith('127.0.0.1:') and node['eth'].startswith('http://127.0.0.1:'),
            'campaign must use driver-owned loopback endpoints')
        require('--nodnsseed' in node['argv'] and '--outpeers=0' in node['argv'], 'peer discovery must remain driver controlled')
        require([arg for arg in node['argv'] if arg.startswith('--appdir=')] == ['--appdir={' + role + '_APPDIR}'],
            'scenario cannot replace the one root DB path for a role')
    source_policies = [paid_observations.source_policy(node['argv']) for node in drill['nodes'].values()]
    if any(policy is not None for policy in source_policies):
        require(all(policy is not None for policy in source_policies) and len(set(source_policies)) == 1,
            'all source-paid-v1 nodes including JOIN must use the same exact operator policy input')
        require(source_policies[0] in {pin.get('path') for pin in plan['pinnedInputs'] if isinstance(pin, dict)},
            'source-paid-v1 policy must have an independently frozen content pin')
    paid_policies = [paid_observations.paid_bootstrap_source_policy(node['argv']) for node in drill['nodes'].values()]
    if any(policy is not None for policy in paid_policies):
        require(all(policy is not None for policy in paid_policies) and len(set(paid_policies)) == 1,
            'all nodes including JOIN must use the same exact V6 bootstrap source policy')
        require(paid_policies[0] in {pin.get('path') for pin in plan['pinnedInputs'] if isinstance(pin, dict)},
            'V6 bootstrap source policy must have an independently frozen content pin')
    endpoints = []
    for node in drill['nodes'].values():
        for key in ('wrpc', 'p2p', 'eth', 'grpc'):
            if key not in node:
                continue
            value = node[key].removeprefix('http://').rstrip('/') if key == 'eth' else node[key]
            match = re.fullmatch(r'127\.0\.0\.1:([0-9]{1,5})', value)
            require(match is not None and 1 <= int(match.group(1)) <= 65535, 'node endpoint must be a finite loopback TCP port')
            endpoints.append(('127.0.0.1', int(match.group(1))))
    require(len(endpoints) == len(set(endpoints)), 'node listeners must own distinct ports across all protocols')
    require(isinstance(plan['pinnedInputs'], list), 'pinnedInputs must be an array')
    for pin in plan['pinnedInputs']:
        exact_keys(pin, ('path', 'sha256'), 'input pin')
    require(isinstance(plan['scenarios'], list) and len(plan['scenarios']) >= 2, 'one campaign requires multiple scenarios')
    seen, claims, kinds, fact_ids, rfcs, normal_claims = set(), set(), set(), set(), set(), set()
    for scenario in plan['scenarios']:
        exact_keys(scenario, ('id', 'kind', 'rfc', 'stage', 'roles', 'claims', 'commands', 'observationTimeoutSeconds', 'requirements'), 'scenario')
        require(isinstance(scenario['id'], str) and IDENTIFIER.fullmatch(scenario['id']) is not None and scenario['id'] not in seen,
            'scenario ids must be unique stable identifiers')
        seen.add(scenario['id'])
        require(scenario['kind'] in ('normal', 'attack'), 'scenario must explicitly be normal or attack')
        kinds.add(scenario['kind'])
        require(isinstance(scenario['rfc'], list) and scenario['rfc'] and set(scenario['rfc']) <= {6, 7, 8, 12}
            and all(type(n) is int for n in scenario['rfc']), 'scenario RFC scope differs')
        rfcs.update(scenario['rfc'])
        require(scenario['stage'] in STAGES and scenario['stage'] != 'run.before', 'scenario stage has no declared live-node hook')
        if scenario['stage'] == 'milestone.shared-bootstrap':
            require(drill.get('nativeKernelConsumers') is not None, 'shared-bootstrap hook requires actual kernel consumers')
        u64(scenario['observationTimeoutSeconds'], 'scenario observation timeout', 1)
        require(isinstance(scenario['roles'], list) and len(set(scenario['roles'])) == len(scenario['roles'])
            and len(scenario['roles']) >= 2 and set(scenario['roles']) <= ROLES, 'scenario requires multiple distinct actual nodes')
        if 'JOIN' in scenario['roles']:
            require(scenario['stage'] in ('milestone.fresh-pruned-join', 'milestone.cold-pruned-join', 'run.after'), 'JOIN has no actual process before pruned P2P import')
        require(isinstance(scenario['claims'], list) and scenario['claims'] and len(set(scenario['claims'])) == len(scenario['claims']), 'scenario claim scope missing/repeated')
        for claim in scenario['claims']:
            claims.add(hash_text(claim, 128, 'frozen claim id'))
        require(isinstance(scenario['commands'], list), 'scenario commands must be arrays')
        for command in scenario['commands']:
            validate_argv(command)
        require(isinstance(scenario['requirements'], list) and scenario['requirements'], 'scenario requires native evidence')
        requirements = set()
        for fact in scenario['requirements']:
            exact_keys(fact, ('id', 'kind', 'role', 'claimId', 'expected'), 'native fact requirement')
            require(isinstance(fact['id'], str) and IDENTIFIER.fullmatch(fact['id']) is not None and fact['id'] not in fact_ids,
                'native fact ids must be globally unique')
            fact_ids.add(fact['id'])
            require(fact['kind'] in NATIVE_KINDS and fact['role'] in scenario['roles'] and fact['claimId'] in scenario['claims'],
                'native fact may not escape its exact predeclared role/claim/kind')
            require(isinstance(fact['expected'], dict), 'native expected scope must be an explicit object')
            if fact['kind'] == 'native-claim-observed':
                exact_keys(fact['expected'], ('classId', 'jobId', 'producerBond', 'expectedConvicted', 'maximumRecordBytes'), 'native claim expected scope')
                for key in ('classId', 'jobId'):
                    hash_text(fact['expected'][key], 128, key)
                require(isinstance(fact['expected']['producerBond'], str) and len(fact['expected']['producerBond']) <= 160, 'native producer scope absent')
                require(type(fact['expected']['expectedConvicted']) is bool, 'expected conviction must be a predeclared boolean')
                u64(fact['expected']['maximumRecordBytes'], 'maximumRecordBytes', 1)
            if fact['kind'] in paid_observations.PAID_KINDS:
                enabled = paid_observations.claims(drill['nodes'][fact['role']]['argv'])
                if enabled or fact['kind'].startswith('native-paid-'):
                    require(fact['claimId'] in enabled, 'paid requirement must name an exact claim enabled in its frozen role argv')
                require(set(fact['expected']) <= {'enrollment', 'context', 'checkerSnapshot'},
                    'paid requirements may pin authority identities, not redefine approval/settlement facts')
                for key, value in fact['expected'].items():
                    hash_text(value, 128, 'paid expected ' + key)
            requirements.add(fact['kind'])
        require({fact['claimId'] for fact in scenario['requirements']} == set(scenario['claims']),
            'every declared claim must have its own evidence requirements')
        if scenario['kind'] == 'normal':
            normal_claims.update(scenario['claims'])
            for claim in scenario['claims']:
                observed_roles = {fact['role'] for fact in scenario['requirements']
                    if fact['claimId'] == claim and fact['kind'] == 'native-claim-observed'}
                require(len(observed_roles) >= 2, 'every normal claim requires actual native observations on multiple nodes')
        if scenario['kind'] == 'attack':
            require(scenario['commands'], 'an attack without a submission command is not an attack scenario')
            for claim in scenario['claims']:
                actual_kinds = {fact['kind'] for fact in scenario['requirements'] if fact['claimId'] == claim}
                require({'attack-submission', 'attack-import', 'attack-adjudication'} <= actual_kinds,
                    'each attack claim requires independent native submission, import and adjudication facts')
    require(kinds == {'normal', 'attack'} and len(claims) >= 2, 'campaign requires normal and attack cases with multiple distinct claims')
    require(len(normal_claims) >= 2, 'campaign requires multiple actual normal work claims, not unused ids')
    require(rfcs == {6, 7, 8, 12}, 'campaign scenario scope must cover all four RFCs')
    require(isinstance(plan['gates'], dict) and set(plan['gates']) == set(GATES), 'all RFC acceptance gates must be explicitly declared')
    indexed = {fact['id']: fact for scenario in plan['scenarios'] for fact in scenario['requirements']}
    for gate, needed_kind in GATES.items():
        ids = plan['gates'][gate]
        require(isinstance(ids, list) and ids and len(ids) == len(set(ids)), 'gate must reference unique frozen evidence ids')
        if needed_kind == 'base-drill-mechanics':
            require(ids == ['base-drill-mechanics'], 'process mechanics must come from the actual base drill, not a fixture')
        else:
            require(all(key in indexed and indexed[key]['kind'] == needed_kind for key in ids),
                'gate cannot substitute a weaker kind of evidence: ' + gate)


class FrozenCampaign:
    def __init__(self, plan_path):
        began = time.monotonic()
        self.plan_pin = FilePin.capture(str(Path(plan_path).absolute()), maximum_bytes=32 * 1024 * 1024)
        self.plan_raw, self.plan = read_json(self.plan_pin.path)
        self.drill_pin = FilePin.capture(self.plan['drillPlan']['path'], self.plan['drillPlan']['sha256'], maximum_bytes=32 * 1024 * 1024)
        self.drill_raw, self.drill = read_json(self.drill_pin.path)
        self.limits_pin = FilePin.capture(self.plan['resourceLimits']['path'], self.plan['resourceLimits']['sha256'], maximum_bytes=32 * 1024 * 1024)
        self.limits_raw, self.limits = read_json(self.limits_pin.path)
        validate_campaign(self.plan, self.drill)
        self.candidate = FilePin.capture(**dict(path=self.plan['candidate']['path'], expected=self.plan['candidate']['sha256']))
        self.pins = [self.plan_pin, self.drill_pin, self.limits_pin, self.candidate]
        self.pins.extend(FilePin.capture(pin['path'], pin['sha256']) for pin in self.plan['pinnedInputs'])
        source_root = Path(__file__).resolve().parent
        self.source_pins = [FilePin.capture(str(source_root / name)) for name in SOURCE_LEAVES]
        self.pins.extend(self.source_pins)
        pinned = {pin.path for pin in self.pins}
        commands = [command for rows in self.drill['fixtures'].values() for command in rows]
        commands.extend(command for scenario in self.plan['scenarios'] for command in scenario['commands'])
        for command in commands:
            validate_argv(command)
            require(command[0] in pinned, 'fixture executable has no independently frozen input pin: ' + command[0])
            for arg in command[1:]:
                if '{' not in arg and Path(arg).is_absolute() and Path(arg).is_file():
                    require(str(Path(arg)) in pinned, 'fixture script/input has no frozen content pin: ' + arg)
        self.templates = commands
        self.identity = hashlib.sha256(canonical(self.plan)).hexdigest()
        self.integrity_costs = [dict(event='preflight.capture', mode='sha256-full',
            logicalFileBytesRead=sum(pin.size for pin in self.pins) + len(self.plan_raw) + len(self.drill_raw) + len(self.limits_raw),
            fileChecks=len(self.pins), elapsedMilliseconds=(time.monotonic() - began) * 1000)]
        self.total_logical_pin_read_bytes = self.integrity_costs[0]['logicalFileBytesRead']
        require(self.integrity_costs[0]['elapsedMilliseconds'] <= self.plan['integrityPolicy']['maximumSingleCheckMilliseconds'],
            'preflight initial pin capture exceeded the fixed verification time bound')
        self.last_full_digest = time.monotonic()
        self.validate(event='preflight.complete')

    def validate(self, full=True, event='explicit.full-check'):
        began = time.monotonic()
        full = full or began - self.last_full_digest >= self.plan['integrityPolicy']['fullDigestIntervalSeconds']
        record = dict(event=event, mode='sha256-full' if full else 'metadata-only', logicalFileBytesRead=0,
            fileChecks=0, status='FAIL')
        try:
            def read_count(size):
                record['logicalFileBytesRead'] += size
            for pin in self.pins:
                if full:
                    pin.revalidate(on_read=read_count)
                else:
                    pin.observe_metadata()
                record['fileChecks'] += 1
            if full:
                require(hashlib.sha256(canonical(self.plan)).hexdigest() == self.identity, 'in-memory frozen plan was mutated')
                require(canonical(self.drill) == canonical(json.loads(self.drill_raw)), 'in-memory base drill plan was mutated')
                self.last_full_digest = time.monotonic()
            record['status'] = 'PASS'
        except (CampaignError, OSError) as error:
            raise CampaignIdentityError(str(error)) from error
        finally:
            record['elapsedMilliseconds'] = (time.monotonic() - began) * 1000
            self.integrity_costs.append(record)
            self.total_logical_pin_read_bytes += record['logicalFileBytesRead']
        policy = self.plan['integrityPolicy']
        require(record['elapsedMilliseconds'] <= policy['maximumSingleCheckMilliseconds'], 'predeclared pin verification time bound exceeded')
        require(self.total_logical_pin_read_bytes <= policy['maximumLogicalFileReadBytes'],
            'predeclared pin verification logical file-read budget exceeded')
        return record

    def manifest(self):
        return dict(schemaVersion=SCHEMA, kind='misaka-rfc-integration-campaign-freeze-v1', campaignId=self.plan['campaignId'],
            canonicalPlanSha256=self.identity, planSha256=hashlib.sha256(self.plan_raw).hexdigest(),
            candidate=self.candidate.report(), pinnedFiles=[pin.report() for pin in self.pins],
            sourceAndBuildClaims=self.plan['sourceClaims'], sourceClaimsAreBuildAttestation=False,
            frozenBaseDrillPlan=json.loads(self.drill_raw), frozenResourceLimits=json.loads(self.limits_raw),
            network=self.plan['network'], integrityPolicy=self.plan['integrityPolicy'], scenarios=self.plan['scenarios'], gates=self.plan['gates'],
            interpretation='candidate SHA256 establishes byte identity; revision/build inputs are separate declarations')


def load_base_drill():
    source = Path(__file__).with_name('misaka-rfc12-multiprocess.py')
    spec = importlib.util.spec_from_file_location('misaka_rfc12_campaign_base', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def unknown_fact(requirement, reason):
    return dict(id=requirement['id'], kind=requirement['kind'], role=requirement['role'], claimId=requirement['claimId'],
        status='UNKNOWN', reason=reason, nativeAuthorityEstablished=False)


def native_claim_fact(requirement, payload, dag):
    if payload.get('available') is not True or payload.get('found') is not True:
        return unknown_fact(requirement, 'exact native claim is not currently available; absence is not attack prevention')
    expected = requirement['expected']
    def native_require(condition, message):
        if not condition:
            raise NativeMismatch(message)
    native_require(payload.get('claimId') == requirement['claimId'], 'native response names another claim')
    for key in ('classId', 'jobId', 'producerBond'):
        native_require(payload.get(key) == expected[key], 'native response substitutes ' + key)
    require(u64(payload.get('tipDaa'), 'claim native DAA') == u64(dag['virtualDaaScore'], 'bracket native DAA'), 'claim came from another native DAA')
    native_require(type(payload.get('convicted')) is bool and payload['convicted'] == expected['expectedConvicted'], 'native conviction differs from expected scenario')
    record_digests = {}
    for key in ('publicRecord', 'recordHeader'):
        raw = payload.get(key)
        require(isinstance(raw, str) and raw and len(raw) % 2 == 0 and len(raw) // 2 <= expected['maximumRecordBytes'], 'bounded native record absent')
        hash_text(raw, len(raw), key)
        digest = hashlib.sha256()
        for offset in range(0, len(raw), 128 * 1024):
            digest.update(bytes.fromhex(raw[offset:offset + 128 * 1024]))
        record_digests[key] = dict(bytes=len(raw) // 2, sha256=digest.hexdigest())
    roots = {key: hash_text(payload.get(key), 128, key) for key in ('ledgerRoot', 'auxRoot')}
    return dict(id=requirement['id'], kind=requirement['kind'], role=requirement['role'], claimId=requirement['claimId'],
        status='PASS', actualNativeClaimObserved=True, roots=roots, records=record_digests,
        selectedVerifierApproval='UNKNOWN', settlement='UNKNOWN', publicCertificate=False)


def final_status(scenarios, gates, base_passed, resources_passed, fatal=None):
    if fatal is not None or any(s.get('status') == 'FAIL' for s in scenarios):
        return 'FAIL'
    if not base_passed or not resources_passed or not scenarios or not gates or any(
        row.get('status') != 'PASS' for row in scenarios + list(gates.values())):
        return 'UNKNOWN'
    return 'PASS'


def make_campaign_drill(base):
    class CampaignDrill(base.MeasuredDrill):
        def __init__(self, frozen, root):
            # Keep the original frozen object separate; base kernel bootstrap may
            # own its own copied execution configuration, never the freeze.
            super().__init__(json.loads(frozen.drill_raw), root, frozen.limits)
            self.frozen = frozen
            self.stage_records, self.scenario_results, self.integrity_records = [], [], []
            self.resolved_commands = []
            self.executed_scenarios = set()
            self.root_identity = (str(root.resolve()), root.stat().st_dev, root.stat().st_ino)
            self.db_paths = {role: self.values[role + '_APPDIR'] for role in ROLES}
            self.stage_ordinal = 0
            self.last_contexts = {}
            self.current_stage = None

        def integrity(self, event, full=True):
            self.frozen.validate(full=full, event=event)
            def identity(condition, message):
                if not condition:
                    raise CampaignIdentityError(message)
            identity((str(self.root.resolve()), self.root.stat().st_dev, self.root.stat().st_ino) == self.root_identity, 'owned campaign root replaced')
            identity(all(self.values[role + '_APPDIR'] == path for role, path in self.db_paths.items()), 'scenario changed an owned DB path')
            for role, process in self.processes.items():
                identity(process.args[0] == self.frozen.candidate.path, 'owned live process launched another executable')
            self.integrity_records.append(dict(event=event, monotonicSeconds=time.monotonic(),
                candidateSha256=self.frozen.candidate.sha256, canonicalPlanSha256=self.frozen.identity,
                ownedRoot=self.root_identity[0], ownedDatabases=self.db_paths.copy(),
                children={role: dict(pid=process.pid, returnCode=process.poll()) for role, process in self.processes.items()},
                lastObservedContexts=self.last_contexts.copy(), contextsAreFreshAuthority=False))

        def start(self, role):
            self.integrity('launch.before.' + role)
            super().start(role)
            self.last_contexts.pop(role, None)
            self.integrity('launch.after.' + role)

        def command(self, argv, timeout=None):
            self.integrity('command.before')
            allowed = []
            for template in self.frozen.templates:
                try:
                    allowed.append(self.argv(template))
                except KeyError:
                    # Later hooks such as pruning acquire OLD_BLOCK only from
                    # their actual earlier native observation, never stdout.
                    continue
            require(argv in allowed, 'command was not in the frozen scenario/base plan')
            command_record = dict(stage=self.current_stage, resolvedArgvSha256=hashlib.sha256(canonical(argv)).hexdigest(),
                completed=False, fixtureOutputIsNativeAuthority=False, sequence=len(self.resolved_commands) + 1)
            self.resolved_commands.append(command_record)
            try:
                result = super().command(argv, timeout)
                command_record['completed'] = True
                return result  # always empty; fixture stdout is not native evidence
            finally:
                self.integrity('command.after')

        def assert_network(self, observation):
            expected = self.frozen.plan['network']
            if observation['node']['genesisHash'] != expected['genesisHash'] or observation['node']['consensusParamsId'] != expected['consensusParamsId']:
                raise CampaignIdentityError('owned node changed network/genesis/params')
            epoch, point, phase, stable = base.import_stamp(observation['node'])
            base.probe_context(observation['dag'], observation['node'])
            # A fresh JOIN is allowed to report real import progress. Only
            # native authority evidence needs stable/completed import below.
            return stable is True and point == '' and phase in ('idle', 'legacy-stable')

        def probe(self, role, old=None):
            self.integrity('probe.before.' + role, full=False)
            value = super().probe(role, old)
            self.assert_network(value)
            self.last_contexts[role] = dict(sink=value['dag']['sink'], pruningPoint=value['dag']['pruningPointHash'],
                daa=value['dag']['virtualDaaScore'], genesis=value['node']['genesisHash'], params=value['node']['consensusParamsId'],
                importStamp=base.import_stamp(value['node']), stage=self.current_stage,
                launch=self.node_launches[role][-1] if self.node_launches[role] else None)
            self.integrity('probe.after.' + role, full=False)
            return value

        def fixture(self, name):
            self.stage('fixture.' + name + '.before')
            super().fixture(name)
            self.stage('fixture.' + name + '.after')

        def kernel_checkpoint(self, label, roles, require_present=True):
            result = super().kernel_checkpoint(label, roles, require_present)
            self.stage('milestone.' + label)
            return result

        def stage(self, name):
            require(name in STAGES, 'base drill produced an undeclared stage')
            self.integrity('stage.before.' + name)
            self.stage_ordinal += 1
            self.current_stage = name
            rows = [scenario for scenario in self.frozen.plan['scenarios'] if scenario['stage'] == name]
            contexts = {}
            for role, process in self.processes.items():
                if process.poll() is None:
                    observation = self.probe(role)
                    contexts[role] = base.probe_context(observation['dag'], observation['node'])
            stage_record = dict(stage=name, ordinal=self.stage_ordinal, scenarioIds=[s['id'] for s in rows],
                liveRoles=sorted(self.processes), ownedRoot=str(self.root), before=contexts)
            self.stage_records.append(stage_record)
            for scenario in rows:
                self.scenario(scenario)
            stage_record['after'] = {}
            for role, process in self.processes.items():
                if process.poll() is None:
                    observation = self.probe(role)
                    stage_record['after'][role] = base.probe_context(observation['dag'], observation['node'])
            self.integrity('stage.after.' + name)

        def fact(self, requirement):
            if requirement['kind'] in paid_observations.PAID_KINDS:
                role = requirement['role']
                # Unconfigured legacy plans retain UNKNOWN without opening a
                # file or treating fixture/Final/receipt-count JSON as authority.
                if requirement['claimId'] not in paid_observations.claims(self.plan['nodes'][role]['argv']):
                    return unknown_fact(requirement, 'exact paid claim observation flag is not enabled for this owned role')
                try:
                    observed = paid_observations.observe(self, role, requirement['claimId'], base.import_stamp,
                        base.probe_context, self.frozen.plan['network']['consensusParamsId'], self.frozen.plan['network']['genesisHash'])
                    return paid_observations.fact(requirement, observed)
                except (ValueError, OSError, KeyError, TypeError, RuntimeError) as error:
                    return unknown_fact(requirement, 'current own paid observation unavailable: ' + str(error))
            if requirement['kind'] != 'native-claim-observed':
                return unknown_fact(requirement, 'no reviewed native collector exists for this exact capability; old RPC/Final/WAL/fixture output cannot substitute')
            role = requirement['role']
            rpc = self.rpc(role)
            try:
                before, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
                stamp = base.probe_context(before, node)
                if not self.assert_network(dict(dag=before, node=node)):
                    return unknown_fact(requirement, 'native import is still in progress; no stable authority observation yet')
                payload = rpc.call('getPalwKernelClaim', {'claimId': requirement['claimId']})
                after, after_node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
                stable_after = self.assert_network(dict(dag=after, node=after_node))
                if not stable_after or stamp != base.probe_context(after, after_node):
                    return unknown_fact(requirement, 'native fact crossed a changing sink/import context; retry within the frozen observation deadline')
            finally:
                rpc.close()
            result = native_claim_fact(requirement, payload, before)
            result['context'] = dict(sink=before['sink'], pruningPoint=before['pruningPointHash'], daa=before['virtualDaaScore'],
                genesis=node['genesisHash'], params=node['consensusParamsId'], importStamp=base.import_stamp(node))
            return result

        def scenario(self, scenario):
            require(scenario['id'] not in self.executed_scenarios, 'scenario attempted twice in the same campaign')
            self.executed_scenarios.add(scenario['id'])
            record = dict(id=scenario['id'], kind=scenario['kind'], stage=self.current_stage, claims=scenario['claims'], status='UNKNOWN', facts=[])
            self.scenario_results.append(record)
            began = time.monotonic()
            try:
                require(all(role in self.processes and self.processes[role].poll() is None for role in scenario['roles']), 'scenario lacks its actual live nodes')
                record['before'] = {}
                for role in scenario['roles']:
                    observation = self.probe(role)
                    record['before'][role] = base.probe_context(observation['dag'], observation['node'])
                for command in scenario['commands']:
                    self.command(self.argv(command))
                for requirement in scenario['requirements']:
                    try:
                        deadline = time.monotonic() + min(scenario['observationTimeoutSeconds'], self.remaining())
                        while True:
                            fact = self.fact(requirement)
                            if fact['status'] != 'UNKNOWN' or requirement['kind'] not in (
                                    'native-claim-observed', 'native-paid-observed', 'native-paid-assigned-quorum') or time.monotonic() >= deadline:
                                break
                            self.remaining()
                            time.sleep(.2)
                        record['facts'].append(fact)
                    except CampaignIdentityError:
                        raise
                    except NativeMismatch as error:
                        fact = unknown_fact(requirement, str(error))
                        fact['status'] = 'FAIL'
                        record['facts'].append(fact)
                    except Exception as error:
                        record['facts'].append(unknown_fact(requirement, type(error).__name__ + ': ' + str(error)))
                record['after'] = {}
                for role in scenario['roles']:
                    observation = self.probe(role)
                    record['after'][role] = base.probe_context(observation['dag'], observation['node'])
                if scenario['kind'] == 'normal':
                    for claim in scenario['claims']:
                        evidence = [fact['records'] for fact in record['facts'] if fact['claimId'] == claim
                            and fact['kind'] == 'native-claim-observed' and fact['status'] == 'PASS']
                        if evidence and any(row != evidence[0] for row in evidence):
                            record.update(status='FAIL', reason='actual nodes disagree on the immutable record of the same claim')
                if record['status'] != 'FAIL':
                    record['status'] = 'FAIL' if any(fact['status'] == 'FAIL' for fact in record['facts']) else (
                        'PASS' if all(fact['status'] == 'PASS' for fact in record['facts']) else 'UNKNOWN')
            except (base.Blocked, TimeoutError, OSError) as error:
                record['reason'] = 'scenario unavailable/timeout is not a defensive success: ' + str(error)
            except CampaignError:
                raise  # identity/plan drift aborts the campaign, never a retry
            except Exception as error:
                record.update(status='FAIL', reason=type(error).__name__ + ': ' + str(error))
            finally:
                record['elapsedSeconds'] = time.monotonic() - began
                self.integrity('scenario.after.' + scenario['id'])

    return CampaignDrill
