//! **RFC-0016 V8 — a verifier's capacity attestation: the wire terms of kernel-route inner kind 76** (2026-10-17; dormant).
//!
//! One bond's signed statement of what its machine can verify per span for one model class — compute, memory, data availability and
//! court load, measured over a closed window, with an expiry and the root of the measurement's raw evidence. The route envelope's
//! signature is the attestation's authentication (the consumer requires `verifier` to be the object's signer). An all-zero resource
//! vector is a revocation of the bond's attestation for that class.
//!
//! The bare ledger refuses the kind: only the authenticated native consumer, past its own dormant fence
//! (`palw_capacity_attestation_v1`), interprets it. Nothing here proves the machine's capability; the figure is an operator's
//! declaration that bounds the capacity it is credited (never above what it declares, and never twice for one machine), and the
//! evidence root names what an independent reviewer re-measures. An attestation earns no PALW reward, block right or Final weight.

use crate::hash::Digest;
use borsh::{BorshDeserialize, BorshSerialize};

pub const CAPACITY_ATTESTATION_VERSION_V1: u16 = 1;
/// Every inner 76 object is bounded by this (version byte and tag included), with headroom over the attestation.
pub const MAX_CAPACITY_ATTESTATION_OBJECT_BYTES_V1: usize = 1024;

/// **Inner 76.** Signed by `verifier`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapacityAttestationV1 {
    pub version: u16,
    pub network: Digest,
    /// The attesting bond's kernel bond id; must be the object's signer.
    pub verifier: Digest,
    /// The model class the capability is attested for.
    pub class_id: Digest,
    /// Replay compute per span, in the economic CCU unit the demand terms use.
    pub compute_ccu_per_span: u128,
    /// Memory available to hold one claim's working set (a fit gate).
    pub memory_bytes: u64,
    /// Data-availability bytes it can fetch and retain per span.
    pub da_bytes_per_span: u64,
    /// Court load (permille of a session) it can carry per span.
    pub court_permille_per_span: u64,
    /// The closed measurement window `[from, to)`, in DAA.
    pub window_from_daa: u64,
    pub window_to_daa: u64,
    /// The DAA from which the attestation counts nothing.
    pub expires_daa: u64,
    /// The measurement evidence's root (never zero, except in a revocation).
    pub evidence_root: Digest,
}

impl CapacityAttestationV1 {
    /// An all-zero resource vector: the bond withdraws its attestation for the class.
    pub fn is_revocation(&self) -> bool {
        self.compute_ccu_per_span == 0 && self.memory_bytes == 0 && self.da_bytes_per_span == 0 && self.court_permille_per_span == 0
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != CAPACITY_ATTESTATION_VERSION_V1 {
            return Err("a capacity attestation of an unknown version");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capacity_attestation_fits_its_bound_and_a_zero_vector_revokes() {
        let a = CapacityAttestationV1 {
            version: CAPACITY_ATTESTATION_VERSION_V1,
            network: [1; 64],
            verifier: [2; 64],
            class_id: [3; 64],
            compute_ccu_per_span: u128::MAX,
            memory_bytes: u64::MAX,
            da_bytes_per_span: u64::MAX,
            court_permille_per_span: u64::MAX,
            window_from_daa: 1,
            window_to_daa: 2,
            expires_daa: 3,
            evidence_root: [4; 64],
        };
        a.validate().unwrap();
        assert!(!a.is_revocation());
        assert!(borsh::to_vec(&a).unwrap().len() + 2 <= MAX_CAPACITY_ATTESTATION_OBJECT_BYTES_V1);
        let revoke =
            CapacityAttestationV1 { compute_ccu_per_span: 0, memory_bytes: 0, da_bytes_per_span: 0, court_permille_per_span: 0, ..a.clone() };
        assert!(revoke.is_revocation());
        assert!(CapacityAttestationV1 { version: 2, ..a }.validate().is_err());
    }
}
