"""Hypothetical structured fixtures; no transaction, node or RPC is executed."""
import unittest
from audit import audit
H=lambda n:f'{n:0128x}'
def start():return dict(ev='measurement_started',t_unix_ms=1000,elapsed_ms=1000,target_tps=1,target_count=10,confirmed_utxos=10,duration_secs=10)
def end(success=True):return dict(ev='measurement_ended',t_unix_ms=11000,elapsed_ms=11000,success=success,reason=None)
def send(n=1,outcome='accepted',sequence=1):
    return dict(ev='submit',phase='load',sequence=sequence,txid=H(n),rpc_txid=H(n) if outcome=='accepted' else None,
        t_unix_ms=1500,success=outcome=='accepted',rpc_outcome=outcome,payload_bytes=17408,serialized_bytes=25000,
        serialization='borsh_transaction_v1',mass=100,compute_mass=100,storage_mass=80,transient_mass=90)
def trace(*sends,success=True):
    out=[start()]
    for r in sends:
        out.append(dict(ev='submit_attempt',phase='load',sequence=r['sequence'],txid=r['txid'],t_unix_ms=1400,payload_bytes=17408))
        out.append(r)
    return out+[end(success)]
def accepted(tx=1,block=20,daa=50,t=4):return dict(ev='selected_chain_acceptance',txid=H(tx),accepting_block=H(block),accepting_daa=daa,utc=t)
def removed(block):return dict(ev='rpc',method='getVirtualChainFromBlock',response={'removedChainBlockHashes':[H(block)]})
def observe(*events):
    return [dict(ev='start',observer_schema=1,utc=0),
        dict(ev='rpc',method='getBlockDagInfo',utc=0,response={'virtualDaaScore':40}),
        dict(ev='rpc',method='getMempoolEntries',utc=0,response={'mempoolEntries':[]}),
        dict(ev='rpc',method='getBlocks',utc=0,response={'blocks':[]}),
        dict(ev='rpc',method='getVirtualChainFromBlock',utc=0,response={'removedChainBlockHashes':[]}),
        *events,dict(ev='rpc',method='getBlockDagInfo',utc=12,response={'virtualDaaScore':60}),
        dict(ev='end',observer_schema=1,utc=13)]
