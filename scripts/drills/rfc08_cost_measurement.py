"""RFC-0008 cost measurement: plan schema, exact per-payer attribution and judgement (checkbox 8).

Scenarios run on one owned private paid-v4 network and each is measured between
two generation-bracketed snapshots:
  * accepted native fees and payments, attributed exactly to the address that
    funded each accepted transaction (inputs resolved from the scenario window
    or the opening UTXO snapshot; an unresolved input leaves that transaction
    UNKNOWN, never estimated);
  * carrier totals per dedicated payer: provider ACK payer (seat 3), producer's
    own carriers (enrollment / replacement / lifecycle, seat 0), kernel claim
    payer, selected-checker receipt payers, CLI funding key;
  * coinbase reward payouts per address and vesting created/moved/burned deltas;
  * capital locked x time, integrated from capacity-shadow samples of the
    producer bond (committed and collateral), in sompi*DAA;
  * the collection floor actually realised: the producer bond's collateral at
    the first sample after a conviction, and the burned delta.
Decision inputs (capital_rate_ppm and its period, c_retain, W_mat, W_budget,
C_upper per class, p_collect_lower) stay plan fields; a null value makes every
quantity derived from it UNKNOWN. Cap state and window saturation are read from
the node's capacity shadow, which is advisory: a declared scenario condition that
is never observed is BLOCKED, not assumed. Nothing here is a release threshold,
an economic approval or a PASS.
"""
import hashlib
import json

import rfc12_acceptance_judgement as J
import rfc12_acceptance_plan as P

PLAN_KIND = 'rfc08-cost-measurement-plan'
PLAN_VERSION = 1
MODEL_SIZES = ('small', 'large')
CAP_STATES = ('below-cap', 'at-cap')
WINDOW_STATES = ('unsaturated', 'saturated')
TOP_KEYS = ('schemaVersion', 'kind', 'commit', 'network', 'salt', 'binaries', 'identity', 'bounds',
            'observationBoundsFile', 'fees', 'scan', 'settlement', 'usefulWork', 'accounting', 'reconciliation',
            'decisionInputs', 'scenarios')
UNKNOWN = 'UNKNOWN'


def _optional_int(value, name, minimum=0):
    return None if value is None else P._int(value, name, minimum)


def _decision_inputs(value):
    fields = ('capitalRatePpm', 'capitalRatePeriodDaa', 'retentionCostPerDaaSompi', 'materialWindowDaa',
              'budgetWindowDaa', 'pCollectLowerPpm', 'classCostUpperSompi')
    P._keys(value, fields, 'decisionInputs')
    upper = value['classCostUpperSompi']
    if not isinstance(upper, dict):
        raise P.PlanError('decisionInputs.classCostUpperSompi must map class ids to sompi or null')
    for class_id, amount in upper.items():
        P._hex(class_id, 'decisionInputs.classCostUpperSompi key', 128, True)
        _optional_int(amount, 'decisionInputs.classCostUpperSompi[%s]' % class_id[:8], 1)
    ppm = _optional_int(value['pCollectLowerPpm'], 'decisionInputs.pCollectLowerPpm', 1)
    if ppm is not None and ppm > 1_000_000:
        raise P.PlanError('decisionInputs.pCollectLowerPpm is a probability in ppm')
    return dict(capitalRatePpm=_optional_int(value['capitalRatePpm'], 'decisionInputs.capitalRatePpm'),
                capitalRatePeriodDaa=_optional_int(value['capitalRatePeriodDaa'], 'decisionInputs.capitalRatePeriodDaa', 1),
                retentionCostPerDaaSompi=_optional_int(value['retentionCostPerDaaSompi'],
                                                       'decisionInputs.retentionCostPerDaaSompi', 1),
                materialWindowDaa=_optional_int(value['materialWindowDaa'], 'decisionInputs.materialWindowDaa', 1),
                budgetWindowDaa=_optional_int(value['budgetWindowDaa'], 'decisionInputs.budgetWindowDaa', 1),
                pCollectLowerPpm=ppm, classCostUpperSompi=dict(upper))


