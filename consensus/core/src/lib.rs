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

pub mod acceptance_data;
pub mod api;
pub mod block;
pub mod blockhash;
pub mod blockstatus;
pub mod coinbase;
pub mod config;
pub mod constants;
pub mod daa_score_timestamp;
pub mod errors;
pub mod hashing;
pub mod header;
pub mod mass;
pub mod merkle;
pub mod mining_rules;
pub mod muhash;
pub mod network;
pub mod pruning;
pub mod sign;
pub mod subnets;
pub mod trusted;
pub mod tx;
pub mod utxo;
/// kaspa-pq Phase 7 (PR-7.6): 64-byte production UTXO commitment type
/// (see docs/adr/0004-utxo-commitment64.md). The header field is still
/// 32-byte `Hash` for the PoC; this module exists so the header switch
/// PR is a small mechanical type swap.
pub mod utxo_commitment;
/// kaspa-pq Phase 8 (PR-8.3): Layer 0 PoW finalizer + difficulty-lift
/// helpers (see docs/adr/0007-layered-pow.md). Self-contained; the
/// PoW-validation wiring step is PR-8.6.
pub mod pow_layer0;

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
pub type BlockHashMap<V> = HashMap<Hash, V, BlockHasher>;

/// Same as `BlockHashMap` but a `HashSet`.
pub type BlockHashSet = HashSet<Hash, BlockHasher>;

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
    pub added: Vec<Hash>,
    pub removed: Vec<Hash>,
}

/// `hashes::Hash` writes 4 u64s so we just use the last one as the hash here
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
    use kaspa_hashes::Hash;
    use std::hash::{Hash as _, Hasher as _};
    #[test]
    fn test_block_hasher() {
        let hash = Hash::from_le_u64([1, 2, 3, 4]);
        let mut hasher = BlockHasher::default();
        hash.hash(&mut hasher);
        assert_eq!(hasher.finish(), 4);
    }
}
