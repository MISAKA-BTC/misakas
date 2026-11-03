#!/usr/bin/env python3
"""Runtime ONLY, SOURCE_NULL until Root supplies closed39/exact cosmetic-lint-pending+actual artifact pins and separate GO. No Cargo/build/model pre-read."""
from pathlib import Path
import argparse,ast,hashlib,json,os,re,shutil,subprocess,sys
POLICY=Path('/tmp/positive-a0-ordinary-trained-runtime-only-source-null-v4.json')
POLICY_SHA='7ba190ae66ba10c4c0172a9e0eaa37f1f0d0980d7f4ada6acab104996c28eb3b'
def sha(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1<<20):d.update(b)
 return d.hexdigest()
def need(ok,msg):
 if not ok:raise RuntimeError(msg)
def pin(p):
 need(isinstance(p,dict) and re.fullmatch('[0-9a-f]{64}',p.get('sha256','')) and isinstance(p.get('path'),str),'Explicit actual file pin required')
 need(sha(p['path'])==p['sha256'],'Pinned file changed: '+p['path'])
 return json.loads(Path(p['path']).read_text())
def controls(policy):
 p=policy['v8_loader'];need(sha(p['path'])==p['sha256'],'ReviewedV8loader changed')
 tree=ast.parse(Path(p['path']).read_text());selected=[];names=[]
 for n in tree.body:
  if isinstance(n,(ast.Import,ast.ImportFrom)):selected.append(n)
  elif isinstance(n,ast.FunctionDef) and n.name in ['sha','require','helpers','closed_command']:selected.append(n);names.append(n.name)
 need(set(names)=={'sha','require','helpers','closed_command'},'V8 exact loader surface changed')
 ns={'__name__':'ordinary_runtime_loader_only'};exec(compile(ast.Module(body=selected,type_ignores=[]),p['path'],'exec'),ns)
 return ns['helpers'](policy),ns['closed_command'] # V8 loads exact20 unchanged R6 controls; never main/group_commands.
def composed39(f,fp,m,h,closed,policy):
 need(f.get('schema')==policy['composed39_schema'],'Unknown composed39 schema')
 c=f.get('composition',{});need(c.get('kind')==policy['composed39_kind'] and c.get('original_manifests_unchanged') is True,'Explicit A7only recovery composition required')
 pins=c.get('physical_manifests',{});need(set(pins)=={'primary','a7_recovery'},'Exactly both physical manifest pins required')
 parts={k:pin(v) for k,v in pins.items()}
 need(c.get('physical_original_status')=={'primary':'INCOMPLETE','a7_recovery':'PASS'} and parts['primary'].get('status')=='INCOMPLETE' and parts['a7_recovery'].get('status')=='PASS','Never relabel physical originals')
 need('environment_overrides' not in f and 'guard_sha256' not in f,'Composed39 must not pretend one physical environment/guard')
 for key,r in parts.items():
  need(r.get('finished_utc') and all(r.get(k)==f.get(k) for k in ['source','source_before','native_config','native_inputs','plan','runner']),'Physical same-source/plan/driver/static scope mismatch')
  actual={k:r.get(k) for k in ['environment_overrides','guard_sha256','RUST_MIN_STACK_effective','RUSTFLAGS_effective']}
  need(c.get('physical_environments',{}).get(key)==actual and actual['RUST_MIN_STACK_effective'] is None and actual['RUSTFLAGS_effective'] is None,'Physical environment receipt mismatch')
  env=actual['environment_overrides'];need(isinstance(env,dict) and 'CXX' in env and env==m['fixed_environment']|{'CXX':env['CXX']} and sha(env['CXX'])==actual['guard_sha256'],'Actual physical CXX/environment mismatch')
 p,r=parts['primary'],parts['a7_recovery']
 need(len(p['builds'])==3 and len(p['runs'])==12 and len(r['builds'])==1 and len(r['runs'])==1,'Exact primary34/recovery5 physical scope count')
 need(p['selected_scopes']==fp['specs'] and r['selected_scopes']==[x for x in fp['specs'] if x['group']=='a7'],'Recovery must be A7ONLY same original plan')
 failed=p['builds'][2];closed(failed,h)
 expected_failure={'build_index':2,'group':'a7','stop_reason':'disk_below_512MiB','exit_code':-15,'cleanup_group':failed['cleanup']['group']}
 need(fp['builds'][2]['group']=='a7' and c.get('allowed_primary_failure')==expected_failure and failed.get('stop_reason')==expected_failure['stop_reason'] and failed.get('exit_code')==-15 and not failed.get('success'),'Only original closed A7 disk-stop failure permitted')
 bo=[{'manifest':'primary','index':0},{'manifest':'primary','index':1},{'manifest':'a7_recovery','index':0}];ro=[{'manifest':'primary','index':i} for i in range(12)]+[{'manifest':'a7_recovery','index':0}]
 need(c.get('build_origins')==bo and c.get('run_origins')==ro,'Exact physical entry origins required')
 need(f['builds']==[parts[x['manifest']]['builds'][x['index']] for x in bo] and f['runs']==[parts[x['manifest']]['runs'][x['index']] for x in ro],'Composed entries must be exact deep copies of physical successful entries')
 counts={k:sum(len(t['scope']['names']) for t in v['runs']) for k,v in parts.items()}
 need(c.get('success_counts')==counts=={'primary':34,'a7_recovery':5},'Exact34+5 only')
 need(f.get('finished_utc')==r['finished_utc'] and isinstance(f.get('derived_at_utc'),str) and f['derived_at_utc'],'Availability/derivation time receipts required')
 need((f.get('small_passed'),f.get('os_main_passed'),f.get('hf_passed'),f.get('hf_cases_collected'))==(39,0,0,0),'No model/OS or older-source aggregate')
 for v in pins.values():need(sha(v['path'])==v['sha256'],'Physical original changed during verification')

