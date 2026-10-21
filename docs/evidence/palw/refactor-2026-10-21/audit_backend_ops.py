from pathlib import Path
import re,json,subprocess,importlib.util
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
base='7b59c3489'
def old(p):return subprocess.check_output(['git','show',base+':'+p],text=True)
def close(m,a,left='(',right=')'):
 d=1;i=a+1
 while d:d+=(m[i]==left)-(m[i]==right);i+=1
 return i
core='consensus/core/src/palw_backend.rs';source=Path(core).read_text();before=old(core);assert source.startswith(before),'compatibility trait or old test body changed'
assert source[len(before):].startswith('\nmod operations;\npub use operations::{')
# Reuse only the generator's parser, never its mutations.
parse=(r/'docs/evidence/palw/refactor-2026-10-21/backend_ops_parser.py').read_text();env={};exec(parse,env);original={x['name']:x for x in env['methods']}
ops=Path('consensus/core/src/palw_backend/operations.rs').read_text();mask=h.mask(ops);checked=[]
for x in re.finditer(r'pub fn (\w+)\(',mask):
 a=x.start();op=mask.index('(',a);ae=close(mask,op);b=mask.index('{',ae);e=close(mask,b,'{','}');name=x[1];body=ops[b+1:e-1];norm=lambda s:re.sub(r'\s+','',s)
 if name=='new':assert norm(body)=='Self{backend}';continue
 item=original[name];signature=ops[a+4:b].strip();assert norm(signature)==norm(item['signature']),name
 assert norm(body)==norm('self.backend.'+name+'('+', '.join(item['args'])+')'),name
 checked.append(name)
inventory=json.loads((r/'docs/evidence/palw/refactor-2026-10-21/backend-ops-inventory.json').read_text());assert len(checked)==97;assert set(checked)==set(original)-set(inventory['retained_compatibility_hooks'])
port=json.loads((r/'docs/evidence/palw/refactor-2026-10-21/backend-ops-port.json').read_text());files=sorted({x['file'] for x in port});projected=[]
for path in files:
 s=Path(path).read_text();m=h.mask(s);edits=[]
 for x in re.finditer(r'kaspa_consensus_core::palw_backend::Palw(?:Execution|Replay|Material|CourtEvidence|Resource)OpsV1::new\(',m):
  op=m.index('(',x.start());e=close(m,op);edits.append((x.start(),e,s[op+1:e-1]))
 for row in port:
  if row['file']!=path or row['operation']!='parameter':continue
  name=row['function'];prior=old(path);pm=h.mask(prior);fa=pm.index('fn '+name);popen=pm.index('(',fa);pend=close(pm,popen);phead=prior[popen:pend];oldparam=re.search(r'backend:\s*&dyn\s+(?:kaspa_consensus_core::palw_backend::)?PalwExecutionBackendV1',phead)[0]
  fa=m.index('fn '+name);op=m.index('(',fa);ae=close(m,op);pnew=re.search(r"backend:\s*kaspa_consensus_core::palw_backend::Palw\w+OpsV1<'_>",s[op:ae]);assert pnew,(path,name);edits.append((op+pnew.start(),op+pnew.end(),oldparam))
 for a,b,t in sorted(edits,reverse=True):s=s[:a]+t+s[b:]
 assert s==old(path),path
 projected.append(path)
report={'base':base,'original_trait_methods':len(original),'original_trait_and_old_tests_byte_prefix_unchanged':True,'operation_forwarders_checked':len(checked),'unique_methods_forwarded':len(set(checked)),'retained_compatibility_hooks':inventory['retained_compatibility_hooks'],'node_files_exact_after_inverse_parameter_and_factory_substitution':projected,'parameter_and_argument_sites':len(port),'scope':'Source forwarding/caller projection only. Runtime and crate type checks are reported separately; this is not exhaustive execution equivalence.'}
Path('/tmp/palw-backend-ops-source-audit.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
