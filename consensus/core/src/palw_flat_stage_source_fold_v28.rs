//! Independently paid original source/trace pursuit28. Missing external R is
//! never a producer default; only its exact original signed bound PC/trace unit
//! can create an answer obligation. Expensive failed attempts remain spent.
use super::*;
use misaka_palw_kernel::{flat_quota11::{source16::*,stage14::source26::*},merkle::{AXIS_ROW,AXIS_COL},merkle3::LayoutV3};
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct Pursuit {version:u16,job:Digest,original:Digest,requests:u32,ordinary:u32,pending:u32,served:u32,required_until:u64,open:[Digest;11]}
fn pending_key(id:Digest)->Vec<u8>{bytes(&(28u8,id))}
fn served_key(unit:Digest)->Vec<u8>{bytes(&(28u8,unit))}
fn slot_key(signer:PalwBondKeyV2,slot:u8)->Vec<u8>{bytes(&(28u8,signer,slot))}
fn pending(r:&PalwKernelRouteStateV1,id:Digest)->Result<NativeStagePendingV28,PalwStateV2Error>{let raw=r.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(id))).ok_or_else(||refused("stage28 original pending unit absent"))?;if raw.len()>2048{return Err(refused("stage28 original pending fixed cap"));}let p:NativeStagePendingV28=borsh::from_slice(raw).map_err(|_|refused("stage28 pending fixed grammar"))?;if p.version!=28||p.id!=id||p.original_id()!=id{return Err(refused("stage28 pending original immutable demand differs"));}Ok(p)}
fn pursuit(builder:&TransitionBuilder<'_>,h:&JobHeader)->Result<Pursuit,PalwStateV2Error>{let row:Option<Pursuit>=read(builder.state.kernel_route.as_ref().unwrap(),16,&h.id,2048);let row=row.unwrap_or(Pursuit{version:28,job:h.id,original:h.original,requests:0,ordinary:0,pending:0,served:0,required_until:0,open:[[0;64];11]});if row.version!=28||row.job!=h.id||row.original!=h.original||row.pending>11||row.open.iter().filter(|n|**n!=[0;64]).count()!=row.pending as usize||row.ordinary>32{return Err(refused("stage28 original public pursuit counters differ"));}Ok(row)}
fn unit(h:&JobHeader,claim:Digest,q:&NativeStageRequestV28)->Digest{object_id(b"misaka-palw/native/stage-original-source-trace-unit/v28",&(28u16,h.id,h.original,claim,h.contract.root(),q.param,q.layer_code,q.kind,q.axis,q.level,q.index))}
pub(super) fn source_quote(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,h:&JobHeader)->Result<SourcePlanV16,PalwStateV2Error>{let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(16u8,h.id,1u8)))).ok_or_else(||refused("stage28 original producer source quote absent"))?;let record=SourcePlanRecordV16::from_rooted_bytes(raw,&h.contract.source_plan).map_err(refused)?;SourcePlanV16::restore(&record,&h.contract.source_plan,&p.execution.quota,&h.quota_record.root()).map_err(refused)}
pub(super) fn caller_quote(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,h:&JobHeader)->Result<CheckedStageSourcePlanV26,PalwStateV2Error>{let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(26u8,h.id,0u8)))).ok_or_else(||refused("stage28 original caller source quote absent"))?;CheckedStageSourcePlanV26::restore_native_record(raw,h.contract.caller_source,&p.execution.quota).map_err(refused)}
pub(super) fn producer_shard(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,epoch:&TrustedEpochV11,h:&JobHeader,quote:&SourcePlanV16)->Result<SourceShardV16,PalwStateV2Error>{let shard=super::super::super::palw_flat_execution_fold_v11::quota_shard(builder,&p.execution,epoch,palw_kernel_bond_id_v1(&h.producer),h.id)?;let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(16u8,h.id)))).ok_or_else(||refused("stage28 original producer source reserve absent"))?;let(root,row):(Digest,SourceObligationV16)=borsh::from_slice(raw).map_err(|_|refused("stage28 original producer source reserve grammar"))?;if row.original_consent_root!=h.consent||row.plan_root!=h.contract.source_plan{return Err(refused("stage28 producer source original28 consent differs"));}super::super::super::palw_flat_execution_fold_v11::source_shard_from_rows(&p.execution,quote,&shard.global_row(),&shard.actor_row(),shard.subject_row().ok_or_else(||refused("stage28 original source subject absent"))?,Some((&row,&root)))}
pub(super) fn caller_shard(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,epoch:&TrustedEpochV11,h:&JobHeader,actor:Digest,owner:Digest,quote:&CheckedStageSourcePlanV26)->Result<StageSourceCallerShardV26,PalwStateV2Error>{let base=super::super::super::palw_flat_execution_fold_v11::quota_shard(builder,&p.execution,epoch,actor,h.id)?;let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(26u8,h.id,owner)))).ok_or_else(||refused("stage28 original paid caller reserve absent"))?;let(root,row):(Digest,StageSourceCallerRowV26)=borsh::from_slice(raw).map_err(|_|refused("stage28 original paid caller grammar"))?;let subject=base.subject_row().unwrap();StageSourceCallerShardV26::from_native_rows(&p.execution.quota,quote,&base.global_row(),&base.global_row().root(),actor,Some((&base.actor_row(),&base.actor_row().root())),(subject,&subject.row_root()),Some((&row,&root))).map_err(refused)}
pub(super) fn slots(builder:&TransitionBuilder<'_>,h:&JobHeader)->Result<crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22,PalwStateV2Error>{let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(22u8,h.id,2u8)))).ok_or_else(||refused("stage28 original physical streams absent"))?;let(root,s):(Digest,crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22)=borsh::from_slice(raw).map_err(|_|refused("stage28 original streams grammar"))?;if s.root()!=root||s.job!=h.id||s.original_consent23!=h.consent{return Err(refused("stage28 original physical stream roots differ"));}Ok(s)}
fn served(r:&PalwKernelRouteStateV1,unit:Digest)->Option<Vec<u8>>{let raw=r.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_SERVED_TABLE_V11,served_key(unit)))?;(raw.len()<=128<<10).then(||raw.clone())}
fn node_request(q:&NativeStageRequestV28)->NativeStageRequestV28{let mut n=q.clone();n.kind=3;n.axis=0;n.level=0;n.index&=65535;n}
fn node_from_cache(builder:&TransitionBuilder<'_>,h:&JobHeader,claim:Digest,q:&NativeStageRequestV28)->Result<misaka_palw_kernel::seg::NodeOpeningV1,PalwStateV2Error>{let n=node_request(q);let raw=served(builder.state.kernel_route.as_ref().unwrap(),unit(h,claim,&n)).ok_or_else(||refused("stage28 original trace node header not publicly acquired"))?;if raw.first()!=Some(&3){return Err(refused("stage28 trace cache namespace differs"));}decode_stage_node_opening_v28(&raw[1..]).map_err(refused)}
fn check_coordinate(builder:&TransitionBuilder<'_>,h:&JobHeader,class:&CheckedStageClassV28,actual:&CheckedStageJobV28,book:&StageJobBookV28,q:&NativeStageRequestV28)->Result<Digest,PalwStateV2Error>{
    let actual32=master32::actual_job_fixed(builder,h,class)?;
    let actual=actual32.as_ref().unwrap_or(actual);
    if q.kind<=2 {
        if q.layer_code>65536||q.kind==0&&(q.axis!=0||q.level!=0||q.index!=0)||q.kind>0&&q.axis!=AXIS_ROW&&q.axis!=AXIS_COL||q.kind==2&&q.level!=0{return Err(refused("stage28 original real-PC coordinate grammar differs"));}
        let layer=if q.layer_code==0{None}else{Some((q.layer_code-1) as u16)};let pc=*class.source().parameters().by_instance.get(&(q.param,layer)).ok_or_else(||refused("stage28 cannot request actual external inputs as original source weights"))?;
        let decl=class.source().program().params.get(q.param as usize).ok_or_else(||refused("stage28 original real parameter absent"))?;
        let shape=decl.shape.iter().map(|n|*n as usize).collect::<Vec<_>>();let layout=LayoutV3::try_of(&shape).ok_or_else(||refused("stage28 original source tree geometry"))?;
        if q.kind>0 {let mut count=layout.leaves(q.axis);for _ in 0..q.level{if count<=1{return Err(refused("stage28 original source subtree level exceeded"));}count=count.div_ceil(2);}if q.index>=count{return Err(refused("stage28 original source subtree/leaf index exceeded"));}}
        Ok(pc)
    }else{
        let node=(q.index&65535) as u16;let leaf=q.index>>16;let geometry=actual.trace_geometry(q.layer_code,q.param,node).map_err(refused)?;
        if q.kind==3&&(leaf!=0||q.axis!=0||q.level!=0)||q.kind>3&&q.level!=0{return Err(refused("stage28 original trace coordinate grammar differs"));}
        if q.kind>=4 {
            let values=geometry.shape.iter().try_fold(1u64,|n,d|n.checked_mul(*d as u64)).ok_or_else(||refused("stage28 trace value extent overflow"))?;
            if q.kind==6 {if values>=64||q.axis!=AXIS_ROW&&q.axis!=AXIS_COL{return Err(refused("stage28 legacy trace leaf grammar differs"));}let l=LayoutV3::try_of(&geometry.shape).ok_or_else(||refused("stage28 trace legacy geometry"))?;if leaf>=l.leaves(q.axis){return Err(refused("stage28 trace legacy leaf coordinate exceeded"));}}
            else {if values<64||q.axis!=0{return Err(refused("stage28 flat trace value grammar differs"));}let shape=geometry.shape.iter().map(|n|*n as u64).collect::<Vec<_>>();let l=misaka_palw_kernel::merkle5::FlatLayoutV5::of(&shape).ok_or_else(||refused("stage28 original trace flat layout"))?;if leaf>=l.leaves(){return Err(refused("stage28 trace flat leaf coordinate exceeded"));}}
            Ok(node_from_cache(builder,h,book.claim().unwrap(),q)?.commitment)
        }else{
            let data:JobData=read(builder.state.kernel_route.as_ref().unwrap(),6,&h.id,(1<<20)+(2<<20)+16384).ok_or_else(||refused("stage28 original trace body absent"))?;let trace=StageTraceV14::from_bytes(&data.trace,master32::actual_positions(builder.state.kernel_route.as_ref().unwrap(),h.id,h.positions)?).map_err(refused)?;let index=q.layer_code/misaka_palw_kernel::seg::SEG_LEN_V4;trace.segment_roots.get(index as usize).copied().ok_or_else(||refused("stage28 original trace segment absent"))
        }
    }
}
fn authenticate_answer(class:&CheckedStageClassV28,actual:&CheckedStageJobV28,data:&JobData,d:&NativeStagePendingV28,raw:&[u8])->Result<(),PalwStateV2Error>{
    let q=&d.request;
    if q.kind<=2 {
        let a=opening::decode_answer_v16(raw).map_err(refused)?;
        let okay=match &a {
            opening::AnswerV16::Header(h)=>q.kind==0&&h.authenticates(&d.pc),
            opening::AnswerV16::Subtree(s)=>q.kind==1&&s.axis==q.axis&&s.level==q.level&&s.index==q.index&&s.authenticates(&d.pc),
            opening::AnswerV16::Leaf(l)=>{let shape=l.shape.iter().map(|n|usize::try_from(*n)).collect::<Result<Vec<_>,_>>().map_err(|_|refused("stage28 original PC leaf shape overflow"))?;let layout=LayoutV3::try_of(&shape).ok_or_else(||refused("stage28 original PC leaf geometry"))?;let index=if l.axis==AXIS_ROW{l.line.checked_mul(layout.row_tiles).and_then(|n|n.checked_add(l.tile))}else{l.line.checked_mul(layout.col_tiles).and_then(|n|n.checked_add(l.tile))};q.kind==2&&l.axis==q.axis&&index==Some(q.index)&&l.authenticates(&d.pc)},
        };
        if !okay{return Err(refused("stage28 answer does not authenticate original real bound PC unit"));}
        let layer=if q.layer_code==0{None}else{Some((q.layer_code-1) as u16)};if class.source().parameters().by_instance.get(&(q.param,layer))!=Some(&d.pc){return Err(refused("stage28 original source answer uses wrong namespace/root"));}
    }else{
        if raw.first()!=Some(&q.kind){return Err(refused("stage28 original trace answer kind differs"));}let node=(q.index&65535) as u16;let leaf=q.index>>16;let g=actual.trace_geometry(q.layer_code,q.param,node).map_err(refused)?;
        match q.kind {
            3=>{let o=decode_stage_node_opening_v28(&raw[1..]).map_err(refused)?;let trace=StageTraceV14::from_bytes(&data.trace,g.positions).map_err(refused)?;if o.position!=q.layer_code||o.occurrence!=q.param||o.node!=node||!o.authenticates(&trace.segment_roots,g.positions,g.node_index,g.node_count){return Err(refused("stage28 original trace node not authenticated"));}},
            4=>{let o=misaka_palw_kernel::merkle5::decode_flat_hash_v5(&raw[1..]).map_err(|e|refused(e.to_string()))?;if o.index!=leaf||!o.authenticates(&d.pc){return Err(refused("stage28 original trace hash leaf not authenticated"));}},
            5=>{let o=misaka_palw_kernel::merkle5::decode_flat_value_v5(&raw[1..]).map_err(|e|refused(e.to_string()))?;if o.opening.index!=leaf||!o.authenticates(&d.pc){return Err(refused("stage28 original trace value leaf not authenticated"));}},
            6=>{let a=opening::decode_answer_v16(&raw[1..]).map_err(refused)?;let opening::AnswerV16::Leaf(o)=a else{return Err(refused("stage28 legacy trace exact leaf grammar"));};let dims=o.shape.iter().map(|n|*n as usize).collect::<Vec<_>>();let l=LayoutV3::try_of(&dims).ok_or_else(||refused("stage28 legacy trace leaf geometry"))?;let index=if o.axis==AXIS_ROW{o.line.checked_mul(l.row_tiles).and_then(|n|n.checked_add(o.tile))}else{o.line.checked_mul(l.col_tiles).and_then(|n|n.checked_add(o.tile))};if index!=Some(leaf)||o.axis!=q.axis||!o.authenticates(&d.pc){return Err(refused("stage28 original legacy trace leaf not authenticated"));}},
            _=>return Err(refused("stage28 unknown original trace answer")),
        }
    }
    Ok(())
}
pub(super) fn request(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    if body.len()!=147{return Err(refused("stage28 source/trace request exact fixed147-byte grammar"));}let q:NativeStageRequestV28=borsh::from_slice(body).map_err(|_|refused("stage28 fixed request grammar"))?;let h=original_header(builder,q.job)?;
    let actor=palw_kernel_bond_id_v1(signer);let operator=builder.state.bonds.get(signer).ok_or_else(||refused("stage28 original request actor absent"))?.operator_id.as_bytes();let epoch=TrustedEpochV11::from_chain(&p.execution.quota,ctx.block.as_bytes(),ctx.daa_score).map_err(refused)?;
    if h.terminal!=StageTerminalV28::Challengeable||q.lane>2||q.slot>=2||q.kind>6||q.lane==0&&q.entitlement!=[0;64]||q.lane!=0&&q.entitlement==[0;64]{return Err(refused("stage28 original public/paid request lane differs"));}
    if q.lane==0{let(s,c)=original_extents(builder,h.class)?;if !pay_restore(builder,ctx,signer,p,s,c,h.input_bytes,body.len() as u32)?{return Ok(());}}else{
        let own=builder.state.kernel_route.as_ref().unwrap().native_paged_seal_v29(h.id,q.entitlement).ok_or_else(||refused("stage28 original paid owner absent"))?;
        if own.namespace!=28||own.actor!=actor||own.operator!=operator||own.retired||own.spent||ctx.daa_score<own.matures||q.lane==1&&own.discovery||q.lane==2&&!own.discovery{return Err(refused("stage28 own proof/discovery lane/operator/maturity differs"));}
        let quote=caller_quote(builder,p,&h)?;let mut shard=caller_shard(builder,p,&epoch,&h,actor,q.entitlement,&quote)?;shard.debit_request(&p.execution.quota,&epoch,&quote,q.entitlement,operator).map_err(refused)?;write_caller(builder,&shard);
    }
    let(class,actual,plan,book,_data)=restore_job(builder,p,&h)?;let maximum=contract(builder,p,&class,&actual,&plan)?;
    if maximum.record()!=&h.contract||operator==book.producer_operator(){return Err(refused("stage28 independently restored source/caller is not original or independent"));}
    let mut ps=pursuit(builder,&h)?;let claim=book.claim().unwrap();let target=unit(&h,claim,&q);if served(builder.state.kernel_route.as_ref().unwrap(),target).is_some(){return Err(refused("stage28 canonical original unit already served publicly"));}
    let deadline=ctx.daa_score.checked_add(h.contract.temporal.answer_window).ok_or_else(||refused("stage28 answer deadline overflow"))?;let proof_until=deadline.checked_add(p.execution.execution.horizons.proof_grace).ok_or_else(||refused("stage28 proof grace overflow"))?;
    if q.lane==0&&(ps.ordinary>=32||ctx.daa_score>book.challenge_end().unwrap())||proof_until>=book.deadline()||ps.pending>=11{return Err(refused("stage28 original ordinary lifetime/public stream/horizon exhausted"));}
    if q.lane!=0 {let own=builder.state.kernel_route.as_ref().unwrap().native_paged_seal_v29(h.id,q.entitlement).unwrap();if proof_until>own.horizon{return Err(refused("stage28 paid request outlives original owner's signed horizon"));}}
    let r=builder.state.kernel_route.as_ref().unwrap();if let Some(id)=r.aux_row::<Digest>(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,&slot_key(*signer,q.slot)){if pending(r,id).is_ok_and(|d|!d.settled){return Err(refused("stage28 original requester source/trace slot occupied"));}}
    if super::super::super::palw_flat_execution_fold_v11::source::slot_occupied(r,signer,q.slot)?||r.aux_row::<crate::Hash64>(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,&bytes(&(*signer,q.slot))).and_then(|id|r.execution_demand_v11(&id)).is_some_and(|d|!d.body.settled)||r.flat_vector_slot_v9(signer,q.slot).and_then(|id|r.flat_vector_demand_v9(&id)).is_some_and(|d|!d.settled)||r.flat_vector_slot_v7(signer,q.slot).and_then(|id|r.flat_vector_demand_v7(&id)).is_some_and(|d|!d.settled)||r.flat_vector_slot_v3(signer,q.slot).and_then(|id|r.flat_vector_demand_v3(&id)).is_some_and(|d|!d.settled)||r.model_vector_demand_slot_v2(signer,q.slot).and_then(|id|r.model_vector_demand_v2(&id)).is_some_and(|d|!d.settled){return Err(refused("stage28 shared original requester slot occupied"));}
    let pc=check_coordinate(builder,&h,&class,&actual,&book,&q)?;
    let ledger=super::super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;let bond=ledger.demand_bond.max(1);let payer=builder.state.bonds.get(signer).unwrap();if (payer.collateral as u128).saturating_sub(builder.committed_at(signer,ctx.daa_score))<bond as u128{return Err(refused("stage28 actual original demand bond unavailable"));}
    let mut d=NativeStagePendingV28{version:28,id:[0;64],unit:target,request:q.clone(),original:h.original,consent:h.consent,contract:h.contract.root(),source_plan:h.contract.source_plan,caller_plan:h.contract.caller_source,requester:*signer,requester_operator:operator,requester_payout:payer.payout_payload,producer:h.producer,producer_operator:book.producer_operator(),claim,pc,bond,filed:ctx.daa_score,deadline,proof_until,attempts:0,settled:false};d.id=d.original_id();
    let mut streams=slots(builder,&h)?;streams.reserve_request(q.lane,q.entitlement,q.slot,if q.kind<=2{0}else{1},d.id,ctx.daa_score,deadline).map_err(refused)?;
    let quote=source_quote(builder,p,&h)?;let mut producer=producer_shard(builder,p,&epoch,&h,&quote)?;producer.admit_valid_request(&p.execution.quota,&epoch,&quote,&h.id).map_err(refused)?;
    if q.lane!=0{let quote=caller_quote(builder,p,&h)?;let mut caller=caller_shard(builder,p,&epoch,&h,actor,q.entitlement,&quote)?;caller.admit_request(&quote).map_err(refused)?;write_caller(builder,&caller);}
    super::super::super::palw_flat_execution_fold_v11::write_source_shard(builder,&producer,h.id);
    builder.write_kernel_row(STAGE_TABLE_V14,key(17,&(h.id,ps.requests)),Some(bytes(&(d.id,d.unit,d.requester,d.request.slot))));
    ps.requests=ps.requests.checked_add(1).ok_or_else(||refused("stage28 lifetime request overflow"))?;if q.lane==0{ps.ordinary+=1;}ps.pending+=1;ps.open.iter_mut().find(|id|**id==[0;64]).ok_or_else(||refused("stage28 original open streams exhausted"))?.clone_from(&d.id);ps.required_until=ps.required_until.max(proof_until);
    builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(d.id),Some(bytes(&d)));builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,slot_key(*signer,q.slot),Some(bytes(&d.id)));builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(22u8,h.id,2u8)),Some(bytes(&(streams.root(),streams))));builder.write_kernel_row(STAGE_TABLE_V14,key(16,&h.id),Some(bytes(&ps)));builder.note_kernel_bond(signer);Ok(())
}

