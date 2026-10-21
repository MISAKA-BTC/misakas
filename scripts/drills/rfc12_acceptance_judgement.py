"""Mechanical RFC-0012 acceptance judgement over recorded real-process observations.

Verdicts:
  OBSERVED  the predicate held on authoritative reads of owned real processes;
  BLOCKED   a precondition, capability or exercise was absent, so nothing was judged;
  FAIL      an observation contradicts a required invariant, or a required event
            did not happen within the plan's predeclared bound;
  NOT_RUN   the scenario stopped before this predicate.
There is deliberately no PASS: an OBSERVED run is a mechanics observation of one
pinned plan, not soundness, security, economics or release approval.

Only rows whose `source` is a real read (node wRPC, ETH JSON-RPC, an owned child
process exit, or an owned file the node wrote) can contribute. A row produced by
a fixture command, or flagged `synthetic`, makes the predicate BLOCKED. Every
predicate is a pure function of the rows it is given, so the same evidence file
can be re-judged offline.
"""
import json

OBSERVED, BLOCKED, FAIL, NOT_RUN = 'OBSERVED', 'BLOCKED', 'FAIL', 'NOT_RUN'
VERDICTS = (OBSERVED, BLOCKED, FAIL, NOT_RUN)
AUTHORITATIVE_SOURCES = ('node-rpc', 'eth-rpc', 'owned-process', 'owned-file')
TAGS = ('latest', 'safe', 'finalized')
STABLE_IMPORT_PHASES = ('idle', 'legacy-stable')
EVM_NATIVE_SCALE = 10_000_000_000  # consensus/core/src/evm/mod.rs EVM_NATIVE_SCALE (sompi -> wei)
FINALIZED_CONFLICT = 'finalizedConflict'

CHECKBOXES = {
    1: 'continuous real useful work across fork, partition, IBD, pruning and DNS retirement',
    2: '100k accepted claims/population, non-genesis native Finalized pruning checkpoint, fresh join and cold reopen',
    3: 'OS crash points, import retry/rollback atomicity, generation mismatch, finalized conflict, capacity recovery',
    4: 'EVM executed prefix vs Latest/Safe/Finalized, publication history, settlement policy/provenance at one generation',
    5: 'bridge debit/backing/refund/withdrawal exactly once, both ledger roots, once-only settlement, market fill',
    6: 'CLI fee exposure vs accepted native fee, provider carrier cost in the workflow budget beside real income',
    7: 'old entitlements, DNS reward stop, reserve/exit/liability history across the retirement boundary',
    # Folded into this lane (lead, 2026-10-15); reported as their own checkboxes.
    8: 'RFC-0008: costs, minimum price inputs, capital cost and collection floor for small/large classes, '
       'before/after the bond-budget cap and with the W_budget window saturated',
    9: 'RFC-0006: EXEC origin header coverage from an archive role and a pruned role at one native generation',
}
COVERAGE_KIND = 'rfc06-private-exec-origin-coverage-v1'


class Violation(Exception):
    """A required invariant is contradicted (FAIL). Deliberately not an AssertionError,
    so the drill's wait loop cannot retry it away."""


class NotObservable(Exception):
    """A precondition, capability or exercise is absent (BLOCKED)."""


class Pending(AssertionError):
    """Not yet observed; retryable inside a bounded wait. At judgement time it is a FAIL
    because the plan fixed the bound before launch."""


def norm(value):
    return value.removeprefix('0x').lower() if isinstance(value, str) else value


def head32(value):
    """The prescribed 32-byte ETH projection of a 64-byte native hash."""
    return None if value is None else norm(value)[:64]


def exact_int(value, name, minimum=0):
    if type(value) is not int or value < minimum:
        raise Violation('%s must be an exact integer >= %d' % (name, minimum))
    return value


def decimal(value, name):
    if not isinstance(value, str) or not value.isdigit() or not value.isascii() \
            or (len(value) > 1 and value[0] == '0'):
        raise Violation(name + ' must be canonical decimal text')
    return int(value)


def require_real(rows):
    for row in rows:
        if not isinstance(row, dict) or row.get('source') not in AUTHORITATIVE_SOURCES or row.get('synthetic'):
            raise NotObservable('fixture-sourced or synthetic evidence cannot certify an RFC-0012 predicate')
    return rows


def finding(checkbox, predicate, verdict, reason='', facts=None):
    if checkbox not in CHECKBOXES or verdict not in VERDICTS:
        raise ValueError('unknown checkbox or verdict')
    return dict(checkbox=checkbox, predicate=predicate, verdict=verdict, reason=reason, facts=facts)


def judge(checkbox, predicate, function, *args, **kwargs):
    """Run one predicate and map its outcome to a verdict. Unknown exceptions propagate."""
    try:
        facts = function(*args, **kwargs)
    except Violation as error:
        return finding(checkbox, predicate, FAIL, str(error))
    except NotObservable as error:
        return finding(checkbox, predicate, BLOCKED, str(error))
    except Pending as error:
        return finding(checkbox, predicate, FAIL, 'not reached within the predeclared bound: %s' % error)
    return finding(checkbox, predicate, OBSERVED, '', facts)


def aggregate(findings, boxes=None):
    """Per-checkbox and overall status over the checkboxes this driver owns (a box with no finding is BLOCKED)."""
    by_box = {number: [] for number in (boxes or CHECKBOXES)}
    for item in findings:
        if item['checkbox'] not in by_box:
            raise ValueError('finding for checkbox %r outside this driver' % item['checkbox'])
        by_box[item['checkbox']].append(item)
    boxes = {}
    for number, items in by_box.items():
        verdicts = {item['verdict'] for item in items}
        if FAIL in verdicts:
            status = FAIL
        elif not items or verdicts & {BLOCKED, NOT_RUN}:
            status = BLOCKED
        else:
            status = OBSERVED
        boxes[str(number)] = dict(title=CHECKBOXES[number], status=status, findings=items)
    statuses = {box['status'] for box in boxes.values()}
    overall = FAIL if FAIL in statuses else BLOCKED if BLOCKED in statuses else OBSERVED
    assert overall in VERDICTS and 'PASS' not in json.dumps(sorted(statuses))
    return dict(status=overall, checkboxes=boxes, passLabelUsed=False, releaseApproval=False,
                fullRfc12SecurityApproval=False)


