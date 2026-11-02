from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/legacy-native-history-20261102';ns=runpy.run_path(str(out/'operator-scripts/native-history-controller-20261102.py'));old=json.loads((root/'handoff/positive-court/legacy-cert-aux-20261102/exec-fresh-functional-first.json').read_text());base=old['command'][:old['command'].index('--')];command=[];i=0
while i<len(base):
 if base[i] in ['--test','--features']:i+=2;continue
 command.append('kaspa-consensus' if base[i]=='misaka-palw-tir-exec' else base[i]);i+=1
r=ns['run']('native-cert-first',command+['--lib','--','g14_cert_kernel_route_','--nocapture','--test-threads=1'],1800,4*1024**3,16*1024**3,old['extra_environment']);assert r['exit_code']==0 and r['stop_reason'] is None
