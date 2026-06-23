//! kaspa-pq Selected-Parent EVM Lane (ADR-0020) — consensus type surface
//! (design v0.4, `docs/misaka-evm-design-v0.4.md`).
//!
//! This module carries the **types only** for the EVM execution lane: the
//! block-body [`EvmExecutionPayload`] (bounded system ops + EIP-2718 user txs),
//! the executor-output [`EvmExecutionHeader`] (whose keyed BLAKE2b-512 digest
//! becomes `Header::evm_commitment_root`), the UTXO↔EVM op types
//! ([`EvmSystemOp`]/[`DepositClaim`]/[`WithdrawOp`]),
//! and the small EVM-domain newtypes ([`EvmAddress`], [`EvmBloom`],
//! [`EvmU256`]). The executor itself (revm) lands in the `kaspa-evm` crate
//! behind the `evm` cargo feature; nothing here pulls revm or secp256k1.
//!
//! Design alignment (v0.4 §3/§4): execution is **mergeset delayed acceptance**
//! — `EvmResult(B) = exec(state(selected_parent(B)), B.system_ops,
//! AcceptedEvmTxs(B))` where `AcceptedEvmTxs(B)` is the mergeset's payload txs
//! in canonical order; a block's OWN user payload is data committed by
//! `Header::evm_payload_hash` and is executed by its selected child. The L1
//! header carries exactly two EVM commitments (`evm_payload_hash` +
//! `evm_commitment_root`); the full execution metadata lives here in the block
//! body. An EVM result is a pure function of the block's parents + its own
//! system ops, so it is computed once and never re-executed on a virtual reorg
//! (§2.2 / §10).

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

/// Reserved precompile address for `MLDSA87_VERIFY` (PREA design v1.1 §9 / FSL
/// §4.3). A pure post-quantum signature-verify precompile (ML-DSA-87, FIPS 204):
/// it changes no state and moves no value, so it is reachable from any frame
/// (incl. `STATICCALL`). The call is **version-discriminated** (`input[0]`):
/// `0x01` = FSL generic Hash64 verify; `0x02` = PREA key-bound root authorization
/// (additionally binds the pubkey to its UTXO address payload). Any malformed
/// input, wrong length, unknown version, key-payload mismatch, or invalid
/// signature returns the 32-byte ABI `false` (never panics, never reverts).
/// ACTIVATION-FENCED + INERT until `evm_f003_mldsa_verify_activation_daa_score`
/// (u64::MAX on every network); below the fence the handler is not registered, so
/// a call to this address behaves exactly as a call to an empty account today
/// (byte-identical execution, genesis/state-root unchanged).
pub const MISAKA_MLDSA_VERIFY_PRECOMPILE: EvmAddress = EvmAddress::from_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF0, 0x03,
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

// --- Domain separators (design v0.4 §4.1). Frozen once testnet activates. ---

/// Keyed-BLAKE2b-512 domain for the L1 `evm_commitment_root` over the full
/// [`EvmExecutionHeader`] (design v0.4 §4.1 normative key).
pub const MISAKA_EVM_COMMITMENT_CONTEXT: &[u8] = b"EvmCommitment64";
/// Keyed-BLAKE2b-512 domain for the L1 `evm_payload_hash` over the borsh
/// encoding of the block's own [`EvmExecutionPayload`] (design v0.4 §4.1).
pub const MISAKA_EVM_PAYLOAD_HASH_CONTEXT: &[u8] = b"EvmPayload64";
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

// --- v0.4 two-stage caps (design §7, D4). ---
// Inclusion-side: a DAG block's own payload size, checked at body validation
// (class-1 admission). Execution-side: the accepted-gas budget of one chain
// block's mergeset acceptance, applied as a deterministic prefix-take over
// `AcceptedEvmTxs(B)` by declared tx gas_limit (over-cap txs are class-5
// skipped; nonce unchanged, re-acceptable later). Numeric freeze before
// activation = open decision O13 (per-second G_limit derivation + measured
// propagation, design §14.3); changing them pre-activation is not a hard fork.

/// Max serialized `EvmExecutionPayload` bytes per DAG block (inclusion cap).
pub const MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK: usize = 128 * 1024;
/// Max accepted user-tx gas per chain block (execution cap) = `G_limit_block`
/// (design §5.1: kept equal to the EVM block gas limit, audit AH-2).
pub const MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK: u64 = EVM_GAS_LIMIT;

// --- F002 withdraw precompile (design §9.3). ---

