#!/usr/bin/env python3
"""Run real private-node fork/partition/pruned-join/restart drills from a pinned plan.

The plan supplies a compatible private binary, finite signed-work fixture commands,
policy/ruleset IDs, and acceptance limits BEFORE execution. This never rewrites
consensus parameters or opens a production retirement fence. Today's shipped
presets produce BLOCKED: native retirement is dormant, not a successful drill.

Each role owns a separate process and fresh DB. Only ordinary P2P bytes cross the
loopback links; no header/body/state/DB files are copied between roles. The JSON
probe uses the repository's dependency-free WsRpc client against real public wRPC.
ETH latest/safe/finalized are read separately and must match that native generation.
This measures scenario mechanics, not soundness/common-prefix/security approval.
"""
import argparse
import hashlib
import json
from pathlib import Path
import selectors
import socket
import subprocess
import tempfile
import threading
import time
import urllib.request
from misaka_wrpc_json import WsRpc
from rfc12_owned_resources import OwnedResourceSampler, ResourceLimitExceeded


class Blocked(RuntimeError):
    pass


def read_process_limits(path):
    limits = json.loads(Path(path).read_bytes())
    fields = {'schemaVersion', 'kind', 'maximumOwnedProcessRssKiB', 'maximumOwnedDatabaseLogicalBytes',
        'maximumProbeMs', 'samplePeriodMs', 'maximumFixtureOutputBytes'}
    if set(limits) != fields or type(limits['schemaVersion']) is not int or limits['schemaVersion'] != 1 \
            or limits['kind'] != 'rfc12-real-process-observation-bounds':
        raise ValueError('real-process limits require their exact version-1 observation schema')
    for field in fields - {'schemaVersion', 'kind'}:
        if type(limits[field]) is not int or limits[field] <= 0:
            raise ValueError(field + ' must be a positive integer fixed before launch')
    if not 100 <= limits['samplePeriodMs'] <= 5000:
        raise ValueError('samplePeriodMs must be between 100 and 5000')
    return limits


def normalized(value):
    return value.removeprefix('0x') if isinstance(value, str) else value


def import_stamp(node):
    if node.get('importSessionAvailable') is not True:
        raise Blocked('this node predates durable import-session diagnostics')
    if type(node.get('importSessionEpoch')) is not int or not 0 <= node['importSessionEpoch'] < 2 ** 64:
        raise AssertionError('durable import epoch is not an exact unsigned counter')
    return (node['importSessionEpoch'], node['importSessionPoint'], node['importSessionPhase'], node['pruningImportStable'])


def decimal_u128(value):
    # Native work is serialized as decimal text. A float, exponent, sign or
    # coerced boolean cannot serve as exact evidence above JavaScript's 2^53.
    if not isinstance(value, str) or not value or any(c < '0' or c > '9' for c in value) \
            or (len(value) > 1 and value[0] == '0') or len(value) > 39:
        raise AssertionError('native useful work must be canonical u128 decimal text')
    number = int(value)
    if number >= 2 ** 128:
        raise AssertionError('native useful work exceeds u128')
    return number


def exact_u64(value, name, minimum=0):
    if type(value) is not int or not minimum <= value < 2**64:
        raise AssertionError(name + ' must be an exact u64 at or above ' + str(minimum))
    return value


def validate_scenario_bounds(plan):
    """Freeze meaningful exact thresholds before starting any child process."""
    exact_u64(plan['minimum_isolated_daa_span'], 'minimum_isolated_daa_span', 5400)
    exact_u64(plan['minimum_claims'], 'minimum_claims', 100_000)
    exact_u64(plan['minimum_depth'], 'minimum_depth', 1)
    if decimal_u128(plan['minimum_work']) == 0:
        raise AssertionError('minimum_work must be positive canonical u128 decimal text')
    exact_u64(plan['maximum_probe_ms'], 'maximum_probe_ms', 1)
    exact_u64(plan['workflow_timeout_s'], 'workflow_timeout_s', 1)
    for key in ('scenario_timeout_s', 'command_timeout_s', 'probe_timeout_s'):
        if key in plan:
            exact_u64(plan[key], key, 1)


