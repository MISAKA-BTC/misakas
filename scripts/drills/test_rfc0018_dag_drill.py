"""RPC contract and false-positive controls for the RFC-0018 acceptance driver.

Run: python3 -m unittest discover -s scripts/drills -p test_rfc0018_dag_drill.py
These are driver tests, not real-node acceptance evidence.
"""
import copy
import json
from pathlib import Path
import socket
import threading
import unittest
from unittest.mock import patch

from rfc0018_dag_drill import Blocked, DelayedLink, Drill, panel_seats_for_claim


class Rpc:
    def __init__(self, replies):
        self.replies, self.calls = replies, []

    def call(self, method, params):
        self.calls.append((method, params))
        return copy.deepcopy(self.replies[params['bond']])


class DriverContracts(unittest.TestCase):
    def drill(self):
        return Drill({'nodes': {}, 'miners': {
            'a': {'bond': 'aa:0'}, 'b': {'bond': 'bb:1'},
        }}, Path('/unused'))

    def reply(self, bond, claim):
        return {'available': True, 'truncated': False,
                'claims': [{'claimId': claim, 'executorBond': bond, 'phase': 'final'}]}

    def test_claim_queries_use_each_executor_and_include_final_claims(self):
        rpc = Rpc({'aa:0': self.reply('aa:0', 'a'), 'bb:1': self.reply('bb:1', 'b')})
        rows = self.drill().read_claims(rpc)['claims']
        self.assertEqual({r['claimId'] for r in rows}, {'a', 'b'})
        self.assertEqual(rpc.calls, [('getPalwClaims', {
            'bond': b, 'role': 'executor', 'includeTerminal': True, 'limit': 0,
        }) for b in ['aa:0', 'bb:1']])

    def test_unavailable_and_truncated_claim_reads_cannot_pass(self):
        for key, value in [('available', False), ('truncated', True)]:
            reply = self.reply('aa:0', 'a')
            reply[key] = value
            with self.subTest(key=key), self.assertRaises(Blocked):
                self.drill().read_claims(Rpc({'aa:0': reply}))

    def test_empty_plan_cannot_observe_an_empty_claim_set_as_success(self):
        with self.assertRaises(Blocked):
            Drill({'nodes': {}}, Path('/unused')).read_claims(Rpc({}))

    def test_wrong_executor_and_duplicate_claim_ids_fail(self):
        for second in [self.reply('aa:0', 'b'), self.reply('bb:1', 'a')]:
            with self.subTest(second=second), self.assertRaises(AssertionError):
                self.drill().read_claims(Rpc({'aa:0': self.reply('aa:0', 'a'), 'bb:1': second}))

    def test_named_fences_are_read_from_the_current_fence_view(self):
        d = self.drill()
        d.plan['expected_consensus_params_id'] = 'pin'
        d.plan['required_fences'] = ['palw_dag_sibling_attribution_v1']
        d.probe = lambda role: {'node': {'consensusParamsId': 'pin'}}
        d.call = lambda role, method, params: {
            'available': True, 'found': True, 'viewJson': json.dumps({'fences': [
                {'name': 'palw_dag_sibling_attribution_v1', 'armed': True},
            ]}),
        }
        d.ruleset(['A'])
        d.call = lambda *args: {
            'available': True, 'found': True, 'viewJson': json.dumps({'fences': [
                {'name': 'palw_dag_sibling_attribution_v1', 'armed': False},
            ]}),
        }
        with self.assertRaises(Blocked):
            d.ruleset(['A'])

    def test_unpinned_rulesets_cannot_produce_acceptance_evidence(self):
        with self.assertRaises(Blocked):
            self.drill().ruleset(['A'])

    def test_panel_assignments_bind_the_claim_model_and_nested_seat_ids(self):
        reply = {'available': True, 'truncated': False, 'assignments': [{
            'claimId': 'claim', 'classId': 'model', 'seats': [{'seatId': 'bb:1'}, {'seatId': 'aa:0'}],
        }]}
        self.assertEqual(panel_seats_for_claim(reply, 'claim', 'model'), ['aa:0', 'bb:1'])
        for key, value in [('claimId', 'other-claim'), ('classId', 'other-model'), ('seats', [])]:
            bad = copy.deepcopy(reply)
            bad['assignments'][0][key] = value
            with self.subTest(key=key), self.assertRaises(AssertionError):
                panel_seats_for_claim(bad, 'claim', 'model')
        for bad in [{'available': True, 'claimId': 'claim', 'seats': ['aa:0']},
                    reply | {'truncated': True}]:
            with self.assertRaises(AssertionError):
                panel_seats_for_claim(bad, 'claim', 'model')

    def observation(self):
        return {'dag': {'sink': 'sink'}, 'sinkHeader': {'hash': 'sink'},
                'claims': {'claims': [{'claimId': 'claim', 'phase': 'final'}]},
                'stateRoot': 'root', 'settlement': {'available': True, 'depth': 1}}

    def converger(self, a, b):
        d = self.drill()
        d.probe = lambda role: a if role == 'A' else b
        d.block = lambda *args: {'verboseData': {
            'selectedParentHash': 'parent', 'mergeSetBluesHashes': [], 'mergeSetRedsHashes': [],
        }}
        return d

    def test_same_claim_ids_with_different_records_are_not_convergence(self):
        a, b = self.observation(), self.observation()
        b['claims']['claims'][0]['phase'] = 'voided'
        with self.assertRaises(AssertionError):
            self.converger(a, b).converged(['A', 'B'])

    def test_state_or_settlement_divergence_is_not_convergence(self):
        for key, value in [('stateRoot', 'different'), ('settlement', {'available': True, 'depth': 0})]:
            a, b = self.observation(), self.observation()
            b[key] = value
            with self.subTest(key=key), self.assertRaises(AssertionError):
                self.converger(a, b).converged(['A', 'B'])

    def test_complete_identical_observations_converge(self):
        a = self.observation()
        self.assertEqual(len(self.converger(a, copy.deepcopy(a)).converged(['A', 'B'])), 2)

    def test_siblings_merged_in_several_join_blocks_have_observed_colours(self):
        d = self.drill()
        def block(parents, selected, blues, reds):
            return {'header': {'parentsByLevel': [parents]}, 'verboseData': {
                'selectedParentHash': selected, 'mergeSetBluesHashes': blues, 'mergeSetRedsHashes': reds,
            }}
        blocks = {
            's1': block(['base'], 'base', [], []),
            's2': block(['base'], 'base', [], []),
            's3': block(['base'], 'base', [], []),
            'join1': block(['s1', 's2'], 's1', ['s1', 's2'], []),
            'join2': block(['join1', 's3'], 'join1', ['join1'], ['s3']),
        }
        d.block = lambda role, h: blocks[h]
        self.assertEqual(d.sibling_colours('A', 'join2', 'base', {'a': 's1', 'b': 's2', 'c': 's3'}),
                         {'a': 'SELECTED_PARENT', 'b': 'BLUE', 'c': 'RED'})
        # A common template may already have multiple parents. Their older
        # history is outside this experiment and need not be fetched or retained.
        for h in ['s1', 's2', 's3']:
            blocks[h]['header']['parentsByLevel'][0].append('older-tip')
        self.assertEqual(d.sibling_colours('A', 'join2', 'base', {'c': 's3'}, ['base', 'older-tip']), {'c': 'RED'})
        blocks['join2'] = block(['join1'], 'join1', ['join1'], [])
        with self.assertRaises(AssertionError):
            d.sibling_colours('A', 'join2', 'base', {'a': 's1', 'b': 's2', 'c': 's3'}, ['base', 'older-tip'])

    def test_missing_gate_c_metrics_are_not_zero_measurements(self):
        d = self.drill()
        a = self.observation()
        a['claims']['claims'][0]['securityCertification'] = 'pending'
        d.probe = lambda role: a
        with self.assertRaises(Blocked):
            d.gate_c_checks(['A'])

    def test_empty_adversary_claim_set_cannot_pass_the_attack(self):
        d = self.drill()
        d.probe = lambda role: self.observation()
        with self.assertRaises(Blocked):
            d.adversary_metrics('A', {'payout'})

    def test_one_physical_link_uses_both_directed_delays_from_the_plan(self):
        d = Drill({'nodes': {'B': {'p2p': '127.0.0.1:12345', 'wrpc': '127.0.0.1:12346'}},
                   'link_delays_ms': {'A>B': 17, 'B>A': 91}}, Path('/unused'))
        d.call = lambda *args: None
        with patch('rfc0018_dag_drill.DelayedLink') as link:
            link.return_value.address = '127.0.0.1:12347'
            d.link('A', 'B')
            link.assert_called_once_with(('127.0.0.1', 12345), 17, 91)

    def test_loopback_relay_preserves_bytes_and_applies_both_directional_delays(self):
        failures = []
        with socket.socket() as server:
            server.bind(('127.0.0.1', 0))
            server.listen()
            server.settimeout(2)

            def echo():
                try:
                    with server.accept()[0] as peer:
                        peer.settimeout(2)
                        data = bytearray()
                        while len(data) < 7:
                            chunk = peer.recv(7 - len(data))
                            if not chunk:
                                raise AssertionError('relay closed before delivering the payload')
                            data.extend(chunk)
                        self.assertEqual(data, b'forward')
                        peer.sendall(b'reverse')
                except Exception as error:
                    failures.append(error)

            worker = threading.Thread(target=echo, daemon=True)
            worker.start()
            # Exercise real loopback sockets, with delay calls recorded instead
            # of wall-clock timing assertions under a loaded build machine.
            with patch('rfc0018_dag_drill.time.sleep') as delays:
                link = DelayedLink(server.getsockname(), 17, 91)
                link.enabled.set()
                try:
                    host, port = link.address.split(':')
                    with socket.create_connection((host, int(port)), timeout=2) as client:
                        client.sendall(b'forward')
                        received = bytearray()
                        while len(received) < 7:
                            chunk = client.recv(7 - len(received))
                            self.assertTrue(chunk)
                            received.extend(chunk)
                        self.assertEqual(received, b'reverse')
                    applied = [c.args[0] for c in delays.call_args_list]
                    self.assertEqual(set(applied), {0.017, 0.091})
                    self.assertEqual((applied[0], applied[-1]), (0.017, 0.091))
                finally:
                    link.close()
                    worker.join(timeout=2)
            self.assertFalse(worker.is_alive())
            self.assertFalse(failures)


if __name__ == '__main__':
    unittest.main()
