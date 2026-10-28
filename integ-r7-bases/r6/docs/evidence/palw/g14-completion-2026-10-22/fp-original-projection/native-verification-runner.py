from pathlib import Path
import subprocess,json,hashlib,shutil,time,sys,os
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet');os.chdir(repo)
prior=repo/'docs/evidence/palw/g14-completion-2026-10-22/fp-original-projection';d=json.loads((prior/'verification.json').read_text());pin=Path(d['pin'])
out=repo/'docs/evidence/palw/g14-completion-2026-10-22/fp-original-projection-native';out.mkdir();steps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
current={p:sha(repo/p) for p in d['sources']};changed={p:dict(prior_sha256=h,current_sha256=current[p]) for p,h in d['sources'].items() if h!=current[p]}
assert not changed,changed
(out/'source.patch').write_bytes(subprocess.check_output(['git','diff','--binary']))
for name in subprocess.check_output(['git','ls-files','--others','--exclude-standard']).decode().splitlines():
 if name in current:
  target=out/'added-source'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(repo/name,target)
fixture=repo/'consensus/tests/fixtures/g14-funded-original-native';files={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()}
meta=dict(base=d['base'],prior_core='fp-original-projection/verification.json',sources=current,test_only_delta=changed,tracked_patch_sha256=sha(out/'source.patch'),fixture=files,steps=steps)
def frozen():return all(sha(repo/p)==h for p,h in current.items()) and files=={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()}
def save():(out/'verification.json').write_text(json.dumps(meta,indent=2)+'\n')
def run(cmd,name):
 assert frozen();print('START',name,flush=True);t=time.time()
 with (out/name).open('xb') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT)
 steps.append(dict(command=cmd,log=name,exit_code=r.returncode,elapsed_seconds=time.time()-t));save();assert frozen();print('END',name,r.returncode,flush=True);return r.returncode
save()
if run(['cargo','test','--release','-p','kaspa-consensus','--lib','--no-run','--message-format=json'],'native-build.log'):sys.exit(1)
art=[]
for l in (out/'native-build.log').read_text().splitlines():
 try:r=json.loads(l)
 except ValueError:continue
 if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='kaspa_consensus':art.append(r)
assert len(art)==1
p=Path(art[0]['executable']);dest=pin/'kaspa_consensus_fp_projection';shutil.copy2(p,dest);meta['executable']=dict(path=str(dest),bytes=dest.stat().st_size,sha256=sha(dest));save()
for i,f in enumerate(['a_callers_prompt_on_a_registered_class_opens_a_claim_at_the_shipped_quantum','palw_v2_a_derivation_rides_signed_by_the_claims_executor_and_is_dropped_otherwise','palw_funded_original_real_native_inputs_preserve_fallback_and_refuse_incomplete_admission']):
 if run([str(dest),f,'--nocapture'],f'native-selection-{i}.log'):sys.exit(1)
run(['rustc','-Vv'],'rustc-version.log');run(['cargo','-V'],'cargo-version.log');meta['source_and_fixture_unchanged']=frozen();save()
