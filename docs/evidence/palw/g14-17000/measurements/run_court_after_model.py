#!/usr/bin/env python3
"""Build current core timings after native build, then request a quiet batch after 32k.

This finite continuation uses existing jobs, never restarts them, and leaves every
failed/loaded/timeout outcome visible. Run from the measurement worktree.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import time


def wait_record(path, field, running, timeout):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        try:
            record = json.loads(path.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(30)
            continue
        if record.get(field) not in running:
            return record
        time.sleep(30)
    raise TimeoutError(f'Observation timeout: {path}; no job was restarted')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--buildslot', type=Path, required=True)
    args = parser.parse_args()
    root = args.artifacts
    state_path = root / 'quiet-core-workflow.json'
    assert not state_path.exists(), 'preserve previous workflow'
    state = {'stage': 'WAIT_NATIVE_BUILD', 'arming_evidence': False,
             'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'commands': []}
    def save():
        state_path.write_text(json.dumps(state, indent=2) + '\n')
    def run(arguments):
        command = [str(args.buildslot), 'python3', *map(str, arguments)]
        state['commands'].append(command); save()
        return subprocess.run(command, env=dict(os.environ, CARGO_TARGET_DIR=str(root/'target'))).returncode
    save()
    try:
        native = wait_record(root/'native-postcommit-build-record.json', 'stage', {'BUILDING'}, 4*3600)
        state['native_build_stage'] = native['stage']
        # A failed native build is retained; this independent core target may still build.
        state['stage'] = 'BUILD_CORE'; save()
        code = run([Path(__file__).with_name('build_measures.py'), '--target', 'court', '--record-name', 'core-postcommit', '--out', root])
        if code:
            state.update(stage='CORE_BUILD_FAILED', exit_code=code); save(); return code
        state['stage'] = 'WAIT_MODEL_TERMINAL'; save()
        model = wait_record(root/'dolphin-long-n32768-2026-10-26/raw.json', 'status', {'RUNNING'}, 13*3600)
        state['model_status'] = model['status']
        state['stage'] = 'QUIET_BATCH'; save()
        code = run([Path(__file__).with_name('run_court_batch.py'), '--build-record', root/'core-postcommit-build-record.json', '--out', root/'core-postcommit-quiet-2026-10-26', '--n', '1000', '--max-wait-seconds', '7200'])
        state.update(stage='FINISHED', exit_code=code); save()
        return code
    except Exception as error:
        state.update(stage='OBSERVATION_OR_WORKFLOW_ERROR', error=str(error)); save()
        raise


if __name__ == '__main__':
    raise SystemExit(main())
