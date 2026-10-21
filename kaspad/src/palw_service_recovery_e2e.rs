//! Actual producer answer worker and independent public verifier recovery. The producer alone
//! receives its real retained capture; the verifier starts and restarts with an empty retention
//! directory and obtains signed material exclusively from accepted public carriers.
use super::*;
#[path = "palw_service_network_e2e.rs"]
mod network;
#[path = "palw_service_pruned_e2e.rs"]
pub(super) mod pruned;
use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;

type Foreign = (
    kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
    kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
    Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_producer_multipart_and_fresh_verifier_resume_after_partial_transport() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::Restart)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restarted_public_verifier_defaults_when_automatic_producer_holds_no_material() {
    fresh_public_service_with_recovery(PublicCase::UnretainedWithheldFpV3Binding, Some(RecoveryMode::Restart)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_multipart_recovery_and_continuous_public_verifier_survive_a_real_selected_chain_reorg() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::Reorg)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_verifier_reacquires_one_pruned_parallel_input_lane_from_the_automatic_producer() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::PrunedRound)).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryMode {
    Restart,
    Reorg,
    Network,
    NetworkVerifier,
    Pruned,
    PrunedRound,
}

pub(super) struct RecoveryRun {
    _directory: tempfile::TempDir,
    producer: ServiceRun,
    producer_config: PalwPanelConfig,
    verifier_config: PalwPanelConfig,
    verifier_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    manager: Arc<ConsensusManager>,
    flow: Arc<FlowContext>,
    config: Arc<Config>,
    notification_root: Arc<kaspa_consensus_notify::root::ConsensusNotificationRoot>,
    producer_restarted: bool,
    verifier_restarted: bool,
    paused_input: bool,
    pruned_round: Option<(usize, u8)>,
    pruned_numbered_before: Option<(u8, u64)>,
    honest: bool,
    job_only_recovery: Option<(std::path::PathBuf, Vec<u8>)>,
    mode: RecoveryMode,
    premine: Vec<Funding>,
    reorged: bool,
    network: Option<network::NetworkRun>,
}

pub(super) async fn launch(
    config: PalwPanelConfig,
    manager: Arc<ConsensusManager>,
    flow: Arc<FlowContext>,
    consensus_config: Arc<Config>,
    notification_root: Arc<kaspa_consensus_notify::root::ConsensusNotificationRoot>,
) -> ServiceRun {
    // A real node restart constructs a new FlowContext. Its gossip inbox is single-owner;
    // sharing or restarting the old context disables the next panel service entirely.
    // Keep only the public consensus, transaction pool, addresses and connected hub shared.
    let tick = Arc::new(TickService::new());
    let hub = flow.hub().clone();
    let rules = Arc::new(MiningRuleEngine::new(
        manager.clone(),
        consensus_config.clone(),
        Default::default(),
        tick.clone(),
        hub.clone(),
        Default::default(),
        Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
    ));
    let fresh = Arc::new(FlowContext::new(
        manager.clone(),
        flow.address_manager.clone(),
        consensus_config.clone(),
        flow.mining_manager().clone(),
        tick,
        notification_root,
        hub,
        rules,
    ));
    launch_on_flow(config, manager, fresh, consensus_config).await
}

async fn launch_on_flow(
    config: PalwPanelConfig,
    manager: Arc<ConsensusManager>,
    flow: Arc<FlowContext>,
    consensus_config: Arc<Config>,
) -> ServiceRun {
    let service = Arc::new(PalwPanelService::new(config, manager, flow, consensus_config));
    assert!(service.keypair.is_some());
    let started = service.clone();
    let task = tokio::spawn(async move { started.start().await.unwrap() });
    ServiceRun { service, task: Some(task) }
}

impl RecoveryRun {
    pub(super) async fn shutdown_network(&mut self) {
        if let Some(network) = self.network.take() {
            network.shutdown().await;
        }
    }

