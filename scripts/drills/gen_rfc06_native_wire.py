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
manual={'crate::palw_state_v2::PalwStateCarriageV2','crate::palw_bond_budget_v1::PalwBondBudgetStateV1','crate::palw_model_market_v1::PalwModelMarketV1','misaka_palw_panel::PanelCandidatesV1','crate::palw_kernel_route_v1::KernelLedgerCacheV1','crate::palw_kernel_route_v1::PalwKernelRouteStateV1'}

def qualify(t,item):
    def token(m):
        x=m.group();name=x.split('::')[-1]
        if name=='EVM_ADDRESS_SIZE':return 'crate::evm::EVM_ADDRESS_SIZE'
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
    if path.startswith('crate::') and not item['type_public']:
        private.setdefault(item['module'],[]).append(code)
    else:public.append(code.replace('crate::palw_bond_budget_v1::material::','crate::palw_bond_budget_v1::').replace('crate::palw_bond_budget_v1::consumer::','crate::palw_bond_budget_v1::'))
OUT.write_text('//! Generated structural Borsh visitors; manual codecs are separate adapters.\n#![allow(unused_mut)]\nuse super::*;\nuse std::collections::{BTreeMap,BTreeSet};\nuse std::sync::Arc;\nuse super::wire::wire_struct;\n'+'\n'.join(public)+'\n')
for module,codes in private.items():
    p=CORE/'palw_native_resource_v1/private'/ (module.replace('::','_')+'.rs')
    with p.open('a') as f:f.write('\nuse super::*;\n#[allow(unused_imports)]\nuse crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireReaderV1,NativeWireShapeV1};\nuse crate::palw_native_resource_v1::wire::wire_struct;\n'+'\n'.join(codes)+'\n')
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

aux={
32:'crate::palw_state_v2::PalwBondKeyV2',33:'crate::palw_kernel_route_v1::KernelAssignmentV1',34:'Vec<misaka_palw_kernel::receipt::PalwConstraintReceiptV1>',35:'(u64,u32,u64)',
36:'crate::palw_onboarding_v1::ArtifactBindingRowV1',37:'crate::palw_onboarding_v1::KernelBindingRowV1',38:'crate::palw_onboarding_v1::ConformanceRowV1',39:'crate::palw_onboarding_v1::ConformanceAttemptRowV1',40:'crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1',41:'crate::palw_kernel_route_v1::PalwKernelChunkGroupV1',
107:'u64',108:'misaka_palw_kernel::beacon_source::BeaconSourceTargetV1',109:'misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1',110:'crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1',111:'crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1',
112:'crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2',113:'misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2',114:'misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2',115:'crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2',116:'crate::palw_claim_challenge_v2::PalwPaidClaimLockV2',117:'misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2',118:'misaka_palw_challenge::approval_scope::LifetimeCounterV3',119:'crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3',120:'crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1',122:'misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3',123:'crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3',124:'crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2',
}
code='//! Exact native aux codecs; no opaque row size stands for nested decoded demand.\nuse super::*;\nuse std::{io::Read,mem::size_of,collections::BTreeMap};\nimpl NativeWireScanV1 for crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {\n fn scan_native_wire_v1<R:Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1> {\n  let mut sum=crate::palw_kernel_route_v1::PalwKernelRouteHeaderV1::scan_native_wire_v1(reader)?;\n  // The persisted ledger rows are native byte Vecs. State root/restore hashes\n  // them directly; later full ledger class/program expansion is a separate\n  // consumer operation, never inferred from their encoded row length here.\n  sum=sum.merge(BTreeMap::<(u8,Vec<u8>),Vec<u8>>::scan_native_wire_v1(reader)?)?;\n  let count=reader.word()? as u64;\n  for _ in 0..count {\n   let table=reader.byte()?;let key_bytes=reader.word()? as u64;\n   if key_bytes>reader.remaining() {return Err(wire::wire_bad("native aux key exceeds frame"));}\n   let prefix=if key_bytes!=0 {let p=reader.byte()?;reader.skip(key_bytes-1)?;p}else{0};\n   sum=sum.allocation(key_bytes.checked_mul(2).ok_or_else(||wire::wire_bad("native aux key allocation overflow"))?,1)?;\n   let row_bytes=reader.word()? as u64;\n   if row_bytes>reader.remaining() {return Err(wire::wire_bad("native aux row exceeds frame"));}\n   let mut frame=NativeWireReaderV1::new((&mut *reader).take(row_bytes),row_bytes);\n   let shape=match table {\n'
code+=''.join(str(table)+(' if key_bytes==64' if table in (115,123) else '')+' => <'+typ+' as NativeWireScanV1>::scan_native_wire_v1(&mut frame)?,\n' for table,typ in aux.items())
code+='115 if key_bytes==65 && matches!(prefix,0x49|0x42)=>crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(&mut frame)?,\n123 if key_bytes==65 && prefix==0x49=>crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(&mut frame)?,\n121 => match (key_bytes,prefix) {\n (65,0x41)=>crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2::scan_native_wire_v1(&mut frame)?,\n (65,0x42)=>crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2::scan_native_wire_v1(&mut frame)?,\n (64,_)=>crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2::scan_native_wire_v1(&mut frame)?,\n _=>return Err(wire::wire_bad("unallocated Panel lifetime row namespace")),},\n _=>return Err(std::io::Error::new(std::io::ErrorKind::Unsupported,NativeCoverageV1::unsupported::<crate::palw_kernel_route_v1::PalwKernelRouteStateV1>(&format!("native aux table {table}"),"nested row codec is not in the explicit supported shape inventory"))),};\n   if frame.remaining()!=0 {return Err(wire::wire_bad("native aux row codec has trailing/mismatched bytes"));}\n   sum=sum.merge(shape.allocation(row_bytes.checked_mul(2).ok_or_else(||wire::wire_bad("native aux byte allocation overflow"))?,1)?)?;\n  }\n  sum=sum.allocation(count.checked_mul(2).ok_or_else(||wire::wire_bad("native aux Vec overflow"))?,size_of::<((u8,Vec<u8>),Vec<u8>)>())?;\n  sum=sum.allocation(count,size_of::<typed::BTreeNodeEnvelopeV1<(u8,Vec<u8>),Vec<u8>>>())?;\n  // Restore consistency builds per-job and per-producer counters from the\n  // actual aux rows. The total row count bounds both fixed-key scratch maps.\n  sum=sum.allocation(count.checked_mul(2).ok_or_else(||wire::wire_bad("native aux scratch count overflow"))?,size_of::<typed::BTreeNodeEnvelopeV1<[u8;64],u32>>())?;\n  sum.outer_entries=count;sum.collection_entries=sum.collection_entries.checked_add(count).ok_or_else(||wire::wire_bad("native aux count overflow"))?;\n  Ok(sum)\n }\n}\n'
(OUT.parent/'wire_kernel_aux.rs').write_text(code)
