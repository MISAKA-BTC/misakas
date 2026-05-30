//! kaspa-pq Phase 7 (PR-7.1): RPC byte-typed wire fields for the
//! kaspa-pq cryptographic primitives.
//!
//! These newtypes carry the fixed-size byte blobs that the kaspa-pq
//! consensus produces:
//!
//! - [`RpcMlDsa65PublicKey`] — 1952 bytes (ADR-0002).
//! - [`RpcMlDsa65Signature`] — 3309 bytes (ADR-0002).
//! - [`RpcUtxoCommitment`]   — 32 bytes, the kaspa-pq PoC final
//!   commitment width (see ADR-0004 §"Decision"). The production
//!   64-byte switch lands in PR-7.6 and introduces a separate
//!   `RpcUtxoCommitment64` type rather than widening this one.
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

/// ML-DSA-65 (FIPS 204) public key length in bytes. Locked at this
/// crate level to avoid pulling in `kaspa_txscript` purely for the
/// constant — the value must match `kaspa_txscript::MLDSA65_PK_LEN`
/// (asserted by [`tests::pq_constants_match_txscript`]).
pub const RPC_MLDSA65_PK_LEN: usize = 2592;

/// ML-DSA-65 signature length in bytes. Same alignment-with-txscript
/// contract as [`RPC_MLDSA65_PK_LEN`].
pub const RPC_MLDSA65_SIG_LEN: usize = 4627;

/// kaspa-pq PoC UTXO-commitment width in bytes (32). The production
/// 64-byte switch is `RpcUtxoCommitment64` (added in PR-7.6).
pub const RPC_UTXO_COMMITMENT_LEN: usize = 32;

/// kaspa-pq production-width UTXO commitment in bytes (64). PR-7.6.
pub const RPC_UTXO_COMMITMENT_64_LEN: usize = 64;

/// 1952-byte ML-DSA-65 public key, RPC-serialized as a 3904-character
/// lowercase hex string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcMlDsa65PublicKey(pub [u8; RPC_MLDSA65_PK_LEN]);

/// 3309-byte ML-DSA-65 signature, RPC-serialized as a 6618-character
/// lowercase hex string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcMlDsa65Signature(pub [u8; RPC_MLDSA65_SIG_LEN]);

/// 32-byte kaspa-pq UTXO commitment (PoC width). Production
/// 64-byte width is `RpcUtxoCommitment64`, introduced in PR-7.6.
/// `Default` is supplied by the [`impl_rpc_pq_bytes`] macro below
/// (not the derive list) for symmetry with the two ML-DSA types.
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcUtxoCommitment(pub [u8; RPC_UTXO_COMMITMENT_LEN]);

/// 64-byte kaspa-pq UTXO commitment, production width. PR-7.6.
///
/// Built from `kaspa_muhash::MuHash::finalize_64()` (BLAKE2b-512 of the
/// 2048-byte LtHash state). RPC tooling that needs to display or
/// verify the production-width commitment must use this newtype, not
/// [`RpcUtxoCommitment`]; the two are intentionally not
/// interchangeable (no `From` either direction — see ADR-0004
/// §"Type discipline").
#[derive(Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcUtxoCommitment64(pub [u8; RPC_UTXO_COMMITMENT_64_LEN]);

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
                // `RpcMlDsa65PublicKey` / `RpcMlDsa65Signature` are too large
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

impl_rpc_pq_bytes!(RpcMlDsa65PublicKey, RPC_MLDSA65_PK_LEN, "RpcMlDsa65PublicKey");
impl_rpc_pq_bytes!(RpcMlDsa65Signature, RPC_MLDSA65_SIG_LEN, "RpcMlDsa65Signature");
impl_rpc_pq_bytes!(RpcUtxoCommitment, RPC_UTXO_COMMITMENT_LEN, "RpcUtxoCommitment");
impl_rpc_pq_bytes!(RpcUtxoCommitment64, RPC_UTXO_COMMITMENT_64_LEN, "RpcUtxoCommitment64");

