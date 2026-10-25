//! kaspa-pq UTXO accumulator: LtHash32_1024.
//!
//! This module replaces the upstream Kaspa 3072-bit multiplicative MuHash
//! with a 32-bit-lane, 1024-lane additive LtHash (Meta LtHash32_1024). The
//! struct name `MuHash` is **kept** during the kaspa-pq PoC so that
//! downstream call sites do not have to be retyped; the internal data and
//! the on-the-wire serialization, however, are completely different.
//!
//! Public surface that downstream crates rely on (preserved):
//!   - [`MuHash::new`]
//!   - [`MuHash::add_element`] / [`MuHash::remove_element`]
//!   - [`MuHash::add_element_builder`] / [`MuHash::remove_element_builder`]
//!     plus the [`MuHashElementBuilder`] hasher-style API.
//!   - [`MuHash::combine`] (component-wise addition mod 2^32).
//!   - [`MuHash::serialize`] / [`MuHash::deserialize`] (4096 bytes, was 384).
//!   - [`MuHash::finalize`] — kaspa-pq (ADR-0004 / design §12): returns the
//!     64-byte [`Hash64`] UTXO-set commitment (keyed BLAKE2b-512 under the
//!     `b"UtxoCommitment64"` domain), so it matches every other 64-byte PQ
//!     consensus identity. Only this final down-hash is 64-byte; the element
//!     hash and the u3072/LtHash math are unchanged.
//!   - [`EMPTY_MUHASH`] — finalize of a fresh accumulator (4096 zero bytes).
//!   - [`SERIALIZED_MUHASH_SIZE`] — now `LTHASH_STATE_BYTES` = 4096.
//!   - [`OverflowError`] / [`MuHashError`] — kept for API compat but never
//!     actually returned, because every byte string of the correct length
//!     decodes to a valid LtHash state.
//!
//! See docs/adr/0003-lthash-utxo-accumulator.md for the design rationale,
//! docs/adr/0004-utxo-commitment64.md for why the PoC finalize is still
//! 32 bytes.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64, Hasher, HasherBase, MuHashElementHash, UtxoCommitmentHash64};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use std::error::Error;
use std::fmt::Display;

/// Number of 32-bit lanes in the LtHash state.
pub const LTHASH_LANES: usize = 1024;
/// Bytes per LtHash lane (32-bit lanes => 4 bytes).
pub const LTHASH_LANE_BYTES: usize = 4;
/// Serialized LtHash state size in bytes (`LTHASH_LANES * LTHASH_LANE_BYTES`).
pub const LTHASH_STATE_BYTES: usize = LTHASH_LANES * LTHASH_LANE_BYTES;

/// Output size of `finalize()` in bytes. kaspa-pq (ADR-0004 / design §12):
/// the UTXO commitment is 64-byte (BLAKE2b-512) so it matches every other
/// PQ consensus identity.
pub const HASH_SIZE: usize = 64;

/// Serialized accumulator state size. Renamed conceptually to "LtHash state
/// size" — but the constant name is kept for source compatibility with the
/// many call sites that already reference `SERIALIZED_MUHASH_SIZE`.
pub const SERIALIZED_MUHASH_SIZE: usize = LTHASH_STATE_BYTES;

/// `MuHash::new().finalize()` — the 64-byte commitment of an empty UTXO set.
///
/// kaspa-pq (ADR-0004 / design §12): concretely this is
/// `UtxoCommitmentHash64::hash([0u8; 4096])` (keyed BLAKE2b-512 over the
/// LtHash state). The value below is asserted at runtime by
/// `test_empty_hash`; if the underlying hasher or state size ever changes,
/// that test will fail and this constant must be re-derived from the test
/// panic.
pub const EMPTY_MUHASH: Hash64 = Hash64::from_bytes([
    0x7c, 0xa0, 0x83, 0xb7, 0xbf, 0x5b, 0xe3, 0x81, 0x23, 0xa2, 0x45, 0x77, 0x46, 0x16, 0x07, 0x6e, 0x1c, 0x5f, 0x04, 0x43, 0xa1,
    0xa7, 0xbe, 0x2a, 0xa8, 0xee, 0x7f, 0x6d, 0x7d, 0x10, 0x9e, 0x38, 0x6f, 0xe8, 0xb0, 0x97, 0xba, 0x7c, 0xa0, 0xb3, 0x9a, 0xd0,
    0xb3, 0x7d, 0xc4, 0x4b, 0x19, 0xc9, 0x6d, 0x7a, 0x68, 0x6a, 0xe5, 0x69, 0x4a, 0xd9, 0xdd, 0xed, 0x95, 0xee, 0xe2, 0xa2, 0x76,
    0xdc,
]);

