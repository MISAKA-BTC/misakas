use crate::{BlockHash, BlockHashSet, HashMapCustomHasher};
use kaspa_hashes::HASH64_SIZE;
use std::sync::Arc;

pub type BlockHashes = Arc<Vec<BlockHash>>;

// PR-9.5e: NONE / ORIGIN are 64-byte sentinels now that `BlockHash`
// is `Hash64`. The all-`0xfe` ORIGIN pattern is preserved (just
// 64 bytes wide); it stays disjoint from any real BLAKE2b-512 block
// hash with overwhelming probability, exactly as the 32-byte original did.

/// `blockhash::NONE` is a hash which is used in rare cases as the `None` block hash
pub const NONE: BlockHash = BlockHash::from_bytes([0u8; HASH64_SIZE]);

/// `blockhash::ORIGIN` is a special hash representing a `virtual genesis` block.
/// It serves as a special local block which all locally-known
/// blocks are in its future.
pub const ORIGIN: BlockHash = BlockHash::from_bytes([0xfe; HASH64_SIZE]);

pub trait BlockHashExtensions {
    fn is_none(&self) -> bool;
    fn is_origin(&self) -> bool;
}

impl BlockHashExtensions for BlockHash {
    fn is_none(&self) -> bool {
        self.eq(&NONE)
    }

    fn is_origin(&self) -> bool {
        self.eq(&ORIGIN)
    }
}

/// Generates a unique block hash for each call to this function.
/// To be used for test purposes only.
pub fn new_unique() -> BlockHash {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    BlockHash::from_u64_word(c)
}

pub trait BlockHashIteratorExtensions: Iterator<Item = BlockHash> {
    /// Copy of itertools::unique, adapted for block hashes (uses `BlockHashSet` under the hood)
    ///
    /// Returns an iterator adaptor that filters out hashes that have
    /// already been produced once during the iteration.
    ///
    /// Clones of visited elements are stored in a hash set in the
    /// iterator.
    ///
    /// The iterator is stable, returning the non-duplicate items in the order
    /// in which they occur in the adapted iterator. In a set of duplicate
    /// items, the first item encountered is the item retained.
    ///
    /// NOTE: currently usages are expected to contain no duplicates, hence we alloc the expected capacity
    fn block_unique(self) -> BlockUnique<Self>
    where
        Self: Sized,
    {
        let (lower, _) = self.size_hint();
        BlockUnique { iter: self, seen: BlockHashSet::with_capacity(lower) }
    }
}

impl<T: ?Sized> BlockHashIteratorExtensions for T where T: Iterator<Item = BlockHash> {}

#[derive(Clone)]
pub struct BlockUnique<I: Iterator<Item = BlockHash>> {
    iter: I,
    seen: BlockHashSet,
}

impl<I> Iterator for BlockUnique<I>
where
    I: Iterator<Item = BlockHash>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.by_ref().find(|&hash| self.seen.insert(hash))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, hi) = self.iter.size_hint();
        ((low > 0 && self.seen.is_empty()) as usize, hi)
    }
}

impl<I> DoubleEndedIterator for BlockUnique<I>
where
    I: DoubleEndedIterator<Item = BlockHash>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.by_ref().rev().find(|&hash| self.seen.insert(hash))
    }
}

impl<I> std::iter::FusedIterator for BlockUnique<I> where I: Iterator<Item = BlockHash> + std::iter::FusedIterator {}
