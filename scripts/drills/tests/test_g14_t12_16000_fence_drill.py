"""Plan / judgement logic of the G14 full-activation fence-crossing drill (no processes are started)."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g14_t12_16000_fence_drill as D  # noqa: E402

SALT = 'ab' * 32


def good_obs(plan):
    at = plan['at']
    return {
        'identity': {'profileStart': {'id': 1}, 'profileEnd': {'id': 1}, 'binariesStart': {'b': 'x'}, 'binariesEnd': {'b': 'x'},
                     'genesisMatchesKeyring': True},
        'claims': [
            {'claimId': 'a', 'acceptedDaa': at - 10, 'phase': 'Final', 'phaseDaa': at + 30},
            {'claimId': 'b', 'acceptedDaa': at - 5, 'phase': 'Settled', 'phaseDaa': at + 31},
            {'claimId': 'c', 'acceptedDaa': at, 'phase': 'Open'},
            {'claimId': 'd', 'acceptedDaa': at + 9, 'phase': 'Final'},
        ],
        'partition': {'qRestartedBelowH': True, 'P': {'sink': 's1', 'daa': at + 6}, 'Q': {'sink': 's2', 'daa': at + 7},
                      'loserBefore': {'sink': 's1', 'daa': at + 6}, 'mergedSink': 's2', 'mergedSame': True, 'loser': 'P'},
        'restart': {'stoppedAtDaa': at + 10, 'resumed': True, 'same': True},
        'ibd': {'same': True, 'sink': 's9', 'fromGenesis': True},
    }


class PlanTests(unittest.TestCase):
    def test_plan_heights_derive_from_h(self):
        p = D.build_plan(60)
        self.assertEqual((p['partitionStartDaa'], p['partitionEndDaa'], p['restartDaa']), (54, 66, 70))
        self.assertEqual(p['phases'], list(D.PHASES))
        self.assertEqual(p['publicTarget'], 17_000)
        self.assertEqual(D.build_plan(60, public_target=18_000)['publicTarget'], 18_000)

    def test_h_must_clear_the_factory_flag_days_and_not_be_the_public_height(self):
        for bad in (0, 28):
            with self.assertRaises(D.PlanError):
                D.build_plan(bad)
        with self.assertRaises(D.PlanError):
            D.build_plan(17_000)
        with self.assertRaises(D.PlanError):
            D.build_plan(16_500, public_target=16_500)
        with self.assertRaises(D.PlanError):
            D.build_plan(60, public_target=20)
        with self.assertRaises(D.PlanError):
            D.build_plan(32, partition_lead=6)  # the partition would start at 26 <= 28

    def test_restart_after_the_reorg_and_claims_on_both_sides(self):
        with self.assertRaises(D.PlanError):
            D.build_plan(60, partition_tail=6, restart_after=6)
        with self.assertRaises(D.PlanError):
            D.build_plan(60, pre_claims=0)
        with self.assertRaises(D.PlanError):
            D.build_plan(60, partition_lead=1)

    def test_salt(self):
        self.assertEqual(D.validate_salt('AB' * 32), 'ab' * 32)
        for bad in ('0' * 64, 'zz' * 32, 'ab' * 31, None):
            with self.assertRaises(D.PlanError):
                D.validate_salt(bad)

    def test_roles_q_is_outside_the_panel(self):
        r = D.roles_for(D.build_plan(60), 3)
        self.assertEqual(r['Q']['card'], 4)
        self.assertTrue(r['P']['hostsPanel'])
        self.assertIsNone(D.build_plan(60)['roles']['Q']['card'], 'roles_for does not mutate the plan')

    def test_argv_moves_the_full_set_to_h_and_is_loopback_only(self):
        argv = D.node_argv('/bin/node', SALT, 60, 3, '/w/P/app', (1, 2, 3, 4, 5), ['--palw-drill-int13-at=60'])
        self.assertIn('--private-probe-g14-at=60', argv)
        self.assertIn('--private-probe-g14-set=full', argv)
        self.assertIn('--nodnsseed', argv)
        self.assertTrue(all('0.0.0.0' not in a for a in argv))
        self.assertEqual(argv[-1], '--palw-drill-int13-at=60')
        with self.assertRaises(D.PlanError):
            D.node_argv('/bin/node', SALT, 60, 3, '/w', (1, 2, 3, 4, 5), ['--connect=1.2.3.4'])


class JudgeTests(unittest.TestCase):
    def setUp(self):
        self.plan = D.build_plan(60)

    def test_all_observed_is_pass(self):
        r = D.judge(self.plan, good_obs(self.plan))
        self.assertEqual(r['verdict'], 'PASS', r['rows'])
        self.assertEqual([x['phase'] for x in r['rows']], list(D.PHASES))
        self.assertFalse(r['releaseApproval'])

    def test_missing_observations_are_blocked_never_pass(self):
        r = D.judge(self.plan, {})
        self.assertEqual(r['verdict'], 'BLOCKED')
        self.assertTrue(all(x['status'] == 'BLOCKED' for x in r['rows']))

    def test_split_claims_at_h_is_inclusive_on_the_new_side(self):
        pre, post = D.split_claims([{'acceptedDaa': 59}, {'acceptedDaa': 60}, {'acceptedDaa': None}], 60)
        self.assertEqual((len(pre), len(post)), (1, 1))

    def test_old_claim_voided_by_a_new_rule_fails_f6(self):
        obs = good_obs(self.plan)
        obs['claims'][0].update({'phase': 'Void', 'voidReason': 'G14 route required', 'phaseDaa': 61})
        r = D.judge(self.plan, obs)
        f6 = [x for x in r['rows'] if x['phase'] == 'F6'][0]
        self.assertEqual(f6['status'], 'FAIL')
        self.assertEqual(r['verdict'], 'FAIL')

    def test_old_claim_voided_below_h_is_not_a_new_rule(self):
        self.assertFalse(D.voided_by_new_rules({'voidReason': 'g14 fence', 'phaseDaa': 59}, 60))

    def test_unfinished_old_claim_fails_f6(self):
        obs = good_obs(self.plan)
        obs['claims'][1]['phase'] = 'Challengeable'
        f6 = [x for x in D.judge(self.plan, obs)['rows'] if x['phase'] == 'F6'][0]
        self.assertEqual(f6['status'], 'FAIL')

    def test_too_few_claims_on_either_side_fail(self):
        obs = good_obs(self.plan)
        obs['claims'] = obs['claims'][:3]  # one post-H claim
        rows = {x['phase']: x['status'] for x in D.judge(self.plan, obs)['rows']}
        self.assertEqual((rows['F1'], rows['F5']), ('PASS', 'FAIL'))

    def test_no_reorg_or_reorg_below_h_fails_f3(self):
        self.assertFalse(D.reorg_observed({'sink': 's', 'daa': 70}, 's', 60))
        self.assertFalse(D.reorg_observed({'sink': 's', 'daa': 59}, 't', 60))
        self.assertTrue(D.reorg_observed({'sink': 's', 'daa': 60}, 't', 60))
        obs = good_obs(self.plan)
        obs['partition']['mergedSink'] = 's1'
        rows = {x['phase']: x['status'] for x in D.judge(self.plan, obs)['rows']}
        self.assertEqual(rows['F3'], 'FAIL')

    def test_identity_drift_fails_f0(self):
        obs = good_obs(self.plan)
        obs['identity']['binariesEnd'] = {'b': 'y'}
        rows = {x['phase']: x['status'] for x in D.judge(self.plan, obs)['rows']}
        self.assertEqual(rows['F0'], 'FAIL')

    def test_restart_below_h_fails_f4(self):
        obs = good_obs(self.plan)
        obs['restart']['stoppedAtDaa'] = 59
        rows = {x['phase']: x['status'] for x in D.judge(self.plan, obs)['rows']}
        self.assertEqual(rows['F4'], 'FAIL')



class CrossingFlagTest(unittest.TestCase):
    def test_the_default_crossing_moves_the_release_list_with_kaspads_flag(self):
        self.assertEqual(D.crossing_flags('full-activation-flag', 60), ['--palw-drill-full-activation-at=60'])
        self.assertEqual(D.crossing_flags('factory', 60), [])
        with self.assertRaises(D.PlanError):
            D.crossing_flags('nope', 60)
        argv = D.node_argv('n', 'ab' * 32, 60, 3, Path('/x'), (1, 2, 3, 4, 5), D.crossing_flags('full-activation-flag', 60))
        self.assertIn('--palw-drill-full-activation-at=60', argv)

if __name__ == '__main__':
    unittest.main()
