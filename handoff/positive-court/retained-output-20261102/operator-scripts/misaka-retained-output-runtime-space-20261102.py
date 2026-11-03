from pathlib import Path
import fcntl,gzip,hashlib,json,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation');out=root/'handoff/positive-court/retained-output-20261102/runtime-space-preservation.json'
rows=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(256*1024):h.update(b)
 return h.hexdigest()
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 for name in ['run_wide_q24_v2-0fdee7bfd1253dd4','local_forward_metadata_v2-59f987b35f5d8f7a','local_wide_forward_v2-43ccdd4d821ffb8c','tir_exec_bench-14266bc34159bb62','tir_exec_bench-679853095df13205']:
  p=base/name;st=p.stat();assert st.st_mtime>1793572790
  h=sha(p);dst=cache/(name+'.retained-runtime-20261102.gz')
  with p.open('rb') as f,dst.open('xb') as raw,gzip.GzipFile(filename='',fileobj=raw,mode='wb',compresslevel=1,mtime=0) as z:
   while b:=f.read(256*1024):z.write(b)
  restored=hashlib.sha256();size=0
  with gzip.open(dst,'rb') as z:
   while b:=z.read(256*1024):restored.update(b);size+=len(b)
  assert restored.hexdigest()==h and size==st.st_size and sha(p)==h
  row={'path':str(p),'bytes':st.st_size,'sha256':h,'archive':str(dst),'archive_bytes':dst.stat().st_size,'archive_sha256':sha(dst),'restored_stream_sha256':restored.hexdigest(),'restored_stream_bytes':size,'released':False}
  rows.append(row);out.write_text(json.dumps(rows,indent=2)+'\n');p.unlink();row['released']=True;row['observed_free_after']=shutil.disk_usage(base).free
  out.write_text(json.dumps(rows,indent=2)+'\n')
 print('preserved exact own test/build binaries',len(rows),'free',shutil.disk_usage(base).free)
