use super::*;
use crate::{
    palw_court_original_escrow_v1::PreparedEscrowFundingV1,
    palw_court_service_state_v2::{CourtServiceStateV2, court_service_profile_id_v2},
    palw_court_service_v1::ServiceReservations,
    palw_funded_escrow_v1::PalwFundedEscrowRowV1,
    palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
    palw_native_resource_v1::NativeRetainedV1,
    palw_positive_contract_v2::{
        PublicOriginalAdmissionV1, PublicOriginalReservationV1, PublicTfp1FrameV1,
        tfp1_original::{
            PreparedTfp1PublicOriginalV1, preclaim::ProtectedBatchBookingV1, service_booking::PreparedProtectedServiceBookingV1,
        },
    },
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_tfp1_original_history_v1::rooted_tfp1_original_history_v1,
    palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{identity::Digest, public_one_honest::OriginalClock};

/// Exercise the real paired checkpoint and storage writers with local class,
/// accepted-input and scheduling fixtures. Full public intake remains separate.
#[allow(clippy::too_many_arguments)]
pub(crate) fn exercise_protected_original_service_writer_v1(
    fixture: &PalwTirPublicFrameFixtureV1,
    source: &PalwTirPublicSourceV1,
    admission: &PublicOriginalAdmissionV1<'_>,
    reservation: &PublicOriginalReservationV1<'_>,
    pairing: &PalwFundedOriginalPairingV1,
    prepared: &PreparedTfp1PublicOriginalV1,
    funded: &PreparedEscrowFundingV1<'_>,
    booking: &ProtectedBatchBookingV1,
    before_book: &ServiceReservations,
    after_book: &NativeRetainedV1<ServiceReservations>,
    at: &PalwBlockContextV2,
) {
    let (mut parent, params, mut extras) = super::super::palw_tir_public_fold_v1::conviction_e2e_tests::world(fixture, &fixture.frame);
    let claim_id = pairing.claim();
    let accepted = pairing.history_inputs_v1().unwrap();
    let reservation = PublicOriginalReservationV1 { origin: accepted.origin_v1(), ..*reservation };
    let reservation_id = Digest([0x51; 64]);
    let prior = PalwBlockContextV2 {
        block: accepted.parent_v1(),
        daa_score: at.daa_score.checked_sub(1).unwrap(),
        blue_score: at.blue_score.checked_sub(1).unwrap(),
        subsidy: 0,
    };
    parent.last_point = Some(prior);
    parent.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: prior.daa_score, burned: 0 },
    ));

    // Reuse the native source/class, with a fresh original in this separate
    // fixture. Strip the old claim, panel, collateral and material rows.
    let mut claim = parent.claim(&claim_id).unwrap().clone();
    parent.set_false_valid_rows_for_tests(claim_id, None, None, None, 0);
    parent.reserved_exposure.remove(&claim.bond);
    parent.slashable_locks.retain(|(_, claim), _| *claim != claim_id);
    let legacy_key = Key::TirClaim(claim_id);
    let legacy = parent.staged_courts.remove(&legacy_key).unwrap();
    super::super::palw_staged_court_fold_v1::update_indices(&mut parent, legacy_key, Some(&legacy), None);
    claim.accepted_block = at.block;
    claim.accepted_daa = at.daa_score;
    claim.accepted_blue_score = at.blue_score;
    claim.trace_retention_daa = reservation.budget.retention_until;
    claim.phase = PalwClaimPhaseV2::Provisional;
    claim.rcore = Default::default();

    let source_profile_id = h64(0x5100);
    assert!(before_book.packets.is_empty());
    assert!(
        parent
            .staged_courts
            .insert(
                Key::CourtService,
                Row::CourtService(
                    Box::new(CourtServiceStateV2 {
                        version: 2,
                        profile_id: court_service_profile_id_v2(source_profile_id),
                        source_profile_id,
                        book: before_book.clone(),
                        claims: BTreeMap::new(),
                    })
                    .into(),
                ),
            )
            .is_none()
    );

    let terms = &funded.consent().terms;
    let escrow = PalwFundedEscrowRowV1::fresh_v1(
        terms.id_v1(),
        claim_id,
        funded.actual().outpoint(),
        funded.actual().output().value,
        terms.payer_key_id,
        terms.refund_payload,
        at.daa_score,
        OriginalClock::new(at.daa_score).unwrap().hard(),
        true,
    )
    .unwrap();
    let frame_key = Key::PublicTfp1FrameV1(claim_id);
    let parent_root = parent.state_root();
    extras.tir_funded_originals.insert(claim_id, pairing.clone());

    for conviction_active in [false, true] {
        extras.tir_public_conviction_active = conviction_active;
        let mut builder = TransitionBuilder::try_new(&parent, &params, false, false, false, false, &extras).unwrap();
        builder.tfp1_history_journal_parent = Some(prior.block);
        assert!(builder.native_writers_tracking_v1());

        let mut reached_service = false;
        let result = super::super::objects::apply_with_native_checkpoint_v1(&mut builder, Some(claim_id), false, |builder| {
            let prefix = super::super::objects::capture_tfp1_prefix_v1(builder, at, claim_id)?;
            assert_eq!(prefix.root_v1(), parent_root);
            assert_eq!(prefix.end_v1(), 0);
            builder.tfp1_original_prefix = Some(prefix);
            let history = admission
                .prepare_tfp1_history_v1(at, pairing, prepared.obligation_v1(), &reservation, reservation_id, &prefix)
                .unwrap();
            let frame = PublicTfp1FrameV1::prepare_v1(
                prepared,
                fixture.frame.original_v1(),
                source,
                &fixture.record,
                at,
                reservation_id,
                reservation.budget.retention_until,
                fixture.rules.maximum_leaves,
            )
            .unwrap();
            let service =
                PreparedProtectedServiceBookingV1::prepare_v1(&builder.state, &frame, booking, after_book.try_clone_v1().unwrap())
                    .unwrap();
            builder.reserve_for_claim(&claim)?;
            builder.write_claim(claim_id, Some(claim.clone()))?;
            builder.install_funded_escrow_v1(escrow)?;
            builder.install_public_tfp1_original_frame_v1(at, frame, history)?;
            assert_eq!(builder.state.last_point(), Some(&prior));
            assert_eq!(builder.paired_original_checkpoint_claim, Some(claim_id));
            assert_eq!(builder.state.funded_escrows_v1().get(&escrow.contract_id), Some(&escrow));
            reached_service = true;
            builder.install_public_tfp1_service_booking_v1(at, service)
        });
        assert!(reached_service, "claim, escrow, frame and history must be written before attempting the service row: {result:?}");
        assert!(builder.paired_original_checkpoint_claim.is_none());
        assert!(builder.tfp1_original_prefix.is_none());

        if !conviction_active {
            assert!(matches!(
                result,
                Err(PalwStateV2Error::CarriageInconsistent(reason))
                    if reason == "the released court service row is written only past palw_tir_public_conviction_v1"
            ));
            assert_eq!(builder.state, parent);
            assert_eq!(builder.state.state_root(), parent_root);
            assert!(builder.entries.is_empty());
            assert!(builder.state.claim(&claim_id).is_none());
            assert!(builder.state.staged_court_row_v1(&frame_key).is_none());
            assert!(!builder.state.funded_escrows_v1().contains_key(&escrow.contract_id));
            continue;
        }

        result.unwrap();
        assert_eq!(builder.state.last_point(), Some(&prior));
        let service = builder.state.court_service_row_v3_v1().unwrap();
        assert_eq!(&service.state.book, &**after_book);
        assert_eq!(service.state.claims[&claim_id].packet_ids.len(), booking.graph_v1().len());
        assert!(matches!(
            builder.entries.last(),
            Some(PalwDeltaEntryV2::StagedCourtSeed {
                key: Key::CourtService,
                old: Some(Row::CourtService(_)),
                new: Some(Row::CourtServiceV3(_))
            })
        ));

        builder.state.last_point = Some(*at);
        builder.entries.push(PalwDeltaEntryV2::LastPoint { old: Some(prior), new: Some(*at) });
        let delta = PalwStateDeltaV2 { point: *at, entries: builder.entries };
        let replayed = apply_delta_v2(&parent, &delta, &params).unwrap();
        assert_eq!(replayed.state_root(), builder.state.state_root());
        assert_eq!(replayed.court_service_row_v3_v1(), builder.state.court_service_row_v3_v1());
        assert_eq!(replayed.funded_escrows_v1().get(&escrow.contract_id), Some(&escrow));
        let history = rooted_tfp1_original_history_v1(&replayed, claim_id).unwrap().unwrap();
        assert_eq!(history.origin, reservation.origin);
        assert_eq!(history.before_delta_end, 0);
        assert_eq!(history.pre_event_root, Digest(parent_root.as_bytes()));

        replayed.validate_public_verification_rows_v1().unwrap();
        let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&replayed)).unwrap();
        crate::palw_native_resource_v1::preflight_native_carriage_v1(&encoded[..], encoded.len() as u64).unwrap();
        let snapshot =
            borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&params, Some(replayed.state_root())).unwrap();
        assert_eq!(snapshot.public_tfp1_frame_v1(claim_id).unwrap(), replayed.public_tfp1_frame_v1(claim_id).unwrap());
        assert_eq!(snapshot.court_service_row_v3_v1(), replayed.court_service_row_v3_v1());
        assert_eq!(admission.restore_tfp1_pairing_v1(&snapshot, pairing, &reservation).unwrap(), *prepared);

        let frame_index = delta
            .entries
            .iter()
            .position(|entry| {
                matches!(entry,
                    PalwDeltaEntryV2::StagedCourtSeed { key: Key::PublicTfp1FrameV1(id), .. } if *id == claim_id
                )
            })
            .unwrap();
        let route_less = PalwChainStateV2::genesis();
        route_less.validate_public_verification_delta_v1(&delta.entries, true).unwrap();
        for offset in 0..5 {
            let mut partial = delta.clone();
            partial.entries.remove(frame_index + offset);
            assert!(apply_delta_v2(&parent, &partial, &params).is_err(), "partial original offset {offset}");
            assert!(revert_delta_v2(&replayed, &partial, &params).is_err(), "partial undo offset {offset}");
            assert!(route_less.validate_public_verification_delta_v1(&partial.entries, true).is_err());
        }
        let mut own_prefix = delta.entries.clone();
        let PalwDeltaEntryV2::KernelRouteRow { key, new: Some(bytes), .. } = &mut own_prefix[frame_index + 3] else {
            panic!("fourth original insertion must be history");
        };
        let mut changed = crate::palw_tfp1_original_history_v1::Tfp1OriginalHistoryBoundaryV1::decode_row(key, bytes).unwrap();
        changed.before_delta_end = u64::try_from(frame_index + 1).unwrap();
        *bytes = borsh::to_vec(&changed).unwrap();
        assert!(route_less.validate_public_verification_delta_v1(&own_prefix, true).is_err());

        // Service-only changes must recheck the reverse frame dependency,
        // including entirely absent, structurally valid empty, and foreign
        // scope rows. Use the direct validator to isolate this requirement.
        let prior_service = replayed.staged_court_row_v1(&Key::CourtService).unwrap();
        let mut empty = replayed.court_service_row_v3_v1().unwrap().clone();
        empty.state.book = before_book.clone();
        empty.state.claims.clear();
        empty.causal.clear();
        empty.check().unwrap();
        let mut foreign = replayed.court_service_row_v3_v1().unwrap().clone();
        foreign.state.claims.get_mut(&claim_id).unwrap().source_id = h64(0x5151);
        foreign.check().unwrap();
        let mut future = replayed.court_service_row_v3_v1().unwrap().clone();
        future.state.book.last_observed_daa = Some(at.daa_score + 1);
        for replacement in [
            None,
            Some(Row::CourtServiceV3(Box::new(empty).into())),
            Some(Row::CourtServiceV3(Box::new(foreign).into())),
            Some(Row::CourtServiceV3(Box::new(future).into())),
        ] {
            let mut invalid = replayed.clone();
            match &replacement {
                Some(row) => {
                    invalid.staged_courts.insert(Key::CourtService, row.clone());
                }
                None => {
                    invalid.staged_courts.remove(&Key::CourtService);
                }
            }
            let entries =
                vec![PalwDeltaEntryV2::StagedCourtSeed { key: Key::CourtService, old: Some(prior_service.clone()), new: replacement }];
            assert!(invalid.validate_public_verification_rows_v1().is_err());
            for undo in [false, true] {
                assert!(invalid.validate_public_verification_delta_v1(&entries, undo).is_err());
            }
            assert!(apply_delta_v2(&replayed, &PalwStateDeltaV2 { point: *at, entries }, &params).is_err());
        }
        let mut orphan = replayed.clone();
        orphan.kernel_route = None;
        assert!(orphan.validate_public_verification_rows_v1().is_err());

        // Existing cross-row dependency tests now start from the complete
        // five-row fixture, so missing service cannot mask their failures.
        let old_claim = replayed.claim(&claim_id).unwrap().clone();
        for shorter_retention in [false, true] {
            let mut changed = old_claim.clone();
            if shorter_retention {
                changed.trace_retention_daa = reservation.budget.retention_until - 1;
            } else {
                changed.execution_root = h64(901);
            }
            let entries = vec![PalwDeltaEntryV2::Claim { key: claim_id, old: Some(old_claim.clone()), new: Some(changed.clone()) }];
            let mut invalid = replayed.clone();
            invalid.claims.insert(claim_id, changed);
            for undo in [false, true] {
                assert!(invalid.validate_public_verification_delta_v1(&entries, undo).is_err());
            }
            assert!(apply_delta_v2(&replayed, &PalwStateDeltaV2 { point: *at, entries }, &params).is_err());
        }
        let entries = vec![PalwDeltaEntryV2::StagedCourtSeed { key: legacy_key, old: None, new: Some(legacy.clone()) }];
        let mut invalid = replayed.clone();
        invalid.staged_courts.insert(legacy_key, legacy.clone());
        assert!(invalid.validate_public_verification_delta_v1(&entries, false).is_err());
        assert!(apply_delta_v2(&replayed, &PalwStateDeltaV2 { point: *at, entries }, &params).is_err());
        let class_id = old_claim.class_id;
        let previous = replayed.tir_class_v1(&class_id).unwrap().clone();
        let mut changed = previous.clone();
        changed.graph_ir_root = h64(902);
        let entries = vec![PalwDeltaEntryV2::TirClass { key: class_id, old: Some(previous), new: Some(changed.clone()) }];
        let mut invalid = replayed.clone();
        invalid.tir_classes.insert(class_id, changed);
        assert!(invalid.validate_public_verification_delta_v1(&entries, false).is_err());
        assert!(apply_delta_v2(&replayed, &PalwStateDeltaV2 { point: *at, entries }, &params).is_err());

        // The fifth row is the admission append, not necessarily the last
        // service write in its block: the native observation follows it.
        let mut observed = replayed.court_service_row_v3_v1().unwrap().clone();
        observed.state.book.observe_block(at.daa_score, at.block.as_bytes(), Default::default()).unwrap();
        let observation = PalwDeltaEntryV2::StagedCourtSeed {
            key: Key::CourtService,
            old: Some(prior_service.clone()),
            new: Some(Row::CourtServiceV3(Box::new(observed.clone()).into())),
        };
        let mut with_observation = delta.clone();
        with_observation.entries.insert(with_observation.entries.len() - 1, observation);
        let observed_state = apply_delta_v2(&parent, &with_observation, &params).unwrap();
        assert_eq!(observed_state.court_service_row_v3_v1(), Some(&observed));
        assert_eq!(revert_delta_v2(&observed_state, &with_observation, &params).unwrap().state_root(), parent_root);

        let restored = revert_delta_v2(&replayed, &delta, &params).unwrap();
        assert_eq!(restored.state_root(), parent_root);
        assert_eq!(restored.staged_court_row_v1(&Key::CourtService), parent.staged_court_row_v1(&Key::CourtService));
        assert!(restored.claim(&claim_id).is_none());
        assert!(restored.staged_court_row_v1(&frame_key).is_none());
        assert!(!restored.funded_escrows_v1().contains_key(&escrow.contract_id));
    }
}