def coherent(observation, plan, require_work=True, require_finalized=False):
    settlement = observation.get('settlement') or {}
    native = settlement.get('nativeSettlement')
    if settlement.get('dnsRetiredAt') is None or native is None:
        raise Blocked('the binary exposes no active native retirement; no fence is armed by this driver')
    if observation['dns']['available']:
        raise AssertionError('a retired node still exposes DNS finality authority')
    assert normalized(native['generation']) == normalized(observation['dag']['sink']), 'mixed native/L1 generation'
    assert normalized(native['rulesetId']) == normalized(plan['ruleset_id']), 'unpinned consensus rules'
    assert normalized(native['policyId']) == normalized(plan['policy_id']), 'unpinned D/W/concentration policy'
    assert normalized(observation['node']['consensusParamsId']) == normalized(plan['ruleset_id'])
    epoch, import_point, phase, stable = import_stamp(observation['node'])
    assert stable is True and import_point == '' and phase in ('idle', 'legacy-stable'), 'import is still incomplete or active'
    readiness = settlement.get('nativeReadiness')
    assert readiness is not None, 'native readiness missing at the observed stable generation'
    assert normalized(readiness['generation']) == normalized(native['generation'])
    assert readiness['safe'] == native['safe'] and readiness['finalized']['finalized'] == native['finalized']
    if require_work:
        assert native['safe'] is not None, 'no genuine useful-work covered Safe prefix'
        assert decimal_u128(native['uniqueWork']) >= decimal_u128(plan['minimum_work']), 'unique mature work below the predeclared bound'
        assert exact_u64(native['depth'], 'native anchor depth') >= exact_u64(plan['minimum_depth'], 'minimum_depth', 1), 'Final anchor depth below the predeclared bound'
    if require_finalized:
        assert native['finalized'] is not None, 'pruned join/restart has no native Finalized head'
        pruning = normalized(observation['dag']['pruningPointHash'])
        assert pruning != normalized(observation['node']['genesisHash']), 'pruned finality still names genesis'
        assert normalized(native['finalized']) == pruning, 'native Finalized does not authenticate the actual pruning checkpoint'
    return native


def old_body_is_available(rpc, old):
    """GetBlock deliberately serves header-only blocks; RPC success is not a body."""
    try:
        block = rpc.call('getBlock', {'hash': old, 'includeTransactions': True})['block']
    except RuntimeError as error:
        # Full header removal is valid GC too. A transport error, timeout, busy
        # server or unrelated RPC failure is never pruning evidence.
        if 'cannot find header ' + normalized(old) in str(error):
            return False
        raise
    verbose = block.get('verboseData') or {}
    assert isinstance(verbose.get('isHeaderOnly'), bool), 'old-block response omitted explicit body status'
    if verbose['isHeaderOnly']:
        assert not block['transactions'], 'header-only status contradicts a served body'
        return False
    assert block['transactions'], 'a full native block must contain its coinbase'
    return True


