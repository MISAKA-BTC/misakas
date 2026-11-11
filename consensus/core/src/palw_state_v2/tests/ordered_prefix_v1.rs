//! Full-prefix rehearsal must retain the canonical journal, not concatenate
//! singleton deltas or run GC between candidates. These are ledger fixtures,
//! not signed public admission or a completed T12 claim.
use super::*;
fn replay_rules() -> PalwOrderedReplayRulesV1 {
    PalwOrderedReplayRulesV1 { unavailable_abstains: false, capability_bound: false, uncertified_weightless: false, da_court: false }
}

#[test]
fn ordered_prefix_matches_actual_journal_after_preparation_and_retry() {
    let p = economy_params();
    let mut parent =
        apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 10, 1), &[registration(h64(1), 1000, None)], None)
            .unwrap()
            .0;
    let paid = h64(0xface);
    parent.pending_payouts.insert(paid, PalwPayoutV2 { payload: h64(9), amount: 7 });
    let original_root = parent.state_root();
    let point = ctx(2, 11, 2);
    let extras = PalwTransitionExtrasV1::default();
    let first = registration(h64(2), 1, Some(bond_key(1)));
    let second = registration(h64(3), 1, Some(bond_key(1)));
    let rejected = [first.clone(), first.clone(), second.clone()];
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(
        &parent,
        &p,
        point,
        replay_rules(),
        PalwOrderedReplayLimitsV1 { passes: 2, candidate_visits: 5 },
    )
    .unwrap();
    fn run<'a>(
        budget: &mut PalwOrderedReplayBudgetV1<'a>,
        extras: &'a PalwTransitionExtrasV1,
        objects: &[PalwConsensusObjectV2],
    ) -> Result<PalwOrderedPrefixV1<'a>, PalwOrderedReplayFailureV1> {
        palw_v2_replay_accepted_prefix_v1(budget, objects, PalwBlockWorkV3::None, &[], Hash64::default(), extras)
    }
    let rejected_pass = run(&mut budget, &extras, &rejected).err().unwrap();
    assert!(matches!(rejected_pass.error, PalwStateV2Error::DuplicateClass(id) if id == h64(2)));
    assert_eq!(rejected_pass.location, PalwOrderedReplayLocationV1::Ordinary { object_index: 1 });
    assert_eq!(rejected_pass.ordinary_refusal_index_v1(), Some(1));
    assert_eq!(budget.remaining_v1(), PalwOrderedReplayLimitsV1 { passes: 1, candidate_visits: 2 });
    assert_eq!(parent.state_root(), original_root, "a failed candidate returns no partial owner");
    assert!(parent.pending_payouts.contains_key(&paid));

    // Filtering is outside this API. Retry from the parent with the complete
    // corrected list, rather than retaining the first failed pass's prefix.
    let accepted = [first, second];
    let replay = run(&mut budget, &extras, &accepted).unwrap();
    assert_eq!(budget.remaining_v1(), PalwOrderedReplayLimitsV1 { passes: 0, candidate_visits: 0 });
    let exhausted = run(&mut budget, &extras, &[]).err().unwrap();
    assert_eq!(exhausted.location, PalwOrderedReplayLocationV1::Budget);
    assert!(matches!(exhausted.error, PalwStateV2Error::NativeResource(_)));
    assert_eq!(exhausted.ordinary_refusal_index_v1(), None);
    assert!(!replay.state_v1().pending_payouts.contains_key(&paid));
    assert_eq!(replay.state_v1().registration_exposure(&bond_key(1)), 2_000);
    assert_eq!(replay.state_v1().last_point(), parent.last_point());
    assert!(!replay.builder.entries.iter().any(|entry| matches!(entry, PalwDeltaEntryV2::LastPoint { .. })));
    assert_eq!(replay.builder.tfp1_history_journal_parent, None);
    assert!(matches!(replay.builder.entries.first(), Some(PalwDeltaEntryV2::Payout { key, old: Some(_), new: None }) if *key == paid));
    let prefix_bytes = borsh::to_vec(&replay.builder.entries).unwrap();
    let end = replay.journal_len_v1();
    assert!(end > 2);
    let (state, delta, _) = apply_palw_transition_retained_v7(
        &parent,
        &p,
        None,
        &point,
        &accepted,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .unwrap();
    assert_eq!(borsh::to_vec(&delta.entries[..end]).unwrap(), prefix_bytes);
    assert!(
        delta.entries[end..].iter().any(|entry| matches!(entry, PalwDeltaEntryV2::LastPoint { new: Some(at), .. } if *at == point))
    );
    assert_eq!(state.registration_exposure(&bond_key(1)), 2_000);
    assert_eq!(parent.state_root(), original_root);
}

