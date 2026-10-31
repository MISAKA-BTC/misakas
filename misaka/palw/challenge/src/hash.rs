//! The hash suite every identity and seed in this crate is minted with (`HASH_SUITE_KEYED_BLAKE2B512_V1`): keyed BLAKE2b-512
//! whose key is the domain string, over `u64_le(len(payload)) ‖ payload`, where the payload is the canonical Borsh encoding of
//! a typed object. Domains are the RFC-0007 Part VI strings (`MISAKA/PALW/...`); none is shared with the kernel crate's
//! `misaka-palw/kernel/...` ids, Panel draws, tickets or generation `R`.

/// A 64-byte digest.
pub type Digest = [u8; 64];

/// `H(domain; payload)`. A domain longer than BLAKE2b's 64-byte key is a programming error (every domain is a constant here).
pub fn h(domain: &[u8], payload: &[u8]) -> Digest {
    assert!(domain.len() <= 64, "a hash domain is a BLAKE2b key: at most 64 bytes");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    s.update(&(payload.len() as u64).to_le_bytes());
    s.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    out
}

/// `H(domain; borsh(object))`.
pub fn object_id<T: borsh::BorshSerialize>(domain: &[u8], object: &T) -> Digest {
    assert!(domain.len() <= 64, "a hash domain is a BLAKE2b key: at most 64 bytes");
    let length = borsh::object_length(object).expect("in-memory Borsh identity length");
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    state.update(&(length as u64).to_le_bytes());
    let mut writer = IdentityWriter { state, written: 0 };
    object.serialize(&mut writer).expect("in-memory Borsh identity serialization");
    assert_eq!(writer.written, length, "Borsh identity serialization changed length between passes");
    let mut out = [0; 64];
    out.copy_from_slice(writer.state.finalize().as_bytes());
    out
}

struct IdentityWriter {
    state: blake2b_simd::State,
    written: usize,
}
impl std::io::Write for IdentityWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.written = self.written.checked_add(bytes.len()).ok_or_else(|| std::io::Error::other("Borsh identity length overflow"))?;
        self.state.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The id of a named algorithm or policy this binary implements: `H("MISAKA/PALW/ALGORITHM-ID/V1"; name)`.
pub fn named_id(name: &str) -> Digest {
    h(DOMAIN_ALGORITHM_ID, name.as_bytes())
}

pub const DOMAIN_ALGORITHM_ID: &[u8] = b"MISAKA/PALW/ALGORITHM-ID/V1";
pub const DOMAIN_POLICY: &[u8] = b"MISAKA/PALW/CHALLENGE-POLICY/V1";
pub const DOMAIN_WORK_BEACON_ITEM: &[u8] = b"MISAKA/PALW/WORK-BEACON/ITEM/V1";
pub const DOMAIN_WORK_BEACON: &[u8] = b"MISAKA/PALW/WORK-BEACON/V1";
pub const DOMAIN_WORK_BEACON_MIX: &[u8] = b"MISAKA/PALW/WORK-BEACON/MIX/V1";
pub const DOMAIN_CHALLENGE_ANCHOR: &[u8] = b"MISAKA/PALW/CHALLENGE-ANCHOR/V1";
pub const DOMAIN_CHALLENGE: &[u8] = b"MISAKA/PALW/CHALLENGE/V1";
pub const DOMAIN_STREAM_KEY: &[u8] = b"MISAKA/PALW/CHALLENGE-STREAM/KEY/V1";
pub const DOMAIN_STREAM_BLOCK: &[u8] = b"MISAKA/PALW/CHALLENGE-STREAM/BLOCK/V1";
pub const DOMAIN_STAGED_ROUND: &[u8] = b"MISAKA/PALW/CHALLENGE/STAGED-ROUND/V1";
pub const DOMAIN_FS_ROUND: &[u8] = b"MISAKA/PALW/CHALLENGE/FS-ROUND/V1";
pub const DOMAIN_SUBJECT: &[u8] = b"MISAKA/PALW/CHALLENGE-SUBJECT/V1";
pub const DOMAIN_CONFORMANCE_COMMITMENT: &[u8] = b"MISAKA/PALW/CONFORMANCE-COMMITMENT/V1";
pub const DOMAIN_CONFORMANCE_EVIDENCE: &[u8] = b"MISAKA/PALW/CONFORMANCE-EVIDENCE/V1";
pub const DOMAIN_LOCK_EVIDENCE: &[u8] = b"MISAKA/PALW/WORK-BEACON/LOCK-EVIDENCE/V1";
pub const DOMAIN_BEACON_CONTEXT: &[u8] = b"MISAKA/PALW/WORK-BEACON/CONTEXT/V1";

/// Lower-case hex, for messages and JSON tool records.
pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod identity_controls {
    use super::*;
    #[test]
    fn streaming_keeps_legacy_framing_for_subject_collections_and_capacity_boundaries() {
        let mut profiles = std::collections::BTreeSet::new();
        profiles.insert([7u8; 64]);
        profiles.insert([2; 64]);
        for size in [0, 1, 1023, 1024, 1025, 65537] {
            let object = (&profiles, vec![11u8; size], Some("公開"));
            assert_eq!(
                object_id(b"MISAKA/PALW/IDENTITY-COMPATIBILITY/CONTROL", &object),
                h(b"MISAKA/PALW/IDENTITY-COMPATIBILITY/CONTROL", &borsh::to_vec(&object).unwrap())
            );
        }
    }
}
