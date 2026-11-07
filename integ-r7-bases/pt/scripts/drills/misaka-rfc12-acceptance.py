#!/usr/bin/env python3
"""RFC-0012 multi-process acceptance: one continuous real useful-work scenario from a pinned plan.

One owned private paid-v4 network (producer A, provider B, checker JOIN, later a
fresh joiner FRESH and a crash joiner CRASHJOIN) runs, in order: registration
and native custody bootstrap; real kernel paid work (request -> offline export ->
--private-probe-kernel-paid-work producer -> provider custody -> assigned
--private-probe-kernel-paid-check checker receipt -> Final -> native safe);
bridge deposit/claim/refund/withdrawal; a market fill; a long fork; a partition;
local-capacity refusal and recovery; the 100k scale fixture; pruning; a fresh
pruned P2P join; a cold reopen of the same DB; a wrong-profile reopen; legacy
DNS reconciliation; injected SIGKILL points throughout. Every public view is
read between identical generation stamps and compared across processes.

Verdicts come only from rfc12_acceptance_judgement: OBSERVED, BLOCKED, FAIL or
NOT_RUN. There is no PASS. A preset network, a profile that cannot arm
palw_dns_retirement_v1, a missing node read or a missing operator command yields
BLOCKED. Plan values are operator inputs fixed before launch; nothing here
invents a release threshold, changes a preset, or touches a public network.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_acceptance_observe as O  # noqa: E402
import rfc12_acceptance_plan as P  # noqa: E402
import rfc12_complete_bootstrap as complete  # noqa: E402
import rfc12_retirement_reconciliation as R  # noqa: E402
from rfc12_owned_resources import OwnedResourceSampler, ResourceLimitExceeded  # noqa: E402


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bootstrap = _load('rfc12_acceptance_bootstrap', 'misaka-rfc12-private-bootstrap.py')
transport = bootstrap.transport
smoke = bootstrap.smoke

EVIDENCE_KIND = 'rfc12-acceptance-evidence'
EVIDENCE_VERSION = 1
PRIMARY = ('A', 'B', 'JOIN')
# The checkboxes this driver owns; checkbox 8 (RFC-0008 costs) belongs to misaka-rfc08-cost-measure.py.
BOXES = (1, 2, 3, 4, 5, 6, 7, 9)
SOURCE_OF = {'B': 'A', 'JOIN': 'A'}
MARKER = 'rfc12-private-probe-v1.json'
# Implementation locations are recorded separately from runtime evidence. A
# compiled capability is not an observed acceptance result; missing/old binaries
# still produce explicit BLOCKED findings at the corresponding read/step.
NODE_GAPS = [
    dict(read='DNS security reserve balance per block', proposed='getPalwAcceptanceReads (op 246)',
         implementation='get_palw_acceptance_reads_call', runtimeObserved=False,
         consumers=['reserve reconciliation; independent accrual/drip evidence still required']),
    dict(read='in-process crash failpoints between import WAL barriers', proposed='private-integration-config build feature',
         implementation='exact owned import-wal rendezvous', runtimeObserved=False,
         consumers=['checkbox 3: one of eight validation rendezvous stages per bounded run']),
    dict(read='first-party paid-work request from a live authenticated snapshot', proposed='private node request generator',
         implementation='--private-probe-generate-paid-request', runtimeObserved=False,
         consumers=['usefulWork.requestCommand with explicit pinned block/spec/inputs']),
    dict(read='virtual UTXO point read by outpoint', proposed='getPalwAcceptanceReads (op 246)',
         implementation='generation-locked point lookup', runtimeObserved=False,
         consumers=['original deposit/refund inputs and named refund/withdrawal outputs']),
]



class WaitTimeout(Exception):
    """A required event did not happen within the plan's predeclared bound (FAIL)."""


class ProcessExited(transport.Blocked):
    pass


class AcceptanceDrill(bootstrap.FiniteBootstrapDrill):
    """Owned child processes with one log per launch and recorded loopback links."""

    def __init__(self, plan, root, sampler):
        super().__init__(plan, root, sampler, {})
        self.launch_count, self.launches, self.log_paths, self.link_roles = {}, [], {}, []
        self.import_wal = {}

    def start(self, role, extra=()):
        self.sampler.check()
        folder = self.root / role
        folder.mkdir(exist_ok=True)
        argv = self.argv(self.plan['nodes'][role]['argv']) + list(extra)
        assert '--nodnsseed' in argv and '--outpeers=0' in argv, 'all peer discovery must be controlled by owned links'
        assert not any(x.startswith(('--connect', '--addpeer')) for x in argv), 'use driver-owned links only'
        assert self.values[role + '_APPDIR'] in ' '.join(argv), 'each node must use its own owned appdir'
        count = self.launch_count.get(role, 0) + 1
        path = folder / ('node-%s-%d.log' % (role, count))
        log = open(path, 'w')
        self.logs[role] = log
        env = os.environ.copy()
        for name in ('MISAKA_RFC12_IMPORT_CRASH_STAGE', 'MISAKA_RFC12_IMPORT_CRASH_DIRECTORY'):
            env.pop(name, None)
        if role in self.import_wal:
            rendezvous = folder / 'import-wal'
            rendezvous.mkdir(exist_ok=True)
            for name in ('ready', 'armed'):
                (rendezvous / name).unlink(missing_ok=True)
            env.update(MISAKA_RFC12_IMPORT_CRASH_STAGE=self.import_wal[role],
                       MISAKA_RFC12_IMPORT_CRASH_DIRECTORY=str(rendezvous))
        self.processes[role] = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT, env=env)
        self.launch_count[role] = count
        self.sampler.register(role, self.processes[role])
        self.sampler.register('%s_LAUNCH_%d' % (role, count), self.processes[role])
        self.log_paths.setdefault(role, []).append(str(path))
        self.launches.append(dict(role=role, launch=count, argv=argv, log=str(path), pid=self.processes[role].pid,
                                  startedUnix=time.time()))

    def join_link(self, role, target):
        link = super().join_link(role, target)
        self.link_roles.append((role, target, link))
        return link


