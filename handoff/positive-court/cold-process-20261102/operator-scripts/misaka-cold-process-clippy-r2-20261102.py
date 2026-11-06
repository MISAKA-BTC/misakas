from pathlib import Path
import json,runpy
s=Path('/tmp/misaka-cold-process-controller-20261102.py').read_text().replace("if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2':","if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='positive_wide_material_v2' and e.get('executable'):")
p=Path('/tmp/misaka-cold-process-clippy-controller-20261102.py');p.write_text(s);ns=runpy.run_path(str(p));r=json.loads((ns['out']/'runtime-r5.json').read_text());command=r['command'][:r['command'].index('--')];command[1]='clippy'
r=ns['run']('clippy-r2',command+['--no-deps','--','-D','warnings'],900,600*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None
