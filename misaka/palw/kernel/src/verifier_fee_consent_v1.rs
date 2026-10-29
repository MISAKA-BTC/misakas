//! ADR-0207: explicit Clean attestation and finite ordinary check-fee consent.
//! Authentication is the existing kernel-route envelope signature, not a computation proof.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};

pub const VERIFIER_FEE_CONSENT_VERSION_V1: u16 = 1;
pub const MAX_VERIFIER_FEE_CONSENT_OBJECT_BYTES_V1: usize = 1024;

/// Each field is covered by the route signature. One consent covers exactly one drawn slot.
/// The native consumer checks these values against the original claim, draw and material covenant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerifierFeeConsentV1 {
    pub version: u16,
    pub network: Digest,
    pub ruleset: Digest,
    pub claim: Digest,
    pub verifier: Digest,
    pub slot: u64,
    pub poster: Digest,
    pub fee: u64,
    pub liability_until: u64,
    pub material_until: u64,
    pub policy_digest: Digest,
    pub nonce: u64,
    /// Must be true: this new object expressly attests Clean. A generic fee event does not.
    pub clean: bool,
}

pub fn verifier_fee_policy_digest_v1() -> Digest {
    object_id(b"misaka-palw/verifier-fee-policy/v1", &(1u16, 1000u16, true, true))
}

impl VerifierFeeConsentV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != VERIFIER_FEE_CONSENT_VERSION_V1 || !self.clean {
            return Err("unknown verifier fee consent version or missing explicit Clean attestation");
        }
        if self.policy_digest != verifier_fee_policy_digest_v1() {
            return Err("verifier fee consent names another policy");
        }
        if self.liability_until == u64::MAX || self.material_until < self.liability_until {
            return Err("verifier fee consent lacks a finite covered liability deadline");
        }
        Ok(())
    }

    /// Pure deadline decision; neither a claim's Final status nor its credit is changed.
    pub fn payee_after_deadline(&self, now: u64, convicted_daa: Option<u64>) -> Option<Digest> {
        if now <= self.liability_until {
            return None;
        }
        Some(if convicted_daa.is_some_and(|at| at <= self.liability_until) { self.poster } else { self.verifier })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn consent() -> VerifierFeeConsentV1 {
        VerifierFeeConsentV1 {
            version: 1,
            network: [1; 64],
            ruleset: [2; 64],
            claim: [3; 64],
            verifier: [4; 64],
            slot: 0,
            poster: [5; 64],
            fee: 17,
            liability_until: 100,
            material_until: 101,
            policy_digest: verifier_fee_policy_digest_v1(),
            nonce: 0,
            clean: true,
        }
    }
    #[test]
    fn deadline_refund_and_late_verdict_are_exact() {
        let c = consent();
        assert!(c.validate().is_ok());
        assert_eq!(c.payee_after_deadline(100, Some(100)), None);
        assert_eq!(c.payee_after_deadline(101, Some(100)), Some(c.poster));
        assert_eq!(c.payee_after_deadline(101, Some(101)), Some(c.verifier));
        assert_eq!(c.payee_after_deadline(101, None), Some(c.verifier));
    }
    #[test]
    fn new_wire_preserves_identity_and_refuses_other_policy() {
        let c = consent();
        let obj = crate::route::KernelRouteObjectV1::ConsentVerifierFeeV1 { terms: c };
        assert_eq!(crate::route::KernelRouteObjectV1::decode(&obj.encode()).unwrap(), obj);
        let mut changed = c;
        changed.policy_digest[0] ^= 1;
        assert!(changed.validate().is_err());
        changed = c;
        changed.clean = false;
        assert!(changed.validate().is_err());
        changed = c;
        changed.liability_until = u64::MAX;
        assert!(changed.validate().is_err());
    }
}
