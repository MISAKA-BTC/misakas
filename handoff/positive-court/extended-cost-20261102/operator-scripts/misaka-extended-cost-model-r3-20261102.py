from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-extended-cost-controller-20261102.py');out=ns['out'];sha=ns['sha'];root=ns['root'];r=json.loads((out/'runtime-r4.json').read_text());binary=r['binary'];assert r['exit_code']==0 and sha(Path(binary['path']))==binary['sha256'] and ns['snapshot']()==r['source_after']
inputs=json.loads((root/'handoff/positive-court/trace-scratch-20261102/input-sha.json').read_text())['after']
for p,h in inputs.items():assert sha(Path(p))==h
command=[binary['path'],'--exact','trained_roles::pythia70m_original_short_job_extended_cost_measurement','--ignored','--nocapture','--test-threads=1']
case=ns['run']('pythia-r3',command,1800,512*1024**2+305797608+16*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1','MISAKA_TRAINED_COST_WINDOW':'1440s-8gib-16mi-calls'})
after={p:sha(Path(p)) for p in inputs};assert after==inputs and sha(Path(binary['path']))==binary['sha256']
(out/'input-sha-r3.json').write_text(json.dumps({'before':inputs,'after':after,'binary':binary,'match':True,'sha_warms_os_cache':True},indent=2)+'\n')
print('ACTUAL_RESULT',case['exit_code'],case['stop_reason'],case['test_counts'],flush=True)
