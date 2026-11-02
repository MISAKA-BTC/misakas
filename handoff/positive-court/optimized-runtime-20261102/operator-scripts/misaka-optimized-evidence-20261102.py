from pathlib import Path
import hashlib,json,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/optimized-runtime-20261102'
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(256*1024):h.update(b)
 return h.hexdigest()
records={n:json.loads((out/(n+'.json')).read_text()) for n in ['optimized-runtime-first','optimized-runtime-r2','hashed-walk-runtime-first','hashed-walk-clippy-first','pythia-optimized-first','pythia-hashed-walk-first']}
for name,r in records.items():
 assert r['source_before']==r['source_after'] and not r['remaining_pg_members']
 assert r['log_sha256']==sha(out/(name+'.log'))
for n in ['optimized-runtime-r2','hashed-walk-runtime-first']:
 assert records[n]['test_counts']==[[11,0,0],[4,0,0],[20,0,2]] and records[n]['exit_code']==0 and records[n]['stop_reason'] is None
assert records['hashed-walk-clippy-first']['exit_code']==0 and records['hashed-walk-clippy-first']['stop_reason'] is None
for n in ['pythia-optimized-first','pythia-hashed-walk-first']:
 assert records[n]['exit_code']==-24 and records[n]['stop_reason'] is None and records[n]['test_counts']==[]
base=records['optimized-runtime-r2']['source_after'];current=records['hashed-walk-runtime-first']['source_after'];changed=[p for p in current if current[p]!=base[p]]
assert changed==['misaka/palw/tir/exec/src/node/positive_wide_material_v2/trace.rs','misaka/palw/tir/exec/tests/positive_wide_material_v2.rs']
for n in ['hashed-walk-clippy-first','pythia-hashed-walk-first']:assert records[n]['source_after']==current
assert records['pythia-optimized-first']['source_after']==base
for p,h in current.items():assert sha(root/p)==h
binary=records['hashed-walk-runtime-first']['binary'];assert sha(Path(binary['path']))==binary['sha256']
profiles={}
for line in (out/'hashed-walk-runtime-first.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name') in ['kaspa_consensus_core','misaka_palw_tir','misaka_palw_tir_exec','positive_wide_material_v2']:profiles[e['target']['name']]=e['profile']
assert len(profiles)==4 and all(p['opt_level']=='2' and p['debug_assertions'] and p['overflow_checks'] and p['debuginfo']==0 for p in profiles.values())
(out/'hashed-walk-verified-profiles.json').write_text(json.dumps(profiles,indent=2)+'\n')
scripts=out/'operator-scripts';scripts.mkdir(exist_ok=True);inventory=[]
for p in sorted(set(Path('/tmp').glob('misaka-optimized*-20261102.py'))|set(Path('/tmp').glob('misaka-hashed-walk*-20261102.py'))):
 target=scripts/p.name;shutil.copyfile(p,target);inventory.append({'original_path':str(p),'preserved_path':str(target.relative_to(root)),'sha256':sha(target),'qualification':'final operator-file bytes at checkpoint; runtime records separately pin executed commands, input/source snapshots, exits and logs'})
(out/'operator-script-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
commands=[['rustfmt','--check','--config','skip_children=true','--edition','2024',*changed],['git','diff','--check']]
checks=[]
for command in commands:
 r=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);checks.append({'command':command,'exit_code':r.returncode,'output':r.stdout});assert r.returncode==0
# Audit raw logs without rewriting their bytes or their digests.
raw=[]
for p in sorted(out.glob('*.log')):
 lines=p.read_bytes().splitlines();raw.append({'path':str(p.relative_to(root)),'sha256':sha(p),'trailing_whitespace_lines':[i+1 for i,l in enumerate(lines) if l.rstrip(b' \t')!=l]})
(out/'whitespace-check.json').write_text(json.dumps({'checks':checks,'raw_log_audit_only':raw,'skip_children_scope':'only the two changed files; existing unrelated child-module formatting is not normalized'},indent=2)+'\n')
summary={'implementation_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'before_source_commit':'1f250d0f9','changed_rust_files':changed,'source_unchanged_during_each_run':True,'baseline_source_matches_baseline_model':True,'final_source_matches_runtime_model_and_clippy':True,'runtime_counts':records['hashed-walk-runtime-first']['test_counts'],'strict_selected_clippy':0,'binary_current_sha_matches_compiled_record':binary,'selected_profiles':profiles,'toolchain':subprocess.check_output(['rustc','-vV'],cwd=root,text=True),'trained_model_success':False,'full_node_verdict':False,'native_authority_or_activation':False,'new_production_config':False,'qualification':'local prefix performance only; current producer/Node coexist and retain producer owners; source hashing warms OS caches'}
(out/'validation-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k not in ['selected_profiles','binary_current_sha_matches_compiled_record']},indent=2))