# ---- identity and retirement (checkbox 1) -------------------------------------------------------

def retirement_armed(network, manifest, identity):
    """The private factory, not a preset, arms palw_dns_retirement_v1 with the pinned identity."""
    if network['kind'] != 'private-profile':
        raise NotObservable('preset %s keeps palw_dns_retirement_v1 dormant; only a private profile arms it' % network['name'])
    nested = manifest.get('privateProbeProfile', manifest)
    if nested.get('skipProofOfWork') is not False or nested.get('releaseApproval') is not False:
        raise Violation('the private factory must keep real PoW and carry no release approval')
    retirement = nested.get('retirement')
    if not isinstance(retirement, dict) or retirement.get('activationDaa') is None:
        raise NotObservable('this profile does not arm palw_dns_retirement_v1; retirement cannot be exercised')
    pinned = (identity['genesis'], identity['consensusParamsId'], identity['retirementActivationDaa'],
              identity['retirementPolicyId'])
    actual = (norm(nested.get('genesis')), norm(nested.get('consensusParamsId')), retirement['activationDaa'],
              norm(retirement.get('policyId')))
    if actual != pinned:
        raise NotObservable('the factory identity %r differs from the pinned plan identity %r' % (actual, pinned))
    rule_e = nested.get('forkChoiceRuleEDaa')
    if type(rule_e) is not int or rule_e > retirement['activationDaa']:
        raise Violation('Rule E is not armed at or below the retirement height (R12-AUTHORITY 1)')
    return dict(activationDaa=retirement['activationDaa'], policyId=actual[3], ruleEDaa=rule_e,
                depth=retirement.get('depth'), uniqueWork=retirement.get('uniqueWork'),
                legacyEvidenceHorizonDaa=retirement.get('legacyEvidenceHorizonDaa'))


# ---- consumer coherence at one generation (checkbox 4) ------------------------------------------

def roots_of(header):
    return tuple(norm(header.get(field)) for field in ('utxoCommitment', 'evmCommitmentRoot', 'palwStateRoot'))


def consumer_row(row, identity):
    """Native snapshot, readiness, publication history and ETH tags of ONE bracketed read."""
    require_real([row])
    value = row['value']
    settlement = value.get('settlement') or {}
    native = settlement.get('nativeSettlement')
    if settlement.get('dnsRetiredAt') is None or native is None:
        raise NotObservable('%s exposes no active native retirement at this generation' % row['role'])
    if (value.get('dns') or {}).get('available'):
        raise Violation('a retired node still serves DNS finality authority')
    sink = norm(row['dag']['sink'])
    if norm(native['generation']) != sink:
        raise Violation('native settlement generation differs from the bracketed L1 sink')
    if norm(native['rulesetId']) != identity['consensusParamsId']:
        raise Violation('native settlement names another consensus ruleset')
    if norm(native['policyId']) != identity['retirementPolicyId']:
        raise Violation('native settlement names another D/W/concentration policy (provenance)')
    if native.get('retirementDaa') != identity['retirementActivationDaa']:
        raise Violation('native settlement names another retirement height')
    if settlement['dnsRetiredAt'] != identity['retirementActivationDaa']:
        raise Violation('settlement retirement height differs from the native snapshot')
    readiness = settlement.get('nativeReadiness')
    if readiness is None:
        raise NotObservable('readiness explanation absent at a bracketed stable generation')
    finalized_readiness = readiness.get('finalized') or {}
    if norm(readiness.get('generation')) != sink or readiness.get('safe') != native.get('safe') \
            or finalized_readiness.get('finalized') != native.get('finalized') or readiness.get('stop') != native.get('stop'):
        raise Violation('readiness disagrees with the committed native snapshot it is attached to')
    stop, withdrawn = native.get('stop'), finalized_readiness.get('withdrawnFrom')
    if stop == FINALIZED_CONFLICT and (native.get('safe') is not None or native.get('finalized') is not None):
        raise Violation('FinalizedConflict still publishes safe/finalized heads')
    if withdrawn is not None and (native.get('finalized') is not None or stop == FINALIZED_CONFLICT):
        raise Violation('withdrawnFrom is reported beside a published finalized head or a conflict')
    if native.get('finalized') is not None and native.get('safe') is None:
        raise Violation('finalized is published without a safe head')
    if native.get('safe') is not None and native.get('latest') is None:
        raise Violation('safe is published without an executed latest head')
    numbers = {}
    for tag in TAGS:
        answer = (value.get('eth') or {}).get(tag)
        if not isinstance(answer, dict) or answer.get('error') is not None:
            raise NotObservable('ETH %s read is missing or failed in this row' % tag)
        block, expected = answer.get('result'), native.get(tag)
        if expected is None:
            if block is not None:
                raise Violation('ETH %s serves a block where the native %s head is absent (fabricated fallback)' % (tag, tag))
            continue
        if block is None:
            raise Violation('ETH %s is absent while the native %s head is published' % (tag, tag))
        if norm(block.get('hash')) != head32(expected):
            raise Violation('ETH %s hash is not the 32-byte projection of the native head' % tag)
        numbers[tag] = int(block['number'], 16)
    if 'finalized' in numbers and numbers['finalized'] > numbers['safe']:
        raise Violation('finalized is above safe in executed EVM-result order')
    if 'safe' in numbers and numbers['safe'] > numbers['latest']:
        raise Violation('safe is above latest in executed EVM-result order')
    return dict(role=row['role'], label=row.get('label'), sink=sink, heads={tag: native.get(tag) for tag in TAGS},
                evmNumbers=numbers, roots=roots_of(value['header']), stop=stop, withdrawnFrom=withdrawn,
                depth=native.get('depth'), uniqueWork=native.get('uniqueWork'))


