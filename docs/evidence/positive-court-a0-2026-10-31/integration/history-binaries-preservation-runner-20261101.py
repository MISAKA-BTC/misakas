from pathlib import Path
import subprocess,os,re,json,gzip,hashlib,tempfile,time
base=Path('/Users/wata/.codex/worktrees/positive-court-a0/target');deps=base/'debug/deps';out=base/'history-binaries-preserved-20261101';out.mkdir(exist_ok=False)
ps=subprocess.check_output(['ps','-axo','pid,comm'],text=True)
assert not any(re.search(r'\b(?:cargo|rustc|clippy-driver|cargo-clippy)$',l) for l in ps.splitlines())
allfiles=[p for d in [base/'debug',deps] for p in d.iterdir() if p.is_file() and not p.is_symlink()]
groups=[]
for p in deps.iterdir():
 if not (p.is_file() and re.fullmatch('[A-Za-z0-9_]+-[0-9a-f]{16}',p.name) and os.access(p,os.X_OK)):continue
 with p.open('rb') as f:
  if f.read(4)!=b'\xcf\xfa\xed\xfe':continue
 s=p.stat();aliases=[q for q in allfiles if (q.stat().st_dev,q.stat().st_ino)==(s.st_dev,s.st_ino)]
 groups.append((p,s,aliases))
rows=[]
for p,st,aliases in sorted(groups,key=lambda x:-x[1].st_size):
 for alias in aliases:
  r=subprocess.run(['lsof','-t',str(alias)],capture_output=True,text=True);assert r.returncode==1 and not r.stdout,(alias,r.stdout)
 h=hashlib.sha256();gz=out/(p.name+'.gz')
 with p.open('rb') as src,gzip.open(gz,'wb',compresslevel=3) as dst:
  while b:=src.read(1048576):h.update(b);dst.write(b)
 actual=h.hexdigest();verify=hashlib.sha256();n=0
 with tempfile.TemporaryFile(dir=out) as restored:
  with gzip.open(gz,'rb') as src:
   while b:=src.read(1048576):restored.write(b);n+=len(b)
  restored.flush();os.fsync(restored.fileno());restored.seek(0)
  while b:=restored.read(1048576):verify.update(b)
 assert n==st.st_size and verify.hexdigest()==actual
 for alias in aliases:
  s=alias.stat();assert (s.st_ino,s.st_size,s.st_mtime_ns,s.st_nlink)==(st.st_ino,st.st_size,st.st_mtime_ns,st.st_nlink)
 row={'original':str(p),'aliases':[str(a) for a in aliases],'bytes':st.st_size,'sha256':actual,'mode':st.st_mode,'mtime_ns':st.st_mtime_ns,'inode':st.st_ino,'nlink':st.st_nlink,'gzip':str(gz),'gzip_bytes':gz.stat().st_size,'gzip_sha256':hashlib.sha256(gz.read_bytes()).hexdigest(),'actual_tempfile_restore_bytes_sha_verified':True,'deleted_aliases':[]}
 rows.append(row)
 # Record the verified backup BEFORE unlinking any original path.
 manifest={'scope':'Root-owned inactive Cargo Mach-O executable artifacts only, preserved losslessly before unlink. No rawlogs/source/models/nativearchives/rlibs/other cache removed. No new test/qualification. No Rust rlib copy.','source_head':'cc22b0da1','restore_policy':'Restore into a fresh directory, set recorded executable mode; do not silently overwrite Cargo/current source artifacts. Original gzip decompressed into an actual tempfile and read back for size/SHA verification.','files':rows}
 (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
 if st.st_nlink==len(aliases):
  for alias in aliases:
   s=alias.stat();assert s.st_ino==st.st_ino and s.st_size==st.st_size and s.st_mtime_ns==st.st_mtime_ns;alias.unlink();row['deleted_aliases'].append(str(alias))
  (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
 if len(rows)%10==0:print('preserved',len(rows),'free',os.statvfs(base).f_bavail*os.statvfs(base).f_frsize,flush=True)
summary={'files':len(rows),'apparent_original_bytes':sum(x['bytes'] for x in rows),'gzip_bytes':sum(x['gzip_bytes'] for x in rows),'unlinked_inode_groups':sum(bool(x['deleted_aliases']) for x in rows),'free_bytes_after':os.statvfs(base).f_bavail*os.statvfs(base).f_frsize,'manifest':str(out/'manifest.json'),'manifest_sha256':hashlib.sha256((out/'manifest.json').read_bytes()).hexdigest()}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
