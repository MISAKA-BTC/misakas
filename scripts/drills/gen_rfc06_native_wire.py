#!/usr/bin/env python3
"""Write structural wire scanner source; executes no compilation or validation.
The native inventory generator supplies the recursively resolved concrete type graph.
Manual/versioned Borsh codecs have explicit owner-local adapters, never derive assumptions.
"""
import runpy,re
from pathlib import Path
G=runpy.run_path(str(Path(__file__).with_name('gen_rfc06_native_inventory.py')))
ROOT,CORE,defs,aliases,byname,alias_byname=(G[k] for k in ['ROOT','CORE','defs','aliases','byname','alias_byname'])
split,stripattrs,brace=(G[k] for k in ['split','stripattrs','brace'])
OUT=CORE/'palw_native_resource_v1/wire_generated.rs'
manual={'misaka_palw_kernel::state::ClassRecordV1','misaka_palw_kernel::state::PipelineClassRecordV1','misaka_palw_kernel::flat_candidate7::FlatCandidateRecordV7','misaka_palw_kernel::flat_candidate9::FlatCandidateRecordV9','crate::palw_state_v2::PalwStateCarriageV2','crate::palw_bond_budget_v1::PalwBondBudgetStateV1','crate::palw_model_market_v1::PalwModelMarketV1','misaka_palw_panel::PanelCandidatesV1','crate::palw_kernel_route_v1::KernelLedgerCacheV1','crate::palw_kernel_route_v1::PalwKernelRouteStateV1'}

# Public constants by name (array lengths in a layout, e.g. `[u128; QUOTA_AXES_V11]`), at their canonical module path: an
# unqualified constant is not in scope in the generated module.
PUB_CONSTS={}
for _base,_crate in G['CRATES']:
    for _file in _base.rglob('*.rs'):
        if 'tests' in _file.parts:continue
        _module=G['canonical_module'](_crate,G['module_of'](_base,_crate,_file))
        for _m in re.finditer(r'(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:',G['clean'](_file.read_text())):
            PUB_CONSTS.setdefault(_m[1],set()).add('::'.join(x for x in [_crate,_module,_m[1]] if x))
def qualify(t,item):
    def token(m):
        x=m.group();name=x.split('::')[-1]
        if name=='EVM_ADDRESS_SIZE':return 'crate::evm::EVM_ADDRESS_SIZE'
        if name=='PalwSharedCourtV1':return 'crate::palw_staged_court_state_v1::PalwSharedCourtV1'
        if x==name and re.fullmatch(r'[A-Z][A-Z0-9_]*',name) and len(PUB_CONSTS.get(name,()))==1:
            path=next(iter(PUB_CONSTS[name]))
            return path if not path.startswith('crate::') or item['path'].startswith('crate::') else path
        if x.startswith('crate::') and not item['path'].startswith('crate::'):x=item['path'].split('::')[0]+'::'+x[7:]
        if name=='Digest':return '[u8;64]'
        if name=='Hash64':return 'crate::Hash64'
        if name=='BlockHash':return 'crate::BlockHash'
        if name in {'Hash','Hash32'}:return 'kaspa_hashes::Hash'
        if name=='HashContinuation':return 'crate::palw_rendering_stream_v1::LeafHashContinuationV1'
        if x.startswith(('flat_candidate7::','flat_candidate9::')):x='misaka_palw_kernel::'+x
        if name=='BeaconContextV1':x='misaka_palw_challenge::beacon::BeaconContextV1'
        if name=='Block' and item['path'].startswith('misaka_palw_tir::'):x='misaka_palw_tir::program::Block'
        if x in defs or x in aliases:result=x
        elif len(alias_byname.get(name,[]))==1:result=alias_byname[name][0]
        elif len(byname.get(name,[]))==1:result=byname[name][0]['path']
        elif '::' not in x and item['path'].rsplit('::',1)[0]+'::'+x in defs:result=item['path'].rsplit('::',1)[0]+'::'+x
        else:result=x
        return result.replace('crate::palw_bond_budget_v1::material::','crate::palw_bond_budget_v1::').replace('crate::palw_bond_budget_v1::consumer::','crate::palw_bond_budget_v1::')
    return re.sub(r'(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*',token,t)

