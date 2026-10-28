"""Oracle/fault controls only; no test here is a node/P2P/soundness PASS."""
import copy
import importlib.util
from pathlib import Path
import socket
import sys
import threading
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
path = Path(__file__).resolve().parents[1] / 'misaka-rfc12-multiprocess.py'
spec = importlib.util.spec_from_file_location('rfc12_drill', path)
drill = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drill)


class Oracle(unittest.TestCase):
    def test_scenario_thresholds_refuse_rounded_unbounded_or_vacuous_plans_before_launch(self):
        plan = dict(minimum_isolated_daa_span=5400, minimum_claims=100_000,
            minimum_depth=3, minimum_work=str(2**100), maximum_probe_ms=200,
            workflow_timeout_s=600, probe_timeout_s=10)
        drill.validate_scenario_bounds(plan)
        for key, values in {
            'minimum_claims': (99_999, 100_000.0, True, float('inf'), 2**64),
            'minimum_isolated_daa_span': (5399, 5400.0),
            'minimum_depth': (0, True, 3.0),
            'minimum_work': ('0', '01', 2**100, float(2**100), str(2**128)),
            'workflow_timeout_s': (0, -1, 600.0, float('inf')),
            'maximum_probe_ms': (0, True, 200.0),
        }.items():
            for value in values:
                with self.subTest(key=key, value=value), self.assertRaises(AssertionError):
                    drill.validate_scenario_bounds(dict(plan, **{key: value}))

    def test_completed_scenarios_cannot_hide_an_unobserved_cold_reopen(self):
        limits = dict(maximumProbeMs=200, maximumOwnedDatabaseLogicalBytes=1000)
        launches = dict(A=['A_LAUNCH_1'], B=['B_LAUNCH_1'], JOIN=['JOIN_LAUNCH_1', 'JOIN_LAUNCH_2'])
        children = {name: dict(samples=1) for name in ('A', 'B', 'JOIN', 'A_LAUNCH_1', 'B_LAUNCH_1',
            'JOIN_LAUNCH_1', 'JOIN_LAUNCH_2')}
        result = dict(sampledOwnedResources=dict(violation=None, observationFailures=0, ownedChildren=children),
            probeLatenciesMs=[100], ownedDatabaseLogicalBytes=dict(A=100, B=100, JOIN=100),
            requiredNodeLaunchSamples=launches)
        drill.check_process_limits(limits, result)
        children['JOIN_LAUNCH_2']['samples'] = 0
        with self.assertRaisesRegex(drill.ResourceLimitExceeded, 'launch/reopen'):
            drill.check_process_limits(limits, result)
        children['JOIN_LAUNCH_2']['samples'] = 1
        result['sampledOwnedResources']['observationFailures'] = 1
        with self.assertRaisesRegex(drill.ResourceLimitExceeded, 'sampling failed'):
            drill.check_process_limits(limits, result)

    def fixture(self):
        plan = dict(ruleset_id='01' * 32, policy_id='02' * 64,
                    minimum_work=str(2 ** 100), minimum_depth=7)
        native = dict(generation='03' * 64, rulesetId=plan['ruleset_id'], policyId=plan['policy_id'],
                      safe='04' * 64, finalized=None, uniqueWork=str(2 ** 100), depth=7)
        row = dict(dag=dict(sink=native['generation']), dns=dict(available=False),
                   node=dict(consensusParamsId=plan['ruleset_id'], importSessionAvailable=True,
                             importSessionEpoch=4, importSessionPoint='', importSessionPhase='idle', pruningImportStable=True),
                   settlement=dict(dnsRetiredAt=1, nativeSettlement=native,
                                   nativeReadiness=dict(generation=native['generation'], safe=native['safe'], finalized=dict(finalized=None))))
        return row, plan

    def test_actual_probe_rejects_daa_or_identity_changes_even_with_a_fixed_sink(self):
        base_dag = dict(sink='03' * 64, pruningPointHash='04' * 64, virtualDaaScore=100)
        base_node = dict(genesisHash='05' * 64, consensusParamsId='06' * 32,
            importSessionAvailable=True, importSessionEpoch=1, importSessionPoint='',
            importSessionPhase='idle', pruningImportStable=True)
        # Exercise both public-view brackets in the real probe consumer, rather
        # than treating a sink-only comparison as adequate coherence evidence.
        for stage in (2, 3):
            for field, value in (('virtualDaaScore', 101), ('genesisHash', '07' * 64),
                    ('consensusParamsId', '08' * 32), ('importSessionEpoch', 2)):
                with self.subTest(stage=stage, field=field):
                    class Rpc:
                        reads = 0
                        def call(self, method, *args):
                            if method == 'getBlockDagInfo':
                                self.reads += 1
                                row = dict(base_dag)
                                if self.reads >= stage and field in row:
                                    row[field] = value
                                return row
                            if method == 'getPalwNodeStatus':
                                row = dict(base_node)
                                if self.reads >= stage and field in row:
                                    row[field] = value
                                return row
                            return {'getBlock': dict(block=dict(header={})),
                                'getDnsConfirmation': dict(available=False),
                                'getPalwSettlement': {}, 'getConnectedPeerInfo': [],
                                'getPalwCapacityShadow': {}}[method]
                        def close(self):
                            pass
                    rpc = Rpc()
                    instance = drill.Drill(dict(nodes={}), Path('/unused-owned-probe'))
                    instance.rpc = lambda role: rpc
                    with self.assertRaisesRegex(AssertionError, 'context changed'):
                        instance.probe('A')
                    self.assertEqual(instance.observations, [])

    def test_missing_retirement_is_blocked_even_with_large_work(self):
        row, plan = self.fixture()
        row['settlement']['dnsRetiredAt'] = None
        with self.assertRaises(drill.Blocked):
            drill.coherent(row, plan)

    def test_live_dns_stale_generation_or_readiness_cannot_certify(self):
        row, plan = self.fixture()
        for changed in ('dns', 'generation', 'readiness'):
            bad = copy.deepcopy(row)
            if changed == 'dns':
                bad['dns']['available'] = True
            elif changed == 'generation':
                bad['dag']['sink'] = '05' * 64
            else:
                bad['settlement']['nativeReadiness']['safe'] = '05' * 64
            with self.assertRaises(AssertionError):
                drill.coherent(bad, plan)

    def test_missing_diagnostics_and_mixed_or_incomplete_import_cannot_certify(self):
        row, plan = self.fixture()
        missing = copy.deepcopy(row)
        missing['node'].pop('importSessionAvailable')
        with self.assertRaises(drill.Blocked):
            drill.coherent(missing, plan)
        for key, value in [('pruningImportStable', False), ('importSessionPoint', 'ff' * 64), ('importSessionPhase', 'utxo-stream')]:
            bad = copy.deepcopy(row)
            bad['node'][key] = value
            with self.assertRaises(AssertionError):
                drill.coherent(bad, plan)

    def test_u128_work_is_exact_and_one_unit_short_refuses(self):
        row, plan = self.fixture()
        drill.coherent(row, plan)
        row['settlement']['nativeSettlement']['uniqueWork'] = str(2 ** 100 - 1)
        with self.assertRaises(AssertionError):
            drill.coherent(row, plan)

    def test_numeric_coercion_cannot_satisfy_native_work_or_import_epoch(self):
        row, plan = self.fixture()
        for value in (True, 2 ** 100, float(2 ** 100), '+1', '01', '1e38', str(2 ** 128), '\u0661'):
            bad = copy.deepcopy(row)
            bad['settlement']['nativeSettlement']['uniqueWork'] = value
            with self.assertRaises(AssertionError):
                drill.coherent(bad, plan)
        for value in (True, 4.0, -1, 2 ** 64):
            bad = copy.deepcopy(row)
            bad['node']['importSessionEpoch'] = value
            with self.assertRaises(AssertionError):
                drill.coherent(bad, plan)

    def test_pruned_join_cannot_pass_with_null_genesis_or_unrelated_finalized(self):
        row, plan = self.fixture()
        row['dag']['pruningPointHash'] = '06' * 64
        row['node']['genesisHash'] = '07' * 64
        with self.assertRaisesRegex(AssertionError, 'no native Finalized'):
            drill.coherent(row, plan, require_finalized=True)
        native = row['settlement']['nativeSettlement']
        readiness = row['settlement']['nativeReadiness']['finalized']
        native['finalized'] = readiness['finalized'] = row['dag']['pruningPointHash']
        drill.coherent(row, plan, require_finalized=True)
        native['finalized'] = readiness['finalized'] = '08' * 64
        with self.assertRaisesRegex(AssertionError, 'actual pruning checkpoint'):
            drill.coherent(row, plan, require_finalized=True)
        native['finalized'] = readiness['finalized'] = row['dag']['pruningPointHash'] = row['node']['genesisHash']
        with self.assertRaisesRegex(AssertionError, 'still names genesis'):
            drill.coherent(row, plan, require_finalized=True)

    def test_header_only_success_and_named_missing_header_are_gc_evidence(self):
        class Rpc:
            def __init__(self, answer):
                self.answer = answer
            def call(self, *args):
                if isinstance(self.answer, Exception):
                    raise self.answer
                return {'block': self.answer}
        old = 'aa' * 64
        self.assertFalse(drill.old_body_is_available(Rpc(dict(verboseData=dict(isHeaderOnly=True), transactions=[])), old))
        self.assertFalse(drill.old_body_is_available(Rpc(RuntimeError('getBlock: cannot find header ' + old)), old))
        self.assertTrue(drill.old_body_is_available(Rpc(dict(verboseData=dict(isHeaderOnly=False), transactions=[dict(id='coinbase')])), old))
        for error in (RuntimeError('server closed the connection'), RuntimeError('busy'), RuntimeError('cannot find header ' + 'bb' * 64)):
            with self.assertRaises(RuntimeError):
                drill.old_body_is_available(Rpc(error), old)
        with self.assertRaises(AssertionError):
            drill.old_body_is_available(Rpc(dict(verboseData=dict(isHeaderOnly=False), transactions=[])), old)

    def test_owned_transport_cut_closes_active_connection_and_resume_accepts(self):
        listener = socket.socket()
        listener.bind(('127.0.0.1', 0))
        listener.listen()
        listener.settimeout(.2)
        done = threading.Event()

        def echo():
            while not done.is_set():
                try:
                    conn, _ = listener.accept()
                except socket.timeout:
                    continue
                except OSError:
                    return
                with conn:
                    conn.settimeout(.2)
                    while not done.is_set():
                        try:
                            data = conn.recv(1024)
                            if not data:
                                break
                            conn.sendall(data)
                        except socket.timeout:
                            continue
                        except OSError:
                            break

        thread = threading.Thread(target=echo, daemon=True)
        thread.start()
        link = drill.Link(listener.getsockname())
        link.enabled.set()
        try:
            host, port = link.address.split(':')
            with socket.create_connection((host, int(port)), timeout=2) as first:
                first.settimeout(2)
                first.sendall(b'ordinary-opaque-p2p-bytes')
                self.assertEqual(first.recv(1024), b'ordinary-opaque-p2p-bytes')
                link.cut()
                self.assertEqual(first.recv(1024), b'', 'partition must drop an already established transport')
            link.enabled.set()
            with socket.create_connection((host, int(port)), timeout=2) as second:
                second.sendall(b'retry')
                self.assertEqual(second.recv(1024), b'retry')
        finally:
            link.close()
            done.set()
            listener.close()
            thread.join(timeout=2)


if __name__ == '__main__':
    unittest.main()
