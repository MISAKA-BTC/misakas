import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

root = Path('/Users/wata/.codex/worktrees/positive-court-a1/MISAKA-testnet')
prefix = Path('/tmp/misaka-a1-row-profile-2026-11-01')
archive = Path('/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out/librocksdb.a')
target = Path('/Users/wata/.cache/misaka-positive-a1-target')
def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()
def git(*args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()
files = ['Cargo.lock', 'Cargo.toml', 'consensus/core/Cargo.toml', 'consensus/core/src/lib.rs',
         'consensus/core/src/palw_tir_row_profile_v2.rs', 'consensus/core/src/palw_tir_row_profile_v2/tests.rs',
         'consensus/core/src/palw_tir_class_v1.rs', 'consensus/core/src/palw_tir_class_record_v2.rs',
         'consensus/core/src/palw_tir_profile_resolver_v2.rs', 'consensus/core/src/palw_tir_admission_v1.rs',
         'consensus/core/src/palw_tir_attempt_v1.rs', 'consensus/core/src/palw_tir_dissect_v1.rs',
         'misaka/palw/tir/core/src/program.rs', 'misaka/palw/tir/core/src/validate.rs',
         'misaka/palw/tir/core/src/types.rs']
envvars = {'CARGO_TARGET_DIR': str(target), 'CARGO_INCREMENTAL': '0',
           'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_TEST_DEBUG': '0',
           'CARGO_BUILD_JOBS': '2', 'ROCKSDB_STATIC': '1', 'ROCKSDB_LIB_DIR': str(archive.parent)}
assert not git('status', '--porcelain'), 'source must be clean before validation'
assert target.is_dir(), 'only the authorized existing cache is permitted'
data = {'candidate': git('rev-parse', 'HEAD'), 'tree': git('rev-parse', 'HEAD^{tree}'),
        'base': git('rev-parse', '017bff082'), 'source_sha256': {p: sha(root / p) for p in files},
        'archive': {'path': str(archive), 'size': archive.stat().st_size, 'sha256_before': sha(archive)},
        'env': envvars, 'commands': [], 'binaries': [],
        'started_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}
Path(str(prefix) + '-inputs.json').write_text(json.dumps(data, indent=2) + '\n')
common = ['--offline', '--locked', '--config', 'profile.dev.package.kaspa-consensus-core.opt-level=0']
commands = [
    ('row-tests', ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'palw_tir_row_profile_v2', *common]),
    ('identity-test', ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'digest_helper_preserves_legacy_preimage_and_binds_every_part', *common]),
    ('clippy', ['cargo', 'clippy', '-p', 'kaspa-consensus-core', '--lib', '--no-deps', *common, '--', '-D', 'warnings'])]
env = os.environ.copy()
env.update(envvars)
for name, argv in commands:
    log = Path(str(prefix) + '-' + name + '.log')
    print('Starting ' + name, flush=True)
    start = time.monotonic()
    with log.open('xb') as output:
        result = subprocess.run(argv, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
    data['commands'].append({'name': name, 'argv': argv, 'exit_code': result.returncode,
                             'elapsed_seconds': round(time.monotonic()-start, 3), 'log': str(log), 'log_sha256': sha(log)})
    content = log.read_text()
    for relative in re.findall(r'Running unittests [^\n]* \(([^)]+)\)', content):
        binary = Path(relative)
        if not binary.is_absolute():
            binary = root / binary
        if binary.is_file() and not any(x['path'] == str(binary) for x in data['binaries']):
            data['binaries'].append({'path': str(binary), 'size': binary.stat().st_size, 'sha256': sha(binary)})
    print(name + ' exit=' + str(result.returncode), flush=True)
    Path(str(prefix) + '-result.json').write_text(json.dumps(data, indent=2) + '\n')
    if result.returncode:
        print(content[-9000:], flush=True)
        break
data['source_unchanged_during_run'] = all(sha(root / p) == expected for p, expected in data['source_sha256'].items())
data['tracked_source_clean_after'] = not bool(git('status', '--porcelain'))
data['archive']['sha256_after'] = sha(archive)
data['static_link_output'] = []
for output in (target / 'debug/build').glob('librocksdb-sys-*/output'):
    text = output.read_text()
    if 'static=rocksdb' in text and str(archive.parent) in text:
        data['static_link_output'].append({'path': str(output), 'sha256': sha(output)})
data['finished_at_utc'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
Path(str(prefix) + '-result.json').write_text(json.dumps(data, indent=2) + '\n')
assert data['source_unchanged_during_run'] and data['tracked_source_clean_after']
assert data['archive']['sha256_before'] == data['archive']['sha256_after']
assert data['binaries'] and data['static_link_output']
raise SystemExit(data['commands'][-1]['exit_code'])
