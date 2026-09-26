//! **Node-side IBD checkpoints: blocks a syncing node refuses to sync past without.**
//!
//! A node with no chain of its own (sink == genesis) weighs the histories peers offer it by what
//! the headers say, and on a PALW network the headers are cheap to say: an unbonded attempt header
//! costs one signature and carries the full attempt blue work (see `rcore/hf-pptake2`). So a peer
//! can hand a fresh node a heavier chain of free attempts and the node adopts it. An established
//! node is defended by its own history (the dormant IBD-commit fence); a fresh node has none.
//!
//! This module is that history, supplied from outside: `(DAA score, block hash)` pairs of the live
//! chain, built in per network and extendable with `--checkpoint=<daa>:<hash>` (repeatable). During
//! IBD a pruning-point proof or a synced header chain that COVERS a checkpoint's DAA score — its
//! selected chain runs from below that score to at or above it — must pass through exactly that
//! block at exactly that score, or the peer is refused as misbehaving. A checkpoint above what a
//! proof or chain reaches is ignored by that proof or chain (the header sync that follows checks
//! it), and one below the window a node can still judge (its own pruning point) is history it has
//! already committed to.
//!
//! **Node policy only.** Nothing here is read by consensus: no block verdict, no params id, no
//! fork id moves. A node that already holds the checkpointed chain is unaffected — every chain it
//! is offered that covers the checkpoint passes through it.
//!
//! Separate from [`super::trusted_checkpoint`], which is ONE operator-vouched ancestor judged at
//! the staging commit (and carries a params id). These are many, checked as the data arrives, and
//! a violation is the peer's fault.

use std::{fmt, str::FromStr, sync::Arc};

use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

use crate::header::Header;

/// A block of the live chain: "the selected chain at `daa_score` is `block_hash`".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IbdCheckpoint {
    pub daa_score: u64,
    pub block_hash: Hash64,
}

/// The marker an operator replaces, the same one the t12 deploy kit uses in `fleet.env`
/// (`contrib/t12-deploy-kit/lib.sh require_release`). An entry still reading this is skipped and
/// reported by [`builtin_ibd_checkpoints_unfilled`], never parsed as a checkpoint.
pub const IBD_CHECKPOINT_PLACEHOLDER: &str = "__FILL_ME__";

/// **OPERATOR FILL — testnet-12 (genesis a27f8f44…).** `"<daa-score>:<128-hex block hash>"` pairs
/// of the LIVE public chain, read from a synced fleet node (`getBlock` / the explorer) at the
/// release build. Replace the `__FILL_ME__` entry; add as many lines as wanted, any order.
///
/// Keep every entry above the network's pruning point minus the proof window: a checkpoint the
/// honest pruning point has passed by more than the pruning proof's level-0 window can no longer
/// be shown by an honest proof, and a fresh node would refuse honest proofs over it. On t12 the
/// honest pruning point stays at genesis until DAA ~75k, so today every entry is checkable.
///
/// Only the public genesis gets these ([`builtin_ibd_checkpoints`] keys on the genesis hash), so a
/// drill chain (`--palw-drill-genesis-salt`) or any other network is untouched.
pub const PALW_T12_IBD_CHECKPOINTS: &[&str] = &[IBD_CHECKPOINT_PLACEHOLDER];

/// The built-in table: genesis hash → that chain's checkpoints.
fn builtin_table(genesis_hash: Hash64) -> &'static [&'static str] {
    if genesis_hash == super::genesis::PALW_T12_GENESIS.hash { PALW_T12_IBD_CHECKPOINTS } else { &[] }
}

/// The built-in checkpoints for the chain whose genesis is `genesis_hash` (placeholders skipped).
///
/// # Panics
/// On a malformed non-placeholder entry — a checkpoint list the operator believes is protecting
/// the node must not silently shrink. `builtin_entries_parse` keeps the shipped table honest.
pub fn builtin_ibd_checkpoints(genesis_hash: Hash64) -> Vec<IbdCheckpoint> {
    builtin_table(genesis_hash)
        .iter()
        .filter(|raw| **raw != IBD_CHECKPOINT_PLACEHOLDER)
        .map(|raw| raw.parse().unwrap_or_else(|e| panic!("built-in IBD checkpoint {raw:?} is invalid: {e}")))
        .collect()
}

