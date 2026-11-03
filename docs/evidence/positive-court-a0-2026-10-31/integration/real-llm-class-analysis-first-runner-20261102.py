from pathlib import Path
import sys,os,json,hashlib,subprocess,time,shutil
root=Path.cwd();sys.path.insert(0,str(root/'testing/positive-court-a7'))
from process_control import cancellation_signals,stop_group,active,members,ProcessInterrupted
from real_llm_process import parse_darwin_time
base=root/'docs/evidence/positive-court-a0-2026-10-31/integration';out=base/'real-llm-class-analysis-first-run-20261102';out.mkdir(exist_ok=False)
def h(p):
 a=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1048576):a.update(b)
 return a.hexdigest()
build_path=base/'real-llm-class-analysis-first-build-20261102.json';build=json.loads(build_path.read_text());assert build['build_success'];art=build['binary'][0];assert h(art['path'])==art['sha256']
source=build['source_after'];snapshot=lambda:{p:h(p) for p in source};assert snapshot()==source
old=json.loads((base/'real-llm-run-8gib-20261101/manifest.json').read_text());models={m['slug']:m for m in old['models']}
env=os.environ.copy()
for k in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']:env.pop(k,None)
env['RAYON_NUM_THREADS']='2'
d={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'build_record':{'path':str(build_path),'sha256':h(build_path)},'binary':art,'source_before':source,'status':'RUNNING','scope':'CLASS_ONLY_DATA_ANALYSIS; original trained-artifact declarations; no CPU model execution, native registration, full admission, roles/P/funds/credit qualification','diagnostic_leaf_cap':str(2**64-1),'diagnostic_court':'both','outer_process_policy':{'timeout_seconds_per_model':180,'polled_process_group_rss_stop_bytes':2*(1<<30),'rss_poll_seconds':0.2,'rss_bound_guaranteed':False,'disk_stop_bytes':512*(1<<20),'RUST_MIN_STACK':None,'cold_claimed':False},'results':[]}
manifest=out/'manifest.json';manifest.write_text(json.dumps(d,indent=2)+'\n')
for slug in ['pythia-70m','smollm2-135m','smollm2-360m']:
 m=models[slug];case=out/slug;case.mkdir();assert h(art['path'])==art['sha256'];assert snapshot()==source
 report=case/'analysis.json';cmd=[art['path'],'--model',m['model'],'--artifact',m['artifact'],'--fixture',m['fixture'],'--out',str(report),'--diagnostic-leaf-cap',str(2**64-1),'--diagnostic-court','both'];timed=['/usr/bin/time','-l','-o',str(case/'time-l.log')]+cmd
 item={'model':m['model'],'slug':slug,'revision':m['revision'],'original_max_context':m['max_context'],'command':cmd,'timed_command':timed,'started_unix':time.time(),'status':'RUNNING'};d['results'].append(item);manifest.write_text(json.dumps(d,indent=2)+'\n');start=time.monotonic();peak=0
 with cancellation_signals() as cancel,(case/'stdout.log').open('xb') as stdout,(case/'stderr.log').open('xb') as stderr:
  process=subprocess.Popen(timed,cwd=root,env=env,stdout=stdout,stderr=stderr,start_new_session=True);cleanup=None;reason=None
  try:
   while process.poll() is None:
    if cancel['signal'] is not None:reason='CANCELLED'
    elif time.monotonic()-start>=180:reason='TIMEOUT'
    elif shutil.disk_usage(root).free<512*(1<<20):reason='DISK_STOP'
    rows=subprocess.check_output(['ps','-A','-o','pid=,pgid=,rss='],text=True).splitlines();rss=sum(int(fields[2])*1024 for row in rows if len(fields:=row.split())==3 and int(fields[1])==process.pid);peak=max(peak,rss)
    if rss>2*(1<<30):reason='POLLED_RSS_STOP'
    if reason:cleanup=stop_group(process);break
    time.sleep(0.2)
   process.wait()
   if cleanup is None:
    if active(process.pid):reason='DESCENDANT_LEAK';cleanup=stop_group(process)
    else:cleanup={'group':process.pid,'live_descendants':[],'zombies':members(process.pid)}
  except BaseException:
   if cleanup is None:stop_group(process)
   raise
 item.update(exit_code=process.returncode,stop_reason=reason,cleanup=cleanup,elapsed_seconds=time.monotonic()-start,polled_group_peak_rss_bytes=peak,resource_measurement=parse_darwin_time(case/'time-l.log'))
 if report.exists():
  r=json.loads(report.read_text());item['analysis_status']=r['status'];item['phase']=r['phase'];item['error']=r.get('error');item['geometry']=r.get('geometry');item['twin_comparisons']=r.get('twin_comparisons');item['analysis_complete']=process.returncode==0 and r['status']=='CLASS_ONLY_ANALYSIS_COMPLETE';assert not r['cpu_executed'] and not r['native_v2_admission_executed'] and not r['credit_executed']
 else:item['analysis_complete']=False;item['analysis_status']='NO_FINAL_REPORT'
 item['status']='CLASS_DATA_COMPLETE' if item['analysis_complete'] else 'CLASS_DATA_INCOMPLETE';item['raw_files']={str(p):{'bytes':p.stat().st_size,'sha256':h(p)} for p in case.iterdir() if p.is_file()};item['source_and_binary_stable']=snapshot()==source and h(art['path'])==art['sha256'];manifest.write_text(json.dumps(d,indent=2)+'\n');print(json.dumps({k:item.get(k) for k in ['model','status','exit_code','stop_reason','analysis_status','error','elapsed_seconds','geometry','resource_measurement']}),flush=True)
 if not item['source_and_binary_stable'] or reason=='CANCELLED':break
 d['source_after']=snapshot();d['source_stable']=d['source_after']==source
 d['status']='CLASS_DATA_COMPLETE_ALL' if len(d['results'])==3 and all(r['analysis_complete'] for r in d['results']) else 'CLASS_DATA_INCOMPLETE';manifest.write_text(json.dumps(d,indent=2)+'\n')