class Link:
    """Opaque loopback P2P link; cut closes only sockets created by this link."""
    def __init__(self, target):
        self.target = target
        self.enabled = threading.Event()
        self.closed = threading.Event()
        self.lock = threading.Lock()
        self.connections = set()
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1', 0))
        self.listener.listen()
        self.listener.settimeout(.2)
        self.address = '127.0.0.1:%d' % self.listener.getsockname()[1]
        self.thread = threading.Thread(target=self.accept, daemon=True)
        self.thread.start()

    def accept(self):
        while not self.closed.is_set():
            try:
                incoming, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            if not self.enabled.is_set():
                incoming.close()
                continue
            threading.Thread(target=self.forward, args=(incoming,), daemon=True).start()

    def forward(self, incoming):
        outgoing = None
        try:
            outgoing = socket.create_connection(self.target, timeout=5)
            with self.lock:
                if not self.enabled.is_set():
                    return
                self.connections.update((incoming, outgoing))
            with selectors.DefaultSelector() as poll:
                poll.register(incoming, selectors.EVENT_READ, outgoing)
                poll.register(outgoing, selectors.EVENT_READ, incoming)
                while self.enabled.is_set() and not self.closed.is_set():
                    for event, _ in poll.select(.2):
                        data = event.fileobj.recv(65536)
                        if not data:
                            return
                        event.data.sendall(data)
        except OSError:
            pass
        finally:
            with self.lock:
                self.connections.discard(incoming)
                self.connections.discard(outgoing)
            incoming.close()
            if outgoing:
                outgoing.close()

    def cut(self):
        self.enabled.clear()
        with self.lock:
            for conn in list(self.connections):
                try:
                    conn.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                conn.close()

    def close(self):
        self.cut()
        self.closed.set()
        self.listener.close()
        self.thread.join(timeout=2)


