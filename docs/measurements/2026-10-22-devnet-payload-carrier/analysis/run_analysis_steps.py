from pathlib import Path
import subprocess,json,hashlib,sys,time
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet')
b=repo/'docs/measurements/2026-10-22-devnet-payload-carrier'
h=Path(Path('/tmp/g14-current-payload-run-dir').read_text().strip())
commands=[
(['python3',str(b/'lossless_compress.py'),str(h/'run/sender-events.jsonl'),str(b/'raw/sender-events.jsonl.xz')],b/'sender-compression.log',repo),
(['python3','-m','unittest','-v','test_recalc'],b/'analysis-tests.log',b/'analysis'),
(['python3',str(b/'analysis/run_trace_audit.py')],b/'trace-audit.log',repo),
(['python3',str(b/'analysis/recalc.py'),'--sender',str(b/'raw/sender-events.jsonl.xz'),'--observer',str(b/'raw/observer.jsonl.xz'),'--funded-root',str(b/'raw/run/observed-funded-root.json'),'--params',str(b/'analysis/params-source-candidate.json')],b/'independent-recalculation.json',repo)]
steps=[]
for cmd,out,cwd in commands:
 start=time.time();print('START',out.name,flush=True)
 with out.open('xb') as f:r=subprocess.run(cmd,cwd=cwd,stdout=f,stderr=subprocess.STDOUT)
 steps.append(dict(command=cmd,cwd=str(cwd),log=str(out.relative_to(b)),exit_code=r.returncode,elapsed_seconds=time.time()-start))
 (b/'analysis-steps.json').write_text(json.dumps(steps,indent=2)+'\n')
 print('END',out.name,r.returncode,flush=True)
 if r.returncode:sys.exit(r.returncode)
 if out.name=='independent-recalculation.json':
  data=json.loads(out.read_text())
  if data.get('status')=='INCOMPLETE_EVIDENCE':raise RuntimeError(data['reason'])
files={str(p.relative_to(b)):dict(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in b.rglob('*') if p.is_file() and p.name!='preservation-sha256.json' and '__pycache__' not in p.parts}
(b/'preservation-sha256.json').write_text(json.dumps(dict(kind='final evidence checkpoint before report prose',files=files),indent=2)+'\n')
