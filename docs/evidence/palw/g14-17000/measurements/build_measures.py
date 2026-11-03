#!/usr/bin/env python3
"""Build one test target inside buildslot; freeze source after slot acquisition."""
import argparse, hashlib, json, os, shutil, subprocess, time
from pathlib import Path

def git(*args): return subprocess.check_output(['git', *args], text=True).strip()
def code():
    lines = git('ls-tree', '-r', 'HEAD').splitlines()
    return hashlib.sha256('\n'.join(line for line in lines if not line.split('\t', 1)[1].startswith('docs/')).encode()).hexdigest()
def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for data in iter(lambda: f.read(1 << 20), b''): h.update(data)
    return h.hexdigest()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--target', choices=['native', 'dolphin', 'court'], required=True)
    p.add_argument('--record-name', required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    if a.target == 'native':
        package, target, selector = 'kaspa-consensus', 'kaspa_consensus', ['--lib']
    elif a.target == 'court':
        package, target, selector = 'kaspa-consensus-core', 'palw_tir_court_verify_cpu', ['--test', 'palw_tir_court_verify_cpu']
    else:
        # This measurement package has one CLI, unlike BASE-0's many auto bins.
        package, target, selector = 'misaka-palw-opv-meas', 'g14_real_model_measure', ['--test', 'g14_real_model_measure']
    cmd = ['cargo', 'test', '--offline', '--release', '-p', package, *selector, '--no-run', '--message-format=json']
    record = a.out / f'{a.record_name}-build-record.json'
    assert not record.exists(), 'never overwrite a previous build record'
    start = time.monotonic(); initial = code()
    r = {'source_commit': git('rev-parse', 'HEAD'), 'source_trees': initial, 'command': cmd, 'shared_slot': True,
         'stage': 'BUILDING', 'features': 'default', 'minimum_hardware': False, 'load_before': os.getloadavg(),
         'hardware': subprocess.check_output(['sysctl', 'hw.model', 'hw.memsize', 'hw.ncpu'], text=True),
         'builder_sha256': sha(Path(__file__))}
    if a.target == 'court':
        r['files'] = [{'path': name, 'sha256': sha(Path(name))} for name in ['Cargo.lock', 'consensus/core/tests/palw_tir_court_verify_cpu.rs', 'consensus/core/tests/palw_tir_fixture_common.rs']]
    record.write_text(json.dumps(r, indent=2) + '\n')
    env = dict(os.environ, CARGO_TARGET_DIR=str(a.out/'target'), TMPDIR='/private/tmp')
    with (a.out/f'{a.record_name}-build.stdout.log').open('w') as stdout, (a.out/f'{a.record_name}-build.stderr.log').open('w') as stderr:
        child = subprocess.run(cmd, env=env, stdout=stdout, stderr=stderr)
    r.update(exit_code=child.returncode, wall_seconds=time.monotonic()-start, load_after=os.getloadavg(),
             stage='BUILT' if child.returncode == 0 else 'FAILED')
    if child.returncode == 0:
        found = []
        for line in (a.out/f'{a.record_name}-build.stdout.log').read_text().splitlines():
            try: j = json.loads(line)
            except ValueError: continue
            if j.get('reason') == 'compiler-artifact' and j.get('target', {}).get('name') == target and j.get('executable'):
                found.append(j['executable'])
        assert len(found) == 1, found
        src = Path(found[0]); dest = a.out/'pins'/(a.record_name+'-'+src.name)
        shutil.copy2(src, dest); r['binary'] = {'path': str(dest), 'sha256': sha(dest)}
        r['source_unchanged'] = initial == code() and not git('diff', 'HEAD', '--name-only', '--', '.', ':(exclude)docs/**')
    record.write_text(json.dumps(r, indent=2) + '\n'); print(json.dumps(r), flush=True)
    return child.returncode

if __name__ == '__main__': raise SystemExit(main())
