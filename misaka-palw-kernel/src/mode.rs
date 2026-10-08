//! **RFC-0015 §4.1: the verification mode is part of the class identity.**
//!
//! A class is registered under exactly one [`VerificationModeV1`]. The mode is bound into the class binding id, so the same
//! program, plan and artifact registered under another mode is **another class**: its jobs, claims, evidence headers (which carry
//! the class id) and canonical work ids are disjoint from the other mode's. A producer cannot choose a lighter mode for a claim of
//! a class registered under the stricter one, and a claim of one mode can never be re-labelled as the other's.
//!
//! * [`VerificationModeV1::PanelLicensed`] — the legacy route (a fixed Panel's coverage starts the pass). Its class ids are the
//!   historical ones, **unchanged**: no existing class is reinterpreted.
//! * [`VerificationModeV1::OptimisticPublicVerification`] — RFC-0015's Panel=0 route: no Panel tally, a fixed public challenge
//!   window, objective fraud proofs and DA/default, and a producer reservation sized to the claim's maximum gain.
//!
//! Dormant: nothing registers an OPV class unless the ledger's [`crate::opv::OpvPolicyV1`] is present and its activation height
//! (derived by the consumer from the `palw_panel_free_v1` fence) has been reached.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{Digest, object_id};

/// The domain of a non-legacy mode's class id: `H(domain; (legacy binding id, mode))`.
pub const CLASS_MODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/class-mode/v1";

/// How a class's claims are verified and finalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum VerificationModeV1 {
    /// The fixed Panel's coverage passes a claim (the route as it was before RFC-0015).
    PanelLicensed = 0,
    /// RFC-0015 `OptimisticPublicVerification`: a fixed public challenge window, no Panel.
    OptimisticPublicVerification = 1,
}

impl VerificationModeV1 {
    /// A stable name for receipts, RPC and errors.
    pub const fn name(self) -> &'static str {
        match self {
            Self::PanelLicensed => "PanelLicensed",
            Self::OptimisticPublicVerification => "OptimisticPublicVerification",
        }
    }

    pub const fn is_optimistic(self) -> bool {
        matches!(self, Self::OptimisticPublicVerification)
    }
}

/// **The class id of `legacy_binding_id` under `mode`.** `PanelLicensed` is the legacy id itself (history is not reinterpreted); any
/// other mode is a domain-separated hash of the legacy id and the mode, so it can never collide with a legacy id.
pub fn class_id_for_mode_v1(legacy_binding_id: &Digest, mode: VerificationModeV1) -> Digest {
    match mode {
        VerificationModeV1::PanelLicensed => *legacy_binding_id,
        other => object_id(CLASS_MODE_DOMAIN_V1, &(*legacy_binding_id, other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legacy_mode_keeps_its_historical_id_and_another_mode_is_another_class() {
        let base = [7u8; 64];
        assert_eq!(class_id_for_mode_v1(&base, VerificationModeV1::PanelLicensed), base);
        let opv = class_id_for_mode_v1(&base, VerificationModeV1::OptimisticPublicVerification);
        assert_ne!(opv, base, "the same program under another mode is another class id");
        assert_eq!(opv, class_id_for_mode_v1(&base, VerificationModeV1::OptimisticPublicVerification), "deterministic");
        assert_ne!(opv, class_id_for_mode_v1(&[8u8; 64], VerificationModeV1::OptimisticPublicVerification));
    }

    #[test]
    fn the_mode_wire_discriminants_are_declared_and_an_unknown_one_is_never_success() {
        assert_eq!(borsh::to_vec(&VerificationModeV1::PanelLicensed).unwrap(), [0]);
        assert_eq!(borsh::to_vec(&VerificationModeV1::OptimisticPublicVerification).unwrap(), [1]);
        assert!(borsh::from_slice::<VerificationModeV1>(&[2]).is_err());
        assert!(borsh::from_slice::<VerificationModeV1>(&[255]).is_err());
    }
}