private={};public=[]
for path in sorted(G['visited']):
    item=defs.get(path)
    if not item or path in manual:continue
    if path=='misaka_palw_kernel::ledger::KernelLedgerV1':continue # not a Borsh wire layout
    kind,form,body=(item[k] for k in ['kind','form','body'])
    fieldtypes=[]
    if kind=='struct':
        for part in split(body):
            if re.search(r'#\[borsh\(skip\)\]',part):continue
            part=stripattrs(part)
            if form=='named':
                m=re.match(r'(?:pub(?:\([^)]*\))?\s+)?\w+\s*:\s*(.*)',part,re.S)
                if not m:raise ValueError((path,part))
                typ=m[1]
            else:typ=re.sub(r'^pub(?:\([^)]*\))?\s+','',part)
            fieldtypes.append(qualify(typ,item))
        code='wire_struct!('+path+'; '+', '.join(fieldtypes)+');'
    else:
        arms=[];next_tag=0
        explicit=bool(re.search(r'use_discriminant\s*=\s*true',item['attrs']))
        for index,part in enumerate(split(body)):
            part=stripattrs(part);m=re.match(r'(\w+)\s*(.*)',part,re.S);name,rest=m.groups();rest=rest.strip();types=[]
            if rest.startswith(('{','(')):
                vb,_=brace(rest,0)
                for field in split(vb):
                    if re.search(r'#\[borsh\(skip\)\]',field):continue
                    field=stripattrs(field)
                    if rest.startswith('{'):field=re.match(r'\w+\s*:\s*(.*)',field,re.S)[1]
                    types.append(qualify(field,item))
            discriminant=re.search(r'=\s*(0x[0-9a-fA-F_]+|[0-9_]+)\s*$',rest)
            if explicit and '=' in rest and not discriminant:raise ValueError(('unreviewed discriminant',path,rest))
            tag=(int(discriminant[1].replace('_',''),0) if discriminant else next_tag) if explicit else index
            next_tag=tag+1
            arms.append(str(tag)+' => {let mut sum=NativeWireShapeV1::default();'+''.join('sum=sum.merge(<'+typ+' as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;' for typ in types)+'Ok(sum)},')
        code='impl NativeWireScanV1 for '+path+' {fn scan_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1> {reader.step()?;match reader.byte()? {'+''.join(arms)+'_=>Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant"))}}}'
    # A public type whose wire layout names a crate-private type is visited beside that type's owner (a child module may
    # name its parent's private items; the public generated module may not).
    hidden={defs[t]['module'] for t in re.findall(r'crate::(?:\w+::)*\w+',code) if t!=path and t in defs and not defs[t]['type_public']}
    if hidden-{item['module']}:raise ValueError(('wire layout names another module\'s private type',path,hidden))
    if path.startswith('crate::') and (not item['type_public'] or hidden):
        private.setdefault(item['module'],[]).append(code)
    else:public.append(code.replace('crate::palw_bond_budget_v1::material::','crate::palw_bond_budget_v1::').replace('crate::palw_bond_budget_v1::consumer::','crate::palw_bond_budget_v1::'))
OUT.write_text('//! Generated structural Borsh visitors; manual codecs are separate adapters.\n#![allow(unused_mut)]\nuse super::*;\nuse std::collections::{BTreeMap,BTreeSet};\nuse std::sync::Arc;\nuse super::wire::wire_struct;\n'+'\n'.join(public)+'\n')
# Idempotent: the inventory generator (run above via runpy) has just rewritten every owner-local
# file it owns, so appending once lands after its fresh content. A module the inventory did not
# write this run is rewritten here instead of appended to, so re-running never duplicates code.
for module,codes in private.items():
    p=CORE/'palw_native_resource_v1/private'/ (module.replace('::','_')+'.rs')
    with p.open('a' if module in G['private'] else 'w') as f:f.write('\nuse super::*;\n#[allow(unused_imports)]\nuse crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};\nuse crate::palw_native_resource_v1::wire::wire_struct;\n'+'\n'.join(codes)+'\n')
# The manual carriage serializer defines exactly the legacy fields and tail read
# order. Derive the scanner's dispatch from those actual names/types; refuse any
# special decoder not explicitly audited below.
source=(CORE/'palw_state_v2.rs').read_text()
carriage=defs['crate::palw_state_v2::PalwStateCarriageV2'];fieldmap={name:typ for name,typ,_ in G['fields'](carriage)}
legacy=re.search(r'struct PalwStateCarriageV2Legacy<[^>]+>\s*\{',source);lb,_=brace(source,legacy.end()-1)
legacynames=[re.match(r'\s*(\w+)\s*:',part)[1] for part in split(G['clean'](lb))]
start=source.index('impl borsh::BorshDeserialize for PalwStateCarriageV2');start=source.index('match tail[0] {',start)
# find the match opening exactly
op=source.index('{',start);dispatch,_=brace(source,op)
arms=[]
parts=[];cursor=0
while cursor<len(dispatch):
    remaining=dispatch[cursor:].lstrip(' ,\n\t');prefix=len(dispatch[cursor:])-len(remaining);cursor+=prefix
    if remaining.startswith('_ =>'):break
    arm=re.match(r'(?:crate::[\w:]+|\w+)\s+if\s+.*?=>\s*\{',remaining,re.S)
    if not arm:raise ValueError(remaining[:160])
    _,end=brace(remaining,arm.end()-1);parts.append(remaining[:end]);cursor+=end
