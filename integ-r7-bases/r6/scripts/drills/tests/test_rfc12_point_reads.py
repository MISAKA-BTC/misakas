"""Reader/judgement adversarial controls; these are synthetic unit evidence only."""
import copy
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_observe as O
import rfc12_acceptance_judgement as J

A, B = 'aa' * 64 + ':0', 'bb' * 64 + ':0'
ENTRY = dict(amount=100, scriptPublicKey='000076c440' + 'ab' * 64 + '88a6', isCoinbase=False, blockDaaScore=10)


class Rpc:
    def __init__(self, value): self.value = value
    def call(self, method, params):
        assert method == O.ACCEPTANCE_READS_METHOD
        if isinstance(self.value, Exception): raise self.value
        v = self.value
        reserve = v['reserveBalance']
        rows = []
        for row in v['utxos']:
            entry = row['utxoEntry']
            point = row['outpoint']
            spk = entry['scriptPublicKey'] if entry else '0000'
            rows.append(dict(outpoint='%s:%s' % (point['transactionId'], point['index']), present=entry is not None,
                             amount=entry['amount'] if entry else 0, scriptVersion=int(spk[:4], 16), scriptHex=spk[4:],
                             blockDaaScore=entry['blockDaaScore'] if entry else 0, isCoinbase=entry['isCoinbase'] if entry else False))
        return dict(available=True, sink=v['virtualSelectedParent'], virtualDaa=v['virtualDaaScore'], outpoints=rows,
                    reserveBalances=[dict(blockHash=v['blockHash'], known=reserve is not None,
                                          balanceSompi=reserve if reserve is not None else 0)] if params['reserveBlocks'] else [])



def snapshot():
    return dict(version=1, blockHash='cc' * 64, reserveBalance=0, virtualSelectedParent='cc' * 64,
                virtualDaaScore=10, virtualParents=['cc' * 64],
                utxos=[dict(outpoint=dict(transactionId='aa' * 64, index=0), utxoEntry=None)])


def observed(states):
    return dict(source='node-rpc', role='A', value=dict(pointStates=states, withdrawalOutpoints=[B], refundOutpoints=[A]))


class PointReads(unittest.TestCase):
    def test_known_zero_and_known_absence_are_distinct_from_unavailable(self):
        value = O.rfc12_state_value(Rpc(snapshot()), [A], 'cc' * 64)
        self.assertEqual(value['reserveBalance'], 0)
        self.assertEqual(value['pointStates'], {A: None})
        unknown = snapshot(); unknown['reserveBalance'] = None
        self.assertIsNone(O.rfc12_state_value(Rpc(unknown), [A], 'cc' * 64)['reserveBalance'])
        with self.assertRaises(J.NotObservable): O.rfc12_state_value(Rpc(OSError('store failure')), [A])

    def test_missing_duplicate_unsolicited_wrong_block_and_inexact_values_are_refused(self):
        cases = []
        v = snapshot(); v['utxos'] = []; cases.append(v)
        v = snapshot(); v['utxos'] *= 2; cases.append(v)
        v = snapshot(); v['utxos'][0]['outpoint']['transactionId'] = 'dd' * 64; cases.append(v)
        v = snapshot(); v['reserveBalance'] = 2**64; cases.append(v)
        v = snapshot(); v['virtualDaaScore'] = True; cases.append(v)
        v = snapshot(); v['utxos'][0]['utxoEntry'] = dict(ENTRY, amount=1.5); cases.append(v)
        for value in cases:
            with self.subTest(value=value), self.assertRaises(J.NotObservable): O.rfc12_state_value(Rpc(value), [A], 'cc' * 64)
        with self.assertRaises(J.NotObservable): O.rfc12_state_value(Rpc(snapshot()), [A], 'dd' * 64)

    def test_consumption_presence_and_restart_loss_use_the_named_point(self):
        J.point_consumed([observed({A: None})], A)
        J.point_present([observed({A: ENTRY})], A, 99, 100)
        with self.assertRaises(J.NotObservable): J.point_consumed([observed({})], A)
        with self.assertRaises(J.Pending): J.point_present([observed({A: None})], A, 99, 100)
        with self.assertRaises(J.Pending): J.point_consumed([observed({A: ENTRY})], A)
        first = observed({A: ENTRY, B: None})
        later = copy.deepcopy(first); later['value']['pointStates'][A]['blockDaaScore'] += 1
        J.effects_stable([first, later])
        later['value']['pointStates'][A] = None
        with self.assertRaises(J.Violation): J.effects_stable([first, later])

    def test_refund_must_link_the_original_input_and_conserve_value(self):
        txid, _ = B.split(':')
        tx = dict(inputs=[dict(previousOutpoint=dict(transactionId='aa' * 64, index=0))], outputs=[dict(value=99)])
        row = dict(source='node-rpc', value={txid: dict(transaction=tx)})
        self.assertEqual(J.refund_link(row, A, B, 100, 2)['feeSompi'], 1)
        tx['inputs'][0]['previousOutpoint']['transactionId'] = 'dd' * 64
        with self.assertRaises(J.Violation): J.refund_link(row, A, B, 100, 2)
        tx['inputs'][0]['previousOutpoint']['transactionId'] = 'aa' * 64
        tx['outputs'][0]['value'] = 101
        with self.assertRaises(J.Violation): J.refund_link(row, A, B, 100, 2)


if __name__ == '__main__': unittest.main()