def settled_with_work(row, settlement_pins):
    """Useful work actually advanced native safe to the predeclared depth/work."""
    native = row['value']['settlement']['nativeSettlement']
    if native.get('safe') is None:
        raise Pending('no useful-work covered native safe prefix yet')
    if decimal(native['uniqueWork'], 'uniqueWork') < int(settlement_pins['minimumWork']):
        raise Pending('unique mature work below the predeclared bound')
    if exact_int(native['depth'], 'depth') < settlement_pins['minimumDepth']:
        raise Pending('settled anchor depth below the predeclared bound')
    return dict(safe=native['safe'], depth=native['depth'], uniqueWork=native['uniqueWork'])


def agreement(rows):
    """Rows of different roles at the same sink agree on roots, native snapshot and ETH heads."""
    require_real(rows)
    by_sink = {}
    for row in rows:
        value = row['value']
        eth = {tag: (lambda block: None if block is None else (norm(block.get('hash')), norm(block.get('stateRoot')),
                                                                block.get('number')))((value['eth'][tag] or {}).get('result'))
               for tag in TAGS}
        key = (roots_of(value['header']), json.dumps(value['settlement']['nativeSettlement'], sort_keys=True), json.dumps(eth))
        by_sink.setdefault(norm(row['dag']['sink']), set()).add(key)
    for sink, keys in by_sink.items():
        if len(keys) != 1:
            raise Violation('processes at the same sink %s disagree on roots, native heads or ETH heads' % sink[:16])
    return dict(sinks=sorted(by_sink))


def root_consistency(rows):
    """Across every recorded row (any role, any time): one sink, one set of ledger roots."""
    require_real(rows)
    seen = {}
    for row in rows:
        header = (row.get('value') or {}).get('header') or row.get('header')
        if not header or 'dag' not in row:
            continue
        sink, roots = norm(row['dag']['sink']), roots_of(header)
        if seen.setdefault(sink, roots) != roots:
            raise Violation('two observations of sink %s carry different UTXO/EVM/PALW roots' % sink[:16])
    if not seen:
        raise NotObservable('no rooted observation was recorded')
    return dict(distinctSinks=len(seen))


def finalized_history(rows_by_role):
    """Publication history over each role's own DB: no silent relabel, sticky conflict, named withdrawal."""
    unknown, transitions, conflicts, withdrawals = 0, 0, 0, 0
    for role, rows in rows_by_role.items():
        require_real(rows)
        previous = None
        for row in rows:
            native = row['value']['settlement']['nativeSettlement']
            readiness = row['value']['settlement'].get('nativeReadiness') or {}
            epoch = row['node']['importSessionEpoch']
            conflicts += native.get('stop') == FINALIZED_CONFLICT
            if previous is not None:
                old_native, old_epoch = previous['value']['settlement']['nativeSettlement'], previous['node']['importSessionEpoch']
                same_import = epoch == old_epoch
                old_final, new_final = old_native.get('finalized'), native.get('finalized')
                if old_native.get('stop') == FINALIZED_CONFLICT and native.get('stop') != FINALIZED_CONFLICT and same_import:
                    raise Violation('%s cleared a sticky FinalizedConflict without a validated import' % role)
                if old_final is not None and new_final is not None and norm(new_final) != norm(old_final):
                    transitions += 1
                    ancestor = (row['value'].get('ancestry') or {}).get(norm(old_final))
                    if ancestor is None:
                        unknown += 1
                    elif ancestor is False and same_import:
                        raise Violation('%s silently relabelled finalized away from %s' % (role, norm(old_final)[:16]))
                if old_final is not None and new_final is None and same_import and native.get('stop') != FINALIZED_CONFLICT:
                    withdrawals += 1
                    if norm((readiness.get('finalized') or {}).get('withdrawnFrom')) != norm(old_final):
                        raise Violation('%s withdrew finalized %s without naming it' % (role, norm(old_final)[:16]))
            previous = row
    if unknown:
        raise NotObservable('%d finalized advances could not be checked for chain ancestry' % unknown)
    return dict(advances=transitions, conflicts=conflicts, withdrawals=withdrawals)


def conflict_exercised(rows):
    require_real(rows)
    hits = [row['role'] for row in rows if row['value']['settlement']['nativeSettlement'].get('stop') == FINALIZED_CONFLICT]
    if not hits:
        raise NotObservable('no below-finalized conflict occurred during the run; the alarm path was not exercised')
    return dict(roles=sorted(set(hits)), rows=len(hits))


# ---- real paid useful work: producer -> provider -> checker -> Final (checkbox 1) --------------

def paid_claim_accepted(rows, expected):
    """Every process at one sink serves the same accepted paid claim with the explicit work identity."""
    require_real(rows)
    sinks = {norm(row['dag']['sink']) for row in rows}
    if len(sinks) != 1:
        raise Pending('paid-claim reads were not taken at one common sink')
    for row in rows:
        value = row['value']
        if not value.get('available'):
            raise NotObservable('%s serves no paid-claim route state' % row['role'])
        if norm(value.get('sink')) != norm(row['dag']['sink']):
            raise Violation('the paid-claim read names another generation than its bracket')
        if not value.get('found'):
            raise Pending('the paid claim is not accepted on %s yet' % row['role'])
    identities = {(norm(row['value']['enrollment']), row['value']['producerBond'], norm(row['value']['classId']),
                   norm(row['value']['jobId']), norm(row['value']['materialConsent'])) for row in rows}
    if len(identities) != 1:
        raise Violation('processes at one sink serve different paid-claim identities')
    (enrollment, producer, class_id, job, _), = identities
    if producer != expected['producerBond'] or class_id != norm(expected['classId']) or job != norm(expected['jobId']):
        raise Violation('the accepted paid claim substitutes the producer, class or job of the explicit work')
    return dict(enrollment=enrollment, sink=sinks.pop(), roles=sorted(row['role'] for row in rows))


