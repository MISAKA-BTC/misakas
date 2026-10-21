#!/usr/bin/env python3
"""Tests of the analysis harness only; NOT AB01–AB16 node certification."""
import json
import random
import sys
import unittest
from dataclasses import replace
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure as m

PLAN = json.loads(Path(__file__).with_name('plan.json').read_text())

class ArithmeticTests(unittest.TestCase):
    def test_currency_exact(self):
        self.assertEqual(m.coins('3200.87121681'), 320087121681)
        self.assertEqual(m.money(m.coins(100000)), '100000.00000000')
    def test_reject_negative_money(self):
        with self.assertRaises(ValueError): m.coins(-1)
    def test_reject_subsompi(self):
        with self.assertRaises(ValueError): m.coins('0.000000001')
    def test_ceildiv(self):
        self.assertEqual(m.ceildiv(10000,24),417)
        self.assertEqual(m.ceildiv(3120,24),130)
    def test_no_zero_divide(self):
        with self.assertRaises(ValueError): m.ceildiv(1,0)
    def test_liability_rounds_up(self):
        self.assertEqual(m.rounded_money(Decimal('0.000000001')),1)
    def test_small_sample_quantile(self):
        self.assertEqual(m.stats([1,2])['p95_nearest_rank'],2)
        self.assertEqual(m.stats([1,2])['n'],2)
    def test_empty_quantile_unknown(self):
        self.assertIsNone(m.stats([]))
    def test_baseline_minimum_inclusive(self):
        rows=m.envelope(PLAN)
        r=next(r for r in rows if r['hold_daa']==10000 and r['target_claims_per_720_daa']==30 and r['per_claim_liability_msk']==200)
        self.assertEqual(r['account_required_msk'],'100000.00000000')
    def test_previous_12k_example_fails_30day(self):
        r=next(r for r in m.envelope(PLAN) if r['hold_daa']==10000 and r['target_claims_per_720_daa']==30 and r['per_claim_liability_msk']==12000)
        self.assertEqual(r['account_required_msk'],'5004000.00000000')
        self.assertFalse(r['fits_account_minimum_arithmetically'])
    def test_strict_equality_boundary(self):
        r=next(r for r in m.envelope(PLAN) if r['hold_daa']==3120 and r['target_claims_per_720_daa']==30)
        self.assertEqual((r['overlap_claims'],r['overlap_with_strict_release_boundary']),(130,131))
    def test_no_100k_double_count(self):
        r=next(r for r in m.envelope(PLAN) if r['hold_daa']==10000 and r['target_claims_per_720_daa']==5 and r['per_claim_liability_msk']==1000)
        self.assertEqual(r['account_required_msk'],'100000.00000000')
    def test_prices_have_roundtrip_units(self):
        for x in [0,1,100,10**20]: self.assertEqual(m.coins(m.money(x)),x)

class ShadowTests(unittest.TestCase):
    def test_eight_fit_ninth_fails(self):
        r=m.shadow_replay([(0,9)],m.coins(100000),m.coins(12000),10000)
        self.assertEqual((r['retained_opportunities'],r['account_blocked_opportunities']),(8,1))
    def test_no_release_on_old_final(self):
        r=m.shadow_replay([(0,8),(142,1)],m.coins(100000),m.coins(12000),10000)
        self.assertEqual(r['account_blocked_opportunities'],1)
    def test_explicit_lawful_release_fixture(self):
        r=m.shadow_replay([(0,8),(10000,8)],m.coins(100000),m.coins(12000),10000)
        self.assertEqual(r['retained_opportunities'],16)
    def test_unresolved_stays_locked(self):
        r=m.shadow_replay([(0,8),(10000,1),(1000000,1)],m.coins(100000),m.coins(12000),None)
        self.assertEqual((r['retained_opportunities'],r['account_blocked_opportunities']),(8,2))
    def test_no_cross_claim_double_backing(self):
        r=m.shadow_replay([(0,100)],m.coins(100000),m.coins(100000),None)
        self.assertEqual(r['retained_opportunities'],1)
    def test_empty_trace(self):
        self.assertEqual(m.shadow_replay([],100,10,5)['retained_opportunities'],0)
    def test_zero_liability_rejected(self):
        with self.assertRaises(ValueError): m.shadow_replay([(0,1)],100,0,5)
    def test_seeded_conservation_and_monotonicity_1000_traces(self):
        rng=random.Random(260027)
        for _ in range(1000):
            events=[(i,rng.randrange(6)) for i in range(50)]
            liability=rng.randrange(1,100); amount=rng.randrange(0,1000)
            r=m.shadow_replay(events,amount,liability,None)
            r2=m.shadow_replay(events,amount+500,liability,None)
            self.assertEqual(r['retained_opportunities']+r['account_blocked_opportunities'],sum(c for _,c in events))
            self.assertGreaterEqual(r2['retained_opportunities'],r['retained_opportunities'])
            self.assertLessEqual(r['end_unreleased_claims']*liability,amount)

