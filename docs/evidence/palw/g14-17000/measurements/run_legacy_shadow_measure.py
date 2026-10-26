#!/usr/bin/env python3
"""Run the honest legacy/kernel same-work preflight from a pinned binary.

Run under buildslot. This does not cover attack pairs, Gate C, honest per-seat
verification costs, production parameters or a public network campaign.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

TEST = 'pipeline::virtual_processor::tests::g14_kernel_route_e2e::canonical::legacy_shadow::g14_legacy_kernel_honest_shadow_measure'


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for data in iter(lambda: stream.read(1 << 20), b''):
            digest.update(data)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-record', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    build = json.loads(args.build_record.read_text())
    assert build['stage'] == 'BUILT' and build['source_unchanged'] is True
    assert sha(build['binary']['path']) == build['binary']['sha256']
    args.out.mkdir(parents=True, exist_ok=False)
    command = ['/usr/bin/time', '-l', build['binary']['path'], TEST,
               '--exact', '--ignored', '--nocapture', '--test-threads=1']
    env = dict(os.environ, TMPDIR='/private/tmp', RUST_BACKTRACE='1')
    report = dict(build=build, runner_sha256=sha(__file__), command=command,
                  env={key: env[key] for key in ['TMPDIR', 'RUST_BACKTRACE']},
                  status='RUNNING', arming_evidence=False, minimum_hardware=False,
                  timeout_seconds=1800, load_before=os.getloadavg())
    record = args.out / 'raw.json'

    def save():
        record.write_text(json.dumps(report, indent=2) + '\n')

    save()
    start = time.monotonic()
    with (args.out / 'stdout.log').open('w') as stdout, (args.out / 'stderr.log').open('w') as stderr:
        child = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
        report['pid'] = child.pid
        save()
        try:
            rc = child.wait(timeout=report['timeout_seconds'])
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            rc = 124
    report.update(exit_code=rc, wall_seconds=time.monotonic()-start,
                  load_after=os.getloadavg(), stdout_sha256=sha(args.out/'stdout.log'),
                  stderr_sha256=sha(args.out/'stderr.log'))
    raw = (args.out/'stdout.log').read_text()
    lines = re.findall(r'^(?:test \S+ \.\.\. )?G14_LEGACY_SHADOW (\{.*\})$', raw, re.MULTILINE)
    report['measurement'] = json.loads(lines[0]) if len(lines) == 1 else None
    report['under_load'] = max(report['load_before'][0], report['load_after'][0]) > 5
    err = (args.out/'stderr.log').read_text()
    for key, pattern in [('rss_bytes', r'(\d+)\s+maximum resident set size'),
                         ('user_seconds', r'([\d.]+)\s+user'), ('sys_seconds', r'([\d.]+)\s+sys')]:
        match = re.search(pattern, err)
        report[key] = float(match[1]) if match else None
    row = report['measurement'] or {}
    valid = (row.get('scope') == 'legacy_panel_and_kernel_panel1_same_work'
             and row.get('same_program_weights_prompt_output') is True
             and row.get('kernel_seats') == 1 and row.get('legacy_seats', 0) > 0
             and isinstance(row.get('legacy_final_daa'), int)
             and isinstance(row.get('kernel_final_daa'), int)
             and row.get('arming_evidence') is False)
    passed = rc == 0 and len(lines) == 1 and valid
    report['status'] = ('UNDER_LOAD_HONEST_PREFLIGHT_ONLY' if report['under_load'] else 'HONEST_PREFLIGHT_ONLY') if passed else 'FAILED'
    save()
    print(json.dumps(report), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