fn check_original(d:&NativeStagePendingV28,h:&JobHeader)->Result<(),PalwStateV2Error>{
    if d.request.job!=h.id||d.original!=h.original||d.consent!=h.consent||d.contract!=h.contract.root()||d.source_plan!=h.contract.source_plan||d.caller_plan!=h.contract.caller_source||d.producer!=h.producer||d.deadline<d.filed||d.proof_until<=d.deadline||d.attempts>2{return Err(refused("stage28 pending does not bind immutable original job/source/caller/producer/deadline"));}Ok(())
}
fn closed_pursuit(builder:&TransitionBuilder<'_>,h:&JobHeader,d:&NativeStagePendingV28)->Result<Pursuit,PalwStateV2Error>{
    let mut row=pursuit(builder,h)?;let slot=row.open.iter_mut().find(|id|**id==d.id).ok_or_else(||refused("stage28 original pending membership absent"))?;*slot=[0;64];row.pending=row.pending.checked_sub(1).ok_or_else(||refused("stage28 original pending count underflow"))?;Ok(row)
}
fn write_closed(builder:&mut TransitionBuilder<'_>,h:&JobHeader,d:&mut NativeStagePendingV28,row:&Pursuit){
    d.settled=true;builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(d.id),Some(bytes(d)));builder.write_kernel_row(STAGE_TABLE_V14,key(16,&h.id),Some(bytes(row)));builder.note_kernel_bond(&d.requester);
}
pub(super) fn reserved(r:&PalwKernelRouteStateV1,bond:&PalwBondKeyV2)->u128 {
    (0..2u8).filter_map(|slot|r.aux_row::<Digest>(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,&slot_key(*bond,slot))).filter_map(|id|pending(r,id).ok()).filter(|d|d.requester==*bond&&!d.settled).map(|d|d.bond as u128).sum()
}
pub(super) fn answer(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    if body.len()<68||body.len()-68>SOURCE_ANSWER_MAX_BYTES_V16 as usize{return Err(refused("stage28 original answer fixed prefix/cap"));}
    let id:Digest=body[..64].try_into().unwrap();let mut d=pending(builder.state.kernel_route.as_ref().unwrap(),id)?;let h=original_header(builder,d.request.job)?;check_original(&d,&h)?;
    if d.settled||d.attempts>=2||h.terminal!=StageTerminalV28::Challengeable||d.producer!=*signer||ctx.daa_score>d.deadline||builder.state.bonds.get(signer).is_none_or(|b|b.operator_id.as_bytes()!=d.producer_operator){return Err(refused("stage28 original producer answer/pending/operator/window differs"));}
    let epoch=TrustedEpochV11::from_chain(&p.execution.quota,ctx.block.as_bytes(),ctx.daa_score).map_err(refused)?;let quote=source_quote(builder,p,&h)?;let mut producer=producer_shard(builder,p,&epoch,&h,&quote)?;
    // Debit the actual bounded packet before trusting its nested declaration,
    // allocating the answer, or restoring any class/input/trace data.
    producer.debit_answer(&p.execution.quota,&epoch,&quote,&palw_kernel_bond_id_v1(signer),(body.len()-68) as u64).map_err(refused)?;d.attempts+=1;
    super::super::super::palw_flat_execution_fold_v11::write_source_shard(builder,&producer,h.id);builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(id),Some(bytes(&d)));
    let n=u32::from_le_bytes(body[64..68].try_into().unwrap()) as usize;if n!=body.len()-68{return Err(refused("stage28 exact original answer length differs"));}
    finish_answer(builder,ctx,p,&h,&mut d,&body[68..])
}
fn finish_answer(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,p:&NativeStagePoliciesV28,h:&JobHeader,d:&mut NativeStagePendingV28,answer:&[u8])->Result<(),PalwStateV2Error>{
    let(class,actual,plan,book,data)=restore_job(builder,p,h)?;if contract(builder,p,&class,&actual,&plan)?.record()!=&h.contract||book.claim()!=Some(d.claim)||book.producer_operator()!=d.producer_operator{return Err(refused("stage28 answer rebinds original source/claim/operator"));}
    let actual32=master32::actual_job_fixed(builder,h,&class)?;
    authenticate_answer(&class,actual32.as_ref().unwrap_or(&actual),&data,d,answer)?;
    let row=closed_pursuit(builder,h,d)?;if builder.state.bonds.get(&d.requester).is_none_or(|b|b.collateral<d.bond||b.operator_id.as_bytes()!=d.requester_operator){return Err(refused("stage28 original actual requester delivery fee unavailable"));}
    if h.contract.producer_delivery_fee_cap!=0{return Err(refused("stage28 original burn-only delivery terms differ"));}
    if builder.slash_bond(d.requester,d.bond as u128)?!=d.bond{return Err(PalwStateV2Error::BondBudgetRefused("stage28 actual delivery fee collection changed after preflight".into()));}
    builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SERVED_TABLE_V11,served_key(d.unit),Some(answer.to_vec()));write_closed(builder,h,d,&row);Ok(())
}
pub(super) fn settle(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    let id:Digest=body.try_into().map_err(|_|refused("stage28 default original fixed64-byte unit"))?;let mut d=pending(builder.state.kernel_route.as_ref().unwrap(),id)?;let mut h=original_header(builder,d.request.job)?;check_original(&d,&h)?;
    if d.settled||ctx.daa_score<=d.deadline{return Err(refused("stage28 original answer window remains open or settled"));}
    let(s,c)=original_extents(builder,h.class)?;if !pay_restore(builder,ctx,signer,p,s,c,h.input_bytes,64)?{return Ok(());}
    let row=closed_pursuit(builder,&h,&d)?;
    if matches!(h.terminal,StageTerminalV28::Refunded|StageTerminalV28::Convicted|StageTerminalV28::Final){write_closed(builder,&h,&mut d,&row);return Ok(());}
    let(class,actual,plan,mut book,mut data)=restore_job(builder,p,&h)?;
    if book.terminal()!=StageTerminalV28::Challengeable||book.claim()!=Some(d.claim)||book.producer_operator()!=d.producer_operator||builder.state.bonds.get(&d.requester).is_none_or(|b|b.operator_id.as_bytes()!=d.requester_operator)||builder.state.bonds.get(&h.producer).is_none_or(|b|b.operator_id.as_bytes()!=d.producer_operator||b.collateral<book.reserve())||d.requester_operator==d.producer_operator{return Err(refused("stage28 default lacks exact original independent requester and collectible producer promise"));}
    // Re-authenticate the immutable coordinate. External R availability never
    // appears here: only the producer's signed bound-PC/trace answer promise.
    if check_coordinate(builder,&h,&class,&actual,&book,&d.request)?!=d.pc{return Err(refused("stage28 default coordinate differs from original bound producer unit"));}
    let proof=object_id(b"misaka-palw/native/stage-original-bound-unit-default/v28",&(d.id,d.unit,d.pc,d.deadline,d.contract));
    let established=d.deadline.checked_add(1).ok_or_else(||refused("stage28 original default time overflow"))?;
    let outcome=book.convict(&NativeStageConvictionV28{job:h.id,original:h.original,claim:d.claim,producer_operator:d.producer_operator,finder:palw_kernel_bond_id_v1(&d.requester),finder_operator:d.requester_operator,actual_collected:book.reserve(),proof,established}).map_err(refused)?;
    settle_conviction(builder,ctx,&d.requester,p,&class,&plan,&mut h,&mut data,&book,&outcome,false)?;
    write_closed(builder,&h,&mut d,&row);Ok(())
}

