from pathlib import Path
import subprocess,importlib.util,re,json
root=Path.cwd();sp=importlib.util.spec_from_file_location('h',root/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
base='50d64cf58';path='consensus/src/pipeline/virtual_processor/processor/palw_validate.rs';old=subprocess.check_output(['git','show',base+':'+path],text=True)
def parse(s):
 m=h.mask(s);start=m.index('match object {')+len('match object {');i=start;arms=[]
 def close(a):
  d=1;i=a+1
  while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
  return i
 while True:
  a=i+re.search(r'\S',m[i:]).start()
  if not m[a:].startswith('Obj::'):break
  arrow=m.index('=>',a);b=arrow+2+re.search(r'\S',m[arrow+2:]).start();end=close(b if m[b]=='{' else m.index('{',b));end+=m[end:end+1]==','
  variants=re.findall(r'Obj::(\w+)',m[a:arrow]);arms.append((tuple(variants),m[a:arrow],s[i:end]));i=end
 return arms,start,close(start-1)
original,start,end=parse(old);draft=root/'docs/evidence/palw/refactor-2026-10-21/validator-family-inventory.json';inventory=json.loads(draft.read_text());seen=[];moved=[]
def norm(s):return re.sub(r'\s+','',s)
for row in inventory:
 p=Path(path).with_suffix('')/(row['family']+'.rs');new=p.read_text();arms,_,_=parse(new);expected=[a for a in original if a[0][0] in row['variants']];assert len(expected)==len(arms)==row['arms']
 for (ovs,op,ob),(nvs,np,nb) in zip(expected,arms):
  assert ovs==nvs and norm(op)==norm(np),(row['family'],ovs,nvs)
  expected_body=re.sub(r'\bcontinue\s*;', 'return Ok(());',ob)
  assert norm(expected_body)==norm(nb),(row['family'],ovs)
  # There is no control-flow keyword hidden in a literal/comment that the substitution rewrites.
  assert len(re.findall(r'\bcontinue\s*;',ob))==len(re.findall(r'\bcontinue\s*;',h.mask(ob)))
  moved.append(ovs)
 used=[v for v in ['state','state_params','point','panel_params'] if re.search(r'\b'+v+r'\b',h.mask(''.join(a[2] for a in expected)))];assert used==row['inputs'];seen+=row['variants']
assert len(moved)==len(original)==133
new=Path(path).read_text();prefix=old[:start];nstart=new.index('match object {')+len('match object {');npre=new[:nstart]
for row in inventory:npre=npre.replace('mod '+row['family']+';\n','')
assert norm(prefix)==norm(npre),'shared network/receipt precheck changed'
# The parent match is exhaustive and routes each unique original variant once.
parent_variants=re.findall(r'Obj::(\w+)\s*(?:\{|\()',h.mask(new[nstart:]));assert len(parent_variants)==len(set(parent_variants));assert set(parent_variants)==set(seen)
# Every transformed continue targets the original per-object loop, not a nested loop.
om=h.mask(old)
def close_mask(m,a):
 d=1;i=a+1
 while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
 return i
returns=re.findall(r'\breturn\s+(\w+)',om[om.index('match object {'):]);assert set(returns)=={'Err'},'an old successful early return would not be a helper success'
loops=[(x.start(),close_mask(om,om.index('{',x.end()))) for x in re.finditer(r'\bfor\s+',om)]
for x in re.finditer(r'\bcontinue\s*;',om):assert sum(a<x.start()<b for a,b in loops)==1
nm=h.mask(new);cursor=nstart;by_family={x['family']:x for x in inventory};routed=[]
for call in re.finditer(r'=> self\.palw_validate_(\w+)_object_v1\(([^)]*)\)\?',nm[nstart:]):
 a=nstart+call.start();b=nstart+call.end();row=by_family[call[1]]
 vs=re.findall(r'Obj::(\w+)\s*(?:\{|\()',nm[cursor:a]);assert vs==row['variants'],call[1]
 assert [x.strip() for x in call[2].split(',')]==row['inputs']+['object'],call[1]
 routed.append(call[1]);cursor=b
assert len(routed)==len(inventory)
nend=close_mask(nm,nstart-1);assert norm(old[end:])==norm(new[nend:]),'outer object loop/success changed'
report={'base':base,'families':len(inventory),'original_arms_projected':len(moved),'unique_variants_routed':len(parent_variants),'old_object_loop_continues_to_helper_success':sum(x['old_outer_continues'] for x in inventory),'old_explicit_returns_all_error':len(returns),'shared_entry_precheck':'normalized unchanged; remains before dispatch for every object','handler_inputs':[{k:x[k] for k in ['family','inputs','arms']} for x in inventory],'limits':'Exact normalized arm projection except the 16 outer-loop continues now return helper success; not exhaustive runtime equivalence. Group fallback reachability is checked against exhaustive parent variant routing and compilation.'}
Path('/tmp/validator-split-projection.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
