//! kaspa-pq Selected-Parent EVM Lane (ADR-0020) — consensus type surface (v0.2/v0.3).
//!
//! This module carries the **types only** for the EVM execution lane: the
//! block-body [`EvmExecutionPayload`] (bounded system ops + EIP-2718 user txs),
//! the executor-output [`EvmExecutionHeader`] (whose keyed BLAKE2b-512 digest
//! becomes `Header::evm_commitment_root`), the UTXO↔EVM op types
//! ([`EvmSystemOp`]/[`DepositClaim`]/[`WithdrawOp`]/[`EvmDepositLockOutput`]),
//! and the small EVM-domain newtypes ([`EvmAddress`], [`EvmBloom`],
//! [`EvmU256`]). The executor itself (revm) lands in the `kaspa-evm` crate
//! behind the `evm` cargo feature; nothing here pulls revm or secp256k1.
//!
//! Design alignment (v0.2 §3.2): the L1 header carries a **single**
//! `evm_commitment_root`; the full execution metadata lives here in the block
//! body and is committed by that one keyed digest. The EVM parent of a DAG
//! block `B` is its GHOSTDAG `selected_parent(B)` (§2.1), so an EVM result is an
//! append-only function of the block alone and is never re-executed on a
//! virtual reorg (§2.3 / §10.1).

mod u256;
pub use u256::*;

use crate::tx::{ScriptPublicKey, TransactionOutpoint};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{EvmH256, Hash64, blake2b_512_keyed};
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
/// unit (wei, 18 decimals): `10^(18-8) = 10^10`. A deposit of `amount_sompi`
/// credits `amount_sompi * EVM_NATIVE_SCALE` wei; a withdrawal must be an exact
/// multiple of this scale, else the precompile reverts (design §7/§8/§9.1).
pub const EVM_NATIVE_SCALE: u64 = 10_000_000_000;

/// MISAKA EVM chain id (testnet target). Deliberately distinct from every
/// public Ethereum network so `eth_chainId` can never collide with mainnet
/// (1) or common testnets. `0x4D534B` spells "MSK". Frozen in ADR-0020; the
/// mainnet id will be a different value chosen at mainnet launch. EIP-155
/// replay protection is mandatory (design §4.4).
pub const EVM_CHAIN_ID: u64 = 0x4D_53_4B;

/// Reserved system-predeploy address for **WMISAKA** (the WETH9-equivalent
/// wrapped-native ERC-20 used by v2/v3 DEX pools, design §19.3). A normal EVM
/// contract (not a precompile) deployed into the activation state; carried here
/// so the executor and RPC agree on the canonical wrapped-native address.
pub const WMISAKA_ADDRESS: EvmAddress = EvmAddress::from_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF0, 0x01,
]);

/// Reserved precompile address for `MISAKA_WITHDRAW` (EVM → UTXO, design §8.1).
/// User-input failures here revert the tx (block stays valid, §8.2); only a
/// producer commitment/diff mismatch makes a block invalid.
pub const MISAKA_WITHDRAW_PRECOMPILE: EvmAddress = EvmAddress::from_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF0, 0x02,
]);

/// The EVM genesis state root — the `parent_state_root` of the first EVM block.
/// With no system predeploys this is the canonical empty Merkle-Patricia-Trie
/// root `keccak256(rlp(()))` (= `alloy_trie::EMPTY_ROOT_HASH`); the P2 executor
/// asserts an empty block reproduces it. When the WMISAKA predeploy lands
/// (design §19.3, P5+) this becomes the post-predeploy state root.
pub const EVM_GENESIS_STATE_ROOT: EvmH256 = EvmH256::from_bytes([
    0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e, 0x5b, 0x48, 0xe0, 0x1b,
    0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21,
]);

// --- Domain separators (design §3.3). Frozen once testnet activates. ---

