from pathlib import Path
import json,subprocess,hashlib,gzip,shutil,tempfile,os,time
root=Path.cwd();base=root/'docs/evidence/positive-court-a0-2026-10-31/integration';target=Path('/Users/wata/.codex/worktrees/positive-court-a0/target');out=target/'panel-r5-binaries-preserved-20261102';out.mkdir(exist_ok=False)
def h(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1<<20):d.update(b)
 return d.hexdigest()
ps=subprocess.check_output(['ps','-axo','comm'],text=True);assert not any(Path(x.strip()).name in ['cargo','rustc','clippy-driver'] for x in ps.splitlines())
arts={}
for record in ['panel-r5-exec-1c903-20261102.json','panel-r5-core-geometry-artifact-1c903-20261102.json']:
 for r in json.loads((base/record).read_text())['runs']:
  a=r['artifact']
  if r['scope']['key']=='misaka_palw_tir_artifact':continue
  arts[a['path']]=a
allaliases={};inodes={}
for name,a in arts.items():
 p=Path(name);s=p.stat();assert s.st_size==a['bytes'] and h(p)==a['sha256'];inodes[(s.st_dev,s.st_ino)]=name;allaliases[name]=[]
for directory,_,files in os.walk(target/'debug'):
 for name in files:
  p=Path(directory)/name
  if p.is_symlink():continue
  s=p.stat();key=(s.st_dev,s.st_ino)
  if key in inodes:allaliases[inodes[key]].append({'path':str(p),'bytes':s.st_size,'mode':s.st_mode,'mtime_ns':s.st_mtime_ns,'inode':s.st_ino,'device':s.st_dev,'nlink':s.st_nlink})
paths=[x['path'] for v in allaliases.values() for x in v];q=subprocess.run(['lsof','-Fpn','--',*paths],capture_output=True,text=True);assert q.returncode==1 and not q.stdout and not q.stderr
entries=[]
for name,a in arts.items():
 p=Path(name);aliases=allaliases[name];assert len(aliases)==p.stat().st_nlink;z=out/(p.name+'.gz')
 with p.open('rb') as src,gzip.open(z,'wb',compresslevel=6) as dst:shutil.copyfileobj(src,dst,1<<20)
 with tempfile.NamedTemporaryFile(dir=out,prefix='restore-') as tmp:
  with gzip.open(z,'rb') as src:shutil.copyfileobj(src,tmp,1<<20)
  tmp.flush();assert Path(tmp.name).stat().st_size==a['bytes'] and h(tmp.name)==a['sha256']
 entry={'artifact':a,'aliases':aliases,'gzip':str(z),'gzip_bytes':z.stat().st_size,'gzip_sha256':h(z),'actual_tempfile_restore_size_sha_verified':True};entries.append(entry)
 for alias in aliases:
  f=Path(alias['path']);s=f.stat();assert (s.st_size,s.st_mtime_ns,s.st_ino,s.st_dev)==(alias['bytes'],alias['mtime_ns'],alias['inode'],alias['device']);assert h(f)==a['sha256']
 for alias in aliases:Path(alias['path']).unlink()
 (out/'manifest.json').write_text(json.dumps(entries,indent=2)+'\n')
summary={'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'purpose':'Preserve actual source1c903 runtime86 Core-dependent executables before reclaim after native Core source change; all raw/source/models and reusable artifact/TIR leaf binaries kept','entries':entries,'original_unique_bytes':sum(x['artifact']['bytes'] for x in entries),'gzip_bytes':sum(x['gzip_bytes'] for x in entries),'free_bytes_final':shutil.disk_usage(target).free,'restore_rule':'Restore to a NEW directory with original executable mode. Never overwrite current Cargo outputs. No Rust rlib restore or manual copy.'};p=base/'panel-r5-binaries-preserved-20261102.json';assert not p.exists();p.write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:summary[k] for k in ['original_unique_bytes','gzip_bytes','free_bytes_final']}))
