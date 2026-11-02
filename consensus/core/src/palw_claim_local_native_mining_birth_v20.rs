//! Actual native39 original birth/hold journal receipt. Representation only;
//! collection and local replay never substitute for selected approval.
//! Context registry: owner20/schema1, root-confirmed 2026-11-02; distinct
//! from legacy owner18 and Source16/finance owner17. No old tag is reused.
use super::*;
use super::palw_claim_local_context_store_v2::{self as store,StateNodesV2};
use crate::palw_claim_local_context_nodes_v2::{self as nodes,ComponentRootV2,ComponentSpecV2,ContextErrorV2,RowValueV2};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use crate::palw_native_mining_v39::{NativeMiningClaimRecordV39 as Mining,MINING_TABLE_V39 as MINING,MINING_CLAIM_KIND_V39 as CLAIM,MINING_JOB_HOLD_KIND_V39 as HOLD,MINING_COLLECTED_KIND_V39 as COLLECTED};
use crate::palw_native_resource_v1::{NativeRetainedV1,NativeWireScanV1,NativeHeapV1,TirNativeRequestsV1,reserve_native_requests_v1};
use borsh::{BorshSerialize,BorshDeserialize};
const OWNER_V20:u8=20;
const SCHEMA_V20:u16=1;
const RECEIPTS:u32=1;
fn spec(table:u32)->ComponentSpecV2{ComponentSpecV2{owner:OWNER_V20,table,schema:SCHEMA_V20,key_bytes:64,value_bytes:u64::MAX}}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn native_err(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(e) => ContextErrorV2::Resource(e),
        _ => bad("native mining journal receipt journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("native mining journal receipt encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out)
        .map_err(|_| bad("native mining journal receipt encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || {
        borsh::from_slice(raw)
    })
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => {
            ContextErrorV2::Resource(r)
        }
        _ => bad("native mining journal receipt typed body codec"),
    })
}
fn value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 {
        data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
        bytes: raw.len() as u64,
    }
}
fn root(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let r = StateNodesV2(s);
    match scope {
        Some(id) => r.owned_current(id, spec(table)),
        None => r.current(spec(table)),
    }
}
fn lookup<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    let r = StateNodesV2(s);
    nodes::lookup(&r, root(s, table, scope)?, key.as_byte_slice())?
        .map(|v| {
            let body = r.body(v)?;
            decode(&body)
        })
        .transpose()
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
    row: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = row.map(encode).transpose()?;
    let r = StateNodesV2(&b.state);
    let before = root(&b.state, table, scope)?;
    let old = nodes::lookup(&r, before, key.as_byte_slice())?;
    let plan = nodes::update(
        &r,
        before,
        key.as_byte_slice(),
        old,
        raw.as_ref().map(|r| value(r)),
    )?;
    match scope {
        Some(id) => {
            store::publish_owned_component_v2(b, id, &plan, raw.as_deref().map(Vec::as_slice))?
        }
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    Ok(plan.after())
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct HoldV20{job:[u8;64],until:u64}
crate::palw_native_resource_v1::wire::wire_struct!(HoldV20;[u8;64],u64);
crate::palw_native_resource_v1::native_struct!(HoldV20;job,until);
#[derive(Debug,BorshSerialize,BorshDeserialize)]
struct ReceiptV20{
    version:u16,id:Hash64,birth:PalwBlockContextV2,
    original:Mining,holds:Vec<HoldV20>,liability_until:u64,
    latest:Mining,updated:PalwBlockContextV2,updated_ordinal:u64,collected:Option<PalwBlockContextV2>,anchors:u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(ReceiptV20;u16,Hash64,PalwBlockContextV2,Mining,Vec<HoldV20>,u64,Mining,PalwBlockContextV2,u64,Option<PalwBlockContextV2>,u64);
crate::palw_native_resource_v1::native_struct!(ReceiptV20;version,id,birth,original,holds,liability_until,latest,updated,updated_ordinal,collected,anchors);
fn immutable(c:&Mining)->impl BorshSerialize+'_ {
    (c.version,c.id,c.model,c.master,c.master_final,c.master_established,c.complete_source,&c.stages,c.producer,c.operator,c.admitted_daa,c.epoch,c.policy,&c.work,&c.price)
}
pub(super) fn same_native_source_v20(a:&Mining,b:&Mining)->Result<bool,ContextErrorV2>{
    // Exact stream comparison with no duplicate native row construction.
    let a=encode(&immutable(a))?;let b=encode(&immutable(b))?;Ok(a.as_slice()==b.as_slice())
}
fn native_key(kind:u8,id:Hash64)->[u8;66]{let mut out=[0;66];out[0]=39;out[1]=kind;out[2..].copy_from_slice(id.as_byte_slice());out}
fn raw<'a>(s:&'a PalwChainStateV2,kind:u8,id:Hash64)->Result<Option<&'a Vec<u8>>,ContextErrorV2>{
    let Some(route)=s.kernel_route.as_ref()else{return Ok(None)};
    let _lease=reserve_native_requests_v1(66).map_err(ContextErrorV2::Resource)?;
    Ok(route.aux.get(&(MINING,native_key(kind,id).to_vec())))
}
fn native_claim(raw:&[u8],id:Hash64)->Result<NativeRetainedV1<([u8;64],Mining)>,ContextErrorV2>{
    let row=decode::<([u8;64],Mining)>(raw)?;
    if row.0!=row.1.root()||row.1.version!=39||row.1.id!=id.as_bytes(){return Err(bad("mining20 actual native ID/root"))}Ok(row)
}
fn read(s:&PalwChainStateV2,id:Hash64)->Result<Option<NativeRetainedV1<ReceiptV20>>,ContextErrorV2>{lookup(s,RECEIPTS,None,id)}
fn shape(r:&ReceiptV20)->Result<(),ContextErrorV2>{
    if r.version!=1||r.original.version!=39||r.id.as_bytes()!=r.original.id||r.original.closed.is_some()||r.birth.block==Hash64::default()||r.birth.daa_score!=r.original.admitted_daa||r.updated.blue_score<r.birth.blue_score||r.updated.daa_score<r.birth.daa_score||r.holds.is_empty()||!same_native_source_v20(&r.original,&r.latest)?{return Err(bad("mining20 original/birth/native identity"))}
    if r.holds.windows(2).any(|x|x[0].job>=x[1].job)||r.holds.iter().any(|h|h.until<r.birth.daa_score)||r.holds.iter().map(|h|h.until).max()!=Some(r.liability_until){return Err(bad("mining20 actual signed job-hold maximum"))}
    if r.collected.is_some_and(|p|p!=r.updated){return Err(bad("mining20 actual collection/hold clock"))}Ok(())
}
fn preserve_closed(a:&Mining,b:&Mining)->bool{a.closed.is_none()||b.closed.is_some()}
fn retain_birth(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,original:&Mining,ordinal:u64,actual_holds:&BTreeMap<(Hash64,[u8;64]),u64>)->Result<(),ContextErrorV2>{
    if original.admitted_daa!=ctx.daa_score||original.closed.is_some(){return Err(bad("mining20 original is not native birth"))}
    if read(&b.state,id)?.is_some(){return Err(bad("mining20 actual birth already captured"))}
    let lo=(id,[0;64]);let hi=(id,[255;64]);let count=actual_holds.range(lo..=hi).count()as u64;
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<HoldV20>(count).map_err(ContextErrorV2::Resource)?;
    let h=original.native_heap_v1("mining20 actual original/current");q.add_v1(h.bytes().ok_or_else(||ContextErrorV2::Resource(h))?.checked_mul(2).ok_or_else(||bad("mining20 actual original copy overflow"))?).map_err(ContextErrorV2::Resource)?;
    b.native_reservation.grow_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let holds=actual_holds.range(lo..=hi).map(|((_,job),until)|HoldV20{job:*job,until:*until}).collect::<Vec<_>>();
    let liability_until=holds.iter().map(|h|h.until).max().ok_or_else(||bad("mining20 native birth lacks actual source job holds"))?;
    let row=ReceiptV20{version:1,id,birth:*ctx,original:original.clone(),holds,liability_until,latest:original.clone(),updated:*ctx,updated_ordinal:ordinal,collected:None,anchors:0};shape(&row)?;
    put(b,RECEIPTS,None,id,Some(&row))?;Ok(())
}
fn replace_latest(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,old:&Mining,next:Option<&Mining>,ordinal:u64)->Result<(),ContextErrorV2>{
    let Some(row)=read(&b.state,id)?else{return Ok(())}; // historical F1 remains unchanged
    shape(&row)?;
    if row.latest!=*old||row.collected.is_some()||!same_native_source_v20(&row.original,old)?{return Err(bad("mining20 actual update differs from captured current row"))}
    if let Some(next)=next{if !same_native_source_v20(old,next)?||!preserve_closed(old,next){return Err(bad("mining20 native immutable row or closed outcome changed"))}}
    else{
        let tombstone=raw(&b.state,COLLECTED,id)?.ok_or_else(||bad("mining20 collection lacks actual tombstone"))?;
        let(root,closed,at):([u8;64],Option<(u64,bool)>,u64)=borsh::from_slice(tombstone).map_err(|_|bad("mining20 actual tombstone codec"))?;
        if root!=old.root()||closed!=old.closed||at!=ctx.daa_score{return Err(bad("mining20 tombstone/actual native hold differs"))}
    }
    let encoded=encode(&*row)?;
    let h=row.native_heap_v1("mining20 actual current replacement");b.native_reservation.grow_v1(h.bytes().ok_or_else(||ContextErrorV2::Resource(h))?).map_err(ContextErrorV2::Resource)?;
    let mut next_row:ReceiptV20=borsh::from_slice(&encoded).map_err(|_|bad("mining20 independently owned current receipt"))?;
    next_row.latest=next.unwrap_or(old).clone();next_row.updated=*ctx;next_row.updated_ordinal=ordinal;next_row.collected=next.is_none().then_some(*ctx);
    shape(&next_row)?;put(b,RECEIPTS,None,id,Some(&next_row))?;Ok(())
}
fn anchor_change(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,source:&Mining,add:bool)->Result<(),ContextErrorV2>{
    let Some(row)=read(&b.state,id)?else{return Ok(())};
    shape(&row)?;if source.id!=id.as_bytes()||!same_native_source_v20(&row.original,source)?{return Err(bad("mining20 actual anchor changes original native source"))}
    let raw=encode(&*row)?;let h=row.native_heap_v1("mining20 anchor reference replacement");b.native_reservation.grow_v1(h.bytes().ok_or_else(||ContextErrorV2::Resource(h))?).map_err(ContextErrorV2::Resource)?;
    let mut next:ReceiptV20=borsh::from_slice(&raw).map_err(|_|bad("mining20 owned anchor reference receipt"))?;
    next.anchors=if add{next.anchors.checked_add(1)}else{next.anchors.checked_sub(1)}.ok_or_else(||bad("mining20 actual anchor count"))?;
    let _=ctx;put(b,RECEIPTS,None,id,Some(&next))?;Ok(())
}
fn anchor(raw:&[u8])->Result<NativeRetainedV1<crate::palw_native39_frontier_v1::Native39FrontierAnchorV1>,ContextErrorV2>{
    let row=decode::<crate::palw_native39_frontier_v1::Native39FrontierAnchorV1>(raw)?;
    if row.version!=1||row.claim.as_bytes()!=row.claim_source.id||row.claim_root.as_bytes()!=row.claim_source.root(){return Err(bad("mining20 actual anchor source root"))}Ok(row)
}
/// Only the actual successful native journal can mint/update this receipt.
/// No public raw Mining->birth constructor, no current-policy/DAA fallback.
pub(super) fn capture_native_journal_v20(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<(),PalwStateV2Error>{
    capture_native_journal_inner_v21(b,ctx,None)
}
/// Opaque, transition-local owner evidence. This is not persisted approval,
/// funding, source availability or a producer-supplied readiness capability.
pub(super) struct NativeMiningEventCoverageV21 {point:PalwBlockContextV2,events:BTreeMap<u64,Hash64>}
fn event_identity_v21(point:PalwBlockContextV2,ordinal:u64,event:&PalwDeltaEntryV2)->Result<Hash64,ContextErrorV2>{
    struct Sink(blake2b_simd::State);
    impl std::io::Write for Sink{fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0.update(b);Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
    let mut out=Sink(blake2b_simd::Params::new().hash_length(64).key(b"misaka/native20/event-identity/v21").to_state());
    borsh::BorshSerialize::serialize(&(point,ordinal,event),&mut out).map_err(|_|bad("mining20 actual event identity codec"))?;
    let mut digest=[0;64];digest.copy_from_slice(out.0.finalize().as_bytes());Ok(Hash64::from_bytes(digest))
}
impl NativeMiningEventCoverageV21{
    pub(super) fn covers_event_v21(&self,point:PalwBlockContextV2,ordinal:u64,event:&PalwDeltaEntryV2)->Result<bool,PalwStateV2Error>{
        if self.point!=point{return Ok(false)}
        let Some(expected)=self.events.get(&ordinal)else{return Ok(false)};
        Ok(*expected==event_identity_v21(point,ordinal,event).map_err(err)?)
    }
}
/// Explicit new path; the unchanged old wrapper allocates/hashes no coverage
/// inventory. Reserve this inventory BEFORE any source/native mutation.
pub(super) fn capture_native_journal_with_coverage_v21(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<NativeRetainedV1<NativeMiningEventCoverageV21>,PalwStateV2Error>{
    if !b.params.claim_local_settlement_active_at(ctx.daa_score)||b.claim_local_lock_point_v6!=Some(*ctx){return Err(err(bad("mining20 event coverage lacks actual active point")))}
    let count=b.entries.iter().filter(|e|matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if (*table==MINING&&key.len()==66&&key[0]==39&&matches!(key[1],CLAIM|HOLD))||*table==crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1)).count();
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<NativeMiningEventCoverageV21>(1).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<u64,Hash64>>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut coverage=NativeMiningEventCoverageV21{point:*ctx,events:BTreeMap::new()};
    capture_native_journal_inner_v21(b,ctx,Some(&mut coverage.events))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(coverage,lease))
}
fn capture_native_journal_inner_v21(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,mut coverage:Option<&mut BTreeMap<u64,Hash64>>)->Result<(),PalwStateV2Error>{
    if !b.params.claim_local_settlement_active_at(ctx.daa_score){return Ok(())}
    if b.claim_local_lock_point_v6!=Some(*ctx){return Err(err(bad("mining20 lacks actual executing point")))}
    let end=b.entries.len();let first=end;let gc=b.claim_local_gc_sequence_v3;
    let result=(||{
    let mut holds_n=0u64;let mut refs_n=0u64;
    for e in &b.entries[..end]{if let PalwDeltaEntryV2::KernelRouteRow{table,key,new,..}=e{
        if *table==MINING&&key.len()==66&&key[..2]==[39,HOLD]&&new.is_some(){holds_n=holds_n.checked_add(1).ok_or_else(||bad("mining20 journal hold count"))?;}
        if *table==MINING&&key.len()==66&&key[..2]==[39,CLAIM]{refs_n=refs_n.checked_add(1).ok_or_else(||bad("mining20 journal claim count"))?;}
        if *table==crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1{refs_n=refs_n.checked_add(2).ok_or_else(||bad("mining20 journal anchor count"))?;}
    }}
    if holds_n==0&&refs_n==0&&coverage.is_none(){return Ok(())}
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(Hash64,[u8;64]),u64>>(holds_n).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64,()>>(refs_n).map_err(ContextErrorV2::Resource)?;
    let _journal_lease=reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut actual_holds=BTreeMap::new();
    for e in &b.entries[..end]{if let PalwDeltaEntryV2::KernelRouteRow{table,key,new:Some(raw),..}=e{
        if *table==MINING&&key.len()==66&&key[..2]==[39,HOLD]{let(claim,until):([u8;64],u64)=borsh::from_slice(raw).map_err(|_|bad("mining20 actual job-hold codec"))?;
            let job=key[2..].try_into().map_err(|_|bad("mining20 actual job identity"))?;
            if actual_holds.insert((Hash64::from_bytes(claim),job),until).is_some_and(|old|old!=until){return Err(bad("mining20 same-admission job hold changed"))}
        }
    }}
    let mut native_ordinal=0u64;
    for at in 0..end{
        // Identical native event numbering to F1/Source10: auxiliary Context
        // rows are excluded, while chain/EXEC/native route mutations count.
        let entry=&b.entries[at];let ordinal=native_ordinal;
        if matches!(entry,PalwDeltaEntryV2::Claim{..}|PalwDeltaEntryV2::ExecV2Row{..})||matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,..}if *table!=PALW_CLAIM_LOCAL_TABLE_V1){native_ordinal=native_ordinal.checked_add(1).ok_or_else(||bad("mining20 native event ordinal overflow"))?;}
        let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=entry else{continue};
        if *table!=MINING||key.len()!=66||key[..2]!=[39,CLAIM]{continue}
        let id=Hash64::from_bytes(key[2..].try_into().map_err(|_|bad("mining20 native claim key width"))?);
        let represented=coverage.is_some()&&(old.is_none()||read(&b.state,id)?.is_some());
        let identity=if represented{Some(event_identity_v21(*ctx,ordinal,entry)?)}else{None};
        let old=old.as_ref().map(|v|native_claim(v,id)).transpose()?;let new=new.as_ref().map(|v|native_claim(v,id)).transpose()?;
        match(old.as_deref(),new.as_deref()){
            (None,Some((_,row)))=>retain_birth(b,ctx,id,row,ordinal,&actual_holds)?,
            (Some((_,old)),next)=>replace_latest(b,ctx,id,old,next.map(|(_,v)|v),ordinal)?,
            (None,None)=>return Err(bad("mining20 empty native mutation")),
        }
        if let(Some(out),Some(identity))=(coverage.as_mut(),identity){out.insert(ordinal,identity);}
    }
    // Frontier references are captured from actual native writes, never by an
    // all-source snapshot or guessed expiry. The reference follows collection.
    let mut touched=BTreeSet::new();
    for at in 0..end{
        let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=&b.entries[at]else{continue};
        if *table==MINING&&key.len()==66&&key[..2]==[39,CLAIM]{touched.insert(Hash64::from_bytes(key[2..].try_into().map_err(|_|bad("mining20 touched claim key"))?));}
        if *table!=crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1{continue}
        let old=old.as_deref().map(anchor).transpose()?;let new=new.as_deref().map(anchor).transpose()?;
        for a in old.iter().chain(new.iter()){
            if key.as_slice()!=crate::palw_native39_frontier_v1::native39_frontier_identity_v1(&a.work,a.quantum).as_byte_slice(){return Err(bad("mining20 actual frontier key/work identity"))}
        }
        if let(Some(old),Some(new))=(&old,&new){if **old==**new{continue}return Err(bad("mining20 immutable native frontier anchor replaced"))}
        if let Some(old)=old{touched.insert(old.claim);anchor_change(b,ctx,old.claim,&old.claim_source,false)?;}
        if let Some(new)=new{touched.insert(new.claim);anchor_change(b,ctx,new.claim,&new.claim_source,true)?;}
    }
    if let Some(out)=coverage.as_mut(){
        let mut ordinal=0u64;
        for entry in &b.entries[..end]{
            let native=matches!(entry,PalwDeltaEntryV2::Claim{..}|PalwDeltaEntryV2::ExecV2Row{..})||matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,..}if *table!=PALW_CLAIM_LOCAL_TABLE_V1);
            let current=ordinal;if native{ordinal=ordinal.checked_add(1).ok_or_else(||bad("mining20 coverage ordinal overflow"))?;}
            let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=entry else{continue};
            let represented=if *table==MINING&&key.len()==66&&key[..2]==[39,HOLD]{
                let job:[u8;64]=key[2..].try_into().map_err(|_|bad("mining20 coverage actual job key"))?;
                let mut represented=true;
                for raw in old.iter().chain(new.iter()){
                    let(id,until):([u8;64],u64)=borsh::from_slice(raw).map_err(|_|bad("mining20 coverage actual job hold"))?;
                    let Some(row)=read(&b.state,Hash64::from_bytes(id))?else{represented=false;break};
                    shape(&row)?;
                    if row.holds.binary_search_by_key(&job,|h|h.job).ok().is_none_or(|at|row.holds[at].until!=until){represented=false;break}
                }
                represented&&(old.is_some()||new.is_some())
            }else if *table==crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1{
                let mut represented=true;
                for raw in old.iter().chain(new.iter()){
                    let a=anchor(raw)?;
                    let Some(row)=read(&b.state,a.claim)?else{represented=false;break};
                    shape(&row)?;
                    if !same_native_source_v20(&row.original,&a.claim_source)?{represented=false;break}
                }
                represented&&(old.is_some()||new.is_some())
            }else{false};
            if represented{out.insert(current,event_identity_v21(*ctx,current,entry)?);}
        }
    }
    for id in touched{if let Some(row)=read(&b.state,id)?{if row.collected.is_some()&&row.anchors==0{
        // Identical source liveness to the existing native checkpoint: no live
        // native Claim and no remaining actual work anchor. Independent A2
        // consumers retain any immutable literal versions they really own.
        if raw(&b.state,CLAIM,id)?.is_some(){return Err(bad("mining20 collected source is still live"))}
        put::<ReceiptV20>(b,RECEIPTS,None,id,None)?;
    }}}
    Ok(())})();
    if result.is_err(){super::palw_claim_local_context_gc_v3::rollback_aux_v3(b,first,gc,|table,key|table==PALW_CLAIM_LOCAL_TABLE_V1&&matches!(key.first(),Some(b'N'|b'B'|b'I'))).map_err(err)?;}
    result.map_err(err)
}
fn check_current(s:&PalwChainStateV2,r:&ReceiptV20)->Result<(),ContextErrorV2>{
    shape(r)?;
    match(raw(s,CLAIM,r.id)?,r.collected){
        (Some(raw),None)=>{let native=native_claim(raw,r.id)?;if native.1!=r.latest{return Err(bad("mining20 current native source changed"))}},
        (None,Some(at))=>{let raw=raw(s,COLLECTED,r.id)?.ok_or_else(||bad("mining20 collected native tombstone absent"))?;
            let(root,closed,daa):([u8;64],Option<(u64,bool)>,u64)=borsh::from_slice(raw).map_err(|_|bad("mining20 retained collection tombstone codec"))?;
            if(root,closed,daa)!=(r.latest.root(),r.latest.closed,at.daa_score)||r.anchors==0{return Err(bad("mining20 collected source ownership differs"))}},
        _=>return Err(bad("mining20 native current/collection identity")),
    }Ok(())
}
fn validate_profiles_v20(s:&PalwChainStateV2)->Result<(),PalwStateV2Error>{
    // Reject orphan/unknown owner20 profiles BEFORE the expected empty-root
    // fast path. No other table, schema or owned-current grammar is allocated.
    if let Some(route)=&s.kernel_route{for((table,key),raw)in &route.aux{
        if *table!=PALW_CLAIM_LOCAL_TABLE_V1||key.first()!=Some(&store::CONTEXT_CURRENT_PREFIX_V2){continue}
        let rooted=decode::<ComponentRootV2>(raw).map_err(err)?;
        let key_owner=match key.len(){18=>key.get(1),84=>key.get(67),_=>None};
        if rooted.spec.owner==OWNER_V20||key_owner==Some(&OWNER_V20){
            let mut expected=[0u8;18];expected[0]=store::CONTEXT_CURRENT_PREFIX_V2;
            spec(RECEIPTS).serialize(&mut &mut expected[1..]).map_err(|_|err(bad("mining20 current key codec")))?;
            if rooted.spec!=spec(RECEIPTS)||key.as_slice()!=expected{return Err(err(bad("mining20 unallocated native profile or current-root grammar")))}
        }
    }}
    Ok(())
}
/// Check persisted facts against the actual native rows; restore never mints.
/// The only population census here is the ordinary cold integrity pass.
pub(super) fn validate_native_receipts_v20(s:&PalwChainStateV2,p:&PalwStateParamsV2)->Result<(),PalwStateV2Error>{
    validate_profiles_v20(s)?;
    if root(s,RECEIPTS,None).map_err(err)?.rows==0{return Ok(())}
    let anchors=crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(s)?;
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64,u64>>(anchors.len()as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;let mut counts=BTreeMap::<Hash64,u64>::new();
    for(_,a)in anchors.iter(){let n=counts.entry(a.claim).or_default();*n=n.checked_add(1).ok_or(PalwStateV2Error::Overflow("mining20 cold anchor count"))?;
        if let Some(row)=read(s,a.claim).map_err(err)?{if !same_native_source_v20(&row.original,&a.claim_source).map_err(err)?{return Err(err(bad("mining20 actual frontier original source changed")))}}
    }
    nodes::complete_rows(&StateNodesV2(s),root(s,RECEIPTS,None).map_err(err)?,|key,rv|{
        let r=StateNodesV2(s);let bytes=r.body(rv)?;let row=decode::<ReceiptV20>(&bytes)?;check_current(s,&row)?;
        if key!=row.id.as_byte_slice()||!p.claim_local_settlement_active_at(row.birth.daa_score)||s.last_point.is_none_or(|tip|row.updated.blue_score>tip.blue_score||row.updated.daa_score>tip.daa_score)||counts.get(&row.id).copied().unwrap_or(0)!=row.anchors{return Err(bad("mining20 persisted clock/anchor census"))}
        Ok(())
    }).map_err(err)
}
/// Typed source coordinate, frozen actual hold and latest real native body.
/// Every returned raw envelope borrows actual immutable component storage.
pub(super) fn visit_receipts_v20<'a>(s:&'a PalwChainStateV2,mut f:impl FnMut(Hash64,PalwBlockContextV2,u64,PalwBlockContextV2,u64,&Mining,&'a Vec<u8>)->Result<(),PalwStateV2Error>)->Result<(),PalwStateV2Error>{
    let r=StateNodesV2(s);nodes::complete_rows(&r,root(s,RECEIPTS,None).map_err(err)?,|key,rv|{
        let bytes=r.body(rv)?;let row=decode::<ReceiptV20>(&bytes)?;check_current(s,&row)?;
        if key!=row.id.as_byte_slice(){return Err(bad("mining20 borrowed coordinate key"))}
        let _lease=reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;let mut k=[0;65];k[0]=b'B';k[1..].copy_from_slice(rv.data.as_byte_slice());
        let bytes=s.kernel_route.as_ref().and_then(|r|r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1,k.to_vec()))).ok_or_else(||bad("mining20 borrowed coordinate disappeared"))?;
        f(row.id,row.birth,row.liability_until,row.updated,row.updated_ordinal,&row.latest,bytes).map_err(native_err)
    }).map_err(err)
}
/// No F1 arming: this is an exact event-coverage query for its eventual caller.
pub(super) fn has_receipt_v20(s:&PalwChainStateV2,id:Hash64)->Result<bool,PalwStateV2Error>{
    let Some(row)=read(s,id).map_err(err)?else{return Ok(false)};check_current(s,&row).map_err(err)?;Ok(true)
}
/// Current checkpoint material is this exact typed receipt plus this subject's
/// actual native certification row. Missing approval remains missing; retaining
/// bytes does not certify anything. No unrelated route/book is cloned.
pub(super) fn visit_checkpoint_material_v20(s:&PalwChainStateV2,id:Hash64,mut f:impl FnMut(&[u8])->Result<(),PalwStateV2Error>)->Result<bool,PalwStateV2Error>{
    let r=StateNodesV2(s);let Some(rv)=nodes::lookup(&r,root(s,RECEIPTS,None).map_err(err)?,id.as_byte_slice()).map_err(err)?else{return Ok(false)};
    let bytes=r.body(rv).map_err(err)?;let row=decode::<ReceiptV20>(&bytes).map_err(err)?;check_current(s,&row).map_err(err)?;
    let q=encode(&(1u16,id,rv,row.collected)).map_err(err)?;f(&q)?;f(&bytes)?;
    if let Some(raw)=raw(s,CLAIM,id).map_err(err)?{f(raw)?;}
    let _lease=reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    use crate::palw_security_certification_v1 as cert;
    let key=cert::certification_native_key_v39(&cert::palw_native_certification_id_v39(&id));
    let approval=s.kernel_route.as_ref().and_then(|r|r.aux.get(&(cert::PALW_SECURITY_CERTIFICATION_TABLE_V1,key)));
    // Bind absence explicitly in the typed local material, with no replacement
    // approval or native-closure capability inferred from receipt retention.
    let q=encode(&(1u16,id,approval.map(Vec::as_slice))).map_err(err)?;f(&q)?;Ok(true)
}
/// Read-only data identity summary; no approval or monetary capability.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PalwNativeMiningSourceSummaryV20{
    pub claim:Hash64,pub birth:PalwBlockContextV2,pub liability_until:u64,
    pub original_root:Hash64,pub latest_root:Hash64,pub updated:PalwBlockContextV2,pub updated_ordinal:u64,pub collected:Option<PalwBlockContextV2>,pub anchor_references:u64,
}
impl PalwChainStateV2{
    pub fn native_mining_source_summary_v20(&self,id:Hash64)->Result<Option<PalwNativeMiningSourceSummaryV20>,PalwStateV2Error>{
        let Some(row)=read(self,id).map_err(err)?else{return Ok(None)};check_current(self,&row).map_err(err)?;
        Ok(Some(PalwNativeMiningSourceSummaryV20{claim:id,birth:row.birth,liability_until:row.liability_until,original_root:Hash64::from_bytes(row.original.root()),latest_root:Hash64::from_bytes(row.latest.root()),updated:row.updated,updated_ordinal:row.updated_ordinal,collected:row.collected,anchor_references:row.anchors}))
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn metadata_state(spec:ComponentSpecV2,owned:bool)->PalwChainStateV2{
        let mut s=PalwChainStateV2::genesis();
        let mut route=crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_u64_word(1),Hash64::from_u64_word(2)),None,
            misaka_palw_kernel::rows::LedgerScalarsV1{daa:0,burned:0});
        let mut key=if owned{let mut k=vec![0u8;84];k[0]=b'I';k[1..3].copy_from_slice(&2u16.to_le_bytes());k}else{let mut k=vec![0u8;18];k[0]=b'I';k};
        let at=if owned{67}else{1};spec.serialize(&mut &mut key[at..]).unwrap();
        route.aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,key),borsh::to_vec(&ComponentRootV2::empty(spec).unwrap()).unwrap());s.kernel_route=Some(route);s
    }
    #[test]
    fn owner20_empty_expected_directory_does_not_hide_orphan_profiles(){
        assert!(validate_profiles_v20(&metadata_state(spec(RECEIPTS),false)).is_ok());
        for field in 0..4{let mut unsupported=spec(RECEIPTS);match field{0=>unsupported.table=2,1=>unsupported.schema=2,2=>unsupported.key_bytes=63,_=>unsupported.value_bytes=0};
            let s=metadata_state(unsupported,false);assert_eq!(root(&s,RECEIPTS,None).unwrap().rows,0);
            assert!(validate_profiles_v20(&s).is_err(),"an unallocated owner20 profile cannot hide behind its empty expected table");
        }
        assert!(validate_profiles_v20(&metadata_state(spec(RECEIPTS),true)).is_err(),"owner20 allocates no owned-current key grammar");
        let mut other=spec(RECEIPTS);other.owner=21;other.schema=2;
        assert!(validate_profiles_v20(&metadata_state(other,false)).is_ok(),"another owner's codec remains its own validator's responsibility");
    }
}

