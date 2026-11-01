#!/usr/bin/env python3
"""Derivedsame-source39 collector only AFTER RootclosesA7recovery. No tests/builds/models/cleanup."""
from pathlib import Path
import argparse,copy,hashlib,json,re,runpy,datetime
PLAN=Path('/tmp/positive-a0-ordinary39-a867-v1-frozen.json');PLAN_SHA='c135c1404d3180266095e0008bbbd24251dae65da4a18b90852bb2840bf3dd51'
PRIMARY=Path('/Users/wata/.codex/worktrees/rfc42-a6-recovery/MISAKA-testnet/docs/evidence/positive-court-a0-2026-10-31/integration/ordinary39-a867-20261102.json');PRIMARY_SHA='9c6b24131e701954d15db4e0c4d9c7c719b4e2a9ff7829538ce8765edd4c5b8a'
VERIFIER=Path('/tmp/positive-a0-ordinary-trained-runtime-only-driver-v3.py');VERIFIER_SHA='85ba0246a9ceaa163b2685c4294270ba8ecd55391dccbbba75517d8074ab4343'
def H(path):
 h=hashlib.sha256()
 with Path(path).open('rb') as f:
  while b:=f.read(1<<20):h.update(b)
 return h.hexdigest()
def need(ok,msg):
 if not ok:raise RuntimeError(msg)
