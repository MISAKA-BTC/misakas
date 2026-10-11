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
    def fixture(self):
        plan = dict(ruleset_id='01' * 32, policy_id='02' * 64,
                    minimum_work=str(2 ** 100), minimum_depth=7)
        native = dict(generation='03' * 64, rulesetId=plan['ruleset_id'], policyId=plan['policy_id'],
                      safe='04' * 64, finalized=None, uniqueWork=str(2 ** 100), depth=7)
        row = dict(dag=dict(sink=native['generation']), dns=dict(available=False),
                   node=dict(consensusParamsId=plan['ruleset_id']),
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

    def test_u128_work_is_exact_and_one_unit_short_refuses(self):
        row, plan = self.fixture()
        drill.coherent(row, plan)
        row['settlement']['nativeSettlement']['uniqueWork'] = str(2 ** 100 - 1)
        with self.assertRaises(AssertionError):
            drill.coherent(row, plan)

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