for part in parts:
    part=G['clean'](part).strip()
    if part.startswith('_ =>'):continue
    match=re.match(r'(.*?)\s+if\s+(.*?)\s*=>\s*\{',part,re.S)
    if not match:raise ValueError(part[:160])
    tag,guard=match.groups();body,_=brace(part,match.end()-1)
    assigned=re.findall(r'(\w+)\s*=\s*(?:Some\()?[^;]*?(?:deserialize_reader|::deserialize)\([^;]*?;',body,re.S)
    names=[name for name in assigned if name in fieldmap]
    if not names:raise ValueError((tag,body))
    scans=[]
    for name in names:
        typ=qualify(fieldmap[name],carriage)
        if 'Some(' in body[body.index(name+' ='):body.index(';',body.index(name+' ='))]:
            assert typ.startswith('Option<'),(name,typ);typ=typ[7:-1]
        scans.append('let shape=<'+typ+' as NativeWireScanV1>::scan_native_wire_v1(reader)?;sum=sum.merge(shape)?;counts.push(("'+name+'",shape.outer_entries,shape.collection_entries));')
    arms.append(tag+' => { '+''.join(scans)+' },')
legacy_scans=''.join('let shape=<'+qualify(fieldmap[name],carriage)+' as NativeWireScanV1>::scan_native_wire_v1(reader)?;sum=sum.merge(shape)?;counts.push(("'+name+'",shape.outer_entries,shape.collection_entries));' for name in legacynames if name!='version')
legacy_scans='if reader.short()?!=PALW_STATE_V2_VERSION {return Err(crate::palw_native_resource_v1::wire::wire_bad("unknown native carriage version"));}'+legacy_scans
file=CORE/'palw_native_resource_v1/private/wire_carriage.rs'
file.write_text('''//! Generated manual carriage field/tail dispatch. Core manual decoder remains authority.
use super::*;
use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};
pub(crate) fn scan_carriage_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<(NativeWireShapeV1,Vec<(&'static str,u64,u64)>)> {
 let mut sum=NativeWireShapeV1::default();let mut counts=Vec::with_capacity(128);
'''+legacy_scans+'''
 let mut seen=[false;256];while reader.remaining()!=0 {let tag=reader.byte()?;if seen[tag as usize] {return Err(crate::palw_native_resource_v1::wire::wire_bad("duplicate carriage tail"));}seen[tag as usize]=true;
 match tag {'''+''.join(arms)+'''_=>return Err(crate::palw_native_resource_v1::wire::wire_bad("unknown carriage tail")),}}
 Ok((sum,counts))
}
''')
print(f'Wrote structural scanners: {len(public)} public, {sum(len(v) for v in private.values())} private, {len(arms)} carriage tail adapters')

