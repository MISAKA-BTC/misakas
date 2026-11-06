from pathlib import Path
import datetime,hashlib,json,subprocess,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/extended-cost-20261102'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
runtime=json.loads((out/'runtime-r4.json').read_text());clippy=json.loads((out/'clippy-r3.json').read_text());failed=json.loads((out/'pythia-r2.json').read_text());old=json.loads((out/'runtime-r3.json').read_text())
assert runtime['exit_code']==clippy['exit_code']==0 and runtime['test_counts']==[[11,0,0],[4,0,0],[21,0,3]]
assert runtime['source_after']==clippy['source_after'] and failed['source_after']==old['source_after']
changed=[p for p in runtime['source_after'] if runtime['source_after'][p]!=old['source_after'][p]];assert changed==['misaka/palw/tir/exec/tests/positive_wide_material_v2/trained_roles.rs']
for p,h in runtime['source_after'].items():assert sha(root/p)==h
for name in ['runtime-r4','clippy-r3','pythia-r2']:
 r=json.loads((out/(name+'.json')).read_text());assert r['source_before']==r['source_after'] and not r['remaining_pg_members'] and sha(out/(name+'.log'))==r['log_sha256']
assert failed['exit_code']==101 and failed['test_counts']==[[0,1,0]] and failed['stop_reason'] is None
assert json.loads((out/'input-sha-r2.json').read_text())['binary']['sha256']==old['binary']['sha256']
binary=runtime['binary'];assert sha(Path(binary['path']))==binary['sha256']
ps=subprocess.run(['ps','-p','80621','-o','pid,ppid,lstart,etime,time,state,command'],capture_output=True,text=True);assert ps.returncode==0 and 'trained_roles::pythia70m_original_short_job_extended_cost_measurement' in ps.stdout
raw=(out/'pythia-r3.log').read_bytes();events=[json.loads(l.split('ROLE_MEASURE ',1)[1]) for l in raw.decode().splitlines() if 'ROLE_MEASURE ' in l];window=next(e for e in events if e['phase']=='measurement-window');assert window['detail']['per_relation_elements']==32768 and window['detail']['per_relation_workspace_cap']==1<<30
progress=next(e for e in reversed(events) if e['phase']=='node-progress');assert progress['detail']['peak_workspace_bytes']<=914148224
snapshot={'snapshot_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'purpose':'historical liveness observation only; revalidate exact PID/start time/session; never restart on observation timeout','pid':80621,'tool_session_hint':9891,'ps_at_snapshot':ps.stdout,'source_matches_runtime_r4_and_clippy_r3':True,'binary_sha_at_snapshot':sha(Path(binary['path'])),'partial_log_bytes_observed':len(raw),'partial_log_sha256_at_snapshot':hashlib.sha256(raw).hexdigest(),'window':window,'last_progress_at_snapshot':progress,'terminal_outcome':None,'native_authority_or_activation':False}
(out/'running-snapshot-r3.json').write_text(json.dumps(snapshot,indent=2)+'\n')
profiles={}
for line in (out/'runtime-r4.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
checks=[]
for cmd in [['rustfmt','--check','--config','skip_children=true','--edition','2024',*changed],['git','diff','--check']]:
 r=subprocess.run(cmd,cwd=root,capture_output=True,text=True);assert r.returncode==0;checks.append({'command':cmd,'exit_code':r.returncode,'output':r.stdout+r.stderr})
summary={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'current_source_matches_runtime_r4_clippy_r3':True,'runtime_counts':runtime['test_counts'],'selected_strict_clippy':True,'changed_source_files':changed,'failed_model_r2_matches_runtime_r3':True,'failed_model_r2_exit':101,'failed_model_r2_numeric_prefix':106198,'failed_model_r2_whole_record_authenticated':False,'new_model_r3_outcome_at_snapshot':'unproven, running','profiles':profiles,'checks':checks,'current_compiled_binary_at_snapshot':binary,'full_goal_complete':False,'native_authority_or_activation':False}
(out/'validation-element-checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n')
scripts=out/'operator-scripts';inventory=[]
for p in sorted(set(Path('/tmp').glob('misaka-extended-cost*-20261102.py'))|set(Path('/tmp').glob('misaka-extended-element*-20261102.py'))):
 q=scripts/p.name;shutil.copyfile(p,q);inventory.append({'original_path':str(p),'preserved_path':str(q.relative_to(root)),'sha256':sha(q),'scope':'final operator-file bytes at checkpoint; executed commands/source/input/log pins are independent raw records'})
(out/'operator-script-inventory-element-checkpoint.json').write_text(json.dumps(inventory,indent=2)+'\n')
logs={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'trailing_whitespace_lines':[i+1 for i,l in enumerate(p.read_bytes().splitlines()) if l.rstrip(b' \t')!=l]} for p in sorted(out.glob('*.log')) if p.name!='pythia-r3.log'}
(out/'completed-raw-log-inventory-element-checkpoint.json').write_text(json.dumps({'audit_only_no_rewrite':True,'live_log_excluded':'pythia-r3.log','files':logs},indent=2)+'\n')
print('Verified completed element-limit failure; new exact process remains live',80621,'prefix',progress['detail']['checked_relations'])