def checker_receipts(rows, checker_bonds):
    """Each configured selected checker's receipt is counted exactly once, identically everywhere."""
    require_real(rows)
    if len({norm(row['dag']['sink']) for row in rows}) != 1:
        raise Pending('receipt reads were not taken at one common sink')
    views = {json.dumps(sorted(row['value']['receipts'])) for row in rows}
    if len(views) != 1:
        raise Violation('processes at one sink count different receipt signers')
    receipts = rows[0]['value']['receipts']
    for bond in checker_bonds:
        if receipts.count(bond) > 1:
            raise Violation('the receipt of %s is counted more than once' % bond)
        if bond not in receipts:
            raise Pending('the selected checker %s has no accepted receipt yet' % bond)
    return dict(receipts=list(receipts), quorum=rows[0]['value'].get('quorum'))


def paid_claim_final(rows):
    require_real(rows)
    if len({norm(row['dag']['sink']) for row in rows}) != 1:
        raise Pending('Final reads were not taken at one common sink')
    finals = {(row['value'].get('final'), row['value'].get('finalDaa')) for row in rows}
    if len(finals) != 1:
        raise Violation('processes at one sink disagree on the paid claim Final')
    (final, final_daa), = finals
    if final is not True:
        raise Pending('the paid claim has not reached Final')
    return dict(finalDaa=final_daa)


def checker_outcome(diagnostics):
    """The honest owned provider served material: no checker filed a material challenge instead of a receipt.
    Log lines are the node's own diagnostics, used only to classify a failure, never to certify success."""
    challenged = [item for item in diagnostics if item.get('kind') == 'challenge-accepted']
    if challenged:
        raise Violation('a selected checker filed a material challenge against the honest owned provider: %s'
                        % challenged[0].get('line', '')[:200])
    stopped = [item for item in diagnostics if item.get('kind') in ('checker-stopped', 'producer-stopped')]
    if stopped:
        # A stopped adapter names its own reason (spec, key, funding, assignment); the useful-work path was not exercised.
        raise NotObservable('a paid producer/checker adapter stopped: %s' % stopped[0].get('line', '')[:200])
    return dict(diagnosticLines=len(diagnostics))


# ---- crash, import and generation controls (checkbox 3) -----------------------------------------

def import_is_stable(node):
    return node.get('pruningImportStable') is True and node.get('importSessionPoint') == '' \
        and node.get('importSessionPhase') in STABLE_IMPORT_PHASES


def incomplete_import_publishes_nothing(rows):
    """R12-HEADS 4: an incomplete intermediate copy publishes no canonical native or ETH tag."""
    require_real(rows)
    examined = 0
    for row in rows:
        if import_is_stable(row['node']):
            continue
        examined += 1
        native = ((row.get('value') or {}).get('settlement') or {}).get('nativeSettlement')
        if native is not None and any(native.get(tag) is not None for tag in TAGS):
            raise Violation('an incomplete import published native heads')
        for tag in TAGS:
            answer = ((row.get('value') or {}).get('eth') or {}).get(tag) or {}
            if answer.get('result') is not None:
                raise Violation('an incomplete import served ETH %s' % tag)
    if not examined:
        raise NotObservable('no observation inside an incomplete import was captured')
    return dict(incompleteRows=examined)


def crash_point(point, record):
    """One injected SIGKILL and its recovery, judged from owned-process and node reads."""
    if not record.get('killed'):
        raise NotObservable('injection point %s was never reached within its bound' % point['name'])
    require_real([record['trigger']] + record.get('afterRows', []))
    kind = point['trigger']['kind']
    before, after = record['before'], record.get('after') or {}
    if not record.get('recovered'):
        raise Violation('%s did not recover after SIGKILL at %s' % (point['role'], point['name']))
    if kind == 'import-phase':
        if before.get('importSessionPhase') not in point['trigger']['phases']:
            raise NotObservable('the kill was not observed inside a requested import phase')
        if not import_is_stable(after):
            raise Violation('the crashed import did not complete or roll back to a stable state')
    elif kind == 'import-wal':
        if record['trigger'].get('stage') != point['trigger']['stage'] or before.get('walStage') != point['trigger']['stage']:
            raise Violation('the SIGKILL was not at the exact requested WAL rendezvous')
        if not import_is_stable(after):
            raise Violation('the WAL-crashed import did not recover to a stable state')
    elif kind in ('kernel-outbox', 'checker-outbox'):
        for name, digest in before['outbox'].items():
            if after.get('outbox', {}).get(name) != digest:
                raise Violation('immutable %s record %s changed across the crash' % (kind, name))
        claims = record.get('claimRows') or []
        if not claims:
            raise NotObservable('no paid-claim read after restart; acceptance of the WAL record is unjudged')
        identities = {(norm(row['value']['enrollment']), row['value']['producerBond'], norm(row['value']['jobId']),
                       norm(row['value']['classId'])) for row in claims}
        if len(identities) != 1:
            raise Violation('the paid claim identity differs across processes after the crash')
        if kind == 'checker-outbox':
            bond = record['checkerBond']
            if any(row['value']['receipts'].count(bond) != 1 for row in claims):
                raise Violation('the crashed checker receipt is absent or counted more than once after restart')
    elif kind == 'provider-store':
        for path, size in before['stores'].items():
            if after.get('stores', {}).get(path, -1) < size:
                raise Violation('a durably published provider store was lost or truncated by the crash')
    elif kind == 'after-convergence':
        if after.get('importSessionEpoch') != before.get('importSessionEpoch'):
            raise Violation('a cold restart of a converged node changed its completed import epoch')
    return dict(point=point['name'], role=point['role'], kind=kind)


