//! Actual native PublicPrefix owner/custodian workers. Only the producer receives its
//! executed capture; the independent verifier consumes signed public carrier transitions.
use super::*;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1, PalwConstraintStreamActionV1},
    palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1,
    palw_offence_v1::PalwPanelContradictionV1,
    palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1,
    palw_staged_court_state_v1::PalwStagedOwnedCursorV1,
};

type Foreign = (
    kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
    kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
    Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
);

pub(super) fn assert_no_producer_capture(directory: &std::path::Path, claim: Hash64) {
    let capture = crate::palw_producer::palw_retained_material_path(directory, &claim);
    for path in [&capture, &capture.with_extension("material.partial")] {
        assert!(
            !path.try_exists().expect("producer capture absence can be inspected"),
            "outside verifier never receives producer capture: {}",
            path.display()
        );
    }
}

#[test]
fn native_prefix_retention_guard_accepts_public_pages_but_refuses_capture() {
    let directory = tempfile::tempdir().unwrap();
    let claim = Hash64::from_u64_word(0xF1);
    let public_pages = directory.path().join("public-prefix-owned");
    std::fs::create_dir_all(&public_pages).unwrap();
    std::fs::write(public_pages.join("authenticated-page"), b"public authenticated page").unwrap();
    assert_no_producer_capture(directory.path(), claim);

    let capture = crate::palw_producer::palw_retained_material_path(directory.path(), &claim);
    for path in [&capture, &capture.with_extension("material.partial")] {
        std::fs::write(path, b"private producer capture").unwrap();
        assert!(std::panic::catch_unwind(|| assert_no_producer_capture(directory.path(), claim)).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

pub(super) struct Producer {
    _directory: tempfile::TempDir,
    running: ServiceRun,
}
impl Producer {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start(
        chain: &Chain,
        claim: Hash64,
        fixture: &tiled::TiledFixture,
        foreign: &Foreign,
        funding: &Funding,
        manager: Arc<ConsensusManager>,
        flow: Arc<FlowContext>,
    ) -> Self {
        let (_, _, Some((commitment, ids))) = foreign else { panic!("native prefix fixture requires actual accepted FP job") };
        assert!(commitment.job.is_prefix_state());
        assert_eq!(kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment), claim);
        let backend = fixture.backend(chain);
        let run = backend.execute_free_prompt(&commitment.job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        assert_eq!(
            (run.outcome.execution_root, run.outcome.trace_root, run.outcome.output_root),
            (commitment.execution_root, commitment.trace_root, commitment.output_root),
            "the producer retains the actual committed original leaves, not a fabricated court witness"
        );
        let directory = tempfile::tempdir().unwrap();
        let mut producer_config = panel_config(chain, directory.path(), chain.bonds[0], funding);
        std::fs::write(&producer_config.key_path, faster_hex::hex_string(&[0xB0; 32])).unwrap();
        producer_config.retention_dir = directory.path().join("producer-retention");
        std::fs::create_dir_all(&producer_config.retention_dir).unwrap();
        producer_config.class_artifacts.push(fixture.path.clone());
        producer_config.chain_classes = true;
        // Targeting does not grant the producer accusation standing against its own claim.
        producer_config.public_claims = vec![claim];
        let capture = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(&commitment.job, ids, &run.outcome.material);
        std::fs::write(crate::palw_producer::palw_retained_material_path(&producer_config.retention_dir, &claim), capture).unwrap();
        drop(run);
        drop(backend);

        let running =
            recovery::launch(producer_config, manager, flow, Arc::new(chain.config.clone()), chain.tc.notification_root()).await;
        assert_eq!(running.service.bond, Some(chain.bonds[0].0));
        Self { _directory: directory, running }
    }
    pub(super) async fn finish(&mut self) {
        self.running.finish().await;
    }
}

#[derive(Default)]
pub(super) struct Monitor {
    seed: Option<(Hash64, u64)>,
    completed: bool,
    pending_sources: BTreeSet<Hash64>,
    expected_signers: BTreeSet<PalwBondKeyV2>,
}
impl Monitor {
    pub(super) fn new(mut valid_signers: BTreeSet<PalwBondKeyV2>, producer: PalwBondKeyV2) -> Self {
        valid_signers.insert(producer);
        Self { expected_signers: valid_signers, ..Self::default() }
    }
    pub(super) fn observe(&mut self, chain: &Chain, claim: Hash64, owner: PalwBondKeyV2) {
        let Some(progress) = chain.tc.palw_staged_owned_progress_v1((claim, owner)).unwrap() else { return };
        let Some(PalwStagedOwnedCursorV1::PublicPrefix { cursor, pending, completed, .. }) = progress.cursor else { return };
        let seed = (progress.seed_id.expect("native prefix owns a literal accepted seed"), progress.reserved_daa);
        if let Some(previous) = self.seed {
            assert_eq!(seed, previous, "one ordinary reservation never opens a replacement public court");
        }
        self.seed = Some(seed);
        if let Some(pending) = pending.as_ref() {
            self.pending_sources.insert(pending.demand_id);
            assert!(pending.source.byte_length > 0);
            let mut expected = self.expected_signers.clone();
            expected.insert(owner);
            assert_eq!(
                pending.signers.keys().copied().collect::<BTreeSet<_>>(),
                expected,
                "the owner, producer and every literal Valid custodian are exactly the eligible sources"
            );
        }
        // A protected acquisition in progress reports Ok(None) from
        // certificate(), so `Some(false)` is not a terminal Honest verdict.
        if cursor == PalwPrefixFamilyCursorV1::Complete && pending.is_none() {
            assert_ne!(completed, Some(false), "the deliberately false named prefix cannot finish Honest");
        }
        if completed == Some(true) {
            assert_eq!(cursor, PalwPrefixFamilyCursorV1::Complete);
            self.completed = true;
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
        assert!(self.completed, "a native snapshot reached authenticated public family completion before conviction");
        assert!(!self.pending_sources.is_empty(), "original leaves were requested through the public acquisition court");
        let PalwPanelContradictionV1::StagedCourt { reference } = &evidence.contradiction else {
            panic!("native public prefix must convict through the rooted staged certificate, never legacy court17")
        };
        assert_eq!((reference.claim, reference.reserver), (claim, owner));
        assert_eq!((reference.seed_id, reference.reserved_daa), self.seed.unwrap());
        let seeds: Vec<_> = carriers
            .iter()
            .filter_map(|object| match object {
                PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed } if (seed.claim, seed.reserver) == (claim, owner) => {
                    Some(seed)
                }
                _ => None,
            })
            .collect();
        assert_eq!(seeds.len(), 1);
        let seed = seeds[0];
        assert_eq!(Hash64::from_bytes(seed.signing_message().try_into().unwrap()), reference.seed_id);
        assert!(matches!(seed.auxiliary, Some(PalwAuxiliaryPublicCourtSeedV1::PublicPrefix { slots: 7, prefix_tokens: 2, .. })));
        let mut demands = 0;
        let mut answers = 0;
        let mut bundle_controls = 0;
        for object in carriers {
            if let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object
                && (movement.claim, movement.reserver) == (claim, owner)
            {
                movement.check().unwrap();
                if matches!(
                    movement.action,
                    PalwConstraintStreamActionV1::BundleOpen { .. }
                        | PalwConstraintStreamActionV1::ActivateStage { .. }
                        | PalwConstraintStreamActionV1::InventoryPart { .. }
                        | PalwConstraintStreamActionV1::InventoryFinish
                ) {
                    bundle_controls += 1;
                    continue;
                }
                assert_eq!(movement.seed_id, reference.seed_id, "every prefix continuation names the exact convicted family seed");
                match &movement.action {
                    PalwConstraintStreamActionV1::PrefixPublicDemand(_) => demands += 1,
                    PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) => {
                        assert_eq!(
                            answer.answerer,
                            chain.view(claim).producer,
                            "the actual custodian service answered retained original material"
                        );
                        answers += 1;
                    }
                    PalwConstraintStreamActionV1::Prefix(_) | PalwConstraintStreamActionV1::PrefixBatch(_) => {}
                    _ => panic!("native exact-prefix case must not route through other courts or DA collection"),
                }
            }
        }
        assert!(bundle_controls > 0, "the claim's required-family bundle is carried through its own control seed");
        assert!(demands > 0 && answers >= demands, "signed automatic custodian answers fulfilled every demanded original leaf");
        let original = chain
            .history
            .iter()
            .flat_map(|block| &block.transactions)
            .find_map(|tx| {
                kaspa_consensus_core::palw_history_custody_v1::PalwHistoryFpEnvelopeV1::decode_v1(&tx.payload)
                    .ok()
                    .filter(|envelope| envelope.pledge.claim == claim)
            })
            .expect("the producer carried the authenticated original binding in the accepted HFP1 claim");
        let payload = original.original_fp_v1().unwrap();
        assert_eq!(payload.claim_id(), claim);
        assert_eq!(original.pledge.producer, chain.view(claim).producer);
        assert_eq!(
            original.original_binding,
            kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(
                kaspa_consensus_core::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1::of_binding_v1(&seed.binding)
            ),
            "the public court uses the exact original binding, without a redundant DA disclosure"
        );
        let view = chain.view(claim);
        let state = chain.tc.palw_state_v2_store.read().load_tip(&chain.bundle.state).unwrap().unwrap().1;
        assert!(
            matches!(
                reference.resolve_fault_v1(&state, view.execution_root).unwrap(),
                kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedFaultV1::Prefix(_)
            ),
            "the paid owner certificate remains auditable after ordinary reservation release"
        );
        assert!(
            state.legacy_dispute_reservation_v1(&claim, &owner).is_none(),
            "objective conviction releases the ordinary owner reservation"
        );
    }
}
