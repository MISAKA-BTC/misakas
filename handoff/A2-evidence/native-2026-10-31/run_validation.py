#!/usr/bin/env python3
"""Record reproducible checks with exact source hashes, without claiming native acceptance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('kind', choices=['remote-rocks', 'default-off', 'client-check', 'journal-contract', 'source-focused'])
args = parser.parse_args()
common = ['--offline', '--config', 'profile.dev.debug=0', '--config', 'profile.test.debug=0',
          '--config', 'profile.dev.incremental=false', '--config', 'profile.test.incremental=false',
          '--target-dir', os.environ.get('MISAKA_A2_TARGET_DIR', '/tmp/misaka-a2-target'), '-j', '2']
commands = {
    'remote-rocks': ['cargo', 'test', '-p', 'misaka-palw-remote', '--lib', '--test', 'positive_native_source_v2',
                     '--features', 'positive-miner-rocksdb'],
    'default-off': ['cargo', 'test', '-p', 'misaka-palw-remote', '--lib', '--no-default-features'],
    'client-check': ['cargo', 'check', '-p', 'kaspad', '--lib', '--no-default-features', '--features', 'positive-miner-client'],
    'journal-contract': ['cargo', 'test', '-p', 'misaka-palw-positive-contract', '--test', 'case_journal'],
    'source-focused': ['cargo', 'test', '-p', 'misaka-palw-remote', '--test', 'positive_native_source_v2',
                       '--features', 'positive-miner-rocksdb'],
}
paths = [ROOT / x for x in ['Cargo.lock', 'Cargo.toml', 'consensus/core/Cargo.toml', 'consensus/core/src/lib.rs',
    'consensus/core/src/palw_state_v2.rs', 'consensus/core/src/palw_positive_rows_v2.rs',
    'consensus/core/tests/positive_source_v2.rs', 'kaspad/Cargo.toml', 'kaspad/src/lib.rs',
    'kaspad/src/palw_positive_miner_source_v2.rs', 'misaka/palw/remote/Cargo.toml',
    'misaka/palw/remote/tests/positive_native_source_v2.rs',
    'misaka/palw/positive-contract/tests/case_journal.rs',
    'docs/design/palw/positive-court/case-journal-vectors-r1.json']]
paths.append(HERE / 'run_validation.py')
paths += list((ROOT / 'consensus/core/src').glob('palw_positive_source*.rs'))
paths += list((ROOT / 'consensus/core/src/palw_positive_source_geometry_v2').glob('*.rs'))
paths += list((ROOT / 'misaka/palw/remote/src').glob('positive_miner_v2*.rs'))
paths += list((ROOT / 'misaka/palw/positive-contract/src').glob('*.rs'))
paths = sorted(set(paths))
def hashes():
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
command = commands[args.kind] + common
before = hashes()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
log = HERE / (args.kind + '.log')
started = time.time()
with log.open('wb') as out:
    completed = subprocess.run(command, cwd=ROOT, stdout=out, stderr=subprocess.STDOUT)
after = hashes()
report = {
    'head_at_start': head, 'command': command,
    'environment': {k: os.environ[k] for k in ['ROCKSDB_LIB_DIR', 'ROCKSDB_STATIC'] if k in os.environ},
    'exit_code': completed.returncode, 'elapsed_seconds': time.time() - started,
    'source_before': before, 'source_after': after, 'source_unchanged': before == after,
    'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(),
    'summaries': [line for line in log.read_text(errors='replace').splitlines() if line.startswith('test result:')],
    'scope': 'Local source geometry/finite preparation, real protected DB/process recovery, or client typecheck only. '
             'Not native admission, actual funding, current-view duty success, canonical reorg or IBD.',
}
(HERE / (args.kind + '.json')).write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({k: report[k] for k in ['exit_code', 'elapsed_seconds', 'source_unchanged', 'summaries']}))
raise SystemExit(completed.returncode if before == after else 99)
