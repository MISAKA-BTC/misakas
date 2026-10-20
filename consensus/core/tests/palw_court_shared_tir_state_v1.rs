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

#[test]
fn shared_stream_chunk_append_tags_and_preserve_bounded_decode_shape() {
    use kaspa_consensus_core::palw_court_shared_tir_stream_v1::{SignedSharedStreamV1,SharedStreamTargetV1,SharedStreamActionV1};
    use kaspa_consensus_core::palw_court_shared_tir_stream_state_v1::{SharedTirStreamV1,SharedTirChunkV1};
    let h=Hash64::from_u64_word;let signer=PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(1),0));
    let control=SignedSharedStreamV1 {version:1,claim:h(2),scope:h(3),target:SharedStreamTargetV1::FullIds,signer,
        action:SharedStreamActionV1::Begin {total_bytes:3,body_hash:h(4)},signature:vec![]};
    let slot=control.slot_id().unwrap();let stream=control.declared_stream_id(3,h(4)).unwrap();
    let fixtures=[
        (Key::CourtSharedStream(slot),Row::CourtSharedStream(Box::new(SharedTirStreamV1 {version:1,claim:h(2),scope_id:h(3),target:SharedStreamTargetV1::FullIds,
            signer,slot_id:slot,stream_id:stream,total_bytes:3,body_hash:h(4),begun_block:h(5),begun_daa:100,begin_intent:h(6),last_block:h(5),last_daa:100,
            accepted_bytes:0,head:Hash64::default(),finished:None,material_until_daa:20_000}))),
        (Key::CourtSharedChunk(slot,0),Row::CourtSharedChunk(Box::new(SharedTirChunkV1 {version:1,claim:h(2),slot_id:slot,stream_id:stream,offset:0,
            previous:Hash64::default(),bytes:vec![1,2,3],block:h(7),daa:101,intent:h(8),material_until_daa:20_000}).into())),
    ];
    for (i,(key,row)) in fixtures.into_iter().enumerate() {
        row.check(&key).unwrap();let wire=borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::to_vec(&key).unwrap()[0],17+i as u8);assert_eq!(wire[0],23+i as u8);
        assert_eq!(borsh::from_slice::<Row>(&wire).unwrap(),row);
        let mut scan=NativeWireReaderV1::new(wire.as_slice(),wire.len() as u64);let shape=Row::scan_native_wire_v1(&mut scan).unwrap();
        assert_eq!(scan.remaining(),0);assert!(shape.heap>=row.native_heap_v1("shared stream").bytes().unwrap());
        let mut cut=NativeWireReaderV1::new(&wire[..wire.len()-1],(wire.len()-1) as u64);assert!(Row::scan_native_wire_v1(&mut cut).is_err());
        let mut unsupported=wire;unsupported[1..3].copy_from_slice(&2u16.to_le_bytes());assert!(borsh::from_slice::<Row>(&unsupported).unwrap().check(&key).is_err());
    }
    // Chunk's custom decoder must reject length before trying to read a huge payload.
    let mut prefix=Vec::new();prefix.extend(1u16.to_le_bytes());
    for value in [h(2),slot,stream] {prefix.extend(borsh::to_vec(&value).unwrap());}
    prefix.extend(0u64.to_le_bytes());prefix.extend(borsh::to_vec(&Hash64::default()).unwrap());prefix.extend(u32::MAX.to_le_bytes());
    assert_eq!(<SharedTirChunkV1 as borsh::BorshDeserialize>::deserialize_reader(&mut prefix.as_slice()).unwrap_err().kind(),std::io::ErrorKind::InvalidData);
}

#[test]
fn shared_data_response_appends_tags_and_has_native_codec_coverage() {
    use kaspa_consensus_core::palw_court_shared_tir_response_v1::SharedDataResponseV1;
    let h=Hash64::from_u64_word;let signer=PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(1),0));
    let key=Key::CourtSharedResponse(h(2),signer);
    let row=Row::CourtSharedResponse(Box::new(SharedDataResponseV1 {version:1,claim:h(3),scope_id:h(4),request_id:h(2),signer,
        query_block:h(5),query_daa:100,data_digest:h(6),accepted_block:h(7),accepted_daa:101,intent_digest:h(8),material_until_daa:20_000}));
    row.check(&key).unwrap();let wire=borsh::to_vec(&row).unwrap();
    assert_eq!(borsh::to_vec(&key).unwrap()[0],19);assert_eq!(wire[0],25);
    assert_eq!(borsh::from_slice::<Row>(&wire).unwrap(),row);
    let mut scan=NativeWireReaderV1::new(wire.as_slice(),wire.len() as u64);let shape=Row::scan_native_wire_v1(&mut scan).unwrap();
    assert_eq!(scan.remaining(),0);assert!(shape.heap>=row.native_heap_v1("response").bytes().unwrap());
    let mut cut=NativeWireReaderV1::new(&wire[..wire.len()-1],(wire.len()-1) as u64);assert!(Row::scan_native_wire_v1(&mut cut).is_err());
    let mut unsupported=wire;unsupported[1..3].copy_from_slice(&2u16.to_le_bytes());assert!(borsh::from_slice::<Row>(&unsupported).unwrap().check(&key).is_err());
}
