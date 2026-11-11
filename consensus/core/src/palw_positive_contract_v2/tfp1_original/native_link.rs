//! Immutable cross-reference installed with the native TFP1 frame, funded
//! reservation and PublicOriginal admission. Merely decoding the row confers
//! no right to open a case.

use super::{NativeTfp1PublicOriginalFactsV1, PairedTfp1CarrierV1, PreparedTfp1PublicOriginalV1};
use crate::{
    Hash64,
    palw_native_resource_v1::{typed::native_struct, wire::wire_struct},
    palw_positive_contract_v2::{
        AcceptedMaterial, PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, PUBLIC_TFP1_LINK_TABLE_V1, PublicOriginalAdmissionRecordV1,
        PublicOriginalAdmissionV1, PublicOriginalReservationV1, require_current_public_origin_v1,
    },
    palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimSourceV2},
    palw_tir_public_claim_v1::{PalwTirPublicClaimV1, palw_tir_public_claim_v1},
    palw_tir_public_source_v1::public_source_header_id_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{ContractError, identity::Digest, public_one_honest::OriginalId};

const VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct Tfp1PublicOriginalLinkV1 {
    version: u16,
    claim: Hash64,
    source: Hash64,
    execution_root: Hash64,
    fp_allocation: Hash64,
    fp_rules: Hash64,
    fp_resource: Hash64,
    original: OriginalId,
    carrier: AcceptedMaterial,
    literal_fp: Hash64,
    admission_reservation: Digest,
    obligation: Digest,
    profile: Hash64,
    resource: Hash64,
}

native_struct!(Tfp1PublicOriginalLinkV1; version, claim, source, execution_root, fp_allocation, fp_rules, fp_resource,
    original, carrier, literal_fp, admission_reservation, obligation, profile, resource);
wire_struct!(Tfp1PublicOriginalLinkV1; u16, Hash64, Hash64, Hash64, Hash64, Hash64, Hash64, OriginalId, AcceptedMaterial,
    Hash64, Digest, Digest, Hash64, Hash64);

/// Only the actual paired-carrier preparation can seal a native link. The
/// accepting owner supplies the reservation already derived for this
/// admission and calls the writer inside the frame/book/escrow checkpoint.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreparedTfp1PublicOriginalLinkV1 {
    row: Tfp1PublicOriginalLinkV1,
    admission: PublicOriginalAdmissionRecordV1,
    pairing: PairedTfp1CarrierV1,
}

/// One intake candidate owns both the immutable link and the replay locator.
/// No public DATA constructor or separately supplied boundary can create it.
pub(crate) struct PreparedTfp1OriginalHistoryLinkV1 {
    pub(super) link: PreparedTfp1PublicOriginalLinkV1,
    boundary: crate::palw_tfp1_original_history_v1::Tfp1OriginalHistoryBoundaryV1,
}
impl PreparedTfp1OriginalHistoryLinkV1 {
    pub(crate) fn link_v1(&self) -> &PreparedTfp1PublicOriginalLinkV1 {
        &self.link
    }
    pub(crate) fn boundary_v1(&self) -> &crate::palw_tfp1_original_history_v1::Tfp1OriginalHistoryBoundaryV1 {
        &self.boundary
    }
}

