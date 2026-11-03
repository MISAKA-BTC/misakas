//! Consuming replay remains equal to transition-generated states and the borrowed oracle.
use super::*;

fn assert_same(actual: &PalwChainStateV2, expected: &PalwChainStateV2) {
    assert_eq!(actual, expected, "all primary tables and derived indices");
    assert_eq!(actual.state_root(), expected.state_root());
    assert_eq!(actual.last_point(), expected.last_point());
    assert_eq!(
        borsh::to_vec(&PalwStateCarriageV2::from_state(actual)).unwrap(),
        borsh::to_vec(&PalwStateCarriageV2::from_state(expected)).unwrap(),
    );
}

#[test]
fn owned_delta_replay_transition_states_and_multileg_reorg_match_borrowed() {
    let p = params();
    let mut state = PalwChainStateV2::genesis();
    let steps = [
        (ctx(1, 100, 1), register_class_and_bond()),
        (ctx(2, 101, 2), vec![fp_commit(0xFC, 60, 3)]),
        (
            ctx(3, 102, 3),
            vec![PalwConsensusObjectV2::PanelBound {
                claim: h64(0xFC),
                anchor: h64(77),
                seats: vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(21) }],
            }],
        ),
        (ctx(4, 103, 4), vec![PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }]),
        (ctx(5, 124, 5), Vec::new()),
    ];
    let mut history = Vec::new();
    for (point, objects) in steps {
        let (child, delta) = apply(&state, &p, &point, &objects, None);
        let replayed = replay_delta_owned_v1(state.clone(), &delta, &p, false).unwrap();
        assert_same(&replayed, &child);
        assert_same(&replayed, &apply_delta_v2(&state, &delta, &p).unwrap());
        history.push((state, child.clone(), delta));
        state = child;
    }
    let fork = state.clone();
    let mut branches = Vec::new();
    for (a, b) in [(0xA1, 0xA2), (0xB1, 0xB2)] {
        let (one, d1) = apply(&fork, &p, &ctx(a, 125, 6), &[], None);
        let (two, d2) = apply(&one, &p, &ctx(b, 126, 7), &[], None);
        branches.push((one, two, d1, d2));
    }
    let (a1, a2, da1, da2) = &branches[0];
    let (b1, b2, db1, db2) = &branches[1];
    let mut owned = a2.clone();
    for (delta, revert, expected) in [(da2, true, a1), (da1, true, &fork), (db1, false, b1), (db2, false, b2)] {
        let oracle = if revert { revert_delta_v2(&owned, delta, &p) } else { apply_delta_v2(&owned, delta, &p) }.unwrap();
        owned = replay_delta_owned_v1(owned, delta, &p, revert).unwrap();
        assert_same(&owned, &oracle);
        assert_same(&owned, expected);
    }
    for (parent, child, delta) in history.iter().rev() {
        assert_same(&replay_delta_owned_v1(child.clone(), delta, &p, true).unwrap(), parent);
    }
}

#[test]
fn owned_delta_replay_late_mismatch_and_index_failure_preserve_borrowed_input() {
    let p = params();
    let source = fp_state_with_claim(&p);
    let before = source.clone();
    // First row succeeds; the second mismatches. Neither borrowed API exposes a half replay.
    let point = ctx(0xE1, 105, 5);
    let bad = PalwStateDeltaV2 {
        point,
        entries: vec![
            PalwDeltaEntryV2::LastPoint { old: source.last_point, new: Some(point) },
            PalwDeltaEntryV2::Weights { old: (u128::MAX, u128::MAX), new: (0, 0) },
        ],
    };
    let old_error = apply_delta_v2(&source, &bad, &p).unwrap_err();
    let new_error = replay_delta_owned_v1(source.clone(), &bad, &p, false).unwrap_err();
    assert_eq!(old_error, new_error);
    assert_same(&source, &before);
    // In revert order LastPoint is changed before Weights fails as well.
    let bad_revert = PalwStateDeltaV2 {
        point,
        entries: vec![
            PalwDeltaEntryV2::Weights { old: (0, 0), new: (u128::MAX, u128::MAX) },
            PalwDeltaEntryV2::LastPoint { old: Some(point), new: source.last_point },
        ],
    };
    assert_eq!(
        revert_delta_v2(&source, &bad_revert, &p).unwrap_err(),
        replay_delta_owned_v1(source.clone(), &bad_revert, &p, true).unwrap_err(),
    );
    assert_same(&source, &before);
    // All rows install, but rebuilding a receipt deadline overflows. Keep this fallible phase.
    let original = source.claims.get(&h64(0xFC)).unwrap().clone();
    let mut invalid = original.clone();
    invalid.phase = PalwClaimPhaseV2::PanelBound { bound_daa: u64::MAX };
    let invalid_index =
        PalwStateDeltaV2 { point, entries: vec![PalwDeltaEntryV2::Claim { key: h64(0xFC), old: Some(original), new: Some(invalid) }] };
    let old_error = apply_delta_v2(&source, &invalid_index, &p).unwrap_err();
    assert!(matches!(old_error, PalwStateV2Error::Overflow(_)));
    assert_eq!(old_error, replay_delta_owned_v1(source.clone(), &invalid_index, &p, false).unwrap_err());
    assert_same(&source, &before);
}

#[test]
fn owned_delta_replay_keeps_unmodified_heap_payload_backing() {
    let p = params();
    let mut source = fp_state_with_claim(&p);
    source.fp_public_jobs.insert(h64(0xFC), vec![0x5a; 64 * 1024]);
    let backing = source.fp_public_jobs[&h64(0xFC)].as_ptr();
    let empty = PalwStateDeltaV2 { point: ctx(0xE2, 106, 6), entries: Vec::new() };
    // The borrowed result owns a separate allocation while source stays live.
    let borrowed = apply_delta_v2(&source, &empty, &p).unwrap();
    assert_ne!(backing, borrowed.fp_public_jobs[&h64(0xFC)].as_ptr());
    let owned = replay_delta_owned_v1(source, &empty, &p, false).unwrap();
    assert_eq!(backing, owned.fp_public_jobs[&h64(0xFC)].as_ptr());
    assert_same(&owned, &borrowed);
}
