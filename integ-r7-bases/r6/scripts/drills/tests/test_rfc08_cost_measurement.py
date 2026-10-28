"""RFC-0008 cost plan/attribution/judgement controls on synthetic rows; never a measured cost."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

TESTS = Path(__file__).resolve().parent
sys.path.insert(0, str(TESTS.parent))
import rfc08_cost_measurement as C  # noqa: E402
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_acceptance_plan as P  # noqa: E402

SPEC = importlib.util.spec_from_file_location('acceptance_plan_fixture', TESTS / 'test_rfc12_acceptance_plan.py')
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


def cost_plan():
    base = fixture.valid_plan()
    plan = {key: base[key] for key in C.TOP_KEYS if key in base}
    plan.update(schemaVersion=1, kind=C.PLAN_KIND)
    plan['decisionInputs'] = dict(capitalRatePpm=None, capitalRatePeriodDaa=None, retentionCostPerDaaSompi=None,
                                  materialWindowDaa=None, budgetWindowDaa=400, pCollectLowerPpm=None,
                                  classCostUpperSompi={'c1' * 64: None})
    plan['scenarios'] = [dict(name='small-below', modelSize='small', classId='c1' * 64, capState='below-cap',
                              window='unsaturated', setupCommands=[], request=None,
                              workCommands=[['/owned/bin/claims', '--class={ROOT}']], convictionCommands=[],
                              sampleIntervalSeconds=5, observeAfterSeconds=60)]
    return plan


def raw(plan):
    return json.dumps(plan).encode()


class CostPlan(unittest.TestCase):
    def test_parses_with_unknown_decision_inputs_kept_null(self):
        parsed = C.parse_cost_plan(raw(cost_plan()))
        self.assertIsNone(parsed['decisionInputs']['capitalRatePpm'])
        self.assertEqual(parsed['crash']['points'], [])
        self.assertIsNone(parsed['coverage'])

    def test_refusals(self):
        cases = [
            lambda p: p['scenarios'][0].update(request=None, workCommands=[]),
            lambda p: p['scenarios'].append(copy.deepcopy(p['scenarios'][0])),
            lambda p: p['decisionInputs'].update(pCollectLowerPpm=1_000_001),
            lambda p: p['decisionInputs'].update(capitalRatePpm=True),
            lambda p: p['decisionInputs'].update(classCostUpperSompi={'zz': 1}),
            lambda p: p['scenarios'][0].update(capState='over'),
            lambda p: p['scenarios'][0].update(request=dict(requestCommand=['/x'], requestFile='/tmp/r', claimSalt='44' * 64)),
            lambda p: p.pop('decisionInputs'),
            lambda p: p.update(scenarios=[]),
        ]
        for index, mutate in enumerate(cases):
            plan = cost_plan()
            mutate(plan)
            with self.subTest(case=index), self.assertRaises(P.PlanError):
                C.parse_cost_plan(raw(plan))


def tx(inputs, outputs):
    return dict(inputs=[dict(previousOutpoint=dict(transactionId=t, index=i)) for t, i in inputs],
                outputs=[dict(value=v, verboseData=dict(scriptPublicKeyAddress=a)) for v, a in outputs])


class Attribution(unittest.TestCase):
    def test_fees_payments_payouts_and_unknowns_are_exact(self):
        known = {'f0:0': (1000, 'addr:payer')}
        window = {
            'cb': tx([], [(50, 'addr:producer')]),
            'c1': tx([('f0', 0)], [(900, 'addr:payer'), (40, 'addr:provider')]),
            'c2': tx([('c1', 0)], [(880, 'addr:payer')]),
            'x1': tx([('zz', 3)], [(1, 'addr:other')]),
        }
        labels = {'addr:payer': 'provider-ack-payer (seat 3)', 'addr:producer': 'producer-own-carriers'}
        result = C.attribute(window, known, labels)
        payer = result['byPayer']['provider-ack-payer (seat 3)']
        self.assertEqual((payer['feesSompi'], payer['paymentsSompi'], payer['transactions']), (60 + 20, 40, 2))
        self.assertEqual(result['coinbasePayouts'], {'producer-own-carriers': 50})
        self.assertEqual(result['unresolvedTransactions'], ['x1'])
        self.assertEqual(J.judge(8, 'u', C.costs_measured, result)['verdict'], J.BLOCKED)
        del window['x1']
        self.assertEqual(J.judge(8, 'u', C.costs_measured, C.attribute(window, known, labels))['verdict'], J.OBSERVED)
        with self.assertRaises(J.Violation):
            C.attribute({'bad': tx([('f0', 0)], [(1001, 'addr:payer')])}, known, labels)


def sample(daa, committed='0', collateral=100, n_more=3, budget='10', convictions=0):
    return dict(virtualDaaScore=daa, committedTodaySompi=committed, collateralSompi=collateral, nMoreToday=n_more,
                rBudgetSompi=budget, convictions=convictions)


class Samples(unittest.TestCase):
    def test_capital_time_integration(self):
        samples = [sample(10, '5'), sample(20, '7'), sample(25, '0')]
        self.assertEqual(C.integrate(samples, 'committedTodaySompi'), 5 * 10 + 7 * 5)
        self.assertIsNone(C.integrate(samples[:1], 'committedTodaySompi'))
        with self.assertRaises(J.Violation):
            C.integrate([sample(20), sample(10)], 'committedTodaySompi')

    def test_declared_conditions_must_be_observed(self):
        scenario = dict(capState='at-cap', window='saturated')
        reached = [sample(1), sample(2, committed='10', n_more=0)]
        self.assertEqual(C.scenario_conditions(scenario, reached)['capState'], 'at-cap')
        self.assertEqual(J.judge(8, 'u', C.scenario_conditions, scenario, [sample(1)])['verdict'], J.BLOCKED)
        self.assertEqual(J.judge(8, 'u', C.scenario_conditions, scenario, [])['verdict'], J.BLOCKED)
        self.assertEqual(J.judge(8, 'u', C.scenario_conditions, scenario, [dict(sample(1), nMoreToday=None)])['verdict'],
                         J.BLOCKED)

    def test_collection_floor_needs_an_actual_conviction(self):
        floor = C.collection_floor([sample(1), sample(2, collateral=80, convictions=1)], '10', '25')
        self.assertEqual((floor['collectibleCollateralSompi'], floor['burnedDeltaSompi']), (80, 15))
        self.assertEqual(J.judge(8, 'u', C.collection_floor, [sample(1), sample(2)], '0', '0')['verdict'], J.BLOCKED)

    def test_derived_quantities_are_unknown_until_supplied(self):
        inputs = C.parse_cost_plan(raw(cost_plan()))['decisionInputs']
        derived = C.derived(dict(capitalCommittedSompiDaa=1000, measuredCostSompi=None), inputs, 'c1' * 64)
        self.assertEqual(derived['capitalCostSompi'], C.UNKNOWN)
        self.assertEqual(derived['measuredWithinCUpper'], C.UNKNOWN)
        self.assertEqual(derived['budgetWindowDaa'], 400)
        supplied = dict(inputs, capitalRatePpm=50_000, capitalRatePeriodDaa=100, pCollectLowerPpm=500_000,
                        classCostUpperSompi={'c1' * 64: 1000})
        derived = C.derived(dict(capitalCommittedSompiDaa=1000, measuredCostSompi=900), supplied, 'c1' * 64)
        self.assertEqual(derived['capitalCostSompi'], 1)  # ceil(1000 * 50000 / (1e6 * 100))
        self.assertEqual(derived['requiredMinimumSompi'], 2000)
        self.assertTrue(derived['measuredWithinCUpper'])
        self.assertEqual(J.judge(8, 'u', C.decision_inputs_supplied, inputs, 'c1' * 64)['verdict'], J.BLOCKED)

    def test_matrix_needs_every_cell(self):
        rows = [dict(modelSize=size, observedCapState=cap, observedWindow='unsaturated')
                for size in C.MODEL_SIZES for cap in C.CAP_STATES]
        self.assertEqual(J.judge(8, 'u', C.matrix_covered, rows)['verdict'], J.BLOCKED)  # no saturated window
        rows[0]['observedWindow'] = 'saturated'
        self.assertEqual(J.judge(8, 'u', C.matrix_covered, rows)['verdict'], J.OBSERVED)
        self.assertEqual(J.judge(8, 'u', C.matrix_covered, rows[1:])['verdict'], J.BLOCKED)


if __name__ == '__main__':
    unittest.main()
