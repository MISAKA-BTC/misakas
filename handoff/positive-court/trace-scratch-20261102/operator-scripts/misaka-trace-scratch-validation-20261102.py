from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/trace-scratch-20261102';out.mkdir(exist_ok=False)
s=(root/'handoff/positive-court/optimized-runtime-20261102/operator-scripts/misaka-optimized-controller-20261102.py').read_text().replace("out=root/'handoff/positive-court/optimized-runtime-20261102'","out=root/'handoff/positive-court/trace-scratch-20261102'")
# Keep the full raw output in the log; avoid echoing thousands of model lines.
s=s.replace("if not line.startswith('{'):print(line)","if not line.startswith('{') and not 'ROLE_MEASURE ' in line:print(line)")
p=Path('/tmp/misaka-trace-scratch-controller-20261102.py');p.write_text(s);ns=runpy.run_path(str(p))
command=json.loads((root/'handoff/positive-court/optimized-runtime-20261102/hashed-walk-runtime-first.json').read_text())['command']
r=ns['run']('runtime-first',command,900,1100*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None and r['test_counts']==[(11,0,0),(4,0,0),(21,0,2)]
