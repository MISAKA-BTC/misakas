from pathlib import Path
import importlib.util,re,ast
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();wa=s.index('pub async fn worker(');a=s.index('            // Forget what the chain',wa);b=s.index('\n        }\n    }\n}',a);body=s[a:b];Path('/tmp/retirement-original-stage.rs').write_text(body)
def dicts(path,name):
 t=ast.parse(Path(path).read_text());return next(ast.literal_eval(n.value) for n in t.body if isinstance(n,ast.Assign) and any(isinstance(x,ast.Name) and x.id==name for x in n.targets))
allfields={**dicts('/tmp/extract_seat_progress.py','fields'),**dicts('/tmp/extract_collector_progress.py','fields'),'retention_liabilities':'HashMap<Hash64,u64>','liabilities_unread':'HashSet<Hash64>','own_executions':'HashMap<Hash64,Vec<u8>>'}
# Only used, truly writable books belong to this scope; court/submission data is read-only.
fields={n:t for n,t in allfields.items() if re.search(r'(?<!\.)\b'+n+r'\b',h.mask(body)) and n not in {'court_outbox','submitted'}}
reads={'duties':'[kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2]','court_progress':'court_progress::CourtProgressResultV1','court_outbox':'CourtOutboxV1','submitted':'HashMap<Hash64,u64>','challenged':'HashSet<Hash64>'}
# submitted retains its existing age sweep at the end, so it needs a mutable reference.
reads.pop('submitted');fields['submitted']='HashMap<Hash64,u64>'
new='''//! End-of-tick retention, replay retirement and receipt bookkeeping sweeps.
//! This scope cannot spend carrier funding or advance court sessions.
use super::*;

pub(super) struct RetirementReadV1<'a> {
'''
for n,t in reads.items():new+=f"    pub(super) {n}: &'a {t},\n"
new+='    pub(super) seat_r: bool,\n}\n\npub(super) struct RetirementRefsV1<\'a> {\n'
for n,t in fields.items():new+=f"    pub(super) {n}: &'a mut {t},\n"
new+='''}
impl PalwPanelService {
    pub(super) async fn retire_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        read: RetirementReadV1<'_>,
        books: RetirementRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let current_daa = tick.daa();
        let RetirementReadV1 {
'''
for n in reads:new+=f'            {n},\n'
new+='            seat_r,\n        } = read;\n        let RetirementRefsV1 {\n'
for n in fields:new+='            '+('mut ' if re.search(r'&mut\s+'+n+r'\b',h.mask(body)) else '')+n+',\n'
new+='        } = books;\n'
new+='\n'.join(line[4:] if line.startswith('    ') else line for line in body.splitlines())+'\n    }\n}\n';(r/'kaspad/src/palw_panel/retirement.rs').write_text(new)
call='''            self.retire_tick_v1(
                tick_read,
                retirement::RetirementReadV1 {
'''
for n in reads:call+=f'                    {n}: &{n},\n'
call+='                    seat_r,\n                },\n                retirement::RetirementRefsV1 {\n'
for n in fields:call+=f'                    {n}: &mut {n},\n'
call+='                },\n            ).await;'
s=s[:a]+call+s[b:];s=s.replace('mod collector;','mod retirement;\nmod collector;',1);p.write_text(s);print('retirement',len(fields),'mutable books')
