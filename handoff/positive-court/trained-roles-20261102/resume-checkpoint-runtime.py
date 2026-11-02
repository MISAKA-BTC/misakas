import runpy,json,hashlib
from pathlib import Path
ns=runpy.run_path('/tmp/misaka-trained-roles-run-20261102.py')
out=ns['out'];sha=ns['sha'];run=ns['run']
build=json.loads((out/'build-checkpoint.json').read_text());binary=build['binary']['path']
assert sha(Path(binary))==build['binary']['sha256'] and ns['snapshot']()==build['source_after']
pinned={
'/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir':'2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651',
'/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/registration-fixture.json':'29c0d221c7f998dee2e4778bd8bfd9f9b8b4e610b17a62b1bf6f4484b7f367a8'}
for p,s in pinned.items():assert sha(Path(p))==s
case=run('pythia-original-short-checkpoint',[binary,'--exact','trained_roles::pythia70m_original_short_job_ordinary_panel_and_complete_node','--ignored','--nocapture','--test-threads=1'],360,832*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
assert case['exit_code']==101 and case['stop_reason'] is None and case['test_counts']==[(0,1,0)]
assert 'wide Panel builder simultaneous owners cap' in (out/'pythia-original-short-checkpoint.log').read_text()
quote=run('storage-checkpoint',[binary,'--exact','trained_roles::pythia70m_original_short_job_storage_quote','--ignored','--nocapture','--test-threads=1'],120,650*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
assert quote['exit_code']==0 and quote['test_counts']==[(1,0,0)]
after={p:sha(Path(p)) for p in pinned};assert pinned==after
(out/'input-sha-checkpoint.json').write_text(json.dumps({'expected_before':pinned,'after':after,'match':True,'sha_warms_os_cache':True},indent=2)+'\n')
print('Verified partial checkpoint only: full numerical flow remains refused; Node not started')
