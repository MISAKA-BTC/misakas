//! Current owner-scoped material. References resolve in the same authenticated
//! state as the checkpoint; historical birth/approval frames remain independent.
//! Internal execution spans identify non-credit writes, never producer authority.
use super::*;
use crate::palw_claim_local_v1::*;
use crate::palw_native_resource_v1::{NativeRetainedV1, TirNativeRequestsV1, BTreeNodeEnvelopeV1};
use crate::palw_court_family_material_v1 as material;
use crate::palw_court_family_supplier_v1 as finance;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FamilyEventV2 { Delivery(Hash64), Expiry, Finance, ConsumedCashQueue(Hash64) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FamilySpanV2 { pub claim: Hash64, pub event: FamilyEventV2, pub first: usize, pub end: usize }
fn bad(s:&str)->PalwStateV2Error { PalwStateV2Error::CarriageInconsistent(s.into()) }
fn resource(s:&'static str)->PalwStateV2Error { PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<FamilySpanV2>(s)) }

/// Append only after the owning fold successfully authenticated/executed its
/// operation. Storage belongs to the transition reservation, including rollback.
pub(super) fn mark(b:&mut TransitionBuilder<'_>,claim:Hash64,event:FamilyEventV2,first:usize)->Result<(),PalwStateV2Error>{
    if !b.params.claim_local_settlement_active_at(b.state.last_point.map_or(0,|p|p.daa_score)) { return Ok(()) }
    let end=b.entries.len();
    if first>end{return Err(bad("family non-credit journal span is reversed"))}
    if first==end{return Ok(())}
    b.native_reservation.grow_v1(std::mem::size_of::<FamilySpanV2>() as u64).map_err(PalwStateV2Error::NativeResource)?;
    b.claim_local_family_spans.try_reserve_exact(1).map_err(|_|resource("family journal span allocation"))?;
    b.claim_local_family_spans.push(FamilySpanV2{claim,event,first,end});Ok(())
}

/// Expiry runs after finance GC. A just-deleted immutable header is resolved
/// from this transition's real journal, never from a guessed claim hash.
pub(super) fn expiry_owner(b:&TransitionBuilder<'_>,key:&[u8],raw:&[u8])->Result<Hash64,PalwStateV2Error>{
    if key.len()==65&&key[0]==4{return hash_at(raw,0)}
    let contract=if key.len()==73&&key[0]==2{
        let draft=hash_at(raw,0)?;
        let _lease=crate::palw_native_resource_v1::reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        let draft_key=material::material_aux_key_v1(1,draft,None);
        let row=b.state.kernel_route.as_ref().and_then(|r|r.aux.get(&(material::FAMILY_MATERIAL_TABLE_V1,draft_key))).ok_or_else(||bad("expiring material part lost draft"))?;
        hash_at(row,0)?
    }else if key.len()==65&&matches!(key[0],1|3|5|6){hash_at(raw,0)?}else{return Err(bad("unallocated expiring family material"))};
    let bytes=b.state.kernel_route.as_ref().and_then(|r|r.aux.iter().find_map(|((t,k),v)|(*t==finance::FAMILY_SUPPLIER_TABLE_V1&&k.as_slice()==contract.as_byte_slice()).then_some(v)))
        .or_else(||b.entries.iter().rev().find_map(|entry|match entry{PalwDeltaEntryV2::KernelRouteRow{table,key,old:Some(old),new:None} if *table==finance::FAMILY_SUPPLIER_TABLE_V1&&key.as_slice()==contract.as_byte_slice()=>Some(old),_=>None}))
        .ok_or_else(||bad("expiring material lost its actual supplier header"))?;
    let row=crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes,||borsh::from_slice::<finance::FamilySupplierRowV1>(bytes)).map_err(|e|match e{crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r)=>PalwStateV2Error::NativeResource(r),crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)=>bad("expiring material supplier header malformed")})?;
    if row.consent.id_v1().map_err(|_|bad("expiring supplier identity"))?!=contract{return Err(bad("expiring material supplier identity differs"))}
    Ok(row.consent.terms.claim)
}

