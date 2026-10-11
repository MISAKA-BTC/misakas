//! Socket-level public block transport into an independent native consensus reader. The receiver
//! has no producer retention and its old original FP body is deliberately unavailable. Only the
//! ordinary P2P relay flows may insert the service's new answer carriers into its database.
use super::*;
use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
use kaspa_p2p_lib::{Adaptor, pb};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_public_job_multipart_is_authenticated_after_socket_p2p_block_relay() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::Network)).await;
}

pub(super) struct NetworkRun {
    receiver: Chain,
    _metadata: kaspa_database::utils::DbLifetime,
    _flow: Arc<FlowContext>,
    primary: Arc<Adaptor>,
    client: Arc<Adaptor>,
    relayed: usize,
}

impl NetworkRun {
    pub(super) async fn start(chain: &Chain, premine: &[Funding], primary_flow: Arc<FlowContext>, claim: Hash64) -> Self {
        let mut receiver = Chain::new(chain.config.clone(), chain.bundle.clone(), premine);
        for block in &chain.history {
            receiver.tc.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("independent prefix validation");
            receiver.history.push(block.clone());
        }
        assert_eq!(receiver.tc.get_sink(), chain.tc.get_sink());
        // Remove old original job/body rows on the independently validating receiver, just as a
        // pruned reader loses them. New suffix carriers must arrive exclusively over the socket.
        let outside = receiver.tc.palw_bond_of_pubkey_v2(&card_pubkey(OUTSIDER)).unwrap().0;
        let candidate = receiver.tc.palw_fraud_filer_candidates_v1(outside);
        let commitment_block = receiver
            .history
            .iter()
            .find(|b| {
                b.transactions.iter().any(|t| {
                    t.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                        && borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>(&t.payload)
                            .is_ok_and(|p| p.claim_id() == claim)
                })
            })
            .unwrap()
            .hash();
        receiver.tc.block_transactions_store.delete(commitment_block).unwrap();
        if let Some(row) = candidate.iter().find(|c| c.claim_id == claim) {
            receiver.tc.acceptance_data_store.delete(row.job.accepted_block).unwrap();
        }
        assert!(receiver.tc.get_block(commitment_block).is_err());
        assert!(receiver.view(claim).public_fp_job.is_none());
        let config = Arc::new(chain.config.clone());
        let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(receiver.tc.clone()))));
        let tick = Arc::new(TickService::new());
        let (metadata, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), db, tick.clone());
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
        let flow =
            Arc::new(FlowContext::new(manager, addresses, config, pool, tick, receiver.tc.notification_root(), hub.clone(), rules));
        // Select a loopback ephemeral port, then let the actual P2P adaptor own the listener.
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let primary = Adaptor::bidirectional(address.into(), primary_flow.hub().clone(), primary_flow, Default::default()).unwrap();
        let client = Adaptor::client_only(hub, flow.clone(), Default::default());
        tokio::time::timeout(Duration::from_secs(15), client.connect_peer(address.to_string()))
            .await
            .expect("loopback connection has a bounded deadline")
            .expect("actual P2P handshake accepts the same rules");
        eprintln!("[service-network] actual socket P2P handshake completed at DAA {}", chain.daa());
        Self { receiver, _metadata: metadata, _flow: flow, primary, client, relayed: chain.history.len() }
    }

    pub(super) async fn relay(&mut self, chain: &Chain) {
        for block in &chain.history[self.relayed..] {
            self.primary
                .broadcast(
                    pb::KaspadMessage {
                        payload: Some(pb::kaspad_message::Payload::InvRelayBlock(pb::InvRelayBlockMessage {
                            hash: Some(block.hash().into()),
                        })),
                        ..Default::default()
                    },
                    None,
                )
                .await;
        }
        self.relayed = chain.history.len();
    }

    pub(super) async fn finish(&mut self, chain: &Chain, claim: Hash64, service_start: usize) {
        self.relay(chain).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while self.receiver.tc.get_sink() != chain.tc.get_sink() {
            assert!(std::time::Instant::now() < deadline, "socket relay never validated the complete public suffix");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let floor = chain.history[service_start].header.daa_score;
        let wanted = [(claim, chain.view(claim).execution_root)].into_iter().collect();
        let legacy = [(claim, PalwLegacyHeldUnitV2::FpJob)].into_iter().collect();
        let mut previous = None;
        let mut opened = None;
        loop {
            let (walk, _, answers) = crate::palw_panel::palw_fraud_filer::palw_fraud_filer_read_page_v1(
                self.receiver.tc.as_ref().as_ref(),
                floor,
                floor,
                previous,
                64,
                &wanted,
                &legacy,
            )
            .expect("public history reads independently validated remote blocks");
            for ((id, unit), answer) in answers {
                if id == claim && unit == PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob) {
                    opened = Some(answer);
                }
            }
            if walk.next.is_none() {
                break;
            }
            assert_ne!(walk.next, Some(walk.anchor), "missing public suffix must not silently complete a read");
            previous = Some(walk);
        }
        let PalwDaBuiltAnswerV1::LegacyHeldV2(answer) = opened.expect("the network reader reconstructs the signed multipart job")
        else {
            panic!("the independent receiver must reconstruct the legacy public descriptor");
        };
        assert_eq!(
            kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&answer.0.job_context),
            chain.view(claim).job_identity
        );
        assert!(matches!(answer.1, kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerV2::FpJob(_)));
        eprintln!(
            "[service-network] independently validated socket blocks authenticated the multipart public job at DAA {}",
            self.receiver.daa()
        );
        self.client.close().await;
        self.primary.close().await;
    }
}