def _scenario(value, index, allowed):
    name = 'scenarios[%d]' % index
    fields = ('name', 'modelSize', 'classId', 'capState', 'window', 'setupCommands', 'request', 'workCommands',
              'convictionCommands', 'sampleIntervalSeconds', 'observeAfterSeconds')
    P._keys(value, fields, name)
    request = value['request']
    if request is not None:
        P._keys(request, ('requestCommand', 'requestFile', 'claimSalt'), name + '.request')
        request_file = P._template(request['requestFile'], name + '.request.requestFile', allowed)
        if not request_file.startswith('{ROOT}/') or '..' in request_file:
            raise P.PlanError(name + '.request.requestFile must name a path under the owned {ROOT}')
        request = dict(requestCommand=P._argv(request['requestCommand'], name + '.request.requestCommand', allowed),
                       requestFile=request_file, claimSalt=P._hex(request['claimSalt'], name + '.request.claimSalt', 128, True))
    out = dict(name=P._text(value['name'], name + '.name', r'[a-z0-9][a-z0-9-]{0,63}'),
               modelSize=P._enum(value['modelSize'], name + '.modelSize', MODEL_SIZES),
               classId=P._hex(value['classId'], name + '.classId', 128, True),
               capState=P._enum(value['capState'], name + '.capState', CAP_STATES),
               window=P._enum(value['window'], name + '.window', WINDOW_STATES),
               setupCommands=P._argv_list(value['setupCommands'], name + '.setupCommands', allowed, 0),
               request=request,
               workCommands=P._argv_list(value['workCommands'], name + '.workCommands', allowed, 0),
               convictionCommands=P._argv_list(value['convictionCommands'], name + '.convictionCommands', allowed, 0),
               sampleIntervalSeconds=P._int(value['sampleIntervalSeconds'], name + '.sampleIntervalSeconds', 1),
               observeAfterSeconds=P._int(value['observeAfterSeconds'], name + '.observeAfterSeconds', 1))
    if out['request'] is None and not out['workCommands']:
        raise P.PlanError(name + ' drives no work: give a paid-work request or work commands')
    return out


def parse_cost_plan(raw):
    """Exact plan bytes -> normalized plan shaped so the acceptance run's launch/bootstrap/work code can drive it."""
    if not isinstance(raw, (bytes, bytearray)):
        raise P.PlanError('the plan must be read as exact bytes')
    try:
        plan = json.loads(raw)
    except ValueError as error:
        raise P.PlanError('plan is not JSON: %s' % error)
    P._keys(plan, TOP_KEYS, 'plan')
    if type(plan['schemaVersion']) is not int or plan['schemaVersion'] != PLAN_VERSION or plan['kind'] != PLAN_KIND:
        raise P.PlanError('unsupported cost plan kind/version')
    allowed = P.placeholders()
    binaries = P._keys(plan['binaries'], ('node', 'cli'), 'binaries')
    scenarios = plan['scenarios']
    if not isinstance(scenarios, list) or not scenarios:
        raise P.PlanError('scenarios must list at least one scenario')
    out = dict(schemaVersion=PLAN_VERSION, kind=PLAN_KIND, sha256=hashlib.sha256(bytes(raw)).hexdigest(),
               commit=P._hex(plan['commit'], 'commit', 40, True), network=P._network(plan['network']),
               salt=P._hex(plan['salt'], 'salt', 64, True),
               binaries=dict(node=P._absolute(binaries['node'], 'binaries.node'), cli=P._absolute(binaries['cli'], 'binaries.cli')),
               identity=P._identity(plan['identity']), bounds=P._bounds(plan['bounds']),
               observationBoundsFile=P._absolute(plan['observationBoundsFile'], 'observationBoundsFile'),
               fees=P._fees(plan['fees']), scan=P._scan(plan['scan']), settlement=P._settlement(plan['settlement']),
               usefulWork=P._useful_work(plan['usefulWork'], allowed), accounting=P._accounting(plan['accounting']),
               reconciliation=P._reconciliation(plan['reconciliation']),
               decisionInputs=_decision_inputs(plan['decisionInputs']),
               scenarios=[_scenario(item, index, allowed) for index, item in enumerate(scenarios)],
               # Structural, not measured values: a cost run injects no crash and takes no EVM checkpoint.
               evm=dict(consumerCheckpoints=[], roles=['producer', 'checker']),
               crash=dict(points=[], generationMismatch=None, conflictFixture=[]), coverage=None)
    names = [item['name'] for item in out['scenarios']]
    if len(set(names)) != len(names):
        raise P.PlanError('scenario names must be unique')
    payers = [out['usefulWork']['kernelPayerSeat']] + out['usefulWork']['checker']['payerSeats']
    if set(payers) & set(out['accounting']['incomeSeats']):
        raise P.PlanError('a dedicated carrier payer cannot also be an income seat')
    return out


