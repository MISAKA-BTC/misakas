from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-three-roles-controller-20261102.py');out=ns['out'];sha=ns['sha'];root=ns['root'];r=json.loads((out/'runtime-r3.json').read_text());binary=r['binary'];assert r['exit_code']==0 and sha(Path(binary['path']))==binary['sha256'] and ns['snapshot']()==r['source_after']
inputs=json.loads((root/'handoff/positive-court/trace-scratch-20261102/input-sha.json').read_text())['after']
for p,h in inputs.items():assert sha(Path(p))==h
command=[binary['path'],'--exact','trained_roles::isolated_process::trained_miner_process_only','--ignored','--nocapture','--test-threads=1']
case=ns['run']('miner-process-first',command,120,512*1024**2+16*1024**2,2*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1','MISAKA_THREE_ROLES':'original-16-1-three-processes'})
after={p:sha(Path(p)) for p in inputs};assert after==inputs and sha(Path(binary['path']))==binary['sha256'];(out/'input-sha-miner-first.json').write_text(json.dumps({'before':inputs,'after':after,'binary':binary,'match':True,'sha_warms_os_cache':True},indent=2)+'\n');assert case['exit_code']==0 and case['test_counts']==[(1,0,0),(1,0,0)]