/// How many built-in entries for this genesis are still the placeholder (for a startup warning).
pub fn builtin_ibd_checkpoints_unfilled(genesis_hash: Hash64) -> usize {
    builtin_table(genesis_hash).iter().filter(|raw| **raw == IBD_CHECKPOINT_PLACEHOLDER).count()
}

#[derive(Debug, PartialEq, Eq)]
pub enum IbdCheckpointParseError {
    /// Not exactly `<daa>:<hash>`.
    WrongShape(usize),
    InvalidDaaScore(String),
    InvalidBlockHash(String),
}

impl fmt::Display for IbdCheckpointParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongShape(n) => write!(f, "expected <daa-score>:<block-hash> (2 colon-separated parts), got {n}"),
            Self::InvalidDaaScore(s) => write!(f, "DAA score {s:?} is not a number"),
            Self::InvalidBlockHash(s) => write!(f, "block hash {s:?} is not a valid 64-byte (128-hex) hash"),
        }
    }
}

impl std::error::Error for IbdCheckpointParseError {}

impl FromStr for IbdCheckpoint {
    type Err = IbdCheckpointParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.trim().split(':').collect();
        if parts.len() != 2 {
            return Err(IbdCheckpointParseError::WrongShape(parts.len()));
        }
        let daa_score = parts[0].parse::<u64>().map_err(|_| IbdCheckpointParseError::InvalidDaaScore(parts[0].to_owned()))?;
        let block_hash = Hash64::from_str(parts[1]).map_err(|_| IbdCheckpointParseError::InvalidBlockHash(parts[1].to_owned()))?;
        Ok(Self { daa_score, block_hash })
    }
}

impl fmt::Display for IbdCheckpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.daa_score, self.block_hash)
    }
}

/// A chain that covers `checkpoint.daa_score` without passing through `checkpoint.block_hash` there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IbdCheckpointViolation {
    pub checkpoint: IbdCheckpoint,
    /// The DAA score of the top of the offending chain (the proof's pruning point, or the synced tip).
    pub top_daa_score: u64,
}

impl fmt::Display for IbdCheckpointViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "its selected chain reaches DAA {} but does not pass through checkpoint block {} at DAA {}",
            self.top_daa_score, self.checkpoint.block_hash, self.checkpoint.daa_score
        )
    }
}

/// **The rule.** A chain whose top is at `top_daa_score`, judgeable down to `floor_daa_score`,
/// covers every checkpoint with `floor ≤ daa ≤ top`; for each of those `on_chain(cp)` must hold
/// (the block is on the chain's selected chain AND sits at `cp.daa_score`). Checkpoints above the
/// top are beyond this chain and ignored; checkpoints below the floor are outside what it can show.
pub fn judge_chain_against_checkpoints(
    checkpoints: &[IbdCheckpoint],
    floor_daa_score: u64,
    top_daa_score: u64,
    mut on_chain: impl FnMut(&IbdCheckpoint) -> bool,
) -> Result<(), IbdCheckpointViolation> {
    for cp in checkpoints {
        if cp.daa_score < floor_daa_score || cp.daa_score > top_daa_score {
            continue;
        }
        if !on_chain(cp) {
            return Err(IbdCheckpointViolation { checkpoint: *cp, top_daa_score });
        }
    }
    Ok(())
}

/// The checkpoints a chain with this floor and top must contain (for callers that gather facts
/// asynchronously before judging).
pub fn covered_checkpoints(checkpoints: &[IbdCheckpoint], floor_daa_score: u64, top_daa_score: u64) -> Vec<IbdCheckpoint> {
    checkpoints.iter().copied().filter(|cp| cp.daa_score >= floor_daa_score && cp.daa_score <= top_daa_score).collect()
}

/// **The proof pre-check, before anything is downloaded or applied.** A pruning-point proof claims
/// the whole history from genesis to its pruning point, so it covers every checkpoint at or below
/// the pruning point's DAA score, and must at least CONTAIN each of those blocks (at that score).
/// Whether a contained block is on the selected chain is checked once the proof is applied, where
/// reachability exists.
///
/// A proof whose pruning point is below a checkpoint is not judged on it here: that checkpoint is
/// beyond the proof, and the header sync that follows must pass through it.
pub fn judge_proof_against_checkpoints(
    checkpoints: &[IbdCheckpoint],
    proof: &[Vec<Arc<Header>>],
) -> Result<(), IbdCheckpointViolation> {
    let Some(pruning_point) = proof.first().and_then(|level| level.last()) else {
        return Ok(()); // an empty proof is refused by validation, not here
    };
    judge_chain_against_checkpoints(checkpoints, 0, pruning_point.daa_score, |cp| {
        proof.iter().flatten().any(|h| h.hash == cp.block_hash && h.daa_score == cp.daa_score)
    })
}

