//! Candidate-local booking and payer validation with one publication point.

use super::{Tfp1FundingErrorV1, Tfp1FundingPolicyV1, Tfp1FundingRequirementsV1};
use crate::{
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_original_escrow_v1::{FundingRefusalV1, OriginalFundingScopeV1, PreparedEscrowFundingV1},
    palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations},
    palw_court_shared_material_intake_v1::batch::{
        admission::BatchDeliveryV1, plain_output::projection::PlainBatchOriginalProjectionV2,
    },
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeRetainedV1, btree_node_envelope_bytes_v1, native_typed_value_requests_v1,
        reserve_native_pending_requests_v1,
    },
    palw_positive_contract_v2::{PublicOriginalAdmissionV1, PublicOriginalProspectiveReservationV2, PublicOriginalReservationV1},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwTransitionExtrasV1},
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{OriginalCommit, SignedPublicOriginalV1},
};

use super::super::{
    PreparedTfp1PublicOriginalV1,
    carrier_material::MaterialPairingErrorV2,
    preclaim::{ProtectedBatchBookingV1, ProtectedBatchPreclaimTermsV1},
};

#[derive(Debug)]
pub(crate) enum Tfp1ProtectedFundingAdmissionErrorV1 {
    LocalResource(NativeCoverageV1),
    Batch(BatchCourtErrorV1),
    Funding(FundingRefusalV1),
    Material(MaterialPairingErrorV2),
}

impl From<MaterialPairingErrorV2> for Tfp1ProtectedFundingAdmissionErrorV1 {
    fn from(error: MaterialPairingErrorV2) -> Self {
        match error {
            MaterialPairingErrorV2::LocalResource(coverage) => Self::LocalResource(coverage),
            error => Self::Material(error),
        }
    }
}

impl From<BatchCourtErrorV1> for Tfp1ProtectedFundingAdmissionErrorV1 {
    fn from(error: BatchCourtErrorV1) -> Self {
        match error {
            BatchCourtErrorV1::LocalResource(coverage) => Self::LocalResource(coverage),
            error => Self::Batch(error),
        }
    }
}

impl From<Tfp1FundingErrorV1> for Tfp1ProtectedFundingAdmissionErrorV1 {
    fn from(error: Tfp1FundingErrorV1) -> Self {
        match error {
            Tfp1FundingErrorV1::LocalResource(coverage) => Self::LocalResource(coverage),
            Tfp1FundingErrorV1::Funding(refusal) => Self::Funding(refusal),
        }
    }
}

impl PreparedTfp1PublicOriginalV1 {
    /// Rebook signed obligations on the supplied candidate and publish the new
    /// book only after the literal carrier's payer and own output are checked.
    /// The caller still authenticates this candidate's parent, resolves the
    /// remaining profile and atomically installs any resulting consensus rows.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reserve_and_prepare_protected_funding_v1<'a>(
        &self,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        companion: &OriginalCommit,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        pairing: &'a PalwFundedOriginalPairingV1,
        point: &PalwBlockContextV2,
        book: &mut NativeRetainedV1<ServiceReservations>,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        policy: Tfp1FundingPolicyV1,
    ) -> Result<(ProtectedBatchBookingV1, PreparedEscrowFundingV1<'a>), Tfp1ProtectedFundingAdmissionErrorV1> {
        with_candidate_book_v2(book, maximum_packets, |candidate| {
            let booking = self.reserve_protected_batch_candidates_v1(projection, terms, candidate, maximum_packets, price)?;
            let required = Tfp1FundingRequirementsV1::derive_v1(
                fp,
                source,
                companion,
                terms,
                &self.companion_v1().budget,
                self.fee_used,
                &booking,
                policy,
            )?;
            let funded = required.prepare_pairing_v1(self, pairing, point)?;
            Ok((booking, funded))
        })
    }
}

/// An unsigned fee quotation, not an escrow debit, accepted origin or funded
/// reservation. Its booking remains candidate-local and owns native leases.
/// The public owner must still check literal materials and all non-Batch
/// duties before reporting a complete PublicOneHonest quote.
pub(crate) struct ProtectedFundingQuoteV2 {
    pub booking: ProtectedBatchBookingV1,
    pub funding: OriginalFundingScopeV1,
    pub original_fee_sompi: u64,
    pub future_carrier_fee_sompi: u64,
}

