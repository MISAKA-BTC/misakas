#!/usr/bin/env python3
"""Clippy4 selected8target checkmetadata only. Root current lint-only source pin and separateGO required; no test/model execution."""
from pathlib import Path
import argparse,ast,hashlib,json,os,re,runpy,shutil,subprocess,sys
POLICY=Path('/tmp/positive-a0-7a0-plain-Clippy4-policy-v1.json')
POLICY_SHA='d053513b3142b76c876de8b138e9ade9f87f3ad436ed2d84ec76398891080617'
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
 return isinstance(m.get('runtime_artifacts'),dict) and bool(m['runtime_artifacts']) and all(Path(a['path']).is_file() and Path(a['path']).stat().st_size==a['bytes'] and H(a['path'])==a['sha256'] for a in m['runtime_artifacts'].values())
def stable(m,h):return h['stable'](m) and binaries_stable(m)
def verify_lint_source(m,policy,h):
 need(m.get('status')=='ROOT_EXPLICIT_PLAIN_CLIPPY4_PINNED','Explicit lint-only pin required')
 for k in ['authorized_source','authorized_tree','workspace','target_dir','branch','source_roots','native_config','native_inputs','fixed_environment','commands','required_supported_source_profile','resource_policy','raw_parse_limits','runtime_artifacts','baseline_provenance_only','approved_style_delta','source_before']:
  need(m.get(k)==policy[k],'Plain lint contract changed: '+k)
 need(m['execution_source']==policy['authorized_source'] and m['execution_tree']==policy['authorized_tree'],'Root exact source required')
 root=Path(m['workspace']).resolve();h['head'].__globals__['ROOT']=root;h['head'].__globals__['TARGET']=Path(m['target_dir']).resolve()
 need(stable(m,h),'Current lint source/static/protected binaries drift')
 need(subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()==m['execution_tree'],'Tree drift')
 need(subprocess.check_output(['git','branch','--show-current'],cwd=root,text=True).strip()==m['branch'],'Branch drift')
 need(not subprocess.check_output(['git','diff','--name-only','HEAD','--'],cwd=root,text=True).strip(),'Tracked dirty source')
 tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0'));need(set(m['source_before'])<=tracked,'Eligible untracked source')
 provenance=m['baseline_provenance_only'];previous=load_pin(provenance['snapshot']);delta=m['approved_style_delta'];before=previous['source_before_style']
 need(set(before)==set(m['source_before']) and sorted(p for p in before if before[p]!=m['source_before'][p])==delta['exact_paths'],'Only exact two approved style paths may change')
 need(delta['exact_paths']==['misaka/palw/tir/exec/src/node/owned_artifact_params_v2/forward/metadata.rs','testing/positive-court-a7/tests/ordinary_trained_forward_guarded_v2.rs'] and [v['path'] for v in delta['changes']]==delta['exact_paths'],'Exact two style paths required')
 for change in delta['changes']:
  p=change['path'];need(before[p]==change['before_sha256'] and m['source_before'][p]==change['after_sha256'],'Style path hashes differ')
  need(H(change['patch']['path'])==change['patch']['sha256'],'Style patch drift')
  need(subprocess.check_output(['git','diff',delta['from_source'],delta['to_source'],'--',p],cwd=root,text=True)==Path(change['patch']['path']).read_text(),'Exact style file delta differs')
 for key in ['ordinary39','functional3']:load_pin(provenance[key])
 return provenance