pub(super) struct Coordinate<'a>{pub claim:Hash64,pub table:u8,pub key:&'a [u8],bytes:&'a [u8]}
/// Exact identity of an actual native coordinate, never a producer index flag.
pub(super) fn coordinate_identity_v3(c:&Coordinate<'_>)->(Hash64,u64){(palw_liability_material_id_v1(c.bytes),c.bytes.len() as u64)}
fn hash_at(raw:&[u8],offset:usize)->Result<Hash64,PalwStateV2Error>{
    Ok(Hash64::from_bytes(raw.get(offset..offset+64).ok_or_else(||bad("short scoped material identity"))?.try_into().expect("fixed identity")))
}
fn own_contract(rows:&[(Hash64,finance::FamilySupplierRowV1)],id:Hash64)->Result<Hash64,PalwStateV2Error>{
    rows.binary_search_by_key(&id,|r|r.0).ok().map(|i|rows[i].1.consent.terms.claim).ok_or_else(||bad("scoped material contract missing"))
}
pub(super) fn coordinates(state:&PalwChainStateV2)->Result<NativeRetainedV1<Vec<Coordinate<'_>>>,PalwStateV2Error>{
    let rows=state.family_supplier_rows_v1()?;
    let count=state.kernel_route.as_ref().map_or(0,|r|r.aux.keys().filter(|(t,_)|matches!(*t,169|192)).count());
    let mut quote=TirNativeRequestsV1::new();
    quote.exact_v1::<Coordinate<'_>>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    quote.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Hash64>>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease=crate::palw_native_resource_v1::reserve_native_requests_v1(quote.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut drafts=BTreeMap::new();let mut out=Vec::with_capacity(count);
    if let Some(route)=&state.kernel_route {
        for ((table,key),raw) in &route.aux {if *table==material::FAMILY_MATERIAL_TABLE_V1&&key.len()==65&&key[0]==1{
            drafts.insert(hash_at(key,1)?,own_contract(&rows,hash_at(raw,0)?)?);
        }}
        for ((table,key),raw) in &route.aux {
            let claim=match *table {
                material::FAMILY_MATERIAL_TABLE_V1=>match (key.len(),key.first().copied()) {
                    (65,Some(1|3|5|6))=>own_contract(&rows,hash_at(raw,0)?)?,
                    (73,Some(2))=>{let draft=hash_at(raw,0)?;if hash_at(key,1)?!=draft{return Err(bad("scoped material part draft identity"))}*drafts.get(&draft).ok_or_else(||bad("scoped material part lost draft"))?},
                    (65,Some(4))=>{let claim=hash_at(raw,0)?;if hash_at(key,1)?!=claim{return Err(bad("scoped table claim identity"))}claim},
                    _=>return Err(bad("unallocated scoped family material row")),
                },
                finance::FAMILY_SUPPLIER_TABLE_V1=>match key.len(){
                    64=>own_contract(&rows,hash_at(key,0)?)?,
                    65 if matches!(key[0],1|4)=>{let id=hash_at(raw,2)?;if id!=hash_at(key,1)?{return Err(bad("scoped finance status identity"))}own_contract(&rows,id)?},
                    129 if matches!(key[0],2|3)=>{let id=hash_at(raw,2)?;if id!=hash_at(key,1)?{return Err(bad("scoped finance completion identity"))}own_contract(&rows,id)?},
                    _=>return Err(bad("unallocated scoped family finance row")),
                },
                _=>continue,
            };out.push(Coordinate{claim,table:*table,key,bytes:raw});
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out,lease))
}

