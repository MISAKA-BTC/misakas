//! **RFC07 R7-ECON / RFC08 R8-WINDOW / R8-ECON signed economic contracts** (2026-10-15; dormant). The wire terms of three
//! consumer-owned kernel-route inner kinds and the one kernel-ledger hook the horizon extension needs:
//!
//! * inner 67 [`ModelVerificationAttestationV1`] — a stake-drawn verifier's attestation of one sampled Final claim of a model-epoch
//!   (the optional verification-attestation gate on `palw_model_bond_allocation_v1`, ECON §5c.4, user ruling 2026-10-10);
//! * inner 68 [`HorizonExtensionTermsV1`] — one affected party's signature on an extension of a live dependency liability horizon
//!   (RFC08 2026-10-15 R8-WINDOW item 3 / item 9 "承諾付きhorizon延長");
//! * inner 69 [`MinimumPriceContractTermsV1`] — one party's signature on a worker/poster minimum-price contract (§15.7, R8-ECON).
//!
//! None of these is a bare-ledger move: the ledger refuses all three and only the authenticated native consumer, past its own dormant
//! fence, interprets them. No value here is an approved economic or security parameter; the consumer's fenced policy bounds them.

use crate::hash::{Digest, object_id};
use crate::ledger::KernelLedgerV1;
use crate::lifecycle::ClaimStateV1;
use borsh::{BorshDeserialize, BorshSerialize};

pub const MODEL_VERIFICATION_ATTESTATION_VERSION_V1: u16 = 1;
pub const HORIZON_EXTENSION_TERMS_VERSION_V1: u16 = 1;
pub const MINIMUM_PRICE_CONTRACT_VERSION_V1: u16 = 1;
/// Every inner 67–69 object is a fixed-size record; this bounds its encoding (version byte and tag included) with headroom.
pub const MAX_ECON_CONTRACT_OBJECT_BYTES_V1: usize = 1024;

/// A drawn verifier's verdict on the sampled claim. A failed check is NOT an attestation: it is a fraud proof (`FileProof`), and the
/// claim is convicted as usual (ECON §5c.4 item 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ModelVerificationVerdictV1 {
    /// Decodes (a zero body is a valid encoding) but never validates.
    Unset = 0,
    /// The verifier checked the sampled claim claim-specifically with its own copy authenticated against the registered root.
    Verified = 1,
    /// An honest slot that could not obtain the model. Not penalized; no producer is penalized either (ADR-0177 D1).
    CouldNotVerify = 2,
}

/// **Inner 67**: the attestation of one drawn slot holder for one model-epoch tally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVerificationAttestationV1 {
    pub version: u16,
    pub network: Digest,
    /// The drawn verifier's kernel bond id; must be the object's signer.
    pub verifier: Digest,
    /// The consumer's tally id for `(model, epoch)`.
    pub tally: Digest,
    pub model: Digest,
    pub epoch: u64,
    /// The beacon-chosen Final claim the tally sampled (the attestation names it, so it cannot count for another subject).
    pub sampled_claim: Digest,
    /// The registered model root the verifier authenticated its own copy against.
    pub model_root: Digest,
    pub verdict: ModelVerificationVerdictV1,
}

impl ModelVerificationAttestationV1 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/MODEL-VERIFICATION-ATTESTATION/V1", self)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != MODEL_VERIFICATION_ATTESTATION_VERSION_V1
            || [self.network, self.verifier, self.tally, self.model, self.sampled_claim, self.model_root].contains(&[0; 64])
            || self.verdict == ModelVerificationVerdictV1::Unset
        {
            return Err("a model verification attestation needs its version, every typed subject and a verdict");
        }
        Ok(())
    }
}

/// **Inner 68**: an extension of a live EXEC-bound kernel claim's liability horizon, signed separately by every affected party
/// (the worker = the claim's producer, and the sponsor = its job's poster). Providers of a transferred claim consent through their
/// own signed leases (tag 150), which must already cover the new horizon; a lease is never rewritten or lengthened by this object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HorizonExtensionTermsV1 {
    pub version: u16,
    pub network: Digest,
    pub claim: Digest,
    /// The EXEC root that holds the claim's reward (its `exec_reward_holds` owner).
    pub root: Digest,
    pub worker: Digest,
    pub sponsor: Digest,
    /// The exact live horizon being extended: a consent over a stale horizon never applies.
    pub current_liability_until_daa: u64,
    pub new_liability_until_daa: u64,
    /// The claim-fixed absolute deadline (X3: fixed at claim acceptance, never moved by an extension).
    pub hard_deadline_daa: u64,
    /// Last inclusive DAA at which the last required signature may land.
    pub consent_by_daa: u64,
    /// The minimum-price contract (inner 69) funding the longer retention; zero when the fenced policy does not require one.
    pub price_contract: Digest,
}

impl HorizonExtensionTermsV1 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/HORIZON-EXTENSION/V1", self)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != HORIZON_EXTENSION_TERMS_VERSION_V1
            || [self.network, self.claim, self.root, self.worker, self.sponsor].contains(&[0; 64])
        {
            return Err("a horizon extension needs its version and every typed party and subject");
        }
        if self.new_liability_until_daa <= self.current_liability_until_daa {
            return Err("a horizon extension must lengthen the horizon (it never shortens a Final or court window)");
        }
        if self.new_liability_until_daa > self.hard_deadline_daa || self.hard_deadline_daa == u64::MAX {
            return Err("a horizon extension never passes the claim-fixed hard deadline");
        }
        if self.consent_by_daa > self.current_liability_until_daa {
            return Err("every consent must land while the horizon being extended is still live");
        }
        Ok(())
    }

    /// The parties whose signatures the extension needs, canonical (sorted, deduplicated).
    pub fn required_parties(&self) -> Vec<Digest> {
        let mut parties = vec![self.worker, self.sponsor];
        parties.sort();
        parties.dedup();
        parties
    }
}

