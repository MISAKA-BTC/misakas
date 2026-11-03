//! Local construction upper bytes, not a wire field or remote resource claim.
//! Keep this function shared with gRPC's actual generated-message heap tests.
//!
//! Each generated UTXO entry owns nested optional messages plus independent
//! address, Hash64 hex and script hex strings. 512 bytes is an intentionally
//! conservative inline/nested-structure envelope on supported 64-bit targets;
//! 3x covers Vec growth overlap. Dynamic strings and construction scratch are
//! separate: address encoding can hold several base32 vectors, and current
//! to_rpc_hex holds both its 2x Vec and its copied 2x String at once.
pub const UTXO_PROTO_BASE_BYTES: u64 = 4096;
pub fn utxo_proto_per_entry_bytes(max_address_payload_bytes: u64, max_script_bytes: u64) -> Option<u64> {
    max_address_payload_bytes.checked_mul(8)
        .and_then(|n|max_script_bytes.checked_mul(4).and_then(|script|n.checked_add(script)))
        // address prefix/checksum allowance, Hash64 hex, allocation slack
        .and_then(|n|n.checked_add(3*512+64+128+64))
}
pub fn utxo_proto_reservation_bytes(entries: u64, max_address_payload_bytes: u64, max_script_bytes: u64) -> Option<u64> {
    utxo_proto_per_entry_bytes(max_address_payload_bytes, max_script_bytes)?.checked_mul(entries)?.checked_add(UTXO_PROTO_BASE_BYTES)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_response_keeps_outer_enum_and_overflow_never_wraps() {
        assert_eq!(utxo_proto_reservation_bytes(0, 64, 74), Some(4096));
        assert!(utxo_proto_reservation_bytes(u64::MAX, 64, 74).is_none());
        assert!(utxo_proto_per_entry_bytes(u64::MAX, 74).is_none());
        assert!(utxo_proto_per_entry_bytes(64, u64::MAX).is_none());
        assert!(utxo_proto_reservation_bytes(1, 64, 74).unwrap() > utxo_proto_reservation_bytes(1, 32, 34).unwrap());
    }
}
