from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import time

root = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out = root / 'handoff/positive-court/remaining-capacity-20261102'
previous = json.loads((out / 'source-court-integration-full.json').read_text())
command = [*previous['command'][:2], '--message-format=json-render-diagnostics', *previous['command'][2:]]
env = os.environ.copy()
env.update(previous['environment_overrides'])
env.update(CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
env.pop('RUSTFLAGS', None)
env.pop('RUST_MIN_STACK', None)
inventory = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root, text=True).split('\0')
paths = sorted({p for p in inventory if p and Path(p).suffix in ['.rs', '.toml', '.lock', '.py', '.json', '.h', '.cpp', '.cc']
                and not any(part in Path(p).parts for part in ['evidence', 'validation', 'history'])})

def snapshot():
    return {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in paths}

before = snapshot()
log = out / 'source-court-integration-functional-r3.log'
minimum_free = shutil.disk_usage(root).free
assert minimum_free > 1024**3
started = time.monotonic()
stop = None
with log.open('xb') as stream:
    process = subprocess.Popen(command, cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
    print('LIVE final capacity validation', process.pid, flush=True)
    try:
        while process.poll() is None:
            minimum_free = min(minimum_free, shutil.disk_usage(root).free)
            if minimum_free < 512 * 1024**2 or time.monotonic() - started > 900:
                stop = 'disk floor' if minimum_free < 512 * 1024**2 else 'timeout'
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                break
            time.sleep(.25)
    except BaseException:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        raise
data = log.read_text()
counts = [tuple(map(int, row)) for row in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored', data)]
record = {'source_file_count': len(paths), 'command': command, 'exit_code': process.returncode, 'stop_reason': stop,
          'elapsed_seconds': round(time.monotonic() - started, 3), 'source_before': before,
          'source_after': snapshot(), 'environment_overrides': previous['environment_overrides'],
          'functional_build_overrides': {k: env[k] for k in ['CARGO_BUILD_JOBS', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG']},
          'RUST_MIN_STACK': None, 'RUSTFLAGS': None, 'minimum_free_bytes': minimum_free,
          'test_summaries': counts, 'log': log.name, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
artifacts = []
for line in data.splitlines():
    if not line.startswith('{'):
        continue
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        continue
    if event.get('reason') == 'compiler-artifact' and event.get('target', {}).get('name') == 'positive_court_a4_integration':
        artifacts.append(event)
if len(artifacts) != 1:
    record['selected_artifact_count'] = len(artifacts)
    (out / 'source-court-integration-functional-r3.json').write_text(json.dumps(record, indent=2) + '\n')
    print('INCOMPLETE', process.returncode, stop, 'counts', counts, flush=True)
    print(data[-5000:], flush=True)
    raise SystemExit(1)
artifact = artifacts[0]
record['selected_target_profile'] = artifact['profile']
assert artifact['profile']['opt_level'] == '0'
assert artifact['profile']['debuginfo'] == 0
assert artifact['profile']['debug_assertions'] and artifact['profile']['overflow_checks']
binary = Path(artifact['executable'])
if binary.is_file():
    record['binary'] = {'path': str(binary), 'bytes': binary.stat().st_size,
                        'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
record['remaining_process_group_members'] = [line for line in subprocess.check_output(['ps', '-axo', 'pid,pgid,state'], text=True).splitlines()[1:]
                                             if line.split()[1] == str(process.pid)]
(out / 'source-court-integration-functional-r3.json').write_text(json.dumps(record, indent=2) + '\n')
print('EXIT', process.returncode, 'stop', stop, 'counts', counts, flush=True)
print(data[-7500:], flush=True)
assert process.returncode == 0 and stop is None and counts == [(40, 0, 0)]
assert record['source_before'] == record['source_after'] and not record['remaining_process_group_members']