# Each restoration term uses a reviewed source table, rather than total encoded
# length. Nested member bounds are needed for payees/refuted/staged hold vectors.
index_sources={
 'exec_v2_job_use_deadlines':('exec_v2_job_uses',False),
 'exec_v2_reward_payees':('exec_v2_rewards',True),
 'staged_holds':('staged_courts',True),'staged_due':('staged_courts',False),'staged_by_owner':('staged_courts',False),
 'improvement_due':('improvement_lines',False),'improvement_heads_of':('improvement_lines',False),
 'improvement_retiring':('improvement_epochs',False),'improvement_eval_claims':('improvement_eval_jobs',True),
 'improvement_artifact_outputs':('improvement_artifacts',False),'improvement_artifact_answers':('improvement_artifacts',False),
 'improvement_artifact_deadlines':('improvement_artifacts',False),'improvement_material_epochs':('ALL_MATERIAL_EPOCH_SOURCES',False),
 'improvement_opt_in_expiries':('improvement_opt_ins',False),'improvement_licence_expiries':('improvement_licences',False),
 'deadlines':('claims',False),'unresolved':('claims',False),'work_ids':('claims',False),
 'open_courts_by_claim':('court_sessions',False),'court_deadlines':('court_sessions',False),
 'court_close_deadlines':('court_close_groups',False),'courts_by_challenger':('court_sessions',False),
 'commitments_by_reporter':('reporter_commitments',False),'commitments_by_daa':('reporter_commitments',False),
 'vesting_order':('vesting',False),'vesting_unlatched':('vesting',False),'vesting_unlatched_by_settled':('vesting',False),'vesting_payees':('vesting',True),
 'da_deadlines':('da_sessions',False),'da_by_accuser':('ALL_DA_SOURCES',True),
 'legacy_dispute_deadlines':('legacy_disputes',False),'legacy_disputes_by_reserver':('legacy_disputes',False),
 'capacity_weight_index':('claims',False),
}
state=defs['crate::palw_state_v2::PalwChainStateV2'];state_fields={name:typ for name,typ,_ in G['fields'](state)}
def index_terms(typ,count):
 typ=qualify(typ,state)
 if typ=='crate::palw_weight_cap_v1::PalwBondWeightIndexV1':typ='BTreeMap<crate::palw_state_v2::PalwBondKeyV2,crate::palw_weight_cap_v1::PalwBondWeightV1>'
 match=re.match(r'(BTreeMap|BTreeSet)<(.*)>',typ,re.S)
 if not match:raise ValueError(typ)
 kind,body=match.groups();args=split(body)
 if kind=='BTreeMap':key,value=args
 else:key=args[0];value='()'
 out='sum=sum.allocation('+count+',std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<'+key+','+value+'>>())?;'
 if value.startswith(('BTreeMap<','BTreeSet<')):out+=index_terms(value,count)
 return out
restores=''
for name,(source,nested) in index_sources.items():
 if source.startswith('ALL_'):
  sources=['improvement_artifacts','improvement_cases','improvement_setter_sets'] if source=='ALL_MATERIAL_EPOCH_SOURCES' else ['da_sessions','da_claims']
  count='+'.join('count("'+item+'",'+str(nested).lower()+')' for item in sources)
 else:count='count("'+source+'",'+str(nested).lower()+')'
 restores+='let n=('+count+');'+index_terms(state_fields[name],'n')
with file.open('a') as f:
 f.write('\npub(crate) fn restore_index_native_shape_v1(counts:&[(&str,u64,u64)])->std::io::Result<NativeWireShapeV1> {\n let count=|name:&str,nested:bool| counts.iter().find(|(field,_,_)|*field==name).map_or(0,|(_,outer,members)| if nested {outer.saturating_add(*members)} else {*outer});\n let mut sum=NativeWireShapeV1::default();\n'+restores+'Ok(sum)\n}\n')
# Private fields of manual monetary versions cannot be guessed from struct order.
# Emit exact version-conditioned fields and finite source-derived rebuild terms.
budget=defs['crate::palw_bond_budget_v1::PalwBondBudgetStateV1'];bf={n:t for n,t,_ in G['fields'](budget)}
header=defs['crate::palw_bond_budget_v1::PalwBondBudgetHeaderV1']
header_types=[qualify(t,header) for n,t,_ in G['fields'](header) if n!='version']
code='use super::*;\nuse std::collections::{BTreeMap,BTreeSet};\nimpl NativeWireScanV1 for crate::palw_bond_budget_v1::PalwBondBudgetStateV1 { fn scan_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1> {let version=reader.short()?;if !(1..=4).contains(&version) {return Err(wire::wire_bad("unknown budget version"));}let mut sum=NativeWireShapeV1::default();'
code+=''.join('sum=sum.merge(<'+t+' as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;' for t in header_types)
for name in ['bonds','claims','releases','assignments','models','liability_releases','reporter_income','material_income','consumer_contracts']:
 version={'reporter_income':2,'material_income':3,'consumer_contracts':4}.get(name,1)
 code+='let '+name+'=if version>='+str(version)+' { <'+qualify(bf[name],budget)+' as NativeWireScanV1>::scan_native_wire_v1(reader)? }else{NativeWireShapeV1::default()};sum=sum.merge('+name+')?;'
for name,source in [('consumer_service_due','consumer_contracts'),('consumer_exhausted','consumer_contracts'),('material_roots','material_income'),('material_sources','material_income'),('material_due','material_income'),('deferred_by_bond','claims'),('deferred_awards','claims'),('pending_reporters','reporter_income')]:
 code+=index_terms(bf[name],source+'.outer_entries')
code+='Ok(sum)}}\n'
(OUT.parent/'wire_budget.rs').write_text(code)

