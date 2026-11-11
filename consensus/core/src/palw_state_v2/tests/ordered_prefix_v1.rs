//! Full-prefix rehearsal must retain the canonical journal, not concatenate
//! singleton deltas or run GC between candidates. These are ledger fixtures,
//! not signed public admission or a completed T12 claim.
use super::*;

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
    let run = |objects: &[PalwConsensusObjectV2]| {
        palw_v2_replay_accepted_prefix_v1(
            &parent,
            &p,
            &point,
            objects,
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        )
    };
    assert!(matches!(run(&rejected), Err(PalwStateV2Error::DuplicateClass(id)) if id == h64(2)));
    assert_eq!(parent.state_root(), original_root, "a failed candidate returns no partial owner");
    assert!(parent.pending_payouts.contains_key(&paid));

    // Filtering is outside this API. Retry from the parent with the complete
    // corrected list, rather than retaining the first failed pass's prefix.
    let accepted = [first, second];
    let replay = run(&accepted).unwrap();
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
    for block in [crate::blockhash::ORIGIN, Hash64::default()] {
        let point = PalwBlockContextV2 { block, ..ctx(2, 11, 2) };
        assert!(matches!(
            palw_v2_replay_accepted_prefix_v1(
                &parent,
                &p,
                &point,
                &[],
                PalwBlockWorkV3::None,
                &[],
                Hash64::default(),
                false,
                false,
                false,
                false,
                &extras,
            ),
            Err(PalwStateV2Error::CarriageInconsistent(_))
        ));
    }
    assert!(matches!(
        palw_v2_replay_accepted_prefix_v1(
            &parent,
            &p,
            parent.last_point().unwrap(),
            &[],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        ),
        Err(PalwStateV2Error::NonMonotonicContext(_))
    ));
}