def generation_mismatch(record):
    """A DB pinned to one private profile is refused under another, unmodified, then reopens intact."""
    require_real([record['launch']])
    launch = record['launch']
    if not launch.get('exitedWithinBound'):
        raise Violation('a DB pinned to another profile kept running under the wrong profile')
    if launch.get('exitCode') == 0:
        raise Violation('a mismatched profile exited successfully over a pinned DB')
    if record['markerBefore'] != record['markerAfter']:
        raise Violation('the mismatched launch rewrote the pinned identity marker')
    if record['dbBefore'] != record['dbAfter']:
        raise Violation('the mismatched launch modified the owned DB')
    if 'differs from its pinned identity' not in launch.get('stderrTail', ''):
        raise NotObservable('the launch failed for another reason; the identity refusal was not exercised')
    if not record.get('recovered'):
        raise Violation('the correctly pinned relaunch did not rejoin')
    if record['relaunchEpoch'] != record['priorEpoch']:
        raise Violation('the correctly pinned relaunch changed its completed import epoch')
    return dict(exitCode=launch['exitCode'], epoch=record['priorEpoch'])


def capacity_refusal_exercised(record):
    constrained = record['constrained']
    if constrained.get('exitCode') not in (None, 0):
        return dict(refusal='process-exit', exitCode=constrained['exitCode'])
    raise NotObservable('the constrained relaunch neither exited nor reported a local capacity refusal')


def capacity_recovery(record, signal):
    """After consensus was already exchanged: a local constraint leaves state intact, and lifting it rejoins."""
    require_real(record.get('constrainedRows', []) + record.get('recoveredRows', []))
    prior = record['priorEpoch']
    for row in record.get('constrainedRows', []):
        if row['node']['importSessionEpoch'] != prior:
            raise Violation('a local capacity constraint changed the completed import epoch')
    if not record.get('recovered'):
        raise Violation('the node did not rejoin after its local capacity constraint was lifted')
    before, after = record['participationBefore'], record['participationAfter']
    if signal == 'sink-follows':
        if not (after['virtualDaaScore'] > before['virtualDaaScore'] and after['sink'] == after['networkSink']):
            raise Violation('the recovered node does not follow the network sink')
    elif signal == 'provider-store-grows':
        if not after['storeBytes'] > before['storeBytes']:
            raise Violation('the recovered provider published no further material')
    elif signal == 'produced-blocks-increase':
        if not after['producedBlocks'] > before['producedBlocks']:
            raise Violation('the recovered producer produced no further block')
    return dict(signal=signal, before=before, after=after)


# ---- scale, pruning, join and reopen (checkbox 2) -----------------------------------------------

def counter_value(row, counter):
    value = row['value']
    if counter == 'capacityShadowLiveClaims':
        shadow = value.get('shadow') or {}
        if not shadow.get('available', True):
            raise NotObservable('capacity shadow unavailable')
        return exact_int(shadow.get('claimsTotal'), 'claimsTotal')
    finals = value.get('finals') or {}
    if not finals.get('available'):
        raise NotObservable('kernel finals unavailable')
    return exact_int(finals.get('total'), 'kernel finals total')


def scale_reached(rows, counter, minimum):
    require_real(rows)
    values = {(norm(row['dag']['sink']), counter_value(row, counter)) for row in rows}
    if len({sink for sink, _ in values}) != 1:
        raise Pending('scale counters were not read at one common generation')
    if len(values) != 1:
        raise Violation('processes at one generation disagree on the %s counter' % counter)
    (_, count), = values
    if count < minimum:
        raise Pending('%s is %d, below the predeclared %d' % (counter, count, minimum))
    return dict(counter=counter, value=count, roles=sorted(row['role'] for row in rows))


def registry_population(rows, bonds):
    require_real(rows)
    for row in rows:
        shadow = row['value'].get('shadow') or {}
        if shadow.get('bondsTotal') != bonds:
            raise Violation('%s registry holds %r bonds, not the pinned %d' % (row['role'], shadow.get('bondsTotal'), bonds))
    return dict(bonds=bonds, roles=sorted(row['role'] for row in rows))


def non_genesis_finalized(row, genesis):
    native = row['value']['settlement']['nativeSettlement']
    pruning = norm(row['dag']['pruningPointHash'])
    if pruning == norm(genesis):
        raise Pending('the pruning point has not moved off genesis')
    if native.get('finalized') is None:
        raise Pending('no native Finalized head at a moved pruning point')
    if norm(native['finalized']) != pruning:
        raise Violation('native Finalized does not authenticate the actual pruning checkpoint')
    return dict(finalized=pruning)


def fresh_join(source, joined, genesis, require_finalized=True):
    require_real([source, joined])
    if norm(source['dag']['sink']) != norm(joined['dag']['sink']):
        raise Pending('the fresh join has not reached the source sink')
    if roots_of(source['value']['header']) != roots_of(joined['value']['header']):
        raise Violation('the fresh P2P join reached the source sink with different ledger roots')
    if source['value']['settlement']['nativeSettlement'] != joined['value']['settlement']['nativeSettlement']:
        raise Violation('the fresh P2P join derived different native heads at the same sink')
    if require_finalized:
        non_genesis_finalized(joined, genesis)
    if exact_int(joined['node']['importSessionEpoch'], 'import epoch') < 1:
        raise Violation('the fresh pruned join reports no completed durable import session')
    if joined.get('oldBodyAvailable') is not False:
        raise Violation('the pruned join retained or could not classify the old block body')
    return dict(sink=norm(joined['dag']['sink']), finalized=joined['value']['settlement']['nativeSettlement']['finalized'],
                importEpoch=joined['node']['importSessionEpoch'])


