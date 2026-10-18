"""Unexecuted source controls. Synthetic files are not native acceptance evidence."""
from copy import deepcopy
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc08_native_paid_observation as paid


def h(number):
    return ('%02x' % number) * 64


def stamp(node):
    return node['epoch'], node['point'], node['phase'], node['stable']


def context(dag, node):
    return dag['sink'], dag['virtualDaaScore'], dag['pruningPointHash'], node['consensusParamsId'], node['genesisHash'], stamp(node)


def fixture():
    dag = dict(sink=h(1), virtualDaaScore=100, pruningPointHash=h(2))
    node = dict(consensusParamsId=h(3), genesisHash=h(4), epoch=5, point='', phase='idle', stable=True)
    observation = dict(version=5, baseClaim=h(6), authorityPresent=True, enrollment=h(7), context=h(8), checkerSnapshot=h(9),
        snapshot=dict(checkpoint=h(1), statePoint=h(1), stateRoot=h(10), daa=100, importEpoch=5),
        expectedRounds=1, checkedRounds=1, sourceContextsObserved=1, sourcePromisesObserved=1,
        nativeConsumerComplete=True, nativeAssignedQuorum='OBSERVED', finalProjection='OBSERVED',
        nativeSelectedApproval='UNKNOWN', sourceProvenanceRevalidated='UNKNOWN', historicalSignatureReplay='UNKNOWN',
        coinbaseOutputReplayed='UNKNOWN', oneTimeFeeSettlement='UNKNOWN', observationLimitReached=False,
        escrowHeldSompi=0, escrowPaidSompi=2**100, escrowRefundedSompi=0, latestMatchingNativeDrain=None,
        lastLock=dict(round=0, context=h(8), anchor=h(11), sampleSeed=h(12), assignment=h(13), lockedDaa=90,
            seats=5, quorum=3, distinctCompleteReceipts=3, sourceValidNativeFlag=True, assignmentRecomputed=True),
        releaseApproval=False, securitySoundnessMeasured=False)
    envelope = dict(schemaVersion=5, kind='rfc08-private-native-paid-claim-observation-v5', baseClaim=h(6),
        consensusParamsId=h(3), genesis=h(4), available=True, observation=observation, error=None,
        ownProcessObservationOnly=True, releaseApproval=False, securitySoundnessMeasured=False)
    return envelope, dag, node


