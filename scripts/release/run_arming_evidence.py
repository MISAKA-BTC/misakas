#!/usr/bin/env python3
"""Run the targeted arming-requirement suites ONCE and write one pin-form result JSON per run (lane ACT2, 2026-10-27).

Each run is a `cargo test` command (through buildslot). The JSON carries `exitCode`, `passed` (exit 0 AND at least one test ran, so an
empty filter never passes), the test counts, the source commit (the tree must be clean), the command and the log's sha256 — the form
`scripts/release/pin_arming_record.py` accepts. Output goes to `--out` (not under docs/). Nothing is pinned here.

Run (release window): BUILDSLOT_PRIORITY=release BUILDSLOT_SINCE=1760000000 PER_BUILD_GB=2 MAX_SLOTS=6 CARGO_BUILD_JOBS=8 \\
  python3 scripts/release/run_arming_evidence.py --jobs 6 --out scripts/release/arming-runs-17400
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
    # Split so each part fits inside 60 minutes (lane T, 2026-10-27); one JSON for the four parts.
    ('core-lib-tir_public_fold', [CORE + ['--lib', '--', 'palw_tir_public_fold_v1::conviction_e2e_tests::g14'],
                                  CORE + ['--lib', '--', 'palw_tir_public_fold_v1::conviction_e2e_tests::rooted_'],
                                  CORE + ['--lib', '--', 'palw_tir_public_fold_v1::conviction_e2e_tests::', '--skip', 'g14',
                                          '--skip', 'rooted_'],
                                  CORE + ['--lib', '--', 'palw_tir_public_fold_v1', '--skip', 'conviction_e2e_tests']]),
    ('core-lib-tir_public_claim_state', CORE + ['--lib', '--', 'palw_tir_public_claim_v1', 'palw_tir_public_state_v1',
                                                'palw_tir_public_source_v1']),
    ('core-lib-ruling_fences', CORE + ['--lib', '--', 'palw_ruling_fences_v1']),
    ('core-lib-kernel_route_onboarding_fold', CORE + ['--lib', '--', 'palw_kernel_route_fold_v1', 'palw_onboarding_fold_v1']),
    ('core-lib-provider_court', CORE + ['--lib', '--', 'palw_provider_court_v1', 'palw_provider_court_fold_v1']),
    ('core-lib-rfc0018', CORE + ['--lib', '--', 'palw_rfc0018_v1']),
    ('core-lib-model_tier', CORE + ['--lib', '--', 'palw_model_tier_v1']),
    ('core-adr0177_allocation_v3', CORE + ['--test', 'adr0177_allocation_v3']),
    ('core-court_capacity_v7', CORE + ['--test', 'g14_court_capacity', '--', '--exact', 'v7_restart_reorg_ibd_and_pruned_import_mid_queue_agree']),
    ('core-rfc0019_s11', [CORE + ['--lib', '--', 'palw_annual_catalog_v1'], CORE + ['--lib', '--', 'palw_model_tier_v1::tests']]),
    ('kernel-adversarial', [['-p', 'misaka-palw-kernel', '--test', 'k2_adversarial'],
                            ['-p', 'misaka-palw-kernel', '--test', 'k2_ledger_route', '--', 'a_proof_resubmitted_after_a_restart_or_an_ibd_replay_is_a_duplicate'],
                            ['-p', 'misaka-palw-kernel', '--test', 'econ_seal_veto', '--', 'one_withheld_seal_vetoes_every_attempt_whose_window_contains_it'],
                            ['-p', 'misaka-palw-kernel', '--test', 'k2_opv', '--', 'past_the_fence_a_withheld_seal'],
                            ['-p', 'misaka-palw-kernel', '--test', 'adv_c4_route', '--', 'a_producer_and_every_panel_seat_colluding_lose'],
                            ['-p', 'misaka-palw-kernel', '--test', 'k2_e2e', '--', 'a_challenge_drawn_before_the_evidence_was_bound_is_refused']]),
    ('consensus-lg14a_restart', ['-p', 'kaspa-consensus', '--lib', '--', 'lg14a_the_pursuit_survives_a_node_restart']),
    ('kaspad-lg14b_suite', ['-p', 'kaspad', '--lib', '--', 'lg14b_']),
    ('kaspad-lg14b_restart', ['-p', 'kaspad', '--lib', '--', '--exact',
                              'palw_legacy_held_e2e::lg14b_a_restarted_outsider_reacquires_answered_units_without_replaying_old_accusations']),
]
COUNT = re.compile(r'test result: \w+\. (\d+) passed; (\d+) failed')


def _part(env, part, test_threads, part_log=None):
    """One cargo test invocation through buildslot; returns (rc, log). The part's log is written as soon as it finishes."""
    cmd = [BUILDSLOT, 'cargo', 'test', '--offline', '--manifest-path', str(ROOT / 'Cargo.toml')] + part
    penv = dict(env)
    if test_threads:
        penv['RUST_TEST_THREADS'] = str(test_threads)
    p = subprocess.run(cmd, capture_output=True, text=True, env=penv, cwd=str(ROOT))
    log = '$ cargo test --offline ' + ' '.join(part) + '\n' + p.stdout + p.stderr
    if part_log is not None:
        part_log.write_text(log)
        failed = p.returncode != 0 or 'FAILED' in log
        print('part %s rc=%d%s' % (part_log.name, p.returncode, ' FAILED' if failed else ''), flush=True)
    return p.returncode, log