# Kernel-route aux tables 42–106 and 112: one arm per (table, key condition) of scripts/drills/rfc06_native_aux_spec.py.
import importlib.util
_aux_spec=importlib.util.spec_from_file_location('rfc06_native_aux_spec',Path(__file__).with_name('rfc06_native_aux_spec.py'))
AUX=importlib.util.module_from_spec(_aux_spec);_aux_spec.loader.exec_module(AUX)
# Modules included through `#[path]` (fold modules nested in palw_state_v2) cannot be named from here: their private row
# structs are mirrored, and no generated type may name a path inside one.
FOLD_STEMS={Path(m).stem for f in CORE.rglob('*.rs') if 'tests' not in f.parts for m in re.findall(r'#\[path\s*=\s*"([^"]+\.rs)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+',f.read_text()) if 'palw_native_resource_v1' not in m}
def aux_nest(ts):
    if len(ts)==1:return ts[0]
    if len(ts)<=6:return '('+','.join(ts)+')'
    return aux_nest([aux_nest(ts[i:i+6]) for i in range(0,len(ts),6)])
def aux_mirror(key):
    item=defs[key];types=[]
    if item['kind']!='struct' or item['form']!='named':raise ValueError(('only named structs can be mirrored',key))
    for part in split(item['body']):
        if re.search(r'#\[borsh\(skip\)\]',part):continue
        m=re.match(r'(?:pub(?:\([^)]*\))?\s+)?\w+\s*:\s*(.*)',stripattrs(part),re.S)
        typ=m[1].strip()
        types.append(aux_expand(typ) if typ.startswith('mirror:') else aux_expand(qualify(typ,item)))
    return aux_nest(types)
def aux_expand(t):
    t=re.sub(r'mirror:([\w:]+)',lambda m:aux_mirror(m[1]),t)
    for tok in re.findall(r'(?:[A-Za-z_]\w*::)+[A-Za-z_]\w*',t):
        if tok.startswith('crate::') and tok.split('::')[1] in FOLD_STEMS:raise ValueError(('fold-private path leaked into a generated type',tok))
        if tok not in defs and tok not in aliases and tok not in {'crate::Hash64','crate::BlockHash','kaspa_hashes::Hash'} and not re.search(r'::[A-Z0-9_]+$',tok):raise ValueError(('unresolved aux type',tok,t))
    return t
def aux_scan(value):
    if value.startswith('raw:'):return 'raw_aux_row_v1(frame,('+value[4:]+') as u64)?'
    if value.startswith('any:'):
        alts=[aux_expand(a) for a in value[4:].split('|')]
        return 'any_aux_row_v1(frame,&['+','.join('try_exact_aux_row_v1::<'+a+'>' for a in alts)+'])?'
    return '<'+aux_expand(value)+' as NativeWireScanV1>::scan_native_wire_v1(frame)?'
AUX_SPEC_ARMS='// Tables 42–106 and 112 (rfc06_native_aux_spec.py; inventory: docs/design/palw/native-resource-aux-inventory-2026-10-15.md).\n'
_total=set()
for _n in AUX.NS:
    if _n['cond']=='true':
        _total.add(_n['table']);AUX_SPEC_ARMS+=f"{_n['table']}=>{aux_scan(_n['value'])},\n"
    else:AUX_SPEC_ARMS+=f"{_n['table']} if {_n['cond']}=>{aux_scan(_n['value'])},\n"
AUX_SPEC_ARMS+=' | '.join(str(t) for t in sorted(AUX.RESERVED))+'=>return Err(wire::wire_bad("reserved aux table holds no rows")),\n'
for _t in AUX.TABLES:
    if _t not in AUX.RESERVED and _t not in _total:AUX_SPEC_ARMS+=f'{_t}=>return Err(wire::wire_bad("unallocated aux table {_t} row namespace")),\n'
