"""PAV3 loopback native provider plan controls (2026-10-15), never native custody or P2P acceptance."""
import importlib.util
from pathlib import Path
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('private_bootstrap_driver_native', SCRIPTS / 'misaka-rfc12-private-bootstrap.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)

FIXTURE = Path('/owned/fixture')


def seat(n):
    return dict(keyFile='base-material/bond-%d.seed' % n if n else 'base-material/bond.seed',
        bond='%s:%d' % ('ab' * 32, n), payAddress='misakatest:owned%d' % n, feeOutpoint='%s:%d' % ('cd' * 32, n))


def paid(seats=3):
    return dict(schema='misaka.private-paid-bootstrap.v3', skipProofOfWork=False, releaseApproval=False,
        baseArtifactsAreControlReceipts=False, paidClaimIncluded=False, nativeFinalObserved=False,
        seats=[seat(n) for n in range(seats)], artifact='base-material/wide128-code-fixture.palwtir', floorClass='11' * 64)


def complete():
    return dict(paid(), kind='rfc12-private-complete-bootstrap-file-fixture', schema=None)


def paid_v4():
    manifest = paid(seats=4)
    for n, row in enumerate(manifest['seats']):
        row.update(bond='%s:%d' % ('ab' * 64, n), feeOutpoint='%s:%d' % ('cd' * 64, n))
    manifest.update(schema='misaka.private-paid-bootstrap.v4', genesis='11' * 64,
        consensusParamsId='22' * 64, compactContractIncluded=False, recoveryEnrollmentIncluded=False,
        fundingKeyFile='base-material/funding.seed')
    manifest['privateProbeProfile'] = dict(privatePaidProfileVersion=4, genesis=manifest['genesis'],
        consensusParamsId=manifest['consensusParamsId'], skipProofOfWork=False, releaseApproval=False,
        compactClaimPolicy=dict(retired=True, reason='PRINCIPLES §8'), releaseRecoveryPolicy=dict(id='44' * 64, approved=False))
    return manifest


def flags(argv, name):
    return [arg for arg in argv if arg.startswith(name + '=')]


class NativeProviderPlan(unittest.TestCase):
    def test_v4_requires_separate_identity_and_cannot_promote_offline_receipts(self):
        original = paid_v4()
        driver.validate_factory_manifest(original, 'paid-v4')
        for field in ('compactContractIncluded', 'recoveryEnrollmentIncluded'):
            with self.subTest(field=field), self.assertRaises(ValueError):
                driver.validate_factory_manifest(dict(original, **{field: True}), 'paid-v4')
        # PRINCIPLES §8: compact V4 is retired, so an armed (id/approved) compact policy is refused like any other substitution.
        for field, value in [('consensusParamsId', '55' * 64), ('skipProofOfWork', True),
                ('releaseRecoveryPolicy', dict(id='44' * 64, approved=True)),
                ('compactClaimPolicy', dict(id='33' * 64, approved=False))]:
            changed = dict(original, privateProbeProfile=dict(original['privateProbeProfile'], **{field: value}))
            with self.subTest(field=field), self.assertRaises(ValueError):
                driver.validate_factory_manifest(changed, 'paid-v4')
        with self.assertRaises(ValueError):
            driver.validate_factory_manifest(paid(), 'paid-v4')

    def test_v4_carriers_have_a_dedicated_fee_payer_and_explicit_cap(self):
        manifest = paid_v4()
        # Rust orders the raw Hash64 bytes, then the u32, rather than the
        # textual decimal suffix (10 must follow 2).
        manifest['seats'][1]['bond'] = 'ab' * 64 + ':10'
        manifest['seats'][2]['bond'] = 'ab' * 64 + ':2'
        provider = driver.native_provider_config_v4(FIXTURE, manifest, '127.0.0.1:7000', 900)
        self.assertEqual(provider['carrierPayerSeat'], 3)
        self.assertEqual(provider['providerBonds'], ['ab' * 64 + ':2', 'ab' * 64 + ':10'])
        self.assertIn('--private-probe-recovery-ack-key=' + str(FIXTURE / manifest['seats'][3]['keyFile']),
            provider['hostArgv'])
        self.assertIn('--private-probe-recovery-ack-funding=' + manifest['seats'][3]['feeOutpoint'], provider['hostArgv'])
        self.assertIn('--private-probe-recovery-ack-fee-cap=900', provider['hostArgv'])
        self.assertIn('--private-probe-native-recovery-providers=' + ','.join(provider['providerBonds']), provider['clientArgv'])
        for cap in (None, 0, -1, True, 2**64, 1.5):
            with self.subTest(cap=cap), self.assertRaises(ValueError):
                driver.native_provider_config_v4(FIXTURE, manifest, '127.0.0.1:7000', cap)
        reused = paid_v4()
        reused['seats'][3]['feeOutpoint'] = reused['seats'][0]['feeOutpoint']
        with self.assertRaisesRegex(ValueError, 'dedicated'):
            driver.native_provider_config_v4(FIXTURE, reused, '127.0.0.1:7000', 900)
        short_hash = paid_v4()
        short_hash['seats'][1]['bond'] = 'ab' * 32 + ':1'
        with self.assertRaisesRegex(ValueError, 'canonical'):
            driver.native_provider_config_v4(FIXTURE, short_hash, '127.0.0.1:7000', 900)

    def test_default_paid_and_complete_plans_carry_no_native_wiring(self):
        plan = driver.plan_for(Path('/owned/node'), '74' * 32, FIXTURE, paid(), 30, 'paid-v3')
        self.assertIn('nativeCustodyProvider', plan)
        self.assertIsNone(plan['nativeCustodyProvider'])
        original = driver.plan_for(Path('/owned/node'), '74' * 32, FIXTURE, complete(), 30, 'complete-v1')
        self.assertNotIn('nativeCustodyProvider', original, 'the complete-v1 plan keeps its exact fields')
        for role, node in list(plan['nodes'].items()) + list(original['nodes'].items()):
            with self.subTest(role=role):
                self.assertFalse([arg for arg in node['argv'] if arg.startswith('--private-probe-native-')])

    def test_requested_provider_is_b_hosting_seats_one_two_for_a_on_one_loopback_socket(self):
        plan = driver.plan_for(Path('/owned/node'), '74' * 32, FIXTURE, paid(), 30, 'paid-v3', True)
        provider = plan['nativeCustodyProvider']
        self.assertEqual(provider['kind'], driver.NATIVE_PROVIDER_KIND)
        self.assertEqual((provider['host'], provider['exchangeClient']), ('B', 'A'))
        self.assertEqual(provider['providerSeats'], [1, 2])
        self.assertEqual(provider['executorSeat'], 0)
        self.assertTrue(provider['unverifiedSource'])
        self.assertFalse(provider['nativeCustodyMeasured'] or provider['releaseApproval'])
        self.assertTrue(provider['listen'].startswith('127.0.0.1:'))
        endpoints = {node[k] for node in plan['nodes'].values() for k in ('p2p', 'grpc', 'wrpc', 'borsh')}
        self.assertNotIn(provider['listen'], endpoints, 'the provider socket is its own reserved port')
        b, a, join = (plan['nodes'][role]['argv'] for role in ('B', 'A', 'JOIN'))
        keys = flags(b, '--private-probe-native-provider-keys')
        self.assertEqual(len(keys), 1)
        named = keys[0].split('=', 1)[1].split(',')
        self.assertEqual(named, ['%s@%s' % (FIXTURE / seat(n)['keyFile'], seat(n)['bond']) for n in (1, 2)])
        self.assertEqual(flags(b, '--private-probe-native-provider-listen'),
            ['--private-probe-native-provider-listen=' + provider['listen']])
        self.assertEqual(flags(a, '--private-probe-native-exchange'), ['--private-probe-native-exchange=' + provider['listen']])
        self.assertIn('--palw-produce', a)
        self.assertNotIn('--palw-produce', b)
        self.assertFalse([arg for arg in join if arg.startswith('--private-probe-native-')])
        self.assertFalse(flags(a, '--private-probe-native-provider-keys'), 'the executor hosts no provider')

    def test_provider_is_refused_off_paid_v3_and_without_two_provider_seats(self):
        with self.assertRaisesRegex(ValueError, 'paid-v3 profile option only'):
            driver.plan_for(Path('/owned/node'), '74' * 32, FIXTURE, complete(), 30, 'complete-v1', True)
        with self.assertRaisesRegex(ValueError, 'two provider seats'):
            driver.native_provider_config(FIXTURE, paid(seats=2), '127.0.0.1:7000')
        doubled = paid()
        doubled['seats'][2] = dict(doubled['seats'][2], bond=doubled['seats'][0]['bond'])
        with self.assertRaisesRegex(ValueError, 'distinct'):
            driver.native_provider_config(FIXTURE, doubled, '127.0.0.1:7000')
        ambiguous = paid()
        ambiguous['seats'][1] = dict(ambiguous['seats'][1], keyFile='a,b.seed')
        with self.assertRaisesRegex(ValueError, 'unambiguously'):
            driver.native_provider_config(FIXTURE, ambiguous, '127.0.0.1:7000')


if __name__ == '__main__':
    unittest.main()
