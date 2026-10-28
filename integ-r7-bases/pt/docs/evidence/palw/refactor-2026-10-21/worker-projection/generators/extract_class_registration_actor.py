from pathlib import Path
import re,importlib.util
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();m=h.mask(s);wa=m.index('pub async fn worker(')
def close(m,a):
 d=1;i=a+1
 while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
 return i
we=close(m,m.index('{',wa));worker=s[wa:we];wm=h.mask(worker)
ia=wm.index('        let mut class_registration:');ib=wm.index(';',wm.index('let mut class_registration_known_id:',ia))+1;initial=worker[ia:ib];Path('/tmp/class-registration-original-init.rs').write_text(initial)
a=wm.index('if let Some((txid, sent_daa)) = class_registration_inflight');hold=wm.index('if palw_class_registration_holds_the_tick_v1(',a);brace=wm.index('{',hold);b=close(wm,brace);body=worker[a:b];Path('/tmp/class-registration-original-advance.rs').write_text(body)
assert len(re.findall(r'\bcontinue\s*;',h.mask(body)))==1
mapping={'class_registration':'self.object','class_registration_inflight':'self.inflight','class_registration_done':'self.done','class_registration_left_pool':'self.left_pool','class_registration_drops':'self.drops','class_registration_known_id':'self.known_id','self':'service'}
bm=h.mask(body);edits=[]
for x in re.finditer(r'\b('+'|'.join(mapping)+r')\b',bm):edits.append((x.start(),x.end(),mapping[x[1]]))
for aa,bb,t in reversed(edits):body=body[:aa]+t+body[bb:]
body=body.replace('continue;','return true;')
needle='                                    self.left_pool.unwrap_or(current_daa)\n';assert body.count(needle)==1;body=body.replace(needle,'                                    self.left_pool.unwrap_or(current_daa),\n                                    class_registration_drops = self.drops\n')
new='''//! Node-local registration intent, observation and retry state. A mempool
//! submission is still not chain registration. This actor cannot mutate receipt,
//! court, replay or material books; the existing fee-chain capability is explicit.
use super::*;

pub(super) struct ClassRegistrationV1 {
    object: Option<PalwConsensusObjectV2>,
    inflight: Option<(kaspa_consensus_core::tx::TransactionId, u64)>,
    done: bool,
    left_pool: Option<u64>,
    drops: u32,
    known_id: Option<Option<Hash64>>,
}
impl ClassRegistrationV1 {
    pub(super) fn new() -> Self {
'''+initial.replace('let mut ','let ')+'''
        Self {
            object: class_registration, inflight: class_registration_inflight, done: class_registration_done,
            left_pool: class_registration_left_pool, drops: class_registration_drops, known_id: class_registration_known_id,
        }
    }
    pub(super) fn done(&self) -> bool { self.done }
    pub(super) fn inflight(&self) -> Option<(kaspa_consensus_core::tx::TransactionId, u64)> { self.inflight }
    pub(super) fn object(&self) -> &Option<PalwConsensusObjectV2> { &self.object }
    pub(super) fn cloned_object(&self) -> Option<PalwConsensusObjectV2> { self.object.clone() }
    pub(super) fn replace_object(&mut self, object: PalwConsensusObjectV2) { self.object = Some(object); }
    pub(super) fn record_submission(&mut self, txid: kaspa_consensus_core::tx::TransactionId, current_daa: u64) {
        self.inflight = Some((txid, current_daa));
        self.left_pool = None;
    }
    pub(super) async fn advance_v1(
        &mut self,
        service: &Arc<PalwPanelService>,
        session: &kaspa_consensusmanager::ConsensusProxy,
        current_daa: u64,
        carrier_chain: &mut CarrierChainV1,
    ) -> bool {
'''+body+'''
        false
    }
}
'''
(r/'kaspad/src/palw_panel/registration.rs').write_text(new)
# Replace the complete initial state and the early stage at their existing positions.
worker=worker[:a]+'if registration.advance_v1(self, &session, current_daa, &mut carrier_chain).await {\n                continue;\n            }'+worker[b:]
worker=worker[:ia]+'        let mut registration = registration::ClassRegistrationV1::new();'+worker[ib:]
# Later funding remains a separate ordered stage, through explicit observation/update methods.
worker=worker.replace('class_registration_inflight = Some((txid, current_daa));\n                                        class_registration_left_pool = None;', 'registration.record_submission(txid, current_daa);')
worker=worker.replace('class_registration = Some(object)', 'registration.replace_object(object)')
worker=worker.replace('class_registration.clone()', 'registration.cloned_object()')
worker=worker.replace('&class_registration,', 'registration.object(),')
wm=h.mask(worker);edits=[]
for x in re.finditer(r'\b(class_registration_done|class_registration_inflight|class_registration)\b',wm):
 name=x[1];value={'class_registration_done':'registration.done()','class_registration_inflight':'registration.inflight()','class_registration':'registration.object()'}[name];edits.append((x.start(),x.end(),value))
for aa,bb,t in reversed(edits):worker=worker[:aa]+t+worker[bb:]
assert not re.search(r'\bclass_registration_(left_pool|drops|known_id)\b',h.mask(worker))
s=s[:wa]+worker+s[we:];s=s.replace('mod ingress;','mod registration;\nmod ingress;',1);p.write_text(s)
# Bring only the copied stage's indentation down to its new lexical depth.
p=r/'kaspad/src/palw_panel/registration.rs';s=p.read_text();a=s.index('if let Some((txid, sent_daa)) = self.inflight');b=s.index('\n        false\n',a);lines=s[a:b].splitlines();lines[0]='        '+lines[0]
for i in range(1,len(lines)):
 if lines[i].strip():assert lines[i].startswith('    ');lines[i]=lines[i][4:]
p.write_text(s[:a]+'\n'.join(lines)+s[b:])
