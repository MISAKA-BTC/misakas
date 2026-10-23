#!/usr/bin/env python3
"""RFC-0008 cost measurement on one owned private paid-v4 network (checkbox 8).

Reuses the RFC-0012 acceptance run's identity, launch, custody bootstrap and real
paid useful-work cycle, then runs each pinned scenario (small/large class,
below/at the bond-budget cap, unsaturated/saturated W_budget window) between two
generation-bracketed snapshots. Per scenario it records accepted native fees and
payments attributed to the exact funding address, carrier totals of every
dedicated payer (provider ACK, producer's own enrollment/replacement/lifecycle
carriers, kernel claim, selected-checker receipts, CLI funding), coinbase and
vesting payouts, capital locked x time from capacity-shadow samples, and the
collection floor realised at a conviction. Decision inputs are plan fields and
everything derived from a missing one is UNKNOWN.

Verdicts: OBSERVED / BLOCKED / FAIL / NOT_RUN; never PASS. Nothing here sets a
price, a cap or a release threshold.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rfc08_cost_measurement as C  # noqa: E402
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_acceptance_observe as O  # noqa: E402
import rfc12_acceptance_plan as P  # noqa: E402


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


acceptance = _load('rfc08_cost_acceptance_driver', 'misaka-rfc12-acceptance.py')
transport = acceptance.transport


class CostRun(acceptance.AcceptanceRun):
    def __init__(self, plan, root, limits, out):
        super().__init__(plan, root, limits, out)
        self.boxes, self.identity_box, self.work_box, self.resource_box = (8,), 8, 8, 8
        self.samples, self.sample_every, self.last_sample, self.results = None, None, 0.0, []
        self.evidence.update(kind='rfc08-cost-measurement-evidence', decisionInputs=plan['decisionInputs'])

    def labels(self):
        seats, uw = self.manifest['seats'], self.plan['usefulWork']
        labels = {seat['payAddress']: 'seat-%d' % index for index, seat in enumerate(seats)}
        labels[self.manifest['fundingAddress']] = 'cli-funding'
        labels[seats[0]['payAddress']] = 'producer-own-carriers (seat 0: enrollment/replacement/lifecycle, not split by kind)'
        labels[seats[P.PROVIDER_PAYER_SEAT]['payAddress']] = 'provider-ack-payer (seat 3)'
        labels[seats[uw['kernelPayerSeat']]['payAddress']] = 'kernel-claim-payer (seat %d)' % uw['kernelPayerSeat']
        for seat in uw['checker']['payerSeats']:
            labels[seats[seat]['payAddress']] = 'checker-receipt-payer (seat %d)' % seat
        return labels

    def tick(self):
        if self.samples is None or time.monotonic() - self.last_sample < self.sample_every:
            return
        try:
            self.sample()
        except (O.ContextMoved, OSError, RuntimeError, J.Pending):
            pass  # the next tick retries; a missed sample only widens one integration step

    def sample(self):
        self.last_sample = time.monotonic()
        producer = self.manifest['seats'][0]['bond']

        def body(rpc, dag, node):
            shadow = rpc.call('getPalwCapacityShadow', {'steps': [], 'bond': producer, 'adversaryBonds': [],
                                                        'includeClaims': False, 'limit': 64})
            row = next((bond for bond in shadow.get('bonds') or [] if bond.get('bond') == producer), None)
            return dict(available=shadow.get('available'), convictionsTotal=shadow.get('convictionsTotal'), bond=row)
        row = O.bracketed(self.drill, 'JOIN', 'capital-sample', body)
        bond = row['value']['bond'] or {}
        self.samples.append(dict(source='node-rpc', virtualDaaScore=row['dag']['virtualDaaScore'], sink=J.norm(row['dag']['sink']),
                                 committedTodaySompi=bond.get('committedTodaySompi'), collateralSompi=bond.get('collateralSompi'),
                                 rBudgetSompi=bond.get('rBudgetSompi'), nMoreToday=bond.get('nMoreToday'),
                                 convictions=bond.get('convictions'), convictionsTotal=row['value']['convictionsTotal'],
                                 shadowAvailable=row['value']['available']))

    def snapshot(self, label):
        addresses = sorted(self.labels())
        maximum = self.plan['scan']['maximumUtxoRowsPerAddress']

        def body(rpc, dag, node):
            vest = rpc.call('getPalwRewardVesting', {'bond': '', 'payoutAddress': '', 'claimId': '', 'limit': 1,
                                                     'after': '', 'execAfter': ''})
            native = vest.get('native') or {}
            return dict(entries=O.utxo_entries(rpc, addresses, maximum),
                        vesting=dict(available=bool(vest.get('available') and native.get('available')),
                                     createdSompi=native.get('createdSompi'), movedSompi=native.get('movedSompi'),
                                     burnedSompi=native.get('burnedSompi'), outstandingSompi=vest.get('outstandingSompi')))
        row = self.wait(lambda: O.bracketed(self.drill, 'JOIN', label, body), label)
        self.rows.append(row)
        return row

    def require_class(self, class_id):
        row = self.wait(lambda: O.bracketed(self.drill, 'JOIN', 'class-registered',
                                            lambda rpc, dag, node: rpc.call('getPalwOnboarding', {'classId': class_id})),
                        'scenario class read')
        if not row['value'].get('available') or not row['value'].get('found'):
            raise transport.Blocked('scenario class %s is not registered on the chain (setup commands must register it)'
                                    % class_id[:16])

    def run_scenario(self, scenario):
        name = scenario['name']
        before = self.snapshot('before:' + name)
        self.samples, self.sample_every = [], scenario['sampleIntervalSeconds']
        self.sample()
        try:
            self.fixtures('SETUP_' + name, scenario['setupCommands'])
            self.require_class(scenario['classId'])
            if scenario['request']:
                uw = dict(self.plan['usefulWork'], **scenario['request'])
                self.phase_useful_work(uw, self.root / 'work' / name)
            self.fixtures('WORK_' + name, scenario['workCommands'])
            self.fixtures('CONVICT_' + name, scenario['convictionCommands'])
            until = time.monotonic() + scenario['observeAfterSeconds']
            self.wait(lambda: time.monotonic() >= until or None, 'post-scenario observation window',
                      seconds=scenario['observeAfterSeconds'] + 5)
            self.sample()
        finally:
            samples, self.samples = self.samples, None
        after = self.snapshot('after:' + name)
        rpc = self.drill.rpc('JOIN')
        try:
            window = O.accepted_transactions(rpc, J.norm(before['dag']['sink']), None, self.plan['scan'],
                                             end=J.norm(after['dag']['sink']))
        finally:
            rpc.close()
        known = {J.outpoint_key(entry): (entry['utxoEntry']['amount'], entry['address']) for entry in before['value']['entries']}
        attribution = C.attribute({txid: row['transaction'] for txid, row in window.items()}, known, self.labels())
        payers = attribution['byPayer']

        def fees(prefix):
            return sum(row['feesSompi'] for label, row in payers.items() if label.startswith(prefix))
        tracked = sum(row['feesSompi'] for label, row in payers.items() if label != 'other')
        vest_before, vest_after = before['value']['vesting'], after['value']['vesting']

        def delta(field):
            if vest_before.get(field) is None or vest_after.get(field) is None:
                return None
            return int(vest_after[field]) - int(vest_before[field])
        measured = dict(window=dict(fromSink=J.norm(before['dag']['sink']), toSink=J.norm(after['dag']['sink']),
                                    fromDaa=before['dag']['virtualDaaScore'], toDaa=after['dag']['virtualDaaScore']),
                        acceptedNativeFeesAndPaymentsByPayer=payers,
                        providerAckCarrierSompi=fees('provider-ack-payer'),
                        producerOwnCarrierSompi=fees('producer-own-carriers'),
                        kernelClaimCarrierSompi=fees('kernel-claim-payer'),
                        checkerReceiptCarrierSompi=fees('checker-receipt-payer'),
                        cliFeeSompi=fees('cli-funding'),
                        coinbasePayoutsByAddressLabel=attribution['coinbasePayouts'],
                        vestingDeltaSompi=dict(created=delta('createdSompi'), moved=delta('movedSompi'), burned=delta('burnedSompi')),
                        capitalCommittedSompiDaa=C.integrate(samples, 'committedTodaySompi'),
                        capitalCollateralSompiDaa=C.integrate(samples, 'collateralSompi'),
                        measuredCarrierCostSompi=None if attribution['unresolvedTransactions'] else tracked,
                        unresolvedTransactions=attribution['unresolvedTransactions'])
        inputs = self.plan['decisionInputs']
        partial = C.derived(dict(capitalCommittedSompiDaa=measured['capitalCommittedSompiDaa']), inputs, scenario['classId'])
        capital = partial['capitalCostSompi']
        measured['measuredCostSompi'] = None if measured['measuredCarrierCostSompi'] is None or capital == C.UNKNOWN \
            else measured['measuredCarrierCostSompi'] + capital
        result = dict(name=name, modelSize=scenario['modelSize'], classId=scenario['classId'],
                      declared=dict(capState=scenario['capState'], window=scenario['window']), samples=samples,
                      measured=measured, derived=C.derived(measured, inputs, scenario['classId']))
        conditions = self.judge(8, 'scenario %s: declared cap/window condition observed' % name, C.scenario_conditions,
                                scenario, samples)
        try:
            observed = C.observed_conditions(samples)
            result.update(observedCapState=observed['capState'], observedWindow=observed['window'])
        except J.NotObservable:
            pass
        result['conditionsMatchDeclared'] = conditions is not None
        self.judge(8, 'scenario %s: accepted fees, payments and carriers measured exactly' % name, C.costs_measured, attribution)
        if scenario['convictionCommands']:
            floor = self.judge(8, 'scenario %s: collection floor realised at conviction' % name, C.collection_floor, samples,
                               vest_before.get('burnedSompi'), vest_after.get('burnedSompi'))
            result['collectionFloor'] = floor
        else:
            self.block(8, 'scenario %s: collection floor realised at conviction' % name,
                       'the scenario drives no conviction, so no collectible collateral was realised')
        self.judge(8, 'scenario %s: decision inputs supplied' % name, C.decision_inputs_supplied, inputs, scenario['classId'])
        self.results.append(result)
        self.evidence['scenarios'] = self.results

    def phase_matrix(self):
        self.judge(8, 'cost matrix covers small/large x below/at cap and a saturated window', C.matrix_covered, self.results)

    def execute(self):
        steps = [('identity', (8,), (), self.phase_identity),
                 ('launch', (8,), ('identity',), self.phase_launch),
                 ('bootstrap', (8,), ('nodes',), self.phase_bootstrap)]
        for scenario in self.plan['scenarios']:
            steps.append(('scenario:' + scenario['name'], (8,), ('custody',), lambda scenario=scenario: self.run_scenario(scenario)))
        steps.append(('matrix', (8,), ('custody',), self.phase_matrix))
        for name, boxes, requires, function in steps:
            self.phase(name, boxes, requires, function)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True)
    args = parser.parse_args(argv)
    raw = Path(args.plan).read_bytes()
    try:
        plan = C.parse_cost_plan(raw)
        limits = transport.read_process_limits(plan['observationBoundsFile'])
    except (P.PlanError, ValueError, OSError) as error:
        print(json.dumps(dict(status=J.BLOCKED, reason='plan refused before launch: %s' % error, passLabelUsed=False)))
        return 2
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc08-cost-')).resolve()
    (root / 'plan.json').write_bytes(raw)
    run = CostRun(plan, root, limits, Path(args.out).resolve())
    try:
        run.execute()
    finally:
        try:
            run.close()
        finally:
            run.write_evidence()
    print(json.dumps({key: run.evidence[key] for key in ('status', 'artifactDirectory', 'planSha256', 'stopped',
                                                         'passLabelUsed')}, indent=2))
    return 0 if run.evidence['status'] == J.OBSERVED else 1


if __name__ == '__main__':
    raise SystemExit(main())
