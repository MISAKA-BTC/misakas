"""Counterexamples for owned native metadata reads, never live-node acceptance."""
from copy import deepcopy
import sys
from pathlib import Path
import unittest
import tempfile
import json
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_native_recovery_observation as recovery


def h(n):
    return ('%02x' % n) * 64


def bond(n):
    return dict(transactionId=h(n), index=n)


def stamp(node):
    return node['epoch'], '', 'idle', True


def fixture():
    node = dict(consensusParamsId=h(1), genesisHash=h(2), epoch=3)
    dag = dict(sink=h(4), virtualDaaScore=20, pruningPointHash=h(5))
    bootstrap = dict(consensusParamsId=h(1), genesis=h(2), artifactRoot=h(6),
        seats=[dict(bond=recovery.outpoint(bond(7)))],
        privateProbeProfile=dict(releaseRecoveryPolicy=dict(maxReservedContracts=1024)), **{'class': h(8)})
    row = dict(root=h(9), consent=h(10), recovery=h(11), artifactRoot=h(6), executor=bond(7),
        providers=[bond(12), bond(13)] + [None] * 6, acceptedDaa=10, enrollmentUntilDaa=100,
        boundClaim=h(14), enrollmentFinalized=False, **{'class': h(8)})
    observation = dict(version=4, checkpoint=h(4), daa=20, importEpoch=3, finalizedCheckpoint=None,
        rows=[row], releaseApproval=False, securitySoundnessMeasured=False)
    envelope = dict(schemaVersion=4, kind='rfc08-private-native-recovery-observation-v4',
        consensusParamsId=h(1), genesis=h(2), available=True, error=None, observation=observation,
        releaseApproval=False, securitySoundnessMeasured=False)
    providers = [recovery.outpoint(bond(12)), recovery.outpoint(bond(13))]
    return envelope, dag, node, bootstrap, providers


class OwnedRecoveryRead(unittest.TestCase):
    def read(self, values, **kwargs):
        return recovery.validate(*values, stamp, **kwargs)

    def test_owned_native_file_uses_the_actual_appdir_network_subdirectory(self):
        envelope, dag, node, bootstrap, providers = fixture()
        with tempfile.TemporaryDirectory() as folder:
            appdir = Path(folder) / 'B' / 'appdir'
            data = appdir / 'misaka-testnet-12'
            data.mkdir(parents=True)
            (data / recovery.FILE).write_text(json.dumps(envelope))
            drill = type('OwnedDrill', (), dict(root=Path(folder), values={'B_APPDIR': str(appdir)},
                plan=dict(nativeCustodyProvider=dict(providerBonds=providers))))()
            def rpc_read(_drill, _role, method):
                return dag if method == 'getBlockDagInfo' else node
            row = recovery.observe(drill, 'B', bootstrap, rpc_read, stamp)
            self.assertEqual(row['row']['root'], h(9))
            self.assertFalse(row['publicCertificate'])

    def test_current_native_metadata_requires_exact_generation_and_binding(self):
        values = fixture()
        result = self.read(values, expected_root=h(9), expected_claim=h(14))
        self.assertTrue(result['acceptedNativeRowObserved'])
        self.assertTrue(result['nativeBindingObserved'])
        self.assertFalse(result['enrollmentFinalizedObserved'])
        self.assertFalse(result['releaseApproval'])
        self.assertFalse(result['publicCertificate'])
        self.assertIsNone(self.read(values, expected_root=h(15)))
        self.assertIsNone(self.read(values, expected_claim=h(15)))
        values[0]['observation']['rows'][0]['boundClaim'] = None
        self.assertIsNone(self.read(values))
        self.assertIsNotNone(self.read(values, require_bound=False))

    def test_stale_branch_epoch_clock_or_identity_never_satisfies_observation(self):
        for target, field, value in [('observation', 'checkpoint', h(15)), ('observation', 'importEpoch', 2),
                ('observation', 'daa', 19), ('envelope', 'consensusParamsId', h(15)), ('envelope', 'genesis', h(15)),
                ('observation', 'daa', True), ('observation', 'importEpoch', True)]:
            with self.subTest(field=field, value=value):
                values = fixture()
                destination = values[0] if target == 'envelope' else values[0]['observation']
                destination[field] = value
                with self.assertRaises(AssertionError):
                    self.read(values)

    def test_refusal_cannot_be_promoted_to_release_or_security_approval(self):
        for target in ('envelope', 'observation'):
            for field in ('releaseApproval', 'securitySoundnessMeasured'):
                values = fixture()
                (values[0] if target == 'envelope' else values[0]['observation'])[field] = True
                with self.subTest(target=target, field=field), self.assertRaises(AssertionError):
                    self.read(values)
        values = fixture()
        values[0]['available'] = False
        self.assertIsNone(self.read(values))

    def test_finality_requires_non_genesis_native_checkpoint(self):
        values = fixture()
        self.assertIsNone(self.read(values, require_finalized=True))
        values[0]['observation']['rows'][0]['enrollmentFinalized'] = True
        for checkpoint in (None, h(2)):
            values[0]['observation']['finalizedCheckpoint'] = checkpoint
            with self.assertRaises(AssertionError):
                self.read(values, require_finalized=True)
        values[0]['observation']['finalizedCheckpoint'] = h(16)
        self.assertTrue(self.read(values, require_finalized=True)['enrollmentFinalizedObserved'])

    def test_wrong_executor_roster_class_or_artifact_has_no_matching_contract(self):
        for field, value in [('executor', bond(17)), ('providers', [bond(12), bond(17)] + [None] * 6),
                ('class', h(17)), ('artifactRoot', h(17))]:
            values = fixture()
            values[0]['observation']['rows'][0][field] = value
            with self.subTest(field=field):
                self.assertIsNone(self.read(values))
        values = fixture()
        values[0]['observation']['rows'][0]['providers'] = [bond(12), None, bond(13)] + [None] * 5
        with self.assertRaisesRegex(AssertionError, 'holes'):
            self.read(values)

    def test_duplicate_or_over_bound_rows_are_refused(self):
        values = fixture()
        values[0]['observation']['rows'] *= 2
        with self.assertRaisesRegex(AssertionError, 'repeats'):
            self.read(values)
        values[3]['privateProbeProfile']['releaseRecoveryPolicy']['maxReservedContracts'] = 1
        with self.assertRaises(AssertionError):
            self.read(values)

    def test_cross_node_agreement_compares_native_claim_but_epochs_are_local(self):
        value = self.read(fixture())
        views = [deepcopy(value) for _ in range(3)]
        views[1]['importEpoch'] = 99
        with patch.object(recovery, 'observe', side_effect=views):
            self.assertEqual(set(recovery.agreement(None, {}, None, stamp)), {'A', 'B', 'JOIN'})
        views[2]['row']['boundClaim'] = h(19)
        with patch.object(recovery, 'observe', side_effect=views):
            self.assertIsNone(recovery.agreement(None, {}, None, stamp))


if __name__ == '__main__':
    unittest.main()
