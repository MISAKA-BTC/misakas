//! API-dependent consumer candidate. No full F1/A5 body is required by a
//! source view. Native terminal summaries are NEVER inferred from E expiry.
use super::*;
use crate::palw_native_resource_v1::{NativeDemandLeaseV1,reserve_native_requests_v1};
use super::super::palw_claim_local_source_origin_v8::SourceOriginV8;
use super::super::palw_claim_local_carrier_lineage_v9::{self as lineage,CarrierOutcomeV9};
type Birth=(Hash64,Hash64,u64,u64);
type Entry=(Option<SourceOriginV8>,Option<NativeDemandLeaseV1>);
fn enqueue(pending:&mut BTreeMap<Birth,Entry>,origin:SourceOriginV8)->Result<(),PalwStateV2Error>{
    let point=origin.accepting_point();let key=(origin.claim_id(),point.block,point.daa_score,point.blue_score);
    if pending.contains_key(&key){return Ok(())}
    let n=crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Birth,Entry>();
    let lease=reserve_native_requests_v1(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    pending.insert(key,(Some(origin),lease));Ok(())
}
/// This is a genuine borrowed native owner proof, NOT a caller certificate.
/// It preserves established compact CHAIN settlement semantics, without minting
/// FULL native PermittedClosed or guessing E scope from carried RED provenance.
fn borrowed_closed_chain_readiness_v9(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,now:u64,source_id:Hash64,
    actual_point:PalwBlockContextV2,winner:Hash64,original:Option<&PalwClaimStateV2>,
    closed:&super::super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1,
)->Result<RewardReadinessV1,PalwStateV2Error>{
    if closed.version!=1||closed.claim!=source_id||closed.birth!=actual_point||closed.winner!=Some(winner)
        ||original.is_some_and(|c|c!=&closed.original)
        ||!matches!(closed.original.source,PalwClaimSourceV2::FreePrompt{..})
        ||closed.original.accepted_block!=actual_point.block
        ||closed.original.accepted_daa!=actual_point.daa_score||closed.original.accepted_blue_score!=actual_point.blue_score{
        return Err(bad("borrowed compact carrier proof changes exact source birth/winner/original"))
    }
    // Owner authenticated this immutable proof and current exact native denial.
    // Do not hydrate E/A8/F1 after a known denied decision.
    if !closed.permitted{return Ok(RewardReadinessV1::Denied)}
    if !matches!(closed.terminal_phase,PalwClaimPhaseV2::Final{..})||now<closed.closed_at.daa_score{
        return Err(bad("borrowed permitted carrier proof is not actual closed Final"))
    }
    if !params.claim_local_settlement_active_at(actual_point.daa_score){return Ok(RewardReadinessV1::Ready)}
    // The borrowed proof authenticates the full actual point incl subsidy. This
    // exact key lookup does not require rehydrating a released A8/F1/full world.
    let Some(e)=state.prefix_export_reservation_v1(&actual_point,source_id)?else{return Ok(RewardReadinessV1::Ready)};
    if e.bond!=closed.original.bond{return Err(bad("borrowed compact proof changes exact export producer"))}
    Ok(if e.convicted_daa.is_some(){RewardReadinessV1::Denied}
        else if now<=e.material_until{RewardReadinessV1::Hold(e.material_until)}else{RewardReadinessV1::Ready})
}
pub(super) fn readiness(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,now:u64,id:Hash64,born:&PalwClaimStateV2,
    target:Hash64,target_birth:&PalwClaimStateV2,retiring:bool,
)->Result<RewardReadinessV1,PalwStateV2Error>{
    let Some(origin)=SourceOriginV8::for_claim(state,params,id,born)?else{return Ok(RewardReadinessV1::Hold(u64::MAX))};
    let mut pending=BTreeMap::new();enqueue(&mut pending,origin)?;
    let mut result=RewardReadinessV1::Ready;
    while let Some(cursor)=pending.values_mut().find_map(|(origin,_)|origin.take()){
        let cursor_id=cursor.claim_id();let cursor_birth=cursor.original_claim();
        result=result.join(source(state,params,now,cursor_id,cursor_birth,target,target_birth,retiring)?);
        if result==RewardReadinessV1::Denied{return Ok(result)}
        result=result.join(prefix_rewards::readiness(state,params,now,cursor_id,cursor_birth,target,target_birth,retiring)?);
        if result==RewardReadinessV1::Denied{return Ok(result)}
        // Actual consumer birth is authenticated by the opaque origin, not a
        // manufactured Claim.accepted_block/subsidy=0 point.
        let mut saw_denied=false;
        let mut visit=|fact:&lineage::CarrierFactV9,view:lineage::CarrierSourceViewV9|->Result<(),PalwStateV2Error>{
            // Item5 reader already binds fact and view; retain explicit consumer
            // scope check and strict causal clocks at this semantic boundary.
            let point=view.accepting_point();
            if fact.consumer()!=cursor_id||fact.consumer_point()!=cursor.accepting_point()
                ||fact.source()!=view.source()||fact.source_point()!=point||fact.spent_txid()!=view.winning_txid()
                ||point.blue_score>=cursor.accepting_point().blue_score||point.daa_score>cursor.accepting_point().daa_score{
                return Err(bad("carrier opaque fact changes actual source/consumer birth"))
            }
            // Same raw ID across distinct causally prior births is meaningful;
            // do not discard it as a self edge.
            match view.outcome(){
                CarrierOutcomeV9::Denied=>{saw_denied=true;return Err(bad("internal known carrier denial: stop read-only visitor"))}
                CarrierOutcomeV9::PermittedClosed=>{
                    // Authoritative native FULL closure only. Item5 semantic
                    // owner MUST NOT mint this from A5 terminal/E.H alone. No
                    // original body or full F1 hydration is required or allowed
                    // to overturn this irreversible native terminal decision.
                }
                CarrierOutcomeV9::Unresolved=>{
                    let proof=view.closed_source_proof();
                    let compact=proof.map(|closed|borrowed_closed_chain_readiness_v9(
                        state,params,now,view.source(),point,view.winning_txid(),view.original_claim(),closed)).transpose()?;
                    if compact==Some(RewardReadinessV1::Denied){saw_denied=true;return Err(bad("internal known carrier denial: stop read-only visitor"))}
                    let original=view.original_claim().or_else(||proof.map(|closed|&closed.original));
                    let Some(original)=original else{
                        result=result.join(RewardReadinessV1::Hold(u64::MAX));return Ok(())
                    };
                    // Preserve live/scoped dependency traversal whenever real
                    // source metadata exists; compact proof is not a shortcut
                    // around a retained source's actual dependencies.
                    if let Some(next)=SourceOriginV8::at_point(state,params,view.source(),&point)?{
                        if !same_birth(next.original_claim(),original){return Err(bad("opaque carrier source original differs"))}
                        result=result.join(source(state,params,now,view.source(),original,target,target_birth,retiring)?);
                        if result==RewardReadinessV1::Denied{saw_denied=true;return Err(bad("internal known carrier denial: stop read-only visitor"))}
                        if let Some(decision)=compact{result=result.join(decision)}
                        enqueue(&mut pending,next)?;
                    }else if let Some(decision)=compact{
                        // A real compact settlement records dependency closure.
                        // Original financial/native duties may still be pending:
                        // DO NOT change the owner outcome from Unresolved.
                        result=result.join(decision);
                    }else{result=result.join(RewardReadinessV1::Hold(u64::MAX));}
                }
            }
            Ok(())
        };
        let visited=lineage::visit_consumer_facts_v9(state,cursor_id,cursor.accepting_point(),&mut visit)
            .and_then(|found|if found{Ok(true)}else{
                // REQUIRED Item5 reader: only actual retained/signed legacy
                // spend provenance may create these borrowed fact views.
                lineage::visit_legacy_consumer_facts_v9(state,params,&cursor,&mut visit)
            });
        drop(visit);
        // Native visitor propagates callback Err immediately. The private flag
        // is set only after an authenticated semantic Denied and prevents ANY
        // later source hydration from masking it. No error string is matched.
        if saw_denied{return Ok(RewardReadinessV1::Denied)}
        let found=visited?;
        if !found{
            // No scoped facts and no complete authenticated legacy inventory.
            // Never equate absence with an independent/empty input set.
            result=result.join(RewardReadinessV1::Hold(u64::MAX));
        }
        if result==RewardReadinessV1::Denied{return Ok(result)}
        if let Some(consumed)=cursor.prefix_use()?{
            let identity=consumed.intent.source;
            let point=match state.claim_local_prefix_source_point_v1(&identity){
                Ok(point)=>point,
                Err(PalwStateV2Error::NativeResource(e))=>return Err(PalwStateV2Error::NativeResource(e)),
                Err(_)=>{result=result.join(RewardReadinessV1::Hold(u64::MAX));continue}
            };
            if let Some(next)=SourceOriginV8::at_point(state,params,identity.source_claim,&point)?{
                enqueue(&mut pending,next)?;
            }else{result=result.join(RewardReadinessV1::Hold(u64::MAX));}
        }
    }
    Ok(result)
}

#[cfg(test)]
mod borrowed_proof_chain_tests_v9{
    use super::*;
    use super::super::super::{palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1 as Closed,
        palw_claim_local_prefix_reservation_v1 as reservations};
    use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1,prefix_v1::PalwPrefixExportReservationV1};
    fn h(n:u64)->Hash64{Hash64::from_u64_word(n)}
    fn point(n:u64)->PalwBlockContextV2{PalwBlockContextV2{block:h(n+1000),daa_score:n,blue_score:n,subsidy:123}}
    fn fixture()->(PalwStateParamsV2,PalwChainStateV2,Closed){
        // Semantic boundary fixture only. Real tiny native job supplies original
        // identity, but this does NOT establish paid source admission/A5 mint or
        // ownership authority. Runtime integration must use actual owner views.
        let f=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2,|_|{},None);
        let j=f.frame.original_v1();let p=point(10);
        let original=PalwClaimStateV2{source:PalwClaimSourceV2::FreePrompt{quanta:1,spent:Default::default()},
            class_id:j.job.class_id,bond:PalwBondKeyV2(j.job.executor_bond),pwu:1,accepted_daa:p.daa_score,
            rebound_daa:None,accepted_blue_score:p.blue_score,accepted_block:p.block,
            trace_root:j.trace_root,output_root:j.output_root,execution_root:j.execution_root,
            trace_chunk_count:j.trace_chunk_count,trace_retention_daa:j.trace_retention_daa,
            reserved:1,immature_contribution:0,escrowed_reward:0,work_leaves:j.work_leaves,work_id:None,
            phase:PalwClaimPhaseV2::Provisional,rights_reserved:0,job_identity:h(20_000),rcore:PalwClaimRcoreV1::default()};
        let params=PalwStateParamsV2::new(100,10,10,20,500,1000,h(2),4,1000,100,1000,0).unwrap().with_claim_local_settlement_from_daa(Some(0));
        let mut state=PalwChainStateV2::genesis();state.last_point=Some(point(20));
        state.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(11),h(12)),None,
            misaka_palw_kernel::rows::LedgerScalarsV1{daa:20,burned:0}));
        let closed=Closed{version:1,ruleset:crate::Hash::from_bytes([11;32]),claim:crate::palw_freeprompt_v3::fp_claim_id_v3(j),birth:p,
            original,terminal_phase:PalwClaimPhaseV2::Final{final_daa:11},closed_at:point(12),
            material_until:j.trace_retention_daa,winner:Some(h(777)),approval:vec![],permitted:true};
        (params,state,closed)
    }
    #[test]
    fn rfc40_borrowed_carrier_compact_chain_proof_uses_actual_point_e_without_a8_f1_or_full_closure(){
        use crate::palw_bond_budget_v1::*;
        let (params,mut state,closed)=fixture();let winner=closed.winner.unwrap();
        let read=|state:&PalwChainStateV2,now|borrowed_closed_chain_readiness_v9(state,&params,now,closed.claim,closed.birth,winner,Some(&closed.original),&closed);
        assert_ne!(closed.birth.subsidy,0,"the exact authenticated point retains its actual subsidy");
        assert_eq!(read(&state,20).unwrap(),RewardReadinessV1::Ready,"authentic compact chain settlement need not retain A8/F1 forever");
        let budget_id=reservations::reservation_id(&closed.birth,closed.claim);
        let policy=PalwBondBudgetPolicyV1::unapproved_probe_v1();let mut budget=PalwBondBudgetStateV1::new(0,&policy);
        budget.reserve_liable(&policy,1_000_000,budget_id,closed.original.bond,None,10,
            PalwBudgetVectorV1{claims:1,..PalwBudgetVectorV1::ZERO},1,PalwBudgetOriginV1::FreePrompt,true,&mut Vec::new()).unwrap();
        state.bond_budget=Some(budget);
        let horizon=closed.material_until;assert!(horizon>30);
        let mut e=PalwPrefixExportReservationV1{source_claim:closed.claim,point:closed.birth,bond:closed.original.bond,budget_id,
            liability_until:horizon-1,material_until:horizon,liability_sompi:1,convicted_daa:None};
        let key=(PALW_CLAIM_LOCAL_TABLE_V1,reservations::row_key(budget_id).to_vec());
        state.kernel_route.as_mut().unwrap().aux.insert(key.clone(),borsh::to_vec(&e).unwrap());
        assert_eq!(read(&state,horizon).unwrap(),RewardReadinessV1::Hold(horizon),"inclusive exact E H still holds");
        assert_eq!(read(&state,horizon+1).unwrap(),RewardReadinessV1::Ready,"other native money obligations do not manufacture permanent chain Hold");
        e.convicted_daa=Some(horizon-1);state.kernel_route.as_mut().unwrap().aux.insert(key, borsh::to_vec(&e).unwrap());
        assert_eq!(read(&state,horizon+1).unwrap(),RewardReadinessV1::Denied,"actual old E conviction defeats compact permitted chain source");
        // This reader does not mint/write CarrierOutcome::PermittedClosed.
    }
    #[test]
    fn rfc40_borrowed_carrier_compact_denial_precedes_missing_e_and_rejects_scope_substitution(){
        let (params,mut state,mut closed)=fixture();let winner=closed.winner.unwrap();
        let budget=reservations::reservation_id(&closed.birth,closed.claim);
        state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,reservations::row_key(budget).to_vec()),vec![0]);
        closed.permitted=false;
        assert_eq!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,winner,None,&closed).unwrap(),RewardReadinessV1::Denied,
            "known authentic denial must not decode malformed/new E or hydrate absent source");
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,point(11),winner,None,&closed).is_err());
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,h(778),None,&closed).is_err());
        let mut wrong_namespace=closed.clone();wrong_namespace.original.source=PalwClaimSourceV2::Attempt;
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,winner,None,&wrong_namespace).is_err(),"native carrier source namespace is FP only");
        let mut wrong_birth=closed.clone();wrong_birth.original.accepted_block=h(999_999);
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,winner,None,&wrong_birth).is_err(),"FP source cannot carry unrelated RED provenance");
        let mut original=closed.original.clone();original.job_identity=h(888);
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,winner,Some(&original),&closed).is_err());
        closed.permitted=true;
        assert!(borrowed_closed_chain_readiness_v9(&state,&params,20,closed.claim,closed.birth,winner,None,&closed).is_err(),"permitted still propagates malformed E");
    }
}
