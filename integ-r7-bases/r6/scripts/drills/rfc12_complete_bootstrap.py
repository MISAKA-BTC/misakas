"""Optional actual complete-only onboarding for known owned private processes.

The caller must predeclare separate bond-cost and TOTAL carrier-cost limits.
Matching roots/clock come from three exact factory processes controlled by this
drill; they are not a generic trust rule for three arbitrary remote responses.
No clock, maturity, PoW, quorum, claim or Final is injected.
"""
def costs_before_nodes(bootstrap, maximum_binding, maximum_check):
    if maximum_binding is None or maximum_check is None:
        raise ValueError('complete-check requires explicit binding-reservation and complete-check-fee maxima')
    if type(maximum_binding) is not int or type(maximum_check) is not int \
            or not 0 <= maximum_binding <= 2**64 - 1 or not 0 <= maximum_check <= 2**64 - 1:
        raise ValueError('bond-cost maxima must fit u64')
    if canonical_u64_text(bootstrap['bindingReservationSompi']) > maximum_binding or canonical_u64_text(bootstrap['completeCheckFeeSompi']) > maximum_check:
        raise ValueError('the factory bond reservation/check fee exceeds the predeclared cap; no node was started')
    for name in ('commitment', 'statementRoot', 'commitmentScope'):
        if not bootstrap.get(name):
            raise ValueError('the exporter did not supply actual bounded code-fixture statement metadata')


def canonical_u64_text(value):
    if not isinstance(value, str) or not value or len(value) > 20 or (value != '0' and value.startswith('0')) \
            or any(c not in '0123456789' for c in value) or int(value) > 2**64 - 1:
        raise ValueError('factory money must be canonical u64 decimal text')
    return int(value)


def remaining_budget(budget):
    if budget.get('unresolved', 0) or budget.get('submissionOutstanding', False):
        raise AssertionError('a prior broadcast has unresolved fees; no further carrier may be submitted')
    if any(type(budget.get(field)) is not int or not 0 <= budget[field] <= 2**64 - 1 for field in ('maximum', 'spent')) \
            or budget['spent'] > budget['maximum']:
        raise AssertionError('workflow fee budget is not an exact u64 ledger')
    return budget['maximum'] - budget['spent']


def submit_with_reserved_budget(drill, sampler, label, argv, timeout, budget, command):
    # A CLI may broadcast part of a batch before a timeout or refusal. Hold
    # its complete predeclared remaining cap until a full receipt resolves it.
    remaining = remaining_budget(budget)
    budget.update(unresolved=remaining, submissionOutstanding=True, submittedFeesKnown=False)
    sent = command(drill, sampler, label, argv, timeout)
    if sent.get('ok') is not True or sent.get('submitted') is not True:
        raise AssertionError('broadcast did not return a complete success receipt')
    cap_and_charge(sent, budget, outstanding=remaining)
    return sent


def cli_prefix(drill, cli, salt, role='A'):
    return [str(cli), '--network=testnet-12', '--rpc=' + drill.plan['nodes'][role]['borsh'],
        '--palw-drill-genesis-salt=' + salt, '--output=json']


def cap_and_charge(sent, budget, outstanding=None):
    fees = [entry['fee_sompi'] for entry in sent['carriers']]
    if not fees or any(type(fee) is not int or fee < 0 or fee > 2**64 - 1 for fee in fees):
        raise AssertionError('the submitted carrier fees are not u64 deductions')
    remaining = remaining_budget(budget) if outstanding is None else outstanding
    if outstanding is not None and (budget.get('unresolved') != outstanding or budget.get('submissionOutstanding') is not True):
        raise AssertionError('broadcast reservation differs from this exact pending submission')
    total = sum(fees)
    if total > 2**64 - 1 or total > remaining:
        raise AssertionError('the actual submitted fees exceed the remaining total workflow cap')
    budget['spent'] += total
    budget.update(unresolved=0, submissionOutstanding=False, submittedFeesKnown=True)
    return total


def submit(drill, sampler, cli, salt, fixture, bootstrap, object_path, label, budget, command):
    remaining = remaining_budget(budget)
    argv = cli_prefix(drill, cli, salt) + ['palw', 'submit-object',
        '--key-file=' + str(fixture / bootstrap['fundingKeyFile']),
        '--maximum-total-fee-sompi=' + str(remaining), '--object', str(object_path)]
    preview = command(drill, sampler, label + '_PREVIEW', argv, 60)
    assert preview['dry_run'] is True and type(preview['total_fee_sompi']) is int and 0 <= preview['total_fee_sompi'] <= remaining
    sent = submit_with_reserved_budget(drill, sampler, label + '_SUBMIT', argv + ['--yes'], 60, budget, command)
    assert sent['ok'] is True and sent['submitted'] is True and len(sent['carriers']) == 1
    return dict(preview=preview, submission=sent, cumulativeCarrierFeeSompi=budget['spent'])


