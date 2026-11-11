//! Native protected booking -> payer consent/output authentication.
//! This does not install escrow, price unimplemented duties, or enable a profile.
pub(crate) mod admission;
use super::{
    NativeTfp1PublicOriginalFactsV1, PreparedTfp1PublicOriginalV1, Tfp1PublicOriginalObligationV1,
    preclaim::{ProtectedBatchBookingV1, ProtectedBatchPreclaimTermsV1, ProtectedBatchRoleV1},
};
use crate::{
    Hash64,
    palw_court_original_escrow_v1::{
        FundingRefusalV1, OriginalFundingScopeV1, PreparedEscrowFundingV1, inspect_transaction_v1, prepare_funding_v1,
    },
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
    palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::require_current_public_origin_v1,
    palw_state_v2::PalwBlockContextV2,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{
    messages::AggregateBudget,
    public_one_honest::{OriginalCommit, OriginalId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tfp1FundingErrorV1 {
    Funding(FundingRefusalV1),
    LocalResource(NativeCoverageV1),
}

impl From<FundingRefusalV1> for Tfp1FundingErrorV1 {
    fn from(error: FundingRefusalV1) -> Self {
        Self::Funding(error)
    }
}

/// Independently resolved native values for duties outside the Batch graph.
/// No network currently resolves the complete protected profile. Supplying
/// these values is not evidence that those other duties have been reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tfp1FundingPolicyV1 {
    pub profiles: [Hash64; 4],
    pub other_relative_graph: Hash64,
    pub original_fee_sompi: u64,
    pub capital_floor_sompi: u64,
}

#[cfg(test)]
pub(crate) mod tests;

/// Only native recomputation creates this owner. Provider-carrier fees stay
/// separate from the original output: future paid carrier fees are not paid
/// twice into original escrow, nor does an own output count as transaction fee.
/// Borrowing the sealed booking keeps its native allocation leases alive.
/// Actual admission must rebook against the current candidate; matching clocks
/// alone do not authenticate a parent or a reservation book.
pub(crate) struct Tfp1FundingRequirementsV1<'booking> {
    booking: &'booking ProtectedBatchBookingV1,
    scope: OriginalFundingScopeV1,
    network: Hash64,
    original: OriginalId,
    obligation: Tfp1PublicOriginalObligationV1,
    fee_used: u64,
    fee_budget: u64,
    retention_until: u64,
    original_fee_sompi: u64,
    future_carrier_fee_sompi: u64,
}

impl<'booking> Tfp1FundingRequirementsV1<'booking> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_v1(
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        companion: &OriginalCommit,
        terms: &ProtectedBatchPreclaimTermsV1,
        budget: &AggregateBudget,
        fee_used: u64,
        booking: &'booking ProtectedBatchBookingV1,
        policy: Tfp1FundingPolicyV1,
    ) -> Result<Self, Tfp1FundingErrorV1> {
        use FundingRefusalV1::{Coverage, Scope};
        if policy.profiles.contains(&Hash64::default())
            || policy.other_relative_graph == Hash64::default()
            || policy.original_fee_sompi == 0
            || policy.capital_floor_sompi == 0
        {
            return Err(Coverage.into());
        }
        super::super::tfp1_material::require_cold_tfp1_v1(fp, &source.header_v1().job_context).map_err(|_| Scope)?;
        // Legacy claim/header/context identities serialize their preimages.
        // Reserve the actual bounded inputs before those hashes allocate.
        let fp_bytes = borsh::object_length(fp).map_err(|_| Scope)?;
        if fp_bytes > crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES {
            return Err(Scope.into());
        }
        let mut scratch = TirNativeRequestsV1::new();
        for bytes in [
            fp_bytes,
            borsh::object_length(source.header_v1()).map_err(|_| Scope)?,
            borsh::object_length(&source.header_v1().job_context).map_err(|_| Scope)?,
        ] {
            scratch.vec_v1::<u8>((bytes as u64).max(1024)).map_err(Tfp1FundingErrorV1::LocalResource)?;
        }
        let _hash_scratch = reserve_native_pending_requests_v1(scratch.bytes_v1()).map_err(Tfp1FundingErrorV1::LocalResource)?;
        let obligation = terms.obligation_v1();
        let facts = NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source).map_err(|_| Scope)?;
        if facts != *obligation.facts_v1() {
            return Err(Scope.into());
        }
        obligation.check_companion_v1(companion, budget).map_err(|_| Scope)?;
        let original = companion.id();
        if booking.original_v1() != original || booking.obligation_v1() != obligation {
            return Err(Scope.into());
        }
        let quotes = booking.quotes_v1();
        let rescue = terms.rescue_v1();
        let count = rescue.max_drawn_bonds.checked_add(rescue.max_outsiders).and_then(|n| n.checked_add(1)).ok_or(Coverage)?;
        if usize::try_from(count).ok() != Some(quotes.len()) {
            return Err(Scope.into());
        }
        let (early_escrow, mut future_carrier_fee) =
            booking.early_fees_v1().map_or((0, 0), |fees| (fees.escrow_sompi, fees.future_carrier_sompi));
        let mut node_storage = 0u64;
        for (index, quote) in quotes.iter().enumerate() {
            let index = u64::try_from(index).map_err(|_| Coverage)?;
            let (role, rank) = if index < rescue.max_drawn_bonds {
                (ProtectedBatchRoleV1::Drawn, index)
            } else if index - rescue.max_drawn_bonds < rescue.max_outsiders {
                (ProtectedBatchRoleV1::Outsider, index - rescue.max_drawn_bonds)
            } else {
                (ProtectedBatchRoleV1::Defense, 0)
            };
            if quote.claim != facts.claim
                || quote.scope != terms.reservation_scope_v1(original, role, rank).map_err(|_| Scope)?
                || quote.relative_graph_id != terms.relative_graph_id_v1()
            {
                return Err(Scope.into());
            }
            if !(1..=crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).contains(&quote.required_daa)
                || quote.material_until_daa == 0
                || quote.material_until_daa > budget.retention_until
                || quote.retained_numerical_fault_bytes == 0
                || quote.retained_numerical_fault_byte_daa < quote.retained_numerical_fault_bytes
            {
                return Err(Coverage.into());
            }
            node_storage = node_storage.checked_add(quote.numerical_fault_storage_rent_sompi).ok_or(Coverage)?;
            future_carrier_fee = future_carrier_fee.checked_add(quote.carrier_storage_rent_sompi).ok_or(Coverage)?;
        }
        let original_fee =
            policy.original_fee_sompi.checked_add(node_storage).and_then(|fee| fee.checked_add(early_escrow)).ok_or(Coverage)?;
        let total_fee = original_fee.checked_add(future_carrier_fee).and_then(|n| n.checked_add(fee_used)).ok_or(Coverage)?;
        if total_fee > budget.fee {
            return Err(Coverage.into());
        }
        let relative_graph_id = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-protected-funding-scope/v1",
            &(
                policy.other_relative_graph,
                obligation.profile,
                obligation.resource,
                terms.relative_graph_id_v1(),
                rescue,
                policy.profiles,
                policy.original_fee_sompi,
                policy.capital_floor_sompi,
                node_storage,
                future_carrier_fee,
            ),
        )
        .map_err(|_| Scope)?;
        let scope = OriginalFundingScopeV1 {
            claim: facts.claim,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&source.header_v1().job_context),
            execution_root: facts.execution_root,
            source_id: facts.source,
            relative_graph_id,
            fee_profile_id: policy.profiles[0],
            capital_profile_id: policy.profiles[1],
            retention_profile_id: policy.profiles[2],
            settlement_policy_id: policy.profiles[3],
            required_amount_sompi: original_fee.checked_add(policy.capital_floor_sompi).ok_or(Coverage)?,
            required_material_until_daa: budget.retention_until,
        };
        Ok(Self {
            booking,
            scope,
            network: facts.network,
            original,
            obligation,
            fee_used,
            fee_budget: budget.fee,
            retention_until: budget.retention_until,
            original_fee_sompi: original_fee,
            future_carrier_fee_sompi: future_carrier_fee,
        })
    }

    pub(crate) fn scope_v1(&self) -> &OriginalFundingScopeV1 {
        &self.scope
    }
    pub(crate) fn original_fee_sompi_v1(&self) -> u64 {
        self.original_fee_sompi
    }
    pub(crate) fn future_carrier_fee_sompi_v1(&self) -> u64 {
        self.future_carrier_fee_sompi
    }

    /// Requires the exact accepted UTXO handoff and paired companion, then
    /// authenticates the payer's separate consent against the native quote.
    /// Creating this borrowed result grants no settlement or spend privilege.
    pub(crate) fn prepare_pairing_v1<'a>(
        &self,
        prepared: &PreparedTfp1PublicOriginalV1,
        pairing: &'a PalwFundedOriginalPairingV1,
        point: &PalwBlockContextV2,
    ) -> Result<PreparedEscrowFundingV1<'a>, Tfp1FundingErrorV1> {
        use FundingRefusalV1::Scope;
        let pinned = prepared.pairing.ok_or(Scope)?;
        let companion = prepared.companion_v1();
        let inputs = pairing.history_inputs_v1().ok_or(Scope)?;
        require_current_public_origin_v1(companion.carrier.origin, point).map_err(|_| Scope)?;
        let envelope = pairing.envelope();
        if !self.booking.matches_acceptance_v1(point.daa_score, point.blue_score)
            || prepared.obligation != self.obligation
            || prepared.fee_used != self.fee_used
            || companion.original != self.original
            || companion.budget.fee != self.fee_budget
            || companion.budget.retention_until != self.retention_until
            || companion.carrier.origin != inputs.origin_v1()
            || pairing.claim() != self.scope.claim
            || pairing.carrier().id().as_bytes() != companion.carrier.origin.carrier_transaction.0
            || pairing.outpoint() != pinned.funding_outpoint
            || envelope.funding.terms.id_v1() != pinned.funding_contract
            || envelope.funding.terms.funding_amount_sompi != pinned.funding_amount
        {
            return Err(Scope.into());
        }
        // The private immutable pairing was created by literal payload/output
        // matching. Recheck its actual own output and payer signature below,
        // without re-encoding/decoding the entire nested transport a second time.
        let actual = inspect_transaction_v1(pairing.carrier(), envelope.funding.terms.funding_output_index)?;
        if actual.outpoint() != pairing.outpoint() {
            return Err(Scope.into());
        }
        prepare_funding_v1(actual, &envelope.funding, &envelope.payer_pubkey, &self.scope, self.network, point.daa_score)
            .map_err(Into::into)
    }
}
