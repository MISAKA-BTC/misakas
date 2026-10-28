//! kaspa-pq Phase 9 (PR-9.2): 64-byte consensus identity type.
//!
//! `Hash64` is the kaspa-pq production-width hash type — it carries
//! block hash, transaction id, transaction hash, merkle root,
//! accepted-id merkle root, UTXO commitment, pruning point, and
//! parent references after the Phase 9 consensus migration (see
//! ADR-0008). Construction is via the keyed BLAKE2b-512 hashers
//! defined in [`crate::hashers`] (`BlockHash64`,
//! `TransactionHash64`, `TransactionId64`, etc.).
//!
//! The existing 32-byte [`crate::Hash`] type is retained for legacy
//! use sites (cache keys, debug fingerprints, the explicitly-
//! swappable Layer 1 kHeavyHash internals) and is re-exported as
//! [`crate::Hash32`].
//!
//! ## Security framing (verbatim from ADR-0008)
//!
//! - 512-bit commitment domain.
//! - 256-bit quantum preimage margin (Grover bound).
//! - "high-margin quantum collision resistance" — **not**
//!   "256-bit quantum collision". The BHT bound gives
//!   ~2^(512/3) ≈ 2^170.
//!
//! Downstream user-facing material **must** use these exact
//! phrasings.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    mem_size::MemSizeEstimator,
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use std::{
    array::TryFromSliceError,
    fmt::{Debug, Display, Formatter},
    hash::{Hash as StdHash, Hasher as StdHasher},
    str::{self, FromStr},
};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

pub const HASH64_SIZE: usize = 64;

/// 64-byte kaspa-pq consensus identity hash. See ADR-0008.
///
/// PR-9.5c adds the `#[wasm_bindgen]` + `CastFromJs` surface
/// mirroring the 32-byte [`crate::Hash`] type so downstream
/// `kaspa-consensus-client` WASM bindings compose against
/// `TransactionId`, `TransactionOutpoint`, and the other
/// PR-9.5c-widened consensus identities without further
/// per-call-site WASM glue.
#[derive(Eq, Clone, Copy, BorshSerialize, BorshDeserialize, CastFromJs)]
#[wasm_bindgen]
pub struct Hash64([u8; HASH64_SIZE]);

// PartialOrd / Ord are still derived but written separately so
// the `#[wasm_bindgen]` macro on the struct does not see them
// in the derive list (the macro's expanded code reserves a few
// trait names and the derive set must stay minimal — Hash and
// PartialEq are set up below alongside the manual impls).
impl PartialOrd for Hash64 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hash64 {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

// Manual serde impls. The upstream
// `kaspa_utils::serde_impl_*_fixed_bytes_ref!` macros assume
// `[u8; N]: serde::Deserialize` which serde only auto-implements for
// `N <= 32`. We hand-roll both directions so Hash64 round-trips through
// hex (human-readable) and raw bytes (compact) encoders with no
// dependency on a larger fixed-array deserialise impl.
impl Serialize for Hash64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            // 128-char lowercase hex (ADR-0008 RPC display width).
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Hash64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Hash64;
            fn expecting(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "a 64-byte Hash64 (either 128-char hex or 64 raw bytes)")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Hash64, E> {
                Hash64::from_str(s).map_err(de::Error::custom)
            }
            fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> Result<Hash64, E> {
                Hash64::from_str(s).map_err(de::Error::custom)
            }
            fn visit_bytes<E: de::Error>(self, b: &[u8]) -> Result<Hash64, E> {
                if b.len() != HASH64_SIZE {
                    return Err(E::invalid_length(b.len(), &self));
                }
                let mut out = [0u8; HASH64_SIZE];
                out.copy_from_slice(b);
                Ok(Hash64(out))
            }
            fn visit_byte_buf<E: de::Error>(self, b: Vec<u8>) -> Result<Hash64, E> {
                self.visit_bytes(&b)
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Hash64, A::Error> {
                // Some compact encoders serialize byte arrays as
                // length-prefixed sequences of u8. Fill one byte at
                // a time so we don't need `[u8; 64]: Deserialize`.
                let mut out = [0u8; HASH64_SIZE];
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = seq.next_element::<u8>()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                if seq.next_element::<u8>()?.is_some() {
                    return Err(de::Error::invalid_length(HASH64_SIZE + 1, &self));
                }
                Ok(Hash64(out))
            }
        }
        if deserializer.is_human_readable() { deserializer.deserialize_str(V) } else { deserializer.deserialize_bytes(V) }
    }
}

