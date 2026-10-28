//! Candidate codec/arithmetic fixtures, not native admission or measurements.
#![allow(warnings)]
use kaspa_consensus_core::palw_court_duty_clock_v1::CourtDutyClockV1;
use kaspa_consensus_core::palw_native_resource_v1::{NativeHeapV1, NativeWireReaderV1, NativeWireScanV1};
use kaspa_consensus_core::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::{BlockHash, Hash64};
fn fixture() -> CourtDutyClockV1 {
    let h = Hash64::from_u64_word;
    CourtDutyClockV1 {
        version: 1,
        profile_id: h(1),
        duty_id: h(2),
        claim: h(3),
        allocation_id: h(4),
        source_id: h(5),
        acquisition: Key::TirAcquisition(h(3), PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(6), 0)), h(7)),
        answerer: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0)),
        demand_id: h(9),
        turn: 10,
        predecessor_block: BlockHash::from_u64_word(11),
        predecessor_daa: 101,
        first_eligible_daa: 102,
        response_deadline_daa: 105,
        original_hard_daa: 10_100,
        material_until_daa: 11_000,
    }
}
#[test]
fn duty_clock_new_tags_roundtrip_scan_and_typed_heap_agree() {
    let clock = fixture();
    let key = Key::CourtDutyClock(clock.duty_id);
    let row = Row::CourtDutyClock(Box::new(clock));
    row.check(&key).unwrap();
    let encoded = borsh::to_vec(&row).unwrap();
    assert_eq!(encoded[0], 19);
    assert_eq!(borsh::to_vec(&key).unwrap()[0], 13);
    assert_eq!(borsh::to_vec(&Key::CourtService).unwrap(), vec![12]);
    assert_eq!(borsh::to_vec(&Key::TirContainer(Hash64::default())).unwrap()[0], 11);
    assert_eq!(borsh::from_slice::<Row>(&encoded).unwrap(), row);
    let mut scan = NativeWireReaderV1::new(encoded.as_slice(), encoded.len() as u64);
    let shape = Row::scan_native_wire_v1(&mut scan).unwrap();
    assert_eq!(scan.remaining(), 0);
    assert!(shape.heap >= row.native_heap_v1("duty clock").bytes().unwrap());
    for end in [1, encoded.len() - 1] {
        let mut cut = NativeWireReaderV1::new(&encoded[..end], end as u64);
        assert!(Row::scan_native_wire_v1(&mut cut).is_err());
    }
    let mut unknown = encoded;
    unknown[1..3].copy_from_slice(&2u16.to_le_bytes());
    let decoded: Row = borsh::from_slice(&unknown).unwrap();
    assert!(decoded.check(&key).is_err(), "unsupported marker payload never silently upgraded");
}
#[test]
fn duty_clock_timing_is_strict_without_inventing_a_future_window() {
    let clock = fixture();
    let key = Key::CourtDutyClock(clock.duty_id);
    assert!(!clock.stage_started_before_unit(101));
    assert!(clock.stage_started_before_unit(102));
    assert!(clock.stage_started_before_unit(104));
    assert!(!clock.stage_started_before_unit(105));
    // ServicePacket::service_before and original hard are exclusive, even
    // when an old native response deadline lies beyond the service envelope.
    let mut hard = clock.clone();
    hard.original_hard_daa = 104;
    assert!(hard.stage_started_before_unit(103));
    assert!(!hard.stage_started_before_unit(104));
    let mut late = clock.clone();
    late.predecessor_daa = 104;
    late.first_eligible_daa = 105;
    late.check(&key).unwrap();
    assert!(!late.stage_started_before_unit(105));
    let mut overflow = clock.clone();
    overflow.predecessor_daa = u64::MAX;
    overflow.first_eligible_daa = 0;
    assert!(overflow.check(&key).is_err());
    let mut forged = clock.clone();
    forged.first_eligible_daa = 101;
    assert!(forged.check(&key).is_err());
    let mut forged = clock.clone();
    forged.acquisition = Key::CourtService;
    assert!(forged.check(&key).is_err());
    assert!(clock.check(&Key::CourtDutyClock(Hash64::default())).is_err());
}
