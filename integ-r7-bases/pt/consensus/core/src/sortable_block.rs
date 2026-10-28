//! The canonical order between two blocks: blue work, then hash.
//!
//! This lived in `consensus/src/processes/ghostdag/ordering.rs`, where GHOSTDAG uses it to sort
//! mergesets. It is here because it is not only GHOSTDAG's: chain selection outside consensus —
//! notably deciding whether a verified IBD candidate beats the chain a node is already holding —
//! must reach the same verdict as the DAG would, and the p2p crates cannot depend on the consensus
//! crate to say so.
//!
//! The alternative was a second implementation of `(blue_work, hash)` next to the first. Two
//! implementations of a fork-choice rule do not stay equal: a change to the tie-break, or to the
//! hash ordering underneath it, would silently make one path prefer a chain the other rejects. One
//! definition, used by both.

use crate::{BlockHash, BlueWorkType};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A block reduced to what fork choice needs.
///
/// Ordering is by blue work, ties broken by hash. The tie-break is not decoration: without it every
/// work tie is an impasse, and work ties happen — two independently mined branches measured
/// identical work at their pruning points, and a node that should have had a decisive answer had
/// none.
///
/// `PartialEq` is by hash alone, deliberately: two `SortableBlock`s are the same block or they are
/// not, whatever work was recorded for them.
#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct SortableBlock {
    pub hash: BlockHash,
    pub blue_work: BlueWorkType,
}

impl SortableBlock {
    pub fn new(hash: BlockHash, blue_work: BlueWorkType) -> Self {
        Self { hash, blue_work }
    }
}

impl PartialEq for SortableBlock {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl PartialOrd for SortableBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.blue_work.cmp(&other.blue_work).then_with(|| self.hash.cmp(&other.hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_decides_first_and_the_hash_only_breaks_ties() {
        let heavy = SortableBlock::new(BlockHash::from_u64_word(1), BlueWorkType::from_u64(100));
        let light = SortableBlock::new(BlockHash::from_u64_word(9), BlueWorkType::from_u64(99));
        assert!(heavy > light, "more work wins even with the lower hash");

        let a = SortableBlock::new(BlockHash::from_u64_word(1), BlueWorkType::from_u64(100));
        let b = SortableBlock::new(BlockHash::from_u64_word(2), BlueWorkType::from_u64(100));
        assert_ne!(a.cmp(&b), Ordering::Equal, "a work tie must still resolve");
        assert_eq!(a.cmp(&b), a.hash.cmp(&b.hash));
    }

    #[test]
    fn equal_means_the_same_block() {
        let a = SortableBlock::new(BlockHash::from_u64_word(7), BlueWorkType::from_u64(5));
        let same = SortableBlock::new(BlockHash::from_u64_word(7), BlueWorkType::from_u64(5));
        assert_eq!(a.cmp(&same), Ordering::Equal);
        assert_eq!(a, same);
    }
}
