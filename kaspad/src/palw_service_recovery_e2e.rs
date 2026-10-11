//! Actual producer answer worker and independent public verifier recovery. The producer alone
//! receives its real retained capture; the verifier starts and restarts with an empty retention
//! directory and obtains signed material exclusively from accepted public carriers.
use super::*;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;

type Foreign = (
    kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
    kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
    Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_producer_multipart_and_fresh_verifier_resume_after_partial_transport() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restarted_public_verifier_defaults_when_automatic_producer_holds_no_material() {
    fresh_public_service_with_recovery(PublicCase::UnretainedWithheldFpV3Binding, true).await;
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
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut producer_config = panel_config(chain, directory.path(), chain.bonds[0], funding);
        // Separate key, fee chain, state and retention. The shared node is only the public
        // consensus/mempool interface; neither worker is handed the other worker's book/cache.
        std::fs::write(&producer_config.key_path, faster_hex::hex_string(&[0xB0; 32])).unwrap();
        producer_config.retention_dir = directory.path().join("producer-retention");
        std::fs::create_dir_all(&producer_config.retention_dir).unwrap();
        let honest = case == PublicCase::UnretainedLargeConstraintJob;
        if honest {
            let (outcome, _, Some((commitment, prompt))) = foreign.unwrap() else { unreachable!() };
            assert!(!outcome.material.is_empty(), "the producer keeps actual executed family material");
            let capture =
                kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(&commitment.job, prompt, &outcome.material);
            let claim = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment);
            let path = crate::palw_producer::palw_retained_material_path(&producer_config.retention_dir, &claim);
            std::fs::write(path, capture).unwrap();
        }
        let config = Arc::new(chain.config.clone());
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
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn tick(
        &mut self,
        chain: &Chain,
        claim: Hash64,
        service_start: usize,
        service: &mut Arc<PalwPanelService>,
        verifier: &mut ServiceRun,
        answer_submitted: &mut Option<kaspa_consensus_core::tx::TransactionId>,
        job_preimage_submitted: &mut bool,
        input_chunks: &mut BTreeSet<u32>,
    ) {
        assert!(!self.producer.task.as_ref().unwrap().is_finished(), "automatic producer worker stopped");
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
            self.producer = launch(self.producer_config.clone(), self.manager.clone(), self.flow.clone(), self.config.clone()).await;
            self.producer_restarted = true;
            eprintln!("[service-recovery] producer restarted from accepted partial manifest at DAA {}", chain.daa());
        }
        let waiting_for_binding = view.sessions.iter().any(|(_, s)| s.units.contains(&PalwDaUnitV1::Event { row: 0, tile: 0 }));
        if !self.verifier_restarted && (partial || (!self.honest && waiting_for_binding)) {
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

    pub(super) async fn finish(&mut self, chain: &Chain, claim: Hash64, service_start: usize) {
        assert!(self.verifier_restarted, "the test must actually restart the public service while its case is live");
        if self.honest {
            assert!(self.producer_restarted, "the producer must restart during accepted partial multipart transport");
            let mut declarations = 0;
            let mut chunks = BTreeSet::new();
            for tx in chain.history[service_start..].iter().flat_map(|b| b.transactions.iter()) {
                let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
                match payload.object {
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
        self.producer.finish().await;
    }
}
