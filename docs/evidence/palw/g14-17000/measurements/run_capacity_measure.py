#!/usr/bin/env python3
"""Fresh-process native multi-claim budget measurement; reduced test cap, not production capacity."""
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

TEST = 'pipeline::virtual_processor::tests::g14_kernel_route_e2e::canonical::capacity_measurement::g14_kernel_capacity_measure'


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
    p.add_argument('--claims', type=int, action='append', choices=[1, 2, 4, 6])
    p.add_argument('--positions', type=int, action='append', choices=[6, 64, 65])
    p.add_argument('--adjudication-cap', type=int, choices=[4,64], default=4)
    p.add_argument('--early-anchor-window', action='store_true')
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
    for positions, count, ordinal, n in itertools.product(a.positions or [6, 64], a.claims or [1, 2, 4, 6], range(a.runs), [0, 1]):
        before = os.getloadavg()
        if before[0] > 5 and not a.allow_under_load:
            report['status'] = 'NOT_RUN_LOAD'; save(); return 75
        label = f'p{positions}-c{count}-n{n}-{ordinal:02d}'
        env = dict(os.environ, G14_CAP_ADJUDICATION_CAP=str(a.adjudication_cap), G14_MEASURE_EARLY_ANCHOR_WINDOW=str(int(a.early_anchor_window)), TMPDIR='/private/tmp', G14_CAP_POSITIONS=str(positions), G14_CAP_CLAIMS=str(count), G14_CAP_N=str(n))
        cmd = ['/usr/bin/time', '-l', binary, TEST, '--exact', '--ignored', '--nocapture', '--test-threads=1']
        utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
        start = time.monotonic()
        with (a.out / (label + '.stdout.log')).open('w') as stdout, (a.out / (label + '.stderr.log')).open('w') as stderr:
            child = subprocess.Popen(cmd, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                rc = child.wait(timeout=300)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGTERM)
                try: child.wait(timeout=10)
                except subprocess.TimeoutExpired: os.killpg(child.pid, signal.SIGKILL); child.wait()
                rc = 124
        after = os.getloadavg()
        out = a.out / (label + '.stdout.log'); err = a.out / (label + '.stderr.log')
        records = re.findall(r'^(?:test \S+ \.\.\. )?G14_CAP_MEASURE (\{.*\})$', out.read_text(), re.MULTILINE)
        row = {'label': label, 'utc_start': utc, 'command': cmd,
               'env': {k: env[k] for k in ['TMPDIR', 'G14_CAP_POSITIONS', 'G14_CAP_CLAIMS', 'G14_CAP_N', 'G14_CAP_ADJUDICATION_CAP', 'G14_MEASURE_EARLY_ANCHOR_WINDOW']},
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
        if rc or valid is None or valid['n'] != n or valid['claims'] != count or valid['positions'] != positions or (n and not valid['outsider']['convicted']):
            report['status'] = 'FAILED'; save(); return 1
    report['status'] = 'UNDER_LOAD_FIXTURE_ONLY' if any(r['under_load'] for r in report['samples']) else 'FIXTURE_ONLY'
    save(); return 0


if __name__ == '__main__':
    raise SystemExit(main())
