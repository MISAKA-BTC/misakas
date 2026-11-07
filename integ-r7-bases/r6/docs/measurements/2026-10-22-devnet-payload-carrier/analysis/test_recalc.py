"""Hypothetical DTO fixtures; zero MLDSA signatures are intentionally NOT validated."""
import copy,gzip,hashlib,json,lzma,tempfile,unittest
from pathlib import Path
from recalc import normalize,txid,borsh,recalc,analyze,records,Unknown,MAX
P=dict(mass_per_tx_byte=1,mass_per_script_pub_key_byte=10,mass_per_sig_op=10000,storage_mass_parameter=10**12,transient_byte_to_mass_factor=4)
def fixture():
    pub=bytes(2592);payload=hashlib.blake2b(pub,digest_size=64,key=b'kaspa-pq-v2/address/mldsa87').digest()
    spk='000076c440'+payload.hex()+'88a6'
    signature=b'\x4d\x14\x12'+bytes(4627)+b'\x01'+b'\x4d\x20\x0a'+pub
    t=dict(version=0,lockTime=0,gas=0,subnetworkId='00'*20,mass=404,payload='',
        inputs=[dict(previousOutpoint=dict(transactionId='11'*64,index=0),sequence=MAX,sigOpCount=1,signatureScript=signature.hex())],
        outputs=[dict(value=99_000_000,scriptPublicKey=spk)])
    root=dict(outputs=[dict(outpoint=t['inputs'][0]['previousOutpoint'],utxoEntry=dict(amount=100_000_000,scriptPublicKey=spk,isCoinbase=False))])
    n=normalize(t);tid=txid(n);t['verboseData']=dict(transactionId=tid)
    m=recalc(n,root['outputs'][0]['utxoEntry'],P)
    trace=dict(ev='submit',phase='load',txid=tid,sequence=1,serialization='borsh_transaction_v1',**{k:m[k] for k in ['serialized_bytes','compute_mass','transient_mass','storage_mass','mass','fee_sompi']})
    obs=[dict(ev='rpc',method='getMempoolEntries',response=dict(mempoolEntries=[dict(fee=1_000_000,transaction=t)]))]
    return t,root,trace,obs
class Recalc(unittest.TestCase):
    def test_exact_borsh_length_is_distinct_from_native_mass_estimated_size(self):
        t,root,trace,obs=fixture();m=recalc(normalize(t),root['outputs'][0]['utxoEntry'],P)
        self.assertEqual((m['serialized_bytes'],m['native_mass_estimated_size']),(7512,7491))
        self.assertEqual((m['compute_mass'],m['transient_mass'],m['storage_mass'],m['fee_sompi']),(18201,29964,404,1_000_000))
        self.assertEqual(analyze([trace],iter(obs),root,P)['counts'],dict(MATCH=1,MISMATCH=0,UNKNOWN=0))
    def test_arithmetic_storage_branch_and_checked_overflow(self):
        t,root,_,_=fixture();t['outputs']*=2
        for o in t['outputs']:o['value']=49_000_000
        t['mass']=123264
        self.assertEqual(recalc(normalize(t),root['outputs'][0]['utxoEntry'],P)['storage_mass'],123264)
        too_large=dict(P,storage_mass_parameter=MAX)
        with self.assertRaises(Unknown):recalc(normalize(t),root['outputs'][0]['utxoEntry'],too_large)
    def test_missing_body_input_and_same_id_different_signed_body_are_unknown(self):
        t,root,trace,obs=fixture()
        self.assertEqual(analyze([trace],[],root,P)['counts']['UNKNOWN'],1)
        self.assertEqual(analyze([trace],obs,dict(outputs=[]),P)['counts']['UNKNOWN'],1)
        altered=copy.deepcopy(t);sig=bytearray.fromhex(altered['inputs'][0]['signatureScript']);sig[100]^=1
        altered['inputs'][0]['signatureScript']=sig.hex()
        self.assertEqual(txid(normalize(altered)),trace['txid'])
        obs.append(dict(ev='rpc',method='getBlocks',response=dict(blocks=[dict(transactions=[altered])])))
        self.assertEqual(analyze([trace],obs,root,P)['counts']['UNKNOWN'],1)
    def test_fee_or_mass_disagreement_is_not_silently_a_match(self):
        _,root,trace,obs=fixture();trace['fee_sompi']+=1
        self.assertEqual(analyze([trace],obs,root,P)['counts']['MISMATCH'],1)
    def test_plain_gzip_xz_are_lossless_iterable_representations(self):
        _,_,_,obs=fixture()
        with tempfile.TemporaryDirectory() as d:
            for extension,opener in [('.jsonl',open),('.jsonl.gz',gzip.open),('.jsonl.xz',lzma.open)]:
                p=Path(d)/('fixture'+extension)
                with opener(p,'wt') as out:out.write(json.dumps(obs[0])+'\n')
                self.assertEqual(list(records(p)),obs)
    def test_inflight_attempt_remains_unknown_with_or_without_observed_body(self):
        _,root,trace,obs=fixture();attempt=dict(ev='submit_attempt',phase='load',txid=trace['txid'],sequence=1)
        for observation in [[],obs]:
            r=analyze([attempt],observation,root,P)
            self.assertEqual(r['counts']['UNKNOWN'],1)
            self.assertIn('no RPC result',r['transactions'][0]['reason'])
        retry=dict(attempt,sequence=2)
        self.assertEqual(analyze([trace,retry],obs,root,P)['counts']['UNKNOWN'],1)
    def test_conflicting_duplicate_trace_metrics_are_not_skipped(self):
        _,root,trace,obs=fixture();other=dict(trace,fee_sompi=trace['fee_sompi']+1)
        r=analyze([trace,other],obs,root,P)
        self.assertEqual(r['counts']['UNKNOWN'],1)
        self.assertIn('sender metric',r['transactions'][0]['reason'])
    def test_positive_verbose_compute_is_checked_and_zero_or_none_is_unavailable(self):
        t,root,trace,obs=fixture()
        for value in [None,0,trace['compute_mass']]:
            t['verboseData']['computeMass']=value
            result=analyze([trace],obs,root,P)
            self.assertEqual(result['counts']['MATCH'],1)
            self.assertEqual(result['transactions'][0]['metrics']['rpc_positive_verbose_compute_mass_observations'],
                [value] if value else [])
        t['verboseData']['computeMass']=trace['compute_mass']+1
        self.assertEqual(analyze([trace],obs,root,P)['counts']['MISMATCH'],1)
if __name__=='__main__':unittest.main()
