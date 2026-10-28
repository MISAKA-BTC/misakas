#!/usr/bin/env python3
"""Reaggregate recorded observations; no node, sender, network or rate calibration."""
import argparse
import collections
import hashlib
import json
import statistics
from pathlib import Path


def audit(path):
    raw = path.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == "579c9593e1032ca859a9583cabc3c4375d73463f87bd9db50be529c6200cbd2c", "input differs from the pinned 799a058fa source log"
    records = [json.loads(line) for line in raw.splitlines() if line.strip()]
    counts = collections.Counter(row['ev'] for row in records)
    blocks = [row for row in records if row['ev'] == 'block']
    ticks = [row for row in records if row['ev'] == 'tick']
    seen = [row for row in records if row['ev'] == 'mempool']
    included = [row for row in records if row['ev'] == 'included']
    assert len({row['txid'] for row in seen}) == len(seen), 'duplicate first-mempool txid'
    assert len({row['txid'] for row in included}) == len(included), 'duplicate included txid'
    mempool_by_id = {row['txid']: row for row in seen}
    by_block = collections.defaultdict(list)
    for row in blocks:
        by_block[row['block']].append(row)
    fields = ['daa', 'blue', 'kind', 'txs', 'tx_bytes', 'meas_txs']
    for observations in by_block.values():
        assert all([row[k] for k in fields] == [observations[0][k] for k in fields]
                   for row in observations), 'same recorded block ID has conflicting observations'
    for row in included:
        first = mempool_by_id[row['txid']]
        assert row['label'] == first['label'], 'cross-stage tx pairing'
        assert row['submit_daa'] == first['daa'], 'submit_daa is not first mempool observation'
        assert row['latency_daa'] == row['include_daa'] - row['submit_daa'], 'latency arithmetic differs'
        assert any(block['daa'] == row['include_daa'] and block['meas_txs'] > 0
                   for block in by_block[row['block']]), 'included event lacks matching observed header'
    phases = {}
    for label in dict.fromkeys(row['label'] for row in records):
        pt = [row for row in ticks if row['label'] == label]
        pm = [row for row in seen if row['label'] == label]
        pi = [row for row in included if row['label'] == label]
        phases[label] = {
            'tick_observations': len(pt),
            'first_tick_daa': pt[0]['daa'], 'last_tick_daa': pt[-1]['daa'],
            'observed_tick_span_seconds': round(pt[-1]['t'] - pt[0]['t'], 6),
            'first_mempool_txids': len(pm), 'included_txids': len(pi),
            'recorded_latency_daa': [row['latency_daa'] for row in pi],
            'recorded_latency_seconds': [row['latency_s'] for row in pi],
            'measured_filter_pending_count_max': max(row['mempool_meas_pending'] for row in pt),
        }
    maximum = max(row['tx_bytes'] for row in blocks)
    without_measured = [row for row in blocks if row['meas_txs'] == 0]
    changes = []
    for row in ticks:
        if not changes or row['daa'] != changes[-1]['daa']:
            changes.append(row)
    intervals = [{'from_daa': a['daa'], 'to_daa': b['daa'], 'delta_daa': b['daa']-a['daa'],
                  'seconds': round(b['t']-a['t'], 6)} for a, b in zip(changes, changes[1:])]
    return {
        'kind': 'reaggregation of recorded observations only; not new node execution or capacity calibration',
        'source_commit': '799a058fa97d5a1c270b98b4bdbfed731dd84fb3',
        'input_file': path.name, 'input_sha256': hashlib.sha256(raw).hexdigest(),
        'line_count': len(raw.splitlines()), 'event_counts': dict(sorted(counts.items())),
        'phases': phases,
        'blocks': {
            'observation_rows': len(blocks), 'distinct_recorded_block_ids': len(by_block),
            'id_note': 'recorded block IDs are 16-hex prefixes, not independently fetched full hashes',
            'duplicated_recorded_ids': {key: [row['label'] for row in value] for key, value in by_block.items() if len(value)>1},
            'kind_counts': dict(collections.Counter(row['kind'] for row in blocks)),
            'median_all_tx_bytes_observation_rows': statistics.median(row['tx_bytes'] for row in blocks),
            'maximum_all_tx_bytes': maximum,
            'maximum_rows': [row for row in blocks if row['tx_bytes']==maximum],
            'maximum_all_tx_bytes_where_meas_txs_is_zero': max(row['tx_bytes'] for row in without_measured),
            'floor_only_tx_bytes': None,
        },
        'tick_daa_transition_intervals': {
            'method': 'consecutive first ticks of each new observed DAA across the concatenated file',
            'count': len(intervals), 'delta_counts': dict(collections.Counter(row['delta_daa'] for row in intervals)),
            'median_seconds': round(statistics.median(row['seconds'] for row in intervals), 6),
            'maximum_seconds': max(row['seconds'] for row in intervals),
        },
        'paired_header_and_observation_daa': [
            {'txid': row['txid'], 'label': row['label'], 'first_mempool_daa': row['submit_daa'],
             'carrying_header_daa': row['include_daa'],
             'observed_sample_daa': next(block['sample_daa'] for block in by_block[row['block']]
                                         if block['label']==row['label']),
             'reported_header_latency_daa': row['latency_daa']}
            for row in included
        ],
        'limits': [
            'No sender output or RPC-submit timestamps: submit_daa matches first mempool observation.',
            'Included DAA matches an observed sink header; not proof of selected-chain acceptance time.',
            'No per-transaction serialized bytes or mass; block kind does not classify every transaction.',
            'The maximum total-tx-byte rows include measured transactions; floor-only maximum cannot be isolated.',
            'No saturated offered load, inclusion service-rate lower bound, p99 guarantee or court reservation calibration.',
            'Reported 17408-byte payload setting is in the source Markdown; no transaction bodies are present in this log.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = audit(args.input)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'rows': result['line_count'], 'events': result['event_counts'],
                      'block_observations': result['blocks']['observation_rows'],
                      'distinct_recorded_block_ids': result['blocks']['distinct_recorded_block_ids']}))


if __name__ == '__main__':
    main()
