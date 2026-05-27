//! kaspa-pq Phase 7 (PR-7.6): production-width UTXO commitment.
//!
//! [`UtxoCommitment64`] is the 64-byte production form of the kaspa-pq
//! UTXO commitment field (see [ADR-0004](../../docs/adr/0004-utxo-commitment64.md)).
//! It is a dedicated newtype — **not** a wider [`kaspa_hashes::Hash`] —
//! because the rest of the consensus code uses 32-byte `Hash` for
//! everything (txid, block hash, merkle roots) and the kaspa-pq design
//! deliberately widens only the UTXO commitment to honestly carry the
//! ≥200-bit security claim of LtHash16_1024.
//!
//! The PoC-width [`kaspa_hashes::Hash`] commitment used in Phase 3–7.5
//! stays the active header field. The header switch to
//! [`UtxoCommitment64`] is the final follow-up — a single mechanical
//! type swap inside `Header::utxo_commitment` and a recompute of the
//! genesis hashes — once the surrounding RPC / WASM / SDK ecosystem
//! consumes the new type. Per ADR-0006 §"Implementation order", the
//! type lives here and in RPC core ahead of the header switch so the
//! switch PR is small.
//!
//! Construction comes from [`kaspa_muhash::MuHash::finalize_64`]:
//! `BLAKE2b-512(LtHash16_1024 state)` over the full 2048-byte
//! accumulator state.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use std::{
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
};

/// Byte width of the production kaspa-pq UTXO commitment. Locked at
/// this crate level; downstream code that needs the integer must
/// pull it from here rather than redefining.
pub const UTXO_COMMITMENT_64_BYTES: usize = 64;

/// 64-byte UTXO commitment (production width).
///
/// Construction-site invariants:
///
///  - Always materialised through `kaspa_muhash::MuHash::finalize_64`
///    (Blake2b-512 of the 2048-byte LtHash state).
///  - Never produced by truncating, padding, or otherwise altering
///    the 32-byte PoC commitment. The two forms are not
///    interchangeable; that is the whole point of the dedicated
///    newtype (see ADR-0004 §"Type discipline").
///
/// **No `From<Hash>` and no `From<UtxoCommitment64> for Hash` impls
/// are provided.** Downstream consumers that need a 32-byte view
/// must define their own explicit truncation and own its semantics.
#[derive(Copy, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct UtxoCommitment64(pub [u8; UTXO_COMMITMENT_64_BYTES]);

/// Error returned when a hex-encoded [`UtxoCommitment64`] fails to parse.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum UtxoCommitment64ParseError {
    #[error("expected {expected} hex characters, got {got}")]
    WrongHexLength { expected: usize, got: usize },
    #[error("invalid hex: {0}")]
    Hex(String),
}

impl UtxoCommitment64 {
    /// Construct from a fixed-size byte array. Use only at the
    /// `MuHash::finalize_64` boundary or in tests / RPC parsers.
    #[inline]
    pub const fn new(bytes: [u8; UTXO_COMMITMENT_64_BYTES]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8; UTXO_COMMITMENT_64_BYTES] {
        &self.0
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut out = vec![0u8; UTXO_COMMITMENT_64_BYTES * 2];
        faster_hex::hex_encode(&self.0, &mut out).expect("output is exactly twice the input");
        unsafe { String::from_utf8_unchecked(out) }
    }

    pub fn from_hex(hex: &str) -> Result<Self, UtxoCommitment64ParseError> {
        if hex.len() != UTXO_COMMITMENT_64_BYTES * 2 {
            return Err(UtxoCommitment64ParseError::WrongHexLength {
                expected: UTXO_COMMITMENT_64_BYTES * 2,
                got: hex.len(),
            });
        }
        let mut out = [0u8; UTXO_COMMITMENT_64_BYTES];
        faster_hex::hex_decode(hex.as_bytes(), &mut out).map_err(|e| UtxoCommitment64ParseError::Hex(e.to_string()))?;
        Ok(Self(out))
    }

    /// All-zero commitment. Structurally valid but **never** the value
    /// returned by `MuHash::finalize_64` for any reachable accumulator
    /// state — the empty-state finalize is a non-trivial digest.
    /// `Default::default()` resolves here.
    #[inline]
    pub const fn zero() -> Self {
        Self([0u8; UTXO_COMMITMENT_64_BYTES])
    }
}

impl Default for UtxoCommitment64 {
    fn default() -> Self {
        Self::zero()
    }
}

impl Debug for UtxoCommitment64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "UtxoCommitment64({})", self.to_hex())
    }
}

impl Display for UtxoCommitment64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for UtxoCommitment64 {
    type Err = UtxoCommitment64ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl Serialize for UtxoCommitment64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for UtxoCommitment64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = UtxoCommitment64;
            fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                write!(f, "a {}-character lowercase hex string", UTXO_COMMITMENT_64_BYTES * 2)
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<UtxoCommitment64, E> {
                UtxoCommitment64::from_hex(s).map_err(de::Error::custom)
            }
            fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> Result<UtxoCommitment64, E> {
                UtxoCommitment64::from_hex(s).map_err(de::Error::custom)
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<UtxoCommitment64, E> {
                UtxoCommitment64::from_hex(&s).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let mut bytes = [0u8; UTXO_COMMITMENT_64_BYTES];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7);
        }
        let c = UtxoCommitment64::new(bytes);
        let h = c.to_hex();
        assert_eq!(h.len(), UTXO_COMMITMENT_64_BYTES * 2);
        let back = UtxoCommitment64::from_hex(&h).unwrap();
        assert_eq!(back, c);
        let back_str: UtxoCommitment64 = h.parse().unwrap();
        assert_eq!(back_str, c);
    }

    #[test]
    fn hex_wrong_length_rejected() {
        assert_eq!(
            UtxoCommitment64::from_hex("00").unwrap_err(),
            UtxoCommitment64ParseError::WrongHexLength { expected: 128, got: 2 },
        );
    }

    #[test]
    fn hex_invalid_chars_rejected() {
        // Right length, wrong alphabet.
        let bad: String = "zz".repeat(UTXO_COMMITMENT_64_BYTES);
        assert!(matches!(UtxoCommitment64::from_hex(&bad), Err(UtxoCommitment64ParseError::Hex(_))));
    }

    #[test]
    fn borsh_roundtrip() {
        let c = UtxoCommitment64::new([0x77; UTXO_COMMITMENT_64_BYTES]);
        let bytes = borsh::to_vec(&c).unwrap();
        assert_eq!(bytes.len(), UTXO_COMMITMENT_64_BYTES);
        let back: UtxoCommitment64 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn json_roundtrip() {
        let c = UtxoCommitment64::new([0xee; UTXO_COMMITMENT_64_BYTES]);
        let s = serde_json::to_string(&c).unwrap();
        // 64 bytes -> 128-char hex + 2 quotes = 130 chars.
        assert_eq!(s.len(), 130);
        let back: UtxoCommitment64 = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn default_is_zero() {
        let c = UtxoCommitment64::default();
        assert_eq!(c, UtxoCommitment64::zero());
        assert_eq!(c.as_bytes(), &[0u8; UTXO_COMMITMENT_64_BYTES]);
    }

    #[test]
    fn debug_includes_full_hex() {
        let c = UtxoCommitment64::new([0xab; UTXO_COMMITMENT_64_BYTES]);
        let dbg = format!("{c:?}");
        assert!(dbg.starts_with("UtxoCommitment64("));
        assert!(dbg.contains(&"ab".repeat(UTXO_COMMITMENT_64_BYTES)));
    }
}