# The RFC07/RFC08 tables above dispatch on guarded key shapes only; a shape none of their folds writes is malformed too.
AUX_SPEC_ARMS+='115 | 123 | 125 | 126 | 127 | 130=>return Err(wire::wire_bad("unallocated aux row namespace")),\n'
AUX_HELPERS='''
/// The longest aux key any fold writes is far below this; a longer key is malformed, never truncated.
const NATIVE_AUX_KEY_MAX_BYTES_V1: u64 = 4096;
/// A raw byte row (stored verbatim, not Borsh-framed, never decoded by restore or ledger projection): its declared bound, then
/// one transient copy of its bytes (the outer loop already holds the stored row and its copy).
fn raw_aux_row_v1<R: Read>(frame: &mut NativeWireReaderV1<R>, cap: u64) -> std::io::Result<NativeWireShapeV1> {
    let n = frame.remaining();
    if n > cap {
        return Err(wire::wire_bad("raw aux row exceeds its declared bound"));
    }
    frame.step()?;
    frame.skip(n)?;
    NativeWireShapeV1 { byte_entries: n, ..Default::default() }.allocation(n, 1)
}
/// One exact layout of an ambiguous key shape over a buffered row: accepted only if it consumes every byte.
fn try_exact_aux_row_v1<T: NativeWireScanV1>(row: &[u8]) -> Option<NativeWireShapeV1> {
    let mut reader = NativeWireReaderV1::new(row, row.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader).ok()?;
    (reader.remaining() == 0).then_some(shape)
}
/// A key shape two families write with different layouts: buffer the bounded row, try every layout, account the LARGEST
/// accepted shape plus the buffer (never under-count); no layout accepting fails closed.
fn any_aux_row_v1<R: Read>(
    frame: &mut NativeWireReaderV1<R>,
    layouts: &[fn(&[u8]) -> Option<NativeWireShapeV1>],
) -> std::io::Result<NativeWireShapeV1> {
    let n = frame.remaining();
    if n > 1 << 20 {
        return Err(wire::wire_bad("ambiguous aux row exceeds its buffered bound"));
    }
    let mut row = vec![0u8; n as usize];
    frame.step()?;
    frame.read_exact(&mut row)?;
    let mut best: Option<NativeWireShapeV1> = None;
    for layout in layouts {
        if let Some(shape) = layout(&row) {
            best = Some(match best {
                Some(b) if b.heap >= shape.heap => b,
                _ => shape,
            });
        }
    }
    best.ok_or_else(|| wire::wire_bad("no audited layout accepts this ambiguous aux row"))?.allocation(n, 1)
}
'''
aux={
32:'crate::palw_state_v2::PalwBondKeyV2',33:'crate::palw_kernel_route_v1::KernelAssignmentV1',34:'Vec<misaka_palw_kernel::receipt::PalwConstraintReceiptV1>',35:'(u64,u32,u64)',
36:'crate::palw_onboarding_v1::ArtifactBindingRowV1',37:'crate::palw_onboarding_v1::KernelBindingRowV1',38:'crate::palw_onboarding_v1::ConformanceRowV1',39:'crate::palw_onboarding_v1::ConformanceAttemptRowV1',40:'crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1',41:'crate::palw_kernel_route_v1::PalwKernelChunkGroupV1',
107:'u64',108:'misaka_palw_kernel::beacon_source::BeaconSourceTargetV1',109:'misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1',110:'crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1',111:'crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1',
128:'crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2',113:'misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2',114:'misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2',115:'crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2',116:'crate::palw_claim_challenge_v2::PalwPaidClaimLockV2',117:'misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2',118:'misaka_palw_challenge::approval_scope::LifetimeCounterV3',119:'crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3',120:'crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1',122:'misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3',123:'crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3',124:'crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2',
}
code='//! Exact native aux codecs; no opaque row size stands for nested decoded demand.\nuse super::*;\nuse std::{io::Read,mem::size_of,collections::BTreeMap};\nimpl NativeWireScanV1 for crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {\n fn scan_native_wire_v1<R:Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1> {\n  let mut sum=crate::palw_kernel_route_v1::PalwKernelRouteHeaderV1::scan_native_wire_v1(reader)?;\n  // The persisted ledger rows are native byte Vecs. State root/restore hashes\n  // them directly; later full ledger class/program expansion is a separate\n  // consumer operation, never inferred from their encoded row length here.\n  sum=sum.merge(BTreeMap::<(u8,Vec<u8>),Vec<u8>>::scan_native_wire_v1(reader)?)?;\n  let count=reader.word()? as u64;\n  for _ in 0..count {\n   let table=reader.byte()?;let key_bytes=reader.word()? as u64;\n   if key_bytes>reader.remaining() {return Err(wire::wire_bad("native aux key exceeds frame"));}\n   if key_bytes>NATIVE_AUX_KEY_MAX_BYTES_V1 {return Err(wire::wire_bad("native aux key exceeds the audited aux key bound"));}\n   let mut key=[0u8;NATIVE_AUX_KEY_MAX_BYTES_V1 as usize];reader.step()?;reader.read_exact(&mut key[..key_bytes as usize])?;\n   sum=sum.allocation(key_bytes.checked_mul(2).ok_or_else(||wire::wire_bad("native aux key allocation overflow"))?,1)?;\n   let row_bytes=reader.word()? as u64;\n   if row_bytes>reader.remaining() {return Err(wire::wire_bad("native aux row exceeds frame"));}\n   let mut frame=NativeWireReaderV1::new((&mut *reader).take(row_bytes),row_bytes);\n   let shape=match table {\n'
code+=''.join(str(table)+(' if key_bytes==64' if table in (115,123) else '')+' => <'+typ+' as NativeWireScanV1>::scan_native_wire_v1(&mut frame)?,\n' for table,typ in aux.items())
code+='115 if key_bytes==65 && matches!(prefix,0x49|0x42)=>crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(&mut frame)?,\n123 if key_bytes==65 && prefix==0x49=>crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(&mut frame)?,\n// RFC07/RFC08 signed economic contracts (2026-10-15): tallies keyed by a 64-byte id; extensions and contracts under their\n// 0x01 prefix; the minimum-price job index (0x02) holds a 64-byte contract id.\n125 if key_bytes==64=>crate::palw_econ_contracts_v1::PalwModelVerificationTallyV1::scan_native_wire_v1(&mut frame)?,\n126 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::HORIZON_EXTENSION_PREFIX_V1=>crate::palw_econ_contracts_v1::PalwHorizonExtensionRowV1::scan_native_wire_v1(&mut frame)?,\n127 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::MINIMUM_PRICE_CONTRACT_PREFIX_V1=>crate::palw_econ_contracts_v1::PalwMinimumPriceRowV1::scan_native_wire_v1(&mut frame)?,\n127 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::MINIMUM_PRICE_JOB_PREFIX_V1=><[u8;64] as NativeWireScanV1>::scan_native_wire_v1(&mut frame)?,\n// Round 3 (2026-10-15): the verification gate wiring rows (aux 125, 65-byte prefixed keys) and the N3\n// BootstrapExact rounds (aux 130, keyed by the 64-byte round id).\n125 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::MODEL_VERIFICATION_CONTEXT_PREFIX_V1=>crate::palw_econ_contracts_v1::PalwModelVerificationContextV1::scan_native_wire_v1(&mut frame)?,\n125 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::MODEL_VERIFICATION_DEFERRED_PREFIX_V1=>crate::palw_econ_contracts_v1::PalwModelVerificationDeferredV1::scan_native_wire_v1(&mut frame)?,\n125 if key_bytes==65 && prefix==crate::palw_econ_contracts_v1::MODEL_VERIFICATION_DECIDED_PREFIX_V1=>crate::palw_econ_contracts_v1::PalwModelVerificationDecidedV1::scan_native_wire_v1(&mut frame)?,\n130 if key_bytes==64=>crate::palw_beacon_bootstrap_v1::PalwBootstrapRoundRowV1::scan_native_wire_v1(&mut frame)?,\n121 => match (key_bytes,prefix) {\n (65,0x41)=>crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2::scan_native_wire_v1(&mut frame)?,\n (65,0x42)=>crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2::scan_native_wire_v1(&mut frame)?,\n (64,_)=>crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2::scan_native_wire_v1(&mut frame)?,\n _=>return Err(wire::wire_bad("unallocated Panel lifetime row namespace")),},\n'+AUX_SPEC_ARMS+' _=>return Err(std::io::Error::new(std::io::ErrorKind::Unsupported,NativeCoverageV1::unsupported::<crate::palw_kernel_route_v1::PalwKernelRouteStateV1>(&format!("native aux table {table}"),"nested row codec is not in the explicit supported shape inventory"))),};\n   if frame.remaining()!=0 {return Err(wire::wire_bad("native aux row codec has trailing/mismatched bytes"));}\n   sum=sum.merge(shape.allocation(row_bytes.checked_mul(2).ok_or_else(||wire::wire_bad("native aux byte allocation overflow"))?,1)?)?;\n  }\n  sum=sum.allocation(count.checked_mul(2).ok_or_else(||wire::wire_bad("native aux Vec overflow"))?,size_of::<((u8,Vec<u8>),Vec<u8>)>())?;\n  sum=sum.allocation(count,size_of::<typed::BTreeNodeEnvelopeV1<(u8,Vec<u8>),Vec<u8>>>())?;\n  // Restore consistency builds per-job and per-producer counters from the\n  // actual aux rows. The total row count bounds both fixed-key scratch maps.\n  sum=sum.allocation(count.checked_mul(2).ok_or_else(||wire::wire_bad("native aux scratch count overflow"))?,size_of::<typed::BTreeNodeEnvelopeV1<[u8;64],u32>>())?;\n  sum.outer_entries=count;sum.collection_entries=sum.collection_entries.checked_add(count).ok_or_else(||wire::wire_bad("native aux count overflow"))?;\n  Ok(sum)\n }\n}\n'
a=code.index('let shape=match table {');b=code.index('\n   if frame.remaining()',a)
body=code[a:b].removeprefix('let shape=').removesuffix(';').replace('&mut frame','frame')
code=code[:a]+'let shape=scan_native_aux_row_v1(table,&key[..key_bytes as usize],&mut frame)?;'+code[b:]
code+='\n/// One aux row\'s exact codec, dispatched on its table and FULL key (`k`) and, where two family layouts share a key shape, its row\n/// length (`n`). Tables 42–106 and 112 follow scripts/drills/rfc06_native_aux_spec.py; a key shape no fold writes fails closed.\npub(crate) fn scan_native_aux_row_v1<R:Read>(table:u8,key:&[u8],frame:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1>{let k=key;let n=frame.remaining();let key_bytes=k.len() as u64;let prefix=k.first().copied().unwrap_or(0);'+body+'}\n'
code+=AUX_HELPERS
(OUT.parent/'wire_kernel_aux.rs').write_text(code)