def _warm(env, runs):
    """One `cargo test --no-run` per package over every target the selected runs need (builds them once, up front)."""
    by_pkg = {}
    for _, args in runs:
        for part in (args if isinstance(args[0], list) else [args]):
            pkg = part[part.index('-p') + 1]
            head = part[:part.index('--')] if '--' in part else part
            targets = by_pkg.setdefault(pkg, set())
            if '--lib' in head:
                targets.add(('--lib',))
            if '--test' in head:
                targets.add(('--test', head[head.index('--test') + 1]))
    for pkg, targets in by_pkg.items():
        cmd = [BUILDSLOT, 'cargo', 'test', '--offline', '--no-run', '--manifest-path', str(ROOT / 'Cargo.toml'), '-p', pkg]
        for t in sorted(targets):
            cmd += list(t)
        p = subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=str(ROOT))
        print('warm %s rc=%d' % (pkg, p.returncode), flush=True)


def main():
    from concurrent.futures import ThreadPoolExecutor
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--only', action='append', default=[])
    ap.add_argument('--jobs', type=int, default=1, help='runs AND the parts of a multi-part run execute concurrently (via buildslot)')
    ap.add_argument('--no-warm', action='store_true', help='skip the up-front cargo test --no-run')
    ap.add_argument('--tir-test-threads', type=int, default=3, help='RUST_TEST_THREADS for the tir_public_fold parts')
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
    runs = [(rid, args) for rid, args in RUNS if not a.only or rid in a.only]
    if not a.no_warm:
        _warm(env, runs)
    started_at = {rid: time.time() for rid, _ in runs}
    with ThreadPoolExecutor(max_workers=max(1, a.jobs)) as pool:
        # Submission order (longest first): the tir_public_fold parts (part 3, the >1h45m one, first), then the other lib runs,
        # then the integration-test and kernel runs. The JSONs are still written in list order below.
        tasks = []
        for rid, args in runs:
            parts = args if isinstance(args[0], list) else [args]
            threads = a.tir_test_threads if rid == 'core-lib-tir_public_fold' else None
            for n, part in enumerate(parts):
                if rid == 'core-lib-tir_public_fold':
                    rank = (0, 0 if n == 2 else 1)
                elif '--lib' in part:
                    rank = (1, 0)
                else:
                    rank = (2, 0)
                tasks.append((rank, rid, n, part, threads))
        handles = {}
        for rank, rid, n, part, threads in sorted(tasks, key=lambda t: t[0]):
            handles[(rid, n)] = pool.submit(_part, env, part, threads, out / ('%s.part%d.log' % (rid, n)))
        futures = {}
        for rid, args in runs:
            parts = args if isinstance(args[0], list) else [args]
            futures[rid] = (parts, [handles[(rid, n)] for n in range(len(parts))])
        for rid, (parts, futs) in futures.items():
            results = [f.result() for f in futs]
            rc = next((r for r, _ in results if r), 0)
            log = ''.join(l for _, l in results)
            (out / (rid + '.log')).write_text(log)
            counts = [(int(x), int(y)) for x, y in COUNT.findall(log)]
            passed_n, failed_n = sum(c[0] for c in counts), sum(c[1] for c in counts)
            body = {
                'kind': 'misaka-arming-run/v1', 'run': rid,
                'command': ' && '.join(' '.join(['cargo', 'test', '--offline'] + part) for part in parts),
                'exitCode': rc, 'testsPassed': passed_n, 'testsFailed': failed_n,
                'passed': rc == 0 and passed_n > 0 and failed_n == 0 and 'FAILED' not in log, 'estimate': False,
                'sourceCommit': head, 'treeDirty': False, 'startedAt': started_at[rid],
                'seconds': round(time.time() - started_at[rid]), 'logSha256': hashlib.sha256(log.encode()).hexdigest(),
            }
            (out / (rid + '.json')).write_text(json.dumps(body, indent=2) + '\n')
            print('%s exit=%d passed=%d failed=%d -> %s' % (rid, rc, passed_n, failed_n, body['passed']), flush=True)


if __name__ == '__main__':
    main()