/// Keyed-BLAKE2b-512 domain for the L1 `evm_commitment_root` over the full
/// [`EvmExecutionHeader`].
pub const MISAKA_EVM_COMMITMENT_CONTEXT: &[u8] = b"MISAKA_EVM_COMMITMENT_V2";
/// Keyed-BLAKE2b-256 domain for `EvmExecutionHeader::system_ops_root`.
pub const MISAKA_EVM_SYSTEM_OPS_CONTEXT: &[u8] = b"MISAKA_EVM_SYSTEM_OPS_V2";
/// Keyed-BLAKE2b-256 domain for `EvmExecutionHeader::withdrawals_root`.
pub const MISAKA_EVM_WITHDRAWAL_CONTEXT: &[u8] = b"MISAKA_EVM_WITHDRAWAL_V2";
/// Keyed-BLAKE2b-256 domain for `EvmExecutionHeader::deposit_claim_queue_root`.
pub const MISAKA_EVM_DEPOSIT_CLAIM_CONTEXT: &[u8] = b"MISAKA_EVM_DEPOSIT_CLAIM_V2";
/// Keyed-BLAKE2b-512 domain for withdrawal synthetic-outpoint txids (P4, §8.3).
/// MUST stay separate from the normal transaction-id domain so a synthetic
/// outpoint can never collide with a real txid.
pub const MISAKA_EVM_SYNTHETIC_OUTPOINT_CONTEXT: &[u8] = b"MISAKA_EVM_SYNTHETIC_OUTPOINT_V2";
/// Keyed-BLAKE2b-256 domain for the EVM `prevrandao` derivation (design §4.3).
pub const MISAKA_EVM_PREVRANDAO_CONTEXT: &[u8] = b"MISAKA_EVM_PREVRANDAO_V2";

// --- Bounded deposit-claim / system-gas limits (design §7.3 / §15.2). ---
// Enforced in P4 when DepositClaim validation lands; defined here so the
// limits are frozen with the rest of the spec.

/// Max `DepositClaim` system ops per EVM block.
pub const MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK: usize = 256;
/// Max total serialized `DepositClaim` bytes per EVM block.
pub const MAX_DEPOSIT_CLAIM_BYTES_PER_EVM_BLOCK: usize = 64 * 1024;
/// System gas charged to `gas_used` per applied deposit claim (design §7.4).
pub const SYSTEM_DEPOSIT_GAS_PER_CLAIM: u64 = 25_000;
/// Max total system gas (deposit claims + future system ops) per EVM block.
pub const MAX_SYSTEM_GAS_PER_EVM_BLOCK: u64 = 10_000_000;

// --- EVM block gas schedule + EIP-1559 base fee (design §5; P2 freeze). ---
// The design's per-second gas targeting (§5.2: `G_target_sec × τ_sc` from
// `W_eff/BPS`) is a documented pre-activation refinement. P2 freezes a fixed
// block gas limit so the gas schedule and base-fee update are deterministic and
// independently verifiable by every node (the values feed the committed
// `EvmExecutionHeader`). Refining before activation is not a hard fork (the lane
// is `u64::MAX`-inert until deploy); changing them after activation is.

/// EVM block gas limit (frozen). `gas_target = EVM_GAS_LIMIT / EVM_ELASTICITY_MULTIPLIER`.
pub const EVM_GAS_LIMIT: u64 = 30_000_000;
/// EIP-1559 elasticity multiplier: `gas_limit = EVM_ELASTICITY_MULTIPLIER × gas_target`.
pub const EVM_ELASTICITY_MULTIPLIER: u64 = 2;
/// EIP-1559 base-fee max change denominator (≤ `1/8` change per EVM block).
pub const EVM_BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;
/// Base fee (wei) of the first EVM block (1 gwei). Base fee is burned, never paid
/// to the block coinbase (design §9.2), and accumulates in `evm_burn_accumulator`.
pub const EVM_INITIAL_BASE_FEE: u64 = 1_000_000_000;

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

/// A 256-byte Ethereum logs bloom filter.
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize)]
pub struct EvmBloom([u8; EVM_BLOOM_SIZE]);

