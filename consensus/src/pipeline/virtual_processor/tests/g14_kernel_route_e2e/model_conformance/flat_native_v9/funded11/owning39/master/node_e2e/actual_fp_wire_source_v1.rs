//! DRAFT cfg(test) child of the existing native39 node_e2e fixture.
//! Actual normal admissions, authenticated core observer, independent native
//! reorg and ordinary TCP import. Requires two genuine producer captures and
//! the separately reviewed core/wire registration patch; not runtime proven.
#![allow(dead_code)]
use super::*;
use super::family_legacy_campaign::{self as family, FamilyCaptureV2, AdmittedFamilyOriginalV2};
use std::{path::Path};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2,PalwChainStateV2,PalwStateParamsV2};

/// Expected observations from actual accepted native state. Not import authority,
/// a paid token, signed protocol data or an alternate state/claim constructor.
#[derive(borsh::BorshSerialize,borsh::BorshDeserialize)]
pub(crate) struct FpWireExpectedOriginalV1 {
    pub claim:Hash64,
    pub outer:Hash64,
    pub birth:PalwBlockContextV2,
}
#[derive(borsh::BorshSerialize,borsh::BorshDeserialize)]
pub(crate) struct FpWireSourceDescriptorV1 {
    pub version:u16,
    pub source_db:String,
    pub genesis:Hash64,
    pub params_id:String,
    pub source:FpWireExpectedOriginalV1,
    pub consumer:FpWireExpectedOriginalV1,
    pub pruning_point:Hash64,
    pub pruning_root:Hash64,
    pub tip:Hash64,
    pub tip_root:Hash64,
    pub simulated_time:u64,
    pub nonce:u64,
}
fn expected(a:&AdmittedFamilyOriginalV2,state:&PalwChainStateV2)->FpWireExpectedOriginalV1 {
    let actual=kaspa_consensus_core::palw_state_v2::palw_actual_fp_accepting_point_from_live_stored_v1(state,a.claim,
        a.stored_origin).unwrap();
    assert_eq!(a.actual_origin,Some(*actual),"independent live lookup retains the admitted full point");
    FpWireExpectedOriginalV1{claim:a.claim,outer:a.carrier.id(),birth:*actual}
}

fn observe(state:&PalwChainStateV2,params:&PalwStateParamsV2,a:&FpWireExpectedOriginalV1,c:&FpWireExpectedOriginalV1){
    let actual=kaspa_consensus_core::palw_state_v2::palw_actual_fp_carrier_observation_v1(state,params,
        (a.claim,a.birth,a.outer),(c.claim,c.birth,c.outer)).unwrap();
    assert_eq!(actual.exact_edges(),1);
    assert!(actual.owner9_rows().iter().all(|n|*n>0));
}
/// Independent ordinary branch from the actual accepted pair, not a forced
/// sink. If real native work cannot beat incumbent, the fixture fails honestly.
async fn reorg_after_pair(net:&mut Net){
    let fork=net.chain.sink();let fork_clock=net.chain.ctx.simulated_time;
    let ttpb=net.ttpb();net.chain.heartbeat(ttpb,Vec::new()).await;
    let incumbent_tip=net.chain.sink();
    let incumbent=net.chain.vp().palw_candidate_order_v2_checked(incumbent_tip).unwrap().unwrap();
    let mut b=t12_genesis_chain(&net.config,&net.bundle,&net.premine,&net.floats);
    b.ctx.consensus.set_validation_time_for_test(fork_clock);
    for blk in past_in_order(&net.chain,fork){arrive(&b,blk,"FP source fork replay").await;}
    assert_eq!(b.sink(),fork);b.ctx.simulated_time=fork_clock;b.set_nonce_for_fork(1_000_001);
    let economic=|o:&kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1|(o.safe_frontier_blue_score,o.safe_weight,o.live_total);
    for n in 0..8usize{
        let current=b.vp().palw_candidate_order_v2_checked(b.sink()).unwrap().unwrap();
        if economic(&current)>economic(&incumbent){break}
        let(_,claim)=b.attempt(5+n%3,ttpb,Vec::new(),&|_|true).await;
        assert!(b.tip_state().1.claim(&claim).is_some());
    }
    let challenger=b.vp().palw_candidate_order_v2_checked(b.sink()).unwrap().unwrap();
    assert!(economic(&challenger)>economic(&incumbent),"actual FP source reorg must earn native superiority: incumbent {incumbent:?} challenger {challenger:?}");
    net.chain.ctx.consensus.set_validation_time_for_test(b.ctx.simulated_time);
    for blk in past_in_order(&b,b.sink()){
        arrive(&net.chain,blk,"FP source independent signed native branch").await;
    }
    assert_eq!(net.chain.sink(),b.sink());
    assert!(!net.chain.vp().reachability_service.try_is_chain_ancestor_of(incumbent_tip,net.chain.sink()).unwrap(),"actual selected branch change, not extension");
    net.chain.ctx.simulated_time=b.ctx.simulated_time;net.chain.set_nonce_for_fork(b.nonce_for_reopen());
}

