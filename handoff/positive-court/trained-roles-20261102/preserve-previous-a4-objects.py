from pathlib import Path
import fcntl,hashlib,json,shutil,tarfile
base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps')
cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
archive=cache/'a4-b0b-completed-test-objects-20261102.tar.gz'
out=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet/handoff/positive-court/trained-roles-20261102/completed-test-object-preservation.json')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 paths=sorted(base.glob('positive_court_a4_integration-b0b7dbbb6d304557.*.rcgu.o'))
 assert paths and all(p.stat().st_mtime>1793572790 for p in paths)
 rows={p.name:{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size,'inode':p.stat().st_ino} for p in paths}
 before=shutil.disk_usage(base).free
 with tarfile.open(archive,'x:gz',compresslevel=1) as tar:
  for p in paths:tar.add(p,arcname=p.name,recursive=False)
 with tarfile.open(archive,'r:gz') as tar:
  members=tar.getmembers();assert len(members)==len(rows)
  for m in members:
   row=rows[m.name];h=hashlib.sha256();n=0
   with tar.extractfile(m) as f:
    while b:=f.read(256*1024):h.update(b);n+=len(b)
   assert h.hexdigest()==row['sha256'] and n==row['bytes']
 record={'scope':'exact previous own A4 b0b test object prefix; its linked executable is already fully archived in remaining-capacity evidence; no source/model/native archive removal','archive':str(archive),'archive_sha256':sha(archive),'archive_bytes':archive.stat().st_size,'all_extracted_streams_verified':True,'before_free_bytes':before,'files':list(rows.values()),'released':False}
 out.write_text(json.dumps(record,indent=2)+'\n')
 for p in paths:
  row=rows[p.name];assert p.stat().st_ino==row['inode'] and sha(p)==row['sha256'];p.unlink()
 record['released']=True;record['after_free_bytes']=shutil.disk_usage(base).free;out.write_text(json.dumps(record,indent=2)+'\n')
 print('objects',len(rows),'bytes',sum(r['bytes'] for r in rows.values()),'archive',archive.stat().st_size,'free',record['after_free_bytes'])