/// Fixed gas charged by a successful (or user-fault-reverted) `F002` withdraw
/// call, on top of the carrying tx's intrinsic gas. Frozen at activation.
pub const F002_WITHDRAW_GAS: u64 = 9_000;
/// Max `WithdrawOp`s an accepting chain block may MATERIALIZE (audit M-03): each
/// withdraw adds a synthetic UTXO output + MuHash/index/script work that the flat
/// F002 EVM gas under-prices, so the count of L1-materialized withdrawals must be
/// independently bounded (mirrors the deposit-claim per-block cap). ENFORCEMENT IS
/// FENCED + NOT YET WIRED — see `evm_f002_withdraw_cap_activation_daa_score` and
/// `docs/adr/0020-…` / the executor note: the safe enforcement is a per-tx class-2
/// skip once the block's withdraw count would exceed this cap (transact-without-
/// commit so the skipped tx's state never commits), implemented + supply-invariant-
/// tested as a dedicated activation, NOT a rushed change. Inert until activated.
pub const MAX_WITHDRAWALS_PER_EVM_BLOCK: usize = 256;
/// Max byte length of the destination `ScriptPublicKey` SCRIPT a withdraw may
/// name (the standard ML-DSA P2PKH script is 69 bytes; this is a sanity bound,
/// the class check is the real gate).
pub const MAX_WITHDRAW_SCRIPT_BYTES: usize = 128;

// --- F003 MLDSA87_VERIFY precompile (PREA design v1.1 §9 / FSL §4.3). ---
//
// P0-0 FROZEN VALUES (proposed; benchmark-confirm on a low-end no-SIMD reference
// image before activation — these are inert under u64::MAX and so freely tunable
// until the activation DAA is set; changing them AFTER activation is a hard fork).
// The gas charge is the PRIMARY deterministic bound: one ML-DSA-87 portable
// verify is ~tens of µs, so a flat per-call gas calibrated so that
// `block_gas / F003_VERIFY_GAS ≈ MAX_MLDSA_VERIFY_PER_EVM_BLOCK` bounds the
// per-block verify CPU even though the precompile is reachable from inner frames
// (the named count caps below DOCUMENT that gas-implied ceiling).

/// F003 input version tags (`input[0]`). 0x01 = FSL generic Hash64 verify;
/// 0x02 = PREA key-bound root authorization.
pub const F003_VERSION_FSL_GENERIC: u8 = 0x01;
pub const F003_VERSION_PREA_ROOT: u8 = 0x02;

/// F003 version-0x01 (FSL generic) fixed input length (exact-match; any other ⇒
/// ABI `false`): `version(1) ‖ pubkey(2592) ‖ message_hash64(64) ‖ signature(4627)`.
/// The caller supplies the 64-byte digest (FSL computes its own fact digest).
pub const F003_INPUT_LEN_FSL: usize = 1 + 2592 + 64 + 4627; // 7284

/// F003 version-0x02 (PREA root) FIXED-PREFIX length (design v1.1 §9.3 option B):
/// `version(1) ‖ expected_key_payload64(64) ‖ pubkey(2592) ‖ signature(4627)`,
/// FOLLOWED by a variable `op_preimage` (1..=`F003_MAX_PREA_PREIMAGE_BYTES`). F003
/// itself computes `message_hash64 = keyed_blake2b_512(F003_PREA_OP_MLDSA87_CONTEXT,
/// op_preimage)` so an on-chain caller (the PQ smart account `executeRoot`) does NOT
/// need keyed-BLAKE2b-512 in the EVM: it just passes the canonical operation bytes
/// it is about to execute, and the ML-DSA signature is verified over the full-PQ
/// Hash64 digest of exactly those bytes — binding the signature to the operation.
pub const F003_PREA_PREFIX_LEN: usize = 1 + 64 + 2592 + 4627; // 7284
/// Max `op_preimage` bytes for an F003 version-0x02 call. Bounds the keyed-BLAKE2b
/// work + the input size (with the prefix, ≤ ~23.6 KiB/call); generous for an op's
/// target/value/calldata. Frozen at activation.
pub const F003_MAX_PREA_PREIMAGE_BYTES: usize = 16 * 1024;

/// Fixed gas charged by an F003 call (any version, success OR fail-closed-false),
/// on top of the carrying tx's calldata + intrinsic gas. Set so the gas-implied
/// per-block verify count (`EVM_GAS_LIMIT / F003_VERIFY_GAS` = 60) stays at or
/// under `MAX_MLDSA_VERIFY_PER_EVM_BLOCK`; ~conservative vs the ~tens-of-µs
/// portable verify (deliberately over-priced — root operations are infrequent and
/// under-pricing is the real risk). Charged BEFORE dispatch so a malformed flood
/// pays the same. Frozen at activation.
pub const F003_VERIFY_GAS: u64 = 500_000;

