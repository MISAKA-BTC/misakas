//! kaspa-pq Selected-Parent EVM Lane (ADR-0020): a 256-bit unsigned integer
//! stored as 32 **big-endian** bytes (Ethereum `uint256` convention) for the
//! EVM-domain quantity fields carried in the consensus-committed
//! [`super::EvmExecutionHeader`] — `base_fee_per_gas` and `evm_burn_accumulator`.
//!
//! `EvmU256` is a **feature-free** stand-in for the executor's
//! `alloy_primitives::U256`, so the always-compiled consensus types (and the
//! `evm_commitment_root` borsh preimage) carry the Ethereum-faithful 32-byte
//! width without pulling revm / alloy into the default secp-free build. The
//! `evm`-feature executor converts `EvmU256 <-> U256` at its boundary via
//! [`EvmU256::to_be_bytes`] / [`EvmU256::from_be_bytes`]. Mirrors the
//! [`kaspa_hashes::EvmH256`] surface (minus the WASM glue) so it is a drop-in
//! 32-byte value across serde and borsh.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    mem_size::MemSizeEstimator,
    serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref,
};
use std::{
    fmt::{Debug, Display, Formatter},
    str::{self, FromStr},
};

/// Width of an [`EvmU256`] in bytes.
pub const EVM_U256_SIZE: usize = 32;

/// A 256-bit unsigned integer as 32 big-endian bytes (Ethereum `uint256`).
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Default, BorshSerialize, BorshDeserialize)]
pub struct EvmU256([u8; EVM_U256_SIZE]);

serde_impl_ser_fixed_bytes_ref!(EvmU256, EVM_U256_SIZE);
serde_impl_deser_fixed_bytes_ref!(EvmU256, EVM_U256_SIZE);

impl EvmU256 {
    /// The additive identity.
    pub const ZERO: EvmU256 = EvmU256([0u8; EVM_U256_SIZE]);

    #[inline(always)]
    pub const fn from_be_bytes(bytes: [u8; EVM_U256_SIZE]) -> Self {
        EvmU256(bytes)
    }

    #[inline(always)]
    pub const fn to_be_bytes(self) -> [u8; EVM_U256_SIZE] {
        self.0
    }

    /// Construct from a `u128` (placed big-endian into the low 16 bytes).
    #[inline]
    pub fn from_u128(value: u128) -> Self {
        let mut bytes = [0u8; EVM_U256_SIZE];
        bytes[EVM_U256_SIZE - 16..].copy_from_slice(&value.to_be_bytes());
        EvmU256(bytes)
    }

    /// Returns the value as a `u128` iff the high 16 bytes are all zero.
    #[inline]
    pub fn try_to_u128(self) -> Option<u128> {
        if self.0[..EVM_U256_SIZE - 16].iter().all(|&b| b == 0) {
            Some(u128::from_be_bytes(self.0[EVM_U256_SIZE - 16..].try_into().unwrap()))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; EVM_U256_SIZE]
    }
}

impl From<u128> for EvmU256 {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

impl From<u64> for EvmU256 {
    fn from(value: u64) -> Self {
        Self::from_u128(value as u128)
    }
}

impl From<[u8; EVM_U256_SIZE]> for EvmU256 {
    fn from(value: [u8; EVM_U256_SIZE]) -> Self {
        EvmU256(value)
    }
}

impl AsRef<[u8; EVM_U256_SIZE]> for EvmU256 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; EVM_U256_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for EvmU256 {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for EvmU256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; EVM_U256_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("the output is exactly twice the input size");
        f.write_str(unsafe { str::from_utf8_unchecked(&hex) })
    }
}

impl Debug for EvmU256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "EvmU256(0x{self})")
    }
}

impl FromStr for EvmU256 {
    type Err = faster_hex::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Accept an optional `0x` prefix; expects exactly 64 hex chars (32 bytes).
        let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        let mut bytes = [0u8; EVM_U256_SIZE];
        faster_hex::hex_decode(s.as_bytes(), &mut bytes)?;
        Ok(EvmU256(bytes))
    }
}

impl ToHex for EvmU256 {
    fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl FromHex for EvmU256 {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl MemSizeEstimator for EvmU256 {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u128_roundtrip_and_be_layout() {
        let v = EvmU256::from_u128(1_000_000_000_000u128);
        assert_eq!(v.try_to_u128(), Some(1_000_000_000_000u128));
        // big-endian: low 16 bytes carry the u128, high 16 are zero.
        let be = v.to_be_bytes();
        assert!(be[..16].iter().all(|&b| b == 0));
        assert_eq!(EvmU256::from_be_bytes(be), v);
        assert!(EvmU256::ZERO.is_zero());
        assert_eq!(EvmU256::default(), EvmU256::ZERO);
    }

    #[test]
    fn try_to_u128_overflow_is_none() {
        let mut be = [0u8; EVM_U256_SIZE];
        be[0] = 1; // a bit set above the low 128 bits
        assert_eq!(EvmU256::from_be_bytes(be).try_to_u128(), None);
    }

    #[test]
    fn serde_borsh_roundtrip() {
        let v = EvmU256::from_u128(0xDEAD_BEEFu128);
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(v, serde_json::from_str::<EvmU256>(&j).unwrap());
        let b = borsh::to_vec(&v).unwrap();
        assert_eq!(b.len(), EVM_U256_SIZE);
        assert_eq!(v, borsh::from_slice::<EvmU256>(&b).unwrap());
    }
}
