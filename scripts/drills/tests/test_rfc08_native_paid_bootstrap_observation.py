"""Unexecuted contract controls. Synthetic rows/files are not native evidence."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc08_native_paid_observation as legacy
import rfc08_native_paid_bootstrap_observation as bootstrap
from test_rfc08_native_paid_observation import fixture as legacy_fixture, h, stamp


def fixture():
    envelope, dag, node = legacy_fixture()
    envelope['schemaVersion'] = 6
    envelope['kind'] = 'rfc08-private-native-paid-bootstrap-observation-v6'
    snapshot = envelope['observation']['snapshot']
    lock = envelope['observation']['lastLock']
    lock['context'] = h(14)
    value = dict(version=6, snapshot=snapshot, baseClaim=h(6), authorityPresent=True, explicitV6Binding=True,
        enrollment=h(7), context=h(8), checkerSnapshot=h(9), policy=h(15), binding=h(14), bootstrapEpoch=h(16),
        requiredSources=2, currentSources=2, sourceStage='LOCKED', sourceContext=h(17), sourceRoot=h(18),
        sourceOutput=h(19), sourceLockedDaa=89, lock=lock, nativeConsumerComplete=True,
        nativeAssignedQuorum='OBSERVED', sourceGraphRevalidated='OBSERVED', finalProjection='OBSERVED',
        nativeSelectedApproval='UNKNOWN', historicalSignatureReplay='UNKNOWN', coinbaseOutputReplayed='UNKNOWN',
        oneTimeFeeSettlement='UNKNOWN', releaseApproval=False, securitySoundnessMeasured=False)
    envelope['observation'] = value
    return envelope, dag, node


class BootstrapObservationContract(unittest.TestCase):
    def validate(self, envelope):
        _, dag, node = fixture()
        return bootstrap.validate(envelope, h(6), dag, node, stamp, h(3), h(4))

    def test_legacy_cannot_be_a_v6_source_graph_and_v6_is_not_a_legacy_lock(self):
        old, _, _ = legacy_fixture()
        with self.assertRaises(legacy.Unavailable):
            self.validate(old)
        current, dag, node = fixture()
        with self.assertRaises(legacy.Unavailable):
            legacy.validate(current, h(6), dag, node, stamp, h(3), h(4))
        self.assertEqual(self.validate(current)['sourceStage'], 'LOCKED')

    def test_exact_binding_clock_counts_and_noncoercing_schema(self):
        good, _, _ = fixture()
        for key, value in (('version', True), ('requiredSources', True), ('currentSources', 3),
                ('sourceLockedDaa', 101), ('sourceLockedDaa', 89.0), ('explicitV6Binding', False),
                ('sourceStage', 'PENDING'), ('bootstrapEpoch', None), ('policy', '0' * 128)):
            bad = deepcopy(good)
            bad['observation'][key] = value
            with self.subTest(key=key), self.assertRaises(legacy.Unavailable):
                self.validate(bad)
        for key, value in (('context', h(40)), ('round', 1), ('seats', 2), ('lockedDaa', 101), ('quorum', True)):
            bad = deepcopy(good)
            bad['observation']['lock'][key] = value
            with self.subTest(lock=key), self.assertRaises(legacy.Unavailable):
                self.validate(bad)

    def test_current_graph_does_not_upgrade_full_approval_or_payment_even_if_text_is_observed(self):
        envelope, _, _ = fixture()
        value = self.validate(envelope)
        for key in bootstrap.UNRESOLVED:
            value[key] = 'OBSERVED'
        observed = dict(available=True, observation=value)
        for kind in bootstrap.KINDS:
            result = bootstrap.fact(dict(id=kind, kind=kind, role='A', claimId=h(6), expected={}), observed)
            self.assertEqual(result['status'], 'PASS')
            self.assertEqual(result['selectedVerifierApproval'], 'UNKNOWN')
            self.assertEqual(result['historicalSignatures'], 'UNKNOWN')
            self.assertEqual(result['oneTimeFeeSettlement'], 'UNKNOWN')
            self.assertFalse(result['nativeAuthorityEstablished'])
        value['sourceGraphRevalidated'] = 'INCOMPLETE'
        result = bootstrap.fact(dict(id='q', kind='native-paid-bootstrap-quorum', role='A', claimId=h(6), expected={}), observed)
        self.assertEqual(result['status'], 'UNKNOWN')
        for kind in ('selected-verifier-approval', 'native-fee-settlement', 'all-rewards-once', 'independent-source-availability'):
            result = bootstrap.fact(dict(id=kind, kind=kind, role='A', claimId=h(6), expected={}), observed)
            self.assertEqual(result['status'], 'UNKNOWN')

    def test_restart_invalidates_both_namespaces_only_for_explicit_owned_claim(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            argv = ['--private-probe-profile=source-paid-v1', legacy.SOURCE_POLICY_FLAG + '/frozen/source',
                legacy.PAID_BOOTSTRAP_POLICY_FLAG + '/frozen/paid', legacy.FLAG + h(6)]
            drill = SimpleNamespace(values={'A_APPDIR': str(root / 'appdir')}, processes={},
                plan=dict(nodes=dict(A=dict(argv=argv))))
            for directory in (legacy.directory(drill, 'A'), bootstrap.directory(drill, 'A')):
                directory.mkdir(parents=True)
                (directory / (h(6) + '.json')).write_text('stale')
                (directory / (h(7) + '.json')).write_text('unrelated')
            legacy.invalidate(drill, 'A')
            for directory in (legacy.directory(drill, 'A'), bootstrap.directory(drill, 'A')):
                self.assertFalse((directory / (h(6) + '.json')).exists())
                self.assertTrue((directory / (h(7) + '.json')).exists())


if __name__ == '__main__':
    unittest.main()
