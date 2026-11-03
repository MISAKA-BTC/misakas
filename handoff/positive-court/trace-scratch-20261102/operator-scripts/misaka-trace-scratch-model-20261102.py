from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-trace-scratch-controller-20261102.py');out=ns['out'];sha=ns['sha'];root=ns['root'];r=json.loads((out/'runtime-first.json').read_text());binary=r['binary'];assert r['exit_code']==0 and sha(Path(binary['path']))==binary['sha256'] and ns['snapshot']()==r['source_after']
inputs=json.loads((root/'handoff/positive-court/source-profile-20261102/input-sha.json').read_text())['after']
for p,h in inputs.items():assert sha(Path(p))==h
case=ns['run']('pythia-first',[binary['path'],'--exact','trained_roles::pythia70m_original_short_job_ordinary_panel_and_complete_node','--ignored','--nocapture','--test-threads=1'],360,512*1024**2+305797608+16*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
after={p:sha(Path(p)) for p in inputs};assert after==inputs and sha(Path(binary['path']))==binary['sha256']
(out/'input-sha.json').write_text(json.dumps({'before':inputs,'after':after,'binary':binary,'match':True,'sha_warms_os_cache':True},indent=2)+'\n')
assert case['stop_reason'] is None
if case['exit_code']==0:assert case['test_counts']==[(1,0,0)]
else:assert case['exit_code']==-24
