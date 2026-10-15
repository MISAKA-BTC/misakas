"""PAV3 prospective native session drill controls (2026-10-15 round 3), never native custody, Final or P2P acceptance."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('private_bootstrap_driver_session', SCRIPTS / 'misaka-rfc12-private-bootstrap.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)

FIXTURE = Path('/owned/fixture')


def seat(n):
    return dict(keyFile='base-material/bond-%d.seed' % n if n else 'base-material/bond.seed',
        bond='%s:%d' % ('ab' * 32, n), payAddress='misakatest:owned%d' % n, feeOutpoint='%s:%d' % ('cd' * 32, n),
        operatorId=('%02x' % (n + 1)) * 64)


def paid(seats=3):
    return dict(schema='misaka.private-paid-bootstrap.v3', networkDomain='0d' * 64, seats=[seat(n) for n in range(seats)],
        artifact='base-material/wide128-code-fixture.palwtir', floorClass='11' * 64, artifactRoot='22' * 64,
        **{'class': '33' * 64},
        monetaryMaterialPolicy=dict(id='44' * 64, windowDaa=20, minimumCollateralSompi=100, minimumFeeSompi=100,
            maxEnrollmentDaa=10000, maxBytesPerContract=32 << 20, nativeSessionMaxUnits=1000000, approval=False))


class NativeSessionSpec(unittest.TestCase):
    def test_spec_names_the_non_floor_class_executor_seat_and_two_provider_seats_at_factory_terms(self):
        spec = driver.native_session_spec(paid(), 500)
        session = spec['nativeSession']
        self.assertEqual(session['classId'], '33' * 64)
        self.assertEqual(session['executor'], seat(0)['bond'])
        self.assertEqual(session['operator'], seat(0)['operatorId'])
        self.assertEqual(spec['sponsor'], seat(0)['bond'])
        self.assertEqual([offer['provider'] for offer in spec['offers']], [seat(1)['bond'], seat(2)['bond']])
        self.assertTrue(all(offer['collateralSompi'] == 100 and offer['prepaidFeeSompi'] == 100 for offer in spec['offers']))
        self.assertEqual((session['admitFromDaa'], session['admitUntilDaa'], spec['enrollmentUntilDaa']), (500, 2500, 2500))
        self.assertEqual((spec['policyId'], spec['windowDaa'], spec['maxMaterialBytes']), ('44' * 64, 20, 32 << 20))
        short = paid()
        short['monetaryMaterialPolicy'] = dict(short['monetaryMaterialPolicy'], maxEnrollmentDaa=300)
        self.assertEqual(driver.native_session_spec(short, 500)['enrollmentUntilDaa'], 800, 'capped by the policy')

    def test_spec_is_refused_for_the_floor_an_approved_policy_missing_operators_or_shared_operators(self):
        floor = paid()
        floor['class'] = floor['floorClass']
        with self.assertRaisesRegex(ValueError, 'non-floor'):
            driver.native_session_spec(floor, 1)
        approved = paid()
        approved['monetaryMaterialPolicy'] = dict(approved['monetaryMaterialPolicy'], approval=True)
        with self.assertRaisesRegex(ValueError, 'unapproved'):
            driver.native_session_spec(approved, 1)
        with self.assertRaisesRegex(ValueError, 'two provider seats'):
            driver.native_session_spec(paid(seats=2), 1)
        unnamed = paid()
        del unnamed['seats'][1]['operatorId']
        with self.assertRaisesRegex(ValueError, 'operators'):
            driver.native_session_spec(unnamed, 1)
        shared = paid()
        shared['seats'][2]['operatorId'] = shared['seats'][0]['operatorId']
        with self.assertRaisesRegex(ValueError, 'distinct card operators'):
            driver.native_session_spec(shared, 1)

    def test_commands_draft_once_sign_per_party_with_its_own_key_and_seal_every_signature(self):
        folder = Path('/owned/run/native-session')
        steps, sealed = driver.material_session_commands(['misaka', '--output=json'], FIXTURE, paid(), folder)
        labels = [label for label, _ in steps]
        self.assertEqual(labels, ['NATIVE_SESSION_DRAFT', 'NATIVE_SESSION_SIGN_0', 'NATIVE_SESSION_SIGN_1',
            'NATIVE_SESSION_SIGN_2', 'NATIVE_SESSION_SEAL'])
        for (label, argv), n in zip(steps[1:4], (0, 1, 2)):
            with self.subTest(label=label):
                self.assertIn('--key-file=' + str(FIXTURE / seat(n)['keyFile']), argv)
                self.assertIn('--bond=' + seat(n)['bond'], argv)
                self.assertEqual(argv[2:5], ['palw', 'material-session', 'sign'])
        seal = steps[-1][1]
        self.assertEqual(seal[seal.index('--signature') + 1:],
            [str(folder / ('native-session-sig-%d.json' % n)) for n in (0, 1, 2)])
        self.assertIn('--out=' + str(sealed), seal)
        self.assertFalse([arg for _, argv in steps for arg in argv if arg in ('--yes', 'submit-object')],
            'the material-session steps never broadcast; submit-object files the sealed object separately')


class Observations(unittest.TestCase):
    def page(self, fragments, next_row, total, available=True):
        return dict(available=available, fragments=fragments, nextRow=next_row, nextOffset=0, totalRows=total)

    def test_rows_are_observed_only_from_a_complete_available_walk(self):
        root = bytes([7] * 64)
        rows = [dict(table=120, key=list(root), offset=0, row=0), dict(table=40, key=list(b'x'), offset=0, row=1)]
        self.assertEqual(driver.monetary_rows_observed([self.page(rows, 2, 2)]), {root.hex()})
        hexed = [dict(table=120, key=root.hex(), offset=0, row=0)]
        self.assertEqual(driver.monetary_rows_observed([self.page(hexed, 1, 1)]), {root.hex()})
        self.assertIsNone(driver.monetary_rows_observed([self.page(rows, 1, 2)]), 'an unfinished walk observes nothing')
        self.assertIsNone(driver.monetary_rows_observed([self.page([], 0, 0, available=False)]))
        self.assertIsNone(driver.monetary_rows_observed([]))

    def test_a_custody_store_is_observed_only_when_a_provider_file_is_nonempty(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory) / 'testnet-12' / driver.NATIVE_PROVIDER_STORE_DIR
            folder.mkdir(parents=True)
            (folder / 'a-1.store').write_bytes(b'')
            (folder / 'a-1.store.lock').write_bytes(b'x')
            self.assertEqual(driver.custody_store_observed(directory), [])
            (folder / 'a-2.store').write_bytes(b'bundle')
            self.assertEqual(driver.custody_store_observed(directory), [str(folder / 'a-2.store')])

    def test_legacy_session_step_requires_separate_complete_check_workflow(self):
        with self.assertRaisesRegex(ValueError, 'complete-check'):
            driver.validate_workflow_options('paid-v3', True, True)

    def test_v4_requires_actual_complete_check_before_non_floor_execution(self):
        with self.assertRaisesRegex(ValueError, 'requires --complete-check'):
            driver.validate_workflow_options('paid-v4', True, False)
        driver.validate_workflow_options('paid-v4', True, True)
        driver.validate_workflow_options('paid-v4', False, False)


if __name__ == '__main__':
    unittest.main()