/// **The reach rule for a node that has not yet reached a checkpoint.** A fresh node must not be
/// moved onto a chain that stops short of a checkpoint it has not yet passed: a free-attempt chain
/// can simply end below the checkpoint's DAA score, cover nothing, and still outweigh the honest
/// chain. Returns the lowest checkpoint above `local_daa_score` that a chain topping out at
/// `top_daa_score` fails to reach. A node already past every checkpoint is never refused here.
pub fn unreached_checkpoint(checkpoints: &[IbdCheckpoint], local_daa_score: u64, top_daa_score: u64) -> Option<IbdCheckpoint> {
    checkpoints
        .iter()
        .copied()
        .filter(|cp| cp.daa_score > local_daa_score && cp.daa_score > top_daa_score)
        .min_by_key(|cp| cp.daa_score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::Header;
    use kaspa_hashes::Hash64;

    fn h(n: u8) -> Hash64 {
        let mut b = [0u8; 64];
        b[0] = n;
        b[63] = n;
        Hash64::from_bytes(b)
    }

    /// A header at `daa` whose hash is forced to `hash` (the proof check reads only hash and score).
    fn header(hash: Hash64, daa: u64) -> Arc<Header> {
        let mut hd = Header::from_precomputed_hash(hash, vec![]);
        hd.daa_score = daa;
        Arc::new(hd)
    }

    /// A linear selected chain `(daa, hash)` from genesis to the tip.
    fn chain(prefix: u8, len: u64) -> Vec<(u64, Hash64)> {
        (0..len).map(|i| (i, if i == 0 { h(0) } else { h(prefix.wrapping_add(i as u8)) })).collect()
    }

    fn on(chain: &[(u64, Hash64)]) -> impl FnMut(&IbdCheckpoint) -> bool + '_ {
        |cp| chain.iter().any(|(d, x)| *d == cp.daa_score && *x == cp.block_hash)
    }

    #[test]
    fn round_trips_and_says_what_is_wrong() {
        let s = format!("300:{}", h(7));
        let cp: IbdCheckpoint = s.parse().unwrap();
        assert_eq!(cp, IbdCheckpoint { daa_score: 300, block_hash: h(7) });
        assert_eq!(cp.to_string(), s);
        assert_eq!("1".parse::<IbdCheckpoint>(), Err(IbdCheckpointParseError::WrongShape(1)));
        assert!(matches!(format!("x:{}", h(7)).parse::<IbdCheckpoint>(), Err(IbdCheckpointParseError::InvalidDaaScore(_))));
        assert!(matches!("1:deadbeef".parse::<IbdCheckpoint>(), Err(IbdCheckpointParseError::InvalidBlockHash(_))));
        // A --trusted-checkpoint string (daa:hash:params-id) is not an IBD checkpoint.
        assert!(format!("1:{}:{}", h(7), "00".repeat(32)).parse::<IbdCheckpoint>().is_err());
    }

    #[test]
    fn builtin_entries_parse_and_only_the_public_t12_genesis_has_any() {
        // Every shipped entry that is not the placeholder parses (builtin_ibd_checkpoints panics otherwise).
        let t12 = super::super::genesis::PALW_T12_GENESIS.hash;
        let parsed = builtin_ibd_checkpoints(t12);
        assert_eq!(parsed.len() + builtin_ibd_checkpoints_unfilled(t12), PALW_T12_IBD_CHECKPOINTS.len());
        // Any other genesis (mainnet, testnet-11, a drill's salted genesis) gets none.
        assert!(builtin_ibd_checkpoints(h(1)).is_empty());
        assert_eq!(builtin_ibd_checkpoints_unfilled(h(1)), 0);
    }

    #[test]
    fn the_honest_chain_passes() {
        let honest = chain(10, 500);
        let cps =
            [IbdCheckpoint { daa_score: 300, block_hash: honest[300].1 }, IbdCheckpoint { daa_score: 450, block_hash: honest[450].1 }];
        assert_eq!(judge_chain_against_checkpoints(&cps, 0, 499, on(&honest)), Ok(()));
    }

    #[test]
    fn a_fake_chain_that_skips_the_checkpoint_is_refused() {
        let honest = chain(10, 500);
        let cp = IbdCheckpoint { daa_score: 300, block_hash: honest[300].1 };
        // Same genesis, a different block at every height: covers DAA 300 with the wrong block.
        let fake = chain(100, 700);
        assert_eq!(
            judge_chain_against_checkpoints(&[cp], 0, 699, on(&fake)),
            Err(IbdCheckpointViolation { checkpoint: cp, top_daa_score: 699 })
        );
        // A fake that jumps over DAA 300 (no block there at all) is refused the same way.
        let skipping: Vec<_> = chain(100, 700).into_iter().filter(|(d, _)| *d != 300).collect();
        assert!(judge_chain_against_checkpoints(&[cp], 0, 699, on(&skipping)).is_err());
        // Holding the checkpoint's hash at another score is not passing through it.
        let misplaced: Vec<_> = fake.iter().map(|(d, x)| if *d == 301 { (*d, cp.block_hash) } else { (*d, *x) }).collect();
        assert!(judge_chain_against_checkpoints(&[cp], 0, 699, on(&misplaced)).is_err());
    }

    #[test]
    fn a_checkpoint_beyond_the_chain_or_below_its_floor_is_ignored() {
        let fake = chain(100, 200);
        let above = IbdCheckpoint { daa_score: 300, block_hash: h(9) };
        assert_eq!(judge_chain_against_checkpoints(&[above], 0, 199, on(&fake)), Ok(()), "beyond the chain's top");
        let below = IbdCheckpoint { daa_score: 50, block_hash: h(9) };
        assert_eq!(judge_chain_against_checkpoints(&[below], 60, 199, on(&fake)), Ok(()), "below the judgeable floor");
        assert!(judge_chain_against_checkpoints(&[below], 50, 199, on(&fake)).is_err(), "at the floor it is judged");
    }

    #[test]
    fn a_fake_proof_that_skips_the_checkpoint_is_refused_and_the_honest_one_passes() {
        let cp = IbdCheckpoint { daa_score: 300, block_hash: h(33) };
        // Honest proof: level 0 ends at the pruning point (DAA 900) and holds the checkpoint block.
        let honest: Vec<Vec<Arc<Header>>> =
            vec![vec![header(h(31), 290), header(cp.block_hash, 300), header(h(90), 900)], vec![header(h(0), 0), header(h(90), 900)]];
        assert_eq!(judge_proof_against_checkpoints(&[cp], &honest), Ok(()));
        // Fake proof over the same range without the checkpoint block.
        let fake: Vec<Vec<Arc<Header>>> = vec![vec![header(h(131), 290), header(h(132), 300), header(h(190), 900)]];
        assert_eq!(judge_proof_against_checkpoints(&[cp], &fake), Err(IbdCheckpointViolation { checkpoint: cp, top_daa_score: 900 }));
        // The right hash at the wrong score does not count.
        let wrong_score: Vec<Vec<Arc<Header>>> = vec![vec![header(cp.block_hash, 301), header(h(190), 900)]];
        assert!(judge_proof_against_checkpoints(&[cp], &wrong_score).is_err());
    }

    #[test]
    fn a_checkpoint_beyond_the_proofs_pruning_point_is_ignored() {
        let cp = IbdCheckpoint { daa_score: 1_000, block_hash: h(33) };
        let proof: Vec<Vec<Arc<Header>>> = vec![vec![header(h(131), 290), header(h(190), 900)]];
        assert_eq!(judge_proof_against_checkpoints(&[cp], &proof), Ok(()));
        assert_eq!(judge_proof_against_checkpoints(&[cp], &[]), Ok(()));
    }

    #[test]
    fn a_node_short_of_a_checkpoint_is_not_moved_onto_a_chain_that_stops_short_of_it() {
        let cps = [IbdCheckpoint { daa_score: 300, block_hash: h(3) }, IbdCheckpoint { daa_score: 480, block_hash: h(4) }];
        // Fresh node (DAA 0): a chain topping out at 470 has not reached the 480 checkpoint.
        assert_eq!(unreached_checkpoint(&cps, 0, 470), Some(cps[1]));
        assert_eq!(unreached_checkpoint(&cps, 0, 250), Some(cps[0]), "the lowest unreached one is named");
        assert_eq!(unreached_checkpoint(&cps, 0, 600), None);
        // A node already past every checkpoint is never refused on reach.
        assert_eq!(unreached_checkpoint(&cps, 500, 470), None);
        assert_eq!(unreached_checkpoint(&[], 0, 0), None);
    }
}
