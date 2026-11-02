from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
legacy=root/'handoff/positive-court/legacy-native-history-20261102'
ns=runpy.run_path(str(legacy/'operator-scripts/native-history-controller-20261102.py'))
ns['run'].__globals__['out']=root/'handoff/positive-court/assignment-native-integration-20261102'
extra=json.loads((legacy/'native-legacy-eight-erased-futures.json').read_text())['extra_environment']
common=['--message-format=json-render-diagnostics','--offline','--locked']
r=ns['run']('shared-library-clippy-fifth',['cargo','clippy']+common+['-p','misaka-palw-positive-contract','-p','kaspa-consensus-core','-p','kaspa-consensus','--lib','--','-D','warnings'],1800,4*1024**3,16*1024**3,extra)
assert r['exit_code']==0