def cold_reopen(before, after, source, require_finalized=True):
    require_real([before, after, source])
    if after['node']['importSessionEpoch'] != before['node']['importSessionEpoch']:
        raise Violation('a cold reopen of the same DB changed its completed import epoch')
    if norm(after['dag']['sink']) != norm(source['dag']['sink']):
        raise Pending('the reopened DB has not reached the source sink')
    if roots_of(after['value']['header']) != roots_of(source['value']['header']) \
            or after['value']['settlement']['nativeSettlement'] != source['value']['settlement']['nativeSettlement']:
        raise Violation('the cold-reopened DB reached the source sink with different roots or native heads')
    if require_finalized and after['value']['settlement']['nativeSettlement'].get('finalized') is None:
        raise Violation('the cold-reopened DB lost its native Finalized head')
    return dict(sink=norm(after['dag']['sink']), importEpoch=after['node']['importSessionEpoch'])


# ---- bridge, settlement and market (checkbox 5) -------------------------------------------------

def deposit_credit(before_wei, rows, amount, tip):
    """Each process at one ETH latest block credits (amount - tip) * scale exactly once."""
    require_real(rows)
    expected = (amount - tip) * EVM_NATIVE_SCALE
    heads = {row['value']['ethLatestHash'] for row in rows}
    if len(heads) != 1:
        raise Pending('deposit balances were not read at one ETH latest block')
    deltas = {row['value']['balanceWei'] - before_wei for row in rows}
    if len(deltas) != 1:
        raise Violation('processes at one ETH block disagree on the credited balance')
    (delta,) = deltas
    if delta == 0:
        raise Pending('the deposit claim has not executed yet')
    if delta != expected:
        raise Violation('deposit credited %d wei, expected exactly %d (double or partial credit)' % (delta, expected))
    return dict(creditedWei=delta, ethLatestHash=heads.pop())


def refused(record, what):
    require_real([record])
    if record.get('exitCode') in (None, 0):
        raise Violation(what + ' was accepted')
    return dict(exitCode=record['exitCode'])


def outpoint_key(entry):
    point = entry['outpoint']
    return '%s:%d' % (norm(point['transactionId']), point['index'])


def exactly_one_new_utxo(before, after, low, high):
    """A dedicated destination gained exactly one non-coinbase UTXO in [low, high]."""
    known = {outpoint_key(entry) for entry in before}
    new = [entry for entry in after if outpoint_key(entry) not in known]
    if not new:
        raise Pending('no materialized output at the dedicated destination yet')
    if len(new) > 1:
        raise Violation('the dedicated destination gained %d outputs; the effect materialized more than once' % len(new))
    entry = new[0]['utxoEntry']
    amount = exact_int(entry['amount'], 'materialized amount')
    if entry.get('isCoinbase') or not low <= amount <= high:
        raise Violation('the materialized output %d is outside [%d, %d] or is a coinbase' % (amount, low, high))
    return dict(outpoint=outpoint_key(new[0]), amount=amount)


def backing(before_wei, after_wei, credited_wei, withdrawn_sompi, gas_wei):
    """One EVM account: before + credit - withdrawal - gas == after, in wei."""
    expected = before_wei + credited_wei - withdrawn_sompi * EVM_NATIVE_SCALE - gas_wei
    if expected != after_wei:
        raise Violation('EVM backing does not reconcile: expected %d wei, observed %d' % (expected, after_wei))
    return dict(beforeWei=before_wei, afterWei=after_wei, creditedWei=credited_wei,
                withdrawnWei=withdrawn_sompi * EVM_NATIVE_SCALE, gasWei=gas_wei)


def effects_stable(snapshots):
    """Withdrawal/refund materializations and the backed balance are identical everywhere, every time."""
    require_real(snapshots)
    first = None
    for row in snapshots:
        value = row['value']
        key = (tuple(sorted(value['withdrawalOutpoints'])), tuple(sorted(value['refundOutpoints'])),
               stable_point_states(value.get('pointStates', {})))
        if first is None:
            first = key
        elif key != first:
            raise Violation('a bridge effect appeared, disappeared or duplicated across reorg/restart/join (%s)' % row['role'])
    if first is None:
        raise NotObservable('no bridge effect snapshot was recorded')
    if len(first[0]) != 1 or len(first[1]) != 1:
        raise Violation('expected exactly one withdrawal and one refund materialization')
    return dict(snapshots=len(snapshots), withdrawal=first[0][0], refund=first[1][0])


def once_only_settlement(rows):
    """The paid claim's vesting row exists at most once and never re-enters vesting after its move."""
    require_real(rows)
    by_sink, moved_seen, settled = {}, {}, False
    for row in rows:
        value = row['value']
        if value['rowsForClaim'] > 1 or value['finalsForClaim'] > 1:
            raise Violation('the claim is settled or listed as Final more than once')
        key = json.dumps([value['stage'], value['vestingRow'], value['finalsForClaim']], sort_keys=True)
        if by_sink.setdefault(norm(row['dag']['sink']), key) != key:
            raise Violation('processes at one sink disagree on the claim settlement')
        settled |= value['stage'] in ('moved', 'latched', 'maturing') or value['finalsForClaim'] == 1
        if row['role'] in moved_seen and value.get('movedAncestor') is True and value['stage'] in ('maturing', 'latched'):
            raise Violation('the claim re-entered vesting after its reward moved on the same history')
        if value['stage'] == 'moved':
            moved_seen[row['role']] = norm(row['dag']['sink'])
    if not settled:
        raise NotObservable('the paid claim never reached Final settlement during the run')
    return dict(observations=len(rows), moved=bool(moved_seen))


