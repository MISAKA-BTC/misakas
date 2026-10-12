//! **LEGACY — the selected-chain block-hash beacon (RFC-0011 §15.3 items 1, 2 and 5). Not for new routes.**
//!
//! RFC-0007 Part VI replaced block-hash entropy with the PALW Work Beacon of the single challenge contract
//! (`misaka-palw-challenge`): a claim's check randomness is `challenge_seed_v1(ctx, claim_challenge_subject(claim), work_beacon)`,
//! never `H(block hashes)` (block hashes are producer-influenced and rewrappable). The kernel ledger carries no beacon in its claim
//! rows any more, outsiders check with their own salt, and nothing in the route reads this module. It stays only because the
//! reference harness's rebind/grinding tests (`k2_adversarial`) still exercise its arithmetic; a new route must not use it.
//!
//! The original description follows.
//!
//! 1. A claim's commitments are carried first; its **anchor** is the first selected-chain block at DAA `≥ inclusion + delay`
//!    (`delay ≥ 1`), so nothing the beacon is made from exists when the commitments are bound.
//! 2. The **beacon** is a domain-separated hash of the policy, the claim's inclusion DAA and `span` consecutive selected-chain
//!    blocks from the anchor — never one convenient recent block hash. It exists only once the last of them has `finality_daa`
//!    confirmations; before that the state is [`BeaconStateV1::Pending`] and nobody, the producer included, can draw.
//! 5. It is a pure function of chain bytes: IBD, a pruned node and a live node derive the same seed. A reorg that moves the
//!    window before finality is a **rebind**: the old anchor's receipts are stale (they bind the anchor, so admission refuses them),
//!    and the rebind is counted — a producer cannot resample until success. Past `max_rebinds` the claim times out.
//!
//! **Withholding and grinding are bounded, not denied.** A miner that produces `j` of the window's blocks can choose, block by block,
//! to publish or withhold, so it can steer the beacon among at most `2^j` candidates; [`grinding_attempts_v1`] turns that into the
//! `Q` of RFC-0011 §15.4's network bound. Every candidate is still bound to commitments fixed before the window began, so grinding
//! buys draws, not a chosen vector. The construction is hash-based (no algebraic assumption a quantum adversary breaks beyond the
//! hash's own); it is **not** a reviewed unbiased beacon, and an interactive multi-round (GKR) transcript would need one per round.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{Digest, finish, keyed, object_id};

pub const BEACON_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/challenge-beacon/v1";
pub const BEACON_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/challenge-beacon-policy/v1";

/// The network-versioned beacon policy (one for every claim on the route; never chosen by a class or a claim).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconPolicyV1 {
    /// The anchor is the first selected-chain block at DAA `≥ inclusion + delay_daa`.
    pub delay_daa: u64,
    /// Consecutive selected-chain blocks, from the anchor, the beacon hashes.
    pub span: u32,
    /// Confirmations (DAA) the window's last block needs before the beacon exists.
    pub finality_daa: u64,
    /// A claim whose commitments are carried later than this after its job was issued is refused (no late binding to a
    /// beacon window the producer has already seen forming).
    pub inclusion_cutoff_daa: u64,
    /// Rebinds a claim may undergo (a reorg moving its window before finality) before it times out.
    pub max_rebinds: u32,
}

impl BeaconPolicyV1 {
    pub fn id(&self) -> Digest {
        object_id(BEACON_POLICY_DOMAIN_V1, self)
    }

    pub fn well_formed(&self) -> Result<(), &'static str> {
        if self.delay_daa == 0 {
            return Err("the anchor must come strictly after the claim's commitments (delay ≥ 1)");
        }
        if self.span == 0 {
            return Err("the beacon needs at least one block");
        }
        if self.finality_daa == 0 {
            return Err("a beacon with no confirmations can be reorged away after it was drawn");
        }
        Ok(())
    }
}

