from pathlib import Path
import json
import os
import runpy

out = Path('/tmp/misaka-a0-native-ingress-integrated')
previous = json.loads(Path('/tmp/misaka-a0-bounded-case-archive-integrated/archive-native-store-runtime.json').read_text())
env = dict(previous['extra_environment'])
env['PATH'] = '/tmp/misaka-native-relay-libcxx/bin' + os.pathsep + os.environ['PATH']
run = runpy.run_path('handoff/positive-court/legacy-native-history-20261102/operator-scripts/native-history-controller-20261102.py')['run']
run.__globals__['out'] = out
common = ['--message-format=json-render-diagnostics', '--offline', '--locked']
core = ['cargo', 'test', *common, '-p', 'kaspa-consensus-core', '--lib']
single = ['--', '--test-threads=1']
cases = [
    ('ingress-claim-runtime', [*core, 'rooted_positive_claim_refuses_legacy_primary_and_kernel_holder_writes', *single], 3600),
    ('ingress-job-runtime', [*core, 'positive_job_projection_tracks_result_install_remove_and_cold_decode', *single], 900),
    ('ingress-staged-runtime', [*core, 'staged_alias_probes_cover_subject_namespaces_and_boundaries', *single], 900),
    ('ingress-legacy-index-runtime', [*core, 'native_claim_job_lookup_tracks_repeated_jobs_and_delta_undo', *single], 900),
    ('ingress-gate-c-runtime', [*core, 'p_terminal_case_requires_its_exact_result_scope_and_liability', *single], 900),
    ('ingress-clippy', ['cargo', 'clippy', *common, '-p', 'misaka-palw-positive-contract', '-p', 'kaspa-consensus-core', '-p', 'kaspa-consensus', '--lib', '--', '-D', 'warnings'], 2400),
    ('ingress-test-targets', ['cargo', 'check', *common, '-p', 'kaspa-consensus-core', '-p', 'kaspa-consensus', '--lib', '--tests'], 1800),
    ('ingress-archive-stores', ['cargo', 'test', *common, '-p', 'kaspa-consensus', '--lib', 'model::stores::', *single], 3600),
]
for label, command, timeout in cases:
    result = run(label, command, timeout, 1100 * 1024**2, 20 * 1024**3, env)
    if result['exit_code'] or (command[1] == 'test' and not any(n and not f for n, f, _ in result['test_counts'])):
        raise SystemExit(1)