/// Documented gas-implied per-block ceiling on F003 verifies (the gas charge is
/// the enforcing mechanism: `EVM_GAS_LIMIT / F003_VERIFY_GAS = 60 ≤ 64`, and the
/// ~7.3 KB calldata cost lowers it further). Named so monitoring + a future
/// explicit counter (if benchmarks demand a sub-gas bound) have a single source.
pub const MAX_MLDSA_VERIFY_PER_EVM_BLOCK: usize = 64;
/// Documented gas-implied per-tx ceiling on F003 verifies (a tx doing this many
/// verifies costs ~`8 × F003_VERIFY_GAS` = 4M gas; tx/block gas bounds it).
pub const MAX_MLDSA_VERIFY_PER_TX: usize = 8;
/// Documented per-block ceiling on total F003 auth input bytes (a v0x02 call is
/// `F003_PREA_PREFIX_LEN + op_preimage`; this bounds the aggregate across a block
/// independently of the per-verify count cap, and is also bounded by calldata gas).
pub const MAX_MLDSA_AUTH_BYTES_PER_EVM_BLOCK: usize = 512 * 1024;

/// F003 version-0x02 (PREA root) ML-DSA-87 signing context — the `ctx` domain
/// separator. Distinct from every other ML-DSA-87 context (att/unbond/takeover/
/// audit-ckpt/tx/address) so a UTXO/attestation/tx signature can never be
/// cross-protocol-replayed as an EVM root authorization, and vice versa.
pub const F003_PREA_ROOT_MLDSA87_CONTEXT: &[u8] = b"misaka-pq-evm-v1/root/mldsa87";
/// F003 version-0x02 keyed-BLAKE2b-512 domain key for the OP-PREIMAGE digest
/// (design v1.1 §9.3 option B): `message_hash64 = keyed_blake2b_512(this,
/// op_preimage)`, then ML-DSA-87-verified under `F003_PREA_ROOT_MLDSA87_CONTEXT`.
/// A distinct domain from the address-payload key and the ML-DSA contexts so an
/// op digest can never collide with an address payload or be reinterpreted.
pub const F003_PREA_OP_MLDSA87_CONTEXT: &[u8] = b"misaka-pq-evm-v1/op/mldsa87";
/// F003 version-0x01 (FSL generic) ML-DSA-87 signing context. Reserved for the
/// Fact Settlement Layer (FSL v0.3 §4.3) generic Hash64 verification; the FSL
/// spec adopts THIS context as the canonical one for `0xF003` version 0x01.
pub const F003_FSL_VERIFY_MLDSA87_CONTEXT: &[u8] = b"misaka-pq-fsl-v1/verify/mldsa87";

