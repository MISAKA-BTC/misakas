"""Read-helper controls with fake transports and owned temp files; never a node or P2P observation."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_acceptance_observe as O  # noqa: E402

SCAN = dict(maximumScannedChainBlocks=10, maximumMergesetBlockReads=10, maximumScannedEvmBlocks=10,
            maximumUtxoRowsPerAddress=10, kernelFinalsLimit=10)


def tx(txid, inputs=(), outputs=(5,)):
    return dict(verboseData=dict(transactionId=txid),
                inputs=[dict(previousOutpoint=dict(transactionId=t, index=i)) for t, i in inputs],
                outputs=[dict(value=v) for v in outputs])


class ChainRpc:
    def __init__(self, removed=()):
        self.removed, self.calls = list(removed), []

    def call(self, method, params=None):
        self.calls.append((method, params))
        if method == 'getVirtualChainFromBlock':
            if params['startHash'] == 'anchor':
                return dict(removedChainBlockHashes=self.removed, addedChainBlockHashes=['c1', 'c2'],
                            acceptedTransactionIds=[dict(acceptingBlockHash='c1', acceptedTransactionIds=['t1']),
                                                    dict(acceptingBlockHash='c2', acceptedTransactionIds=['t2', 't3'])])
            return dict(removedChainBlockHashes=[], addedChainBlockHashes=[], acceptedTransactionIds=[])
        if method == 'getBlock' and not params['includeTransactions']:
            merged = {'c1': ['m1'], 'c2': ['m2', 'm3']}[params['hash']]
            return dict(block=dict(verboseData=dict(mergeSetBluesHashes=merged, mergeSetRedsHashes=[])))
        if method == 'getBlock':
            bodies = {'m1': [tx('t1')], 'm2': [tx('t2', [('t1', 0)])], 'm3': [tx('t3')]}
            return dict(block=dict(transactions=bodies[params['hash']]))
        raise AssertionError(method)


class AcceptedTransactions(unittest.TestCase):
    def test_frozen_endpoint_excludes_later_accepted_fees_in_the_same_page(self):
        rpc = ChainRpc()
        found = O.accepted_transactions(rpc, 'anchor', None, SCAN, end='c1')
        self.assertEqual(list(found), ['t1'])
        self.assertFalse(any(method == 'getBlock' and params['hash'] in ('c2', 'm2', 'm3') for method, params in rpc.calls))
        self.assertEqual(O.accepted_transactions(ChainRpc(), 'anchor', None, SCAN, end='anchor'), {})
        with self.assertRaisesRegex(J.NotObservable, 'chain-block bound'):
            O.accepted_transactions(ChainRpc(), 'anchor', None, dict(SCAN, maximumScannedChainBlocks=1), end='c1')

    def test_frozen_endpoint_paginates_and_is_rechecked_after_body_reads(self):
        class Pages(ChainRpc):
            def call(self, method, params=None):
                if method == 'getVirtualChainFromBlock' and params['startHash'] == 'c1':
                    return dict(removedChainBlockHashes=[], addedChainBlockHashes=['c2'],
                                acceptedTransactionIds=[dict(acceptingBlockHash='c2', acceptedTransactionIds=['t2', 't3'])])
                value = super().call(method, params)
                if method == 'getVirtualChainFromBlock' and params['startHash'] == 'anchor':
                    value['addedChainBlockHashes'] = ['c1']
                    value['acceptedTransactionIds'] = value['acceptedTransactionIds'][:1]
                return value
        self.assertEqual(sorted(O.accepted_transactions(Pages(), 'anchor', None, SCAN, end='c2')), ['t1', 't2', 't3'])
        class Reorg(ChainRpc):
            def call(self, method, params=None):
                if method == 'getVirtualChainFromBlock' and params['startHash'] == 'c1':
                    return dict(removedChainBlockHashes=['c1'], addedChainBlockHashes=[])
                return super().call(method, params)
        with self.assertRaises(O.AnchorReorged):
            O.accepted_transactions(Reorg(), 'anchor', None, SCAN, end='c1')
        with self.assertRaises(J.NotObservable):
            O.accepted_transactions(ChainRpc(), 'anchor', None, SCAN, end='absent')

    def test_collect_all_cannot_silently_omit_an_accepted_transaction_body(self):
        class MissingBody(ChainRpc):
            def call(self, method, params=None):
                if method == 'getBlock' and params['includeTransactions'] and params['hash'] == 'm1':
                    return dict(block=dict(transactions=[]))
                return super().call(method, params)
        with self.assertRaisesRegex(J.NotObservable, 'bodies are incomplete'):
            O.accepted_transactions(MissingBody(), 'anchor', None, SCAN, end='c1')

    def test_wanted_ids_are_found_with_their_accepting_block(self):
        found = O.accepted_transactions(ChainRpc(), 'anchor', ['T2'], SCAN)
        self.assertEqual(list(found), ['t2'])
        self.assertEqual(found['t2']['acceptingBlock'], 'c2')
        self.assertEqual(found['t2']['containingBlock'], 'm2')

    def test_collect_all_walks_the_whole_window(self):
        found = O.accepted_transactions(ChainRpc(), 'anchor', None, SCAN)
        self.assertEqual(sorted(found), ['t1', 't2', 't3'])
        self.assertEqual(O.outputs_as_amounts(found)['t2:0'], 5)

    def test_reorged_anchor_and_bounds_are_blocked(self):
        with self.assertRaises(O.AnchorReorged):
            O.accepted_transactions(ChainRpc(removed=['x']), 'anchor', ['t1'], SCAN)
        with self.assertRaisesRegex(J.NotObservable, 'chain-block bound'):
            O.accepted_transactions(ChainRpc(), 'anchor', ['t3'], dict(SCAN, maximumScannedChainBlocks=1))
        with self.assertRaisesRegex(J.NotObservable, 'mergeset read bound'):
            O.accepted_transactions(ChainRpc(), 'anchor', None, dict(SCAN, maximumMergesetBlockReads=2))


class Bracket(unittest.TestCase):
    def drill(self, stamps):
        class Rpc:
            def __init__(self):
                self.daa = iter(stamps)

            def call(self, method, params=None):
                if method == 'getBlockDagInfo':
                    return dict(sink='ab' * 64, pruningPointHash='cd' * 64, virtualDaaScore=next(self.daa))
                if method == 'getPalwNodeStatus':
                    return dict(importSessionAvailable=True, importSessionEpoch=1, importSessionPoint='',
                                importSessionPhase='idle', pruningImportStable=True, genesisHash='ef' * 64,
                                consensusParamsId='12' * 32)
                raise AssertionError(method)

            def close(self):
                pass
        rpc = Rpc()
        return type('Drill', (), dict(rpc=lambda self, role: rpc, latencies=[]))()

    def test_a_moved_generation_is_retried_never_merged(self):
        row = O.bracketed(self.drill([5, 5]), 'A', 'unit', lambda rpc, dag, node: 'value')
        self.assertEqual((row['source'], row['value']), ('node-rpc', 'value'))
        with self.assertRaises(O.ContextMoved):
            O.bracketed(self.drill([5, 6]), 'A', 'unit', lambda rpc, dag, node: 'value')

    def test_missing_import_diagnostics_block(self):
        with self.assertRaises(J.NotObservable):
            O.context(dict(sink='a', pruningPointHash='b', virtualDaaScore=1), dict(importSessionAvailable=False))


class PaidClaimRead(unittest.TestCase):
    def test_an_absent_paid_claim_read_is_blocked_not_inferred(self):
        class Rpc:
            def call(self, method, params=None):
                raise RuntimeError('getPalwPaidClaim: unknown method')
        with self.assertRaises(J.NotObservable):
            O.paid_claim_value(Rpc(), 'aa' * 64, 0)

        class Busy:
            def call(self, method, params=None):
                raise RuntimeError('the server closed the connection')
        with self.assertRaises(RuntimeError):
            O.paid_claim_value(Busy(), 'aa' * 64, 0)
        spec = O.paid_claim_read_spec()
        self.assertEqual((spec['op'], spec['protoFields'], spec['readOnly']), (247, [1312, 1313], True))


class AcceptanceReads(unittest.TestCase):
    """Op 246: the reserve balance per block and the outpoint point read; a node without it is BLOCKED, never inferred."""

    def test_served_balances_and_outpoints_are_decoded_and_absent_ones_are_none(self):
        class Rpc:
            def call(self, method, params=None):
                assert method == O.ACCEPTANCE_READS_METHOD
                return dict(available=True, sink='AA' * 64, virtualDaa=9,
                            reserveBalances=[dict(blockHash='BB' * 64, known=True, balanceSompi=7),
                                             dict(blockHash='CC' * 64, known=False, balanceSompi=0)],
                            outpoints=[dict(outpoint='dd:0', present=True, amount=5, scriptVersion=0, scriptHex='AB',
                                            blockDaaScore=3, isCoinbase=False),
                                       dict(outpoint='ee:1', present=False, amount=0, scriptVersion=0, scriptHex='',
                                            blockDaaScore=0, isCoinbase=False)])
        got = O.acceptance_reads(Rpc(), ['bb' * 64, 'cc' * 64], ['dd:0', 'ee:1'])
        self.assertEqual(got['reserve'], {'bb' * 64: 7, 'cc' * 64: None})
        self.assertEqual(got['outpoints']['dd:0'], dict(amount=5, blockDaaScore=3, isCoinbase=False, scriptPublicKey='0000ab'))
        self.assertIsNone(got['outpoints']['ee:1'])

    def test_a_node_without_op_246_is_blocked(self):
        class Rpc:
            def call(self, method, params=None):
                raise RuntimeError('unknown method')
        with self.assertRaises(J.NotObservable):
            O.acceptance_reads(Rpc(), ['bb' * 64])
        with self.assertRaises(J.NotObservable):
            O.acceptance_reads(Rpc(), ['bb' * 64] * 65)


class OwnedFiles(unittest.TestCase):
    def test_outbox_store_coverage_and_log_reads(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            network = O.network_dir(root, 'A')
            record = network / O.KERNEL_OUTBOX / ('bb' * 32) / ('cc' * 64)
            record.mkdir(parents=True)
            (record / 'object.borsh').write_bytes(b'signed')
            records = O.outbox_records(root, 'A', 'kernel-outbox', ('object.borsh', 'transaction.borsh'))
            self.assertEqual(list(records), [('bb' * 32) + '/' + ('cc' * 64)])
            self.assertEqual(set(records[list(records)[0]]), {'object.borsh'})
            store = network / 'palw-native-provider-v3' / 'bond.store'
            store.parent.mkdir(parents=True)
            store.write_bytes(b'x' * 7)
            self.assertEqual(sum(O.provider_stores(root, 'A').values()), 7)
            self.assertIsNone(O.coverage_file(root, 'A'))
            (network / O.COVERAGE_FILE).write_text(json.dumps(dict(schemaVersion=1)))
            self.assertEqual(O.coverage_file(root, 'A')['schemaVersion'], 1)
            listing = O.db_listing(root, 'A')
            self.assertEqual(listing[O.NETWORK_DIR + '/' + O.COVERAGE_FILE], (network / O.COVERAGE_FILE).stat().st_size)
            self.assertEqual(listing[O.NETWORK_DIR + '/palw-native-provider-v3/bond.store'], 7)
            log = root / 'node.log'
            log.write_text('x\nprivate objective court action accepted: %s (not a computation approval)\n' % ('ab' * 64))
            rows = O.log_lines([log])
            self.assertEqual([(row['kind'], row['authoritative']) for row in rows], [('challenge-accepted', False)])


if __name__ == '__main__':
    unittest.main()