// `EvmBloom` is 256 bytes; the `serde_impl_*_fixed_bytes_ref!` macros bottom out
// on serde's fixed-array impls (which only cover N ≤ 32), so hand-roll serde: a
// hex string in human-readable encoders (JSON/RPC), raw bytes otherwise
// (bincode/compact). borsh is handled by the derive above (no array cap).
impl Serialize for EvmBloom {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_hex())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for EvmBloom {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = <String as serde::Deserialize>::deserialize(deserializer)?;
            EvmBloom::from_hex(&s).map_err(serde::de::Error::custom)
        } else {
            struct BloomVisitor;
            impl<'de> serde::de::Visitor<'de> for BloomVisitor {
                type Value = EvmBloom;
                fn expecting(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                    write!(f, "a {EVM_BLOOM_SIZE}-byte EVM logs bloom")
                }
                fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<EvmBloom, E> {
                    let arr: [u8; EVM_BLOOM_SIZE] = v.try_into().map_err(E::custom)?;
                    Ok(EvmBloom(arr))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<EvmBloom, A::Error> {
                    let mut arr = [0u8; EVM_BLOOM_SIZE];
                    for (i, slot) in arr.iter_mut().enumerate() {
                        *slot = seq.next_element()?.ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                    }
                    Ok(EvmBloom(arr))
                }
            }
            deserializer.deserialize_bytes(BloomVisitor)
        }
    }
}

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

impl From<[u8; EVM_BLOOM_SIZE]> for EvmBloom {
    fn from(value: [u8; EVM_BLOOM_SIZE]) -> Self {
        EvmBloom(value)
    }
}

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

impl AsRef<[u8; EVM_BLOOM_SIZE]> for EvmBloom {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; EVM_BLOOM_SIZE] {
        &self.0
    }
}

impl ToHex for EvmBloom {
    fn to_hex(&self) -> String {
        let mut hex = vec![0u8; EVM_BLOOM_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("twice the input size");
        // SAFETY: hex_encode only writes ASCII hex digits.
        unsafe { String::from_utf8_unchecked(hex) }
    }
}

impl FromHex for EvmBloom {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let hex_str = hex_str.strip_prefix("0x").or_else(|| hex_str.strip_prefix("0X")).unwrap_or(hex_str);
        let mut bytes = [0u8; EVM_BLOOM_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(EvmBloom(bytes))
    }
}

impl MemSizeEstimator for EvmBloom {}

// ---------------------------------------------------------------------------
// UTXO ↔ EVM op types (design §7 / §8). Types only this pass; the bounded
// validation + UTXO-diff materialization land in P4.
// ---------------------------------------------------------------------------

/// A bounded, producer-selected, consensus-validated EVM system op carried in
/// `EvmExecutionPayload::system_ops` (design §3.1 / §7.3). The op ordering is
/// the payload order, committed by `system_ops_root`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmSystemOp {
    /// Claim an `EVM_DEPOSIT_LOCK` UTXO output and credit the EVM account
    /// (design §7.3). The lock is consumed in the same block's UTXO diff (P4).
    DepositClaim(DepositClaim),
}

impl MemSizeEstimator for EvmSystemOp {}

/// Claims a previously-locked deposit (an unspent `EVM_DEPOSIT_LOCK` output in
/// the `selected_parent(B)` UTXO view) and credits the EVM account (design §7.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositClaim {
    /// The `EVM_DEPOSIT_LOCK` output being claimed.
    pub deposit_outpoint: TransactionOutpoint,
    /// EVM account credited `amount_sompi * EVM_NATIVE_SCALE` wei. MUST equal
    /// the lock output's recorded address.
    pub evm_address: EvmAddress,
    /// Sompi amount. MUST equal the lock output's value.
    pub amount_sompi: u64,
}

impl MemSizeEstimator for DepositClaim {}

/// A successful withdrawal emitted by the F002 precompile (design §8.1). The
/// executor materializes one synthetic UTXO output per `WithdrawOp` in the
/// block's UTXO diff (P4, §8.3). User-input failures do **not** emit a
/// `WithdrawOp` (they revert the tx, §8.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawOp {
    /// Index of the user EVM tx (within `transactions`) that emitted this op.
    pub evm_tx_index: u32,
    /// Index of this op within that tx (a tx may withdraw more than once).
    pub op_index: u32,
    /// EVM account debited.
    pub from: EvmAddress,
    /// Destination UTXO script (consensus script-rule validated; failure ⇒ revert).
    pub script_public_key: ScriptPublicKey,
    /// Sompi paid out (= `amount_wei / EVM_NATIVE_SCALE`, exact multiple required).
    pub amount_sompi: u64,
}

