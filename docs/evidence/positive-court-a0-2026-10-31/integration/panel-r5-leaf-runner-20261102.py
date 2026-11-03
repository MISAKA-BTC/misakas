from pathlib import Path
import os,subprocess,hashlib,json,time,re
root=Path.cwd(); out=root/'docs/evidence/positive-court-a0-2026-10-31/integration'; stem='panel-r5-leaf-20261102'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
paths=['Cargo.toml','Cargo.lock']+[str(p) for p in (root/'misaka/palw').rglob('*') if p.is_file() and p.suffix in ['.rs','.toml']]
def snap():return {str(p):sha(p) for p in sorted(set(paths))}
record=out/(stem+'.json');log=out/(stem+'.log');assert not record.exists() and not log.exists()
envadd={'CARGO_TARGET_DIR':'/Users/wata/.codex/worktrees/positive-court-a0/target','CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0'}
env=os.environ|envadd
for key in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(key,None)
cmd=['cargo','test','--offline','--locked','-p','misaka-palw-tir','--test','node_cost_noalloc']
d={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_before':snap(),'command':cmd,'environment_overrides':envadd,'default_test_stack':True,'scope':'Only frozen node_cost parity/allocation test. No execution or native admission/credit.','started_unix':time.time()};record.write_text(json.dumps(d,indent=2)+'\n')
with log.open('w') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
s=log.read_text(); rows=re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed',s)
d.update(exit_code=r.returncode,finished_unix=time.time(),test_summaries=rows,source_after=snap(),log={'path':str(log),'sha256':sha(log)})
d['source_stable']=d['source_before']==d['source_after'];d['success']=r.returncode==0 and rows==[('ok','1','0')] and d['source_stable']
d['binaries']=[{'path':p,'bytes':Path(p).stat().st_size,'sha256':sha(p)} for p in re.findall(r'Running [^\n]*\((/[^\n]+)\)',s)]
record.write_text(json.dumps(d,indent=2)+'\n'); print(json.dumps({k:d[k] for k in ['exit_code','success','test_summaries','source_stable','binaries']}),flush=True)