# ---- pure measurement ---------------------------------------------------------------------------

def output_address(output):
    return (output.get('verboseData') or {}).get('scriptPublicKeyAddress') or None


def attribute(transactions, known, labels):
    """Exact fees, payments and coinbase payouts per labelled address over one window of accepted transactions.

    transactions: {txid: transaction JSON} accepted in the window; known: {'txid:index': (amount, address)} from the
    opening snapshot; labels: {address: label}. Each non-coinbase transaction is attributed to the address funding its
    inputs, only when every input resolves to the same address; payments are its outputs to any other address.
    Unresolved or mixed-owner inputs leave the transaction UNKNOWN; no fee-sharing rule is invented."""
    outputs = dict(known)
    for txid, tx in transactions.items():
        for index, output in enumerate(tx['outputs']):
            outputs['%s:%d' % (J.norm(txid), index)] = (J.exact_int(output['value'], 'output value'), output_address(output))
    payers, payouts, unresolved = {}, {}, []
    for txid, tx in transactions.items():
        if not tx['inputs']:
            for output in tx['outputs']:
                label = labels.get(output_address(output), 'other')
                payouts[label] = payouts.get(label, 0) + output['value']
            continue
        points = ['%s:%d' % (J.norm(item['previousOutpoint']['transactionId']), item['previousOutpoint']['index'])
                  for item in tx['inputs']]
        if any(point not in outputs for point in points):
            unresolved.append(J.norm(txid))
            continue
        if len(set(points)) != len(points):
            raise J.Violation('accepted transaction %s repeats an input' % J.norm(txid)[:16])
        funder = outputs[points[0]][1]
        if funder is None or any(outputs[point][1] != funder for point in points):
            unresolved.append(J.norm(txid))
            continue
        label = labels.get(funder, 'other')
        total_in = sum(J.exact_int(outputs[point][0], 'resolved input amount') for point in points)
        total_out = sum(output['value'] for output in tx['outputs'])
        if total_out > total_in:
            raise J.Violation('accepted transaction %s pays out more than its inputs' % J.norm(txid)[:16])
        row = payers.setdefault(label, dict(feesSompi=0, paymentsSompi=0, transactions=0))
        row['feesSompi'] += total_in - total_out
        row['paymentsSompi'] += sum(output['value'] for output in tx['outputs'] if output_address(output) != funder)
        row['transactions'] += 1
    return dict(byPayer=payers, coinbasePayouts=payouts, unresolvedTransactions=unresolved,
                acceptedTransactions=len(transactions))


