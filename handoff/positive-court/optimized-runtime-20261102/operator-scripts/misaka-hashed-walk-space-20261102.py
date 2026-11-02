from pathlib import Path
import fcntl,hashlib,json,os,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/optimized-runtime-20261102';deps=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps');cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(1024**2):h.update(b)
 return h.hexdigest()
def artifacts(log,own=False):
 paths=set()
 for line in log.read_text().splitlines():
  if not line.startswith('{'):continue
  try:e=json.loads(line)
  except json.JSONDecodeError:continue
  if e.get('reason')=='compiler-artifact' and (not own or e.get('manifest_path','').startswith(str(root)+'/')):paths.update(e['filenames'])
 return paths
current=artifacts(out/'hashed-walk-runtime-first.log');old=set()
for label in ['runtime-measured-terms','clippy-core-first','clippy-panel-measured-terms']:
 old.update(artifacts(root/'handoff/positive-court/evaluation-diagnostics-20261102'/(label+'.log'),True))
baseline=json.loads((root/'handoff/positive-court/evaluation-diagnostics-20261102/runtime-measured-terms.json').read_text())['binary'];assert sha(Path(baseline['path']))==baseline['sha256']
with (deps.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 rows=[]
 for f in sorted(old-current):
  p=Path(f)
  if p.is_file() and p.parent==deps and p.suffix in ['.rlib','.rmeta']:
   rows.append({'path':f,'bytes':p.stat().st_size,'sha256':sha(p),'preserved_copy':False})
 record={'scope':'retire only own old opt0 intermediates proven absent from the complete current optimized build dependency set; all executable baselines retained','files':rows,'released':False,'free_before':shutil.disk_usage(deps).free}
 target=out/'obsolete-opt0-dependency-retirement.json';target.write_text(json.dumps(record,indent=2)+'\n')
 for r in rows:
  p=Path(r['path']);assert sha(p)==r['sha256'];p.unlink()
 record['released']=True;record['free_after']=shutil.disk_usage(deps).free;target.write_text(json.dumps(record,indent=2)+'\n')
 print('retired',len(rows),'obsolete own intermediates; free',record['free_after'],flush=True)
 # Transparent compression only: existing currently used dependency bytes and
 # mtimes stay identical. Skip already compressed/sparse and multiply-linked
 # files; do not touch native .a archives or restore any missing Rust output.
 compressed=[]
 for f in sorted(current):
  p=Path(f)
  if not p.is_file() or p.parent!=deps or p.suffix not in ['.rlib','.rmeta']:continue
  st=p.stat()
  if st.st_size<2*1024**2 or st.st_blocks*512<st.st_size//2 or st.st_nlink!=1:continue
  h=sha(p);tmp=cache/(p.name+'.current-dependency-hfs-20261102');assert not tmp.exists()
  subprocess.run(['/usr/bin/ditto','--hfsCompression',str(p),str(tmp)],check=True)
  ts=tmp.stat();assert sha(tmp)==h and ts.st_size==st.st_size and ts.st_mtime_ns==st.st_mtime_ns
  if ts.st_blocks<st.st_blocks:os.replace(tmp,p)
  else:tmp.unlink()
  assert sha(p)==h and p.stat().st_mtime_ns==st.st_mtime_ns
  compressed.append({'path':f,'sha256_unchanged':h,'bytes_unchanged':st.st_size,'mtime_ns_unchanged':st.st_mtime_ns,'allocated_before':st.st_blocks*512,'allocated_after':p.stat().st_blocks*512,'observed_free_after':shutil.disk_usage(deps).free})
  (out/'current-dependency-filesystem-compression.json').write_text(json.dumps({'scope':'transparent compression of existing immutable dependencies from the actual successful current Cargo artifact manifest','files':compressed},indent=2)+'\n')
  print(p.name,'saved',compressed[-1]['allocated_before']-compressed[-1]['allocated_after'],'free',compressed[-1]['observed_free_after'],flush=True)
 assert sha(Path(baseline['path']))==baseline['sha256']
