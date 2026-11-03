from pathlib import Path
import json,subprocess,hashlib,time
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/three-roles-20261102';libs={}
for line in (out/'runtime-r3.log').read_text().splitlines():
 if line.startswith('{'):
  try:r=json.loads(line)
  except:continue
  if r.get('reason')=='compiler-artifact' and r['target']['name'] in ['kaspa_consensus_core','borsh']:libs[r['target']['name']]=next(p for p in r['filenames'] if p.endswith('.rlib'))
source=out/'operator-scripts/legacy-cert-scan.rs';binary=Path('/tmp/misaka-legacy-cert-scan-20261102');command=['rustc','--edition=2024',str(source),'-o',str(binary),'-L','dependency=/Users/wata/Downloads/MISAKA-testnet/target/debug/deps']
for n,p in libs.items():command+=['--extern',n+'='+p]
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest();pins={p:sha(p) for p in libs.values()};t=time.monotonic();compile=subprocess.run(command,cwd=root,capture_output=True,text=True,timeout=60);record={'command':command,'source_sha256':sha(source),'rlib_sha256':pins,'compile_exit':compile.returncode,'compile_stdout':compile.stdout,'compile_stderr':compile.stderr,'elapsed_compile_seconds':time.monotonic()-t,'scope':'old canonical row codec shape only; zero diagnostic header, not actual accepted state/funding/P'}
if compile.returncode==0:
 run=subprocess.run([str(binary)],capture_output=True,text=True,timeout=30);record.update({'binary_sha256':sha(binary),'run_exit':run.returncode,'run_stdout':run.stdout,'run_stderr':run.stderr})
record['rlib_unchanged']=all(sha(p)==h for p,h in pins.items());(out/'legacy-cert-scan.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps({k:v for k,v in record.items() if k not in ['rlib_sha256','command']},indent=2))
