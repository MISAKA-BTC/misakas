from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/legacy-cert-aux-20261102';ops=out/'operator-scripts';s=(ops/'fresh-functional-controller-20261102.py').read_text().replace("if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2':","if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2' and e.get('executable'):");p=ops/'fresh-check-controller-20261102.py';p.write_text(s);ns=runpy.run_path(str(p));previous=json.loads((out/'exec-fresh-functional-first.json').read_text());extra=previous['extra_environment'];base=previous['command'][:previous['command'].index('--')];core=[];i=0
while i<len(base):
 if base[i] in ['--test','--features']:i+=2;continue
 core.append('kaspa-consensus-core' if base[i]=='misaka-palw-tir-exec' else base[i]);i+=1
core+=['--test','legacy_certification_aux_codec'];r=ns['run']('core-fresh-functional-first',core+['--','--nocapture','--test-threads=1'],1200,2*1024**3,16*1024**3,extra);assert r['exit_code']==0 and r['stop_reason'] is None
command=list(core);command[1]='clippy';r=ns['run']('core-fresh-clippy-first',command+['--no-deps','--','-D','warnings'],1200,2*1024**3,16*1024**3,extra);assert r['exit_code']==0 and r['stop_reason'] is None
command=list(base);command[1]='clippy';r=ns['run']('exec-fresh-clippy-first',command+['--no-deps','--','-D','warnings'],1200,2*1024**3,16*1024**3,extra);assert r['exit_code']==0 and r['stop_reason'] is None
