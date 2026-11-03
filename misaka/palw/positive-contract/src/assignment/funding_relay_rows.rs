//! RFC-0046 retained relay DATA. A complete fixed package survives historical
//! raw pruning; it still requires actual native lineage, fee and custody replay.
//! These values create no UTXO, binding capability, service receipt or payment.
use super::{funding_relay::*, funding_relay_policy::*};
use crate::{
    ContractError,
    identity::{Digest, PUBKEY_BYTES, SIGNATURE_BYTES, hash, principal},
    rows::{AcceptedMaterial, AcceptedOrigin, NativeOutpoint},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const FUNDING_RELAY_CHARGE_BYTES_V2: usize = 4 * size_of::<u64>();

/// Observed transaction fee partition. Construction checks arithmetic only;
/// the UTXO fee owner must supply the amounts that reward calculation used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FundingRelayCarrierFeesV2 {
    gross_fee: u64,
    mandatory_burn: u64,
    before_relay: u64,
    credited_tip: u64,
}
impl FundingRelayCarrierFeesV2 {
    pub fn new(gross_fee: u64, mandatory_burn: u64, before_relay: u64, credited_tip: u64) -> Result<Self, ContractError> {
        if gross_fee.checked_sub(mandatory_burn).is_none_or(|remaining| before_relay > remaining) || credited_tip > before_relay {
            return Err(ContractError::Budget);
        }
        Ok(Self { gross_fee, mandatory_burn, before_relay, credited_tip })
    }

    /// Canonical preimage archived under the accepted charge digest.
    pub fn archive_bytes(self) -> [u8; FUNDING_RELAY_CHARGE_BYTES_V2] {
        let mut raw = [0; FUNDING_RELAY_CHARGE_BYTES_V2];
        borsh::to_writer(raw.as_mut_slice(), &self).expect("four fixed-width carrier fee fields");
        raw
    }

    pub fn from_archive_bytes(raw: &[u8]) -> Result<Self, ContractError> {
        let (gross, mandatory, before_relay, credited): (u64, u64, u64, u64) =
            borsh::from_slice(raw).map_err(|_| ContractError::NonCanonical)?;
        Self::new(gross, mandatory, before_relay, credited)
    }

    pub fn gross_fee(self) -> u64 {
        self.gross_fee
    }

    pub fn before_relay(self) -> u64 {
        self.before_relay
    }

    pub fn credited_tip(self) -> u64 {
        self.credited_tip
    }

    pub fn relay_charge(self) -> u64 {
        self.before_relay - self.credited_tip
    }

    /// Recover the input to a fee preview without changing the observed
    /// partition or the earlier native burns it already accounted for.
    pub fn before_registration(self) -> Self {
        Self { credited_tip: self.before_relay, ..self }
    }

    /// Compute a fresh partition from an unallocated native fee. The result
    /// does not record an actual reward or debit.
    pub fn plan_registration_charge(self, charge: u64) -> Result<Self, ContractError> {
        if charge == 0 || self.relay_charge() != 0 {
            return Err(ContractError::Budget);
        }
        let credited_tip = self.before_relay.checked_sub(charge).ok_or(ContractError::Budget)?;
        Ok(Self { credited_tip, ..self })
    }
}

/// One purpose signature, distinct from the normal transaction signature in
/// the other row. All references must be resolved by native restore/admission.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayAuthorizationRowV2 {
    pub terms: FundingRelayTermsV2,
    pub payer_key: [u8; PUBKEY_BYTES],
    pub purpose_signature: [u8; SIGNATURE_BYTES],
    pub origin: AcceptedOrigin,
    pub material: AcceptedMaterial,
    pub metadata_quote: AcceptedMaterial,
    pub metadata_charge: AcceptedMaterial,
    /// Complete ORIGINAL preimages and quote, retained even after raw pruning.
    /// Native must recompute the usage from actual construction/staging.
    pub policy: FundingRelayPolicyV2,
    pub fees: FundingRelayFeeScheduleV2,
    pub physical: FundingRelayPhysicalProfileV2,
    pub quote: FundingRelayMoneyQuoteV2,
}
impl FundingRelayAuthorizationRowV2 {
    /// Assemble the pending row pair for the canonical writer. The caller
    /// supplies actual acceptance, eligibility and fee-allocation evidence.
    pub fn registration_rows(
        body: &FundingRelayRegistrationBodyV2,
        payer_key: [u8; PUBKEY_BYTES],
        origin: AcceptedOrigin,
        policy: FundingRelayPolicyV2,
        fees: FundingRelayFeeScheduleV2,
        physical: FundingRelayPhysicalProfileV2,
        observed: FundingRelayCarrierFeesV2,
    ) -> Result<(Self, FundingRelayCustodyRowV2), ContractError> {
        if origin.event_ordinal != 0 {
            return Err(ContractError::NonCanonical);
        }
        let terms = body.terms;
        let quote = fees.registration_quote(&policy, &physical, &terms, origin.accepted_daa)?;
        let material = AcceptedMaterial { origin, digest: body.material_id() };
        let metadata_quote = AcceptedMaterial { origin, digest: quote.id() };
        let metadata_charge = Self::registration_charge(material, metadata_quote, quote.fee, observed)?;
        let auth = Self {
            terms,
            payer_key,
            purpose_signature: body.purpose_signature,
            origin,
            material,
            metadata_quote,
            metadata_charge,
            policy,
            fees,
            physical,
            quote,
        };
        let retention_until_daa = terms.custody_until_daa;
        let custody = FundingRelayCustodyRowV2 {
            funding_outpoint: terms.funding_outpoint,
            purpose: terms.funding_purpose,
            terms: terms.id(),
            relay_signature: body.relay_signature,
            material,
            reservation: auth.pending_reservation_material(retention_until_daa),
            retention_until_daa,
            binding: None,
        };
        custody.check_pair(&auth)?;
        Ok((auth, custody))
    }