/// Actual source workload with separate normal admission/observer/wire adapters. Capture executor/domain and all normal admission gates remain
/// authoritative. No claim, row, delta, amount, approval or world is fabricated.
pub(crate) async fn prepare_and_import_actual_fp_v1(
    first_capture:&Path,second_capture:&Path,out:&Path,
) {
    use kaspa_database::prelude::ConnBuilder;
    assert!(out.is_absolute()&&!out.exists(),"fresh isolated retained output required");
    std::fs::create_dir(out).unwrap();
    let parts=crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
    assert!(parts.1.state.claim_local_settlement_active_at(0));
    let db_path=out.join("source-db");
    let db=ConnBuilder::default().with_files_limit(20).with_db_path(db_path.clone()).build().unwrap();
    let (tx,rx)=async_channel::unbounded();
    let mut net=Net::over_cfg(parts,|cfg|TestConsensus::with_db(db.clone(),cfg,tx));
    net._keep.push(Box::new(rx));
    net.chain.ctx.consensus.set_validation_time_for_test(net.chain.ctx.simulated_time);
    let first=FamilyCaptureV2::read(first_capture,&net);
    let second=FamilyCaptureV2::read(second_capture,&net);
    assert_eq!(first.card,second.card,"both real captures must name same funded native key; do not rewrite executor");
    assert_eq!(first.semantics,second.semantics,"same immutable registered class");
    assert_ne!(first.object,second.object,"distinct actual FP executions are required");
    assert_ne!(first.prompt,second.prompt,"avoid selling identical already-paid work");
    family::register_family_capture_class_v2(&mut net,&first).await;
    let source=family::admit_family_original_v2(&mut net,&first,None).await;
    let source_change=TransactionOutpoint::new(source.carrier.id(),0);
    assert_eq!(net.funding[first.card].0,source_change);
    let consumer=family::admit_family_original_v2(&mut net,&second,None).await;
    assert_ne!(source.claim,consumer.claim);
    assert!(consumer.carrier.inputs.iter().any(|i|i.previous_outpoint==source_change),
        "second actual signed FP directly spends first literal winner output");
    let state=net.chain.tip_state().1;
    let a=expected(&source,&state);let c=expected(&consumer,&state);
    family::assert_family_winning_carrier_v2(&state,&source);
    family::assert_family_winning_carrier_v2(&state,&consumer);
    observe(&state,&net.bundle.state,&a,&c);
    drop(state);
    reorg_after_pair(&mut net).await;
    let state=net.chain.tip_state().1;
    family::assert_family_winning_carrier_v2(&state,&source);
    family::assert_family_winning_carrier_v2(&state,&consumer);
    observe(&state,&net.bundle.state,&a,&c);
    drop(state);
    net=net.restart(db.clone());
    let state=net.chain.tip_state().1;
    observe(&state,&net.bundle.state,&a,&c);
    drop(state);
    // Keep the actual native pruning lattice. If signed retention/native limits
    // cannot carry these components to PP, fail; never install sentinel rows.
    let max_beats=net.config.params.pruning_depth().checked_mul(2).and_then(|n|n.checked_add(10_000)).unwrap();
    let mut reached=false;
    for _ in 0..max_beats {
        let pp=net.chain.ctx.consensus.pruning_point();
        if pp!=net.config.params.genesis.hash
            &&net.chain.vp().reachability_service.try_is_chain_ancestor_of(consumer.folding.header.hash,pp).unwrap() {
            // A covering PP may equal the folding block; production keeps the
            // PP/anticone bodies, and physical pruning is a separate process.
            // Observe actual deletion and the durable snapshot identity before
            // publishing the body-pruned wire source. Keep all native depths,
            // signed horizons and the existing finite max_beats unchanged.
            let vp=net.chain.vp();
            let body_absent=!vp.block_transactions_store.has(c.birth.block).unwrap();
            let store=vp.palw_state_v2_store.read();
            let delta_absent=!store.has_delta(c.birth.block).unwrap();
            let snapshot_ready=store.pruning_snapshot_key().unwrap().is_some_and(|(block,_)|block==pp);
            drop(store);
            if body_absent&&delta_absent&&snapshot_ready{reached=true;break;}
        }
        let ttpb=net.ttpb();net.chain.heartbeat(ttpb,Vec::new()).await;
    }
    assert!(reached,"actual PP failed to cover accepted FP history under unmodified native depth");
    let pp=net.chain.ctx.consensus.pruning_point();
    let (snapshot_at,snapshot)=net.chain.vp().palw_state_v2_store.read()
        .load_pruning_snapshot_retained_v1(&net.bundle.state).unwrap().expect("real native pruning snapshot");
    assert_eq!(snapshot_at,pp);
    family::assert_family_winning_carrier_v2(&snapshot,&source);
    family::assert_family_winning_carrier_v2(&snapshot,&consumer);
    observe(&snapshot,&net.bundle.state,&a,&c);
    let pruning_root=snapshot.state_root();drop(snapshot);
    let descriptor=FpWireSourceDescriptorV1{version:1,source_db:db_path.to_str().expect("UTF8 fixture path").to_owned(),genesis:net.config.params.genesis.hash,
        params_id:net.config.params.consensus_params_id().to_string(),source:a,consumer:c,pruning_point:pp,pruning_root,
        tip:net.chain.sink(),tip_root:net.chain.tip_state().1.state_root(),simulated_time:net.chain.ctx.simulated_time,
        nonce:net.chain.nonce_for_reopen()};
    // No source descriptors become protocol wire or native accepted authority.
    std::fs::write(out.join("actual-fp-source-observations.borsh"),borsh::to_vec(&descriptor).unwrap()).unwrap();
    drop(net);drop(db);
    crate::pipeline::virtual_processor::tests::g14_kernel_route_e2e::material_typed_ingress_v4::source_manifest::pruned_import::wire::actual_fp_v1::import_actual_fp_source_v1(&descriptor,&out.join("wire-run")).await;
}

#[cfg(feature="evm")]
#[tokio::test]
#[ignore="requires two genuine same-card FP producer captures for exact family-native genesis; expensive actual pruning/P2P"]
async fn rfc37_actual_signed_fp_carrier_owner9_v10_d3_tcp_full_import_and_cold_reopen(){
    let required=|key|std::path::PathBuf::from(std::env::var_os(key).expect("explicit actual capture/output environment is required"));
    prepare_and_import_actual_fp_v1(&required("MISAKA_FP_WIRE_SOURCE_CAPTURE_DIR_V1"),&required("MISAKA_FP_WIRE_CONSUMER_CAPTURE_DIR_V1"),&required("MISAKA_FP_WIRE_RUN_V1")).await;
}