impl PalwKernelRouteStateV1 {
    /// Fixed original metadata only. This is a signature preflight/scheduler
    /// projection, never a court, Final or monetary grant.
    pub fn stage_source_answer_owner_v28(&self,id:&Digest,now:u64)->Option<PalwBondKeyV2>{
        let d=pending(self,*id).ok()?;let h:JobHeader=read(self,5,&d.request.job,8192)?;check_original(&d,&h).ok()?;
        if h.version!=28||h.id!=d.request.job||h.terminal!=StageTerminalV28::Challengeable||d.settled||d.attempts>=2||now>d.deadline{return None;}
        let raw=self.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(16u8,h.id))))?;if raw.len()>64+SOURCE_ROW_BYTES_V16 as usize{return None;}
        let(root,r):(Digest,SourceObligationV16)=borsh::from_slice(raw).ok()?;
        if r.root()!=root||r.retired||r.job_id!=h.id||r.plan_root!=d.source_plan||r.producer!=palw_kernel_bond_id_v1(&d.producer)||r.producer_operator!=d.producer_operator||r.original_consent_root!=h.consent||r.quota_plan_root!=h.quota_record.root()||r.answers_used>=r.requests_admitted.checked_mul(2)?{return None;}Some(d.producer)
    }
    pub fn stage_paid_answer_eligibility_v28(&self,id:Digest,now:u64,mass:&crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,service:&crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22)->Option<crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22>{
        let owner=self.stage_source_answer_owner_v28(&id,now)?;self.stage_answer_eligibility_impl_v28(id,now,mass,service,owner)
    }
    fn stage_answer_eligibility_impl_v28(&self,id:Digest,now:u64,mass:&crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,service:&crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22,owner:PalwBondKeyV2)->Option<crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22>{
        let d=pending(self,id).ok()?;let h:JobHeader=read(self,5,&d.request.job,8192)?;
        let table=crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
        let bounded=|k:Vec<u8>,cap|self.aux.get(&(table,k)).filter(|b|b.len()<=cap);
        let(root,j):(Digest,crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29)=borsh::from_slice(bounded(bytes(&(29u8,h.id,22u8)),2048)?).ok()?;
        let c:crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29=borsh::from_slice(bounded(bytes(&(29u8,h.id,23u8)),4096)?).ok()?;
        let(qroot,q):(Digest,crate::palw_native_queue_v30::NativeQueueLeaseV30)=borsh::from_slice(bounded(bytes(&(30u8,h.id,1u8)),1024)?).ok()?;
        if !service.implemented||service.check(mass).is_err()||j.root()!=root||j.namespace!=28||j.job!=h.id||j.funding!=h.original||j.maximum!=d.contract||j.service!=service.root()||j.lease.root()!=j.lease_root||j.lease.retired||c.root()!=h.contract.physical_plan||c.service!=j.service||c.original_job_plan!=h.quota_record.root()||q.root()!=qroot||q.retired||q.job!=h.id||q.original_funding!=h.original||q.original_consent!=h.consent||q.original_physical_lease!=j.lease_root||q.original_plan!=h.contract.queue_plan||q.original_policy!=h.contract.queue_policy||q.not_before<d.proof_until||mass.packet_mass(&c.answer_envelope).ok()?>service.slot_mass{return None;}
        Some(crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22{original_job:h.id,unit:id,owner,deadline:d.deadline,kind:if d.request.kind<=2{0}else{1},lease_root:j.lease_root,original_service:j.service,envelope:c.answer_envelope})
    }
    pub fn stage_reply_owner_v28(&self,id:Digest,group:Digest,index:u16,count:u16,total:u32,now:u64)->Option<PalwBondKeyV2>{
        let d=pending(self,id).ok()?;let h:JobHeader=read(self,5,&d.request.job,8192)?;check_original(&d,&h).ok()?;let(c,j)=original_reply_contract(self,&h).ok()?;
        if d.settled||h.terminal!=StageTerminalV28::Challengeable||now>d.deadline||group==[0;64]||index>=count||count==0||count>c.answer_parts||total==0||total>c.original_answer_bytes||total.div_ceil(32<<10)!=count as u32{return None;}
        let old=reply_progress(self,id).ok()?;
        let continuation=old.as_ref().is_some_and(|r|!r.burned&&!r.completed&&r.group==group&&r.count==count&&r.total==total&&r.attempt==d.attempts&&r.producer==palw_kernel_bond_id_v1(&d.producer)&&r.original_claim==d.claim&&r.original_source==d.source_plan&&r.original_lease==j.lease_root&&index==r.accepted.count_ones() as u16);
        if !continuation&&(index!=0||d.attempts>=2||old.as_ref().is_some_and(|r|!r.burned&&!r.completed)){return None;}
        // The second whole attempt's continuation remains eligible even after
        // both Source16 attempt debits; no third attempt can begin.
        if !continuation{self.stage_source_answer_owner_v28(&id,now)?;}
        Some(d.producer)
    }
    pub fn stage_paid_reply_eligibility_v28(&self,id:Digest,group:Digest,index:u16,count:u16,total:u32,now:u64,mass:&crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,service:&crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22)->Option<crate::palw_flat_execution_capacity_v22::NativePaidCarrierEligibilityV22>{
        let owner=self.stage_reply_owner_v28(id,group,index,count,total,now)?;let mut e=self.stage_answer_eligibility_impl_v28(id,now,mass,service,owner)?;e.kind=4;Some(e)
    }

}

