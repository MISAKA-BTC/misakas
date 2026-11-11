//! Immutable native UTXO seed shared by rehearsal and actual replay. Each
//! attempt owns its fee ledger; dropping an outer body drops that ledger too.
//! Fee projections are compared by the accepting owner, not paid a second time.
use super::*;
use crate::palw_native_resource_v1::TirNativeRequestsV1;
use std::sync::Arc;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct ManifestRequestReplaySeedV1 {
    parent_root: Hash64,
    initial: ValidatedManifestRequestCarrierV1,
}

pub(crate) type SharedManifestRequestReplaySeedV1 = Arc<NativeRetainedV1<ManifestRequestReplaySeedV1>>;
crate::palw_native_resource_v1::native_struct!(ManifestRequestReplaySeedV1; parent_root, initial);

/// Read-only accounting result. A projection is not a paid receipt. Production
/// must reconcile preview/observed/actual at this exact parent and ordinal,
/// then deduct `charge` once from the same carrier's collectible fee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManifestRequestFeeV1 {
    parent_root: Hash64,
    context: ManifestRequestCarrierContextV1,
    actual_fee: u64,
    preceding_fee: u64,
    charge: u64,
    collectible_fee: u64,
}

crate::palw_native_resource_v1::native_struct!(ManifestRequestFeeV1; parent_root, context, actual_fee, preceding_fee, charge, collectible_fee);

impl ValidatedManifestRequestCarrierV1 {
    pub(crate) fn prepare_shared_v1(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        parent_root: Hash64,
        carrier_block: Hash64,
        at: PalwBlockContextV2,
        accepted_ordinal: usize,
    ) -> Result<Option<SharedManifestRequestReplaySeedV1>, ManifestRequestCarrierErrorV1> {
        Self::prepare_v1(validated, preceding_fee, carrier_block, at, accepted_ordinal)?
            .map(|handoff| handoff.into_replay_seed_v1(parent_root))
            .transpose()
    }

    /// Only an untouched validated ledger can become a replay seed. A charged
    /// attempt cannot be used to mint a seed with a silently reduced budget.
    pub(crate) fn into_replay_seed_v1(
        self,
        parent_root: Hash64,
    ) -> Result<SharedManifestRequestReplaySeedV1, ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::{Context, Resource};
        if parent_root == Hash64::default() || self.charged || self.charge_purpose.is_some() || self.replay_parent_root.is_some() {
            return Err(Context);
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<NativeRetainedV1<ManifestRequestReplaySeedV1>>(1).map_err(Resource)?;
        q.exact_v1::<usize>(3).map_err(Resource)?; // Arc counters and alignment
        // Candidate, accepted and actual ordered indexes can coexist.
        for _ in 0..3 {
            q.vec_v1::<(usize, SharedManifestRequestReplaySeedV1)>(1).map_err(Resource)?;
        }
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Resource)?;
        Ok(Arc::new(NativeRetainedV1::from_owner_reserved_v1(ManifestRequestReplaySeedV1 { parent_root, initial: self }, owner)))
    }
}

impl ManifestRequestReplaySeedV1 {
    pub(crate) fn context_v1(&self) -> ManifestRequestCarrierContextV1 {
        self.initial.context
    }

    pub(crate) fn accepted_ordinal_v1(&self) -> usize {
        self.initial.context.accepted_ordinal
    }

    pub(crate) fn carrier_v1(&self) -> TransactionId {
        self.initial.context.carrier
    }

    /// The parent root is fixed before replay; it is not the changing prefix
    /// root after an earlier ordinary object or native carrier has executed.
    pub(crate) fn candidate_at_v1(
        &self,
        parent_root: Hash64,
        at: &PalwBlockContextV2,
    ) -> Result<ValidatedManifestRequestCarrierV1, ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::{Context, Resource};
        if parent_root != self.parent_root || at != &self.initial.context.at {
            return Err(Context);
        }
        let owner = reserve_native_pending_requests_v1(fee_ledger_bytes_v1(1)).map_err(Resource)?;
        Ok(ValidatedManifestRequestCarrierV1 {
            context: self.initial.context,
            request: self.initial.request,
            budget: NativeRetainedV1::from_owner_reserved_v1(CarrierFeeBudgetV1::clone(&self.initial.budget), owner),
            charged: false,
            charge_purpose: None,
            replay_parent_root: Some(self.parent_root),
        })
    }

    /// Called with the native writer's actual newly charged amount. Semantic
    /// refusals and exact idempotent replays leave an untouched zero-charge
    /// candidate. A caller's claimed amount alone cannot spend this budget.
    pub(crate) fn project_after_candidate_v1(
        &self,
        candidate: &ValidatedManifestRequestCarrierV1,
        charged: u64,
    ) -> Result<ManifestRequestFeeV1, ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::{Charge, Context};
        if candidate.replay_parent_root != Some(self.parent_root)
            || candidate.context != self.initial.context
            || candidate.request != self.initial.request
            || candidate.budget.actual_fee_v1() != self.initial.budget.actual_fee_v1()
        {
            return Err(Context);
        }
        let available = self.initial.budget.remaining_v1();
        if available.checked_sub(charged) != Some(candidate.budget.remaining_v1()) || candidate.charged != (charged != 0) {
            return Err(Charge);
        }
        let signed = SignedManifestRequestV1::decode(&self.initial.request).map_err(|_| Charge)?;
        let packet = crate::palw_court_shared_material_intake_v1::batch::admission::protected::manifest_request_packet_id_v1(
            self.initial.context.scope.claim,
            self.initial.context.scope.reservation_scope,
        )
        .map_err(|_| Charge)?;
        let purpose =
            super::super::manifest_paid::manifest_request_charge_purpose_v1(signed.request.original, packet).map_err(|_| Charge)?;
        if candidate.charge_purpose != (charged != 0).then_some(purpose) {
            return Err(Charge);
        }
        let actual_fee = self.initial.budget.actual_fee_v1();
        let preceding_fee = actual_fee.checked_sub(available).ok_or(Charge)?;
        Ok(ManifestRequestFeeV1 {
            parent_root: self.parent_root,
            context: self.initial.context,
            actual_fee,
            preceding_fee,
            charge: charged,
            collectible_fee: candidate.budget.remaining_v1(),
        })
    }
}

