from pathlib import Path
import subprocess,hashlib,json,tempfile,shutil,time,os,sys
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet');os.chdir(repo)
out=repo/'docs/evidence/palw/g14-completion-2026-10-22/native-source-policy-and-stop/retry2';out.mkdir()
pin=Path(tempfile.mkdtemp(prefix='g14-native-source-policy-and-stop-'));(Path('/tmp/g14-current-material-native-pin')).write_text(str(pin))
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
fixture=repo/'consensus/tests/fixtures/g14-funded-original-native'
metadata['native_fixture']={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()};save()
if run(['cargo','test','--release','-p','kaspa-consensus','--lib','--no-run','--message-format=json'],'native-build.log'):sys.exit(1)
arts=[]
for line in (out/'native-build.log').read_text().splitlines():
 try:r=json.loads(line)
 except ValueError:continue
 if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='kaspa_consensus':arts.append(r)
assert len(arts)==1
p=Path(arts[0]['executable']);dest=pin/'kaspa_consensus';shutil.copy2(p,dest);metadata['executables']['kaspa_consensus']=dict(path=str(dest),bytes=dest.stat().st_size,sha256=sha(dest));save()
if run([str(dest),'palw_funded_original_real_native_inputs_preserve_fallback_and_refuse_incomplete_admission','--nocapture'],'native-candidate-test.log'):sys.exit(1)
assert metadata['native_fixture']=={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()}
metadata['source_and_fixture_unchanged']=True;save()