impl PublicOriginalAdmissionV1<'_> {
    /// Candidate entry: native literal material checks, complete early-duty
    /// identity, one reservation book, then native funding requirements.
    /// This is not an enabled public profile: the native owner must supply
    /// the complete policy/graph and authenticate the selected parent.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn quote_tfp1_material_funding_v3(
        &self,
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        prompt_form: PalwPromptIdsFormV1,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        companion: &OriginalCommit,
        prompt: &[u8],
        manifest: &[u8],
        descriptor: &[u8],
        reservation: &PublicOriginalProspectiveReservationV2<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        book: &mut NativeRetainedV1<ServiceReservations>,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        policy: Tfp1FundingPolicyV1,
    ) -> Result<ProtectedFundingQuoteV2, Tfp1ProtectedFundingAdmissionErrorV1> {
        let (_, supply) = self.check_tfp1_candidate_material_v3(
            parent,
            extras,
            prompt_form,
            fp,
            source,
            companion,
            prompt,
            manifest,
            descriptor,
            terms.obligation_v1(),
            reservation,
        )?;
        terms.check_initial_supply_v2(&supply)?;
        self.reserve_and_quote_protected_funding_v2(
            fp,
            source,
            companion,
            reservation,
            projection,
            terms,
            book,
            maximum_packets,
            price,
            policy,
        )
    }

    /// Actual carrier entry uses the same material and fee calculations,
    /// then additionally authenticates the real origin, payer consent and
    /// output. No candidate book is published on material/funding refusal.
    /// Returned preparations still need the single native storage checkpoint.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub(crate) fn prepare_tfp1_material_funding_v3<'a>(
        &self,
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        point: &PalwBlockContextV2,
        prompt_form: PalwPromptIdsFormV1,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        pairing: &'a PalwFundedOriginalPairingV1,
        reservation: &PublicOriginalReservationV1<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        book: &mut NativeRetainedV1<ServiceReservations>,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        policy: Tfp1FundingPolicyV1,
    ) -> Result<
        (PreparedTfp1PublicOriginalV1, ProtectedBatchBookingV1, PreparedEscrowFundingV1<'a>),
        Tfp1ProtectedFundingAdmissionErrorV1,
    > {
        let (prepared, _, supply) = self.prepare_tfp1_material_pairing_v2(
            parent,
            extras,
            point,
            prompt_form,
            fp,
            source,
            pairing,
            terms.obligation_v1(),
            reservation,
        )?;
        terms.check_initial_supply_v2(&supply)?;
        let signed = SignedPublicOriginalV1::decode(
            pairing.public_companion_v3().ok_or(MaterialPairingErrorV2::Contract(ContractError::Scope))?,
        )
        .map_err(MaterialPairingErrorV2::from)?;
        let (booking, funded) = prepared.reserve_and_prepare_protected_funding_v1(
            fp,
            source,
            &signed.original,
            projection,
            terms,
            pairing,
            point,
            book,
            maximum_packets,
            price,
            policy,
        )?;
        Ok((prepared, booking, funded))
    }

    /// Use the same reservation and funding calculator as actual signed
    /// intake. Even unsigned callers must pass the native fee/capital check
    /// before the candidate book is replaced; a fee ceiling is not a balance.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reserve_and_quote_protected_funding_v2(
        &self,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        companion: &OriginalCommit,
        reservation: &PublicOriginalProspectiveReservationV2<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        book: &mut NativeRetainedV1<ServiceReservations>,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
        policy: Tfp1FundingPolicyV1,
    ) -> Result<ProtectedFundingQuoteV2, Tfp1ProtectedFundingAdmissionErrorV1> {
        with_candidate_book_v2(book, maximum_packets, |candidate| {
            let booking = self.reserve_tfp1_protected_batch_candidate_v1(
                companion,
                reservation,
                projection,
                terms,
                candidate,
                maximum_packets,
                price,
            )?;
            let required = Tfp1FundingRequirementsV1::derive_v1(
                fp,
                source,
                companion,
                terms,
                reservation.budget,
                reservation.usage.fee,
                &booking,
                policy,
            )?;
            let funding = required.scope_v1().clone();
            let original_fee_sompi = required.original_fee_sompi_v1();
            let future_carrier_fee_sompi = required.future_carrier_fee_sompi_v1();
            Ok(ProtectedFundingQuoteV2 { booking, funding, original_fee_sompi, future_carrier_fee_sompi })
        })
    }
}

/// Lease and stage one candidate for both unsigned fee quotation and signed
/// payer checks. An error at any step leaves the caller's book unchanged.
fn with_candidate_book_v2<T>(
    book: &mut NativeRetainedV1<ServiceReservations>,
    maximum_packets: usize,
    plan: impl FnOnce(&mut ServiceReservations) -> Result<T, Tfp1ProtectedFundingAdmissionErrorV1>,
) -> Result<T, Tfp1ProtectedFundingAdmissionErrorV1> {
    use Tfp1ProtectedFundingAdmissionErrorV1::LocalResource;
    type PacketId = [u8; 64];
    let path = "TFP1 protected funding candidate book";
    let overflow = || LocalResource(NativeCoverageV1::overflow::<ServiceReservations>(path));
    if cfg!(not(target_pointer_width = "64")) {
        return Err(LocalResource(NativeCoverageV1::unsupported::<ServiceReservations>(
            path,
            "pinned BTree allocation layout only covers 64-bit targets",
        )));
    }
    // The scheduler's inner workspace covers the scheduling copy until
    // assignment succeeds. This independent ticket covers the outer copy
    // and the replacement after that workspace is dropped. Each new packet
    // can add one slot, assignment and packet entry. Supply is unchanged.
    let base = native_typed_value_requests_v1(&**book).map_err(LocalResource)?;
    let per_packet = (btree_node_envelope_bytes_v1::<u64, ServiceCost>()
        + btree_node_envelope_bytes_v1::<PacketId, u64>()
        + btree_node_envelope_bytes_v1::<PacketId, ServicePacket>()) as u64;
    let maximum_packets_u64 = u64::try_from(maximum_packets).map_err(|_| overflow())?;
    let requests = maximum_packets_u64.checked_mul(per_packet).and_then(|growth| base.checked_add(growth)).ok_or_else(overflow)?;
    let mut lease = reserve_native_pending_requests_v1(requests).map_err(LocalResource)?;
    let mut candidate = (**book).clone();
    let result = plan(&mut candidate)?;
    let retained = native_typed_value_requests_v1(&candidate).map_err(LocalResource)?;
    let unused = requests.checked_sub(retained).ok_or_else(|| {
        LocalResource(NativeCoverageV1::unsupported::<ServiceReservations>(path, "candidate exceeded its reserved bound"))
    })?;
    if unused != 0 {
        if let Some(lease) = &mut lease {
            drop(lease.split_memory_v1(unused).map_err(LocalResource)?);
        }
    }
    *book = NativeRetainedV1::from_owner_reserved_v1(candidate, lease);
    Ok(result)
}
