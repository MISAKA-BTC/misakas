"""Adversarial RPC reader cases; constructed replies are not chain evidence."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_inventory_acceptance import ContextMoved, observe, observe_transaction
from test_protected_claim_observation import Rpc as ClaimRpc, h


class Rpc(ClaimRpc):
    def __init__(self):
        super().__init__()
        self.expected = dict(version=1, genesisHash=h(1), consensusParamsId=h(2)[64:],
                             transactionId=h(5), scanFromBlock=h(11))
        self.chain = dict(removedChainBlockHashes=[], addedChainBlockHashes=[h(9), h(10)],
                          acceptedTransactionIds=[self.entry])

    def call(self, method, request=None):
        if method == 'getVirtualChainFromBlock':
            self.calls.append(method)
            return copy.deepcopy(self.chain)
        return super().call(method, request)


class AcceptanceTests(unittest.TestCase):
    def test_semantics_carrier_clock_does_not_assert_a_native_binding(self):
        rpc = Rpc()
        result = observe_transaction(rpc, rpc.expected)
        self.assertEqual(result['kind'], 'protectedCarrierTransactionAcceptance')
        self.assertEqual(result['acceptedDaa'], 100)
        self.assertNotIn('acceptedOpenDaa', result)
        self.assertFalse(result['nativeEffectAuthenticated'])

    def test_reads_accepting_daa_without_asserting_native_effect(self):
        rpc = Rpc()
        observed = observe(rpc, rpc.expected)
        self.assertEqual(observed['acceptedOpenDaa'], 100)
        self.assertNotEqual(observed['acceptedOpenDaa'], rpc.blocks[h(8)]['header']['daaScore'])
        self.assertFalse(observed['nativeInventoryAccepted'])
        self.assertFalse(observed['nativeTokenizerRegistered'])
        self.assertFalse(any('submit' in method.lower() for method in rpc.calls))

    def test_body_presence_without_acceptance_does_not_supply_a_clock(self):
        rpc = Rpc()
        rpc.entry['acceptedTransactionIds'] = []
        with self.assertRaisesRegex(ValueError, 'no unique acceptance'):
            observe(rpc, rpc.expected)

    def test_reorg_or_mid_read_generation_change_is_rejected(self):
        rpc = Rpc()
        rpc.chain['removedChainBlockHashes'] = [h(11)]
        with self.assertRaises(ContextMoved):
            observe(rpc, rpc.expected)
        rpc = Rpc()
        def mutate(instance, method, request):
            if method == 'getBlock':
                instance.node['importSessionEpoch'] += 1
        rpc.mutate = mutate
        with self.assertRaises(ContextMoved):
            observe(rpc, rpc.expected)

    def test_duplicate_or_unselected_acceptance_is_rejected(self):
        cases = [
            lambda rpc: rpc.entry['acceptedTransactionIds'].append(h(5)),
            lambda rpc: rpc.chain['acceptedTransactionIds'].append(copy.deepcopy(rpc.entry)),
            lambda rpc: rpc.chain['acceptedTransactionIds'].append(dict(acceptingBlockHash=h(10), acceptedTransactionIds=[h(5)])),
            lambda rpc: rpc.entry.update(acceptingBlockHash=h(8)),
            lambda rpc: rpc.blocks[h(9)]['verboseData'].update(isChainBlock=False),
            lambda rpc: rpc.blocks[h(9)]['verboseData'].update(isHeaderOnly=True),
        ]
        for change in cases:
            with self.subTest(change=change):
                rpc = Rpc()
                change(rpc)
                with self.assertRaises(ValueError):
                    observe(rpc, rpc.expected)

    def test_incomplete_scan_wrong_profile_and_invalid_clock_are_rejected(self):
        for change in (
            lambda rpc: rpc.chain.update(addedChainBlockHashes=[h(9)]),
            lambda rpc: rpc.node.update(consensusParamsId=h(99)),
            lambda rpc: rpc.node.update(pruningImportStable=False),
            lambda rpc: rpc.blocks[h(9)]['header'].update(daaScore=True),
            lambda rpc: rpc.blocks[h(9)]['header'].update(daaScore=201),
        ):
            rpc = Rpc()
            change(rpc)
            with self.assertRaises(ValueError):
                observe(rpc, rpc.expected)
        rpc = Rpc()
        with self.assertRaisesRegex(ValueError, 'scan bound'):
            observe(rpc, rpc.expected, maximum_chain_blocks=1)


if __name__ == '__main__':
    unittest.main()