impl Default for Hash64 {
    #[inline]
    fn default() -> Self {
        Self([0u8; HASH64_SIZE])
    }
}

impl From<[u8; HASH64_SIZE]> for Hash64 {
    #[inline]
    fn from(value: [u8; HASH64_SIZE]) -> Self {
        Hash64(value)
    }
}

impl TryFrom<&[u8]> for Hash64 {
    type Error = TryFromSliceError;
    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Hash64::try_from_slice(value)
    }
}

impl Hash64 {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; HASH64_SIZE]) -> Self {
        Hash64(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(self) -> [u8; HASH64_SIZE] {
        self.0
    }

    #[inline(always)]
    pub const fn as_byte_slice(&self) -> &[u8; HASH64_SIZE] {
        &self.0
    }

    /// # Panics
    /// Panics if `bytes` is not exactly `HASH64_SIZE` long.
    #[inline(always)]
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self(<[u8; HASH64_SIZE]>::try_from(bytes).expect("Slice must have the length of Hash64"))
    }

    #[inline(always)]
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self(<[u8; HASH64_SIZE]>::try_from(bytes)?))
    }

    /// 8-lane little-endian u64 view. Matches the same access pattern
    /// `Hash::to_le_u64` provides for the 32-byte type, so DAG-side
    /// `BlockHasher` logic generalises cleanly when call sites move
    /// to `Hash64`.
    #[inline]
    pub fn to_le_u64(self) -> [u64; 8] {
        let mut out = [0u64; 8];
        for (slot, chunk) in out.iter_mut().zip(self.0.chunks_exact(8)) {
            *slot = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        out
    }

    #[inline]
    pub fn from_le_u64(arr: [u64; 8]) -> Self {
        let mut out = [0u8; HASH64_SIZE];
        for (chunk, word) in out.chunks_exact_mut(8).zip(arr.iter()) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        Self(out)
    }

    /// PR-9.5d: single-word constructor mirroring
    /// [`crate::Hash::from_u64_word`]. The word occupies the last
    /// (most-significant, in the 8-lane little-endian view) lane so
    /// small distinct integers map to distinct, BlockHasher-friendly
    /// `Hash64` values — used by `<int>.into()` test fixtures and by
    /// any consumer that needs a compact deterministic hash from a
    /// counter.
    #[inline(always)]
    pub fn from_u64_word(word: u64) -> Self {
        Self::from_le_u64([0, 0, 0, 0, 0, 0, 0, word])
    }
}

impl From<u64> for Hash64 {
    #[inline(always)]
    fn from(word: u64) -> Self {
        Self::from_u64_word(word)
    }
}

// Override the default `StdHash` so a `BlockHasher`-style consumer
// only sees the prefix u64 words — same trick the 32-byte `Hash`
// uses to keep the in-memory HashMap fast.
impl StdHash for Hash64 {
    #[inline]
    fn hash<H: StdHasher>(&self, state: &mut H) {
        for word in self.to_le_u64() {
            word.hash(state);
        }
    }
}

impl PartialEq for Hash64 {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Display for Hash64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; HASH64_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        // safety: hex output is always ASCII.
        f.write_str(unsafe { str::from_utf8_unchecked(&hex) })
    }
}

impl Debug for Hash64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl FromStr for Hash64 {
    type Err = faster_hex::Error;
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Strict length check: 128 hex characters exactly. faster_hex
        // does not always raise InvalidLength for shorter inputs
        // because the output buffer width is fixed, so guard
        // explicitly here.
        if s.len() != HASH64_SIZE * 2 {
            return Err(faster_hex::Error::InvalidLength(HASH64_SIZE * 2));
        }
        let mut bytes = [0u8; HASH64_SIZE];
        faster_hex::hex_decode(s.as_bytes(), &mut bytes)?;
        Ok(Hash64(bytes))
    }
}