def pins_from_owned_agreement(rows):
    if set(rows) != {'A', 'B', 'JOIN'}:
        raise AssertionError('all three known private processes must supply a chain view')
    keys = [(row['sink'], row['virtualDaaScore'], row['onboarding']['ledgerRoot'], row['onboarding']['auxRoot'])
        for row in rows.values()]
    if len(set(keys)) != 1:
        raise AssertionError('private process roots or DAA disagree')
    row = rows['A']
    return dict(ledger=row['onboarding']['ledgerRoot'], aux=row['onboarding']['auxRoot'], daa=row['virtualDaaScore'])


def pin_args(pins):
    return ['--ledger-root=' + pins['ledger'], '--aux-root=' + pins['aux'], '--tip-daa=' + str(pins['daa'])]


def conformance_agreement(drill, bootstrap, agreement, rpc_read, state=None):
    rows = agreement(drill, bootstrap)
    if not rows:
        return None
    pins = pins_from_owned_agreement(rows)
    evidence = {}
    for role in ('A', 'B', 'JOIN'):
        r = rpc_read(drill, role, 'getPalwConformanceEvidence', {'classId': bootstrap['class']})
        if not r['available'] or not r['found'] or not r['attemptRow']:
            return None
        if (r['ledgerRoot'], r['auxRoot'], r['tipDaa']) != (pins['ledger'], pins['aux'], pins['daa']):
            return None
        assert r['statementRoot'] == bootstrap['statementRoot'], 'the included attempt substituted the fixture statement'
        if r['lastFailure'] or r['attemptEnd']:
            raise AssertionError('actual conformance failed: ' + r['lastFailure'] + '/' + r['attemptEnd'])
        if state is not None and r['lifecycleState'] != state:
            return None
        evidence[role] = r
    return dict(pins=pins, rows=rows, evidence=evidence)


def wait_mature(drill, bootstrap, agreement):
    def mature():
        rows = agreement(drill, bootstrap)
        if not rows:
            return None
        for row in rows.values():
            matching = [binding for binding in row['onboarding']['artifactBindings']
                if binding['kernelParamRoot'] == bootstrap['kernelParamRoot'] and binding['binder'] == bootstrap['bond']]
            assert len(matching) == 1
            binding = matching[0]
            if row['virtualDaaScore'] < binding['maturesDaa'] or binding['state'] not in ('Matured', 'Final'):
                return None
        return rows
    return drill.wait(mature, 'actual native artifact binding maturity without clock injection')


def kernel_registered(drill, bootstrap, agreement, rpc_read):
    rows = agreement(drill, bootstrap)
    if not rows:
        return None
    pins = pins_from_owned_agreement(rows)
    # Observation only: the CLI's SDK authenticates the complete inventory
    # against these caller pins before signing the following binding.
    for role in ('A', 'B', 'JOIN'):
        cursor = dict(hasCursor=False, afterTable=0, afterKey='')
        last, found, count, size, identity = None, False, 0, 0, None
        for _ in range(256):
            page = rpc_read(drill, role, 'getPalwKernelRows', dict(maxBytes=32768, **cursor))
            current = (page['tipDaa'], page['ledgerRoot'], page['auxRoot'], page['header'], page['totalRows'])
            if current[:3] != (pins['daa'], pins['ledger'], pins['aux']):
                return None
            if identity is not None and current != identity:
                return None
            identity = current
            if not page['available'] or page['totalRows'] > 10000:
                raise AssertionError('the finite fixture kernel inventory exceeds its observation bound')
            for r in page['rows']:
                key = (r['table'], r['key'])
                if last is not None and key <= last:
                    raise AssertionError('kernel pagination repeated or reordered a row')
                last = key
                count += 1
                size += len(r['key']) // 2 + len(r['row']) // 2
                if count > 10000 or size > 8 << 20:
                    raise AssertionError('the finite fixture kernel inventory exceeds its observation bound')
                found |= r['table'] == 2 and r['key'] == bootstrap['kernelClass']
            if not page['more']:
                if count != page['totalRows']:
                    raise AssertionError('the kernel inventory is incomplete')
                break
            next_key = (page['nextTable'], page['nextKey'])
            if not page['rows'] or next_key != last:
                raise AssertionError('the kernel cursor is not the last served row')
            cursor = dict(hasCursor=True, afterTable=next_key[0], afterKey=next_key[1])
        else:
            raise AssertionError('finite kernel pagination did not terminate')
        if not found:
            return None
    return rows


