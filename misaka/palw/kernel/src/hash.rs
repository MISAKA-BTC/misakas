//! Keyed BLAKE2b-512, the construction every id in this crate is minted with: the key is the domain,
//! then the object's length as a little-endian `u64`, then its bytes. Domains are
//! `misaka-palw/kernel/...`; none of them is a consensus domain yet.

/// A 64-byte digest.
pub type Digest = [u8; 64];

pub fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

pub fn finish(state: blake2b_simd::State) -> Digest {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// `H(domain ‖ len_le64 ‖ bytes)`.
pub fn id(domain: &[u8], bytes: &[u8]) -> Digest {
    let mut s = keyed(domain);
    s.update(&(bytes.len() as u64).to_le_bytes());
    s.update(bytes);
    finish(s)
}

/// The id of a borsh-encodable object under `domain`.
pub fn object_id<T: borsh::BorshSerialize>(domain: &[u8], object: &T) -> Digest {
    // Borsh's length writer traverses fields without retaining an encoded
    // copy. Feed the same canonical bytes directly into the existing framed
    // hash, so a large claim/checker set needs no second allocation of itself.
    let length = borsh::object_length(object).expect("in-memory Borsh identity length");
    let mut state = keyed(domain);
    state.update(&(length as u64).to_le_bytes());
    let mut writer = IdentityWriter { state, written: 0 };
    object.serialize(&mut writer).expect("in-memory Borsh identity serialization");
    assert_eq!(writer.written, length, "Borsh identity serialization changed length between passes");
    finish(writer.state)
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

#[cfg(test)]
mod identity_controls {
    use super::*;
    #[test]
    fn streaming_keeps_legacy_framing_for_nested_collections_and_capacity_boundaries() {
        // Encoding compatibility only, not computation correctness or native
        // admission. These controls are not an executed acceptance report.
        let mut map = std::collections::BTreeMap::new();
        map.insert(9u64, Some("日本語".to_owned()));
        map.insert(1, None);
        for size in [0, 1, 1023, 1024, 1025, 65537] {
            let object = (map.clone(), vec![37u8; size], [Some(2u64), None]);
            let legacy = borsh::to_vec(&object).unwrap();
            assert_eq!(
                object_id(b"misaka-palw/kernel/id-compatibility/control", &object),
                id(b"misaka-palw/kernel/id-compatibility/control", &legacy)
            );
        }
    }
}

/// Hex, for messages.
pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}