/// LtHash32_1024 UTXO accumulator.
///
/// The state is `LTHASH_LANES` ( = 1024 ) lanes of 32 bits each. Add and
/// remove are component-wise addition and subtraction modulo `2^32`.
/// `combine` is component-wise addition.
///
/// Note: 32-bit lanes wrap after 2^32 identical additions. The kaspa-pq
/// design defends against this by **always** including the spending
/// outpoint `(txid, index)` in the element bytes (see
/// `consensus/core/src/muhash.rs::write_utxo`), which makes every UTXO
/// element uniquely tagged.
// `MuHash` derives `Borsh{Serialize,Deserialize}` natively because Borsh
// supports primitive arrays of any length. The serde `Serialize` /
// `Deserialize` impls are written by hand (further down this file) because
// serde does not provide derives for `[T; N]` with `N > 32`. The serde
// encoding is the same 4096-byte little-endian state that
// `MuHash::serialize` produces.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MuHash {
    /// `[u32; LTHASH_LANES]` stored as a fixed-size array so that
    /// `BorshSerialize` / `BorshDeserialize` are trivially correct
    /// (Borsh handles primitive arrays natively).
    lanes: [u32; LTHASH_LANES],
}

/// Kept for API compatibility with the upstream multiplicative MuHash.
/// LtHash32_1024 cannot overflow: every 4096-byte sequence is a valid
/// state, so this error is no longer constructed.
#[derive(Debug, PartialEq, Eq)]
pub struct OverflowError;

impl Display for OverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overflow in the MuHash field")
    }
}

impl Error for OverflowError {}

impl MuHash {
    /// Return an empty initialized accumulator. `finalize()` on a fresh
    /// `MuHash` equals [`EMPTY_MUHASH`].
    #[inline]
    pub fn new() -> Self {
        Self { lanes: [0u32; LTHASH_LANES] }
    }

    /// Hash `data` and add it to the accumulator. Supports arbitrary-length
    /// input via the keyed BLAKE2b-256 [`MuHashElementHash`] (domain
    /// separator `"MuHashElement"`).
    #[inline]
    pub fn add_element(&mut self, data: &[u8]) {
        let lanes = element_lanes(MuHashElementHash::hash(data));
        for (s, e) in self.lanes.iter_mut().zip(lanes.iter()) {
            *s = s.wrapping_add(*e);
        }
    }

    /// Hash `data` and remove it from the accumulator.
    #[inline]
    pub fn remove_element(&mut self, data: &[u8]) {
        let lanes = element_lanes(MuHashElementHash::hash(data));
        for (s, e) in self.lanes.iter_mut().zip(lanes.iter()) {
            *s = s.wrapping_sub(*e);
        }
    }

