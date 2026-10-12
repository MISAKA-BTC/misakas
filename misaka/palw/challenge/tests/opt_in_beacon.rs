//! Version separation, immutable signed target binding and finite failure semantics for v4.
use misaka_palw_challenge::beacon::VerifiedWorkBeaconV1;
use misaka_palw_challenge::hash::object_id;
use misaka_palw_challenge::*;

fn context() -> BeaconContextV1 {
    BeaconContextV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        policy: opt_in_source_policy_v4(2, 2, 20, 2, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [3; 64],
        commitment_position: 100,
        challenge_epoch: 0,
        eligible_profiles: [[4; 64]].into_iter().collect(),
        excluded_profiles: [[5; 64]].into_iter().collect(),
        candidate_profile_id: RootV1::Present([5; 64]),
    }
}
fn contract(c: &BeaconContextV1) -> OptInContractV4 {
    OptInContractV4 { context_id: c.id(), terms_id: [6; 64], source_final_by: 200 }
}
fn source(c: &BeaconContextV1, p: u8) -> OptInSourceV4 {
    OptInSourceV4 {
        context_id: c.id(),
        terms_id: [6; 64],
        opted_in_position: 110,
        forfeited_position: None,
        source: SealedSourceV3 {
            source_profile_id: [4; 64],
            attribution: SourceAttributionV1 { producer_id: [p; 64], consumer_id: RootV1::Present([p + 10; 64]) },
            seal: [p + 20; 64],
            seal_position: 105,
            reveal: Some(SealRevealV3 {
                reveal_position: 125,
                salt: [p + 30; 64],
                fate: SourceFateV3::Final(WorkFinalEventV1 {
                    kind: WorkSourceKindV1::RealUsefulWork,
                    source_profile_id: [4; 64],
                    canonical_work_id: [p + 40; 64],
                    execution_commitment: [p + 50; 64],
                    accepted_position: 125,
                    settlement_position: 175,
                    occurrence_index: 0,
                    claim_final: true,
                    da_satisfied: true,
                    validity_independent: true,
                    depends_on_profiles: vec![],
                    final_path: FinalPathV1::PanelIndependent,
                }),
            }),
        },
    }
}
fn locked(c: &BeaconContextV1, t: &OptInContractV4, s: &[OptInSourceV4]) -> VerifiedWorkBeaconV1 {
    match collect_opt_in_work_beacon_v4(c, t, s, 201).unwrap() {
        SealedBeaconStateV3::Locked(b) => b,
        state => panic!("{state:?}"),
    }
}
#[test]
fn contracts_and_versions_do_not_cross_and_input_order_is_canonical() {
    let c = context();
    let t = contract(&c);
    let s = vec![source(&c, 1), source(&c, 2)];
    let b = locked(&c, &t, &s);
    assert_eq!(b.output, locked(&c, &t, &[s[1].clone(), s[0].clone()]).output);
    verify_opt_in_work_beacon_v4(&c, &t, &s, 201, b.beacon()).unwrap();
    let ordinary: Vec<_> = s.iter().map(|s| s.source.clone()).collect();
    assert!(collect_sealed_work_beacon_v3(&c, &ordinary, 201).is_err());
    assert!(collect_attributed_work_beacon_v1(&c, &[], 201).is_err());
    assert!(collect_work_beacon_v1(&c, &[], 201).is_err());
    let mut old = c.clone();
    old.policy = sealed_source_policy_v3(2, 2, 20, 2, 1);
    assert!(collect_opt_in_work_beacon_v4(&old, &t, &s, 201).is_err());
    let SealedBeaconStateV3::Locked(v3) = collect_sealed_work_beacon_v3(&old, &ordinary, 201).unwrap() else { panic!() };
    assert_ne!(v3.output, b.output);
    // The old v3 wire policy and hash domains keep the published pre-refactor accumulator formula.
    let mut acc = object_id(
        b"MISAKA/PALW/WORK-BEACON/SEALED/V3",
        &(old.chain_genesis, old.ruleset_id, old.policy.id(), old.commitment_root, old.challenge_epoch),
    );
    let mut ordered = ordinary.clone();
    ordered.sort_by_key(|s| (s.seal_position, s.seal));
    for (i, s) in ordered.iter().enumerate() {
        let r = s.reveal.as_ref().unwrap();
        let SourceFateV3::Final(e) = &r.fate else { panic!() };
        let item = object_id(
            b"MISAKA/PALW/WORK-BEACON/SEALED/ITEM/V3",
            &(
                s.seal_position,
                s.seal,
                s.attribution.producer_id,
                s.source_profile_id,
                e.canonical_work_id,
                e.execution_commitment,
                r.salt,
            ),
        );
        acc = object_id(b"MISAKA/PALW/WORK-BEACON/SEALED/MIX/V3", &(acc, (i + 1) as u32, item));
    }
    assert_eq!(v3.output, acc);
}
#[test]
fn terms_and_full_context_change_output_and_cannot_be_substituted() {
    let c = context();
    let t = contract(&c);
    let s = vec![source(&c, 1), source(&c, 2)];
    let b = locked(&c, &t, &s);
    let mut changed = t;
    changed.terms_id = [99; 64];
    assert!(collect_opt_in_work_beacon_v4(&c, &changed, &s, 201).is_err());
    let mut facts = s.clone();
    for f in &mut facts {
        f.terms_id = changed.terms_id;
    }
    assert_ne!(locked(&c, &changed, &facts).output, b.output);
    assert!(verify_opt_in_work_beacon_v4(&c, &changed, &facts, 201, b.beacon()).is_err());
    changed = t;
    changed.context_id = [99; 64];
    assert!(collect_opt_in_work_beacon_v4(&c, &changed, &s, 201).is_err());
    let mut other = c.clone();
    other.challenge_epoch += 1;
    let other_t = contract(&other);
    let other_s = vec![source(&other, 1), source(&other, 2)];
    assert_ne!(locked(&other, &other_t, &other_s).output, b.output);
}
#[test]
fn irreversible_forfeit_withholding_and_late_final_veto_instead_of_disappearing() {
    let c = context();
    let t = contract(&c);
    let a = source(&c, 1);
    let mut b = source(&c, 2);
    b.source.reveal = None;
    assert!(matches!(
        collect_opt_in_work_beacon_v4(&c, &t, &[a.clone(), b], 142).unwrap(),
        SealedBeaconStateV3::Vetoed { withheld: 1, .. }
    ));
    b = source(&c, 2);
    b.forfeited_position = Some(160);
    assert!(matches!(
        collect_opt_in_work_beacon_v4(&c, &t, &[a.clone(), b], 201).unwrap(),
        SealedBeaconStateV3::Vetoed { failed: 1, .. }
    ));
    b = source(&c, 2);
    let SourceFateV3::Final(e) = &mut b.source.reveal.as_mut().unwrap().fate else { panic!() };
    e.settlement_position = 201;
    assert!(matches!(collect_opt_in_work_beacon_v4(&c, &t, &[a, b], 203).unwrap(), SealedBeaconStateV3::Vetoed { failed: 1, .. }));
}
#[test]
fn live_sources_have_a_finite_deadline_and_invalid_clocks_are_refused() {
    let c = context();
    let t = contract(&c);
    let a = source(&c, 1);
    let mut b = source(&c, 2);
    b.source.reveal.as_mut().unwrap().fate = SourceFateV3::Live;
    assert!(matches!(
        collect_opt_in_work_beacon_v4(&c, &t, &[a.clone(), b.clone()], 200).unwrap(),
        SealedBeaconStateV3::Settling { finals: 1, .. }
    ));
    assert!(matches!(collect_opt_in_work_beacon_v4(&c, &t, &[a, b], 201).unwrap(), SealedBeaconStateV3::Vetoed { failed: 1, .. }));
    for end in [141, u64::MAX - 1, u64::MAX] {
        let bad = OptInContractV4 { source_final_by: end, ..t };
        assert!(collect_opt_in_work_beacon_v4(&c, &bad, &[], 200).is_err());
    }
}
#[test]
fn foreign_attempts_do_not_veto_but_shared_payers_do_not_supply_quorum() {
    let c = context();
    let t = contract(&c);
    let a = source(&c, 1);
    let b = source(&c, 2);
    let output = locked(&c, &t, &[a.clone(), b.clone()]).output;
    let mut foreign = source(&c, 3);
    foreign.context_id = [77; 64];
    foreign.source.reveal = None;
    assert_eq!(locked(&c, &t, &[a.clone(), b.clone(), foreign]).output, output);
    let mut shared = b;
    shared.source.attribution.consumer_id = a.source.attribution.consumer_id;
    assert!(matches!(
        collect_opt_in_work_beacon_v4(&c, &t, &[a, shared], 201).unwrap(),
        SealedBeaconStateV3::Unavailable { mixed: 1, need: 2 }
    ));
}
