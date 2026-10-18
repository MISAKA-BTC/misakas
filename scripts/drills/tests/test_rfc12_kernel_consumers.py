"""Source controls for real-consumer wiring; no node/TIR/P2P acceptance runs."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import rfc12_kernel_consumers as consumers

SPEC = importlib.util.spec_from_file_location('kernel_consumer_transport', SCRIPTS / 'misaka-rfc12-multiprocess.py')
transport = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(transport)


def h(n):
    return ('%02x' % n) * 64


def point(n):
    return h(n) + ':0'


class ConsumerSourceControls(unittest.TestCase):
    def fixture(self, root):
        root = root.resolve(strict=True)
        # Opaque tiny fixture bytes exercise driver validation only. A real
        # node must independently reject these as non-work if ever launched.
        work = root / 'program-work-v3.borsh'
        work.write_bytes(b'fixture-not-executable-work')
        keys = []
        for n in range(3):
            key = root / ('key-%d.seed' % n)
            key.write_text('01' * 32)
            key.chmod(0o600)
            keys.append(str(key))
        producer = dict(version=4, work_path=str(work), checkpoint=h(20), paid_class=h(21), paid_job=h(22),
            salt=h(23), commit_daa=1, expires_daa=50, payer_key=keys[0], funding_outpoints=[point(7)],
            maximum_fee_sompi=100, max_input_bytes=1024, max_material_bytes=1024, max_workspace_bytes=4096, max_units=10)
        checker = dict(version=4, work_path=str(work), enrollment=h(24), round=0,
            checker_bond=point(4), checker_key=keys[1], provider_bond=point(2), provider_address='127.0.0.1:12345',
            payer_key=keys[2], funding_outpoints=[point(8)], maximum_fee_sompi=100,
            max_input_bytes=1024, max_unit_bytes=1024, max_total_fetch_bytes=4096, max_workspace_bytes=4096, max_requests=10)
        spec_path, check_path, manifest_path = root / 'producer.json', root / 'checker.json', root / 'manifest.json'
        for path, value in ((spec_path, producer), (check_path, checker)):
            path.write_text(json.dumps(value))
        manifest = dict(schema='misaka.private-paid-native-work.v4', realTirExecution=True, work=work.name,
            producer=consumers.kernel_bond(point(1)), paidClass=h(21), paidJob=h(22), baseJob=h(25))
        manifest_path.write_text(json.dumps(manifest))
        supplied = dict(version=4, producer=dict(role='A', specification=str(spec_path), workManifest=str(manifest_path)),
            checkers=[dict(role='B', specification=str(check_path))], claim=h(30), kernelClass=h(31),
            networkDomain=consumers.network_domain(h(33)), genesis=h(33), consensusParamsId=h(34), maximumDirectCarrierExposureSompi=200,
            maximumPublicRecordBytes=1024)
        plan = dict(nodes={role: dict(argv=['/owned/private-node', '--private-probe-profile=paid-v4'], wrpc='127.0.0.1:12346')
            for role in ('A', 'B', 'JOIN')},
            nativeKernelConsumers=supplied)
        plan['nodes']['A']['argv'] += ['--palw-produce', '--palw-producer-bond=' + point(1),
            '--private-probe-native-exchange=127.0.0.1:12345',
            '--private-probe-native-recovery-providers=' + point(2) + ',' + point(3)]
        plan['nodes']['B']['argv'] += ['--private-probe-native-provider-listen=127.0.0.1:12345',
            '--private-probe-native-provider-keys=' + keys[0] + '@' + point(2) + ',' + keys[1] + '@' + point(3),
            '--private-probe-recovery-ack-funding=' + point(9)]
        return plan, producer, checker

    def test_bootstrap_precedes_worker_activation_and_fresh_join_remains_unstarted(self):
        with tempfile.TemporaryDirectory() as folder:
            plan, _, _ = self.fixture(Path(folder))
            events = []
            class OwnedControlDrill(transport.Drill):
                def kill(self, role):
                    events.append(('kill', role))
                    del self.processes[role]
                def start(self, role):
                    events.append(('start', role))
                    self.processes[role] = object()
                def probe(self, role):
                    return True
                def wait(self, predicate, _label):
                    return predicate()
                def join_link(self, role, target):
                    events.append(('link', role, target))
                def kernel_checkpoint(self, label, roles):
                    events.append(('observe', label, roles))
            drill = OwnedControlDrill(plan, Path(folder).resolve(strict=True))
            self.assertIsNone(drill.kernel_consumers)
            self.assertFalse(any(arg.startswith(consumers.WORK_FLAG) for arg in plan['nodes']['A']['argv']))
            # run() performs fixture('bootstrap') before invoking activation.
            # These fake process handles exercise ordering only, never an OS kill.
            drill.processes = {'A': object(), 'B': object()}
            def own_prefix(_drill, prepared, _context):
                events.append(('own-prefix', prepared['workers'][0]['checkpoint']))
                return True
            with patch.object(consumers, 'bootstrap_checkpoint', own_prefix):
                drill.activate_kernel_consumers()
            self.assertEqual(events, [('own-prefix', h(20)), ('kill', 'B'), ('kill', 'A'), ('start', 'B'), ('start', 'A'),
                ('link', 'B', 'A'), ('observe', 'shared-bootstrap', ('A', 'B'))])
            self.assertNotIn('JOIN', drill.processes)
            self.assertEqual(drill.kernel_observations[0]['selectedVerifierApprovalStatus'], 'UNKNOWN')

    def test_exported_checkpoint_requires_both_own_chain_prefixes(self):
        with tempfile.TemporaryDirectory() as folder:
            plan, _, _ = self.fixture(Path(folder))
            prepared = consumers.prepare(plan)
            class Rpc:
                def __init__(self, role, missing=False, changed=False):
                    self.role, self.missing, self.changed, self.reads = role, missing, changed, 0
                def call(self, method, payload=None):
                    if method == 'getBlockDagInfo':
                        self.reads += 1
                        return dict(sink=h(40), pruningPointHash=h(41), virtualDaaScore=10)
                    if method == 'getPalwNodeStatus':
                        return dict(genesisHash=h(33), consensusParamsId=h(34), importSessionAvailable=True,
                            importSessionEpoch=2 if self.changed and self.reads > 1 else 1,
                            importSessionPoint='', importSessionPhase='idle', pruningImportStable=True)
                    if method == 'getBlock':
                        if self.missing and self.role == 'B':
                            raise RuntimeError('not yet imported')
                        return dict(block=dict(verboseData=dict(hash=payload['hash'], isChainBlock=True)))
                    raise AssertionError('unexpected own RPC')
                def close(self):
                    pass
            for missing, changed, ready in ((True, False, False), (False, True, False), (False, False, True)):
                drill = type('OwnedDrill', (), {'rpc': lambda self, role: Rpc(role, missing, changed)})()
                rows = consumers.bootstrap_checkpoint(drill, prepared, transport.probe_context)
                self.assertEqual(rows is not None, ready)
            # A producer starting only after the JOIN import would create a
            # circular prerequisite for the initial actual-claim observation.
            plan['nativeKernelConsumers']['producer']['role'] = 'JOIN'
            with self.assertRaisesRegex(ValueError, 'before the later fresh JOIN'):
                consumers.validate_request(plan)

    def test_failed_kill_keeps_owned_handle_and_prevents_reopen(self):
        import subprocess
        with tempfile.TemporaryDirectory() as folder:
            drill = transport.Drill(dict(nodes={}), Path(folder).resolve(strict=True))
            class Child:
                def poll(self):
                    return None
                def kill(self):
                    pass
                def wait(self, timeout):
                    raise subprocess.TimeoutExpired('owned-child', timeout)
            child = Child()
            drill.processes['A'] = child
            with self.assertRaises(subprocess.TimeoutExpired):
                drill.kill('A')
            self.assertIs(drill.processes['A'], child)
            with self.assertRaisesRegex(transport.Blocked, 'handle still exists'):
                drill.start('A')

    def test_real_node_flags_use_exact_json_and_keep_approval_unknown(self):
        with tempfile.TemporaryDirectory() as folder:
            plan, _, _ = self.fixture(Path(folder))
            prepared = consumers.prepare(plan)
            self.assertTrue(any(arg.startswith(consumers.WORK_FLAG) for arg in plan['nodes']['A']['argv']))
            self.assertTrue(any(arg.startswith(consumers.CHECK_FLAG) for arg in plan['nodes']['B']['argv']))
            self.assertEqual(prepared['directCarrierExposureUpperSompi'], 200)
            self.assertEqual(prepared['selectedVerifierApprovalStatus'], 'UNKNOWN')
            self.assertFalse(prepared['securitySoundnessMeasured'])
            consumers.revalidate(prepared, 'A')
            Path(plan['nativeKernelConsumers']['producer']['specification']).write_text('{}')
            with self.assertRaisesRegex(ValueError, 'changed after'):
                consumers.revalidate(prepared, 'A')
            self.assertIsNone(consumers.prepare(dict(nodes={})), 'legacy plans have no side effects')

    def test_bad_exact_limits_or_duplicate_funding_refuse_before_mutating_argv(self):
        for changed in ('fee', 'funding', 'boolean', 'profile', 'bond', 'provider'):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as folder:
                plan, producer, checker = self.fixture(Path(folder))
                before = deepcopy(plan['nodes'])
                if changed == 'fee':
                    plan['nativeKernelConsumers']['maximumDirectCarrierExposureSompi'] = 199
                elif changed == 'funding':
                    checker['funding_outpoints'] = producer['funding_outpoints']
                elif changed == 'boolean':
                    checker['max_requests'] = True
                elif changed == 'profile':
                    plan['nodes']['B']['argv'][1] = '--private-probe-profile=paid-v3'
                    before = deepcopy(plan['nodes'])
                elif changed == 'bond':
                    checker['funding_outpoints'] = [checker['checker_bond']]
                else:
                    checker['provider_bond'] = point(99)
                Path(plan['nativeKernelConsumers']['checkers'][0]['specification']).write_text(json.dumps(checker))
                with self.assertRaises(ValueError):
                    consumers.prepare(plan)
                self.assertEqual(plan['nodes'], before)

    def observe_fixture(self, prepared, changed_epoch=False):
        config = prepared['configuration']
        producer = prepared['workers'][0]
        class Rpc:
            reads = 0
            def call(self, method, payload=None):
                if method == 'getBlockDagInfo':
                    self.reads += 1
                    return dict(sink=h(40), pruningPointHash=h(41), virtualDaaScore=10)
                if method == 'getPalwNodeStatus':
                    return dict(genesisHash=config['genesis'], consensusParamsId=config['consensusParamsId'],
                        importSessionAvailable=True, importSessionEpoch=2 if changed_epoch and self.reads > 1 else 1,
                        importSessionPoint='', importSessionPhase='idle', pruningImportStable=True)
                if method == 'getPalwKernelClaim':
                    return dict(available=True, found=True, claimId=config['claim'], classId=config['kernelClass'],
                        jobId=producer['baseJob'], producerBond=producer['kernelProducer'], tipDaa=10,
                        publicRecord='0102', recordHeader='0304', convicted=False, settlement='settled', state='Final',
                        quorum=0, receiptsCounted=0, ledgerRoot=h(41), auxRoot=h(42))
                raise AssertionError('unexpected RPC method')
            def close(self):
                pass
        rpc = Rpc()
        return type('OwnedDrill', (), {'rpc': lambda self, role: rpc})()

    def test_actual_claim_final_string_never_becomes_selected_verifier_approval(self):
        with tempfile.TemporaryDirectory() as folder:
            plan, _, _ = self.fixture(Path(folder))
            prepared = consumers.prepare(plan)
            row = consumers.observe(self.observe_fixture(prepared), 'A', prepared, transport.probe_context)
            self.assertTrue(row['actualNativeClaimObserved'])
            self.assertEqual(row['selectedVerifierApprovalStatus'], 'UNKNOWN')
            self.assertEqual(row['settledFeeStatus'], 'UNKNOWN')
            self.assertFalse(row['releaseApproval'])
            with self.assertRaisesRegex(AssertionError, 'generation/import'):
                consumers.observe(self.observe_fixture(prepared, True), 'A', prepared, transport.probe_context)

    def test_named_checker_wal_snapshot_is_bounded_and_not_acceptance(self):
        with tempfile.TemporaryDirectory() as folder:
            plan, _, _ = self.fixture(Path(folder))
            prepared = consumers.prepare(plan)
            worker = prepared['workers'][1]
            appdir = Path(folder).resolve(strict=True) / 'B' / 'appdir'
            job = appdir / 'misaka-testnet-12' / 'kernel-paid-checker-v4' / consumers.kernel_bond(worker['checker']) / \
                (worker['enrollment'] + '-0')
            job.mkdir(parents=True)
            drill = type('OwnedDrill', (), {'values': {'B_APPDIR': str(appdir)}})()
            self.assertEqual(consumers.checker_wal(drill, worker), {})
            path = job / 'receipt.borsh'
            path.write_bytes(b'opaque-WAL-fixture')
            before = consumers.checker_wal(drill, worker)
            self.assertEqual(before['receipt.borsh']['bytes'], len(b'opaque-WAL-fixture'))
            self.assertEqual(before, consumers.checker_wal(drill, worker))
            path.write_bytes(b'x' * (worker['maximumInputBytes'] + 1))
            with self.assertRaises(ValueError):
                consumers.checker_wal(drill, worker)


if __name__ == '__main__':
    unittest.main()
