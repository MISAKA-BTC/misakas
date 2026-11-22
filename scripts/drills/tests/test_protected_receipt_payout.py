import json
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_private_registration_run import (
    ContextMoved, follow_receipt_settlement, observe_admitted_studio, receipt_checkpoint, receipt_payout,
    receipt_settlement_view, resume_receipt_settlement,
)
from protected_claim_observation import receipt_from_producer_log


class ReceiptPayoutTest(unittest.TestCase):
    def setUp(self):
        self.receipt = '01' * 64
        child = '02' * 64
        self.txid = '03' * 64
        self.script = '0000abcd'
        self.entry = dict(isCoinbase=True, amount=5, scriptPublicKey=self.script, blockDaaScore=19)
        payload = bytes(8) + (10).to_bytes(8, 'little') + bytes.fromhex('000002abcd')
        self.block = dict(header=dict(powAlgoId=7, daaScore=17), verboseData=dict(isChainBlock=True),
                          transactions=[dict(payload=payload.hex())])
        self.child = dict(header=dict(hash=child, daaScore=18), verboseData=dict(isChainBlock=True,
                          selectedParentHash=self.receipt, mergeSetBluesHashes=[self.receipt], mergeSetRedsHashes=[]),
                          transactions=[dict(verboseData=dict(transactionId=self.txid),
                                             outputs=[dict(value=5, scriptPublicKey=self.script)])])
        self.page = dict(addedChainBlockHashes=[child])

        accepting = '05' * 64
        self.accepting = dict(header=dict(hash=accepting, daaScore=19),
                              verboseData=dict(isChainBlock=True, selectedParentHash=child))
        self.accepted = dict(addedChainBlockHashes=[accepting], acceptedTransactionIds=[
            dict(acceptingBlockHash=accepting, acceptedTransactionIds=[self.txid])])

    def read(self, previously_seen=None):
        with patch('protected_private_registration_run.block_read', side_effect=[self.block, self.child, self.accepting]), \
             patch('protected_private_registration_run.chain_read', side_effect=[self.page, self.accepted]), \
             patch('protected_private_registration_run.rfc12_state_value',
                   return_value=dict(pointStates={self.txid + ':0': self.entry})):
            return receipt_payout(None, self.receipt, previously_seen=previously_seen)

    def test_first_blue_reward_and_utxo(self):
        self.assertEqual(self.read()['outpoint'], self.txid + ':0')
        self.entry['amount'] = 6
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.read()

    def test_detached_receipt_waits_even_with_a_previous_payout(self):
        previous = self.read()
        self.block['verboseData']['isChainBlock'] = False
        self.assertIsNone(self.read(previous))
        self.block['verboseData']['isChainBlock'] = True
        self.assertEqual(self.read(previous), previous)

    def test_wait_for_accepted_utxo(self):
        self.entry = None
        self.assertIsNone(self.read())

    def test_spent_output_reuses_observation_only_under_same_acceptance(self):
        previous = self.read()
        self.entry = None
        self.assertEqual(self.read(previous), previous)
        self.assertIsNone(self.read(dict(previous, acceptingBlock='06' * 64)))
        self.assertIsNone(self.read(dict(previous, outpoint=self.txid + ':1')))

    def test_require_acceptance_of_same_coinbase_at_utxo_daa(self):
        self.entry['blockDaaScore'] = 18
        with self.assertRaisesRegex(ValueError, 'accepting DAA'):
            self.read()
        self.accepted['acceptedTransactionIds'][0]['acceptedTransactionIds'] = []
        with self.assertRaisesRegex(ValueError, 'uniquely accept'):
            self.read()
        self.accepted['addedChainBlockHashes'] = []
        self.assertIsNone(self.read())

    def test_reject_another_worker_or_ambiguous_merge(self):
        self.child['transactions'][0]['outputs'][0]['scriptPublicKey'] = '0000eeee'
        with self.assertRaisesRegex(ValueError, 'did not pay'):
            self.read()
        self.child['verboseData']['mergeSetRedsHashes'].append('04' * 64)
        with self.assertRaisesRegex(ValueError, 'ambiguous'):
            self.read()


