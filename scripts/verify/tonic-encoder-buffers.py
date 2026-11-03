#!/usr/bin/env python3
"""Run vendored tonic encoder regressions with a fresh, offline Root-version harness.

Only a NEW work directory is written. Does not use vendor/tonic/Cargo.lock,
modify repository/registry source, install dependencies, or operate any node.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import time
import tomllib

NORMAL = ('base64', 'bytes', 'flate2', 'http', 'http-body', 'http-body-util',
          'percent-encoding', 'pin-project', 'sync_wrapper', 'tokio-stream',
          'tower-layer', 'tower-service', 'tracing')
DEV = ('static_assertions', 'tokio', 'tower', 'hyper')


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_files(root):
    files = {}
    require(root.is_dir() and not root.is_symlink(), 'vendor directory missing/symlink')
    for path in sorted(root.rglob('*')):
        mode = path.lstat().st_mode
        require(stat.S_ISDIR(mode) or stat.S_ISREG(mode), 'vendor symlink/special file')
        if stat.S_ISREG(mode):
            files[str(path.relative_to(root))] = digest(path)
    return files


def project_manifest(root_lock, vendor_manifest):
    packages = tomllib.loads(root_lock)['package']
    def version(name):
        rows = [p for p in packages if p['name'] == name and p.get('source')]
        require(len(rows) == 1, 'Root version is absent/ambiguous: ' + name)
        return rows[0]['version']
    package = vendor_manifest['package']
    require(package['name'] == 'tonic' and package['version'] == '0.14.5', 'unexpected tonic package')
    rows = ['[package]', 'name = "tonic"', 'version = "0.14.5"', 'edition = "2021"',
            'license = "MIT"', '', '[lib]', 'name = "tonic"', 'path = "src/lib.rs"',
            '', '[features]', 'default = []']
    for feature in vendor_manifest['features']:
        if feature != 'default':
            rows.append(json.dumps(feature) + ' = ' + (' ["dep:flate2"]' if feature == 'gzip' else ' []'))
    for name in NORMAL + DEV:
        kind = 'dependencies' if name in NORMAL else 'dev-dependencies'
        rows += ['', '[' + kind + '.' + name + ']', 'version = ' + json.dumps('=' + version(name))]
        if name == 'flate2':
            rows.append('optional = true')
        if name == 'tokio-stream':
            rows.append('default-features = false')
        if name == 'tokio':
            rows.append('features = ["rt-multi-thread", "macros", "test-util"]')
        if name == 'tower':
            rows.append('features = ["load-shed", "timeout", "util", "limit"]')
    return '\n'.join(rows + ['', '[workspace]', ''])


def check_graph(lock, root_lock):
    known = {(p['name'], p['version'], p.get('source'), p.get('checksum'))
             for p in tomllib.loads(root_lock)['package'] if p.get('source')}
    selected = []
    for package in tomllib.loads(lock)['package']:
        if package.get('source'):
            row = tuple(package.get(k) for k in ('name', 'version', 'source', 'checksum'))
            require(row in known, 'non-Root dependency resolution: ' + repr(row))
            selected.append(row)
        else:
            require(package['name'] == 'tonic' and package['version'] == '0.14.5', 'unexpected path dependency')
    return sorted(selected)


def allocated(root):
    total = 0
    if root.exists():
        for path in root.rglob('*'):
            try:
                info = path.lstat()
            except FileNotFoundError:  # Cargo may replace/remove its own temporary output.
                continue
            require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), 'target symlink/special file')
            if stat.S_ISREG(info.st_mode):
                total += info.st_blocks * 512
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--work-dir', type=Path, required=True, help='must not already exist or be inside the repository')
    parser.add_argument('--timeout-seconds', type=int, default=900)
    parser.add_argument('--max-target-bytes', type=int, default=1536 << 20)
    args = parser.parse_args()
    repo = args.repo.resolve()
    require(not os.path.lexists(args.work_dir), 'work directory already exists; never reuse a prior root-package target')
    work = args.work_dir.resolve()
    require(not work.is_relative_to(repo), 'work directory must be outside the repository')
    require(0 < args.timeout_seconds <= 3600 and args.max_target_bytes > 0, 'invalid resource bound')
    for key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
        require(not os.environ.get(key), 'unset compiler override before reproduction: ' + key)
    root_lock_path = repo / 'Cargo.lock'
    root_lock = root_lock_path.read_text()
    root_lock_hash = digest(root_lock_path)
    vendor = repo / 'vendor/tonic'
    before = source_files(vendor)
    test_relative = 'src/codec/oversized_encode_tests.rs'
    require(test_relative in before and 'src/codec/encode.rs' in before, 'vendored encoder tests missing')
    expected = set(re.findall(r'#\[test\]\s*fn\s+([a-zA-Z0-9_]+)', (vendor / test_relative).read_text()))
    require(len(expected) == 9, 'expected exactly nine reviewed encoder regressions')
    channel = tomllib.loads((repo / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    found = subprocess.run(['rustup', 'which', '--toolchain', channel, 'cargo'], capture_output=True, text=True)
    require(found.returncode == 0, 'pinned toolchain must already be installed; no installation attempted')
    cargo = found.stdout.strip()
    work.mkdir(parents=True, mode=0o700)
    workspace = work / 'tonic'
    shutil.copytree(vendor, workspace)
    (workspace / 'Cargo.toml').write_text(project_manifest(root_lock, tomllib.loads((vendor / 'Cargo.toml').read_text())))
    # Root's lock is the seed. The upstream vendored lock is never used as the version authority.
    (workspace / 'Cargo.lock').write_text(root_lock)
    target = work / 'target'
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS='1',
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN=channel)
    started = time.monotonic()
    commands = []
    result = {'scope': 'tonic gzip-only unit harness; not full transport/Node or a heap-owner measurement',
              'rootLockSha256': root_lock_hash, 'vendorSourceBefore': before, 'toolchain': channel}

    def run(argv, label):
        stdout, stderr = work / (label + '.stdout'), work / (label + '.stderr')
        with stdout.open('xb') as out, stderr.open('xb') as err:
            child = subprocess.Popen(argv, cwd=work, env=env, stdout=out, stderr=err, start_new_session=True)
            try:
                while child.poll() is None:
                    time.sleep(0.25)
                    require(time.monotonic() - started < args.timeout_seconds, 'finite runtime limit')
                    require(allocated(target) <= args.max_target_bytes, 'target allocation limit')
                    require(stdout.stat().st_size + stderr.stat().st_size < 8 << 20, 'output limit')
            except BaseException:
                if child.poll() is None:
                    os.killpg(child.pid, signal.SIGINT)
                    try:
                        child.wait(timeout=8)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                raise
        require(allocated(target) <= args.max_target_bytes, 'final target allocation limit')
        require(stdout.stat().st_size + stderr.stat().st_size < 8 << 20, 'final output limit')
        commands.append({'argv': argv, 'exitCode': child.returncode,
                         'stdout': stdout.name, 'stdoutSha256': digest(stdout),
                         'stderr': stderr.name, 'stderrSha256': digest(stderr)})
        require(child.returncode == 0, 'command failed: ' + label + '; retained logs in ' + str(work))
        return stdout

    common = [cargo, '--manifest-path', str(workspace / 'Cargo.toml')]
    try:
        # Metadata may prune/update ONLY the NEW copied lock; refuse all non-Root registry rows before compilation.
        meta = ['metadata', '--offline', '--format-version=1', '--no-default-features', '--features', 'gzip']
        run([common[0], *meta, *common[1:]], 'metadata-prepare')
        selected = check_graph((workspace / 'Cargo.lock').read_text(), root_lock)
        frozen_lock = digest(workspace / 'Cargo.lock')
        run([common[0], *meta, '--locked', *common[1:]], 'metadata-locked')
        require(digest(workspace / 'Cargo.lock') == frozen_lock, 'locked metadata changed the lock')
        output = run([common[0], 'test', '--offline', '--locked', *common[1:], '--no-default-features',
                      '--features', 'gzip', '--lib', '--no-run', '--message-format=json-render-diagnostics'], 'compile')
        artifacts = []
        for line in output.read_text().splitlines():
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if item.get('reason') == 'compiler-artifact' and item.get('executable') and item.get('profile', {}).get('test'):
                artifacts.append(item)
        require(len(artifacts) == 1, 'exactly one root test artifact required')
        artifact = artifacts[0]
        require(artifact.get('fresh') is False and artifact['target']['src_path'] == str(workspace / 'src/lib.rs')
                and artifact.get('manifest_path') == str(workspace / 'Cargo.toml')
                and artifact['package_id'].startswith('path+file://' + str(workspace) + '#'), 'artifact source/root binding failed')
        executable = Path(artifact['executable'])
        require(executable.parent == target / 'debug/deps' and executable.is_file() and not executable.is_symlink(), 'artifact target binding failed')
        executable_hash = digest(executable)
        listing = run([str(executable), 'oversized_encode_tests', '--list'], 'list')
        actual = set(re.findall(r'^codec::encode::oversized_encode_tests::([a-zA-Z0-9_]+): test$', listing.read_text(), re.M))
        require(actual == expected, 'compiled binary does not contain the exact nine tests')
        stdout = run([str(executable), 'oversized_encode_tests', '--test-threads=1'], 'tests')
        require('9 passed; 0 failed' in stdout.read_text(), 'nine regressions did not pass')
        copied_unchanged = all(digest(workspace / rel) == h for rel, h in before.items()
                               if rel not in ('Cargo.toml', 'Cargo.lock'))
        require(copied_unchanged, 'copied workspace source changed')
        require(digest(executable) == executable_hash and source_files(vendor) == before
                and digest(root_lock_path) == root_lock_hash, 'source, Root lock, or executable changed')
        result.update(success=True, actualPassed=9, actualFailed=0, artifact=artifact,
                      executableSha256=executable_hash, harnessLockSha256=frozen_lock,
                      registryRowsExactRootSubset=selected, targetAllocatedBytes=allocated(target))
    except BaseException as error:
        result.update(success=False, error=str(error))
        raise
    finally:
        result['commands'] = commands
        result['elapsedSeconds'] = time.monotonic() - started
        (work / 'result.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
        print(json.dumps({'result': str(work / 'result.json'), 'success': result.get('success', False)}))


if __name__ == '__main__':
    main()