/// Exact retained copies plus metadata construction. The checkpoint owner holds
/// this inventory's demand throughout use; temporary decoders retain their own.
pub(super) fn requests(state:&PalwChainStateV2)->Result<u64,PalwStateV2Error>{
    let mut q=TirNativeRequestsV1::new();let mut count=0u64;
    if let Some(route)=&state.kernel_route{for((table,key),raw)in &route.aux{if !matches!(*table,169|192){continue}count=count.checked_add(1).ok_or_else(||resource("scoped row count"))?;
        q.exact_v1::<u8>(raw.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        // Key+digest+length+table, Borsh vector growth and ordered index nodes.
        q.exact_v1::<u8>((key.len() as u64+73).checked_mul(3).ok_or_else(||resource("scoped key count"))?).map_err(PalwStateV2Error::NativeResource)?;
    }}
    for (key,row) in &state.staged_courts { if staged_claim(key).is_some() {
        count=count.checked_add(1).ok_or_else(||resource("scoped staged row count"))?;
        let n=borsh::object_length(&(key,row)).map_err(|_|resource("scoped staged row wire"))? as u64;
        q.exact_v1::<u8>(n.max(1024).checked_mul(3).ok_or_else(||resource("scoped staged row copy"))?).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<u8>(768).map_err(PalwStateV2Error::NativeResource)?;
    }}
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(count.saturating_mul(2)).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(count.saturating_mul(2)).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>(count.checked_mul(256).ok_or_else(||resource("scoped index framing"))?).map_err(PalwStateV2Error::NativeResource)?;
    Ok(q.bytes_v1())
}

fn staged_claim(key:&crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)->Option<Hash64>{
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as K;
    match key { K::CourtSharedIds(claim)|K::CourtMaterial(claim,_)=>Some(*claim),_=>None }
}
/// D binds the actual snapshot point; this stable index binds coordinates, owner
/// and data hashes. No point stamp churns an unrelated owner's material. Large immutable
/// bodies are Q references; only the small current index/status/draft uses M.
pub(super) fn attach(state:&PalwChainStateV2,_point:&PalwBlockContextV2,records:&mut BTreeMap<PalwClaimLocalWorkRefV1,PalwClaimLocalRecordV1>,bodies:&mut BTreeMap<Hash64,Vec<u8>>)->Result<(),PalwStateV2Error>{
    let coordinates=coordinates(state)?;
    for record in records.values_mut().filter(|r|!r.liability_closed){
        let PalwClaimLocalWorkRefV1::ChainClaim{claim}=record.work else{continue};
        let n=coordinates.iter().filter(|c|c.claim==claim).count()+state.staged_courts.keys().filter(|k|staged_claim(k)==Some(claim)).count();if n==0{continue}
        let capacity=74+coordinates.iter().filter(|c|c.claim==claim).map(|c|77+c.key.len()).sum::<usize>()+state.staged_courts.keys().filter(|k|staged_claim(k)==Some(claim)).count()*206;
        let mut index=Vec::with_capacity(capacity);
        borsh::BorshSerialize::serialize(&2u16,&mut index).expect("scoped index version");
        borsh::BorshSerialize::serialize(&claim,&mut index).expect("scoped owner");
        borsh::BorshSerialize::serialize(&(n as u64),&mut index).expect("scoped row count");
        for c in coordinates.iter().filter(|c|c.claim==claim){
            let id=palw_liability_material_id_v1(c.bytes);
            borsh::BorshSerialize::serialize(&(c.table,c.key,id,c.bytes.len() as u64),&mut index).expect("scoped row coordinate");
            record.material.insert(id);bodies.entry(id).or_insert_with(||c.bytes.to_vec());
        }
        for (key,row) in &state.staged_courts { if staged_claim(key)==Some(claim) {
            let key_bytes=borsh::to_vec(key).expect("scoped staged key");
            let bytes=borsh::to_vec(&(key,row)).expect("scoped staged material");
            let id=palw_liability_material_id_v1(&bytes);
            borsh::BorshSerialize::serialize(&(255u8,key_bytes.as_slice(),id,bytes.len() as u64),&mut index).expect("scoped staged coordinate");
            record.material.insert(id);bodies.entry(id).or_insert(bytes);
        }}
        let id=palw_liability_material_id_v1(&index);record.material.insert(id);bodies.insert(id,index);
    }Ok(())
}

/// Only the actual owning paths can annotate spans. Still check every delta's
/// kind and actual signed accepted object; annotations cannot hide claim,
/// selected-approval, EXEC or native credit events in a mixed block.
pub(super) fn noncredit_only(b:&TransitionBuilder<'_>,objects:&[PalwConsensusObjectV2],work:PalwBlockWorkV3<'_>,merged:&[PalwMergedWorkV1<'_>])->Result<bool,PalwStateV2Error>{
    if !matches!(work,PalwBlockWorkV3::None)||!merged.is_empty(){return Ok(false)}
    let network=b.extras.tir_public_network_domain_v1;
    for object in objects {
        let PalwConsensusObjectV2::CourtFamilyMaterialV1{control}=object else{return Ok(false)};
        let Some(network)=network else{return Ok(false)};
        let message=control.signing_message_v1(network).map_err(|_|bad("scoped signed material message"))?;
        if !b.claim_local_family_spans.iter().any(|s|s.event==FamilyEventV2::Delivery(message)){return Ok(false)}
    }
    if b.entries.is_empty(){return Ok(false)}
    for (ordinal,entry) in b.entries.iter().enumerate(){
        // Checkpoint's own generation and held-responsibility bookkeeping do
        // not constitute a new source event. All other writes need attribution.
        if matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,..} if *table==PALW_CLAIM_LOCAL_TABLE_V1){continue}
        let Some(span)=b.claim_local_family_spans.iter().find(|s|s.first<=ordinal&&ordinal<s.end)else{return Ok(false)};
        if let FamilyEventV2::ConsumedCashQueue(queue)=span.event {
            // A system-owned physical queue observation, NOT ownership of the
            // coalesced funds by any one computational claim.
            if span.end!=span.first+1||!matches!(entry,PalwDeltaEntryV2::Payout{key,old:Some(_),new:None} if *key==queue){return Err(bad("consumed cash queue span differs from actual payout deletion"))}
            continue;
        }
        if span.claim==ZERO_HASH64{return Err(bad("scoped family span has no owner"))}
        let allowed=match entry{
            PalwDeltaEntryV2::Bond{old:Some(_),new:Some(_),..}|PalwDeltaEntryV2::Exposure{..}|PalwDeltaEntryV2::Payout{..}|PalwDeltaEntryV2::BondBudgetRow{..}|PalwDeltaEntryV2::BondBudgetHeader{..}=>true,
            PalwDeltaEntryV2::KernelRouteRow{table,..}=>matches!(*table,140|169|192|197),
            PalwDeltaEntryV2::StagedCourtSeed{key,..}=>match key{
                crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtSharedIds(claim)|crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtMaterial(claim,_)=>*claim==span.claim,
                crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService=>matches!(span.event,FamilyEventV2::Delivery(_)),
                _=>false,
            },
            _=>false,
        };if !allowed{return Ok(false)}
    }Ok(true)
}

