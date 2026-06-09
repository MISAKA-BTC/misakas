//! kaspa-pq Selected-Parent EVM Lane (ADR-0020) — consensus type surface.
//!
//! This module carries the **types only** for the EVM execution lane: the
//! block-body [`EvmExecutionPayload`], the executor-output
//! [`EvmExecutionHeader`] (whose keyed BLAKE2b-512 digest becomes
//! `Header::evm_commitment_root`), and the small EVM-domain newtypes
//! ([`EvmAddress`], [`EvmBloom`]). The executor itself (revm) lands in a
//! later phase behind the `evm` cargo feature; nothing here pulls revm or
//! secp256k1.
//!
//! Design invariant (ADR-0020 §3): the EVM parent of a DAG block `B` is its
//! GHOSTDAG `selected_parent(B)`, so an EVM result is an append-only function
//! of the block alone and never needs re-execution on a virtual reorg.

use crate::BlockHash;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{EvmH256, Hash64};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    mem_size::MemSizeEstimator,
    serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Debug, Display, Formatter},
    mem::size_of,
    str::{self, FromStr},
};

// ---------------------------------------------------------------------------
// Frozen EVM-lane constants (ADR-0020 §"Spec freeze"). Network-tunable values
// (activation height) live on `Params`; these are protocol-wide constants.
// ---------------------------------------------------------------------------

/// Ratio between one UTXO atomic unit (sompi, 8 decimals) and the EVM native
/// unit (wei, 18 decimals): `10^(18-8) = 10^10`. A deposit of `amount_atomic`
/// sompi credits `amount_atomic * EVM_NATIVE_SCALE` wei; a withdrawal must be
/// an exact multiple of this scale (ADR-0020 §6/§7).
pub const EVM_NATIVE_SCALE: u64 = 10_000_000_000;

/// MISAKA EVM chain id (testnet target). Deliberately distinct from every
/// public Ethereum network so `eth_chainId` can never collide with mainnet
/// (1) or common testnets. `0x4D534B` spells "MSK". Frozen in ADR-0020; the
/// mainnet id will be a different value chosen at mainnet launch.
pub const EVM_CHAIN_ID: u64 = 0x4D_53_4B;

/// Reserved precompile address for `MISAKA_WITHDRAW` (EVM → UTXO), ADR-0020 §7.
/// Carried as a constant here so the (later) precompile and the RPC layer agree.
pub const MISAKA_WITHDRAW_PRECOMPILE: EvmAddress = EvmAddress::from_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF0, 0x02,
]);

/// The EVM genesis state root. **P1 placeholder** = all-zero; the EVM executor
/// phase (P2) pins this to the canonical empty Merkle-Patricia-Trie root
/// `keccak256(rlp(()))` once the trie backend is wired.
pub const EVM_GENESIS_STATE_ROOT: EvmH256 = kaspa_hashes::ZERO_EVM_H256;

// ---------------------------------------------------------------------------
// EvmAddress — 20-byte Ethereum account address.
// ---------------------------------------------------------------------------

/// Width of an [`EvmAddress`] in bytes.
pub const EVM_ADDRESS_SIZE: usize = 20;

/// A 20-byte Ethereum account address (the `fee_recipient` / `to` surface).
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Default, BorshSerialize, BorshDeserialize)]
pub struct EvmAddress([u8; EVM_ADDRESS_SIZE]);

serde_impl_ser_fixed_bytes_ref!(EvmAddress, EVM_ADDRESS_SIZE);
serde_impl_deser_fixed_bytes_ref!(EvmAddress, EVM_ADDRESS_SIZE);

impl EvmAddress {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; EVM_ADDRESS_SIZE]) -> Self {
        EvmAddress(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(self) -> [u8; EVM_ADDRESS_SIZE] {
        self.0
    }

    #[inline(always)]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; EVM_ADDRESS_SIZE]
    }
}

impl From<[u8; EVM_ADDRESS_SIZE]> for EvmAddress {
    fn from(value: [u8; EVM_ADDRESS_SIZE]) -> Self {
        EvmAddress(value)
    }
}

impl AsRef<[u8; EVM_ADDRESS_SIZE]> for EvmAddress {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; EVM_ADDRESS_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for EvmAddress {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for EvmAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; EVM_ADDRESS_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("twice the input size");
        f.write_str(unsafe { str::from_utf8_unchecked(&hex) })
    }
}

impl Debug for EvmAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "EvmAddress({self})")
    }
}

impl FromStr for EvmAddress {
    type Err = faster_hex::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        let mut bytes = [0u8; EVM_ADDRESS_SIZE];
        faster_hex::hex_decode(s.as_bytes(), &mut bytes)?;
        Ok(EvmAddress(bytes))
    }
}

