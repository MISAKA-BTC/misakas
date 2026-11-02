from pathlib import Path
import json, runpy
root = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out = root / 'handoff/positive-court/legacy-native-history-20261102'
ns = runpy.run_path(str(out / 'operator-scripts/native-history-controller-20261102.py'))
first = json.loads((out / 'native-cert-first.json').read_text())
command = []
i = 0
while i < len(first['command']):
    if first['command'][i] == '--config':
        i += 2
        continue
    command.append(first['command'][i])
    i += 1
extra = first['extra_environment'] | {'CARGO_BUILD_JOBS': '1'}
record = ns['run']('native-cert-reopen-erased-futures', command, 1800, 4 * 1024**3, 16 * 1024**3, extra)
assert record['exit_code'] == 0 and record['stop_reason'] is None
assert record['test_counts'] == [(3, 0, 0)]