    /// Return a hasher-style builder that, on `finalize`, adds the hashed
    /// element to the accumulator.
    #[inline]
    pub fn add_element_builder(&mut self) -> MuHashElementBuilder<'_> {
        MuHashElementBuilder::new(&mut self.lanes, BuilderSign::Add)
    }

    /// Return a hasher-style builder that, on `finalize`, removes the
    /// hashed element from the accumulator.
    #[inline]
    pub fn remove_element_builder(&mut self) -> MuHashElementBuilder<'_> {
        MuHashElementBuilder::new(&mut self.lanes, BuilderSign::Remove)
    }

    /// Merge `other`'s lanes into `self`. Equivalent to manually applying
    /// every add/remove operation that produced `other`.
    #[inline]
    pub fn combine(&mut self, other: &Self) {
        for (s, o) in self.lanes.iter_mut().zip(other.lanes.iter()) {
            *s = s.wrapping_add(*o);
        }
    }

    /// kaspa-pq (ADR-0004 / design §12): 64-byte finalize — keyed
    /// `BLAKE2b-512` (`UtxoCommitmentHash64`, domain `"UtxoCommitment64"`)
    /// over the 4096-byte serialized LtHash state. This is the UTXO-set
    /// commitment stored in the block header's `utxo_commitment` field; it
    /// is 64-byte so it carries the full security margin of LtHash32_1024
    /// and matches every other PQ consensus identity. The element hash and
    /// the LtHash group math are unchanged — only this final down-hash is
    /// 64-byte. The `&mut self` receiver is retained for source
    /// compatibility with upstream Kaspa's `MuHash::finalize`; no
    /// normalization is performed.
    #[inline]
    pub fn finalize(&mut self) -> Hash64 {
        let bytes = self.serialize();
        UtxoCommitmentHash64::hash(bytes)
    }

    /// Serialize the LtHash state as little-endian 32-bit lanes. The byte
    /// length is `SERIALIZED_MUHASH_SIZE` = 4096.
    #[inline]
    pub fn serialize(&mut self) -> [u8; SERIALIZED_MUHASH_SIZE] {
        let mut out = [0u8; SERIALIZED_MUHASH_SIZE];
        for (i, lane) in self.lanes.iter().enumerate() {
            let off = i * LTHASH_LANE_BYTES;
            out[off..off + LTHASH_LANE_BYTES].copy_from_slice(&lane.to_le_bytes());
        }
        out
    }

    /// Deserialize a 4096-byte LtHash state. Always succeeds — every byte
    /// sequence of the correct length is a valid state. The `Result`
    /// signature is preserved for source compatibility with upstream
    /// Kaspa's `MuHash::deserialize`.
    #[inline]
    pub fn deserialize(data: [u8; SERIALIZED_MUHASH_SIZE]) -> Result<Self, OverflowError> {
        let mut lanes = [0u32; LTHASH_LANES];
        for (i, lane) in lanes.iter_mut().enumerate() {
            let off = i * LTHASH_LANE_BYTES;
            *lane = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
        }
        Ok(Self { lanes })
    }
}

#[derive(Debug)]
pub enum MuHashError {
    /// Retained for API compatibility with upstream Kaspa, where a `MuHash`
    /// could not be unwrapped to a `Uint3072` (the multiplicative-MuHash
    /// field element) while the denominator was still non-trivial.
    /// LtHash32_1024 has no denominator, so this variant is never
    /// produced by kaspa-pq code, but downstream `match` arms that
    /// handle it remain valid.
    NonNormalizedValue,
}

/// LtHash element-to-lane-vector expansion.
///
/// The 32-byte BLAKE2b-256 element fingerprint seeds a ChaCha20 stream,
/// which produces `LTHASH_STATE_BYTES` of pseudo-random bytes; these are
/// reinterpreted as `LTHASH_LANES` little-endian `u16`s.
///
/// This is a domain-separation choice the kaspa-pq PoC inherits from
/// upstream Kaspa's element-to-U3072 expansion (which used the same
/// MuHashElementHash + ChaCha20 chain, sized to 384 bytes). The ADR
/// allows a BLAKE3 XOF as an alternative; we keep the ChaCha20 chain for
/// the PoC because it (a) needs no new dependency and (b) reuses the
/// already-domain-separated `MuHashElementHash` so add/remove are bound
/// to the kaspa-pq UTXO context.
#[inline]
fn element_lanes(hash: Hash) -> [u32; LTHASH_LANES] {
    let mut bytes = [0u8; LTHASH_STATE_BYTES];
    let mut stream = ChaCha20Rng::from_seed(hash.as_bytes());
    stream.fill_bytes(&mut bytes);
    let mut lanes = [0u32; LTHASH_LANES];
    for (i, lane) in lanes.iter_mut().enumerate() {
        let off = i * LTHASH_LANE_BYTES;
        *lane = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
    }
    lanes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuilderSign {
    Add,
    Remove,
}

/// Hasher-style builder for incrementally feeding data into the
/// accumulator. The `finalize(self)` step applies the resulting element
/// to the borrowed `lanes` array using the configured sign.
pub struct MuHashElementBuilder<'a> {
    lanes_field: &'a mut [u32; LTHASH_LANES],
    sign: BuilderSign,
    element_hasher: MuHashElementHash,
}

impl HasherBase for MuHashElementBuilder<'_> {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.element_hasher.write(data);
        self
    }
}

impl<'a> MuHashElementBuilder<'a> {
    fn new(lanes_field: &'a mut [u32; LTHASH_LANES], sign: BuilderSign) -> Self {
        Self { lanes_field, sign, element_hasher: MuHashElementHash::new() }
    }

    pub fn finalize(self) {
        let lanes = element_lanes(self.element_hasher.finalize());
        match self.sign {
            BuilderSign::Add => {
                for (s, e) in self.lanes_field.iter_mut().zip(lanes.iter()) {
                    *s = s.wrapping_add(*e);
                }
            }
            BuilderSign::Remove => {
                for (s, e) in self.lanes_field.iter_mut().zip(lanes.iter()) {
                    *s = s.wrapping_sub(*e);
                }
            }
        }
    }
}

