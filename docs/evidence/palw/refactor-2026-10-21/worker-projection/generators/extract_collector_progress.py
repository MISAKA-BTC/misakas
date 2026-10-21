from pathlib import Path
import importlib.util,re
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();wa=s.index('pub async fn worker(');a=s.index('            if self.config.fee_outpoint.is_some()',s.index('self.tir_public_filer_tick_v1',wa));b=s.index('            // Forget what the chain',a);body=s[a:b];assert body==Path('/tmp/collector-original-stage.rs').read_text()
reads={'receipt_pool_v2':'crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV2>','receipt_pool_v3':'crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV3>','receipt_facts':'crate::palw_receipt_pool::ReceiptChainFactsV1','licence_spread':'std::collections::hash_map::RandomState','own_receipts':'HashMap<Hash64, (PalwSeatReceiptV2, u64)>','own_receipt_class':'HashMap<Hash64, Hash64>'}
fields={'shard_books':'tir_shard::PalwTirShardBooksV1','submitted':'HashMap<Hash64,u64>','submit_attempts':'HashMap<Hash64,u32>','v1_sent':'HashMap<Hash64,u32>','supplementary_submitted':'HashMap<Hash64,u64>','vertex_book':'vertex::PalwVertexBookV1','trap_book':'mesh::PalwTrapBookV1','supplementary_v3_submitted':'HashMap<Hash64,u64>','supplementary_idle':'HashMap<Hash64,(u64,u64)>','licence_idle':'HashMap<Hash64,(u64,u64)>','court_outbox':'CourtOutboxV1','challenged':'HashSet<Hash64>','carrier_chain':'CarrierChainV1','registration':'registration::ClassRegistrationV1','readiness_lane':'PalwReadinessLaneV1','receipt_lanes':'BTreeMap<Hash64,PalwReadinessLaneV1>','readiness_waiting':'bool','last_lane':'Option<PalwCarrierLaneV1>','held_before':'bool','canonical_last_daa':'u64','improve':'improve::PalwImproveLoopV1'}
new='''//! Serial funding settlement, readiness, licence and supplementary submission.
//! Read-only receipt facts are separate from the mutable carrier and filing books.
//! No verification pool, replay task or material cache can be mutated here.
use super::*;

pub(super) struct CollectorReadV1<'a> {
'''
for n,t in reads.items():new+=f"    pub(super) {n}: &'a {t},\n"
new+='    pub(super) seat_r: bool,\n    pub(super) vertex_fence: Option<u64>,\n}\n\npub(super) struct CollectorRefsV1<\'a> {\n'
for n,t in fields.items():new+=f"    pub(super) {n}: &'a mut {t},\n"
new+='''}
impl PalwPanelService {
    pub(super) async fn collector_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        tick_guard: &mut PalwTickGuardV1,
        read: CollectorReadV1<'_>,
        books: CollectorRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let CollectorReadV1 {
'''
for n in reads:new+=f'            {n},\n'
new+='            seat_r, vertex_fence,\n        } = read;\n        let CollectorRefsV1 {\n'
scalars={'held_before','readiness_waiting','last_lane','canonical_last_daa'}
for n in fields:new+='            '+('mut ' if n not in scalars and re.search(r'&mut\s+'+n+r'\b',h.mask(body)) else '')+n+',\n'
new+='        } = books;\n'
m=h.mask(body);ed=[]
for x in re.finditer(r'\b(?:'+ '|'.join(scalars)+r')\b',m):
 n=x[0];ed.append((x.start(),x.end(),f'(*{n})'))
for aa,bb,t in reversed(ed):body=body[:aa]+t+body[bb:]
new+='\n'.join(line[4:] if line.startswith('    ') else line for line in body.splitlines())+'\n    }\n}\n';(r/'kaspad/src/palw_panel/collector.rs').write_text(new)
call='''            self.collector_tick_v1(
                tick_read,
                &mut tick_guard,
                collector::CollectorReadV1 {
'''
for n in reads:call+=f'                    {n}: &{n},\n'
call+='                    seat_r, vertex_fence,\n                },\n                collector::CollectorRefsV1 {\n'
for n in fields:call+=f'                    {n}: &mut {n},\n'
call+='                },\n            ).await;\n\n'
s=s[:a]+call+s[b:];s=s.replace('mod seat_progress;','mod collector;\nmod seat_progress;',1);p.write_text(s)
print('collector extraction:',len(fields),'mutable refs')