/// `synthetic_withdrawal_txid` (design §9.3): the deterministic txid of the
/// synthetic UTXO output materializing one `WithdrawOp` in the accepting block
/// B's own UTXO diff. Keyed under the FROZEN
/// [`MISAKA_EVM_SYNTHETIC_OUTPOINT_CONTEXT`] domain — deliberately separate
/// from the real transaction-id domain so a synthetic outpoint can never
/// collide with a real txid. Preimage (frozen byte order):
/// `evm_tx_hash(32) ‖ op_index(4 LE) ‖ from(20) ‖ amount_sompi(8 LE)
///  ‖ spk_version(2 LE) ‖ script_len(4 LE) ‖ script(script_len)`.
///
/// Keyed by the WITHDRAWING EVM TX's keccak256 hash — NOT the accepting block
/// hash. The block hash includes the nonce and `utxo_commitment`, and the
/// synthetic output is itself part of `utxo_commitment`: a block-hash key is
/// CIRCULAR — the producer could never compute its own commitment before
/// mining (found live: the first real withdraw-bearing template self-
/// disqualified). The tx-hash key is pre-mining-stable.
///
/// The preimage ALSO binds the full materialized content `(from, amount_sompi,
/// destination script)` — not just `evm_tx_hash ‖ op_index`. The earlier
/// "withdraw content is fixed by the SIGNED tx itself" assumption holds only
/// for a top-level `EOA → F002` call; F002 is also reachable via an INNER
/// contract `CALL` (the intercept matches any frame targeting F002), so a
/// single signed tx routed through a contract can withdraw a branch-dependent
/// `(amount, script)` (the contract may read `NUMBER`/`TIMESTAMP`/`COINBASE`/
/// `PREVRANDAO`/`BASEFEE` or selected-parent state, all of which differ per
/// accepting block). The same tx can be accepted in more than one chain block
/// (`EvmTxLocations.accepted_in` allows side branches), so without binding the
/// content two acceptances would share an outpoint but carry DIFFERENT UTXO
/// content — corrupting reorg / mempool / indexer / descendant-spend semantics.
/// Content-addressing makes identical withdraws share an outpoint (as a real
/// txid would) while divergent ones can never collide. (Audit F1.)
pub fn synthetic_withdrawal_txid(
    evm_tx_hash: EvmH256,
    op_index: u32,
    from: EvmAddress,
    amount_sompi: u64,
    script_public_key: &ScriptPublicKey,
) -> Hash64 {
    let script = script_public_key.script();
    let mut preimage = Vec::with_capacity(32 + 4 + EVM_ADDRESS_SIZE + 8 + 2 + 4 + script.len());
    preimage.extend_from_slice(&evm_tx_hash.as_bytes());
    preimage.extend_from_slice(&op_index.to_le_bytes());
    preimage.extend_from_slice(&from.as_bytes()); // bind the debited EVM account
    preimage.extend_from_slice(&amount_sompi.to_le_bytes()); // bind the paid-out amount
    preimage.extend_from_slice(&script_public_key.version().to_le_bytes()); // bind the destination spk version
    preimage.extend_from_slice(&(script.len() as u32).to_le_bytes()); // length-prefix the variable-length script (no ambiguity)
    preimage.extend_from_slice(script); // bind the destination script bytes
    blake2b_512_keyed(MISAKA_EVM_SYNTHETIC_OUTPOINT_CONTEXT, &preimage)
}

// ---------------------------------------------------------------------------
// EvmAddress — 20-byte Ethereum account address.
// ---------------------------------------------------------------------------

/// Width of an [`EvmAddress`] in bytes.
pub const EVM_ADDRESS_SIZE: usize = 20;

/// A 20-byte Ethereum account address (the `evm_coinbase` / `to` surface).
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
    /// EVM account credited `(amount_sompi − claim_tip_sompi) × EVM_NATIVE_SCALE`
    /// wei. MUST equal the lock output's recorded address.
    pub evm_address: EvmAddress,
    /// Sompi amount. MUST equal the lock output's value.
    pub amount_sompi: u64,
    /// The claim-inclusion incentive (audit AH-1, v0.4 §9.2): credited as
    /// `claim_tip_sompi × EVM_NATIVE_SCALE` wei to the ACCEPTING block's
    /// declared `evm_coinbase`. MUST equal the lock output's recorded tip
    /// (≤ `amount_sompi`, consensus-validated).
    pub claim_tip_sompi: u64,
}

impl MemSizeEstimator for DepositClaim {}

/// A successful withdrawal emitted by the F002 precompile (design §8.1). The
/// executor materializes one synthetic UTXO output per `WithdrawOp` in the
/// block's UTXO diff (P4, §8.3). User-input failures do **not** emit a
/// `WithdrawOp` (they revert the tx, §8.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawOp {
    /// Receipt index of the accepted/executed EVM tx that emitted this op (its
    /// position among the block's RECEIPTS, i.e. accepted txs only — NOT the
    /// index within the source payload `transactions`, which differs whenever a
    /// tx ahead of it was skipped, classes 2/3/5). audit #7. The synthetic-UTXO
    /// key is `evm_tx_hash + op_index`, so this field is metadata for
    /// RPC/index/explorer, never a consensus key.
    pub receipt_index: u32,
    /// Index of this op within that tx (a tx may withdraw more than once).
    pub op_index: u32,
    /// keccak256 hash of the withdrawing EVM tx — the [`synthetic_withdrawal_txid`]
    /// key (pre-mining-stable, unlike the accepting block hash; see that fn).
    pub evm_tx_hash: EvmH256,
    /// EVM account debited.
    pub from: EvmAddress,
    /// Destination UTXO script (consensus script-rule validated; failure ⇒ revert).
    pub script_public_key: ScriptPublicKey,
    /// Sompi paid out (= `amount_wei / EVM_NATIVE_SCALE`, exact multiple required).
    pub amount_sompi: u64,
}

impl MemSizeEstimator for WithdrawOp {}