impl Default for MuHash {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for MuHash {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}

// Manual serde impls: see the comment above the `MuHash` struct. The wire
// format is the same 4096-byte little-endian state that
// `MuHash::serialize` produces, so swapping the on-disk store between
// borsh and serde-based encoders yields the same bytes.
impl Serialize for MuHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut bytes = [0u8; SERIALIZED_MUHASH_SIZE];
        for (i, lane) in self.lanes.iter().enumerate() {
            let off = i * LTHASH_LANE_BYTES;
            bytes[off..off + LTHASH_LANE_BYTES].copy_from_slice(&lane.to_le_bytes());
        }
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for MuHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MuHashVisitor;
        impl<'de> Visitor<'de> for MuHashVisitor {
            type Value = MuHash;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a {} -byte LtHash32_1024 state", SERIALIZED_MUHASH_SIZE)
            }

            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<MuHash, E> {
                if bytes.len() != SERIALIZED_MUHASH_SIZE {
                    return Err(E::invalid_length(bytes.len(), &self));
                }
                let mut data = [0u8; SERIALIZED_MUHASH_SIZE];
                data.copy_from_slice(bytes);
                MuHash::deserialize(data).map_err(|_| E::custom("LtHash deserialize failed"))
            }

            fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<MuHash, E> {
                self.visit_bytes(&bytes)
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<MuHash, A::Error> {
                // For sequence-style encoders (bincode in some configurations
                // serializes byte slices as sequences). Fill a buffer one
                // u8 at a time, then delegate to the byte path.
                let mut data = [0u8; SERIALIZED_MUHASH_SIZE];
                for (i, slot) in data.iter_mut().enumerate() {
                    *slot = seq.next_element::<u8>()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                if seq.next_element::<u8>()?.is_some() {
                    // Trailing bytes — but we already consumed `SERIALIZED_MUHASH_SIZE`,
                    // so any further element is an error.
                    return Err(de::Error::invalid_length(SERIALIZED_MUHASH_SIZE + 1, &self));
                }
                MuHash::deserialize(data).map_err(|_| de::Error::custom("LtHash deserialize failed"))
            }
        }
        deserializer.deserialize_bytes(MuHashVisitor)
    }
}

