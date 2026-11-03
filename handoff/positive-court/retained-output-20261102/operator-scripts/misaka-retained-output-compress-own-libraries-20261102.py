from pathlib import Path
import fcntl,hashlib,json,os,shutil,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');base=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps');cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation')
out=root/'handoff/positive-court/retained-output-20261102/existing-library-filesystem-compression.json';rows=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while b:=f.read(256*1024):h.update(b)
 return h.hexdigest()
with (base.parent/'.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 for name in ['libkaspa_consensus_core-fdf047ace05c4991.rlib','libmisaka_palw_tir_exec-90f9ade7034b1ade.rlib','libkaspa_consensus-cd5ac97790b645f7.rlib']:
  p=base/name;st=p.stat();assert st.st_mtime>1793572790
  original=sha(p);tmp=cache/(name+'.hfs-staging-20261102')
  assert not tmp.exists();subprocess.run(['/usr/bin/ditto','--hfsCompression',str(p),str(tmp)],check=True)
  ts=tmp.stat();assert ts.st_size==st.st_size and sha(tmp)==original and ts.st_mtime_ns==st.st_mtime_ns
  if ts.st_blocks<st.st_blocks:os.replace(tmp,p)
  else:tmp.unlink()
  assert sha(p)==original and p.stat().st_mtime_ns==st.st_mtime_ns
  rows.append({'existing_path':str(p),'logical_bytes':st.st_size,'unchanged_sha256':original,'mtime_ns_unchanged':st.st_mtime_ns,'allocated_before':st.st_blocks*512,'allocated_after':p.stat().st_blocks*512,'observed_free_after':shutil.disk_usage(base).free})
  out.write_text(json.dumps({'scope':'transparent filesystem compression of already-present valid own compiled libraries; identical readable bytes/paths/mtime; no absent library restored or foreign binary injected','files':rows},indent=2)+'\n')
  print(name,'reported allocation saved',rows[-1]['allocated_before']-rows[-1]['allocated_after'],'free',rows[-1]['observed_free_after'],flush=True)
