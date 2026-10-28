#!/usr/bin/env python3
"""Narrow offline candidate: sender-listed one-input MLDSA87 native transactions only.
No signature/UTXO validation, P2P byte estimate, native executor or service guarantee.
"""
import argparse,gzip,hashlib,json,lzma,zlib
from pathlib import Path
MAX=(1<<64)-1
class Unknown(ValueError):pass
def uint(v,bits=64):
    if type(v) is not int or not 0<=v<(1<<bits):raise Unknown('out-of-range integer')
    return v
def add(a,b):return uint(a+b)
def mul(a,b):return uint(a*b)
def blob(v,n=None):
    if not isinstance(v,str):raise Unknown('non-hex byte field')
    try:b=bytes.fromhex(v)
    except ValueError as e:raise Unknown('invalid hex') from e
    if len(v)!=len(b)*2 or n is not None and len(b)!=n:raise Unknown('incorrect byte width')
    return b
def le(v,n):return uint(v,n*8).to_bytes(n,'little')
def var(b,n=8):return le(len(b),n)+b
def spk(v):
    raw=blob(v)
    if len(raw)!=71 or raw[:2]!=b'\0\0' or raw[2:5]!=bytes.fromhex('76c440') or raw[-2:]!=bytes.fromhex('88a6'):
        raise Unknown('outside sender v0 69-byte P2PKH-MLDSA87 scope')
    return (0,raw[2:])
def normalize(t):
    if uint(t['version'],16)!=0 or uint(t['lockTime'])!=0 or uint(t['gas'])!=0 or blob(t['subnetworkId'],20)!=bytes(20):raise Unknown('outside sender native transaction scope')
    if len(t['inputs'])!=1 or not 1<=len(t['outputs'])<=32:raise Unknown('outside one-input bounded sender scope')
    i=t['inputs'][0];sig=blob(i['signatureScript'])
    if uint(i['sequence'])!=MAX or uint(i['sigOpCount'],8)!=1:raise Unknown('outside sender sequence/sigop grammar')
    if len(sig)!=7226 or sig[:3]!=b'\x4d\x14\x12' or sig[4630]!=1 or sig[4631:4634]!=b'\x4d\x20\x0a':
        raise Unknown('outside MLDSA87 signature/public-key push grammar')
    op=i['previousOutpoint'];prev=(blob(op['transactionId'],64),uint(op['index'],32))
    outputs=[(uint(o['value']),spk(o['scriptPublicKey'])) for o in t['outputs']]
    if any(v==0 for v,_ in outputs):raise Unknown('zero output')
    return dict(prev=prev,sig=sig,outputs=outputs,payload=blob(t['payload']),mass=uint(t['mass']))
def encode_identity(n,full):
    out=le(0,2)+le(1,8)+n['prev'][0]+le(n['prev'][1],4)
    out+=var(n['sig'])+b'\x01' if full else var(b'')
    out+=le(MAX,8)+le(len(n['outputs']),8)
    for value,(version,script) in n['outputs']:out+=le(value,8)+le(version,2)+var(script)
    out+=le(0,8)+bytes(20)+le(0,8)+var(n['payload'])
    if full and n['mass']>0:out+=le(n['mass'],8)
    return out
def txid(n):return hashlib.blake2b(encode_identity(n,False),digest_size=64,key=b'TransactionID64').hexdigest()
def borsh(n):
    out=le(0,2)+le(1,4)+n['prev'][0]+le(n['prev'][1],4)+var(n['sig'],4)+le(MAX,8)+b'\x01'
    out+=le(len(n['outputs']),4)
    for value,(version,script) in n['outputs']:out+=le(value,8)+le(version,2)+var(script,4)
    return out+le(0,8)+bytes(20)+le(0,8)+var(n['payload'],4)+le(n['mass'],8)+bytes.fromhex(txid(n))
