from pathlib import Path
import fcntl,gzip,hashlib,json,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps');cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
out=root/'handoff/positive-court/trained-roles-20261102/build-space-release.json'
record={'before_free_bytes':shutil.disk_usage(base).free,'binary_preservation':[],'matched_archive_objects':[]}
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(256*1024):h.update(b)
 return h.hexdigest()
known=set()
for log in [root/'handoff/positive-court/remaining-capacity-20261102/source-court-integration-functional-r3.log',root/'handoff/positive-court/trace-free-panel-20261102/full-runtime.log']:
 for line in log.read_text().splitlines():
  if not line.startswith('{'):continue
  try:e=json.loads(line)
  except json.JSONDecodeError:continue
  if e.get('reason')=='compiler-artifact':known.update(e['filenames'])
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 for name in ['positive_court_a4_integration-1f1c784b94b2a323','positive_wide_material_v2-b6a66b1c57fb18c4','local_forward_metadata_v2-59f987b35f5d8f7a','local_wide_forward_v2-43ccdd4d821ffb8c']:
  src=base/name;assert str(src) in known
  st=src.stat();before=sha(src);dst=cache/(name+'.pre-trained-20261102.gz')
  with src.open('rb') as f,dst.open('xb') as raw,gzip.GzipFile(filename='',fileobj=raw,mode='wb',compresslevel=1,mtime=0) as z:
   while b:=f.read(256*1024):z.write(b)
  h=hashlib.sha256();n=0
  with gzip.open(dst,'rb') as f:
   while b:=f.read(256*1024):h.update(b);n+=len(b)
  assert h.hexdigest()==before and n==st.st_size and sha(src)==before
  row={'original':str(src),'bytes':st.st_size,'sha256':before,'archive':str(dst),'archive_bytes':dst.stat().st_size,'archive_sha256':sha(dst),'full_restored_stream_sha256':h.hexdigest(),'full_restored_stream_bytes':n,'released':False}
  record['binary_preservation'].append(row);out.write_text(json.dumps(record,indent=2)+'\n')
  src.unlink();row['released']=True;out.write_text(json.dumps(record,indent=2)+'\n')
 for p in sorted(base.glob('*.rcgu.o')):
  if p.stat().st_mtime<1793572790:continue
  key=p.name.split('.')[0];lib=base/('lib'+key+'.rlib')
  if str(lib) not in known or not lib.is_file():continue
  result=subprocess.run(['ar','p',str(lib),p.name],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  if result.returncode or not result.stdout:continue
  actual=sha(p)
  if hashlib.sha256(result.stdout).hexdigest()!=actual:continue
  record['matched_archive_objects'].append({'path':str(p),'bytes':p.stat().st_size,'sha256':actual,'retained_archive':str(lib),'archive_member':p.name,'member_sha256':actual})
  out.write_text(json.dumps(record,indent=2)+'\n');p.unlink()
 record['after_free_bytes']=shutil.disk_usage(base).free;out.write_text(json.dumps(record,indent=2)+'\n')
print('preserved binaries',len(record['binary_preservation']),'matched objects',len(record['matched_archive_objects']),'object bytes',sum(x['bytes'] for x in record['matched_archive_objects']),'observed free',record['after_free_bytes'])