    pub(super) async fn start(
        chain: &mut Chain,
        case: PublicCase,
        foreign: Option<&Foreign>,
        funding: &Funding,
        manager: Arc<ConsensusManager>,
        flow: Arc<FlowContext>,
        verifier_config: PalwPanelConfig,
        mode: RecoveryMode,
        premine: &[Funding],
    ) -> Self {
        if mode == RecoveryMode::NetworkVerifier {
            // Use already elapsed normal clock slots inside the real near-sync window.
            // Keep the original native cadence, lead cap, deadline and admission checks.
            let threshold = chain.config.expected_difficulty_window_duration_in_milliseconds() / 4;
            let margin = 2 * kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;
            assert!(threshold > margin, "the native near-sync history must leave elapsed protocol slots");
            chain.time = kaspa_core::time::unix_now().saturating_sub(threshold - margin);
            chain.heartbeat(vec![]).await;
            assert!(matches!(
                chain
                    .view(kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(
                        &foreign.expect("FP network fixture").2.as_ref().unwrap().0
                    ))
                    .phase,
                PalwClaimPhaseV2::ReceiptLicensed { .. }
            ));
        }
        let directory = tempfile::tempdir().unwrap();
        let mut producer_config = panel_config(chain, directory.path(), chain.bonds[0], funding);
        // Separate key, fee chain, state and retention. The shared node is only the public
        // consensus/mempool interface; neither worker is handed the other worker's book/cache.
        std::fs::write(&producer_config.key_path, faster_hex::hex_string(&[0xB0; 32])).unwrap();
        producer_config.retention_dir = directory.path().join("producer-retention");
        std::fs::create_dir_all(&producer_config.retention_dir).unwrap();
        let honest = case == PublicCase::UnretainedLargeConstraintJob;
        let mut job_only_recovery = None;
        if honest {
            let (outcome, _, Some((commitment, prompt))) = foreign.unwrap() else { unreachable!() };
            assert!(!outcome.material.is_empty(), "the producer keeps actual executed family material");
            let capture =
                kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(&commitment.job, prompt, &outcome.material);
            let claim = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment);
            let path = crate::palw_producer::palw_retained_material_path(&producer_config.retention_dir, &claim);
            std::fs::write(&path, capture).unwrap();
            job_only_recovery =
                Some((path, kaspa_consensus_core::palw_freeprompt_v3::palw_fp_material_encode_v1(&commitment.job, prompt)));
        }
        let config = Arc::new(chain.config.clone());
        let network = if matches!(mode, RecoveryMode::Network | RecoveryMode::NetworkVerifier) {
            Some(
                network::NetworkRun::start(
                    chain,
                    premine,
                    flow.clone(),
                    kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&foreign.unwrap().2.as_ref().unwrap().0),
                )
                .await,
            )
        } else {
            None
        };
        let notification_root = chain.tc.notification_root();
        let producer = launch(producer_config.clone(), manager.clone(), flow.clone(), config.clone(), notification_root.clone()).await;
        assert_eq!(producer.service.bond, Some(chain.bonds[0].0));
        let verifier_bond =
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(crate::palw_producer::parse_outpoint(&verifier_config.bond).unwrap());
        Self {
            _directory: directory,
            producer,
            producer_config,
            verifier_config,
            verifier_bond,
            manager,
            flow,
            config,
            notification_root,
            producer_restarted: false,
            verifier_restarted: false,
            paused_input: false,
            pruned_round: None,
            pruned_numbered_before: None,
            honest,
            job_only_recovery,
            mode,
            premine: premine.to_vec(),
            reorged: false,
            network,
        }
    }

    pub(super) async fn activate_network_verifier(&mut self, service: &mut Arc<PalwPanelService>, verifier: &mut ServiceRun) {
        if self.mode != RecoveryMode::NetworkVerifier {
            return;
        }
        assert!(verifier.task.is_none(), "the primary node never starts a second outsider service");
        let network = self.network.as_ref().expect("independent socket receiver");
        let (manager, flow) = network.verifier_endpoint().await;
        let launched = launch_on_flow(self.verifier_config.clone(), manager, flow, self.config.clone()).await;
        *service = launched.service.clone();
        *verifier = launched;
        eprintln!("[service-network] sole outside verifier runs on the independently validated receiver DB");
    }

    /// Stop before inclusion so the first worker cannot learn a local copy of the answer
    /// that this fixture will prune. The pending bytes came from the real producer worker.
    pub(super) async fn before_carriers(
        &mut self,
        claim: Hash64,
        mut pending: impl Iterator<Item = PalwConsensusObjectV2>,
        verifier: &mut ServiceRun,
    ) {
        if self.mode != RecoveryMode::PrunedRound || self.paused_input || self.pruned_round.is_some() {
            return;
        }
        // Merely recognize the already signed producer object; no public answer is fabricated.
        if pending.any(|o| {
            matches!(o, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
            if answer.claim == claim && answer.unit == PalwLegacyHeldUnitV2::PromptIds { chunk: 0 })
        }) {
            verifier.finish().await;
            // The carrier is recovered from actual accepted history in tick, after native validation.
            self.paused_input = true;
            eprintln!("[service-recovery] outsider paused before exact template input carrier inclusion");
        }
    }

    /// Pin the same reservation's actual counter before the exact immutable template,
    /// rather than assuming no ordinary round sessions opened since its body was pruned.
    pub(super) fn observe_numbered_template(&mut self, chain: &Chain, claim: Hash64, objects: &[PalwConsensusObjectV2]) {
        if self.pruned_round.is_none() {
            return;
        }
        let unit = PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk: 0 });
        let requests: Vec<_> = objects
            .iter()
            .filter_map(|object| match object {
                PalwConsensusObjectV2::DisputeReacquiredV1 { request, signature }
                    if request.claim == claim && request.reserver == self.verifier_bond && request.unit == unit =>
                {
                    assert!(!signature.is_empty());
                    Some(request)
                }
                _ => None,
            })
            .collect();
        if requests.is_empty() {
            return;
        }
        assert_eq!(requests.len(), 1, "one template must not carry duplicate numbered input requests");
        assert!(self.pruned_numbered_before.is_none(), "the numbered input request gets exactly one immutable template");
        // No other owner operation in this SAME template may open/join a session before the
        // numbered request. Other owners do not affect this reservation's counter.
        assert!(!objects.iter().any(|object| matches!(object,
            PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand }
                if demand.claim == claim && demand.accuser == self.verifier_bond)));
        assert!(!objects.iter().any(|object| matches!(object,
            PalwConsensusObjectV2::DefaultAccused { claim: id, accuser, .. }
                if *id == claim && *accuser == self.verifier_bond)));
        assert_eq!(
            objects
                .iter()
                .filter(|object| matches!(object,
            PalwConsensusObjectV2::DisputeReacquiredV1 { request, .. }
                if request.claim == claim && request.reserver == self.verifier_bond))
                .count(),
            1,
            "the exact input template contains no other numbered owner request"
        );
        let view = chain.view(claim);
        assert!(view.unit_was_answered_v1(&unit));
        assert!(!view.sessions.iter().any(|(owner, _)| *owner == self.verifier_bond));
        let reservation = view.record.as_ref().unwrap().live.get(&self.verifier_bond).unwrap();
        assert_eq!(requests[0].session_number, reservation.sessions_opened.checked_add(1).unwrap());
        self.pruned_numbered_before = Some((reservation.sessions_opened, reservation.reserved_daa));
    }

    /// After the body is actually pruned, let both restarted production workers
    /// consume the ordinary block stream. The generic harness's 50-block bursts
    /// can expire the original reservation while a two-second worker is draining
    /// its sixteen carriers. This changes only test-clock pacing, never a chain
    /// deadline, fee, session/lane limit or the native acceptance path.
    pub(super) fn recovering_pruned_round(&self) -> bool {
        self.mode == RecoveryMode::PrunedRound && self.pruned_round.is_some()
    }

    pub(super) fn verifier_paused(&self) -> bool {
        self.mode == RecoveryMode::PrunedRound && self.paused_input && self.pruned_round.is_none()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn tick(
        &mut self,
        chain: &mut Chain,
        claim: Hash64,
        service_start: usize,
        service: &mut Arc<PalwPanelService>,
        verifier: &mut ServiceRun,
        answer_submitted: &mut Option<kaspa_consensus_core::tx::TransactionId>,
        job_preimage_submitted: &mut bool,
        input_chunks: &mut BTreeSet<u32>,
    ) {
        assert!(!self.producer.task.as_ref().unwrap().is_finished(), "automatic producer worker stopped");
        if let Some(network) = self.network.as_mut() {
            network.relay(chain).await;
        }
        for tx in chain.history[service_start..].iter().flat_map(|b| b.transactions.iter()) {
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            match payload.object {
                PalwConsensusObjectV2::MaterialDisclosedV2 { claim: id, unit: PalwDaUnitV1::Event { row: 0, tile: 0 }, .. }
                    if id == claim =>
                {
                    *answer_submitted = Some(tx.id());
                }
                PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } if answer.claim == claim => {
                    if let PalwLegacyHeldUnitV2::PromptIds { chunk } = answer.unit {
                        input_chunks.insert(chunk);
                    }
                }
                _ => {}
            }
        }
        let view = chain.view(claim);
        *job_preimage_submitted = view.unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob));
        let progress = chain.tc.palw_legacy_answer_progress_v2((claim, PalwLegacyHeldUnitV2::FpJob, chain.bonds[0])).unwrap();
        let partial = progress.as_ref().is_some_and(|p| p.received != 0 && p.next_index().is_some());
        self.after_accepted_carrier(chain, claim).await;
        if self.verifier_paused()
            && view.unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk: 0 }))
            && !view.sessions.iter().any(|(bond, _)| *bond == self.verifier_bond)
        {
            let unit = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
            let key = PalwDaUnitV1::LegacyHeldV2(unit);
            // Native accepted extraction, not mempool presence or the retained producer file.
            let accepted = accepted_objects(chain, service_start);
            assert!(
                accepted.iter().any(|o| matches!(o, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                if answer.claim == claim && answer.unit == unit && answer.discloser == chain.bonds[0])),
                "production template and selected-chain acceptance must include the paused input answer"
            );
            assert_multipart_transport(chain, claim, service_start);
            let source = chain.history[service_start..]
                .iter()
                .find(|block| {
                    block.transactions.iter().any(|tx| {
                        borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).is_ok_and(|p| {
                            matches!(p.object, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                        if answer.claim == claim && answer.unit == unit)
                        })
                    })
                })
                .unwrap()
                .hash();
            let acceptance = chain
                .history
                .iter()
                .find(|block| {
                    chain
                        .tc
                        .get_block_acceptance_data(block.hash())
                        .is_ok_and(|rows| rows.iter().any(|row| row.block_hash == source && !row.accepted_transactions.is_empty()))
                })
                .expect("the carrier's actual acceptance row exists")
                .hash();
            let me = self.verifier_bond;
            let opened =
                view.record.as_ref().unwrap().live.get(&me).expect("the same ordinary reservation remains live").sessions_opened;
            assert!(view.unit_was_answered_v1(&key));
            assert!(!view.sessions.iter().any(|(bond, _)| *bond == me), "the answered bootstrap session actually closed");
            chain.tc.block_transactions_store.delete(source).unwrap();
            chain.tc.acceptance_data_store.delete(acceptance).unwrap();
            assert!(chain.tc.get_block(source).is_err());
            assert!(chain.tc.get_block_acceptance_data(acceptance).is_err());
            assert!(chain.view(claim).unit_was_answered_v1(&key), "rooted answer guard survives body pruning");
            assert_eq!(chain.view(claim).record.as_ref().unwrap().live.get(&me).unwrap().sessions_opened, opened);
            assert!(verifier.task.is_none(), "the old book was stopped before it could learn the input");
            // Forget producer sent keys too. Its public descriptor/input is retained, not copied
            // into the fresh verifier. A genuine numbered request must drive the second answer.
            self.producer.finish().await;
            self.producer = launch(
                self.producer_config.clone(),
                self.manager.clone(),
                self.flow.clone(),
                self.config.clone(),
                self.notification_root.clone(),
            )
            .await;
            let restarted = launch(
                self.verifier_config.clone(),
                self.manager.clone(),
                self.flow.clone(),
                self.config.clone(),
                self.notification_root.clone(),
            )
            .await;
            *service = restarted.service.clone();
            *verifier = restarted;
            self.verifier_restarted = true;
            assert!(service.fraud_filer_honest_replays.lock().unwrap().is_empty());
            assert!(std::fs::read_dir(&self.verifier_config.retention_dir).unwrap().next().is_none());
            self.pruned_round = Some((chain.history.len(), opened));
            eprintln!("[service-recovery] pruned actual input carrier and restarted empty public book at DAA {}", chain.daa());
        }
        let waiting_for_binding = view.sessions.iter().any(|(_, s)| s.units.contains(&PalwDaUnitV1::Event { row: 0, tile: 0 }));
        if self.mode == RecoveryMode::Restart && !self.verifier_restarted && (partial || (!self.honest && waiting_for_binding)) {
            verifier.finish().await;
            let restarted = launch(
                self.verifier_config.clone(),
                self.manager.clone(),
                self.flow.clone(),
                self.config.clone(),
                self.notification_root.clone(),
            )
            .await;
            *service = restarted.service.clone();
            *verifier = restarted;
            self.verifier_restarted = true;
            assert!(service.fraud_filer_honest_replays.lock().unwrap().is_empty(), "restart retains no private replay verdict");
            assert_eq!(
                service.config.public_claims,
                vec![claim],
                "the explicit operator target survives an empty-cache service restart"
            );
            assert!(
                std::fs::read_dir(&self.verifier_config.retention_dir).unwrap().next().is_none(),
                "verifier starts with an empty retention directory and never receives producer capture"
            );
            eprintln!("[service-recovery] public verifier restarted with empty capture cache at DAA {}", chain.daa());
        }
    }

    /// Called immediately after ONE native block is accepted, before another template can
    /// finish the assembly. This observes the rooted bitmap, never mempool submission.
    pub(super) async fn after_accepted_carrier(&mut self, chain: &mut Chain, claim: Hash64) {
        let progress = chain.tc.palw_legacy_answer_progress_v2((claim, PalwLegacyHeldUnitV2::FpJob, chain.bonds[0])).unwrap();
        // Stop after a genuinely accepted partial assembly. Restarting the producer discards all
        // private sent keys; the native manifest/bitmap must select only the missing carrier.
        let partial = progress.as_ref().is_some_and(|p| p.received != 0 && p.next_index().is_some());
        if self.honest && partial && !self.producer_restarted {
            self.producer.finish().await;
            // Simulate a lost capture after a genuinely accepted part, preserving only the
            // producer's original public descriptor/input. A fresh backend must remake it
            // within its constraint scope before reproducing the accepted signed manifest.
            let (path, descriptor) = self.job_only_recovery.as_ref().unwrap();
            std::fs::write(path, descriptor).unwrap();
            if self.mode == RecoveryMode::Reorg {
                self.reorg_partial_answer(chain, claim).await;
            }
            self.producer = launch(
                self.producer_config.clone(),
                self.manager.clone(),
                self.flow.clone(),
                self.config.clone(),
                self.notification_root.clone(),
            )
            .await;
            self.producer_restarted = true;
            eprintln!("[service-recovery] producer restarted from accepted partial manifest at DAA {}", chain.daa());
        }
    }

    async fn reorg_partial_answer(&mut self, chain: &mut Chain, claim: Hash64) {
        let part = chain
            .history
            .iter()
            .position(|b| {
                b.transactions.iter().any(|tx| {
                    borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).is_ok_and(|p| {
                        matches!(p.object,
                PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
                    if chunk.claim == claim && chunk.unit == PalwLegacyHeldUnitV2::FpJob && chunk.index == 0)
                    })
                })
            })
            .expect("the first part was actually carried");
        assert!(part > 0);
        let fork = chain.history[part - 1].hash();
        let old_sink = chain.tc.get_sink();
        let old_work = chain.tc.get_header(old_sink).unwrap().blue_work;
        let mut branch = Chain::new(chain.config.clone(), chain.bundle.clone(), &self.premine);
        for block in &chain.history[..part] {
            branch
                .tc
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .expect("the independent fork validates public history");
            branch.history.push(block.clone());
        }
        assert_eq!(branch.tc.get_sink(), fork);
        branch.time = chain.history[part - 1].header.timestamp;
        branch.nonce = chain.nonce.saturating_add(1000);
        let fork_daa = branch.daa();
        let count = branch.history.len();
        while branch.tc.get_header(branch.tc.get_sink()).unwrap().blue_work <= old_work {
            branch.heartbeat(vec![]).await;
            assert!(
                branch.daa().saturating_sub(fork_daa) <= kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1,
                "the fork must remain inside the shipped shallow economic-tie window"
            );
        }
        let expected = branch
            .tc
            .palw_legacy_answer_progress_v2((claim, PalwLegacyHeldUnitV2::FpJob, branch.bonds[0]))
            .unwrap()
            .expect("the common signed manifest survives but its later part is absent");
        assert_eq!(expected.received, 0);
        let added = branch.history[count..].to_vec();
        for block in added {
            chain
                .tc
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .expect("the live node validates the competing fork");
            chain.history.push(block);
        }
        assert_eq!(chain.tc.get_sink(), branch.tc.get_sink(), "the heavier admissible fork actually becomes selected");
        assert_ne!(chain.tc.get_sink(), old_sink);
        let rolled_back =
            chain.tc.palw_legacy_answer_progress_v2((claim, PalwLegacyHeldUnitV2::FpJob, chain.bonds[0])).unwrap().unwrap();
        assert_eq!(rolled_back, expected, "the actual V-fold rollback restores the declaration and accepted bitmap exactly");
        assert!(!chain.view(claim).unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob)));
        chain.time = chain.time.max(branch.time);
        chain.nonce = chain.nonce.max(branch.nonce);
        self.reorged = true;
        eprintln!("[service-recovery] actual selected-chain reorg rolled accepted multipart bitmap back at DAA {}", chain.daa());
    }

    pub(super) async fn finish(&mut self, chain: &Chain, claim: Hash64, service_start: usize) {
        assert_eq!(self.verifier_config.public_claims, vec![claim], "actual recovery uses the bounded native exact-target read");
        if self.mode == RecoveryMode::Restart {
            assert!(self.verifier_restarted, "the test must actually restart the public service while its case is live");
        } else if self.mode == RecoveryMode::Reorg {
            assert!(self.reorged, "a real selected-chain switch must roll back the accepted partial assembly");
            assert!(!self.verifier_restarted, "the public verifier must continue across the real reorg without losing its book");
        }
        if self.honest {
            assert!(self.producer_restarted, "the producer must restart during accepted partial multipart transport");
            if let Some((after, opened)) = self.pruned_round {
                assert!(self.verifier_restarted);
                let unit = PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk: 0 });
                let objects = accepted_objects(chain, after);
                let me = self.verifier_bond;
                let requests: Vec<_> = objects
                    .iter()
                    .filter_map(|o| match o {
                        PalwConsensusObjectV2::DisputeReacquiredV1 { request, signature }
                            if request.claim == claim && request.reserver == me && request.unit == unit =>
                        {
                            assert!(!signature.is_empty());
                            Some(request)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(requests.len(), 1, "exactly one signed numbered reacquisition must be accepted");
                let (before, reserved_daa) =
                    self.pruned_numbered_before.expect("the exact numbered template must pin the accepted counter");
                assert_eq!(requests[0].session_number, before.checked_add(1).unwrap());
                assert_eq!(requests[0].reserved_daa, reserved_daa);
                // The fresh book first drains the original bounded StepNode round. Its
                // ordinary tag157 sessions share the SAME global counter; their starts are
                // distinct from a numbered reacquisition of historically answered input.
                let ordinary: Vec<_> = objects
                    .iter()
                    .filter_map(|object| match object {
                        PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand } if demand.claim == claim && demand.accuser == me => {
                            Some(demand.unit)
                        }
                        _ => None,
                    })
                    .collect();
                // The existing sixteen-lane StepNode round, drained before the input retry. A lane the filer saw no landing for
                // within `PALW_FRAUD_FILER_RESEND_DAA_V1` is sent again, so the accepted demands may repeat a lane (validation
                // 2026-10-17: 17 accepted demands, lane (9, 6) twice) — the round is the DISTINCT set: sixteen StepNodes of one
                // level with contiguous indices.
                let distinct: BTreeSet<_> = ordinary.iter().copied().collect();
                assert_eq!(distinct.len(), 16, "only the existing sixteen-lane round is drained before input retry: {ordinary:?}");
                let mut nodes: Vec<(u8, u64)> = distinct
                    .iter()
                    .map(|unit| match unit {
                        PalwLegacyHeldUnitV2::StepNode { level, index } => (*level, *index),
                        other => panic!("only the existing StepNode round is drained before input retry: {other:?}"),
                    })
                    .collect();
                nodes.sort();
                assert!(nodes.iter().all(|(l, _)| *l == nodes[0].0), "one round is one level: {nodes:?}");
                assert!(nodes.windows(2).all(|w| w[1].1 == w[0].1 + 1), "one round's lanes are contiguous: {nodes:?}");
                eprintln!(
                    "[service-recovery] the existing {}-lane StepNode round drained before input retry ({} accepted demands)",
                    distinct.len(),
                    ordinary.len()
                );
                assert!(
                    before >= opened && usize::from(before - opened) <= ordinary.len(),
                    "every additional ordinary session is accounted by its accepted round demand"
                );
                assert_eq!(
                    objects
                        .iter()
                        .filter(|object| matches!(object,
                    PalwConsensusObjectV2::DisputeReacquiredV1 { request, .. }
                        if request.claim == claim && request.reserver == me))
                        .count(),
                    1,
                    "no second numbered reacquisition of any other unit may hide in the global counter"
                );
                assert!(
                    !objects.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. }
                        if reservation.claim == claim && reservation.reserver == me)),
                    "the fresh book must retain its original ordinary reservation"
                );
                eprintln!(
                    "[service-recovery] input retry accounting: prune counter {opened}, pre-request counter {before}, exact accepted session {}, ordinary round demands {}",
                    requests[0].session_number,
                    ordinary.len()
                );
                assert!(
                    objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                    if answer.claim == claim && answer.unit == PalwLegacyHeldUnitV2::PromptIds { chunk: 0 }
                        && answer.discloser == chain.bonds[0])),
                    "the actual automatic producer reanswers after pruning"
                );
                assert!(chain.view(claim).unit_was_answered_v1(&unit));
            } else {
                assert!(self.mode != RecoveryMode::PrunedRound, "the pruning fixture must actually prune a carried input");
                assert_multipart_transport(chain, claim, service_start);
            }
        } else {
            assert!(chain.view(claim).da_defaulted);
        }
        if let Some(network) = self.network.as_mut() {
            network.finish(chain, claim, service_start, self.honest).await;
        }
        self.producer.finish().await;
    }
}