class Audit(unittest.TestCase):
    def test_success_rejected_unknown_are_distinct_and_false_is_never_rejection(self):
        r=audit(trace(send(),send(2,'rejected',2)),observe())
        self.assertTrue(r['prerequisites_pass']);self.assertEqual(r['actual_rpc_successes'],1)
        self.assertEqual(r['known_rpc_rejected_ids'],[H(2)]);self.assertEqual(r['achieved_unique_rpc_success_tps'],.1)
        for reason in ['timeout','transport error','RPC id mismatch']:
            unknown=send(2,'unknown',2);unknown['error']=reason
            r=audit(trace(send(),unknown,success=False),observe(accepted(2)))
            self.assertFalse(r['prerequisites_pass']);self.assertIsNone(r['actual_rpc_successes'])
            self.assertEqual(r['known_matching_rpc_successes'],1);self.assertEqual(r['known_rpc_rejected_ids'],[])
            self.assertEqual(r['rpc_unknown_but_chain_observed_ids'],[H(2)])
    def test_detach_and_reaccept_use_full_block_identity_at_same_daa(self):
        r=audit(trace(send(),send(2,sequence=2)),observe(accepted(),accepted(2,21,50),removed(20),accepted(1,22,50,t=6)))
        self.assertEqual(r['selected_chain_accepted_attempt_ids'],2);self.assertEqual(r['unobserved_attempt_ids'],[])
        r=audit(trace(send(),send(2,sequence=2)),observe(accepted(),accepted(2,21,50),removed(20)))
        self.assertEqual(r['selected_chain_accepted_attempt_ids'],1);self.assertEqual(r['unobserved_attempt_ids'],[H(1)])
    def test_duplicate_success_id_does_not_double_count_or_double_count_borsh_bytes(self):
        r=audit(trace(send(),send(1,sequence=2)),observe(accepted(),accepted()))
        self.assertTrue(r['prerequisites_pass']);self.assertEqual(r['actual_rpc_successes'],1)
        self.assertEqual((r['rpc_success_attempts'],r['duplicate_success_attempts']),(2,1))
        self.assertEqual(r['known_unique_borsh_dto_bytes'],25000)
        self.assertIn('not comparable',r['bytes_scope'])
    def test_missing_malformed_and_inflight_are_unknown_not_zero(self):
        malformed=dict(ev='submit',phase='load');inflight=dict(ev='submit_attempt',phase='load',sequence=1,txid=H(1))
        for data in [[],[start()],[start(),malformed,end()],[start(),inflight,end(False)]]:
            r=audit(data,observe());self.assertFalse(r['prerequisites_pass']);self.assertIsNone(r['actual_rpc_successes'])
        r=audit([start(),inflight,end(False)],observe(accepted()))
        self.assertEqual(r['rpc_unknown_but_chain_observed_ids'],[H(1)])
    def test_conflicting_facts_reused_sequence_gap_unknown_encoding_and_mass_fail(self):
        for field,value in [('mass',101),('serialized_bytes',25001)]:
            changed=send(1,sequence=2);changed[field]=value
            self.assertFalse(audit(trace(send(),changed),observe())['prerequisites_pass'])
        self.assertFalse(audit(trace(send(),send(2,sequence=1)),observe())['prerequisites_pass'])
        self.assertFalse(audit(trace(send(),send(2,sequence=3)),observe())['prerequisites_pass'])
        changed=send();changed['serialization']='p2p_undefined'
        self.assertFalse(audit(trace(changed),observe())['prerequisites_pass'])
    def test_observation_keeps_carrying_accepting_and_mempool_daa_separate(self):
        mem=dict(ev='first_mempool_observation',txid=H(1),observed_daa=49,utc=2)
        carrying=dict(ev='rpc',method='getBlocks',response={'blocks':[dict(header={'daaScore':48},verboseData={'hash':H(30)},
            transactions=[{'verboseData':{'transactionId':H(1)}}])]})
        r=audit(trace(send()),observe(mem,carrying,accepted()));pair=r['paired_observed_latencies'][0]
        self.assertEqual((pair['first_mempool_observed_daa'],pair['accepting_daa']),(49,50))
        self.assertEqual(pair['carrying_observations'][0]['carrying_header_daa'],48)
    def test_mismatched_response_is_not_rpc_acceptance(self):
        changed=send();changed['rpc_txid']=H(3)
        r=audit(trace(changed),observe(accepted()));self.assertFalse(r['prerequisites_pass'])
        self.assertEqual(r['known_matching_rpc_successes'],0);self.assertEqual(r['known_rpc_rejected_ids'],[])
        self.assertEqual(r['rpc_unknown_but_chain_observed_ids'],[H(1)])
    def test_missing_error_and_partial_observer_are_unknown_despite_complete_sender(self):
        for observation in [[],observe()[:-1],observe(dict(ev='rpc_error',error='timeout')),
                            [r for r in observe() if r.get('method')!='getMempoolEntries']]:
            r=audit(trace(send()),observation)
            self.assertTrue(r['sender_trace_complete']);self.assertFalse(r['observer_complete'])
            self.assertFalse(r['prerequisites_pass']);self.assertIsNone(r['selected_chain_accepted_attempt_ids'])
            self.assertIsNone(r['unobserved_attempt_ids'])
        clipped=observe();clipped[1]['utc']=2
        self.assertFalse(audit(trace(send()),clipped)['observer_complete'])
    def test_rate_uses_monotonic_elapsed_and_bad_monotonic_is_unknown(self):
        data=trace(send());data[-1]['t_unix_ms']=500
        result=audit(data,observe())
        self.assertEqual(result['achieved_unique_rpc_success_tps'],.1)
        self.assertFalse(result['observer_complete'],'cross-process UTC pairing is ambiguous after clock jump')
        for end_elapsed in [None,0]:
            data=trace(send())
            if end_elapsed is None:del data[-1]['elapsed_ms']
            else:data[-1]['elapsed_ms']=end_elapsed
            result=audit(data,observe())
            self.assertIsNone(result['achieved_unique_rpc_success_tps'])
    def test_initial_pool_target_is_not_achieved_or_nominal_offered_count(self):
        data=trace(send());data[0]['target_count']=20;data[0]['confirmed_utxos']=20
        r=audit(data,observe())
        self.assertEqual(r['initial_pool_target_count'],20)
        self.assertEqual(r['nominal_load_attempts_at_target_rate'],10)
        self.assertEqual(r['actual_rpc_successes'],1)
if __name__=='__main__':unittest.main()
