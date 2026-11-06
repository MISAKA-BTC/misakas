#!/usr/bin/env python3
"""Strict3-library Clippy executor. No test/model target. Root explicit completedCore54 pin and separateGO required."""
from pathlib import Path
import argparse,ast,hashlib,json,os,re,runpy,shutil,subprocess,sys
POLICY=Path('/tmp/positive-a0-50fe-Clippy3-library-policy-v1.json')
POLICY_SHA='50b4af7e97d2a105ccc061b5b4cca43faecf4d0228b84caf190e6bd8f3d2301a'
def H(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1<<20):d.update(b)
 return d.hexdigest()
def need(ok,msg):
 if not ok:raise RuntimeError(msg)
def load_pin(pin):
 need(isinstance(pin,dict) and set(pin)>={'path','sha256'} and re.fullmatch('[0-9a-f]{64}',pin['sha256']),'Explicit immutable pin missing')
 need(H(pin['path'])==pin['sha256'],'Pinned content changed: '+pin['path']);return json.loads(Path(pin['path']).read_text())
def bounded_text(path,limits):
 need(Path(path).stat().st_size<=limits['max_log_bytes'],'Rawlog exceeds boundedmetadata parsecap; raw retained')
 text=Path(path).read_text(errors='replace');need(all(len(line.encode('utf-8'))<=limits['max_line_bytes'] for line in text.splitlines()),'Rawline exceeds boundedmetadata parsecap');return text
def events_from(path,limits):
 events=[]
 for line in bounded_text(path,limits).splitlines():
  try:value=json.loads(line)
  except ValueError:continue
  need(len(events)<limits['max_JSON_events'],'Too manyCargoJSONevents');events.append(value)
 return events
def controls(policy):
 # Reuse exact reviewed20 R6definitions, ASTloader only; never run either importedmain.
 q=policy['helper_loader'];need(H(q['path'])==q['sha256'],'Helperloader changed');ns=runpy.run_path(q['path'],run_name='CLIPPY_HELPER_IMPORT_ONLY');return ns['helpers'](policy)
def closed(c,h):
 need(c.get('executed') is True and c.get('no_live_before_raw_hash') is True,'Runtime command not closed')
 need(H(c['log'])==c['sha256'],'Completed runtime raw changed');p=c['cleanup'];need(p['direct_child_reaped'] and not p['live_members'] and not p['zombies'] and not h['group_members'](p['group']),'Owned runtimePG not gone')
def binaries_stable(m):
 return all(Path(a['path']).is_file() and Path(a['path']).stat().st_size==a['bytes'] and H(a['path'])==a['sha256'] for a in [m['core_binary'],m['protected_Node_binary']])
