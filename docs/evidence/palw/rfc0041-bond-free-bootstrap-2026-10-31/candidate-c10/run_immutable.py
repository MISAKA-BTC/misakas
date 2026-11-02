import argparse,hashlib,json,os,re,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--manifest',required=True);p.add_argument('--crate',required=True);p.add_argument('--filter',required=True);p.add_argument('--exact',action='store_true');p.add_argument('--minimum',type=int,default=1);p.add_argument('--output',required=True);a=p.parse_args()
m=json.loads(Path(a.manifest).read_text());artifact=next(x for x in m['artifacts'] if x['crate']==a.crate);binary=Path(artifact['immutable_binary']);out=Path(a.output)
sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
assert sha(binary)==artifact['sha256'],'immutable executable changed'
flags=[a.filter]+(['--exact'] if a.exact else [])
listing=subprocess.run([str(binary),*flags,'--list'],text=True,capture_output=True);matched=[l[:-6] for l in listing.stdout.splitlines() if l.endswith(': test')]
out.with_suffix('.list.txt').write_text(listing.stdout+listing.stderr)
r={'candidate':m['candidate'],'crate':a.crate,'source_build':m['build_log'],'source_build_sha256':m['build_log_sha256'],'binary':str(binary),'binary_sha256':artifact['sha256'],'features':artifact['features'],'filter':a.filter,'exact':a.exact,'matched':matched,'minimum':a.minimum,'listing_exit':listing.returncode,'command':[str(binary),*flags,'--nocapture','--test-threads=1']}
if listing.returncode or len(matched)<a.minimum:r['status']='refused_missing_tests';out.with_suffix('.json').write_text(json.dumps(r,indent=2));raise SystemExit(2)
env=os.environ.copy();env['RUST_MIN_STACK']='67108864';start=time.monotonic()
with out.with_suffix('.log').open('w') as f:result=subprocess.run(r['command'],stdout=f,stderr=subprocess.STDOUT,env=env)
summaries=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',out.with_suffix('.log').read_text(errors='replace'))
passed=bool(summaries) and result.returncode==0 and summaries[-1][0]=='ok' and int(summaries[-1][1])>=a.minimum and int(summaries[-1][2])==0 and int(summaries[-1][3])==0
assert sha(binary)==artifact['sha256'],'immutable executable changed while executing'
r.update(status='PASS' if passed else 'FAIL',exit_code=result.returncode,elapsed_seconds=round(time.monotonic()-start,3),summaries=summaries,log_sha256=sha(out.with_suffix('.log')))
out.with_suffix('.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));raise SystemExit(0 if passed else 1)
