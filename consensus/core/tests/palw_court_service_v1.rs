//! Scheduler component tests, NOT a proof of T04/T05 node capacity.
use kaspa_consensus_core::palw_court_service_v1::*;

fn cost() -> ServiceCost {
    ServiceCost { bytes: 10, work: 20, io: 30, controls: 1 }
}
fn packet(id: u8, start: u64, end: u64) -> ServicePacket {
    ServicePacket { id: [id; 64], release_daa: start, service_before: end, cost: cost() }
}

#[test]
fn bounded_admission_is_atomic_and_cannot_displace_existing_rights() {
    let mut ledger = ServiceReservations::new(cost()).unwrap();
    ledger.reserve(&[packet(1, 100, 102)], 102).unwrap();
    let before = ledger.clone();
    assert_eq!(ledger.reserve(&[packet(2, 100, 102), packet(3, 100, 102)], 102), Err(ServiceRefusal::NoCapacity));
    assert_eq!(ledger, before);
    ledger.reserve(&[packet(2, 100, 102)], 102).unwrap();
    assert_eq!(ledger.assigned_daa(&[1; 64]), Some(100));
    assert_eq!(ledger.assigned_daa(&[2; 64]), Some(101));
}

#[test]
fn schedule_ignores_submission_order_and_has_no_owner_identity() {
    let mut a = ServiceReservations::new(cost()).unwrap();
    let mut b = a.clone();
    let rows = [packet(4, 100, 103), packet(2, 100, 102), packet(3, 100, 103)];
    a.reserve(&rows, 103).unwrap();
    b.reserve(&rows.into_iter().rev().collect::<Vec<_>>(), 103).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.assigned_daa(&[2; 64]), Some(100));
}

#[test]
fn duplicate_hostile_branches_and_resource_excess_are_not_free() {
    let mut ledger = ServiceReservations::new(cost()).unwrap();
    let before = ledger.clone();
    assert_eq!(ledger.reserve(&[packet(1, 1, 2), packet(1, 1, 2)], 2), Err(ServiceRefusal::DuplicateIdentity));
    let mut too_large = packet(2, 1, 2);
    too_large.cost.work += 1;
    assert_eq!(ledger.reserve(&[too_large], 2), Err(ServiceRefusal::NoCapacity));
    assert_eq!(ledger.reserve(&[packet(3, 2, 3)], 2), Err(ServiceRefusal::InvalidInterval));
    assert_eq!(ledger, before);
}

#[test]
fn silence_does_not_stop_the_clock_but_skipped_supply_never_defaults() {
    let mut ledger = ServiceReservations::new(cost()).unwrap();
    ledger.reserve(&[packet(1, 100, 102), packet(2, 100, 102)], 102).unwrap();
    assert_eq!(ledger.assess(&[1; 64]), ServiceAssessment::Pending);
    ledger.observe_block(100, [10; 64], ServiceCost::default()).unwrap();
    // DAA 101 had no accepting-chain service unit. No response is needed to
    // advance the clock; the two obligations must NOT get the same verdict.
    ledger.observe_block(102, [12; 64], ServiceCost::default()).unwrap();
    assert_eq!(ledger.assess(&[1; 64]), ServiceAssessment::OpportunityProvided);
    assert_eq!(ledger.assess(&[2; 64]), ServiceAssessment::MissingSupply);
    assert_eq!(ledger.assess(&[99; 64]), ServiceAssessment::UnknownObligation);
    assert!(ledger.supply_at(101).is_none());
    assert!(ledger.supply_at(102).is_none(), "unreserved positions allocate no history rows");
}

#[test]
fn additional_blocks_neither_duplicate_supply_nor_replace_its_identity() {
    let mut ledger = ServiceReservations::new(cost()).unwrap();
    ledger.reserve(&[packet(1, 100, 101)], 101).unwrap();
    assert_eq!(ledger.observe_block(100, [1; 64], ServiceCost::default()), Ok(ServiceObservation::FirstBlock));
    let first = *ledger.supply_at(100).unwrap();
    assert_eq!(ledger.observe_block(100, [2; 64], cost()), Ok(ServiceObservation::AdditionalBlockAtSameDaa));
    assert_eq!(ledger.supply_at(100), Some(&first));
    ledger.observe_block(101, [3; 64], ServiceCost::default()).unwrap();
    assert_eq!(ledger.assess(&[1; 64]), ServiceAssessment::OpportunityProvided);
    let before = ledger.clone();
    assert_eq!(ledger.observe_block(100, [4; 64], ServiceCost::default()), Err(ServiceRefusal::DaaWentBackwards));
    assert_eq!(ledger, before);
    assert_eq!(ledger.reserve(&[packet(2, 101, 102)], 102), Err(ServiceRefusal::ReservationInThePast));
    assert_eq!(ledger, before);
}

#[test]
fn other_court_traffic_cannot_spend_reserved_bytes_work_or_io() {
    for occupied in [
        ServiceCost { bytes: 1, ..ServiceCost::default() },
        ServiceCost { work: 1, ..ServiceCost::default() },
        ServiceCost { io: 1, ..ServiceCost::default() },
        ServiceCost { controls: 1, ..ServiceCost::default() },
    ] {
        let mut ledger = ServiceReservations::new(cost()).unwrap();
        ledger.reserve(&[packet(1, 100, 101)], 101).unwrap();
        let before = ledger.clone();
        assert_eq!(ledger.observe_block(100, [1; 64], occupied), Err(ServiceRefusal::ReservedCapacitySpent));
        assert_eq!(ledger, before, "an invalid block cannot fabricate an opportunity");
        assert_eq!(ledger.assess(&[1; 64]), ServiceAssessment::Pending);
    }
}

#[test]
fn rollback_restores_the_missing_service_assessment() {
    let mut prefix = ServiceReservations::new(cost()).unwrap();
    prefix.reserve(&[packet(1, 100, 101)], 101).unwrap();
    let mut served = prefix.clone();
    served.observe_block(100, [1; 64], ServiceCost::default()).unwrap();
    served.observe_block(101, [2; 64], ServiceCost::default()).unwrap();
    let mut skipped = prefix.clone();
    skipped.observe_block(101, [3; 64], ServiceCost::default()).unwrap();
    assert_eq!(served.assess(&[1; 64]), ServiceAssessment::OpportunityProvided);
    assert_eq!(skipped.assess(&[1; 64]), ServiceAssessment::MissingSupply);
    assert_eq!(prefix.assess(&[1; 64]), ServiceAssessment::Pending);
    // The book has no wall-clock/global side effect: replay the same branch.
    prefix.observe_block(100, [1; 64], ServiceCost::default()).unwrap();
    prefix.observe_block(101, [2; 64], ServiceCost::default()).unwrap();
    assert_eq!(prefix, served);
}
