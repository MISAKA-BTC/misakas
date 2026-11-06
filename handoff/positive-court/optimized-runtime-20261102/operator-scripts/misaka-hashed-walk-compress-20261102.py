from pathlib import Path
import fcntl,hashlib,json,os,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/optimized-runtime-20261102';base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps');cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
record=json.loads((out/'hashed-walk-runtime-first.json').read_text());assert record['exit_code']==0
names=set()
for line in (out/'hashed-walk-runtime-first.log').read_text().splitlines():
 if not line.startswith('{'):continue
 e=json.loads(line)
 if e.get('reason')!='compiler-artifact' or not e.get('manifest_path','').startswith(str(root)+'/'):continue
 for f in e.get('filenames',[]):
  p=Path(f)
  if p.suffix in ['.rlib','.rmeta'] and p.parent==base and p.is_file() and p.stat().st_blocks*512>2*1024**2:names.add(p)
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as stream:
  while b:=stream.read(1024**2):h.update(b)
 return h.hexdigest()
rows=[]
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 for p in sorted(names):
  st=p.stat();before=sha(p);tmp=cache/(p.name+'.hashed-walk-hfs-20261102');assert not tmp.exists()
  subprocess.run(['/usr/bin/ditto','--hfsCompression',str(p),str(tmp)],check=True)
  copied=tmp.stat();assert sha(tmp)==before and copied.st_size==st.st_size and copied.st_mtime_ns==st.st_mtime_ns
  if copied.st_blocks<st.st_blocks:os.replace(tmp,p)
  else:tmp.unlink()
  assert sha(p)==before and p.stat().st_mtime_ns==st.st_mtime_ns
  rows.append({'path':str(p),'logical_bytes':st.st_size,'sha256_unchanged':before,'mtime_ns_unchanged':st.st_mtime_ns,'allocated_before':st.st_blocks*512,'allocated_after':p.stat().st_blocks*512,'observed_free_after':shutil.disk_usage(base).free})
  (out/'fresh-library-filesystem-compression-hashed-walk.json').write_text(json.dumps({'scope':'transparent filesystem compression of already-present successful own build artifacts; identical bytes/paths/mtime, no absent artifact restored','files':rows},indent=2)+'\n')
  print(p.name,'saved allocation',rows[-1]['allocated_before']-rows[-1]['allocated_after'],'free',rows[-1]['observed_free_after'],flush=True)