// The `EVM_DEPOSIT_LOCK` UTXO output is represented on the wire by the raw-`u64`
// script encoding parsed into `kaspa_txscript::script_class::EvmDepositLockFields`
// (timeout uses `u64::MAX` = never refundable). A separate in-memory mirror struct
// here previously diverged (an `Option<u64>` timeout) and was never constructed,
// serialized, or read — removed (audit INFO-a) to keep one representation.

// ---------------------------------------------------------------------------
// EvmExecutionPayload — the block-body EVM unit (design §3.1).
// ---------------------------------------------------------------------------

/// The EVM execution payload carried in a block body, separate from the UTXO
/// `transactions` because UTXO txs are DAG-inclusive-accepted while EVM txs are
/// executed via mergeset delayed acceptance (design v0.4 §3): a block's own
/// `transactions` are pure DATA — they are NOT part of this block's own
/// `EvmResult`; they are accepted/executed by the chain block whose mergeset
/// includes this block (its selected child, for a chain block). Only
/// `system_ops` (producer-selected, validated against `selected_parent(B)`)
/// execute in this block itself. Pre-activation blocks (header version &lt;
/// `EVM_HEADER_VERSION`) MUST carry the [`Default`] (empty) payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionPayload {
    /// Bounded, producer-selected system ops (deposit claims), applied before
    /// the accepted user txs and committed by `system_ops_root` in their
    /// payload order. Unlike `transactions`, these execute in THIS block.
    pub system_ops: Vec<EvmSystemOp>,
    /// EIP-2718 typed-transaction bytes, in payload order. Data-only here;
    /// executed by the accepting chain block (design v0.4 §3.1).
    pub transactions: Vec<Vec<u8>>,
    /// The producer's declared EVM coinbase (design v0.4 §8.2): receives the
    /// priority fees of THIS payload's txs when they are accepted (wherever
    /// that happens), and is the `COINBASE`/`block.coinbase` value of the EVM
    /// block this block forms when it is a chain block.
    pub evm_coinbase: EvmAddress,
    /// Optional miner extra data (consensus-rule length-capped at activation).
    pub extra_data: Vec<u8>,
}

/// kaspa-pq EVM Lane v0.4 (§15 step 6 / §16): the producer-side inputs for the
/// node's OWN template payload — the mempool-selected raw EIP-2718 txs plus the
/// miner's declared EVM coinbase (`--evm-fee-recipient`, §8.2: receives the
/// priority fees of this payload's txs when they are accepted). `Default` = no
/// candidates + zero coinbase = the empty pre-§16 payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvmTemplateData {
    pub evm_coinbase: EvmAddress,
    pub transactions: Vec<Vec<u8>>,
    /// §9.2 producer-selected deposit claims (resolved + pre-validated by the
    /// node from submitted lock outpoints). The VSP template path re-validates
    /// each against the live selected-parent claim view before committing, so a
    /// stale claim is dropped rather than invalidating the node's own block.
    pub system_ops: Vec<DepositClaim>,
}

impl EvmExecutionPayload {
    /// An EVM-inert payload (`== Default`): no system ops, no txs, no extra
    /// data, zero coinbase. Pre-activation blocks must satisfy this.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.system_ops.is_empty() && self.transactions.is_empty() && self.extra_data.is_empty() && self.evm_coinbase.is_zero()
    }

    /// The canonical commitment encoding of this payload (borsh, design v0.4
    /// §4.1) — the byte string [`payload_hash_of_bytes`] digests and the D4
    /// inclusion-side size cap measures.
    #[inline]
    pub fn payload_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("EvmExecutionPayload borsh serialization is infallible")
    }

    /// `EvmPayloadHash(B)` (design v0.4 §4.1) — keyed BLAKE2b-512 under
    /// [`MISAKA_EVM_PAYLOAD_HASH_CONTEXT`] over this payload's borsh encoding,
    /// carried in `Header::evm_payload_hash`. The DATA commitment: it binds the
    /// payload bytes a producer includes, independent of who later accepts and
    /// executes them. Pure (no revm), so every build can verify it at body
    /// validation.
    #[inline]
    pub fn payload_hash(&self) -> Hash64 {
        payload_hash_of_bytes(&self.payload_bytes())
    }
}

