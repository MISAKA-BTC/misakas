from pathlib import Path
import json, runpy
root = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out = root / 'handoff/positive-court/legacy-native-history-20261102'
ns = runpy.run_path(str(out / 'operator-scripts/native-history-controller-20261102.py'))
old = json.loads((out / 'native-cert-first.json').read_text())
command = []
i = 0
base = old['command'][:old['command'].index('--')]
while i < len(base):
    if base[i] == '--config':
        i += 2
        continue
    command.append(base[i])
    i += 1
filters = [
    'g14_kernel_route_a_covered_lie_is_convicted',
    'g14_kernel_route_a_chunked_object_is_signature_checked_at_the_completing_chunk',
    'g14r_kernel_v2_the_pursuit_resumes_after_a_node_restart_mid_filing',
    'g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption',
]
record = ns['run']('native-legacy-fraud-money-final', command + ['--'] + filters + ['--nocapture', '--test-threads=1'], 1200, 4 * 1024**3, 16 * 1024**3, old['extra_environment'] | {'CARGO_BUILD_JOBS': '1'})
assert record['exit_code'] == 0 and record['stop_reason'] is None
assert record['test_counts'] == [(5, 0, 0)]