impl PublicOriginalAdmissionV1<'_> {
    /// Called by the original admission owner with its sealed pre-write
    /// boundary and pairing-bound validated UTXO handoff. Complete policy, booking,
    /// paid retention and FP/payer signature checks remain mandatory there.
    pub(crate) fn prepare_tfp1_history_link_v1(
        &self,
        state: &PalwChainStateV2,
        point: &PalwBlockContextV2,
        pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
        obligation: super::Tfp1PublicOriginalObligationV1,
        inputs: &PublicOriginalReservationV1<'_>,
        reservation: Digest,
        prefix: &crate::palw_state_v2::Tfp1OriginalPrefixV1,
    ) -> Result<PreparedTfp1OriginalHistoryLinkV1, ContractError> {
        let prepared = self.prepare_tfp1_pairing_v1(pairing, obligation, inputs)?;
        let boundary = crate::palw_tfp1_original_history_v1::Tfp1OriginalHistoryBoundaryV1::from_actual_data_v1(
            obligation.facts.claim,
            obligation.facts.source,
            prepared.companion.original,
            inputs.origin,
            prefix,
            pairing.history_inputs_v1().ok_or(ContractError::Scope)?,
            self,
            inputs,
        )?;
        let link = prepared.prepare_native_link_v1(state, point, reservation)?;
        Ok(PreparedTfp1OriginalHistoryLinkV1 { link, boundary })
    }

    /// Restore a paired original from its literal accepted transaction and
    /// the admission inputs reconstructed at its original inclusion point.
    /// The current rooted link chooses profile/resource and origin; callers
    /// cannot nominate a replacement obligation or refresh the original clock.
    ///
    /// The history owner must authenticate carrier inclusion and reconstruct
    /// the original admission inputs, FP/payer authorization and UTXO funding.
    /// This method rechecks the companion signature and every stored admission
    /// field, but does not reconstruct the UTXO prefix or prove that a complete
    /// production reservation was installed.
    /// Table 206 commits the full admission inputs, including raw readiness
    /// inputs omitted from table 198. The history owner must reconstruct them
    /// before calling; the commitment checks equality, not policy validity.
    pub(crate) fn restore_tfp1_pairing_v1(
        &self,
        state: &PalwChainStateV2,
        pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
        inputs: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedTfp1PublicOriginalV1, ContractError> {
        let link = state.public_tfp1_original_link_v1(pairing.claim())?.ok_or(ContractError::Scope)?;
        if inputs.origin != link.carrier.origin {
            return Err(ContractError::Scope);
        }
        let history = crate::palw_tfp1_original_history_v1::rooted_tfp1_original_history_v1(state, pairing.claim())?
            .ok_or(ContractError::Scope)?;
        history.check_admission_inputs_v1(self, inputs)?;
        let frame = palw_tir_public_claim_v1(state, link.claim).ok_or(ContractError::Scope)?;
        let facts = link.check_frame(state, frame)?;
        let obligation = facts.service_obligation_v1(link.profile, link.resource)?;
        let prepared = self.prepare_tfp1_pairing_v1(pairing, obligation, inputs)?;
        // Includes the literal companion digest, original rescue capacities,
        // deadlines and reservation, not only semantic claim equality.
        if prepared.rooted_link_v1(state)? != link {
            return Err(ContractError::Scope);
        }
        Ok(prepared)
    }
}

impl PreparedTfp1PublicOriginalV1 {
    /// Restore the association from the literal paired transaction on the
    /// executing branch. Settled funding is allowed here: only installation
    /// needs the fresh escrow. A detached signed companion is insufficient.
    pub(super) fn rooted_link_v1(&self, state: &PalwChainStateV2) -> Result<Tfp1PublicOriginalLinkV1, ContractError> {
        let pairing = self.pairing.ok_or(ContractError::Scope)?;
        let row = state.public_tfp1_original_link_v1(self.obligation.facts.claim)?.ok_or(ContractError::Scope)?;
        if row.original != self.companion.original
            || row.carrier != self.companion.carrier
            || row.literal_fp != pairing.literal_fp
            || (row.profile, row.resource, row.obligation)
                != (self.obligation.profile, self.obligation.resource, self.obligation.obligation)
            || row.admission_at(state, state.last_point().ok_or(ContractError::Scope)?)?
                != self.companion.record(row.admission_reservation)
        {
            return Err(ContractError::Scope);
        }
        let frame = palw_tir_public_claim_v1(state, row.claim).ok_or(ContractError::Scope)?;
        if row.check_frame(state, frame)? != self.obligation.facts {
            return Err(ContractError::Scope);
        }
        Ok(row)
    }

