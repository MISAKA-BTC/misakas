from pathlib import Path
import json, runpy
root = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
legacy = root / 'handoff/positive-court/legacy-native-history-20261102'
ns = runpy.run_path(str(legacy / 'operator-scripts/native-history-controller-20261102.py'))
ns['run'].__globals__['out'] = root / 'handoff/positive-court/assignment-contract-20261102'
previous = json.loads((legacy / 'native-legacy-eight-erased-futures.json').read_text())
extra = previous['extra_environment']
base = ['cargo', 'test', '--message-format=json-render-diagnostics', '--offline', '--locked']
a = ns['run']('assignment-contract-first', base + ['-p', 'misaka-palw-positive-contract', '--test', 'assignment', '--', '--nocapture', '--test-threads=1'], 900, 4 * 1024**3, 16 * 1024**3, extra)
assert a['exit_code'] == 0 and a['test_counts'] == [(4, 0, 0)]
b = ns['run']('assignment-core-first', base + ['-p', 'kaspa-consensus-core', '--lib', '--', 'palw_positive_metadata_v2', 'positive_assignment_tests', '--nocapture', '--test-threads=1'], 1800, 4 * 1024**3, 16 * 1024**3, extra)
assert b['exit_code'] == 0 and len(b['test_counts']) == 1 and b['test_counts'][0][0] >= 10 and b['test_counts'][0][1:] == (0, 0)
