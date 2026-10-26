#!/usr/bin/env python3
"""Run inside one shared buildslot, capturing source only after slot acquisition."""
import hashlib,json,os,shutil,subprocess,time
from pathlib import Path

out=Path('/Users/wata/.codex/artifacts/g14-17000-measure')
def git(*args):return subprocess.check_output(['git',*args],text=True).strip()
def code():
 lines=git('ls-tree','-r','HEAD').splitlines()
 return hashlib.sha256('\n'.join(line for line in lines if not line.split('\t',1)[1].startswith('docs/')).encode()).hexdigest()
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1<<20),b''):h.update(b)
 return h.hexdigest()

for name,package,target,selector in [('native-verified','kaspa-consensus','kaspa_consensus',['--lib']),('dolphin','misaka-palw-base0','g14_real_model_measure',['--test','g14_real_model_measure'])]:
 cmd=['cargo','test','--offline','--release','-p',package,*selector,'--no-run','--message-format=json']
 start=time.monotonic();initial=code()
 r={'source_commit':git('rev-parse','HEAD'),'source_trees':initial,'command':cmd,'shared_slot':True,
    'stage':'BUILDING','features':'default','minimum_hardware':False,'load_before':os.getloadavg(),
    'hardware':subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.ncpu'],text=True)}
 record=out/f'{name}-build-record.json';record.write_text(json.dumps(r,indent=2)+'\n')
 env=dict(os.environ,CARGO_TARGET_DIR=str(out/'target'),TMPDIR='/private/tmp')
 with (out/f'{name}-build.stdout.log').open('w') as stdout,(out/f'{name}-build.stderr.log').open('w') as stderr:
  child=subprocess.run(cmd,env=env,stdout=stdout,stderr=stderr)
 r.update(exit_code=child.returncode,wall_seconds=time.monotonic()-start,load_after=os.getloadavg(),stage='BUILT' if child.returncode==0 else 'FAILED')
 if child.returncode==0:
  found=[]
  for line in (out/f'{name}-build.stdout.log').read_text().splitlines():
   try:j=json.loads(line)
   except ValueError:continue
   if j.get('reason')=='compiler-artifact' and j.get('target',{}).get('name')==target and j.get('executable'):found.append(j['executable'])
  assert len(found)==1,found
  src=Path(found[0]);dest=out/'pins'/(name+'-'+src.name)
  shutil.copy2(src,dest);r['binary']={'path':str(dest),'sha256':sha(dest)}
  r['source_unchanged']=initial==code() and not git('diff','HEAD','--name-only','--','.',':(exclude)docs/**')
 record.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r),flush=True)
 if child.returncode:raise SystemExit(child.returncode)

# Continue the bounded preflight within the same acquired build slot.
raise SystemExit(subprocess.run(["python3",str(out/"run_smokes.py")]).returncode)
