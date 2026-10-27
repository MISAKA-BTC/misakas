#!/usr/bin/env python3
"""Run the targeted arming-requirement suites ONCE and write one pin-form result JSON per run (lane ACT2, 2026-10-27).

Each run is a `cargo test` command (through buildslot). The JSON carries `exitCode`, `passed` (exit 0 AND at least one test ran, so an
empty filter never passes), the test counts, the source commit (the tree must be clean), the command and the log's sha256 — the form
`scripts/release/pin_arming_record.py` accepts. Output goes to `--out` (not under docs/). Nothing is pinned here.

Run: python3 scripts/release/run_arming_evidence.py --out scripts/release/arming-runs-17400
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BUILDSLOT = os.path.expanduser('~/Downloads/MISAKA-wt-b/buildslot.sh')
CORE = ['-p', 'kaspa-consensus-core']

# (run id, cargo test args) — one per arming requirement slot class (R), see the ACT2 leftover table.
RUNS = [
    ('core-g14_close_legacy', CORE + ['--test', 'g14_close_legacy']),
    ('core-g14_court_capacity', CORE + ['--test', 'g14_court_capacity']),
    ('core-g14_close_gates-T-N8', CORE + ['--test', 'g14_close_gates', '--', '--exact',
                                          'legacy_route_claim_without_a_g14_path_earns_nothing_past_14000']),
    ('core-g14_close_gates-T-N10', CORE + ['--test', 'g14_close_gates', '--', '--exact',
                                           'legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed']),
    ('core-lib-staged_court', CORE + ['--lib', '--', 'palw_staged_court_fold_v1', 'palw_staged_court_state_v1',
                                      'palw_staged_route_forms_v1']),
    ('core-lib-tir_public_fold', CORE + ['--lib', '--', 'palw_tir_public_fold_v1']),
    ('core-lib-tir_public_claim_state', CORE + ['--lib', '--', 'palw_tir_public_claim_v1', 'palw_tir_public_state_v1',
                                                'palw_tir_public_source_v1']),
    ('core-lib-ruling_fences', CORE + ['--lib', '--', 'palw_ruling_fences_v1']),
    ('core-lib-kernel_route_onboarding_fold', CORE + ['--lib', '--', 'palw_kernel_route_fold_v1', 'palw_onboarding_fold_v1']),
    ('core-lib-provider_court', CORE + ['--lib', '--', 'palw_provider_court_v1', 'palw_provider_court_fold_v1']),
    ('core-lib-rfc0018', CORE + ['--lib', '--', 'palw_rfc0018_v1']),
    ('core-lib-model_tier', CORE + ['--lib', '--', 'palw_model_tier_v1']),
    ('core-adr0177_allocation_v3', CORE + ['--test', 'adr0177_allocation_v3']),
    ('consensus-lg14a_restart', ['-p', 'kaspa-consensus', '--lib', '--', 'lg14a_the_pursuit_survives_a_node_restart']),
    ('kaspad-lg14b_suite', ['-p', 'kaspad', '--lib', '--', 'lg14b_']),
    ('kaspad-lg14b_restart', ['-p', 'kaspad', '--lib', '--', '--exact',
                              'palw_legacy_held_e2e::lg14b_a_restarted_outsider_reacquires_answered_units_without_replaying_old_accusations']),
]
COUNT = re.compile(r'test result: \w+\. (\d+) passed; (\d+) failed')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--only', action='append', default=[])
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    head = subprocess.run(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], capture_output=True, text=True, check=True).stdout.strip()
    dirty = subprocess.run(['git', '-C', str(ROOT), 'status', '--porcelain', '--untracked-files=no'], capture_output=True,
                           text=True, check=True).stdout.strip()
    if dirty:
        sys.exit('the tree is not clean: a run must name the commit it ran')
    env = dict(os.environ, CARGO_INCREMENTAL='0', TMPDIR='/private/tmp', RUST_MIN_STACK='67108864',
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
    for rid, args in RUNS:
        if a.only and rid not in a.only:
            continue
        cmd = [BUILDSLOT, 'cargo', 'test', '--offline', '--manifest-path', str(ROOT / 'Cargo.toml')] + args
        started = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=str(ROOT))
        log = p.stdout + p.stderr
        (out / (rid + '.log')).write_text(log)
        counts = [(int(x), int(y)) for x, y in COUNT.findall(log)]
        passed_n, failed_n = sum(c[0] for c in counts), sum(c[1] for c in counts)
        body = {
            'kind': 'misaka-arming-run/v1', 'run': rid, 'command': ' '.join(['cargo', 'test', '--offline'] + args),
            'exitCode': p.returncode, 'testsPassed': passed_n, 'testsFailed': failed_n,
            'passed': p.returncode == 0 and passed_n > 0 and failed_n == 0, 'estimate': False,
            'sourceCommit': head, 'treeDirty': False, 'startedAt': started, 'seconds': round(time.time() - started),
            'logSha256': hashlib.sha256(log.encode()).hexdigest(),
        }
        (out / (rid + '.json')).write_text(json.dumps(body, indent=2) + '\n')
        print('%s exit=%d passed=%d failed=%d -> %s' % (rid, p.returncode, passed_n, failed_n, body['passed']), flush=True)


if __name__ == '__main__':
    main()
