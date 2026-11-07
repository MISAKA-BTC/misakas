//! **Lane G14-R: the ACTUAL public filer service resumes mid-pursuit** — the parent's fresh public bond (registered after genesis,
//! matured, outside every Panel) runs the real `PalwPanelService` against an all-Panel-Valid claim whose producer withholds; the
//! service is stopped after its reservation is mined and a NEW service (same state directory: a process restart) carries on; then,
//! after its demand is mined, the node itself is replaced by an empty database that validates every public block (the parent's
//! `import_fresh`: a full re-validation, the node's mempool gone) and a third service on it carries the case to the objective DA
//! default. The book is a cache (module doc of `palw_fraud_filer`): each service rebuilds its cases from the tip and its answers from
//! the accepted blocks. What rode the chain is counted: ONE reservation and no unit demanded twice — no second deposit, no duplicate.
//!
//! Harness exceptions are the parent's (test keys and premine, skipped PoW, inert EVM, the public-filer fence armed in the fixture
//! while the shipping activation validator still refuses it).
use super::*;

/// A running node around `chain`'s consensus: its manager, its mempool and the service.
struct Node {
    manager: Arc<ConsensusManager>,
    pool: MiningManagerProxy,
    running: ServiceRun,
    _flow: Arc<FlowContext>,
}

/// Start the actual service of `bond` over `chain` (a mempool and a flow context of its own), its state in `dir`. The address
/// manager's database lives in `keep` (the test's end drops it, after every service).
fn start_service(
    chain: &Chain,
    dir: &std::path::Path,
    bond: PalwBondKeyV2,
    funding: &Funding,
    keep: &mut Vec<kaspa_database::utils::DbLifetime>,
) -> Node {
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))).unwrap());
    let config = Arc::new(chain.config.clone());
    let tick = Arc::new(TickService::new());
    let (meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    keep.push(meta_lifetime);
    let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta_db, tick.clone());
    let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        Default::default(),
        true,
        config.palw_model_market.is_some(),
        config.palw_activation_pool_fence().is_some(),
        None,
        AttestationMempoolPolicy::disabled(),
        kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
    )));
    let hub = kaspa_p2p_lib::Hub::new();
    let rules = Arc::new(MiningRuleEngine::new(
        manager.clone(),
        config.clone(),
        Default::default(),
        tick.clone(),
        hub.clone(),
        Default::default(),
        Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
    ));
    let flow = Arc::new(FlowContext::new(
        manager.clone(),
        addresses,
        config.clone(),
        pool.clone(),
        tick,
        chain.tc.notification_root(),
        hub,
        rules,
    ));
    let service = Arc::new(PalwPanelService::new(panel_config(chain, dir, bond, funding), manager.clone(), flow.clone(), config));
    let cloned = service.clone();
    let task = tokio::spawn(async move {
        cloned.start().await.unwrap();
    });
    Node { manager, pool, running: ServiceRun { service, task: Some(task) }, _flow: flow }
}

/// What the actual blocks carry of `bond`'s pursuit of `claim` since block `from`: reservations, demands, distinct demanded units.
fn carried(chain: &Chain, bond: PalwBondKeyV2, claim: Hash64) -> (usize, usize, usize) {
    let (mut reservations, mut demands) = (0usize, 0usize);
    let mut units = BTreeSet::new();
    for object in chain
        .history
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
        .map(|p| p.object)
    {
        match object {
            PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. }
                if reservation.claim == claim && reservation.reserver == bond =>
            {
                reservations += 1
            }
            PalwConsensusObjectV2::DefaultAccused { claim: c, accuser, missing_event_index, .. } if c == claim && accuser == bond => {
                demands += 1;
                units.insert(missing_event_index);
            }
            _ => {}
        }
    }
    (reservations, demands, units.len())
}

/// Mine whatever the node's mempool holds (two blocks: carrying, folding), else `idle` heartbeats.
async fn mine(chain: &mut Chain, node: &Node, idle: usize) {
    let (pending, _) = node.pool.clone().get_all_transactions(TransactionQuery::TransactionsOnly).await;
    if !pending.is_empty() {
        pool_heartbeat(chain, &node.manager, &node.pool).await;
        pool_heartbeat(chain, &node.manager, &node.pool).await;
    } else {
        for _ in 0..idle {
            pool_heartbeat(chain, &node.manager, &node.pool).await;
        }
    }
}

