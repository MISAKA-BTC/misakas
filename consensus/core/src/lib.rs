//!
//! # Consensus Core
//!
//! This crate implements primitives used in the Kaspa node consensus processing.
//!

extern crate alloc;
extern crate core;
extern crate self as consensus_core;

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

pub use kaspa_hashes::Hash;
// PR-9.5f: companion re-export so downstream crates (rpc/grpc, wallet,
// rpc-service) that depend on consensus-core but not directly on
// kaspa-hashes can name the 64-byte type as `kaspa_consensus_core::Hash64`,
// mirroring the long-standing `kaspa_consensus_core::Hash` path.
pub use kaspa_hashes::Hash64;
// kaspa-pq Selected-Parent EVM Lane (ADR-0020): the 32-byte Ethereum-compatible
// hash used by the EVM execution lane (state/tx/receipts roots, EVM block hash).
pub use kaspa_hashes::EvmH256;

// ---------------------------------------------------------------------
// kaspa-pq Phase 9 (PR-9.5b): consensus-identity semantic aliases.
//
// The Hash → Hash64 cascade (ADR-0008 / docs/hash64-migration-inventory.md)
// stages width changes per identity. This module introduces NAMES for
// each identity, all pointing at the upstream 32-byte `Hash` /
// `kaspa_hashes::Hash32` today. PR-9.5c onward flips individual
// aliases to `kaspa_hashes::Hash64` one identity at a time:
//
//   PR-9.5c    TransactionId, TransactionHash, MerkleHash, MerkleRoot,
//              AcceptedIdMerkleRoot                       → Hash64
//   PR-9.5d    Header.pow_algo_id field (PR-8.4)          → (additive)
//   PR-9.5e    BlockHash + PruningPoint (both are block-hash
//              identities) and every user — the header, stores,
//              GHOSTDAG, reachability, pruning, relations  → Hash64
//
// kaspa-pq (ADR-0004 / design §12): UtxoCommitment is 64-byte
// (BLAKE2b-512 of the LtHash state) like every other PQ consensus
// identity. It is an accumulator commitment, not a block-hash
// identity, so it is never keyed into a BlockHashMap.
//
// `LegacyHash32` is the **stable** 32-byte name — it is the alias
// that NEVER widens. Use it in source that wants to be explicit
// about staying 32 B (RNG seeds, debug fingerprints, cache keys
// that are not on the consensus surface). The `algo_id = 1`
// kHeavyHash L1 inner-loop seed (PR-8.5 / PR-9.3 §"l1_seed32") is
// the canonical example.
//
// Adding aliases here is **purely additive**: no existing call
// site changes, no type-system pressure, no breaking semantics.
// The actual width flips happen in PR-9.5c onward when individual
// alias bodies switch from `kaspa_hashes::Hash32` to
// `kaspa_hashes::Hash64`.
// ---------------------------------------------------------------------

/// Stable 32-byte hash alias for surface that **does not** widen to
/// `Hash64` under ADR-0008. Use this for RNG seeds, debug
/// fingerprints, and cache keys that are not part of the consensus
/// identity surface. The `algo_id = 1` kHeavyHash L1 seed
/// (`l1_seed32`, PR-9.3) is the canonical caller.
pub type LegacyHash32 = kaspa_hashes::Hash32;

/// Block identity — the header hash returned by `Block::hash()` and
/// stored in `Header::hash`. **Flipped to `Hash64` in PR-9.5e**
/// (ADR-0008): the identity digest is now produced by the keyed
/// BLAKE2b-512 `BlockHash64` hasher (crypto/hashes/src/hashers.rs).
/// Keys `BlockHashMap` / `BlockHashSet` and flows through every
/// store, GHOSTDAG, reachability and pruning structure.
pub type BlockHash = kaspa_hashes::Hash64;

/// Transaction id — the `TransactionId` returned by the upstream
/// `TransactionHasher` flow (txid). **Flipped to `Hash64` in
/// PR-9.5c** per ADR-0008 §"Full Hash64 consensus identity"; the
/// underlying digest is now produced by the keyed BLAKE2b-512
/// `TransactionId64` hasher from PR-9.2 / crypto/hashes/src/hashers.rs.
pub type TransactionId = kaspa_hashes::Hash64;

/// Full-content transaction hash — distinct from [`TransactionId`]
/// (which omits witness data per upstream Kaspa convention).
/// **Flipped to `Hash64` in PR-9.5c**; underlying hasher is the
/// keyed BLAKE2b-512 `TransactionHash64`.
pub type TransactionHash = kaspa_hashes::Hash64;

/// Generic merkle-tree node hash (intermediate digest along a
/// merkle path). **Flipped to `Hash64` in PR-9.5c**; underlying
/// hasher is the keyed BLAKE2b-512 `MerkleBranchHash64`.
pub type MerkleHash = kaspa_hashes::Hash64;

/// Merkle root over a block's transaction id list
/// (`Header::hash_merkle_root`). **Flipped to `Hash64` in PR-9.5c**.
pub type MerkleRoot = kaspa_hashes::Hash64;

