use super::*;
use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_WIRE_MAGIC, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2};

fn envelope(version: u16, key_len: usize, sig_len: usize) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version,
            network_domain: Hash64::from_u64_word(1),
            challenge: Hash64::from_u64_word(2),
            class_id: Hash64::from_u64_word(3),
            executor_bond: crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(4), 5),
            executor_pubkey: vec![0xA7; key_len],
            operator_id: Hash64::from_u64_word(6),
            artifact_root: Hash64::from_u64_word(7),
            trace_root: Hash64::from_u64_word(8),
            output_root: Hash64::from_u64_word(9),
            pwu: u64::MAX,
            trace_manifest_root: Hash64::from_u64_word(10),
            trace_chunk_count: u32::MAX,
            trace_retention_daa: 0xABCD_1234_5678_90EF,
            execution_root: Hash64::from_u64_word(11),
        },
        signature: vec![0xB8; sig_len],
    }
}

fn same_decoding(bytes: &[u8]) {
    let ordinary = PalwAttemptEnvelopeV2::decode_wire(bytes);
    let bounded = decode_attempt(bytes);
    match (ordinary, bounded) {
        (Ok(a), Ok(b)) => assert_eq!(a, b),
        (Err(_), Err(_)) => {}
        (a, b) => panic!("ordinary/bounded codec disagree: {a:?} / {b:?}"),
    }
}

#[test]
fn exec_origin_attempt_codec_matches_ordinary_wire_without_imposing_current_shape_policy() {
    // Unsupported versions and non-admission key/signature sizes still decode: original accepted identity is authority.
    for version in [0, 2, 3, u16::MAX] {
        for key in [0, 1, 2592, 8192] {
            for sig in [0, 1, 4627, 8192] {
                let value = envelope(version, key, sig);
                let bytes = value.encode_wire();
                assert_eq!(decode_attempt(&bytes).unwrap(), value);
                same_decoding(&bytes);
            }
        }
    }
}

#[test]
fn exec_origin_attempt_codec_matches_truncation_trailing_magic_and_hostile_vector_lengths() {
    let value = envelope(3, 3, 5);
    let bytes = value.encode_wire();
    for end in 0..bytes.len() {
        same_decoding(&bytes[..end]);
        assert!(decode_attempt(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    same_decoding(&trailing);
    assert!(decode_attempt(&trailing).is_err());
    let mut magic = bytes.clone();
    magic[0] ^= 1;
    same_decoding(&magic);
    assert!(decode_attempt(&magic).is_err());
    let key_offset = PALW_ATTEMPT_V2_WIRE_MAGIC.len()
        + borsh::to_vec(&(
            value.attempt.version,
            value.attempt.network_domain,
            value.attempt.challenge,
            value.attempt.class_id,
            value.attempt.executor_bond,
        ))
        .unwrap()
        .len();
    let sig_offset = PALW_ATTEMPT_V2_WIRE_MAGIC.len() + borsh::to_vec(&value.attempt).unwrap().len();
    for offset in [key_offset, sig_offset] {
        for len in [u32::MAX, bytes.len() as u32 + 1] {
            let mut hostile = bytes.clone();
            hostile[offset..offset + 4].copy_from_slice(&len.to_le_bytes());
            assert!(decode_attempt(&hostile).is_err());
            same_decoding(&hostile);
        }
    }
}