class Drill:
    def __init__(self, plan, root):
        self.plan, self.root = plan, root
        self.processes, self.logs, self.links, self.observations = {}, {}, [], []
        self.values = {'ROOT': str(root)}
        for role, node in plan['nodes'].items():
            self.values[role + '_APPDIR'] = str(root / role / 'appdir')
            self.values[role + '_WRPC'] = node['wrpc']
            if 'grpc' in node:
                self.values[role + '_GRPC'] = node['grpc']
        self.latencies = []

    def argv(self, template):
        assert isinstance(template, list) and template and all(isinstance(x, str) for x in template), 'commands must be argv lists'
        return [x.format_map(self.values) for x in template]

    def command(self, argv, timeout=None):
        return subprocess.run(argv, check=True, capture_output=True, text=True,
                              timeout=timeout or self.plan.get('command_timeout_s', 60)).stdout.strip()

    def start(self, role):
        folder = self.root / role
        folder.mkdir(exist_ok=True)
        log = open(folder / ('node-%d.log' % len(self.logs)), 'w')
        argv = self.argv(self.plan['nodes'][role]['argv'])
        assert '--nodnsseed' in argv and '--outpeers=0' in argv, 'all peer discovery must be controlled by the loopback links'
        assert not any(x.startswith(('--connect', '--addpeer')) for x in argv), 'use driver-owned links, not an unobserved peer route'
        assert self.values[role + '_APPDIR'] in ' '.join(argv), 'each node must use its own owned appdir'
        self.logs[role] = log
        self.processes[role] = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)

    def kill(self, role):
        process = self.processes.pop(role)
        process.kill()  # own child: actual OS kill, never graceful RPC/destructor shutdown
        process.wait(timeout=20)
        self.logs[role].close()

    def rpc(self, role):
        host, port = self.plan['nodes'][role]['wrpc'].split(':')
        return WsRpc(host=host, port=int(port), timeout=self.plan.get('probe_timeout_s', 30))

    def probe(self, role, old=None):
        began = time.monotonic()
        rpc = self.rpc(role)
        try:
            dag = rpc.call('getBlockDagInfo')
            block = rpc.call('getBlock', {'hash': dag['sink'], 'includeTransactions': False})['block']
            node = rpc.call('getPalwNodeStatus')
            dns = rpc.call('getDnsConfirmation')
            try:
                settlement = rpc.call('getPalwSettlement', {'daaScore': dag['virtualDaaScore']})
                settlement_error = None
            except RuntimeError as error:
                settlement, settlement_error = None, str(error)
            peers = rpc.call('getConnectedPeerInfo')
            old_body_available = None
            if old:
                old_body_available = old_body_is_available(rpc, old)
            after = rpc.call('getBlockDagInfo')
            after_node = rpc.call('getPalwNodeStatus')
            assert import_stamp(after_node) == import_stamp(node), 'import session changed during the probe; retry'
            assert (after['sink'], after['pruningPointHash']) == (dag['sink'], dag['pruningPointHash']), 'context changed; retry'
        finally:
            rpc.close()
        # Capacity shadow is intentionally a final query on its own connection.
        rpc = self.rpc(role)
        try:
            shadow = rpc.call('getPalwCapacityShadow', {'steps': [], 'bond': '', 'adversaryBonds': [], 'includeClaims': True, 'limit': 1})
            shadow_error = None
        except RuntimeError as error:
            shadow, shadow_error = None, str(error)
        finally:
            rpc.close()
        rpc = self.rpc(role)
        try:
            after_shadow = rpc.call('getBlockDagInfo')
            after_shadow_node = rpc.call('getPalwNodeStatus')
            assert import_stamp(after_shadow_node) == import_stamp(node), 'import session changed during the capacity read; retry'
            assert (after_shadow['sink'], after_shadow['pruningPointHash']) == (dag['sink'], dag['pruningPointHash']), 'capacity context changed; retry'
        finally:
            rpc.close()
        value = {'schemaVersion': 1, 'dag': dag, 'header': block['header'], 'node': node, 'dns': dns,
                 'settlement': settlement, 'settlementError': settlement_error, 'peers': peers,
                 'oldBodyAvailable': old_body_available, 'shadow': shadow, 'shadowError': shadow_error}
        self.latencies.append((time.monotonic() - began) * 1000)
        self.observations.append({'role': role, 'observation': value})
        return value

    def wait(self, predicate, label):
        deadline = time.monotonic() + self.plan.get('scenario_timeout_s', 300)
        last = None
        while time.monotonic() < deadline:
            try:
                for role, process in self.processes.items():
                    if process.poll() is not None:
                        raise Blocked('%s process exited %s; inspect its preserved log' % (role, process.returncode))
                result = predicate()
                if result:
                    for role, process in self.processes.items():
                        if process.poll() is not None:
                            raise Blocked('%s process exited during observation' % role)
                    return result
            except (subprocess.SubprocessError, OSError, ValueError, AssertionError) as error:
                last = str(error)
            for role, process in self.processes.items():
                if process.poll() is not None:
                    raise Blocked('%s process exited %s; inspect its preserved log' % (role, process.returncode))
            time.sleep(.5)
        raise AssertionError('%s timed out: %s' % (label, last))

    def fixture(self, name):
        for argv in self.plan['fixtures'][name]:
            self.command(self.argv(argv))

    def join_link(self, role, target):
        host, port = self.plan['nodes'][target]['p2p'].split(':')
        assert host == '127.0.0.1', 'P2P fixture peers must be loopback'
        link = Link((host, int(port)))
        self.links.append(link)
        link.enabled.set()
        rpc = self.rpc(role)
        try:
            peer_ip, peer_port = link.address.rsplit(':', 1)
            # JSON wRPC uses ContextualNetAddress's native serde object, while
            # the command-line flags and link logs use the socket string.
            rpc.call('addPeer', {'peerAddress': {'ip': peer_ip, 'port': int(peer_port)}, 'isPermanent': True})
        finally:
            rpc.close()
        return link

    def convergence(self, roles, expected, require_finalized=False):
        observed = [self.probe(role) for role in roles]
        natives = [coherent(row, self.plan, require_finalized=require_finalized) for row in observed]
        assert all(row['dag']['sink'] == expected for row in observed), 'fork selection disagrees with the predeclared winner'
        keys = [(row['header'], native) for row, native in zip(observed, natives)]
        assert all(key == keys[0] for key in keys), 'UTXO/PALW/EVM commitment roots or native heads disagree'
        eth = []
        for role, native, original in zip(roles, natives, observed):
            tags = []
            for tag in ('latest', 'safe', 'finalized'):
                request = urllib.request.Request(self.plan['nodes'][role]['eth'],
                    json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'eth_getBlockByNumber', 'params': [tag, False]}).encode(),
                    {'Content-Type': 'application/json'})
                with urllib.request.urlopen(request, timeout=10) as response:
                    answer = json.load(response)
                assert 'error' not in answer, 'ETH tag read failed: %r' % answer
                block = answer['result']
                expected_hash = normalized(native[tag])[:64] if native[tag] else None
                assert normalized(block['hash'] if block else None) == expected_hash, 'ETH/native prescribed 32-byte hash projection mismatch'
                tags.append(block)
            rpc = self.rpc(role)
            try:
                after_tags = rpc.call('getBlockDagInfo')
                after_tags_node = rpc.call('getPalwNodeStatus')
                assert (after_tags['sink'], after_tags['pruningPointHash']) == (original['dag']['sink'], original['dag']['pruningPointHash']), 'context changed during ETH tag reads; retry'
                assert import_stamp(after_tags_node) == import_stamp(original['node']), 'import changed during ETH tag reads; retry'
            finally:
                rpc.close()
            eth.append(tags)
        assert all(tags == eth[0] for tags in eth), 'executed EVM state roots disagree across processes'
        return observed

    def run(self):
        for role in ('A', 'B'):
            self.start(role)
        for role in ('A', 'B'):
            self.wait(lambda role=role: self.probe(role), role + ' RPC startup')
        self.fixture('bootstrap')
        for role in ('A', 'B'):
            coherent(self.probe(role), self.plan, require_work=False)
        isolated_start = {role: self.probe(role)['dag']['virtualDaaScore'] for role in ('A', 'B')}
        self.fixture('fork')
        separate = {role: self.probe(role) for role in ('A', 'B')}
        for role in ('A', 'B'):
            coherent(separate[role], self.plan)
            assert separate[role]['dag']['virtualDaaScore'] - isolated_start[role] >= self.plan['minimum_isolated_daa_span'], 'private long-range branch did not reach the predeclared actual DAA horizon'
        assert separate['A']['dag']['sink'] != separate['B']['dag']['sink'], 'fixture did not create separate branches'
        winner = separate[self.plan['fork_winner']]['dag']['sink']
        link = self.join_link('B', 'A')
        self.wait(lambda: self.convergence(('A', 'B'), winner), 'initial fork convergence')
        link.cut()
        self.wait(lambda: all(not self.probe(r)['peers']['peerInfo'] for r in ('A', 'B')), 'real P2P partition')
        self.fixture('partition')
        separate = {role: self.probe(role) for role in ('A', 'B')}
        assert separate['A']['dag']['sink'] != separate['B']['dag']['sink'], 'partition fixture did not diverge'
        winner = separate[self.plan['partition_winner']]['dag']['sink']
        link.enabled.set()
        self.wait(lambda: self.convergence(('A', 'B'), winner), 'partition healing')
        old = self.probe('A')['dag']['sink']
        self.values['OLD_BLOCK'] = old
        self.fixture('prune')
        def pruned_source():
            row = self.probe('A', old)
            coherent(row, self.plan, require_finalized=True)
            return row if row['oldBodyAvailable'] is False else None
        source = self.wait(pruned_source, 'actual old-body garbage collection')
        coherent(source, self.plan, require_finalized=True)
        assert source['dag']['pruningPointHash'] != source['node']['genesisHash'], 'pruning point never moved'
        assert source['oldBodyAvailable'] is False, 'source did not actually GC the old block body'
        winner = source['dag']['sink']
        self.start('JOIN')  # fresh DB, never copied from a source
        self.wait(lambda: self.probe('JOIN'), 'fresh join startup')
        self.join_link('JOIN', 'A')
        self.wait(lambda: self.convergence(('A', 'JOIN'), winner, require_finalized=True), 'actual fresh pruned P2P join')
        joined = self.probe('JOIN', old)
        coherent(joined, self.plan, require_finalized=True)
        assert joined['oldBodyAvailable'] is False, 'join secretly retained full old history'
        joined_epoch = import_stamp(joined['node'])[0]
        assert joined_epoch > 0, 'fresh pruned join did not complete a durable import session'
        self.kill('JOIN')
        self.start('JOIN')  # the same owned DB after a forced process crash
        self.join_link('JOIN', 'A')
        self.wait(lambda: self.convergence(('A', 'JOIN'), winner, require_finalized=True), 'cold process restart agreement')
        row = self.probe('JOIN')
        coherent(row, self.plan, require_finalized=True)
        assert import_stamp(row['node'])[0] == joined_epoch, 'completed import epoch did not survive cold restart'
        count = exact_u64((row.get('shadow') or {}).get('claimsTotal'), 'native capacity claimsTotal')
        assert count >= self.plan['minimum_claims'], '100k-scale fixture was not reached'
        assert max(self.latencies) <= self.plan['maximum_probe_ms'], 'predeclared release probe latency limit exceeded'

    def close(self):
        for link in self.links:
            link.close()
        for role in list(self.processes):
            self.kill(role)


