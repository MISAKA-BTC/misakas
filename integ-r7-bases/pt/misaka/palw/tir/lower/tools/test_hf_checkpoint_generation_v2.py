"""Independent EOS termination, length and comparison bounds before loading weights."""
import copy,unittest
from hf_checkpoint_generation_v2 import protocol_check
from score_checkpoint_generation_v2 import score
from test_hf_checkpoint_generation import policy
class V2(unittest.TestCase):
 def protocol(self):
  p=policy();p['schema']='misaka.rfc02.greedy-task-protocol.v2';p.update({'tokenizer_config_sha256':'a'*64,'generation_config_sha256':'b'*64,'chat_template_sha256':'c'*64,'decode_contract':'greedy-first-EOS-no-padding.v2'});p['task_jobs'][0]['user_text']='prompt';return p
 def test_version_and_new_policy_must_be_explicit(self):
  p=self.protocol();protocol_check(p,{'vocab_size':4,'max_position_embeddings':4})
  for key,value in [('schema','misaka.rfc02.greedy-task-protocol.v1'),('decode_contract','fixed32'),('chat_template_sha256','f'*63)]:
   bad=copy.deepcopy(p);bad[key]=value
   with self.assertRaises(ValueError):protocol_check(bad,{'vocab_size':4,'max_position_embeddings':4})
 def test_no_post_eos_padding_or_false_length_is_credited(self):
  p=self.protocol();job={'id':'job','task':'arithmetic','prompt_tokens':[1,2],'generated_tokens':[2,3],'actual_generated_length':2,'model_fed_tokens':[1,2,2],'actual_fed_length':3,'first_eos_index':1,'answer_text':'2','predeclared_task_answer_passed':True}
  src={'schema':'misaka.rfc02.independent-greedy-source.v2','jobs':[job]};own={'schema':'misaka.rfc02.independent-greedy-integer.v2','jobs':[{'id':'job','task':'arithmetic','prompt_tokens':[1,2],'generated_tokens':[2,3],'actual_generated_length':2,'model_fed_tokens':[1,2,2],'actual_fed_length':3,'source_generated_tokens':[2,3]}],'token_agreement':1.0,'exact_job_fraction':1.0,'predeclared_free_running_fidelity_passed':True}
  class Decoder:
   def decode(self,ts,skip_special_tokens):return ''.join(str(t) for t in ts)
  result=score(p,src,own,Decoder());self.assertEqual(result['actual_compared_output_slots'],2);self.assertTrue(result['predeclared_generation_and_answer_thresholds_met'])
  for tokens,count in [([2,3,0],3),([2,3],3),([],0)]:
   bad=copy.deepcopy(own);bad['jobs'][0]['generated_tokens']=tokens;bad['jobs'][0]['actual_generated_length']=count
   with self.assertRaises(ValueError):score(p,src,bad,Decoder())
 def test_missing_generated_position_counts_as_failure(self):
  p=self.protocol();p['token_agreement_min']=0.9
  src={'schema':'misaka.rfc02.independent-greedy-source.v2','jobs':[{'id':'job','task':'arithmetic','prompt_tokens':[1,2],'generated_tokens':[2,3],'actual_generated_length':2,'model_fed_tokens':[1,2,2],'actual_fed_length':3,'first_eos_index':1,'answer_text':'2','predeclared_task_answer_passed':True}]}
  own={'schema':'misaka.rfc02.independent-greedy-integer.v2','jobs':[{'id':'job','task':'arithmetic','prompt_tokens':[1,2],'generated_tokens':[3],'actual_generated_length':1,'model_fed_tokens':[1,2],'actual_fed_length':2,'source_generated_tokens':[2,3]}],'token_agreement':0.0,'exact_job_fraction':0.0,'predeclared_free_running_fidelity_passed':False}
  class Decoder:
   def decode(self,ts,skip_special_tokens):return ''.join(str(t) for t in ts)
  result=score(p,src,own,Decoder());self.assertEqual(result['actual_compared_output_slots'],2);self.assertFalse(result['predeclared_free_running_fidelity_passed'])
if __name__=='__main__':unittest.main()