impl MemSizeEstimator for WithdrawOp {}

/// The `EVM_DEPOSIT_LOCK` UTXO output payload (design §7.2). Created by a
/// `DepositLockTx` on the UTXO layer; later claimed via [`DepositClaim`] or, if
/// `timeout_daa_score` elapses unclaimed, refunded to `refund_script_public_key`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmDepositLockOutput {
    pub value_sompi: u64,
    pub evm_address: EvmAddress,
    pub refund_script_public_key: ScriptPublicKey,
    pub timeout_daa_score: Option<u64>,
}

impl MemSizeEstimator for EvmDepositLockOutput {}

// ---------------------------------------------------------------------------
// EvmExecutionPayload — the block-body EVM unit (design §3.1).
// ---------------------------------------------------------------------------

/// The EVM execution payload carried in a block body, separate from the UTXO
/// `transactions` because UTXO txs are DAG-inclusive-accepted while EVM txs are
/// canonical only when their block enters the selected-parent chain (design
/// §2.2). Pre-activation blocks (header version &lt; `EVM_HEADER_VERSION`) MUST
/// carry the [`Default`] (empty) payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionPayload {
    /// Bounded, producer-selected system ops (deposit claims), applied before
    /// user txs and committed by `system_ops_root` in their payload order.
    pub system_ops: Vec<EvmSystemOp>,
    /// EIP-2718 typed-transaction bytes, in execution order.
    pub transactions: Vec<Vec<u8>>,
    /// Priority-fee recipient / EVM `block.coinbase` declared by the producer.
    pub fee_recipient: EvmAddress,
    /// Optional miner extra data (consensus-rule length-capped at activation).
    pub extra_data: Vec<u8>,
}

impl EvmExecutionPayload {
    /// An EVM-inert payload (`== Default`): no system ops, no txs, no extra
    /// data, zero fee recipient. Pre-activation blocks must satisfy this.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.system_ops.is_empty() && self.transactions.is_empty() && self.extra_data.is_empty() && self.fee_recipient.is_zero()
    }
}

impl MemSizeEstimator for EvmExecutionPayload {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.transactions.capacity() * size_of::<Vec<u8>>()
            + self.transactions.iter().map(|t| t.capacity()).sum::<usize>()
            + self.system_ops.capacity() * size_of::<EvmSystemOp>()
            + self.extra_data.capacity()
    }
}

// ---------------------------------------------------------------------------
// EvmExecutionHeader — executor output, committed via evm_commitment_root.
// ---------------------------------------------------------------------------

/// The consensus-committed EVM execution header (design §3.2). Its keyed
/// BLAKE2b-512 digest under [`MISAKA_EVM_COMMITMENT_CONTEXT`] is carried in
/// `Header::evm_commitment_root`; the verifier re-executes, rebuilds this
/// header, and checks the digest. The current L1 block hash and current EVM
/// block hash are intentionally **absent** (they would be a circular
/// dependency, design §4.2); only ancestor-derived values appear.
///
/// **FROZEN FIELD ORDER (hard fork to change once testnet activates):** the
/// commitment preimage is this struct's borsh encoding in declared order
/// ([`EvmExecutionHeader::commitment_preimage`]). All fields are fixed-width, so
/// borsh is a deterministic concatenation. Never reorder, remove, or change the
/// width of a field below after activation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionHeader {
    /// `EvmStateRoot(selected_parent(B))` — the parent EVM state this block executed against.
    pub parent_state_root: EvmH256,
    /// keccak256 MPT state root after applying system ops + user txs.
    pub state_root: EvmH256,
    /// keccak256 MPT root over the executed `transactions`.
    pub transactions_root: EvmH256,
    /// keccak256 MPT root over the per-tx receipts.
    pub receipts_root: EvmH256,
    /// MISAKA keyed root over the ordered `system_ops` (`MISAKA_EVM_SYSTEM_OPS_V2`).
    pub system_ops_root: EvmH256,
    /// MISAKA keyed root over the ordered `WithdrawOp`s (`MISAKA_EVM_WITHDRAWAL_V2`).
    pub withdrawals_root: EvmH256,
    /// MISAKA keyed root over the applied deposit-claim queue (`MISAKA_EVM_DEPOSIT_CLAIM_V2`).
    pub deposit_claim_queue_root: EvmH256,
    /// Ethereum logs bloom over all receipts' logs.
    pub logs_bloom: EvmBloom,
    pub gas_used: u64,
    pub gas_limit: u64,
    /// EIP-1559 base fee (wei). 32-byte to match Ethereum `uint256`; the
    /// executor converts to/from `alloy_primitives::U256`.
    pub base_fee_per_gas: EvmU256,
    /// Selected-parent-tree height: `evm_number(selected_parent) + 1` (§4.1).
    pub evm_number: u64,
    /// Strictly-monotonic EVM logical time `max(header_ts_sec, parent_ts+1)` (§4.1).
    pub evm_timestamp_sec: u64,
    pub evm_chain_id: u64,
    /// Cumulative EVM basefee burn up to and including this block (design §9.2).
    pub evm_burn_accumulator: EvmU256,
}

