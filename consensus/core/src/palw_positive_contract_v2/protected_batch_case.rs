//! Derive Batch case authority from an accepted public original, a restored
//! draw and the literal accepted request. Reserving capacity, selecting the
//! draw and resolving inclusion history remain native caller responsibilities.

use super::{PreparedPublicDrawV1, PreparedPublicOriginalV1, PublicManifestClaimantAuthorityV1};
use crate::{Hash64, palw_court_shared_tir_stream_v1::stream_digest, palw_state_v2::PalwChainStateV2};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{Digest, PrincipalId},
    public_one_honest::{CaseId, Claimant, ManifestRequestContext, OriginalId, PanelBond},
    reservations::RemainingCapacity,
};

/// A normalized rank in this original's native draw. The compiler can reserve
/// the same rank before a target or CaseId exists. Ranks use ascending outpoint
/// order independently of the order used to commit the members of the draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RootedDrawnBatchOpportunityV1 {
    original: OriginalId,
    draw: Digest,
    bond: PanelBond,
    principal: PrincipalId,
    index: u64,
    rescue_reservation: Digest,
    capacity: RemainingCapacity,
}

#[allow(dead_code)]
impl RootedDrawnBatchOpportunityV1 {
    pub(crate) fn bond_v1(&self) -> PanelBond {
        self.bond
    }

    pub(crate) fn principal_v1(&self) -> PrincipalId {
        self.principal
    }

    pub(crate) fn index_v1(&self) -> u64 {
        self.index
    }

    pub(crate) fn capacity_v1(&self) -> RemainingCapacity {
        self.capacity
    }

    pub(crate) fn rescue_reservation_v1(&self) -> Digest {
        self.rescue_reservation
    }
}

/// A normalized case and its protected opportunity. CaseId binds the logical
/// stream. It does not change any physical packet IDs already booked by rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RootedDrawnBatchCaseV1 {
    opportunity: RootedDrawnBatchOpportunityV1,
    case: CaseId,
}

#[allow(dead_code)]
impl RootedDrawnBatchCaseV1 {
    pub(crate) fn case_id_v1(&self) -> CaseId {
        self.case
    }

    pub(crate) fn opportunity_v1(&self) -> RootedDrawnBatchOpportunityV1 {
        self.opportunity
    }

    /// Child namespace for exact accepted case requests. The enclosing Batch
    /// scope remains the scope authenticated by original admission.
    pub(crate) fn case_scope_v1(&self, batch_scope: Hash64) -> Result<Hash64, ContractError> {
        if batch_scope == Hash64::default() {
            return Err(ContractError::Scope);
        }
        stream_digest(
            b"misaka-palw/batch/protected-case/v1",
            &(batch_scope, self.opportunity.original, self.opportunity.draw, self.opportunity.index, self.case),
        )
        .map_err(|_| ContractError::NonCanonical)
    }
}

#[allow(dead_code)]
impl PreparedPublicOriginalV1 {
    /// Recheck both rooted records on the executing branch before using a
    /// prepared draw to find a claimant's prebookable rank.
    pub(crate) fn rooted_drawn_batch_opportunity_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &PreparedPublicDrawV1,
        bond: PanelBond,
    ) -> Result<RootedDrawnBatchOpportunityV1, ContractError> {
        let admission = state.public_original_admission_record_v1(self.original)?.ok_or(ContractError::Scope)?;
        if admission != self.record(admission.reservation)
            || draw.original != self.original
            || state.public_draw_record_v1(self.original)? != Some(draw.record())
        {
            return Err(ContractError::Scope);
        }
        let now = state.last_point().ok_or(ContractError::Scope)?.daa_score;
        if now >= draw.hard {
            return Err(ContractError::Range);
        }
        let key = (bond.transaction, bond.index);
        let member = draw.members.get(&key).ok_or(ContractError::Identity)?;
        let index = u64::try_from(draw.members.range(..key).count()).map_err(|_| ContractError::Budget)?;
        if draw.members.len() as u64 > self.rescue.max_drawn_bonds || member.rescue_reservation == Digest([0; 64]) {
            return Err(ContractError::Budget);
        }
        Ok(RootedDrawnBatchOpportunityV1 {
            original: self.original,
            draw: draw.draw,
            bond,
            principal: member.principal,
            index,
            rescue_reservation: member.rescue_reservation,
            capacity: draw.per_bond,
        })
    }

    /// Bind the rank to an accepted request. The caller resolves its literal
    /// bytes, origin, supply obligation and reservation from native history
    /// and the service book. A packet cannot nominate those expected values.
    pub(crate) fn rooted_drawn_batch_case_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &PreparedPublicDrawV1,
        bytes: &[u8],
        context: &ManifestRequestContext<'_>,
    ) -> Result<RootedDrawnBatchCaseV1, ContractError> {
        let Claimant::DrawnBond(bond) = context.claimant else {
            return Err(ContractError::Identity);
        };
        let opportunity = self.rooted_drawn_batch_opportunity_v1(state, draw, bond)?;
        let pending = self.verify_manifest_request_bytes(PublicManifestClaimantAuthorityV1::Drawn(draw), bytes, context)?;
        let case = pending.id();
        if state.public_pending_manifest_record_v1(self.original, case)? != Some(pending.record()) {
            return Err(ContractError::Scope);
        }
        Ok(RootedDrawnBatchCaseV1 { opportunity, case })
    }
}
