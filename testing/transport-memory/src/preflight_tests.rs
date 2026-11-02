use super::{fixtures::*, p2p, rpc, transport::RetainedSize, wire_preflight::*};
use prost::Message;

fn compare<T: Message + Default + WirePreflight + RetainedSize>(wire: &[u8]) -> HeapBound {
    let estimate = T::wire_heap_bound(wire, 0).unwrap();
    let decoded = T::decode(wire).unwrap();
    assert!(
        estimate.retained_bytes >= decoded.retained_heap_bytes(),
        "bound={} actual={}",
        estimate.retained_bytes,
        decoded.retained_heap_bytes()
    );
    assert!(estimate.peak_bytes >= estimate.retained_bytes);
    estimate
}
fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn field(tag: u32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    varint(u64::from(tag) * 8 + 2, &mut out);
    varint(data.len() as u64, &mut out);
    out.extend_from_slice(data);
    out
}

#[test]
fn real_nested_empty_rpc_messages_charge_vector_inline_capacity() {
    let message = rpc::RpcBlock { transactions: vec![rpc::RpcTransaction::default(); 4097], ..Default::default() };
    let wire = message.encode_to_vec();
    let estimate = compare::<rpc::RpcBlock>(&wire);
    assert!(estimate.retained_bytes > wire.len() as u64 * 16);
    let message = p2p::AddressesMessage { address_list: vec![p2p::NetAddress::default(); 4097] };
    compare::<p2p::AddressesMessage>(&message.encode_to_vec());
}

#[test]
fn recursive_box_strings_vectors_and_oneof_match_decoded_capacities() {
    use super::fixtures::node::Choice;
    let leaf = Node {
        data: vec![1; 131],
        blobs: vec![vec![], vec![2; 9]],
        labels: vec!["a".into(), "b".repeat(29)],
        values: vec![0; 513],
        label: "c".repeat(65),
        choice: Some(Choice::Selected(vec![3; 257])),
        ..Default::default()
    };
    let message = Node {
        child: Some(Box::new(leaf.clone())),
        children: vec![leaf.clone(); 17],
        choice: Some(Choice::Nested(Box::new(leaf))),
        ..Default::default()
    };
    compare::<Node>(&message.encode_to_vec());
    for choice in [Choice::Scalar(u32::MAX), Choice::Text("hello".into())] {
        compare::<Node>(&Node { choice: Some(choice), ..Default::default() }.encode_to_vec());
    }
}

#[test]
fn repeated_packed_and_unpacked_primitives_charge_all_elements() {
    let message = Scalars { varints: vec![u64::MAX; 129], wide: vec![17; 33], small: vec![-1; 17], flag: Some(true) };
    compare::<Scalars>(&message.encode_to_vec());
    let mut unpacked = Vec::new();
    for _ in 0..129 {
        unpacked.push(8);
        varint(17, &mut unpacked);
    }
    for _ in 0..33 {
        unpacked.push(17);
        unpacked.extend_from_slice(&17u64.to_le_bytes());
    }
    for _ in 0..17 {
        unpacked.push(29);
        unpacked.extend_from_slice(&(-1i32).to_le_bytes());
    }
    compare::<Scalars>(&unpacked);
    assert!(Scalars::wire_heap_bound(&[18, 1, 0], 0).is_err()); // partial fixed64 packed field
}

#[test]
fn packed_scalar_batch_charges_intermediate_reallocation_overlap() {
    let message = Scalars { varints: vec![1; 768], ..Default::default() };
    let wire = message.encode_to_vec();
    let decoded = Scalars::decode(wire.as_slice()).unwrap();
    assert_eq!(decoded.varints.capacity(), 1024);
    let estimate = compare::<Scalars>(&wire);
    assert_eq!(estimate.retained_bytes, 1024 * 8);
    assert_eq!(estimate.peak_bytes, (512 + 1024) * 8, "packed decoding pushes through the intermediate Vec capacities");
}

#[test]
fn duplicate_blob_fields_remain_bounded_but_nested_merge_is_explicitly_unsupported() {
    let mut wire = Vec::new();
    for n in [1, 9, 17, 33, 65, 129] {
        wire.extend(field(3, &vec![7; n]));
    }
    compare::<Node>(&wire);
    for n in [1, 5, 17, 65] {
        wire.extend(field(1, &Node { values: vec![1; n], blobs: vec![vec![2; n]], ..Default::default() }.encode_to_vec()));
    }
    assert!(Node::decode(wire.as_slice()).is_ok(), "this is valid protobuf with merge semantics");
    assert_eq!(Node::wire_heap_bound(&wire, 0).unwrap_err().0, "unsupported repeated singular/message-oneof merge");
}

