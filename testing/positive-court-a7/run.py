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
from process_control import run_group

ROOT = Path(__file__).resolve().parents[2]
LOCAL = Path(__file__).resolve().parent

def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--independent-only', action='store_true', help='diagnostic subset; never joint-suite PASS')
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    build_env = {'CARGO_INCREMENTAL':'0', 'CARGO_PROFILE_DEV_DEBUG':'0', 'CARGO_PROFILE_TEST_DEBUG':'0', 'CARGO_TARGET_DIR':str(ROOT/'target'), 'RUST_MIN_STACK':'16777216'}
    env = dict(os.environ, PC_A7_REPORT_DIR=str(out), **build_env)
    source = {}
    for tree in ['consensus/core','misaka/palw/tir/core','misaka/palw/tir/exec','misaka/palw/positive-contract','misaka/palw/positive-accounting','testing/positive-court-a7']:
        for path in sorted((ROOT/tree).rglob('*')):
            if path.is_file() and path.suffix in {'.rs','.toml','.py','.md'} and not any(x in path.parts for x in ['runs','evidence','__pycache__']):
                source[str(path.relative_to(ROOT))] = sha(path)
    record = {'schema':'a7-r1-audit-v2','commit':capture('git','rev-parse','HEAD'), 'status_before':capture('git','status','--short'),
        'started_utc':datetime.now(timezone.utc).isoformat(), 'host':{'platform':platform.platform(),'cpus':os.cpu_count()},
        'rustc':capture('rustc','-Vv'),'cargo':capture('cargo','-V'),'build_env':build_env,'source_sha256':source,
        'cargo_lock_sha256':{str(path.relative_to(ROOT)):sha(path) for path in [ROOT/'Cargo.lock',LOCAL/'Cargo.lock'] if path.exists()},
        'activation_allowed':False,'unverified':['A2 actual Miner adapter and source duties','A3 remote delivery/provider path','real LLM','cold/provider delivery','native network/carrier',
            'native Gate C/reward/DAG/settlement reads','A6 atomic service/attempt/fund/crash/reorg/IBD recovery','approved caps/p95/p99/concurrency'], 'commands':[]}
    commands = [
        [sys.executable,'-m','unittest','discover','-s','testing/positive-court-a7','-p','test_process_control.py','-v'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--no-run','--message-format=json'],
        ['cargo','test','--manifest-path','testing/positive-court-a7/Cargo.toml','--offline','--locked','--test','independent','--','--nocapture','--test-threads=1'],
    ]
    if not args.independent_only:
        commands += [
            ['cargo','test','-p','misaka-palw-positive-contract','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-positive-accounting','--offline','--locked'],
            ['cargo','test','-p','misaka-palw-tir-exec','--offline','--locked','--features','positive-panel','--test','positive_panel_r1','--test','positive_contract','--test','positive_check_hardening','--test','positive_check','--test','court','--test','node_leg','--test','capture_codec'],
            ['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--test','palw_tir_v1_fence'],
        ]
    for i, command in enumerate(commands):
        if command[0]=='cargo':
            command = [command[0],'--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.test.package.kaspa-consensus-core.opt-level=0',*command[1:]]
        print(f'A7 [{i+1}/{len(commands)}] '+ ' '.join(command), flush=True)
        log=out/f'{i:02d}.log'
        entry=run_group(command,cwd=ROOT,env=env,log=log,timeout=900)
        entry.update(argv=command,log=log.name,sha256=sha(log))
        data=log.read_text()
        counts=[{'passed':int(a),'failed':int(b),'ignored':int(c)} for a,b,c in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',data)]
        if command[0]==sys.executable and 'OK' in data:
            counts=[{'passed':int(re.search(r'Ran (\d+) tests',data)[1]),'failed':0,'ignored':0}]
        entry['test_counts']=counts
        if i!=1 and entry['exit_code']==0 and sum(x['passed'] for x in counts)==0:
            entry['validation_error']='NO_EXECUTED_TESTS'
        record['commands'].append(entry)
        if i==1 and entry['exit_code']==0:
            for line in data.splitlines():
                try: item=json.loads(line)
                except ValueError: continue
                if item.get('reason')=='compiler-artifact' and item.get('target',{}).get('name')=='independent' and item.get('executable'):
                    binary=Path(item['executable']);record['a7_binary']={'path':str(binary),'sha256':sha(binary)}
            if 'a7_binary' not in record:entry['validation_error']='MISSING_BINARY_IDENTITY'
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
    record['result']='PASS_DIAGNOSTIC_SUBSET' if args.independent_only else 'PASS_PINNED_PRIVATE_R1_CANDIDATE_ONLY'
    record['finished_utc']=datetime.now(timezone.utc).isoformat()
    (out/'run.json').write_text(json.dumps(record,indent=2)+'\n')
    print(record['result'],out)
    return 0

if __name__=='__main__':sys.exit(main())
