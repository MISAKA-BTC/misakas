from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-optimized-controller-20261102.py');out=ns['out'];sha=ns['sha'];run=ns['run']
record=json.loads((out/'hashed-walk-runtime-first.json').read_text());binary=record['binary']['path']
assert record['exit_code']==0 and sha(Path(binary))==record['binary']['sha256'] and ns['snapshot']()==record['source_after']
profiles=json.loads((out/'verified-profiles.json').read_text());assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] for p in profiles.values())
pinned={
'/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir':'2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651',
'/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/registration-fixture.json':'29c0d221c7f998dee2e4778bd8bfd9f9b8b4e610b17a62b1bf6f4484b7f367a8'}
for p,s in pinned.items():assert sha(Path(p))==s
case=run('pythia-hashed-walk-first',[binary,'--exact','trained_roles::pythia70m_original_short_job_ordinary_panel_and_complete_node','--ignored','--nocapture','--test-threads=1'],360,512*1024**2+305797608+16*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
after={p:sha(Path(p)) for p in pinned};assert after==pinned and sha(Path(binary))==record['binary']['sha256']
(out/'input-sha-hashed-walk.json').write_text(json.dumps({'expected_before':pinned,'after':after,'match':True,'sha_warms_os_cache':True,'binary_matches_compiled_record_after':True},indent=2)+'\n')
assert case['exit_code']==0 and case['stop_reason'] is None and case['test_counts']==[(1,0,0)]
