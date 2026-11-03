#!/usr/bin/env python3
"""SOURCE_NULL v8 proposal: runtime only; no build/compile/debugger invocation exists. Not pinned/executed."""
from pathlib import Path
import argparse,ast,datetime,hashlib,json,os,re,shutil,subprocess,sys
POLICY=Path('/tmp/positive-a0-node-runtime-only-source-null-v8.json')
POLICY_SHA='5e679fc48c41d65cabf6ceda025fc960af49f39e3bed09f479274b107416f229'
def sha(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1<<20):d.update(b)
 return d.hexdigest()
def require(ok,msg):
 if not ok:raise RuntimeError(msg)
def read_pin(pin):
 require(isinstance(pin,dict) and set(pin)>={'path','sha256'} and re.fullmatch('[0-9a-f]{64}',pin['sha256']) is not None,'Explicit full file pin required')
 require(sha(pin['path'])==pin['sha256'],'Pinned file hash mismatch: '+pin['path'])
 return json.loads(Path(pin['path']).read_text())
def helpers(policy):
 # Exact 20 existing definitions. Never compile/evaluate R6 main/group_commands.
 pin=policy['runtime_helpers'];require(sha(pin['path'])==pin['sha256'],'Reviewed helper changed')
 src=Path(pin['path']).read_text();tree=ast.parse(src);selected=[];names=[]
 for node in tree.body:
  if isinstance(node,(ast.Import,ast.ImportFrom)):selected.append(node)
  elif isinstance(node,(ast.FunctionDef,ast.ClassDef)) and node.name not in ['main','group_commands']:
   selected.append(node);names.append(node.name)
 require(len(names)==20 and 'command' in names and 'stable' in names and 'main' not in names and 'group_commands' not in names,'Exact helper allowlist failed')
 ns={'ROOT':None,'TARGET':None,'__name__':'reviewed_runtime_helpers_only'}
 exec(compile(ast.Module(body=selected,type_ignores=[]),pin['path'],'exec'),ns)
 return ns

def closed_command(c,h):
 require(c.get('executed') is True and c.get('no_live_before_raw_hash') is True,'Completed command required')
 require(sha(c['log'])==c['sha256'],'Completed raw mismatch')
 q=c['cleanup'];require(q['direct_child_reaped'] and not q['live_members'] and not q['zombies'],'Closed PG cleanup required')
 require(not h['group_members'](q['group']),'Historical owned PG is live now')

