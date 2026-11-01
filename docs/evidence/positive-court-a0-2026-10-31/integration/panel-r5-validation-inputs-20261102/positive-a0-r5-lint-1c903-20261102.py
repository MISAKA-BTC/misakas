from pathlib import Path
import subprocess,os,json,hashlib,time,shutil,signal
root=Path.cwd();out=root/'docs/evidence/positive-court-a0-2026-10-31/integration';stem='panel-r5-clippy-1c903-20261102'
plan=Path('/tmp/positive-a0-r5-core-geometry-artifact-1c903-plan.json');p=json.loads(plan.read_text())
def h(x):return hashlib.sha256(Path(x).read_bytes()).hexdigest()
def stable():return all(h(x)==v for x,v in p['source_before'].items()) and all(h(x)==v['sha256'] for x,v in p['archives'].items())
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==p['head'];assert stable()
envadd={'CARGO_TARGET_DIR':'/Users/wata/.codex/worktrees/positive-court-a0/target','CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0','ROCKSDB_LIB_DIR':'/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out','ROCKSDB_STATIC':'1','ROCKSDB_COMPILE':'0','SNAPPY_LIB_DIR':'/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out','SNAPPY_STATIC':'1','SNAPPY_COMPILE':'0','CXX':str(out/'panel-r5-core-geometry-artifact-1c903-20261102-forbid-cxx.py')}
env=os.environ|envadd
for k in list(env):
 if k in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS'] or k.endswith('_RUSTFLAGS'):env.pop(k,None)
base=['cargo','clippy','--offline','--locked','--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.test.package.kaspa-consensus-core.opt-level=0']
commands=[base+['-p','misaka-palw-tir','-p','kaspa-consensus-core','-p','misaka-palw-tir-artifact','-p','misaka-palw-tir-exec','--features','misaka-palw-tir-exec/positive-panel','--lib','--no-deps','--','-D','warnings'],base+['-p','misaka-palw-tir','--test','node_cost_noalloc','--no-deps','--','-D','warnings'],base+['-p','misaka-palw-tir-exec','--features','positive-panel','--test','positive_wide_material_v2','--test','wide_quote_noalloc','--no-deps','--','-D','warnings']]
record=out/(stem+'.json');assert not record.exists()
d={'source_commit':p['head'],'source_before':p['source_before'],'archives':p['archives'],'environment_overrides':envadd,'scope':'Strict library and selected newly added integration test Clippy only; no additional runtime/native/HF qualification.','results':[]};record.write_text(json.dumps(d,indent=2)+'\n')
for i,cmd in enumerate(commands):
 assert stable();log=out/(stem+'-'+str(i)+'.log');assert not log.exists();start=time.monotonic()
 with log.open('x') as f:
  process=subprocess.Popen(cmd,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
  while process.poll() is None:
   if shutil.disk_usage(envadd['CARGO_TARGET_DIR']).free<512*(1<<20):
    os.killpg(process.pid,signal.SIGTERM)
    try:process.wait(timeout=10)
    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
    f.write('\nStopped own process group: free space below512MiB\n');break
   time.sleep(1)
 item={'command':cmd,'exit_code':process.returncode,'source_archives_stable':stable(),'elapsed_seconds':time.monotonic()-start,'log':str(log),'sha256':h(log)};d['results'].append(item);d['success']=all(x['exit_code']==0 and x['source_archives_stable'] for x in d['results']) and len(d['results'])==len(commands);record.write_text(json.dumps(d,indent=2)+'\n');print(json.dumps(item),flush=True)
 if process.returncode or not item['source_archives_stable']:break
