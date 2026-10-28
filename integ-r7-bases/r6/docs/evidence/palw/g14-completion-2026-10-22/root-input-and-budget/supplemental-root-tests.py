from pathlib import Path
import subprocess,json,hashlib,time,os
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet');os.chdir(repo)
out=repo/'docs/evidence/palw/g14-completion-2026-10-22/root-input-and-budget';p=out/'verification.json';meta=json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def frozen():return all(sha(repo/n)==h for n,h in meta['sources'].items())
exe=meta['executables']['kaspa_consensus_core'];assert sha(Path(exe['path']))==exe['sha256'];assert frozen()
for i,f in enumerate(['actual_root_input_coordinates_keep_parameter_relation_and_authenticate_carry_units','actual_input0_numerical_roots_name_prompt_and_generated_coordinates','token_source_helper_is_identical_to_native_court_split_at_boundaries']):
 cmd=[exe['path'],f,'--nocapture'];name=f'core-input-coverage-{i}.log';print('START',name,flush=True);t=time.time()
 with (out/name).open('xb') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 meta['steps'].append(dict(command=cmd,log=name,exit_code=r.returncode,elapsed_seconds=time.time()-t));meta['source_hashes_unchanged']=frozen();p.write_text(json.dumps(meta,indent=2)+'\n');print('END',name,r.returncode,flush=True);assert frozen();assert r.returncode==0