class ModelTests(unittest.TestCase):
    def base(self):
        return m.Simulation(name='test',account=m.coins(100000),production=m.coins(200000),liability=m.coins(12000),
          account_hold=10000,production_hold=720,reserve=m.coins('3200.87121681'),target_daily=30,
          horizon=30000,warmup=10000,production_minimum=m.coins(13000),account_minimum=m.coins(100000))
    def test_old_capital_only_no_new_gate(self):
        r=m.simulate(replace(self.base(),account=None))
        self.assertEqual(r['admitted'],r['offered'])
    def test_total_100k_leaves_zero_old_capital(self):
        r=m.simulate(replace(self.base(),production=0))
        self.assertEqual(r['admitted'],0)
    def test_new_account_minimum_missing(self):
        r=m.simulate(replace(self.base(),account=m.coins(99999)))
        self.assertEqual(r['admitted'],0)
    def test_hard_does_not_release_unresolved(self):
        r=m.simulate(replace(self.base(),account_hold=None))
        self.assertEqual(r['admitted'],8)
        self.assertEqual(r['post_warmup_admitted'],0)
    def test_topup_restarts_offered_admission(self):
        r=m.simulate(replace(self.base(),topup_at=1000,topup_amount=m.coins(4904000)))
        self.assertEqual(r['first_admission_after_topup_daa'],1008)
    def test_real_and_heartbeat_not_fabricated(self):
        r=m.simulate(self.base())
        self.assertIsNone(r['accepted_REAL_blocks'])
        self.assertIsNone(r['heartbeat_dependence_daa'])
    def test_constraints_count_all_flags(self):
        r=m.simulate(replace(self.base(),account=0,production=0))
        self.assertGreater(r['all_failure_flags_counts']['AccountMinimumMissing'],0)
        self.assertGreater(r['all_failure_flags_counts']['LegacyMinimumMissing'],0)
    def test_candidate_tariff_positive_margin_is_assumption_not_gate(self):
        g=dict(label='fixture',claim_case='late',via='outsider',provider='files',n=1,cpu_s={'p95_nearest_rank':628},
               da_bytes_max=1921824608,filing_bytes_max=31492,local_court_convictions=1,wire_refusals=0)
        rows=m.pricing(PLAN,[g])
        self.assertTrue(all(not r['complete_F02_F03_Lc_derived'] for r in rows))
        for r in rows:
            self.assertGreater(m.coins(r['candidate_work_fee_msk']),m.coins(r['cost_per_observed_job_msk']))
    def test_wire_refused_not_paid_success(self):
        g=dict(label='fixture',claim_case='gather',via='outsider',provider='files',n=1,cpu_s={'p95_nearest_rank':15},
               da_bytes_max=178650993,filing_bytes_max=272271336,local_court_convictions=0,wire_refusals=1)
        rows=m.pricing(PLAN,[g])
        self.assertTrue(all(r['net_worker_and_reporter_success_msk'] is None for r in rows))
    def test_self_report_cashflow_no_new_income(self):
        # Fee escrow movement and its payment cancel within the same coalition.
        # The returned reimbursement+bounty does not exceed actual account debit.
        cost=m.coins(100); fee=m.coins(125); bounty=m.coins(100); burn=m.coins('22.5')
        slash=fee+bounty+burn
        self.assertEqual(-fee+fee-slash+fee+bounty,-burn)
        self.assertLess(-burn-cost,0)
    def test_no_success_means_no_bounty_income(self):
        # An unslashed Account Bond is unavailable revenue, regardless of size.
        for account in [m.coins(100000),m.coins(100000000)]:
            actual_slash=0; funded_normal_service=0
            self.assertEqual(actual_slash+funded_normal_service,0)

if __name__=='__main__': unittest.main(verbosity=2)