pub(super) fn pending_owned(builder:&TransitionBuilder<'_>,h:&JobHeader,owner:Digest)->Result<u32,PalwStateV2Error>{
    let row=pursuit(builder,h)?;let mut count=0;for id in row.open.iter().filter(|id|**id!=[0;64]){let d=pending(builder.state.kernel_route.as_ref().unwrap(),*id)?;check_original(&d,h)?;if d.settled{return Err(refused("stage28 settled pending retained as open"));}if owner==[0;64]||d.request.entitlement==owner{count+=1;}}Ok(count)
}
pub(super) fn quiescent(builder:&TransitionBuilder<'_>,h:&JobHeader,now:u64)->Result<(),PalwStateV2Error>{
    let row=pursuit(builder,h)?;if row.pending!=0||now<=row.required_until{return Err(refused("stage28 original public source/trace pending or proof grace retained"));}let stream=slots(builder,h)?;if stream.owners.iter().any(|id|*id!=[0;64])||stream.not_before.iter().any(|at|now<=*at){return Err(refused("stage28 original physical owner or cooldown retained"));}Ok(())
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct Collection {version:u16,job:Digest,original:Digest,next:u32,total:u32}
/// At most 32 rooted original cached units are removed per ordinary paid call;
/// no unbounded row scan or producer cooperation is required after terminal.
pub(super) fn collect_units(builder:&mut TransitionBuilder<'_>,h:&JobHeader)->Result<bool,PalwStateV2Error>{
    let p=pursuit(builder,h)?;if p.pending!=0{return Err(refused("stage28 terminal collection still has original pending units"));}
    let mut cursor:Collection=read(builder.state.kernel_route.as_ref().unwrap(),18,&h.id,256).unwrap_or(Collection{version:28,job:h.id,original:h.original,next:0,total:p.requests});
    if cursor.version!=28||cursor.job!=h.id||cursor.original!=h.original||cursor.total!=p.requests||cursor.next>cursor.total{return Err(refused("stage28 original collection cursor differs"));}
    let end=cursor.next.saturating_add(32).min(cursor.total);
    let mut selected=Vec::new();for ordinal in cursor.next..end{let(id,unit,requester,slot):(Digest,Digest,PalwBondKeyV2,u8)=read(builder.state.kernel_route.as_ref().unwrap(),17,&(h.id,ordinal),256).ok_or_else(||refused("stage28 original source cache collection journal absent"))?;let d=pending(builder.state.kernel_route.as_ref().unwrap(),id)?;check_original(&d,h)?;if !d.settled||d.unit!=unit||d.requester!=requester||d.request.slot!=slot{return Err(refused("stage28 source cache has not genuinely settled"));}selected.push((ordinal,id,unit,requester,slot));}
    for(ordinal,id,unit,requester,slot)in selected{let k=slot_key(requester,slot);if builder.state.kernel_route.as_ref().unwrap().aux_row::<Digest>(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,&k)==Some(id){builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11,k,None);}builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(id),None);builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SERVED_TABLE_V11,served_key(unit),None);for attempt in 1..=2u8{for part in 0..4u16{builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SEAL_TABLE_V11,reply_part_key(id,attempt,part),None);}}builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,progress_key(id),None);builder.write_kernel_row(STAGE_TABLE_V14,key(17,&(h.id,ordinal)),None);}
    cursor.next=end;builder.write_kernel_row(STAGE_TABLE_V14,key(18,&h.id),Some(bytes(&cursor)));Ok(end==cursor.total)
}

