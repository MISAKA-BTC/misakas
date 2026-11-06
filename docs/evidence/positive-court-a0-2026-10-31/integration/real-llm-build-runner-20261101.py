from pathlib import Path
import subprocess,hashlib,json,os,time
root=Path.cwd();out=root/'docs/evidence/positive-court-a0-2026-10-31/integration';stem='real-llm-harness-first-build-20261101'
paths=['Cargo.toml','Cargo.lock']
for b in ['consensus/core/src','misaka/palw','testing/positive-court-a7']:
 paths.extend(str(p) for p in Path(b).rglob('*') if p.is_file() and p.suffix in ['.rs','.toml','.py'] and 'evidence' not in p.parts and 'validation' not in p.parts)
paths+=['testing/positive-court-a7/Cargo.lock']
def h(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1048576):d.update(b)
 return d.hexdigest()
def snap():return {p:h(p) for p in sorted(set(paths))}
profile=Path('/tmp/misaka-a7-root-dev-profiles-20261101.toml')
archive=Path('/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out')
envadd={'CARGO_TARGET_DIR':'/Users/wata/.codex/worktrees/positive-court-a0/target','CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0','ROCKSDB_LIB_DIR':str(archive),'ROCKSDB_STATIC':'1','SNAPPY_LIB_DIR':str(archive),'SNAPPY_STATIC':'1'}
cmd=['cargo','rustc','--manifest-path','testing/positive-court-a7/Cargo.toml','--bin','real_llm_repro','--offline','--locked','--message-format=json-render-diagnostics','--config',str(profile),'--','-l','c++']
p=out/(stem+'.json');assert not p.exists()
d={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_before':snap(),'scope':'Compile only explicit A7 retained trained-LLM local CPU/capture harness. No model quote/inference/registration/credit executed here. Panel r3 known runtime/provider budget failures and resource gaps are separate; direct legacy/wide CPU/capture API only.','command':cmd,'environment_overrides':envadd,'RUST_MIN_STACK_effective':None,'profile_file':{'path':str(profile),'sha256':h(profile),'contents':profile.read_text()},'archives':{n:{'path':str(archive/n),'bytes':(archive/n).stat().st_size,'sha256':h(archive/n)} for n in ['librocksdb.a','libsnappy.a']},'started_unix':time.time()}
p.write_text(json.dumps(d,indent=2)+'\n')
env=os.environ|envadd
for k in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(k,None)
a=out/(stem+'.stdout.jsonl');b=out/(stem+'.stderr.log');assert not a.exists() and not b.exists()
with a.open('w') as stdout,b.open('w') as stderr:
 r=subprocess.run(cmd,stdout=stdout,stderr=stderr,env=env)
d['exit_code']=r.returncode;d['finished_unix']=time.time();d['source_after']=snap();d['source_stable']=d['source_before']==d['source_after'];d['archives_stable']=all(h(x['path'])==x['sha256'] for x in d['archives'].values());d['logs']={f.name:h(f) for f in [a,b]}
arts=[]
for line in a.read_text().splitlines():
 try:x=json.loads(line)
 except ValueError:continue
 if x.get('reason')=='compiler-artifact' and x.get('target',{}).get('name')=='real_llm_repro' and x.get('executable'):
  e=Path(x['executable']);arts.append({'path':str(e),'bytes':e.stat().st_size,'sha256':h(e),'cargo_profile':x.get('profile'),'fresh':x.get('fresh')})
d['binary']=arts;d['build_success']=r.returncode==0 and d['source_stable'] and d['archives_stable'] and len(arts)==1
p.write_text(json.dumps(d,indent=2)+'\n');print(json.dumps({k:d[k] for k in ['exit_code','build_success','source_stable','archives_stable','binary']}),flush=True)
