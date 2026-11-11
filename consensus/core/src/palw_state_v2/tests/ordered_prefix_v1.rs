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
