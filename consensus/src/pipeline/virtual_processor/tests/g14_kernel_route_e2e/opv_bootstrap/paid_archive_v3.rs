//! Genuine SDK/ML-DSA carrier → actual paid native source → scoped archive.
//! This uses the existing private paid onboarding mechanics policies without
//! weakening any public activation validator. A bootstrap selected approval is
//! not claimed to earn EXEC credit or to be an independent randomness source.
use super::*;

fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (c, _, premine, floats) = paid_boot_config();
    let mut params = c.params.clone();
    params.palw_claim_local_settlement_v1 = Some(ForkActivation::new(1));
    params.sync_palw_claim_local_settlement_v1();
    assert!(params.validate_palw_v2().is_err(), "existing private mechanics world is not public activation evidence");
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("private paid world has V2 native rules")
    };
    let bundle = bundle.clone();
    (Config::new(params), bundle, premine, floats)
}
fn archived(net: &Net, claim: Digest) -> Option<bool> {
    net.chain
        .tip_state()
        .1
        .claim_local_paid_archived_selected_approval_v2(Hash64::from_bytes(claim))
        .expect("actual source archive resolves")
}

#[tokio::test]
async fn rfc23s2_paid_archive_genuine_selected_receipts_same_db_restart_and_reorg() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(config(), |c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    net.beat_to(1).await;
    let b = onboard_all(&mut net, vec![Spec::honest(0x23A3, BOOT, true)]).await.pop().unwrap();
    commit(&mut net, &b, 0x22).await;
    let complete = complete_post(&net, &b);
    let object = net.evidence(BOOT, b.v2, ConformanceEvidenceActionV1::PostComplete(Box::new(complete)));
    net.send(vec![(BOOT, object)]).await;
    assert!(register_opv(&mut net, BOOT, &b).await);

    // Every registration, prospective job, covenant, seal, reveal, lock and
    // drawn-seat receipt is carried by the ordinary signed native route.
    let jobs = post_jobs_from_payers(&mut net, &PRODUCERS[..1], b.opv, 0x23).await;
    let paid_class = net.paid_class_over(BOOT, b.opv).await;
    let paid_jobs = net.enroll_paid_jobs(paid_class, &[(PRODUCERS[0], jobs[0].id())]).await;
    let sealed = net.seal_paid(&b.kfx(), b.opv, &jobs, &paid_jobs, &PRODUCERS[2..3]).await;
    let paid = net.reveal_paid(paid_class, &sealed).await;
    let claim = paid[0].0;
    assert_eq!(archived(&net, claim), Some(false), "actual admission is not selected approval");
    let fork = net.chain.sink();
    let before_receipts = chain_blocks(&net.chain, fork);
    net.lock_and_receipt(BOOT, &paid).await;
    let until = net.api().unwrap().paid_claim_authority_for_base_unchecked_v2(&claim).unwrap().enrollment.lifetime.liability_until;
    let mut final_seen = false;
    for _ in 0..3_000 {
        if kaspa_consensus_core::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&net.ledger().claims[&claim], net.daa())
            == kaspa_consensus_core::palw_exec_v2_verify::PalwSliceVerificationV1::ProvenFalse
        {
            panic!("honest source convicted")
        }
        if archived(&net, claim) == Some(true) {
            final_seen = true;
            break;
        }
        assert!(net.daa() <= until, "assigned quorum's real source must Final inside its signed lifetime");
        net.chain.heartbeat(net.ttpb(), vec![]).await;
    }
    assert!(final_seen, "only the actual native paid Final creates selected source approval");
    let (sink, root, route) = (net.chain.sink(), net.chain.tip_state().1.state_root(), net.api());
    let mut net = net.restart(db.clone());
    assert_eq!(net.chain.sink(), sink);
    assert_eq!(net.chain.tip_state().1.state_root(), root);
    assert_eq!(net.api(), route, "immutable bodies/index roots/archive inventories survive exact DB restore");
    assert_eq!(archived(&net, claim), Some(true));

    // The competing real branch forked before lock/receipts. Its clock alone
    // must not turn the absent selected signatures into an approval. A heavier
    // branch reorgs the real node, undoing all archive writes with their deltas.
    let mut chain = t12_genesis_chain(&net.config, &net.bundle, &net.premine, &net.floats);
    for block in &before_receipts {
        arrive(&chain, block.clone(), "source pre-receipt fork").await;
    }
    // Arrival validates history; it does not advance the harness producer's
    // clock/nonce. Start a genuinely independent fork at its actual timestamp.
    chain.ctx.simulated_time = before_receipts.last().expect("actual non-genesis fork").header.timestamp;
    chain.set_nonce_for_fork(1_000_000);
    let mut branch = net.on_chain(chain);
    let original_daa = net.daa();
    branch.beat_to(original_daa.saturating_add(16)).await;
    assert_eq!(archived(&branch, claim), Some(false), "timeout/local replay do not create a selected approval");

    // DAA/heartbeat blue work alone is NOT an economic win. The unchanged
    // strict reorg fence refuses a deep all-economic tie, even at higher DAA.
    // Earn a real independent native Attempt, and read the actual processor
    // order before asking this node to select the competing branch.
    let order = |net: &Net| net.chain.vp().palw_candidate_order_v2_checked(net.chain.sink())
        .expect("actual candidate order resource admission").expect("actual retained candidate state");
    let economic = |o: &kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1|
        (o.safe_frontier_blue_score,o.safe_weight,o.live_total);
    let incumbent = order(&net);
    for round in 0..8usize {
        if economic(&order(&branch)) > economic(&incumbent) { break; }
        let ttpb=branch.ttpb();
        let (_,native_claim)=branch.chain.attempt(5+round%3,ttpb,Vec::new(),&|_|true).await;
        assert!(branch.chain.tip_state().1.claim(&native_claim).is_some(),"ordinary native Attempt actually admitted");
    }
    let challenger=order(&branch);
    assert!(economic(&challenger)>economic(&incumbent),
        "receipt-free fork must earn a strict actual economic win, not assume higher DAA: incumbent {incumbent:?}, challenger {challenger:?}, fork DAA {}, current DAA {}, finality depth {}",
        net.chain.daa_of(fork),net.daa(),net.config.params.finality_depth());
    assert_eq!(archived(&branch,claim),Some(false),"independent native work does not forge this paid source's selected signatures");
    for block in chain_blocks(&branch.chain, branch.chain.sink()) {
        arrive(&net.chain, block, "source branch removes assigned receipts").await;
    }
    assert_eq!(net.chain.sink(), branch.chain.sink(), "the actual node selected the heavier receipt-free branch");
    assert_eq!(
        net.chain.tip_state().1.state_root(),
        branch.chain.tip_state().1.state_root(),
        "archive/body/index deltas invert and replay exactly"
    );
    assert_eq!(archived(&net, claim), Some(false));
}
