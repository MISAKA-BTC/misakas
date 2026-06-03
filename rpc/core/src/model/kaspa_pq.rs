//! kaspa-pq Phase 7 (PR-7.1): RPC byte-typed wire fields for the
//! kaspa-pq cryptographic primitives.
//!
//! These newtypes carry the fixed-size byte blobs that the kaspa-pq
//! consensus produces:
//!
//! - [`RpcMlDsa87PublicKey`] — 2592 bytes (ADR-0002).
//! - [`RpcMlDsa87Signature`] — 4627 bytes (ADR-0002).
//!
//! The UTXO commitment travels on the wire as a 64-byte `Hash64` (the
//! production accumulator width, ADR-0004 §"Decision"); the RPC layer
//! reuses the existing `Hash64` hex encoding for it rather than a
//! dedicated newtype.
//!
//! Wire formats:
//!
//! - **Borsh** — fixed-size byte array, native (Borsh handles
//!   primitive arrays of any length).
//! - **serde JSON** — lowercase hex string of length `2 * N`, with
//!   the length validated at deserialize time. The encoding is
//!   `serialize_str` / `deserialize_str` rather than `serde_bytes` so
//!   that JSON clients can read the fields with no special framing.
//!
//! Display / Debug / FromStr / parse match the JSON form (lowercase
//! hex), so log lines reproduce the wire value verbatim.

use std::{
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

/// ML-DSA-87 (FIPS 204) public key length in bytes. Locked at this
/// crate level to avoid pulling in `kaspa_txscript` purely for the
/// constant — the value must match `kaspa_txscript::MLDSA87_PK_LEN`
/// (asserted by [`tests::pq_constants_match_txscript`]).
pub const RPC_MLDSA87_PK_LEN: usize = 2592;

/// ML-DSA-87 signature length in bytes. Same alignment-with-txscript
/// contract as [`RPC_MLDSA87_PK_LEN`].
pub const RPC_MLDSA87_SIG_LEN: usize = 4627;

/// 2592-byte ML-DSA-87 public key, RPC-serialized as a 5184-character
/// lowercase hex string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcMlDsa87PublicKey(pub [u8; RPC_MLDSA87_PK_LEN]);

/// 4627-byte ML-DSA-87 signature, RPC-serialized as a 9254-character
/// lowercase hex string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcMlDsa87Signature(pub [u8; RPC_MLDSA87_SIG_LEN]);

/// Error returned when a hex-encoded kaspa-pq RPC field fails to parse.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RpcPqParseError {
    #[error("expected {expected} hex characters, got {got}")]
    WrongHexLength { expected: usize, got: usize },
    #[error("invalid hex: {0}")]
    Hex(String),
}

/// Common bytes-only API. Implemented manually rather than via a
/// trait so that downstream code can use the inherent methods without
/// importing a trait. Each newtype gets the same five methods:
/// `new`, `from_bytes`, `as_bytes`, `as_slice`, `to_hex`, `from_hex`.
macro_rules! impl_rpc_pq_bytes {
    ($name:ident, $len:expr, $kind:literal) => {
        impl $name {
            #[inline]
            pub const fn new(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            #[inline]
            pub fn from_bytes(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            #[inline]
            pub fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            #[inline]
            pub fn as_slice(&self) -> &[u8] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                let mut out = vec![0u8; $len * 2];
                faster_hex::hex_encode(&self.0, &mut out).expect("output is twice the input");
                // safety: hex output is ASCII.
                unsafe { String::from_utf8_unchecked(out) }
            }

            pub fn from_hex(hex: &str) -> Result<Self, RpcPqParseError> {
                if hex.len() != $len * 2 {
                    return Err(RpcPqParseError::WrongHexLength { expected: $len * 2, got: hex.len() });
                }
                let mut out = [0u8; $len];
                faster_hex::hex_decode(hex.as_bytes(), &mut out).map_err(|e| RpcPqParseError::Hex(e.to_string()))?;
                Ok(Self(out))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                // `RpcMlDsa87PublicKey` / `RpcMlDsa87Signature` are too large
                // for derive(Default) on stable; we provide an all-zeros
                // default, which is structurally valid but cryptographically
                // never produced by libcrux. Tests that depend on a
                // specific default must construct one explicitly.
                Self([0u8; $len])
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", $kind, self.to_hex())
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_hex())
            }
        }

        impl FromStr for $name {
            type Err = RpcPqParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::from_hex(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $name;
                    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                        write!(f, "a lowercase hex string of length {} encoding {}", $len * 2, $kind)
                    }
                    fn visit_str<E: de::Error>(self, s: &str) -> Result<$name, E> {
                        <$name>::from_hex(s).map_err(de::Error::custom)
                    }
                    fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> Result<$name, E> {
                        <$name>::from_hex(s).map_err(de::Error::custom)
                    }
                    fn visit_string<E: de::Error>(self, s: String) -> Result<$name, E> {
                        <$name>::from_hex(&s).map_err(de::Error::custom)
                    }
                }
                deserializer.deserialize_str(V)
            }
        }
    };
}

