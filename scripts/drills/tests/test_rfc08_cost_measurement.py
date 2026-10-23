"""RFC-0008 cost plan/attribution/judgement controls on synthetic rows; never a measured cost."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock
from types import SimpleNamespace

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
    def test_multiple_inputs_need_one_exact_payer_not_the_first_input_owner(self):
        known = {'a:0': (100, 'addr:a'), 'b:0': (100, 'addr:b'), 'a:1': (100, 'addr:a')}
        labels = {'addr:a': 'producer', 'addr:b': 'provider'}
        same = C.attribute({'s': tx([('a', 0), ('a', 1)], [(190, 'addr:a')])}, known, labels)
        self.assertEqual(same['byPayer']['producer']['feesSompi'], 10)
        for points in [[('a', 0), ('b', 0)], [('b', 0), ('a', 0)]]:
            mixed = C.attribute({'m': tx(points, [(190, 'addr:a')])}, known, labels)
            self.assertEqual(mixed['byPayer'], {})
            self.assertEqual(mixed['unresolvedTransactions'], ['m'])
            self.assertEqual(J.judge(8, 'mixed', C.costs_measured, mixed)['verdict'], J.BLOCKED)
        unknown = C.attribute({'u-spend': tx([('u', 0)], [(90, 'addr:a')])}, {'u:0': (100, None)}, labels)
        self.assertEqual(unknown['unresolvedTransactions'], ['u-spend'])
        with self.assertRaises(J.Violation):
            C.attribute({'d': tx([('a', 0), ('a', 0)], [(190, 'addr:a')])}, known, labels)

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
    def test_missing_capital_samples_do_not_bridge_an_unknown_interval(self):
        for samples in [
            [sample(10, None), sample(20, '7'), sample(30, '0')],
            [sample(10, '5'), sample(20, None), sample(30, '0')],
        ]:
            self.assertIsNone(C.integrate(samples, 'committedTodaySompi'))
        self.assertEqual(C.integrate([sample(10, '5'), sample(20, '7'), sample(30, None)],
                                     'committedTodaySompi'), 5 * 10 + 7 * 10)
        for invalid in [True, 1.5, '1.5', '-1', '01', '1e2']:
            with self.assertRaises(J.Violation):
                C.integrate([sample(10, invalid), sample(20, '0')], 'committedTodaySompi')
        for daa in [True, 10.5]:
            with self.assertRaises(J.Violation):
                C.integrate([sample(daa, '5'), sample(20, '0')], 'committedTodaySompi')

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

    def test_unknown_budget_cannot_be_called_unsaturated(self):
        for field in ['committedTodaySompi', 'rBudgetSompi']:
            missing = dict(sample(1))
            missing[field] = None
            with self.assertRaises(J.NotObservable):
                C.observed_conditions([missing])
        for field, value in [('nMoreToday', False), ('committedTodaySompi', '01'), ('rBudgetSompi', 1.5)]:
            malformed = dict(sample(1))
            malformed[field] = value
            with self.assertRaises(J.Violation):
                C.observed_conditions([malformed])

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


DRIVER_SPEC = importlib.util.spec_from_file_location('cost_driver_under_test', TESTS.parent / 'misaka-rfc08-cost-measure.py')
D = importlib.util.module_from_spec(DRIVER_SPEC)
DRIVER_SPEC.loader.exec_module(D)


def capital_row(daa, sink, committed='5', **node_changes):
    node = dict(importSessionAvailable=True, importSessionEpoch=7, pruningImportStable=True,
                importSessionPoint='', importSessionPhase=next(iter(J.STABLE_IMPORT_PHASES)))
    node.update(node_changes)
    return dict(dag=dict(virtualDaaScore=daa, sink=sink), node=node,
                value=dict(capital=dict(sample(daa, committed), shadowAvailable=True)))


class CapitalWindow(unittest.TestCase):
    def test_native_endpoints_and_epoch_must_match_the_fee_window(self):
        before, after = capital_row(10, 'a'), capital_row(20, 'b')
        rows = [D.CostRun.capital_sample(before), D.CostRun.capital_sample(after)]
        result = C.capital_measured(rows, before['dag'], after['dag'])
        self.assertEqual(result['committedSompiDaa'], 50)
        self.assertEqual(result['collateralSompiDaa'], 1000)
        self.assertIn('not measured', result['assumption'])
        for mutation in [lambda r: r[0].update(sink='other'), lambda r: r[-1].update(virtualDaaScore=21),
                         lambda r: r[-1].update(importEpoch=8), lambda r: r[0].pop('sampleStatus'),
                         lambda r: r[0].update(shadowAvailable=False)]:
            changed = copy.deepcopy(rows)
            mutation(changed)
            with self.assertRaises(J.NotObservable):
                C.capital_measured(changed, before['dag'], after['dag'])

    def test_failed_poll_is_preserved_and_no_later_recovery_erases_the_gap(self):
        for error in [OSError('socket closed'), RuntimeError('RPC failed'), J.Pending('not ready'),
                      J.NotObservable('import unavailable'), D.O.ContextMoved('fork moved')]:
            run = D.CostRun.__new__(D.CostRun)
            run.samples, run.sample_every, run.last_sample = [], 1, 0
            run.manifest, run.drill = dict(seats=[dict(bond='producer')]), object()
            before, after = capital_row(10, 'a'), capital_row(20, 'b')
            run.samples.append(run.capital_sample(before))
            with mock.patch.object(D.O, 'bracketed', side_effect=[error, after]), mock.patch.object(D.time, 'monotonic', return_value=100):
                run.tick()
                self.assertEqual(len(run.samples), 2)
                self.assertEqual(run.samples[1]['sampleStatus'], 'unavailable')
                self.assertNotIn('virtualDaaScore', run.samples[1], 'failure invents no native clock')
                self.assertEqual(run.samples[1]['error'], str(error))
                run.last_sample = 0
                run.tick()
            self.assertEqual(len(run.samples), 3)
            self.assertIsNone(C.integrate(run.samples, 'committedTodaySompi'))
            with self.assertRaises(J.NotObservable):
                C.capital_measured(run.samples, before['dag'], after['dag'])
            with self.assertRaises(J.NotObservable):
                C.observed_conditions(run.samples)
            with self.assertRaises(J.NotObservable):
                C.collection_floor(run.samples, 0, 100)

    def test_unstable_import_cannot_supply_a_balance(self):
        for changes in [dict(pruningImportStable=False), dict(importSessionPoint='active'), dict(importSessionAvailable=False)]:
            result = D.CostRun.capital_sample(capital_row(10, 'a', **changes))
            self.assertEqual(result['sampleStatus'], 'unavailable')
            self.assertNotIn('committedTodaySompi', result)

    def scenario_run(self, failed_after_poll=False):
        run = D.CostRun.__new__(D.CostRun)
        before, after = capital_row(10, 'a'), capital_row(20, 'b')
        for row in [before, after]:
            row['value'].update(entries=[], vesting=dict(available=True, createdSompi=0, movedSompi=0, burnedSompi=0))
        run.samples, run.findings = None, []

        def snapshot(label):
            if label.startswith('before:'):
                return before
            self.assertIsNotNone(run.samples, 'sampling stays active through the final snapshot read')
            if failed_after_poll:
                run.samples.append(dict(sampleStatus='unavailable', source='node-rpc-unavailable', error='missed final poll'))
            return after
        run.snapshot = snapshot
        run.sample = mock.Mock(side_effect=AssertionError('separate endpoint poll must not run'))
        run.labels = lambda: {}
        run.fixtures = lambda *args: None
        run.require_class = lambda *args: None
        run.wait = lambda *args, **kwargs: None
        run.block = lambda *args: None
        def judge(box, title, fn, *args):
            item = J.judge(box, title, fn, *args)
            run.findings.append(item)
            return item['facts'] if item['verdict'] == J.OBSERVED else None
        run.judge = judge
        rpc = SimpleNamespace(close=lambda: None)
        run.drill = SimpleNamespace(rpc=lambda role: rpc)
        run.plan = dict(scan={}, decisionInputs=dict(capitalRatePpm=1000000, capitalRatePeriodDaa=1,
                       retentionCostPerDaaSompi=None, materialWindowDaa=None, budgetWindowDaa=None,
                       pCollectLowerPpm=None, classCostUpperSompi={}))
        run.root, run.results, run.evidence = Path('/owned'), [], {}
        scenario = dict(name='sample-window', sampleIntervalSeconds=5, setupCommands=[], request=None,
                        classId='c', workCommands=[], convictionCommands=[], observeAfterSeconds=0,
                        modelSize='small', capState='below-cap', window='unsaturated')
        with mock.patch.object(D.O, 'accepted_transactions', return_value={}):
            run.run_scenario(scenario)
        return run

    def test_scenario_uses_the_snapshot_endpoints_not_two_nearby_polls(self):
        run = self.scenario_run()
        result = run.results[0]
        self.assertEqual([row['virtualDaaScore'] for row in result['samples']], [10, 20])
        self.assertEqual(result['measured']['capitalCommittedSompiDaa'], 50)
        self.assertEqual(result['measured']['capitalObservation']['fromSink'], 'a')
        self.assertEqual(result['measured']['capitalObservation']['toSink'], 'b')
        self.assertIsNone(run.samples)


    def test_final_snapshot_poll_failure_blocks_total_cost_despite_later_recovery(self):
        run = self.scenario_run(failed_after_poll=True)
        result = run.results[0]
        self.assertEqual(result['samples'][1]['sampleStatus'], 'unavailable')
        self.assertEqual(result['samples'][-1]['sampleStatus'], 'observed')
        self.assertIsNone(result['measured']['capitalCommittedSompiDaa'])
        self.assertIsNone(result['measured']['capitalCollateralSompiDaa'])
        self.assertIsNone(result['measured']['measuredCostSompi'])
        self.assertEqual(result['derived']['capitalCostSompi'], C.UNKNOWN)
        self.assertEqual(result['derived']['measuredCostSompi'], C.UNKNOWN)
        capital = next(row for row in run.findings if 'capital polls' in row['predicate'])
        self.assertEqual(capital['verdict'], J.BLOCKED)


if __name__ == '__main__':
    unittest.main()