fn progress_key(id:Digest)->Vec<u8>{bytes(&(28u8,id,29u8))}
fn reply_part_key(id:Digest,attempt:u8,index:u16)->Vec<u8>{bytes(&(28u8,id,attempt,index))}
fn reply_progress(r:&PalwKernelRouteStateV1,id:Digest)->Result<Option<crate::palw_native_reply_parts_v29::NativeReplyProgressV29>,PalwStateV2Error>{
    let Some(raw)=r.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,progress_key(id)))else{return Ok(None);};if raw.len()>1024{return Err(refused("stage28 reply fixed progress cap"));}let(root,p):(Digest,crate::palw_native_reply_parts_v29::NativeReplyProgressV29)=borsh::from_slice(raw).map_err(|_|refused("stage28 reply original fixed grammar"))?;p.check().map_err(refused)?;if p.root()!=root||p.pending!=id{return Err(refused("stage28 reply original progress differs"));}Ok(Some(p))
}
fn write_progress(builder:&mut TransitionBuilder<'_>,p:&crate::palw_native_reply_parts_v29::NativeReplyProgressV29){builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,progress_key(p.pending),Some(bytes(&(p.root(),p))));}
fn original_reply_contract(r:&PalwKernelRouteStateV1,h:&JobHeader)->Result<(crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29,crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29),PalwStateV2Error>{
    let table=crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;let raw=r.aux.get(&(table,bytes(&(29u8,h.id,23u8)))).ok_or_else(||refused("stage28 original reply carriage absent"))?;if raw.len()>4096{return Err(refused("stage28 original reply carriage cap"));}let c:crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29=borsh::from_slice(raw).map_err(|_|refused("stage28 original reply carriage grammar"))?;
    let raw=r.aux.get(&(table,bytes(&(29u8,h.id,22u8)))).ok_or_else(||refused("stage28 original reply physical lease absent"))?;if raw.len()>2048{return Err(refused("stage28 original reply lease cap"));}let(root,j):(Digest,crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29)=borsh::from_slice(raw).map_err(|_|refused("stage28 original reply lease grammar"))?;
    if c.root()!=h.contract.physical_plan||j.root()!=root||j.namespace!=28||j.job!=h.id||j.funding!=h.original||j.maximum!=h.contract.root()||j.service!=h.contract.physical||j.lease_root!=j.lease.root()||j.lease.retired{return Err(refused("stage28 original reply carriage/lease/source roots differ"));}Ok((c,j))
}
pub(super) fn reply_part(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    use crate::palw_native_reply_parts_v29::*;
    let part=borrowed_native_reply_metadata_v29(body).ok_or_else(||refused("stage28 original reply page bounded143-byte metadata"))?;
    let mut d=pending(builder.state.kernel_route.as_ref().unwrap(),part.pending)?;let h=original_header(builder,d.request.job)?;check_original(&d,&h)?;let (carriage,lease)=original_reply_contract(builder.state.kernel_route.as_ref().unwrap(),&h)?;
    if d.settled||d.producer!=*signer||h.terminal!=StageTerminalV28::Challengeable||ctx.daa_score>d.deadline||builder.state.bonds.get(signer).is_none_or(|b|b.operator_id.as_bytes()!=d.producer_operator)||part.count>carriage.answer_parts||part.total>carriage.original_answer_bytes||part.group==[0;64]{return Err(refused("stage28 original producer reply owner/count/window/source maximum differs"));}
    let old=reply_progress(builder.state.kernel_route.as_ref().unwrap(),d.id)?;
    let start=old.as_ref().is_none_or(|r|r.burned||r.completed);
    let mut progress=if start{
        if part.index!=0||d.attempts>=2{return Err(refused("stage28 original reply whole attempt exhausted or first page missing"));}
        let epoch=TrustedEpochV11::from_chain(&p.execution.quota,ctx.block.as_bytes(),ctx.daa_score).map_err(refused)?;let quote=source_quote(builder,p,&h)?;let mut producer=producer_shard(builder,p,&epoch,&h,&quote)?;producer.debit_answer(&p.execution.quota,&epoch,&quote,&palw_kernel_bond_id_v1(signer),part.total as u64).map_err(refused)?;d.attempts+=1;super::super::super::palw_flat_execution_fold_v11::write_source_shard(builder,&producer,h.id);builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11,pending_key(d.id),Some(bytes(&d)));
        NativeReplyProgressV29{version:29,pending:d.id,producer:palw_kernel_bond_id_v1(signer),original_claim:d.claim,original_source:d.source_plan,original_lease:lease.lease_root,attempt:d.attempts,group:part.group,count:part.count,total:part.total,accepted:0,burned:false,completed:false}
    }else{old.unwrap()};
    // Full whole-attempt price is already persisted, including all original
    // four page copies and assembly. Any invalid page burns this attempt.
    if progress.original_claim!=d.claim||progress.original_source!=d.source_plan||progress.original_lease!=lease.lease_root||progress.producer!=palw_kernel_bond_id_v1(signer)||progress.attempt!=d.attempts{return Err(refused("stage28 original producer reply progress rebound"));}
    let exact=borrowed_native_reply_part_v29(body);
    if progress.group!=part.group||progress.count!=part.count||progress.total!=part.total||part.index!=progress.accepted.count_ones() as u16||exact.is_none(){progress.burned=true;write_progress(builder,&progress);return Ok(());}
    progress.accepted|=1u8<<part.index;write_progress(builder,&progress);builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SEAL_TABLE_V11,reply_part_key(d.id,progress.attempt,part.index),Some(part.bytes.to_vec()));
    if progress.accepted!=(1u8<<progress.count)-1{return Ok(());}
    let mut answer=Vec::with_capacity(progress.total as usize);for index in 0..progress.count{let raw=builder.state.kernel_route.as_ref().unwrap().aux.get(&(crate::palw_flat_execution_v11::EXECUTION_SEAL_TABLE_V11,reply_part_key(d.id,progress.attempt,index))).ok_or_else(||refused("stage28 original paid reply page absent"))?;if raw.len()>32<<10{return Err(refused("stage28 original reply page extent differs"));}answer.extend_from_slice(raw);}
    if answer.len()!=progress.total as usize||native_reply_group_v29(d.id,&answer)!=progress.group{progress.burned=true;write_progress(builder,&progress);return Ok(());}
    let result=finish_answer(builder,ctx,p,&h,&mut d,&answer);
    match result{Ok(())=>{progress.completed=true;write_progress(builder,&progress);},Err(PalwStateV2Error::KernelRouteRefused(_))=>{progress.burned=true;write_progress(builder,&progress);},Err(e)=>return Err(e)}
    for index in 0..progress.count{builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SEAL_TABLE_V11,reply_part_key(d.id,progress.attempt,index),None);}Ok(())
}
