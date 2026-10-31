#!/usr/bin/env python3
"""A7 same-candidate checks; no partial/native/LLM PASS from toy measurements."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time
from process_control import ProcessInterrupted, run_group

ROOT = Path(__file__).resolve().parents[2]
LOCAL = Path(__file__).resolve().parent
SOURCE_TREES = ['consensus','database','consensus-vectors','crypto','math','utils','core','wasm/core',
                'misaka','docs/design/palw/positive-court',
                'testing/positive-court-a7']

# Literal acceptance inventory pinned to the A7/A0 candidate test functions.
# Runtime logs must contain every name as an actual successful harness line.
REQUIRED_TARGETS = {
    "pipeline": {
        "min_passed": 8,
        "passed_names": [
            "bound_a2_rejects_parameters_that_do_not_open_to_the_fixed_registration",
            "bound_a2_foreign_class_job_and_prompt_never_reach_the_signer",
            "committed_a2_a3_a1_and_a5_connect_with_independent_expected_tokens",
            "checked_math_does_not_replace_gate_c_funding_maturity_or_current_fraud_state",
            "a2_signed_wrong_output_is_checked_mismatch_and_never_private_credit",
            "missing_last_a3_frame_cannot_produce_a5_terminal_authority",
            "paid_success_has_only_existing_worker_payment_and_settlement_after_real_checked_execution",
            "omitted_existing_entitlement_port_denies_even_after_real_checked_execution_and_all_prerequisites"
        ]
    },
    "positive_court_fence_v2": {
        "min_passed": 8,
        "passed_names": [
            "checks::positive2_policy_root_matches_independent_python_fixed_schema_vector",
            "checks::positive2_released_presets_are_absent_and_none_writes_no_fingerprint_field",
            "checks::positive2_never_collapses_the_whole_profile_but_raw_diagnostics_commit_it",
            "checks::positive2_scheduled_profile_preserves_pre_fork_peer_identity_and_is_not_a_public_permit",
            "checks::positive2_all_profile_fields_are_committed",
            "checks::positive2_rejects_unsupported_ids_zero_unbounded_overflow_and_inconsistent_caps",
            "checks::positive2_ruleset_binds_only_the_fired_fork_and_current_policy",
            "checks::positive2_raw_tag_is_opaque_before_decode_signature_resources_or_funding"
        ]
    }
}

def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()

def build_inputs(root, environ):
    """Resolve existing build inputs without creating or changing any cache."""
    target_value = environ.get('CARGO_TARGET_DIR', str(root/'target'))
    if not target_value:
        raise ValueError('CARGO_TARGET_DIR must name an existing directory')
    target = Path(target_value)
    target = (target if target.is_absolute() else root/target).resolve()
    if not target.is_dir():
        raise ValueError('CARGO_TARGET_DIR does not exist: '+str(target))
    build_env = {'CARGO_INCREMENTAL':'0', 'CARGO_PROFILE_DEV_DEBUG':'0', 'CARGO_PROFILE_TEST_DEBUG':'0',
                 'CARGO_TARGET_DIR':str(target), 'RUST_MIN_STACK':'16777216', 'CARGO_BUILD_JOBS':'2', 'CARGO_TERM_COLOR':'never'}
    native_archive = None
    if 'ROCKSDB_LIB_DIR' in environ:
        value = environ['ROCKSDB_LIB_DIR']
        if not value:
            raise ValueError('ROCKSDB_LIB_DIR must name an existing archive directory')
        directory = Path(value)
        directory = (directory if directory.is_absolute() else root/directory).resolve()
        archive = (directory/'librocksdb.a').resolve()
        if not archive.is_file():
            raise ValueError('missing existing RocksDB archive: '+str(archive))
        native_archive = {'path':str(archive), 'sha256':sha(archive),
                          'ROCKSDB_LIB_DIR':value, 'ROCKSDB_STATIC':environ.get('ROCKSDB_STATIC')}
        # Build scripts may run from a crate directory; pin the same resolved
        # directory whose archive was hashed, while retaining the raw env above.
        build_env['ROCKSDB_LIB_DIR'] = str(directory)
    if 'ROCKSDB_STATIC' in environ:
        build_env['ROCKSDB_STATIC'] = environ['ROCKSDB_STATIC']
    return {'build_env':build_env, 'native_archive':native_archive}

def source_snapshot(root, excluded=()):
    source = {}
    for tree in SOURCE_TREES:
        for path in sorted((root/tree).rglob('*')):
            if any(path==directory or directory in path.parents for directory in excluded):continue
            relative = path.relative_to(root)
            if path.is_file() and path.suffix in {'.rs','.toml','.py','.md','.json'} and not any(x in relative.parts for x in ['runs','evidence','validation','__pycache__']):
                source[str(relative)] = sha(path)
    for name in ['Cargo.toml','.cargo/config.toml','rust-toolchain.toml','rust-toolchain']:
        path=root/name
        if path.is_file():source[name]=sha(path)
    return source

def test_counts(data):
    counts = []
    target = None
    passed_names = set()
    for line in data.splitlines():
        running = re.search(r'Running tests/(\S+)\.rs\s', line)
        if running:
            target = Path(running[1]).name
            passed_names = set()
        elif re.search(r'Running unittests |Doc-tests ', line):
            target = None
            passed_names = set()
        elif re.fullmatch(r'running \d+ tests?', line):
            passed_names = set()
        passed = re.fullmatch(r'test (\S+) \.\.\. ok', line)
        if passed:
            passed_names.add(passed[1])
        summary = re.match(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored', line)
        if summary:
            counts.append(dict(target=target, passed=int(summary[1]), failed=int(summary[2]),
                               ignored=int(summary[3]), passed_names=sorted(passed_names)))
            # Neither a later summary nor another target can borrow these names.
            passed_names = set()
    return counts

def required_target_errors(command, counts):
    required = [command[i+1] for i,x in enumerate(command[:-1]) if x=='--test']
    missing = []
    for target in required:
        requirement = REQUIRED_TARGETS.get(target, {'min_passed':1, 'passed_names':[]})
        if not any(c.get('target')==target and c['passed']>=requirement['min_passed'] and c['failed']==0
                   and set(requirement['passed_names']).issubset(c.get('passed_names', [])) for c in counts):
            missing.append(target)
    return missing

def suite_requirement_errors(entries, independent_only=False):
    """Require actual successful executions, even if the command plan omitted one."""
    required = ['pipeline'] if independent_only else list(REQUIRED_TARGETS)
    missing = []
    for target in required:
        completed = False
        for entry in entries:
            command = entry.get('argv', [])
            # Cargo target selection precedes the harness separator. A build
            # artifact or a failed/incomplete command cannot stand in for a run.
            cargo_args = command[:command.index('--')] if '--' in command else command
            if (not cargo_args or cargo_args[0]!='cargo' or 'test' not in cargo_args
                    or '--no-run' in cargo_args or entry.get('exit_code')!=0 or 'validation_error' in entry):
                continue
            explicit = [cargo_args[i+1] for i,x in enumerate(cargo_args[:-1]) if x=='--test']
            if target in explicit and not required_target_errors(['--test', target], entry.get('test_counts', [])):
                completed = True
                break
        if not completed:
            missing.append(target)
    return missing

def final_identity_errors(root, record, current_commit, environ=None):
    environ = os.environ if environ is None else environ
    errors = []
    if current_commit != record['commit']:
        errors.append('HEAD_CHANGED')
    excluded=[Path(record['output_path'])] if 'output_path' in record else []
    if source_snapshot(root, excluded) != record['source_sha256']:
        errors.append('SOURCE_SET_OR_BYTES_CHANGED')
    for name, expected in record['cargo_lock_sha256'].items():
        path = root/name
        if not path.is_file() or sha(path)!=expected:
            errors.append('LOCK_CHANGED:'+name)
    binary = record.get('a7_binary')
    if not binary or not Path(binary['path']).is_file() or sha(Path(binary['path']))!=binary['sha256']:
        errors.append('BINARY_CHANGED_OR_MISSING')
    native = record.get('native_archive')
    if native is not None:
        if any(environ.get(name)!=native[name] for name in ['ROCKSDB_LIB_DIR','ROCKSDB_STATIC']):
            errors.append('NATIVE_ARCHIVE_ENV_CHANGED')
        value = environ.get('ROCKSDB_LIB_DIR')
        if value:
            directory = Path(value)
            current_path = ((directory if directory.is_absolute() else root/directory)/'librocksdb.a').resolve()
            if str(current_path)!=native['path']:
                errors.append('NATIVE_ARCHIVE_PATH_CHANGED')
        archive = Path(native['path'])
        if not archive.is_file() or sha(archive)!=native['sha256']:
            errors.append('NATIVE_ARCHIVE_CHANGED_OR_MISSING')
    return errors

def suite_commands(python, independent_only=False):
    """Return command inventory without building or executing any target."""
    commands = [
        [python,'-m','unittest','discover','-s','testing/positive-court-a7','-p','test_*.py','-v'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--test','pipeline','--no-run','--message-format=json'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--test','pipeline','--','--nocapture','--test-threads=1'],
    ]
    if not independent_only:
        commands += [
            ['cargo','test','-p','misaka-palw-positive-contract','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-positive-accounting','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-positive-accounting-node','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-remote','--offline','--locked','--features','positive-panel','--lib','miner::positive_v2'],
            ['cargo','test','-p','misaka-palw-remote','--offline','--locked','--features','positive-panel','--test','positive_panel_r1','--test','positive_panel_sources_r1','--test','positive_native_source_v2','--test','positive_panel_record_r1'],
            ['cargo','test','-p','misaka-palw-tir-exec','--offline','--locked','--features','positive-panel','--test','positive_panel_r1','--test','positive_contract','--test','positive_check_hardening','--test','positive_check','--test','court','--test','node_leg','--test','capture_codec','--test','positive_full_relation','--test','positive_expanded_plan','--test','positive_production_port','--test','positive_typed_capture','--test','positive_record','--test','positive_court_a4_integration','--test','positive_court_a4_admission','--test','positive_court_a4_native'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--test','positive_court_fence_v2','--test','positive_source_v2','--test','positive_lifecycle_predecode_v2','--test','palw_tir_v1_fence'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--lib','palw_positive_'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--lib','palw_court_shared'],
        ]
    return commands

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--independent-only', action='store_true', help='diagnostic subset; never joint-suite PASS')
    args = p.parse_args()
    out = args.out.resolve()
    try:
        inputs = build_inputs(ROOT, os.environ)
    except (ValueError, OSError) as error:
        p.error(str(error))
    out.mkdir(parents=True, exist_ok=False)
    build_env = inputs['build_env']
    env = dict(os.environ, PC_A7_REPORT_DIR=str(out), **build_env)
    source = source_snapshot(ROOT, [out])
    record = {'schema':'a7-parallel-audit-v4','requirements':REQUIRED_TARGETS,'commit':capture('git','rev-parse','HEAD'), 'status_before':capture('git','status','--short'),
        'started_utc':datetime.now(timezone.utc).isoformat(), 'host':{'platform':platform.platform(),'cpus':os.cpu_count()},
        'output_path':str(out),
        'rustc':capture('rustc','-Vv'),'cargo':capture('cargo','-V'),'build_env':build_env,'native_archive':inputs['native_archive'],'source_sha256':source,
        'cargo_lock_sha256':{str(path.relative_to(ROOT)):sha(path) for path in [ROOT/'Cargo.lock',LOCAL/'Cargo.lock'] if path.exists()},
        'activation_allowed':False,'unverified':['native binding/protected-store custody for A2 duties','A3 real provider/cold network delivery','real LLM','native network/carrier',
            'native Gate C/reward/DAG/settlement production reads','A6 r1 atomic service/attempt/fund/crash/reorg/IBD recovery','approved caps/p95/p99/concurrency'], 'commands':[]}
    commands = suite_commands(sys.executable, args.independent_only)
    for i, command in enumerate(commands):
        if command[0]=='cargo':
            command = [command[0],'--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.test.package.kaspa-consensus-core.opt-level=0',*command[1:]]
        print(f'A7 [{i+1}/{len(commands)}] '+ ' '.join(command), flush=True)
        log=out/f'{i:02d}.log'
        started=time.monotonic()
        try:
            entry=run_group(command,cwd=ROOT,env=env,log=log,timeout=900)
        except ProcessInterrupted as interrupted:
            entry={'exit_code':'INTERRUPTED', 'signal':interrupted.signum,
                   'elapsed_seconds':time.monotonic()-started,'cleanup':interrupted.cleanup}
        entry.update(argv=command,log=log.name,sha256=sha(log))
        data=log.read_text()
        counts=test_counts(data)
        if command[0]==sys.executable and 'OK' in data:
            counts=[{'target':None,'passed':int(re.search(r'Ran (\d+) tests',data)[1]),'failed':0,'ignored':0,'passed_names':[]}]
        entry['test_counts']=counts
        if i!=1 and entry['exit_code']==0 and sum(x['passed'] for x in counts)==0:
            entry['validation_error']='NO_EXECUTED_TESTS'
        missing=required_target_errors(command,counts) if i!=1 else []
        if entry['exit_code']==0 and missing:
            if entry.get('validation_error')!='NO_EXECUTED_TESTS':
                entry['validation_error']='REQUIRED_TEST_TARGET_INCOMPLETE'
            entry['missing_targets']=missing
        record['commands'].append(entry)
        if i==1 and entry['exit_code']==0:
            for line in data.splitlines():
                try: item=json.loads(line)
                except ValueError: continue
                if item.get('reason')=='compiler-artifact' and item.get('executable') and item.get('target',{}).get('name') in {'independent','pipeline'}:
                    binary=Path(item['executable']);identity={'path':str(binary),'sha256':sha(binary)}
                    record.setdefault('test_binaries',{})[item['target']['name']]=identity
                    if item['target']['name']=='independent':record['a7_binary']=identity
            missing_binaries={'independent','pipeline'}-set(record.get('test_binaries',{}))
            if missing_binaries:
                entry['validation_error']='MISSING_BINARY_IDENTITY'
                entry['missing_binaries']=sorted(missing_binaries)
        (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
        if entry['exit_code']!=0 or 'validation_error' in entry:
            record['result']='FAILED_OR_INCOMPLETE'
            (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
            print(data[-8000:],file=sys.stderr)
            return 1
    missing = suite_requirement_errors(record['commands'], args.independent_only)
    record['missing_required_targets'] = missing
    if missing:
        record['result']='FAILED_REQUIRED_SUITE_TARGETS'
        (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
        return 1
    report=out/'process-report.json'
    if not report.is_file():raise RuntimeError('missing process evidence')
    record['process_report_sha256']=sha(report)
    record['raw_fixture_sha256']={path.name:sha(path) for path in sorted(out.glob('*.borsh'))}
    identity_errors=final_identity_errors(ROOT,record,capture('git','rev-parse','HEAD'))
    for name,binary in record.get('test_binaries',{}).items():
        if not Path(binary['path']).is_file() or sha(Path(binary['path']))!=binary['sha256']:
            identity_errors.append('TEST_BINARY_CHANGED:'+name)
    record['final_identity_errors']=identity_errors
    if identity_errors:
        record['result']='FAILED_CANDIDATE_IDENTITY_CHANGED'
        (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
        return 1
    record['result']='PASS_DIAGNOSTIC_SUBSET' if args.independent_only else 'PASS_PINNED_PRIVATE_R1_CANDIDATE_ONLY'
    record['finished_utc']=datetime.now(timezone.utc).isoformat()
    (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
    print(record['result'],out)
    return 0

if __name__=='__main__':sys.exit(main())
