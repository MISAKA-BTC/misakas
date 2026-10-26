#!/usr/bin/env python3
"""Validate captured native budget measurements and summarize each fixture condition."""
import argparse
import hashlib
import json
import math
import re
import statistics
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stats(values):
    values = sorted(values)
    return {'samples': len(values), 'median': statistics.median(values),
            'p95_nearest_rank': values[math.ceil(.95 * len(values)) - 1],
            'max': values[-1]}


def summarize(folder):
    raw = json.loads((folder / 'raw.json').read_text())
    assert raw['status'] in ('FIXTURE_ONLY', 'UNDER_LOAD_FIXTURE_ONLY')
    assert raw['build']['stage'] == 'BUILT' and raw['build']['source_unchanged']
    assert raw['arming_evidence'] is False and raw['minimum_hardware'] is False
    groups, pairs, seen = {}, {}, set()
    for row in raw['samples']:
        label = row['label']
        assert label not in seen
        seen.add(label)
        stdout = folder / (label + '.stdout.log')
        stderr = folder / (label + '.stderr.log')
        assert digest(stdout) == row['stdout_sha256']
        assert digest(stderr) == row['stderr_sha256']
        records = re.findall(r'^(?:test \S+ \.\.\. )?G14_CAP_MEASURE (\{.*\})$', stdout.read_text(), re.M)
        assert len(records) == 1 and json.loads(records[0]) == row['measurement']
        m = row['measurement']
        assert row['exit_code'] == 0 and m['schema'] in (1, 2, 3)
        for key in ('arming_evidence', 'production_cap_measured', 'continuous_adversary_inclusion_guaranteed', 'gate_c_measured', 'real_model_measured'):
            assert m[key] is False
        assert m['fixture_only'] and m['test_policy']
        count, positions, n = m['claims'], m['positions'], m['n']
        assert count in (1, 2, 4, 6) and positions in (6, 64, 65) and n in (0, 1)
        assert all(int(row['env'][key]) == value for key, value in [('G14_CAP_CLAIMS', count), ('G14_CAP_POSITIONS', positions), ('G14_CAP_N', n)])
        assert m['test_adjudication_cap'] in (4, 64)
        assert m['reserved_prosecution_runs'] == m['test_adjudication_cap'] // 2
        if m['schema'] < 3:
            assert m['test_adjudication_cap'] == 4
        else:
            assert m['test_adjudication_cap'] == int(row['env']['G14_CAP_ADJUDICATION_CAP'])
            assert m['production_cap_configured'] == (m['test_adjudication_cap'] == 64)
            assert m['early_anchor_window_test_policy'] == bool(int(row['env']['G14_MEASURE_EARLY_ANCHOR_WINDOW']))
        assert len(m['claim_states_at_measurement']) == count
        if m['schema'] >= 2:
            assert len(m['material_profiles']) == count
            for profile in m['material_profiles']:
                assert profile['job_token_total'] == positions
                assert profile['trace_positions'] == positions - 1
                assert profile['fault_position'] == (63 if positions == 65 else 1)
                assert profile['published_tensor_count'] > 0 and profile['published_tensor_wire_bytes'] > 0
                assert profile['disk_or_network_fetch_measured'] is False
                assert profile['includes_artifact_parameters'] is False
                assert profile['includes_transport_framing'] is False
        else:
            assert positions != 65
        cap = min(m['test_adjudication_cap'], m['work_cap'] // m['per_claim_worst_work'])
        assert m['calculated_adjudications'] == cap
        pair = re.sub(r'-n[01]-', '-', label)
        pairs.setdefault(pair, set()).add(n)
        if n == 0:
            assert m['rounds'] == [] and m['outsider'] is None
        else:
            assert len(m['rounds']) == 2 and m['outsider']['convicted']
            for index, r in enumerate(m['rounds']):
                charged = min(count, cap)
                assert r['round'] == index and r['adjudicated'] == charged and r['over_budget'] == count - charged
                assert len(r['dismissal_fees']) == count
                assert sum(fee > 0 for fee in r['dismissal_fees']) == charged
                assert all(fee >= 0 for fee in r['dismissal_fees'])
                assert r['filing']['charged_adjudications'] == charged
                assert r['filing']['charged_work'] == charged * m['per_claim_worst_work']
                assert r['filing']['objects'] == count and r['filing']['transactions'] == count
                assert r['filing']['daa_after'] > r['filing']['daa_before']
            assert m['rounds'][1]['filing']['budget_blue'] > m['rounds'][0]['filing']['budget_blue']
            assert m['outsider']['filing']['budget_blue'] > m['rounds'][1]['filing']['budget_blue']
            assert m['outsider']['filing']['charged_adjudications'] == 1
        groups.setdefault((positions, count, n), []).append(row)
    assert pairs and all(values == {0, 1} for values in pairs.values())
    assert raw['status'] == ('UNDER_LOAD_FIXTURE_ONLY' if any(r['under_load'] for r in raw['samples']) else 'FIXTURE_ONLY')
    result = {'status': raw['status'], 'source_commit': raw['build']['source_commit'],
              'binary_sha256': raw['build']['binary']['sha256'], 'processes': len(seen),
              'arming_evidence': False, 'minimum_hardware': False, 'production_cap_measured': False,
              'load_1m_min': min(min(r['load_before'][0], r['load_after'][0]) for r in raw['samples']),
              'load_1m_max': max(max(r['load_before'][0], r['load_after'][0]) for r in raw['samples']),
              'quantile_scope': 'Descriptive process samples; no population latency bound. N=0 includes setup; not subtracted from N=1.',
              'groups': []}
    for (positions, count, n), rows in sorted(groups.items()):
        entry = {'positions': positions, 'claims': count, 'n': n,
                 'process_real_seconds': stats([r['real_seconds'] for r in rows]),
                 'peak_rss_bytes': stats([r['rss_bytes'] for r in rows]),
                 'setup_wall_ms': stats([r['measurement']['setup_wall_ns'] / 1e6 for r in rows])}
        if all(r['measurement']['schema'] >= 3 for r in rows):
            caps = {r['measurement']['test_adjudication_cap'] for r in rows}
            anchors = {r['measurement']['early_anchor_window_test_policy'] for r in rows}
            assert len(caps) == len(anchors) == 1, 'mixed configuration within condition'
            entry['test_adjudication_cap'] = caps.pop()
            entry['early_anchor_window_test_policy'] = anchors.pop()
        if all(r['measurement']['schema'] >= 2 for r in rows):
            profiles = [profile for r in rows for profile in r['measurement']['material_profiles']]
            entry['trace_positions'] = sorted({p['trace_positions'] for p in profiles})
            entry['fault_positions'] = sorted({p['fault_position'] for p in profiles})
            entry['published_tensor_count_per_claim'] = sorted({p['published_tensor_count'] for p in profiles})
            entry['published_tensor_wire_bytes_per_claim'] = sorted({p['published_tensor_wire_bytes'] for p in profiles})
        if n:
            for index in range(2):
                entry[f'round_{index}_filing_wall_ms'] = stats([r['measurement']['rounds'][index]['filing']['wall_ns'] / 1e6 for r in rows])
            for key in ('local_check_wall_ns',):
                entry['outsider_local_check_wall_ms'] = stats([r['measurement']['outsider'][key] / 1e6 for r in rows])
            entry['outsider_filing_wall_ms'] = stats([r['measurement']['outsider']['filing']['wall_ns'] / 1e6 for r in rows])
            entry['adjudicated_per_round'] = sorted({r['measurement']['rounds'][0]['adjudicated'] for r in rows})
            entry['over_budget_per_round'] = sorted({r['measurement']['rounds'][0]['over_budget'] for r in rows})
            entry['filing_transaction_borsh_bytes'] = sorted({r['measurement']['rounds'][0]['filing']['transaction_borsh_bytes'] for r in rows})
        result['groups'].append(entry)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.directory), indent=2))