def pending_style_lint(lint,m,policy,h,closed):
 frozen=policy['frozen_functional_phase']
 need(m['execution_source']==frozen['source'] and m['execution_tree']==frozen['tree'] and len(m['source_before'])==frozen['source_count'],'Exact frozen a867/4956 functional phase only')
 need(m['prerequisite39_manifest']==frozen['prerequisite39_manifest'] and m['lint_manifest']==frozen['lint_manifest'] and m['binary']==frozen['binary'],'Exact closed composed39/lint/caller463e only')
 need(lint.get('schema')==policy['required_lint_schema'] and lint.get('status')=='INCOMPLETE' and lint.get('finished_utc'),'Actual cosmetic lint remains INCOMPLETE')
 need(lint.get('runtime_executed_by_lint_executor')==0 and lint.get('RUST_MIN_STACK_effective') is None and lint.get('RUSTFLAGS_effective') is None,'Lint cannot supply runtime or stack overrides')
 env=lint.get('environment_overrides',{});need('CXX' in env and env==m['fixed_environment']|{'CXX':env['CXX']} and sha(env['CXX'])==lint.get('CXXguard_sha256'),'Actual lint environment/guard changed')
 steps=lint.get('lint_commands',[]);need(len(steps)==2 and 'selected_lint_surfaces' not in lint and 'source_after' not in lint,'Only Core completed, Exec stopped; A7/SDK lint UNRUN')
 for index,step in enumerate(steps):
  closed(step,h);need(step.get('stop_reason') is None and step.get('inputs_stable') and step.get('runtime_tests_executed')==0,'Lint command not closed/stable')
  need({'path':step['log'],'sha256':step['sha256']}==frozen['lint_raw_pins'][index],'Exact known closed raw required')
  raw=Path(step['log']).read_text(errors='strict');events=[]
  for line in raw.splitlines():
   try:events.append(json.loads(line))
   except ValueError:pass
  need(not [v for v in events if v.get('reason')=='compiler-message' and v.get('message',{}).get('level') in ['warning','error']],'Unexpected JSON compiler diagnostics')
  selected=step['selection'];need(step['command']==selected['argv'],'Selected lint command mismatch')
  if index==0:
   need(selected['id']=='core_geometry' and step.get('success') is True and step.get('exit_code')==0 and [v.get('success') for v in events if v.get('reason')=='build-finished']==[True],'Core lint must actually PASS')
   need(step.get('selected_warning_error_JSON_count')==0 and not re.search(r'^(?:error|warning)(?:\[|:)',raw,re.M),'Core diagnostic found')
   for target in selected['targets']:
    matches=[v for v in events if v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')==target['name'] and target['kind'] in v.get('target',{}).get('kind',[]) and selected['package'] in str(v.get('package_id',''))]
    need(target['required_debug']==0 and matches and all(v['profile']['debuginfo']==0 for v in matches),'Actual Core profile0 metadata missing')
  else:
   need(selected['id']=='exec_forward_metadata' and step.get('success') is False and step.get('exit_code')==101 and [v.get('success') for v in events if v.get('reason')=='build-finished']==[False],'Exec exact known lint failure required')
   need(re.findall(r'^error(?:\[[^]]+\])?:.*$',raw,re.M)==['error: this `if` statement can be collapsed','error: could not compile `misaka-palw-tir-exec` (lib) due to 1 previous error'],'Only the one known cosmetic diagnostic and Cargo summary allowed')
   need(re.findall(r'^warning(?:\[[^]]+\])?:.*$',raw,re.M)==['warning: build failed, waiting for other jobs to finish...'],'Unexpected warning')
   need(raw.count('   --> '+frozen['diagnostic_location'])==1 and '#collapsible_if' in raw and '`-D clippy::collapsible-if`' in raw,'Exact known collapsible_if at metadata385 required')
 need(lint.get('ordinary39_prerequisite',{}).get('manifest')==m['prerequisite39_manifest'],'Lint must link exact composed39')

def authenticate(m,policy,h,closed):
 need(m.get('status')=='ROOT_EXPLICIT_ORDINARY_RUNTIME_ONLY_PINNED','SOURCE_NULL cannot execute')
 for k in ['execution_source','execution_tree']:need(re.fullmatch('[0-9a-f]{40}',m.get(k) or ''),'Actual source/tree pin missing')
 for k in ['workspace','target_dir','branch','source_roots','native_config','fixed_environment','required_supported_source_profile','runtime_helpers','v8_loader','cases','resource_policy']:
  need(m.get(k)==policy[k],'Immutable policy changed: '+k)
 need(isinstance(m.get('source_before'),dict) and m['source_before'],'Whole source snapshot missing')
 focused=pin(m['prerequisite39_manifest']);fp=pin(focused['plan'])
 need(isinstance(m.get('native_inputs'),dict) and len(m['native_inputs'])==policy['required_native_input_count'] and m['native_inputs']==focused.get('native_inputs')==fp.get('native_inputs'),'Exact final39 full26 native input binding required')
 root=Path(m['workspace']).resolve();target=Path(m['target_dir']).resolve();need(root.is_dir() and target.is_dir(),'Existing selected workspace/target only')
 h['head'].__globals__['ROOT']=root;h['head'].__globals__['TARGET']=target
 def stable():
  return h['stable'](m) and subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()==m['execution_tree']
 need(stable(),'Source/native snapshot drift')
 need(subprocess.check_output(['git','branch','--show-current'],cwd=root,text=True).strip()==m['branch'],'Owner branch changed')
 need(not subprocess.check_output(['git','diff','--name-only','HEAD','--'],cwd=root,text=True).strip(),'Tracked source dirty')
 tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'));need(set(m['source_before'])<=tracked,'Untracked source snapshot')
 need(m.get('runtime_driver')=={'path':str(Path(__file__).resolve()),'sha256':sha(__file__)},'Actual runtime driver pin missing')
 lint=pin(m['lint_manifest'])
 need(focused.get('status') in policy['allowed_prerequisite39_statuses'] and focused.get('finished_utc') and focused.get('small_passed')==39,'Closed explicit actual39 PASS/composed39 required')
 if focused['status']=='SAME_SOURCE_COMPOSED_39_PASS':composed39(focused,fp,m,h,closed,policy)
 else:
  need('composition' not in focused and focused.get('schema')!=policy['composed39_schema'],'A composed receipt cannot be relabeled ordinary PASS')
  env=focused.get('environment_overrides',{});need('CXX' in env and env==m['fixed_environment']|{'CXX':env['CXX']} and focused.get('RUST_MIN_STACK_effective') is None and focused.get('RUSTFLAGS_effective') is None and sha(env['CXX'])==focused.get('guard_sha256'),'Single physical PASS environment/guard required')
 pending_style_lint(lint,m,policy,h,closed)
 for rec in [focused,lint]:
  need(rec.get('source')==m['execution_source'] and rec.get('source_before')==m['source_before'],'Prerequisite source/snapshot mismatch')
  need(rec.get('native_config')==m['native_config'],'Native config mismatch')
  need(rec.get('native_inputs')==m['native_inputs'],'Full final39 native/static inputs differ')
 need(fp['execution_source']==m['execution_source'] and fp['source_before']==m['source_before'] and fp['specs']==focused['selected_scopes'],'Actual39 plan/source/scope link')
 count=0;names=[]
 for run in focused['runs']:
  closed(run,h);closed(run['identity_list'],h);scope=run['scope'];raw=Path(run['log']).read_text(errors='replace')
  need(run.get('success') and run.get('inputs_stable') and run.get('binary_stable') and run.get('stop_reason') is None and not scope.get('hf') and not scope.get('os_main'),'39 contains nonclosed/refused/model runtime')
  need(h['summaries'](raw)==[{'status':'ok','passed':len(scope['names']),'failed':0,'ignored':len(scope['ignored'])}],'Actual39 raw count mismatch')
  need(sorted(h['listed'](Path(run['identity_list']['log']).read_text()))==sorted(scope['names']+scope['ignored']),'39 selector raw mismatch')
  count+=len(scope['names']);names += [(scope['key'],name) for name in scope['names']]
 need(count==len(set(names))==39,'39 unique nonzero cases required')
 link=lint.get('ordinary39_prerequisite',{});need(link.get('manifest')==m['prerequisite39_manifest'] and link.get('outer_pure_tests')==39 and link.get('models_executed')==0,'Clippy4 does not link exact selected actual39 manifest')
 a=m.get('binary');need(isinstance(a,dict),'Actual A7 runtime artifact missing');binary=Path(a['path']).resolve()
 need(binary.is_relative_to(target/'debug/deps') and re.fullmatch('ordinary_trained_forward_guarded_v2-[a-f0-9]+',binary.name),'Actual selected A7 test target only')
 need(a['profile']['test'] and a['profile']['opt_level']=='0' and a['profile']['debuginfo']==2,'Actual A7 mixedprofile artifact required')
 builds=[b for b in focused['builds'] if b.get('artifacts',{}).get('ordinary_trained_forward_guarded_v2')==a];need(len(builds)==1,'Binary is not actual39 compiled artifact')
 b=builds[0];closed(b,h);need(b['success'] and b['inputs_stable'] and b['supported_source_profile_verified'],'Actual A7 supported-source build required')
 events=[]
 for line in Path(b['log']).read_text(errors='replace').splitlines():
  try:events.append(json.loads(line))
  except ValueError:pass
 need(any(v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')=='ordinary_trained_forward_guarded_v2' and Path(v.get('executable') or '/').resolve()==binary and v['profile']==a['profile'] for v in events),'Raw compiler-artifact mismatch')
 need(any(v.get('reason')=='build-finished' and v.get('success') is True for v in events),'Raw compile not complete')
 def binary_stable():return binary.stat().st_size==a['bytes'] and sha(binary)==a['sha256']
 need(binary_stable() and stable(),'Binary/source/native drift after authentication')
 return stable,binary_stable
REPORT='ORDINARY_LOCAL_SELF_CONSISTENCY_ONLY'
def parse_report(raw,case):
 need(raw.count(REPORT)==1,'Exactly one fresh structured ordinary report required')
 line=next(v[v.index(REPORT)+len(REPORT):] for v in raw.splitlines() if REPORT in v);fields={}
 for item in line.split():
  need('=' in item,'Malformed report field');key,val=item.split('=',1);need(key not in fields,'Duplicate report field');fields[key]=val
 exact={'model':case['model'],'REQUESTED_BYTES_CAP':'8589934592','physical_rss_quota':'UNSUPPORTED','outer_monitor':'EXTERNAL_NOT_ATTESTED','outer_monitor_semantics':'sampled_pg_kill_after_sample_only','cpu_limit_process_seconds':case['environment']['POSITIVE_ORDINARY_CPU_SECONDS'],'context_max':str(case['original_max_context']),'prefill':'16','decode':'1','metadata_allowance':case['environment']['POSITIVE_ORDINARY_METADATA_BYTES'],'ordinary_no_trace':'1','dense_comparison_has_trace':'1','node_claim':'0','native_authority':'0','public_P':'0','full_context_inference':'0','independent_HF_expected':'0'}
 need(all(fields.get(k)==v for k,v in exact.items()),'Actual report scope/model/context mismatch')
 numeric=['earlier_E','peak_requested','cumulative_requested','load_ns','ordinary_ns','dense_prepare_ns','dense_ns']
 need(all(re.fullmatch('[0-9]+',fields.get(k,'')) for k in numeric),'Missing numeric report fields')
 need(0<int(fields['earlier_E'])<=512*(1<<20) and 0<int(fields['peak_requested'])<=8*(1<<30) and int(fields['cumulative_requested'])>=int(fields['peak_requested']),'Actual request ledger report invalid')
 need(int(fields['ordinary_ns'])>0 and int(fields['dense_ns'])>0,'Actual CPU phases missing')
 need(re.fullmatch('[0-9]+,',fields.get('generated','')),'Exactly one generated ID required');ids=[int(fields['generated'][:-1])];need(ids[0]<case['token_bound'],'Generated ID outside original vocabulary')
 need(re.fullmatch('[0-9a-f]{128}',fields.get('output_hash','')),'Actual existing output hash missing')
 need(set(fields)==set(exact)|set(numeric)|{'generated','output_hash'},'Unexpected structured report fields')
 return {'reported':fields,'actual_generated_ids':ids,'actual_output_hash':fields['output_hash'],'expected_context_hash_NOT_REPORTED':case['expected_context_hash_SOURCE_ENFORCED_NOT_REPORTED'],'context_binding_and_dense_bool_IDs_hash_match':'ENFORCED_BY_PINNED_CALLER_SOURCE_BEFORE_EMIT_NOT_INDEPENDENT_REPORT_COLUMNS','independent_HF_expected':False,'scope':'same-owned dense SELF CONSISTENCY ONLY'}
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--inspect-template',action='store_true');ap.add_argument('--run',action='store_true');ap.add_argument('--root-go',action='store_true');ap.add_argument('--plan',type=Path);ap.add_argument('--plan-sha256');ap.add_argument('--stem');args=ap.parse_args()
 need(sha(POLICY)==POLICY_SHA,'SOURCE_NULL policy changed');policy=json.loads(POLICY.read_text())
 if args.inspect_template:
  need(not args.run,'Inspection cannot dispatch');print(json.dumps({'status':policy['status'],'source':None,'tree':None,'binary':None,'prerequisite39':None,'lint':None,'model_processes':0,'case_names':[c['name'] for c in policy['cases']]}));return 0
 need(args.run and args.root_go and args.plan and re.fullmatch('[0-9a-f]{64}',args.plan_sha256 or ''),'Root actual plan pin and SEPARATE GO required')
 need(sha(args.plan)==args.plan_sha256,'Actual Root plan changed');m=json.loads(args.plan.read_text());h,closed=controls(policy);stable,binary_stable=authenticate(m,policy,h,closed)
 need(sys.platform=='darwin','Selected local Darwin owner only');need(shutil.disk_usage(m['target_dir']).free>=2*(1<<30),'Runtime initial2GiB free floor unmet')
 need(re.fullmatch('[A-Za-z0-9_-]+',args.stem or ''),'Unique explicit evidence stem required')
 out=Path(m['workspace'])/'docs/evidence/positive-court-a0-2026-10-31/integration';record=out/(args.stem+'.json');need(not record.exists(),'Never overwrite evidence')
 env,removed=h['scrub_inherited_environment'](os.environ)
 for key in list(env):
  if key.startswith('POSITIVE_ORDINARY_'):removed[key]=env.pop(key)
 h['validate_fixed_environment'](m,policy,Path(m['target_dir']));env.update(m['fixed_environment'])
 rec={'schema':'misaka.ordinary-trained-runtime-only.observation.v2','functional_testing_phase':policy['functional_testing_phase'],'lint_qualification':'COSMETIC_LINT_PENDING','unrun_lint_groups':['a7','sdk'],'strict_lint_pass':False,'native_P_public_qualification':False,'status':'RUNNING','started_utc':h['utc'](),'source':m['execution_source'],'tree':m['execution_tree'],'plan':{'path':str(args.plan),'sha256':args.plan_sha256},'driver':{'path':str(Path(__file__).resolve()),'sha256':sha(__file__)},'reviewed_v8_loader':policy['v8_loader'],'reviewed_R6_controls20':policy['runtime_helpers'],'prerequisite39':m['prerequisite39_manifest'],'lint':m['lint_manifest'],'binary':m['binary'],'source_before':m['source_before'],'native_config':m['native_config'],'native_inputs':m['native_inputs'],'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'resource_policy':m['resource_policy'],'builds':[],'runs':[],'old_model_results_reused':False,'scope':policy['scope']};h['write_new'](record,rec)
 def save():record.write_text(json.dumps(rec,indent=2)+'\n')
 allok=True
 try:
  for case in m['cases']:
   need(stable() and binary_stable(),'Source/native/binary drift before case')
   own_env=env|case['environment'];base=[m['binary']['path'],case['name'],'--ignored','--exact'];li=h['command'](out/(args.stem+'-'+case['slug']+'-list.log'),base+['--list'],own_env,timeout=60,rss=8*(1<<30));rec['active_case']=case['slug'];rec['last_command']=li;save();closed(li,h)
   li['listed_test_names']=h['listed'](Path(li['log']).read_text(errors='replace'));li['exact_one_identity']=li['exit_code']==0 and li['stop_reason'] is None and li['listed_test_names']==[case['name']]
   need(stable() and binary_stable(),'Source/native/binary drift after list')
   need(li['stop_reason'] not in ['disk_before_start','disk_below_512MiB','descendant_leak','cancelled'],'Fatal selection resource/group stop')
   if not li['exact_one_identity']:
    rec['runs'].append({'model':case['slug'],'identity_list':li,'executed':False,'success':False,'error':'nonzero exact ignored identity absent'});allok=False;save();continue
   t=h['command'](out/(args.stem+'-'+case['slug']+'.log'),base+['--nocapture','--test-threads=1'],own_env,timeout=900,rss=8*(1<<30));rec['last_command']=t;save();closed(t,h);raw=Path(t['log']).read_text(errors='replace');summaries=h['summaries'](raw);structured=None;error=None
   try:structured=parse_report(raw,case)
   except RuntimeError as e:error=str(e)
   t.update({'model':case['slug'],'identity_list':li,'actual_environment':m['fixed_environment']|case['environment'],'summaries':summaries,'structured_ordinary_report':structured,'report_error':error,'inputs_stable':stable(),'binary_stable':binary_stable(),'source_enforced_matches_not_independent_arithmetic':True})
   unsupported=[x for x in m['required_supported_source_profile']['unsupported_markers'] if x in raw];t['unsupported_markers']=unsupported;t['success']=t['exit_code']==0 and t['stop_reason'] is None and summaries==[{'status':'ok','passed':1,'failed':0,'ignored':0}] and structured is not None and not unsupported and t['inputs_stable'] and t['binary_stable'];rec['runs'].append(t);allok &=t['success'];save()
   need(t['stop_reason'] not in ['disk_before_start','disk_below_512MiB','descendant_leak','cancelled'],'Fatal own-group/disk/cancellation issue')
   need(t['inputs_stable'] and t['binary_stable'],'Source/native/binary drift; no other dispatch')
  rec['status']='ACTUAL_ORDINARY3_LOCAL_PASS_COSMETIC_LINT_PENDING' if allok else 'COMPLETE_WITH_MODEL_REFUSALS_COSMETIC_LINT_PENDING';rec['models_collected']=len(rec['runs']);rec['models_passed']=sum(bool(v['success']) for v in rec['runs']);rec['finished_utc']=h['utc']();save();record.chmod(0o444);return 0 if allok else 1
 except BaseException as e:
  rec['status']='INCOMPLETE';rec['stopped_reason']=repr(e);rec['interrupted_or_failed_command']=getattr(e,'command_result',None);rec['finished_utc']=h['utc']();save();record.chmod(0o444);return 1
if __name__=='__main__':raise SystemExit(main())
