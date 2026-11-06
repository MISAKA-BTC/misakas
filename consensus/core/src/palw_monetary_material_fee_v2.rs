//! Adopted finite fee-only consent. Existing event-relative service and bond remain unchanged.
use crate::palw_monetary_material_v1::*;
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const TABLE: u8 = 191;
pub const VERSION: u16 = 2;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialFeeEnrollmentV2 {
    pub ruleset: Hash64,
    pub enrollment: PalwMonetaryMaterialEnrollmentV1,
    /// Each provider separately signs the fee rule; original service signatures remain intact.
    pub provider_fee_signatures: Vec<Vec<u8>>,
}
impl PalwMaterialFeeEnrollmentV2 {
    pub fn fee_payload(&self) -> Vec<u8> {
        borsh::to_vec(&(
            b"MISAKA/PALW/MATERIAL-FEE-ONLY/V2".as_slice(),
            VERSION,
            self.ruleset,
            self.enrollment.id(),
            self.enrollment.terms.enrollment_until_daa,
            // actual default <= H refunds all; later default never reopens this fee
            true,
            true,
        ))
        .expect("fee payload")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMaterialFeeStatusV2 {
    Held,
    ProviderQueued,
    RefundQueued { queue: Hash64, receipt: Hash64 },
    RefundPaid { daa: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialFeeRowV2 {
    pub ruleset: Hash64,
    pub version: u16,
    pub enrollment: Hash64,
    pub root: Hash64,
    pub sponsor: PalwBondKeyV2,
    pub provider: PalwBondKeyV2,
    pub amount: u64,
    pub horizon: u64,
    pub accepted_daa: u64,
    pub defaulted_daa: Option<u64>,
    pub status: PalwMaterialFeeStatusV2,
}
pub fn fee_key(context: Hash64, provider: PalwBondKeyV2) -> Vec<u8> {
    borsh::to_vec(&(context, provider)).expect("fee key")
}
impl PalwChainStateV2 {
    pub fn material_fee_row_checked_v2(
        &self,
        context: Hash64,
        provider: PalwBondKeyV2,
    ) -> Result<Option<PalwMaterialFeeRowV2>, crate::palw_state_v2::PalwStateV2Error> {
        let Some(bytes) = self.kernel_route().and_then(|r| r.aux.get(&(TABLE, fee_key(context, provider)))) else { return Ok(None) };
        let bad = |s: &str| crate::palw_state_v2::PalwStateV2Error::CarriageInconsistent(s.into());
        let row: PalwMaterialFeeRowV2 = borsh::from_slice(bytes).map_err(|_| bad("malformed finite provider fee"))?;
        if row.version != VERSION || row.enrollment != context || row.provider != provider {
            return Err(bad("finite fee key/generation mismatch"));
        }
        Ok(Some(row))
    }
    pub fn material_fee_row_v2(&self, context: Hash64, provider: PalwBondKeyV2) -> Option<PalwMaterialFeeRowV2> {
        // Non-authoritative convenience for status/tests. Fold uses checked reader.
        self.material_fee_row_checked_v2(context, provider).ok().flatten()
    }
    pub fn material_provider_fee_complete_v2(&self, p: &PalwMonetaryMaterialProviderV1) -> bool {
        p.fee_paid
            || self.material_fee_row_v2(p.fee_context, p.provider).is_some_and(|r| {
                r.amount == p.prepaid_fee_sompi
                    && r.accepted_daa == p.fee_accepted_daa
                    && p.fee_queued
                    && matches!(r.status,PalwMaterialFeeStatusV2::RefundPaid{daa} if daa>=r.accepted_daa && r.defaulted_daa.is_some_and(|at|at<=daa && at<=r.horizon))
            })
    }
}

/// SDK adapter: obtain both existing service signatures and new fee signatures,
/// then the sponsor authorizes the complete atomic action. No partial object.
pub fn monetary_material_fee_object_v2(
    network: Hash64,
    ruleset: Hash64,
    mut enrollment: PalwMonetaryMaterialEnrollmentV1,
    replacement: Option<(Hash64, u64)>,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as C, palw_provider_court_message_v1};
    if ruleset == Hash64::default() {
        return None;
    }
    let (tag, payload) = match replacement {
        None => (240, enrollment.payload()),
        Some((previous, sequence)) => (
            241,
            PalwMonetaryMaterialReplacementV1 { root: enrollment.terms.root, previous, sequence, enrollment: enrollment.clone() }
                .payload(),
        ),
    };
    for offer in &mut enrollment.offers {
        let message = palw_provider_court_message_v1(network, tag, &offer.provider, &payload);
        offer.signature = sign(&offer.provider, message.as_byte_slice(), C).filter(|s| !s.is_empty())?;
    }
    let mut fee = PalwMaterialFeeEnrollmentV2 { ruleset, enrollment, provider_fee_signatures: Vec::new() };
    let payload = fee.fee_payload();
    for offer in &fee.enrollment.offers {
        let message = palw_provider_court_message_v1(network, 242, &offer.provider, &payload);
        fee.provider_fee_signatures.push(sign(&offer.provider, message.as_byte_slice(), C).filter(|s| !s.is_empty())?);
    }
    let signer = fee.enrollment.terms.sponsor;
    let action = match replacement {
        None => PalwMonetaryMaterialActionV1::EnrollFeeV2(Box::new(fee)),
        Some((previous, sequence)) => PalwMonetaryMaterialActionV1::ReplaceFeeV2 { previous, sequence, enrollment: Box::new(fee) },
    };
    let payload = borsh::to_vec(&action).ok()?;
    let message = palw_provider_court_message_v1(network, 242, &signer, &payload);
    let signature = sign(&signer, message.as_byte_slice(), C).filter(|s| !s.is_empty())?;
    Some(crate::palw_state_v2::PalwConsensusObjectV2::MonetaryMaterialActionV1 { action: Box::new(action), signer, signature })
}
