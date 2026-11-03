//! Rooted candidate codec/resource checks, not production service or node capacity.
use kaspa_consensus_core::palw_native_resource_v1::{NativeHeapV1, NativeWireReaderV1, NativeWireScanV1};
use kaspa_consensus_core::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use kaspa_consensus_core::{Hash64, palw_court_service_state_v1::*, palw_court_service_v1::*};
use std::collections::{BTreeMap, BTreeSet};

fn candidate() -> CourtServiceState {
    let cost = ServiceCost { bytes: 8, work: 13, io: 0, controls: 1 };
    let mut book = ServiceReservations::new(ServiceCost { io: 1, ..cost }).unwrap();
    book.reserve(&[ServicePacket { id: [7; 64], release_daa: 101, service_before: 102, cost }], 10_100).unwrap();
    book.observe_block(101, [9; 64], Default::default()).unwrap();
    book.observe_block(102, [10; 64], Default::default()).unwrap();
    let claim = Hash64::from_u64_word(1);
    CourtServiceState {
        version: 1,
        profile_id: Hash64::from_u64_word(99),
        book,
        claims: BTreeMap::from([(
            claim,
            ServiceClaimBinding {
                claim,
                allocation_id: Hash64::from_u64_word(2),
                source_id: Hash64::from_u64_word(3),
                original_accepted_daa: 100,
                hard_deadline_daa: 10_100,
                packet_ids: BTreeSet::from([[7; 64]]),
            },
        )]),
    }
}
#[test]
fn rooted_service_codec_is_bounded_and_appends_its_tags() {
    let row = Row::CourtService(Box::new(candidate()).into());
    row.check(&Key::CourtService).unwrap();
    let bytes = borsh::to_vec(&row).unwrap();
    assert_eq!(bytes[0], 17, "new row does not reuse an old tag");
    assert_eq!(borsh::to_vec(&Key::CourtService).unwrap(), vec![11]);
    let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    let shape = Row::scan_native_wire_v1(&mut reader).unwrap();
    assert_eq!(reader.remaining(), 0, "scanner includes the complete nested ledger");
    assert!(shape.heap >= row.native_heap_v1("service").bytes().expect("typed native heap coverage"));
    assert_eq!(borsh::from_slice::<Row>(&bytes).unwrap(), row);
    let mut truncated = NativeWireReaderV1::new(&bytes[..bytes.len() - 1], (bytes.len() - 1) as u64);
    assert!(Row::scan_native_wire_v1(&mut truncated).is_err());
}
#[test]
fn persisted_bindings_cannot_alias_packets_or_rebase_hard_deadline() {
    let mut state = candidate();
    state.check().unwrap();
    let claim = *state.claims.keys().next().unwrap();
    state.claims.get_mut(&claim).unwrap().hard_deadline_daa += 1;
    assert!(state.check().is_err());
    let mut state = candidate();
    let mut alias = state.claims[&claim].clone();
    alias.claim = Hash64::from_u64_word(4);
    state.claims.insert(alias.claim, alias);
    assert!(state.check().is_err(), "one reservation cannot fulfill two original claims");
    let mut state = candidate();
    state.claims.clear();
    assert!(state.check().is_err(), "no unbound packet can be presented as a claim obligation");
}

#[test]
fn persisted_residual_quota_is_recomputed_instead_of_trusted() {
    let state = candidate();
    let mut bytes = borsh::to_vec(&state.book).unwrap();
    // Alter the serialized budget without recomputing the persisted residual.
    bytes[..8].copy_from_slice(&9u64.to_le_bytes());
    let forged: ServiceReservations = borsh::from_slice(&bytes).unwrap();
    assert!(forged.check().is_err());
    let mut forged_state = state;
    forged_state.book = forged;
    assert!(forged_state.check().is_err());
}

#[test]
fn canonical_claim_repair_keeps_blocked_claim_held_and_serves_another() {
    let cost = ServiceCost { bytes: 8, work: 13, io: 0, controls: 1 };
    let packets = [(1, 100, 104), (2, 100, 104), (3, 100, 106), (4, 104, 105)]
        .map(|(id, release_daa, service_before)| ServicePacket { id: [id; 64], release_daa, service_before, cost });
    let mut book = ServiceReservations::new(ServiceCost { io: 1, ..cost }).unwrap();
    book.reserve(&packets, 10_099).unwrap();
    book.observe_block(103, [13; 64], Default::default()).unwrap();
    let binding = |n, ids: BTreeSet<[u8; 64]>| ServiceClaimBinding {
        claim: Hash64::from_u64_word(n),
        allocation_id: Hash64::from_u64_word(n + 10),
        source_id: Hash64::from_u64_word(n + 20),
        original_accepted_daa: 99,
        hard_deadline_daa: 10_099,
        packet_ids: ids,
    };
    let a = binding(1, BTreeSet::from([[1; 64], [2; 64], [4; 64]]));
    let b = binding(2, BTreeSet::from([[3; 64]]));
    let mut state = CourtServiceState {
        version: 1,
        profile_id: Hash64::from_u64_word(99),
        book,
        claims: BTreeMap::from([(a.claim, a.clone()), (b.claim, b.clone())]),
    };
    let bindings = state.claims.clone();
    let mut reversed = state.clone();
    reversed.claims = BTreeMap::from([(b.claim, b.clone()), (a.claim, a.clone())]);
    let report = state.repair_missing_supply().unwrap();
    assert_eq!(reversed.repair_missing_supply().unwrap(), report);
    assert_eq!(state, reversed);
    assert!(report.iter().any(|r| r.claim == a.claim && r.result == Err(ServiceRefusal::DeadlineExhausted)));
    assert!(report.iter().any(|r| r.claim == b.claim && r.result == Ok(1)));
    assert_eq!(state.book.assigned_daa(&[1; 64]), Some(100));
    assert_eq!(state.book.assigned_daa(&[2; 64]), Some(101));
    assert_eq!(state.book.assigned_daa(&[3; 64]), Some(105));
    assert_eq!(state.book.assigned_daa(&[4; 64]), Some(104));
    assert_eq!(state.claims, bindings);
    state.check().unwrap();
}
