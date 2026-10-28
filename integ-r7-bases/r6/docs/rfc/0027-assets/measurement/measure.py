#!/usr/bin/env python3
"""Reaggregate uploaded measurements and execute RFC-0027 shadow experiments.

Standard library only. No node writes, network access, protocol patch, assumed block
inclusion or cryptocurrency price lookup. Monetary arithmetic uses integer sompi.
"""
from __future__ import annotations
import argparse
import collections
import csv
import hashlib
import heapq
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal, ROUND_CEILING
from pathlib import Path
from typing import Any

COIN = 100_000_000
CAP = 'docs/evidence/palw/claim-capacity-map-2026-09-26'
OPV = 'docs/evidence/palw/opv-measurements-data'
VERIFY_FILES = ['qwen25-p3.jsonl', 'qwen25-sweep.jsonl', 'gap07-verify.jsonl']


def coins(x: Any) -> int:
    d = Decimal(str(x)) * COIN
    if d < 0 or d != d.to_integral_value():
        raise ValueError(f'non-integral or negative sompi: {x!r}')
    return int(d)


def money(x: int) -> str:
    return format(Decimal(x) / COIN, '.8f')


def ceildiv(n: int, d: int) -> int:
    if n < 0 or d <= 0:
        raise ValueError('ceildiv needs n >= 0 and d > 0')
    return (n + d - 1) // d


def rounded_money(x: Decimal) -> int:
    if x < 0:
        raise ValueError('negative amount')
    return int((x * COIN).to_integral_value(rounding=ROUND_CEILING))


def stats(v: list[float]) -> dict[str, Any] | None:
    if not v:
        return None
    a = sorted(v)
    return {'n': len(a), 'p50': statistics.median(a),
            'p95_nearest_rank': a[math.ceil(.95 * len(a)) - 1],
            'min': a[0], 'max': a[-1],
            'tail_warning': 'Descriptive sample quantile, not a population tail guarantee; small n.'}


def put(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + '\n', encoding='utf-8')


def table(path: Path, rows: list[dict[str, Any]]) -> None:
    if not rows:
        raise ValueError(f'no rows for {path}')
    with path.open('w', newline='', encoding='utf-8') as f:
        fields = list(dict.fromkeys(k for r in rows for k in r))
        w = csv.DictWriter(f, fieldnames=fields, lineterminator='\n')
        w.writeheader()
        for r in rows:
            w.writerow({k: json.dumps(v, ensure_ascii=False) if isinstance(v, (dict, list)) else v for k, v in r.items()})


