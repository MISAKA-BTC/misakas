//! Socket-level public block transport into an independent native consensus reader. The receiver
//! has no producer retention and its old original FP body is deliberately unavailable. Only the
//! ordinary P2P relay flows may insert the service's new answer carriers into its database.
use super::*;
use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
use kaspa_p2p_lib::{Adaptor, pb};
#[path = "palw_service_rpc_e2e.rs"]
mod rpc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_public_job_multipart_is_authenticated_after_socket_p2p_block_relay() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::Network)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sole_socket_receiver_verifier_acquires_automatic_multipart_and_independently_releases() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::NetworkVerifier)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sole_socket_receiver_verifier_carries_requests_through_real_tx_relay_to_objective_default() {
    fresh_public_service_with_recovery(PublicCase::UnretainedWithheldFpV3Binding, Some(RecoveryMode::NetworkVerifier)).await;
}

pub(super) struct NetworkRun {
    receiver: Chain,
    _flow: Arc<FlowContext>,
    manager: Arc<ConsensusManager>,
    rules: Arc<MiningRuleEngine>,
    tick: Arc<TickService>,
    primary: Arc<Adaptor>,
    client: Arc<Adaptor>,
    relayed: usize,
    // The adaptors and FlowContext own the address-manager DB. Drop its lifetime only after
    // those references have been released, including on test failure.
    _metadata: kaspa_database::utils::DbLifetime,
}

impl NetworkRun {
    pub(super) async fn shutdown(self) {
        let Self { receiver, _flow: flow, manager, rules, tick, primary, client, _metadata: metadata, .. } = self;
        let flow_released = Arc::downgrade(&flow);
        let addresses_released = Arc::downgrade(&flow.address_manager);
        // SendPingsFlow keeps its context while waiting on the ordinary 120-second
        // tick. Process shutdown signals the tick service before waiting for owners.
        tick.shutdown();
        client.close().await;
        primary.close().await;
        // close() drains peers only. Dropping both adaptors also terminates the listener,
        // releases connection initializers and closes the asynchronous hub event queues.
        drop(client);
        drop(primary);
        drop(flow);
        drop(rules);
        drop(manager);
        tokio::time::timeout(Duration::from_secs(10), async {
            while flow_released.strong_count() != 0 || addresses_released.strong_count() != 0 {
                tokio::task::yield_now().await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("socket initializers, RPC workers and flow clones release the address DB before destruction");
        assert_eq!(addresses_released.strong_count(), 0);
        drop(metadata); // The strict DbLifetime reference assertion remains enabled.
        drop(receiver);
        eprintln!("[service-network] socket adaptors, flow/address owners and metadata DB joined and released");
    }

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
        let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(receiver.tc.clone()))).unwrap());
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
        let flow = Arc::new(FlowContext::new(
            manager.clone(),
            addresses,
            config,
            pool,
            tick.clone(),
            receiver.tc.notification_root(),
            hub.clone(),
            rules.clone(),
        ));
        // Select a loopback ephemeral port, then let the actual P2P adaptor own the listener.
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let primary = Adaptor::bidirectional(address.into(), primary_flow.hub().clone(), primary_flow, Default::default()).unwrap();
        let client = Adaptor::client_only(hub, flow.clone(), Default::default());
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match client.connect_peer(address.to_string()).await {
                    Ok(peer) => break Ok(peer),
                    Err(error) => {
                        let mut source: &(dyn std::error::Error + 'static) = &error;
                        let mut refused = false;
                        loop {
                            if source
                                .downcast_ref::<std::io::Error>()
                                .is_some_and(|e| e.kind() == std::io::ErrorKind::ConnectionRefused)
                            {
                                refused = true;
                            }
                            let Some(next) = source.source() else { break };
                            source = next;
                        }
                        if !refused {
                            break Err(error);
                        }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }
            }
        })
        .await
        .expect("loopback connection has a bounded deadline")
        .expect("actual P2P handshake accepts the same rules");
        eprintln!("[service-network] actual socket P2P handshake completed at DAA {}", chain.daa());
        Self { receiver, _metadata: metadata, _flow: flow, manager, rules, tick, primary, client, relayed: chain.history.len() }
    }

    pub(super) async fn verifier_endpoint(&self) -> (Arc<ConsensusManager>, Arc<FlowContext>) {
        assert!(
            self._flow.is_nearly_synced(&self.manager.consensus().unguarded_session()).await,
            "the normal transaction relay sync predicate must hold without a bypass"
        );
        (self.manager.clone(), self._flow.clone())
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

    pub(super) async fn finish(&mut self, chain: &Chain, claim: Hash64, service_start: usize, material_answered: bool) {
        self.relay(chain).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while self.receiver.tc.get_sink() != chain.tc.get_sink() {
            assert!(std::time::Instant::now() < deadline, "socket relay never validated the complete public suffix");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !material_answered {
            assert!(chain.view(claim).da_defaulted, "the source accepted the remote verifier's objective default");
            assert!(self.receiver.view(claim).da_defaulted, "the independent receiver validates the same default");
            assert_eq!(self.receiver.view(claim), chain.view(claim));
            self.client.close().await;
            self.primary.close().await;
            return;
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
        rpc::verify_socket_evidence(&self.receiver, self._flow.clone(), self.rules.clone(), claim, floor).await;
        self.client.close().await;
        self.primary.close().await;
    }
}
