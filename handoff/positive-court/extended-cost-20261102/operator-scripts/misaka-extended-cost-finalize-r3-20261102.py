from pathlib import Path
import hashlib,json,subprocess,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/extended-cost-20261102'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
case=json.loads((out/'pythia-r3.json').read_text());runtime=json.loads((out/'runtime-r4.json').read_text());clippy=json.loads((out/'clippy-r3.json').read_text())
assert case['source_before']==case['source_after']==runtime['source_after']==clippy['source_after']
for p,h in case['source_after'].items():assert sha(root/p)==h
assert not case['remaining_pg_members'] and sha(out/'pythia-r3.log')==case['log_sha256']
assert case['exit_code']==0 and case['stop_reason'] is None and case['test_counts']==[[1,0,0]],(case['exit_code'],case['stop_reason'],case['test_counts'])
inputs=json.loads((out/'input-sha-r3.json').read_text());assert inputs['match'] and inputs['before']==inputs['after'] and sha(Path(runtime['binary']['path']))==runtime['binary']['sha256']==inputs['binary']['sha256']
ms=[json.loads(l.split('ROLE_MEASURE ',1)[1]) for l in (out/'pythia-r3.log').read_text().splitlines() if 'ROLE_MEASURE ' in l]
complete=next(e for e in ms if e['phase']=='complete-node');d=complete['detail'];assert d['checked_relations']==d['observed_evaluations']==d['claim_relations_read']==213298 and d['verdict']=='Matched'
assert not complete['native_authority'] and not complete['canonical_job'] and not complete['full_context_execution'] and not complete['node_process_isolated']
assert next(e for e in ms if e['phase']=='ordinary-miner')['detail']['witness_runs']==0
assert next(e for e in ms if e['phase']=='panel-record')['detail']['tape_bytes']==305797608
assert case['sampled_pg_rss_peak_bytes']<=4<<30 and complete['requested_peak']<=8<<30 and d['peak_workspace_bytes']<=1<<30
stages=[];last_user=last_system=0
for e in ms:
 if e['phase']=='node-progress':continue
 stages.append({'phase':e['phase'],'phase_wall_seconds':e['wall_ns']/1e9,'process_user_seconds':e['process_user_seconds'],'process_system_seconds':e['process_system_seconds'],'detail':e['detail']})
summary={'scope':'actual pinned original noncanonical Pythia 16/1 short job, shared producer/Node process; complete local checks and EOF/root/signature only, not native acceptance, isolated cold-role qualification or activation','source_commit':'db0f42fce','runtime_record':'pythia-r3.json','actual_case_exit':case['exit_code'],'actual_case_test_counts':case['test_counts'],'all_numeric_relations':d['checked_relations'],'complete_event':complete,'other_phases':stages,'monitor':{k:case[k] for k in ['elapsed_seconds','minimum_free_bytes','sampled_pg_rss_peak_bytes','rss_monitor_is_kernel_quota','stop_reason','remaining_pg_members']},'unchanged_original_inputs_after_execution':True,'full_goal_complete':False,'native_accepted_case':False,'activation_approved_or_enabled':False,'physical_cold_role_costs_qualified':False}
(out/'complete-short-job-r3.json').write_text(json.dumps(summary,indent=2)+'\n')
checks=[]
for cmd in [['rustfmt','--check','--config','skip_children=true','--edition','2024','misaka/palw/tir/exec/tests/positive_wide_material_v2/trained_roles.rs'],['git','diff','--check']]:
 r=subprocess.run(cmd,cwd=root,capture_output=True,text=True);assert r.returncode==0;checks.append({'command':cmd,'exit_code':r.returncode,'output':r.stdout+r.stderr})
validation={'source_commit_at_validation':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'compiled_source_commit':'db0f42fce','source_matches_runtime_r4_clippy_r3_and_actual_model_r3':True,'selected_runtime_counts':runtime['test_counts'],'selected_strict_clippy_exit':clippy['exit_code'],'actual_model_counts':case['test_counts'],'actual_model_exit':case['exit_code'],'whole_node_local_matched':True,'all_relations_and_eof_root_signature_checked':True,'all_original_input_and_binary_sha_match_before_after':True,'all_role_guards_drop_before_exit_asserted':True,'checks':checks,'full_goal_complete':False,'remaining_requirements':'COMPLETION-REQUIREMENTS.md; native admission/roles/funding/P/credit readers, real-node recovery/dependencies, isolated/canonical/full-context costs, explicitly approved finite activation'}
(out/'validation-final-r3.json').write_text(json.dumps(validation,indent=2)+'\n')
p=Path('/tmp/misaka-extended-cost-finalize-r3-20261102.py');q=out/'operator-scripts'/p.name;shutil.copyfile(p,q)
(out/'operator-script-inventory-final-r3.json').write_text(json.dumps([{'path':str(p.relative_to(root)),'sha256':sha(p)} for p in sorted((out/'operator-scripts').glob('*.py'))],indent=2)+'\n')
print(json.dumps({'checked':d['checked_relations'],'verdict':d['verdict'],'producer_material_seconds':d['producer_material_ns']/1e9,'node_submit_seconds':d['node_check_ns']/1e9,'record_auth_finish_seconds':d['record_auth_finish_ns']/1e9,'elapsed':case['elapsed_seconds'],'peak_rss':case['sampled_pg_rss_peak_bytes'],'complete_detail':d},indent=2))
