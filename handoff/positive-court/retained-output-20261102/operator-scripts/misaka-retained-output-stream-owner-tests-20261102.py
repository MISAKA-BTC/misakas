from pathlib import Path
import hashlib,json,os,re,shutil,signal,subprocess,time
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out=root/'handoff/positive-court/retained-output-20261102'
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
 record=run('runtime-stream-owner',base+['--','--nocapture','--test-threads=1'],900,650*1024**2)
 assert record['exit_code']==0 and record['test_counts']==[(19,0,2)]
