from pathlib import Path
import hashlib,json,os,re,shutil,signal,subprocess,time
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
out=root/'handoff/positive-court/retained-output-20261102'
out.mkdir(exist_ok=True)
old=json.loads((root/'handoff/positive-court/remaining-capacity-20261102/source-court-integration-functional-r3.json').read_text())
env=os.environ.copy(); env.update(old['environment_overrides']); env.update(old['functional_build_overrides'])
env.pop('RUST_MIN_STACK',None);env.pop('RUSTFLAGS',None)
base=old['command'][:old['command'].index('--')]
base[base.index('positive_court_a4_integration')]='positive_wide_material_v2'
base[1]='clippy'
command=base+['--no-deps','--','-D','warnings']
paths=sorted(p for p in subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root,text=True).split('\0') if p and Path(p).suffix in ('.rs','.toml','.lock') and not any(part in Path(p).parts for part in ('evidence','handoff','validation','history')))
def snapshot():return {p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in paths}
before=snapshot(); log=out/'retained-output-clippy-first.log';stop=None;start=time.monotonic();minimum=shutil.disk_usage(root).free
assert minimum>550*1024**2
with log.open('xb') as stream:
 p=subprocess.Popen(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
 print('LIVE',p.pid,flush=True)
 try:
  while p.poll() is None:
   minimum=min(minimum,shutil.disk_usage(root).free)
   if minimum<512*1024**2 or time.monotonic()-start>900:
    stop='disk floor' if minimum<512*1024**2 else 'timeout';os.killpg(p.pid,signal.SIGTERM)
    try:p.wait(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
    break
   time.sleep(.25)
 except BaseException:
  if p.poll() is None:os.killpg(p.pid,signal.SIGKILL);p.wait()
  raise
text=log.read_text();counts=[tuple(map(int,v)) for v in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored',text)]
r={'command':command,'environment':{k:env[k] for k in old['environment_overrides']|old['functional_build_overrides']},'RUST_MIN_STACK':None,'RUSTFLAGS':None,'exit_code':p.returncode,'stop_reason':stop,'elapsed_seconds':time.monotonic()-start,'minimum_free_bytes':minimum,'source_before':before,'source_after':snapshot(),'test_counts':counts,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
for line in text.splitlines():
 if line.startswith('{'):
  try:e=json.loads(line)
  except json.JSONDecodeError:continue
  if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2':
   r['profile']=e['profile'];f=e.get('executable');r['executable']=f
(out/'retained-output-clippy-first.json').write_text(json.dumps(r,indent=2)+'\n')
print('EXIT',p.returncode,stop,counts,flush=True)
for line in text.splitlines():
 if not line.startswith('{'):print(line)
 elif 'compiler-message' in line:
  e=json.loads(line)
  if e['message']['level']=='error': print(e['message'].get('rendered',e['message']['message']))
assert p.returncode==0 and stop is None and counts==[] and before==r['source_after']
