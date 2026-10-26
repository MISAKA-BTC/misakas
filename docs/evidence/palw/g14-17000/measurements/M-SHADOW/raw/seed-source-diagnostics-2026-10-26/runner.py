import datetime,hashlib,json,os,subprocess
from pathlib import Path
r=Path('/Users/wata/.codex/artifacts/g14-17000-measure')
b=json.loads((r/'native-postcommit-build-record.json').read_text())
binary=b['binary']['path']
assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==b['binary']['sha256']
out=r/'seed-source-diagnostics-2026-10-26';out.mkdir(exist_ok=False)
cmd=['/usr/bin/time','-l',binary,'canonical::seed::lane_seed_','--nocapture','--test-threads=1']
j={'build':b,'command':cmd,'utc_start':datetime.datetime.now(datetime.timezone.utc).isoformat(),'load_before':os.getloadavg(),'arming_evidence':False}
with (out/'stdout.log').open('w') as stdout,(out/'stderr.log').open('w') as stderr:
 p=subprocess.run(cmd,stdout=stdout,stderr=stderr,env=dict(os.environ,TMPDIR='/private/tmp'),timeout=300)
j.update(exit_code=p.returncode,load_after=os.getloadavg())
for key in ['stdout','stderr']:j[key+'_sha256']=hashlib.sha256((out/(key+'.log')).read_bytes()).hexdigest()
(out/'raw.json').write_text(json.dumps(j,indent=2)+'\n')
print(j['exit_code'],flush=True)
raise SystemExit(p.returncode)