def prepare(drill, sampler, cli, salt, fixture, bootstrap, stage, rows, maximum_binding, maximum_check, command):
    pins = pins_from_owned_agreement(rows)
    output = drill.root / (stage + '.borsh')
    argv = cli_prefix(drill, cli, salt) + ['model', 'onboard', 'bootstrap-prepare', stage,
        '--registration=' + str(fixture / bootstrap['nativeRegistration']), '--artifact=' + str(fixture / bootstrap['artifact']),
        '--max-positions=' + str(bootstrap['maxPositions']), '--bond=' + bootstrap['bond'],
        '--key-file=' + str(fixture / bootstrap['bondKeyFile']), '--out=' + str(output),
        '--maximum-binding-reservation-sompi=' + str(maximum_binding), '--maximum-check-fee-sompi=' + str(maximum_check)] + pin_args(pins)
    if stage == 'commit':
        argv += ['--commitment-borsh=' + str(fixture / bootstrap['commitment'])]
    prepared = command(drill, sampler, stage.upper() + '_PREPARE', argv, 60)
    assert prepared['submitted'] is False and prepared['verification_scope'] == 'caller-pinned-bootstrap-snapshot'
    assert prepared['class'] == bootstrap['class'] and prepared['kernel_class'] == bootstrap['kernelClass']
    assert output.is_file() and 0 < output.stat().st_size <= 2 << 20
    return output, prepared


def run(drill, sampler, cli, salt, fixture, bootstrap, budget, maximum_binding, maximum_check, agreement, rpc_read, command):
    rows = wait_mature(drill, bootstrap, agreement)
    evidence = dict(matured=rows, scope='known-owned-private-factory-complete-code-fixture')
    for stage in ('kernel-register', 'kernel-bind', 'commit'):
        output, prepared = prepare(drill, sampler, cli, salt, fixture, bootstrap, stage, rows, maximum_binding, maximum_check, command)
        evidence[stage] = dict(preparation=prepared, **submit(drill, sampler, cli, salt, fixture, bootstrap, output, stage.upper(), budget, command))
        if stage == 'kernel-register':
            rows = drill.wait(lambda: kernel_registered(drill, bootstrap, agreement, rpc_read), 'real kernel registration across three P2P processes')
        elif stage == 'kernel-bind':
            def bound():
                r = agreement(drill, bootstrap)
                if not r:
                    return None
                return r if all(row['onboarding']['kernelBound'] and row['onboarding']['kernelClass'] == bootstrap['kernelClass']
                    and row['onboarding']['kernelPlanRoot'] == bootstrap['verificationPlanRoot'] for row in r.values()) else None
            rows = drill.wait(bound, 'real native/kernel binding across three P2P processes')
        else:
            committed = drill.wait(lambda: conformance_agreement(drill, bootstrap, agreement, rpc_read, 'CHALLENGE_PENDING'), 'real complete-policy conformance commitment')
            rows = committed['rows']
            evidence['committed'] = committed
    pins = pins_from_owned_agreement(rows)
    output = drill.root / 'post-complete.borsh'
    argv = cli_prefix(drill, cli, salt) + ['model', 'onboard', 'complete-post', '--class=' + bootstrap['class'],
        '--artifact=' + str(fixture / bootstrap['artifact']), '--bond=' + bootstrap['bond'],
        '--key-file=' + str(fixture / bootstrap['bondKeyFile']), '--out=' + str(output)] + pin_args(pins)
    prepared = command(drill, sampler, 'POST_COMPLETE_PREPARE', argv, 60)
    assert prepared['submitted'] is False and output.is_file() and 0 < output.stat().st_size <= 100000
    evidence['post'] = dict(preparation=prepared, **submit(drill, sampler, cli, salt, fixture, bootstrap, output, 'POST_COMPLETE', budget, command))
    evidence['passed'] = drill.wait(lambda: conformance_agreement(drill, bootstrap, agreement, rpc_read, 'CONFORMANCE_PASSED'), 'actual in-fold exhaustive complete-check verdict')
    return evidence


def verify_after_cold_reopen(drill, sampler, cli, salt, fixture, bootstrap, agreement, rpc_read, command):
    passed = drill.wait(lambda: conformance_agreement(drill, bootstrap, agreement, rpc_read, 'CONFORMANCE_PASSED'), 'cold persisted complete-check verdict across three processes')
    verified = {}
    for role in ('A', 'B', 'JOIN'):
        argv = cli_prefix(drill, cli, salt, role) + ['model', 'onboard', 'verify', '--class=' + bootstrap['class'],
            '--artifact=' + str(fixture / bootstrap['artifact'])] + pin_args(passed['pins'])
        report = command(drill, sampler, 'VERIFY_' + role, argv, 60)
        assert report['agrees'] is True and report['honest_passes'] is True and report['chain_says_passed'] is True
        assert report['chain_post_id'] == report['honest_post_id'] and report['chain_post_id'], 'the chain did not judge this exact exhaustive post'
        verified[role] = report
    return dict(passed=passed, verification=verified)
