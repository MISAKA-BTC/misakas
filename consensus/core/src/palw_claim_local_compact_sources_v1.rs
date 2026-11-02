//! Finite full-frame retention with an owning consensus-derived closed-source
//! ledger. This preserves source standing after its actual duties have ended;
//! it is not a producer DTO or a proof of model computation correctness.
use super::*;
use super::palw_claim_local_rewards_v1 as rewards;
use crate::palw_claim_local_v1::*;
use crate::palw_native_resource_v1::{NativeRetainedV1,NativeHeapV1};
use borsh::{BorshSerialize,BorshDeserialize};
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct PalwClosedClaimSourceV1{
    pub version:u16,pub ruleset:crate::Hash,pub claim:Hash64,pub birth:PalwBlockContextV2,
    /// Exact original accepted record, not a later observation/redated birth.
    pub original:PalwClaimStateV2,pub terminal_phase:PalwClaimPhaseV2,
    pub closed_at:PalwBlockContextV2,pub material_until:u64,
    pub winner:Option<Hash64>,pub approval:Vec<u8>,pub permitted:bool,
}
crate::palw_native_resource_v1::native_struct!(PalwClosedClaimSourceV1;version,ruleset,claim,birth,original,terminal_phase,closed_at,material_until,winner,approval,permitted);
crate::palw_native_resource_v1::wire::wire_struct!(PalwClosedClaimSourceV1;u16,crate::Hash,Hash64,PalwBlockContextV2,PalwClaimStateV2,PalwClaimPhaseV2,PalwBlockContextV2,u64,Option<Hash64>,Vec<u8>,bool);
fn bad(s:impl ToString)->PalwStateV2Error{PalwStateV2Error::CarriageInconsistent(s.to_string())}
pub const PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1:u8=b'S';
fn key(id:Hash64)->[u8;65]{let mut k=[0;65];k[0]=b'S';k[1..].copy_from_slice(id.as_byte_slice());k}
fn decode(bytes:&[u8])->Result<NativeRetainedV1<PalwClosedClaimSourceV1>,PalwStateV2Error>{rewards::decoded(bytes)}
fn same_birth(a:&PalwClaimStateV2,b:&PalwClaimStateV2)->bool{a.accepted_block==b.accepted_block&&a.accepted_blue_score==b.accepted_blue_score&&a.accepted_daa==b.accepted_daa}
pub(super) fn read(state:&PalwChainStateV2,id:Hash64)->Result<Option<NativeRetainedV1<PalwClosedClaimSourceV1>>,PalwStateV2Error>{
    let Some(route)=&state.kernel_route else{return Ok(None)};
    let _k=crate::palw_native_resource_v1::reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let Some(raw)=route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1,key(id).to_vec()))else{return Ok(None)};
    Ok(Some(decode_checked_index_v15(raw,id)?))
}
/// Exact native typed S1 decoder shared with the derived projection writer.
pub(super) fn decode_checked_index_v15(raw:&[u8],id:Hash64)->Result<NativeRetainedV1<PalwClosedClaimSourceV1>,PalwStateV2Error>{
    let row=decode(raw)?;
    if row.version!=1||row.claim!=id||row.original.accepted_block==Hash64::default()
        ||row.birth.daa_score!=row.original.accepted_daa||row.birth.blue_score!=row.original.accepted_blue_score
        ||row.material_until<row.original.trace_retention_daa||row.birth.blue_score>row.closed_at.blue_score
        ||row.birth.daa_score>row.closed_at.daa_score||!matches!(row.terminal_phase,PalwClaimPhaseV2::Final{..}|PalwClaimPhaseV2::Voided{..}){
        return Err(bad("compact source actual birth/terminal/duty scope"))
    }
    let terminal_daa=match row.terminal_phase{PalwClaimPhaseV2::Final{final_daa}=>final_daa,PalwClaimPhaseV2::Voided{voided_daa,..}=>voided_daa,_=>unreachable!()};
    if terminal_daa<row.birth.daa_score||terminal_daa>row.closed_at.daa_score{return Err(bad("compact source terminal clock"))}
    Ok(row)
}
pub(super) fn recorded(state:&PalwChainStateV2,id:Hash64,born:&PalwClaimStateV2)->Result<Option<rewards::RewardReadinessV1>,PalwStateV2Error>{
    if let Some(row)=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(state,id,born)?{return Ok(Some(if row.permitted{rewards::RewardReadinessV1::Ready}else{rewards::RewardReadinessV1::Denied}))}
    Ok(read(state,id)?.filter(|r|same_birth(&r.original,born)).map(|r|if r.permitted{rewards::RewardReadinessV1::Ready}else{rewards::RewardReadinessV1::Denied}))
}
/// Match the exact source identity actually visible in the consumer's retained
/// parent. A coincidentally equal id/root or a later re-admission is insufficient.
pub(super) fn carrier(state:&PalwChainStateV2,consumer:&PalwClaimLocalAdmissionContextV1,id:Hash64)
    ->Result<Option<rewards::RewardReadinessV1>,PalwStateV2Error>{
    let Some(source)=read(state,id)?else{return Ok(None)};
    let born=consumer.original_claim();
    if source.birth.blue_score>=born.accepted_blue_score||source.birth.daa_score>born.accepted_daa{return Err(bad("compact carrier source is not causally prior"))}
    rewards::frame_read(consumer.frame(),|prior,_|{
        let visible=if let Some(original)=prior.claims.get(&id){same_birth(&source.original,original)}else{
            use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
            source.winner.is_some_and(|winner|prior.kernel_route.as_ref().and_then(|r|r.aux.iter()
                .find(|((t,k),_)|*t==PALW_FP_WINNING_CARRIER_TABLE_V1&&k.as_slice()==id.as_byte_slice()).map(|(_,raw)|raw))
                .is_some_and(|raw|raw.as_slice()==winner.as_byte_slice()))
        };
        Ok(visible.then_some(if source.permitted{rewards::RewardReadinessV1::Ready}else{rewards::RewardReadinessV1::Denied}))
    })
}
pub(super) fn shared_root(state:&PalwChainStateV2,born:&PalwClaimStateV2)->Result<rewards::RewardReadinessV1,PalwStateV2Error>{
    if born.execution_root==ZERO_HASH64{return Ok(rewards::RewardReadinessV1::Ready)}
    if let Some(result)=super::palw_claim_local_execution_cohort_census_v15::compact_shared_root_v15(state,born)?{return Ok(result)}
    shared_root_legacy_v15(state,born)
}
/// Literal legacy fallback when native runtime projection is unavailable.
pub(super) fn shared_root_legacy_v15(state:&PalwChainStateV2,born:&PalwClaimStateV2)->Result<rewards::RewardReadinessV1,PalwStateV2Error>{
    if born.execution_root==ZERO_HASH64{return Ok(rewards::RewardReadinessV1::Ready)}
    if let Some(route)=&state.kernel_route{for ((table,k),raw) in &route.aux{
        if *table!=PALW_CLAIM_LOCAL_TABLE_V1||k.len()!=65||k[0]!=b'S'{continue}
        let row=decode(raw)?;
        if row.original.execution_root==born.execution_root&&row.birth.blue_score<=born.accepted_blue_score&&row.birth.daa_score<=born.accepted_daa&&!row.permitted{
            return Ok(rewards::RewardReadinessV1::Denied)
        }
    }}Ok(rewards::RewardReadinessV1::Ready)
}
fn enabled(b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->bool{b.params.claim_local_settlement_active_at(ctx.daa_score)&&b.extras.native_fact_ruleset.is_some()}
pub(super) fn compact_and_collect_v1(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<(),PalwStateV2Error>{
    if !enabled(b,ctx){return Ok(())}
    let saved=b.state.last_point;b.state.last_point=Some(*ctx);
    let result=compact_inner(b,ctx);b.state.last_point=saved;result
}
fn compact_inner(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<(),PalwStateV2Error>{
    // Revisit actual recorded removals: a dependent source can become closed
    // later. Never discard it merely because readiness was held at retirement.
    let count=rewards::frames(&b.state).count();
    let _keys=crate::palw_native_resource_v1::reserve_native_requests_v1((count as u64).checked_mul(65).ok_or_else(||bad("compact frame keys overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
    let mut keys=Vec::<[u8;65]>::with_capacity(count);
    for(k,_)in rewards::frames(&b.state){keys.push(k.as_slice().try_into().map_err(|_|bad("typed frame key"))?)}
    for frame_key in keys{
        let Some(raw)=b.state.kernel_route.as_ref().and_then(|r|r.aux.iter().find(|((t,k),_)|*t==PALW_CLAIM_LOCAL_TABLE_V1&&k.as_slice()==frame_key).map(|(_,v)|v))else{continue};
        let frame=rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        super::palw_claim_local_kernel_origins_v1::mint_from_frame(b,ctx,&frame)?;
        for (retirement_ordinal,event) in frame.native_changes.iter().enumerate(){
            if event.owner!=PalwClaimLocalRowOwnerV1::ChainClaims||event.new.is_some(){continue}
            let Some(raw)=&event.old else{continue};
            let id=Hash64::from_bytes(event.key.as_slice().try_into().map_err(|_|bad("compact removal claim key"))?);
            let terminal=rewards::decoded::<PalwClaimStateV2>(raw)?;
            mint_source(b,ctx,id,&terminal,&frame,retirement_ordinal)?;
        }
    }
    super::palw_claim_local_prefix_reservation_v1::collect_closed_exports_v1(b,ctx)?;
    collect_frames(b,ctx)
}
fn mint_source(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,terminal:&PalwClaimStateV2,frame:&PalwClaimLiabilityFrameV1,retirement_ordinal:usize)->Result<(),PalwStateV2Error>{
        // This is the exact OLD retirement writer value, not the current raw-ID
        // claim (which may already be a later birth in this same transition).
        if b.state.claims.get(&id).is_some_and(|c|same_birth(c,terminal)){return Ok(())}
        let Some(origin)=super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&b.state,b.params,id,terminal)?else{return Ok(())};
        if super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(&b.state,id,origin.original_claim())?.is_some(){return Ok(())}
        if let Some(old)=super::palw_claim_local_compact_sources_v1::read(&b.state,id)?.filter(|r|r.birth==origin.accepting_point()){
            super::palw_claim_local_exact_closed_source_v5::publish_closed_source_v5(b,&old)?;
            return Ok(())
        }
        let readiness=rewards::palw_claim_local_recorded_settlement_v1(&b.state,b.params,id,terminal,ctx.daa_score)?;
        let permitted=match readiness{PalwClaimLocalSettlementV1::Settled=>true,PalwClaimLocalSettlementV1::Denied=>false,_=>return Ok(())};
        let mut cert_key=[0;65];cert_key[0]=crate::palw_security_certification_v1::CERTIFICATION_CLAIM_PREFIX_V1;cert_key[1..].copy_from_slice(&crate::palw_security_certification_v1::palw_certification_id_v1(&id));
        let changed=frame.native_changes[..retirement_ordinal].iter().rev().find(|e|e.owner==PalwClaimLocalRowOwnerV1::KernelRoute&&e.table==crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1&&e.key.as_slice()==cert_key);
        // An observed deletion is true absence. Neither a later birth's
        // certification nor the deleted old bytes replace the actual old row.
        let raw=changed.and_then(|e|e.new.as_deref());
        let approval_len=if changed.is_some(){raw.map_or(0,|r|r.len())}else{rewards::frame_read(&frame,|prior,_|Ok(rewards::cert_bytes(prior.kernel_route.as_ref(),id).map_or(0,|v|v.len())))?};
        let original=origin.original_claim();
        let shape=original.native_heap_v1("compact original claim").add::<PalwClosedClaimSourceV1>(terminal.phase.native_heap_v1("compact terminal phase"),"compact terminal phase");
        let requests=shape.bytes().ok_or_else(||PalwStateV2Error::NativeResource(shape))?
            .checked_add(approval_len as u64).and_then(|n|n.checked_add((std::mem::size_of::<NativeRetainedV1<PalwClosedClaimSourceV1>>()+std::mem::size_of::<[usize;3]>()) as u64)).ok_or_else(||bad("compact source owner quote overflow"))?;
        let lease=crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
        let approval=if changed.is_some(){raw.unwrap_or(&[]).to_vec()}else{rewards::frame_read(&frame,|prior,_|Ok(rewards::cert_bytes(prior.kernel_route.as_ref(),id).unwrap_or(&[]).to_vec()))?};
        if permitted&&rewards::approval(b.params,id,terminal,Some(&approval))?!=rewards::RewardReadinessV1::Ready{return Ok(())}
        let winner_change=frame.native_changes[..retirement_ordinal].iter().rev().find(|e|e.owner==PalwClaimLocalRowOwnerV1::KernelRoute&&e.table==crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1&&e.key.as_slice()==id.as_byte_slice());
        let winner=if let Some(change)=winner_change{change.new.as_deref().map(|v|borsh::from_slice::<Hash64>(v).map_err(bad)).transpose()?}else{
            rewards::frame_read(frame,|prior,_|prior.kernel_route.as_ref().and_then(|r|r.aux.iter().find(|((t,k),_)|*t==crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1&&k.as_slice()==id.as_byte_slice()).map(|(_,v)|v)).map(|v|borsh::from_slice::<Hash64>(v).map_err(bad)).transpose())?
        };
        let mut material_until=original.trace_retention_daa.max(terminal.trace_retention_daa);
        if let Some(reservation)=b.state.prefix_export_reservation_v1(&origin.accepting_point(),id)?{material_until=material_until.max(reservation.material_until)}
        let row=NativeRetainedV1::from_owner_reserved_v1(PalwClosedClaimSourceV1{version:1,ruleset:b.extras.native_fact_ruleset.expect("enabled actual network"),claim:id,birth:origin.accepting_point(),original:original.clone(),terminal_phase:terminal.phase.clone(),closed_at:*ctx,material_until,winner,approval,permitted},lease);
        super::palw_claim_local_exact_closed_source_v5::publish_closed_source_v5(b,&row)?;
        // Preserve S1 compatibility without overwriting another born cohort.
        if !b.state.claims.contains_key(&id)&&read(&b.state,id)?.is_none(){b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key(id),Some(&*row))?;}
        Ok(())
}

fn collect_frames(b:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2)->Result<(),PalwStateV2Error>{
    let mut cutoff=ctx.blue_score;
    for claim in b.state.claims.values(){cutoff=cutoff.min(claim.accepted_blue_score)}
    if let Some(route)=&b.state.kernel_route{for ((table,k),raw) in &route.aux{
        if *table==PALW_CLAIM_LOCAL_TABLE_V1&&k.len()==65&&k[0]==b'S'{
            let row=decode(raw)?;if ctx.daa_score<=row.material_until{cutoff=cutoff.min(row.birth.blue_score)}
        }
        if *table==crate::palw_state_v2::LEGACY_PENDING_FACT_TABLE_V1{
            let row=rewards::decoded::<crate::palw_state_v2::LegacyPendingFactV1>(raw)?;
            cutoff=cutoff.min(row.claim.accepted_blue_score);
        }
        if *table==PALW_CLAIM_LOCAL_TABLE_V1&&k.len()==65&&k[0]==b'E'{
            let row=rewards::decoded::<crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1>(raw)?;
            cutoff=cutoff.min(row.point.blue_score);
        }
    }}
    let anchors=crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(&b.state)?;
    let count=rewards::frames(&b.state).count();
    let _keys=crate::palw_native_resource_v1::reserve_native_requests_v1((count as u64).checked_mul(65).ok_or_else(||bad("frame collection keys overflow"))?).map_err(PalwStateV2Error::NativeResource)?;
    let mut candidates=Vec::<[u8;65]>::with_capacity(count);
    let mut latest=None::<(u64,Hash64)>;
    for (k,raw) in rewards::frames(&b.state){
        let frame=rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        frame.validate_context_v1().map_err(|e|match e{PalwClaimCheckpointErrorV1::Resource(r)=>PalwStateV2Error::NativeResource(r),other=>bad(other)})?;
        if latest.is_none_or(|(blue,_)|blue<frame.point.blue_score){latest=Some((frame.point.blue_score,frame.point.block))}
        for event in &frame.native_changes{
            if event.old.is_some(){continue}let Some(new)=&event.new else{continue};
            let protect=match (event.owner,event.table){
                (PalwClaimLocalRowOwnerV1::ChainClaims,_)=>{
                    let id=Hash64::from_bytes(event.key.as_slice().try_into().map_err(|_|bad("frame collection birth key"))?);
                    // If a retired source cannot yet obtain a closed decision,
                    // retain its complete history. Missing origin is never success.
                    b.state.claims.contains_key(&id)||read(&b.state,id)?.is_none()
                },
                (PalwClaimLocalRowOwnerV1::KernelRoute,misaka_palw_kernel::rows::TABLE_CLAIMS_V1)=>{
                    let id=Hash64::from_bytes(event.key.as_slice().try_into().map_err(|_|bad("kernel origin key width"))?);
                    rewards::kernel_claim_row(&b.state,&id).is_some()&&super::palw_claim_local_kernel_origins_v1::read(&b.state,id)?.is_none()
                },
                (PalwClaimLocalRowOwnerV1::KernelRoute,crate::palw_native_mining_v39::MINING_TABLE_V39)
                    if event.key.starts_with(&[39,crate::palw_native_mining_v39::MINING_CLAIM_KIND_V39])=>{
                    let row=rewards::decoded::<(misaka_palw_kernel::hash::Digest,crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>(new)?;
                    b.state.kernel_route.as_ref().is_some_and(|r|r.aux.iter().any(|((t,k),_)|*t==event.table&&k.as_slice()==event.key.as_slice()))
                        ||anchors.iter().any(|(_,a)|a.claim.as_bytes()==row.1.id)
                },
                (PalwClaimLocalRowOwnerV1::ExecV2,PALW_EXEC_V2_TABLE_ROOTS_V1)=>{
                    let id:Hash64=borsh::from_slice(&event.key).map_err(bad)?;b.state.exec_v2.roots.contains_key(&id)||b.state.exec_v2_rewards.contains_key(&id)
                },
                (PalwClaimLocalRowOwnerV1::ExecV2,PALW_EXEC_V2_TABLE_SLICES_V1)=>{
                    let id:(Hash64,u32)=borsh::from_slice(&event.key).map_err(bad)?;b.state.exec_v2.slices.contains_key(&id)
                },_=>false,
            };
            if protect{cutoff=cutoff.min(frame.point.blue_score)}
        }
        candidates.push(k.as_slice().try_into().map_err(|_|bad("frame collection canonical key"))?);
    }
    // Preserve the latest actual executed native context even without a claim
    // vertex (e.g. an in-flight owning job). Old full snapshots can be released.
    for k in candidates{
        let block=Hash64::from_bytes(k[1..].try_into().expect("frame key width"));
        if latest.is_some_and(|(_,latest)|latest==block){continue}
        if super::palw_claim_local_legacy_terminal_v18::immutable_frame_pinned_v18(&b.state,block)?{continue}
        let Some(raw)=b.state.kernel_route.as_ref().and_then(|r|r.aux.iter().find(|((t,key),_)|*t==PALW_CLAIM_LOCAL_TABLE_V1&&key.as_slice()==k).map(|(_,v)|v))else{continue};
        let point=PalwBlockContextV2::deserialize_reader(&mut raw.as_slice()).map_err(bad)?;
        if point.blue_score<cutoff{b.write_kernel_row(PALW_CLAIM_LOCAL_TABLE_V1,k.to_vec(),None)?;}
    }
    Ok(())
}

pub fn validate_compact_sources_context_v1(state:&PalwChainStateV2,global:&crate::config::params::Params)->Result<(),PalwStateV2Error>{
    super::palw_claim_local_kernel_origins_v1::validate_context(state,global)?;
    super::palw_claim_local_paid_archive_v2::validate_network_context_v3(state,global)?;
    super::palw_claim_local_prefix_financial_archive_v4::validate_network_context_v4(state,global)?;
    super::palw_claim_local_exact_closed_source_v5::validate_network_context_v5(state,global)?;
    super::palw_claim_local_legacy_terminal_v18::validate_network_context_v18(state,global)?;
    super::palw_claim_local_native_cohort_baseline_v22::validate_network_v22(state,global)?;
    super::palw_claim_local_source_admission_v8::validate_source_network_v8(state,global)?;
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)=&global.palw_consensus_mode else{
        if state.kernel_route.as_ref().is_some_and(|r|r.aux.keys().any(|(t,k)|*t==PALW_CLAIM_LOCAL_TABLE_V1&&k.len()==65&&k[0]==b'S')){return Err(bad("compact sources require actual V2 network rules"))}return Ok(())
    };
    if let Some(route)=&state.kernel_route{for ((table,k),_) in &route.aux{
        if *table!=PALW_CLAIM_LOCAL_TABLE_V1||k.len()!=65||k[0]!=b'S'{continue}
        let id=Hash64::from_bytes(k[1..].try_into().expect("canonical compact key"));let row=read(state,id)?.expect("existing compact source");
        if row.ruleset!=global.consensus_params_id()||state.last_point().is_none_or(|last|row.closed_at.blue_score>last.blue_score||row.closed_at.daa_score>last.daa_score)
            ||!bundle.state.claim_local_settlement_active_at(row.birth.daa_score){return Err(bad("compact source differs from original network/fence context"))}
        if row.permitted&&(!matches!(row.terminal_phase,PalwClaimPhaseV2::Final{..})
            ||rewards::approval(&bundle.state,id,&row.original,Some(&row.approval))?!=rewards::RewardReadinessV1::Ready){return Err(bad("compact permitted source lacks selected approval"))}
        validate_compact_source_winner_v5(state,&bundle.state,id,&row)?;
    }}Ok(())
}

// Candidate: requires Item5 actual-retirement A5 factory and module registration.
// Current row167 belongs to the active/raw slot; never use a new birth's slot
// as authority for the old S1 winner. Preserve same-birth corruption refusal.
pub(super) fn validate_compact_source_winner_v5(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,id:Hash64,row:&PalwClosedClaimSourceV1,
)->Result<(),PalwStateV2Error>{
    if let Some(exact)=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(state,id,&row.original)?{
        if *exact!=*row{return Err(bad("compact source contradicts exact A5 body"))}
    }
    let Some(winner)=row.winner else{return Ok(())};
    let current=state.kernel_route.as_ref().and_then(|route|route.aux.iter()
        .find(|((t,key),_)|*t==crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1
            &&key.as_slice()==id.as_byte_slice()).map(|(_,raw)|raw));
    let matches=current.is_some_and(|raw|raw.as_slice()==winner.as_byte_slice());
    let same_live=state.claims.get(&id).is_some_and(|live|same_birth(live,&row.original));
    if same_live {
        if !matches{return Err(bad("compact same-birth winning carrier differs"))}
        return Ok(())
    }
    let different_live=state.claims.get(&id).is_some();
    if !different_live&&matches{return Ok(())}
    // This reader checks BOTH birth directories, authenticated native body,
    // and A5 inventory. Unlike read_exact_closed_source_v5 it NEVER falls back
    // to S1, which would compare the untrusted row against itself.
    let exact=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(state,id,&row.original)?
        .ok_or_else(||bad("compact old winner requires exact A5 authority"))?;
    if exact.birth!=row.birth{return Err(bad("compact old winner accepting birth differs"))}
    if *exact!=*row{return Err(bad("compact source differs from exact old A5 body"))}
    let _=params; // Exact API enforces native authority; outer validator enforces network/fence.
    Ok(())
}

/// Exact existing S1 compatibility rule shared by the native retirement owner.
/// A later born source's raw-id slot is never overwritten.
pub(super) fn publish_compatible_closed_v18(b:&mut TransitionBuilder<'_>,row:&PalwClosedClaimSourceV1)->Result<(),PalwStateV2Error>{
    if !b.state.claims.contains_key(&row.claim)&&read(&b.state,row.claim)?.is_none(){b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1,&key(row.claim),Some(row))?;}Ok(())
}