def integrate(samples, field):
    """Left Riemann sum of a piecewise-constant sampled quantity over DAA: sum(v_i * (daa_{i+1} - daa_i))."""
    if len(samples) < 2 or any(sample.get('sampleStatus', 'observed') != 'observed'
                                or sample.get('shadowAvailable', True) is not True for sample in samples):
        return None
    daas = [J.exact_int(sample['virtualDaaScore'], 'capital sample DAA') for sample in samples]
    if any(right < left for left, right in zip(daas, daas[1:])):
        raise J.Violation('capital samples are not ordered by DAA')
    values = [None if sample.get(field) is None else
              J.decimal(sample[field], 'capital sample value') if isinstance(sample[field], str) else
              J.exact_int(sample[field], 'capital sample value') for sample in samples]
    # Only interval starts contribute a value to the left sum. Keep the final
    # clock even when its unused endpoint value is unavailable.
    if any(value is None for value in values[:-1]):
        return None
    total = 0
    for daa, value, next_daa in zip(daas, values, daas[1:]):
        total += value * (next_daa - daa)
    return total


def capital_measured(samples, before, after):
    """Successful polls bracket the SAME native window as fees; the sum is a sampled estimate."""
    if not samples or any(sample.get('sampleStatus') != 'observed' or sample.get('source') != 'node-rpc'
                          or sample.get('shadowAvailable') is not True for sample in samples):
        raise J.NotObservable('capital polls lack explicit successful native observations; cost stays UNKNOWN')
    epochs = [J.exact_int(sample.get('importEpoch'), 'capital import epoch') for sample in samples]
    if len(set(epochs)) != 1:
        raise J.NotObservable('capital polling crossed a native import generation; cost stays UNKNOWN')
    committed = integrate(samples, 'committedTodaySompi')
    collateral = integrate(samples, 'collateralSompi')
    if committed is None or collateral is None:
        raise J.NotObservable('capital polling has missing clocks/balances or failed observations; cost stays UNKNOWN')
    for sample, endpoint in [(samples[0], before), (samples[-1], after)]:
        endpoint_daa = J.exact_int(endpoint.get('virtualDaaScore'), 'scenario endpoint DAA')
        if sample.get('virtualDaaScore') != endpoint_daa or not sample.get('sink') \
                or J.norm(sample['sink']) != J.norm(endpoint['sink']):
            raise J.NotObservable('capital and fee windows have different native endpoints; cost stays UNKNOWN')
    return dict(committedSompiDaa=committed, collateralSompiDaa=collateral,
                fromSink=J.norm(before['sink']), toSink=J.norm(after['sink']),
                fromDaa=before['virtualDaaScore'], toDaa=after['virtualDaaScore'],
                method='left-Riemann sum over successful native DAA samples',
                assumption='sampled balances are held constant until the next poll; intermediate transitions are not measured')


def observed_conditions(samples):
    if not samples or any(sample.get('sampleStatus', 'observed') != 'observed'
                          or sample.get('shadowAvailable', True) is not True for sample in samples):
        raise J.NotObservable('capacity-shadow observations are missing or unavailable')
    caps = [sample.get('nMoreToday') for sample in samples]
    if any(value is None for value in caps):
        raise J.NotObservable('the capacity shadow serves no per-bond remaining-claim count')
    caps = [J.exact_int(value, 'remaining claim count') for value in caps]
    if any(sample.get(field) is None for sample in samples for field in ('committedTodaySompi', 'rBudgetSompi')):
        raise J.NotObservable('budget balances are missing; saturation state is UNKNOWN')
    balances = [[J.decimal(sample[field], field) if isinstance(sample[field], str) else J.exact_int(sample[field], field)
                 for field in ('committedTodaySompi', 'rBudgetSompi')] for sample in samples]
    saturated = any(committed >= budget for committed, budget in balances)
    return dict(capState='at-cap' if any(value == 0 for value in caps) else 'below-cap',
                window='saturated' if saturated else 'unsaturated', source='capacity-shadow (advisory, node-only)')