def stable(m,h):return h['stable'](m) and binaries_stable(m)
def verify_core54(m,policy,h):
 need(m.get('status')=='ROOT_EXPLICIT_CLIPPY3_LIBRARY_PINNED','SOURCE_NULL/unreviewedplan cannotlint')
 need(m.get('execution_source')==policy['authorized_source'] and m.get('execution_tree')==policy['authorized_tree'],'ExactRootselectedsource/tree required')
 for k in ['workspace','target_dir','branch','source_roots','native_config','native_inputs','fixed_environment','commands','core54_plan','core54_driver','required_supported_source_profile','resource_policy','raw_parse_limits']:
  need(m.get(k)==policy[k],'Policycontract changed: '+k)
 need(isinstance(m.get('source_before'),dict) and bool(m['source_before']) and isinstance(m.get('core_binary'),dict) and isinstance(m.get('protected_Node_binary'),dict),'Source/binarydata missing')
 root=Path(m['workspace']).resolve();target=Path(m['target_dir']).resolve();need(root.is_dir() and target.is_dir(),'ExistingRootworkspace/target required')
 h['head'].__globals__['ROOT']=root;h['head'].__globals__['TARGET']=target
 need(stable(m,h),'CurrentwholeSource/native/runtimebinary drift')
 need(subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()==m['execution_tree'],'Currenttree changed')
 need(subprocess.check_output(['git','branch','--show-current'],cwd=root,text=True).strip()==m['branch'],'Currentbranch changed')
 need(not subprocess.check_output(['git','diff','--name-only','HEAD','--'],cwd=root,text=True).strip(),'TrackedSource dirty')
 tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'));need(set(m['source_before'])<=tracked,'EligibleuntrackedSource')
 base=load_pin(m['core54_plan']);f=load_pin(m['core54_manifest']);need(H(m['core54_driver']['path'])==m['core54_driver']['sha256'],'Core54driver changed')
 need(f.get('status')=='PASS' and bool(f.get('finished_utc')) and (f.get('small_passed'),f.get('os_main_passed'),f.get('hf_passed'),f.get('hf_cases_collected'))==(54,0,0,0),'ClosedcurrentCore54PASS required; partial/failure/zero rejected')
 need(f['source']==base['execution_source']==m['execution_source'] and base['authorized_tree']==m['execution_tree'],'Core54source/tree mismatch')
 need(f['source_before']==base['source_before']==m['source_before'] and f['native_inputs']==m['native_inputs']==base['native_inputs'] and f['native_config']==m['native_config'],'Core54 source/static/native mismatch')
 need(f['plan']==m['core54_plan'] and f['runner']['sha256']==m['core54_driver']['sha256'] and Path(f['runner']['path']).resolve()==Path(m['core54_driver']['path']).resolve() and base['runner_sha256']==m['core54_driver']['sha256'],'Core54 exactplan/helper linkage')
 need(f['optional_explicit_min_free_start_bytes']==2147483648 and f['disk_stop_bytes']==536870912,'Core54 runtime2GiB/per512policy mustremain unchanged')
 need(f['selected_scopes']==base['specs']==policy['core54_specs'] and len(f['runs'])==14 and len(f['builds'])==1,'Core54 exact14selectors')
 need(f['environment_overrides']==m['fixed_environment']|{'CXX':f['environment_overrides']['CXX']} and f['RUST_MIN_STACK_effective'] is None and f['RUSTFLAGS_effective'] is None and H(f['environment_overrides']['CXX'])==f['guard_sha256'],'Core54 actualenvironment changed')
 b=f['builds'][0];a=m['core_binary'];profile=policy['required_supported_source_profile']
 need(b['success'] and b['exit_code']==0 and b['stop_reason'] is None and b['inputs_stable'] and b['mixed_artifact_profiles_ok'] and b['supported_source_profile_required'] and b['supported_source_profile_verified'],'Core54 build/actualSourcegate notverified')
 need(b['artifacts']=={'kaspa_consensus_core':a} and a['profile']['opt_level']=='0' and a['profile']['debuginfo']==0 and a['profile']['test'],'ActualCoretestartifact/profile required')
 need(Path(a['path']).resolve().is_relative_to(target/'debug/deps') and re.fullmatch('kaspa_consensus_core-[a-f0-9]+',Path(a['path']).name),'ActualCoretestpath changed')
 need(b['command'][:8]==['cargo','test','-p','kaspa-consensus-core','--offline','--locked','--no-run','--message-format=json-render-diagnostics'] and b['command'][-1]=='--lib','Core54 actual librarytest provenance')
 closed(b,h);events=events_from(b['log'],policy['raw_parse_limits'])
 need([v.get('success') for v in events if v.get('reason')=='build-finished']==[True],'ActualCore raw buildfinished missing')
 need(any(v.get('reason')=='compiler-artifact' and v.get('executable')==a['path'] and v.get('profile')==a['profile'] for v in events),'ActualCore raw compilerartifact missing')
 gates=[v for v in events if v.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(v.get('package_id',''))]
 need(gates and all(profile['cfg'] in v.get('cfgs',[]) and dict(v.get('env',[])).get(profile['environment_name'])==profile['environment_value'] for v in gates),'Actualruntime supportedSourcecfg/env absent')
 names=[];rawpins=[];nested=0
 for t,s in zip(f['runs'],policy['core54_specs']):
  need(t['scope']==s and t['success'] and t['exit_code']==0 and t['stop_reason'] is None and t['inputs_stable'] and t['binary_stable'] and t['artifact']==a,'Failed/drifted/wrongCore54scope')
  li=t['identity_list'];closed(li,h);closed(t,h);need(li['exit_code']==0 and li['stop_reason'] is None and li['exact_identity'] and sorted(h['listed'](bounded_text(li['log'],policy['raw_parse_limits'])))==sorted(s['names']+s['ignored']),'ActualCore exactnonzero list missing')
  basecmd=[a['path'],s['filter']]+(['--exact'] if s['exact'] else []);need(li['command']==basecmd+['--list'] and t['command']==basecmd+(['--nocapture'] if s.get('nocapture') else [])+['--test-threads=1'],'ActualCore runtimeargs changed')
  raw=bounded_text(t['log'],policy['raw_parse_limits']);sums=h['summaries'](raw);outer=sums[-1:] if s.get('nocapture') else sums;need(outer==t['outer_summaries']==[{'status':'ok','passed':len(s['names']),'failed':0,'ignored':len(s['ignored'])}],'Core outerzero/count/failure')
  if not s.get('nocapture'):need(all(re.search(r'^test '+re.escape(n)+r' \.\.\. ok$',raw,re.M) for n in s['names']),'NamedCoretest missing')
  need(not any(x in raw for x in profile['unsupported_markers']),'RuntimeSourceunsupported cannotqualify')
  names+=s['names'];nested+=sum(x['passed'] for x in sums[:-1]) if s.get('nocapture') else 0;rawpins +=[{'path':t['log'],'sha256':t['sha256']},{'path':li['log'],'sha256':li['sha256']}]
 need(len(names)==len(set(names))==54,'Core54 distinctcardinality changed')
 # Native22 is a prior qualified prerequisite only; no test or lint count reuse.
 native=load_pin(policy['required_Native22_audit']);need(native['status']=='SAME_SOURCE_NATIVE22_SCOPED_PASS' and native['source']==m['execution_source'] and native['tree']==m['execution_tree'] and native['actualbinary']['sha256']==m['protected_Node_binary']['sha256'],'Native22 protection/provenance mismatch')
 need(stable(m,h),'After-prerequisite input drift')
 return {'manifest':m['core54_manifest'],'outer_tests':54,'nested_child_tests_not_added':nested,'rawpins':rawpins,'Native22':policy['required_Native22_audit'],'no_lint_or_extra_runtime_qualification':True}