/// [`EvmExecutionPayload::payload_hash`] over pre-serialized payload bytes
/// (lets body validation serialize once for both the size cap and the digest).
#[inline]
pub fn payload_hash_of_bytes(bytes: &[u8]) -> Hash64 {
    blake2b_512_keyed(MISAKA_EVM_PAYLOAD_HASH_CONTEXT, bytes)
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
    /// Selected-parent-tree height: `evm_number(selected_parent) + 1` (§5.2).
    pub evm_number: u64,
    /// Non-decreasing EVM logical time `max(header_ts_sec, parent_ts)` (design
    /// v0.4 §5.3, D6 — replaced the v0.2 strict-monotone clamp; consecutive EVM
    /// blocks may share a timestamp).
    pub evm_timestamp_sec: u64,
    pub evm_chain_id: u64,
    /// The accepting block's declared `evm_coinbase` — the `COINBASE` opcode
    /// value of this EVM block (design v0.4 §8.2, audit AM-3). Priority fees
    /// route per-tx to each PAYLOAD block's coinbase, not (necessarily) here.
    pub coinbase: EvmAddress,
    /// Accepted-and-executed user txs in this block's mergeset acceptance
    /// (skips excluded; class-4 executed failures included). (v0.4 §4.2)
    pub accepted_tx_count: u32,
    /// Deterministically skipped user txs (classes 2/3/5). Statistics only —
    /// skips leave no other trace in the execution result. (v0.4 §4.2/§6)
    pub skipped_tx_count: u32,
    /// Total native (wei) balance held across ALL EVM accounts after this
    /// block — the O(1) supply-invariant accumulator (v0.4 §9.1, audit AM-5):
    /// `total(B) = total(parent) + deposits(B) − withdrawals(B) − burn(B)`.
    pub evm_total_native_balance: EvmU256,
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
/// Per-candidate outcome of one acceptance run (§6.1), parallel to the input
/// `AcceptedEvmTxs(B)` order. Store/RPC data ONLY — outcomes never enter the
/// commitment preimage (the committed surface is `EvmExecutionHeader`), so the
/// receipt/lookup indexes can evolve without a fork.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmCandidateOutcome {
    /// Executed: `receipt_index` points into `EvmExecutionResult::receipts`.
    Accepted { receipt_index: u32 },
    /// Deterministically skipped with the §6.1 class (2 = acceptance-invalid
    /// [subsumes 3], 5 = over-cap prefix-take).
    Skipped { class: u8 },
}

impl MemSizeEstimator for EvmCandidateOutcome {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmExecutionResult {
    pub header: EvmExecutionHeader,
    pub receipts: Vec<EvmReceipt>,
    /// Withdrawal ops in receipt/log order → synthetic UTXO outputs (P4).
    pub withdrawals: Vec<WithdrawOp>,
    /// Deposit claims applied this block → consumed lock outputs (P4).
    pub applied_deposit_claims: Vec<DepositClaim>,
    /// §16: per-candidate outcomes, parallel to the acceptance input order
    /// (feeds the tx-lookup index; NOT part of the commitment).
    pub candidate_outcomes: Vec<EvmCandidateOutcome>,
}

impl MemSizeEstimator for EvmExecutionResult {}

/// The receipts row of one ACCEPTING chain block (§16, store prefix 203):
/// receipts in accepted order plus the parallel Ethereum tx hashes (so
/// `eth_getTransactionReceipt` can return `transactionHash` without re-reading
/// payloads). Store/RPC data only — never part of the commitment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockReceipts {
    pub receipts: Vec<EvmReceipt>,
    pub tx_hashes: Vec<EvmH256>,
}

impl MemSizeEstimator for EvmBlockReceipts {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.tx_hashes.capacity() * size_of::<EvmH256>()
            + self.receipts.capacity() * size_of::<EvmReceipt>()
            + self.receipts.iter().map(|r| r.logs.iter().map(|l| l.data.capacity() + l.topics.capacity() * 32).sum::<usize>()).sum::<usize>()
    }
}

/// The tx-lookup row (§16, store prefix 204): where a tx hash was SEEN
/// (payload blocks, DA visibility), where it was ACCEPTED (executing chain
/// blocks — side branches allowed, the reader resolves canonicality against
/// the current chain), and the most recent skip class when never accepted
/// (informational, §6.1). All vecs are bounded at write time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmTxLocations {
    /// Payload blocks carrying the raw tx (bounded; inclusion ≠ execution).
    pub included_in: Vec<Hash64>,
    /// `(accepting chain block, receipt index)` per acceptance (bounded).
    pub accepted_in: Vec<(Hash64, u32)>,
    /// §6.1 class of the most recent skip, while never accepted (2 or 5).
    pub last_skip_class: Option<u8>,
}