def load_archive(repo: Path) -> tuple[list[dict], list[dict], list[dict], list[str], list[dict]]:
    samples, sources, checks = [], [], []
    grouped: dict[tuple, list[dict]] = collections.defaultdict(list)
    for filename in VERIFY_FILES:
        rel = f'{OPV}/{filename}'
        sources.append(rel)
        for line, raw in enumerate((repo / rel).read_text().splitlines(), 1):
            row = json.loads(raw)
            if row.get('cmd') != 'verify':
                continue
            result = row['result']
            phases = row['phases']
            sample = {
                'source': rel, 'line': line, 'evidence': 'M_ARCHIVE',
                'label': row['label'], 'claim_case': row['claim'], 'via': row['via'],
                'provider': row['provider'], 'cpu_s': row['cpu_total_s'], 'wall_s': row['wall_total_s'],
                'da_bytes': row['da_bytes'], 'artifact_bytes': row['artifact_bytes'],
                'peak_rss_bytes': row['peak_rss_bytes'],
                'filing_bytes': result.get('filing_object_bytes', 0),
                'finding': result.get('finding'), 'court_events': result.get('court_events', []),
                'wire_refusal': result.get('filing_refused_at_wire'),
                'fits_route_object_cap': result.get('fits_route_object_cap'),
                'phases': phases,
            }
            sample['cpu_outside_recorded_phases_s'] = sample['cpu_s'] - sum(p['cpu_s'] for p in phases)
            # Harness total is process rusage, not the sum of timed phases (verify.rs:378).
            # Preserve setup/teardown overhead instead of forcing equality.
            checks.append({'name': f'cpu_total_covers_nonnegative_phases:{filename}:{line}',
                           'ok': sample['cpu_outside_recorded_phases_s'] >= -1e-6 and all(p['cpu_s'] >= 0 for p in phases)})
            samples.append(sample)
            key = (sample['label'], sample['claim_case'], sample['via'], sample['provider'])
            grouped[key].append(sample)
    groups = []
    for key, rows in sorted(grouped.items()):
        g = dict(zip(['label', 'claim_case', 'via', 'provider'], key))
        g.update({'evidence': 'M_ARCHIVE', 'n': len(rows),
                  'cpu_s': stats([r['cpu_s'] for r in rows]),
                  'wall_s': stats([r['wall_s'] for r in rows]),
                  'peak_rss_bytes': stats([r['peak_rss_bytes'] for r in rows]),
                  'da_bytes_max': max(r['da_bytes'] for r in rows),
                  'artifact_bytes_max': max(r['artifact_bytes'] for r in rows),
                  'filing_bytes_max': max(r['filing_bytes'] for r in rows),
                  'local_court_convictions': sum(any(e.startswith('Convicted(') for e in r['court_events']) for r in rows),
                  'wire_refusals': sum(bool(r['wire_refusal']) for r in rows),
                  'source_rows': [{'path': r['source'], 'line': r['line']} for r in rows]})
        groups.append(g)
    runs = []
    for f in sorted((repo / CAP / 'runs').glob('*.json')):
        rel = str(f.relative_to(repo))
        sources.append(rel)
        d = json.loads(f.read_text())
        for subject, rows in d['subjects'].items():
            lives = [r for r in d['lives'] if r[0] == subject]
            per_daa = collections.Counter()
            for r in rows:
                per_daa[r[0]] += r[2]
            acceptance_count = sum(per_daa.values())
            multiplicity = collections.Counter(r[0] for r in rows)
            max_multiplicity = max(multiplicity.values())
            checks.append({'name': f'accept_lives_match:{f.stem}:{subject}', 'ok': acceptance_count == len(lives)})
            runs.append({
                'source': rel, 'run': d['run'], 'subject_balance_msk': int(subject),
                'evidence': 'M_ARCHIVE', 'row_count': len(rows),
                'observed_daa_slots': max(per_daa) - min(per_daa) + 1,
                'account_multiplicity': max_multiplicity,
                'account_identity_ambiguous': max_multiplicity > 1,
                'accepted_claims': acceptance_count,
                'first80_accepted': sum(v for t, v in per_daa.items() if 0 <= t < 80),
                'last80_accepted': sum(v for t, v in per_daa.items() if max(per_daa) - 79 <= t <= max(per_daa)),
                'legacy_escrow_release_delay_daa': stats([r[4]-r[1] for r in lives if r[4] is not None]),
                'observed_final_delay_daa': stats([r[5]-r[1] for r in lives if r[5] is not None]),
                'final_unobserved_or_censored': sum(r[5] is None for r in lives),
                'post_fill_hold_reasons_NOT_refusal_counts': dict(collections.Counter(r[7] for r in rows)),
                'accepted_real_block_count': None, 'heartbeat_share': None,
                'accepted_events': sorted(per_daa.items()),
            })
    return samples, groups, runs, sources, checks


def shadow_replay(events: list[tuple[int, int]], amount: int, liability: int,
                  hold: int | None) -> dict:
    if amount < 0 or liability <= 0 or (hold is not None and hold <= 0):
        raise ValueError('invalid shadow input')
    releases: list[int] = []
    active = kept = lost = 0
    first_lost = None
    for t, count in sorted(events):
        while releases and releases[0] <= t:
            heapq.heappop(releases)
            active -= 1
        for _ in range(count):
            if (active + 1) * liability <= amount:
                kept += 1
                active += 1
                if hold is not None:
                    heapq.heappush(releases, t + hold)
            else:
                lost += 1
                if first_lost is None:
                    first_lost = t
    return {'retained_opportunities': kept, 'account_blocked_opportunities': lost,
            'first_blocked_relative_daa': first_lost, 'end_unreleased_claims': active}


