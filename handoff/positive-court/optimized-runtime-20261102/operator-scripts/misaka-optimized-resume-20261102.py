from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-optimized-controller-20261102.py');out=ns['out'];old=json.loads((out/'optimized-runtime-first.json').read_text())
assert old['exit_code']==-15 and old['stop_reason']=='disk floor' and old['remaining_pg_members']==[]
assert ns['snapshot']()==old['source_after']
r=ns['run']('optimized-runtime-r2',old['command'],1800,600*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None and r['test_counts']==[(11,0,0),(4,0,0),(20,0,2)]
profiles={}
for line in (out/'optimized-runtime-r2.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:
  profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
(out/'verified-profiles.json').write_text(json.dumps(profiles,indent=2)+'\n')
