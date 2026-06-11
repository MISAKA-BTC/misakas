//! kaspa-pq Selected-Parent EVM Lane (ADR-0020): the 32-byte
//! Ethereum-compatible hash type used by the EVM execution lane.
//!
//! `EvmH256` is a deliberate **32-byte** identity — it carries Ethereum
//! keccak256 / Merkle-Patricia-Trie roots (`stateRoot`, `transactionsRoot`,
//! `receiptsRoot`, EVM block hash, etc.) and must stay byte-for-byte
//! compatible with `geth`/`revm`/`ethers` so external Ethereum tooling can
//! verify our `eth_*` RPC output. It is therefore **not** widened to the
//! kaspa-pq 64-byte [`crate::Hash64`] consensus identity.
//!
//! The kaspa-side EVM commitment (`Header::evm_commitment_root`) is a
//! separate [`crate::Hash64`] (keyed BLAKE2b-512, MISAKA domain) over the
//! full `EvmExecutionHeader`; only the three Ethereum trie roots use
//! `EvmH256`.
//!
//! The surface mirrors the 32-byte [`crate::Hash`] type exactly (including
//! the `#[wasm_bindgen]` / `CastFromJs` glue) so it is a drop-in 32-byte
//! identity across RPC, serde, borsh and WASM with no per-call-site glue.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    mem_size::MemSizeEstimator,
    serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref,
};
use std::{
    array::TryFromSliceError,
    fmt::{Debug, Display, Formatter},
    hash::{Hash as StdHash, Hasher as StdHasher},
    str::{self, FromStr},
};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

/// Width of an [`EvmH256`] in bytes (Ethereum H256).
pub const EVM_H256_SIZE: usize = 32;

/// 32-byte Ethereum-compatible hash (keccak256 / MPT root). See ADR-0020.
///
/// @category Consensus
#[derive(Eq, Clone, Copy, Default, PartialOrd, Ord, BorshSerialize, BorshDeserialize, CastFromJs)]
#[wasm_bindgen]
pub struct EvmH256([u8; EVM_H256_SIZE]);

serde_impl_ser_fixed_bytes_ref!(EvmH256, EVM_H256_SIZE);
serde_impl_deser_fixed_bytes_ref!(EvmH256, EVM_H256_SIZE);

impl From<[u8; EVM_H256_SIZE]> for EvmH256 {
    fn from(value: [u8; EVM_H256_SIZE]) -> Self {
        EvmH256(value)
    }
}

impl TryFrom<&[u8]> for EvmH256 {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        EvmH256::try_from_slice(value)
    }
}

impl EvmH256 {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; EVM_H256_SIZE]) -> Self {
        EvmH256(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(self) -> [u8; EVM_H256_SIZE] {
        self.0
    }

    #[inline(always)]
    /// # Panics
    /// Panics if `bytes` length is not exactly `EVM_H256_SIZE`.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self(<[u8; EVM_H256_SIZE]>::try_from(bytes).expect("Slice must have the length of EvmH256"))
    }

    #[inline(always)]
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self(<[u8; EVM_H256_SIZE]>::try_from(bytes)?))
    }

    #[inline(always)]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; EVM_H256_SIZE]
    }
}

// Mirror the 32-byte `Hash`: override `StdHash`/`PartialEq` rather than
// derive, so the `#[wasm_bindgen]` macro on the struct sees a minimal
// derive set.
impl StdHash for EvmH256 {
    #[inline(always)]
    fn hash<H: StdHasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for EvmH256 {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Display for EvmH256 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; EVM_H256_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(unsafe { str::from_utf8_unchecked(&hex) })
    }
}

impl Debug for EvmH256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl FromStr for EvmH256 {
    type Err = faster_hex::Error;

    #[inline]
    fn from_str(hash_str: &str) -> Result<Self, Self::Err> {
        // Accept an optional `0x` prefix (Ethereum convention) but store bare bytes.
        let hash_str = hash_str.strip_prefix("0x").or_else(|| hash_str.strip_prefix("0X")).unwrap_or(hash_str);
        let mut bytes = [0u8; EVM_H256_SIZE];
        faster_hex::hex_decode(hash_str.as_bytes(), &mut bytes)?;
        Ok(EvmH256(bytes))
    }
}

impl AsRef<[u8; EVM_H256_SIZE]> for EvmH256 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; EVM_H256_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for EvmH256 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ToHex for EvmH256 {
    fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl FromHex for EvmH256 {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl MemSizeEstimator for EvmH256 {}

#[wasm_bindgen]
impl EvmH256 {
    #[wasm_bindgen(constructor)]
    pub fn constructor(hex_str: &str) -> Self {
        EvmH256::from_str(hex_str).expect("invalid EvmH256 value")
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }
}

type TryFromError = workflow_wasm::error::Error;
impl TryCastFromJs for EvmH256 {
    type Error = TryFromError;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let bytes = value.as_ref().try_as_vec_u8()?;
            Ok(EvmH256(
                <[u8; EVM_H256_SIZE]>::try_from(bytes)
                    .map_err(|_| TryFromError::WrongSize("Slice must have the length of EvmH256".into()))?,
            ))
        })
    }
}

/// The all-zero [`EvmH256`]. Also the natural default / "EVM-inert" value
/// carried by pre-activation (header version &lt; `EVM_HEADER_VERSION`) headers.
pub const ZERO_EVM_H256: EvmH256 = EvmH256([0; EVM_H256_SIZE]);

#[cfg(test)]
mod tests {
    use super::EvmH256;
    use std::str::FromStr;

    #[test]
    fn test_evm_h256_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = EvmH256::from_str(hash_str).unwrap();
        assert_eq!(hash_str, hash.to_string());
        // 0x prefix is accepted on input and stripped.
        let prefixed = EvmH256::from_str(&format!("0x{hash_str}")).unwrap();
        assert_eq!(hash, prefixed);
        assert!(!hash.is_zero());
        assert!(EvmH256::default().is_zero());
    }

    #[test]
    fn test_evm_h256_serde_borsh_roundtrip() {
        let h = EvmH256::from_bytes([7u8; 32]);
        let j = serde_json::to_string(&h).unwrap();
        assert_eq!(h, serde_json::from_str::<EvmH256>(&j).unwrap());
        let b = borsh::to_vec(&h).unwrap();
        assert_eq!(b.len(), 32);
        assert_eq!(h, borsh::from_slice::<EvmH256>(&b).unwrap());
    }
}
