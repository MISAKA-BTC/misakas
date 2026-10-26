#!/usr/bin/env python3
"""Fresh-process full-cap court/transport experiment using independently funded UTXOs."""
import argparse
import datetime
import hashlib
import json
import os
import re
import signal
import subprocess
import time
from pathlib import Path

TEST = 'pipeline::virtual_processor::tests::g14_kernel_route_e2e::canonical::saturation_measurement::g14_kernel_saturation_measure'


def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for data in iter(lambda: f.read(1 << 20), b''):
            h.update(data)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build-record', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--allow-under-load', action='store_true')
    p.add_argument('--claims', type=int, action='append', choices=[2, 12])
    p.add_argument('--positions', type=int, action='append', choices=[6, 65])
    a = p.parse_args()
    assert a.runs >= 1
    build = json.loads(a.build_record.read_text())
    assert build['stage'] == 'BUILT' and build['source_unchanged'] is True
    binary = build['binary']['path']
    assert sha(binary) == build['binary']['sha256']
    a.out.mkdir(parents=True, exist_ok=False)
    report = {'build': build, 'runner_sha256': sha(__file__), 'status': 'RUNNING',
              'minimum_hardware': False, 'arming_evidence': False, 'samples': []}
    def save():
        (a.out / 'raw.json').write_text(json.dumps(report, indent=2) + '\n')
    save()
    import itertools
    for positions, count, ordinal, n in itertools.product(a.positions or [6, 65], a.claims or [2, 12], range(a.runs), [0, 1]):
        before = os.getloadavg()
        if before[0] > 5 and not a.allow_under_load:
            report['status'] = 'NOT_RUN_LOAD'; save(); return 75
        label = f'p{positions}-c{count}-n{n}-{ordinal:02d}'
        env = dict(os.environ, TMPDIR='/private/tmp', G14_SAT_POSITIONS=str(positions), G14_SAT_CLAIMS=str(count), G14_SAT_N=str(n))
        cmd = ['/usr/bin/time', '-l', binary, TEST, '--exact', '--ignored', '--nocapture', '--test-threads=1']
        utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
        start = time.monotonic()
        with (a.out / (label + '.stdout.log')).open('w') as stdout, (a.out / (label + '.stderr.log')).open('w') as stderr:
            child = subprocess.Popen(cmd, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                rc = child.wait(timeout=1800)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGTERM)
                try: child.wait(timeout=10)
                except subprocess.TimeoutExpired: os.killpg(child.pid, signal.SIGKILL); child.wait()
                rc = 124
        after = os.getloadavg()
        out = a.out / (label + '.stdout.log'); err = a.out / (label + '.stderr.log')
        records = re.findall(r'^(?:test \S+ \.\.\. )?G14_SAT_MEASURE (\{.*\})$', out.read_text(), re.MULTILINE)
        row = {'label': label, 'utc_start': utc, 'command': cmd,
               'env': {k: env[k] for k in ['TMPDIR', 'G14_SAT_POSITIONS', 'G14_SAT_CLAIMS', 'G14_SAT_N']},
               'exit_code': rc, 'monitor_wall_seconds': time.monotonic() - start,
               'load_before': before, 'load_after': after, 'under_load': max(before[0], after[0]) > 5,
               'stdout_sha256': sha(out), 'stderr_sha256': sha(err),
               'measurement': json.loads(records[0]) if len(records) == 1 else None}
        for key, pat in [('rss_bytes', r'(\d+)\s+maximum resident set size'), ('real_seconds', r'([\d.]+)\s+real'),
                         ('user_seconds', r'([\d.]+)\s+user'), ('sys_seconds', r'([\d.]+)\s+sys')]:
            match = re.search(pat, err.read_text()); row[key] = float(match[1]) if match else None
        report['samples'].append(row); save()
        print(label, rc, row['real_seconds'], flush=True)
        valid = row['measurement']
        if rc or valid is None or valid['n'] != n or valid['claims'] != count or valid['trace_positions'] != positions - 1 or (n and not valid['outsider']['convicted']):
            report['status'] = 'FAILED'; save(); return 1
    measured = [r['measurement'] for r in report['samples'] if r['measurement']['n'] == 1]
    report['run_cap_saturated_in_any_sample'] = any(r['run_cap_saturated'] for r in measured)
    report['scope'] = 'native transport and court curve; no continuous refill or same-block outsider priority'
    report['status'] = 'UNDER_LOAD_FIXTURE_ONLY' if any(r['under_load'] for r in report['samples']) else 'FIXTURE_ONLY'
    save(); return 0


if __name__ == '__main__':
    raise SystemExit(main())