#[test]
fn ordered_history_replay_refuses_template_and_nonmonotonic_points() {
    let p = params();
    let parent = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 10, 1), &[registration(h64(1), 1000, None)], None)
        .unwrap()
        .0;
    let extras = PalwTransitionExtrasV1::default();
    let limits = PalwOrderedReplayLimitsV1 { passes: 1, candidate_visits: 0 };
    for block in [crate::blockhash::ORIGIN, Hash64::default()] {
        let point = PalwBlockContextV2 { block, ..ctx(2, 11, 2) };
        assert!(matches!(
            PalwOrderedReplayBudgetV1::new_v1(&parent, &p, point, replay_rules(), limits),
            Err(PalwStateV2Error::CarriageInconsistent(_))
        ));
    }
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(&parent, &p, *parent.last_point().unwrap(), replay_rules(), limits).unwrap();
    let failure =
        palw_v2_replay_accepted_prefix_v1(&mut budget, &[], PalwBlockWorkV3::None, &[], Hash64::default(), &extras).err().unwrap();
    assert_eq!(failure.location, PalwOrderedReplayLocationV1::Preparation);
    assert!(matches!(failure.error, PalwStateV2Error::NonMonotonicContext(_)));
    assert_eq!(budget.remaining_v1().passes, 0, "failed preparation is still an attempted pass");
}

#[test]
fn ordered_visit_exhaustion_is_local_and_precedes_candidate_application() {
    let p = params();
    let parent = PalwChainStateV2::genesis();
    let point = ctx(1, 10, 1);
    let extras = PalwTransitionExtrasV1::default();
    let limits = PalwOrderedReplayLimitsV1 { passes: 2, candidate_visits: 0 };
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(&parent, &p, point, replay_rules(), limits).unwrap();
    let objects = [registration(h64(1), 1000, None)];
    let failure = palw_v2_replay_accepted_prefix_v1(&mut budget, &objects, PalwBlockWorkV3::None, &[], Hash64::default(), &extras)
        .err()
        .unwrap();
    assert_eq!(failure.location, PalwOrderedReplayLocationV1::Budget);
    assert!(matches!(failure.error, PalwStateV2Error::NativeResource(_)));
    assert_eq!(failure.ordinary_refusal_index_v1(), None);
    assert_eq!(budget.remaining_v1(), limits, "a refused precharge changes neither counter");
    assert!(parent.classes.is_empty());
    let order_fault = PalwOrderedReplayFailureV1 {
        location: PalwOrderedReplayLocationV1::Ordinary { object_index: 0 },
        error: PalwStateV2Error::CarriageInconsistent("fixture: mismatched native carrier".into()),
    };
    assert_eq!(order_fault.ordinary_refusal_index_v1(), None);
    let unavailable = PalwStateV2Error::PublicOriginalProfileUnavailable { claim: h64(19) };
    assert_eq!(
        PalwOrderedReplayFailureV1 { location: PalwOrderedReplayLocationV1::Ordinary { object_index: 3 }, error: unavailable.clone() }
            .ordinary_refusal_index_v1(),
        Some(3)
    );
    for location in [
        PalwOrderedReplayLocationV1::Preparation,
        PalwOrderedReplayLocationV1::Order,
        PalwOrderedReplayLocationV1::Tokenizer { accepted_tx_ordinal: 3 },
    ] {
        assert_eq!(PalwOrderedReplayFailureV1 { location, error: unavailable.clone() }.ordinary_refusal_index_v1(), None);
    }
    for error in [
        PalwStateV2Error::Overflow("fixture exposure"),
        PalwStateV2Error::InvalidParams("fixture params"),
        PalwStateV2Error::NonMonotonicContext("fixture context"),
    ] {
        assert_eq!(
            PalwOrderedReplayFailureV1 { location: PalwOrderedReplayLocationV1::Ordinary { object_index: 0 }, error }
                .ordinary_refusal_index_v1(),
            None
        );
    }
}

