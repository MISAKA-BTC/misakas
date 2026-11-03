from pathlib import Path
import hashlib,json,os,re,shutil,signal,subprocess,time
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out=root/'handoff/positive-court/trained-roles-20261102'
old=json.loads((root/'handoff/positive-court/remaining-capacity-20261102/source-court-integration-functional-r3.json').read_text())
env=os.environ.copy();env.update(old['environment_overrides']);env.update(old['functional_build_overrides']);env.pop('RUSTFLAGS',None);env.pop('RUST_MIN_STACK',None);env['RAYON_NUM_THREADS']='1'
paths=sorted(p for p in subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root,text=True).split('\0') if p and Path(p).suffix in ('.rs','.toml','.lock') and not any(s in Path(p).parts for s in ('evidence','handoff','validation','history')))
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as stream:
  while b:=stream.read(256*1024):h.update(b)
 return h.hexdigest()
def snapshot():return {p:sha(root/p) for p in paths}
def run(label,command,timeout,initial_free,rss_cap=None,extra_env=None):
 before=snapshot();log=out/(label+'.log');minimum=shutil.disk_usage(root).free
 assert minimum>=initial_free,('insufficient initial free space',minimum,initial_free)
 start=time.monotonic();stop=None;peak=0
 with log.open('xb') as stream:
  child=subprocess.Popen(command,cwd=root,env=env|dict(extra_env or {}),stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
  print('LIVE',label,child.pid,flush=True)
  try:
   while child.poll() is None:
    minimum=min(minimum,shutil.disk_usage(root).free)
    if rss_cap:
     rows=subprocess.check_output(['ps','-axo','pid,pgid,rss'],text=True).splitlines()[1:]
     peak=max(peak,sum(int(r.split()[2])*1024 for r in rows if r.split()[1]==str(child.pid)))
    if minimum<512*1024**2:stop='disk floor'
    elif time.monotonic()-start>timeout:stop='wall timeout'
    elif rss_cap and peak>rss_cap:stop='sampled process-group RSS limit exceeded; not kernel admission'
    if stop:
     os.killpg(child.pid,signal.SIGTERM)
     try:child.wait(timeout=5)
     except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
     break
    time.sleep(.25)
  except BaseException:
   if child.poll() is None:os.killpg(child.pid,signal.SIGKILL);child.wait()
   raise
 text=log.read_text();counts=[tuple(map(int,row)) for row in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',text)]
 record={'command':command,'effective_environment':{k:env[k] for k in old['environment_overrides']|old['functional_build_overrides']|{'RAYON_NUM_THREADS':None}},'extra_environment':extra_env,'RUSTFLAGS':None,'RUST_MIN_STACK':None,'exit_code':child.returncode,'stop_reason':stop,'elapsed_seconds':time.monotonic()-start,'minimum_free_bytes':minimum,'sampled_pg_rss_peak_bytes':peak if rss_cap else None,'rss_monitor_is_kernel_quota':False,'source_before':before,'source_after':snapshot(),'test_counts':counts,'log_sha256':sha(log),'process_group':child.pid}
 record['remaining_pg_members']=[r.strip() for r in subprocess.check_output(['ps','-axo','pid,pgid,state'],text=True).splitlines()[1:] if r.split()[1]==str(child.pid)]
 for line in text.splitlines():
  if not line.startswith('{'):continue
  try:e=json.loads(line)
  except json.JSONDecodeError:continue
  if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2':
   record['profile']=e['profile'];record['binary']={'path':e['executable'],'sha256':sha(Path(e['executable'])),'bytes':Path(e['executable']).stat().st_size}
 (out/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
 print('EXIT',label,child.returncode,stop,counts,flush=True)
 for line in text.splitlines():
  if not line.startswith('{'):print(line)
  elif 'compiler-message' in line:
   e=json.loads(line)
   if e['message']['level']=='error':print(e['message'].get('rendered',e['message']['message']))
 assert before==record['source_after'] and not record['remaining_pg_members']
 return record
if __name__=='__main__':
 base=old['command'][:old['command'].index('--')]
 base[base.index('positive_court_a4_integration')]='positive_wide_material_v2'
 build=run('build-checkpoint',base+['--no-run'],900,800*1024**2)
 assert build['exit_code']==0 and build['stop_reason'] is None
 binary=build['binary']['path'];profile=build['profile']
 assert profile['opt_level']=='0' and profile['debuginfo']==0 and profile['debug_assertions'] and profile['overflow_checks']
 default=run('default-checkpoint',[binary,'--nocapture','--test-threads=1'],120,650*1024**2,4*1024**3)
 assert default['exit_code']==0 and default['test_counts']==[(16,0,2)]
 pinned={
 '/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir':'2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651',
 '/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/registration-fixture.json':'29c0d221c7f998dee2e4778bd8bfd9f9b8b4e610b17a62b1bf6f4484b7f367a8'}
 for p,s in pinned.items():assert sha(Path(p))==s
 # Independent input SHA reads explicitly warm the OS page cache.
 case=run('pythia-original-short-checkpoint',[binary,'--exact','trained_roles::pythia70m_original_short_job_ordinary_panel_and_complete_node','--ignored','--nocapture','--test-threads=1'],360,832*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
 after={p:sha(Path(p)) for p in pinned}
 (out/'input-sha-checkpoint.json').write_text(json.dumps({'before_expected':pinned,'after':after,'match':pinned==after,'sha_warms_os_cache':True,'native_authority':False,'canonical_job':False},indent=2)+'\n')
 assert pinned==after
 assert case['exit_code']==101 and case['stop_reason'] is None and case['test_counts']==[(0,1,0)]
 assert 'wide Panel builder simultaneous owners cap' in (out/'pythia-original-short-checkpoint.log').read_text()
 quote=run('storage-checkpoint',[binary,'--exact','trained_roles::pythia70m_original_short_job_storage_quote','--ignored','--nocapture','--test-threads=1'],120,650*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
 assert quote['exit_code']==0 and quote['test_counts']==[(1,0,0)]
 print('CHECKPOINT complete: whole numerical flow remains refused, no Node result',flush=True)