// Bidirectional conversion between the consensus-core type and its
// RPC wire form. The two newtypes intentionally exist in separate
// crates so the RPC layer can evolve its on-wire encoding (Borsh /
// serde JSON hex) without touching `consensus_core`.
impl From<kaspa_consensus_core::utxo_commitment::UtxoCommitment64> for RpcUtxoCommitment64 {
    fn from(c: kaspa_consensus_core::utxo_commitment::UtxoCommitment64) -> Self {
        RpcUtxoCommitment64(*c.as_bytes())
    }
}
impl From<RpcUtxoCommitment64> for kaspa_consensus_core::utxo_commitment::UtxoCommitment64 {
    fn from(c: RpcUtxoCommitment64) -> Self {
        kaspa_consensus_core::utxo_commitment::UtxoCommitment64::new(c.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_constants_match_txscript() {
        assert_eq!(RPC_MLDSA65_PK_LEN, kaspa_txscript::MLDSA65_PK_LEN);
        assert_eq!(RPC_MLDSA65_SIG_LEN, kaspa_txscript::MLDSA65_SIG_LEN);
    }

    #[test]
    fn utxo_commitment_64_constant_matches_consensus_core() {
        assert_eq!(RPC_UTXO_COMMITMENT_64_LEN, kaspa_consensus_core::utxo_commitment::UTXO_COMMITMENT_64_BYTES,);
    }

    #[test]
    fn utxo_commitment_64_consensus_core_roundtrip() {
        use kaspa_consensus_core::utxo_commitment::UtxoCommitment64;
        let bytes = [0xcdu8; RPC_UTXO_COMMITMENT_64_LEN];
        let core_form = UtxoCommitment64::new(bytes);
        let rpc_form: RpcUtxoCommitment64 = core_form.into();
        assert_eq!(rpc_form.as_bytes(), &bytes);
        let back: UtxoCommitment64 = rpc_form.into();
        assert_eq!(back, core_form);
    }

    #[test]
    fn utxo_commitment_64_serde_json_roundtrip() {
        let c = RpcUtxoCommitment64::new([0xefu8; RPC_UTXO_COMMITMENT_64_LEN]);
        let s = serde_json::to_string(&c).unwrap();
        // 64 bytes -> 128-char hex + 2 quotes = 130 chars.
        assert_eq!(s.len(), 130);
        let parsed: RpcUtxoCommitment64 = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, c);
    }

    #[test]
    fn utxo_commitment_64_borsh_roundtrip() {
        let c = RpcUtxoCommitment64::new([0x21u8; RPC_UTXO_COMMITMENT_64_LEN]);
        let bytes = borsh::to_vec(&c).unwrap();
        assert_eq!(bytes.len(), RPC_UTXO_COMMITMENT_64_LEN);
        let parsed: RpcUtxoCommitment64 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, c);
    }