impl EvmExecutionHeader {
    /// The canonical commitment preimage = the borsh encoding of this header in
    /// declared field order. All fields are fixed-width, so borsh is a stable,
    /// deterministic concatenation (design §3.2 `SCALE(EvmExecutionHeader)`).
    #[inline]
    pub fn commitment_preimage(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("EvmExecutionHeader borsh serialization is infallible")
    }

    /// `evm_commitment_root(B)` (design §3.2) — keyed BLAKE2b-512 over the
    /// canonical preimage under [`MISAKA_EVM_COMMITMENT_CONTEXT`], producing the
    /// 64-byte digest carried in `Header::evm_commitment_root`. Pure (no revm),
    /// so a non-`evm` build can still recompute/verify the L1 field.
    #[inline]
    pub fn commitment_root(&self) -> Hash64 {
        blake2b_512_keyed(MISAKA_EVM_COMMITMENT_CONTEXT, &self.commitment_preimage())
    }
}

impl MemSizeEstimator for EvmExecutionHeader {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

// ---------------------------------------------------------------------------
// Executor output (design §6 / §11.1). Returned by the `kaspa-evm` executor
// (P2) and persisted across the EVM stores (P3). Not consensus-committed
// directly — the committed digest is `header.commitment_root()`.
// ---------------------------------------------------------------------------

/// A single EVM log entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmLog {
    pub address: EvmAddress,
    pub topics: Vec<EvmH256>,
    pub data: Vec<u8>,
}

impl MemSizeEstimator for EvmLog {}

/// A per-transaction EVM receipt. `succeeded == false` for a user-caused
/// failure (revert / out-of-gas / bad nonce) — which is NOT block-invalid
/// (design §6.3 / §8.2); gas is still consumed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmReceipt {
    pub succeeded: bool,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub logs: Vec<EvmLog>,
}

impl MemSizeEstimator for EvmReceipt {}

/// The full output of executing one block's EVM lane. The committed
/// `header.commitment_root()` equals the L1 `Header::evm_commitment_root`; the
/// rest is store/RPC data and the UTXO-diff source for P4.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionResult {
    pub header: EvmExecutionHeader,
    pub receipts: Vec<EvmReceipt>,
    /// Withdrawal ops in receipt/log order → synthetic UTXO outputs (P4).
    pub withdrawals: Vec<WithdrawOp>,
    /// Deposit claims applied this block → consumed lock outputs (P4).
    pub applied_deposit_claims: Vec<DepositClaim>,
}

impl MemSizeEstimator for EvmExecutionResult {}

// ---------------------------------------------------------------------------
// EvmStateSnapshot — persisted full EVM account state (design §11, P3).
// ---------------------------------------------------------------------------

/// One account in a persisted EVM state snapshot. Secp-free + borsh, so the
/// consensus stores (P3, prefix 206) can persist EVM state without pulling revm:
/// the `evm`-feature executor converts `EvmAccountSnapshot <-> revm AccountInfo`
/// at its boundary. `storage` is sorted by slot (deterministic encoding).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmAccountSnapshot {
    pub address: EvmAddress,
    pub nonce: u64,
    pub balance: EvmU256,
    pub code_hash: EvmH256,
    /// Account bytecode (empty for an EOA).
    pub code: Vec<u8>,
    /// Non-zero storage slots, sorted by slot key.
    pub storage: Vec<(EvmU256, EvmU256)>,
}

