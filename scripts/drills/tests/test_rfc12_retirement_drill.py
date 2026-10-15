"""Shipped-binary drill reader controls (plan, keyring, retirement, agreement); never a drill observation."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('retirement_drill_under_test', SCRIPTS / 'misaka-rfc12-retirement-drill.py')
drill = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(drill)
J, P, R = drill.J, drill.P, drill.R


def plan():
    return {
        'schemaVersion': 1, 'kind': 'rfc12-retirement-drill-plan', 'commit': 'ab' * 20,
        'shippedBinary': {'path': '/owned/bin/kaspad', 'sha256': 'cd' * 32}, 'salt': '74' * 32,
        'identity': {'genesis': '11' * 64, 'consensusParamsId': '22' * 32, 'retirementActivationDaa': 14000,
                     'retirementPolicyId': '33' * 64},
        'nodes': [{'name': 'seat0', 'wrpc': '127.0.0.1:27711', 'archival': True},
                  {'name': 'seat1', 'wrpc': '127.0.0.1:27712', 'archival': False}],
        'keyringManifest': {'path': '/owned/keyring/manifest.json', 'sha256': 'ef' * 32},
        'palwPayoutGroups': ['seats', 'payout'],
        'window': {'scanFromDaa': 0, 'observeAfterFenceDaa': 700, 'legacySettlementWindowDaa': 603,
                   'dnsCadence': {'epochLengthBlocks': 2, 'rewardUniquenessWindowBlocks': 600, 'maxReorgHorizonBlocks': 3},
                   'derivation': 'node rule, see the template'},
        'scan': {'maximumChainBlocks': 100000, 'maximumBlockReads': 200000, 'maximumStatusReads': 5000,
                 'bondPageLimit': 1000, 'maximumBondPages': 100},
        'bounds': {'workflowTimeoutSeconds': 86400, 'probeTimeoutSeconds': 30, 'pollSeconds': 60},
    }


def raw(value):
    return json.dumps(value).encode()


class DrillPlan(unittest.TestCase):
    def test_valid_plan_and_refusals(self):
        self.assertEqual(drill.parse_drill_plan(raw(plan()))['identity']['retirementActivationDaa'], 14000)
        cases = [
            lambda p: p['nodes'][0].update(wrpc='10.0.0.5:27711'),
            lambda p: p['nodes'][0].update(archival=False),
            lambda p: p['identity'].update(retirementActivationDaa=1),
            lambda p: p['window'].update(scanFromDaa=14000),
            lambda p: p['window'].update(legacySettlementWindowDaa=300),
            lambda p: p['window'].update(observeAfterFenceDaa=603),
            lambda p: p['window'].update(derivation=''),
            lambda p: p['window']['dnsCadence'].update(maxReorgHorizonBlocks=4),
            lambda p: p.update(palwPayoutGroups=['validator']),
            lambda p: p['scan'].update(bondPageLimit=1001),
            lambda p: p['nodes'].append(dict(p['nodes'][0])),
            lambda p: p.pop('keyringManifest'),
        ]
        for index, mutate in enumerate(cases):
            changed = copy.deepcopy(plan())
            mutate(changed)
            with self.subTest(case=index), self.assertRaises(P.PlanError):
                drill.parse_drill_plan(raw(changed))

    def test_the_committed_template_refuses_until_pinned(self):
        template = SCRIPTS / 'plans' / 'rfc12-retirement-drill-t12-daa14000.template.json'
        data = json.loads(template.read_bytes())
        self.assertEqual(data['identity']['retirementActivationDaa'], 14000)
        with self.assertRaises(P.PlanError):
            drill.parse_drill_plan(template.read_bytes())


class Reader(unittest.TestCase):
    def test_keyring_groups(self):
        manifest = dict(seats=[dict(address='misakatest:s0')], main=dict(address='misakatest:m'), payout=[])
        self.assertEqual(drill.keyring_addresses(manifest, ['seats', 'main']), {'misakatest:s0', 'misakatest:m'})
        with self.assertRaises(J.NotObservable):
            drill.keyring_addresses(manifest, ['heartbeat'])

    def test_shipped_retirement_is_read_not_assumed(self):
        identity = dict(retirementActivationDaa=14000)
        row = dict(source='node-rpc', role='seat0', value=dict(settlement=dict(dnsRetiredAt=14000), dns=dict(available=False)))
        self.assertEqual(J.judge(7, 'u', drill.shipped_retirement, row, identity)['verdict'], J.OBSERVED)
        dormant = dict(row, value=dict(settlement=dict(dnsRetiredAt=None), dns=dict(available=False)))
        self.assertEqual(J.judge(7, 'u', drill.shipped_retirement, dormant, identity)['verdict'], J.BLOCKED)
        live = dict(row, value=dict(settlement=dict(dnsRetiredAt=14000), dns=dict(available=True)))
        self.assertEqual(J.judge(7, 'u', drill.shipped_retirement, live, identity)['verdict'], J.FAIL)

    def test_archival_nodes_must_classify_alike(self):
        block = dict(hash='aa' * 64, outputs=[dict(index=0, kind=R.DNS, bond='b:0')])
        other = dict(block, outputs=[dict(index=0, kind=R.MINER, bond=None)])
        self.assertEqual(J.judge(7, 'u', drill.classifications_agree, {'a': [block], 'b': [block]})['verdict'], J.OBSERVED)
        self.assertEqual(J.judge(7, 'u', drill.classifications_agree, {'a': [block], 'b': [other]})['verdict'], J.FAIL)
        self.assertEqual(J.judge(7, 'u', drill.classifications_agree, {'a': [block]})['verdict'], J.BLOCKED)


if __name__ == '__main__':
    unittest.main()
