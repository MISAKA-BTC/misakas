//! Candidate-local booking and payer validation with one publication point.

use super::{Tfp1FundingErrorV1, Tfp1FundingPolicyV1, Tfp1FundingRequirementsV1};
use crate::{
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_original_escrow_v1::{FundingRefusalV1, PreparedEscrowFundingV1},
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
    palw_state_v2::PalwBlockContextV2,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::public_one_honest::OriginalCommit;

use super::super::{
    PreparedTfp1PublicOriginalV1,
    preclaim::{ProtectedBatchBookingV1, ProtectedBatchPreclaimTermsV1},
};

#[derive(Debug)]
pub(crate) enum Tfp1ProtectedFundingAdmissionErrorV1 {
    LocalResource(NativeCoverageV1),
    Batch(BatchCourtErrorV1),
    Funding(FundingRefusalV1),
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
        let booking = self.reserve_protected_batch_candidates_v1(projection, terms, &mut candidate, maximum_packets, price)?;
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
        Ok((booking, funded))
    }
}