def units_changed(before_units, rows, delta, what):
    require_real(rows)
    tips = {norm(row['value']['tipHash']) for row in rows}
    if len(tips) != 1:
        raise Pending('positions were not read at one native tip')
    values = {row['value']['units'] for row in rows}
    if len(values) != 1:
        raise Violation('processes at one tip disagree on the holder position')
    (units,) = values
    if units == before_units:
        raise Pending('the %s has not settled yet' % what)
    if units != before_units + delta:
        raise Violation('the %s moved %d units, expected exactly %d' % (what, units - before_units, delta))
    return dict(before=before_units, after=units, tip=tips.pop())


# ---- fees, carrier cost and income (checkbox 6) -------------------------------------------------

def accepted_fee(transaction, amounts):
    """Exact fee of an accepted transaction from resolved input amounts; None when any input is unknown."""
    points = ['%s:%d' % (norm(item['previousOutpoint']['transactionId']), item['previousOutpoint']['index'])
              for item in transaction['inputs']]
    unresolved = [point for point in points if point not in amounts]
    if unresolved:
        return None, unresolved
    total_in = sum(amounts[point] for point in points)
    total_out = sum(exact_int(output['value'], 'output value') for output in transaction['outputs'])
    if total_out > total_in:
        raise Violation('an accepted transaction pays out more than its inputs')
    return total_in - total_out, []


def fee_separation(cli_records, accepted):
    """CLI submission exposure and accepted native fees stay two ledgers; they are compared, never merged."""
    exposure, accepted_total, unresolved, exposure_only = 0, 0, [], []
    for record in cli_records:
        exposure += exact_int(record['exposureFeeSompi'], 'CLI fee exposure')
        row = accepted.get(norm(record['txid']))
        if row is None or not row['accepted']:
            exposure_only.append(record['txid'])
            continue
        if row['feeSompi'] is None:
            unresolved.append(record['txid'])
            continue
        if row['feeSompi'] != record['exposureFeeSompi']:
            raise Violation('%s: CLI fee exposure %d differs from the accepted native fee %d' % (
                record['label'], record['exposureFeeSompi'], row['feeSompi']))
        accepted_total += row['feeSompi']
    facts = dict(cliSubmissionFeeExposureSompi=exposure, acceptedNativeFeeSompi=accepted_total,
                 exposureOnlyTxids=exposure_only, unresolvedAcceptedTxids=unresolved, submissions=len(cli_records))
    if not cli_records:
        raise NotObservable('no CLI submission was recorded')
    if unresolved:
        raise NotObservable('accepted fees of %d CLI carriers could not be resolved exactly' % len(unresolved))
    return facts


def dedicated_payer_cost(start, end, cap, label):
    """Total cost of a dedicated payer: start balance - end balance at one generation, capped."""
    known = {outpoint_key(entry) for entry in start}
    for entry in end:
        if outpoint_key(entry) not in known and entry['utxoEntry'].get('isCoinbase'):
            raise NotObservable('%s payer received a coinbase output; it is not dedicated' % label)
    cost = sum(entry['utxoEntry']['amount'] for entry in start) - sum(entry['utxoEntry']['amount'] for entry in end)
    if cost < 0:
        raise NotObservable('%s payer gained value; the balance method does not measure its carriers' % label)
    if cost > cap:
        raise Violation('%s carrier cost %d exceeds its predeclared budget %d' % (label, cost, cap))
    return dict(costSompi=cost, startOutputs=len(start), endOutputs=len(end))


def workflow_budget(components, cap):
    known = {name: value for name, value in components.items() if value is not None}
    total = sum(known.values())
    if total > cap:
        raise Violation('measured workflow cost %d exceeds the predeclared total %d' % (total, cap))
    missing = sorted(name for name, value in components.items() if value is None)
    if missing:
        raise NotObservable('workflow cost components unmeasured: ' + ', '.join(missing))
    return dict(totalSompi=total, components=known, capSompi=cap)


def real_income(income):
    if income.get('vestingAvailable') is not True:
        raise NotObservable('reward vesting reads are unavailable; real reward income is unmeasured')
    return income


# ---- RFC-0006 EXEC origin header coverage (checkbox 9) ------------------------------------------

def coverage_envelope(row, consensus_params_id):
    """One owned rfc06-exec-origin-coverage-v1.json, read inside a generation bracket of the same role."""
    require_real([row])
    envelope = row['value'].get('coverage')
    if envelope is None:
        raise Pending('%s has not written its coverage observation at this generation' % row['role'])
    if envelope.get('schemaVersion') != 1 or envelope.get('kind') != COVERAGE_KIND:
        raise NotObservable('unknown coverage observation schema')
    if envelope.get('activationApproval') is not False:
        raise Violation('a coverage observation claims activation approval')
    if norm(envelope.get('consensusParamsId')) != norm(consensus_params_id):
        raise Violation('the coverage observation names another consensus ruleset')
    if envelope.get('virtualDaaScore') != row['dag']['virtualDaaScore']:
        raise Pending('the coverage file predates this generation (written every 60 s)')
    if envelope.get('census') is None:
        raise NotObservable('%s could not take the census: %s' % (row['role'], envelope.get('error')))
    return envelope['census']


def coverage_same_generation(archive, pruned):
    if norm(archive['dag']['sink']) != norm(pruned['dag']['sink']) \
            or archive['dag']['virtualDaaScore'] != pruned['dag']['virtualDaaScore']:
        raise Pending('archive and pruned coverage were not read at one native generation')
    return dict(sink=norm(archive['dag']['sink']), virtualDaaScore=archive['dag']['virtualDaaScore'])


