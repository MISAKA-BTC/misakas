from pathlib import Path
import subprocess, os, hashlib, json, time, re

root = Path.cwd()
out = root / 'docs/evidence/positive-court-a0-2026-10-31/integration'
stem = 'wide-admission-stack-joined-20261101'
paths = ['Cargo.toml', 'Cargo.lock']
for base in ['consensus/core/src', 'consensus/core/tests', 'misaka/palw/tir', 'misaka/palw/sdk', 'testing/reference/palw-tir-ref2/src']:
    paths.extend(str(p) for p in Path(base).rglob('*') if p.is_file() and p.suffix in ['.rs', '.toml'])
def digest(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def snapshot():
    return {p: digest(p) for p in sorted(set(paths))}
envadd = {
    'CARGO_TARGET_DIR': '/Users/wata/.codex/worktrees/positive-court-a0/target',
    'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '2',
    'ROCKSDB_STATIC': '1',
    'ROCKSDB_LIB_DIR': '/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out',
}
archive = Path(envadd['ROCKSDB_LIB_DIR']) / 'librocksdb.a'
d = {
    'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'source_before': snapshot(), 'environment_overrides': envadd,
    'inherited_RUST_MIN_STACK': os.environ.get('RUST_MIN_STACK'),
    'effective_RUST_MIN_STACK': None,
    'archive': {'path': str(archive), 'bytes': archive.stat().st_size, 'sha256': digest(archive)},
    'preceding_review': 'A1 78e274630 and A4 13e independently reviewed; A4 first-run stack/Clippy failures retained; mechanical object dispatch verifier and independent review ready; actual default stack not yet passed',
    'scope': 'Same-source defaultstack SDK16 + old state/carriage/T12 goldens and rollback/receipt regressions; explicit purewide admission and sizing plus old GEN pricing. No native acceptance/credit/funding or trained LLM registration qualification',
    'results': [],
}
commands = [
    ['cargo', 'test', '-p', 'misaka-palw-sdk', '--test', 'runtime_pack'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--test', 'rcore_m5_v22_golden', '--test', 't12_regenesis', '--test', 'palw_gen_close_price', '--test', 'palw_tir_close_size_wide_v2', '--test', 'palw_tir_close_size', '--test', 'palw_tir_close_range', '--test', 'palw_gen_close'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'palw_tir_admission_v2::tests'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'wide_price_arithmetic_tests'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'the_version_22_state_root_golden_vectors'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'court_deadline_workspace_v2'],
    ['cargo', 'test', '-p', 'kaspa-consensus-core', '--lib', 'g14_model_panel_one_v1'],
]
for i, cmd in enumerate(commands):
    cmd += ['--offline', '--locked', '--config', 'profile.dev.package.kaspa-consensus-core.opt-level=0', '--config', 'profile.test.package.kaspa-consensus-core.opt-level=0']
    log = out / f'{stem}-{i}.log'
    if log.exists():
        raise RuntimeError(f'refusing to overwrite evidence {log}')
    start = time.monotonic()
    with log.open('w') as f:
        runenv = os.environ | envadd
        runenv.pop('RUST_MIN_STACK', None)
        runenv.pop('RUSTFLAGS', None)
        runenv.pop('CARGO_ENCODED_RUSTFLAGS', None)
        result = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT, env=runenv)
    content = log.read_text()
    passed = sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed', content)))
    binaries = []
    for path in re.findall(r'Running [^\n]*\((/[^\n]+)\)', content):
        p = Path(path)
        binaries.append({'path': path, 'bytes': p.stat().st_size, 'sha256': digest(p)})
    summaries = [{'status': m[0], 'passed': int(m[1]), 'failed': int(m[2])} for m in re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed', content)]
    all_targets_positive = len(summaries) == len(binaries) and bool(summaries) and all(x['status'] == 'ok' and x['passed'] > 0 and x['failed'] == 0 for x in summaries)
    item = {
        'command': cmd, 'exit_code': result.returncode, 'passed': passed,
        'effective_success': result.returncode == 0 and all_targets_positive,
        'test_summaries': summaries, 'all_targets_positive': all_targets_positive,
        'elapsed_seconds': time.monotonic() - start, 'log': log.name,
        'sha256': digest(log), 'binaries': binaries,
    }
    d['results'].append(item)
    d['source_after'] = snapshot()
    d['source_stable'] = d['source_before'] == d['source_after']
    d['archive_stable'] = digest(archive) == d['archive']['sha256']
    (out / f'{stem}.json').write_text(json.dumps(d, indent=2) + '\n')
    print(json.dumps(item), flush=True)
    if not item['effective_success'] or not d['source_stable']:
        break
