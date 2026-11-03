from pathlib import Path
import fcntl,hashlib,json,re,shutil,tarfile
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
paths=[]
for log in ['contract-accounting-final.log','source-and-old-fences-final.log']:
 paths += [Path(p) for p in re.findall(r'Running .*?\((/Users/[^)]+/target/debug/deps/[^)]+)\)',(root/'handoff/positive-court/remaining-capacity-20261102'/log).read_text())]
paths=sorted({p for p in paths if p.is_file()})
archive=Path('/Users/wata/.cache/misaka-positive-court-build-preservation/previous-own-contract-fence-tests-20261102.tar.gz')
out=root/'handoff/positive-court/retained-output-20261102/previous-test-preservation.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 assert len(paths)==17 and all(p.stat().st_mtime>1793572790 for p in paths)
 rows={p.name:{'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)} for p in paths}
 with tarfile.open(archive,'x:gz',compresslevel=1) as tar:
  for p in paths:tar.add(p,arcname=p.name,recursive=False)
 with tarfile.open(archive,'r:gz') as tar:
  for member in tar.getmembers():
   with tar.extractfile(member) as f:h=hashlib.sha256(f.read()).hexdigest()
   assert h==rows[member.name]['sha256'] and member.size==rows[member.name]['bytes']
 record={'archive':str(archive),'archive_bytes':archive.stat().st_size,'archive_sha256':sha(archive),'full_extracted_streams_verified':True,'files':list(rows.values()),'released':False}
 out.write_text(json.dumps(record,indent=2)+'\n')
 for p in paths:assert sha(p)==rows[p.name]['sha256'];p.unlink()
 record['released']=True;record['observed_free_after']=shutil.disk_usage(base).free;out.write_text(json.dumps(record,indent=2)+'\n')
 print('preserved',len(paths),'free',record['observed_free_after'])