# Concrete native ledger row codecs; reflected from the same KernelLedger
# collection field types from_rows populates. Special class records expand their
# nested byte codecs through owner-audited adapters in ledger_expansion.rs.
ledger=defs['misaka_palw_kernel::ledger::KernelLedgerV1']
lf={name:typ for name,typ,_ in G['fields'](ledger)}
opv=defs['misaka_palw_kernel::opv::OpvStateV1'];of={name:typ for name,typ,_ in G['fields'](opv)}
typedstate=defs['misaka_palw_kernel::spec::TypedStateV1'];tf={name:typ for name,typ,_ in G['fields'](typedstate)}
tables={0:('flat_conformance_classes_v7',None),1:('bonds',None),2:('classes','misaka_palw_kernel::state::ClassRecordV1'),3:('pipeline_classes','misaka_palw_kernel::state::PipelineClassRecordV1'),4:('jobs',None),5:('pipeline_jobs',None),6:('claims',None),7:('demands',None),8:('served',None),9:('attested_artifacts',None),10:('job_claims',None),11:('seals',None),12:('admitted','opv'),13:('classes','opv'),14:('claims','opv'),15:('proof_seals',None),16:('job_escrows',None),17:('served_demands',None),18:('job_posters',None),19:('flat_conformance_classes_v9',None),20:('tiled_jobs',None),21:('seg_progress',None),22:('classes','spec'),23:('jobs','typed'),24:('lines','typed'),25:('claim_beacon_salts',None),26:('forfeited_claim_seals',None),27:('verifier_pay','pay'),28:('court_scope',None),29:('conformance_classes','candidate'),30:('beacon_sources',None),31:('exec_reward_holds',None)}
lines=[]
for tag,(field,special) in tables.items():
    owner=opv if special=='opv' else typedstate if special in {'typed','spec'} else ledger
    fieldtype=qualify((of if special=='opv' else tf if special in {'typed','spec'} else lf)[field],owner)
    match=re.match(r'(?:std::collections::)?(BTreeMap|BTreeSet)<(.*)>$',fieldtype,re.S)
    if special=='pay':key='(u8,[u8;64])';value='misaka_palw_kernel::verifier_pay::VerifierPayRowV1'
    elif match:
        args=split(match[2]);key=args[0];value=args[1] if match[1]=='BTreeMap' else '()'
    else:raise ValueError(('ledger collection alias needs audit',tag,fieldtype))
    wirevalue=value
    if special and special.startswith('misaka_'):wirevalue=special
    elif special=='candidate':wirevalue='([u8;64],misaka_palw_kernel::state::ClassRecordV1)'
    elif special=='spec':wirevalue='misaka_palw_kernel::spec::ComputationSpecV1'
    lines.append(str(tag)+' => <'+wirevalue+' as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<'+key+','+value+'>>>()),')
(CORE/'palw_native_resource_v1/ledger_rows_generated.rs').write_text('// Generated actual ledger-table row dispatch.\nfn scan_ledger_row_v1<R:Read>(table:u8,r:&mut NativeWireReaderV1<R>)->io::Result<NativeWireShapeV1>{match table{'+''.join(lines)+'_=>Err(wire::wire_bad("unknown persisted ledger table")),}}\n')
