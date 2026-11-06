from pathlib import Path
import fcntl,gzip,hashlib,json,os,shutil,subprocess,time
src=Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/deps/libkaspa_consensus_core-3f367eb1dd97a347.rlib')
cache=Path('/Users/wata/.cache/misaka-positive-court-build-preservation');cache.mkdir(exist_ok=True)
dst=cache/(src.name+'.20261102.gz')
record=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet/handoff/positive-court/trace-free-panel-20261102/old-core-library-compressed-preservation.json')
with Path('/Users/wata/Downloads/MISAKA-testnet/target/debug/.cargo-lock').open('a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 ps=subprocess.check_output(['ps','-axo','pid,command'],text=True)
 assert not any(src.name in line for line in ps.splitlines())
 stamp=src.stat();assert stamp.st_mtime<1793572790 and stamp.st_size==1208521856
 original=hashlib.sha256();archive=hashlib.sha256();minimum=shutil.disk_usage(src).free
 with src.open('rb') as inp,dst.open('xb') as raw,gzip.GzipFile(filename='',fileobj=raw,mode='wb',compresslevel=1,mtime=0) as out:
  while data:=inp.read(8*1024**2):
   original.update(data);out.write(data)
   minimum=min(minimum,shutil.disk_usage(src).free)
   assert minimum>128*1024**2,'preservation staging floor'
 restored=hashlib.sha256();restored_bytes=0
 with gzip.open(dst,'rb') as inp:
  while data:=inp.read(8*1024**2):restored.update(data);restored_bytes+=len(data)
 assert original.digest()==restored.digest() and restored_bytes==stamp.st_size and src.stat()==stamp
 with dst.open('rb') as inp:
  while data:=inp.read(8*1024**2):archive.update(data)
 data={'original_path':str(src),'original_bytes':stamp.st_size,'original_mtime':stamp.st_mtime,'original_sha256':original.hexdigest(),'archive_path':str(dst),'archive_bytes':dst.stat().st_size,'archive_sha256':archive.hexdigest(),'full_decompressed_stream_sha256':restored.hexdigest(),'full_decompressed_stream_bytes':restored_bytes,'minimum_staging_free_bytes':minimum,'before_goal':True,'cargo_lock_exclusive':True,'no_live_reference_observed':True,'reason':'preserve old regenerable Rust library before releasing cache for final trace-free Panel strict lint','rust_output_manually_restored':False}
 record.write_text(json.dumps(data,indent=2)+'\n')
 src.unlink()
 print(json.dumps({'preserved':src.name,'archive_bytes':dst.stat().st_size,'freed_net_bytes':stamp.st_size-dst.stat().st_size,'free_after':shutil.disk_usage(dst).free}))