    /// A native TFP1 request must satisfy both the immutable paired link and
    /// the accepted draw/request chain. No caller-supplied claim or namespace
    /// is promoted into authority. The paired paid row must already exist;
    /// restoring it does not charge again or consume a second physical slot.
    pub(crate) fn rooted_drawn_batch_case_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &crate::palw_positive_contract_v2::PreparedPublicDrawV1,
        bytes: &[u8],
        context: &misaka_palw_positive_contract::public_one_honest::ManifestRequestContext<'_>,
    ) -> Result<RootedTfp1DrawnBatchCaseV1, ContractError> {
        let link = self.rooted_link_v1(state)?;
        let case = self.companion.rooted_drawn_batch_case_v1(state, draw, bytes, context)?;
        // This namespace is specific to the new PublicOriginal profile. Do
        // not relabel a legacy first-notice or shared-context packet with it.
        let scope = self.obligation.batch_scope_v1(link.original)?;
        let reservation_scope = self.drawn_batch_reservation_scope_v1(case.opportunity_v1().index_v1())?;
        let packet = crate::palw_court_shared_material_intake_v1::batch::admission::protected::manifest_request_packet_id_v1(
            link.claim,
            reservation_scope,
        )
        .map_err(|_| ContractError::Scope)?;
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let paid_bytes = route
            .aux
            .get(&(crate::palw_positive_contract_v2::PUBLIC_MANIFEST_PAID_TABLE_V1, packet.to_vec()))
            .ok_or(ContractError::Scope)?;
        let paid = super::manifest_paid::PaidManifestRequestRowV1::decode_row(&packet, paid_bytes)?;
        paid.check_state_at_v1(state, state.last_point().ok_or(ContractError::Scope)?)?;
        paid.check_continuation_v1(&link, reservation_scope, case.case_id_v1(), context.origin)?;
        Ok(RootedTfp1DrawnBatchCaseV1 {
            claim: link.claim,
            source: link.source,
            scope,
            case_scope: case.case_scope_v1(scope)?,
            reservation_scope,
            case,
        })
    }

    pub(crate) fn prepare_native_link_v1(
        self,
        state: &PalwChainStateV2,
        point: &PalwBlockContextV2,
        admission_reservation: Digest,
    ) -> Result<PreparedTfp1PublicOriginalLinkV1, ContractError> {
        let pairing = self.pairing.ok_or(ContractError::Scope)?;
        let admission = self.companion.record(admission_reservation);
        let carrier = admission.carrier();
        require_current_public_origin_v1(carrier.origin, point)?;
        let claim = self.obligation.facts.claim;
        check_paired_escrow(state, point, claim, pairing)?;
        let frame = palw_tir_public_claim_v1(state, claim).ok_or(ContractError::Scope)?;
        let plan = frame.plan_v1();
        let row = Tfp1PublicOriginalLinkV1 {
            version: VERSION,
            claim,
            source: self.obligation.facts.source,
            execution_root: self.obligation.facts.execution_root,
            fp_allocation: plan.allocation_id,
            fp_rules: plan.rules_id,
            fp_resource: plan.resource_plan_id,
            original: admission.original,
            carrier,
            literal_fp: pairing.literal_fp,
            admission_reservation,
            obligation: self.obligation.obligation,
            profile: self.obligation.profile,
            resource: self.obligation.resource,
        };
        row.check_shape(&claim.as_bytes())?;
        if row.check_frame(state, frame)? != self.obligation.facts {
            return Err(ContractError::Scope);
        }
        Ok(PreparedTfp1PublicOriginalLinkV1 { row, admission, pairing })
    }
}

/// A branch-local result, not a wire credential. Native writers must derive
/// it again from their current prefix before validating reserved slots/fees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootedTfp1DrawnBatchCaseV1 {
    claim: Hash64,
    source: Hash64,
    scope: Hash64,
    case_scope: Hash64,
    reservation_scope: Hash64,
    case: crate::palw_positive_contract_v2::protected_batch_case::RootedDrawnBatchCaseV1,
}

impl RootedTfp1DrawnBatchCaseV1 {
    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }
    pub(crate) fn source_v1(&self) -> Hash64 {
        self.source
    }
    pub(crate) fn scope_v1(&self) -> Hash64 {
        self.scope
    }
    pub(crate) fn case_scope_v1(&self) -> Hash64 {
        self.case_scope
    }
    pub(crate) fn reservation_scope_v1(&self) -> Hash64 {
        self.reservation_scope
    }
    pub(crate) fn case_v1(&self) -> crate::palw_positive_contract_v2::protected_batch_case::RootedDrawnBatchCaseV1 {
        self.case
    }
}

impl PreparedTfp1PublicOriginalLinkV1 {
    pub(crate) fn storage_row(
        &self,
        state: &PalwChainStateV2,
        point: &PalwBlockContextV2,
    ) -> Result<Option<([u8; 64], &Tfp1PublicOriginalLinkV1)>, ContractError> {
        let row = &self.row;
        let key = row.claim.as_bytes();
        require_current_public_origin_v1(row.carrier.origin, point)?;
        check_paired_escrow(state, point, row.claim, self.pairing)?;
        row.check_state_at_v1(state, point)?;
        if row.admission_at(state, point)? != self.admission {
            return Err(ContractError::Scope);
        }
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        if let Some(previous) = route.aux.get(&(PUBLIC_TFP1_LINK_TABLE_V1, key.to_vec())) {
            if Tfp1PublicOriginalLinkV1::decode_row(&key, previous)? != *row {
                return Err(ContractError::Scope);
            }
            return Ok(None);
        }
        Ok(Some((key, row)))
    }
}