def authenticate(m,policy,h):
 """No source selection, repin, Cargo, model or test invocation. Read/verify only."""
 require(m.get('status')=='ROOT_EXPLICIT_RUNTIME_ONLY_PINNED','SOURCE_NULL or unreviewed plan cannot execute')
 for k in ['execution_source','execution_tree']:
  require(isinstance(m.get(k),str) and re.fullmatch('[0-9a-f]{40}',m[k]) is not None,'Future source/tree explicit pin required')
 for k in ['workspace','target_dir','branch','source_roots','specs','fixed_environment','native_config','native_inputs','required_supported_source_profile','hf_pins','resource_policy','mandatory_tiny_case']:
  require(m.get(k)==policy[k],'Policy contract changed: '+k)
 require(isinstance(m.get('source_before'),dict) and bool(m['source_before']),'Explicit whole source snapshot required')
 require(isinstance(m.get('binary'),dict),'Actual binary pin required')
 root=Path(m['workspace']).resolve();target=Path(m['target_dir']).resolve();require(root.is_dir() and target.is_dir(),'Existing workspace/target only')
 h['head'].__globals__['ROOT']=root;h['head'].__globals__['TARGET']=target
 require(h['stable'](m),'Source/native snapshot changed')
 require(subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()==m['execution_tree'],'Source tree mismatch')
 require(subprocess.check_output(['git','branch','--show-current'],cwd=root,text=True).strip()==m['branch'],'Owner branch mismatch')
 require(not subprocess.check_output(['git','diff','--name-only','HEAD','--'],cwd=root,text=True).strip(),'Tracked source dirty')
 tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'));require(set(m['source_before'])<=tracked,'Untracked eligible source')
 f=read_pin(m['focused_pass_manifest']);p=read_pin(m['focused_pass_plan']);read_helper=m['focused_pass_helper']
 require(sha(read_helper['path'])==read_helper['sha256'],'Focused helper pin mismatch')
 require(f.get('status')=='PASS' and bool(f.get('finished_utc')),'Closed focused PASS required; failure/zero/incomplete refused')
 require(f['source']==m['execution_source']==p['execution_source'] and p['authorized_tree']==m['execution_tree'],'Focused source/tree mismatch')
 require(f['source_before']==m['source_before']==p['source_before'],'Focused full snapshot mismatch')
 require(f['plan']==m['focused_pass_plan'] and f['runner']['sha256']==read_helper['sha256'] and Path(f['runner']['path']).resolve()==Path(read_helper['path']).resolve(),'Focused exact plan/helper linkage')
 require(p['runner_sha256']==read_helper['sha256'] and sha(p['native_config']['path'])==p['native_config']['sha256'],'Focused plan identity')
 require(f['native_config']==m['native_config'] and f['native_inputs']==m['native_inputs'],'Focused native/forensic inputs mismatch')
 require(p['native_inputs']==m['native_inputs'] and p['fixed_environment']==m['fixed_environment'],'Focused native/env policy mismatch')
 require(f['environment_overrides']==m['fixed_environment']|{'CXX':f['environment_overrides']['CXX']} and f['RUST_MIN_STACK_effective'] is None and f['RUSTFLAGS_effective'] is None,'Focused actual environment mismatch')
 require(sha(f['environment_overrides']['CXX'])==f['guard_sha256'],'Focused CXX guard mismatch')
 require(len(f['builds'])==1 and len(f['runs'])==1 and f.get('small_passed')==1 and f.get('hf_cases_collected')==0 and f.get('os_main_passed')==0,'Focused exact1 and no HF/OS required')
 b=f['builds'][0];t=f['runs'][0];li=t['identity_list'];a=m['binary'];name=m['mandatory_tiny_case'];profile=m['required_supported_source_profile']
 require(b['success'] and b['exit_code']==0 and b['stop_reason'] is None and b['inputs_stable'] and b['mixed_artifact_profiles_ok'],'Focused actual build PASS required')
 require(b.get('supported_source_profile_required') is True and b.get('supported_source_profile_verified') is True,'Mandatory audited-source gate required')
 require(b['observed_core_build_profiles'] and all(profile['cfg'] in q['cfgs'] and q['env'].get(profile['environment_name'])==profile['environment_value'] for q in b['observed_core_build_profiles']),'Actual cfg and env required')
 require(b['command'][:8]==['cargo','test','-p','kaspa-consensus','--offline','--locked','--no-run','--message-format=json-render-diagnostics'] and b['command'][-1]=='--lib','Focused Node library build provenance required')
 require(b['artifacts']=={'kaspa_consensus':a} and t['artifact']==a and t['binary_stable'],'Actual focused compiled/runtime artifact must match')
 require(a['profile']['opt_level']=='0' and a['profile']['debuginfo']==0 and a['profile']['test'],'Actual Node opt0/debug0 test profile required')
 binary=Path(a['path']).resolve();require(binary.is_relative_to(target/'debug/deps') and re.fullmatch('kaspa_consensus-[a-f0-9]+',binary.name),'Actual Node target executable only')
 require(binary.stat().st_size==a['bytes'] and sha(binary)==a['sha256'],'Actual runtime binary changed')
 for c in [b,li,t]:closed_command(c,h)
 require(li['exact_identity'] and li['exit_code']==0 and li['stop_reason'] is None and li['listed_test_names']==[name] and h['listed'](Path(li['log']).read_text(errors='replace'))==[name] and li['command']==[a['path'],name,'--exact','--list'],'Focused exact identity required')
 require(t['command']==[a['path'],name,'--exact','--test-threads=1'] and t['success'] and t['exit_code']==0 and t['stop_reason'] is None and t['inputs_stable'],'Focused actual one-case runtime PASS required')
 raw=Path(t['log']).read_text(errors='replace');expect=[{'status':'ok','passed':1,'failed':0,'ignored':0}]
 require(h['summaries'](raw)==t['outer_summaries']==t['summaries']==expect and re.search(r'^test '+re.escape(name)+r' \.\.\. ok$',raw,re.M),'Zero or wrong focused summary cannot qualify')
 require(not any(x in raw for x in profile['unsupported_markers']),'Unsupported focused profile cannot qualify')
 # Independently match original Cargo JSON: not only copied manifest booleans.
 finish=[];artifacts=[];gates=[]
 for line in Path(b['log']).read_text(errors='replace').splitlines():
  try:v=json.loads(line)
  except ValueError:continue
  if v.get('reason')=='build-finished':finish.append(v.get('success'))
  if v.get('reason')=='compiler-artifact' and v.get('executable') and v.get('target',{}).get('name')=='kaspa_consensus':artifacts.append(v)
  if v.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(v.get('package_id','')):gates.append(v)
 require(finish==[True] and any(Path(q['executable']).resolve()==binary and q['profile']==a['profile'] for q in artifacts),'Raw actual compiler-artifact and build-finished required')
 require(gates and all(profile['cfg'] in q.get('cfgs',[]) and dict(q.get('env',[])).get(profile['environment_name'])==profile['environment_value'] for q in gates),'Raw actual audited cfg/env required')
 # Imported focused helper is never executed; compare all22 definitions to reviewed reference.
 def defs(path):
  s=Path(path).read_text();return {x.name:(ast.get_source_segment(s,x),ast.dump(x,include_attributes=False)) for x in ast.parse(s).body if isinstance(x,(ast.FunctionDef,ast.ClassDef))}
 require(len(defs(read_helper['path']))==22 and defs(read_helper['path'])==defs(policy['runtime_helpers']['path']),'Focused control22 changed')
 return f

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--inspect-template',action='store_true');ap.add_argument('--plan',type=Path);ap.add_argument('--plan-sha256');ap.add_argument('--root-go',action='store_true');ap.add_argument('--run',action='store_true');ap.add_argument('--scope',choices=['small','hf','all','journal'],default='all');ap.add_argument('--stem')
 args=ap.parse_args();require(sha(POLICY)==POLICY_SHA,'SOURCE_NULL policy changed');policy=json.loads(POLICY.read_text())
 if args.inspect_template:
  require(not args.run,'Inspection cannot execute');print(json.dumps({'status':policy['status'],'source':None,'binary':None,'focused_pass':None,'scopes':policy['expected_scope_inventory'],'runtime_observations':None}));return 0
 require(args.run and args.root_go and args.plan and re.fullmatch('[0-9a-f]{64}',args.plan_sha256 or ''),'Separate Root explicit-plan SHA and GO required; SOURCE_NULL cannot execute')
 require(sha(args.plan)==args.plan_sha256,'Root-reviewed actual execution plan changed');m=json.loads(args.plan.read_text());h=helpers(policy);authenticate(m,policy,h)
 require(sys.platform=='darwin','Owner Darwin runtime only');require(shutil.disk_usage(m['target_dir']).free>=m['resource_policy']['initial_free_bytes'],'Runtime-only initial2GiB floor unmet')
 require(re.fullmatch('[a-zA-Z0-9_-]+',args.stem or ''),'Unique explicit output stem required')
 specs=[s for s in m['specs'] if (s['id']=='a6_native_positive_journal_14' if args.scope=='journal' else args.scope=='all' or bool(s['hf'])==(args.scope=='hf'))];require(bool(specs),'No selected exact scope')
 if args.scope=='journal':
  require(len(specs)==1 and specs[0]['group']=='node' and specs[0]['key']=='kaspa_consensus' and specs[0]['filter']=='positive_journal' and not specs[0]['exact'] and not specs[0]['hf'] and not specs[0]['ignored'] and len(specs[0]['names'])==len(set(specs[0]['names']))==14 and specs[0].get('requires_supported_source_profile') is True,'Journal scope must be exactly the existing unique14-test native journal spec')
 root=Path(m['workspace']);out=root/'docs/evidence/positive-court-a0-2026-10-31/integration';record=out/(args.stem+'.json');require(not record.exists(),'Never overwrite evidence')
 env,removed=h['scrub_inherited_environment'](os.environ);h['validate_fixed_environment'](m,policy,Path(m['target_dir']));env.update(m['fixed_environment']);guard=out/(args.stem+'-forbid-cxx.py')
 with guard.open('x') as f:f.write('#!/usr/bin/env python3\nimport sys\nsys.stderr.write("C++ compile/probe forbidden in runtime-only driver\\n")\nsys.exit(95)\n')
 guard.chmod(0o555);env['CXX']=str(guard)
 rec={'schema':'misaka.node-runtime-only.observation.v1','status':'RUNNING','started_utc':h['utc'](),'source':m['execution_source'],'tree':m['execution_tree'],'plan':{'path':str(args.plan),'sha256':args.plan_sha256},'runtime_driver':{'path':str(Path(__file__).resolve()),'sha256':sha(__file__)},'focused_pass_link':m['focused_pass_manifest'],'reviewed_helpers':policy['runtime_helpers'],'source_before':m['source_before'],'native_config':m['native_config'],'native_inputs':m['native_inputs'],'actual_binary':m['binary'],'environment_overrides':m['fixed_environment']|{'CXX':str(guard)},'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'resource_policy':m['resource_policy'],'selected_scopes':specs,'builds':[],'runs':[],'older_144_plus_OS1_not_aggregated':True,'whole163_or_Clippy_or_credit_qualification':False}
 h['write_new'](record,rec)
 def save():record.write_text(json.dumps(rec,ensure_ascii=False,indent=2)+'\n')
 allok=True
 try:
  for s in specs:
   a=m['binary'];require(h['stable'](m) and sha(a['path'])==a['sha256'],'Source/native/binary drift before runtime-only selection')
   base=[a['path']]+([s['filter']] if s['filter'] else [])+(['--exact'] if s['exact'] else [])+(['--ignored'] if s['hf'] else [])
   # The only command() arguments are this immutable test artifact plus libtest flags.
   li=h['command'](out/(args.stem+'-'+s['id']+'-list.log'),base+['--list'],env,timeout=60)
   actual=h['listed'](Path(li['log']).read_text(errors='replace'));li['listed_test_names']=actual;li['exact_identity']=li['exit_code']==0 and li['stop_reason'] is None and sorted(actual)==sorted(s['names']+s['ignored']) and bool(actual)
   require(li.get('stop_reason') is None,'Selection command resource/cancellation stopped; no workaround')
   if not li['exact_identity']:
    require(s['hf'],'Missing/changed/zero small identity; no runtime or fallback')
    sp=out/(args.stem+'-'+s['id']+'-stages.json')
    h['write_new'](sp,{'model':s['model'],'stages':[],'results':[],'stage_record_complete':False,'error':'exact ignored selector did not identify one expected test; no zero-test runtime success'});sp.chmod(0o444)
    rec['runs'].append({'scope':s,'identity_list':li,'executed':False,'success':False,'exact_one_executed_summary':False,'stage_json':{'path':str(sp),'sha256':sha(sp)},'error':'missing_changed_or_zero_selector'});allok=False;save();continue
   missing=[]
   if s['hf']:
    pin=next(x for x in m['hf_pins']['models'] if x['slug']==s['model']);missing=[pin[k]['path'] for k in ['artifact','fixture','inventory'] if not Path(pin[k]['path']).is_file()]
   t=h['command'](out/(args.stem+'-'+s['id']+'.log'),base+(['--nocapture'] if s['hf'] else [])+['--test-threads=1'],env,timeout=1800,rss=8589934592 if s['hf'] else None)
   raw=Path(t['log']).read_text(errors='replace');sums=h['summaries'](raw);unsupported=[x for x in m['required_supported_source_profile']['unsupported_markers'] if x in raw];n=len(s['names']);ig=len(s['ignored']);counts=sums==[{'status':'ok','passed':n,'failed':0,'ignored':ig}];named=True if s['hf'] else all(re.search(r'^test '+re.escape(v)+r' \.\.\. ok$',raw,re.M) for v in s['names'])
   t.update({'scope':s,'identity_list':li,'artifact':a,'summaries':sums,'outer_summaries':sums,'nested_summaries_not_counted':[],'unsupported_early_return_markers':unsupported,'nonzero_exact_success_counts':n>0 and counts,'test_names_match':bool(named),'missing_inputs_before_case':missing,'inputs_stable':h['stable'](m),'binary_stable':sha(a['path'])==a['sha256']})
   if s['hf']:
    stages=h['hf_json'](raw,s['model']);sp=out/(args.stem+'-'+s['id']+'-stages.json');h['write_new'](sp,stages);sp.chmod(0o444);t['stage_json']={'path':str(sp),'sha256':sha(sp)};t['structured_native_result_success']=stages['stage_record_complete'] and stages['results'][0].get('result')=='native_registration_checked';t['exact_one_executed_summary']=len(sums)==1 and sums[0]['passed']+sums[0]['failed']==1 and sums[0]['ignored']==0
   t['success']=t['exit_code']==0 and t['stop_reason'] is None and t['nonzero_exact_success_counts'] and t['test_names_match'] and t['inputs_stable'] and t['binary_stable'] and not missing and not unsupported and (not s['hf'] or t['structured_native_result_success'])
   rec['runs'].append(t);allok &= t['success'];save()
   require(t.get('stop_reason') not in ['disk_before_start','disk_below_512MiB','descendant_leak'],'Fatal own-PG/resource stop')
   require(t['inputs_stable'] and t['binary_stable'],'Fundamental input drift; stop')
   require(t['success'] or s['hf'],'Small runtime failure; no automatic retry')
  rec['status']='RUNTIME_ONLY_PASS' if allok else 'RUNTIME_ONLY_COMPLETE_WITH_HF_REFUSALS';rec['small_passed']=sum(len(x['scope']['names']) for x in rec['runs'] if x['success'] and not x['scope']['hf']);rec['HF_collected']=sum(x['scope']['hf'] for x in rec['runs']);rec['HF_passed']=sum(x['scope']['hf'] and x['success'] for x in rec['runs']);rec['finished_utc']=h['utc']();save();record.chmod(0o444);return 0 if allok else 1
 except BaseException as e:
  rec['status']='INCOMPLETE';rec['stopped_reason']=repr(e);rec['interrupted_or_failed_command']=getattr(e,'command_result',None);rec['finished_utc']=h['utc']();save();record.chmod(0o444);return 1
if __name__=='__main__':raise SystemExit(main())