// Required by the `serde_impl_*_fixed_bytes_ref!` macros (hex string in
// human-readable encoders, raw bytes in compact ones).
impl ToHex for EvmAddress {
    fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl FromHex for EvmAddress {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl MemSizeEstimator for EvmAddress {}

// ---------------------------------------------------------------------------
// EvmBloom — 256-byte logs bloom filter.
// ---------------------------------------------------------------------------

/// Width of an [`EvmBloom`] in bytes (Ethereum logs bloom).
pub const EVM_BLOOM_SIZE: usize = 256;

/// A 256-byte Ethereum logs bloom filter. `serde` is intentionally omitted in
/// P1 (nothing serializes an `EvmExecutionHeader` over the wire yet); borsh is
/// derived for the future EVM header store.
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub struct EvmBloom([u8; EVM_BLOOM_SIZE]);

impl EvmBloom {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; EVM_BLOOM_SIZE]) -> Self {
        EvmBloom(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(&self) -> &[u8; EVM_BLOOM_SIZE] {
        &self.0
    }
}

// `[u8; 256]` does not implement `Default` (std only covers N <= 32), so hand-roll.
impl Default for EvmBloom {
    #[inline]
    fn default() -> Self {
        EvmBloom([0u8; EVM_BLOOM_SIZE])
    }
}

impl PartialEq for EvmBloom {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for EvmBloom {}

impl Debug for EvmBloom {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "EvmBloom([0x{:02x}{:02x}{:02x}{:02x}…; {} bytes])", self.0[0], self.0[1], self.0[2], self.0[3], EVM_BLOOM_SIZE)
    }
}

impl AsRef<[u8]> for EvmBloom {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl MemSizeEstimator for EvmBloom {}

// ---------------------------------------------------------------------------
// EvmExecutionPayload — the block-body EVM unit (ADR-0020 §4.3).
// ---------------------------------------------------------------------------

/// The EVM execution payload carried in a block body, separate from the UTXO
/// `transactions` because UTXO txs are DAG-inclusive-accepted while EVM txs are
/// canonical only when their block enters the selected-parent chain (ADR-0020
/// §3.3). Pre-activation blocks (header version &lt; `EVM_HEADER_VERSION`) MUST
/// carry the [`Default`] (empty) payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionPayload {
    /// EIP-2718 typed-transaction bytes, in execution order.
    pub txs: Vec<Vec<u8>>,
    /// Block gas limit declared by the miner for this EVM block.
    pub declared_gas_limit: u64,
    /// Priority-fee recipient (also committed in the EVM env `block.coinbase`).
    pub fee_recipient: EvmAddress,
    /// Optional miner extra data (consensus-rule length-capped at activation).
    pub extra_data: Vec<u8>,
}

impl EvmExecutionPayload {
    /// An EVM-inert payload (`== Default`): no txs, no extra data, zero gas
    /// limit, zero fee recipient. Pre-activation blocks must satisfy this.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty() && self.extra_data.is_empty() && self.declared_gas_limit == 0 && self.fee_recipient.is_zero()
    }
}

impl MemSizeEstimator for EvmExecutionPayload {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.txs.capacity() * size_of::<Vec<u8>>()
            + self.txs.iter().map(|t| t.capacity()).sum::<usize>()
            + self.extra_data.capacity()
    }
}

// ---------------------------------------------------------------------------
// EvmExecutionHeader — executor output, committed via evm_commitment_root.
// ---------------------------------------------------------------------------

/// The full EVM execution header for a block (ADR-0020 §4.2). Its keyed
/// BLAKE2b-512 digest under the `MISAKA_EVM_HEADER` domain is committed in
/// `Header::evm_commitment_root`. The current L1 block hash and current EVM
/// block hash are intentionally **not** inputs to the EVM execution
/// environment (would be a circular dependency); only ancestor hashes are.
///
/// `serde` is omitted in P1 — this type is produced by the executor (P2) and
/// persisted in the EVM header store (P3); neither path exists yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmExecutionHeader {
    pub version: u16,
    pub chain_id: u64,

    /// The L1 (kaspa) block this EVM header belongs to. For store association
    /// only — NOT fed into the EVM commitment (avoids a hash cycle).
    pub l1_block_hash: BlockHash,
    /// `selected_parent(B)` — the EVM parent block on the L1 side.
    pub parent_l1_block_hash: BlockHash,
    pub parent_evm_block_hash: EvmH256,
    pub parent_state_root: EvmH256,

    /// keccak256 over the canonical EVM header view.
    pub evm_block_hash: EvmH256,
    /// Selected-parent-tree height: `evm_number(selected_parent) + 1`.
    pub evm_number: u64,
    pub state_root: EvmH256,
    pub transactions_root: EvmH256,
    pub receipts_root: EvmH256,
    pub logs_bloom: EvmBloom,

    /// Commitment over the ordered system deposits + withdrawals.
    pub system_ops_root: EvmH256,
    pub withdrawals_root: EvmH256,

    pub gas_limit: u64,
    pub gas_used: u64,
    /// EIP-1559 base fee. Ethereum types it as `U256`; wei base fees fit in
    /// `u128` with vast headroom, so P1 stores it as `u128` (the revm executor
    /// in P2 converts to/from `U256`).
    pub base_fee_per_gas: u128,
    pub timestamp_ms: u64,
    pub fee_recipient: EvmAddress,
    pub prev_randao: EvmH256,
}

impl MemSizeEstimator for EvmExecutionHeader {}

/// Domain separator for the keyed BLAKE2b-512 EVM header commitment
/// (`Header::evm_commitment_root`). The actual digest helper lands with the
/// executor (P2); the domain is frozen here so signer/verifier/RPC agree.
pub const MISAKA_EVM_HEADER_CONTEXT: &[u8] = b"MISAKA_EVM_HEADER";

/// Placeholder for the (P2) `evm_commitment_root(&EvmExecutionHeader) -> Hash64`
/// helper. In P1 a pre-activation / empty EVM header commits to the zero
/// `Hash64` (carried by v0/v1 headers, which never hash the EVM fields anyway).
#[inline]
pub fn empty_evm_commitment_root() -> Hash64 {
    Hash64::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_is_empty_and_default() {
        let p = EvmExecutionPayload::default();
        assert!(p.is_empty());
        assert_eq!(p, EvmExecutionPayload::default());
        // A non-empty payload is detected.
        let p2 = EvmExecutionPayload { txs: vec![vec![1, 2, 3]], ..Default::default() };
        assert!(!p2.is_empty());
    }

    #[test]
    fn execution_header_defaults() {
        let h = EvmExecutionHeader::default();
        assert_eq!(h.version, 0);
        assert!(h.state_root.is_zero());
        assert_eq!(h.logs_bloom, EvmBloom::default());
    }
}