/// Extra normalized checkpoint storage for an owner's concrete row envelope.
/// `rows` includes mutable source rows; `key_bytes` is their SUM, not a nominal
/// key width. `mutable_bytes` counts the current source literal copies using M.
/// This does NOT assert a bound on the number of independently paid drafts.
/// The original/control admission owner must supply its actual finite envelope.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct ProjectionGeometryV2 { pub retained_bytes:u64, pub retained_heap:u64 }
pub(super) fn projection_geometry_v2(rows:u64,key_bytes:u64,mutable_rows:u64,mutable_bytes:u64,owners:u64)->Result<ProjectionGeometryV2,PalwStateV2Error>{
    let add=|a:u64,b:u64|a.checked_add(b).ok_or_else(||resource("scoped projection geometry sum"));
    let mul=|a:u64,b:u64|a.checked_mul(b).ok_or_else(||resource("scoped projection geometry product"));
    if mutable_rows>rows{return Err(bad("scoped mutable row envelope exceeds all rows"))}
    let indexes=add(add(mul(74,owners)?,mul(77,rows)?)?,key_bytes)?;
    // Q: v2,id,len,Some(table,key); using Some also bounds a mutable None.
    let refs=add(mul(80,rows)?,key_bytes)?;
    let physical_rows=add(add(rows,mutable_rows)?,mul(2,owners)?)?;
    let node=std::mem::size_of::<BTreeNodeEnvelopeV1<(u8,Vec<u8>),Vec<u8>>>() as u64;
    let keys=mul(65,physical_rows)?;
    let blobs=add(add(mutable_bytes,mul(6,mutable_rows)?)?,add(indexes,mul(6,owners)?)?)?;
    let index_refs=mul(75,owners)?;
    let retained_bytes=add(add(add(refs,index_refs)?,blobs)?,keys)?;
    let material_set=mul(add(rows,owners)?,std::mem::size_of::<BTreeNodeEnvelopeV1<Hash64,()>>() as u64)?;
    let retained_heap=add(add(retained_bytes,mul(physical_rows,node)?)?,material_set)?;
    Ok(ProjectionGeometryV2{retained_bytes,retained_heap})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc23s2_projection_geometry_checks_concrete_mutable_and_key_envelopes() {
        let one=projection_geometry_v2(1,65,0,0,1).unwrap();
        let mutable=projection_geometry_v2(1,65,1,512,1).unwrap();
        assert!(mutable.retained_bytes>one.retained_bytes+512);
        assert!(mutable.retained_heap>mutable.retained_bytes);
        assert!(projection_geometry_v2(1,65,2,512,1).is_err());
        assert!(projection_geometry_v2(u64::MAX,u64::MAX,0,0,1).is_err());
        assert!(projection_geometry_v2(1,u64::MAX,0,0,1).is_err());
    }
    #[test]
    fn rfc23s2_cash_drain_annotation_is_system_owned_and_rollback_exact() {
        let h=Hash64::from_u64_word;
        let params=PalwStateParamsV2::new(100,100,10,20,50,1000,h(1),4,1000,100,1000,0).unwrap().with_claim_local_settlement_from_daa(Some(0));
        let extras=PalwTransitionExtrasV1{claim_local_rule_context_v1:Some(([11;32],[12;32])),kernel_route:Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1{network_domain:h(10),ruleset_digest:h(11),chain_genesis:h(12),..Default::default()}),..Default::default()};
        let point=PalwBlockContextV2{block:h(5),daa_score:1,blue_score:1,subsidy:0};
        let (state,_)=apply_palw_transition_v2_with_extras(&PalwChainStateV2::genesis(),&params,&point,&[],None,false,false,false,false,&extras).unwrap();
        // Prepared cash is installed by the ordinary payout writer, without
        // inventing a computational source, selected approval, or credit claim.
        let mut prepare=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        prepare.add_kernel_payout(Hash64::from_bytes([7;64]),13).unwrap();
        let queued=prepare.state.clone();drop(prepare);
        let mut b=TransitionBuilder::new(&queued,&params,false,false,false,false,&extras);
        let save=b.checkpoint();
        super::super::block_preparation::queue_drain_workspace_v2::drain_consumed_queues_v2(&mut b,&point).unwrap();
        assert!(noncredit_only(&b,&[],PalwBlockWorkV3::None,&[]).unwrap());
        assert_eq!(b.claim_local_family_spans.len(),1);
        assert_eq!(b.claim_local_family_spans[0].claim,ZERO_HASH64,"a coalesced cash queue is not one computation's credit");
        // Extending the observed system span across another kind of effect is
        // refused, rather than lending cash authority to that extra effect.
        b.claim_local_family_spans[0].end+=1;
        b.entries.push(PalwDeltaEntryV2::Exposure{key:PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(90),0)),old:None,new:Some(1)});
        assert!(noncredit_only(&b,&[],PalwBlockWorkV3::None,&[]).is_err());
        b.restore(save);
        assert!(b.entries.is_empty());assert!(b.claim_local_family_spans.is_empty());
        assert_eq!(b.state,queued);
    }
}
