//! Collector regression candidate: real signed prefix use/export, authentic
//! scoped source writer/reader and actual collect_closed_exports_v1. FP birth
//! journal and S1 are historical fixture seams, NOT normal paid node admission.
//! Cold/reorg checks below are serialized-kernel-metadata/snapshot seams only.
use super::*;
use crate::palw_claim_local_v1::*;
use crate::palw_claim_local_v1::prefix_v1::*;
use crate::palw_freeprompt_v3::{PalwFreePromptCommitmentV3, PalwFpPrefixStateV1};
use libcrux_ml_dsa::ml_dsa_87 as dsa;
use crate::palw_state_v2::palw_claim_local_prefix_reservation_v1 as reservations;
use super::super::palw_claim_local_compact_sources_v1 as compact;
fn h(n: u64) -> Hash64 { Hash64::from_u64_word(n) }
fn point(n: u64) -> PalwBlockContextV2 { PalwBlockContextV2 { block: h(10_000+n), daa_score: n, blue_score: n, subsidy: 0 } }
fn claim(job: &PalwFreePromptCommitmentV3, p: PalwBlockContextV2) -> PalwClaimStateV2 {
    PalwClaimStateV2 {
        source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
        class_id: job.job.class_id, bond: PalwBondKeyV2(job.job.executor_bond), pwu: 1,
        accepted_daa: p.daa_score, rebound_daa: None, accepted_blue_score: p.blue_score, accepted_block: p.block,
        trace_root: job.trace_root, output_root: job.output_root, execution_root: job.execution_root,
        trace_chunk_count: job.trace_chunk_count, trace_retention_daa: job.trace_retention_daa,
        reserved: 1, immature_contribution: 0, escrowed_reward: 0, work_leaves: job.work_leaves,
        work_id: None, phase: PalwClaimPhaseV2::Provisional, rights_reserved: 0, job_identity: h(20_000),
        rcore: PalwClaimRcoreV1::default(),
    }
}
fn object(id: Hash64, c: &PalwClaimStateV2, job: &PalwFreePromptCommitmentV3, prefix: PalwFpPrefixStateV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted { claim: id, class_id: c.class_id, bond: c.bond,
        executor_pubkey: job.job.executor_pubkey.clone(), work_leaves: c.work_leaves,
        prompt_token_ids_hash: job.job.prompt_token_ids_hash, prompt_tokens: job.job.prompt_tokens,
        prompt_token_ids: vec![], decode_tokens_executed: job.decode_tokens_executed,
        trace_root: c.trace_root, output_root: c.output_root, execution_root: c.execution_root,
        trace_chunk_count: c.trace_chunk_count, trace_retention_daa: c.trace_retention_daa,
        consumed_prefix_state: prefix, job_pin: c.job_identity, public_job: None, eval: None }
}
fn install_birth(state: &mut PalwChainStateV2, params: &PalwStateParamsV2,
    job: &PalwFreePromptCommitmentV3, c: &PalwClaimStateV2, edge: PalwPrefixDependencyEnvelopeV1,
) -> Hash64 {
    let id = crate::palw_freeprompt_v3::fp_claim_id_v3(job);
    let p = PalwBlockContextV2 { block: c.accepted_block, daa_score: c.accepted_daa, blue_score: c.accepted_blue_score, subsidy: 0 };
    let prior = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    let mut current = prior.clone(); current.last_point = Some(p); current.claims.insert(id,c.clone());
    let descriptor = edge.consumed.as_ref().map_or(PalwFpPrefixStateV1::genesis(c.class_id), |s|s.intent.prefix);
    let frame = PalwClaimLiabilityFrameV1 { point:p, network_identity:[11;32], fired_forks:[12;32],
        policy:PalwClaimLocalPolicyContextV1::from_rules_v1(params,p.daa_score), claims:[id].into_iter().collect(),
        native_changes:vec![PalwClaimLocalNativeChangeV1 { owner:PalwClaimLocalRowOwnerV1::ChainClaims, table:0,
            key:id.as_byte_slice().to_vec(), old:None, new:Some(borsh::to_vec(c).unwrap()) }],
        prior_state:borsh::to_vec(&prior).unwrap(), current_state:borsh::to_vec(&current).unwrap(),
        accepted_objects:vec![object(id,c,job,descriptor)], origin_headers:Default::default(),
        inputs:{
            let mut inputs=vec![PalwClaimLocalInputV1::FreePromptNative { claim:id,job_pin:c.job_identity,public_job:Some(Box::new(job.clone())),eval:None }];
            // Independent B has no prefix input, rather than an invalid empty
            // provenance envelope. prefix_use authenticates any present wrapper.
            if !edge.exports.is_empty() || edge.consumed.is_some() {
                inputs.push(PalwClaimLocalInputV1::PrefixDependency { claim:id,envelope:Box::new(edge) });
            }
            inputs
        } };
    frame.validate_context_v1().unwrap();
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,
        palw_claim_liability_frame_key_v1(p.block).to_vec()),borsh::to_vec(&frame).unwrap()); id
}