class PaidObservationCounterexamples(unittest.TestCase):
    def validate(self, envelope, dag=None, node=None):
        _, default_dag, default_node = fixture()
        return paid.validate(envelope, h(6), dag or default_dag, node or default_node, stamp, h(3), h(4))

    def test_exact_claim_profile_and_bounded_flag_are_required(self):
        profile = ['--private-probe-profile=paid-v4']
        self.assertEqual(paid.claims(profile + [paid.FLAG + h(6)]), (h(6),))
        source_profile = ['--private-probe-profile=source-paid-v1', paid.SOURCE_POLICY_FLAG + '/frozen/policy.borsh']
        self.assertEqual(paid.claims(source_profile + [paid.FLAG + h(6)]), (h(6),))
        for argv in ([paid.FLAG + h(6)], profile + [paid.FLAG + h(6) + ',' + h(6)],
                profile + [paid.FLAG + '0' * 128], profile + [paid.FLAG + ','.join(h(6) for _ in range(65))],
                profile + [paid.FLAG + h(6), paid.FLAG + h(7)],
                ['--private-probe-profile=source-paid-v1', paid.FLAG + h(6)]):
            with self.assertRaises(paid.Unavailable):
                paid.claims(argv)

    def test_stale_root_point_clock_import_params_and_numeric_coercion_refuse(self):
        good, dag, node = fixture()
        self.assertIsNotNone(self.validate(good))
        for section, key, value in (
                ('snapshot', 'statePoint', h(20)), ('snapshot', 'checkpoint', h(21)),
                ('snapshot', 'stateRoot', '0' * 128), ('snapshot', 'daa', 101),
                ('snapshot', 'daa', 100.0), ('snapshot', 'importEpoch', True),
                ('observation', 'escrowPaidSompi', float(2**100)), ('observation', 'checkedRounds', True)):
            bad = deepcopy(good)
            (bad['observation']['snapshot'] if section == 'snapshot' else bad['observation'])[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(paid.Unavailable):
                self.validate(bad)
        for key, value in (('baseClaim', h(20)), ('consensusParamsId', h(21)), ('genesis', h(22)), ('releaseApproval', True)):
            bad = deepcopy(good)
            bad[key] = value
            with self.assertRaises(paid.Unavailable):
                self.validate(bad)
        for key, value in (('epoch', 6), ('point', h(20)), ('phase', 'applying'), ('stable', False)):
            with self.assertRaises(paid.Unavailable):
                self.validate(good, node=dict(node, **{key: value}))

    def test_quorum_final_escrow_latest_drain_never_satisfy_full_approval_or_payments(self):
        envelope, _, _ = fixture()
        # Even a future/fabricated OBSERVED spelling cannot extend the reviewed
        # v5 contract. This is a pure reducer control, not an exporter fixture.
        value = self.validate(envelope)
        for key in paid.UNRESOLVED:
            value[key] = 'OBSERVED'
        observed = dict(available=True, observation=value)
        for kind in ('selected-verifier-approval', 'native-fee-settlement', 'all-rewards-once'):
            result = paid.fact(dict(id=kind, kind=kind, role='A', claimId=h(6), expected={}), observed)
            self.assertEqual(result['status'], 'UNKNOWN')
        for kind in ('native-paid-observed', 'native-paid-assigned-quorum'):
            self.assertEqual(paid.fact(dict(id=kind, kind=kind, role='A', claimId=h(6), expected={}), observed)['status'], 'PASS')
        value['observationLimitReached'] = True
        result = paid.fact(dict(id='quorum', kind='native-paid-assigned-quorum', role='A', claimId=h(6), expected={}), observed)
        self.assertEqual(result['status'], 'UNKNOWN')

    def test_restart_invalidates_exact_files_without_touching_other_claim_or_wal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            reader = SimpleNamespace(values={'A_APPDIR': str(root / 'appdir')}, processes={},
                plan=dict(nodes=dict(A=dict(argv=['--private-probe-profile=paid-v4', paid.FLAG + h(6)]))))
            directory = paid.directory(reader, 'A')
            directory.mkdir(parents=True)
            for name in (h(6) + '.json', h(6) + '.pending', h(7) + '.json', 'object.borsh'):
                (directory / name).write_text('preserve unrelated files')
            reader.processes['A'] = object()
            with self.assertRaises(paid.Unavailable):
                paid.invalidate(reader, 'A')
            reader.processes.clear()
            paid.invalidate(reader, 'A')
            self.assertFalse((directory / (h(6) + '.json')).exists())
            self.assertFalse((directory / (h(6) + '.pending')).exists())
            self.assertTrue((directory / (h(7) + '.json')).exists())
            self.assertTrue((directory / 'object.borsh').exists())

    def test_reader_refuses_symlink_oversize_and_duplicate_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve()
            source = directory / 'source'
            source.write_text('{"available":true,"available":false}')
            with self.assertRaises(paid.Unavailable):
                paid.read_owned(source)
            source.write_bytes(b'x' * (paid.MAX_BYTES + 1))
            with self.assertRaises(paid.Unavailable):
                paid.read_owned(source)
            link = directory / 'link'
            link.symlink_to(source)
            with self.assertRaises(paid.Unavailable):
                paid.read_owned(link)

    def test_collector_brackets_current_owned_process_and_state_root(self):
        class Process:
            pid = 123
            def poll(self):
                return None
        for change in ('none', 'root', 'daa', 'epoch', 'process'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                envelope, dag, node = fixture()
                root = Path(temporary).resolve()
                process = Process()
                reader = SimpleNamespace(values={'A_APPDIR': str(root / 'appdir')}, processes={'A': process},
                    paid_observation_processes={'A': process},
                    plan=dict(nodes=dict(A=dict(argv=['--private-probe-profile=paid-v4', paid.FLAG + h(6)]))))
                directory = paid.directory(reader, 'A')
                directory.mkdir(parents=True)
                path = directory / (h(6) + '.json')
                path.write_text(json.dumps(envelope))
                class Rpc:
                    reads = 0
                    def call(self, method):
                        if method == 'getBlockDagInfo':
                            self.reads += 1
                            return dict(dag, virtualDaaScore=101) if self.reads == 2 and change == 'daa' else dict(dag)
                        if self.reads == 2:
                            if change == 'root':
                                changed = deepcopy(envelope)
                                changed['observation']['snapshot']['stateRoot'] = h(30)
                                path.write_text(json.dumps(changed))
                            if change == 'process':
                                reader.processes['A'] = Process()
                            if change == 'epoch':
                                return dict(node, epoch=6)
                        return dict(node)
                    def close(self):
                        pass
                reader.rpc = lambda role: Rpc()
                if change == 'none':
                    result = paid.observe(reader, 'A', h(6), stamp, context, h(3), h(4))
                    self.assertTrue(result['available'])
                    self.assertEqual(result['pid'], 123)
                    self.assertFalse(result['independentlyVerifiedCommittedRoot'])
                    self.assertEqual(result['oneTimeFeeSettlement'], 'UNKNOWN')
                else:
                    with self.assertRaises(paid.Unavailable):
                        paid.observe(reader, 'A', h(6), stamp, context, h(3), h(4))


if __name__ == '__main__':
    unittest.main()