/// Current source count for native checkpoint allocation inventory. This reads
/// only the actual component header; malformed bodies keep their constructor's
/// later semantic error, while metadata resource refusals retain their class.
pub(super) fn source_count_v20(s:&PalwChainStateV2)->Result<u64,PalwStateV2Error>{Ok(root(s,RECEIPTS,None).map_err(err)?.rows)}

/// Event checker for the separate native v21 partition. The native receipt and
/// exact current checkpoint material own this event; a raw ID or source body
/// cannot mint coverage. Collection without a surviving scoped owner remains
/// an explicit gap until its terminal checkpoint owner is integrated.
pub(super) fn covers_native_current_event_v21(s:&PalwChainStateV2,id:Hash64,point:PalwBlockContextV2,ordinal:u64,entry:&PalwDeltaEntryV2)->Result<bool,PalwStateV2Error>{
 let Some(row)=read(s,id).map_err(err)?else{return Ok(false)};check_current(s,&row).map_err(err)?;
 let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=entry else{return Ok(false)};
 if *table==MINING&&key.as_slice()==native_key(CLAIM,id){
  if row.updated!=point||row.updated_ordinal!=ordinal{return Ok(false)}
  let before=old.as_deref().map(|v|native_claim(v,id)).transpose().map_err(err)?;
  let after=new.as_deref().map(|v|native_claim(v,id)).transpose().map_err(err)?;
  match(before.as_deref(),after.as_deref()){
   (None,Some((_,next)))=>{if row.birth!=point||row.original!=*next||row.collected.is_some(){return Ok(false)}},
   (Some((_,prior)),Some((_,next)))=>{if row.latest!=*next||!same_native_source_v20(prior,next).map_err(err)?||row.collected.is_some(){return Ok(false)}},
   (Some((_,prior)),None)=>{if row.latest!=*prior||row.collected!=Some(point){return Ok(false)}},
   (None,None)=>return Err(err(bad("coverage empty native mining mutation"))),
  }
 }else{
  use crate::palw_security_certification_v1 as cert;
  let _key=reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
  let wanted=cert::certification_native_key_v39(&cert::palw_native_certification_id_v39(&id));
  if *table!=cert::PALW_SECURITY_CERTIFICATION_TABLE_V1||key.as_slice()!=wanted.as_slice(){return Ok(false)}
  let actual=s.kernel_route.as_ref().and_then(|r|r.aux.get(&(*table,wanted)));
  if actual.map(Vec::as_slice)!=new.as_deref(){return Ok(false)}
 }
 visit_checkpoint_material_v20(s,id,|_|Ok(()))
}