/// **Inner 69**: a worker/poster minimum-price contract (RFC08 §15.7): bound before signature to the funded amount, work scope,
/// worker, policy, deadline and hold/release conditions. Both parties sign the same terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MinimumPriceContractTermsV1 {
    pub version: u16,
    pub network: Digest,
    pub worker: Digest,
    pub poster: Digest,
    /// The work scope: the kernel job whose GAP-5 escrow funds the minimum.
    pub job: Digest,
    pub class: Digest,
    /// The consumer's fenced policy id the contract was priced under.
    pub policy: Digest,
    /// `M_net_min`: the funded minimum actually collectible on the joint native-Final-and-collectible event.
    pub funded_minimum_sompi: u64,
    /// The worker's capital locked for the work (counted once over `[accept_by, retention_until]`).
    pub capital_sompi: u64,
    /// Last inclusive DAA at which the worker's claim may be admitted under the contract.
    pub accept_by_daa: u64,
    /// The retention the worker prices (its capital-time area ends here).
    pub retention_until_daa: u64,
}

impl MinimumPriceContractTermsV1 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/MINIMUM-PRICE-CONTRACT/V1", self)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != MINIMUM_PRICE_CONTRACT_VERSION_V1
            || [self.network, self.worker, self.poster, self.job, self.class, self.policy].contains(&[0; 64])
        {
            return Err("a minimum-price contract needs its version and every typed party, scope and policy");
        }
        if self.funded_minimum_sompi == 0 {
            return Err("a minimum-price contract binds a positive funded minimum");
        }
        if self.retention_until_daa < self.accept_by_daa || self.retention_until_daa == u64::MAX {
            return Err("a minimum-price contract's retention is finite and ends after its acceptance deadline");
        }
        Ok(())
    }

    pub fn required_parties(&self) -> Vec<Digest> {
        let mut parties = vec![self.worker, self.poster];
        parties.sort();
        parties.dedup();
        parties
    }
}

impl KernelLedgerV1 {
    /// **R8-WINDOW: the consented extension of a live claim's liability** (the consumer's call, once every party has signed). It extends
    /// the existing reservation's retention only: no collateral is recreated, no payout is cancelled, no Final or court window is
    /// shortened and the EXEC reward hold's root expiry is unchanged. Refused for paid, failed, convicted or unreserved work, for an
    /// expired horizon (it is never reopened), for a stale `expected_until`, and past `hard_deadline`.
    pub fn extend_claim_liability_by_consent_v1(
        &mut self,
        claim: &Digest,
        expected_until: u64,
        new_until: u64,
        hard_deadline: u64,
    ) -> Result<(), String> {
        let daa = self.daa;
        let row = self.claims.get(claim).ok_or("the extended claim is missing")?;
        if row.rewarded
            || row.convicted
            || row.reserved == 0
            || matches!(
                row.life.state,
                ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. }
            )
        {
            return Err("paid, failed or unreserved work has no live horizon to extend".into());
        }
        if row.liability_until != Some(expected_until) {
            return Err("the consent names a horizon the claim no longer has".into());
        }
        if daa > expected_until {
            return Err("an expired horizon cannot be reopened by consent".into());
        }
        if new_until <= expected_until || new_until > hard_deadline {
            return Err("an extension lengthens the horizon and never passes the claim-fixed hard deadline".into());
        }
        if !self.exec_reward_holds.contains_key(claim) {
            return Err("only an EXEC-bound dependency horizon is extended by consent".into());
        }
        self.claims.get_mut(claim).expect("checked").liability_until = Some(new_until);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> HorizonExtensionTermsV1 {
        HorizonExtensionTermsV1 {
            version: 1,
            network: [1; 64],
            claim: [2; 64],
            root: [3; 64],
            worker: [4; 64],
            sponsor: [5; 64],
            current_liability_until_daa: 100,
            new_liability_until_daa: 150,
            hard_deadline_daa: 200,
            consent_by_daa: 90,
            price_contract: [0; 64],
        }
    }

    #[test]
    fn a_horizon_extension_only_lengthens_inside_the_hard_deadline_and_while_live() {
        terms().validate().unwrap();
        assert_eq!(terms().required_parties(), vec![[4; 64], [5; 64]]);
        let mut t = terms();
        t.new_liability_until_daa = 100;
        assert!(t.validate().is_err(), "never equal or shorter");
        let mut t = terms();
        t.new_liability_until_daa = 201;
        assert!(t.validate().is_err(), "never past the claim-fixed deadline");
        let mut t = terms();
        t.consent_by_daa = 101;
        assert!(t.validate().is_err(), "consent lands while the horizon is live");
        let mut t = terms();
        t.sponsor = t.worker;
        assert_eq!(t.required_parties().len(), 1, "one bond in both roles signs once");
    }

    #[test]
    fn zero_bodies_decode_but_never_validate() {
        let a: ModelVerificationAttestationV1 = borsh::from_slice(&[0u8; 2 + 64 * 6 + 8 + 1]).unwrap();
        assert!(a.validate().is_err());
        let m: MinimumPriceContractTermsV1 = borsh::from_slice(&[0u8; 2 + 64 * 6 + 8 * 4]).unwrap();
        assert!(m.validate().is_err());
    }
}
