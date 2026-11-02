from pathlib import Path
import datetime,hashlib,json,subprocess,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/extended-cost-20261102'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
current=json.loads((out/'runtime-r3.json').read_text());clippy=json.loads((out/'clippy-r2.json').read_text());first=json.loads((out/'pythia-first.json').read_text())
assert current['exit_code']==clippy['exit_code']==0 and current['source_after']==clippy['source_after'] and first['exit_code']==101
for p,h in current['source_after'].items():assert sha(root/p)==h
for name in ['runtime-first','runtime-r2','runtime-r3','clippy-first','clippy-r2','pythia-first']:
 r=json.loads((out/(name+'.json')).read_text());assert r['source_before']==r['source_after'] and not r['remaining_pg_members'] and sha(out/(name+'.log'))==r['log_sha256']
binary=current['binary'];assert sha(Path(binary['path']))==binary['sha256']
ps=subprocess.run(['ps','-p','76584','-o','pid,ppid,lstart,etime,time,state,command'],capture_output=True,text=True);assert ps.returncode==0 and 'trained_roles::pythia70m_original_short_job_extended_cost_measurement' in ps.stdout
partial=(out/'pythia-r2.log').read_bytes();events=[json.loads(l.split('ROLE_MEASURE ',1)[1]) for l in partial.decode().splitlines() if 'ROLE_MEASURE ' in l];quote=next(e for e in events if e['phase']=='cumulative-copy-preflight');assert quote['detail']['relations']==213298 and quote['detail']['cumulative_io_and_copy_cap']==9356116492288
snapshot={'snapshot_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'purpose':'historical running snapshot ONLY; revalidate this PID/start time/session before relying on liveness; never restart on a polling timeout','pid':76584,'tool_session_hint':55498,'ps_at_snapshot':ps.stdout,'binary_sha_at_snapshot':sha(Path(binary['path'])),'source_matches_runtime_r3_and_clippy_r2':True,'partial_log_bytes_observed':len(partial),'partial_log_sha256_at_snapshot':hashlib.sha256(partial).hexdigest(),'last_event_at_snapshot':events[-1],'copy_quote_event':quote,'terminal_outcome':None,'native_authority_or_activation':False}
(out/'running-snapshot.json').write_text(json.dumps(snapshot,indent=2)+'\n')
profiles={}
for l in (out/'runtime-r3.log').read_text().splitlines():
 if not l.startswith('{'):continue
 e=json.loads(l)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
checks=[]
for command in [['rustfmt','--check','--config','skip_children=true','--edition','2024','misaka/palw/tir/exec/tests/positive_wide_material_v2/trained_roles.rs'],['git','diff','--check']]:
 r=subprocess.run(command,cwd=root,capture_output=True,text=True);assert r.returncode==0;checks.append({'command':command,'exit_code':r.returncode,'output':r.stdout+r.stderr})
summary={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'current_source_matches_selected_runtime_and_clippy':True,'runtime_counts':current['test_counts'],'strict_selected_clippy':True,'first_model_exit':101,'first_model_test_counts':first['test_counts'],'first_model_is_pass':False,'second_model_at_snapshot':'running; terminal result unproven','second_model_snapshot':'running-snapshot.json','checks':checks,'profiles':profiles,'current_binary_at_snapshot':binary,'full_goal_complete':False,'native_authority_or_activation':False}
(out/'validation-checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n')
scripts=out/'operator-scripts';scripts.mkdir(exist_ok=True);inventory=[]
for p in sorted(Path('/tmp').glob('misaka-extended-cost*-20261102.py')):
 q=scripts/p.name;shutil.copyfile(p,q);inventory.append({'original_path':str(p),'preserved_path':str(q.relative_to(root)),'sha256':sha(q),'scope':'final operator-file versions at this snapshot; actual commands/source/input/log pins are recorded separately'})
(out/'operator-script-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
raw={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'trailing_whitespace_lines':[i+1 for i,l in enumerate(p.read_bytes().splitlines()) if l.rstrip(b' \t')!=l]} for p in sorted(out.glob('*.log')) if p.name!='pythia-r2.log'}
(out/'completed-raw-log-inventory.json').write_text(json.dumps({'audit_only_no_rewrite':True,'excludes_live_log':'pythia-r2.log','files':raw},indent=2)+'\n')
print('Snapshot confirmed live:',76584,'numeric prefix',events[-1]['detail'].get('checked_relations'),'current code and complete earlier records verified')
