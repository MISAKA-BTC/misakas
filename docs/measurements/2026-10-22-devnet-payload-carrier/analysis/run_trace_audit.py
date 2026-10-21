#!/usr/bin/env python3
"""Final frozen evidence only; uses exact runtime audit snapshot and XZ reader."""
import sys,json,hashlib
from pathlib import Path
HERE=Path(__file__).resolve().parent
BASE=HERE.parent
sys.path.insert(0,str(HERE))
from recalc import records
sys.path.insert(0,str(BASE/'runtime-python-source'/'scripts'/'drills'/'payload_load'))
# Runtime snapshot layout is checked below, never silently use current mutable source.
if not (BASE/'runtime-python-source'/'scripts'/'drills'/'payload_load'/'audit.py').exists():
    sys.path.insert(0,str(BASE/'runtime-python-source'))
import audit
sender=list(records(BASE/'raw'/'sender-events.jsonl.xz'))
load={r['txid'].lower() for r in sender if r.get('ev')=='submit_attempt' and r.get('phase')=='load'}
start=next(r for r in sender if r.get('ev')=='measurement_started')['t_unix_ms']/1000
end=next(r for r in sender if r.get('ev')=='measurement_ended')['t_unix_ms']/1000
snapshots=[]
def observations():
    for row in records(BASE/'raw'/'observer.jsonl.xz'):
        if row.get('ev')=='rpc' and row.get('method')=='getMempoolEntries':
            entries=row['response'].get('mempoolEntries',[])
            ids={(e.get('transaction',{}).get('verboseData') or {}).get('transactionId','').lower() for e in entries}
            snapshots.append(dict(utc=row['utc'],phase='preparation' if row['utc']<start else 'load' if row['utc']<=end else 'drain',all_entries=len(entries),load_ids=len(load & ids)))
        yield row
result=audit.audit(sender,observations())
result['audit_source_path']=str(Path(audit.__file__).relative_to(BASE))
result['audit_source_sha256']=hashlib.sha256(Path(audit.__file__).read_bytes()).hexdigest()
(BASE/'trace-audit.json').write_text(json.dumps(result,indent=2)+'\n')
queue=dict(scope='Separate sampled RPC snapshots, not an atomic queue, exact arrival clock, FIFO guarantee or native service reservation',load_started_utc=start,load_ended_utc=end,snapshots=snapshots)
for phase in ['preparation','load','drain']:
    rows=[r for r in snapshots if r['phase']==phase]
    queue[phase]=dict(samples=len(rows),maximum_observed_load_ids=max((r['load_ids'] for r in rows),default=None),first=rows[0] if rows else None,last=rows[-1] if rows else None)
(BASE/'queue-observations.json').write_text(json.dumps(queue,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k not in ['paired_observed_latencies','measurement_daa_observations','unobserved_attempt_ids']},indent=2))
print(json.dumps({k:v for k,v in queue.items() if k!='snapshots'},indent=2))
