import argparse,pathlib,gzip,lzma,hashlib,json,time
p=argparse.ArgumentParser();p.add_argument("source",type=pathlib.Path);p.add_argument("target",type=pathlib.Path);a=p.parse_args()
assert a.source.resolve()!=a.target.resolve(); start=time.time()
def hashfile(path):
 h=hashlib.sha256()
 with path.open("rb") as f:
  for chunk in iter(lambda:f.read(1024*1024),b""):h.update(chunk)
 return h.hexdigest()
original_hash=hashfile(a.source); plain=hashlib.sha256();n=0
filters=[dict(id=lzma.FILTER_LZMA2,dict_size=64*1024*1024,mode=lzma.MODE_FAST,mf=lzma.MF_HC4,nice_len=64)]
source_open = gzip.open if a.source.suffix == ".gz" else open
with source_open(a.source,"rb") as src,a.target.open("xb") as raw:
 with lzma.LZMAFile(raw,"w",format=lzma.FORMAT_XZ,filters=filters) as dest:
  for chunk in iter(lambda:src.read(1024*1024),b""):
   plain.update(chunk);n+=len(chunk);dest.write(chunk)
verify=hashlib.sha256();v=0
with lzma.open(a.target,"rb") as f:
 for chunk in iter(lambda:f.read(1024*1024),b""):verify.update(chunk);v+=len(chunk)
assert n==v and plain.hexdigest()==verify.hexdigest();assert hashfile(a.source)==original_hash
meta=dict(kind="lossless JSONL compression; original retained unchanged",original_path=str(a.source.resolve()),original_sha256=original_hash,original_bytes=a.source.stat().st_size,target_path=str(a.target.resolve()),target_xz_sha256=hashfile(a.target),target_xz_bytes=a.target.stat().st_size,uncompressed_bytes=n,uncompressed_sha256=plain.hexdigest(),roundtrip_identical=True,elapsed_seconds=time.time()-start,filters=[dict(codec="LZMA2",dictionary_bytes=64*1024*1024,mode="FAST",match_finder="HC4",nice_len=64)])
with a.target.with_suffix(a.target.suffix+".json").open("x") as f:json.dump(meta,f,indent=2)
print(json.dumps(meta,indent=2))