#[cfg(test)]
mod tests {
    use crate::{EMPTY_MUHASH, LTHASH_LANES, LTHASH_STATE_BYTES, MuHash, OverflowError, SERIALIZED_MUHASH_SIZE};
    use kaspa_hashes::Hash64;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    fn element_from_byte(b: u8) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0] = b;
        out
    }

    #[test]
    fn test_constants_match_spec() {
        // Locked by docs/kaspa-pq-spec.md §2.
        assert_eq!(LTHASH_LANES, 1024);
        assert_eq!(LTHASH_STATE_BYTES, 4096);
        assert_eq!(SERIALIZED_MUHASH_SIZE, LTHASH_STATE_BYTES);
    }

    #[test]
    fn test_empty_hash() {
        // The `EMPTY_MUHASH` constant must equal `MuHash::new().finalize()`.
        // If this test fails after a change to the underlying hasher, the
        // constant must be re-derived from `MuHash::new().finalize()` and
        // pasted back into `lib.rs`.
        let mut empty = MuHash::new();
        let got = empty.finalize();
        assert_eq!(got, EMPTY_MUHASH, "EMPTYHEX={}=END", got);
    }

    #[test]
    fn test_add_then_remove_is_empty() {
        let mut acc = MuHash::new();
        acc.add_element(b"hello kaspa-pq");
        acc.remove_element(b"hello kaspa-pq");
        assert_eq!(acc.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_order_independence() {
        // For any two permutations of the same set, the finalized commitment
        // must be equal.
        let mut a = MuHash::new();
        let mut b = MuHash::new();
        let mut c = MuHash::new();
        let elements: [&[u8]; 4] = [b"alpha", b"beta", b"gamma", b"delta"];
        for e in elements {
            a.add_element(e);
        }
        for e in elements.iter().rev() {
            b.add_element(e);
        }
        for &i in [2usize, 0, 3, 1].iter() {
            c.add_element(elements[i]);
        }
        assert_eq!(a.finalize(), b.finalize());
        assert_eq!(a.finalize(), c.finalize());
    }

    #[test]
    fn test_combine_equivalent_to_add() {
        // combine(a, b) must equal "add every element of b to a".
        let elements_a: [&[u8]; 3] = [b"a1", b"a2", b"a3"];
        let elements_b: [&[u8]; 3] = [b"b1", b"b2", b"b3"];

        let mut combined = MuHash::new();
        for e in elements_a {
            combined.add_element(e);
        }
        let mut other = MuHash::new();
        for e in elements_b {
            other.add_element(e);
        }
        combined.combine(&other);

        let mut by_hand = MuHash::new();
        for e in elements_a.iter().chain(elements_b.iter()) {
            by_hand.add_element(e);
        }

        assert_eq!(combined.finalize(), by_hand.finalize());
    }

    #[test]
    fn test_combine_inverts_remove() {
        // m1 = add(set), m2 = remove(set). combine(m1, m2) is empty.
        let mut m1 = MuHash::new();
        let mut m2 = MuHash::new();
        for e in [b"x".as_slice(), b"y", b"z"] {
            m1.add_element(e);
            m2.remove_element(e);
        }
        m1.combine(&m2);
        assert_eq!(m1.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_serialize_size_is_4096() {
        let mut acc = MuHash::new();
        acc.add_element(b"some data");
        let ser = acc.serialize();
        assert_eq!(ser.len(), LTHASH_STATE_BYTES);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut acc = MuHash::new();
        for s in ["one", "two", "three"] {
            acc.add_element(s.as_bytes());
        }
        let ser = acc.serialize();
        let mut roundtripped = MuHash::deserialize(ser).unwrap();
        assert_eq!(acc.finalize(), roundtripped.finalize());
    }

    #[test]
    fn test_deserialize_never_overflows() {
        // Every 4096-byte sequence is a valid LtHash state — this is the
        // explicit replacement for the old upstream `OverflowError`
        // behaviour. The function returns `Result` only for source
        // compatibility.
        let all_max = [0xffu8; SERIALIZED_MUHASH_SIZE];
        let _: Result<MuHash, OverflowError> = MuHash::deserialize(all_max);
        // Use the unwrap to assert it really did succeed:
        MuHash::deserialize(all_max).expect("LtHash deserialize is total");
    }

    #[test]
    fn test_random_add_remove_cancels() {
        // 1024 random adds matched 1:1 with the same 1024 removes -> empty.
        const LOOPS: usize = 1024;
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut set = MuHash::new();
        let list: Vec<_> = (0..LOOPS)
            .map(|_| {
                let mut data = [0u8; 100];
                rng.fill(&mut data[..]);
                set.add_element(&data);
                data
            })
            .collect();
        assert_ne!(set.finalize(), EMPTY_MUHASH);
        for elem in list.iter() {
            set.remove_element(elem);
        }
        assert_eq!(set.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_random_permutation_arithmetic() {
        // Adapted from upstream `test_random_muhash_arithmetic`: pick four
        // small elements with random add/remove signs, and permute the
        // order of operations. All permutations must yield the same
        // finalized commitment.
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..10 {
            let mut res = Hash64::default();
            let mut table = [0u8; 4];
            rng.fill(&mut table[..]);
            for order in 0..4 {
                let mut acc = MuHash::new();
                for i in 0..4 {
                    let t = table[i ^ order];
                    if (t & 4) != 0 {
                        acc.remove_element(&element_from_byte(t & 3));
                    } else {
                        acc.add_element(&element_from_byte(t & 3));
                    }
                }
                let out = acc.finalize();
                if order == 0 {
                    res = out;
                } else {
                    assert_eq!(res, out);
                }
            }
        }
    }

    #[test]
    fn test_builder_matches_direct() {
        // The hasher-style builder must produce the same accumulator as
        // calling `add_element` / `remove_element` directly with the
        // concatenated input bytes.
        let chunks: [&[u8]; 3] = [b"chunk-one", b"chunk-two", b"chunk-three"];

        let mut concat = Vec::new();
        for c in chunks {
            concat.extend_from_slice(c);
        }

        let mut direct = MuHash::new();
        direct.add_element(&concat);

        let mut via_builder = MuHash::new();
        {
            let mut b = via_builder.add_element_builder();
            for c in chunks {
                let _ = kaspa_hashes::HasherBase::update(&mut b, c);
            }
            b.finalize();
        }

        assert_eq!(direct.finalize(), via_builder.finalize());
    }
}
