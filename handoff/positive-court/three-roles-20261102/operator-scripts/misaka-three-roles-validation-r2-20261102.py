from pathlib import Path
import json,runpy
ns=runpy.run_path('/tmp/misaka-three-roles-controller-20261102.py');root=ns['root'];command=json.loads((root/'handoff/positive-court/cold-process-20261102/runtime-r5.json').read_text())['command'];r=ns['run']('runtime-r2',command,900,1000*1024**2,16*1024**3);assert r['exit_code']==0 and r['stop_reason'] is None
