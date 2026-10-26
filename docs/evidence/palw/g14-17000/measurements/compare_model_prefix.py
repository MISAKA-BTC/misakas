#!/usr/bin/env python3
"""Compare emitted chunk-end logits; preserve a bounded snapshot of a live run."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path


def digest(data):
    return hashlib.sha256(data).hexdigest()


def capture(folder, positions):
    saved = bytearray()
    chunks = []
    setup = None
    # Stop at the requested complete chunk, without consuming an unfinished line.
    with (folder / 'stdout.log').open('rb') as stream:
        for line in stream:
            saved.extend(line)
            if b'G14_MODEL_SETUP ' in line:
                setup = json.loads(line.split(b'G14_MODEL_SETUP ', 1)[1])
            if line.startswith(b'G14_MODEL_CHUNK '):
                row = json.loads(line.split(b' ', 1)[1])
                assert row['first'] == (chunks[-1]['end'] if chunks else 0)
                assert row['end'] > row['first']
                chunks.append(row)
                if row['end'] >= positions:
                    break
    assert setup and chunks and chunks[-1]['end'] == positions, 'prefix incomplete'
    return bytes(saved), setup, chunks


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('short', type=Path)
    p.add_argument('long', type=Path)
    p.add_argument('out', type=Path)
    a = p.parse_args()
    short = json.loads((a.short / 'raw.json').read_text())
    long = json.loads((a.long / 'raw.json').read_text())
    assert short['exit_code'] == 0 and short['status'] in ('RUNTIME_ONLY', 'UNDER_LOAD_RUNTIME_ONLY')
    assert digest((a.short / 'stdout.log').read_bytes()) == short['stdout_sha256']
    assert short['build']['binary']['sha256'] == long['build']['binary']['sha256']
    positions = short['completed_positions']
    s_bytes, s_setup, s_chunks = capture(a.short, positions)
    l_bytes, l_setup, l_chunks = capture(a.long, positions)
    for key in ('artifact_sha256', 'tokens_sha256', 'shape_profile_id', 'runtime', 'chunk_positions'):
        assert s_setup[key] == l_setup[key], key
    assert len(s_chunks) == len(l_chunks)
    mismatches = []
    for left, right in zip(s_chunks, l_chunks):
        assert (left['first'], left['end']) == (right['first'], right['end'])
        if left['logits_sha256'] != right['logits_sha256']:
            mismatches.append({'end': left['end'], 'short': left['logits_sha256'], 'long': right['logits_sha256']})
    a.out.mkdir(parents=True, exist_ok=False)
    (a.out / 'short-prefix.stdout.log').write_bytes(s_bytes)
    (a.out / 'long-prefix.stdout.log').write_bytes(l_bytes)
    result = {'scope': 'chunk_terminal_logits_prefix_comparison', 'arming_evidence': False,
              'all_positions_logits_compared': False, 'full_long_run_complete': long['status'] in ('RUNTIME_ONLY', 'UNDER_LOAD_RUNTIME_ONLY'),
              'observed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'source_commit': short['build']['source_commit'], 'binary_sha256': short['build']['binary']['sha256'],
              'positions_in_prefix': positions, 'chunk_endpoints_compared': len(s_chunks),
              'short_run_directory': str(a.short), 'long_run_directory': str(a.long),
              'short_reported_status': short['status'], 'long_reported_status': long['status'],
              'artifact_sha256': s_setup['artifact_sha256'], 'tokens_sha256': s_setup['tokens_sha256'],
              'prefix_sha256': {'short': digest(s_bytes), 'long': digest(l_bytes)},
              'mismatches': mismatches, 'match': not mismatches}
    (a.out / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return bool(mismatches)


if __name__ == '__main__':
    raise SystemExit(main())
