"""Judgement predicate counterexamples; synthetic rows here exercise the predicates, never a node PASS."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_judgement as J  # noqa: E402
from rfc06_exec_origin_coverage import NATIVE_HEADER_CAP_BYTES as HEADER_CAP  # noqa: E402

IDENTITY = dict(genesis='11' * 64, consensusParamsId='22' * 32, retirementActivationDaa=1, retirementPolicyId='33' * 64)


def block(head, number):
    return None if head is None else dict(hash='0x' + head[:64], number=hex(number), stateRoot='0x' + '99' * 32)


def row(role='A', sink='aa' * 64, latest='dd' * 64, safe='bb' * 64, finalized='cc' * 64, stop=None, withdrawn=None,
        epoch=1, ancestry=None, numbers=(30, 20, 10), pruning='cc' * 64):
    native = dict(version=1, rulesetId='22' * 32, policyId='33' * 64, generation=sink, retirementDaa=1, frontier=None,
                  latest=latest, safe=safe, finalized=finalized, depth=3, uniqueWork='100', stop=stop)
    readiness = dict(version=1, generation=sink, safe=safe, stop=stop,
                     finalized=dict(finalized=finalized, pruningPoint=pruning, withdrawnFrom=withdrawn))
    eth = {tag: dict(result=block(head, number), error=None)
           for tag, head, number in zip(J.TAGS, (latest, safe, finalized), numbers)}
    return dict(source='node-rpc', role=role, label='unit', oldBodyAvailable=False,
                dag=dict(sink=sink, pruningPointHash=pruning, virtualDaaScore=100),
                node=dict(importSessionEpoch=epoch, importSessionPoint='', importSessionPhase='idle', pruningImportStable=True),
                value=dict(header=dict(utxoCommitment='01' * 64, evmCommitmentRoot='02' * 64, palwStateRoot='03' * 64),
                           settlement=dict(dnsRetiredAt=1, nativeSettlement=native, nativeReadiness=readiness),
                           dns=dict(available=False), eth=eth, ancestry=ancestry or {}))


def verdict(function, *args):
    return J.judge(4, 'unit', function, *args)['verdict']


class ConsumerCoherence(unittest.TestCase):
    def test_empty_evidence_cannot_be_observed(self):
        cases = ((J.agreement, []), (J.finalized_history, {}),
                 (J.finalized_history, {'A': []}),
                 (J.finalized_history, {'A': [row()], 'B': []}))
        for predicate, evidence in cases:
            with self.subTest(predicate=predicate.__name__, evidence=evidence):
                self.assertEqual(verdict(predicate, evidence), J.BLOCKED)

    def test_coherent_row_is_observed_with_executed_order(self):
        facts = J.consumer_row(row(), IDENTITY)
        self.assertEqual(facts['evmNumbers'], dict(latest=30, safe=20, finalized=10))

    def test_each_incoherence_fails(self):
        cases = {
            'mixed generation': lambda r: r['value']['settlement']['nativeSettlement'].update(generation='ff' * 64),
            'other ruleset': lambda r: r['value']['settlement']['nativeSettlement'].update(rulesetId='44' * 32),
            'other policy': lambda r: r['value']['settlement']['nativeSettlement'].update(policyId='44' * 64),
            'readiness safe': lambda r: r['value']['settlement']['nativeReadiness'].update(safe='ee' * 64),
            'live dns': lambda r: r['value']['dns'].update(available=True),
            'eth projection': lambda r: r['value']['eth']['safe']['result'].update(hash='0x' + 'ab' * 32),
            'eth fallback': lambda r: (r['value']['settlement']['nativeSettlement'].update(finalized=None),
                                       r['value']['settlement']['nativeReadiness']['finalized'].update(finalized=None)),
            'order': lambda r: r['value']['eth']['safe']['result'].update(number=hex(40)),
        }
        for name, mutate in cases.items():
            bad = row()
            mutate(bad)
            with self.subTest(name=name):
                self.assertEqual(verdict(J.consumer_row, bad, IDENTITY), J.FAIL)

    def test_conflict_and_withdrawal_publication_rules(self):
        self.assertEqual(verdict(J.consumer_row, row(stop='finalizedConflict'), IDENTITY), J.FAIL)
        conflict = row(safe=None, finalized=None, stop='finalizedConflict', numbers=(30, 0, 0))
        self.assertEqual(verdict(J.consumer_row, conflict, IDENTITY), J.OBSERVED)
        self.assertEqual(verdict(J.consumer_row, row(withdrawn='cc' * 64), IDENTITY), J.FAIL)
        withdrawn = row(finalized=None, withdrawn='cc' * 64)
        self.assertEqual(verdict(J.consumer_row, withdrawn, IDENTITY), J.OBSERVED)

    def test_no_retirement_or_synthetic_rows_are_blocked_not_passed(self):
        dormant = row()
        dormant['value']['settlement']['dnsRetiredAt'] = None
        self.assertEqual(verdict(J.consumer_row, dormant, IDENTITY), J.BLOCKED)
        for source in ('fixture-command', None):
            fake = row()
            fake['source'] = source
            self.assertEqual(verdict(J.consumer_row, fake, IDENTITY), J.BLOCKED)
        flagged = row()
        flagged['synthetic'] = True
        self.assertEqual(verdict(J.consumer_row, flagged, IDENTITY), J.BLOCKED)

    def test_agreement_and_one_root_set_per_sink(self):
        rows = [row('A'), row('B')]
        J.agreement(rows)
        J.root_consistency(rows)
        rows[1]['value']['header']['evmCommitmentRoot'] = '07' * 64
        self.assertEqual(verdict(J.agreement, rows), J.FAIL)
        self.assertEqual(verdict(J.root_consistency, rows), J.FAIL)
        self.assertEqual(verdict(J.root_consistency, []), J.BLOCKED)


class History(unittest.TestCase):
    def test_finalized_advance_must_stay_on_chain_unless_a_validated_import_reset_it(self):
        first, second = row(finalized='cc' * 64), row(finalized='c1' * 64, ancestry={'cc' * 64: True})
        second['value']['settlement']['nativeReadiness']['finalized']['finalized'] = 'c1' * 64
        self.assertEqual(J.finalized_history({'A': [first, second]})['advances'], 1)
        second['value']['ancestry'] = {'cc' * 64: False}
        self.assertEqual(verdict(J.finalized_history, {'A': [first, second]}), J.FAIL)
        second['node']['importSessionEpoch'] = 2
        self.assertEqual(verdict(J.finalized_history, {'A': [first, second]}), J.OBSERVED)
        second['node']['importSessionEpoch'] = 1
        second['value']['ancestry'] = {}
        self.assertEqual(verdict(J.finalized_history, {'A': [first, second]}), J.BLOCKED)

    def test_conflict_is_sticky_and_withdrawal_is_named(self):
        conflict = row(safe=None, finalized=None, stop='finalizedConflict')
        cleared = row(safe=None, finalized=None)
        self.assertEqual(verdict(J.finalized_history, {'A': [conflict, cleared]}), J.FAIL)
        cleared['node']['importSessionEpoch'] = 2
        self.assertEqual(verdict(J.finalized_history, {'A': [conflict, cleared]}), J.OBSERVED)
        silent = row(finalized=None)
        self.assertEqual(verdict(J.finalized_history, {'A': [row(), silent]}), J.FAIL)
        named = row(finalized=None, withdrawn='cc' * 64)
        self.assertEqual(verdict(J.finalized_history, {'A': [row(), named]}), J.OBSERVED)
        self.assertEqual(verdict(J.conflict_exercised, [row()]), J.BLOCKED)
        self.assertEqual(verdict(J.conflict_exercised, [conflict]), J.OBSERVED)

    def test_incomplete_import_publishes_no_tag(self):
        inside = row(latest=None, safe=None, finalized=None)
        inside['node'].update(pruningImportStable=False, importSessionPhase='utxo-stream', importSessionPoint='ab' * 64)
        self.assertEqual(verdict(J.incomplete_import_publishes_nothing, [inside]), J.OBSERVED)
        leaking = row()
        leaking['node'].update(pruningImportStable=False, importSessionPhase='sidecars')
        self.assertEqual(verdict(J.incomplete_import_publishes_nothing, [inside, leaking]), J.FAIL)
        self.assertEqual(verdict(J.incomplete_import_publishes_nothing, [row()]), J.BLOCKED)


class Controls(unittest.TestCase):
    def point(self, kind, **trigger):
        return dict(name='p', role='producer', trigger=dict(kind=kind, **trigger))

    def test_unreached_or_unrecovered_crash_points(self):
        self.assertEqual(verdict(J.crash_point, self.point('after-convergence'), dict(killed=False)), J.BLOCKED)
        record = dict(killed=True, recovered=False, trigger=dict(source='node-rpc'), before={}, after={})
        self.assertEqual(verdict(J.crash_point, self.point('after-convergence'), record), J.FAIL)

    def test_import_phase_kill_must_end_stable(self):
        point = self.point('import-phase', phases=['utxo-stream'])
        stable = dict(pruningImportStable=True, importSessionPoint='', importSessionPhase='idle')
        record = dict(killed=True, recovered=True, trigger=dict(source='node-rpc'),
                      before=dict(importSessionPhase='utxo-stream'), after=stable)
        self.assertEqual(verdict(J.crash_point, point, record), J.OBSERVED)
        record['after'] = dict(stable, importSessionPhase='incomplete', pruningImportStable=False)
        self.assertEqual(verdict(J.crash_point, point, record), J.FAIL)
        record['before'] = dict(importSessionPhase='sidecars')
        self.assertEqual(verdict(J.crash_point, point, record), J.BLOCKED)

    def test_outbox_records_are_immutable_across_the_crash(self):
        point = self.point('kernel-outbox', present='object.borsh', absent='transaction.borsh')
        claim = dict(source='node-rpc', value=dict(enrollment='e1' * 64, producerBond='aa:0', jobId='j' * 128,
                                                   classId='c' * 128, receipts=['bb:1']))
        record = dict(killed=True, recovered=True, trigger=dict(source='owned-file'), before=dict(outbox={'object.borsh': 'x'}),
                      after=dict(outbox={'object.borsh': 'x', 'transaction.borsh': 'y'}), claimRows=[claim, copy.deepcopy(claim)])
        self.assertEqual(verdict(J.crash_point, point, record), J.OBSERVED)
        record['after'] = dict(outbox={'object.borsh': 'rewritten'})
        self.assertEqual(verdict(J.crash_point, point, record), J.FAIL)
        record['after'], record['claimRows'] = dict(outbox={'object.borsh': 'x'}), []
        self.assertEqual(verdict(J.crash_point, point, record), J.BLOCKED)
        checker = self.point('checker-outbox', present='receipt.borsh', absent=None)
        record.update(claimRows=[claim], checkerBond='bb:1')
        self.assertEqual(verdict(J.crash_point, checker, record), J.OBSERVED)
        twice = copy.deepcopy(claim)
        twice['value']['receipts'] = ['bb:1', 'bb:1']
        record['claimRows'] = [twice]
        self.assertEqual(verdict(J.crash_point, checker, record), J.FAIL)

    def test_generation_mismatch_must_refuse_without_writing(self):
        record = dict(launch=dict(source='owned-process', exitedWithinBound=True, exitCode=1,
                                  stderrTail='private probe DB fingerprint/policy differs from its pinned identity'),
                      markerBefore='m', markerAfter='m', dbBefore={'a': 1}, dbAfter={'a': 1}, recovered=True,
                      priorEpoch=3, relaunchEpoch=3)
        self.assertEqual(verdict(J.generation_mismatch, record), J.OBSERVED)
        for field, value in (('markerAfter', 'n'), ('dbAfter', {'a': 2}), ('relaunchEpoch', 4), ('recovered', False)):
            with self.subTest(field=field):
                self.assertEqual(verdict(J.generation_mismatch, dict(record, **{field: value})), J.FAIL)
        self.assertEqual(verdict(J.generation_mismatch, dict(record, launch=dict(record['launch'], exitCode=0))), J.FAIL)
        self.assertEqual(verdict(J.generation_mismatch, dict(record, launch=dict(record['launch'], exitedWithinBound=False))), J.FAIL)
        self.assertEqual(verdict(J.generation_mismatch, dict(record, launch=dict(record['launch'], stderrTail='port in use'))),
                         J.BLOCKED)

    def test_capacity_recovery_requires_unchanged_epoch_and_participation(self):
        record = dict(priorEpoch=2, constrainedRows=[row(epoch=2)], recovered=True, recoveredRows=[row(epoch=2)],
                      constrained=dict(exitCode=None),
                      participationBefore=dict(virtualDaaScore=10, sink='x', networkSink='x', producedBlocks=0, storeBytes=5),
                      participationAfter=dict(virtualDaaScore=20, sink='y', networkSink='y', producedBlocks=1, storeBytes=9))
        for signal in ('sink-follows', 'provider-store-grows', 'produced-blocks-increase'):
            self.assertEqual(verdict(J.capacity_recovery, record, signal), J.OBSERVED)
        self.assertEqual(verdict(J.capacity_refusal_exercised, record), J.BLOCKED)
        self.assertEqual(verdict(J.capacity_refusal_exercised, dict(record, constrained=dict(exitCode=3))), J.OBSERVED)
        self.assertEqual(verdict(J.capacity_recovery, dict(record, constrainedRows=[row(epoch=3)]), 'sink-follows'), J.FAIL)
        self.assertEqual(verdict(J.capacity_recovery, dict(record, recovered=False), 'sink-follows'), J.FAIL)
        lagging = dict(record, participationAfter=dict(record['participationAfter'], networkSink='z'))
        self.assertEqual(verdict(J.capacity_recovery, lagging, 'sink-follows'), J.FAIL)


class Scale(unittest.TestCase):
    def counted(self, role, total, sink='aa' * 64):
        return dict(source='node-rpc', role=role, dag=dict(sink=sink),
                    value=dict(finals=dict(available=True, total=total), shadow=dict(available=True, claimsTotal=total)))

    def test_scale_needs_one_generation_and_the_floor(self):
        rows = [self.counted('A', 100_000), self.counted('B', 100_000)]
        self.assertEqual(verdict(J.scale_reached, rows, 'kernelFinalsTotal', 100_000), J.OBSERVED)
        self.assertEqual(verdict(J.scale_reached, [self.counted('A', 99_999)], 'kernelFinalsTotal', 100_000), J.FAIL)
        self.assertEqual(verdict(J.scale_reached, [self.counted('A', 100_000), self.counted('B', 100_001)],
                                 'capacityShadowLiveClaims', 100_000), J.FAIL)
        with self.assertRaises(J.Pending):
            J.scale_reached([self.counted('A', 100_000), self.counted('B', 100_000, 'bb' * 64)], 'kernelFinalsTotal', 1)

    def test_pruned_join_and_reopen(self):
        source, joined = row('A', pruning='cc' * 64), row('FRESH', pruning='cc' * 64)
        self.assertEqual(verdict(J.fresh_join, source, joined, IDENTITY['genesis']), J.OBSERVED)
        genesis = row('FRESH', pruning='11' * 64, finalized='11' * 64)
        self.assertEqual(verdict(J.non_genesis_finalized, genesis, IDENTITY['genesis']), J.FAIL)  # Pending at judgement
        other = row('FRESH', pruning='cc' * 64, finalized='c9' * 64)
        self.assertEqual(verdict(J.non_genesis_finalized, other, IDENTITY['genesis']), J.FAIL)
        kept = row('FRESH')
        kept['oldBodyAvailable'] = True
        self.assertEqual(verdict(J.fresh_join, source, kept, IDENTITY['genesis']), J.FAIL)
        reopened = row('FRESH', epoch=2)
        self.assertEqual(verdict(J.cold_reopen, joined, reopened, source), J.FAIL)
        self.assertEqual(verdict(J.cold_reopen, joined, row('FRESH'), source), J.OBSERVED)


class Bridge(unittest.TestCase):
    def evm(self, role, balance, head='h1'):
        return dict(source='node-rpc', role=role, value=dict(ethLatestHash=head, balanceWei=balance))

    def test_deposit_credit_is_exact_and_single(self):
        credit = (1000 - 10) * J.EVM_NATIVE_SCALE
        rows = [self.evm('A', 5 + credit), self.evm('B', 5 + credit)]
        self.assertEqual(J.deposit_credit(5, rows, 1000, 10)['creditedWei'], credit)
        self.assertEqual(verdict(J.deposit_credit, 5, [self.evm('A', 5 + 2 * credit)], 1000, 10), J.FAIL)
        with self.assertRaises(J.Pending):
            J.deposit_credit(5, [self.evm('A', 5)], 1000, 10)
        self.assertEqual(verdict(J.deposit_credit, 5, [self.evm('A', 5 + credit), self.evm('B', 5)], 1000, 10), J.FAIL)

    def entry(self, tx, amount, coinbase=False):
        return dict(address='misakatest:q', outpoint=dict(transactionId=tx, index=0),
                    utxoEntry=dict(amount=amount, blockDaaScore=1, isCoinbase=coinbase))

    def test_materialization_exactly_once(self):
        before = [self.entry('aa' * 64, 7)]
        self.assertEqual(J.exactly_one_new_utxo(before, before + [self.entry('bb' * 64, 100)], 100, 100)['amount'], 100)
        self.assertEqual(verdict(J.exactly_one_new_utxo, before, before + [self.entry('bb' * 64, 100),
                                                                           self.entry('cc' * 64, 100)], 100, 100), J.FAIL)
        self.assertEqual(verdict(J.exactly_one_new_utxo, before, before + [self.entry('bb' * 64, 99)], 100, 100), J.FAIL)
        self.assertEqual(verdict(J.exactly_one_new_utxo, before, before + [self.entry('bb' * 64, 100, True)], 100, 100), J.FAIL)
        self.assertEqual(verdict(J.refused, dict(source='owned-process', exitCode=0), 'x'), J.FAIL)
        self.assertEqual(verdict(J.refused, dict(source='owned-process', exitCode=2), 'x'), J.OBSERVED)

    def test_backing_and_stable_effects(self):
        scale = J.EVM_NATIVE_SCALE
        J.backing(10, 10 + 500 * scale - 200 * scale - 7, 500 * scale, 200, 7)
        self.assertEqual(verdict(J.backing, 10, 10 + 500 * scale, 500 * scale, 200, 7), J.FAIL)
        snap = dict(source='node-rpc', role='A', value=dict(withdrawalOutpoints=['w:0'], refundOutpoints=['r:0']))
        self.assertEqual(verdict(J.effects_stable, [snap, dict(snap, role='FRESH')]), J.OBSERVED)
        doubled = dict(snap, value=dict(withdrawalOutpoints=['w:0', 'w2:0'], refundOutpoints=['r:0']))
        self.assertEqual(verdict(J.effects_stable, [snap, doubled]), J.FAIL)

    def test_once_only_settlement(self):
        base = dict(source='node-rpc', role='A', dag=dict(sink='s1'),
                    value=dict(stage='maturing', rowsForClaim=1, finalsForClaim=1, vestingRow={'claimId': 'c'}))
        moved = dict(base, dag=dict(sink='s2'), value=dict(base['value'], stage='moved', rowsForClaim=0, vestingRow=None))
        self.assertEqual(verdict(J.once_only_settlement, [base, moved]), J.OBSERVED)
        again = dict(base, dag=dict(sink='s3'), value=dict(base['value'], movedAncestor=True))
        self.assertEqual(verdict(J.once_only_settlement, [base, moved, again]), J.FAIL)
        twice = dict(base, value=dict(base['value'], rowsForClaim=2))
        self.assertEqual(verdict(J.once_only_settlement, [twice]), J.FAIL)
        never = dict(base, value=dict(base['value'], stage='', finalsForClaim=0))
        self.assertEqual(verdict(J.once_only_settlement, [never]), J.BLOCKED)

    def test_market_fill_units(self):
        rows = [dict(source='node-rpc', role=r, value=dict(tipHash='t', units=7)) for r in ('A', 'B')]
        self.assertEqual(J.units_changed(5, rows, 2, 'buy')['after'], 7)
        self.assertEqual(verdict(J.units_changed, 5, rows, 1, 'buy'), J.FAIL)
        with self.assertRaises(J.Pending):
            J.units_changed(7, rows, 2, 'buy')


class Money(unittest.TestCase):
    def tx(self, inputs, outputs):
        return dict(inputs=[dict(previousOutpoint=dict(transactionId=t, index=i)) for t, i in inputs],
                    outputs=[dict(value=v) for v in outputs])

    def test_accepted_fee_is_exact_or_unknown(self):
        self.assertEqual(J.accepted_fee(self.tx([('aa', 0)], [90]), {'aa:0': 100}), (10, []))
        self.assertEqual(J.accepted_fee(self.tx([('aa', 0)], [90]), {}), (None, ['aa:0']))
        with self.assertRaises(J.Violation):
            J.accepted_fee(self.tx([('aa', 0)], [101]), {'aa:0': 100})

    def test_cli_exposure_and_accepted_fee_stay_separate(self):
        records = [dict(label='x', txid='t1', exposureFeeSompi=10), dict(label='y', txid='t2', exposureFeeSompi=20)]
        facts = J.fee_separation(records, {'t1': dict(accepted=True, feeSompi=10), 't2': dict(accepted=False, feeSompi=None)})
        self.assertEqual((facts['cliSubmissionFeeExposureSompi'], facts['acceptedNativeFeeSompi']), (30, 10))
        self.assertEqual(facts['exposureOnlyTxids'], ['t2'])
        self.assertEqual(verdict(J.fee_separation, records, {'t1': dict(accepted=True, feeSompi=11)}), J.FAIL)
        self.assertEqual(verdict(J.fee_separation, records, {'t1': dict(accepted=True, feeSompi=None)}), J.BLOCKED)
        self.assertEqual(verdict(J.fee_separation, [], {}), J.BLOCKED)

    def entry(self, tx, amount, coinbase=False):
        return dict(outpoint=dict(transactionId=tx, index=0), utxoEntry=dict(amount=amount, isCoinbase=coinbase))

    def test_dedicated_payer_cost_and_budget(self):
        self.assertEqual(J.dedicated_payer_cost([self.entry('a', 100)], [self.entry('b', 70)], 50, 'x')['costSompi'], 30)
        self.assertEqual(verdict(J.dedicated_payer_cost, [self.entry('a', 100)], [self.entry('b', 10)], 50, 'x'), J.FAIL)
        self.assertEqual(verdict(J.dedicated_payer_cost, [self.entry('a', 100)], [self.entry('b', 120)], 50, 'x'), J.BLOCKED)
        self.assertEqual(verdict(J.dedicated_payer_cost, [], [self.entry('b', 1, True)], 50, 'x'), J.BLOCKED)
        self.assertEqual(verdict(J.workflow_budget, dict(a=10, b=None), 100), J.BLOCKED)
        self.assertEqual(verdict(J.workflow_budget, dict(a=101, b=None), 100), J.FAIL)
        self.assertEqual(verdict(J.workflow_budget, dict(a=10, b=20), 100), J.OBSERVED)
        self.assertEqual(verdict(J.real_income, dict(vestingAvailable=False)), J.BLOCKED)


class UsefulWorkAndRetirement(unittest.TestCase):
    def claim(self, role, sink='s', **changes):
        value = dict(available=True, found=True, sink=sink, enrollment='e1' * 64, producerBond='p:0', classId='c' * 128,
                     jobId='j' * 128, materialConsent='m' * 128, seats=['k:1'], quorum=1, receipts=['k:1'], final=True,
                     finalDaa=9)
        value.update(changes)
        return dict(source='node-rpc', role=role, dag=dict(sink=sink), value=value)

    def test_paid_claim_receipts_and_final(self):
        expected = dict(producerBond='p:0', classId='c' * 128, jobId='j' * 128)
        rows = [self.claim('A'), self.claim('B')]
        self.assertEqual(verdict(J.paid_claim_accepted, rows, expected), J.OBSERVED)
        self.assertEqual(verdict(J.paid_claim_accepted, rows, dict(expected, jobId='k' * 128)), J.FAIL)
        self.assertEqual(verdict(J.paid_claim_accepted, [self.claim('A', available=False)], expected), J.BLOCKED)
        stale = self.claim('A')
        stale['value']['sink'] = 'other'  # the read answered from another generation than its own bracket
        self.assertEqual(verdict(J.paid_claim_accepted, [stale], expected), J.FAIL)
        self.assertEqual(verdict(J.checker_receipts, rows, ['k:1']), J.OBSERVED)
        self.assertEqual(verdict(J.checker_receipts, [self.claim('A', receipts=['k:1', 'k:1'])], ['k:1']), J.FAIL)
        self.assertEqual(verdict(J.paid_claim_final, rows), J.OBSERVED)
        self.assertEqual(verdict(J.paid_claim_final, [self.claim('A'), self.claim('B', finalDaa=10)]), J.FAIL)
        self.assertEqual(verdict(J.checker_outcome, [dict(kind='challenge-accepted', line='x')]), J.FAIL)
        self.assertEqual(verdict(J.checker_outcome, [dict(kind='checker-stopped', line='x')]), J.BLOCKED)

    def test_retirement_armed_only_by_a_matching_private_profile(self):
        manifest = dict(privateProbeProfile=dict(genesis='11' * 64, consensusParamsId='22' * 32, skipProofOfWork=False,
                                                 releaseApproval=False, forkChoiceRuleEDaa=1,
                                                 retirement=dict(activationDaa=1, policyId='33' * 64)))
        network = dict(kind='private-profile', profile='paid-v4')
        self.assertEqual(verdict(J.retirement_armed, network, manifest, IDENTITY), J.OBSERVED)
        self.assertEqual(verdict(J.retirement_armed, dict(kind='preset', name='testnet-12'), {}, IDENTITY), J.BLOCKED)
        dormant = copy.deepcopy(manifest)
        dormant['privateProbeProfile']['retirement'] = None
        self.assertEqual(verdict(J.retirement_armed, network, dormant, IDENTITY), J.BLOCKED)
        late_rule = copy.deepcopy(manifest)
        late_rule['privateProbeProfile']['forkChoiceRuleEDaa'] = 2
        self.assertEqual(verdict(J.retirement_armed, network, late_rule, IDENTITY), J.FAIL)
        other = copy.deepcopy(manifest)
        other['privateProbeProfile']['retirement']['policyId'] = '44' * 64
        self.assertEqual(verdict(J.retirement_armed, network, other, IDENTITY), J.BLOCKED)

    def legacy(self, bonds, pov, available=False, precommit=None):
        return dict(source='node-rpc', value=dict(bonds=bonds, povDaa=pov, dns=dict(available=available), precommit=precommit))

    def test_retirement_reconciliation_is_blocked_when_vacuous_and_fails_on_violations(self):
        bond = dict(bondOutpoint='b:0', amount=5, activationDaaScore=0)
        pre, post = self.legacy([bond], 0), self.legacy([bond], None, precommit=dict(retiredAt=1))
        self.assertEqual(verdict(J.retirement_reconciliation, pre, post, 1), J.BLOCKED)  # armed at DAA 1: vacuous
        self.assertEqual(verdict(J.retirement_reconciliation, pre, post, 50), J.FAIL)  # precommit retired elsewhere
        post50 = self.legacy([bond], None, precommit=dict(retiredAt=50))
        self.assertEqual(verdict(J.retirement_reconciliation, pre, post50, 50), J.OBSERVED)
        grown = self.legacy([dict(bond, amount=6)], None, precommit=dict(retiredAt=50))
        self.assertEqual(verdict(J.retirement_reconciliation, pre, grown, 50), J.FAIL)
        late = self.legacy([bond, dict(bondOutpoint='n:0', amount=1, activationDaaScore=60)], None, precommit=dict(retiredAt=50))
        self.assertEqual(verdict(J.retirement_reconciliation, pre, late, 50), J.FAIL)
        self.assertEqual(verdict(J.retirement_reconciliation, pre, self.legacy([bond], None, True, dict(retiredAt=50)), 50), J.FAIL)
        self.assertEqual(verdict(J.dns_reward_stop, post), J.BLOCKED)


class Coverage(unittest.TestCase):
    def file_row(self, role, daa=100, sink='s', oversized=0, unavailable=0, eligible=3, approval=False):
        census = dict(cap_bytes=HEADER_CAP, eligible=eligible, within_cap=eligible - oversized - unavailable,
                      max_bytes_seen=HEADER_CAP + 1 if oversized else (100 if eligible > unavailable else 0),
                      oversized=[[format(i, '0128x'), 'bb' * 64, HEADER_CAP + 1] for i in range(min(oversized, 256))],
                      unavailable=[[format(i + oversized, '0128x'), 'cc' * 64] for i in range(min(unavailable, 256))],
                      oversized_total=oversized, unavailable_total=unavailable)
        envelope = dict(schemaVersion=2, kind=J.COVERAGE_KIND, consensusParamsId='22' * 32,
                        generation=dict(sink=sink, virtualDaaScore=daa, importEpoch=7),
                        census=census, error=None, activationApproval=approval)
        return dict(source='node-rpc', role=role, dag=dict(sink=sink, virtualDaaScore=100),
                    node=dict(importSessionAvailable=True, importSessionEpoch=7, pruningImportStable=True,
                              importSessionPoint='', importSessionPhase=next(iter(J.STABLE_IMPORT_PHASES))), value=dict(coverage=envelope))

    def test_archive_covers_and_pruned_is_only_reported(self):
        params = '22' * 32
        facts = J.archive_coverage(self.file_row('B'), self.file_row('A', unavailable=3), params)
        self.assertEqual(facts['prunedReportedNotJudged']['unavailable_total'], 3)
        self.assertEqual(verdict(J.archive_coverage, self.file_row('B', unavailable=1), self.file_row('A'), params), J.BLOCKED)
        self.assertEqual(verdict(J.archive_coverage, self.file_row('B', oversized=1), self.file_row('A'), params), J.BLOCKED)
        self.assertEqual(verdict(J.archive_coverage, self.file_row('B', approval=True), self.file_row('A'), params), J.FAIL)
        self.assertEqual(verdict(J.archive_coverage, self.file_row('B'), self.file_row('A', eligible=4), params), J.FAIL)
        with self.assertRaises(J.Pending):
            J.archive_coverage(self.file_row('B', daa=99), self.file_row('A'), params)
        with self.assertRaises(J.Pending):
            J.archive_coverage(self.file_row('B'), self.file_row('A', sink='t'), params)

    def test_same_daa_cannot_reuse_another_sink_or_local_import_epoch(self):
        for key, value in [('sink', 'other'), ('importEpoch', 8)]:
            row = self.file_row('B')
            row['value']['coverage']['generation'][key] = value
            with self.assertRaises(J.Pending):
                J.coverage_envelope(row, '22' * 32)
        row = self.file_row('A')
        row['node']['importSessionEpoch'] = 9
        row['value']['coverage']['generation']['importEpoch'] = 9
        J.archive_coverage(self.file_row('B'), row, '22' * 32)

    def test_legacy_missing_unstable_or_inexact_generation_is_not_evidence(self):
        mutations = [
            lambda e, n: e.update(schemaVersion=1, kind='rfc06-private-exec-origin-coverage-v1'),
            lambda e, n: e.pop('generation'),
            lambda e, n: e['generation'].update(importEpoch=True),
            lambda e, n: e['generation'].update(virtualDaaScore=100.0),
            lambda e, n: n.update(pruningImportStable=False),
            lambda e, n: n.update(importSessionPoint='active'),
            lambda e, n: e.update(census=None, error='import changed'),
            lambda e, n: e.update(error='contradictory'),
        ]
        for mutate in mutations:
            row = self.file_row('B')
            mutate(row['value']['coverage'], row['node'])
            with self.assertRaises((J.NotObservable, J.Pending, J.Violation)):
                J.coverage_envelope(row, '22' * 32)


class Aggregation(unittest.TestCase):
    def test_no_pass_label_and_missing_boxes_block(self):
        findings = [J.finding(1, 'x', J.OBSERVED)]
        result = J.aggregate(findings, (1, 2))
        self.assertEqual(result['checkboxes']['1']['status'], J.OBSERVED)
        self.assertEqual(result['checkboxes']['2']['status'], J.BLOCKED)
        self.assertEqual(result['status'], J.BLOCKED)
        self.assertFalse(result['passLabelUsed'])
        self.assertEqual(J.aggregate([J.finding(1, 'x', J.OBSERVED), J.finding(1, 'y', J.FAIL)], (1,))['status'], J.FAIL)
        self.assertEqual(J.aggregate([J.finding(1, 'x', J.OBSERVED)], (1,))['status'], J.OBSERVED)
        with self.assertRaises(ValueError):
            J.aggregate([J.finding(8, 'x', J.OBSERVED)], (1,))
        with self.assertRaises(ValueError):
            J.finding(1, 'x', 'PASS')
        self.assertNotIn('PASS', J.VERDICTS)


if __name__ == '__main__':
    unittest.main()


def recovery_row(states, reserve=500, block='ee' * 64, role='FRESH', source='node-rpc'):
    return dict(source=source, role=role, value=dict(pointStates=states, reserveBalance=reserve, blockHash=block))


LOCK, DEPOSIT, REFUND = 'aa' * 64 + ':0', 'bb' * 64 + ':1', 'cc' * 64 + ':0'
REFUND_ENTRY = dict(amount=900, scriptPublicKey='0000' + '51', isCoinbase=False, blockDaaScore=7)


class RecoveryOnce(unittest.TestCase):
    def states(self, **override):
        base = {LOCK: None, DEPOSIT: None, REFUND: dict(REFUND_ENTRY)}
        base.update(override)
        return base

    def test_identical_rights_across_the_crash_are_observed(self):
        pre, post = [recovery_row(self.states())], [recovery_row(self.states(), role='A')]
        # A re-accepted refund may move its acceptance DAA; the right itself is unchanged.
        post[0]['value']['pointStates'][REFUND]['blockDaaScore'] = 9
        facts = J.refund_recovered_once(pre, post, [LOCK, DEPOSIT], [REFUND])
        self.assertEqual(facts['reads'], 2)

    def test_missed_refund_double_count_and_reopened_lock_are_violations(self):
        pre = [recovery_row(self.states())]
        for after in (self.states(**{REFUND: None}), self.states(**{LOCK: dict(REFUND_ENTRY)}),
                      self.states(**{REFUND: dict(REFUND_ENTRY, amount=901)})):
            self.assertEqual(verdict(J.refund_recovered_once, pre, [recovery_row(after)], [LOCK, DEPOSIT], [REFUND]), J.FAIL)

    def test_missing_side_or_synthetic_rows_are_not_observed(self):
        pre = [recovery_row(self.states())]
        self.assertNotEqual(verdict(J.refund_recovered_once, pre, [], [LOCK], [REFUND]), J.OBSERVED)
        self.assertNotEqual(verdict(J.refund_recovered_once, pre, [recovery_row(self.states(), source='fixture')],
                                    [LOCK], [REFUND]), J.OBSERVED)
        omitted = self.states()
        del omitted[DEPOSIT]
        self.assertNotEqual(verdict(J.refund_recovered_once, pre, [recovery_row(omitted)], [LOCK, DEPOSIT], [REFUND]),
                            J.OBSERVED)


class ReserveAcrossRecovery(unittest.TestCase):
    def test_same_known_balance_at_the_anchor_is_observed(self):
        facts = J.reserve_stable_across_recovery([recovery_row({}, 500)], [recovery_row({}, 500, role='A')])
        self.assertEqual(facts['balanceSompi'], 500)

    def test_changed_balance_is_a_violation_and_unknown_is_not_observed(self):
        self.assertEqual(verdict(J.reserve_stable_across_recovery, [recovery_row({}, 500)], [recovery_row({}, 1000)]), J.FAIL)
        self.assertNotEqual(verdict(J.reserve_stable_across_recovery, [recovery_row({}, 500)], [recovery_row({}, None)]),
                            J.OBSERVED)
        self.assertNotEqual(verdict(J.reserve_stable_across_recovery, [recovery_row({}, 500)],
                                    [recovery_row({}, 500, block='ff' * 64)]), J.OBSERVED)
