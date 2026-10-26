#!/usr/bin/env python3
"""Validate finished runtime logs and derive descriptive costs without editing them."""
import argparse
import hashlib
import json
import math
import re
import statistics
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize(folder):
    raw = json.loads((folder / 'raw.json').read_text())
    assert raw['status'] in ('RUNTIME_ONLY', 'UNDER_LOAD_RUNTIME_ONLY'), raw['status']
    assert raw['exit_code'] == 0
    for name in ('stdout', 'stderr'):
        assert sha(folder / (name + '.log')) == raw[name + '_sha256']
    lines = (folder / 'stdout.log').read_text().splitlines()
    # Bind the collector's JSON to the actual harness records, including the
    # libtest prefix emitted on the first line. Hashes alone do not establish
    # agreement between two independently parsed representations.
    for key, prefix in [('setup', 'G14_MODEL_SETUP '), ('result', 'G14_MODEL_RESULT ')]:
        records = [json.loads(line.split(prefix, 1)[1]) for line in lines if prefix in line]
        assert len(records) == 1 and records[0] == raw[key], f'{key} record mismatch'
    setup = raw['setup']
    expected = int(raw['env']['G14_A16_POSITIONS'])
    chunk_size = int(raw['env']['G14_A16_CHUNK'])
    assert expected == setup['positions_requested'] <= setup['declared_max_context']
    assert 1 <= chunk_size <= 32 and chunk_size == setup['chunk_positions']
    chunks = [json.loads(line.split(' ', 1)[1]) for line in lines
              if line.startswith('G14_MODEL_CHUNK ')]
    result = raw['result']
    position = 0
    for chunk in chunks:
        assert chunk['first'] == position
        assert chunk['end'] == min(position + chunk_size, expected) > position
        assert chunk['wall_ns'] > 0 and chunk['logical_trace_i32_bytes'] > 0
        assert re.fullmatch(r'[0-9a-f]{64}', chunk['logits_sha256'])
        position = chunk['end']
    assert position == result['positions_executed'] == raw['completed_positions'] == expected
    if chunks:
        assert chunks[-1]['logits_sha256'] == result['last_logits_sha256'], 'final logits mismatch'
        assert chunks[-1]['cache_resident_bytes'] == result['cache_resident_bytes'], 'final cache mismatch'
    total_ns = sum(c['wall_ns'] for c in chunks)
    assert total_ns == result['total_forward_wall_ns']
    trace_bytes = sum(c['logical_trace_i32_bytes'] for c in chunks)
    assert trace_bytes == result['logical_trace_i32_bytes_total']
    err = (folder / 'stderr.log').read_text()
    match = re.search(r'([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys', err)
    assert match, 'missing process timing'
    # Within-run chunks have growing context and correlated load. Their p95 is
    # descriptive; it is NOT a repeated-claim latency bound or a capacity value.
    windows = []
    for first in range(0, position, 1024):
        selected = [c for c in chunks if first <= c['first'] < first + 1024]
        times = sorted(c['wall_ns'] for c in selected)
        windows.append({'first': first, 'end': selected[-1]['end'],
                        'chunks': len(selected), 'wall_seconds': sum(times) / 1e9,
                        'median_chunk_seconds': statistics.median(times) / 1e9,
                        'p95_chunk_seconds': times[math.ceil(.95 * len(times)) - 1] / 1e9})
    return {'schema': 1, 'scope': 'real_a16_runtime_only', 'arming_evidence': False,
            'minimum_hardware': False, 'nine_b_measured': False,
            'source_commit': raw['build']['source_commit'],
            'binary_sha256': raw['build']['binary']['sha256'],
            'input_log_sha256': {name: sha(folder / name) for name in ['raw.json', 'stdout.log', 'stderr.log']},
            'status': raw['status'], 'positions': position, 'chunks': len(chunks),
            'forward_wall_seconds': total_ns / 1e9,
            'process_real_seconds': float(match[1]), 'process_user_seconds': float(match[2]),
            'process_system_seconds': float(match[3]),
            'monitor_elapsed_seconds_including_polling': raw['wall_seconds'],
            'peak_rss_bytes': raw['rss_bytes'],
            'max_cache_allocation_bytes': max((c['cache_resident_bytes'] for c in chunks), default=0),
            'logical_transient_trace_bytes': trace_bytes, 'serialized_da_bytes': None,
            'load_1m_min': min([raw['load_before'][0], raw['load_after'][0]] + [r['load'][0] for r in raw['telemetry']]),
            'load_1m_max': max([raw['load_before'][0], raw['load_after'][0]] + [r['load'][0] for r in raw['telemetry']]),
            'windows_1024_positions': windows}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.folder), indent=2))