impl MemSizeEstimator for EvmAccountSnapshot {}

/// A full EVM account-state snapshot after a block (design §11.1). P3 stores one
/// per block hash to seed the executor for the block's selected children; a later
/// phase replaces this O(state) form with an incremental persistent trie.
/// Accounts are sorted by address (deterministic encoding). The empty snapshot is
/// the EVM genesis state (root = `EVM_GENESIS_STATE_ROOT`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmStateSnapshot {
    pub accounts: Vec<EvmAccountSnapshot>,
}

impl EvmStateSnapshot {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

impl MemSizeEstimator for EvmStateSnapshot {
    // Implemented (not the panicking default) so the P3 store is safe under any
    // cache policy — the documented validator-attestation crash was a Vec-valued
    // store left on the default `unimplemented!()` estimator.
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.accounts.capacity() * size_of::<EvmAccountSnapshot>()
            + self.accounts.iter().map(|a| a.code.capacity() + a.storage.capacity() * size_of::<(EvmU256, EvmU256)>()).sum::<usize>()
    }
}

/// The three canonical EVM head pointers (design §10.3 / §11.1). A virtual reorg
/// only updates these — it never re-executes (design §2.3 / §10.1). `latest` =
/// virtual selected-chain head; `safe` = a blue_work-threshold ancestor;
/// `finalized` = the finality / pruning / DNS anchor. (P3 persists them; the
/// blue_work / finality selection lands with the hot-path hook.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalEvmHeads {
    pub latest: Hash64,
    pub safe: Hash64,
    pub finalized: Hash64,
}

impl MemSizeEstimator for CanonicalEvmHeads {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_is_empty_and_default() {
        let p = EvmExecutionPayload::default();
        assert!(p.is_empty());
        assert_eq!(p, EvmExecutionPayload::default());
        // A non-empty payload (any of the four fields) is detected.
        let p2 = EvmExecutionPayload { transactions: vec![vec![1, 2, 3]], ..Default::default() };
        assert!(!p2.is_empty());
        let p3 = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: TransactionOutpoint::default(),
                evm_address: EvmAddress::default(),
                amount_sompi: 1,
            })],
            ..Default::default()
        };
        assert!(!p3.is_empty());
    }

    #[test]
    fn execution_header_defaults_and_genesis_state_root() {
        let h = EvmExecutionHeader::default();
        assert_eq!(h.evm_number, 0);
        assert!(h.state_root.is_zero());
        assert_eq!(h.logs_bloom, EvmBloom::default());
        assert_eq!(h.base_fee_per_gas, EvmU256::ZERO);
        // The pinned genesis state root is the canonical empty-trie root.
        assert_eq!(EVM_GENESIS_STATE_ROOT.to_string(), "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421");
    }

    #[test]
    fn commitment_root_is_deterministic_and_field_sensitive() {
        let mut h = EvmExecutionHeader { evm_chain_id: EVM_CHAIN_ID, gas_used: 21_000, ..Default::default() };
        let c1 = h.commitment_root();
        // Same inputs ⇒ identical commitment.
        assert_eq!(c1, h.clone().commitment_root());
        // Domain-separated 64-byte digest, not the all-zero default.
        assert_ne!(c1, Hash64::default());
        // Any field change ⇒ different commitment.
        h.gas_used = 21_001;
        assert_ne!(c1, h.commitment_root());
    }

    #[test]
    fn bloom_serde_roundtrip() {
        let mut bytes = [0u8; EVM_BLOOM_SIZE];
        bytes[0] = 0xAB;
        bytes[255] = 0xCD;
        let bloom = EvmBloom::from_bytes(bytes);
        let j = serde_json::to_string(&bloom).unwrap();
        assert_eq!(bloom, serde_json::from_str::<EvmBloom>(&j).unwrap());
        let b = borsh::to_vec(&bloom).unwrap();
        assert_eq!(b.len(), EVM_BLOOM_SIZE);
        assert_eq!(bloom, borsh::from_slice::<EvmBloom>(&b).unwrap());
    }
}