/// Run `node`'s service until `done(view)` holds (mining what it submits); at most `secs` of wall time.
async fn run_until(chain: &mut Chain, node: &Node, claim: Hash64, secs: u64, done: impl Fn(&PalwLegacyDisputeViewV1) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        assert!(!node.running.task.as_ref().unwrap().is_finished(), "the service stopped by itself");
        assert!(std::time::Instant::now() < deadline, "no objective progress: {:?}", chain.view(claim));
        if done(&chain.view(claim)) {
            return;
        }
        let idle = if chain.view(claim).sessions.is_empty() { 2 } else { 100 };
        mine(chain, node, idle).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The parent's fixture up to the moment the service starts: an ordinary matured public bond, an all-Panel-Valid false-root claim,
/// and an empty database that validated the public history. Returns `(chain, premine, outsider funding, bond, claim)`.
async fn licensed_withheld_claim() -> (Chain, Vec<Funding>, Funding, PalwBondKeyV2, Hash64) {
    let (config, bundle, premine, mut floats) = fixture_config();
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 0));
    let public_key = card_pubkey(OUTSIDER);
    let payout = card_payout_payload(OUTSIDER);
    let classes = BTreeSet::new();
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let message =
        palw_bond_registration_message_v2(chain.network_domain, &signed_bond, &public_key, &public_key, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(chain.network_domain, &signed_bond, &public_key, &public_key);
    let mut signature = signed(OUTSIDER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(signed(OUTSIDER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: signed_bond,
        pubkey: public_key.clone(),
        operator_pubkey: public_key.clone(),
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let tx = chain
        .send(OUTSIDER, &registration, &mut floats[OUTSIDER], &[TransactionOutput::new(collateral, card_payout_spk(OUTSIDER))])
        .await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
    let maturity = chain.config.params.palw_bond_maturity.unwrap().window_daa;
    let registered_before = chain.daa();
    chain.advance(registered_before + maturity + 1).await;
    let (attempt, claim) = chain.build_attempt_drawn(0, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(attempt).await;
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    chain.advance(slot).await;
    let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(anchor).await;
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty());
    let signed_daa = chain.daa();
    let receipts = duties
        .iter()
        .map(|d| {
            let card = chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap();
            let message = palw_receipt_message_v2(chain.network_domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
            PalwSeatReceiptV2 {
                claim,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: d.seat_bond,
                signed_daa,
                signature: signed(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            }
        })
        .collect();
    let object = chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("all colluding seats license");
    let card = chain.bonds.iter().position(|b| *b == duties[0].seat_bond).unwrap();
    chain.send(card, &object, &mut floats[card], &[]).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    chain.import_fresh(&premine).await;
    let funding = floats[OUTSIDER].clone();
    (chain, premine, funding, bond, claim)
}

/// **The actual service is stopped after its reservation and restarted (same state directory); the node is then replaced by an empty
/// database that re-validated every block, after the demand; the third service reaches the objective default.** One reservation,
/// no unit demanded twice, the default collected, the reservation released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14r_the_actual_public_filer_service_resumes_after_a_restart_and_on_a_fresh_database_without_duplicates() {
    super::on_service_stack(g14r_resumes_after_a_restart_and_on_a_fresh_database_body).await
}

async fn g14r_resumes_after_a_restart_and_on_a_fresh_database_body() {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let mut keep: Vec<kaspa_database::utils::DbLifetime> = Vec::new();
    let (mut chain, premine, funding, bond, claim) = licensed_withheld_claim().await;
    let dir = tempfile::tempdir().unwrap();

    // ---- service 1: runs until its reservation is mined, then stops ----
    let mut node = start_service(&chain, dir.path(), bond, &funding, &mut keep);
    run_until(&mut chain, &node, claim, 240, |v| v.record.as_ref().is_some_and(|r| r.live.contains_key(&bond))).await;
    node.running.finish().await;
    let stopped_at = chain.daa();
    let (reservations, _, _) = carried(&chain, bond, claim);
    assert_eq!(reservations, 1, "service 1 reserved once");
    eprintln!("[g14r-service] service 1 stopped at DAA {stopped_at} after its reservation");
    // The chain moves on with no service (a restart's downtime): ten blocks.
    for _ in 0..10 {
        pool_heartbeat(&mut chain, &node.manager, &node.pool).await;
    }
    drop(node);

    // ---- service 2 (same state directory, same node): rebuilds its case from the tip, never re-reserves ----
    let node = start_service(&chain, dir.path(), bond, &funding, &mut keep);
    run_until(&mut chain, &node, claim, 240, |v| v.sessions.iter().any(|(accuser, _)| *accuser == bond)).await;
    let (reservations, demands, units) = carried(&chain, bond, claim);
    assert_eq!(reservations, 1, "the restarted service did not reserve again");
    assert!(demands >= 1 && demands == units, "demanded once per unit: {demands} / {units}");
    let mut node = node;
    node.running.finish().await;
    drop(node);
    eprintln!("[g14r-service] service 2 demanded; the node is replaced at DAA {}", chain.daa());

    // ---- the node is replaced: an empty database validates every public block; service 3 carries the case to its outcome ----
    chain.import_fresh(&premine).await;
    let view = chain.view(claim);
    assert!(view.record.as_ref().is_some_and(|r| r.live.contains_key(&bond)), "the fresh database holds the reservation");
    let hard = view.sessions.iter().map(|(_, s)| s.deadline_daa).max().unwrap_or(0);
    eprintln!("[g14r-service] fresh database at DAA {}, open session deadline {hard}", chain.daa());
    let mut node = start_service(&chain, dir.path(), bond, &funding, &mut keep);
    run_until(&mut chain, &node, claim, 1500, |v| v.da_defaulted).await;
    let view = chain.view(claim);
    assert!(matches!(view.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    let (reservations, demands, units) = carried(&chain, bond, claim);
    assert_eq!(reservations, 1, "one reservation across two restarts and a fresh database");
    assert_eq!(demands, units, "no unit demanded twice across the restarts: {demands} / {units}");
    let offence = kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&view.producer.0, &claim);
    let filing = chain.tc.palw_reporter_filing_read_v1(offence, Hash64::default(), bond, None).unwrap();
    let consumed = filing.consumed.as_ref().expect("the default is collected");
    assert_eq!(consumed.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::DaDefault);
    assert!(consumed.amount > 0);
    assert!(chain.tc.palw_fraud_filer_status_v1(bond).unwrap().live.iter().all(|r| r.claim_id != claim), "released");
    eprintln!("[g14r-service] objective DA default at {} after two restarts and a fresh database", chain.daa());
    node.running.finish().await;
}