fn archive_birth(state:&mut PalwChainStateV2,params:&PalwStateParamsV2,id:Hash64,p:PalwBlockContextV2) {
    let original=state.claim_local_admission_context_v1(params,&p,id).unwrap().unwrap().original_claim().clone();
    let frame=crate::palw_claim_local_v1::palw_decode_claim_liability_frame_v1(
        &state.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,palw_claim_liability_frame_key_v1(p.block).to_vec())]).unwrap();
    let mut before=state.clone();before.claims.remove(&id);
    let extras=PalwTransitionExtrasV1 {claim_local_rule_context_v1:Some(([11;32],[12;32])),..Default::default()};
    let mut builder=TransitionBuilder::new(&before,params,false,false,false,false,&extras);
    let span=super::super::palw_claim_local_object_spans_v2::prepare(&mut builder,&p,&frame.accepted_objects[0]).unwrap().unwrap();
    builder.write_claim(id,Some(original)).unwrap();
    super::super::palw_claim_local_object_spans_v2::complete(&mut builder,span);
    super::super::palw_claim_local_source_admission_v8::capture_admissions_v8(
        &mut builder,&before,&p,&frame.accepted_objects,&frame.inputs).unwrap();
    *state=builder.state;
}
fn exact_edges(state:&PalwChainStateV2,params:&PalwStateParamsV2)->Vec<(PrefixBirthV1,PrefixBirthV1)> {
    let mut edges=Vec::new();
    super::super::palw_claim_local_source_origin_v8::visit_retained_prefix_origins_v8(state,params,|origin|{
        if let Some(used)=origin.prefix_use()? {
            let s=used.intent.source;
            edges.push((birth_v1(origin.claim_id(),&origin.accepting_point()),(s.source_claim,s.accepted_block,s.accepted_daa,s.accepted_blue_score)));
        }
        Ok(())
    }).unwrap();edges
}
#[test]
fn rfc40_scoped_retired_prefix_edge_survives_f1_release_and_exact_birth_reuse() {
    let params=PalwStateParamsV2::new(100,10,10,20,500,1000,h(2),4,1000,100,1000,0).unwrap()
        .with_claim_local_settlement_from_daa(Some(0));
    let keys=dsa::generate_key_pair([91;32]); let pk=keys.verification_key.as_ref().to_vec();
    let native=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2,|j|j.executor_pubkey=pk.clone(),None);
    let source_job=native.frame.original_v1().clone(); let a=crate::palw_freeprompt_v3::fp_claim_id_v3(&source_job);
    let a_born=claim(&source_job,point(10)); let horizon=source_job.trace_retention_daa; assert!(horizon>30);
    let mut state=PalwChainStateV2::genesis();
    state.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(11),h(12)),None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa:30,burned:0 }));
    state.last_point=Some(point(30));
    let export=PalwPrefixExportIntentV1 { version:1,network:source_job.job.network_domain,source_namespace:1,
        source_nonce:h(91),base_commitment:a,producer_key_id:crate::mldsa87_primitives::mldsa87_key_id(&pk),ordinal:0,
        prefix:PalwFpPrefixStateV1 {class_id:a_born.class_id,state_root:h(92),prefix_tokens:2},
        liability_until:horizon-1,material_until:horizon };
    let signed=PalwSignedPrefixExportV1 { signature:dsa::sign(&keys.signing_key,export.id_v1().as_byte_slice(),
        PREFIX_EXPORT_CONTEXT_V1,[92;32]).unwrap().as_ref().to_vec(),intent:export.clone() };
    install_birth(&mut state,&params,&source_job,&a_born,PalwPrefixDependencyEnvelopeV1 {inner:vec![1],exports:vec![signed],consumed:None});
    let identity=state.claim_local_admission_context_v1(&params,&point(10),a).unwrap().unwrap()
        .prefix_export_v1(0).unwrap().unwrap().1;
    let consumer_native=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2,
        |j| {j.executor_pubkey=pk.clone();j.job_nonce=[93;32];},None);
    let consumer_job=consumer_native.frame.original_v1().clone();
    let c=crate::palw_freeprompt_v3::fp_claim_id_v3(&consumer_job);let c_born=claim(&consumer_job,point(20));
    let usage=PalwPrefixUseIntentV1 {version:1,network:export.network,consumer_nonce:h(93),base_commitment:c,
        consumer_key_id:export.producer_key_id,source:identity,prefix:export.prefix,
        hard_deadline:horizon-1,material_until:horizon};
    let consumed=PalwSignedPrefixUseV1 {signature:dsa::sign(&keys.signing_key,usage.id_v1().as_byte_slice(),
        PREFIX_USE_CONTEXT_V1,[94;32]).unwrap().as_ref().to_vec(),intent:usage};
    install_birth(&mut state,&params,&consumer_job,&c_born,PalwPrefixDependencyEnvelopeV1 {inner:vec![1],exports:vec![],consumed:Some(consumed)});
    let independent_native=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2,
        |j| {j.executor_pubkey=pk.clone();j.job_nonce=[95;32];},None);
    let independent_job=independent_native.frame.original_v1().clone();let b_born=claim(&independent_job,point(21));
    assert_ne!(crate::palw_freeprompt_v3::fp_claim_id_v3(&source_job),crate::palw_freeprompt_v3::fp_claim_id_v3(&independent_job),"B has its own authenticated job identity");
    let b=install_birth(&mut state,&params,&independent_job,&b_born,PalwPrefixDependencyEnvelopeV1 {inner:vec![1],exports:vec![],consumed:None});
    // Actual typed reservation reader must find a real native budget claim row;
    // no unchecked E-only row or later raw claim's liability substitutes for it.
    use crate::palw_bond_budget_v1::*;
    let budget_id=reservations::reservation_id(&point(10),a);
    let policy=PalwBondBudgetPolicyV1::unapproved_probe_v1();let mut budget=PalwBondBudgetStateV1::new(0,&policy);
    budget.reserve_liable(&policy,1_000_000,budget_id,a_born.bond,None,10,
        PalwBudgetVectorV1 {claims:1,..PalwBudgetVectorV1::ZERO},1,PalwBudgetOriginV1::FreePrompt,true,&mut Vec::new()).unwrap();
    state.bond_budget=Some(budget);
    let mut reservation=PalwPrefixExportReservationV1 {source_claim:a,point:point(10),bond:a_born.bond,budget_id,
        liability_until:horizon-1,material_until:horizon,liability_sompi:1,convicted_daa:None};
    let ekey=reservations::row_key(budget_id).to_vec();
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,ekey.clone()),borsh::to_vec(&reservation).unwrap());
    // Terminal/permitted S1 is a retained closed-source seam. It intentionally
    // causes source() to skip live_source while a different birth occupies raw A.
    let closed=compact::PalwClosedClaimSourceV1 {version:1,ruleset:crate::Hash::from_bytes([11;32]),claim:a,birth:point(10),original:a_born.clone(),
        terminal_phase:PalwClaimPhaseV2::Final {final_daa:11},closed_at:point(12),material_until:horizon,
        winner:None,approval:vec![],permitted:true};
    let mut skey=vec![b'S'];skey.extend_from_slice(a.as_byte_slice());
    state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,skey),borsh::to_vec(&closed).unwrap());

    archive_birth(&mut state,&params,a,point(10));
    archive_birth(&mut state,&params,c,point(20));
    archive_birth(&mut state,&params,b,point(21));
    let expected=vec![(birth_v1(c,&point(20)),birth_v1(a,&point(10)))];
    assert_eq!(exact_edges(&state,&params),expected,"scoped + F1 copies are deduplicated");
    let checkpoint=state.clone();
    // A8 dual coverage does not authorize an inconsistent retained F body.
    let mut malformed=state.clone();
    let frame_key=palw_claim_liability_frame_key_v1(point(20).block).to_vec();
    let raw=&malformed.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,frame_key.clone())];
    let mut bad_frame=(*crate::palw_claim_local_v1::palw_decode_claim_liability_frame_v1(raw).unwrap()).clone();
    bad_frame.current_state.clear();
    malformed.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,frame_key),borsh::to_vec(&bad_frame).unwrap());
    assert!(super::super::palw_claim_local_source_origin_v8::visit_retained_prefix_origins_v8(
        &malformed,&params,|_|Ok(())).is_err(),"scoped dedup must not hide a malformed retained carriage");
    // Actual normal provenance-admission reader with genuine producer/use
    // signatures. FP historical birth and A8 capture remain fixture seams.
    let legacy_consumer=state.claim_local_admission_context_v1(&params,&point(20),c).unwrap().unwrap();
    let consumer_envelope=legacy_consumer.frame().inputs.iter().find_map(|input|match input {
        PalwClaimLocalInputV1::PrefixDependency {claim,envelope} if *claim==c=>Some((**envelope).clone()),_=>None,
    }).unwrap();
    drop(legacy_consumer);
    let mut consumer_object=object(c,&c_born,&consumer_job,export.prefix);
    if let PalwConsensusObjectV2::FreePromptCommitted {public_job,..}=&mut consumer_object {
        *public_job=Some(Box::new(consumer_job.clone()));
    }
    let policy=crate::palw_ruling_fences_v1::PalwClaimHardDeadlinePolicyV1 {version:1,
        prepare_daa:1,observe_daa:1,resolve_daa:1,cold_fetch_daa:1,check_daa:1,localize_daa:1,
        carrier_daa:1,reorg_slack_daa:1,retention_cost_per_daa_sompi:1};
    let admission_params=params.clone().with_claim_hard_deadline_v1(Some(crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1 {from_daa:0,policy}));
    let mut scoped_source=state.clone();
    scoped_source.kernel_route.as_mut().unwrap().aux.retain(|(t,k),_|*t!=PALW_CLAIM_LOCAL_TABLE_V1||k.first()!=Some(&b'F'));
    let admit=|s:&PalwChainStateV2,p:PalwBlockContextV2,e:&PalwPrefixDependencyEnvelopeV1| {
        crate::palw_claim_local_v1::prefix_v1::check_admission_v1(s,&admission_params,&p,&consumer_object,Some(e))
    };
    assert!(admit(&state,point(20),&consumer_envelope).is_ok(),"unchanged legacy/scoped dual-source fixture remains admitted");
    assert!(admit(&scoped_source,point(20),&consumer_envelope).is_ok(),"A8 actual signed source survives complete F1 removal");
    let resign_use=|mut envelope:PalwPrefixDependencyEnvelopeV1,edit:fn(&mut PalwPrefixUseIntentV1)| {
        let used=envelope.consumed.as_mut().unwrap();edit(&mut used.intent);
        used.signature=dsa::sign(&keys.signing_key,used.intent.id_v1().as_byte_slice(),PREFIX_USE_CONTEXT_V1,[71;32]).unwrap().as_ref().to_vec();envelope
    };
    let wrong_birth=resign_use(consumer_envelope.clone(),|u|u.source.accepted_block=h(123456));
    assert!(admit(&scoped_source,point(20),&wrong_birth).is_err());
    let wrong_intent=resign_use(consumer_envelope.clone(),|u|u.source.intent=h(123457));
    assert!(admit(&scoped_source,point(20),&wrong_intent).is_err());
    let foreign_network=resign_use(consumer_envelope.clone(),|u|u.network=h(123458));
    assert!(admit(&scoped_source,point(20),&foreign_network).is_err());
    assert!(admit(&scoped_source,point(9),&consumer_envelope).unwrap_err().to_string().contains("causally prior"));
    let mut convicted=scoped_source.clone();let mut denied_reservation=reservation.clone();denied_reservation.convicted_daa=Some(15);
    convicted.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,ekey.clone()),borsh::to_vec(&denied_reservation).unwrap());
    assert!(admit(&convicted,point(20),&consumer_envelope).unwrap_err().to_string().contains("finite signed"));
    let mut foreign_namespace=consumer_envelope.clone();
    let mut foreign_export=export.clone();foreign_export.base_commitment=c;foreign_export.source_namespace=2;
    let signature=dsa::sign(&keys.signing_key,foreign_export.id_v1().as_byte_slice(),PREFIX_EXPORT_CONTEXT_V1,[72;32]).unwrap().as_ref().to_vec();
    foreign_namespace.exports.push(PalwSignedPrefixExportV1 {intent:foreign_export,signature});
    assert!(admit(&scoped_source,point(20),&foreign_namespace).unwrap_err().to_string().contains("incomplete prefix"));
    let mut aliased=state.clone();
    let real_key=palw_claim_liability_frame_key_v1(point(20).block).to_vec();
    let alias_key=palw_claim_liability_frame_key_v1(h(777_000)).to_vec();
    assert_ne!(real_key,alias_key);
    let literal=aliased.kernel_route.as_ref().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,real_key)].clone();
    aliased.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,alias_key),literal);
    assert!(super::super::palw_claim_local_source_origin_v8::visit_retained_prefix_origins_v8(
        &aliased,&params,|_|Ok(())).is_err(),"malformed extra F key cannot revisit the canonical accepting cohort");
    state.claims.remove(&a);state.claims.remove(&c);
    state.kernel_route.as_mut().unwrap().aux.retain(|(t,k),_|*t!=PALW_CLAIM_LOCAL_TABLE_V1||k.first()!=Some(&b'F'));
    assert_eq!(exact_edges(&state,&params),expected,"retired consumer remains in exact scoped directory");
    let live_independent=state.clone();
    // New raw-ID occupancy cannot replace the old archived accepting cohort.
    state.claims.insert(c,PalwClaimStateV2 {accepted_block:point(40).block,accepted_daa:40,accepted_blue_score:40,..c_born.clone()});
    assert_eq!(exact_edges(&state,&params),expected);
    state.claims.remove(&c);
    let extras=PalwTransitionExtrasV1::default();
    let mut absent=live_independent.clone();
    // Remove scoped component roots from this fixture. The live
    // independent B now has neither authenticated scoped origin nor F1.
    absent.kernel_route.as_mut().unwrap().aux.retain(|(t,k),_|*t!=PALW_CLAIM_LOCAL_TABLE_V1||k.first()!=Some(&b'I'));
    let mut unknown=TransitionBuilder::new(&absent,&params,false,false,false,false,&extras);
    collect_closed_exports_v1(&mut unknown,&point(horizon+1)).unwrap();
    assert!(unknown.state.kernel_route.as_ref().unwrap().aux.contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1,ekey.clone())),
        "missing origin is not closed or independent");
    let mut corrupt=live_independent.clone();
    corrupt.kernel_route.as_mut().unwrap().aux.retain(|(t,k),_|*t!=PALW_CLAIM_LOCAL_TABLE_V1||k.first()!=Some(&b'B'));
    let mut rejected=TransitionBuilder::new(&corrupt,&params,false,false,false,false,&extras);
    let before=borsh::to_vec(&PalwStateCarriageV2::from_state(&rejected.state)).unwrap();
    assert!(collect_closed_exports_v1(&mut rejected,&point(horizon+1)).is_err(),"corrupt present source bodies are not closure");
    assert_eq!(borsh::to_vec(&PalwStateCarriageV2::from_state(&rejected.state)).unwrap(),before);
    // No live FP remains: the old F1-only gate cannot conservatively retain E
    // because of independent B. With F1 absent its old edge list is empty and
    // it would collect A's closed, expired E despite retained unknown-closure C.
    state.claims.remove(&b);
    assert!(state.claims.is_empty());
    let mut builder=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
    collect_closed_exports_v1(&mut builder,&point(horizon+1)).unwrap();
    assert!(builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1,ekey.clone())),
        "actual collector retains old E for the retained retired consumer with unknown closure");
    // Cold metadata decoder roundtrip, without claiming a full coherent IBD load.
    let route=builder.state.kernel_route.take().unwrap();
    let raw=borsh::to_vec(&route).unwrap();
    builder.state.kernel_route=Some(borsh::from_slice(&raw).unwrap());
    assert_eq!(exact_edges(&builder.state,&params),expected);
    assert_eq!(exact_edges(&checkpoint,&params),expected,"snapshot restoration retains the same exact edge");
    // Complementary finite progress: the same retained C origin now has an
    // exact closed S1 seam, all its actual signed material clocks have passed,
    // and independent live B still has its authentic scoped origin but no F1.
    let c_closed=compact::PalwClosedClaimSourceV1 {version:1,ruleset:crate::Hash::from_bytes([11;32]),
        claim:c,birth:point(20),original:c_born.clone(),terminal_phase:PalwClaimPhaseV2::Final {final_daa:21},
        closed_at:point(22),material_until:horizon,winner:None,approval:vec![],permitted:true};
    let mut c_skey=vec![b'S'];c_skey.extend_from_slice(c.as_byte_slice());
    builder.state.kernel_route.as_mut().unwrap().aux.insert((PALW_CLAIM_LOCAL_TABLE_V1,c_skey),borsh::to_vec(&c_closed).unwrap());
    builder.state.claims.insert(b,b_born.clone());
    assert_eq!(exact_edges(&builder.state,&params),expected,"retained edge is closed, not silently erased");
    collect_closed_exports_v1(&mut builder,&point(horizon+1)).unwrap();
    assert!(!builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1,ekey)),
        "exact closed expired C permits E collection despite live A8-only independent B");

}
