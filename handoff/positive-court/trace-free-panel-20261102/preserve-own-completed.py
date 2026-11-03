from pathlib import Path
import fcntl,gzip,hashlib,json,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
record=root/'handoff/positive-court/trace-free-panel-20261102/own-completed-build-preservation.json'
rows=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(8*1024**2):h.update(b)
 return h.hexdigest()
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 for name,expected in [('palw_tir_v1_fence-d79d96bfb7bfc74e',132634480),('libkaspa_consensus_core-fdf047ace05c4991.rlib',372009744)]:
  p=base/name;stamp=p.stat();assert stamp.st_size==expected and stamp.st_mtime>1793572790
  log='source-and-old-fences-final.log' if 'fence-' in name else 'source-court-integration-functional-r3.log'
  assert name in (root/'handoff/positive-court/remaining-capacity-20261102'/log).read_text()
  dst=cache/(name+'.completed-20261102.gz');h=hashlib.sha256();minimum=shutil.disk_usage(p).free
  with p.open('rb') as f,dst.open('xb') as raw,gzip.GzipFile(filename='',fileobj=raw,mode='wb',compresslevel=1,mtime=0) as z:
   while b:=f.read(8*1024**2):
    h.update(b);z.write(b);minimum=min(minimum,shutil.disk_usage(p).free);assert minimum>128*1024**2
  restored=hashlib.sha256();size=0
  with gzip.open(dst,'rb') as z:
   while b:=z.read(8*1024**2):restored.update(b);size+=len(b)
  assert size==expected and restored.digest()==h.digest() and p.stat().st_mtime_ns==stamp.st_mtime_ns
  rows.append({'original':str(p),'original_sha256':h.hexdigest(),'original_bytes':expected,'archive':str(dst),'archive_bytes':dst.stat().st_size,'archive_sha256':sha(dst),'restored_stream_sha256':restored.hexdigest(),'restored_stream_bytes':size,'owned_runtime_log':log,'minimum_staging_free_bytes':minimum,'released':False})
  record.write_text(json.dumps(rows,indent=2)+'\n');p.unlink();rows[-1]['released']=True;rows[-1]['free_after']=shutil.disk_usage(base).free
  record.write_text(json.dumps(rows,indent=2)+'\n');print(name,rows[-1]['free_after'],flush=True)
 # The obsolete A4 pre-lint binary already has a retained gzip in tracked evidence.
 x=json.loads((root/'handoff/positive-court/remaining-capacity-20261102/a4-before-lint-executable.json').read_text())
 p=Path(x['original']['path']);archive=root/'handoff/positive-court/remaining-capacity-20261102'/x['archive']
 if p.exists():
  assert sha(p)==x['original']['sha256']
  h=hashlib.sha256();size=0
  with gzip.open(archive,'rb') as z:
   while b:=z.read(8*1024**2):h.update(b);size+=len(b)
  assert h.hexdigest()==x['original']['sha256'] and size==x['original']['bytes']
  p.unlink();rows.append({'original':str(p),'original_sha256':h.hexdigest(),'original_bytes':size,'archive':str(archive),'restored_stream_sha256':h.hexdigest(),'released':True,'free_after':shutil.disk_usage(base).free})
  record.write_text(json.dumps(rows,indent=2)+'\n');print(p.name,rows[-1]['free_after'],flush=True)
