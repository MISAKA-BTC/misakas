//! Actual producer answer worker and independent public verifier recovery. The producer alone
//! receives its real retained capture; the verifier starts and restarts with an empty retention
//! directory and obtains signed material exclusively from accepted public carriers.
use super::*;
#[path = "palw_service_network_e2e.rs"]
mod network;
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryMode {
    Restart,
    Reorg,
    Network,
}

pub(super) struct RecoveryRun {
    _directory: tempfile::TempDir,
    producer: ServiceRun,
    producer_config: PalwPanelConfig,
    verifier_config: PalwPanelConfig,
    manager: Arc<ConsensusManager>,
    flow: Arc<FlowContext>,
    config: Arc<Config>,
    producer_restarted: bool,
    verifier_restarted: bool,
    honest: bool,
    job_only_recovery: Option<(std::path::PathBuf, Vec<u8>)>,
    mode: RecoveryMode,
    premine: Vec<Funding>,
    reorged: bool,
    network: Option<network::NetworkRun>,
}

async fn launch(
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
    pub(super) async fn start(
        chain: &Chain,
        case: PublicCase,
        foreign: Option<&Foreign>,
        funding: &Funding,
        manager: Arc<ConsensusManager>,
        flow: Arc<FlowContext>,
        verifier_config: PalwPanelConfig,
        mode: RecoveryMode,
        premine: &[Funding],
    ) -> Self {
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
        let network = if mode == RecoveryMode::Network {
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
        let producer = launch(producer_config.clone(), manager.clone(), flow.clone(), config.clone()).await;
        assert_eq!(producer.service.bond, Some(chain.bonds[0].0));
        Self {
            _directory: directory,
            producer,
            producer_config,
            verifier_config,
            manager,
            flow,
            config,
            producer_restarted: false,
            verifier_restarted: false,
            honest,
            job_only_recovery,
            mode,
            premine: premine.to_vec(),
            reorged: false,
            network,
        }
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
            self.producer = launch(self.producer_config.clone(), self.manager.clone(), self.flow.clone(), self.config.clone()).await;
            self.producer_restarted = true;
            eprintln!("[service-recovery] producer restarted from accepted partial manifest at DAA {}", chain.daa());
        }
        let waiting_for_binding = view.sessions.iter().any(|(_, s)| s.units.contains(&PalwDaUnitV1::Event { row: 0, tile: 0 }));
        if self.mode == RecoveryMode::Restart && !self.verifier_restarted && (partial || (!self.honest && waiting_for_binding)) {
            verifier.finish().await;
            let restarted = launch(self.verifier_config.clone(), self.manager.clone(), self.flow.clone(), self.config.clone()).await;
            *service = restarted.service.clone();
            *verifier = restarted;
            self.verifier_restarted = true;
            assert!(service.fraud_filer_honest_replays.lock().unwrap().is_empty(), "restart retains no private replay verdict");
            assert!(
                std::fs::read_dir(&self.verifier_config.retention_dir).unwrap().next().is_none(),
                "verifier starts with an empty retention directory and never receives producer capture"
            );
            eprintln!("[service-recovery] public verifier restarted with empty capture cache at DAA {}", chain.daa());
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
        if self.mode == RecoveryMode::Restart {
            assert!(self.verifier_restarted, "the test must actually restart the public service while its case is live");
        } else if self.mode == RecoveryMode::Reorg {
            assert!(self.reorged, "a real selected-chain switch must roll back the accepted partial assembly");
            assert!(!self.verifier_restarted, "the public verifier must continue across the real reorg without losing its book");
        }
        if self.honest {
            assert!(self.producer_restarted, "the producer must restart during accepted partial multipart transport");
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
        } else {
            assert!(chain.view(claim).da_defaulted);
        }
        if let Some(network) = self.network.as_mut() {
            network.finish(chain, claim, service_start).await;
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
