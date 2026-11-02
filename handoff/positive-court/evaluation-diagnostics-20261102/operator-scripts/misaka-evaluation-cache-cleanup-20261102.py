from pathlib import Path
import fcntl,hashlib,json,shutil
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
out=root/'handoff/positive-court/evaluation-diagnostics-20261102/retired-cache-cleanup.json'
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(1024**2):h.update(b)
 return h.hexdigest()
rows=[]
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 # Only explicitly recorded unused before-goal Rust library backups, not
 # measurement executables, original model/native archives, sources or raw logs.
 for record in ['trace-free-panel-20261102','remaining-capacity-20261102']:
  manifest=root/'handoff/positive-court'/record/'old-core-library-compressed-preservation.json'
  old=json.loads(manifest.read_text());p=Path(old['archive_path'])
  assert p.parent==Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
  assert old.get('before_goal',True) and p.stat().st_size==old['archive_bytes'] and sha(p)==old['archive_sha256']
  assert not Path(old['original_path']).exists()
  rows.append({'path':str(p),'bytes':p.stat().st_size,'sha256':old['archive_sha256'],'historical_record':str(manifest),'reason':'retire obsolete unused regenerable before-goal build-library backup','preserved_copy':False})
 # Our exact interrupted compiler output. Cargo reported no successful artifact
 # and the child process group was reaped. These are not accepted Rust libraries.
 failed=[]
 for p in base.glob('kaspa_consensus_core-fdf047ace05c4991.*.rcgu.o'):failed.append(p)
 failed.append(base/'libkaspa_consensus_core-fdf047ace05c4991.rmeta')
 assert len(failed)==17 and not (base/'libkaspa_consensus_core-fdf047ace05c4991.rlib').exists()
 for p in failed:
  s=p.stat();assert 1793584513<=s.st_mtime<=1793584606
  rows.append({'path':str(p),'bytes':s.st_size,'sha256':sha(p),'reason':'temporary output of the terminal disk-stopped own compiler run','preserved_copy':False})
 result={'scope':'generated Rust cache only; original preservation records remain historical and are superseded for current availability by this retirement receipt','files':rows,'released':False,'free_before':shutil.disk_usage(base).free}
 out.write_text(json.dumps(result,indent=2)+'\n')
 for row in rows:
  p=Path(row['path']);assert sha(p)==row['sha256'];p.unlink()
 result['released']=True;result['free_after']=shutil.disk_usage(base).free
 out.write_text(json.dumps(result,indent=2)+'\n')
 print('retired generated cache entries',len(rows),'observed free',result['free_after'],flush=True)