def lint_result(step,selected,m):
 events=events_from(step['log'],m['raw_parse_limits'])
 pkg={'core_library':'kaspa-consensus-core','exec_positive_panel_library':'misaka-palw-tir-exec','sdk_library':'misaka-palw-sdk'}[selected['id']];target=pkg.replace('-','_');debug=0 if pkg=='kaspa-consensus-core' else 2
 arts=[v for v in events if v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')==target and 'lib' in v.get('target',{}).get('kind',[]) and pkg in str(v.get('package_id',''))]
 need(len(arts)<=m['raw_parse_limits']['max_selected_artifacts'],'Too manyselectedCargoartifacts')
 profiles_ok=bool(arts) and all(v['profile'].get('debuginfo')==debug and not v['profile'].get('test') and (debug!=0 or v['profile'].get('opt_level')=='0') and v.get('executable') is None for v in arts)
 messages=[v for v in events if v.get('reason')=='compiler-message'];need(len(messages)<=m['raw_parse_limits']['max_compiler_messages'],'Too manycompilermessages');selectedbad=[v for v in messages if pkg in str(v.get('package_id','')) and v.get('message',{}).get('level') in ['warning','error']]
 cfg=[{'cfgs':v.get('cfgs',[]),'env':dict(v.get('env',[]))} for v in events if v.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(v.get('package_id',''))]
 profile=m['required_supported_source_profile'];observed=bool(cfg) and all(profile['cfg'] in v['cfgs'] and v['env'].get(profile['environment_name'])==profile['environment_value'] for v in cfg)
 return {'success':step['exit_code']==0 and step['stop_reason'] is None and [v.get('success') for v in events if v.get('reason')=='build-finished']==[True] and profiles_ok and not selectedbad,'selected_artifacts':arts,'profiles_verified':profiles_ok,'all_message_levels':[{'package_id':v.get('package_id'),'level':v.get('message',{}).get('level'),'code':v.get('message',{}).get('code'),'message':v.get('message',{}).get('message')} for v in messages],'selected_warning_error_count':len(selectedbad),'observed_Core_source_profiles':cfg,'Source_supported_under_this_Clippy_frontend':observed,'Source_observation_only_never_runtime_certification':True,'runtime_tests_executed':0}
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--plan',type=Path);ap.add_argument('--plan-sha256');ap.add_argument('--run',action='store_true');ap.add_argument('--root-go',action='store_true');ap.add_argument('--stem');a=ap.parse_args();need(H(POLICY)==POLICY_SHA,'ImmutableClippy3policy changed');policy=json.loads(POLICY.read_text())
 if not a.run:print(json.dumps({'status':'PREPARED_NOT_EXECUTED','commands':policy['commands'],'initial_lint_only_bytes':1073741824,'execution_source':None}));return 0
 need(a.root_go and a.plan and re.fullmatch('[0-9a-f]{64}',a.plan_sha256 or '') and H(a.plan)==a.plan_sha256,'Separate Root exactplanreview andGO required')
 m=json.loads(a.plan.read_text());h=controls(policy);prereq=verify_core54(m,policy,h)
 need(sys.platform=='darwin' and shutil.disk_usage(m['target_dir']).free>=1073741824,'Clippy3-only initial1GiBfloor unmet')
 need(re.fullmatch('[A-Za-z0-9_-]+',a.stem or ''),'Unique explicitstem required');out=Path(m['workspace'])/'docs/evidence/positive-court-a0-2026-10-31/integration';manifest=out/(a.stem+'.json');need(not manifest.exists(),'Evidenceoverwrite/retry forbidden')
 env,removed=h['scrub_inherited_environment'](os.environ)
 for key in ['CLIPPY_ARGS','CLIPPY_CONF_DIR','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER']:
  if key in env:removed[key]=env.pop(key)
 h['validate_fixed_environment'](m,policy,Path(m['target_dir']));env.update(m['fixed_environment']);guard=out/(a.stem+'-forbid-cxx.py')
 with guard.open('x') as stream:stream.write('#!/usr/bin/env python3\nimport sys\nsys.stderr.write("CPPcompile/probe forbidden; pinnednativeconfiguration only\\n")\nsys.exit(95)\n')
 guard.chmod(0o555);env['CXX']=str(guard);guard_sha=H(guard)
 rec={'schema':'misaka.strictClippy3.library-only-observation.v1','status':'RUNNING','started_utc':h['utc'](),'source':m['execution_source'],'tree':m['execution_tree'],'plan':{'path':str(a.plan),'sha256':a.plan_sha256},'executor':{'path':str(Path(__file__).resolve()),'sha256':H(__file__)},'policy':{'path':str(POLICY),'sha256':POLICY_SHA},'core54_prerequisite':prereq,'source_before':m['source_before'],'native_config':m['native_config'],'native_inputs':m['native_inputs'],'resource_policy':m['resource_policy'],'raw_parse_limits':m['raw_parse_limits'],'CXXguard_sha256':guard_sha,'environment_overrides':m['fixed_environment']|{'CXX':str(guard)},'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'lint_commands':[],'runtime_executed_by_lint_executor':0,'runtime_test_count_from_this_executor':None}
 h['write_new'](manifest,rec)
 def save():manifest.write_text(json.dumps(rec,ensure_ascii=False,indent=2)+'\n')
 try:
  for selected in m['commands']:
   need(stable(m,h) and H(guard)==guard_sha,'Source/native/protectedtestbinary/CXXguard drift beforelint')
   step=h['command'](out/(a.stem+'-'+selected['id']+'.jsonl'),selected['argv'],env,timeout=1800);rec['lint_commands'].append(step);save();step['selection']=selected;step.update(lint_result(step,selected,m));step['inputs_stable']=stable(m,h) and H(guard)==guard_sha;step['success']=step['success'] and step['inputs_stable'];save();need(step['success'],'Strictlibrarylint failed/partial/drift; no retry or runtimeclaim')
  rec['status']='STRICT_CLIPPY3_LIBRARY_PASS';rec['selected_lint_surfaces']=3;rec['finished_utc']=h['utc']();rec['source_after']=h['snap'](m);need(rec['source_after']==m['source_before'] and stable(m,h) and H(guard)==guard_sha,'FinalwholeSource/static/binary/CXXguard mismatch');save();manifest.chmod(0o444);return 0
 except BaseException as error:
  rec['status']='INCOMPLETE';rec['stopped_reason']=repr(error);rec['interrupted_or_failed_command']=getattr(error,'command_result',None);rec['finished_utc']=h['utc']();save();manifest.chmod(0o444);return 1
if __name__=='__main__':raise SystemExit(main())
