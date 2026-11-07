from pathlib import Path
import re,importlib.util
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();wa=s.index('pub async fn worker(');a=s.index('            tick_guard.phase("schedule");',wa)+len('            tick_guard.phase("schedule");');b=s.index('            tick_guard.phase("duty-sweep");',a);body=s[a:b];Path('/tmp/seat-progress-original-stage.rs').write_text(body)
reads={
'receipt_facts':'crate::palw_receipt_pool::ReceiptChainFactsV1','vertex_state_path':'PathBuf',
'mesh_node':'mesh::PalwMeshNodeV1','trap_book':'mesh::PalwTrapBookV1',
'duties':'[kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2]','seat_r_duties':'[PalwSeatRDutyV1]',
}
fields={
'pool_books':'PoolBooksV1',
'receipt_pool_v2':'crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV2>',
'receipt_pool_v3':'crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV3>',
'shard_shadow':'tir_shard::PalwTirShardShadowV1',
'own_filed':'HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>',
'own_filed_whole':'HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>',
'answered':'HashSet<SeatDutyPanelKeyV1>','first_seen':'HashMap<Hash64, u64>',
'replayed':'HashSet<SeatDutyPanelKeyV1>','seat_replays':'PalwSeatReplaysV1',
'receipts_filed_at':'std::collections::VecDeque<std::time::Instant>',
'seat_resumes':'PalwSeatResumesV1','service':'PalwSeatServiceV1',
'replay_refuted':'HashSet<Hash64>','evidence_kept':'HashSet<Hash64>','capture_sampled':'HashSet<Hash64>',
's3_samplings':'HashMap<SeatDutyPanelKeyV1, tokio::task::JoinHandle<PalwSeatS3SampleV1>>',
's3_sampled':'HashSet<SeatDutyPanelKeyV1>','requested':'HashMap<Hash64, u64>',
'requested_intervals':'HashMap<(Hash64, u32), u64>',
'own_receipts':'HashMap<Hash64, (PalwSeatReceiptV2, u64)>','vertex_book':'vertex::PalwVertexBookV1',
'vertex_equivocated':'bool','court_outbox':'CourtOutboxV1','reporter_filer':'reporter_filer::PalwReporterFilerV1',
'accused':'HashSet<Hash64>','own_receipt_class':'HashMap<Hash64, Hash64>',
'accusations':'PalwSeatAccusationsV1','false_valid':'crate::palw_filer_false_valid::PalwFalseValidFilerV1',
'improve':'improve::PalwImproveLoopV1',
}
new='''//! Ordered seat verification and publication. Read-only duty/chain inputs are
//! separated from mutable replay, service, receipt and filing books. The old
//! per-duty control flow, ledger ownership and task creation order are retained.
use super::*;

pub(super) struct SeatProgressReadV1<'a> {
'''
for name,typ in reads.items():new+=f"    pub(super) {name}: &'a {typ},\n"
new+='    pub(super) seat_r: bool,\n    pub(super) vertex_fence: Option<u64>,\n}\n\npub(super) struct SeatProgressRefsV1<\'a> {\n'
for name,typ in fields.items():new+=f"    pub(super) {name}: &'a mut {typ},\n"
new+='''}
impl PalwPanelService {
    pub(super) async fn seat_progress_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        order: Vec<usize>,
        read: SeatProgressReadV1<'_>,
        books: SeatProgressRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let SeatProgressReadV1 {
'''
for name in reads:new+='            '+name+',\n'
new+='            seat_r, vertex_fence,\n        } = read;\n        let SeatProgressRefsV1 {\n'
for name in fields:new+='            '+('mut ' if re.search(r'&mut\s+'+name+r'\b',h.mask(body)) else '')+name+',\n'
new+='        } = books;\n'
m=h.mask(body);edits=[]
for x in re.finditer(r'\bvertex_equivocated\b',m):edits.append((x.start(),x.end(),'*vertex_equivocated'))
for aa,bb,t in reversed(edits):body=body[:aa]+t+body[bb:]
new+='\n'.join(line[4:] if line.startswith('    ') else line for line in body.splitlines())+'\n    }\n}\n';(r/'kaspad/src/palw_panel/seat_progress.rs').write_text(new)
call='''
            self.seat_progress_tick_v1(
                tick_read,
                order,
                seat_progress::SeatProgressReadV1 {
'''
for name in reads:call+='                    '+name+': &'+name+',\n'
call+='                    seat_r, vertex_fence,\n                },\n                seat_progress::SeatProgressRefsV1 {\n'
for name in fields:call+='                    '+name+': &mut '+name+',\n'
call+='                },\n            ).await;\n'
s=s[:a]+call+s[b:];s=s.replace('mod court_progress;','mod seat_progress;\nmod court_progress;',1);p.write_text(s)
print('seat stage:',len(fields),'writable and',len(reads)+2,'read-only inputs; no new task or collector/funding state')
