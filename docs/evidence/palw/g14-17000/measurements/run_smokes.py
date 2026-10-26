#!/usr/bin/env python3
"""One shared slot for the initial native pair and real-model resource preflight."""
import subprocess,sys,json
from pathlib import Path
root=Path('/Users/wata/.codex/artifacts/g14-17000-measure')
commands=[
 [sys.executable,str(root/'run_native_measure.py'),'--build-record',str(root/'native-verified-build-record.json'),
  '--out',str(root/'native-smoke-retry-2026-10-26'),'--case','honest','--runs','1','--allow-under-load'],
]
for n in [0,32]:
 commands.append([sys.executable,str(root/'run_model_measure.py'),'--build-record',str(root/'dolphin-build-record.json'),
                  '--out',str(root/f'dolphin-preflight-n{n}-2026-10-26'),'--positions',str(n),'--timeout-seconds','1800'])
failed=[]
for cmd in commands:
 output=Path(cmd[cmd.index('--out')+1])
 if output.exists():
  prior=json.loads((output/'raw.json').read_text())
  status=prior['status']
  print('EXISTING',output,status,flush=True)
  rc=0 if status in ['UNDER_LOAD_FIXTURE_ONLY','FIXTURE_ONLY','UNDER_LOAD_RUNTIME_ONLY','RUNTIME_ONLY'] else 1
 else:
  print('RUN',cmd,flush=True)
  rc=subprocess.run(cmd).returncode
 if rc:failed.append((cmd,rc))
print('FAILURES',failed,flush=True)
raise SystemExit(bool(failed))