def envelope(plan: dict) -> list[dict]:
    rows = []
    a = coins(plan['account_minimum_msk'])
    reserve = coins(plan['legacy_reservation_msk'])
    for h in plan['account_hold_daa']:
        for target in plan['daily_target_claims']:
            if 720 % target:
                raise ValueError('targets must divide 720 for uniform integer-DAA fixture')
            gap = 720 // target
            n = ceildiv(h, gap)
            n_strict = ceildiv(h + 1, gap)
            oldn = ceildiv(plan['legacy_hold_daa_for_model'], gap)
            p = max(coins(plan['legacy_minimum_msk']), ceildiv(oldn * reserve * 1000, plan['legacy_utilization_permille']))
            for liab in plan['liability_msk']:
                lc = coins(liab)
                need = max(a, n * lc)
                rows.append({'evidence': 'D_CONDITIONAL', 'hold_daa': h,
                             'target_claims_per_720_daa': target, 'gap_daa': gap,
                             'overlap_claims': n, 'overlap_with_strict_release_boundary': n_strict,
                             'per_claim_liability_msk': liab,
                             'account_required_msk': money(need),
                             'additional_above_minimum_msk': money(max(0, need-a)),
                             'legacy_p_required_msk': money(p),
                             'total_excluding_fee_and_other_responsibilities_msk': money(need+p),
                             'max_liability_for_minimum_only_msk': money(a // n),
                             'fits_account_minimum_arithmetically': n * lc <= a,
                             'not_a_profile_or_node_gate': True})
    return rows


@dataclass(frozen=True)
class Simulation:
    name: str
    account: int | None
    production: int
    liability: int
    account_hold: int | None
    production_hold: int
    reserve: int
    target_daily: int
    horizon: int
    warmup: int
    production_minimum: int
    account_minimum: int
    topup_at: int | None = None
    topup_amount: int = 0


def simulate(c: Simulation) -> dict:
    """Synthetic offered arrivals, not a full node or adoption/heartbeat model.

    Scheduled lawful release events happen before the arrival at the same tick.
    Hard expiry is NOT generated as a release event by this model. Unresolved is
    represented by account_hold=None and remains reserved forever.
    """
    if c.target_daily <= 0 or 720 % c.target_daily or c.horizon <= 0 or not 0 <= c.warmup < c.horizon:
        raise ValueError('invalid simulation timing')
    if c.liability <= 0 or c.reserve <= 0 or c.production_hold <= 0:
        raise ValueError('invalid liability')
    gap = 720 // c.target_daily
    a = c.account
    pending_a: list[int] = []
    pending_p: list[int] = []
    active_a = active_p = peak_a = accepted = late = offered = blocked = 0
    reason = collections.Counter()
    both_flags = collections.Counter()
    previous_accepted = None
    max_gap = 0
    first_post_topup = None
    longest_rejects = current_rejects = 0
    topup_done = False
    for t in range(0, c.horizon, gap):
        if c.topup_at is not None and t >= c.topup_at and not topup_done:
            if a is None:
                raise ValueError('topup without account')
            a += c.topup_amount
            topup_done = True
        while pending_a and pending_a[0] <= t:
            heapq.heappop(pending_a)
            active_a -= 1
        while pending_p and pending_p[0] <= t:
            heapq.heappop(pending_p)
            active_p -= 1
        offered += 1
        flags = []
        if a is not None and a < c.account_minimum:
            flags.append('AccountMinimumMissing')
        if c.production < c.production_minimum:
            flags.append('LegacyMinimumMissing')
        if a is not None and (active_a + 1) * c.liability > a:
            flags.append('AccountLiabilityInsufficient')
        if (active_p + 1) * c.reserve > c.production * 500 // 1000:
            flags.append('LegacyProductionHeadroomInsufficient')
        for flag in flags:
            both_flags[flag] += 1
        if flags:
            blocked += 1
            reason[flags[0]] += 1
            current_rejects += 1
            longest_rejects = max(longest_rejects, current_rejects)
            continue
        current_rejects = 0
        accepted += 1
        late += t >= c.warmup
        if previous_accepted is not None:
            max_gap = max(max_gap, t-previous_accepted)
        previous_accepted = t
        if c.topup_at is not None and t >= c.topup_at and first_post_topup is None:
            first_post_topup = t
        active_p += 1
        heapq.heappush(pending_p, t+c.production_hold)
        if a is not None:
            active_a += 1
            peak_a = max(peak_a, active_a)
            if c.account_hold is not None:
                heapq.heappush(pending_a, t+c.account_hold)
            assert active_a*c.liability <= a
    return {'name': c.name, 'evidence': 'S_MODEL', 'account_initial_msk': None if c.account is None else money(c.account),
            'production_capital_msk': money(c.production), 'liability_msk': money(c.liability),
            'account_hold_daa': c.account_hold, 'legacy_hold_daa': c.production_hold,
            'target_per_720_daa': c.target_daily, 'horizon_daa': c.horizon, 'warmup_daa': c.warmup,
            'offered': offered, 'admitted': accepted, 'rejected': blocked,
            'post_warmup_admitted': late, 'post_warmup_per_720_daa': late*720/(c.horizon-c.warmup),
            'peak_account_live_claims': peak_a, 'first_failure_reason_counts': dict(reason),
            'all_failure_flags_counts': dict(both_flags),
            'max_inter_admission_gap_daa': max_gap,
            'longest_consecutive_rejected_opportunity_span_daa': longest_rejects*gap,
            'tail_since_last_admission_daa': None if previous_accepted is None else c.horizon-1-previous_accepted,
            'topup_at_daa': c.topup_at, 'topup_amount_msk': money(c.topup_amount),
            'first_admission_after_topup_daa': first_post_topup,
            'accepted_REAL_blocks': None, 'heartbeat_dependence_daa': None,
            'scope': 'Offered-work capital-only synthetic admission. No network, signatures, Q/B/R/F, adoption or heartbeat.'}


def model_sweep(plan: dict) -> tuple[list[dict], list[dict]]:
    common = dict(reserve=coins(plan['legacy_reservation_msk']), production_hold=plan['legacy_hold_daa_for_model'],
                  horizon=plan['synthetic_horizon_daa'], warmup=plan['synthetic_warmup_daa'],
                  production_minimum=coins(plan['legacy_minimum_msk']), account_minimum=coins(plan['account_minimum_msk']))
    rows = []
    amin = common['account_minimum']
    # Same offered workload and old-reserve fixture; capital is held fixed in B,
    # while C adds the new minimum and keeps the original production balance.
    for total in plan['total_capital_msk']:
        for target in [5, 30, 90]:
            for h in [3120, 10000]:
                for lc in [200, 1000, 12000]:
                    for mode, a, p in [('A_old_capital_only', None, coins(total)),
                                       ('B_total_capital_fixed', min(amin, coins(total)), max(0, coins(total)-amin)),
                                       ('C_add_new_minimum', amin, coins(total))]:
                        rows.append(simulate(Simulation(name=f'{mode}:C={total}:q={target}:H={h}:L={lc}',
                                                       account=a, production=p, liability=coins(lc), account_hold=h,
                                                       target_daily=target, **common)))
    # Explicit never-release, and external preauthorised cash injection cases.
    extras = [
        Simulation(name='UNRESOLVED_no_release_at_hard', account=amin, production=coins(200000),
                   liability=coins(12000), account_hold=None, target_daily=30, **common),
        Simulation(name='TOPUP_explicit_external_funds', account=amin, production=coins(200000),
                   liability=coins(12000), account_hold=10000, target_daily=30,
                   topup_at=1000, topup_amount=coins(5004000)-amin, **common),
    ]
    return rows, [simulate(c) for c in extras]


def pricing(plan: dict, groups: list[dict]) -> list[dict]:
    """Illustrative partial work costs; all monetary prices remain assumptions."""
    cfg = plan['pricing']
    out = []
    for g in groups:
        for scenario in cfg['scenarios']:
            cpu = Decimal(str(g['cpu_s']['p95_nearest_rank']))
            data = Decimal(g['da_bytes_max'] + g['filing_bytes_max']) / Decimal(2**30)
            c0 = (cpu / 3600 * Decimal(scenario['cpu_hour_msk'])
                  + data * Decimal(scenario['transfer_gib_msk'])
                  + data * Decimal(cfg['retention_daa_for_costing'])/720 * Decimal(scenario['storage_gib_nominal_day_msk'])
                  + Decimal(scenario['operations_msk_per_job']))
            finance = c0 * Decimal(cfg['annual_working_capital_rate']) * Decimal(cfg['panel_payment_wait_daa']) / (720*365)
            cost = rounded_money(c0+finance)
            fee = rounded_money(Decimal(cost)/COIN*(1+Decimal(cfg['margin_fraction'])))
            bounty = rounded_money(Decimal(cost)/COIN*Decimal(cfg['bounty_cost_multiple']))
            burn = rounded_money(Decimal(fee+bounty)/COIN*Decimal(cfg['burn_of_fee_plus_bounty_fraction']))
            partial = fee+bounty+burn
            eligible_conviction = g['local_court_convictions'] > 0 and g['wire_refusals'] == 0
            for mult in cfg['unique_job_multipliers']:
                out.append({'evidence': 'D_CONDITIONAL', 'label': g['label'], 'claim_case': g['claim_case'],
                            'via': g['via'], 'provider': g['provider'], 'n': g['n'], 'scenario': scenario['name'],
                            'cost_per_observed_job_msk': money(cost), 'candidate_work_fee_msk': money(fee),
                            'candidate_fixed_bounty_msk': money(bounty), 'candidate_nonreturnable_burn_msk': money(burn),
                            'candidate_single_job_charge_msk': money(partial),
                            'synthetic_unique_job_multiplier': mult,
                            'modeled_partial_liability_msk': money(mult*partial),
                            'net_worker_with_work_fee_only_msk': money(fee-cost),
                            'net_worker_and_reporter_success_msk': money(fee+bounty-cost) if eligible_conviction else None,
                            'local_success_bounty_eligible': eligible_conviction,
                            'source_wire_refused': bool(g['wire_refusals']),
                            'capacity_claims_at_100k': coins(100000)//(mult*partial),
                            'fits_30_per_720_at_H10000_PARTIAL_ONLY': mult*partial*ceildiv(10000,24) <= coins(100000),
                            'fits_5_per_720_at_H10000_PARTIAL_ONLY': mult*partial*ceildiv(10000,144) <= coins(100000),
                            'source_cpu_quantile_s': float(cpu),
                            'complete_F02_F03_Lc_derived': False,
                            'normal_reward_actual_solvency_or_participation_measured': False})
    return out


def env(repo: Path) -> dict:
    tools = {}
    for cmd in [['cargo','--version'], ['rustc','--version'], ['python3','--version'], ['gcc','--version']]:
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            tools[cmd[0]] = {'exit': r.returncode, 'output': (r.stdout+r.stderr)[:1500]}
        except (OSError, subprocess.TimeoutExpired) as e:
            tools[cmd[0]] = {'exit': None, 'error': str(e), 'available': False}
    return {'run_utc': datetime.now(timezone.utc).isoformat(), 'system': platform.platform(),
            'python': sys.version, 'cpu_count': os.cpu_count(), 'tools': tools,
            'fresh_native_node_run': False, 'fresh_full_LLM_replay': False,
            'blockers': ['No cargo or rustc executable in this runtime.',
                         'New Account Bond consensus/fence/fee wiring is not implemented in this snapshot.',
                         'Full artifacts used in archived OPV runs are not bundled; tiny fixtures are not substitutes.',
                         'No live source for new Account Bond REAL/heartbeat/inclusion or real Panel bills.'],
            'no_protocol_files_modified_by_measurement': True}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--repo', required=True, type=Path)
    ap.add_argument('--out', required=True, type=Path)
    ap.add_argument('--plan', type=Path, default=Path(__file__).with_name('plan.json'))
    args = ap.parse_args()
    repo, out = args.repo.resolve(), args.out.resolve()
    plan = json.loads(args.plan.read_text())
    out.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    samples, groups, runs, sources, checks = load_archive(repo)
    put(out/'archive-panel-samples.json', samples)
    put(out/'archive-panel-summary.json', groups)
    put(out/'archive-claim-summary.json', [{k:v for k,v in r.items() if k != 'accepted_events'} for r in runs])
    trace = []
    for r in runs:
        if r['account_identity_ambiguous']:
            continue
        for amount in plan['trace_account_amount_msk']:
            for lc in plan['liability_msk']:
                for h in [3120, 10000]:
                    trial = shadow_replay(r['accepted_events'], coins(amount), coins(lc), h)
                    checks.append({'name': f'trace_conservation:{r["run"]}:{r["subject_balance_msk"]}:{amount}:{lc}:{h}',
                                   'ok': trial['retained_opportunities']+trial['account_blocked_opportunities']==r['accepted_claims']})
                    trace.append({'evidence':'R_TRACE', 'run':r['run'], 'subject_balance_msk':r['subject_balance_msk'],
                                  'archived_accepted':r['accepted_claims'], 'account_msk':amount,
                                  'liability_msk':lc, 'assumed_lawful_release_daa':h, **trial})
    table(out/'trace-shadow.csv', trace)
    caps = envelope(plan)
    table(out/'capital-envelope.csv', caps)
    sweeps, extras = model_sweep(plan)
    table(out/'synthetic-admission.csv', sweeps)
    put(out/'synthetic-special-cases.json', extras)
    prices = pricing(plan, groups)
    table(out/'panel-cost-sensitivity.csv', prices)
    put(out/'environment.json', env(repo))
    sources += ['docs/adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md',
                'docs/rfc/0026-court-policy-master-contract.md',
                'scripts/drills/adr0176_stage1_measure.py', 'tools/opv-meas/src/verify.rs', 'tools/opv-meas/src/util.rs',
                'docs/evidence/palw/measurements/opv-measurements.md',
                'docs/evidence/palw/measurements/palw-claim-capacity-map-2026-09-26.md']
    manifests = [{'path': p, 'bytes': (repo/p).stat().st_size,
                  'sha256': hashlib.sha256((repo/p).read_bytes()).hexdigest()} for p in sorted(set(sources))]
    put(out/'source-manifest.json', manifests)
    put(out/'reaggregation-checks.json', checks)
    summary = {'evidence_scope': ['M_ARCHIVE','R_TRACE','S_MODEL','D_CONDITIONAL'],
               'archived_node_timeseries_runs': len(set(r['run'] for r in runs)), 'archive_input_json_files': 19, 'archive_static_study_files_not_timeseries_runs': 2, 'archived_subject_groups':len(runs),
               'archived_accepted_claims': sum(r['accepted_claims'] for r in runs),
               'ambiguous_groups_excluded_from_shadow_only': [r['run'] for r in runs if r['account_identity_ambiguous']],
               'archived_panel_samples':len(samples), 'panel_groups':len(groups),
               'local_court_conviction_samples':sum(g['local_court_convictions'] for g in groups),
               'wire_refusal_samples':sum(g['wire_refusals'] for g in groups),
               'shadow_trials':len(trace), 'capital_envelope_rows':len(caps),
               'synthetic_trials':len(sweeps)+len(extras), 'price_rows':len(prices),
               'checks_total':len(checks), 'checks_passed':sum(c['ok'] for c in checks),
               'analysis_tool_wall_seconds_NOT_panel_runtime': time.monotonic()-start,
               'fresh_node_or_new_bond_end_to_end_measurement': False,
               'activation_pass': False,
               'conclusion': '100k is not verified sufficient. L=12000 is incompatible with 5/30/90 claim targets under the long-hold fixtures. Some partial low-cost scenarios fit, but complete F02/F03 resources, real prices, participation and REAL/heartbeat remain unmeasured.'}
    put(out/'summary.json', summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    if any(not c['ok'] for c in checks):
        raise SystemExit('reaggregation consistency checks failed')

if __name__ == '__main__':
    main()
