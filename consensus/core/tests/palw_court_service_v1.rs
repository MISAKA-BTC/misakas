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