class MeasuredDrill(Drill):
    """Finite real-process run with observed bounds fixed before the first child.

    RSS sampling and logical file bytes are observations, not OS hard quotas.
    Fixture commands remain actual owned children; their output is never imported
    as an authoritative node state, work count or successful settlement verdict.
    """
    def __init__(self, plan, root, limits):
        super().__init__(plan, root)
        if type(plan.get('workflow_timeout_s')) is not int or plan['workflow_timeout_s'] <= 0:
            raise ValueError('workflow_timeout_s must be a positive integer fixed before execution')
        self.deadline = time.monotonic() + plan['workflow_timeout_s']
        self.limits = limits.copy()
        self.sampler = OwnedResourceSampler({}, limits)
        self.command_count, self.command_observations = 0, []
        self.command_outputs = []
        self.node_launches = {role: [] for role in plan['nodes']}

    def remaining(self):
        self.sampler.check()
        left = self.deadline - time.monotonic()
        if left <= 0:
            raise Blocked('the total predeclared real-process workflow deadline expired')
        return left

    def start(self, role):
        self.remaining()
        super().start(role)
        self.sampler.register(role, self.processes[role])
        label = '%s_LAUNCH_%d' % (role, len(self.node_launches[role]) + 1)
        self.node_launches[role].append(label)
        self.sampler.register(label, self.processes[role])
        self.sampler.sample_now()
        self.sampler.check()

    def wait(self, predicate, label):
        def bounded():
            self.remaining()
            value = predicate()
            self.remaining()
            return value
        return super().wait(bounded, label)

    def command(self, argv, timeout=None):
        allowed = min(timeout or self.plan.get('command_timeout_s', 60), self.remaining())
        self.command_count += 1
        name = 'FIXTURE_%06d' % self.command_count
        began = time.monotonic()
        # Persist even large stdout directly; a fixture cannot fill the driver's
        # heap with output or hide its failed stderr behind a later exception.
        stdout_path, stderr_path = self.root / (name + '.stdout'), self.root / (name + '.stderr')
        with stdout_path.open('w') as stdout, stderr_path.open('w') as stderr:
            child = subprocess.Popen(argv, stdout=stdout, stderr=stderr, text=True)
            self.sampler.register(name, child)
            self.command_outputs.extend((stdout_path, stderr_path))
            try:
                while child.poll() is None:
                    if time.monotonic() - began >= allowed:
                        raise Blocked(name + ' exceeded its finite command/workflow deadline')
                    self.remaining()
                    self.check_fixture_output()
                    time.sleep(.1)
                self.sampler.check()
                self.check_fixture_output()
                if child.returncode:
                    raise Blocked('%s exited %s; inspect preserved stderr' % (name, child.returncode))
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=20)
                self.command_observations.append(dict(label=name, returnCode=child.returncode,
                    elapsedSeconds=time.monotonic() - began, stdout=str(stdout_path), stderr=str(stderr_path)))
        # The fixture hook does not consume command output as a node authority.
        return ''

    def check_fixture_output(self):
        if sum(path.stat().st_size for path in self.command_outputs) > self.limits['maximumFixtureOutputBytes']:
            raise ResourceLimitExceeded('fixture output exceeded its predeclared logical byte bound')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--resource-limits', required=True,
        help='predeclared sampled process/physical-file observation limits; never an OS quota')
    args = parser.parse_args()
    raw = Path(args.plan).read_bytes()
    plan = json.loads(raw)
    limits = read_process_limits(args.resource_limits)
    assert set(plan['nodes']) == {'A', 'B', 'JOIN'}
    endpoints = []
    for node in plan['nodes'].values():
        assert node['wrpc'].startswith('127.0.0.1:') and node['p2p'].startswith('127.0.0.1:')
        assert node['eth'].startswith('http://127.0.0.1:')
        endpoints.extend((node['wrpc'], node['p2p'], node['eth'].split('//', 1)[1].rstrip('/')))
    assert len(endpoints) == len(set(endpoints)), 'roles/listeners need distinct owned ports'
    assert plan['fork_winner'] in ('A', 'B') and plan['partition_winner'] in ('A', 'B')
    validate_scenario_bounds(plan)
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc12-p2p-'))
    drill = MeasuredDrill(plan, root, limits)
    result = {'planSha256': hashlib.sha256(raw).hexdigest(), 'artifactDirectory': str(root),
              'status': 'FAIL', 'fullRfc12SecurityApproval': False, 'resourceLimits': limits, 'resourceLimitsMet': False,
              'workflowTimeoutSeconds': plan['workflow_timeout_s']}
    try:
        drill.run()
        result['status'] = 'SCENARIOS_PASS'
    except Blocked as error:
        result.update(status='BLOCKED', reason=str(error))
    except Exception as error:
        result['reason'] = repr(error)
    finally:
        try:
            drill.close()
        finally:
            result['sampledOwnedResources'] = drill.sampler.stop()
        result.update(observations=drill.observations, probeLatenciesMs=drill.latencies)
        result['fixtureCommands'] = drill.command_observations
        result['requiredNodeLaunchSamples'] = drill.node_launches
        result['ownedDatabaseLogicalBytes'] = {role: sum(file.stat().st_size
            for file in (root / role).rglob('*') if file.is_file()) for role in ('A', 'B', 'JOIN')}
        if result['status'] == 'SCENARIOS_PASS':
            try:
                check_process_limits(limits, result)
                result['resourceLimitsMet'] = True
            except ResourceLimitExceeded as error:
                result.update(status='FAIL', reason=str(error))
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'observations'}, indent=2))
    return 0 if result['status'] == 'SCENARIOS_PASS' else 1


