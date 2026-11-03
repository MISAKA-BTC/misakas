from pathlib import Path
import json,hashlib,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/legacy-cert-aux-20261102';names=['exec-fresh-functional-first','core-fresh-functional-first','core-fresh-clippy-first','exec-fresh-clippy-first'];records={n:json.loads((out/(n+'.json')).read_text()) for n in names};source=records[names[0]]['source_after'];checks={n:r['exit_code']==0 and r['stop_reason'] is None and not r['remaining_pg_members'] and r['source_before']==r['source_after']==source for n,r in records.items()};checks['exact_runtime_counts']=records[names[0]]['test_counts']==[[11,0,0],[4,0,0],[1,0,0],[1,0,0],[1,0,0],[24,0,8]] and records[names[1]]['test_counts']==[[3,0,0]]
for p,h in source.items():assert hashlib.sha256((root/p).read_bytes()).hexdigest()==h
files=['consensus/core/src/palw_native_resource_v1/generated.rs','consensus/core/src/palw_native_resource_v1/wire_kernel_aux.rs','consensus/core/tests/legacy_certification_aux_codec.rs','scripts/drills/gen_rfc06_native_inventory.py','scripts/drills/gen_rfc06_native_wire.py'];head='27a3712686eb47588c5b6188ceda0cadf1021ad0';pins={}
for p in files:
 b=(root/p).read_bytes();assert b==subprocess.check_output(['git','show',head+':'+p],cwd=root);pins[p]=hashlib.sha256(b).hexdigest()
for r in records.values():
 for p in pins:
  if p.endswith('.rs'):assert r['source_after'][p]==pins[p]
profiles=[]
for line in (out/'exec-fresh-functional-first.log').read_text().splitlines():
 if not line.startswith('{'):continue
 try:e=json.loads(line)
 except:continue
 if e.get('reason')=='compiler-artifact' and e['target']['name'] in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec']:
  assert e['profile']['opt_level']=='0' and e['profile']['debug_assertions'] and e['profile']['overflow_checks'];profiles.append({'target':e['target']['name'],'features':e['features'],'profile':e['profile']})
checks['functional_profile_observed']=len(profiles)==3
native=json.loads((out/'fresh-native-output-pins.json').read_text());checks['fresh_native_outputs_unchanged']=all(hashlib.sha256(Path(f['path']).read_bytes()).hexdigest()==f['sha256'] for f in native['files']);summary={'code_commit':head,'checks':checks,'source_sha256':pins,'runtime_outer_passes':42,'nested_child_scenarios':3,'gated_ignored_cases':8,'records':{n:{k:r.get(k) for k in ['exit_code','elapsed_seconds','stop_reason','test_counts','remaining_pg_members','log_sha256']} for n,r in records.items()},'profiles':profiles,'native_admission_or_p_or_funding_proven':False,'full_legacy_history_money_and_recovery_proven':False,'performance_activation_approved':False};(out/'final-source-validation.json').write_text(json.dumps(summary,indent=2)+'\n');assert all(checks.values());print(json.dumps({'checks':checks,'runtime_outer_passes':42,'code_commit':head},indent=2))
