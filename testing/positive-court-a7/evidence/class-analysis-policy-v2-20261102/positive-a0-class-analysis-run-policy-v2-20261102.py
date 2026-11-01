#!/usr/bin/env python3
"""Next-prefix proposal only; import is inert. No build/download/conversion.

Requires an explicit new --out. The original wrapper and first-run raw directory
are never modified. Main deliberately stays pinned to the original Root source,
build record, binary and three declarations; a changed Root requires a new plan.
"""
from contextlib import contextmanager
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from types import SimpleNamespace

sys.dont_write_bytecode = True
ROOT = Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
BASE = ROOT / 'docs/evidence/positive-court-a0-2026-10-31/integration'
ORIGINAL_OUT = BASE / 'real-llm-class-analysis-first-run-20261102'
BUILD_RECORD = BASE / 'real-llm-class-analysis-first-build-20261102.json'
BUILD_RECORD_SHA256 = '26f24d7c001bea6aaa891de509d9bdf175f57fac832e1a15e83c32e8488630ee'
MODEL_RECORD = BASE / 'real-llm-run-8gib-20261101/manifest.json'
SOURCE_PIN = '4e6724e32ac2cbde9f7d021ee87cf8f7c05f5218'
BINARY_PATH = '/Users/wata/.codex/worktrees/positive-court-a0/target/debug/real_llm_class_analysis'
BINARY_SHA256 = '60a84e2f4597724b24d46d98f87ddb8f232f45af0a0ab8222d35c8a363d98f2b'
BINARY_BYTES = 6259216
TIMEOUT = 180
RSS_STOP_BYTES = 2 * (1 << 30)
DISK_STOP_BYTES = 512 * (1 << 20)
POLL_SECONDS = 0.2
LEAF_CAP = str(2**64 - 1)
MODELS = [
    ('pythia-70m', 'EleutherAI/pythia-70m', 'a39f36b100fe8a5377810d56c3f4789b9c53ac42', 2048),
    ('smollm2-135m', 'HuggingFaceTB/SmolLM2-135M', '93efa2f097d58c2a74874c7e644dbc9b0cee75a2', 8192),
    ('smollm2-360m', 'HuggingFaceTB/SmolLM2-360M', 'f8027fd0eaeea54caa13c31d31b9fdc459c38b49', 8192),
]
SCOPE = ('CLASS_ONLY_DATA_ANALYSIS; original trained-artifact declarations; no CPU model execution, '
         'native registration, full admission, roles/P/funds/credit qualification')


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as source:
        for chunk in iter(lambda: source.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path):
    def refuse(value):
        raise ValueError('nonfinite JSON: ' + value)
    return json.loads(Path(path).read_text(), parse_constant=refuse)


def save(path, value):
    # This path is inside the exclusively created next-prefix directory only.
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def cancelled(state):
    return state.get('signal') is not None


def failure_status(error, state):
    return 'CANCELLED' if isinstance(error, KeyboardInterrupt) or cancelled(state) else 'PREFLIGHT_OR_WRAPPER_ERROR'


def latch_reason(current, candidate, state, observations):
    if candidate is not None and candidate not in observations:
        observations.append(candidate)
    if cancelled(state):
        if 'CANCELLED' not in observations:
            observations.append('CANCELLED')
        return 'CANCELLED'
    return current if current is not None else candidate


def clean_group(cleanup):
    return (isinstance(cleanup, dict) and type(cleanup.get('group')) is int and cleanup['group'] > 0
            and cleanup.get('live_descendants') == [] and cleanup.get('zombies') == [])


def complete(item):
    return (item.get('stop_reason') is None and item.get('cancel_seen') is False
            and type(item.get('exit_code')) is int and item['exit_code'] == 0
            and item.get('leader_reaped') is True and clean_group(item.get('cleanup'))
            and item.get('raw_finalized') is True and item.get('report_valid') is True
            and item.get('source_and_binary_stable') is True)


