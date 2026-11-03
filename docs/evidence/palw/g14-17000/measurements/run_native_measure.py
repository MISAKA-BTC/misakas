#!/usr/bin/env python3
"""Measure the pinned native fixture binary in fresh processes, under buildslot."""
import argparse, hashlib, json, os, re, subprocess, time
from pathlib import Path

CASES = ['honest', 'forged_output', 'fake_state', 'duplicate', 'withheld_reveal']
TEST = 'pipeline::virtual_processor::tests::g14_kernel_route_e2e::canonical::measurement::g14_native_panel_measure'

def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for data in iter(lambda: f.read(1 << 20), b''): h.update(data)
    return h.hexdigest()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build-record', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--case', action='append', choices=CASES)
    p.add_argument('--allow-under-load', action='store_true')
    p.add_argument('--adjudication-cap', type=int, choices=[4,64], default=4)
    p.add_argument('--early-anchor-window', action='store_true')
    a = p.parse_args()
    assert a.runs >= 1
    build = json.loads(a.build_record.read_text())
    assert build['stage'] == 'BUILT' and build['source_unchanged'] is True
    binary = build['binary']['path']
    assert sha(binary) == build['binary']['sha256']
    a.out.mkdir(parents=True, exist_ok=False)
    report = {'build': build, 'minimum_hardware': False, 'arming_evidence': False,
              'status': 'RUNNING', 'samples': [], 'comparisons': []}
    def save():
        (a.out / 'raw.json').write_text(json.dumps(report, indent=2) + '\n')
    save()
    for case in a.case or CASES:
        for ordinal in range(a.runs):
            # Alternate the seat order to expose rather than hide drift with load.
            for seats in ([3, 1] if ordinal % 2 == 0 else [1, 3]):
                for n in [0, 1]:
                    before = os.getloadavg()
                    if before[0] > 5 and not a.allow_under_load:
                        report['status'] = 'NOT_RUN_LOAD'; save(); return 75
                    env = dict(os.environ, G14_CAP_ADJUDICATION_CAP=str(a.adjudication_cap), G14_MEASURE_EARLY_ANCHOR_WINDOW=str(int(a.early_anchor_window)), TMPDIR='/private/tmp', G14_MEASURE_CASE=case,
                               G14_MEASURE_N=str(n), G14_MEASURE_SEATS=str(seats))
                    cmd = ['/usr/bin/time', '-l', binary, TEST, '--exact', '--ignored', '--nocapture', '--test-threads=1']
                    label = f'{case}-p{seats}-n{n}-{ordinal:02d}'
                    start = time.monotonic()
                    with (a.out / (label+'.stdout.log')).open('w') as stdout, (a.out / (label+'.stderr.log')).open('w') as stderr:
                        child = subprocess.Popen(cmd, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
                        try: rc = child.wait(timeout=300)
                        except subprocess.TimeoutExpired:
                            # Stop only this measurement's process group, preserving failure logs.
                            import signal
                            os.killpg(child.pid, signal.SIGTERM)
                            try: child.wait(timeout=10)
                            except subprocess.TimeoutExpired: os.killpg(child.pid, signal.SIGKILL); child.wait()
                            rc = 124
                    after = os.getloadavg()
                    raw = (a.out / (label+'.stdout.log')).read_text()
                    err = (a.out / (label+'.stderr.log')).read_text()
                    lines = re.findall(r'^(?:test \S+ \.\.\. )?G14_MEASURE (\{.*\})$', raw, re.MULTILINE)
                    row = {'label': label, 'command': cmd, 'env': {k: env[k] for k in ['TMPDIR', 'G14_MEASURE_CASE', 'G14_MEASURE_N', 'G14_MEASURE_SEATS', 'G14_CAP_ADJUDICATION_CAP', 'G14_MEASURE_EARLY_ANCHOR_WINDOW']},
                           'exit_code': rc, 'wall_seconds': time.monotonic()-start, 'load_before': before, 'load_after': after,
                           'under_load': max(before[0], after[0]) > 5,
                           'stdout_sha256': sha(a.out/(label+'.stdout.log')), 'stderr_sha256': sha(a.out/(label+'.stderr.log'))}
                    for key, pat in [('rss_bytes', r'(\d+)\s+maximum resident set size'), ('user_seconds', r'([\d.]+)\s+user'), ('sys_seconds', r'([\d.]+)\s+sys')]:
                        m = re.search(pat, err); row[key] = float(m[1]) if m else None
                    row['measurement'] = json.loads(lines[0]) if len(lines) == 1 else None
                    report['samples'].append(row); save()
                    print(label, rc, round(row['wall_seconds'], 3), flush=True)
                    if rc or len(lines) != 1:
                        report['status'] = 'FAILED'; save(); return 1
            pair = [s['measurement'] for s in report['samples'][-4:] if s['measurement']['n'] == 1]
            assert len(pair) == 2
            checks = {key: pair[0][key] == pair[1][key] for key in ['class', 'job', 'claim', 'verdict', 'final_daa', 'final_after_cover_daa']}
            report['comparisons'].append({'case': case, 'ordinal': ordinal, 'equal': checks, 'absolute_final_daa': [r['final_daa'] for r in pair], 'covered_daa': [r['covered_daa'] for r in pair]})
            # Crossing twins intentionally enter at different DAA. Keep the raw
            # Final DAA difference; require identical claim and Final window.
            if not all(v for k, v in checks.items() if k != 'final_daa'):
                report['status'] = 'MISMATCH'; save(); return 1
    report['status'] = 'UNDER_LOAD_FIXTURE_ONLY' if any(s['under_load'] for s in report['samples']) else 'FIXTURE_ONLY'
    save(); return 0

if __name__ == '__main__':
    raise SystemExit(main())