class AcceptanceRun:
    def __init__(self, plan, root, limits, out):
        self.plan, self.root, self.limits, self.out = plan, root, limits, out
        self.bounds, self.fees = plan['bounds'], plan['fees']
        self.deadline = time.monotonic() + self.bounds['workflowTimeoutSeconds']
        self.sampler = OwnedResourceSampler({}, limits)
        self.drill = self.manifest = self.fixture = self.genesis = self.old_block = self.request_block = None
        self.flags, self.findings, self.phases, self.commands = set(), [], [], []
        self.rows, self.consumer_rows, self.import_rows, self.settlement_rows, self.bridge_rows = [], [], [], [], []
        self.cli_records, self.amounts, self.payer_start, self.diagnostics = [], {}, {}, []
        self.last_finalized, self.moved_sink, self.crash_records, self.checker_bond_of = {}, {}, {}, {}
        self.armed, self.in_crash, self.expected_exit, self.counter = [], False, set(), 0
        self.work, self.bridge, self.accounting, self.identity_facts, self.reconciliation = {}, {}, {}, {}, {}
        self.stopped, self.last_log_scan, self.current_work_dir = None, 0.0, None
        # Checkbox numbers the shared steps report under; a subclass driver owning other boxes overrides them.
        self.boxes, self.identity_box, self.work_box, self.resource_box = BOXES, 1, 1, 2
        self.evidence = dict(kind=EVIDENCE_KIND, schemaVersion=EVIDENCE_VERSION, status=J.BLOCKED,
                             artifactDirectory=str(root), planSha256=plan['sha256'], plan=plan, nodeGaps=NODE_GAPS,
                             passLabelUsed=False, releaseApproval=False, fullRfc12SecurityApproval=False)

    # ---- bookkeeping ------------------------------------------------------------------------

    def label(self, name):
        self.counter += 1
        return '%s_%04d' % (name, self.counter)

    def judge(self, box, predicate, function, *args, **kwargs):
        item = J.judge(box, predicate, function, *args, **kwargs)
        self.findings.append(item)
        return item['facts'] if item['verdict'] == J.OBSERVED else None

    def block(self, box, predicate, reason):
        self.findings.append(J.finding(box, predicate, J.BLOCKED, reason))

    def remaining(self):
        self.sampler.check()
        left = self.deadline - time.monotonic()
        if left <= 0:
            raise transport.Blocked('the predeclared total workflow deadline expired')
        return left

    def check_processes(self):
        if not self.drill:
            return
        for role, process in list(self.drill.processes.items()):
            if process.poll() is not None and role not in self.expected_exit:
                raise ProcessExited('%s exited %s unexpectedly; see %s' % (role, process.returncode,
                                                                           self.drill.log_paths.get(role, ['?'])[-1]))

    def wait(self, predicate, label, seconds=None, optional=False):
        seconds = seconds or self.bounds['phaseTimeoutSeconds']
        deadline = min(time.monotonic() + seconds, self.deadline)
        last = None
        while True:
            self.remaining()
            self.check_processes()
            self.poll_crash_points()
            self.tick()
            try:
                value = predicate()
                if value is not None and value is not False:
                    return value
            except transport.Blocked:
                raise
            except (J.Pending, O.ContextMoved, AssertionError, OSError, ValueError, RuntimeError,
                    subprocess.SubprocessError) as error:
                last = '%s: %s' % (type(error).__name__, error)
            if time.monotonic() >= deadline:
                if optional:
                    return None
                raise WaitTimeout('%s not reached within %d s: %s' % (label, seconds, last))
            time.sleep(.5)

    def parallel(self, roles, function):
        """Read several processes at once so continuous production does not separate their sinks."""
        results, errors = {}, {}

        def run(role):
            try:
                results[role] = function(role)
            except BaseException as error:  # re-raised below in priority order
                errors[role] = error
        threads = [threading.Thread(target=run, args=(role,), daemon=True) for role in roles]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=self.bounds['probeTimeoutSeconds'] * 4)
        if any(thread.is_alive() for thread in threads):
            raise J.Pending('a parallel read did not finish within its probe bound')
        for kind in (transport.Blocked, J.Violation, J.NotObservable):
            for error in errors.values():
                if isinstance(error, kind):
                    raise error
        if errors:
            raise next(iter(errors.values()))
        return [results[role] for role in roles]

    def same_sink(self, rows, label):
        if len({J.norm(row['dag']['sink']) for row in rows}) != 1:
            raise J.Pending('%s: processes are at different sinks' % label)
        return rows

    def live(self, roles):
        return [role for role in roles if self.drill and role in self.drill.processes]

    def values(self):
        values = {'ROOT': str(self.root), 'SALT': self.plan['salt'], 'CLI': self.plan['binaries']['cli'],
                  'WORK_DIR': str(self.current_work_dir or self.root / 'useful-work')}
        if self.fixture:
            values['FIXTURE'] = str(self.fixture)
        if self.request_block:
            values['REQUEST_BLOCK'] = self.request_block
        if self.old_block:
            values['OLD_BLOCK'] = self.old_block
        if self.drill:
            for name, key in P.ROLE_KEYS.items():
                node = self.drill.plan['nodes'].get(key)
                if node:
                    prefix = P.PLACEHOLDER_ROLES[name]
                    values.update({prefix + '_APPDIR': str(self.root / key / 'appdir'), prefix + '_WRPC': node['wrpc'],
                                   prefix + '_GRPC': node['grpc'], prefix + '_BORSH': node['borsh'],
                                   prefix + '_ETH': node['eth'], prefix + '_P2P': node['p2p']})
        return values

    def owned_json(self, name, argv, timeout):
        label = self.label(name)
        holder = self.drill or SimpleNamespace(root=self.root, workflow_deadline=self.deadline)
        began = time.monotonic()
        record = dict(label=label, argv=[str(item) for item in argv], kind='owned-json')
        try:
            value = bootstrap.command(holder, self.sampler, label, [str(item) for item in argv], timeout)
            record.update(returnCode=0)
            return value
        except BaseException as error:
            record.update(error=str(error)[:2000])
            raise
        finally:
            record.update(elapsedSeconds=round(time.monotonic() - began, 3),
                          stdout=str(holder.root / (label + '.stdout')), stderr=str(holder.root / (label + '.stderr')))
            self.commands.append(record)

    def cli(self, name, argv, timeout=None):
        value = self.owned_json(name, argv, timeout or self.bounds['commandTimeoutSeconds'])
        if isinstance(value, dict) and value.get('submitted') is True and isinstance(value.get('carriers'), list):
            for carrier in value['carriers']:
                self.cli_records.append(dict(label=name, txid=J.norm(carrier['txid']), exposureFeeSompi=carrier['fee_sompi']))
        return value

    def command_fn(self, drill, sampler, label, argv, timeout):
        return self.cli(label, argv, timeout)

    def run_owned(self, name, argv, timeout, expect_success=True):
        """An owned child whose output is persisted and never read as node state."""
        label = self.label(name)
        folder = self.root / 'commands'
        folder.mkdir(exist_ok=True)
        stdout_path, stderr_path = folder / (label + '.stdout'), folder / (label + '.stderr')
        began = time.monotonic()
        allowed = min(timeout, self.remaining())
        with stdout_path.open('w') as stdout, stderr_path.open('w') as stderr:
            child = subprocess.Popen([str(item) for item in argv], stdout=stdout, stderr=stderr)
            self.sampler.register(label, child)
            try:
                while child.poll() is None:
                    if time.monotonic() - began >= allowed:
                        break
                    self.sampler.check()
                    self.check_processes()
                    self.tick()
                    if sum(path.stat().st_size for path in folder.glob('*.std*')) > self.limits['maximumFixtureOutputBytes']:
                        raise ResourceLimitExceeded('owned command output exceeded its predeclared byte bound')
                    time.sleep(.2)
            finally:
                timed_out = child.poll() is None
                if timed_out:
                    child.kill()
                    child.wait(timeout=20)
        tail = stderr_path.read_text(errors='replace')[-4000:]
        record = dict(source='owned-process', label=label, argv=[str(item) for item in argv], exitCode=child.returncode,
                      exitedWithinBound=not timed_out, elapsedSeconds=round(time.monotonic() - began, 3),
                      stdout=str(stdout_path), stderr=str(stderr_path), stderrTail=tail)
        self.commands.append(record)
        if expect_success and (timed_out or child.returncode):
            raise transport.Blocked('%s %s; the operator command failed (see %s)' % (
                label, 'timed out' if timed_out else 'exited %d' % child.returncode, stderr_path))
        return record

    def fixtures(self, name, templates):
        for template in templates:
            self.run_owned(name, P.fill_argv(template, self.values()), self.bounds['commandTimeoutSeconds'])

    def tick(self):
        """Periodic hook for subclasses that sample while waiting or while an owned command runs."""

    def ensure_private(self, path):
        mode = path.stat().st_mode & 0o777
        if mode & 0o077:
            os.chmod(path, 0o600)  # owned fixture key exported by this run's own factory child
            self.commands.append(dict(kind='chmod-owned-fixture-key', path=str(path), previousMode=oct(mode)))

    def write_private_json(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'w') as file:
            json.dump(value, file, indent=2)
            file.write('\n')
        return path

    # ---- phases -----------------------------------------------------------------------------

    def phase(self, name, boxes, requires, function):
        entry = dict(name=name, checkboxes=list(boxes), startedUnix=time.time())
        missing = [flag for flag in requires if flag not in self.flags]
        if self.stopped or missing:
            reason = ('scenario stopped: ' + self.stopped) if self.stopped else \
                'prerequisite not established: ' + ', '.join(missing)
            for box in boxes:
                self.findings.append(J.finding(box, name, J.NOT_RUN, reason))
            entry.update(status=J.NOT_RUN, reason=reason)
            self.phases.append(entry)
            return
        self.arm(name)
        began, verdict, reason = time.monotonic(), None, ''
        try:
            function()
            entry['status'] = 'COMPLETED'
        except J.Violation as error:
            verdict, reason = J.FAIL, str(error)
        except WaitTimeout as error:
            verdict, reason = J.FAIL, str(error)
        except ResourceLimitExceeded as error:
            verdict, reason = J.FAIL, 'predeclared observation bound exceeded: %s' % error
            self.stopped = reason
        except ProcessExited as error:
            verdict, reason = J.BLOCKED, str(error)
            self.stopped = reason
        except (J.NotObservable, transport.Blocked, P.PlanError) as error:
            verdict, reason = J.BLOCKED, str(error)
            if 'workflow deadline' in reason:
                self.stopped = reason
        except AssertionError as error:
            verdict, reason = J.FAIL, 'invariant assertion: %s' % error
        except Exception as error:  # harness defect: nothing was judged
            verdict, reason = J.BLOCKED, 'harness error %s: %s' % (type(error).__name__, error)
        finally:
            self.finish_crash_points(name)
            entry['elapsedSeconds'] = round(time.monotonic() - began, 3)
        if verdict:
            for box in boxes:
                self.findings.append(J.finding(box, name, verdict, reason))
            entry.update(status=verdict, reason=reason)
        elif name in self.plan['evm']['consumerCheckpoints']:
            self.checkpoint(name)
        self.phases.append(entry)
        self.write_evidence()

    def phase_identity(self):
        network, identity = self.plan['network'], self.plan['identity']
        if network['kind'] == 'preset':
            self.stopped = 'preset network: retirement cannot be armed by this harness'
            J.retirement_armed(network, {}, identity)
        binaries = {}
        for name in ('node', 'cli'):
            path = Path(self.plan['binaries'][name])
            if not path.is_file() or not os.access(path, os.X_OK):
                raise transport.Blocked('binary %s is not an executable file: %s' % (name, path))
            binaries[name] = dict(path=str(path), sha256=O.sha256_file(path))
        repository = Path(__file__).resolve().parents[2]
        head = subprocess.run(['git', '-C', str(repository), 'rev-parse', 'HEAD'], capture_output=True, text=True,
                              timeout=30).stdout.strip()
        dirty = subprocess.run(['git', '-C', str(repository), 'status', '--porcelain', '--untracked-files=no'],
                               capture_output=True, text=True, timeout=30).stdout.strip()
        self.evidence.update(binaries=binaries, harnessCommit=head, harnessCheckoutDirty=bool(dirty))
        if head != self.plan['commit']:
            raise transport.Blocked('the harness checkout %s is not the pinned commit %s' % (head, self.plan['commit']))
        if dirty:
            raise transport.Blocked('the harness checkout has uncommitted changes; the evidence would name no single commit')
        fixture = self.root / 'signed-public-fixture'
        manifest = self.owned_json('MANIFEST', [binaries['node']['path'], '--private-probe-profile=paid-v4',
                                                '--private-probe-salt=' + self.plan['salt'], '--testnet', '--netsuffix=12',
                                                '--private-probe-export-bootstrap=' + str(fixture)] + self.retirement_flags(),
                                   self.bounds['factoryTimeoutSeconds'])
        bootstrap.validate_factory_manifest(manifest, 'paid-v4')
        self.identity_facts = J.retirement_armed(network, manifest, identity)
        self.findings.append(J.finding(self.identity_box, 'retirement-armed-by-private-profile', J.OBSERVED, '',
                                       self.identity_facts))
        complete.costs_before_nodes(manifest, self.fees['maximumBindingReservationSompi'],
                                    self.fees['maximumCompleteCheckFeeSompi'])
        seats = manifest.get('seats') or []
        needed = [self.plan['usefulWork']['kernelPayerSeat'], P.PROVIDER_PAYER_SEAT] + \
            self.plan['usefulWork']['checker']['payerSeats'] + self.plan['accounting']['incomeSeats']
        if len(seats) <= max(needed):
            raise transport.Blocked('the factory exports %d seats; the plan names seat %d' % (len(seats), max(needed)))
        self.manifest, self.fixture, self.genesis = manifest, fixture, J.norm(manifest['genesis'])
        self.evidence['factoryManifest'] = manifest
        if any(p['trigger']['kind'] == 'import-wal' for p in self.plan['crash']['points']):
            capability = manifest.get('privateProbeProfile', {}).get('importCrashFailpoints', {})
            if capability.get('available') is not True or capability.get('rendezvousVersion') != 1:
                raise transport.Blocked('exact WAL crashes require the advertised private-integration rendezvous capability')
        self.flags.add('identity')

    def observer_argv(self, role, endpoints):
        p2p, grpc, wrpc, borsh, eth = endpoints
        return [self.plan['binaries']['node'], '--private-probe-profile=paid-v4', '--private-probe-salt=' + self.plan['salt'],
                '--testnet', '--netsuffix=12', '--nodnsseed', '--outpeers=0', '--disable-upnp', '--listen=' + p2p,
                '--rpclisten=' + grpc, '--rpclisten-json=' + wrpc, '--rpclisten-borsh=' + borsh, '--evm-rpc-listen=' + eth,
                '--appdir={' + role + '_APPDIR}', '--ram-scale=0.1', '--utxoindex', '--unsaferpc', '--nologfiles',
                '--loglevel=info']

    def add_observers(self, node_plan):
        used = set()
        for node in node_plan['nodes'].values():
            used.update(node[key] for key in ('p2p', 'grpc', 'wrpc', 'borsh'))
            used.add(node['eth'].split('//', 1)[1])
        if node_plan.get('nativeCustodyProvider'):
            used.add(node_plan['nativeCustodyProvider']['listen'])
        reservations, endpoints = [], []
        try:
            while len(endpoints) < 10:
                listener = socket.socket()
                listener.bind(('127.0.0.1', 0))
                reservations.append(listener)
                endpoint = '127.0.0.1:%d' % listener.getsockname()[1]
                if endpoint not in used:
                    used.add(endpoint)
                    endpoints.append(endpoint)
            for index, role in enumerate(('FRESH', 'CRASHJOIN')):
                chosen = endpoints[index * 5:index * 5 + 5]
                node_plan['nodes'][role] = dict(p2p=chosen[0], grpc=chosen[1], wrpc=chosen[2], borsh=chosen[3],
                                                eth='http://' + chosen[4], argv=self.observer_argv(role, chosen))
        finally:
            for listener in reservations:
                listener.close()

    def payer_addresses(self):
        seats = self.manifest['seats']
        uw = self.plan['usefulWork']
        named = {'provider': [P.PROVIDER_PAYER_SEAT], 'kernel': [uw['kernelPayerSeat']], 'checker': uw['checker']['payerSeats'],
                 'income': self.plan['accounting']['incomeSeats']}
        return {kind: [seats[index]['payAddress'] for index in indices] for kind, indices in named.items()}

    def money_addresses(self):
        addresses = [self.manifest['fundingAddress']]
        for group in self.payer_addresses().values():
            addresses.extend(group)
        return sorted(set(addresses))

    def money_read(self, role, label, addresses):
        maximum = self.plan['scan']['maximumUtxoRowsPerAddress']
        return O.bracketed(self.drill, role, label, lambda rpc, dag, node: dict(entries=O.utxo_entries(rpc, addresses, maximum)))

    def phase_launch(self):
        node_plan = bootstrap.plan_for(Path(self.plan['binaries']['node']), self.plan['salt'], self.fixture, self.manifest,
                                       self.bounds['phaseTimeoutSeconds'], 'paid-v4', True,
                                       self.fees['maximumProviderAckFeePerCarrierSompi'])
        self.add_observers(node_plan)
        for node in node_plan['nodes'].values():
            node['argv'].extend(self.retirement_flags())  # one later fence for every process of this run
        # The coverage archive role keeps every header (no pruning); the producer stays the pruned GC source.
        if self.plan.get('coverage'):
            node_plan['nodes'][P.ROLE_KEYS[self.plan['coverage']['archiveRole']]]['argv'].append('--archival')
        node_plan.update(scenario_timeout_s=self.bounds['phaseTimeoutSeconds'], probe_timeout_s=self.bounds['probeTimeoutSeconds'],
                         command_timeout_s=self.bounds['commandTimeoutSeconds'])
        (self.root / 'node-plan.json').write_text(json.dumps(node_plan, indent=2) + '\n')
        self.evidence['nodePlan'] = node_plan
        self.drill = AcceptanceDrill(node_plan, self.root, self.sampler)
        self.drill.workflow_deadline = self.deadline
        for role in PRIMARY:
            self.drill.start(role)
            self.wait(lambda role=role: bootstrap.ready(self.drill, role, self.manifest), role + ' exact private identity')
        # Genesis-era money of every fixture address the fee/carrier ledgers resolve against.
        initial = self.wait(lambda: self.money_read('JOIN', 'initial-money', self.money_addresses()), 'initial money read')
        self.rows.append(initial)
        self.amounts.update(O.amounts_of(initial['value']['entries']))
        self.payer_start = initial
        self.evidence['initialMoney'] = initial
        # The pre-fence legacy view, read while the virtual DAA is still below the retirement fence if it is.
        if self.plan.get('reconciliation'):
            self.reconciliation['pre'] = self.legacy_view('JOIN', 'legacy-pre-fence')
        self.flags.add('nodes')

    def phase_bootstrap(self):
        drill, manifest, fixture = self.drill, self.manifest, self.fixture
        cli, salt = self.plan['binaries']['cli'], self.plan['salt']
        budget = dict(maximum=self.fees['maximumWorkflowCarrierFeeSompi'], spent=0, unresolved=0,
                      submissionOutstanding=False, submittedFeesKnown=True)
        self.accounting['cliBudget'] = budget
        argv = complete.cli_prefix(drill, cli, salt) + ['palw', 'submit-object',
            '--key-file=' + str(fixture / manifest['fundingKeyFile']),
            '--maximum-total-fee-sompi=' + str(budget['maximum']), '--object',
            str(fixture / manifest['signedRegistration']), str(fixture / manifest['artifactBinding'])]
        bootstrap.registration_window(manifest, bootstrap.rpc_read(drill, 'A', 'getBlockDagInfo')['virtualDaaScore'])
        preview = self.cli('CARRIER_PREVIEW', argv)
        if preview.get('dry_run') is not True or type(preview.get('total_fee_sompi')) is not int \
                or not 0 <= preview['total_fee_sompi'] <= budget['maximum']:
            raise transport.Blocked('the registration carrier preview exceeds the predeclared CLI fee cap')
        bootstrap.registration_window(manifest, bootstrap.rpc_read(drill, 'A', 'getBlockDagInfo')['virtualDaaScore'])
        complete.submit_with_reserved_budget(drill, self.sampler, 'CARRIER_SUBMIT', argv + ['--yes'],
                                             self.bounds['commandTimeoutSeconds'], budget, self.command_fn)
        for role in ('B', 'JOIN'):
            drill.join_link(role, 'A')
        self.work['registration'] = self.wait(lambda: bootstrap.agreement(drill, manifest),
                                              'native registration/binding on all three processes')
        if self.plan.get('dnsHistory') and self.plan['dnsHistory']['validators']:
            # Before the clock starts: B restarts in activate_complete_clock with its validator flags already set.
            self.dns_history_setup()
        self.work['recoveryClock'] = bootstrap.activate_complete_clock(drill, manifest)
        self.work['completeCheck'] = complete.run(drill, self.sampler, cli, salt, fixture, manifest, budget,
                                                  self.fees['maximumBindingReservationSompi'],
                                                  self.fees['maximumCompleteCheckFeeSompi'], bootstrap.agreement,
                                                  bootstrap.rpc_read, self.command_fn)
        self.work['custody'] = bootstrap.exercise_native_custody(drill, manifest, recovery_v4=True)
        self.flags.add('custody')

    def restart(self, role, append=()):
        self.drill.kill(role)
        self.drill.plan['nodes'][role]['argv'].extend(append)
        self.drill.start(role)
        self.wait(lambda: bootstrap.ready(self.drill, role, self.manifest), role + ' restarted with exact identity')
        self.relink(role)

    def relink(self, role):
        if role == 'A':
            for joiner, target in (('B', 'A'), ('JOIN', 'A')):
                if joiner in self.drill.processes:
                    self.drill.join_link(joiner, target)
        elif role in SOURCE_OF:
            self.drill.join_link(role, SOURCE_OF[role])

    def paid_claim_set(self, roles, label, base=None):
        base, round_number = base or self.work['baseClaim'], self.plan['usefulWork']['checker']['round']
        rows = self.parallel(roles, lambda role: O.bracketed(self.drill, role, label,
                                                             lambda rpc, dag, node: O.paid_claim_value(rpc, base, round_number)))
        return self.same_sink(rows, label)

    def scan_diagnostics(self, roles, force=False):
        if not force and time.monotonic() - self.last_log_scan < 5:
            return
        self.last_log_scan = time.monotonic()
        paths = [path for role in roles for path in self.drill.log_paths.get(role, [])]
        J.checker_outcome(O.log_lines(paths))

    def phase_useful_work(self, uw=None, work_dir=None):
        """One real paid useful-work cycle; repeatable (each cycle owns a fresh work directory and replaces the
        producer/checker spec flags rather than stacking them)."""
        uw = uw or self.plan['usefulWork']
        drill, manifest, fixture = self.drill, self.manifest, self.fixture
        node = self.plan['binaries']['node']
        work_dir = work_dir or self.root / 'useful-work'
        work_dir.mkdir(parents=True, exist_ok=False)
        self.current_work_dir = work_dir
        self.checker_bond_of = {}
        pinned = self.wait(lambda: O.bracketed(drill, 'A', 'request-checkpoint', lambda rpc, dag, node: None),
                           'live paid request checkpoint')
        self.request_block = J.norm(pinned['dag']['sink'])
        self.evidence.setdefault('paidRequestCheckpoints', []).append(pinned)
        self.fixtures('USEFUL_REQUEST', [uw['requestCommand']])
        request = Path(P.fill(uw['requestFile'], self.values())).resolve()
        if not request.is_file() or self.root not in request.parents:
            raise transport.Blocked('the request command produced no owned request file at %s' % request)
        export_dir = work_dir / 'export'
        export = self.owned_json('PAID_WORK_EXPORT', [node, '--private-probe-profile=paid-v4', '--private-probe-salt=' + self.plan['salt'],
                                                      '--testnet', '--netsuffix=12', '--private-probe-paid-request=' + str(request),
                                                      '--private-probe-export-paid-work=' + str(export_dir)] + self.retirement_flags(),
                                 self.bounds['factoryTimeoutSeconds'])
        if export.get('schema') != 'misaka.private-paid-native-work.v4' or export.get('realTirExecution') is not True \
                or export.get('releaseApproval') is not False or export.get('nativeEnrollmentIncluded') is not False:
            raise transport.Blocked('the offline exporter did not produce real unapproved native work')
        work_path = export_dir / export['work']
        if not work_path.is_file():
            raise transport.Blocked('the exported work file is missing')
        self.work['export'] = export
        kernel_seat = manifest['seats'][uw['kernelPayerSeat']]
        kernel_funding = self.payer_funding(kernel_seat, 'kernel claim payer')
        self.ensure_private(fixture / kernel_seat['keyFile'])
        point = self.wait(lambda: O.bracketed(drill, 'A', 'kernel-checkpoint', lambda rpc, dag, node: None), 'checkpoint read')
        daa = point['dag']['virtualDaaScore']
        spec = dict(version=4, work_path=str(work_path), checkpoint=J.norm(point['dag']['sink']),
                    paid_class=J.norm(export['paidClass']), paid_job=J.norm(export['paidJob']), salt=uw['claimSalt'],
                    commit_daa=daa + uw['commitDaaOffset'], expires_daa=daa + uw['commitDaaOffset'] + uw['expiresDaaSpan'],
                    payer_key=str(fixture / kernel_seat['keyFile']), funding_outpoints=kernel_funding,
                    maximum_fee_sompi=self.fees['maximumKernelClaimFeeSompi'], max_input_bytes=uw['maxInputBytes'],
                    max_material_bytes=uw['maxMaterialBytes'], max_workspace_bytes=uw['maxWorkspaceBytes'],
                    max_units=uw['maxUnits'])
        spec_path = self.write_private_json(work_dir / 'kernel-paid-work-v4.json', spec)
        self.work['producerSpec'] = spec
        seen = set(O.outbox_records(self.root, 'A', 'kernel-outbox', P.OUTBOX_FILES))
        self.set_flag('A', '--private-probe-kernel-paid-work=', '--private-probe-kernel-paid-work=' + str(spec_path))
        self.restart('A')

        def outbox():
            self.scan_diagnostics(['A'])
            records = O.outbox_records(self.root, 'A', 'kernel-outbox', P.OUTBOX_FILES)
            fresh = sorted(set(records) - seen)
            return fresh or None
        records = self.wait(outbox, 'the producer durably signs its paid commit (owned outbox)')
        base = records[0].split('/')[-1]
        self.work['baseClaim'] = J.norm(base)
        expected = dict(producerBond=manifest['seats'][0]['bond'], classId=export['paidClass'], jobId=export['paidJob'])

        def accepted():
            self.scan_diagnostics(['A'])
            rows = self.paid_claim_set(self.live(PRIMARY), 'paid-claim-accepted')
            J.paid_claim_accepted(rows, expected)
            return rows
        rows = self.wait(accepted, 'paid claim accepted on every process')
        self.rows.extend(rows)
        facts = self.judge(self.work_box, 'paid-claim-accepted', J.paid_claim_accepted, rows, expected)
        self.flags.add('workAccepted')
        self.start_checkers(rows[0]['value'], work_path)
        bonds = list(self.checker_bond_of.values())

        def receipts():
            self.scan_diagnostics(list(self.checker_bond_of) + ['A', 'B'])
            rows = self.paid_claim_set(self.live(PRIMARY), 'checker-receipts')
            J.checker_receipts(rows, bonds)
            return rows
        rows = self.wait(receipts, 'selected checker receipts accepted everywhere')
        self.rows.extend(rows)
        self.judge(self.work_box, 'checker-receipts', J.checker_receipts, rows, bonds)

        def final():
            rows = self.paid_claim_set(self.live(PRIMARY), 'paid-claim-final')
            J.paid_claim_final(rows)
            return rows
        rows = self.wait(final, 'paid claim Final on every process')
        self.rows.extend(rows)
        self.judge(self.work_box, 'paid-claim-final', J.paid_claim_final, rows)
        self.flags.add('workFinal')
        pins = self.plan['settlement']

        def settled():
            rows = self.consumer_set(self.live(PRIMARY), 'useful-work-settled')
            for row in rows:
                J.settled_with_work(row, pins)
            return rows
        rows = self.wait(settled, 'native safe covered by the useful work on every process')
        for row in rows:
            self.judge(self.work_box, 'useful-work-settles/%s' % P.ROLE_NAMES[row['role']], J.settled_with_work, row, pins)
        self.work['enrollment'] = (facts or {}).get('enrollment')
        self.flags.add('workSettled')

    def payer_funding(self, seat, label):
        """The dedicated payer's current non-coinbase outputs at its own seat address (at most 64, the spec bound).
        Funding that is not indexed there could not be attributed, so it is refused rather than guessed."""
        row = self.wait(lambda: self.money_read('JOIN', 'payer-funding', [seat['payAddress']]), label + ' funding read')
        points = sorted(J.outpoint_key(entry) for entry in row['value']['entries'] if not entry['utxoEntry']['isCoinbase'])
        if self.payer_start and seat['feeOutpoint'] not in set(O.amounts_of(self.payer_start['value']['entries'])):
            raise transport.Blocked('%s fee float %s is not indexed at its seat address; its carrier cost is unmeasurable'
                                    % (label, seat['feeOutpoint']))
        if not points:
            raise transport.Blocked('%s holds no spendable output at its seat address' % label)
        return points[:64]

    def set_flag(self, role, prefix, argument):
        """Replace a flag: a prefix ending in '=' matches the option with any value, otherwise the exact word only
        (so '--palw-produce' never removes '--palw-producer-key=...')."""
        argv = self.drill.plan['nodes'][role]['argv']
        argv[:] = [item for item in argv if not (item.startswith(prefix) if prefix.endswith('=') else item == prefix)]
        if argument is not None:
            argv.append(argument)

    def start_checkers(self, claim, work_path):
        uw, manifest, fixture = self.plan['usefulWork'], self.manifest, self.fixture
        checker = uw['checker']
        quorum = claim.get('quorum')
        if type(quorum) is not int or quorum < 1:
            raise transport.Blocked('the paid claim reports no positive receipt quorum')
        hosts = [P.ROLE_KEYS[name] for name in checker['hosts']]
        if quorum > len(hosts):
            raise transport.Blocked('quorum %d needs %d checker hosts; the plan names %d' % (quorum, quorum, len(hosts)))
        assigned = [index for index, seat in enumerate(manifest['seats']) if seat['bond'] in (claim.get('seats') or [])]
        if len(assigned) < quorum:
            raise transport.Blocked('only %d assigned seats are fixture seats; quorum is %d' % (len(assigned), quorum))
        provider = manifest['seats'][checker['providerSeat']]
        listen = self.drill.plan['nativeCustodyProvider']['listen']
        point = self.wait(lambda: O.bracketed(self.drill, 'JOIN', 'checker-spec', lambda rpc, dag, node: None), 'checker spec read')
        daa = point['dag']['virtualDaaScore']
        specs = []
        for host, seat_index, payer_index in zip(hosts, assigned[:quorum], checker['payerSeats']):
            seat, payer = manifest['seats'][seat_index], manifest['seats'][payer_index]
            funding = self.payer_funding(payer, 'checker receipt payer')
            for key in (seat['keyFile'], payer['keyFile']):
                self.ensure_private(fixture / key)
            span = checker['challengeValidUntilDaaSpan']
            spec = dict(version=4, work_path=str(work_path), enrollment=J.norm(claim['enrollment']), round=checker['round'],
                        checker_bond=seat['bond'], checker_key=str(fixture / seat['keyFile']), provider_bond=provider['bond'],
                        provider_address=listen, payer_key=str(fixture / payer['keyFile']),
                        funding_outpoints=funding, maximum_fee_sompi=self.fees['maximumCheckerReceiptFeeSompi'],
                        max_input_bytes=uw['maxInputBytes'], max_unit_bytes=checker['maxUnitBytes'],
                        max_total_fetch_bytes=checker['maxTotalFetchBytes'], max_workspace_bytes=checker['maxWorkspaceBytes'],
                        max_requests=checker['maxRequests'], challenge_valid_until_daa=None if span is None else daa + span)
            path = self.write_private_json(self.current_work_dir / ('kernel-paid-check-%s.json' % host), spec)
            specs.append(dict(host=host, seat=seat_index, spec=spec))
            self.checker_bond_of[host] = seat['bond']
            self.set_flag(host, '--private-probe-kernel-paid-check=', '--private-probe-kernel-paid-check=' + str(path))
            self.restart(host)
        self.work['checkers'] = specs

    def consumer_set(self, roles, label):
        def read(role):
            return O.consumer_row(self.drill, role, label, ancestry_of=[self.last_finalized.get(role)],
                                  old=self.old_block if role in ('A', 'FRESH') else None)
        return self.record_rows(self.same_sink(self.parallel(roles, read), label))

    def record_rows(self, rows):
        # Every recorded consumer row advances its role's published-finalized history, so the
        # next row of that role carries the chain-ancestry answer for exactly that head.
        for row in rows:
            native = (row['value'].get('settlement') or {}).get('nativeSettlement') or {}
            if native.get('finalized'):
                self.last_finalized[row['role']] = J.norm(native['finalized'])
        self.consumer_rows.extend(rows)
        return rows

    def checkpoint(self, label):
        roles = self.live([P.ROLE_KEYS[name] for name in self.plan['evm']['roles']])
        if len(roles) < 2:
            self.block(4, 'consumers@' + label, 'fewer than two observing roles are live at this checkpoint')
            return
        try:
            rows = self.wait(lambda: self.consumer_set(roles, 'consumers@' + label),
                             'consumer rows at one generation (%s)' % label, optional=True)
        except (J.NotObservable, transport.Blocked, J.Violation) as error:
            self.block(4, 'consumers@' + label, str(error))
            return
        if rows is None:
            self.findings.append(J.finding(4, 'consumers@' + label, J.FAIL,
                                           'processes never shared one generation within the phase bound'))
            return
        for row in rows:
            self.judge(4, 'consumer-coherence@%s/%s' % (label, P.ROLE_NAMES[row['role']]), J.consumer_row, row,
                       self.plan['identity'])
        self.judge(4, 'cross-process-agreement@' + label, J.agreement, rows)
        if 'workFinal' in self.flags:
            self.settlement_snapshot(roles, label)
        if 'bridgeEffects' in self.flags:
            self.bridge_snapshot(roles, label)

    def settlement_snapshot(self, roles, label):
        base, limit = self.work['baseClaim'], self.plan['scan']['kernelFinalsLimit']

        def read(role):
            def body(rpc, dag, node):
                vest = rpc.call('getPalwRewardVesting', {'bond': '', 'payoutAddress': '', 'claimId': base, 'limit': 8,
                                                         'after': '', 'execAfter': ''})
                native = vest.get('native') or {}
                rows = [row for row in native.get('rows') or [] if J.norm(row.get('claimId')) == base]
                finals = rpc.call('getPalwKernelFinals', {'limit': limit})
                listed = [J.norm(row.get('claimId')) for row in finals.get('finals') or []]
                moved = self.moved_sink.get(role)
                return dict(stage=native.get('claimStage', ''), rowsForClaim=len(rows), vestingRow=rows[0] if rows else None,
                            finalsForClaim=listed.count(base), finalsTotal=finals.get('total'),
                            movedAncestor=None if moved is None else O.chain_ancestor(rpc, moved))
            return O.bracketed(self.drill, role, 'settlement@' + label, body)
        rows = self.wait(lambda: self.same_sink(self.parallel(roles, read), 'settlement@' + label),
                         'settlement rows (%s)' % label, optional=True)
        for row in rows or []:
            if row['value']['stage'] == 'moved' and row['role'] not in self.moved_sink:
                self.moved_sink[row['role']] = J.norm(row['dag']['sink'])
        self.settlement_rows.extend(rows or [])

    def point_set(self, roles, label, outpoints):
        def read(role):
            def body(rpc, dag, node):
                value = O.rfc12_state_value(rpc, outpoints)
                if J.norm(value['virtualSelectedParent']) != J.norm(dag['sink']) or value['virtualDaaScore'] != dag['virtualDaaScore']:
                    raise J.Pending('the outpoint read moved to another virtual generation')
                return value
            return O.bracketed(self.drill, role, label, body)
        return self.same_sink(self.parallel(roles, read), label)

    def bridge_snapshot(self, roles, label):
        bridge = self.plan['bridge']
        withdrawal, refund = bridge['withdrawal']['destinationAddress'], bridge['refund']['destinationAddress']
        points = list(self.bridge.get('effectPoints', {}))
        if not points:
            raise J.NotObservable('bridge effects have no directly observed outpoints')

        def read(role):
            def body(rpc, dag, node):
                entries = O.utxo_entries(rpc, [withdrawal, refund], self.plan['scan']['maximumUtxoRowsPerAddress'])
                value = O.rfc12_state_value(rpc, points)
                if J.norm(value['virtualSelectedParent']) != J.norm(dag['sink']) or value['virtualDaaScore'] != dag['virtualDaaScore']:
                    raise J.Pending('bridge point read moved to another virtual generation')
                if J.stable_point_states(value['pointStates']) != J.stable_point_states(self.bridge['effectPoints']):
                    raise J.Violation('a named bridge input/output changed or disappeared')
                return dict(withdrawalOutpoints=[J.outpoint_key(e) for e in entries if e['address'] == withdrawal],
                            refundOutpoints=[J.outpoint_key(e) for e in entries if e['address'] == refund],
                            pointStates=value['pointStates'])
            return O.bracketed(self.drill, role, 'bridge@' + label, body)
        rows = self.wait(lambda: self.same_sink(self.parallel(roles, read), 'bridge@' + label),
                         'bridge effect rows (%s)' % label, optional=True)
        self.bridge_rows.extend(rows or [])

    def evm_set(self, roles, label, accounts):
        timeout = self.bounds['probeTimeoutSeconds']

        def read(role):
            url = self.drill.plan['nodes'][role]['eth']
            return O.bracketed(self.drill, role, label, lambda rpc, dag, node: O.evm_value(url, accounts, timeout))
        rows = self.parallel(roles, read)
        if len({row['value']['ethLatestHash'] for row in rows}) != 1:
            raise J.Pending('%s: ETH latest differs across processes' % label)
        return rows

    @staticmethod
    def credit_rows(rows, account):
        return [dict(row, value=dict(ethLatestHash=row['value']['ethLatestHash'], balanceWei=row['value']['balances'][account]))
                for row in rows]

    def destination_set(self, roles, label, address):
        rows = self.parallel(roles, lambda role: self.money_read(role, label, [address]))
        return self.same_sink(rows, label)

    def expect_refusal(self, name, argv):
        return self.run_owned(name, argv, self.bounds['commandTimeoutSeconds'], expect_success=False)

    def phase_bridge(self):
        bridge, drill = self.plan['bridge'], self.drill
        cli, salt, key = self.plan['binaries']['cli'], self.plan['salt'], self.fixture / self.manifest['fundingKeyFile']
        accounts = [bridge['evmAddress'], bridge['refund']['evmAddress']]
        before = self.wait(lambda: self.evm_set(self.live(PRIMARY), 'bridge-before', accounts), 'EVM balances before deposit')
        self.rows.extend(before)
        self.bridge.update(beforeWei={account: before[0]['value']['balances'][account] for account in accounts},
                           beforeEthNumber=before[0]['value']['ethLatestNumber'])
        self.bridge['destinationsBefore'] = {}
        for name, address in (('withdrawal', bridge['withdrawal']['destinationAddress']),
                              ('refund', bridge['refund']['destinationAddress'])):
            rows = self.wait(lambda address=address: self.destination_set(self.live(PRIMARY), 'dest-before', address),
                             'destination %s before' % name)
            self.bridge['destinationsBefore'][name] = rows[0]['value']['entries']
            self.rows.extend(rows)

        def lock(name, address, amount, tip, delta, fee):
            argv = complete.cli_prefix(drill, cli, salt) + ['evm', 'deposit-lock', '--evm-address=' + address,
                '--amount=' + str(amount), '--claim-tip=' + str(tip), '--timeout-daa-delta=' + str(delta),
                '--fee=' + str(fee), '--key-file=' + str(key)]
            preview = self.cli(name + '_PREVIEW', argv)
            if preview.get('dryRun') is not True or preview.get('feeSompi') != fee:
                raise transport.Blocked('the deposit-lock preview differs from the explicit fee %d' % fee)
            sent = self.cli(name, argv + ['--yes'])
            outpoint = sent.get('lockOutpoint')
            if not isinstance(outpoint, str) or ':' not in outpoint:
                raise transport.Blocked('the deposit-lock submission returned no lock outpoint')
            txid = J.norm(outpoint.rsplit(':', 1)[0])
            self.cli_records.append(dict(label=name, txid=txid, exposureFeeSompi=sent['feeSompi']))
            return dict(outpoint=outpoint, txid=txid, refundTimeoutDaa=sent.get('refundTimeoutDaa'), amount=amount, tip=tip)

        deposit = lock('DEPOSIT_LOCK', bridge['evmAddress'], bridge['depositAmountSompi'], bridge['claimTipSompi'],
                       bridge['claimTimeoutDaaDelta'], bridge['depositLockFeeSompi'])
        self.bridge['deposit'] = deposit
        def accepted_lock():
            rows = self.point_set(self.live(PRIMARY), 'deposit-unspent', [deposit['outpoint']])
            J.point_present(rows, deposit['outpoint'], deposit['amount'], deposit['amount'])
            return rows
        self.rows.extend(self.wait(accepted_lock, 'the original deposit outpoint is unspent on every node'))
        claim = complete.cli_prefix(drill, cli, salt) + ['evm', 'claim', '--outpoint=' + deposit['outpoint']]
        self.wait(lambda: self.run_owned('DEPOSIT_CLAIM', claim, self.bounds['commandTimeoutSeconds'], expect_success=False)
                  ['exitCode'] == 0 or None, 'the deposit claim is queued on the mining node (lock accepted)')
        before_wei = self.bridge['beforeWei'][bridge['evmAddress']]

        def credited():
            rows = self.evm_set(self.live(PRIMARY), 'deposit-credit', accounts)
            J.deposit_credit(before_wei, self.credit_rows(rows, bridge['evmAddress']), deposit['amount'], deposit['tip'])
            return rows
        rows = self.wait(credited, 'the deposit credit executes once on every process')
        self.rows.extend(rows)
        facts = self.judge(5, 'deposit-credited-once', J.deposit_credit, before_wei,
                           self.credit_rows(rows, bridge['evmAddress']), deposit['amount'], deposit['tip'])
        self.bridge['creditedWei'] = (facts or {}).get('creditedWei')
        def consumed_deposit():
            rows = self.point_set(self.live(PRIMARY), 'deposit-consumed', [deposit['outpoint']])
            J.point_consumed(rows, deposit['outpoint'])
            return rows
        point_rows = self.wait(consumed_deposit, 'the credited deposit outpoint is consumed')
        self.rows.extend(point_rows)
        self.judge(5, 'credited-deposit-outpoint-consumed', J.point_consumed, point_rows, deposit['outpoint'])
        target = rows[0]['value']['ethLatestNumber'] + bridge['postClaimEvmBlocks']

        def later():
            rows = self.evm_set(self.live(PRIMARY), 'deposit-after-quiet', accounts)
            if rows[0]['value']['ethLatestNumber'] < target:
                raise J.Pending('waiting for %d more executed EVM blocks' % bridge['postClaimEvmBlocks'])
            return rows
        rows = self.wait(later, 'EVM blocks after the claim')
        self.rows.extend(rows)
        self.judge(5, 'no-later-double-credit', J.deposit_credit, before_wei, self.credit_rows(rows, bridge['evmAddress']),
                   deposit['amount'], deposit['tip'])
        second = self.expect_refusal('DEPOSIT_SECOND_CLAIM', claim)
        self.judge(5, 'second-claim-refused', J.refused, second, 'a second claim of a consumed deposit lock')
        refund = bridge['refund']
        self.bridge['refundLock'] = lock('REFUND_LOCK', refund['evmAddress'], refund['amountSompi'], 0,
                                         refund['timeoutDaaDelta'], refund['lockFeeSompi'])
        def refund_lock_unspent():
            lock = self.bridge['refundLock']
            rows = self.point_set(self.live(PRIMARY), 'refund-lock-unspent', [lock['outpoint']])
            J.point_present(rows, lock['outpoint'], lock['amount'], lock['amount'])
            return rows
        self.rows.extend(self.wait(refund_lock_unspent, 'the refund lock is an actual named unspent output'))
        self.flags.add('bridgeDeposit')

    def phase_market(self):
        market = self.plan['market']
        if not market['enabled']:
            self.block(5, 'market-fill', 'the plan disables the market step; no actual fill was observed')
            return
        if not market['buyCommands'] or not market['sellCommands']:
            self.block(5, 'market-fill', 'the plan supplies no buy/sell commands for a seeded line')
            return
        roles = self.live(PRIMARY)

        def positions(label):
            def read(role):
                def body(rpc, dag, node):
                    answer = rpc.call('getPalwModelPositions', {'holder': market['holderId']})
                    units = sum(row['units'] for row in answer.get('positions') or [] if J.norm(row.get('lineId')) == market['lineId'])
                    return dict(tipHash=answer.get('tipHash'), units=units)
                return O.bracketed(self.drill, role, label, body)
            return self.parallel(roles, read)
        before = self.wait(lambda: positions('market-before'), 'positions before the market step')
        self.rows.extend(before)
        units = before[0]['value']['units']
        self.fixtures('MARKET_OPEN', market['openCommands'])
        self.fixtures('MARKET_BUY', market['buyCommands'])
        bought = self.wait(lambda: (lambda rows: J.units_changed(units, rows, market['buyUnits'], 'buy fill') and rows)(
            positions('market-buy')), 'the buy fill settles on every process')
        self.rows.extend(bought)
        self.judge(5, 'market-buy-fill', J.units_changed, units, bought, market['buyUnits'], 'buy fill')
        after_buy = bought[0]['value']['units']
        self.fixtures('MARKET_SELL', market['sellCommands'])
        sold = self.wait(lambda: (lambda rows: J.units_changed(after_buy, rows, -market['sellUnits'], 'sell fill') and rows)(
            positions('market-sell')), 'the sell fill settles on every process')
        self.rows.extend(sold)
        self.judge(5, 'market-sell-fill', J.units_changed, after_buy, sold, -market['sellUnits'], 'sell fill')

    def cut(self, side):
        cut = []
        for role, target, link in self.drill.link_roles:
            if (role in side) != (target in side):
                link.cut()
                cut.append(link)
        return cut

    def peers(self, role):
        return len(bootstrap.rpc_read(self.drill, role, 'getConnectedPeerInfo')['peerInfo'])

    def phase_topology(self, kind):
        topology = self.plan['topology']
        side = {'B'}
        links = self.cut(side)
        self.wait(lambda: self.peers('B') == 0 or None, 'actual P2P isolation of the provider side')
        start = {role: bootstrap.rpc_read(self.drill, role, 'getBlockDagInfo')['virtualDaaScore'] for role in ('A', 'B')}
        self.fixtures(kind.upper(), topology['forkFixture' if kind == 'fork' else 'partitionFixture'])
        separate = {role: self.wait(lambda role=role: self.consumer_set([role], '%s-isolated' % kind), role + ' isolated read')[0]
                    for role in ('A', 'B')}
        if J.norm(separate['A']['dag']['sink']) == J.norm(separate['B']['dag']['sink']):
            raise transport.Blocked('the %s fixture did not create separate branches' % kind)
        if kind == 'fork':
            for role in ('A', 'B'):
                span = separate[role]['dag']['virtualDaaScore'] - start[role]
                if span < topology['minimumIsolatedDaaSpan']:
                    raise transport.Blocked('%s isolated branch reached %d DAA, below the predeclared %d'
                                            % (role, span, topology['minimumIsolatedDaaSpan']))
        winner_role = P.ROLE_KEYS[topology['forkWinner' if kind == 'fork' else 'partitionWinner']]
        loser_role = 'B' if winner_role == 'A' else 'A'
        winner, loser = J.norm(separate[winner_role]['dag']['sink']), J.norm(separate[loser_role]['dag']['sink'])
        for link in links:
            link.enabled.set()
        roles = self.live(PRIMARY + ('FRESH',))

        def converged():
            def read(role):
                return O.consumer_row(self.drill, role, kind + '-healed', ancestry_of=[winner, loser, self.last_finalized.get(role)])
            rows = self.same_sink(self.parallel(roles, read), kind + '-healed')
            for row in rows:
                ancestry = row['value']['ancestry']
                if ancestry.get(winner) is False or ancestry.get(loser) is True:
                    raise J.Violation('%s selected the branch the plan did not predeclare as the %s winner' % (row['role'], kind))
                if ancestry.get(winner) is not True or ancestry.get(loser) is not False:
                    raise J.Pending('branch membership not yet readable on ' + row['role'])
            return self.record_rows(rows)
        rows = self.wait(converged, '%s convergence on the predeclared winner' % kind)
        self.judge(1, '%s-converges-on-predeclared-winner' % kind, J.agreement, rows)
        if 'workSettled' in self.flags:
            for row in rows:
                self.judge(1, '%s-settlement-after-heal/%s' % (kind, P.ROLE_NAMES[row['role']]), J.settled_with_work, row,
                           self.plan['settlement'])
        if 'bridgeDeposit' in self.flags and 'bridgeEffects' not in self.flags:
            bridge = self.plan['bridge']
            evm = self.wait(lambda: self.evm_set(self.live(PRIMARY), kind + '-deposit', [bridge['evmAddress']]),
                            'deposit credit after the ' + kind)
            self.rows.extend(evm)
            self.judge(5, 'deposit-credit-survives-%s' % kind, J.deposit_credit, self.bridge['beforeWei'][bridge['evmAddress']],
                       self.credit_rows(evm, bridge['evmAddress']), self.bridge['deposit']['amount'], self.bridge['deposit']['tip'])
        self.after_convergence(kind)

    def phase_bridge_settle(self):
        bridge, drill = self.plan['bridge'], self.drill
        cli, salt = self.plan['binaries']['cli'], self.plan['salt']
        refund_lock = self.bridge['refundLock']
        timeout_daa = refund_lock.get('refundTimeoutDaa')
        if type(timeout_daa) is not int:
            raise transport.Blocked('the refund lock carries no refund timeout')
        self.wait(lambda: bootstrap.rpc_read(drill, 'JOIN', 'getBlockDagInfo')['virtualDaaScore'] >= timeout_daa or None,
                  'the refund lock reaches its timeout')
        late = complete.cli_prefix(drill, cli, salt) + ['evm', 'claim', '--outpoint=' + refund_lock['outpoint']]
        self.judge(5, 'expired-lock-claim-refused', J.refused, self.expect_refusal('REFUND_LOCK_LATE_CLAIM', late),
                   'a claim at/after the refund timeout')
        refund = bridge['refund']
        refund_anchor = bootstrap.rpc_read(drill, 'JOIN', 'getBlockDagInfo')['sink']
        if not refund['commands']:
            key = self.fixture / self.manifest['fundingKeyFile']
            argv = complete.cli_prefix(drill, cli, salt) + ['evm', 'refund', '--outpoint=' + refund_lock['outpoint'],
                    '--fee=' + str(refund['maximumRefundFeeSompi']), '--key-file=' + str(key)]
            preview = self.cli('REFUND_PREVIEW', argv)
            if preview.get('to') != refund['destinationAddress']:
                raise transport.Blocked('the plan refund destination must match the original refund owner')
            fee = preview.get('feeSompi')
            if type(fee) is not int or not 0 <= fee <= refund['maximumRefundFeeSompi']:
                raise transport.Blocked('signed refund fee exceeds the plan cap')
            sent = self.cli('REFUND_SIGNED', argv + ['--yes'])
            if not sent.get('refundTxid'):
                raise transport.Blocked('the signed refund returned no outpoint')
        if refund['commands']:
            self.fixtures('REFUND', refund['commands'])
        low, high = refund['amountSompi'] - refund['maximumRefundFeeSompi'], refund['amountSompi']

        def refunded():
            rows = self.destination_set(self.live(PRIMARY), 'refund-after', refund['destinationAddress'])
            for row in rows:
                J.exactly_one_new_utxo(self.bridge['destinationsBefore']['refund'], row['value']['entries'], low, high)
            return rows
        rows = self.wait(refunded, 'the refund materializes once on every process')
        self.rows.extend(rows)
        for row in rows:
            self.judge(5, 'refund-exactly-once/%s' % P.ROLE_NAMES[row['role']], J.exactly_one_new_utxo,
                       self.bridge['destinationsBefore']['refund'], row['value']['entries'], low, high)
        refund_point = J.exactly_one_new_utxo(self.bridge['destinationsBefore']['refund'], rows[0]['value']['entries'], low, high)['outpoint']
        def refunded_points():
            found = self.point_set(self.live(PRIMARY), 'refund-points', [refund_lock['outpoint'], refund_point, self.bridge['deposit']['outpoint']])
            J.point_consumed(found, refund_lock['outpoint'])
            J.point_consumed(found, self.bridge['deposit']['outpoint'])
            J.point_present(found, refund_point, low, high)
            return found
        point_rows = self.wait(refunded_points, 'refund spends the original lock and creates one named output')
        self.rows.extend(point_rows)
        self.bridge['effectPoints'] = point_rows[0]['value']['pointStates']
        def refund_body():
            txid = refund_point.rsplit(':', 1)[0]
            row = O.bracketed(drill, 'JOIN', 'refund-transaction-link',
                lambda rpc, dag, node: O.accepted_transactions(rpc, refund_anchor, [txid], self.plan['scan']))
            J.refund_link(row, refund_lock['outpoint'], refund_point, refund_lock['amount'], refund['maximumRefundFeeSompi'])
            return row
        body = self.wait(refund_body, 'the refund transaction spends the original lock with conserved value')
        self.rows.append(body)
        self.judge(5, 'refund-original-outpoint-value-conservation', J.refund_link, body,
                   refund_lock['outpoint'], refund_point, refund_lock['amount'], refund['maximumRefundFeeSompi'])
        if not refund['commands']:
            self.judge(5, 'second-refund-of-original-outpoint-refused', J.refused,
                       self.expect_refusal('REFUND_SECOND_SPEND', argv + ['--yes']),
                       'a second refund of the consumed lock')
        evm = self.wait(lambda: self.evm_set(self.live(PRIMARY), 'refund-evm', [refund['evmAddress']]), 'refund EVM account read')
        self.rows.extend(evm)
        self.judge(5, 'refunded-lock-never-credited', J.backing, self.bridge['beforeWei'][refund['evmAddress']],
                   evm[0]['value']['balances'][refund['evmAddress']], 0, 0, 0)
        withdrawal = bridge['withdrawal']
        if withdrawal['commands']:
            self.fixtures('WITHDRAWAL', withdrawal['commands'])
            amount = withdrawal['amountSompi']

            def withdrawn():
                rows = self.destination_set(self.live(PRIMARY), 'withdrawal-after', withdrawal['destinationAddress'])
                for row in rows:
                    J.exactly_one_new_utxo(self.bridge['destinationsBefore']['withdrawal'], row['value']['entries'], amount, amount)
                return rows
            rows = self.wait(withdrawn, 'the withdrawal materializes once on every process')
            self.rows.extend(rows)
            for row in rows:
                self.judge(5, 'withdrawal-exactly-once/%s' % P.ROLE_NAMES[row['role']], J.exactly_one_new_utxo,
                           self.bridge['destinationsBefore']['withdrawal'], row['value']['entries'], amount, amount)
            withdrawal_point = J.exactly_one_new_utxo(self.bridge['destinationsBefore']['withdrawal'], rows[0]['value']['entries'], amount, amount)['outpoint']
            def withdrawal_points():
                found = self.point_set(self.live(PRIMARY), 'withdrawal-point', [withdrawal_point])
                J.point_present(found, withdrawal_point, amount, amount)
                return found
            point_rows = self.wait(withdrawal_points, 'the synthetic withdrawal outpoint materializes')
            self.rows.extend(point_rows)
            self.bridge['effectPoints'].update(point_rows[0]['value']['pointStates'])
            evm = self.wait(lambda: self.evm_set(self.live(PRIMARY), 'withdrawal-evm', [bridge['evmAddress']]),
                            'EVM balance after the withdrawal')
            self.rows.extend(evm)
            url = drill.plan['nodes']['JOIN']['eth']
            spends = O.evm_spends(url, bridge['evmAddress'], self.bridge['beforeEthNumber'] + 1, evm[0]['value']['ethLatestNumber'],
                                  self.bounds['probeTimeoutSeconds'], self.plan['scan']['maximumScannedEvmBlocks'])
            gas = sum(spend['gasWei'] for spend in spends)
            self.bridge.update(evmSpends=spends, gasWei=gas)
            if gas > int(withdrawal['maximumGasWei']):
                self.findings.append(J.finding(5, 'withdrawal-gas-bound', J.FAIL, 'gas %d wei exceeds the predeclared %s'
                                               % (gas, withdrawal['maximumGasWei'])))
            if self.bridge.get('creditedWei') is None:
                self.block(5, 'evm-backing-reconciles', 'the deposit credit was not observed')
            else:
                self.judge(5, 'evm-backing-reconciles', J.backing, self.bridge['beforeWei'][bridge['evmAddress']],
                           evm[0]['value']['balances'][bridge['evmAddress']], self.bridge['creditedWei'], amount, gas)
        else:
            self.bridge['gasWei'] = 0
            self.block(5, 'withdrawal-exactly-once', 'no withdrawal command: the plan supplies no EVM withdraw transaction')
        if refund['commands'] and withdrawal['commands']:
            self.flags.add('bridgeEffects')

    def phase_accounting(self):
        scan, drill = self.plan['scan'], self.drill
        wanted = sorted({record['txid'] for record in self.cli_records})
        rpc = drill.rpc('JOIN')
        try:
            found = O.accepted_transactions(rpc, self.genesis, wanted, scan) if wanted else {}
        finally:
            rpc.close()
        amounts = dict(self.amounts)
        amounts.update(O.outputs_as_amounts(found))
        accepted = {}
        for txid in wanted:
            if txid in found:
                fee, unresolved = J.accepted_fee(found[txid]['transaction'], amounts)
                accepted[txid] = dict(accepted=True, feeSompi=fee, unresolvedInputs=unresolved,
                                      acceptingBlock=found[txid]['acceptingBlock'])
            else:
                accepted[txid] = dict(accepted=False, feeSompi=None)
        self.accounting.update(acceptedTransactions=accepted, cliRecords=self.cli_records)
        separation = self.judge(6, 'cli-exposure-vs-accepted-native-fee', J.fee_separation, self.cli_records, accepted)
        addresses = self.payer_addresses()
        end = self.wait(lambda: self.money_read('JOIN', 'payers-end', self.money_addresses()), 'payer balances at one generation')
        self.rows.append(end)
        start_entries, end_entries = self.payer_start['value']['entries'], end['value']['entries']

        def of(entries, address):
            return [entry for entry in entries if entry['address'] == address]
        costs = {}
        provider = self.judge(6, 'provider-carrier-total-cost', J.dedicated_payer_cost, of(start_entries, addresses['provider'][0]),
                              of(end_entries, addresses['provider'][0]), self.fees['maximumProviderCarrierTotalSompi'], 'provider ACK')
        costs['providerCarrierSompi'] = None if provider is None else provider['costSompi']
        kernel = self.judge(6, 'kernel-claim-carrier-cost', J.dedicated_payer_cost, of(start_entries, addresses['kernel'][0]),
                            of(end_entries, addresses['kernel'][0]), self.fees['maximumKernelCarrierTotalSompi'], 'kernel claim')
        costs['kernelClaimCarrierSompi'] = None if kernel is None else kernel['costSompi']
        checker_total = 0
        for address in addresses['checker']:
            facts = self.judge(6, 'checker-receipt-carrier-cost/%s' % address[:24], J.dedicated_payer_cost,
                               of(start_entries, address), of(end_entries, address),
                               self.fees['maximumCheckerCarrierTotalSompi'], 'checker receipt')
            checker_total = None if facts is None or checker_total is None else checker_total + facts['costSompi']
        costs['checkerReceiptCarrierSompi'] = checker_total
        costs['cliAcceptedNativeFeeSompi'] = None if separation is None else separation['acceptedNativeFeeSompi']
        gas = self.bridge.get('gasWei')
        costs['evmGasSompi'] = None if gas is None else -(-gas // J.EVM_NATIVE_SCALE)
        self.accounting['costs'] = costs
        self.judge(6, 'workflow-budget-with-provider-carriers', J.workflow_budget, costs, self.fees['maximumWorkflowTotalCostSompi'])
        income = self.wait(lambda: self.income_read(end_entries), 'reward income read')
        self.accounting['income'] = income
        self.judge(6, 'real-reward-income-measured', J.real_income, income)

    def income_read(self, end_entries):
        producer = self.manifest['seats'][0]['bond']

        def body(rpc, dag, node):
            vest = rpc.call('getPalwRewardVesting', {'bond': producer, 'payoutAddress': '', 'claimId': '', 'limit': 1,
                                                     'after': '', 'execAfter': ''})
            return vest
        row = O.bracketed(self.drill, 'JOIN', 'income', body)
        native = row['value'].get('native') or {}
        launch_daa = self.payer_start['dag']['virtualDaaScore']
        coinbase = sum(entry['utxoEntry']['amount'] for entry in end_entries
                       if entry['address'] in self.payer_addresses()['income'] and entry['utxoEntry']['isCoinbase']
                       and entry['utxoEntry']['blockDaaScore'] > launch_daa)
        return dict(source='node-rpc', vestingAvailable=bool(row['value'].get('available') and native.get('available')),
                    producerBond=producer, createdSompi=native.get('createdSompi'), movedSompi=native.get('movedSompi'),
                    burnedSompi=native.get('burnedSompi'), maturingSompi=native.get('maturingSompi'),
                    queryLatchedSompi=native.get('queryLatchedSompi'), rowsTotal=native.get('rowsTotal'),
                    outstandingSompi=row['value'].get('outstandingSompi'),
                    unspentCoinbaseIncomeSompiLowerBound=coinbase, sink=row['dag']['sink'])

    def phase_capacity(self):
        capacity = self.plan['capacity']
        if capacity['mode'] != 'running-node':
            return  # import mode runs inside the fresh-join phase
        role = P.ROLE_KEYS[capacity['role']]
        reference = 'A' if role != 'A' else 'JOIN'
        prior = self.wait(lambda: self.consumer_set([role, reference], 'capacity-before'), 'capacity prior read')
        before = self.participation(role, prior)
        self.drill.kill(role)
        self.expected_exit.add(role)
        record = dict(priorEpoch=prior[0]['node']['importSessionEpoch'], participationBefore=before, constrainedRows=[])
        try:
            self.drill.start(role, capacity['constrainedArgs'])
            self.relink(role)
            window = time.monotonic() + capacity['refusalWindowSeconds']
            while time.monotonic() < window and self.drill.processes[role].poll() is None:
                self.remaining()
                try:
                    record['constrainedRows'].append(O.bracketed(self.drill, role, 'capacity-constrained', lambda rpc, dag, node: None))
                except (O.ContextMoved, OSError, RuntimeError, J.NotObservable):
                    pass
                time.sleep(1)
            record['constrained'] = dict(exitCode=self.drill.processes[role].poll(), argv=capacity['constrainedArgs'])
            self.drill.kill(role)
        finally:
            self.expected_exit.discard(role)
        self.drill.start(role)
        self.relink(role)
        rows = self.wait(lambda: self.recovered(role, reference, before, capacity['participationSignal']),
                         'recovery after the local capacity constraint', optional=True)
        record.update(recovered=rows is not None, recoveredRows=rows or [])
        record['participationAfter'] = self.participation(role, rows) if rows else before
        self.evidence.setdefault('capacity', []).append(record)
        self.judge(3, 'local-capacity-refusal-exercised', J.capacity_refusal_exercised, record)
        self.judge(3, 'local-capacity-recovery-and-participation', J.capacity_recovery, record, capacity['participationSignal'])

    def participation(self, role, rows):
        row = next(row for row in rows if row['role'] == role)
        other = next((row for row in rows if row['role'] != role), row)
        return dict(virtualDaaScore=row['dag']['virtualDaaScore'], sink=J.norm(row['dag']['sink']),
                    networkSink=J.norm(other['dag']['sink']), producedBlocks=row['node'].get('producedBlocks'),
                    storeBytes=sum(O.provider_stores(self.root, role).values()))

    def recovered(self, role, reference, before, signal):
        rows = self.consumer_set([role, reference], 'capacity-recovered')
        after = self.participation(role, rows)
        if signal == 'sink-follows' and not after['virtualDaaScore'] > before['virtualDaaScore']:
            raise J.Pending('the recovered node has not followed the network past its prior DAA')
        if signal == 'provider-store-grows' and not after['storeBytes'] > before['storeBytes']:
            raise J.Pending('no further provider material yet')
        if signal == 'produced-blocks-increase' and not (after['producedBlocks'] or 0) > (before['producedBlocks'] or 0):
            raise J.Pending('no further produced block yet')
        return rows

    def phase_scale(self):
        scale, roles = self.plan['scale'], self.live(PRIMARY)
        self.fixtures('SCALE', scale['claimFixture'])
        limit = self.plan['scan']['kernelFinalsLimit']

        def reached():
            rows = self.parallel(roles, lambda role: O.bracketed(self.drill, role, 'scale',
                                                                 lambda rpc, dag, node: O.counters_value(rpc, limit)))
            J.scale_reached(rows, scale['claimCounter'], scale['minimumClaims'])
            return rows
        rows = self.wait(reached, '%s >= %d on every process' % (scale['claimCounter'], scale['minimumClaims']))
        self.rows.extend(rows)
        self.work['scaleRows'] = rows
        self.judge(2, 'accepted-claims-at-scale', J.scale_reached, rows, scale['claimCounter'], scale['minimumClaims'])

    def phase_prune(self):
        source = self.wait(lambda: self.consumer_set(['A'], 'prune-before'), 'prune anchor read')[0]
        self.old_block = J.norm(source['dag']['sink'])
        self.fixtures('PRUNE', self.plan['topology']['pruneFixture'])

        def pruned():
            row = O.consumer_row(self.drill, 'A', 'prune-source', ancestry_of=[self.last_finalized.get('A')], old=self.old_block)
            if row['oldBodyAvailable'] is not False:
                raise J.Pending('the source still serves the old block body')
            if J.norm(row['dag']['pruningPointHash']) == self.genesis:
                raise J.Pending('the pruning point has not moved')
            if 'workSettled' in self.flags:
                J.non_genesis_finalized(row, self.genesis)
            return self.record_rows([row])[0]
        row = self.wait(pruned, 'actual old-body GC at a moved pruning point')
        self.work['pruneSource'] = row
        if 'workSettled' in self.flags:
            self.judge(2, 'non-genesis-native-finalized-pruning-checkpoint', J.non_genesis_finalized, row, self.genesis)
        else:
            self.block(2, 'non-genesis-native-finalized-pruning-checkpoint',
                       'useful work never settled, so no native Finalized head can certify the pruning checkpoint')
        self.flags.add('pruned')

    def phase_fresh_join(self):
        source = P.ROLE_KEYS[self.plan['topology']['freshJoinSource']]
        SOURCE_OF.update(FRESH=source, CRASHJOIN=source)
        require_finalized = 'workSettled' in self.flags
        self.drill.start('FRESH')
        self.wait(lambda: bootstrap.ready(self.drill, 'FRESH', self.manifest) or None, 'fresh join exact identity')
        self.drill.join_link('FRESH', source)
        capacity = self.plan['capacity']
        if capacity['mode'] == 'import':
            self.capacity_import(source)
        else:
            self.drill.start('CRASHJOIN')
            if 'CRASHJOIN' in self.drill.import_wal:
                self.wait(lambda: bootstrap.ready(self.drill, 'CRASHJOIN', self.manifest) or None,
                          'crash joiner initializes its own genesis before WAL arming')
                (self.root / 'CRASHJOIN' / 'import-wal' / 'armed').write_text(self.drill.import_wal['CRASHJOIN'])
            self.drill.join_link('CRASHJOIN', source)

        def joined():
            rows = self.consumer_set([source, 'FRESH'], 'fresh-join')
            J.fresh_join(rows[0], rows[1], self.genesis, require_finalized)
            return rows
        rows = self.wait(joined, 'actual fresh pruned P2P join reaches the source generation')
        self.work['freshJoin'] = rows
        self.judge(2, 'fresh-pruned-p2p-join', J.fresh_join, rows[0], rows[1], self.genesis, require_finalized)
        if not require_finalized:
            self.block(2, 'fresh-join-native-finalized', 'useful work never settled; the joiner cannot reach a native Finalized head')

        def crash_joined():
            rows = self.consumer_set([source, 'CRASHJOIN'], 'crash-join')
            if not J.import_is_stable(rows[1]['node']):
                raise J.Pending('the crash joiner import is not stable yet')
            J.fresh_join(rows[0], rows[1], self.genesis, require_finalized)
            return rows
        rows = self.wait(crash_joined, 'the crash joiner completes its import after any injected kill')
        self.judge(3, 'import-retry-completes-atomically', J.fresh_join, rows[0], rows[1], self.genesis, require_finalized)
        self.after_convergence('freshJoin')
        self.flags.add('freshJoined')

    def capacity_import(self, source):
        capacity, role = self.plan['capacity'], 'CRASHJOIN'
        self.expected_exit.add(role)
        record = dict(priorEpoch=0, participationBefore=dict(virtualDaaScore=0, sink=None, networkSink=None, producedBlocks=0,
                                                             storeBytes=0), constrainedRows=[])
        try:
            self.drill.start(role, capacity['constrainedArgs'])
            self.drill.join_link(role, source)
            window = time.monotonic() + capacity['refusalWindowSeconds']
            while time.monotonic() < window and self.drill.processes[role].poll() is None:
                self.remaining()
                self.import_probe(role)
                time.sleep(1)
            record['constrained'] = dict(exitCode=self.drill.processes[role].poll(), argv=capacity['constrainedArgs'])
            self.drill.kill(role)
        finally:
            self.expected_exit.discard(role)
        # The import adopted under the refusal must stay retryable: the same DB reopens unconstrained.
        self.drill.start(role)
        self.drill.join_link(role, source)
        rows = self.wait(lambda: self.recovered(role, source, record['participationBefore'], 'sink-follows'),
                         'the constrained import is retried to a stable state', optional=True)
        record.update(recovered=rows is not None, recoveredRows=rows or [])
        record['participationAfter'] = self.participation(role, rows) if rows else record['participationBefore']
        self.evidence.setdefault('capacity', []).append(record)
        self.judge(3, 'local-capacity-refusal-exercised', J.capacity_refusal_exercised, record)
        self.judge(3, 'local-capacity-import-retry-and-participation', J.capacity_recovery, record, 'sink-follows')

    def phase_cold_reopen(self):
        source = SOURCE_OF['FRESH']
        require_finalized = 'workSettled' in self.flags
        before = self.wait(lambda: self.consumer_set([source, 'FRESH'], 'cold-before'), 'cold reopen prior read')[1]
        self.drill.kill('FRESH')  # SIGKILL of the owned child, then the same DB
        self.drill.start('FRESH')
        self.drill.join_link('FRESH', source)

        def reopened():
            rows = self.consumer_set([source, 'FRESH'], 'cold-reopen')
            J.cold_reopen(before, rows[1], rows[0], require_finalized)
            return rows
        rows = self.wait(reopened, 'the cold-reopened DB reaches the source generation')
        self.judge(2, 'cold-reopen-same-db', J.cold_reopen, before, rows[1], rows[0], require_finalized)
        if 'scaleRows' in self.work:
            limit = self.plan['scan']['kernelFinalsLimit']
            scale = self.plan['scale']

            def counted():
                rows = self.parallel([source, 'FRESH'], lambda role: O.bracketed(
                    self.drill, role, 'scale-after-reopen', lambda rpc, dag, node: O.counters_value(rpc, limit)))
                J.scale_reached(rows, scale['claimCounter'], scale['minimumClaims'])
                return rows
            rows = self.wait(counted, 'the reopened joiner carries the scale population')
            self.judge(2, 'scale-carried-by-pruned-join-and-reopen', J.scale_reached, rows, scale['claimCounter'],
                       scale['minimumClaims'])
        self.after_convergence('coldReopen')

    def mismatch_argv(self, role, profile):
        argv, out = self.drill.argv(self.drill.plan['nodes'][role]['argv']), []
        for item in argv:
            if item.startswith('--private-probe-profile='):
                out.append('--private-probe-profile=' + profile)
            elif item.startswith('--private-probe-') and not item.startswith(('--private-probe-salt=',
                                                                               '--private-probe-retirement-daa=')):
                continue  # paid-only options would refuse before the DB identity pin is reached
            else:
                out.append(item)
        return out

    def phase_generation_mismatch(self):
        control = self.plan['crash']['generationMismatch']
        role = P.ROLE_KEYS[control['role']]
        if role not in self.drill.processes:
            raise transport.Blocked('the generation-mismatch role %s is not running' % control['role'])
        reference = 'A' if role != 'A' else 'JOIN'
        prior = self.wait(lambda: self.consumer_set([role, reference], 'mismatch-before'), 'mismatch prior read')
        marker = self.root / role / 'appdir' / MARKER
        self.drill.kill(role)
        record = dict(priorEpoch=prior[0]['node']['importSessionEpoch'], markerBefore=O.sha256_file(marker),
                      dbBefore=O.db_listing(self.root, role))
        launch = self.run_owned('WRONG_PROFILE_' + role, self.mismatch_argv(role, control['wrongProfile']),
                                control['exitWithinSeconds'], expect_success=False)
        record.update(launch=launch, markerAfter=O.sha256_file(marker), dbAfter=O.db_listing(self.root, role))
        self.drill.start(role)
        self.relink(role)
        rows = self.wait(lambda: self.consumer_set([role, reference], 'mismatch-relaunch'), 'correct relaunch rejoins',
                         optional=True)
        record.update(recovered=rows is not None,
                      relaunchEpoch=rows[0]['node']['importSessionEpoch'] if rows else None)
        self.evidence['generationMismatch'] = record
        self.judge(3, 'generation-mismatch-refused-and-db-intact', J.generation_mismatch, record)

    def legacy_view(self, role, label):
        limit, pages = self.plan['reconciliation']['bondPageLimit'], self.plan['reconciliation']['maximumBondPages']

        def body(rpc, dag, node):
            bonds, cursor = [], None
            for _ in range(pages):
                answer = rpc.call('getStakeBonds', {'ownerPubkeyHash': None, 'statusIn': None, 'cursor': cursor,
                                                    'limit': limit, 'povDaaScore': None})
                bonds.extend(answer.get('bonds') or [])
                cursor = answer.get('nextCursor')
                if not cursor:
                    break
            else:
                raise J.NotObservable('the legacy bond registry exceeds its predeclared page bound')
            precommit = None
            if bonds:
                try:
                    precommit = rpc.call('getPrecommitDuty', {'validatorId': bonds[0].get('validatorId', ''),
                                                              'bondOutpoint': bonds[0].get('bondOutpoint', '')})
                except RuntimeError:
                    precommit = None
            retirement = self.plan['identity']['retirementActivationDaa']
            return dict(bonds=[dict(bondOutpoint=b.get('bondOutpoint'), amount=b.get('amount'),
                                    activationDaaScore=b.get('activationDaaScore'), validatorId=b.get('validatorId'),
                                    effectiveStatus=b.get('effectiveStatus')) for b in bonds],
                        povDaa=dag['virtualDaaScore'] if dag['virtualDaaScore'] < retirement else None,
                        dns=rpc.call('getDnsConfirmation'), precommit=precommit)
        row = self.wait(lambda: O.bracketed(self.drill, role, label, body), label)
        self.rows.append(row)
        return row

    def phase_reconciliation(self):
        post = self.legacy_view('JOIN', 'legacy-post-fence')
        self.reconciliation['post'] = post
        retirement = self.plan['identity']['retirementActivationDaa']
        self.judge(7, 'legacy-bonds-dns-authority-across-retirement', J.retirement_reconciliation,
                   self.reconciliation['pre'], post, retirement)
        if 'workAccepted' in self.flags:
            claims = self.wait(lambda: self.paid_claim_set(self.live(PRIMARY), 'liability-history'), 'liability read',
                               optional=True)
            self.reconciliation['paidClaimLiability'] = claims

    # ---- RFC-0012 §4.4: DNS-validator history before a later fence, and its coinbase reconciliation ----

    def retirement_flags(self):
        fence = self.plan['identity']['retirementActivationDaa']
        return [] if fence == 1 else ['--private-probe-retirement-daa=%d' % fence]

    def registry(self, role='JOIN'):
        history = self.plan['reconciliation']
        rpc = self.drill.rpc(role)
        try:
            return R.bond_registry(rpc, history['bondPageLimit'], history['maximumBondPages'],
                                   R.ReadBudget(history['maximumBondPages']))
        finally:
            rpc.close()

    def dns_history_setup(self):
        """Bond each planned DNS validator (operator commands), bind the ONE new registry bond to it, and run the
        in-process validator service on its role, all below the later fence."""
        dns, fence = self.plan['dnsHistory'], self.plan['identity']['retirementActivationDaa']
        self.fixtures('DNS_KEYRING', dns['keyringCommands'])
        bound = []
        for validator in dns['validators']:
            role = P.ROLE_KEYS[validator['role']]
            known = {bond['bondOutpoint'] for bond in self.registry()}
            self.fixtures('DNS_BOND_' + role, validator['bondCommands'])

            def new_bond():
                fresh = [bond for bond in self.registry() if bond['bondOutpoint'] not in known]
                if len(fresh) > 1:
                    raise transport.Blocked('the bond commands created %d bonds; one validator needs exactly one' % len(fresh))
                return fresh[0] if fresh else None
            bond = self.wait(new_bond, 'the validator stake bond appears in the registry', seconds=dns['bondWaitSeconds'])
            key = Path(P.fill(validator['keyFile'], self.values())).resolve()
            if not key.is_file() or self.root not in key.parents:
                raise transport.Blocked('validator key %s is not an owned file (keyring command)' % key)
            self.ensure_private(key)
            for prefix, argument in (('--enable-validator', '--enable-validator'),
                                     ('--validator-key=', '--validator-key=' + str(key)),
                                     ('--stake-bond=', '--stake-bond=' + bond['bondOutpoint']),
                                     ('--validator-mode=', '--validator-mode=' + validator['mode'])):
                self.set_flag(role, prefix, argument)
            if role != 'B':
                self.restart(role)
            bound.append(dict(role=role, bond=bond, keyFile=str(key), mode=validator['mode']))
        daa = bootstrap.rpc_read(self.drill, 'JOIN', 'getBlockDagInfo')['virtualDaaScore']
        if daa >= fence:
            raise transport.Blocked('the fence (DAA %d) passed at DAA %d before the DNS validators were bonded' % (fence, daa))
        self.reconciliation['validators'] = bound
        # The pre-fence legacy view now holds the bonds that will carry history across the fence.
        self.reconciliation['pre'] = self.legacy_view('JOIN', 'legacy-pre-fence-bonded')

    def legacy_exit(self, fence):
        exit_ = self.plan['dnsHistory']['exit']
        record = dict(source='owned-process', commands=bool(exit_['unbondCommands']), released=False)
        if not exit_['unbondCommands']:
            return record
        bond = self.reconciliation['validators'][0]['bond']
        destination = exit_['destinationAddress']
        before = self.wait(lambda: self.money_read('JOIN', 'exit-before', [destination]), 'exit destination before')
        record.update(bond=bond['bondOutpoint'], amount=bond['amount'], maximumFeeSompi=exit_['maximumFeeSompi'],
                      destinationBefore=before['value']['entries'])
        self.fixtures('DNS_UNBOND', exit_['unbondCommands'])

        def unbonding():
            row = next((entry for entry in self.registry() if entry['bondOutpoint'] == bond['bondOutpoint']), None)
            return row if row and row.get('unbondRequestDaaScore') is not None else None
        row = self.wait(unbonding, 'the legacy bond unbond request is accepted')
        release = row['unbondRequestDaaScore'] + row['unbondingPeriodBlocks']
        record.update(unbondRequestDaaScore=row['unbondRequestDaaScore'], releaseDaa=release,
                      unbondRequestAfterFence=row['unbondRequestDaaScore'] >= fence)
        reached = self.wait(lambda: bootstrap.rpc_read(self.drill, 'JOIN', 'getBlockDagInfo')['virtualDaaScore'] >= release
                            or None, 'the legacy bond release height', optional=True)
        if not reached:
            return record
        record['released'] = True
        self.fixtures('DNS_WITHDRAW', exit_['withdrawCommands'])

        def paid():
            row = self.money_read('JOIN', 'exit-after', [destination])
            J.exactly_one_new_utxo(record['destinationBefore'], row['value']['entries'],
                                   bond['amount'] - exit_['maximumFeeSompi'], bond['amount'])
            return row
        record['destinationAfter'] = self.wait(paid, 'the legacy bond principal is paid once')
        return record

    def phase_retirement_reconciliation(self):
        dns, fence = self.plan.get('dnsHistory'), self.plan['identity']['retirementActivationDaa']
        if fence < 2 or not dns:
            self.block(7, 'dns-history-before-fence',
                       'this run arms retirement at DAA %d; no pre-fence DNS history exists' % fence)
            return
        target = fence + dns['observeAfterFenceDaa']
        archive_role = P.ROLE_KEYS[self.plan['coverage']['archiveRole']]
        self.wait(lambda: all(bootstrap.rpc_read(self.drill, role, 'getBlockDagInfo')['virtualDaaScore'] >= target
                              for role in dict.fromkeys(('JOIN', archive_role))) or None,
                  'the chain passes the fence and its observation span')
        exit_record = self.legacy_exit(fence)
        history = self.plan['reconciliation']
        # The declared archive retains the accepted bodies across pruning. JOIN
        # may legitimately have discarded the pre-retirement history by now.
        rpc = self.drill.rpc(archive_role)
        try:
            to_daa = rpc.call('getBlockDagInfo')['virtualDaaScore']
            block_budget = R.ReadBudget(dns['maximumBlockReads'])
            scan = R.scan_window(rpc, self.genesis, dns['scanFromDaa'], to_daa, self.plan['scan']['maximumScannedChainBlocks'],
                                 block_budget, include_acceptance=True)
            status_budget = R.ReadBudget(dns['maximumStatusReads'])
            registry = R.bond_registry(rpc, history['bondPageLimit'], history['maximumBondPages'], status_budget)
            evidence = dict(sourceRole=archive_role, scanFromDaa=dns['scanFromDaa'], scanToDaa=to_daa, readsUsed=scan['readsUsed'],
                            chainBlocks=scan['chainBlocks'], registry=registry, exit=exit_record)
            try:
                dns_scripts, unread = R.bond_scripts(scan['bondTxs'], registry)
            except J.NotObservable as error:
                dns_scripts, unread = {}, [str(error)]
            evidence.update(dnsScripts=dns_scripts, bondsWithoutReadTransaction=unread)
            palw = {seat['payAddress'] for seat in self.manifest['seats']}
            blocks = R.classify_blocks(scan['blocks'], dns_scripts, set(scan['minerScripts']), palw)
            evidence['blocks'] = blocks
            reserve_reads = []
            for block in blocks:
                block_budget.take()
                reserve_reads.append(O.rfc12_state_value(rpc, block_hash=block['hash']))
            evidence['reserveReads'] = reserve_reads
            evidence['readsUsed'] = block_budget.used
            self.judge(7, 'coinbase-outputs-classified-from-node-reads', R.classification_complete, blocks)
            daas = {block['daa'] for block in blocks
                    if block['daa'] < fence and any(output['kind'] == R.DNS for output in block['outputs'])}
            status = R.statuses_at(rpc, daas | {fence - 1}, history['bondPageLimit'], history['maximumBondPages'],
                                   status_budget)
        finally:
            rpc.close()
        evidence['statusReads'] = {'%s@%d' % key: value for key, value in status.items()}
        self.evidence['retirementReconciliation'] = evidence
        self.judge(7, 'dns-share-before-fence-matches-active-entitlement', R.pre_fence_dns_share, blocks, fence, status)
        entitled = {bond for (bond, daa), value in status.items() if daa == fence - 1 and value == 'active'}
        entitled |= {output['bond'] for block in blocks if block['daa'] < fence
                     for output in block['outputs'] if output['kind'] == R.DNS}
        self.judge(7, 'no-new-dns-share-or-retired-kind-from-the-fence', R.post_fence_dns_share, blocks, scan['bodies'],
                   fence, dns['legacySettlementWindowDaa'], entitled, dns['dnsCadence'])
        self.judge(7, 'legacy-bond-exit-paid-once', R.legacy_exit_paid_once, exit_record)
        self.judge(7, 'dns-reserve-balance-reconciled', R.reserve_balance, dict(balances=reserve_reads))

    def phase_coverage(self):
        """RFC-0006 EXEC origin coverage: freeze production, then read the archive and pruned roles' own minute-cadence
        census files at the one frozen generation. The pruned census is reported, never judged."""
        coverage = self.plan['coverage']
        archive, pruned = P.ROLE_KEYS[coverage['archiveRole']], P.ROLE_KEYS[coverage['prunedRole']]
        for flag in ('--palw-produce', '--private-probe-kernel-paid-work=', '--private-probe-kernel-replace='):
            self.set_flag('A', flag, None)  # paid work/replacement need a producer, so they stop with it
        self.restart('A')
        self.set_flag('B', '--palw-heartbeat-miner-address=', None)
        self.restart('B')
        self.flags.add('frozen')
        roles = self.live(PRIMARY + ('FRESH', 'CRASHJOIN'))
        self.wait(lambda: self.same_sink(self.parallel(roles, lambda role: O.bracketed(
            self.drill, role, 'frozen', lambda rpc, dag, node: None)), 'frozen'), 'every process at one frozen sink')
        params = self.plan['identity']['consensusParamsId']

        def read():
            rows = self.parallel([archive, pruned], lambda role: O.coverage_row(self.drill, self.root, role, 'coverage'))
            J.coverage_same_generation(rows[0], rows[1])
            for row in rows:
                J.coverage_envelope(row, params)
            return rows
        rows = self.wait(read, 'coverage observations at the frozen generation', seconds=coverage['maximumWaitSeconds'])
        self.rows.extend(rows)
        self.evidence['execOriginCoverage'] = dict(archive=rows[0], pruned=rows[1],
                                                   prunedUnavailableReportedNotJudged=rows[1]['value']['coverage']['census'])
        self.judge(9, 'archive-covers-every-eligible-original-header', J.archive_coverage, rows[0], rows[1], params)

    def phase_judgement(self):
        rows = self.consumer_rows + self.rows
        self.judge(4, 'one-sink-one-root-set (all recorded rows)', J.root_consistency, rows)
        self.judge(5, 'both-ledger-roots-agree (UTXO and EVM commitment, all rows)', J.root_consistency, rows)
        by_role = {}
        for row in self.consumer_rows:
            if (row['value'].get('settlement') or {}).get('nativeSettlement'):
                by_role.setdefault(row['role'], []).append(row)
        self.judge(3, 'finalized-history-no-silent-relabel', J.finalized_history, by_role)
        self.judge(4, 'publication-history', J.finalized_history, by_role)
        settled_rows = [row for row in self.consumer_rows if (row['value'].get('settlement') or {}).get('nativeSettlement')]
        self.judge(3, 'finalized-conflict-exercised', J.conflict_exercised, settled_rows)
        self.judge(3, 'incomplete-import-publishes-nothing', J.incomplete_import_publishes_nothing, self.import_rows)
        if self.bridge_rows:
            self.judge(5, 'bridge-effects-exactly-once-across-reorg-restart-join', J.effects_stable, self.bridge_rows)
        else:
            self.block(5, 'bridge-effects-exactly-once-across-reorg-restart-join', 'no bridge effect snapshot was taken')
        if self.settlement_rows:
            self.judge(5, 'once-only-settlement', J.once_only_settlement, self.settlement_rows)
        else:
            self.block(5, 'once-only-settlement', 'the paid claim never reached Final, so no settlement was read')
        paths = [path for paths in self.drill.log_paths.values() for path in paths]
        self.diagnostics = O.log_lines(paths)
        if self.checker_bond_of:
            self.judge(1, 'checker-outcome (owned logs, diagnostic only)', J.checker_outcome, self.diagnostics)

    # ---- crash points -----------------------------------------------------------------------

    def arm(self, phase):
        self.armed = [dict(point=point, state='armed', armedAt=time.monotonic())
                      for point in self.plan['crash']['points'] if point['phase'] == phase]
        for armed in self.armed:
            if armed['point']['trigger']['kind'] == 'import-wal':
                role = P.ROLE_KEYS[armed['point']['role']]
                self.drill.import_wal[role] = armed['point']['trigger']['stage']

    def finish_crash_points(self, phase):
        for armed in self.armed:
            if armed['state'] in ('armed', 'expired'):
                self.crash_records[armed['point']['name']] = dict(killed=False, state=armed['state'])
                self.findings.append(J.judge(3, 'crash:' + armed['point']['name'], J.crash_point, armed['point'],
                                             dict(killed=False)))
        for armed in self.armed:
            if armed['point']['trigger']['kind'] == 'import-wal':
                role = P.ROLE_KEYS[armed['point']['role']]
                self.drill.import_wal.pop(role, None)
                (self.root / role / 'import-wal' / 'armed').unlink(missing_ok=True)
        self.armed = []

    def import_probe(self, role):
        rpc = None
        try:
            rpc = self.drill.rpc(role)
            node, dag = rpc.call('getPalwNodeStatus'), rpc.call('getBlockDagInfo')
            try:
                settlement = rpc.call('getPalwSettlement', {'daaScore': dag['virtualDaaScore']})
            except RuntimeError:
                settlement = None
            eth = {}
            for tag in J.TAGS:
                try:
                    eth[tag] = O.eth_call(self.drill.plan['nodes'][role]['eth'], 'eth_getBlockByNumber', [tag, False],
                                          self.bounds['probeTimeoutSeconds'])
                except OSError:
                    eth[tag] = dict(result=None, error='unreachable')
            row = dict(source='node-rpc', role=role, label='import-probe', node=node, dag=dag,
                       value=dict(settlement=settlement, eth=eth), observedUnix=time.time())
            self.import_rows.append(row)
            return row
        except (OSError, RuntimeError):
            return None
        finally:
            if rpc:
                rpc.close()

    def trigger(self, point, role):
        kind, trigger = point['trigger']['kind'], point['trigger']
        if kind == 'import-wal':
            marker = self.root / role / 'import-wal' / 'ready'
            try:
                if marker.read_bytes() == trigger['stage'].encode('ascii'):
                    return dict(source='owned-file', kind=kind, stage=trigger['stage'], marker=str(marker))
            except FileNotFoundError:
                pass
            return None
        if kind in ('kernel-outbox', 'checker-outbox'):
            files = P.OUTBOX_FILES if kind == 'kernel-outbox' else P.CHECKER_OUTBOX_FILES
            for record, names in O.outbox_records(self.root, role, kind, files).items():
                if trigger['present'] in names and (trigger['absent'] is None or trigger['absent'] not in names):
                    return dict(source='owned-file', kind=kind, record=record, files=names)
        elif kind == 'provider-store':
            stores = O.provider_stores(self.root, role)
            if stores:
                return dict(source='owned-file', kind=kind, stores=stores)
        elif kind == 'import-phase':
            row = self.import_probe(role)
            if row and row['node'].get('importSessionPhase') in trigger['phases']:
                return dict(source='node-rpc', kind=kind, node=row['node'])
        return None

    def poll_crash_points(self):
        if self.in_crash or not self.armed or not self.drill:
            return
        for armed in self.armed:
            point = armed['point']
            role = P.ROLE_KEYS[point['role']]
            if armed['state'] != 'armed' or point['trigger']['kind'] == 'after-convergence' or role not in self.drill.processes:
                continue
            if time.monotonic() - armed['armedAt'] > point['maximumWaitSeconds']:
                armed['state'] = 'expired'
                continue
            observed = self.trigger(point, role)
            if observed:
                self.fire(armed, observed)

    def after_convergence(self, phase):
        for armed in self.armed:
            point = armed['point']
            role = P.ROLE_KEYS[point['role']]
            if armed['state'] == 'armed' and point['trigger']['kind'] == 'after-convergence' and role in self.drill.processes:
                status = bootstrap.rpc_read(self.drill, role, 'getPalwNodeStatus')
                self.fire(armed, dict(source='node-rpc', kind='after-convergence', node=status))

    def crash_state(self, point, role, observed):
        kind = point['trigger']['kind']
        if kind in ('kernel-outbox', 'checker-outbox'):
            files = P.OUTBOX_FILES if kind == 'kernel-outbox' else P.CHECKER_OUTBOX_FILES
            record = (observed or {}).get('record') or self.crash_records.get(point['name'], {}).get('trigger', {}).get('record')
            return dict(outbox=O.outbox_records(self.root, role, kind, files).get(record, {}))
        if kind == 'provider-store':
            return dict(stores=O.provider_stores(self.root, role))
        try:
            return bootstrap.rpc_read(self.drill, role, 'getPalwNodeStatus')
        except (OSError, RuntimeError):
            return {}

    def fire(self, armed, observed):
        point = armed['point']
        role = P.ROLE_KEYS[point['role']]
        self.in_crash = True
        record = dict(killed=False, trigger=observed)
        self.crash_records[point['name']] = record
        try:
            if point['trigger']['kind'] == 'import-wal':
                record['before'] = dict(walStage=observed['stage'])
                self.drill.import_wal.pop(role, None)
                (self.root / role / 'import-wal' / 'armed').unlink(missing_ok=True)
            else:
                record['before'] = observed.get('node') if point['trigger']['kind'] in ('import-phase', 'after-convergence') \
                    else self.crash_state(point, role, observed)
            self.drill.kill(role)  # SIGKILL: no graceful shutdown, no destructor
            armed['state'] = 'killed'
            record.update(killed=True, killedUnix=time.time())
            self.drill.start(role)
            self.relink(role)
            reference = SOURCE_OF.get(role, 'JOIN' if role == 'A' else 'A')

            def back():
                rows = self.consumer_set([role, reference], 'crash-recovery:' + point['name'])
                if not J.import_is_stable(rows[0]['node']):
                    raise J.Pending('the restarted import is not stable yet')
                return rows
            rows = self.wait(back, 'recovery after crash point ' + point['name'], seconds=point['maximumWaitSeconds'],
                             optional=True)
            record.update(recovered=rows is not None, afterRows=rows or [])
            after = self.crash_state(point, role, observed)
            record['after'] = after if point['trigger']['kind'] not in ('import-phase', 'import-wal', 'after-convergence') else \
                (rows[0]['node'] if rows else {})
            if point['trigger']['kind'] in ('kernel-outbox', 'checker-outbox'):
                # The kernel record directory is named by its base claim; a checker record belongs to the current claim.
                base = J.norm(observed['record'].split('/')[-1]) if point['trigger']['kind'] == 'kernel-outbox' \
                    else self.work.get('baseClaim')
                bond = self.checker_bond_of.get(role)
                record['checkerBond'] = bond

                def accepted():
                    rows = self.paid_claim_set(self.live(PRIMARY), 'crash-claim:' + point['name'], base)
                    if not all(row['value'].get('found') for row in rows):
                        raise J.Pending('the restarted WAL record is not accepted yet')
                    if point['trigger']['kind'] == 'checker-outbox' and not all(bond in row['value'].get('receipts', [])
                                                                                for row in rows):
                        raise J.Pending('the restarted checker receipt is not accepted yet')
                    return rows
                try:
                    record['claimRows'] = self.wait(accepted, 'WAL record accepted after ' + point['name'],
                                                    seconds=point['maximumWaitSeconds'], optional=True) or [] if base else []
                except J.NotObservable:
                    record['claimRows'] = []
            armed['state'] = 'recovered' if record['recovered'] else 'unrecovered'
        finally:
            self.in_crash = False
        self.findings.append(J.judge(3, 'crash:' + point['name'], J.crash_point, point, record))

    # ---- population sub-run and evidence ---------------------------------------------------

    def phase_population(self):
        population = self.plan['scale']['registryPopulation']
        if not population['enabled']:
            self.block(2, 'registry-population-at-scale', 'the plan disables the separate population run')
            return
        node, salt, bonds = self.plan['binaries']['node'], self.plan['salt'], population['bonds']
        manifest = self.owned_json('POPULATION_MANIFEST', [node, '--private-probe-profile=population-v1',
                                                           '--private-probe-salt=' + salt, '--testnet', '--netsuffix=12',
                                                           '--print-private-probe-profile', '--private-probe-bonds=' + str(bonds)],
                                   self.bounds['factoryTimeoutSeconds'])
        if manifest.get('kind') != 'rfc12-private-integration-probe' or manifest.get('skipProofOfWork') is not False \
                or manifest.get('releaseApproval') is not False or not manifest.get('retirement') \
                or (manifest.get('population') or {}).get('totalBonds') != bonds:
            raise transport.Blocked('the population factory identity differs from the pinned bond count or arms no retirement')
        folder = self.root / 'population'
        folder.mkdir(exist_ok=True)
        node_plan = smoke.plan_for(Path(node), salt, manifest, population['timeoutSeconds'], 'population-v1', bonds)
        node_plan['probe_timeout_s'] = self.bounds['probeTimeoutSeconds']
        drill = smoke.ResourceDrill(node_plan, folder, self.sampler, {})
        try:
            smoke.run(drill, manifest)
            rows = []
            for role in ('A', 'B', 'JOIN'):
                probe = drill.probe(role)
                rows.append(dict(source='node-rpc', role=role, dag=probe['dag'], node=probe['node'],
                                 value=dict(shadow=probe['shadow'])))
            self.evidence['population'] = dict(manifest=manifest, rows=rows)
            self.judge(2, 'registry-population-at-scale', J.registry_population, rows, bonds)
        finally:
            drill.close()

    def resource_bounds(self):
        sampled = self.evidence.get('sampledOwnedResources') or {}
        if sampled.get('violation') or sampled.get('observationFailures'):
            raise J.Violation(sampled.get('violation') or 'owned RSS sampling failed')
        latencies = self.drill.latencies if self.drill else []
        if not latencies:
            raise J.NotObservable('no probe latency was recorded')
        if max(latencies) > self.limits['maximumProbeMs']:
            raise J.Violation('public read latency %.1f ms exceeds the predeclared %d ms' % (max(latencies), self.limits['maximumProbeMs']))
        sizes = self.evidence.get('ownedDatabaseLogicalBytes') or {}
        over = {role: size for role, size in sizes.items() if size > self.limits['maximumOwnedDatabaseLogicalBytes']}
        if over:
            raise J.Violation('owned DB bytes exceed the predeclared bound: %r' % over)
        return dict(maximumProbeMs=max(latencies), databaseBytes=sizes)

    def write_evidence(self):
        verdict = J.aggregate(self.findings, self.boxes)
        self.evidence.update(status=verdict['status'], checkboxes=verdict['checkboxes'], phases=self.phases,
                             flags=sorted(self.flags), stopped=self.stopped, identityFacts=self.identity_facts,
                             usefulWork=self.work, bridge=self.bridge, accounting=self.accounting,
                             reconciliation=self.reconciliation, crashPoints=self.crash_records,
                             consumerRows=self.consumer_rows, rows=self.rows, importRows=self.import_rows,
                             settlementRows=self.settlement_rows, bridgeRows=self.bridge_rows,
                             diagnostics=self.diagnostics, commands=self.commands,
                             launches=self.drill.launches if self.drill else [],
                             logs=self.drill.log_paths if self.drill else {},
                             probeLatenciesMs=self.drill.latencies if self.drill else [])
        temporary = Path(str(self.out) + '.pending')
        temporary.write_text(json.dumps(self.evidence, indent=2, default=str) + '\n')
        temporary.replace(self.out)

    def execute(self):
        steps = [
            ('identity', (1, 7, 9), (), self.phase_identity),
            ('launch', (1,), ('identity',), self.phase_launch),
            ('bootstrap', (1,), ('nodes',), self.phase_bootstrap),
            ('usefulWork', (1, 3), ('custody',), self.phase_useful_work),
            ('bridge', (5, 6), ('nodes',), self.phase_bridge),
            ('market', (5,), ('nodes',), self.phase_market),
            ('fork', (1, 4), ('nodes',), lambda: self.phase_topology('fork')),
            ('partition', (1, 4), ('nodes',), lambda: self.phase_topology('partition')),
            ('bridgeSettle', (5,), ('bridgeDeposit',), self.phase_bridge_settle),
            # Before pruning: the pre-fence bodies must still be served.
            ('retirementReconciliation', (7,), ('nodes',), self.phase_retirement_reconciliation),
            ('accounting', (6,), ('nodes',), self.phase_accounting),
            ('capacity', (3,), ('nodes',), self.phase_capacity),
            ('scale', (2,), ('nodes',), self.phase_scale),
            ('prune', (1, 2), ('nodes',), self.phase_prune),
            ('freshJoin', (1, 2, 3), ('pruned',), self.phase_fresh_join),
            ('coldReopen', (2, 3), ('freshJoined',), self.phase_cold_reopen),
            ('generationMismatch', (3,), ('nodes',), self.phase_generation_mismatch),
            ('reconciliation', (7,), ('nodes',), self.phase_reconciliation),
            ('coverage', (9,), ('pruned',), self.phase_coverage),
            ('judgement', (1, 3, 4, 5), ('nodes',), self.phase_judgement),
        ]
        for name, boxes, requires, function in steps:
            self.phase(name, boxes, requires, function)

    def close(self):
        try:
            if self.drill:
                self.drill.close()
        finally:
            self.evidence['sampledOwnedResources'] = self.sampler.stop()
            roles = self.drill.plan['nodes'] if self.drill else {}
            self.evidence['ownedDatabaseLogicalBytes'] = {role: sum(file.stat().st_size for file in (self.root / role).rglob('*')
                                                                    if file.is_file()) for role in roles}
            if self.drill:
                self.judge(self.resource_box, 'predeclared-observation-bounds', self.resource_bounds)


def print_identity(binary, salt):
    """Read-only pinning aid: the identity a plan must name for this binary and salt."""
    answer = subprocess.run([binary, '--private-probe-profile=paid-v4', '--private-probe-salt=' + salt, '--testnet',
                             '--netsuffix=12', '--print-private-probe-profile'], capture_output=True, text=True, timeout=120)
    if answer.returncode:
        raise SystemExit('identity read failed: ' + answer.stderr[-2000:])
    manifest = json.loads(answer.stdout)
    retirement = manifest.get('retirement') or {}
    print(json.dumps(dict(genesis=J.norm(manifest['genesis']), consensusParamsId=J.norm(manifest['consensusParamsId']),
                          retirementActivationDaa=retirement.get('activationDaa'),
                          retirementPolicyId=J.norm(retirement.get('policyId'))), indent=2))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--plan')
    parser.add_argument('--out')
    parser.add_argument('--print-identity', action='store_true', help='read-only: print the identity to pin, then exit')
    parser.add_argument('--binary')
    parser.add_argument('--salt')
    args = parser.parse_args(argv)
    if args.print_identity:
        if not args.binary or not args.salt:
            parser.error('--print-identity needs --binary and --salt')
        print_identity(args.binary, args.salt)
        return 0
    if not args.plan or not args.out:
        parser.error('--plan and --out are required')
    raw = Path(args.plan).read_bytes()
    try:
        plan = P.parse_plan(raw)
        limits = transport.read_process_limits(plan['observationBoundsFile'])
    except (P.PlanError, ValueError, OSError) as error:
        print(json.dumps(dict(status=J.BLOCKED, reason='plan refused before launch: %s' % error, passLabelUsed=False)))
        return 2
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc12-acceptance-')).resolve()  # no symlinked ancestors (macOS /var)
    (root / 'plan.json').write_bytes(raw)
    out = Path(args.out).resolve()
    run = AcceptanceRun(plan, root, limits, out)
    try:
        run.execute()
    finally:
        try:
            run.close()
        finally:
            if 'identity' in run.flags and not run.stopped:
                run.phase('registryPopulation', (2,), ('identity',), run.phase_population)
            run.write_evidence()
    summary = {key: run.evidence[key] for key in ('status', 'artifactDirectory', 'planSha256', 'stopped', 'passLabelUsed')}
    summary['checkboxes'] = {key: value['status'] for key, value in run.evidence['checkboxes'].items()}
    print(json.dumps(summary, indent=2))
    return 0 if run.evidence['status'] == J.OBSERVED else 1


if __name__ == '__main__':
    raise SystemExit(main())
