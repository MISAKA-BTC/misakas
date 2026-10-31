//! Native checkpoint vertices use actual retained admission/journal points.
//! Session birth never moves forward to accommodate later slice dependencies.
use super::*;
use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2 as RootPhase,PalwWorkPrefixStageV2 as Prefix,PalwWorkSliceStageV2 as SlicePhase};
use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E,NativeDemandLeaseV1};
#[derive(Clone,Copy)]
pub(super) struct Birth<'a>{pub point:PalwBlockContextV2,pub frame:&'a Vec<u8>,pub full_frame:bool}
pub(super) struct Context<'a>{
    pub births:BTreeMap<PalwClaimLocalWorkRefV1,Birth<'a>>,
    settlements:BTreeMap<PalwClaimLocalWorkRefV1,(PalwClaimLocalNativeEventV1,&'a Vec<u8>)>,
    _event_leases:Vec<Option<NativeDemandLeaseV1>>,
    latest:Option<Birth<'a>>,
    mining_bounds:BTreeMap<Hash64,u64>,
    mining_claims:BTreeMap<Hash64,(u64,usize,NativeRetainedV1<(misaka_palw_kernel::hash::Digest,crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>)>,

}
fn fault(e:E<std::io::Error>)->PalwStateV2Error{match e{E::Resource(r)=>PalwStateV2Error::NativeResource(r),E::Decode(e)=>bad(&e.to_string())}}
fn decode<T:borsh::BorshDeserialize+borsh::BorshSerialize+crate::palw_native_resource_v1::NativeWireScanV1>(bytes:&[u8])
    ->Result<NativeRetainedV1<T>,PalwStateV2Error>{crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(fault)}
pub(super) fn context<'a>(state:&'a PalwChainStateV2)->Result<Context<'a>,PalwStateV2Error>{
    let mut out=Context{births:BTreeMap::new(),settlements:BTreeMap::new(),_event_leases:Vec::new(),latest:None,mining_bounds:BTreeMap::new(),mining_claims:BTreeMap::new()};
    let anchors=crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?;
    for (_,bytes) in rewards::frames(state){
        let frame=decode::<PalwClaimLiabilityFrameV1>(bytes)?;
        frame.validate_context_v1().map_err(checkpoint_err)?;
        if out.latest.is_none_or(|p|(p.point.daa_score,p.point.blue_score,p.point.block)<(frame.point.daa_score,frame.point.blue_score,frame.point.block)){
            out.latest=Some(Birth{point:frame.point,frame:bytes,full_frame:true});
        }
        for (ordinal,c) in frame.native_changes.iter().enumerate(){
            use crate::palw_native_mining_v39::{MINING_TABLE_V39,MINING_CLAIM_KIND_V39,MINING_JOB_HOLD_KIND_V39,NativeMiningClaimRecordV39};
            if c.owner==PalwClaimLocalRowOwnerV1::KernelRoute&&c.table==MINING_TABLE_V39{
                if c.key.starts_with(&[39,MINING_JOB_HOLD_KIND_V39]){
                    if let Some(new)=&c.new{let (claim,until):(misaka_palw_kernel::hash::Digest,u64)=borsh::from_slice(new).map_err(|_|bad("native mining retained hold row"))?;
                        let claim=Hash64::from_bytes(claim);out.mining_bounds.entry(claim).and_modify(|old|*old=(*old).max(until)).or_insert(until);
                    }
                }
                if c.key.starts_with(&[39,MINING_CLAIM_KIND_V39]){
                    let Some(new)=&c.new else{continue};
                    let row=decode::<(misaka_palw_kernel::hash::Digest,NativeMiningClaimRecordV39)>(new)?;
                    if row.0!=row.1.root()||c.key.len()!=66||c.key[2..]!=row.1.id{return Err(bad("native mining claim key or root"))}
                    let id=Hash64::from_bytes(row.1.id);
                    let live=state.kernel_route.as_ref().is_some_and(|r|r.aux.contains_key(&(MINING_TABLE_V39,c.key.clone())));
                    if live||anchors.iter().any(|(_,a)|a.claim==id){
                        if c.old.is_none(){
                            if row.1.admitted_daa!=frame.point.daa_score{return Err(bad("native mining birth changes admitted clock"))}
                            if out.births.insert(PalwClaimLocalWorkRefV1::NativeMiningClaim39{claim:id},Birth{point:frame.point,frame:bytes,full_frame:true}).is_some(){return Err(bad("native mining birth duplicated"))}
                        }
                        // DAA clocks can be equal across several real transitions. Select
                        // the most recent source body by actual chain order, never its
                        // closed clock or the hash ordering of archived frames.
                        let newer=out.mining_claims.get(&id).is_none_or(|(blue,old_ordinal,_)|(*blue,*old_ordinal)<(frame.point.blue_score,ordinal));
                        if newer{out.mining_claims.insert(id,(frame.point.blue_score,ordinal,row));}
                    }
                    continue;
                }
            }
            if c.old.is_some(){continue}let Some(new)=&c.new else{continue};
            let candidate=match (c.owner,c.table){
                (PalwClaimLocalRowOwnerV1::KernelRoute,misaka_palw_kernel::rows::TABLE_CLAIMS_V1)=>{
                    let id:misaka_palw_kernel::hash::Digest=borsh::from_slice(&c.key).map_err(|_|bad("native kernel birth key"))?;
                    let id=Hash64::from_bytes(id);if rewards::kernel_claim_row(state,&id).is_none(){continue}
                    let row=decode::<misaka_palw_kernel::ledger::ClaimRowV1>(new)?;
                    rewards::kernel_claim_row(state,&id).filter(|raw|borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(raw).is_ok_and(|current|current.committed_daa==row.committed_daa))
                        .filter(|_|row.committed_daa==frame.point.daa_score).map(|_|PalwClaimLocalWorkRefV1::KernelClaim{claim:id})
                }
                (PalwClaimLocalRowOwnerV1::ExecV2,PALW_EXEC_V2_TABLE_ROOTS_V1)=>{
                    let id:Hash64=borsh::from_slice(&c.key).map_err(|_|bad("native session birth key"))?;
                    if !state.exec_v2.roots.contains_key(&id){continue}
                    let row=decode::<crate::palw_work_slice_v2::PalwWorkRootV2>(new)?;
                    state.exec_v2.roots.get(&id).filter(|r|r.opened_daa==row.opened_daa&&r.anchor==row.anchor)
                        .filter(|_|row.opened_daa==frame.point.daa_score).map(|_|PalwClaimLocalWorkRefV1::ExecSession{root:id})
                }
                (PalwClaimLocalRowOwnerV1::ExecV2,PALW_EXEC_V2_TABLE_SLICES_V1)=>{
                    let (id,index):(Hash64,u32)=borsh::from_slice(&c.key).map_err(|_|bad("native slice birth key"))?;
                    if !state.exec_v2.slices.contains_key(&(id,index)){continue}
                    let row=decode::<crate::palw_work_slice_v2::PalwWorkSliceRowV2>(new)?;
                    state.exec_v2.slices.get(&(id,index)).filter(|r|r.slice_id==row.slice_id&&r.accepted_daa==row.accepted_daa)
                        .filter(|_|row.accepted_daa==frame.point.daa_score).map(|_|PalwClaimLocalWorkRefV1::ExecSlice{root:id,index})
                }
                _=>None,
            };
            if let Some(work)=candidate{
                if out.births.insert(work,Birth{point:frame.point,frame:bytes,full_frame:true}).is_some(){return Err(bad("native admission birth duplicated in retained history"))}
            }
        }
        let events=frame.native_credit_events_v1().map_err(checkpoint_err)?;
        let (events,lease)=events.into_parts_v1();out._event_leases.push(lease);
        for event in events{
            let PalwClaimLocalWorkRefV1::ExecSettlement{root,..}=event.work else{continue};
            if state.exec_v2.roots.get(&root).is_none_or(|r|r.opened_daa>event.point.daa_score){continue}
            if out.settlements.insert(event.work,(event,bytes)).is_some(){return Err(bad("native settlement event duplicated"))}
        }
    }
    if let Some(route)=&state.kernel_route{
        use crate::palw_native_mining_v39::{MINING_TABLE_V39,MINING_CLAIM_KIND_V39,NativeMiningClaimRecordV39};
        for ((table,key),raw) in &route.aux{if *table!=MINING_TABLE_V39||!key.starts_with(&[39,MINING_CLAIM_KIND_V39]){continue}
            let current=decode::<(misaka_palw_kernel::hash::Digest,NativeMiningClaimRecordV39)>(raw)?;
            let id=Hash64::from_bytes(current.1.id);
            if current.0!=current.1.root()||key.len()!=66||key[2..]!=current.1.id{return Err(bad("current native mining claim key or root"))}
            if out.mining_claims.get(&id).is_none_or(|(_,_,saved)|saved.0!=current.0)||!out.births.contains_key(&PalwClaimLocalWorkRefV1::NativeMiningClaim39{claim:id}){
                return Err(bad("native mining liability lacks its retained authenticated admission frame"))
            }
        }
    }
    if let Some(route)=&state.kernel_route{for ((table,key),raw)in &route.aux{
        if *table!=PALW_CLAIM_LOCAL_TABLE_V1||key.len()!=65||key[0]!=super::super::PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1{continue}
        let id=Hash64::from_bytes(key[1..].try_into().expect("compact kernel key"));
        let origin=super::super::palw_claim_local_kernel_origins_v1::read(state,id)?.expect("existing compact origin");
        let work=PalwClaimLocalWorkRefV1::KernelClaim{claim:id};
        if let Some(birth)=out.births.get(&work){if birth.point!=origin.birth{return Err(bad("compact kernel origin differs from retained actual birth"))}}
        else{out.births.insert(work,Birth{point:origin.birth,frame:raw,full_frame:false});}
    }}
    Ok(out)
}
fn retain_material(record:&mut PalwClaimLocalRecordV1,bytes:Vec<u8>,material:&mut BTreeMap<Hash64,Vec<u8>>){
    let id=palw_liability_material_id_v1(&bytes);record.material.insert(id);material.entry(id).or_insert(bytes);
}
fn with_birth(work:PalwClaimLocalWorkRefV1,birth:Birth<'_>,execution_root:Hash64)->PalwClaimLocalRecordV1{
    PalwClaimLocalRecordV1{work,accepted_daa:birth.point.daa_score,accepted_blue_score:birth.point.blue_score,accepting_block:birth.point.block,
        anchors:false,inputs:BTreeSet::new(),execution_root,reservations:BTreeSet::new(),final_claim:false,certified:false,voided:false,liability_closed:false,material:BTreeSet::new()}
}
fn born<'a>(c:&Context<'a>,work:PalwClaimLocalWorkRefV1)->Result<Birth<'a>,PalwStateV2Error>{c.births.get(&work).copied().ok_or_else(||bad("native liability lacks its retained authenticated admission frame"))}
pub(super) fn attach(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,now:u64,context:&Context<'_>,
    records:&mut BTreeMap<PalwClaimLocalWorkRefV1,PalwClaimLocalRecordV1>,material:&mut BTreeMap<Hash64,Vec<u8>>,
)->Result<(),PalwStateV2Error>{
    use PalwClaimLocalWorkRefV1 as W;
    // Kernel birth blue score and accepting block come from the executed journal,
    // never a default zero or the EXEC carrier's unrelated position.
    for (work,record) in records.iter_mut().filter(|(w,_)|matches!(w,W::KernelClaim{..})){
        let birth=born(context,*work)?;record.accepted_daa=birth.point.daa_score;record.accepted_blue_score=birth.point.blue_score;record.accepting_block=birth.point.block;
        if !record.liability_closed{if !birth.full_frame{return Err(bad("open kernel liability lacks its full admission material"))}retain_material(record,birth.frame.clone(),material);}
    }
    // A chain original does not causally consume evidence first accepted by a
    // later slice. The terminal settlement vertex below consumes those rows.
    for record in records.values_mut().filter(|r|matches!(r.work,W::ChainClaim{..})){
        record.inputs.retain(|input|match input{
            W::KernelClaim{..}=>context.births.get(input).is_some_and(|b|b.point.daa_score<=record.accepted_daa&&b.point.blue_score<=record.accepted_blue_score),
            _=>true,
        });
    }
    for (root_id,root) in &state.exec_v2.roots{
        let chain=records.get(&W::ChainClaim{claim:*root_id}).ok_or_else(||bad("native session lost chain original"))?;
        let execution_root=chain.execution_root;let chain_final=chain.final_claim;let chain_certified=chain.certified;let chain_closed=chain.liability_closed;
        let work=W::ExecSession{root:*root_id};let birth=born(context,work)?;let mut record=with_birth(work,birth,execution_root);
        record.inputs.insert(W::ChainClaim{claim:*root_id});
        if root.prefix!=Prefix::Unbound{record.inputs.insert(W::KernelClaim{claim:root.prefix_claim});}
        record.voided=matches!(root.phase,RootPhase::Voided{..});
        record.final_claim=!record.voided&&chain_final&&matches!(root.phase,RootPhase::Settled{..});
        record.certified=record.final_claim&&chain_certified&&root.prefix!=Prefix::Unbound;
        record.liability_closed=(record.voided&&now>root.expiry_daa)||(record.certified&&chain_closed&&now>root.expiry_daa);
        if !record.liability_closed{retain_material(&mut record,birth.frame.clone(),material);retain_material(&mut record,borsh::to_vec(&(*root_id,root)).expect("native session row"),material);}
        records.insert(work,record);
        for ((_,index),slice) in state.exec_v2.slices.range((*root_id,0)..=(*root_id,u32::MAX)){
            let work=W::ExecSlice{root:*root_id,index:*index};let birth=born(context,work)?;let mut record=with_birth(work,birth,execution_root);
            record.inputs.insert(W::ExecSession{root:*root_id});
            if *index>0{record.inputs.insert(W::ExecSlice{root:*root_id,index:index-1});}
            let kernel=records.get(&W::KernelClaim{claim:slice.evidence_root});
            if root.prefix!=Prefix::Unbound{record.inputs.insert(W::KernelClaim{claim:slice.evidence_root});}
            record.voided=matches!(slice.stage,SlicePhase::Voided{..}|SlicePhase::ProvenFalse{..}|SlicePhase::Defaulted{..});
            record.final_claim=matches!(slice.stage,SlicePhase::Verified{..});
            record.certified=record.final_claim&&kernel.is_some_and(|r|r.certified);
            record.liability_closed=if record.voided{now>root.expiry_daa}else{record.certified&&kernel.is_some_and(|r|r.liability_closed)&&now>root.expiry_daa};
            if !record.liability_closed{retain_material(&mut record,birth.frame.clone(),material);retain_material(&mut record,borsh::to_vec(&(*root_id,*index,slice)).expect("native slice row"),material);}
            records.insert(work,record);
        }
    }
    for (work,(event,frame)) in &context.settlements{
        let W::ExecSettlement{root,..}=work else{continue};let Some(session)=records.get(&W::ExecSession{root:*root})else{continue};
        let mut record=with_birth(*work,Birth{point:event.point,frame,full_frame:true},session.execution_root);
        record.inputs=event.inputs.clone();record.final_claim=session.final_claim;record.certified=session.certified;record.voided=session.voided;record.liability_closed=session.liability_closed;
        if !record.liability_closed{retain_material(&mut record,(*frame).clone(),material);}
        records.insert(*work,record);
    }
    for (id,(_,_,row)) in &context.mining_claims{
        let work=W::NativeMiningClaim39{claim:*id};let birth=born(context,work)?;let mut record=with_birth(work,birth,kaspa_hashes::ZERO_HASH64);
        let claim=&row.1;
        record.voided=crate::palw_native_receipt_v39::native_mining_convicted_v39(state,id)||claim.closed.is_some_and(|(_,issued)|!issued);
        record.final_claim=!record.voided&&claim.closed.is_some_and(|(_,issued)|issued);
        record.certified=record.final_claim&&(crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(params,state,id,claim.admitted_daa)
            ||crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?.iter().any(|(_,a)|a.claim==*id&&a.claim_root.as_bytes()==row.0
                &&crate::palw_native39_frontier_v1::native39_anchor_collected_v1(state,a)&&crate::palw_native39_frontier_v1::native39_frontier_standing_v1(state,params,a)));
        // The actual source job holds were created in this claim's admission
        // journal. A current policy guess cannot shorten that frozen liability.
        let until=context.mining_bounds.get(id).copied().ok_or_else(||bad("native mining admission lost its exact source liability hold"))?;
        record.liability_closed=(record.voided||record.certified)&&now>until&&!crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(state,id);
        if !record.liability_closed{retain_material(&mut record,birth.frame.clone(),material);retain_material(&mut record,borsh::to_vec(&**row).expect("native mining source row"),material);}
        records.insert(work,record);
    }
    for (_,anchor) in crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?.iter(){
        let work=W::NativeMining39{claim:anchor.claim,work:anchor.work,quantum:anchor.quantum};
        let point=PalwBlockContextV2{block:anchor.anchor,daa_score:anchor.anchor_daa,blue_score:anchor.anchor_blue,subsidy:0};
        let voided=crate::palw_native_receipt_v39::native_mining_convicted_v39(state,&anchor.claim);
        let certified=!voided&&crate::palw_native39_frontier_v1::native39_frontier_standing_v1(state,params,anchor);
        let closed=if voided{now>=anchor.matured_daa}else{
            crate::palw_native39_frontier_v1::native39_frontier_anchor_ready_v1(state,params,anchor,&PalwBlockContextV2{daa_score:now,..point})?};
        let mut record=PalwClaimLocalRecordV1{work,accepted_daa:point.daa_score,accepted_blue_score:point.blue_score,accepting_block:point.block,
            anchors:true,inputs:BTreeSet::from([W::NativeMiningClaim39{claim:anchor.claim}]),execution_root:kaspa_hashes::ZERO_HASH64,reservations:BTreeSet::new(),final_claim:!voided,certified,voided,liability_closed:closed,material:BTreeSet::new()};
        if !closed{
            retain_material(&mut record,borsh::to_vec(anchor).expect("native39 anchor"),material);
            if let Some(route)=&state.kernel_route{
                use crate::palw_native_mining_v39::{MINING_TABLE_V39,MINING_CLAIM_KIND_V39,MINING_RECEIPT_KIND_V39};
                for (kind,id) in [(MINING_CLAIM_KIND_V39,anchor.claim),(MINING_RECEIPT_KIND_V39,anchor.receipt)]{
                    let key=crate::palw_native_receipt_v39::native_mining_row_key_v39(kind,&id.as_bytes());
                    if let Some(bytes)=route.aux.get(&(MINING_TABLE_V39,key)){retain_material(&mut record,bytes.clone(),material);}
                }
            }
        }
        records.insert(work,record);
    }
    // After finite receipt/source collection, canonical195 retains the owning
    // fold's irreversible decision. It is authenticated as ordinary pruned
    // consensus state under its bound network/approval assumptions, not as a
    // cryptographic proof of model computation. A live193 vertex keeps its
    // original edges; only an already-closed collected lot uses this handoff.
    for (_,sealed) in crate::palw_native_closed_facts_v1::read_native_closed_facts_v1(state)?.iter(){
        if sealed.origin.family!=crate::palw_native_closed_facts_v1::ClosedFactFamilyV1::NativeMining39{continue}
        let quantum=sealed.origin.quantum.ok_or_else(||bad("closed native lot quantum absent"))?;
        let work=W::NativeMining39{claim:sealed.origin.subject,work:sealed.origin.work,quantum};
        let fact=&sealed.fact;
        if let Some(live)=records.get(&work){
            if live.accepting_block!=fact.anchor||live.accepted_blue_score!=fact.accepted_blue||live.accepted_daa!=fact.accepted_daa{return Err(bad("sealed native lot changes its actual acceptance point"))}
            continue
        }
        records.insert(work,PalwClaimLocalRecordV1{work,accepted_daa:fact.accepted_daa,accepted_blue_score:fact.accepted_blue,
            accepting_block:fact.anchor,anchors:true,inputs:BTreeSet::new(),execution_root:kaspa_hashes::ZERO_HASH64,
            reservations:BTreeSet::new(),final_claim:true,certified:true,voided:false,liability_closed:true,material:BTreeSet::new()});
    }
    // The latest retained executed frame contains the actual current native
    // carriage (excluding this namespace), including later material deliveries,
    // registry/draw context and active court state. A digest alone is insufficient.
    if let Some(latest)=context.latest{
        for record in records.values_mut().filter(|r|!r.liability_closed){retain_material(record,latest.frame.clone(),material);}
    }
    // Every edge must refer to material that was already admitted. Missing rows
    // and out-of-order native sources cannot turn into optimistic closed credit.
    for record in records.values(){for input in &record.inputs{
        let source=records.get(input).ok_or_else(||bad("native liability checkpoint lost consumed work"))?;
        if source.accepted_daa>record.accepted_daa||source.accepted_blue_score>record.accepted_blue_score{return Err(bad("native checkpoint consumed future work"))}
    }}
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rfc23s2_native_settlement_and_mining_quantum_wire_namespaces_roundtrip(){
        use crate::palw_native_resource_v1::preflight_native_value_v1;
        use PalwClaimLocalWorkRefV1 as W;
        let id=Hash64::from_u64_word(13);let block=Hash64::from_u64_word(14);
        let sources=[W::ChainClaim{claim:id},W::KernelClaim{claim:id},W::ExecSession{root:id},W::ExecSlice{root:id,index:0},
            W::ExecSettlement{root:id,block},W::NativeMining39{claim:id,work:block,quantum:0},W::NativeMiningClaim39{claim:id}];
        assert_eq!(sources.iter().copied().collect::<BTreeSet<_>>().len(),sources.len());
        for (tag,source) in sources.iter().enumerate(){
            let bytes=borsh::to_vec(source).unwrap();assert_eq!(bytes[0],tag as u8);
            preflight_native_value_v1::<W>(bytes.as_slice(),bytes.len() as u64).unwrap();
            assert_eq!(*decode::<W>(&bytes).unwrap(),*source);
            assert!(preflight_native_value_v1::<W>(&bytes[..bytes.len()-1],bytes.len() as u64-1).is_err());
        }
        assert_ne!(sources[5],W::NativeMining39{claim:id,work:block,quantum:1});
    }
}