impl MemSizeEstimator for EvmTxLocations {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + self.included_in.capacity() * size_of::<Hash64>() + self.accepted_in.capacity() * (size_of::<Hash64>() + 4)
    }
}

/// Write-time bounds for [`EvmTxLocations`] (DoS caps on a single row).
pub const MAX_TX_LOCATION_INCLUSIONS: usize = 16;
pub const MAX_TX_LOCATION_ACCEPTANCES: usize = 8;

/// A canonical-resolved receipt view (§16 `eth_getTransactionReceipt`
/// semantics): the ACCEPTING chain block currently on the selected chain, its
/// EVM number, and the executed receipt. `None` upstream = the tx is not
/// accepted under the current chain (it may be included-but-skipped, pending,
/// or on an orphaned branch — `EvmTxLocations` tells which).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmTxReceiptView {
    pub accepting_block: Hash64,
    pub evm_number: u64,
    pub receipt_index: u32,
    /// Block-global index of this receipt's FIRST log = the sum of log counts of
    /// all receipts before it in the accepting block. The eth-rpc adapter renders
    /// each log's `logIndex` as `log_index_offset + i` so a receipt's `logIndex`
    /// matches `eth_getLogs` (audit H-05 — both must be block-global).
    pub log_index_offset: u32,
    pub receipt: EvmReceipt,
}

/// A canonical-resolved EVM "block" for the eth-rpc adapter (§16
/// `eth_getBlockByNumber` / `eth_getBlockByHash`): the executed header, the
/// accepting L1 block hash (its first 32 bytes are the eth-rpc `blockHash`), and
/// the block's accepted tx hashes (in accepted order). Store/RPC data only —
/// never part of a commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmBlockResponse {
    pub header: EvmExecutionHeader,
    pub l1_hash: Hash64,
    pub tx_hashes: Vec<EvmH256>,
}

