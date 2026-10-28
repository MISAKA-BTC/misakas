from pathlib import Path
import subprocess,hashlib,json,tempfile,shutil,time,os,sys
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet');os.chdir(repo)
out=repo/'docs/evidence/palw/g14-completion-2026-10-22/source-step-inventory';out.mkdir()
pin=Path(tempfile.mkdtemp(prefix='g14-source-step-inventory-'));(Path('/tmp/g14-current-material-native-pin')).write_text(str(pin))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def sources():
 names=subprocess.check_output(['rg','--files','-g','!docs/**','-g','*.rs','-g','Cargo.toml','-g','Cargo.lock','-g','rust-toolchain*']).decode().splitlines()
 return {n:sha(repo/n) for n in sorted(names)}
initial=sources();base=subprocess.check_output(['git','rev-parse','HEAD']).decode().strip();patch=subprocess.check_output(['git','diff','--binary']);(out/'source.patch').write_bytes(patch)
for n in subprocess.check_output(['git','ls-files','--others','--exclude-standard']).decode().splitlines():
 p=repo/n
 if n in initial:
  target=out/'added-source'/n;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,target)
metadata=dict(base=base,pin=str(pin),sources=initial,tracked_patch_sha256=sha(out/'source.patch'),steps=[],executables={})
def save(): (out/'verification.json').write_text(json.dumps(metadata,indent=2)+'\n')
def run(cmd,name,env=None):
 assert sources()==initial,'Rust source changed while frozen'
 log=out/name;start=time.time();print('START',name,flush=True)
 with log.open('xb') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
 metadata['steps'].append(dict(command=cmd,log=name,exit_code=r.returncode,elapsed_seconds=time.time()-start));save();print('END',name,r.returncode,flush=True)
 assert sources()==initial,'Rust source changed during native execution'
 return r.returncode
save()
cmd=['cargo','test','--release','-p','kaspa-consensus-core','--lib','--test','palw_the_release_did_not_move','--test','palw_activation_pool_is_t12_only','--no-run','--message-format=json']
if run(cmd,'core-build.log'):sys.exit(1)
for line in (out/'core-build.log').read_text().splitlines():
 try:r=json.loads(line)
 except ValueError:continue
 if r.get('reason')=='compiler-artifact' and r.get('executable'):
  name=r['target']['name'];p=Path(r['executable']);dest=pin/name;shutil.copy2(p,dest);metadata['executables'][name]=dict(path=str(dest),sha256=sha(dest),bytes=dest.stat().st_size)
save();core=metadata['executables']['kaspa_consensus_core']['path'];failed=False
filters=['native_step_space_inventory_tests_v1', 'palw_native_resource_v1::tir::tests']
for i,f in enumerate(filters):failed=bool(run([core,f,'--nocapture'],f'core-selection-{i}.log')) or failed
for target in ['palw_the_release_did_not_move','palw_activation_pool_is_t12_only']:
 failed=bool(run([metadata['executables'][target]['path'],'--nocapture'],target+'.log')) or failed
if failed:sys.exit(1)
metadata['source_hashes_unchanged']=sources()==initial;save()