def lint_result(step,selected,m):
 events=events_from(step['log'],m['raw_parse_limits']);profiles=[];ok=True
 for target in selected['targets']:
  matches=[v for v in events if v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')==target['name'] and target['kind'] in v.get('target',{}).get('kind',[]) and selected['package'] in str(v.get('package_id',''))]
  need(len(matches)<=m['raw_parse_limits']['max_selected_artifacts'],'Too manyselectedartifacts');valid=bool(matches) and all(v['profile'].get('debuginfo')==target['required_debug'] and (target['required_debug']!=0 or v['profile'].get('opt_level')=='0') for v in matches);ok &= valid;profiles.append({'target':target,'artifacts':matches,'profile_verified':valid})
 messages=[v for v in events if v.get('reason')=='compiler-message'];need(len(messages)<=m['raw_parse_limits']['max_compiler_messages'],'Too manymessages');bad=[v for v in messages if selected['package'] in str(v.get('package_id','')) and v.get('message',{}).get('level') in ['warning','error']]
 cfg=[{'cfgs':v.get('cfgs',[]),'env':dict(v.get('env',[]))} for v in events if v.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(v.get('package_id',''))];p=m['required_supported_source_profile'];observed=bool(cfg) and all(p['cfg'] in v['cfgs'] and v['env'].get(p['environment_name'])==p['environment_value'] for v in cfg)
 return {'success':step['exit_code']==0 and step['stop_reason'] is None and [v.get('success') for v in events if v.get('reason')=='build-finished']==[True] and ok and not bad,'selected_target_profiles':profiles,'selected_warning_error_JSON_count':len(bad),'all_message_levels':[{'package_id':v.get('package_id'),'level':v.get('message',{}).get('level'),'message':v.get('message',{}).get('message')} for v in messages],'observed_Core_source_profiles':cfg,'Source_supported_under_this_Clippy_frontend':observed,'Source_observation_only_never_runtime_certification':True,'runtime_tests_executed':0}
def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--plan',type=Path);ap.add_argument('--plan-sha256');ap.add_argument('--run',action='store_true');ap.add_argument('--root-go',action='store_true');ap.add_argument('--stem');a=ap.parse_args();need(H(POLICY)==POLICY_SHA,'ImmutableClippy3policy changed');policy=json.loads(POLICY.read_text())
 if not a.run:print(json.dumps({'status':'PREPARED_NOT_EXECUTED','commands':policy['commands'],'initial_lint_only_bytes':1073741824,'execution_source':None}));return 0
 need(a.root_go and a.plan and re.fullmatch('[0-9a-f]{64}',a.plan_sha256 or '') and H(a.plan)==a.plan_sha256,'Separate Root exactplanreview andGO required')
 m=json.loads(a.plan.read_text());h=controls(policy);prereq=verify_lint_source(m,policy,h)
 need(sys.platform=='darwin' and shutil.disk_usage(m['target_dir']).free>=1073741824,'Clippy4-only initial1GiBfloor unmet')
 need(re.fullmatch('[A-Za-z0-9_-]+',a.stem or ''),'Unique explicitstem required');out=Path(m['workspace'])/'docs/evidence/positive-court-a0-2026-10-31/integration';manifest=out/(a.stem+'.json');need(not manifest.exists(),'Evidenceoverwrite/retry forbidden')
 env,removed=h['scrub_inherited_environment'](os.environ)
 for key in ['CLIPPY_ARGS','CLIPPY_CONF_DIR','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER']:
  if key in env:removed[key]=env.pop(key)
 h['validate_fixed_environment'](m,policy,Path(m['target_dir']));env.update(m['fixed_environment']);guard=out/(a.stem+'-forbid-cxx.py')
 with guard.open('x') as stream:stream.write('#!/usr/bin/env python3\nimport sys\nsys.stderr.write("CPPcompile/probe forbidden; pinnednativeconfiguration only\\n")\nsys.exit(95)\n')
 guard.chmod(0o555);env['CXX']=str(guard);guard_sha=H(guard)
 rec={'schema':'misaka.Clippy4.selected-metadata-observation.v1','status':'RUNNING','started_utc':h['utc'](),'source':m['execution_source'],'tree':m['execution_tree'],'plan':{'path':str(a.plan),'sha256':a.plan_sha256},'executor':{'path':str(Path(__file__).resolve()),'sha256':H(__file__)},'policy':{'path':str(POLICY),'sha256':POLICY_SHA},'baseline_provenance_only':prereq,'source_before':m['source_before'],'native_config':m['native_config'],'native_inputs':m['native_inputs'],'resource_policy':m['resource_policy'],'raw_parse_limits':m['raw_parse_limits'],'CXXguard_sha256':guard_sha,'environment_overrides':m['fixed_environment']|{'CXX':str(guard)},'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'lint_commands':[],'runtime_executed_by_lint_executor':0,'runtime_test_count_from_this_executor':None}
 h['write_new'](manifest,rec)
 def save():manifest.write_text(json.dumps(rec,ensure_ascii=False,indent=2)+'\n')
 try:
  for selected in m['commands']:
   need(stable(m,h) and H(guard)==guard_sha,'Source/native/protectedtestbinary/CXXguard drift beforelint')
   step=h['command'](out/(a.stem+'-'+selected['id']+'.jsonl'),selected['argv'],env,timeout=1800);rec['lint_commands'].append(step);save();step['selection']=selected;step.update(lint_result(step,selected,m));step['inputs_stable']=stable(m,h) and H(guard)==guard_sha;step['success']=step['success'] and step['inputs_stable'];save();need(step['success'],'Strictlibrarylint failed/partial/drift; no retry or runtimeclaim')
  rec['status']='STRICT_CLIPPY4_SELECTED_METADATA_PASS';rec['selected_lint_surfaces']=8;rec['finished_utc']=h['utc']();rec['source_after']=h['snap'](m);need(rec['source_after']==m['source_before'] and stable(m,h) and H(guard)==guard_sha,'FinalwholeSource/static/binary/CXXguard mismatch');save();manifest.chmod(0o444);return 0
 except BaseException as error:
  rec['status']='INCOMPLETE';rec['stopped_reason']=repr(error);rec['interrupted_or_failed_command']=getattr(error,'command_result',None);rec['finished_utc']=h['utc']();save();manifest.chmod(0o444);return 1
if __name__=='__main__':raise SystemExit(main())