// The same claim may be carried twice in one accepting block. The native
// escrow's birth outpoint chooses the carrier that actually won admission.
// This check is only for installation; later settlements may move it.
fn check_paired_escrow(
    state: &PalwChainStateV2,
    point: &PalwBlockContextV2,
    claim: Hash64,
    pairing: PairedTfp1CarrierV1,
) -> Result<(), ContractError> {
    let escrow = state.funded_escrows_v1().get(&pairing.funding_contract).ok_or(ContractError::Scope)?;
    if escrow.claim != claim
        || escrow.contract_id != pairing.funding_contract
        || escrow.funding_outpoint != pairing.funding_outpoint
        || escrow.amount_sompi != pairing.funding_amount
        || escrow.accepted_daa != point.daa_score
        || !escrow.liability_open
        || (escrow.paid_sompi, escrow.burned_sompi, escrow.returned_sompi) != (0, 0, 0)
        || escrow.settled_by.is_some()
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

impl Tfp1PublicOriginalLinkV1 {
    pub(crate) const WIRE_BYTES: usize = 2 + 16 * 64 + 2 * 4 + 2 * 8;

    pub(crate) fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != Self::WIRE_BYTES {
            return Err(ContractError::NonCanonical);
        }
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        row.check_shape(key)?;
        Ok(row)
    }

    fn check_shape(&self, key: &[u8]) -> Result<(), ContractError> {
        if self.version != VERSION || key != self.claim.as_byte_slice() {
            return Err(ContractError::Identity);
        }
        if [
            self.claim,
            self.source,
            self.execution_root,
            self.fp_allocation,
            self.fp_rules,
            self.fp_resource,
            self.literal_fp,
            self.profile,
            self.resource,
        ]
        .contains(&Hash64::default())
            || [self.original.0, self.carrier.digest, self.admission_reservation, self.obligation].contains(&Digest([0; 64]))
            || self.carrier.origin.accepting_block.0 == [0; 64]
            || self.carrier.origin.carrier_block.0 == [0; 64]
            || self.carrier.origin.carrier_transaction.0 == [0; 64]
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    fn admission_at(
        &self,
        state: &PalwChainStateV2,
        point: &PalwBlockContextV2,
    ) -> Result<PublicOriginalAdmissionRecordV1, ContractError> {
        // During authoritative install the builder still has its parent's
        // last_point. Use the accepting point supplied by the native owner.
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let key = self.original.0.0;
        let bytes = route.aux.get(&(PUBLIC_ORIGINAL_ADMISSION_TABLE_V1, key.to_vec())).ok_or(ContractError::Scope)?;
        let admission = PublicOriginalAdmissionRecordV1::decode_row(&key, bytes)?;
        admission.check_state_point(point)?;
        if admission.carrier() != self.carrier || admission.reservation != self.admission_reservation {
            return Err(ContractError::Scope);
        }
        Ok(admission)
    }

    fn check_frame(
        &self,
        state: &PalwChainStateV2,
        frame: &PalwTirPublicClaimV1,
    ) -> Result<NativeTfp1PublicOriginalFactsV1, ContractError> {
        let claim = state.claim(&self.claim).ok_or(ContractError::Scope)?;
        let original = frame.original_v1();
        let header = frame.header_v1();
        let plan = frame.plan_v1();
        let origin = self.carrier.origin;
        if !matches!(&claim.source, PalwClaimSourceV2::FreePrompt { .. })
            || claim.accepted_block.as_bytes() != origin.accepting_block.0
            || claim.accepted_daa != origin.accepted_daa
            || claim.accepted_blue_score != origin.accepted_blue_score
            || plan.terms.accepted_daa != origin.accepted_daa
            || claim.class_id != original.job.class_id
            || claim.bond != PalwBondKeyV2(original.job.executor_bond)
            || claim.execution_root != self.execution_root
            || original.execution_root != self.execution_root
            || claim.output_root != original.output_root
            || claim.trace_root != original.trace_root
            || claim.job_identity != super::palw_fp_job_pin_of_context_v1(&header.job_context)
            || plan.original_claim != self.claim
            || plan.source_id != self.source
            || public_source_header_id_v1(header).map_err(|_| ContractError::NonCanonical)? != self.source
            || plan.allocation_id != self.fp_allocation
            || plan.rules_id != self.fp_rules
            || plan.resource_plan_id != self.fp_resource
        {
            return Err(ContractError::Scope);
        }
        let facts = NativeTfp1PublicOriginalFactsV1::from_native_header_v1(original, header, self.claim, self.source)?;
        if facts.service_obligation_v1(self.profile, self.resource)?.obligation_v1() != self.obligation {
            return Err(ContractError::Scope);
        }
        Ok(facts)
    }

    pub(crate) fn check_state_at_v1(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2) -> Result<(), ContractError> {
        self.check_shape(&self.claim.as_bytes())?;
        let frame = palw_tir_public_claim_v1(state, self.claim).ok_or(ContractError::Scope)?;
        self.check_frame(state, frame)?;
        self.admission_at(state, point)?;
        Ok(())
    }

    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }

    pub(crate) fn original_v1(&self) -> OriginalId {
        self.original
    }

    pub(crate) fn source_v1(&self) -> Hash64 {
        self.source
    }

    pub(crate) fn carrier_v1(&self) -> AcceptedMaterial {
        self.carrier
    }

    pub(crate) fn reservation_v1(&self) -> Digest {
        self.admission_reservation
    }
}
