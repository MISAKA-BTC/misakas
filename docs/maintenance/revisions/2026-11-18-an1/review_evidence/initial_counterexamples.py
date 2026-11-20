from pathlib import Path
import sys, copy, json, hashlib
from dataclasses import replace
root=Path('/workspace/scratch/e55b1a86f957/rfc_work/current/docs/maintenance/revisions/2026-11-18-an1/reference_impl')
sys.path.insert(0,str(root))
import an1_reference as a

def run_case(label, count_float=False, oversized=False):
 c, sigma, job, weights=a.fixture_context()
 original, material=a.build_claim(c,sigma,job,weights)
 material=copy.deepcopy(material)
 material.trace_records[0]=(b' '*513 if oversized else a.encode({'i':0,'v':1}))
 rt=a.Tree('trace',original.attempt,material.trace_records)
 bad=replace(original,trace_root=rt.root,trace_count=float(original.trace_count) if count_float else original.trace_count)
 ledger=a.Ledger({(c['beacon_key'],c['beacon_round']):(sigma,10)})
 slot=ledger.register(c,job,0,c['owner'])
 try:
  ledger.seal(slot,bad,20,c['owner'])
 except Exception as e:
  return {'case':label,'seal':'REJECTED','reason':str(e)}
 panel=ledger.select_panel(slot,bytes.fromhex('44'*32),100,[f'actor-{i}' for i in range(5)])
 for actor in panel[:4]:ledger.approve(slot,actor)
 native=ledger.native(slot)
 review=a.panel_review(native,material)
 row=ledger.rows[slot]
 row['materials_ready']=True
 row['closed']=True
 row['delivered']=True
 outcome=ledger.apply_batch(slot,[review['court']] if 'court' in review else [],True,110)
 return {'case':label,'seal':'ACCEPTED','trace_count_type':type(bad.trace_count).__name__,'panel_review':review['status'],'batch':outcome,'credits':ledger.credits}

result={'script_under_review_sha256':hashlib.sha256((root/'an1_reference.py').read_bytes()).hexdigest(),'cases':[run_case('authenticated wrong trace with float count',count_float=True),run_case('authenticated oversized trace leaf',oversized=True)]}
print(json.dumps(result,ensure_ascii=False,indent=2))
Path('/workspace/scratch/e55b1a86f957/tmp/an1_review_counterexamples_results.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