    /// Rebuild the charge reference from the actual fee partition. The
    /// material binds this charge to one accepting origin and one literal
    /// package; the original quote fixes the full registration debit.
    pub fn carrier_charge_material(&self, carrier_fees: FundingRelayCarrierFeesV2) -> Result<AcceptedMaterial, ContractError> {
        Self::registration_charge(self.material, self.metadata_quote, self.quote.fee, carrier_fees)
    }

    fn registration_charge(
        material: AcceptedMaterial,
        quote: AcceptedMaterial,
        fee: u64,
        carrier_fees: FundingRelayCarrierFeesV2,
    ) -> Result<AcceptedMaterial, ContractError> {
        if fee == 0 || carrier_fees.relay_charge() != fee {
            return Err(ContractError::Budget);
        }
        Ok(AcceptedMaterial {
            origin: material.origin,
            digest: hash(b"misaka/positive/funding-relay-charge/v2/r1", &(material, quote, carrier_fees)),
        })
    }

    /// Identity of the initial rooted slot and its fixed reservation. A later
    /// account binding has an independent reservation reference.
    pub fn pending_reservation_material(&self, retention_until_daa: u64) -> AcceptedMaterial {
        AcceptedMaterial {
            origin: self.origin,
            digest: hash(
                b"misaka/positive/funding-relay-reservation/v2/r1",
                &(self.material, self.metadata_quote, self.metadata_charge, retention_until_daa),
            ),
        }
    }

    pub fn validate_shape(&self) -> Result<(), ContractError> {
        self.terms.validate_shape()?;
        let quote = self.fees.quote(&self.policy, &self.physical, &self.terms, self.quote.usage, self.origin.accepted_daa)?;
        if quote != self.quote || self.metadata_quote.digest != quote.id() {
            return Err(ContractError::Identity);
        }
        if principal(&self.payer_key) != self.terms.payer
            || self.origin.accepted_daa >= self.terms.register_before_daa
            || self.material.origin != self.origin
            || self.metadata_quote.origin != self.origin
            || self.metadata_charge.origin != self.origin
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}

/// Only actual F/S acceptance may write this value. Its shape and signatures
/// alone do not establish that the named native carrier was ever accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayBindingV2 {
    pub origin: AcceptedOrigin,
    pub original_funding: NativeOutpoint,
    pub predecessor: Option<NativeOutpoint>,
    pub original_contract: Digest,
    /// Actual account current-slot reservation, separate from pending R custody.
    pub reservation: AcceptedMaterial,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayCustodyRowV2 {
    pub funding_outpoint: NativeOutpoint,
    pub purpose: RelayFundingPurposeV2,
    pub terms: Digest,
    pub relay_signature: [u8; SIGNATURE_BYTES],
    pub material: AcceptedMaterial,
    pub reservation: AcceptedMaterial,
    pub retention_until_daa: u64,
    pub binding: Option<FundingRelayBindingV2>,
}
impl FundingRelayCustodyRowV2 {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.retention_until_daa == 0 || self.retention_until_daa == u64::MAX {
            return Err(ContractError::Range);
        }
        if self.reservation.origin != self.material.origin {
            return Err(ContractError::Identity);
        }
        if let Some(binding) = self.binding
            && (binding.origin.carrier_transaction != self.funding_outpoint.transaction
                || binding.origin.accepting_block == self.material.origin.accepting_block
                || binding.origin.accepted_daa < self.material.origin.accepted_daa
                || binding.reservation.origin != binding.origin)
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }

    /// Crosslinks and exact literal material only. In particular this does not
    /// authenticate a stored AcceptedOrigin, native custody or a future output.
    pub fn check_pair(&self, auth: &FundingRelayAuthorizationRowV2) -> Result<(), ContractError> {
        self.validate_shape()?;
        auth.validate_shape()?;
        if self.funding_outpoint != auth.terms.funding_outpoint
            || self.purpose != auth.terms.funding_purpose
            || self.terms != auth.terms.id()
            || self.material != auth.material
            || self.retention_until_daa < auth.terms.custody_until_daa
            || self.reservation != auth.pending_reservation_material(self.retention_until_daa)
        {
            return Err(ContractError::Identity);
        }
        let literal = FundingRelayRegistrationBodyV2 {
            version: 2,
            revision: 1,
            operation: FUNDING_RELAY_OPERATION_V2,
            terms: auth.terms,
            purpose_signature: auth.purpose_signature,
            relay_signature: self.relay_signature,
        };
        if literal.material_id() != self.material.digest {
            return Err(ContractError::Identity);
        }
        if let Some(binding) = self.binding {
            if binding.origin.accepted_daa >= auth.terms.funding_admission_before_daa {
                return Err(ContractError::Range);
            }
            match auth.terms.target {
                RelayTargetV2::Continuation { original_funding, predecessor, original_contract, .. }
                    if binding.original_funding == original_funding
                        && binding.predecessor == Some(predecessor)
                        && binding.original_contract == original_contract => {}
                RelayTargetV2::InitialAssignment { bundle, .. }
                    if binding.original_funding == self.funding_outpoint
                        && binding.predecessor.is_none()
                        && binding.original_contract == bundle => {}
                RelayTargetV2::InitialCase { .. } | RelayTargetV2::InitialProver { .. }
                    if binding.original_funding == self.funding_outpoint && binding.predecessor.is_none() => {}
                _ => return Err(ContractError::Identity),
            }
        }
        Ok(())
    }
}