/// One resolved EVM log for `eth_getLogs` (§16): the log plus its canonical
/// block/tx context. `block_l1_hash`'s first 32 bytes are the eth-rpc `blockHash`.
/// Store/RPC data only — never part of a commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmLogEntry {
    pub address: EvmAddress,
    pub topics: Vec<EvmH256>,
    pub data: Vec<u8>,
    pub block_number: u64,
    pub block_l1_hash: Hash64,
    pub tx_hash: EvmH256,
    /// Receipt index = transaction index within the accepting block.
    pub tx_index: u32,
    /// Log index within the accepting block (across all its receipts).
    pub log_index: u32,
}

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

    /// PREA P0-1: the F003 ML-DSA-87 signing contexts must be domain-separated from
    /// the UTXO address-payload key and every other ML-DSA-87 context, or a
    /// UTXO/attestation/tx signature could be cross-protocol-replayed as an EVM root
    /// authorization (the C-02 class). Also pins the frozen byte values + layout.
    #[test]
    fn f003_contexts_are_domain_separated_and_layout_frozen() {
        assert_eq!(F003_PREA_ROOT_MLDSA87_CONTEXT, b"misaka-pq-evm-v1/root/mldsa87");
        assert_eq!(F003_FSL_VERIFY_MLDSA87_CONTEXT, b"misaka-pq-fsl-v1/verify/mldsa87");
        // distinct from each other …
        assert_ne!(F003_PREA_ROOT_MLDSA87_CONTEXT, F003_FSL_VERIFY_MLDSA87_CONTEXT);
        // … and from the UTXO address-payload key + the tx/attestation contexts.
        assert_ne!(F003_PREA_ROOT_MLDSA87_CONTEXT, kaspa_hashes::MLDSA87_ADDRESS_CONTEXT);
        assert_ne!(F003_FSL_VERIFY_MLDSA87_CONTEXT, kaspa_hashes::MLDSA87_ADDRESS_CONTEXT);
        assert_ne!(F003_PREA_ROOT_MLDSA87_CONTEXT, &b"kaspa-pq-v2/tx/mldsa87"[..]);
        assert_ne!(F003_PREA_ROOT_MLDSA87_CONTEXT, &b"kaspa-pq-v1/att/mldsa87"[..]);
        // the op-preimage digest domain is distinct from the root + address contexts.
        assert_eq!(F003_PREA_OP_MLDSA87_CONTEXT, b"misaka-pq-evm-v1/op/mldsa87");
        assert_ne!(F003_PREA_OP_MLDSA87_CONTEXT, F003_PREA_ROOT_MLDSA87_CONTEXT);
        assert_ne!(F003_PREA_OP_MLDSA87_CONTEXT, kaspa_hashes::MLDSA87_ADDRESS_CONTEXT);
        // frozen input layout (version-discriminated).
        assert_eq!(F003_INPUT_LEN_FSL, 1 + 2592 + 64 + 4627);
        assert_eq!(F003_PREA_PREFIX_LEN, 1 + 64 + 2592 + 4627);
        assert_ne!(F003_VERSION_FSL_GENERIC, F003_VERSION_PREA_ROOT);
        // the precompile address is 0x…F003 (distinct from F001 WMISAKA / F002 withdraw).
        assert_eq!(MISAKA_MLDSA_VERIFY_PRECOMPILE.as_bytes()[19], 0x03);
        assert_eq!(MISAKA_MLDSA_VERIFY_PRECOMPILE.as_bytes()[18], 0xF0);
    }

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
                claim_tip_sompi: 0,
            })],
            ..Default::default()
        };
        assert!(!p3.is_empty());
    }

    /// EVM audit C3: the per-block deposit-claim BYTE and SYSTEM-GAS bounds are
    /// enforced via the COUNT cap (`check_evm_payload` rejects >
    /// `MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK` ops) — that implication only holds
    /// while a claim's serialized size is fixed and the products stay under the
    /// byte/gas consts. This test pins the implication: if `DepositClaim` ever
    /// grows (or goes variable-length), or the caps drift, it fails and the
    /// byte/gas bounds must become EXPLICIT body-validation rules.
    #[test]
    fn claim_count_cap_subsumes_byte_and_system_gas_bounds() {
        let max_claim = EvmSystemOp::DepositClaim(DepositClaim {
            deposit_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0xFF; 64]), u32::MAX),
            evm_address: EvmAddress::from_bytes([0xFF; 20]),
            amount_sompi: u64::MAX,
            claim_tip_sompi: u64::MAX,
        });
        let min_claim = EvmSystemOp::DepositClaim(DepositClaim {
            deposit_outpoint: TransactionOutpoint::default(),
            evm_address: EvmAddress::default(),
            amount_sompi: 0,
            claim_tip_sompi: 0,
        });
        let max_size = borsh::to_vec(&max_claim).unwrap().len();
        // Fixed-width: extreme and default claims serialize to the same length,
        // so `count cap × size` is exact, not an estimate.
        assert_eq!(
            max_size,
            borsh::to_vec(&min_claim).unwrap().len(),
            "DepositClaim went variable-length: make the byte bound an explicit rule"
        );

        // Count cap ⇒ byte bound.
        assert!(
            MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK * max_size <= MAX_DEPOSIT_CLAIM_BYTES_PER_EVM_BLOCK,
            "256 maximal claims ({} B each) exceed MAX_DEPOSIT_CLAIM_BYTES_PER_EVM_BLOCK: enforce the byte bound explicitly",
            max_size
        );
        // Count cap ⇒ system-gas bound.
        assert!(
            (MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK as u64) * SYSTEM_DEPOSIT_GAS_PER_CLAIM <= MAX_SYSTEM_GAS_PER_EVM_BLOCK,
            "256 claims exceed MAX_SYSTEM_GAS_PER_EVM_BLOCK: enforce the gas bound explicitly"
        );
        // And a count-cap-maximal payload still fits the §7 payload byte cap.
        let payload = EvmExecutionPayload {
            system_ops: (0..MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK).map(|_| max_claim.clone()).collect(),
            ..Default::default()
        };
        assert!(payload.payload_bytes().len() <= MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK);
    }

    #[test]
    fn payload_hash_is_deterministic_domain_separated_and_field_sensitive() {
        // v0.4 §4.1: the payload DATA commitment carried in `Header::evm_payload_hash`.
        let p = EvmExecutionPayload::default();
        let h_empty = p.payload_hash();
        assert_eq!(h_empty, p.payload_hash(), "deterministic");
        assert_ne!(h_empty, Hash64::default(), "a keyed digest, not the zero default");
        // Every field participates: txs and the declared evm_coinbase.
        let p_tx = EvmExecutionPayload { transactions: vec![vec![1, 2, 3]], ..Default::default() };
        assert_ne!(p_tx.payload_hash(), h_empty);
        let p_cb = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes([0xFE; 20]), ..Default::default() };
        assert_ne!(p_cb.payload_hash(), h_empty);
        // Domain separation from the execution commitment (b"EvmPayload64" vs
        // b"EvmCommitment64"): the two roots can never alias.
        assert_ne!(h_empty, EvmExecutionHeader::default().commitment_root());
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
