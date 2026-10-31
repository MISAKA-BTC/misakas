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
SOURCE_TREES = ['consensus/core','consensus-vectors','crypto','math','utils','core','wasm/core',
                'misaka','docs/design/palw/positive-court',
                'testing/positive-court-a7']

def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

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
    for line in data.splitlines():
        running = re.search(r'Running tests/(\S+)\.rs\s', line)
        if running:
            target = Path(running[1]).name
        elif re.search(r'Running unittests |Doc-tests ', line):
            target = None
        summary = re.match(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored', line)
        if summary:
            counts.append(dict(target=target, passed=int(summary[1]), failed=int(summary[2]), ignored=int(summary[3])))
    return counts

def required_target_errors(command, counts):
    required = [command[i+1] for i,x in enumerate(command[:-1]) if x=='--test']
    return [target for target in required if not any(c.get('target')==target and c['passed']>0 and c['failed']==0 for c in counts)]

def final_identity_errors(root, record, current_commit):
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
    return errors

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--independent-only', action='store_true', help='diagnostic subset; never joint-suite PASS')
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    build_env = {'CARGO_INCREMENTAL':'0', 'CARGO_PROFILE_DEV_DEBUG':'0', 'CARGO_PROFILE_TEST_DEBUG':'0', 'CARGO_TARGET_DIR':str(ROOT/'target'), 'RUST_MIN_STACK':'16777216', 'CARGO_BUILD_JOBS':'2', 'CARGO_TERM_COLOR':'never'}
    env = dict(os.environ, PC_A7_REPORT_DIR=str(out), **build_env)
    source = source_snapshot(ROOT, [out])
    record = {'schema':'a7-parallel-audit-v3','commit':capture('git','rev-parse','HEAD'), 'status_before':capture('git','status','--short'),
        'started_utc':datetime.now(timezone.utc).isoformat(), 'host':{'platform':platform.platform(),'cpus':os.cpu_count()},
        'output_path':str(out),
        'rustc':capture('rustc','-Vv'),'cargo':capture('cargo','-V'),'build_env':build_env,'source_sha256':source,
        'cargo_lock_sha256':{str(path.relative_to(ROOT)):sha(path) for path in [ROOT/'Cargo.lock',LOCAL/'Cargo.lock'] if path.exists()},
        'activation_allowed':False,'unverified':['native binding/protected-store custody for A2 duties','A3 real provider/cold network delivery','real LLM','native network/carrier',
            'native Gate C/reward/DAG/settlement production reads','A6 r1 atomic service/attempt/fund/crash/reorg/IBD recovery','approved caps/p95/p99/concurrency'], 'commands':[]}
    commands = [
        [sys.executable,'-m','unittest','discover','-s','testing/positive-court-a7','-p','test_*.py','-v'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--test','pipeline','--no-run','--message-format=json'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--test','pipeline','--','--nocapture','--test-threads=1'],
    ]
    if not args.independent_only:
        commands += [
            ['cargo','test','-p','misaka-palw-positive-contract','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-positive-accounting','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-positive-accounting-node','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-remote','--offline','--locked','--features','positive-panel','--lib','miner::positive_v2'],
            ['cargo','test','-p','misaka-palw-remote','--offline','--locked','--features','positive-panel','--test','positive_panel_r1','--test','positive_panel_sources_r1'],
            ['cargo','test','-p','misaka-palw-tir-exec','--offline','--locked','--features','positive-panel','--test','positive_panel_r1','--test','positive_contract','--test','positive_check_hardening','--test','positive_check','--test','court','--test','node_leg','--test','capture_codec'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--lib','config::params::positive_court_v2::tests::'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--test','palw_tir_v1_fence'],
        ]
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
            counts=[{'passed':int(re.search(r'Ran (\d+) tests',data)[1]),'failed':0,'ignored':0}]
        entry['test_counts']=counts
        if i!=1 and entry['exit_code']==0 and sum(x['passed'] for x in counts)==0:
            entry['validation_error']='NO_EXECUTED_TESTS'
        missing=required_target_errors(command,counts) if i!=1 else []
        if entry['exit_code']==0 and missing:
            entry['validation_error']='REQUIRED_TEST_TARGET_EMPTY_OR_MISSING'
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
