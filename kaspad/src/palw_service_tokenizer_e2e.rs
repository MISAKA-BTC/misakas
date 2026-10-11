//! Actual registered A16 service pursuit of false tokenizer length / key-work metadata.
//! Own D7 file operands enter only the verifier; all claim material crosses native carriers.
use super::*;
use kaspa_consensus_core::{
    palw_class_semantics_v1::*,
    palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1, PalwConstraintStreamActionV1},
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1,
    palw_offence_v1::PalwPanelContradictionV1,
    palw_staged_court_state_v1::PalwStagedOwnedCursorV1,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn registered_a16_false_tokenizer_length_restarts_and_collects_full_panel_through_rooted_court19() {
    fresh_public_service(PublicCase::TiledTokenizerLength).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn registered_a16_understated_tokenizer_key_work_restarts_and_collects_full_panel_through_rooted_court19() {
    fresh_public_service(PublicCase::TiledTokenizerKeyBound).await;
}

pub(super) fn fixture(chain: &Chain) -> tiled::TiledFixture {
    // An ignored JSON field keeps the SAME renderer/table semantics, while >16 packets
    // provide an actual accepted partial raw stream at which to restart an empty worker.
    tiled::TiledFixture::new_with_tokenizer_padding(chain, 1_048_576)
}
pub(super) fn copy_own_holding(fixture: &tiled::TiledFixture, directory: &std::path::Path) -> std::path::PathBuf {
    let directory = directory.join("own-registered-model");
    std::fs::create_dir(&directory).unwrap();
    let own = directory.join(fixture.path.file_name().unwrap());
    std::fs::copy(&fixture.path, &own).unwrap();
    std::fs::copy(fixture.path.parent().unwrap().join("tokenizer.json"), directory.join("tokenizer.json")).unwrap();
    std::fs::copy(
        crate::palw_backends::token_table_sidecar_path_v1(&fixture.path),
        crate::palw_backends::token_table_sidecar_path_v1(&own),
    )
    .unwrap();
    assert_ne!(std::fs::canonicalize(&own).unwrap(), std::fs::canonicalize(&fixture.path).unwrap());
    own
}
fn declared(fixture: &tiled::TiledFixture, case: PublicCase) -> (u64, u64) {
    let path = fixture.path.parent().unwrap().join("tokenizer.json");
    let raw = std::fs::read(path).unwrap();
    let length = raw.len() as u64;
    let keys = kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_key_events_v1(&raw).unwrap();
    match case {
        PublicCase::TiledTokenizerLength => (length - 1, keys),
        PublicCase::TiledTokenizerKeyBound => (length, 1),
        _ => panic!("only the staged tokenizer cases alter declared resource metadata"),
    }
}
pub(super) async fn declare_semantics(fixture: &tiled::TiledFixture, case: PublicCase, chain: &mut Chain, floats: &mut [Funding]) {
    let (raw_tokenizer_bytes, tokenizer_key_events) = declared(fixture, case);
    let declaration = PalwClassSemanticsDeclarationV1 {
        version: PALW_CLASS_SEMANTICS_VERSION_V1,
        class_id: fixture.profile.shape_profile_id(),
        artifact_root: fixture.root,
        model_profile_id: fixture.canonical.model_profile_id,
        runtime_manifest_hash: fixture.canonical.runtime_manifest_hash,
        runtime_class_id: fixture.canonical.runtime_class_id,
        tokenizer_id: fixture.canonical.tokenizer_id,
        cu_ruleset_id: fixture.canonical.cu_ruleset_id,
        token_table_root: fixture.table.root(),
        rendering_rule: kaspa_consensus_core::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        raw_tokenizer_bytes,
        tokenizer_key_events,
        prefix_state_relation: Some(PalwPrefixStateRelationV1::DenseInt32KvV1),
    };
    let message = palw_class_semantics_message_v1(chain.network_domain, &declaration);
    let object = PalwConsensusObjectV2::ClassSemanticsDeclaredV1 {
        declaration: Box::new(declaration),
        signature: signed(0, message.as_byte_slice(), PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1),
    };
    chain.send(0, &object, &mut floats[0], &[]).await;
}

pub(super) struct Monitor {
    signed_length: u64,
    signed_keys: u64,
    actual_length: u64,
    seed: Option<(Hash64, u64)>,
    maximum_offset: u64,
    sealed: bool,
    completed: bool,
    restarted: bool,
}
impl Monitor {
    pub(super) fn new(case: PublicCase, fixture: &tiled::TiledFixture) -> Self {
        let (signed_length, signed_keys) = declared(fixture, case);
        Self {
            signed_length,
            signed_keys,
            actual_length: std::fs::metadata(fixture.path.parent().unwrap().join("tokenizer.json")).unwrap().len(),
            seed: None,
            maximum_offset: 0,
            sealed: false,
            completed: false,
            restarted: false,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn observe_and_restart(
        &mut self,
        chain: &Chain,
        claim: Hash64,
        owner: PalwBondKeyV2,
        manager: &Arc<ConsensusManager>,
        flow: &Arc<FlowContext>,
        service: &mut Arc<PalwPanelService>,
        running: &mut ServiceRun,
    ) {
        let Some(progress) = chain.tc.palw_staged_owned_progress_v1((claim, owner)).unwrap() else { return };
        assert_eq!((progress.claim, progress.reserver), (claim, owner));
        assert!(progress.now_daa <= chain.daa(), "the monitor uses an accepted native snapshot clock");
        let Some(PalwStagedOwnedCursorV1::Tokenizer { offset, tokenizer_sealed, completed, .. }) = progress.cursor else { return };
        let accepted_seed = (progress.seed_id.expect("an owned row pins its literal signed seed"), progress.reserved_daa);
        if let Some(previous) = self.seed {
            assert_eq!(accepted_seed, previous, "restart resumes the same ordinary reservation and signed seed");
        }
        self.seed = Some(accepted_seed);
        self.maximum_offset = self.maximum_offset.max(offset);
        self.sealed |= tokenizer_sealed;
        self.completed |= completed == Some(true);
        assert_ne!(completed, Some(false), "false immutable tokenizer metadata must never be settled Honest");
        if !self.restarted && offset > 0 && offset < self.actual_length && !tokenizer_sealed {
            let panel_config = service.config.clone();
            running.finish().await;
            // The inbox belongs to one service. A real restart rebuilds FlowContext, retaining
            // only public consensus, pool, peer hub and own D7 holding paths / operator wallet.
            let config = Arc::new(chain.config.clone());
            let tick = Arc::new(TickService::new());
            let hub = flow.hub().clone();
            let rules = Arc::new(MiningRuleEngine::new(
                manager.clone(),
                config.clone(),
                Default::default(),
                tick.clone(),
                hub.clone(),
                Default::default(),
                Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
            ));
            let fresh_flow = Arc::new(FlowContext::new(
                manager.clone(),
                flow.address_manager.clone(),
                config.clone(),
                flow.mining_manager().clone(),
                tick,
                chain.tc.notification_root(),
                hub,
                rules,
            ));
            let restarted = Arc::new(PalwPanelService::new(panel_config, manager.clone(), fresh_flow, config));
            assert!(restarted.fraud_filer_honest_replays.lock().unwrap().is_empty());
            assert_eq!(restarted.bond, Some(owner.0));
            assert_eq!(restarted.config.public_claims, vec![claim]);
            let cloned = restarted.clone();
            *running =
                ServiceRun { service: restarted.clone(), task: Some(tokio::spawn(async move { cloned.start().await.unwrap() })) };
            *service = restarted;
            self.restarted = true;
            eprintln!("[service-tokenizer] restarted empty verifier at accepted raw offset {offset}/{}", self.actual_length);
        }
    }
    pub(super) fn assert_conviction(
        &self,
        chain: &Chain,
        claim: Hash64,
        owner: PalwBondKeyV2,
        evidence: &PalwExecutorRefutedEvidenceV1,
        carriers: &[PalwConsensusObjectV2],
    ) {
        assert!(self.restarted, "the actual public verifier restarts during accepted partial raw transport");
        assert!(self.sealed && self.completed, "native snapshots must show authenticated EOF and completed fault");
        assert_eq!(self.maximum_offset, self.actual_length);
        let PalwPanelContradictionV1::StagedCourt { reference } = &evidence.contradiction else {
            panic!("false immutable tokenizer metadata must reach the rooted kind19 court")
        };
        assert_eq!((reference.claim, reference.reserver), (claim, owner));
        assert_eq!((reference.seed_id, reference.reserved_daa), self.seed.unwrap());
        let seeds: Vec<_> = carriers
            .iter()
            .filter_map(|o| match o {
                PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed } if seed.claim == claim && seed.reserver == owner => {
                    Some(seed)
                }
                _ => None,
            })
            .collect();
        assert_eq!(seeds.len(), 1, "restart does not reopen or duplicate the accepted owned court");
        let seed = seeds[0];
        assert_eq!(Hash64::from_bytes(seed.signing_message().try_into().unwrap()), reference.seed_id);
        let Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
            raw_tokenizer_bytes, tokenizer_key_events, operand_tokenizer_bytes, ..
        }) = &seed.auxiliary
        else {
            panic!("native seed is specifically tokenizer-owned")
        };
        assert_eq!(
            (*raw_tokenizer_bytes, *tokenizer_key_events, *operand_tokenizer_bytes),
            (self.signed_length, self.signed_keys, self.actual_length)
        );
        let view = chain.view(claim);
        assert_eq!(seed.binding.committed_execution_root, view.execution_root);
        let job = view.public_fp_job.as_ref().expect("the actual retained accepted original job survives restart");
        let job = borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>(job).unwrap();
        assert_eq!((job.execution_root, job.trace_root), (seed.binding.committed_execution_root, seed.binding.full_logits_trace_root));
        let immutable = view.registered_semantics.as_ref().unwrap();
        assert_eq!(
            (immutable.raw_tokenizer_bytes, immutable.tokenizer_key_events),
            (Some(self.signed_length), Some(self.signed_keys))
        );
        assert_eq!(job.job.tokenizer_id, immutable.tokenizer_id);
        let mut packets = 0;
        let mut seals = 0;
        let mut rendering = 0;
        for carrier in carriers {
            if let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = carrier
                && movement.claim == claim
                && movement.reserver == owner
            {
                assert_eq!(movement.phase, PalwConstraintStreamPhaseV1::Tokenizer);
                movement.check().unwrap();
                match &movement.action {
                    PalwConstraintStreamActionV1::TokenizerPacket(packet) => {
                        assert!(packet.bytes.len() <= 65536 && packet.proofs.len() <= 64);
                        assert!(borsh::object_length(packet).unwrap() <= 90000);
                        packets += 1;
                    }
                    PalwConstraintStreamActionV1::TokenizerSeal => seals += 1,
                    PalwConstraintStreamActionV1::TokenizerRenderingPart { .. } => rendering += 1,
                    _ => panic!("only the specific owned tokenizer continuation is included"),
                }
            }
        }
        assert!(packets > 16 && rendering > 0);
        assert_eq!(seals, 1);
        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. } if reservation.claim == claim && reservation.reserver == owner)));
    }
}
