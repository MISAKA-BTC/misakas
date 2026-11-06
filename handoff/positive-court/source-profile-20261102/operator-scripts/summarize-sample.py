from pathlib import Path
import json,re
out=Path(__file__).resolve().parent.parent
lines=(out/'cpu-sample.txt').read_text().split('Call graph:\n',1)[1].split('Total number in stack',1)[0].splitlines()
patterns={'producer_material':'WidePanelBuilderV2::material','producer_walk':'trace::discover','producer_push':'trace::Walk::push','table_growth':'reserve_rehash','source_auth':'authenticate_source_only_v2','binding_verify':'verify_tir_binding_v2','range_analysis':'analyze_ranges','node_submit':'StreamFullRelationCheckerV2','direct_evaluation':'eval_demanded','file_read':'FileBytes','hash_one':'BuildHasher::hash_one'}
stack=[];worker=False;rows=[];worker_samples=None
for line_number,l in enumerate(lines):
 m=re.match(r'([ +!:|`\-]*)(\d+) (.*)',l)
 if not m:continue
 indent=len(m[1]);n=int(m[2]);name=m[3]
 if name.startswith('Thread_'):
  worker='trained_roles::' in name
  if worker:worker_samples=n
 while stack and stack[-1][0]>=indent:stack.pop()
 if worker:
  for k,v in patterns.items():
   if v in name and not any(v in a[2] for a in stack):rows.append({'category':k,'samples':n,'frame':name})
 stack.append((indent,n,name))
print(json.dumps({'worker_samples':worker_samples,'scope':'inclusive non-nested category samples in the trained worker only; categories overlap and must not be added; one diagnostic window, not CPU seconds or full-job frequency','totals':{k:sum(r['samples'] for r in rows if r['category']==k) for k in patterns},'rows':rows},indent=2))
