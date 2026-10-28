from pathlib import Path
import re,importlib.util
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();m=h.mask(s);wa=m.index('pub async fn worker(')
a=s.index('            // --- the court\'s half:',wa);b=m.index('            tick_guard.phase(',a);assert 'court+da' in s[b:b+60];body=s[a:b];Path('/tmp/court-progress-original-stage.rs').write_text(body)
fields={
'pool_books':'PoolBooksV1','own_executions':'HashMap<Hash64, Vec<u8>>',
'attn_evidence':'HashMap<(Hash64, bool), Arc<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1>>',
'attn_root_filings':'HashMap<Hash64, (u64, Option<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1>)>',
 'tir_root_filings':'HashMap<Hash64, (u64, Option<kaspa_consensus_core::palw_tir_dissect_v1::PalwTirRootClaimV1>)>',
'held_court':'held_court::PalwHeldCourtV1','shard_books':'tir_shard::PalwTirShardBooksV1',
'requested':'HashMap<Hash64, u64>','requested_intervals':'HashMap<(Hash64, u32), u64>',
'court_outbox':'CourtOutboxV1','da_flat_sent':'HashMap<Hash64, u64>',
'public_prefix_response_cursor':'Option<(u64, Hash64, PalwBondKeyV2)>',
'false_valid':'crate::palw_filer_false_valid::PalwFalseValidFilerV1',
}
# Session is the existing ConsensusProxy alias of ConsensusSessionOwned; no pruning guard is introduced.
(r/'kaspad/src/palw_panel/tick_inputs.rs').write_text('''//! The values this worker already captured once per iteration. This does not
//! snapshot the database or replace the existing unguarded consensus session.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct PanelTickReadV1<'a> {
    session: &'a kaspa_consensusmanager::ConsensusProxy,
    bond: PalwBondKeyV2,
    domain: Hash64,
    daa: u64,
}
impl<'a> PanelTickReadV1<'a> {
    pub(super) fn new(session: &'a kaspa_consensusmanager::ConsensusProxy, bond: PalwBondKeyV2, domain: Hash64, daa: u64) -> Self {
        Self { session, bond, domain, daa }
    }
    pub(super) fn session(&self) -> &'a kaspa_consensusmanager::ConsensusProxy { self.session }
    pub(super) fn bond(&self) -> PalwBondKeyV2 { self.bond }
    pub(super) fn domain(&self) -> Hash64 { self.domain }
    pub(super) fn daa(&self) -> u64 { self.daa }
}
''')
new='''//! Serialized court/DA progression. The writable inputs are its outbox,
//! evidence/request books, received material and false-Valid filing state, never
//! the carrier chain or the receipt collector. All original nested loops remain.
use super::*;

pub(super) struct CourtProgressRefsV1<'a> {
'''
for name,typ in fields.items():new+=f"    pub(super) {name}: &'a mut {typ},\n"
new+='''}

/// Retain the former top-level allocations until the caller's iteration ends.
/// Fields are in reverse declaration order to keep the old drop order. These are
/// data buffers, not a new cache, resource reservation or acceptance verdict.
pub(super) struct CourtProgressResultV1 {
    _owed: std::collections::BTreeSet<(Hash64, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1)>,
    pub(super) disclosure_retain: Vec<Hash64>,
    _disclosure: kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutiesV1,
    _da_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwDaDutyV2>,
    _held_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2>,
    _court_stalls: BTreeMap<&'static str, usize>,
    pub(super) court_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2>,
    _intervals_for_close: Vec<(Hash64, Vec<u32>)>,
    _pull_for_close: Vec<Hash64>,
}
impl PalwPanelService {
    pub(super) async fn court_progress_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        books: CourtProgressRefsV1<'_>,
    ) -> CourtProgressResultV1 {
        let session = tick.session();
        let bond_key = tick.bond();
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let CourtProgressRefsV1 {
'''
for name in fields:
 binding=('mut ' if re.search(r'&mut\s+'+name+r'\b',h.mask(body)) else '')+name
 new+='            '+binding+',\n'
new+='        } = books;\n'
# Keep only indentation different; no loop/return conversion is needed in this stage.
new+='\n'.join(line[4:] if line.startswith('    ') else line for line in body.splitlines())
new+='''
        CourtProgressResultV1 {
            _owed: owed, disclosure_retain, _disclosure: disclosure, _da_duties: da_duties,
            _held_duties: held_duties, _court_stalls: court_stalls, court_duties,
            _intervals_for_close: intervals_for_close, _pull_for_close: pull_for_close,
        }
    }
}
''';(r/'kaspad/src/palw_panel/court_progress.rs').write_text(new)
call='''            let court_progress = self.court_progress_tick_v1(
                tick_read,
                court_progress::CourtProgressRefsV1 {
'''
for name in fields:call+='                    '+name+': &mut '+name+',\n'
call+='''                },
            ).await;
'''
s=s[:a]+call+s[b:]
# Only the two old live-out buffers are used later, by their unchanged value type.
m=h.mask(s);x=m.index('            tick_guard.phase(',a);we=m.index('{',wa);d=1;we+=1
while d:d+=(m[we]=='{')-(m[we]=='}');we+=1
part=s[x:we];pm=h.mask(part);edits=[]
for token in re.finditer(r'\b(court_duties|disclosure_retain)\b',pm):
 assert not re.match(r'\s*:',pm[token.end():]);edits.append((token.start(),token.end(),'court_progress.'+token[1]))
for aa,bb,t in reversed(edits):part=part[:aa]+t+part[bb:]
s=s[:x]+part+s[we:]
needle='            let current_daa = session.get_virtual_daa_score();';assert s.count(needle)==1;s=s.replace(needle,needle+'\n            let tick_read = tick_inputs::PanelTickReadV1::new(&session, bond_key, network_domain, current_daa);')
s=s.replace('mod registration;','mod court_progress;\nmod tick_inputs;\nmod registration;',1);p.write_text(s)
print('moved court/DA stage with',len(fields),'explicit writable books; retained old allocation lifetimes')
