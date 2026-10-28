#!/usr/bin/env python3
"""Restore only missing predeclared safetensors, exact revision/size/SHA, no acceptance credit."""
import argparse,concurrent.futures,hashlib,json,os,time,urllib.request,urllib.error
from pathlib import Path

def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for b in iter(lambda:f.read(1<<20),b''):h.update(b)
 return h.hexdigest()

def restore(m,row):
 dest=Path(m['source_directory'])/row['path'];expected=row['bytes'];sha=row['sha256']
 result={'id':m['id'],'repository':m['repository'],'revision':m['revision'],'path':row['path'],'expected_bytes':expected,'expected_sha256':sha}
 if dest.exists():
  result.update(status='EXISTING_PASS' if dest.stat().st_size==expected and digest(dest)==sha else 'EXISTING_MISMATCH_NOT_OVERWRITTEN');return result
 part=dest.with_name(dest.name+'.rfc02-restore-'+sha[:16]+'.part');starttime=time.monotonic()
 for attempt in range(1,6):
  try:
   size=part.stat().st_size if part.exists() else 0
   if size>expected:raise ValueError('own partial file exceeds predeclared size')
   if size<expected:
    url='https://huggingface.co/'+m['repository']+'/resolve/'+m['revision']+'/'+row['path']+'?rfc02_restore='+str(time.time_ns())
    req=urllib.request.Request(url,headers={'User-Agent':'RFC02-pinned-checkpoint-restorer/1','Range':'bytes='+str(size)+'-'})
    with urllib.request.urlopen(req,timeout=45) as remote:
     if remote.status==200:size=0
     elif remote.status!=206:raise ValueError('unexpected range response status')
     elif not remote.headers.get('Content-Range','').startswith('bytes '+str(size)+'-'):raise ValueError('range start mismatch')
     length=remote.headers.get('Content-Length')
     if length is not None and int(length)!=expected-size:raise ValueError('response exceeds/differs from expected byte extent')
     last=time.monotonic()
     with part.open('ab' if size else 'wb') as out:
      while True:
       block=remote.read(1<<20)
       if not block:break
       if size+len(block)>expected:raise ValueError('download exceeds predeclared bytes')
       out.write(block);size+=len(block)
       if time.monotonic()-last>=30:print(m['id'],row['path'],str(size)+'/'+str(expected),flush=True);last=time.monotonic()
   if part.stat().st_size!=expected:raise ValueError('incomplete response')
   actual=digest(part)
   if actual!=sha:raise ValueError('download SHA mismatch; own partial retained')
   # Hard-link installation refuses to replace any file that appeared concurrently.
   os.link(part,dest);part.unlink()
   result.update(status='RESTORED_PASS',actual_bytes=expected,actual_sha256=actual,elapsed_seconds=time.monotonic()-starttime)
   print(m['id'],row['path'],'RESTORED_PASS',flush=True);return result
  except Exception as e:
   # Never expose credentials or signed redirect URLs. Interrupted own partials remain resumable.
   result.update(status='RESTORE_FAILED',failure_type=type(e).__name__,attempt=attempt,partial_bytes=part.stat().st_size if part.exists() else 0)
   print(m['id'],row['path'],type(e).__name__,'attempt',attempt,flush=True)
 return result

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('cohort',type=Path);ap.add_argument('output',type=Path);ap.add_argument('--members',nargs='+',required=True);ap.add_argument('--max-total-bytes',type=int,default=35<<30);a=ap.parse_args()
 if a.output.exists():raise ValueError('fresh restore report required')
 raw=a.cohort.read_bytes()
 if len(raw)>2<<20:raise ValueError('cohort JSON cap')
 p=json.loads(raw);ms=[m for m in p['members'] if m['id'] in a.members]
 if len(ms)!=len(set(a.members)):raise ValueError('all requested IDs must be predeclared')
 work=[]
 for m in ms:
  if len(m['revision'])!=40 or any(c not in '0123456789abcdef' for c in m['revision']):raise ValueError('exact revision required')
  for r in m['source_files']:
   if not r['path'].endswith('.safetensors'):continue
   if Path(r['path']).is_absolute() or any(c in r['path'].split('/') for c in ['.','..']) or '\\' in r['path']:raise ValueError('safe relative payload path')
   if type(r['bytes']) is not int or r['bytes']<=0 or len(r['sha256'])!=64 or any(c not in '0123456789abcdef' for c in r['sha256']):raise ValueError('exact byte/digest pin')
   work.append((m,r))
 if sum(r['bytes'] for _,r in work)>a.max_total_bytes:raise ValueError('predeclared total payload cap')
 with concurrent.futures.ThreadPoolExecutor(max_workers=3) as ex:results=list(ex.map(lambda pair:restore(*pair),work))
 report={'schema':'misaka.rfc02.exact-source-payload-restore.v1','cohort_sha256':hashlib.sha256(raw).hexdigest(),'requested_members':a.members,'predeclared_total_payload_bytes':sum(r['bytes'] for _,r in work),'records':results,'new_revision_labels_authenticated':False,'fullcontext_fidelity_nativeFinal_or_A6_A8_credit':False}
 with a.output.open('x') as f:json.dump(report,f,indent=2);f.write('\n')
 if any(r['status'] not in ['RESTORED_PASS','EXISTING_PASS'] for r in results):raise SystemExit('one or more exact payload restorations failed; records retained')
if __name__=='__main__':main()