def report_errors(report, item):
    if not isinstance(report, dict):
        return ['REPORT_NOT_OBJECT']
    expected = {
        'schema': 'misaka.a7.real-llm-class-analysis.v1', 'mode': 'class-only-analysis',
        'scope': 'CLASS_ONLY_DATA_ANALYSIS', 'status': 'CLASS_ONLY_ANALYSIS_COMPLETE', 'phase': 'complete',
        'model': item['model'], 'revision': item['revision'], 'original_max_context': item['original_max_context'],
        'cpu_executed': False, 'native_v2_admission_executed': False, 'credit_executed': False,
        'independent_expected_executed': False, 'registered_or_rooted_state_installed': False,
        'protocol_params_modified': False, 'economic_or_authentication_fields_synthesized': False,
    }
    errors = ['REPORT_MISMATCH:' + key for key, value in expected.items()
              if type(report.get(key)) is not type(value) or report.get(key) != value]
    if 'error' in report:
        errors.append('REPORT_ERROR_FIELD_PRESENT')
    return errors


@contextmanager
def file_streams(directory):
    with (directory / 'stdout.log').open('xb') as stdout, (directory / 'stderr.log').open('xb') as stderr:
        yield stdout, stderr


def run_case(item, directory, argv, env, state, hooks):
    """Own one group; hooks allow race/cleanup tests without spawning anything."""
    item = dict(item)
    item.update(status='RUNNING', stop_reason=None, observed_stop_reasons=[], cancel_seen=False,
                cleanup=None, exit_code=None, leader_reaped=False, raw_finalized=False,
                report_valid=False, source_and_binary_stable=False, errors=[])
    observations = item['observed_stop_reasons']
    process = None
    start = hooks.monotonic()
    peak = 0
    reason = None
    cleanup = None
    try:
        reason = latch_reason(reason, None, state, observations)
        if reason is None and not hooks.identity_check():
            reason = 'SOURCE_OR_BINARY_CHANGED'
        if reason is None and hooks.disk_free() < DISK_STOP_BYTES:
            reason = latch_reason(reason, 'DISK_STOP', state, observations)
        reason = latch_reason(reason, None, state, observations)
        if reason is None:
            with hooks.streams(directory) as (stdout, stderr):
                # Last check immediately before Popen, after identity/files/limits work.
                reason = latch_reason(reason, None, state, observations)
                if reason is None:
                    process = hooks.popen(argv, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
                    try:
                        while True:
                            reason = latch_reason(reason, None, state, observations)
                            if reason is not None:
                                cleanup = hooks.stop_group(process)
                                break
                            code = process.poll()
                            # poll() can observe exit zero at the same moment as cancellation.
                            reason = latch_reason(reason, None, state, observations)
                            if reason is not None:
                                cleanup = hooks.stop_group(process)
                                break
                            if code is not None:
                                break
                            candidates = []
                            if hooks.monotonic() - start >= TIMEOUT:
                                candidates.append('TIMEOUT')
                            if hooks.disk_free() < DISK_STOP_BYTES:
                                candidates.append('DISK_STOP')
                            rss = hooks.sample_rss(process.pid)
                            peak = max(peak, rss)
                            if rss > RSS_STOP_BYTES:
                                candidates.append('POLLED_RSS_STOP')
                            for candidate in candidates:
                                reason = latch_reason(reason, candidate, state, observations)
                            reason = latch_reason(reason, None, state, observations)
                            if reason is not None:
                                cleanup = hooks.stop_group(process)
                                break
                            hooks.sleep(POLL_SECONDS)
                        process.wait()
                        item['leader_reaped'] = True
                        reason = latch_reason(reason, None, state, observations)
                        if cleanup is None:
                            if hooks.active(process.pid):
                                reason = latch_reason(reason, 'DESCENDANT_LEAK', state, observations)
                                cleanup = hooks.stop_group(process)
                            else:
                                cleanup = {'group': process.pid, 'live_descendants': [], 'zombies': hooks.members(process.pid)}
                        reason = latch_reason(reason, None, state, observations)
                    except BaseException as error:
                        if isinstance(error, KeyboardInterrupt):
                            state['signal'] = state.get('signal') or 2
                        item['errors'].append(type(error).__name__ + ': ' + str(error))
                        try:
                            cleanup = hooks.stop_group(process)
                            process.wait()
                            item['leader_reaped'] = True
                        except BaseException as cleanup_error:
                            item['errors'].append('CLEANUP_FAILED:' + str(cleanup_error))
                            cleanup = None
                        reason = latch_reason(reason, 'PROCESS_OR_CLEANUP_ERROR', state, observations)
            # Stream/context exit may itself record a late signal.
            reason = latch_reason(reason, None, state, observations)
        if process is not None:
            item['exit_code'] = process.returncode
        item['cleanup'] = cleanup
        item['raw_finalized'] = item['leader_reaped'] and clean_group(cleanup)
        if item['raw_finalized']:
            report_path = directory / 'analysis.json'
            if report_path.is_file():
                try:
                    report = read_json(report_path)
                    item['report_validation_errors'] = report_errors(report, item)
                    item['report_valid'] = not item['report_validation_errors']
                    if isinstance(report, dict):
                        for key in ['status', 'phase', 'error', 'geometry', 'twin_comparisons']:
                            item['analysis_status' if key == 'status' else key] = report.get(key)
                except (ValueError, OSError) as error:
                    item['errors'].append('REPORT_READ_FAILED:' + str(error))
            else:
                item['analysis_status'] = 'NO_FINAL_REPORT'
            item['resource_measurement'] = hooks.parse_resource(directory / 'time-l.log')
            item['raw_files'] = hooks.hash_raw(directory)
        else:
            item['analysis_status'] = 'RAW_UNFINALIZED_OR_NO_PROCESS'
            item['raw_files'] = {}
            item['resource_measurement'] = {'status': 'NOT_FINALIZED'}
        item['source_and_binary_stable'] = hooks.identity_check()
    except BaseException as error:
        if isinstance(error, KeyboardInterrupt):
            state['signal'] = state.get('signal') or 2
        item['errors'].append('WRAPPER_ERROR:' + type(error).__name__ + ':' + str(error))
        reason = latch_reason(reason, 'WRAPPER_ERROR', state, observations)
        # A failure before or after the main ownership block still attempts cleanup.
        if process is not None and not item['raw_finalized']:
            try:
                cleanup = hooks.stop_group(process)
                process.wait()
                item.update(cleanup=cleanup, exit_code=process.returncode, leader_reaped=True)
            except BaseException as cleanup_error:
                item['errors'].append('CLEANUP_FAILED:' + str(cleanup_error))
    reason = latch_reason(reason, None, state, observations)
    item.update(stop_reason=reason, cancel_seen=cancelled(state), elapsed_seconds=hooks.monotonic() - start,
                polled_group_peak_rss_bytes=peak)
    item['analysis_complete'] = complete(item)
    item['status'] = 'CLASS_DATA_COMPLETE' if item['analysis_complete'] else 'CLASS_DATA_INCOMPLETE'
    return item


def run_sequence(specs, run_one, state, identity_check, progress):
    """Only progress is emitted here; the aggregate is computed after the for."""
    results = []
    for spec in specs:
        if cancelled(state) or not identity_check():
            break
        # A signal recorded during identity_check must prevent the next spawn.
        if cancelled(state):
            break
        result = run_one(spec)
        results.append(result)
        progress(results)  # caller keeps global status RUNNING
        if cancelled(state) or result.get('stop_reason') == 'CANCELLED' or not result.get('source_and_binary_stable'):
            break
        if not result.get('raw_finalized'):
            break
    else:
        return results, True
    return results, False


def aggregate(results, specs, loop_finished, final_identity, state):
    if cancelled(state) or any(x.get('cancel_seen') or x.get('stop_reason') == 'CANCELLED' for x in results):
        return 'CANCELLED'
    identities = [(x.get('slug'), x.get('model'), x.get('revision'), x.get('original_max_context')) for x in results]
    planned = [(x['slug'], x['model'], x['revision'], x['original_max_context']) for x in specs]
    good = (loop_finished and planned == MODELS and identities == planned and len(set(identities)) == len(planned)
            and all(complete(x) for x in results) and final_identity.get('source_stable') is True
            and final_identity.get('binary_stable') is True and final_identity.get('head_stable') is True)
    return 'CLASS_DATA_COMPLETE_ALL' if good else 'CLASS_DATA_INCOMPLETE'


def real_hooks(identity_check):
    # No Root helpers are imported until main explicitly prepares a new run.
    sys.path.insert(0, str(ROOT / 'testing/positive-court-a7'))
    from process_control import active, members, stop_group
    from real_llm_process import parse_darwin_time

    def rss(group):
        rows = subprocess.check_output(['ps', '-A', '-o', 'pid=,pgid=,rss='], text=True).splitlines()
        return sum(int(f[2]) * 1024 for row in rows if len(f := row.split()) == 3 and int(f[1]) == group)

    def hash_raw(directory):
        return {str(p): {'bytes': p.stat().st_size, 'sha256': sha(p)} for p in sorted(directory.iterdir()) if p.is_file()}

    return SimpleNamespace(popen=subprocess.Popen, monotonic=time.monotonic, sleep=time.sleep, streams=file_streams,
                           active=active, members=members, stop_group=stop_group, sample_rss=rss,
                           disk_free=lambda: shutil.disk_usage(ROOT).free, identity_check=identity_check,
                           parse_resource=parse_darwin_time, hash_raw=hash_raw)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True, help='explicit fresh output prefix; existing paths refused')
    args = parser.parse_args(argv)
    out = args.out.resolve()
    original = ORIGINAL_OUT.resolve()
    if out == original or original in out.parents or out in original.parents:
        parser.error('original first-run output and its ancestors/descendants are protected')
    out.mkdir(parents=True, exist_ok=False)
    manifest_path = out / 'manifest.json'
    document = {'source_commit': SOURCE_PIN, 'status': 'PREFLIGHT', 'scope': SCOPE,
                'policy_revision': 'next-prefix-v2-proposal', 'output': str(out), 'results': [],
                'diagnostic_leaf_cap': LEAF_CAP, 'diagnostic_court': 'both',
                'controller': {'path': str(Path(__file__).resolve()), 'sha256': sha(__file__)},
                'outer_process_policy': {'timeout_seconds_per_model': TIMEOUT,
                    'polled_process_group_rss_stop_bytes': RSS_STOP_BYTES, 'rss_poll_seconds': POLL_SECONDS,
                    'rss_bound_guaranteed': False, 'disk_stop_bytes': DISK_STOP_BYTES,
                    'RUST_MIN_STACK': None, 'cold_claimed': False}}
    save(manifest_path, document)
    state = {'signal': None}
    try:
        if sha(BUILD_RECORD) != BUILD_RECORD_SHA256:
            raise ValueError('original build record changed')
        build = read_json(BUILD_RECORD)
        if build.get('build_success') is not True or build.get('source_commit') != SOURCE_PIN:
            raise ValueError('original build success/source pin mismatch')
        artifact = build['binary'][0]
        if (artifact['path'], artifact['sha256'], artifact['bytes']) != (BINARY_PATH, BINARY_SHA256, BINARY_BYTES):
            raise ValueError('original binary identity metadata changed')
        source = build['source_after']
        snapshot = lambda: {name: sha(name) for name in source}
        head = lambda: subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()

        def identity_check():
            return head() == SOURCE_PIN and snapshot() == source and sha(BINARY_PATH) == BINARY_SHA256

        if not identity_check():
            raise ValueError('Root source/binary is no longer the original tested pin')
        old = read_json(MODEL_RECORD)
        old_models = {row['slug']: row for row in old['models']}
        specs = []
        for slug, model, revision, context in MODELS:
            row = old_models[slug]
            if (row['model'], row['revision'], row['max_context']) != (model, revision, context):
                raise ValueError('original cohort metadata changed: ' + slug)
            directory = Path('/Users/wata/Downloads/misaka-trained-llm-cohort-20261031') / model.replace('/', '__')
            run = directory / ('run-wide-q24-v2' if slug == 'pythia-70m' else 'run')
            expected_artifact = run / ('model.palwtir.private-wide-q24.palwtir' if slug == 'pythia-70m' else 'model.palwtir.testnet-12.palwtir')
            if row['artifact'] != str(expected_artifact) or row['fixture'] != str(run / 'registration-fixture.json'):
                raise ValueError('original declaration paths changed: ' + slug)
            specs.append({'slug': slug, 'model': model, 'revision': revision, 'original_max_context': context,
                          'artifact': row['artifact'], 'fixture': row['fixture']})
        env = os.environ.copy()
        for name in ['RUST_MIN_STACK', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']:
            env.pop(name, None)
        env['RAYON_NUM_THREADS'] = '2'
        document.update(build_record={'path': str(BUILD_RECORD), 'sha256': BUILD_RECORD_SHA256}, binary=artifact,
                        source_before=source, model_record={'path': str(MODEL_RECORD), 'sha256': sha(MODEL_RECORD)},
                        environment_policy={'removed': ['RUST_MIN_STACK', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'],
                                            'RAYON_NUM_THREADS': '2', 'other_environment': 'inherited as in original policy'},
                        status='RUNNING')
        hooks = real_hooks(identity_check)
        from process_control import cancellation_signals

        def progress(results):
            document['results'] = results
            document['status'] = 'RUNNING'
            save(manifest_path, document)

        with cancellation_signals() as state:
            def run_one(spec):
                case = out / spec['slug']
                case.mkdir()
                command = [BINARY_PATH, '--model', spec['model'], '--artifact', spec['artifact'], '--fixture', spec['fixture'],
                           '--out', str(case / 'analysis.json'), '--diagnostic-leaf-cap', LEAF_CAP, '--diagnostic-court', 'both']
                timed = ['/usr/bin/time', '-l', '-o', str(case / 'time-l.log')] + command
                item = dict(spec, command=command, timed_command=timed, started_unix=time.time())
                return run_case(item, case, timed, env, state, hooks)

            results, loop_finished = run_sequence(specs, run_one, state, identity_check, progress)
            final_source = snapshot()
            final_identity = {'source_stable': final_source == source,
                              'binary_stable': sha(BINARY_PATH) == BINARY_SHA256, 'head_stable': head() == SOURCE_PIN}
        # cancellation_signals may record cancellation while leaving its with.
        document.update(results=results, source_after=final_source, source_stable=final_identity['source_stable'],
                        binary_stable=final_identity['binary_stable'], head_stable=final_identity['head_stable'],
                        loop_finished=loop_finished, cancel_signal=state.get('signal'),
                        status=aggregate(results, specs, loop_finished, final_identity, state))
    except BaseException as error:
        document['status'] = failure_status(error, state)
        document['cancel_signal'] = state.get('signal')
        document['error'] = type(error).__name__ + ': ' + str(error)
    save(manifest_path, document)
    print(json.dumps({'status': document['status'], 'manifest': str(manifest_path)}))
    return 0 if document['status'] == 'CLASS_DATA_COMPLETE_ALL' else 1


if __name__ == '__main__':
    raise SystemExit(main())
