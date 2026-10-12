use misaka_palw_challenge::*;

fn fixture() -> (BeaconContextV1, PaidOptInContractV5, Vec<OptInSourceV4>, Vec<PaidSourcePromiseV5>) {
    let context = BeaconContextV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        policy: paid_opt_in_source_policy_v5(2, 2, 20, 2, 1),
        subject_kind: SubjectKindV1::ModelConformance,
        commitment_root: [3; 64],
        commitment_position: 100,
        challenge_epoch: 0,
        eligible_profiles: [[4; 64]].into_iter().collect(),
        excluded_profiles: [[5; 64]].into_iter().collect(),
        candidate_profile_id: RootV1::Present([5; 64]),
    };
    let contract = PaidOptInContractV5 {
        source: OptInContractV4 { context_id: context.id(), terms_id: [6; 64], source_final_by: 200 },
        fee_terms_id: [7; 64],
        minimum_fee: 50,
        fee_per_source: 60,
        max_sources: 2,
        funded_position: 103,
    };
    let sources = [1u8, 2]
        .into_iter()
        .map(|p| OptInSourceV4 {
            context_id: context.id(),
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
        })
        .collect::<Vec<_>>();
    let promises = sources
        .iter()
        .map(|s| PaidSourcePromiseV5 {
            context_id: context.id(),
            producer: s.source.attribution.producer_id,
            fee_terms_id: contract.fee_terms_id,
            accepted_position: s.opted_in_position,
            fee: 60,
        })
        .collect();
    (context, contract, sources, promises)
}

fn locked(
    context: &BeaconContextV1,
    contract: &PaidOptInContractV5,
    sources: &[OptInSourceV4],
    promises: &[PaidSourcePromiseV5],
) -> [u8; 64] {
    let SealedBeaconStateV3::Locked(beacon) = collect_paid_opt_in_work_beacon_v5(context, contract, sources, promises, 201).unwrap()
    else {
        panic!("timely paid sources must lock")
    };
    verify_paid_opt_in_work_beacon_v5(context, contract, sources, promises, 201, beacon.beacon()).unwrap();
    beacon.output
}

#[test]
fn mandatory_fees_are_bound_to_every_mixed_source_and_cannot_be_filtered_away() {
    let (c, t, s, p) = fixture();
    let output = locked(&c, &t, &s, &p);
    assert_eq!(output, locked(&c, &t, &[s[1].clone(), s[0].clone()], &[p[1], p[0]]));
    for fees in [&[][..], &p[..1], &[p[0], p[0]][..]] {
        assert!(collect_paid_opt_in_work_beacon_v5(&c, &t, &s, fees, 201).is_err());
    }
    for mutate in 0..5 {
        let mut fees = p.clone();
        match mutate {
            0 => fees[0].fee = 49,
            1 => fees[0].fee_terms_id = [99; 64],
            2 => fees[0].producer = [99; 64],
            3 => fees[0].accepted_position += 1,
            _ => fees[0].context_id = [99; 64],
        }
        assert!(collect_paid_opt_in_work_beacon_v5(&c, &t, &s, &fees, 201).is_err());
    }
    assert!(collect_paid_opt_in_work_beacon_v5(&c, &t, &s[..1], &p, 201).is_err(), "an orphan fee is not a complete inventory");
}

#[test]
fn paid_policy_and_full_monetary_contract_have_distinct_domains() {
    let (c, t, s, p) = fixture();
    let output = locked(&c, &t, &s, &p);
    assert!(collect_opt_in_work_beacon_v4(&c, &t.source, &s, 201).is_err());
    let mut old = c.clone();
    old.policy = opt_in_source_policy_v4(2, 2, 20, 2, 1);
    assert_ne!(old.id(), c.id());
    assert!(collect_paid_opt_in_work_beacon_v5(&old, &t, &s, &p, 201).is_err());
    let mut changed = t;
    changed.minimum_fee += 1;
    assert_ne!(locked(&c, &changed, &s, &p), output);
    changed = t;
    changed.max_sources += 1;
    assert_ne!(locked(&c, &changed, &s, &p), output);
}

#[test]
fn invalid_funding_quorum_prices_clocks_and_overflow_are_refused() {
    let (c, t, s, p) = fixture();
    for mutate in 0..7 {
        let mut terms = t;
        match mutate {
            0 => terms.minimum_fee = 0,
            1 => terms.minimum_fee = 61,
            2 => terms.max_sources = 1,
            3 => terms.max_sources = 65_537,
            4 => terms.funded_position = 99,
            5 => terms.funded_position = 122,
            _ => terms.fee_per_source = u64::MAX,
        }
        assert!(collect_paid_opt_in_work_beacon_v5(&c, &terms, &s, &p, 201).is_err());
    }
}

#[test]
fn a_paid_but_withheld_or_forfeited_source_still_vetoes() {
    let (c, t, mut s, p) = fixture();
    s[1].source.reveal = None;
    assert!(matches!(collect_paid_opt_in_work_beacon_v5(&c, &t, &s, &p, 201).unwrap(), SealedBeaconStateV3::Vetoed { .. }));
    s[1].forfeited_position = Some(130);
    assert!(matches!(collect_paid_opt_in_work_beacon_v5(&c, &t, &s, &p, 201).unwrap(), SealedBeaconStateV3::Vetoed { .. }));
}