def pin(path,expected):need(H(path)==expected,'Pinnedcontent mismatch');return json.loads(Path(path).read_text())
def main():
 a=argparse.ArgumentParser(description=__doc__);a.add_argument('--root-recovery-closed',action='store_true');a.add_argument('--recovery-manifest',type=Path);a.add_argument('--recovery-sha256');a.add_argument('--write-composed',type=Path);x=a.parse_args()
 need(x.root_recovery_closed and x.recovery_manifest and re.fullmatch('[0-9a-f]{64}',x.recovery_sha256 or ''),'Root CLOSEDnotification+explicitfullrecoverypin required; no activeinputread')
 need(x.write_composed and x.write_composed.is_absolute() and x.write_composed.resolve().is_relative_to(Path('/tmp').resolve()) and not x.write_composed.exists(),'New/tmp output only; neveroverwriteoriginals')
 base=pin(PLAN,PLAN_SHA);p=pin(PRIMARY,PRIMARY_SHA);r=pin(x.recovery_manifest,x.recovery_sha256);need(p['status']=='INCOMPLETE' and r['status']=='PASS' and p['finished_utc'] and r['finished_utc'],'Bothphysicalrecords mustbeclosed originalstatuses')
 need(H(VERIFIER)==VERIFIER_SHA,'Reviewedruntimev3validator changed');api=runpy.run_path(str(VERIFIER),run_name='COMPOSED39_COLLECTOR_VERIFY_ONLY')
 loader=runpy.run_path('/tmp/positive-a0-node-runtime-only-source-null-driver-v8.py',run_name='R6_HELPER_LOADER_ONLY');policy={'runtime_helpers':{'path':'/tmp/positive-a0-next-validation-a6-mixed-v4-r6-runner.py','sha256':'ab0286b47ae577fce0396799504cdeb8c7c6e7dec6a155ba37b0cdf2de09f036','excluded_definitions':['main','group_commands'],'included_definitions':20}};h=loader['helpers'](policy);h['head'].__globals__['ROOT']=Path(base['workspace']);h['head'].__globals__['TARGET']=Path(base['target_dir'])
 def closed(c,helper):
  need(c.get('executed') and c.get('no_live_before_raw_hash') and H(c['log'])==c['sha256'],'Unclosed/changedphysicalraw');q=c['cleanup'];need(q['direct_child_reaped'] and not q['live_members'] and not q['zombies'] and not helper['group_members'](q['group']),'OwnedphysicalPG stilllive')
 bo=[{'manifest':'primary','index':0},{'manifest':'primary','index':1},{'manifest':'a7_recovery','index':0}];ro=[{'manifest':'primary','index':i} for i in range(12)]+[{'manifest':'a7_recovery','index':0}];parts={'primary':p,'a7_recovery':r}
 builds=[copy.deepcopy(parts[o['manifest']]['builds'][o['index']]) for o in bo];runs=[copy.deepcopy(parts[o['manifest']]['runs'][o['index']]) for o in ro]
 envkeys=['environment_overrides','guard_sha256','RUST_MIN_STACK_effective','RUSTFLAGS_effective'];f={'schema':'misaka.same-source.ordinary39.a7-recovery-composition.v1','status':'SAME_SOURCE_COMPOSED_39_PASS',**{k:copy.deepcopy(p[k]) for k in ['source','source_before','native_config','native_inputs','plan','runner']},'selected_scopes':copy.deepcopy(base['specs']),'builds':builds,'runs':runs,'small_passed':39,'os_main_passed':0,'hf_passed':0,'hf_cases_collected':0,'finished_utc':r['finished_utc'],'derived_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'composition':{'kind':'A7_ONLY_RECOVERY_AFTER_DISK_STOP_V1','physical_manifests':{'primary':{'path':str(PRIMARY),'sha256':PRIMARY_SHA},'a7_recovery':{'path':str(x.recovery_manifest),'sha256':x.recovery_sha256}},'physical_original_status':{'primary':'INCOMPLETE','a7_recovery':'PASS'},'physical_environments':{k:{n:copy.deepcopy(v[n]) for n in envkeys} for k,v in parts.items()},'allowed_primary_failure':{'build_index':2,'group':'a7','stop_reason':'disk_below_512MiB','exit_code':-15,'cleanup_group':p['builds'][2]['cleanup']['group']},'success_counts':{'primary':34,'a7_recovery':5},'build_origins':bo,'run_origins':ro,'original_manifests_unchanged':True},'time_semantics':'finishedUTC isrecoveredcohortavailability, derivedUTC iscollector time; thisisnotonephysicalrun/environment','scope':'39purelocal regressions only; 3traineddeclarations ignored, no model/Node/native/P/fullcontext inference qualification.'}
 api['composed39'](f,base,base,h,closed,{'composed39_schema':f['schema'],'composed39_kind':f['composition']['kind']})
 need(p['plan']=={'path':str(PLAN),'sha256':PLAN_SHA} and p['source']==base['execution_source'] and p['source_before']==base['source_before'] and p['native_inputs']==base['native_inputs'] and h['stable'](base),'CurrentfrozenSource/static mismatch')
 for part in parts.values():need(part['optional_explicit_min_free_start_bytes']==2147483648 and part['disk_stop_bytes']==536870912,'Physicalruntimepolicy changed')
 need(r['small_passed']==5 and r['os_main_passed']==r['hf_passed']==r['hf_cases_collected']==0,'OnlyA7pure5 recovered')
 profile=base['required_supported_source_profile'];artifacts={}
 for b,wanted in zip(builds,base['builds']):
  closed(b,h);need(b['command']==wanted['argv'] and b['success'] and b['exit_code']==0 and b['stop_reason'] is None and b['inputs_stable'] and b['supported_source_profile_required'] and b['supported_source_profile_verified'],'Originalsuccessbuild invalid')
  events=[]
  for line in Path(b['log']).read_text(errors='replace').splitlines():
   try:events.append(json.loads(line))
   except ValueError:pass
  need([e.get('success') for e in events if e.get('reason')=='build-finished']==[True],'Rawbuildfinish missing');gates=[e for e in events if e.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(e.get('package_id',''))];need(gates and all(profile['cfg'] in e.get('cfgs',[]) and dict(e.get('env',[])).get(profile['environment_name'])==profile['environment_value'] for e in gates),'ActualruntimeSourcegate missing')
  need(set(b['artifacts'])==set(wanted['expected_test_artifacts']),'Actualcompiledartifactset mismatch')
  for key,v in b['artifacts'].items():
   need(H(v['path'])==v['sha256'] and Path(v['path']).stat().st_size==v['bytes'] and v['profile']['debuginfo']==(0 if wanted['group']=='core' else 2),'Currentartifactidentity/profile changed');need(any(e.get('reason')=='compiler-artifact' and e.get('executable')==v['path'] and e.get('profile')==v['profile'] for e in events),'Rawactualartifact missing');artifacts[key]=v
 names=[]
 for t,s in zip(runs,base['specs']):
  closed(t,h);li=t['identity_list'];closed(li,h);need(t['scope']==s and t['success'] and t['exit_code']==0 and t['stop_reason'] is None and t['inputs_stable'] and t['binary_stable'] and t['artifact']==artifacts[s['key']],'Originalsuccesscase invalid')
  need(li['exit_code']==0 and li['exact_identity'] and sorted(h['listed'](Path(li['log']).read_text()))==sorted(s['names']+s['ignored']),'Actualexactnonzero identity mismatch')
  cmd=[artifacts[s['key']]['path']]+([s['filter']] if s['filter'] else [])+(['--exact'] if s['exact'] else []);need(li['command']==cmd+['--list'] and t['command']==cmd+['--test-threads=1'],'Actualpurecaseargs mismatch')
  raw=Path(t['log']).read_text(errors='replace');need(h['summaries'](raw)==t['outer_summaries']==[{'status':'ok','passed':len(s['names']),'failed':0,'ignored':len(s['ignored'])}],'Rawouter counts mismatch');need(all(re.search(r'^test '+re.escape(n)+r' \.\.\. ok$',raw,re.M) for n in s['names']) and not any(v in raw for v in profile['unsupported_markers']),'Actualnamedcase/Sourceprofile missing');names +=[(s['key'],n) for n in s['names']]
 need(len(names)==len(set(names))==39 and len(artifacts)==7 and h['stable'](base),'Final39/source cardinality mismatch')
 need(H(PRIMARY)==PRIMARY_SHA and H(x.recovery_manifest)==x.recovery_sha256,'Physicaloriginal changed')
 with x.write_composed.open('x') as out:json.dump(f,out,ensure_ascii=False,indent=2);out.write('\n')
 x.write_composed.chmod(0o444);print(json.dumps({'status':f['status'],'path':str(x.write_composed),'sha256':H(x.write_composed),'physical_statuses_unchanged':f['composition']['physical_original_status'],'logical39':39,'actualcommands_bycollector':0}));return 0
if __name__=='__main__':raise SystemExit(main())