#[test]
fn singular_merged_256_by_three_fragments_require_explicit_unsupported() {
    let child = Node { values: vec![1; 256], ..Default::default() }.encode_to_vec();
    let fragment = field(1, &child);
    let independent = Node::wire_heap_bound(&fragment, 0).unwrap().retained_bytes * 3;
    let wire = fragment.repeat(3);
    let decoded = Node::decode(wire.as_slice()).unwrap();
    let values = &decoded.child.as_ref().unwrap().values;
    assert_eq!(values.len(), 768);
    assert_eq!(values.capacity(), 1024);
    let actual = decoded.retained_heap_bytes();
    println!("singular merge: independent={independent} actual={actual}");
    assert!(independent < actual, "the old fragment-sum estimate must reproduce the undercount");
    assert!(Node::wire_heap_bound(&wire, 0).is_err(), "unsupported merge must not return a false heap bound");
}

#[test]
fn real_p2p_oneof_merged_256_by_three_fragments_require_explicit_unsupported() {
    let fragment = p2p::KaspadMessage {
        payload: Some(p2p::kaspad_message::Payload::BlockHeaders(p2p::BlockHeadersMessage {
            block_headers: vec![p2p::BlockHeader::default(); 256],
        })),
        ..Default::default()
    }
    .encode_to_vec();
    let independent = p2p::KaspadMessage::wire_heap_bound(&fragment, 0).unwrap().retained_bytes * 3;
    let wire = fragment.repeat(3);
    let decoded = p2p::KaspadMessage::decode(wire.as_slice()).unwrap();
    let Some(p2p::kaspad_message::Payload::BlockHeaders(headers)) = &decoded.payload else { panic!("headers payload") };
    assert_eq!(headers.block_headers.len(), 768);
    assert_eq!(headers.block_headers.capacity(), 1024);
    let actual = decoded.retained_heap_bytes();
    println!("P2P oneof merge: independent={independent} actual={actual}");
    assert!(independent < actual, "the old fragment-sum estimate must reproduce the undercount");
    assert!(p2p::KaspadMessage::wire_heap_bound(&wire, 0).is_err(), "unsupported merge must not return a false heap bound");
}

#[test]
fn unknown_fields_skip_without_heap_and_validate_groups() {
    let mut wire = field(999, &[3; 4096]);
    varint(998 * 8, &mut wire);
    varint(u64::MAX, &mut wire);
    varint(997 * 8 + 3, &mut wire);
    wire.extend(field(996, b"ignored"));
    varint(997 * 8 + 4, &mut wire);
    assert_eq!(compare::<Node>(&wire), HeapBound::default());
    let mut bad = vec![];
    varint(997 * 8 + 3, &mut bad);
    varint(996 * 8 + 4, &mut bad);
    assert!(Node::wire_heap_bound(&bad, 0).is_err());
}

#[test]
fn malformed_lengths_utf8_tags_and_varints_refuse_before_decoding() {
    for wire in [
        vec![0],
        vec![6],
        vec![0xff; 10],
        vec![0x80; 11],
        vec![26, 5, 0],
        vec![58, 1, 0xff],
        vec![26, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1],
        vec![12],
        vec![8, 0],
    ] {
        assert!(Node::wire_heap_bound(&wire, 0).is_err(), "unexpected preflight success: {wire:?}");
        assert!(Node::decode(wire.as_slice()).is_err());
    }
}

#[test]
fn default_prost_depth_boundary_and_unknown_group_depth_are_preserved() {
    let mut valid = Vec::new();
    for _ in 0..MAX_NESTING {
        valid = field(1, &valid);
    }
    compare::<Node>(&valid);
    let invalid = field(1, &valid);
    assert!(Node::wire_heap_bound(&invalid, 0).is_err());
    assert!(Node::decode(invalid.as_slice()).is_err());
    let mut groups = Vec::new();
    for _ in 0..MAX_NESTING + 2 {
        varint(999 * 8 + 3, &mut groups);
    }
    for _ in 0..MAX_NESTING + 2 {
        varint(999 * 8 + 4, &mut groups);
    }
    assert!(Node::wire_heap_bound(&groups, 0).is_err());
}

#[test]
fn deterministic_malformed_corpus_never_underestimates_successful_decode() {
    let mut seed = 0x381791u64;
    for case in 0..5000 {
        let mut bytes = vec![0; case % 97];
        for byte in &mut bytes {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *byte = (seed >> 32) as u8;
        }
        if Node::decode(bytes.as_slice()).is_ok() {
            compare::<Node>(&bytes);
        }
        if rpc::RpcBlock::decode(bytes.as_slice()).is_ok() {
            compare::<rpc::RpcBlock>(&bytes);
        }
    }
}

#[test]
#[ignore = "256 MiB allocation; run alone with --ignored --test-threads=1"]
fn largest_legitimate_bytes_field_has_schema_bound_without_fixed_multiplier() {
    for payload_bytes in [256 << 20, (320 << 20) - 1024] {
        let wire = field(3, &vec![1; payload_bytes]);
        let estimate = Node::wire_heap_bound(&wire, 0).unwrap();
        assert_eq!(estimate.retained_bytes, payload_bytes as u64);
        assert_eq!(estimate.peak_bytes, payload_bytes as u64);
        compare::<Node>(&wire);
        eprintln!(
            "payload_bytes={payload_bytes} schema_retained_bytes={} schema_peak_bytes={} actual_decode_capacity_checked=true",
            estimate.retained_bytes, estimate.peak_bytes
        );
    }
}