def collection_floor(samples, burned_before, burned_after):
    """The producer bond's collateral at the first sample after its conviction count rose, and the burned delta."""
    if len(samples) < 2 or any(sample.get('sampleStatus', 'observed') != 'observed'
                                or sample.get('shadowAvailable', True) is not True for sample in samples):
        raise J.NotObservable('missing observations prevent placing the first conviction')
    base = samples[0].get('convictions')
    for sample in samples[1:]:
        if base is not None and sample.get('convictions') is not None and sample['convictions'] > base:
            return dict(convictionAtDaa=sample['virtualDaaScore'], collectibleCollateralSompi=sample.get('collateralSompi'),
                        burnedDeltaSompi=None if burned_before is None or burned_after is None
                        else int(burned_after) - int(burned_before))
    raise J.NotObservable('no conviction of the producer bond occurred in this scenario; the collection floor is unrealised')


def derived(measured, inputs, class_id):
    """Quantities that need a decision input; each is UNKNOWN until that input is supplied."""
    capital_time = measured.get('capitalCommittedSompiDaa')
    ppm, period = inputs['capitalRatePpm'], inputs['capitalRatePeriodDaa']
    capital = UNKNOWN if None in (capital_time, ppm, period) else -(-capital_time * ppm // (1_000_000 * period))
    upper = inputs['classCostUpperSompi'].get(class_id)
    total = measured.get('measuredCostSompi')
    p_collect = inputs['pCollectLowerPpm']
    return dict(capitalCostSompi=capital,
                retentionCostSompi=UNKNOWN if inputs['retentionCostPerDaaSompi'] is None
                else UNKNOWN + ' (no retention extension is measured by this run)',
                cUpperSompi=UNKNOWN if upper is None else upper,
                measuredCostSompi=UNKNOWN if total is None else total,
                measuredWithinCUpper=UNKNOWN if None in (upper, total) else total <= upper,
                requiredMinimumSompi=UNKNOWN if None in (upper, p_collect) else -(-upper * 1_000_000 // p_collect),
                materialWindowDaa=inputs['materialWindowDaa'] if inputs['materialWindowDaa'] is not None else UNKNOWN,
                budgetWindowDaa=inputs['budgetWindowDaa'] if inputs['budgetWindowDaa'] is not None else UNKNOWN)


# ---- judgement (checkbox 8) ---------------------------------------------------------------------

def scenario_conditions(scenario, samples):
    observed = observed_conditions(samples)
    missing = [key for key in ('capState', 'window') if observed[key] != scenario[key]]
    if missing:
        raise J.NotObservable('declared %s not reached (observed %s)' % (
            ', '.join('%s=%s' % (key, scenario[key]) for key in missing), ', '.join(observed[key] for key in missing)))
    return observed


def costs_measured(attribution):
    if attribution['unresolvedTransactions']:
        raise J.NotObservable('%d accepted transactions lack resolved single-payer inputs; their fees stay UNKNOWN'
                              % len(attribution['unresolvedTransactions']))
    if not attribution['acceptedTransactions']:
        raise J.NotObservable('no transaction was accepted in the scenario window')
    return attribution


def decision_inputs_supplied(inputs, class_id):
    missing = [key for key in ('capitalRatePpm', 'capitalRatePeriodDaa', 'retentionCostPerDaaSompi', 'materialWindowDaa',
                               'budgetWindowDaa', 'pCollectLowerPpm') if inputs[key] is None]
    if inputs['classCostUpperSompi'].get(class_id) is None:
        missing.append('classCostUpperSompi[%s]' % class_id[:16])
    if missing:
        raise J.NotObservable('decision inputs not supplied (UNKNOWN): ' + ', '.join(missing))
    return dict(supplied=True)


def matrix_covered(results):
    """The plan's scenarios must cover small and large classes on both sides of the cap, and a saturated window."""
    cells = {(row['modelSize'], row['observedCapState']) for row in results if row.get('observedCapState')}
    missing = [cell for cell in ((size, cap) for size in MODEL_SIZES for cap in CAP_STATES) if cell not in cells]
    if not any(row.get('observedWindow') == 'saturated' for row in results):
        missing.append(('any', 'saturated'))
    if missing:
        raise J.NotObservable('cost matrix cells not observed: %r' % missing)
    return dict(cells=sorted(cells))
