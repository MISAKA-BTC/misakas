//! An operator's statement of which history is real.
//!
//! A node with no chain of its own cannot work this out. Given two internally consistent histories
//! it can compare work, but work is what an attacker with hardware — or a long-range attacker with
//! retired validator keys — can manufacture. Every proof-of-stake design hits this, and ADR-0009
//! says so plainly: weak subjectivity is not eliminated, and a node that has been offline longer
//! than the reorg horizon needs a checkpoint from outside itself.
//!
//! This is that outside. It is a hard constraint, not a score: a candidate chain either descends
//! from the checkpoint or it is not a candidate. Chain *quality* is still decided by verified work
//! among the chains that qualify — the checkpoint says which histories are admissible, never which
//! of them is best.
//!
//! Deliberately **not** derived from peers. "Enough peers agreed" is not a trust root; an attacker
//! who can eclipse a node can also supply its quorum, and adopting the majority of whoever happens
//! to answer is the same arrival-order decision this whole effort exists to remove, wearing a
//! better hat. Where a peer-derived checkpoint is used at all it must be an explicitly unsafe,
//! non-mainnet fallback that says so in the logs.

use std::{fmt, str::FromStr};

use kaspa_hashes::{Hash, Hash64};
use serde::{Deserialize, Serialize};

/// A history the operator vouches for: "at this DAA score, this block was canonical, under these
/// rules."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedCheckpoint {
    /// DAA score of the checkpoint block. Recorded so a node can say how stale its trust root is,
    /// and so an obviously wrong pairing (hash from one epoch, score from another) is visible.
    pub daa_score: u64,
    /// The block that must be an ancestor of any admissible chain.
    pub block_hash: Hash64,
    /// The rule set the checkpoint was taken under. A block hash means nothing without it: the
    /// same history validated under different rules is a different history, which is how
    /// testnet-22 forked.
    pub consensus_params_id: Hash,
}

/// Why a `--trusted-checkpoint` string was rejected.
///
/// Specific rather than a single "malformed": an operator typing a checkpoint by hand is doing the
/// most security-critical configuration on the node, and "wrong number of parts" versus "that is
/// not a hash" are different mistakes.
#[derive(Debug, PartialEq, Eq)]
pub enum TrustedCheckpointParseError {
    /// Not exactly `<daa>:<hash>:<params-id>`.
    WrongShape(usize),
    InvalidDaaScore(String),
    InvalidBlockHash(String),
    InvalidConsensusParamsId(String),
}

impl fmt::Display for TrustedCheckpointParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongShape(n) => {
                write!(f, "expected <daa-score>:<block-hash>:<consensus-params-id> (3 colon-separated parts), got {n}")
            }
            Self::InvalidDaaScore(s) => write!(f, "DAA score {s:?} is not a number"),
            Self::InvalidBlockHash(s) => write!(f, "block hash {s:?} is not a valid 64-byte hex hash"),
            Self::InvalidConsensusParamsId(s) => write!(f, "consensus params id {s:?} is not a valid 32-byte hex hash"),
        }
    }
}

impl std::error::Error for TrustedCheckpointParseError {}

impl FromStr for TrustedCheckpoint {
    type Err = TrustedCheckpointParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(TrustedCheckpointParseError::WrongShape(parts.len()));
        }
        let daa_score = parts[0].parse::<u64>().map_err(|_| TrustedCheckpointParseError::InvalidDaaScore(parts[0].to_owned()))?;
        let block_hash = Hash64::from_str(parts[1]).map_err(|_| TrustedCheckpointParseError::InvalidBlockHash(parts[1].to_owned()))?;
        let consensus_params_id =
            Hash::from_str(parts[2]).map_err(|_| TrustedCheckpointParseError::InvalidConsensusParamsId(parts[2].to_owned()))?;
        Ok(Self { daa_score, block_hash, consensus_params_id })
    }
}

impl fmt::Display for TrustedCheckpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.daa_score, self.block_hash, self.consensus_params_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH64: &str = "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002a";
    const HASH32: &str = "000000000000000000000000000000000000000000000000000000000000002a";

    #[test]
    fn round_trips_through_its_own_rendering() {
        let s = format!("12345:{HASH64}:{HASH32}");
        let cp: TrustedCheckpoint = s.parse().unwrap();
        assert_eq!(cp.daa_score, 12345);
        assert_eq!(cp.to_string(), s, "what a node prints must be what it accepts");
    }

    #[test]
    fn each_way_of_getting_it_wrong_says_which() {
        // An operator typing this by hand is doing the most security-critical configuration on the
        // node; "malformed" would not help them.
        assert_eq!("1:2".parse::<TrustedCheckpoint>(), Err(TrustedCheckpointParseError::WrongShape(2)));
        assert!(matches!(
            format!("notanumber:{HASH64}:{HASH32}").parse::<TrustedCheckpoint>(),
            Err(TrustedCheckpointParseError::InvalidDaaScore(_))
        ));
        assert!(matches!(
            format!("1:deadbeef:{HASH32}").parse::<TrustedCheckpoint>(),
            Err(TrustedCheckpointParseError::InvalidBlockHash(_))
        ));
        assert!(matches!(
            format!("1:{HASH64}:deadbeef").parse::<TrustedCheckpoint>(),
            Err(TrustedCheckpointParseError::InvalidConsensusParamsId(_))
        ));
    }

    #[test]
    fn the_two_hashes_are_not_interchangeable() {
        // Block hashes are 64 bytes and params ids 32; swapping them must not parse, or an operator
        // could pin the wrong thing and believe they were protected.
        assert!(format!("1:{HASH32}:{HASH64}").parse::<TrustedCheckpoint>().is_err());
    }
}
