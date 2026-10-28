from pathlib import Path
import re,subprocess,importlib.util,json
root=Path.cwd();sp=importlib.util.spec_from_file_location('h',root/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
def close(s,a,left='(',right=')'):
 d=1;i=a+1
 while d:d+=(s[i]==left)-(s[i]==right);i+=1
 return i

def funcs(s):
 m=h.mask(s);result=[]
 for x in re.finditer(r'\bfn\s+(\w+)\s*\(',m):
  end=close(m,x.end()-1);a=m.find('{',end)
  if a<0 or ';' in m[end:a]:continue
  b=close(m,a,'{','}');result.append((x[1],x.start(),b,s[x.start():b]))
 return result

def where(functions,at):
 found=[(b-a,n) for n,a,b,_ in functions if a<=at<b];return min(found)[1]
def norm(s):
 # Source contracts below contain no literal comment markers, so only actual line comments are removed.
 s=re.sub(r'//[^\n]*','',s);return re.sub(r',([\)\]}>])',r'\1',re.sub(r'\s+','',s))
rows=[];old_routes=[];new_routes=[];unchanged=[]
for path in ['kaspad/src/palw_panel.rs','kaspad/src/palw_panel/improve.rs']:
 old=subprocess.check_output(['git','show','ccb847770:'+path],text=True);new=Path(path).read_text();of,nf=funcs(old),funcs(new)
 for side,text,functions,pattern,output in [('before',old,of,r'let txid = tx\.id\(\);\s*let change = tx\.outputs\[0\]\.clone\(\);',old_routes),('after',new,nf,r'let txid = prepared_carrier\.id\(\);',new_routes)]:
  m=h.mask(text);seen={}
  for x in re.finditer(pattern,m,re.S):
   fn=where(functions,x.start());seen[fn]=seen.get(fn,0)+1
   a=m.index('match ',x.end());b=m.index('{',a);end=close(m,b,'{','}');prefix=m[a:b];body=m[b:end]
   if side=='before':
    route='replacement' if 'submit_rpc_transaction_replacement' in prefix else 'overlay' if 'submit_carrier_v1' in prefix else 'direct' if 'submit_rpc_transaction' in prefix else 'UNKNOWN'
    if 'inflight.saturating_add(1)' in body:count='saturating increment'
    elif re.search(r'\*?inflight\s*=\s*MAX_INFLIGHT_CARRIERS',body):count='set MAX_INFLIGHT_CARRIERS'
    elif 'lane.inflight = 1' in body:count='set 1'
    elif re.search(r'\*?inflight\s*\+=\s*1',body):count='checked increment'
    else:count='UNKNOWN'
   else:
    route='replacement' if 'submit_prepared_replacement_v1' in prefix else 'overlay' if 'submit_prepared_carrier_v1' in prefix else 'direct' if 'submit_prepared_direct_v1' in prefix else 'UNKNOWN'
    if 'CarrierInFlightUpdateV1::SaturatingIncrement' in body:count='saturating increment'
    elif 'CarrierInFlightUpdateV1::Set(MAX_INFLIGHT_CARRIERS)' in body:count='set MAX_INFLIGHT_CARRIERS'
    elif 'CarrierInFlightUpdateV1::Set(1)' in body:count='set 1'
    elif 'CarrierInFlightUpdateV1::Increment' in body:count='checked increment'
    else:count='UNKNOWN'
   persist='conditional main-only' if 'if !lane {' in body else 'main file' if 'self.persist_fee_outpoint(next)' in body else 'none'
   assert 'UNKNOWN' not in (route,count),(path,fn,prefix)
   output.append({'file':path,'function':fn,'site':seen[fn],'route':route,'counter':count,'persistence':persist})
 # Low-level transport, pricing, and funding scan remain source-identical (ignoring formatting).
 if path.endswith('/palw_panel.rs'):
  od={n:t for n,_,_,t in of};nd={n:t for n,_,_,t in nf}
  for name in ['submit_carrier_v1','open_stuck_carrier_v1','build_lifecycle_tx_priced_v1','resolve_fee_funding','resolve_readiness_lane_funding_v1','persist_fee_outpoint']:
   assert norm(od[name])==norm(nd[name]),name
   unchanged.append(name)
assert old_routes==new_routes,(old_routes,new_routes)
assert len(new_routes)==16
shared=Path('kaspad/src/palw_panel/carrier_chain.rs').read_text();ff=funcs(shared)
book_record=next(t for n,_,_,t in ff if n=='record_change');body=book_record[book_record.index('{')+1:book_record.rfind('}')]
expected='''
let next = TransactionOutpoint::new(txid, change_index);
persist(next);
*self.funding = Some((next, UtxoEntry { amount: change.value, script_public_key: change.script_public_key, block_daa_score: submitted_daa, is_coinbase: false, }));
match count {
CarrierInFlightUpdateV1::Increment => *self.inflight += 1,
CarrierInFlightUpdateV1::SaturatingIncrement => *self.inflight = self.inflight.saturating_add(1),
CarrierInFlightUpdateV1::Set(inflight) => *self.inflight = inflight,
}
next
'''
assert norm(body)==norm(expected),'recorder order/values'
prepared=next(t for n,_,_,t in ff if n=='new' and 'tx:' in t[:t.index('{')]);assert prepared.index('let txid = tx.id();')<prepared.index('let output = tx.outputs[index as usize].clone();')
assert sum(Path(p).read_text().count('match self.prepare_lifecycle_carrier_v1(') for p in ['kaspad/src/palw_panel.rs','kaspad/src/palw_panel/improve.rs'])==11
report={'base':'ccb847770','matched_submission_sites':16,'sites':new_routes,'unchanged_low_level_functions':unchanged,'shared_lifecycle_build_sites':11,'recorder':'Exact old field values and persist -> funding -> counter order, with all three old counter operations explicitly retained.','scope':'Targeted source-contract inventory; not a complete function-body or runtime equivalence proof.'}
Path('/tmp/carrier-contract-audit.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
