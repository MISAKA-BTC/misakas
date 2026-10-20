//! Codec/typed-allocation fixtures only, not source authentication or funding.
use kaspa_consensus_core::{Hash64,BlockHash};
use kaspa_consensus_core::palw_court_shared_tir_state_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
use kaspa_consensus_core::palw_native_resource_v1::{NativeHeapV1,NativeWireReaderV1,NativeWireScanV1};
#[test]
fn shared_context_query_ids_append_tags_and_have_complete_native_codec_coverage() {
    let h=Hash64::from_u64_word;let body=vec![1,2,3];
    let fixtures=[
        (Key::CourtSharedContext(h(1)),Row::CourtSharedContext(Box::new(SharedTirContextV1 {version:1,claim:h(1),allocation_id:h(2),source_id:h(3),rules_id:h(4),profile_id:h(5),scope_id:h(6),queries:1,original_accepted_daa:100,hard_daa:10_100,material_until_daa:11_000}))),
        (Key::CourtSharedQuery(h(7)),Row::CourtSharedQuery(Box::new(SharedTirQueryV1 {version:1,claim:h(1),scope_id:h(6),ordinal:0,request_id:h(7),opened_block:BlockHash::from_u64_word(8),opened_daa:101,opened_by:PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(9),0)),intent_digest:h(10),material_until_daa:11_000,body:Some(SharedTirBodyV1 {block:BlockHash::from_u64_word(11),accepted_daa:102,digest:shared_body_digest_v1(&body),bytes:body})}))),
        (Key::CourtSharedIds(h(1)),Row::CourtSharedIds(Box::new(SharedTirIdsV1 {version:1,claim:h(1),scope_id:h(6),rows_root:h(12),ids:vec![1,2],block:BlockHash::from_u64_word(13),accepted_daa:102,material_until_daa:11_000}))),
    ];
    for (i,(key,row)) in fixtures.into_iter().enumerate() {
        row.check(&key).unwrap();let bytes=borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::to_vec(&key).unwrap()[0],14+i as u8);assert_eq!(bytes[0],20+i as u8);
        assert_eq!(borsh::from_slice::<Row>(&bytes).unwrap(),row);
        let mut scan=NativeWireReaderV1::new(bytes.as_slice(),bytes.len() as u64);
        let shape=Row::scan_native_wire_v1(&mut scan).unwrap();assert_eq!(scan.remaining(),0);
        assert!(shape.heap>=row.native_heap_v1("shared row").bytes().unwrap());
        let mut cut=NativeWireReaderV1::new(&bytes[..bytes.len()-1],(bytes.len()-1) as u64);assert!(Row::scan_native_wire_v1(&mut cut).is_err());
        let mut unsupported=bytes;unsupported[1..3].copy_from_slice(&2u16.to_le_bytes());
        assert!(borsh::from_slice::<Row>(&unsupported).unwrap().check(&key).is_err());
    }
    assert_eq!(borsh::to_vec(&Key::CourtService).unwrap(),vec![12]);
    assert_eq!(borsh::to_vec(&Key::CourtDutyClock(h(1))).unwrap()[0],13);
}
