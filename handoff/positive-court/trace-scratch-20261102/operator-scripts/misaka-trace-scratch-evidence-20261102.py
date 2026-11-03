from pathlib import Path
import hashlib,json,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/trace-scratch-20261102';old=root/'handoff/positive-court/optimized-runtime-20261102';profile=root/'handoff/positive-court/source-profile-20261102'
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(256*1024):h.update(b)
 return h.hexdigest()
records={n:json.loads((out/(n+'.json')).read_text()) for n in ['runtime-first','clippy-first','pythia-first']}
for n,r in records.items():
 assert r['source_before']==r['source_after'] and not r['remaining_pg_members'] and r['log_sha256']==sha(out/(n+'.log'))
assert records['runtime-first']['test_counts']==[[11,0,0],[4,0,0],[21,0,2]] and records['runtime-first']['exit_code']==0
assert records['clippy-first']['exit_code']==0
assert records['pythia-first']['exit_code'] in [0,-24] and records['pythia-first']['stop_reason'] is None
current=records['runtime-first']['source_after'];assert all(r['source_after']==current for r in records.values())
for p,h in current.items():assert sha(root/p)==h
before=json.loads((old/'hashed-walk-runtime-first.json').read_text())['source_after'];changed=[p for p in current if current[p]!=before[p]]
assert changed==['misaka/palw/tir/exec/src/node/positive_wide_material_v2/proof.rs','misaka/palw/tir/exec/src/node/positive_wide_material_v2/trace.rs','misaka/palw/tir/exec/tests/positive_wide_material_v2.rs']
profiles={}
for line in (out/'runtime-first.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
def measurements(p):return [json.loads(l.split('ROLE_MEASURE ',1)[1]) for l in p.read_text().splitlines() if 'ROLE_MEASURE ' in l]
a={e['detail']['checked_relations']:e for e in measurements(old/'pythia-hashed-walk-first.log') if e['phase']=='node-progress'};b={e['detail']['checked_relations']:e for e in measurements(out/'pythia-first.log') if e['phase']=='node-progress'}
common=[]
for i in sorted(a.keys()&b.keys()):
 for k in ['file_usage','observed_evaluations','peak_elements','peak_reads','peak_terms','peak_workspace_bytes']:assert a[i]['detail'][k]==b[i]['detail'][k],(i,k)
 assert a[i]['requested_peak']==b[i]['requested_peak'] and a[i]['requested_live']==b[i]['requested_live']
 common.append({'numeric_prefix':i,'before_material_ns':a[i]['detail']['producer_material_ns'],'after_material_ns':b[i]['detail']['producer_material_ns'],'before_node_ns':a[i]['detail']['node_check_ns'],'after_node_ns':b[i]['detail']['node_check_ns'],'provider_usage_equal':True,'evaluator_counts_equal':True,'logical_live_peak_equal':True})
(out/'common-prefix-comparison.json').write_text(json.dumps({'scope':'same original short job and opt2/caps, prefix-only actual observations; not full-job extrapolation or trained material byte-equivalence proof','prefixes':common},indent=2)+'\n')
new_model=measurements(out/'pythia-first.log');model_record=records['pythia-first'];binary=records['runtime-first']['binary'];assert sha(Path(binary['path']))==binary['sha256']
(out/'model-run-summary.json').write_text(json.dumps({'exit_code':model_record['exit_code'],'test_counts':model_record['test_counts'],'last_logged_numeric_prefix':b[max(b)],'other_phases':[e for e in new_model if e['phase']!='node-progress'],'outer_monitor':{k:model_record[k] for k in ['stop_reason','elapsed_seconds','minimum_free_bytes','sampled_pg_rss_peak_bytes','remaining_pg_members']},'scope':'shared process, original noncanonical 16/1, SHA-warmed OS caches; no native authority/activation or isolated cold-role qualification'},indent=2)+'\n')
checks=[]
for command in [['rustfmt','--check','--config','skip_children=true','--edition','2024',*changed],['git','diff','--check']]:
 p=subprocess.run(command,cwd=root,capture_output=True,text=True);checks.append({'command':command,'exit_code':p.returncode,'output':p.stdout+p.stderr});assert p.returncode==0
(out/'validation-summary.json').write_text(json.dumps({'implementation_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'changed_rust_files':changed,'runtime_tests':36,'runtime_counts':records['runtime-first']['test_counts'],'strict_selected_clippy':True,'source_matches_runtime_clippy_model_and_current':True,'binary_current_matches':binary,'selected_profiles':profiles,'checks':checks,'actual_model_exit':model_record['exit_code'],'full_goal_complete':False,'native_authority_or_activation':False},indent=2)+'\n')
scripts=out/'operator-scripts';scripts.mkdir(exist_ok=True);inventory=[]
for p in sorted(Path('/tmp').glob('misaka-trace-scratch*-20261102.py')):
 q=scripts/p.name;shutil.copyfile(p,q);inventory.append({'original_path':str(p),'preserved_path':str(q.relative_to(root)),'sha256':sha(q),'scope':'final operator-file bytes at checkpoint; executed commands/source/input snapshots are pinned independently in raw records'})
(out/'operator-script-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for d in [out,profile]:
 files={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'trailing_whitespace_lines':[i+1 for i,l in enumerate(p.read_bytes().splitlines()) if l.rstrip(b' \t')!=l]} for p in sorted(d.iterdir()) if p.suffix in ['.log','.txt']}
 (d/'raw-log-inventory.json').write_text(json.dumps({'audit_only_no_rewrites':True,'files':files},indent=2)+'\n')
print('verified',records['runtime-first']['test_counts'],'model exit',model_record['exit_code'],'common last',common[-1],'last new',b[max(b)]['detail'])