    #[test]
    fn pubkey_hex_roundtrip() {
        let mut bytes = [0u8; RPC_MLDSA65_PK_LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i & 0xff) as u8;
        }
        let pk = RpcMlDsa65PublicKey::new(bytes);
        let h = pk.to_hex();
        assert_eq!(h.len(), RPC_MLDSA65_PK_LEN * 2);
        let parsed = RpcMlDsa65PublicKey::from_hex(&h).unwrap();
        assert_eq!(parsed, pk);
        // FromStr matches from_hex.
        let parsed_str: RpcMlDsa65PublicKey = h.parse().unwrap();
        assert_eq!(parsed_str, pk);
    }

    #[test]
    fn signature_hex_roundtrip() {
        let mut bytes = [0u8; RPC_MLDSA65_SIG_LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((i * 3) & 0xff) as u8;
        }
        let sig = RpcMlDsa65Signature::new(bytes);
        let h = sig.to_hex();
        assert_eq!(h.len(), RPC_MLDSA65_SIG_LEN * 2);
        let parsed = RpcMlDsa65Signature::from_hex(&h).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn utxo_commitment_hex_roundtrip() {
        let bytes = [0xa5u8; RPC_UTXO_COMMITMENT_LEN];
        let c = RpcUtxoCommitment::new(bytes);
        let h = c.to_hex();
        assert_eq!(h.len(), RPC_UTXO_COMMITMENT_LEN * 2);
        let parsed = RpcUtxoCommitment::from_hex(&h).unwrap();
        assert_eq!(parsed, c);
    }

    #[test]
    fn hex_wrong_length_rejected() {
        assert_eq!(
            RpcMlDsa65PublicKey::from_hex("00").unwrap_err(),
            RpcPqParseError::WrongHexLength { expected: RPC_MLDSA65_PK_LEN * 2, got: 2 },
        );
        assert_eq!(
            RpcUtxoCommitment::from_hex(&"00".repeat(31)).unwrap_err(),
            RpcPqParseError::WrongHexLength { expected: 64, got: 62 },
        );
    }

    #[test]
    fn hex_invalid_chars_rejected() {
        let mut bad = "zz".repeat(RPC_UTXO_COMMITMENT_LEN);
        assert!(matches!(RpcUtxoCommitment::from_hex(&bad), Err(RpcPqParseError::Hex(_))));
        // After fixing length, an embedded non-hex char still fails.
        bad = "00".repeat(RPC_UTXO_COMMITMENT_LEN - 1) + "0g";
        assert!(matches!(RpcUtxoCommitment::from_hex(&bad), Err(RpcPqParseError::Hex(_))));
    }

    #[test]
    fn borsh_roundtrip_pubkey() {
        let pk = RpcMlDsa65PublicKey::new([0x11; RPC_MLDSA65_PK_LEN]);
        let bytes = borsh::to_vec(&pk).unwrap();
        assert_eq!(bytes.len(), RPC_MLDSA65_PK_LEN);
        let parsed: RpcMlDsa65PublicKey = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, pk);
    }

    #[test]
    fn borsh_roundtrip_signature() {
        let sig = RpcMlDsa65Signature::new([0x22; RPC_MLDSA65_SIG_LEN]);
        let bytes = borsh::to_vec(&sig).unwrap();
        assert_eq!(bytes.len(), RPC_MLDSA65_SIG_LEN);
        let parsed: RpcMlDsa65Signature = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn borsh_roundtrip_utxo_commitment() {
        let c = RpcUtxoCommitment::new([0x33; RPC_UTXO_COMMITMENT_LEN]);
        let bytes = borsh::to_vec(&c).unwrap();
        assert_eq!(bytes.len(), RPC_UTXO_COMMITMENT_LEN);
        let parsed: RpcUtxoCommitment = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, c);
    }

    #[test]
    fn serde_json_roundtrip_pubkey() {
        let pk = RpcMlDsa65PublicKey::new([0x44; RPC_MLDSA65_PK_LEN]);
        let s = serde_json::to_string(&pk).unwrap();
        // Wire form is a hex string (note the surrounding quotes).
        assert!(s.starts_with('"') && s.ends_with('"'));
        assert_eq!(s.len(), RPC_MLDSA65_PK_LEN * 2 + 2);
        let parsed: RpcMlDsa65PublicKey = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, pk);
    }

    #[test]
    fn serde_json_roundtrip_signature() {
        let sig = RpcMlDsa65Signature::new([0x55; RPC_MLDSA65_SIG_LEN]);
        let s = serde_json::to_string(&sig).unwrap();
        let parsed: RpcMlDsa65Signature = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn serde_json_roundtrip_utxo_commitment() {
        let c = RpcUtxoCommitment::new([0x66; RPC_UTXO_COMMITMENT_LEN]);
        let s = serde_json::to_string(&c).unwrap();
        // 32 bytes -> 64-char hex + 2 quotes = 66 chars.
        assert_eq!(s.len(), 66);
        let parsed: RpcUtxoCommitment = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, c);
    }

    #[test]
    fn serde_json_rejects_wrong_length() {
        // A 30-byte hex string is rejected as RpcUtxoCommitment.
        let too_short = format!("\"{}\"", "00".repeat(30));
        let err = serde_json::from_str::<RpcUtxoCommitment>(&too_short).unwrap_err();
        assert!(err.to_string().contains("expected"), "got {err}");
    }

    #[test]
    fn display_and_debug_match_hex() {
        let c = RpcUtxoCommitment::new([0xab; RPC_UTXO_COMMITMENT_LEN]);
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

        let pk = RpcMlDsa65PublicKey::new([0x88; RPC_MLDSA65_PK_LEN]);
        let sig = RpcMlDsa65Signature::new([0x99; RPC_MLDSA65_SIG_LEN]);
        let commitment = RpcUtxoCommitment::new([0xaa; RPC_UTXO_COMMITMENT_LEN]);

        // Emulate the per-message wRPC encoder layout: write a version
        // tag, then each field through store!.
        let mut buf = Vec::new();
        store!(u16, &1, &mut buf).unwrap();
        store!(RpcMlDsa65PublicKey, &pk, &mut buf).unwrap();
        store!(RpcMlDsa65Signature, &sig, &mut buf).unwrap();
        store!(RpcUtxoCommitment, &commitment, &mut buf).unwrap();

        // Expected length: 2 (u16 version tag) + 1952 (pk) + 3309 (sig)
        //                + 32 (commitment) = 5295.
        assert_eq!(buf.len(), 2 + RPC_MLDSA65_PK_LEN + RPC_MLDSA65_SIG_LEN + RPC_UTXO_COMMITMENT_LEN);

        let mut r = std::io::Cursor::new(&buf[..]);
        let _ver = load!(u16, &mut r).unwrap();
        let pk_in = load!(RpcMlDsa65PublicKey, &mut r).unwrap();
        let sig_in = load!(RpcMlDsa65Signature, &mut r).unwrap();
        let c_in = load!(RpcUtxoCommitment, &mut r).unwrap();
        assert_eq!(pk_in, pk);
        assert_eq!(sig_in, sig);
        assert_eq!(c_in, commitment);
    }
}