#[test]
fn ordered_node_checks_observe_actual_before_and_after_states_and_discard_failed_pass() {
    use PalwOrderedCandidateFailureV2::{Candidate, Native};
    use PalwOrderedObjectStageV2::{After, Before};
    let p = economy_params();
    let parent = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 10, 1), &[registration(h64(1), 1000, None)], None)
        .unwrap()
        .0;
    let parent_root = parent.state_root();
    let point = ctx(2, 11, 2);
    let extras = PalwTransitionExtrasV1::default();
    let objects = [registration(h64(2), 1, Some(bond_key(1))), registration(h64(3), 1, Some(bond_key(1)))];
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(
        &parent,
        &p,
        point,
        replay_rules(),
        PalwOrderedReplayLimitsV1 { passes: 2, candidate_visits: 4 },
    )
    .unwrap();
    let mut seen = Vec::new();
    let failure = palw_v2_replay_accepted_prefix_with_checks_v2(
        &mut budget,
        &objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        &extras,
        &mut |stage, index, state, object| {
            assert_eq!(object, &objects[index]);
            assert_eq!(state.last_point(), parent.last_point());
            seen.push((stage, index));
            if stage == Before {
                assert!(state.class(&h64(2)).is_none());
            } else {
                assert!(state.class(&h64(2)).is_some());
                assert_eq!(state.registration_exposure(&bond_key(1)), 1_000);
                return Err("node refusal after tentative ordinary write");
            }
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert!(matches!(failure, Candidate { object_index: 0, stage: After, error: "node refusal after tentative ordinary write" }));
    assert_eq!(seen, [(Before, 0), (After, 0)], "a refused pass never checks later candidates");
    assert_eq!(budget.remaining_v1(), PalwOrderedReplayLimitsV1 { passes: 1, candidate_visits: 2 });
    assert_eq!(parent.state_root(), parent_root);
    assert!(parent.class(&h64(2)).is_none());

    // A retry re-enters from the same parent. The callback can distinguish
    // application stages without concatenating singleton journals.
    seen.clear();
    let replay = palw_v2_replay_accepted_prefix_with_checks_v2(
        &mut budget,
        &objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        &extras,
        &mut |stage, index, state, _| {
            seen.push((stage, index));
            let applied = index + usize::from(stage == After);
            assert_eq!(state.registration_exposure(&bond_key(1)), applied as u64 * 1_000);
            assert_eq!(state.class(&h64(2)).is_some(), applied >= 1);
            assert_eq!(state.class(&h64(3)).is_some(), applied >= 2);
            Ok::<(), &'static str>(())
        },
    )
    .unwrap();
    assert_eq!(seen, [(Before, 0), (After, 0), (Before, 1), (After, 1)]);
    assert!(replay.journal_len_v1() > 0);
    let partial = replay.into_rehearsal_state_v2();
    assert_eq!(partial.last_point(), parent.last_point(), "handoff is not a completed block transition");
    assert_eq!(partial.registration_exposure(&bond_key(1)), 2_000);
    let calls = seen.len();
    let exhausted = palw_v2_replay_accepted_prefix_with_checks_v2(
        &mut budget,
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        &extras,
        &mut |stage, index, _, _| {
            seen.push((stage, index));
            Ok::<(), &'static str>(())
        },
    )
    .err()
    .unwrap();
    assert!(matches!(exhausted, Native(PalwOrderedReplayFailureV1 { location: PalwOrderedReplayLocationV1::Budget, .. })));
    assert_eq!(seen.len(), calls, "exhaustion precedes preparation and Node gates");
}

#[test]
fn ordered_node_checks_preserve_native_failure_location_and_do_not_run_after_failed_object() {
    use PalwOrderedObjectStageV2::{After, Before};
    let p = params();
    let parent = PalwChainStateV2::genesis();
    let extras = PalwTransitionExtrasV1::default();
    let first = registration(h64(1), 1000, None);
    let objects = [first.clone(), first];
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(
        &parent,
        &p,
        ctx(1, 10, 1),
        replay_rules(),
        PalwOrderedReplayLimitsV1 { passes: 1, candidate_visits: 2 },
    )
    .unwrap();
    let mut seen = Vec::new();
    let failure = palw_v2_replay_accepted_prefix_with_checks_v2(
        &mut budget,
        &objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        &extras,
        &mut |stage, index, _, _| {
            seen.push((stage, index));
            Ok::<(), &'static str>(())
        },
    )
    .err()
    .unwrap();
    assert_eq!(seen, [(Before, 0), (After, 0), (Before, 1)]);
    let PalwOrderedCandidateFailureV2::Native(failure) = failure else { panic!("native failure was reclassified") };
    assert!(matches!(failure.error, PalwStateV2Error::DuplicateClass(id) if id == h64(1)));
    assert_eq!(failure.ordinary_refusal_index_v1(), Some(1));
    assert!(parent.classes.is_empty());
}

#[test]
fn ordered_retries_rebuild_pass_local_extras_without_resetting_budget() {
    let p = params();
    let parent = PalwChainStateV2::genesis();
    let mut budget = PalwOrderedReplayBudgetV1::new_v1(
        &parent,
        &p,
        ctx(1, 10, 1),
        replay_rules(),
        PalwOrderedReplayLimitsV1 { passes: 2, candidate_visits: 2 },
    )
    .unwrap();
    // Each pass owns a distinct extras value. Its lifetime must not be tied
    // to the parent-bound budget or retained across a filtered-list rebuild.
    for class in [h64(1), h64(2)] {
        let extras = PalwTransitionExtrasV1::default();
        let objects = [registration(class, 1000, None)];
        let prefix = palw_v2_replay_accepted_prefix_with_checks_v2(
            &mut budget,
            &objects,
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            &extras,
            &mut |_, _, _, _| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap();
        assert!(prefix.state_v1().class(&class).is_some());
    }
    assert_eq!(budget.remaining_v1(), PalwOrderedReplayLimitsV1 { passes: 0, candidate_visits: 0 });
    assert!(parent.classes.is_empty());
}
