#!/usr/bin/env python3
"""Fixed local cohort driver. No downloads, conversion, build, or cache setup.

Default mode is dry-run. quote-only invokes the runner but does not claim measured
inference. Successful local runs remain SELF_CONSISTENCY_ONLY. Supply an already
built, explicitly authorized binary and its SHA256; this module never locates one.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from process_control import ProcessInterrupted, active, cancellation_signals, members, stop_group


LOCAL = Path(__file__).resolve().parent
ROOT = LOCAL.parents[1]
INVENTORIES = LOCAL / 'evidence/real-llm-inventory-4753-2026-11-01'
MODEL_BASE = Path('/Users/wata/Downloads/misaka-trained-llm-cohort-20261031')
# Selection, revision and inventory bytes are fixed; failure never selects a substitute.
MODELS = [
    ('smollm2-360m', 'HuggingFaceTB/SmolLM2-360M', 'f8027fd0eaeea54caa13c31d31b9fdc459c38b49', 8192,
     '80e43e0fd9ad9fb0b4dc0b44cdda2b78513ef29ea1fa549d1e2b99fe4bc26548'),
    ('smollm2-135m', 'HuggingFaceTB/SmolLM2-135M', '93efa2f097d58c2a74874c7e644dbc9b0cee75a2', 8192,
     'e6d86d57454fae5ae36b0625b73de537cf0fc552c19d882096cae015824f8920'),
    ('pythia-70m', 'EleutherAI/pythia-70m', 'a39f36b100fe8a5377810d56c3f4789b9c53ac42', 2048,
     '24296d0c2a56a50d662092eb20f1dc8e35dfa46af6762ad32e7e184f1c54f60b'),
]
CAPS = ['max_positions', 'max_step_leaves', 'max_row_lanes', 'max_work',
        'max_requested_bytes', 'max_capture_bytes']
SOURCE_FILES = [LOCAL / 'real_llm_process.py', LOCAL / 'process_control.py',
                LOCAL / 'Cargo.toml', LOCAL / 'Cargo.lock', LOCAL / 'src/bin/real_llm_repro.rs']


class CleanupUnfinalized(RuntimeError):
    """Log files may still have writers and must not be hashed as finalized."""


def terminate(process):
    try:
        return stop_group(process)
    except RuntimeError as error:
        raise CleanupUnfinalized(str(error)) from error


def utc():
    return datetime.now(timezone.utc).isoformat()


def stack_observation(env):
    value = {'rust_min_stack_environment': env.get('RUST_MIN_STACK'),
             'environment_scope': 'Rust-created thread default only; does not configure standalone main OS stack',
             'main_os_stack_limit': None}
    try:
        import resource
        soft, hard = resource.getrlimit(resource.RLIMIT_STACK)
        value['main_os_stack_limit'] = {'soft': soft, 'hard': hard, 'unit': 'bytes',
                                        'unlimited_sentinel': resource.RLIM_INFINITY,
                                        'scope': 'observed process limit, not allocated/resident bytes; unchanged'}
    except (ImportError, OSError, ValueError) as error:
        value['main_os_stack_limit_unavailable'] = str(error)
    return value


def fingerprint(path):
    """Stream hashes; never allocate a checkpoint-sized bytes object."""
    path = Path(path)
    if not path.is_file():
        raise ValueError('missing regular input: ' + str(path))
    before = path.stat()
    real = str(path.resolve(strict=True))
    digest = hashlib.sha256()
    with path.open('rb') as source:
        opened = os.fstat(source.fileno())
        for chunk in iter(lambda: source.read(1 << 20), b''):
            digest.update(chunk)
        closed = os.fstat(source.fileno())
    after = path.stat()
    fields = lambda st: (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns)
    stable = fields(before) == fields(opened) == fields(closed) == fields(after)
    stable = stable and real == str(path.resolve(strict=True))
    if not stable:
        raise ValueError('input changed while hashing: ' + str(path))
    return {'path': str(path), 'realpath': real, 'bytes': after.st_size,
            'sha256': digest.hexdigest(), 'stat': list(fields(after)), 'stable': True}


def checked_file(path, expected_sha, expected_size=None):
    value = fingerprint(path)
    if value['sha256'] != expected_sha or (expected_size is not None and value['bytes'] != expected_size):
        raise ValueError('fixed input identity mismatch: ' + str(path))
    return value


def read_json(path):
    def invalid_constant(value):
        raise ValueError('nonfinite JSON number: ' + value)
    return json.loads(Path(path).read_text(), parse_constant=invalid_constant)


def write_json(path, value):
    # Only a manifest in our newly created output directory may be updated.
    with path.open('w') as target:
        json.dump(value, target, indent=2, ensure_ascii=False, allow_nan=False)
        target.write('\n')


def source_identity(extra=()):
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    names = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    paths = {ROOT / name for name in names if name and
             (Path(name).suffix in {'.rs', '.toml', '.py'} or name.endswith('Cargo.lock'))}
    paths.update(SOURCE_FILES)
    paths.update(Path(x).resolve() for x in extra)
    hashes = {str(p): fingerprint(p)['sha256'] for p in sorted(paths) if p.is_file()}
    return {'commit': commit, 'source_sha256': hashes,
            'required_source_files_missing': [str(p) for p in SOURCE_FILES if not p.is_file()],
            'tracked_status': subprocess.check_output(
                ['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT, text=True)}


def prepare_model(spec, inventory_dir, prompt_dir):
    slug, model, revision, context, inventory_sha = spec
    inventory_path = inventory_dir / (slug + '.json')
    inventory_identity = checked_file(inventory_path, inventory_sha)
    inventory = read_json(inventory_path)
    meta = inventory.get('model', {})
    if (meta.get('repository', inventory.get('repository')) != model or
            meta.get('revision', inventory.get('revision')) != revision or
            meta.get('original_max_context', inventory.get('original_max_context')) != context):
        raise ValueError('inventory model/revision/context mismatch: ' + slug)
    directory = MODEL_BASE / model.replace('/', '__')
    run = directory / ('run-wide-q24-v2' if slug == 'pythia-70m' else 'run')
    artifact = run / ('model.palwtir.private-wide-q24.palwtir' if slug == 'pythia-70m'
                      else 'model.palwtir.testnet-12.palwtir')
    fixture_path = run / 'registration-fixture.json'
    required = [artifact, fixture_path, run / 'checkpoint/source-manifest.json',
                run / 'checkpoint/config.json', run / 'checkpoint/model.safetensors',
                run / 'checkpoint/tokenizer.json', run / 'pack/pack.json', run / 'calibration.json']
    known = {item['path']: item for item in inventory['files'] if item.get('exists', True) and item.get('sha256')}
    identities = []
    for path in required:
        recorded = known.get(str(path))
        if recorded is None:
            raise ValueError('required input absent from fixed inventory: ' + str(path))
        identities.append(checked_file(path, recorded['sha256'], recorded.get('bytes')))
    fixture = read_json(fixture_path)
    class_id = (meta.get('declared_class_id') or inventory.get('declared_identity', {}).get('class_id')
                or inventory.get('declared_class', {}).get('class_id'))
    source = read_json(run / 'checkpoint/source-manifest.json')
    config = read_json(run / 'checkpoint/config.json')
    pack = read_json(run / 'pack/pack.json')
    if (fixture['class_id_hex'] != class_id or fixture['max_context'] != context or
            fixture['artifact_sha256'] != identities[0]['sha256'] or
            source['revision'] != revision or config['max_position_embeddings'] != context or
            pack['model']['revision'] != revision or pack['profile']['context'] != context):
        raise ValueError('declaration/source/context mismatch: ' + slug)
    manifest_path = prompt_dir / 'manifest.json'
    prompt_manifest = read_json(manifest_path)
    prompt_entry = next(x for x in prompt_manifest['prompts'] if x['model_directory'] == directory.name)
    if prompt_entry['prompt_file'] != slug + '.json':
        raise ValueError('prompt file selection mismatch')
    prompt = prompt_dir / prompt_entry['prompt_file']
    identities += [fingerprint(manifest_path), checked_file(prompt, prompt_entry['prompt_sha256'])]
    calibration = run / 'calibration.json'
    checked_file(calibration, prompt_entry['source_sha256'])
    tokens = read_json(prompt)
    if (not isinstance(tokens, list) or not tokens or
            any(type(t) is not int or not 0 <= t <= 0xffffffff for t in tokens) or
            tokens != read_json(calibration)['sequences'][0]):
        raise ValueError('prompt differs from retained sequence zero')
    return {'slug': slug, 'model': model, 'revision': revision, 'max_context': context,
            'class_id': class_id, 'artifact': str(artifact), 'fixture': str(fixture_path),
            'artifact_sha256': identities[0]['sha256'], 'prompt_sha256': identities[-1]['sha256'],
            'checkpoint': str(run / 'checkpoint'), 'tokenizer': str(run / 'checkpoint/tokenizer.json'),
            'prompt_ids': str(prompt), 'prompt_tokens': len(tokens),
            'inventory': inventory_identity, 'inputs': identities}


def parse_darwin_time(path):
    """BSD time -l: ru_maxrss is bytes on Darwin, never Linux KiB."""
    if not path.is_file():
        return {'status': 'MISSING_RESOURCE_LOG'}
    text = path.read_text(errors='replace')
    value = {'status': 'PARTIAL', 'raw_path': str(path), 'platform': 'Darwin',
             'rss_unit': 'bytes', 'io_unit': 'operations, not bytes',
             'scope': 'time child accounting; lifetime peak, not interval RSS or live owner sum'}
    cpu = re.search(r'([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys', text)
    if cpu:
        value.update(time_real_seconds=float(cpu[1]), user_cpu_seconds=float(cpu[2]), system_cpu_seconds=float(cpu[3]))
    for label, key in [('maximum resident set size', 'peak_rss_bytes'), ('block input operations', 'block_input_operations'),
                       ('block output operations', 'block_output_operations')]:
        found = re.search(r'^\s*(\d+)\s+' + re.escape(label) + r'\s*$', text, re.M)
        if found:
            value[key] = int(found[1])
    if cpu and 'peak_rss_bytes' in value:
        value['status'] = 'RECORDED'
    return value


def run_fresh(argv, directory, env, timeout):
    start = time.monotonic()
    stderr = directory / 'stderr.log'
    stdout = directory / 'stdout.log'
    with cancellation_signals() as cancelled, stdout.open('xb') as out, stderr.open('xb') as err:
        if cancelled['signal'] is not None:
            raise ProcessInterrupted(cancelled['signal'], {'group': None, 'live_descendants': [], 'zombies': []})
        process = subprocess.Popen(argv, cwd=ROOT, env=env, stdout=out, stderr=err, start_new_session=True)
        cleanup = None
        try:
            deadline = start + timeout
            while True:
                if cancelled['signal'] is not None:
                    cleanup = terminate(process)
                    raise ProcessInterrupted(cancelled['signal'], cleanup)
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    code = 'TIMEOUT'
                    cleanup = terminate(process)
                    break
                try:
                    code = process.wait(timeout=min(0.1, remaining))
                    break
                except subprocess.TimeoutExpired:
                    continue
            if cleanup is None:
                if active(process.pid):
                    code = 'DESCENDANT_LEAK'
                    cleanup = terminate(process)
                else:
                    cleanup = {'group': process.pid, 'live_descendants': [], 'zombies': members(process.pid)}
            if cancelled['signal'] is not None:
                raise ProcessInterrupted(cancelled['signal'], cleanup)
        except CleanupUnfinalized:
            raise
        except BaseException:
            if cleanup is None:
                terminate(process)
            raise
    if cancelled['signal'] is not None:
        raise ProcessInterrupted(cancelled['signal'], cleanup)
    return {'exit_code': code, 'leader_exit_code': process.returncode,
            'wall_seconds': time.monotonic() - start, 'cleanup': cleanup}


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError('must be positive')
    return number


def result_errors(value, model, mode):
    """An exit-zero JSON error object is never a successful runner result."""
    if not isinstance(value, dict):
        return ['RUNNER_RESULT_NOT_OBJECT']
    expected = {'schema': 'misaka.a7.real-llm-repro.v1', 'mode': mode,
                'status': 'QUOTE_ONLY' if mode == 'quote-only' else 'LOCAL_ROUNDTRIP_PASS',
                'model': model['model'], 'class_id': model['class_id'],
                'original_max_context': model['max_context'], 'scope': 'SELF_CONSISTENCY_ONLY',
                'artifact_sha256': model['artifact_sha256'], 'prompt_ids_sha256': model['prompt_sha256'],
                'prompt_tokens': model['prompt_tokens'], 'declared_prefill_tokens': model['prompt_tokens'],
                'exact_decode_tokens': model['decode_tokens'],
                'caps': model['caps']}
    errors = ['RUNNER_RESULT_MISMATCH:' + key for key, target in expected.items() if value.get(key) != target]
    if mode == 'run' and value.get('roundtrip_pass') is not True:
        errors.append('RUNNER_ROUNDTRIP_NOT_PASSED')
    quote = value.get('quote')
    if not isinstance(quote, dict) or quote.get('external_retained_bytes') != model['external_retained_bytes']:
        errors.append('RUNNER_EXTERNAL_RETAINED_BYTES_MISMATCH')
    # Neither mode is a native, credit or independent-arithmetic certification.
    for key in ['native_v2_admission_executed', 'credit_executed', 'independent_expected_executed']:
        if value.get(key) is not False:
            errors.append('RUNNER_BOUNDARY_NOT_FALSE:' + key)
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True, help='new directory; existing outputs are never reused')
    parser.add_argument('--mode', choices=['dry-run', 'preflight', 'quote-only', 'run'], default='dry-run')
    parser.add_argument('--inventory-dir', type=Path, default=INVENTORIES)
    parser.add_argument('--prompt-dir', type=Path, default=LOCAL / 'real-llm-inputs')
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--binary-sha256')
    parser.add_argument('--source-commit')
    parser.add_argument('--source-file', action='append', default=[], help='additional runner source to hash')
    parser.add_argument('--decode', type=positive, default=1)
    parser.add_argument('--timeout', type=float, default=900)
    parser.add_argument('--min-free-bytes', type=int, default=0)
    parser.add_argument('--stack-bytes', type=positive,
                        help='RUST_MIN_STACK for Rust-created threads; does not configure standalone main stack')
    parser.add_argument('--external-retained-bytes', type=positive,
                        help='explicit allowance for independently live owners, not a measured RSS value')
    parser.add_argument('--env', action='append', default=[], metavar='NAME=VALUE')
    for cap in CAPS:
        parser.add_argument('--' + cap.replace('_', '-'), type=positive)
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or args.timeout <= 0 or args.min_free_bytes < 0:
        parser.error('timeout must be finite/positive and min-free-bytes nonnegative')
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest_path = output / 'manifest.json'
    manifest = {'schema': 'misaka.a7.real-llm-local-process.v1', 'started_utc': utc(),
                'mode': args.mode, 'status': 'PREFLIGHT_IN_PROGRESS', 'output': str(output),
                'scope': 'SELF_CONSISTENCY_ONLY', 'independent_expected_executed': False,
                'native_v2_admission_executed': False, 'credit_executed': False,
                'maximum_context_coverage_executed': False, 'cold_claimed': False,
                'build_status': 'BUILD_NOT_RUN_BY_DRIVER',
                'binary_source_mapping': 'UNVERIFIED_NO_BUILD_MANIFEST; source and binary hashes are separate pins',
                'quote_status': 'NOT_RUN', 'inference_status': 'NOT_RUN',
                'process_policy': 'one fresh process per fixed model; no model substitution',
                'platform': platform.platform(), 'models': [], 'preflight_errors': [],
                'execution_not_run_reasons': []}
    write_json(manifest_path, manifest)
    env = {k: os.environ[k] for k in ['PATH', 'HOME', 'TMPDIR', 'LANG', 'LC_ALL', 'TZ', 'USER', 'LOGNAME', 'RUST_MIN_STACK'] if k in os.environ}
    env.update(HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1', PYTHONDONTWRITEBYTECODE='1', TOKENIZERS_PARALLELISM='false')
    try:
        for item in args.env:
            name, value = item.split('=', 1)
            if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name):
                raise ValueError('invalid environment name')
            env[name] = value
        if args.stack_bytes:
            env['RUST_MIN_STACK'] = str(args.stack_bytes)
        manifest['environment'] = env
        manifest['stack_observation'] = stack_observation(env)
        before = source_identity(args.source_file)
        manifest['source_before'] = before
        if args.source_commit and args.source_commit != before['commit']:
            manifest['preflight_errors'].append('SOURCE_COMMIT_MISMATCH')
        manifest['disk_free_bytes_before'] = shutil.disk_usage(output).free
        if manifest['disk_free_bytes_before'] < args.min_free_bytes:
            manifest['preflight_errors'].append('RESOURCE_LIMIT_DISK_FREE')
        execute = args.mode in ['quote-only', 'run']
        binary = None
        if args.binary:
            try:
                binary = fingerprint(args.binary.resolve())
                if args.binary_sha256 and binary['sha256'] != args.binary_sha256:
                    manifest['preflight_errors'].append('BINARY_SHA256_MISMATCH')
            except (OSError, ValueError) as error:
                (manifest['preflight_errors'] if execute else manifest['execution_not_run_reasons']).append(str(error))
        else:
            (manifest['preflight_errors'] if execute else manifest['execution_not_run_reasons']).append('BINARY_NOT_SUPPLIED')
        manifest['binary'] = binary
        if not args.binary_sha256:
            (manifest['preflight_errors'] if execute else manifest['execution_not_run_reasons']).append('BINARY_EXPECTED_SHA256_NOT_SUPPLIED')
        if not args.source_commit:
            (manifest['preflight_errors'] if execute else manifest['execution_not_run_reasons']).append('EXPECTED_SOURCE_COMMIT_NOT_SUPPLIED')
        if execute and before['required_source_files_missing']:
            manifest['preflight_errors'].append('REQUIRED_RUNNER_SOURCE_MISSING')
        if execute and platform.system() != 'Darwin':
            manifest['preflight_errors'].append('BSD_TIME_L_PLATFORM_UNSUPPORTED')
        if execute and not Path('/usr/bin/time').is_file():
            manifest['preflight_errors'].append('TIME_EXECUTABLE_MISSING')
        if execute and any(getattr(args, cap) is None for cap in CAPS):
            manifest['preflight_errors'].append('EXPLICIT_RESOURCE_CAPS_REQUIRED')
        if execute and args.external_retained_bytes is None:
            manifest['preflight_errors'].append('EXPLICIT_EXTERNAL_OWNER_ALLOWANCE_REQUIRED')
        for spec in MODELS:
            model = {'slug': spec[0], 'status': 'PREFLIGHT_FAILED', 'errors': []}
            manifest['models'].append(model)
            try:
                model.update(prepare_model(spec, args.inventory_dir.resolve(), args.prompt_dir.resolve()))
                directory = output / model['slug']
                directory.mkdir()
                if model['prompt_tokens'] + args.decode > model['max_context']:
                    raise ValueError('short job exceeds original context')
                command = [str(args.binary.resolve()) if args.binary else 'BINARY_NOT_SUPPLIED']
                for key, value in [('model', model['model']), ('artifact', model['artifact']),
                                   ('fixture', model['fixture']), ('prompt-ids', model['prompt_ids']),
                                   ('decode', args.decode), ('out', directory / 'runner.json')]:
                    command.extend(['--' + key, str(value)])
                for cap in CAPS:
                    if getattr(args, cap) is not None:
                        command += ['--' + cap.replace('_', '-'), str(getattr(args, cap))]
                if args.external_retained_bytes is not None:
                    command += ['--external-retained-bytes', str(args.external_retained_bytes)]
                if args.mode == 'quote-only':
                    command.append('--quote-only')
                elif args.mode == 'run':
                    command.append('--run')
                model.update(command=command, timed_command=['/usr/bin/time', '-l', '-o', str(directory / 'time-l.log')] + command,
                             timeout_seconds=args.timeout, caps={cap: getattr(args, cap) for cap in CAPS},
                             external_retained_bytes=args.external_retained_bytes,
                             decode_tokens=args.decode,
                             output_json=str(directory / 'runner.json'), status='PLANNED_NOT_RUN')
            except (OSError, ValueError, KeyError, StopIteration) as error:
                model['errors'].append(str(error))
            write_json(manifest_path, manifest)
        blocked = manifest['preflight_errors'] or any(x['errors'] for x in manifest['models'])
        if not execute or blocked:
            manifest['input_identity_status'] = 'FAILED' if any(x['errors'] for x in manifest['models']) else 'MATCHED_FIXED_INPUTS'
            manifest['status'] = 'NOT_RUN_PREFLIGHT_FAILED' if blocked else ('NOT_RUN_BUILD_NOT_REQUESTED' if not binary else 'NOT_RUN_PREFLIGHT_ONLY')
        else:
            manifest['status'] = 'RUNNING'
            write_json(manifest_path, manifest)
            for model in manifest['models']:
                directory = output / model['slug']
                raw_finalized = True
                process_started = None
                try:
                    checked_file(Path(binary['path']), binary['sha256'])
                    for identity in model['inputs'] + [model['inventory']]:
                        checked_file(Path(identity['path']), identity['sha256'], identity['bytes'])
                    model['started_utc'] = utc()
                    process_started = time.monotonic()
                    model['process'] = run_fresh(model['timed_command'], directory, env, args.timeout)
                    model['resource_measurement'] = parse_darwin_time(directory / 'time-l.log')
                    model['status'] = 'PROCESS_FAILED' if model['process']['exit_code'] != 0 else 'PROCESS_EXIT_ZERO'
                    result_path = Path(model['output_json'])
                    if result_path.is_file():
                        model['runner_result'] = read_json(result_path)
                        model['errors'].extend(result_errors(model['runner_result'], model, args.mode))
                    else:
                        model['errors'].append('MISSING_RUNNER_JSON')
                    stdout_path = directory / 'stdout.log'
                    if stdout_path.stat().st_size:
                        try:
                            model['stdout_json'] = read_json(stdout_path)
                        except (ValueError, UnicodeDecodeError):
                            model['stdout_json'] = None
                    for identity in model['inputs'] + [model['inventory']]:
                        checked_file(Path(identity['path']), identity['sha256'], identity['bytes'])
                    checked_file(Path(binary['path']), binary['sha256'])
                except ProcessInterrupted as error:
                    model['process'] = {'exit_code': 'INTERRUPTED', 'signal': error.signum, 'cleanup': error.cleanup}
                    model['process']['wall_seconds'] = time.monotonic() - process_started if process_started is not None else None
                    model['status'] = 'INTERRUPTED'
                    manifest['status'] = 'INTERRUPTED'
                except CleanupUnfinalized as error:
                    raw_finalized = False
                    model['errors'].append(str(error))
                    model['status'] = 'RAW_UNFINALIZED_LIVE_WRITER_POSSIBLE'
                    manifest['status'] = 'FAILED_GROUP_CLEANUP'
                except (OSError, ValueError, RuntimeError) as error:
                    model['errors'].append(str(error))
                    model['status'] = 'FAILED'
                finally:
                    model['finished_utc'] = utc()
                    model['raw_finalized'] = raw_finalized
                    model['files'] = [fingerprint(p) for p in sorted(directory.iterdir()) if p.is_file()] if raw_finalized else []
                    write_json(manifest_path, manifest)
                if manifest['status'] in ['INTERRUPTED', 'FAILED_GROUP_CLEANUP']:
                    break
            if manifest['status'] not in ['INTERRUPTED', 'FAILED_GROUP_CLEANUP']:
                good = all(x['status'] == 'PROCESS_EXIT_ZERO' and not x['errors'] and
                           x.get('resource_measurement', {}).get('status') == 'RECORDED' for x in manifest['models'])
                manifest['status'] = ('QUOTE_ONLY_COMPLETED' if args.mode == 'quote-only' else 'SELF_CONSISTENCY_PROCESS_COMPLETED') if good else 'FAILED_OR_INCOMPLETE'
                manifest['quote_status' if args.mode == 'quote-only' else 'inference_status'] = 'PROCESS_COMPLETED' if good else 'FAILED_OR_INCOMPLETE'
        manifest['source_after'] = source_identity(args.source_file)
        manifest['source_identity_unchanged'] = manifest['source_after'] == before
        if not manifest['source_identity_unchanged']:
            manifest['status'] = 'FAILED_SOURCE_CHANGED'
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        manifest['status'] = 'PREFLIGHT_OR_DRIVER_FAILED'
        manifest['preflight_errors'].append(str(error))
    manifest['finished_utc'] = utc()
    write_json(manifest_path, manifest)
    print(json.dumps({'status': manifest['status'], 'manifest': str(manifest_path)}))
    return 0 if manifest['status'] in ['NOT_RUN_BUILD_NOT_REQUESTED', 'NOT_RUN_PREFLIGHT_ONLY', 'QUOTE_ONLY_COMPLETED', 'SELF_CONSISTENCY_PROCESS_COMPLETED'] else 1


if __name__ == '__main__':
    sys.dont_write_bytecode = True
    raise SystemExit(main())
