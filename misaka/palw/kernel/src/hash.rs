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
    id(domain, &borsh::to_vec(object).expect("borsh into a Vec cannot fail"))
}

/// Hex, for messages.
pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}