/// Merkle root over a block's accepted-transaction-id list
/// (`Header::accepted_id_merkle_root`). **Flipped to `Hash64` in
/// PR-9.5c**; underlying hasher is the keyed BLAKE2b-512
/// `AcceptedIdMerkleBranchHash64`.
pub type AcceptedIdMerkleRoot = kaspa_hashes::Hash64;

/// Pruning-point block hash (`Header::pruning_point`). This is a
/// **block-hash identity** (it references the pruning-point block),
/// so it **flipped to `Hash64` in PR-9.5e** together with
/// [`BlockHash`] — a 32-byte pruning point could not key the
/// Hash64 block stores.
pub type PruningPoint = kaspa_hashes::Hash64;

pub mod acceptance_data;
pub mod api;
pub mod block;
pub mod blockhash;
pub mod blockstatus;
pub mod coinbase;
pub mod config;
pub mod constants;
pub mod daa_score_timestamp;
/// kaspa-pq Phase 10 (PR-10.3): DNS Probabilistic Finality Overlay
/// type stubs (see docs/adr/0009-dns-probabilistic-finality.md).
/// Carries the type surface only; consensus rule implementations
/// (StakeScore aggregation, reorg gate) land in PR-10.4
/// onward once Phases 1–9 stabilise.
pub mod dns_finality;
pub mod errors;
/// kaspa-pq Selected-Parent EVM Lane (ADR-0020): EVM execution-lane consensus
/// types (block-body payload, executor-output header, EVM-domain newtypes).
/// Types only; the revm executor lives behind the `evm` cargo feature (P2).
pub mod evm;
pub mod hashing;
pub mod header;
pub mod mass;
pub mod merkle;
pub mod mining_rules;
pub mod muhash;
pub mod network;
/// kaspa-pq Phase 8 (PR-8.3): Layer 0 PoW finalizer + difficulty-lift
/// helpers (see docs/adr/0007-layered-pow.md). Self-contained; the
/// PoW-validation wiring step is PR-8.6.
pub mod pow_layer0;
pub mod pruning;
pub mod sign;
pub mod subnets;
pub mod trusted;
pub mod tx;
pub mod utxo;

/// Integer type for accumulated PoW of blue blocks.
///
/// kaspa-pq Phase 8 (PR-8.5) widened this from `Uint192` to `Uint576`
/// per ADR-0007 §"Width chain": the 576-bit width is one machine word
/// above the 512-bit PoW comparison domain (`Uint512`), so a 2^64
/// window of maximum-work blocks accumulates without overflow. The
/// previous upstream comment ("no more than 2^192 work overall") is
/// retained as historical context but no longer drives the type
/// choice — the Layer 0 PoW domain does.
pub type BlueWorkType = kaspa_math::Uint576;

/// The extends directly from the expectation above about having no more than
/// 2^128 work in a single block
pub const MAX_WORK_LEVEL: BlockLevel = 128;

/// The type used to represent the GHOSTDAG K parameter
pub type KType = u16;

/// Map from Block hash to K type
pub type HashKTypeMap = std::sync::Arc<BlockHashMap<KType>>;

/// This HashMap skips the hashing of the key and uses the key directly as the hash.
/// Should only be used for block hashes that have correct DAA,
/// otherwise it is susceptible to DOS attacks via hash collisions.
pub type BlockHashMap<V> = HashMap<BlockHash, V, BlockHasher>;

/// Same as `BlockHashMap` but a `HashSet`.
pub type BlockHashSet = HashSet<BlockHash, BlockHasher>;

pub trait HashMapCustomHasher {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

// HashMap::new and HashMap::with_capacity are only implemented on Hasher=RandomState
// to avoid type inference problems, so we need to provide our own versions.
impl<V> HashMapCustomHasher for BlockHashMap<V> {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

impl HashMapCustomHasher for BlockHashSet {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

#[derive(Default, Debug)]
pub struct ChainPath {
    pub added: Vec<BlockHash>,
    pub removed: Vec<BlockHash>,
}

/// PR-9.5e: the 64-byte `BlockHash` (`Hash64`) writes 8 u64s via its
/// `StdHash` impl; we keep only the last word as the in-memory map
/// hash (same prefix trick the 32-byte `Hash` used with 4 words).
#[derive(Default, Clone, Copy)]
pub struct BlockHasher(u64);

impl BlockHasher {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(0)
    }
}

impl Hasher for BlockHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline(always)]
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
    #[cold]
    fn write(&mut self, _: &[u8]) {
        unimplemented!("use write_u64")
    }
}

impl BuildHasher for BlockHasher {
    type Hasher = Self;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self(0)
    }
}

pub type BlockLevel = u8;

#[cfg(test)]
mod tests {
    use super::BlockHasher;
    use crate::BlockHash;
    use std::hash::{Hash as _, Hasher as _};
    #[test]
    fn test_block_hasher() {
        // PR-9.5e: `BlockHash` is now the 64-byte `Hash64`, whose
        // `StdHash` writes 8 little-endian u64 words; the hasher
        // keeps the last (8th).
        let hash = BlockHash::from_le_u64([1, 2, 3, 4, 5, 6, 7, 8]);
        let mut hasher = BlockHasher::default();
        hash.hash(&mut hasher);
        assert_eq!(hasher.finish(), 8);
    }
}