/// Read the selected chain's actual accepted objects; an orphaned branch is not a duplicate
/// transport publication on the selected chain after the reorg.
pub(super) fn accepted_objects(chain: &Chain, service_start: usize) -> Vec<PalwConsensusObjectV2> {
    let floor = chain.history[service_start].header.daa_score;
    let mut cursor = Some(chain.tc.get_sink());
    let mut objects = Vec::new();
    let mut pages = 0;
    while let Some(start) = cursor {
        pages += 1;
        assert!(pages <= chain.history.len().div_ceil(64) + 2, "selected-chain history pagination must progress");
        cursor = crate::palw_panel::walk_accepted_lifecycle_page_with_block_v2(
            chain.tc.as_ref().as_ref(),
            start,
            floor,
            64,
            &mut |_, object| objects.push(object),
        )
        .expect("terminal verification reads the actual selected-chain acceptance");
    }
    objects
}

fn assert_multipart_transport(chain: &Chain, claim: Hash64, service_start: usize) {
    let mut declarations = 0;
    let mut chunks = BTreeSet::new();
    let objects = accepted_objects(chain, service_start);
    for object in objects {
        match object {
            PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration }
                if declaration.claim == claim && declaration.unit == PalwLegacyHeldUnitV2::FpJob =>
            {
                assert_eq!(declaration.discloser, chain.bonds[0]);
                declarations += 1;
            }
            PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
                if chunk.claim == claim && chunk.unit == PalwLegacyHeldUnitV2::FpJob =>
            {
                assert_eq!(chunk.discloser, chain.bonds[0]);
                assert!(chunks.insert(chunk.index), "automatic restart must not carry an already accepted part again");
            }
            PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                if answer.claim == claim && answer.unit == PalwLegacyHeldUnitV2::FpJob =>
            {
                panic!("the large descriptor must be supplied by the actual multipart worker");
            }
            _ => {}
        }
    }
    assert_eq!(declarations, 1);
    assert_eq!(chunks.len(), 2);
    assert!(chain.view(claim).unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob)));
}