impl AsRef<[u8; HASH64_SIZE]> for Hash64 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; HASH64_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for Hash64 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ToHex for Hash64 {
    fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl FromHex for Hash64 {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl MemSizeEstimator for Hash64 {}

// PR-9.5c WASM surface — mirrors the 32-byte `Hash`
// (`crypto/hashes/src/lib.rs`) so JS callers can construct,
// pretty-print, and pass `Hash64` values across the wasm-bindgen
// boundary the same way they do with the 32-byte type today. The
// only widening is the hex string length: 128 chars instead of
// 64, matching the `Display` impl above.
#[wasm_bindgen]
impl Hash64 {
    #[wasm_bindgen(constructor)]
    pub fn constructor(hex_str: &str) -> Self {
        Hash64::from_str(hex_str).expect("invalid Hash64 value")
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }
}

type Hash64TryFromError = workflow_wasm::error::Error;

impl TryCastFromJs for Hash64 {
    type Error = Hash64TryFromError;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let bytes = value.as_ref().try_as_vec_u8()?;
            Ok(Hash64(
                <[u8; HASH64_SIZE]>::try_from(bytes)
                    .map_err(|_| Hash64TryFromError::WrongSize("Slice must have the length of Hash64 (64 bytes)".into()))?,
            ))
        })
    }
}

/// The all-zero `Hash64` — structurally valid but **never** produced
/// by any consensus-grade BLAKE2b-512 hasher in this crate.
pub const ZERO_HASH64: Hash64 = Hash64([0u8; HASH64_SIZE]);

#[cfg(test)]
mod tests {
    use super::*;

    /// A small but non-trivial byte pattern used as the canonical
    /// test fixture across hex / serde / borsh roundtrips.
    fn fixture() -> [u8; HASH64_SIZE] {
        let mut b = [0u8; HASH64_SIZE];
        for (i, slot) in b.iter_mut().enumerate() {
            *slot = (i as u8).wrapping_mul(13).wrapping_add(7);
        }
        b
    }

    #[test]
    fn hash64_hex_roundtrip() {
        let h = Hash64::from_bytes(fixture());
        let s = h.to_string();
        // 64 bytes -> 128 hex characters. ADR-0008 binds the RPC
        // display width here, so this assertion doubles as a wire-
        // format check.
        assert_eq!(s.len(), HASH64_SIZE * 2);
        let h2: Hash64 = s.parse().unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn hash64_zero_not_displayed_as_empty() {
        // Make sure the all-zero hash still renders as 128 chars of '0'.
        let zero = ZERO_HASH64;
        let s = zero.to_string();
        assert_eq!(s.len(), HASH64_SIZE * 2);
        assert!(s.chars().all(|c| c == '0'));
    }

    #[test]
    fn hash64_from_str_rejects_wrong_length() {
        // 127 chars (one char short of 128) must fail.
        let s: String = "0".repeat(HASH64_SIZE * 2 - 1);
        assert!(Hash64::from_str(&s).is_err());
        // 130 chars (over by 2) must fail.
        let s: String = "0".repeat(HASH64_SIZE * 2 + 2);
        assert!(Hash64::from_str(&s).is_err());
    }

    #[test]
    fn hash64_borsh_roundtrip() {
        let h = Hash64::from_bytes(fixture());
        let bytes = borsh::to_vec(&h).unwrap();
        // Borsh of `[u8; 64]` is just the 64 bytes.
        assert_eq!(bytes.len(), HASH64_SIZE);
        let h2: Hash64 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn hash64_serde_json_roundtrip() {
        // serde uses the fixed-bytes-ref macros, which serialise as
        // a hex string for human-readable encoders (serde_json) and
        // as raw bytes for compact encoders. JSON path:
        let h = Hash64::from_bytes(fixture());
        let s = serde_json::to_string(&h).unwrap();
        let h2: Hash64 = serde_json::from_str(&s).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn hash64_le_u64_roundtrip() {
        let h = Hash64::from_bytes(fixture());
        let words = h.to_le_u64();
        assert_eq!(words.len(), 8);
        let back = Hash64::from_le_u64(words);
        assert_eq!(back, h);
    }
}