/// What a node's selected chain says (a header store, an IBD replay, a pruned node's retained headers).
pub trait ChainViewV1 {
    /// Up to `n` selected-chain blocks `(daa, hash)` from the first one at DAA `≥ from`, in order.
    fn selected_from(&self, from: u64, n: usize) -> Vec<(u64, Digest)>;
    /// The selected tip's DAA.
    fn tip_daa(&self) -> u64;
}

/// The window a claim's beacon was made from.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChallengeAnchorV1 {
    pub policy_id: Digest,
    pub inclusion_daa: u64,
    pub anchor_daa: u64,
    pub window: Vec<(u64, Digest)>,
}

impl ChallengeAnchorV1 {
    /// The anchor digest receipts bind (`PalwConstraintReceiptV1::challenge_anchor`).
    pub fn digest(&self) -> Digest {
        object_id(BEACON_DOMAIN_V1, self)
    }

    /// The beacon value the challenge seed is drawn from.
    pub fn beacon(&self) -> Digest {
        let mut s = keyed(BEACON_DOMAIN_V1);
        s.update(&self.policy_id).update(&self.inclusion_daa.to_le_bytes());
        for (daa, h) in &self.window {
            s.update(&daa.to_le_bytes()).update(h);
        }
        finish(s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeaconStateV1 {
    /// Too early: the window is not yet on chain, or its last block is not yet final. No seed exists.
    Pending {
        until_daa: u64,
    },
    Ready {
        anchor: ChallengeAnchorV1,
        beacon: Digest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BeaconRefusalV1 {
    #[error("malformed policy: {0}")]
    Policy(&'static str),
    #[error("the claim was carried at DAA {inclusion}, past the cutoff {cutoff} after its job")]
    LateInclusion { inclusion: u64, cutoff: u64 },
}

/// **The beacon of a claim carried at `inclusion_daa` for a job issued at `job_daa`, as `chain` stands.**
pub fn beacon_v1(
    policy: &BeaconPolicyV1,
    job_daa: u64,
    inclusion_daa: u64,
    chain: &dyn ChainViewV1,
) -> Result<BeaconStateV1, BeaconRefusalV1> {
    policy.well_formed().map_err(BeaconRefusalV1::Policy)?;
    let cutoff = job_daa.saturating_add(policy.inclusion_cutoff_daa);
    if inclusion_daa > cutoff || inclusion_daa < job_daa {
        return Err(BeaconRefusalV1::LateInclusion { inclusion: inclusion_daa, cutoff });
    }
    let from = inclusion_daa.saturating_add(policy.delay_daa);
    let window = chain.selected_from(from, policy.span as usize);
    if window.len() < policy.span as usize {
        return Ok(BeaconStateV1::Pending { until_daa: from.saturating_add(policy.finality_daa) });
    }
    let last = window.last().expect("span ≥ 1").0;
    let final_at = last.saturating_add(policy.finality_daa);
    if chain.tip_daa() < final_at {
        return Ok(BeaconStateV1::Pending { until_daa: final_at });
    }
    let anchor = ChallengeAnchorV1 { policy_id: policy.id(), inclusion_daa, anchor_daa: window[0].0, window };
    let beacon = anchor.beacon();
    Ok(BeaconStateV1::Ready { anchor, beacon })
}

/// What one observation of the chain did to a claim's binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnchorEventV1 {
    Pending {
        until_daa: u64,
    },
    /// First bound.
    Bound {
        anchor: Digest,
        beacon: Digest,
    },
    Unchanged,
    /// A reorg moved the window: receipts under `stale` no longer admit; the claim is checked again under `anchor`.
    Rebound {
        stale: Digest,
        anchor: Digest,
        beacon: Digest,
        rebinds: u32,
    },
    /// More rebinds than the policy allows: the claim times out (never a pass).
    Exhausted {
        rebinds: u32,
    },
}

/// A claim's anchor across observations: rebinds are counted, never free.
#[derive(Clone, Debug)]
pub struct AnchorTrackerV1 {
    pub policy: BeaconPolicyV1,
    pub job_daa: u64,
    pub inclusion_daa: u64,
    bound: Option<ChallengeAnchorV1>,
    rebinds: u32,
}

impl AnchorTrackerV1 {
    pub fn new(policy: BeaconPolicyV1, job_daa: u64, inclusion_daa: u64) -> Self {
        Self { policy, job_daa, inclusion_daa, bound: None, rebinds: 0 }
    }

    pub fn observe(&mut self, chain: &dyn ChainViewV1) -> Result<AnchorEventV1, BeaconRefusalV1> {
        if self.rebinds > self.policy.max_rebinds {
            return Ok(AnchorEventV1::Exhausted { rebinds: self.rebinds });
        }
        let (anchor, beacon) = match beacon_v1(&self.policy, self.job_daa, self.inclusion_daa, chain)? {
            BeaconStateV1::Pending { until_daa } => return Ok(AnchorEventV1::Pending { until_daa }),
            BeaconStateV1::Ready { anchor, beacon } => (anchor, beacon),
        };
        let event = match &self.bound {
            None => AnchorEventV1::Bound { anchor: anchor.digest(), beacon },
            Some(old) if *old == anchor => return Ok(AnchorEventV1::Unchanged),
            Some(old) => {
                self.rebinds += 1;
                if self.rebinds > self.policy.max_rebinds {
                    return Ok(AnchorEventV1::Exhausted { rebinds: self.rebinds });
                }
                AnchorEventV1::Rebound { stale: old.digest(), anchor: anchor.digest(), beacon, rebinds: self.rebinds }
            }
        };
        self.bound = Some(anchor);
        Ok(event)
    }

    pub fn anchor(&self) -> Option<&ChallengeAnchorV1> {
        self.bound.as_ref()
    }

    /// The draws this claim has had (`1 + rebinds`): what RFC-0011 §15.4's `Q` counts for it.
    pub fn attempts(&self) -> u128 {
        1 + self.rebinds as u128
    }
}

/// **The grinding term**: a party producing `adversary_blocks` of a beacon window can choose among at most `2^j` beacons (publish
/// or withhold each), for each of `claims` claims and each of their `attempts` draws. Saturates at `u128::MAX`.
pub fn grinding_attempts_v1(claims: u128, attempts_per_claim: u128, adversary_blocks: u32) -> u128 {
    let candidates = if adversary_blocks >= 127 { u128::MAX } else { 1u128 << adversary_blocks };
    claims.saturating_mul(attempts_per_claim).saturating_mul(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A selected chain: one block per DAA from 0 to `tip`, hash a function of `(daa, fork)`.
    struct Chain {
        tip: u64,
        fork_at: u64,
        fork: u8,
    }

    impl ChainViewV1 for Chain {
        fn selected_from(&self, from: u64, n: usize) -> Vec<(u64, Digest)> {
            (from..=self.tip)
                .take(n)
                .map(|d| {
                    let tag = if d >= self.fork_at { self.fork } else { 0 };
                    (d, crate::hash::id(b"test/block", &[&d.to_le_bytes()[..], &[tag]].concat()))
                })
                .collect()
        }
        fn tip_daa(&self) -> u64 {
            self.tip
        }
    }

    fn policy() -> BeaconPolicyV1 {
        BeaconPolicyV1 { delay_daa: 10, span: 4, finality_daa: 20, inclusion_cutoff_daa: 50, max_rebinds: 2 }
    }

    #[test]
    fn no_seed_exists_until_the_window_is_on_chain_and_final() {
        let p = policy();
        // Included at 100: the window is DAA 110..=113, final at 133.
        let early = Chain { tip: 112, fork_at: u64::MAX, fork: 0 };
        assert_eq!(beacon_v1(&p, 90, 100, &early).unwrap(), BeaconStateV1::Pending { until_daa: 130 });
        let not_final = Chain { tip: 132, fork_at: u64::MAX, fork: 0 };
        assert_eq!(beacon_v1(&p, 90, 100, &not_final).unwrap(), BeaconStateV1::Pending { until_daa: 133 });
        let ready = Chain { tip: 133, fork_at: u64::MAX, fork: 0 };
        let BeaconStateV1::Ready { anchor, beacon } = beacon_v1(&p, 90, 100, &ready).unwrap() else { panic!() };
        assert_eq!(anchor.anchor_daa, 110);
        assert!(anchor.anchor_daa > anchor.inclusion_daa, "the beacon is made only of blocks after the commitments");
        // Replay determinism: another node (or IBD) with the same chain derives the same beacon; a longer tip changes nothing.
        let later = Chain { tip: 10_000, fork_at: u64::MAX, fork: 0 };
        let BeaconStateV1::Ready { beacon: again, .. } = beacon_v1(&p, 90, 100, &later).unwrap() else { panic!() };
        assert_eq!(beacon, again);
    }

    #[test]
    fn late_inclusion_and_malformed_policies_are_refused() {
        let chain = Chain { tip: 1000, fork_at: u64::MAX, fork: 0 };
        assert!(matches!(beacon_v1(&policy(), 0, 51, &chain), Err(BeaconRefusalV1::LateInclusion { .. })));
        let mut p = policy();
        p.delay_daa = 0;
        assert!(matches!(beacon_v1(&p, 0, 10, &chain), Err(BeaconRefusalV1::Policy(_))));
    }

    #[test]
    fn a_reorg_of_the_window_is_a_counted_rebind_and_too_many_time_out() {
        let p = policy();
        let mut t = AnchorTrackerV1::new(p, 90, 100);
        assert!(matches!(t.observe(&Chain { tip: 120, fork_at: u64::MAX, fork: 0 }).unwrap(), AnchorEventV1::Pending { .. }));
        let AnchorEventV1::Bound { anchor: first, .. } = t.observe(&Chain { tip: 140, fork_at: u64::MAX, fork: 0 }).unwrap() else {
            panic!()
        };
        assert_eq!(t.observe(&Chain { tip: 150, fork_at: u64::MAX, fork: 0 }).unwrap(), AnchorEventV1::Unchanged);
        // A deep reorg inside the window (past the assumed finality — the event the policy bounds rather than prevents).
        let AnchorEventV1::Rebound { stale, anchor, rebinds, .. } = t.observe(&Chain { tip: 160, fork_at: 112, fork: 1 }).unwrap()
        else {
            panic!()
        };
        assert_eq!((stale, rebinds), (first, 1));
        assert_ne!(anchor, first);
        assert!(matches!(t.observe(&Chain { tip: 170, fork_at: 111, fork: 2 }).unwrap(), AnchorEventV1::Rebound { rebinds: 2, .. }));
        assert_eq!(t.observe(&Chain { tip: 180, fork_at: 110, fork: 3 }).unwrap(), AnchorEventV1::Exhausted { rebinds: 3 });
        assert_eq!(t.attempts(), 4, "every draw is an attempt in the network bound");
    }

    #[test]
    fn grinding_is_counted_into_the_network_bound() {
        assert_eq!(grinding_attempts_v1(1, 1, 0), 1);
        assert_eq!(grinding_attempts_v1(1 << 20, 3, 4), (1 << 20) * 3 * 16);
        // 2^20 claims × 3 draws × 2^4 grinding against a 2^-160 check and a 2^-64 environment: the network bound is the environment's.
        let q = grinding_attempts_v1(1 << 20, 3, 4);
        assert_eq!(crate::lifecycle::network_false_acceptance_bits_v1(160, q, 64), 63);
        assert_eq!(crate::lifecycle::network_false_acceptance_bits_v1(160, q, 255), 160 - 26 - 1);
    }
}