def archive_coverage(archive, pruned, consensus_params_id):
    """The archive role covers every eligible original header: oversized_total == 0 and unavailable_total == 0.
    Field names follow PalwExecOriginCoverageV1's serde (snake_case; only the envelope is camelCase).
    The pruned role's census is reported beside it and never judged ("not here" is not "nowhere")."""
    generation = coverage_same_generation(archive, pruned)
    census = coverage_envelope(archive, consensus_params_id)
    pruned_census = coverage_envelope(pruned, consensus_params_id)
    facts = dict(generation=generation, archive=census, prunedReportedNotJudged=pruned_census)
    oversized, unavailable = exact_int(census['oversized_total'], 'oversized_total'), exact_int(census['unavailable_total'], 'unavailable_total')
    if census['eligible'] != pruned_census['eligible']:
        raise Violation('archive and pruned roles count different eligible claims at one generation')
    if oversized or unavailable:
        raise NotObservable('archive coverage incomplete: oversized_total=%d unavailable_total=%d oversized=%r unavailable=%r'
                            % (oversized, unavailable, census['oversized'], census['unavailable']))
    return facts


# ---- retirement reconciliation (checkbox 7) -----------------------------------------------------

def retirement_reconciliation(pre, post, retirement_daa):
    """Legacy bonds, DNS authority and duties on both sides of the fence, from live reads taken before and after it.

    `pre` must have been read while the virtual DAA was still below the fence (its `povDaa`); the stake-bond registry
    has no historical read, so a bond that exits later is only visible as absent."""
    require_real([pre, post])
    if (post['value'].get('dns') or {}).get('available'):
        raise Violation('DNS finality authority is still available after retirement')
    unverified = []
    precommit = post['value'].get('precommit')
    if precommit is None:
        unverified.append('no legacy bond identity to ask getPrecommitDuty with')
    elif precommit.get('retiredAt') != retirement_daa:
        raise Violation('precommit duties are not reported retired at the retirement height')
    pre_bonds = {bond['bondOutpoint']: bond for bond in pre['value']['bonds']}
    post_bonds = {bond['bondOutpoint']: bond for bond in post['value']['bonds']}
    for outpoint, bond in post_bonds.items():
        if outpoint not in pre_bonds and exact_int(bond['activationDaaScore'], 'activation') >= retirement_daa:
            raise Violation('a DNS validator bond activated at or after retirement')
    exited = []
    for outpoint, bond in pre_bonds.items():
        if outpoint in post_bonds:
            if post_bonds[outpoint]['amount'] != bond['amount']:
                raise Violation('pre-fence bond principal changed across retirement')
        else:
            exited.append(outpoint)
    pov = pre['value'].get('povDaa')
    if pov is None or pov >= retirement_daa:
        unverified.append('no live read was taken below the fence')
    if retirement_daa <= 1:
        unverified.append('retirement is armed at DAA %d, so no pre-fence block, accrual or entitlement exists' % retirement_daa)
    if not pre_bonds:
        unverified.append('no legacy DNS bond existed before the fence')
    if exited:
        unverified.append('%d pre-fence bonds exited; their exit payouts are not readable here' % len(exited))
    if unverified:
        raise NotObservable('cross-boundary reconciliation is vacuous or unreadable: ' + '; '.join(unverified))
    return dict(preFenceBonds=len(pre_bonds), postBonds=len(post_bonds))


def dns_reward_stop(post):
    raise NotObservable('no node read exposes the per-coinbase DNS validator share; the reward stop is only covered '
                        'by in-process tests (RFC-0012 x12), not by this real-process run')


def point_consumed(rows, point):
    require_real(rows)
    for row in rows:
        states = row['value'].get('pointStates', {})
        if point not in states:
            raise NotObservable('the consumed outpoint was not read')
        if states[point] is not None:
            raise Pending('the lock outpoint is still unspent')
    return dict(outpoint=point, unspent=False)


def point_present(rows, point, low, high):
    require_real(rows)
    for row in rows:
        states = row['value'].get('pointStates', {})
        if point not in states:
            raise NotObservable('the materialized outpoint was not read')
        entry = states[point]
        if entry is None:
            raise Pending('the outpoint is not materialized yet')
        if entry.get('isCoinbase') or not low <= exact_int(entry['amount'], 'outpoint amount') <= high:
            raise Violation('the materialized outpoint value/type differs from the expected transition')
    return dict(outpoint=point, low=low, high=high)


def stable_point_states(states):
    """Ignore acceptance DAA (which may move on a reorg); preserve actual rights."""
    return tuple(sorted((point, None if entry is None else
                         (exact_int(entry['amount'], 'point amount'), json.dumps(entry['scriptPublicKey'], sort_keys=True),
                          entry['isCoinbase'])) for point, entry in states.items()))


def refund_link(row, lock_point, refund_point, original_amount, maximum_fee):
    require_real([row])
    txid, index = refund_point.rsplit(':', 1)
    found = row['value'].get(norm(txid))
    if not found:
        raise Pending('the refund transaction body is not on the observed accepted chain yet')
    tx = found['transaction']
    inputs = tx.get('inputs') or []
    if len(inputs) != 1:
        raise NotObservable('refund conservation needs the complete one-input first-party refund path')
    if outpoint_key({'outpoint': inputs[0]['previousOutpoint']}) != lock_point:
        raise Violation('the refund output transaction did not spend the original deposit outpoint')
    outputs = tx.get('outputs') or []
    index = int(index)
    if index >= len(outputs):
        raise Violation('the observed refund outpoint is absent from its transaction')
    total = sum(exact_int(output['value'], 'refund output value') for output in outputs)
    fee = original_amount - total
    if not 0 <= fee <= maximum_fee:
        raise Violation('the signed refund does not conserve the deposit within its fee cap')
    if not original_amount - maximum_fee <= outputs[index]['value'] <= original_amount:
        raise Violation('the returned deposit amount differs from the declared refund')
    return dict(inputOutpoint=lock_point, refundOutpoint=refund_point, feeSompi=fee, outputTotalSompi=total)