impl_rpc_pq_bytes!(RpcMlDsa87PublicKey, RPC_MLDSA87_PK_LEN, "RpcMlDsa87PublicKey");
impl_rpc_pq_bytes!(RpcMlDsa87Signature, RPC_MLDSA87_SIG_LEN, "RpcMlDsa87Signature");

// Bidirectional conversion between the consensus-core type and its
// RPC wire form. The two newtypes intentionally exist in separate
// crates so the RPC layer can evolve its on-wire encoding (Borsh /
// serde JSON hex) without touching `consensus_core`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_constants_match_txscript() {
        assert_eq!(RPC_MLDSA87_PK_LEN, kaspa_txscript::MLDSA87_PK_LEN);
        assert_eq!(RPC_MLDSA87_SIG_LEN, kaspa_txscript::MLDSA87_SIG_LEN);
    }





    #[test]
    fn pubkey_hex_roundtrip() {
        let mut bytes = [0u8; RPC_MLDSA87_PK_LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i & 0xff) as u8;
        }
        let pk = RpcMlDsa87PublicKey::new(bytes);
        let h = pk.to_hex();
        assert_eq!(h.len(), RPC_MLDSA87_PK_LEN * 2);
        let parsed = RpcMlDsa87PublicKey::from_hex(&h).unwrap();
        assert_eq!(parsed, pk);
        // FromStr matches from_hex.
        let parsed_str: RpcMlDsa87PublicKey = h.parse().unwrap();
        assert_eq!(parsed_str, pk);
    }

    #[test]
    fn signature_hex_roundtrip() {
        let mut bytes = [0u8; RPC_MLDSA87_SIG_LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((i * 3) & 0xff) as u8;
        }
        let sig = RpcMlDsa87Signature::new(bytes);
        let h = sig.to_hex();
        assert_eq!(h.len(), RPC_MLDSA87_SIG_LEN * 2);
        let parsed = RpcMlDsa87Signature::from_hex(&h).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn hex_wrong_length_rejected() {
        assert_eq!(
            RpcMlDsa87PublicKey::from_hex("00").unwrap_err(),
            RpcPqParseError::WrongHexLength { expected: RPC_MLDSA87_PK_LEN * 2, got: 2 },
        );
        assert_eq!(
            RpcMlDsa87Signature::from_hex(&"00".repeat(31)).unwrap_err(),
            RpcPqParseError::WrongHexLength { expected: RPC_MLDSA87_SIG_LEN * 2, got: 62 },
        );
    }

    #[test]
    fn hex_invalid_chars_rejected() {
        let mut bad = "zz".repeat(RPC_MLDSA87_PK_LEN);
        assert!(matches!(RpcMlDsa87PublicKey::from_hex(&bad), Err(RpcPqParseError::Hex(_))));
        // After fixing length, an embedded non-hex char still fails.
        bad = "00".repeat(RPC_MLDSA87_PK_LEN - 1) + "0g";
        assert!(matches!(RpcMlDsa87PublicKey::from_hex(&bad), Err(RpcPqParseError::Hex(_))));
    }

    #[test]
    fn borsh_roundtrip_pubkey() {
        let pk = RpcMlDsa87PublicKey::new([0x11; RPC_MLDSA87_PK_LEN]);
        let bytes = borsh::to_vec(&pk).unwrap();
        assert_eq!(bytes.len(), RPC_MLDSA87_PK_LEN);
        let parsed: RpcMlDsa87PublicKey = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, pk);
    }

    #[test]
    fn borsh_roundtrip_signature() {
        let sig = RpcMlDsa87Signature::new([0x22; RPC_MLDSA87_SIG_LEN]);
        let bytes = borsh::to_vec(&sig).unwrap();
        assert_eq!(bytes.len(), RPC_MLDSA87_SIG_LEN);
        let parsed: RpcMlDsa87Signature = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn serde_json_roundtrip_pubkey() {
        let pk = RpcMlDsa87PublicKey::new([0x44; RPC_MLDSA87_PK_LEN]);
        let s = serde_json::to_string(&pk).unwrap();
        // Wire form is a hex string (note the surrounding quotes).
        assert!(s.starts_with('"') && s.ends_with('"'));
        assert_eq!(s.len(), RPC_MLDSA87_PK_LEN * 2 + 2);
        let parsed: RpcMlDsa87PublicKey = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, pk);
    }

    #[test]
    fn serde_json_roundtrip_signature() {
        let sig = RpcMlDsa87Signature::new([0x55; RPC_MLDSA87_SIG_LEN]);
        let s = serde_json::to_string(&sig).unwrap();
        let parsed: RpcMlDsa87Signature = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn serde_json_rejects_wrong_length() {
        // A 30-byte hex string is rejected as RpcMlDsa87PublicKey.
        let too_short = format!("\"{}\"", "00".repeat(30));
        let err = serde_json::from_str::<RpcMlDsa87PublicKey>(&too_short).unwrap_err();
        assert!(err.to_string().contains("expected"), "got {err}");
    }

    #[test]
    fn display_and_debug_match_hex() {
        let c = RpcMlDsa87PublicKey::new([0xab; RPC_MLDSA87_PK_LEN]);
        let hex = c.to_hex();
        assert_eq!(format!("{c}"), hex);
        assert!(format!("{c:?}").contains(&hex));
    }

    /// PR-7.3 acceptance: the kaspa-pq RPC byte-typed types are
    /// drop-in usable through the `workflow_serializer` `store!` /
    /// `load!` macros that higher-level wRPC messages use for their
    /// field-by-field encoding. The macros delegate to `BorshSerialize`
    /// / `BorshDeserialize`, which the kaspa-pq types derive, so no
    /// additional `Serializer` / `Deserializer` impl is required (matching
    /// how `kaspa_hashes::Hash` is used in `RpcHeader::Serializer`).
    #[test]
    fn wrpc_store_load_roundtrip() {
        use workflow_serializer::prelude::{load, store};

        let pk = RpcMlDsa87PublicKey::new([0x88; RPC_MLDSA87_PK_LEN]);
        let sig = RpcMlDsa87Signature::new([0x99; RPC_MLDSA87_SIG_LEN]);

        // Emulate the per-message wRPC encoder layout: write a version
        // tag, then each field through store!.
        let mut buf = Vec::new();
        store!(u16, &1, &mut buf).unwrap();
        store!(RpcMlDsa87PublicKey, &pk, &mut buf).unwrap();
        store!(RpcMlDsa87Signature, &sig, &mut buf).unwrap();

        // Expected length: 2 (u16 version tag) + 2592 (pk) + 4627 (sig) = 7221.
        assert_eq!(buf.len(), 2 + RPC_MLDSA87_PK_LEN + RPC_MLDSA87_SIG_LEN);

        let mut r = std::io::Cursor::new(&buf[..]);
        let _ver = load!(u16, &mut r).unwrap();
        let pk_in = load!(RpcMlDsa87PublicKey, &mut r).unwrap();
        let sig_in = load!(RpcMlDsa87Signature, &mut r).unwrap();
        assert_eq!(pk_in, pk);
        assert_eq!(sig_in, sig);
    }
}