def check_process_limits(limits, result):
    sampled = result['sampledOwnedResources']
    if sampled['violation'] or sampled['observationFailures']:
        raise ResourceLimitExceeded(sampled['violation'] or 'owned process sampling failed')
    for role in ('A', 'B', 'JOIN'):
        if sampled['ownedChildren'].get(role, {}).get('samples', 0) <= 0:
            raise ResourceLimitExceeded('no owned RSS observation for ' + role)
        launches = result['requiredNodeLaunchSamples'].get(role, [])
        if not launches or any(sampled['ownedChildren'].get(label, {}).get('samples', 0) <= 0 for label in launches):
            raise ResourceLimitExceeded('an owned node launch/reopen has no RSS observation for ' + role)
    if not result['probeLatenciesMs'] or max(result['probeLatenciesMs']) > limits['maximumProbeMs']:
        raise ResourceLimitExceeded('public RPC latency exceeded the fixed bound')
    if set(result['ownedDatabaseLogicalBytes']) != {'A', 'B', 'JOIN'} or any(
            value > limits['maximumOwnedDatabaseLogicalBytes'] for value in result['ownedDatabaseLogicalBytes'].values()):
        raise ResourceLimitExceeded('owned file measurements are incomplete or exceed the fixed bound')


if __name__ == '__main__':
    raise SystemExit(main())
