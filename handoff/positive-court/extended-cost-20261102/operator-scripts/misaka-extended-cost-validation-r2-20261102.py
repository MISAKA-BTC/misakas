from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/extended-cost-20261102';assert out.is_dir()
s=(root/'handoff/positive-court/trace-scratch-20261102/operator-scripts/misaka-trace-scratch-controller-20261102.py').read_text().replace("out=root/'handoff/positive-court/trace-scratch-20261102'","out=root/'handoff/positive-court/extended-cost-20261102'")
s=s.replace("env['RAYON_NUM_THREADS']='1'","env['RAYON_NUM_THREADS']='1';env.pop('MISAKA_TRAINED_COST_WINDOW',None)")
p=Path('/tmp/misaka-extended-cost-controller-20261102.py');p.write_text(s);ns=runpy.run_path(str(p))
command=json.loads((root/'handoff/positive-court/trace-scratch-20261102/runtime-first.json').read_text())['command']
r=ns['run']('runtime-r2',command,900,1100*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None and r['test_counts']==[(11,0,0),(4,0,0),(21,0,3)]
