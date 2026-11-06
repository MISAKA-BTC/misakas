from pathlib import Path
import json, runpy
root = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
legacy = root / 'handoff/positive-court/legacy-native-history-20261102'
ns = runpy.run_path(str(legacy / 'operator-scripts/native-history-controller-20261102.py'))
ns['run'].__globals__['out'] = root / 'handoff/positive-court/assignment-body-20261102'
extra = json.loads((legacy / 'native-legacy-eight-erased-futures.json').read_text())['extra_environment']
common = ['--message-format=json-render-diagnostics', '--offline', '--locked']
a = ns['run']('assignment-body-library-clippy-first', ['cargo', 'clippy'] + common + ['-p', 'misaka-palw-positive-contract', '-p', 'kaspa-consensus-core', '--lib', '--', '-D', 'warnings'], 1800, 4 * 1024**3, 16 * 1024**3, extra)
assert a['exit_code'] == 0
b = ns['run']('assignment-body-contract-first', ['cargo', 'test'] + common + ['-p', 'misaka-palw-positive-contract', '--test', 'assignment', '--', '--nocapture', '--test-threads=1'], 900, 4 * 1024**3, 16 * 1024**3, extra)
assert b['exit_code'] == 0 and b['test_counts'] == [(8, 0, 0)]
c = ns['run']('assignment-body-core-first', ['cargo', 'test'] + common + ['-p', 'kaspa-consensus-core', '--test', 'positive_assignment_body_v2', '--test', 'positive_assignment_fence_v2', '--', '--nocapture', '--test-threads=1'], 1800, 4 * 1024**3, 16 * 1024**3, extra)
assert c['exit_code'] == 0 and sorted(c['test_counts']) == [(4, 0, 0), (5, 0, 0)]
d = ns['run']('assignment-body-contract-test-clippy-first', ['cargo', 'clippy'] + common + ['-p', 'misaka-palw-positive-contract', '--test', 'assignment', '--', '-D', 'warnings'], 900, 4 * 1024**3, 16 * 1024**3, extra)
assert d['exit_code'] == 0
e = ns['run']('assignment-body-core-test-clippy-first', ['cargo', 'clippy'] + common + ['-p', 'kaspa-consensus-core', '--test', 'positive_assignment_body_v2', '--test', 'positive_assignment_fence_v2', '--', '-D', 'warnings'], 900, 4 * 1024**3, 16 * 1024**3, extra)
assert e['exit_code'] == 0