class StudioReceiptLifecycleTest(unittest.TestCase):
    """Exercise the actual drill orchestration, with controlled RPC and native-audit responses."""
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.root = Path(self.stack.enter_context(tempfile.TemporaryDirectory()))
        self.data = self.root / 'studio'
        self.data.mkdir()
        self.receipt = '01' * 64
        self.claim = '02' * 64
        self.child = '03' * 64
        self.sink = '04' * 64
        self.config = dict(studio=dict(producerPeer=0, receiptValidator='/unused-in-test', timeoutSeconds=1),
                           peers=[dict(seat=i) for i in range(3)])
        self.expected = dict(claim=self.claim)
        self.original = dict(expected=self.expected, origin=dict(acceptedDaa=1))
        self.registration = dict(chain={})
        self.detail = dict(tipDaa=200, gateCGoverns=True, claim=dict(phase='final', quantaSpent=1,
                           securityCertification='certified'), receiptQuanta=dict(spentCount=1, spentIndices=[4],
                           spentIndicesComplete=True, total=21), protectedOriginal=dict(fullReplayApprovalRecorded=True,
                           publicRefutationRecorded=False))
        self.dag = dict(sink=self.sink, virtualDaaScore=200)
        self.payout = dict(child=self.child, childDaa=190, output=dict(blockDaaScore=191))
        self.settled = dict(sinkDaa=199, daaScore=191, available=True, settled=True, depth=1,
                            safeFrontierDaa=191, safeFrontierBlueScore=210)
        self.clients = [Mock() for _ in range(3)]
        for rpc in self.clients:
            rpc.call.return_value = self.settled
        stamp = (self.sink, None, 200)
        observed = dict(self.original, detail=self.detail, receiptBlock=dict(header=dict(hash=self.receipt)),
                        context=list(stamp), panelApprovalAcceptance=dict(carrierTransaction='05' * 64))
        def mock(name, **kw):
            return self.stack.enter_context(patch('protected_private_registration_run.' + name, **kw))
        self.identity = mock('chain_identity', return_value=(self.dag, stamp))
        mock('observe_claim_progress', return_value=self.detail)
        self.observe = mock('observe', return_value=observed)
        self.receipt_log = mock('receipt_from_producer_log', return_value=(self.receipt, 4))
        self.validator = mock('validate_receipt_observation', return_value=dict(claim=self.claim, block=self.receipt,
                                                                               quantumIndex=4, nativeReceiptEnvelopeValidated=True))
        mock('receipt_peer_view', return_value=observed)
        self.payment = mock('receipt_payout', return_value=self.payout)
        self.block_read = mock('block_read', return_value=dict(header=dict(daaScore=199), verboseData=dict(isChainBlock=True)))
        self.expectation = mock('expectation_from_studio', return_value=self.expected)

    def run_lifecycle(self):
        def until(stage, read, *, admitted):
            self.assertTrue(admitted)
            answer = read()
            if answer is None:
                raise TimeoutError(stage)
            return answer
        return observe_admitted_studio(self.config, self.registration, self.clients, until,
                                       [self.root], self.data, self.original, '06' * 64)

    def test_normal_entry_completes_only_after_all_peers_settle_the_utxo_acceptance_daa(self):
        result = self.run_lifecycle()
        self.assertEqual(result['expected'], self.expected)
        self.assertEqual(result['receiptAudit']['quantumIndex'], 4)
        self.assertEqual(result['peers'], 3)
        self.assertEqual(result['status'], 'receipt-settled')
        self.assertEqual(result['payout'], self.payout)
        self.assertEqual(result['settlement'], dict(sink=self.sink, settlement=self.settled))
        for rpc in self.clients:
            rpc.call.assert_called_once_with('getPalwSettlement', dict(daaScore=191))
        self.assertTrue((self.data / 'receipt-checkpoint.json').is_file())
        marker = self.data / 'receipt-checkpoint.json'
        audited = json.loads(marker.read_text())
        marker.rename(self.root / 'result.json')  # historical checkpoint layout
        self.assertEqual(receipt_checkpoint(self.root), audited)

    def test_unsettled_peer_leaves_resumable_receipt_and_rechecks_the_same_spend(self):
        self.clients[-1].call.return_value = dict(self.settled, settled=False)
        with self.assertRaisesRegex(TimeoutError, 'native settlement'):
            self.run_lifecycle()
        self.assertTrue((self.data / 'receipt-checkpoint.json').is_file())
        self.assertEqual(json.loads((self.data / 'receipt-payout.json').read_text()), [self.payout] * 3)
        self.assertFalse((self.data / 'receipt-settlement.json').exists())
        for name, value in [('original.json', self.original), ('studio-run.json', {}),
                            ('studio-evidence.json', dict(studioRun={}, studioJob={}, rail={}))]:
            (self.data / name).write_text(json.dumps(value))
        self.clients[-1].call.return_value = self.settled
        out = self.root / 'resumed'
        out.mkdir()
        result = resume_receipt_settlement(self.config, out, self.root, self.registration,
                                           self.clients, lambda: self.fail('snapshot is already settled'))
        self.assertEqual(result['receiptAudit']['block'], self.receipt)
        self.assertEqual(result['peers'], 3)
        self.assertEqual(json.loads((out / 'studio/receipt-payout.json').read_text()), [self.payout] * 3)
        self.assertEqual(self.payment.call_args.kwargs['previously_seen'], self.payout)
        self.assertTrue((out / 'studio/restarted-claim.json').is_file())
        self.assertTrue((out / 'studio/receipt-checkpoint.json').is_file())

    def test_detached_receipt_waits_and_replaced_payout_survives_restart(self):
        self.clients[-1].call.return_value = dict(self.settled, settled=False)
        with self.assertRaises(TimeoutError):
            self.run_lifecycle()
        previous = receipt_checkpoint(self.root)
        checkpoint = self.data / 'receipt-payout.json'
        old = json.loads(checkpoint.read_text())
        new = dict(self.payout, child='09' * 64, acceptingBlock='0a' * 64)
        self.clients[-1].call.return_value = self.settled
        self.payment.return_value = new
        selected = self.block_read.return_value
        detached = dict(selected, verboseData=dict(isChainBlock=False))
        reads = []
        def until(stage, read, *, admitted):
            self.assertTrue(admitted)
            if not reads:
                self.block_read.return_value = detached
                self.assertIsNone(read())
                self.assertEqual(json.loads(checkpoint.read_text()), old)
                self.block_read.return_value = selected
            reads.append(stage)
            return read()
        result = follow_receipt_settlement(previous, self.data, self.clients, until, restarted=True, seen_payouts=old)
        self.assertEqual(result['status'], 'receipt-settled')
        self.assertEqual(result['payout'], new)
        self.assertEqual(json.loads(checkpoint.read_text()), [new] * 3)
        self.assertEqual(receipt_checkpoint(self.root), previous)

    def test_unconfirmed_spend_cannot_replace_the_payout_checkpoint(self):
        confirm = Mock(return_value=None)
        checkpoint = Mock()
        self.assertIsNone(receipt_settlement_view(self.clients, self.expected, self.receipt,
                                                   on_paid=checkpoint, confirm_spend=confirm))
        confirm.assert_called_once_with()
        checkpoint.assert_not_called()
        for rpc in self.clients:
            rpc.call.assert_not_called()

    def test_restart_reaudits_a_different_selected_receipt_for_the_same_quantum(self):
        self.clients[-1].call.return_value = dict(self.settled, settled=False)
        with self.assertRaises(TimeoutError):
            self.run_lifecycle()
        previous = receipt_checkpoint(self.root)
        for name, value in [('original.json', self.original), ('studio-run.json', {}),
                            ('studio-evidence.json', dict(studioRun={}, studioJob={}, rail={}))]:
            (self.data / name).write_text(json.dumps(value))
        replacement, unrelated = '09' * 64, '0a' * 64
        (self.root / 'clock').mkdir()
        log = self.root / 'clock' / 'peer-0.log'
        log.write_text(''.join(f'PFS3 submitted block={block} claim={self.claim} quantum={q} parent=x\n'
                               for block, q in [(self.receipt, 4), (unrelated, 1), (replacement, 4)]))
        self.receipt_log.side_effect = receipt_from_producer_log
        self.block_read.side_effect = lambda rpc, block: dict(header=dict(daaScore=199),
                                                              verboseData=dict(isChainBlock=block != self.receipt))
        self.observe.return_value = dict(self.observe.return_value, receiptBlock=dict(header=dict(hash=replacement)))
        self.validator.return_value = dict(self.validator.return_value, block=replacement)
        fresh_payout = dict(self.payout, block=replacement, child='0b' * 64)
        self.payment.return_value = fresh_payout
        self.clients[-1].call.return_value = self.settled

        rejected = self.root / 'resumed-refused'
        rejected.mkdir()
        with patch('protected_claim_observation.block_read', self.block_read):
            self.validator.side_effect = ValueError('native receipt validator refused the replacement')
            with self.assertRaisesRegex(ValueError, 'native receipt validator refused'):
                resume_receipt_settlement(self.config, rejected, self.root, self.registration,
                                          self.clients, lambda: self.fail('invalid replacement advanced the clock'))
        self.assertEqual(receipt_checkpoint(rejected), previous)
        self.assertFalse((rejected / 'studio/receipt-settlement.json').exists())
        self.validator.side_effect = None

        out = self.root / 'resumed-reorg'
        out.mkdir()
        with patch('protected_claim_observation.block_read', self.block_read):
            result = resume_receipt_settlement(self.config, out, self.root, self.registration,
                                               self.clients, lambda: self.fail('replacement is already settled'))
        self.assertEqual(result['receiptAudit']['block'], replacement)
        self.assertEqual(result['receiptAudit']['quantumIndex'], 4)
        self.assertEqual(result['payout'], fresh_payout)
        self.assertEqual(receipt_checkpoint(out)['receiptAudit'], result['receiptAudit'])
        self.assertEqual(receipt_checkpoint(self.root), previous)
        self.assertTrue((out / 'studio' / previous['receiptObservation']).is_file())
        self.assertTrue((out / 'studio' / result['receiptObservation']).is_file())
        self.assertEqual(self.validator.call_args.args[4:], (replacement, Path('/unused-in-test'), 4))
        self.assertIsNone(self.payment.call_args.kwargs['previously_seen'])

    def test_different_acceptance_daa_or_last_peer_reorg_cannot_report_settlement(self):
        self.clients[1].call.return_value = dict(self.settled, daaScore=190)
        with self.assertRaisesRegex(ValueError, 'accepting DAA'):
            receipt_settlement_view(self.clients, self.expected, self.receipt)
        self.clients[1].call.return_value = self.settled
        initial = self.identity.return_value
        self.identity.side_effect = [initial]*8 + [(self.dag, ('new generation',))]
        with self.assertRaises(ContextMoved):
            receipt_settlement_view(self.clients, self.expected, self.receipt)

    def test_persistent_payout_disagreement_at_one_sink_fails_before_checkpoint(self):
        checkpoint = Mock()
        other = dict(self.payout, output=dict(self.payout['output'], amount=1))
        self.payment.side_effect = [self.payout, self.payout, other]
        with self.assertRaisesRegex(ValueError, 'worker payout differs at sink .*peer 2'):
            receipt_settlement_view(self.clients, self.expected, self.receipt, on_paid=checkpoint)
        checkpoint.assert_not_called()
        for rpc in self.clients:
            rpc.call.assert_not_called()

    def test_moving_peer_during_payout_comparison_is_retried(self):
        initial = self.identity.return_value
        self.identity.side_effect = [initial] * 6 + [(self.dag, ('new generation',))]
        self.payment.side_effect = [self.payout, self.payout, dict(self.payout, child=self.sink)]
        with self.assertRaises(ContextMoved):
            receipt_settlement_view(self.clients, self.expected, self.receipt)

    def test_settled_peers_must_agree_on_the_anchor_under_the_active_rules(self):
        for changed in (dict(safeFrontierDaa=192), dict(safeFrontierBlueScore=211), dict(depth=2)):
            with self.subTest(changed=changed):
                self.clients[-1].call.return_value = dict(self.settled, **changed)
                with self.assertRaisesRegex(ValueError, 'disagree on the settlement anchor'):
                    receipt_settlement_view(self.clients, self.expected, self.receipt)
        self.clients[-1].call.return_value = dict(self.settled, safeFrontierDaa=190)
        with self.assertRaisesRegex(ValueError, 'precedes the worker UTXO'):
            receipt_settlement_view(self.clients, self.expected, self.receipt)

        native = dict(self.settled, dnsRetiredAt=0, safeFrontierDaa=0, nativeSettlement=dict(safe=self.child))
        for rpc in self.clients:
            rpc.call.return_value = native
        self.assertEqual(len(receipt_settlement_view(self.clients, self.expected, self.receipt)), 3)
        self.clients[-1].call.return_value = dict(native, nativeSettlement=dict(safe=self.sink))
        with self.assertRaisesRegex(ValueError, 'disagree on the settlement anchor'):
            receipt_settlement_view(self.clients, self.expected, self.receipt)

    def test_expired_unspent_is_a_distinct_outcome_without_a_payout(self):
        self.detail.update(tipDaa=10002)
        self.detail['claim']['quantaSpent'] = 0
        self.detail['receiptQuanta'].update(spentCount=0, spentIndices=[])
        result = self.run_lifecycle()
        self.assertEqual(result['status'], 'receipt-deadline-passed-unspent')
        self.assertEqual(len(result['unspentPeerViews']), 3)
        self.assertNotIn('receiptAudit', result)
        (self.root / 'result.json').write_text(json.dumps(result))
        with self.assertRaisesRegex(ValueError, 'completed receipt audit'):
            receipt_checkpoint(self.root)
        self.payment.assert_not_called()
        self.assertFalse((self.data / 'receipt-checkpoint.json').exists())
