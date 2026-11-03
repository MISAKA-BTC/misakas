use misaka_palw_positive_contract::{ContractError, messages::AggregateBudget, reservations::RemainingCapacity, rows::AggregateUsage};

fn budget(limit: u64) -> AggregateBudget {
    AggregateBudget {
        rounds: 1,
        unique_source_bytes: 1,
        transmitted_bytes: limit,
        additional_work: limit,
        retained_bytes: limit,
        retention_until: 1,
        fee: 1,
        witness_attempts: 1,
    }
}
fn remaining(dimension: usize, value: u64) -> RemainingCapacity {
    RemainingCapacity {
        transmitted_bytes: if dimension == 0 { value } else { 0 },
        additional_work: if dimension == 1 { value } else { 0 },
        retained_bytes: if dimension == 2 { value } else { 0 },
    }
}
fn consumed(dimension: usize, value: u64) -> AggregateUsage {
    AggregateUsage {
        transmitted_bytes: if dimension == 0 { value } else { 0 },
        additional_work: if dimension == 1 { value } else { 0 },
        retained_bytes: if dimension == 2 { value } else { 0 },
        ..AggregateUsage::default()
    }
}

#[test]
fn all_small_source_and_other_role_demands_match_an_independent_wide_integer_bound() {
    for dimension in 0..3 {
        for used in 0..5 {
            for source in 0..5 {
                for other_role in 0..5 {
                    for limit in 0..9 {
                        // u128 arithmetic is independent of the production u64
                        // checked-add path. Exact fits and every one-below cap
                        // occur for each dimension and both unfinished roles.
                        let expected = u128::from(used) + u128::from(source) + u128::from(other_role) <= u128::from(limit);
                        assert_eq!(
                            remaining(dimension, source + other_role).check(&consumed(dimension, used), &budget(limit)).is_ok(),
                            expected,
                            "dimension={dimension} used={used} source={source} other={other_role} limit={limit}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fitting_current_consumption_cannot_erase_a_response_or_retention_reservation() {
    let cap = budget(100);
    for dimension in 0..3 {
        assert_eq!(remaining(dimension, 10).check(&consumed(dimension, 90), &cap), Ok(()));
        assert_eq!(remaining(dimension, 10).check(&consumed(dimension, 91), &cap), Err(ContractError::Budget));
        // Completing a response transfers its prepaid work/wire to consumed;
        // retained responsibility can remain reserved after delivery.
        assert_eq!(remaining(dimension, 4).check(&consumed(dimension, 96), &cap), Ok(()));
        assert_eq!(remaining(dimension, 5).check(&consumed(dimension, 96), &cap), Err(ContractError::Budget));
    }
}

#[test]
fn overflow_cannot_wrap_an_outstanding_duty_into_free_capacity() {
    for dimension in 0..3 {
        assert_eq!(remaining(dimension, 1).check(&consumed(dimension, u64::MAX), &budget(u64::MAX)), Err(ContractError::Budget));
        assert_eq!(remaining(dimension, u64::MAX).check(&consumed(dimension, 1), &budget(u64::MAX)), Err(ContractError::Budget));
    }
}

#[test]
fn remaining_capacity_does_not_waive_other_signed_service_caps_or_charge_usage() {
    let cap = budget(100);
    let reserved = remaining(0, 10);
    for dimension in 0..4 {
        let mut used = consumed(0, 90);
        match dimension {
            0 => used.rounds = 2,
            1 => used.unique_source_bytes = 2,
            2 => used.fee = 2,
            _ => used.witness_attempts = 2,
        }
        assert_eq!(reserved.check(&used, &cap), Err(ContractError::Budget));
    }
    let used = consumed(0, 90);
    let before = used;
    assert_eq!(reserved.check(&used, &cap), Ok(()));
    assert_eq!(used, before);
    assert_eq!(reserved.check(&used, &cap), Ok(()));
}
