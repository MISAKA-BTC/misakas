import datetime as dt
import importlib.util
from pathlib import Path
import sys
import unittest
sys.dont_write_bytecode = True
spec=importlib.util.spec_from_file_location('census',Path(__file__).parents[1]/'peer-census.py');c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
NOW=dt.datetime(2026,11,1,13,51,3,tzinfo=dt.timezone.utc)
RUNTIME={'network':'testnet-12','daa':18935,'p2pId':'observer','genesisHash':c.GENESIS,'params':'e'*64,'schedule':'4'*64,'fences':[18400,18640]}
LINES=(Path(__file__).parent/'fixtures/t12-handshake-rejections.log').read_text().splitlines()
FRESH='2026-11-01 14:51:02.516+01:00 [INFO ] Processing selected chain'
class CensusTests(unittest.TestCase):
 def test_actual_next_and_unknown_fired_set_messages_keep_both_session_directions(self):
  rows,warnings,_=c.parse_events(LINES,NOW)
  self.assertEqual(warnings,0);self.assertEqual(len(rows),2)
  self.assertEqual({r['ip'] for r in rows},{'198.51.100.27'})
  self.assertEqual({r['direction'] for r in rows},{'local_rejected_peer','peer_rejected_local'})
  nxt=next(r for r in rows if r['mismatch']=='next_fence_mismatch');self.assertEqual(nxt['crossed_fence'],18640);self.assertEqual(nxt['rejected_peer_next_fence'],'18446744073709551615')
  peer=next(r for r in rows if r['mismatch']=='unknown_fired_set');self.assertEqual(peer['rejecting_node_daa'],19019);self.assertEqual(peer['crossed_fence'],18400)
 def test_named_debug_variant_is_diagnostic_not_an_outdated_build_assumption(self):
  line='2026-11-01 14:51:02.516+01:00 [WARN] handshake failed for inbound peer 203.0.113.5:1234: CurrentNextFenceMismatch { expected: 18640 }'
  rows,warnings,_=c.parse_events([line],NOW);self.assertEqual(warnings,0);self.assertEqual(rows[0]['mismatch'],'next_fence_mismatch');self.assertNotIn('outdated',rows[0]['reason'])
 def test_zero_requires_valid_runtime_binding_and_fresh_log(self):
  good=c.document([FRESH],RUNTIME,True,NOW);self.assertTrue(good['available']);self.assertEqual(good['refused'],[]);self.assertFalse(good['complete_window'])
  for runtime,bound,lines in [(None,True,[FRESH]),([],True,[FRESH]),({**RUNTIME,'params':None},True,[FRESH]),({**RUNTIME,'fences':['18640']},True,[FRESH]),(RUNTIME,False,[FRESH]),({},True,[FRESH]),({**RUNTIME,'genesisHash':'wrong'},True,[FRESH]),(RUNTIME,True,[]),(RUNTIME,True,['2026-10-01 12:00:00.000+00:00 [INFO] old'])]:
   bad=c.document(lines,runtime,bound,NOW);self.assertFalse(bad['available']);self.assertIsNotNone(bad['availability_reason'])
 def test_wrong_log_network_is_unavailable_even_with_fresh_timestamps(self):
  bad=c.document([l.replace('misaka-testnet-12','misaka-testnet-11') for l in LINES],RUNTIME,True,NOW);self.assertFalse(bad['available'])
 def test_unparsed_rejection_cannot_silently_be_zero(self):
  bad=c.document([FRESH,'badtimestamp Fork-id mismatch'],RUNTIME,True,NOW);self.assertFalse(bad['available']);self.assertEqual(bad['unparsed_rejections'],1)
if __name__=='__main__':unittest.main()
