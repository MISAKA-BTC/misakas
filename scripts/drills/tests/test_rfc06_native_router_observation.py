"""Collector counterexamples; these unit fixtures are not live court evidence."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc06_native_router_observation as R


def fixture():
    claim, klass = '11' * 64, '22' * 64
    producer = '33' * 64 + ':0'
    wire = struct.pack('<HQIBHH', 1, 10, 1, 0, 8, 3)
    expected = dict(classId=klass, producerBond=producer, bindingSha256=hashlib.sha256(wire).hexdigest(), table=0, reader=8, router=3)
    value = dict(version=1, claimId=claim, classId=klass, producer=producer, acceptedDaa=11, courtConvicted=True)
    dispute = dict(available=True, observationVersion=1, json=json.dumps(value))
    binding = dict(available=True, armed=True, found=True, nowDaa=12, boundFromDaa=10, rowHex=wire.hex())
    return claim, expected, value, dispute, binding


class RouterObservation(unittest.TestCase):
    def test_binding_is_observed_but_generic_conviction_never_satisfies_route_gate(self):
        claim, expected, _, dispute, binding = fixture()
        observation = R.validate_payload(claim, expected, dispute, binding, 12)
        requirement = dict(id='router', kind='reader-router-court', role='A', claimId=claim)
        fact = R.fact(requirement, observation)
        self.assertTrue(fact['routerBindingObserved'])
        self.assertEqual(fact['status'], 'UNKNOWN')
        self.assertEqual(fact['routeAdjudication'], 'UNKNOWN')
        self.assertFalse(fact['nativeAuthorityEstablished'])

    def test_another_claim_class_producer_or_future_claim_is_refused(self):
        claim, expected, value, dispute, binding = fixture()
        for field, other in [('claimId', '44'*64), ('classId', '44'*64), ('producer', '44'*64+':0'),
                             ('acceptedDaa', 13), ('acceptedDaa', True)]:
            with self.subTest(field=field, other=other):
                changed = dict(value, **{field: other})
                with self.assertRaises(ValueError):
                    R.validate_payload(claim, expected, dict(dispute, json=json.dumps(changed)), binding, 12)

    def test_late_binding_wrong_row_or_incomplete_binding_is_refused(self):
        claim, expected, _, dispute, binding = fixture()
        changes = [dict(boundFromDaa=12), dict(nowDaa=13), dict(found=False), dict(armed=False),
                   dict(rowHex=binding['rowHex']+'00'), dict(rowHex='00'*14), dict(rowHex=binding['rowHex'].upper())]
        # This particular hex row contains only digits; use explicit noncanonical encoding separately.
        changes[-1] = dict(rowHex='AA' + binding['rowHex'][2:])
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                R.validate_payload(claim, expected, dispute, dict(binding, **change), 12)
        with self.assertRaises(ValueError):
            R.validate_payload(claim, dict(expected, reader=9), dispute, binding, 12)
        with self.assertRaises(ValueError):
            R.validate_payload(claim, dict(expected, bindingSha256='00'*32), dispute, binding, 12)

    def test_scope_cannot_inject_authority_or_loose_bond(self):
        _, expected, _, _, _ = fixture()
        for change in [dict(expectedConvicted=True), dict(table=True), dict(router=8),
                       dict(producerBond='33'*64+':00')]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                R.validate_expected(dict(expected, **change))

    def test_exact_native_committed_outcome_closes_only_its_predeclared_route(self):
        claim, expected, _, _, _ = fixture()
        expected['court'] = dict(blockHash='55'*64, objectId='66'*64, objectTag=244,
                                 route='RoutedStep', leafIndex=17, verdict='ExecutorGuilty')
        R.validate_expected(expected)
        requirement = dict(id='router', kind='reader-router-court', role='A', claimId=claim, expected=expected)
        item = dict(claimId=claim, classId=expected['classId'], **{k: v for k, v in expected['court'].items() if k != 'blockHash'})
        envelope = dict(version=1, kind='nativeCommittedMemoryRoutingOutcomeV1', nodeLocalObservationOnly=True,
                        releaseApproval=False, securitySoundnessMeasured=False, sink='77'*64,
                        observation=dict(block='55'*64, postStateRoot='88'*64, daa=12, outcomes=[item]))
        dag = dict(sink='77'*64, virtualDaaScore=13)
        response = dict(available=True, observationJson=json.dumps(envelope))
        observed = R.validate_outcome(response, requirement, dag)
        self.assertEqual(R.fact(requirement, dict(courtOutcome=observed))['status'], 'PASS')
        for key, value in [('objectId', '99'*64), ('claimId', '99'*64), ('route', 'GdnStep'), ('verdict', 'FalseAccusation')]:
            changed = deepcopy(envelope)
            changed['observation']['outcomes'][0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                R.validate_outcome(dict(response, observationJson=json.dumps(changed)), requirement, dag)
        for elapsed in (None, 0, 10_000):
            changed = deepcopy(envelope)
            changed['observation']['outcomes'][0]['adjudicationAndSettlementMicros'] = elapsed
            R.validate_outcome(dict(response, observationJson=json.dumps(changed)), requirement, dag)
        for elapsed in (True, -1, 2**64, '100'):
            changed = deepcopy(envelope)
            changed['observation']['outcomes'][0]['adjudicationAndSettlementMicros'] = elapsed
            with self.subTest(elapsed=elapsed), self.assertRaises(ValueError):
                R.validate_outcome(dict(response, observationJson=json.dumps(changed)), requirement, dag)
        with self.assertRaises(ValueError):
            R.validate_outcome(response, requirement, dict(dag, sink='99'*64))
        self.assertIsNone(R.validate_outcome(dict(available=False, reason='restart'), requirement, dag))

    def test_bracket_change_and_unstable_import_refuse_and_close_rpc(self):
        claim, expected, _, dispute, binding = fixture()
        requirement = dict(role='A', claimId=claim, expected=expected)
        class Rpc:
            def __init__(self, moved):
                self.closed = False
                self.responses = iter([dict(sink='a', virtualDaaScore=12), dict(stable=True), dispute, binding,
                                       dict(sink='b' if moved else 'a', virtualDaaScore=12), dict(stable=True)])
            def call(self, *args):
                return next(self.responses)
            def close(self):
                self.closed = True
        class Drill:
            def __init__(self, rpc, stable=True):
                self.client, self.stable = rpc, stable
            def rpc(self, role):
                return self.client
            def assert_network(self, observation):
                return self.stable
        context = lambda dag, node: (dag['sink'], dag['virtualDaaScore'], node['stable'])
        for moved, stable in [(True, True), (False, False)]:
            rpc = Rpc(moved)
            with self.assertRaises(ValueError):
                R.observe(Drill(rpc, stable), requirement, context)
            self.assertTrue(rpc.closed)
        rpc = Rpc(False)
        result = R.observe(Drill(rpc), requirement, context)
        self.assertEqual(result['binding']['acceptedDaa'], 11)
        self.assertTrue(rpc.closed)


if __name__ == '__main__':
    unittest.main()