def recalc(n,entry,params):
    amount=uint(entry['amount']);_,input_script=spk(entry['scriptPublicKey'])
    # Identity check only. This does NOT verify MLDSA signature validity.
    pub=n['sig'][4634:]
    payload=hashlib.blake2b(pub,digest_size=64,key=b'kaspa-pq-v2/address/mldsa87').digest()
    if input_script[3:67]!=payload:raise Unknown('input public-key identity differs from reconstructed script')
    size=2+8+(64+4+8+len(n['sig'])+8)+8+sum(8+2+8+len(s) for _,(_,s) in n['outputs'])+8+20+8+32+8+len(n['payload'])
    compute=add(add(mul(size,params['mass_per_tx_byte']),mul(sum(2+len(s) for _,(_,s) in n['outputs']),params['mass_per_script_pub_key_byte'])),params['mass_per_sig_op'])
    transient=mul(size,params['transient_byte_to_mass_factor']);c=uint(params['storage_mass_parameter'])
    plurality=lambda s:(95+len(s)+99)//100
    ip=plurality(input_script);op=sum(plurality(s) for _,(_,s) in n['outputs'])
    harmonic=0
    for value,(_,script) in n['outputs']:harmonic=add(harmonic,mul(mul(c,plurality(script)),plurality(script))//value)
    if op==1 or ip==1 or op==ip==2:debit=mul(mul(c,ip),ip)//amount if amount else None
    else:
        mean=amount//ip
        debit=min(MAX,ip*(c//mean)) if mean else None
    if debit is None:raise Unknown('incomputable native storage divisor')
    storage=max(0,harmonic-debit);total=0
    for value,_ in n['outputs']:total=add(total,value)
    if total>amount:raise Unknown('outputs exceed reconstructed input amount')
    return dict(serialized_bytes=len(borsh(n)),serialization='borsh_transaction_v1',compute_mass=compute,
        transient_mass=transient,storage_mass=storage,mass=max(compute,transient,storage),fee_sompi=amount-total,
        native_mass_estimated_size=size,storage_commitment_matches=storage==n['mass'])
def records(path):
    p=Path(path);opener=gzip.open if p.suffix=='.gz' else lzma.open if p.suffix in ('.xz','.lzma') else open
    with opener(p,'rt',encoding='utf-8') as f:
        for line in f:
            if line.strip():yield json.loads(line)
def analyze(sender,observer,roots,params):
    metric_fields=('serialized_bytes','compute_mass','storage_mass','transient_mass','mass','fee_sompi')
    groups={};trace_conflicts=set()
    for event in sender:
        if event.get('ev') not in ('submit_attempt','submit'):continue
        if event.get('phase') not in ('split','load'):raise Unknown('unknown sender trace phase schema')
        tid=blob(event['txid'],64).hex()
        group=groups.setdefault(tid,dict(phase=event['phase'],results=[],attempts=[]))
        if group['phase']!=event['phase']:trace_conflicts.add(tid)
        try:uint(event['sequence'])
        except (KeyError,Unknown):trace_conflicts.add(tid)
        if event['ev']=='submit_attempt':group['attempts'].append(event)
        else:
            try:
                for field in metric_fields:uint(event[field])
                if event.get('serialization')!='borsh_transaction_v1':raise Unknown('unknown sender serialization schema')
            except (KeyError,Unknown):trace_conflicts.add(tid)
            group['results'].append(event)
    for tid,group in groups.items():
        if len({tuple(r.get(field) for field in metric_fields) for r in group['results']})>1:trace_conflicts.add(tid)
    wanted=set(groups);bodies={};conflicts=set();fees={};verbose_compute={}
    for r in observer:
        if r.get('ev')!='rpc':continue
        transactions=[]
        if r.get('method')=='getMempoolEntries':
            for e in r['response'].get('mempoolEntries',[]):
                t=e['transaction'];tid=(t.get('verboseData') or {}).get('transactionId','').lower()
                if tid in wanted and type(e.get('fee')) is int:fees.setdefault(tid,set()).add(e['fee'])
                transactions.append(t)
        elif r.get('method')=='getBlocks':transactions=[t for b in r['response'].get('blocks',[]) for t in b.get('transactions',[])]
        for t in transactions:
            tid=(t.get('verboseData') or {}).get('transactionId','').lower()
            if tid not in wanted:continue
            observed_compute=(t.get('verboseData') or {}).get('computeMass')
            if type(observed_compute) is int and 0<observed_compute<=MAX:
                verbose_compute.setdefault(tid,set()).add(observed_compute)
            try:
                n=normalize(t)
                if txid(n)!=tid:raise Unknown('independently calculated txid mismatch')
                encoded=borsh(n)
                if tid in bodies and bodies[tid][0]!=encoded:conflicts.add(tid)
                else:bodies[tid]=(encoded,n)
            except (Unknown,KeyError,TypeError,ValueError):conflicts.add(tid)
    entries={}
    for row in roots['outputs']:
        op=row['outpoint'];entries[(op['transactionId'].lower(),op['index'])]=row['utxoEntry']
    results=[];consumed=set()
    for tid,group in groups.items():
        item=dict(txid=tid,phase=group['phase'],durable_attempt_records=len(group['attempts']),rpc_result_records=len(group['results']))
        trace=group['results'][0] if group['results'] else None
        try:
            result_sequences={r.get('sequence') for r in group['results']}
            if not group['results'] or any(a.get('sequence') not in result_sequences for a in group['attempts']):
                raise Unknown('durable attempt has no RPC result record')
            if tid in trace_conflicts:raise Unknown('same txid has conflicting/incomplete sender metric records')
            if tid in conflicts:raise Unknown('same txid has conflicting/invalid observed full body')
            if tid not in bodies:raise Unknown('sender body not observed')
            n=bodies[tid][1];previous=(n['prev'][0].hex(),n['prev'][1])
            if previous not in entries:raise Unknown('input entry not reconstructible from observed root/outputs')
            if previous in consumed:raise Unknown('same reconstructed input consumed by different sender transactions')
            metrics=recalc(n,entries[previous],params)
            differences={k:dict(recomputed=v,sender=trace.get(k)) for k,v in metrics.items() if k in ('serialized_bytes','compute_mass','storage_mass','transient_mass','mass','fee_sompi') and trace.get(k)!=v}
            rpc_fees=sorted(fees.get(tid,[]));metrics['rpc_mempool_fee_observations']=rpc_fees
            if any(f!=metrics['fee_sompi'] for f in rpc_fees):differences['mempool_fee']=dict(recomputed=metrics['fee_sompi'],rpc=rpc_fees)
            rpc_compute=sorted(verbose_compute.get(tid,[]));metrics['rpc_positive_verbose_compute_mass_observations']=rpc_compute
            metrics['rpc_verbose_compute_mass_note']='positive observations only; zero/unset is unavailable, not actual zero'
            if any(v!=metrics['compute_mass'] for v in rpc_compute):differences['rpc_verbose_compute_mass']=dict(recomputed=metrics['compute_mass'],rpc=rpc_compute)
            if not metrics['storage_commitment_matches']:differences['storage_commitment']=dict(recomputed=metrics['storage_mass'],committed=n['mass'])
            item.update(status='MATCH' if not differences else 'MISMATCH',metrics=metrics,differences=differences)
            consumed.add(previous)
            # Candidate graph facts only; not accepted/native authenticated UTXO state.
            for ix,(value,(_,script)) in enumerate(n['outputs']):entries[(tid,ix)]=dict(amount=value,scriptPublicKey='0000'+script.hex())
        except (Unknown,KeyError,TypeError,ValueError,ZeroDivisionError) as error:item.update(status='UNKNOWN',reason=str(error))
        results.append(item)
    return dict(scope='one-input sender-listed native MLDSA87 transaction DTO/fee/mass rederivation only',params=params,
        counts={s:sum(r['status']==s for r in results) for s in ['MATCH','MISMATCH','UNKNOWN']},transactions=results,
        limits=['No signature verification or authenticated native UTXO provenance/acceptance audit.',
                'Input map is reconstructed candidate graph anchored to separate observed-funded-root entry, not atomic state.',
                'Exact Transaction Borsh DTO bytes are not P2P framing or block-wire bytes.',
                'MATCH describes reconstructed arithmetic only, including RPC-unknown bodies; never RPC/native admission.'])
def main():
    p=argparse.ArgumentParser()
    for name in ['sender','observer','funded-root','params']:p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args()
    try:result=analyze(list(records(a.sender)),records(a.observer),json.loads(a.funded_root.read_text()),json.loads(a.params.read_text()))
    except (OSError,EOFError,ValueError,KeyError,TypeError,lzma.LZMAError,zlib.error) as error:result=dict(status='INCOMPLETE_EVIDENCE',reason=str(error))
    print(json.dumps(result,indent=2))
if __name__=='__main__':main()