impl ManifestRequestFeeV1 {
    pub(crate) fn charge_v1(&self) -> u64 {
        self.charge
    }

    /// Validate the result at the exact native UTXO collection point. Reusing
    /// an effect from another source block, accepted ordinal or parent fails.
    pub(crate) fn collectible_at_v1(
        &self,
        parent_root: Hash64,
        context: &ManifestRequestCarrierContextV1,
        actual_fee: u64,
        preceding_fee: u64,
    ) -> Result<u64, ManifestRequestCarrierErrorV1> {
        if parent_root != self.parent_root
            || context != &self.context
            || actual_fee != self.actual_fee
            || preceding_fee != self.preceding_fee
        {
            return Err(ManifestRequestCarrierErrorV1::Context);
        }
        if preceding_fee.checked_add(self.charge).and_then(|n| n.checked_add(self.collectible_fee)) != Some(actual_fee) {
            return Err(ManifestRequestCarrierErrorV1::Charge);
        }
        Ok(self.collectible_fee)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ManifestRequestFeeTotalsV1 {
    pub actual_fee: u64,
    pub preceding_fee: u64,
    pub charge: u64,
    pub collectible_fee: u64,
}

/// Compare complete ordered results, not just the sum burned. Equal totals
/// can hide a fee moving to another transaction, parent or source block.
pub(crate) fn reconcile_manifest_request_fees_v1(
    expected: &[ManifestRequestFeeV1],
    actual: &[ManifestRequestFeeV1],
    parent_root: Hash64,
    at: &PalwBlockContextV2,
    maximum_carriers: usize,
) -> Result<ManifestRequestFeeTotalsV1, ManifestRequestCarrierErrorV1> {
    use ManifestRequestCarrierErrorV1::{Charge, Context, Resource};
    if expected.len() > maximum_carriers || expected != actual {
        return Err(Context);
    }
    let bytes = (actual.len() as u64)
        .checked_mul(btree_node_envelope_bytes_v1::<TransactionId, ()>() as u64)
        .ok_or_else(|| Resource(NativeCoverageV1::overflow::<ManifestRequestFeeV1>("PMR1 fee reconciliation")))?;
    let _owner = reserve_native_pending_requests_v1(bytes).map_err(Resource)?;
    let mut carriers = std::collections::BTreeSet::new();
    let mut previous_ordinal = None;
    let mut totals = ManifestRequestFeeTotalsV1::default();
    for effect in actual {
        if effect.parent_root != parent_root
            || &effect.context.at != at
            || previous_ordinal.is_some_and(|prior| effect.context.accepted_ordinal <= prior)
            || !carriers.insert(effect.context.carrier)
        {
            return Err(Context);
        }
        effect.collectible_at_v1(parent_root, &effect.context, effect.actual_fee, effect.preceding_fee)?;
        previous_ordinal = Some(effect.context.accepted_ordinal);
        totals.actual_fee = totals.actual_fee.checked_add(effect.actual_fee).ok_or(Charge)?;
        totals.preceding_fee = totals.preceding_fee.checked_add(effect.preceding_fee).ok_or(Charge)?;
        totals.charge = totals.charge.checked_add(effect.charge).ok_or(Charge)?;
        totals.collectible_fee = totals.collectible_fee.checked_add(effect.collectible_fee).ok_or(Charge)?;
    }
    if totals.preceding_fee.checked_add(totals.charge).and_then(|n| n.checked_add(totals.collectible_fee)) != Some(totals.actual_fee) {
        return Err(Charge);
    }
    Ok(totals)
}
