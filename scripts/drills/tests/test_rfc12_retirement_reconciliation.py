"""RFC-0012 §4.4 classifier, bounded scan and checkbox-7 judgement controls; synthetic rows, never a node result."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_retirement_reconciliation as R  # noqa: E402

OWNER, VALIDATOR, REWARD = 'a1' * 64, 'b2' * 64, 'c3' * 64


def bond_payload(amount=5000, activation=7, period=30, reward=REWARD, key_len=R.VALIDATOR_PUBKEY_LEN):
    raw = (1).to_bytes(2, 'little') + bytes.fromhex(OWNER) + bytes.fromhex(VALIDATOR)
    raw += key_len.to_bytes(4, 'little') + b'\x07' * R.VALIDATOR_PUBKEY_LEN
    raw += amount.to_bytes(8, 'little') + activation.to_bytes(8, 'little') + period.to_bytes(8, 'little')
    return (raw + bytes.fromhex(reward)).hex()


def coinbase_payload(script_hex, subsidy=50, version=0):
    script = bytes.fromhex(script_hex)
    return ((9).to_bytes(8, 'little') + subsidy.to_bytes(8, 'little') + version.to_bytes(2, 'little')
            + bytes([len(script)]) + script + b'extra').hex()


REGISTRY = [dict(bondOutpoint='tb' * 64 + ':0', ownerPubkeyHash=OWNER, validatorId=VALIDATOR, amount=5000,
                 activationDaaScore=7, effectiveStatus='active')]
BOND = REGISTRY[0]['bondOutpoint']
DNS_SPK = R.p2pkh_spk(REWARD)
MINER_SPK = R.p2pkh_spk('d4' * 64)


def output(index, spk, value=10, address=None):
    return dict(index=index, value=value, spk=spk, address=address)


def block(daa, outputs, hash_=None):
    return dict(source='node-rpc', hash=hash_ or ('%02x' % (daa % 256)) * 64, daa=daa, coinbaseOutputs=outputs)


def verdict(function, *args):
    return J.judge(7, 'unit', function, *args)['verdict']


class Codecs(unittest.TestCase):
    def test_p2pkh_script_shape(self):
        self.assertEqual(DNS_SPK, '0000' + '76c440' + REWARD + '88a6')
        with self.assertRaises(ValueError):
            R.p2pkh_spk('aa' * 32)

    def test_stake_bond_payload_is_parsed_exactly(self):
        parsed = R.parse_stake_bond_payload(bond_payload())
        self.assertEqual((parsed['ownerPubkeyHash'], parsed['validatorPubkeyHash'], parsed['amount'],
                          parsed['activationDaaScore'], parsed['unbondingPeriodBlocks'], parsed['ownerRewardSpkPayload']),
                         (OWNER, VALIDATOR, 5000, 7, 30, REWARD))
        with self.assertRaisesRegex(ValueError, 'bytes'):
            R.parse_stake_bond_payload(bond_payload() + '00')
        with self.assertRaisesRegex(ValueError, '2592'):
            R.parse_stake_bond_payload(bond_payload(key_len=2591))

    def test_coinbase_payload_and_subnetwork(self):
        script = MINER_SPK[4:]
        self.assertEqual(R.coinbase_miner_spk(coinbase_payload(script)), (MINER_SPK, 50))
        with self.assertRaises(ValueError):
            R.coinbase_miner_spk(coinbase_payload(script)[:60])
        self.assertEqual(R.subnetwork_kind('10' + '00' * 19), 0x10)
        self.assertEqual(R.subnetwork_kind('00' * 20), 0)
        self.assertIsNone(R.subnetwork_kind('10' + '00' * 18 + '01'))


class Classification(unittest.TestCase):
    def test_bond_scripts_come_from_the_bonds_own_transaction_and_match_the_registry(self):
        scripts, unread = R.bond_scripts({'tb' * 64: bond_payload()}, REGISTRY)
        self.assertEqual((scripts, unread), ({DNS_SPK: BOND}, []))
        self.assertEqual(R.bond_scripts({}, REGISTRY), ({}, [BOND]))
        with self.assertRaises(J.NotObservable):
            R.bond_scripts({'tb' * 64: bond_payload(amount=4999)}, REGISTRY)
        with self.assertRaises(J.NotObservable):
            R.bond_scripts({'tb' * 64: 'zz'}, REGISTRY)

    def test_each_output_class(self):
        dns, miners, palw = {DNS_SPK: BOND}, {MINER_SPK}, {'misakatest:palw'}
        self.assertEqual(R.classify_output(output(0, DNS_SPK), dns, miners, palw), (R.DNS, BOND))
        self.assertEqual(R.classify_output(output(0, MINER_SPK), dns, miners, palw), (R.MINER, None))
        self.assertEqual(R.classify_output(output(0, 'x', address='misakatest:palw'), dns, miners, palw), (R.PALW, None))
        self.assertEqual(R.classify_output(output(0, MINER_SPK, address='misakatest:palw'), dns, miners, palw)[0], R.NON_DNS)
        self.assertEqual(R.classify_output(output(0, DNS_SPK, address='misakatest:palw'), dns, miners, palw)[0], R.AMBIGUOUS)
        self.assertEqual(R.classify_output(output(0, 'unknown'), dns, miners, palw)[0], R.UNCLASSIFIED)

    def test_unclassified_or_ambiguous_outputs_block_never_infer(self):
        dns, miners = {DNS_SPK: BOND}, {MINER_SPK}
        blocks = R.classify_blocks([block(5, [output(0, MINER_SPK), output(1, DNS_SPK)])], dns, miners, set())
        self.assertEqual(verdict(R.classification_complete, blocks), J.OBSERVED)
        blocks = R.classify_blocks([block(5, [output(0, 'unknown')])], dns, miners, set())
        self.assertEqual(verdict(R.classification_complete, blocks), J.BLOCKED)
        self.assertEqual(verdict(R.classification_complete, []), J.BLOCKED)


class Judgement(unittest.TestCase):
    def classified(self, rows):
        return R.classify_blocks(rows, {DNS_SPK: BOND}, {MINER_SPK}, set())

    def test_pre_fence_share_must_exist_and_pay_an_active_bond(self):
        blocks = self.classified([block(10, [output(0, MINER_SPK), output(1, DNS_SPK, 3)]), block(250, [output(0, MINER_SPK)])])
        facts = R.pre_fence_dns_share(blocks, 200, {(BOND, 10): 'active'})
        self.assertEqual(facts['perBondSompi'], {BOND: 3})
        self.assertEqual(verdict(R.pre_fence_dns_share, blocks, 200, {(BOND, 10): 'unbonding'}), J.FAIL)
        self.assertEqual(verdict(R.pre_fence_dns_share, blocks, 200, {}), J.BLOCKED)
        no_history = self.classified([block(10, [output(0, MINER_SPK)])])
        self.assertEqual(verdict(R.pre_fence_dns_share, no_history, 200, {}), J.BLOCKED)

    def test_post_fence_no_new_share_and_no_retired_kind(self):
        rows = [block(150, [output(0, DNS_SPK)]), block(205, [output(0, MINER_SPK), output(1, DNS_SPK)])]
        blocks = self.classified(rows)
        bodies = [dict(source='node-rpc', hash='aa' * 64, daa=150, kinds=[0x11]), dict(source='node-rpc', hash='bb' * 64, daa=205, kinds=[])]
        facts = R.post_fence_dns_share(blocks, bodies, 200, 10, {BOND})
        self.assertEqual((facts['legacySettlementOutputs'], facts['lastLegacySettlementDaa']), (1, 205))
        self.assertEqual(verdict(R.post_fence_dns_share, blocks, bodies, 200, 0, {BOND}), J.FAIL, 'a zero window is strict')
        self.assertEqual(verdict(R.post_fence_dns_share, blocks, bodies, 200, 10, set()), J.FAIL, 'only entitled bonds')
        late = bodies + [dict(source='node-rpc', hash='cc' * 64, daa=201, kinds=[0x19])]
        self.assertEqual(verdict(R.post_fence_dns_share, blocks, late, 200, 10, {BOND}), J.FAIL)
        self.assertEqual(verdict(R.post_fence_dns_share, self.classified([block(150, [])]), bodies, 200, 10, {BOND}),
                         J.BLOCKED)

    def test_legacy_exit_and_reserve(self):
        self.assertEqual(verdict(R.legacy_exit_paid_once, dict(commands=False)), J.BLOCKED)
        self.assertEqual(verdict(R.legacy_exit_paid_once, dict(commands=True, released=False)), J.BLOCKED)
        entry = lambda tx, amount: dict(address='a', outpoint=dict(transactionId=tx, index=0),
                                        utxoEntry=dict(amount=amount, blockDaaScore=1, isCoinbase=False))
        record = dict(commands=True, released=True, amount=5000, maximumFeeSompi=10, destinationBefore=[entry('aa', 1)],
                      destinationAfter=dict(source='node-rpc', value=dict(entries=[entry('aa', 1), entry('bb', 4995)])))
        self.assertEqual(verdict(R.legacy_exit_paid_once, record), J.OBSERVED)
        record['destinationAfter']['value']['entries'].append(entry('cc', 4995))
        self.assertEqual(verdict(R.legacy_exit_paid_once, record), J.FAIL)
        self.assertEqual(verdict(R.reserve_balance, None), J.BLOCKED)


class Scan(unittest.TestCase):
    def rpc(self, missing_body=None):
        def tx(subnetwork, payload='', txid=None):
            return dict(subnetworkId=subnetwork, payload=payload, verboseData=dict(transactionId=txid or 'f0' * 64))
        coinbase = lambda spk_out: dict(subnetworkId='01' + '00' * 19, payload=coinbase_payload(MINER_SPK[4:]),
                                        outputs=[dict(value=10, scriptPublicKey=spk_out, verboseData=dict(scriptPublicKeyAddress='a'))])
        blocks = {
            'c1': dict(header=dict(daaScore=100), verboseData=dict(mergeSetBluesHashes=[], mergeSetRedsHashes=[]),
                       transactions=[coinbase(DNS_SPK), tx('10' + '00' * 19, bond_payload(), 'tb' * 64)]),
            'c2': dict(header=dict(daaScore=200), verboseData=dict(mergeSetBluesHashes=['m1'], mergeSetRedsHashes=[]),
                       transactions=[coinbase(MINER_SPK)]),
            'm1': dict(header=dict(daaScore=199), verboseData={}, transactions=[coinbase(MINER_SPK), tx('11' + '00' * 19)]),
            'c3': dict(header=dict(daaScore=300), verboseData=dict(mergeSetBluesHashes=[], mergeSetRedsHashes=[]),
                       transactions=[coinbase(MINER_SPK)]),
        }
        if missing_body:
            blocks[missing_body]['transactions'] = []

        class Rpc:
            def call(self, method, params=None):
                if method == 'getVirtualChainFromBlock':
                    added = ['c1', 'c2', 'c3'] if params['startHash'] == 'genesis' else []
                    return dict(removedChainBlockHashes=[], addedChainBlockHashes=added)
                if method == 'getBlock':
                    return dict(block=blocks[params['hash']])
                raise AssertionError(method)
        return Rpc()

    def test_window_scan_attributes_kinds_to_each_body_and_collects_bonds(self):
        scan = R.scan_window(self.rpc(), 'genesis', 150, 250, 10, R.ReadBudget(10))
        self.assertEqual([row['daa'] for row in scan['blocks']], [200])
        self.assertEqual({body['daa']: body['kinds'] for body in scan['bodies']}, {200: [], 199: [0x11]})
        scan = R.scan_window(self.rpc(), 'genesis', 0, 1000, 10, R.ReadBudget(10))
        self.assertIn('tb' * 64, scan['bondTxs'])
        self.assertIn(MINER_SPK, scan['minerScripts'])
        blocks = R.classify_blocks(scan['blocks'], {DNS_SPK: BOND}, set(scan['minerScripts']), set())
        self.assertEqual(blocks[0]['outputs'][0]['kind'], R.DNS)
        # A pre-fence attestation merged by a post-fence chain block is history, judged at its own DAA 199.
        self.assertEqual(verdict(R.post_fence_dns_share, blocks, scan['bodies'], 200, 0, {BOND}), J.OBSERVED)
        self.assertEqual(verdict(R.post_fence_dns_share, blocks, scan['bodies'], 199, 0, {BOND}), J.FAIL)

    def test_reads_and_pruned_bodies_are_bounded_and_blocked(self):
        with self.assertRaisesRegex(J.NotObservable, 'read bound'):
            R.scan_window(self.rpc(), 'genesis', 0, 1000, 10, R.ReadBudget(2))
        with self.assertRaisesRegex(J.NotObservable, 'chain-block bound'):
            R.scan_window(self.rpc(), 'genesis', 0, 1000, 2, R.ReadBudget(10))
        with self.assertRaisesRegex(J.NotObservable, 'no body'):
            R.scan_window(self.rpc(missing_body='c2'), 'genesis', 0, 1000, 10, R.ReadBudget(10))


if __name__ == '__main__':
    unittest.main()
