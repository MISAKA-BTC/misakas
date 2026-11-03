//! Finite preimages/price arithmetic only, not accepted native reservations.
use kaspa_consensus_core::{palw_positive_assignment_resources_v2::*, palw_positive_source_geometry_v2::SourceCatalogQuoteV2};
use misaka_palw_positive_contract::identity::Digest;
fn fees() -> AssignmentCaseFeeScheduleV2 {
    AssignmentCaseFeeScheduleV2 {
        version: 2,
        revision: 1,
        admission: 2,
        per_source_unit: 3,
        per_source_opening_byte: 5,
        per_checked_relation: 7,
        per_record_byte: 11,
        capital_per_retained_byte: 13,
    }
}
#[test]
fn case_quote_matches_independent_wide_integer_components_without_success_or_mint() {
    let f = fees();
    for units in [0, 1, 3] {
        for bytes in [0, 1, 17] {
            for relations in [0, 1, 91] {
                for record in [0, 1, 137] {
                    let source = SourceCatalogQuoteV2 {
                        digest: Digest([9; 64]),
                        units,
                        data_bytes: bytes,
                        opening_bytes: bytes,
                        max_unit_bytes: bytes,
                    };
                    let q = f.quote(&source, relations, record, 23).unwrap();
                    let expected = 2u128
                        + 3u128 * u128::from(units)
                        + 5u128 * u128::from(bytes)
                        + 7u128 * u128::from(relations)
                        + 11u128 * u128::from(record);
                    assert_eq!(u128::from(q.fee), expected);
                    assert_eq!(q.capital, 13 * 23);
                    assert_eq!(q.schedule, f.id());
                }
            }
        }
    }
    assert_eq!(f.source_unit_due(17).unwrap(), 3 + 5 * 17);
}
#[test]
fn nonfinite_tariffs_and_all_overflow_dimensions_are_rejected() {
    let source = SourceCatalogQuoteV2 { digest: Digest([9; 64]), units: 1, data_bytes: 1, opening_bytes: 1, max_unit_bytes: 1 };
    let mut invalid = fees();
    invalid.per_record_byte = 0;
    assert_eq!(invalid.validate(), Err(AssignmentResourceErrorV2::Policy));
    invalid = fees();
    invalid.per_source_unit = u64::MAX;
    assert_eq!(invalid.validate(), Err(AssignmentResourceErrorV2::Policy));
    assert_eq!(fees().source_unit_due(u64::MAX), Err(AssignmentResourceErrorV2::Budget));
    for (units, opening, relations, record, retained) in
        [(u64::MAX, 1, 1, 1, 1), (1, u64::MAX, 1, 1, 1), (1, 1, u64::MAX, 1, 1), (1, 1, 1, u64::MAX, 1), (1, 1, 1, 1, u64::MAX)]
    {
        let source = SourceCatalogQuoteV2 { units, opening_bytes: opening, ..source };
        assert_eq!(fees().quote(&source, relations, record, retained), Err(AssignmentResourceErrorV2::Budget));
    }
}
#[test]
fn source_and_cancellation_preimages_have_finite_bounds_and_separate_domains() {
    let mut source = AssignmentSourcePolicyV2 {
        version: 2,
        revision: 1,
        responsibility: kaspa_consensus_core::palw_positive_assignment_resources_v2::AssignmentSourceResponsibilityV2::FeeOnlyV1,
        release: kaspa_consensus_core::palw_positive_assignment_resources_v2::AssignmentSourceReleaseV2::AfterSignedDutiesRetentionAndLiabilityV1,
        work: AssignmentWorkScheduleV2 {
            version: 2,
            revision: 1,
            per_codec_step: 1,
            per_hash_byte: 1,
            per_signature: 1,
            per_element: 1,
            per_term: 1,
            per_io_byte: 1,
            per_control: 1,
        },
        max_parameter_instances: 100,
        max_requests: 10,
        max_responses: 20,
        max_history_events: 30,
        max_history_bytes: 100_000,
        max_intervals: 10,
        max_units_per_request: 3,
        max_native_body_bytes: 20_000,
    };
    source.validate().unwrap();
    let bytes = borsh::to_vec(&source).unwrap();
    assert_eq!(bytes.len(), 130);
    assert_eq!(&bytes[4..6], &[0, 0]);
    for at in [4, 5] {
        let mut changed = bytes.clone();
        changed[at] = 255;
        assert!(borsh::from_slice::<AssignmentSourcePolicyV2>(&changed).is_err());
    }

    let original = source.id();
    source.max_units_per_request += 1;
    assert_ne!(source.id(), original);
    source.max_history_events = 29;
    assert_eq!(source.validate(), Err(AssignmentResourceErrorV2::Policy));
    source.max_history_events = 30;
    source.max_parameter_instances = u64::MAX;
    assert_eq!(source.validate(), Err(AssignmentResourceErrorV2::Policy));
    let mut cancellation =
        AssignmentCancellationPolicyV2 { version: 2, revision: 1, rule: AssignmentCancellationRuleV2::PreserveEarnedAndLiabilityV1 };
    cancellation.validate().unwrap();
    assert_ne!(cancellation.id(), fees().id());
    cancellation.revision = 2;
    assert_eq!(cancellation.validate(), Err(AssignmentResourceErrorV2::Policy));
}

#[test]
fn native_work_mapping_is_explicit_original_policy_and_overflow_checked() {
    let schedule = AssignmentWorkScheduleV2 {
        version: 2,
        revision: 1,
        per_codec_step: 2,
        per_hash_byte: 3,
        per_signature: 5,
        per_element: 7,
        per_term: 11,
        per_io_byte: 13,
        per_control: 17,
    };
    let q = AssignmentWorkQuantitiesV2 {
        codec_steps: 19,
        hash_bytes: 23,
        signatures: 29,
        elements: 31,
        terms: 37,
        io_bytes: 41,
        controls: 43,
    };
    let expected =
        [(2u128, 19u128), (3, 23), (5, 29), (7, 31), (11, 37), (13, 41), (17, 43)].into_iter().map(|(a, b)| a * b).sum::<u128>();
    assert_eq!(u128::from(schedule.charge(q).unwrap()), expected);
    assert_eq!(schedule.charge(AssignmentWorkQuantitiesV2 { elements: u64::MAX, ..q }), Err(AssignmentResourceErrorV2::Budget));
    let mut changed = schedule;
    changed.per_control += 1;
    assert_ne!(changed.id(), schedule.id());
    changed.per_codec_step = 0;
    assert_eq!(changed.validate(), Err(AssignmentResourceErrorV2::Policy));
}
