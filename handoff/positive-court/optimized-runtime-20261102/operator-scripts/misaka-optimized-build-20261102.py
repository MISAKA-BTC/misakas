from pathlib import Path
import fcntl,hashlib,json,runpy,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out=root/'handoff/positive-court/optimized-runtime-20261102';out.mkdir(exist_ok=False)
base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(1024**2):h.update(b)
 return h.hexdigest()
previous=root/'handoff/positive-court/evaluation-diagnostics-20261102'
runtime=json.loads((previous/'runtime-measured-terms.json').read_text());exe=Path(runtime['binary']['path']);assert sha(exe)==runtime['binary']['sha256']
paths={}
for label in ['runtime-measured-terms','clippy-core-first','clippy-panel-measured-terms']:
 for line in (previous/(label+'.log')).read_text().splitlines():
  if not line.startswith('{'):continue
  e=json.loads(line)
  if e.get('reason')!='compiler-artifact' or e.get('manifest_path')!=str(root/'consensus/core/Cargo.toml'):continue
  for name in e['filenames']:
   p=Path(name)
   if p.is_file() and p.suffix in ['.rlib','.rmeta']:
    assert p.parent==base
    paths[p]={'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p),'source_record':label,'preserved_copy':False}
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 assert len(paths)==4
 record={'scope':'retire only current own opt0 Core library/Clippy intermediates before compiling optimized dependencies; baseline executable, sources, logs, models and native archives retained','files':list(paths.values()),'free_before':shutil.disk_usage(base).free,'released':False,'baseline_executable':{'path':str(exe),'sha256':sha(exe)}}
 (out/'cache-retirement.json').write_text(json.dumps(record,indent=2)+'\n')
 for p,row in paths.items():assert sha(p)==row['sha256'];p.unlink()
 assert sha(exe)==record['baseline_executable']['sha256']
 record['released']=True;record['free_after']=shutil.disk_usage(base).free
 (out/'cache-retirement.json').write_text(json.dumps(record,indent=2)+'\n')
 print('retired opt0 intermediates; observed free',record['free_after'],flush=True)
s=(previous/'operator-scripts/misaka-evaluation-validation-20261102.py').read_text().replace("out=root/'handoff/positive-court/evaluation-diagnostics-20261102'","out=root/'handoff/positive-court/optimized-runtime-20261102'")
controller=Path('/tmp/misaka-optimized-controller-20261102.py');controller.write_text(s)
ns=runpy.run_path(str(controller));command=runtime['command'].copy()
for i,value in enumerate(command):
 if value.endswith('kaspa-consensus-core.opt-level=0'):command[i]=value[:-1]+'2'
i=command.index('--')
settings=[]
for profile in ['dev','test']:
 for package in ['misaka-palw-tir','misaka-palw-tir-exec']:
  settings += ['--config',f'profile.{profile}.package.{package}.opt-level=2']
command[i:i]=settings
r=ns['run']('optimized-runtime-first',command,1800,1100*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None and r['test_counts']==[(11,0,0),(4,0,0),(20,0,2)]
profiles={}
for line in (out/'optimized-runtime-first.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:
  profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
(out/'verified-profiles.json').write_text(json.dumps(profiles,indent=2)+'\n')
