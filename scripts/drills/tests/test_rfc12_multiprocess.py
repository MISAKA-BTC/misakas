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
