//! **N3 — BootstrapExact source contributions** (2026-10-15 round 3; dormant). The wire terms of the consumer-owned kernel-route inner
//! kind 73 ([`BootstrapExactContributionV1`]): one bond's sealed input to, or the reveal of its input to, one BootstrapExact source
//! round of the Work Beacon's independent bootstrap (register N3, REQ-N3-1/2/3; RFC-0007 改定 2026-10-12 §VI.3/§VI.8).
//!
//! A round is `(profile, target, slot)` ([`BootstrapExactRoundV1`]): the entropy profile it is judged under, the kernel claim whose
//! commitment its randomness serves, and which of the target's sources it is. The contribution carries ONLY what a contributor knows
//! — its commitment, later its preimage. Every fact the bootstrap's two judgements compare (the target's commitment position, when
//! each input was fixed, how many independent operators contributed, the exact output, the depth below the sink) is derived by the
//! consumer's fold from chain state; none is asserted here. The bare ledger refuses the kind: only the authenticated native consumer,
//! past its own dormant fence (`palw_beacon_bootstrap_v1`), interprets it. A source-only bootstrap record earns no PALW reward, block
//! right or Final weight.
//!
//! **The V1 combiner is a structural placeholder, not an approved BootstrapExact kernel**: [`bootstrap_exact_output_v1`] hashes the
//! round's revealed inputs in canonical order. Which exact computation production sources run (and its resource bound) is a reviewed
//! profile decision that has not been made; the consumer's approved-profile list is empty.

use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};

pub const BOOTSTRAP_EXACT_CONTRIBUTION_VERSION_V1: u16 = 1;
/// Every inner 73 object is a fixed-size record; this bounds its encoding (version byte and tag included) with headroom.
pub const MAX_BOOTSTRAP_EXACT_OBJECT_BYTES_V1: usize = 512;

/// One BootstrapExact source round: `(profile, target, slot)`. Its windows are derived by the consumer from the target's own commitment
/// position and the fenced policy — never chosen by a contributor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct BootstrapExactRoundV1 {
    /// The bootstrap entropy profile the round is judged under (its id).
    pub profile: Digest,
    /// The kernel claim whose commitment the source's randomness serves.
    pub target: Digest,
    /// Which of the target's sources (`< maximum_sources_per_target`).
    pub slot: u32,
}

impl BootstrapExactRoundV1 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/BOOTSTRAP-EXACT-ROUND/V1", self)
    }
}

/// What a contribution does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BootstrapExactActionV1 {
    /// Seal: `commitment = bootstrap_exact_commitment_v1(round, contributor, preimage)`.
    Seal { commitment: Digest },
    /// Reveal the sealed preimage.
    Reveal { preimage: Digest },
}

/// **Inner 73**: one contributor's seal or reveal in one round. Signed by `contributor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapExactContributionV1 {
    pub version: u16,
    pub network: Digest,
    pub round: BootstrapExactRoundV1,
    /// The contributing bond's kernel id; must be the object's signer.
    pub contributor: Digest,
    pub action: BootstrapExactActionV1,
}

impl BootstrapExactContributionV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != BOOTSTRAP_EXACT_CONTRIBUTION_VERSION_V1
            || [self.network, self.round.profile, self.round.target, self.contributor].contains(&[0; 64])
            || matches!(self.action, BootstrapExactActionV1::Seal { commitment } if commitment == [0; 64])
        {
            return Err("a BootstrapExact contribution needs its version, network, profile, target, contributor and a commitment");
        }
        Ok(())
    }
}

/// The seal of `preimage` by `contributor` in round `round` (binds the round and the contributor: a seal cannot be replayed elsewhere).
pub fn bootstrap_exact_commitment_v1(round: &Digest, contributor: &Digest, preimage: &Digest) -> Digest {
    object_id(b"MISAKA/PALW/BOOTSTRAP-EXACT-SEAL/V1", &(*round, *contributor, *preimage))
}

/// **The V1 combiner (structural placeholder)**: the round's output over its revealed `(contributor, preimage)` inputs in the round's
/// canonical seal order. Every validator re-executes it exactly from the stored reveals.
pub fn bootstrap_exact_output_v1(round: &Digest, revealed: &[(Digest, Digest)]) -> Digest {
    object_id(b"MISAKA/PALW/BOOTSTRAP-EXACT-OUTPUT/V1", &(*round, revealed.to_vec()))
}

/// **The source's work identity — content only**: the round and its canonical commitments. No carrier, header, claim or block id enters
/// it, so a rewrap of the same work never mints a new identity.
pub fn bootstrap_exact_work_id_v1(round: &BootstrapExactRoundV1, commitments: &[Digest]) -> Digest {
    object_id(b"MISAKA/PALW/BOOTSTRAP-EXACT-WORK/V1", &(*round, commitments.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seal() -> BootstrapExactContributionV1 {
        BootstrapExactContributionV1 {
            version: 1,
            network: [1; 64],
            round: BootstrapExactRoundV1 { profile: [2; 64], target: [3; 64], slot: 0 },
            contributor: [4; 64],
            action: BootstrapExactActionV1::Seal { commitment: [5; 64] },
        }
    }

    #[test]
    fn a_contribution_validates_only_with_every_typed_field() {
        seal().validate().unwrap();
        let mut c = seal();
        c.action = BootstrapExactActionV1::Seal { commitment: [0; 64] };
        assert!(c.validate().is_err(), "a zero commitment seals nothing");
        let mut c = seal();
        c.round.target = [0; 64];
        assert!(c.validate().is_err());
        let zero: BootstrapExactContributionV1 = borsh::from_slice(&borsh::to_vec(&BootstrapExactContributionV1 {
            version: 0,
            network: [0; 64],
            round: BootstrapExactRoundV1 { profile: [0; 64], target: [0; 64], slot: 0 },
            contributor: [0; 64],
            action: BootstrapExactActionV1::Reveal { preimage: [0; 64] },
        })
        .unwrap())
        .unwrap();
        assert!(zero.validate().is_err(), "a zero body decodes but never validates");
    }

    #[test]
    fn seals_bind_round_and_contributor_and_the_work_identity_is_content_only() {
        let round = seal().round;
        let a = bootstrap_exact_commitment_v1(&round.id(), &[4; 64], &[9; 64]);
        assert_ne!(a, bootstrap_exact_commitment_v1(&round.id(), &[5; 64], &[9; 64]), "another contributor");
        let other = BootstrapExactRoundV1 { slot: 1, ..round };
        assert_ne!(a, bootstrap_exact_commitment_v1(&other.id(), &[4; 64], &[9; 64]), "another round");
        assert_eq!(bootstrap_exact_work_id_v1(&round, &[a]), bootstrap_exact_work_id_v1(&round, &[a]));
        assert_ne!(bootstrap_exact_output_v1(&round.id(), &[([4; 64], [9; 64])]), bootstrap_exact_output_v1(&round.id(), &[]));
        assert!(borsh::to_vec(&seal()).unwrap().len() + 2 <= MAX_BOOTSTRAP_EXACT_OBJECT_BYTES_V1);
    }
}
