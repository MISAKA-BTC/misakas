import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_private_registration_run import receipt_payout


class ReceiptPayoutTest(unittest.TestCase):
    def setUp(self):
        self.receipt = '01' * 64
        child = '02' * 64
        self.txid = '03' * 64
        self.script = '0000abcd'
        self.entry = dict(isCoinbase=True, amount=5, scriptPublicKey=self.script)
        payload = bytes(8) + (10).to_bytes(8, 'little') + bytes.fromhex('000002abcd')
        self.block = dict(header=dict(powAlgoId=7, daaScore=17), verboseData=dict(isChainBlock=True),
                          transactions=[dict(payload=payload.hex())])
        self.child = dict(header=dict(hash=child, daaScore=18), verboseData=dict(isChainBlock=True,
                          selectedParentHash=self.receipt, mergeSetBluesHashes=[self.receipt], mergeSetRedsHashes=[]),
                          transactions=[dict(verboseData=dict(transactionId=self.txid),
                                             outputs=[dict(value=5, scriptPublicKey=self.script)])])
        self.page = dict(addedChainBlockHashes=[child])

    def read(self):
        with patch('protected_private_registration_run.block_read', side_effect=[self.block, self.child]), \
             patch('protected_private_registration_run.chain_read', return_value=self.page), \
             patch('protected_private_registration_run.rfc12_state_value',
                   return_value=dict(pointStates={self.txid + ':0': self.entry})):
            return receipt_payout(None, self.receipt)

    def test_first_blue_reward_and_utxo(self):
        self.assertEqual(self.read()['outpoint'], self.txid + ':0')
        self.entry['amount'] = 6
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.read()

    def test_wait_for_accepted_utxo(self):
        self.entry = None
        self.assertIsNone(self.read())

    def test_reject_another_worker_or_ambiguous_merge(self):
        self.child['transactions'][0]['outputs'][0]['scriptPublicKey'] = '0000eeee'
        with self.assertRaisesRegex(ValueError, 'did not pay'):
            self.read()
        self.child['verboseData']['mergeSetRedsHashes'].append('04' * 64)
        with self.assertRaisesRegex(ValueError, 'ambiguous'):
            self.read()
