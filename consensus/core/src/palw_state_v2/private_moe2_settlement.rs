//! Private post-block accounting. The caller owns the block delta, the branch's
//! settlement book and the authenticated claim release. No public object dispatch.
use super::*;
use crate::config::params::Params;
use crate::palw_bond_budget_v1::palw_bond_budget_export_cap_v1;
use crate::palw_mode_v2::PalwConsensusMode;
use crate::palw_model_market_v1::moe2::{
    Moe2ModelAllocation, Moe2RewardSplit,
    settlement::{
        Moe2ExecSlot, Moe2NativeExecSettlement, Moe2Position, Moe2SettledReward, Moe2SettlementBook, Moe2SettlementError,
        Moe2SettlementFunding, Moe2SettlementUndo, Moe2WorkerRelease,
    },
};
use crate::palw_settlement_liability_v1::PalwSettlementTermsSourcedV1;
use crate::tx::TransactionOutput;

/// Recipient bonds and amounts selected by the accepted claim's worker plan.
/// The block owner supplies the authenticated Miner and Panel legs in plan order.
#[derive(Clone, Copy)]
pub struct Moe2ExecWorkerPayees<'a> {
    pub miner: PalwBondKeyV2,
    pub panel: &'a [(PalwBondKeyV2, u64)],
}

impl PalwChainStateV2 {
    pub fn private_moe2_exec_settlement(&self, claim: &Hash64) -> Option<&Moe2NativeExecSettlement> {
        self.private_moe2_exec_settlements.get(claim)
    }

    /// Render the immediately payable worker section of the award coinbase.
    /// Construction and settlement compare use the same native payout payloads.
    pub fn private_moe2_exec_worker_outputs(
        &self,
        config: &Params,
        award_daa: u64,
        release: Moe2WorkerRelease,
        payees: Moe2ExecWorkerPayees<'_>,
    ) -> Result<Vec<TransactionOutput>, Moe2SettlementError> {
        use Moe2SettlementError::{UnknownClaimContext, WorkerOutputsMismatch};
        if config.private_moe2_registration_from().is_none_or(|from| award_daa < from) {
            return Err(UnknownClaimContext);
        }
        let count = usize::from(release.miner_now > 0).saturating_add(payees.panel.len());
        let mut recipients = BTreeSet::from([payees.miner]);
        if count > PALW_V2_MAX_PAYOUTS_PER_BLOCK
            || payees.panel.iter().any(|(bond, amount)| !recipients.insert(*bond) || *amount == 0)
            || payees.panel.iter().try_fold(0u64, |sum, (_, amount)| sum.checked_add(*amount)) != Some(release.panel_now)
        {
            return Err(WorkerOutputsMismatch);
        }
        let mut outputs = Vec::with_capacity(count);
        for (bond, amount) in std::iter::once((payees.miner, release.miner_now)).chain(payees.panel.iter().copied()) {
            let row = self.bond(&bond).ok_or(WorkerOutputsMismatch)?;
            if amount != 0 {
                outputs.push(TransactionOutput::new(
                    amount,
                    crate::mldsa87_primitives::p2pkh_mldsa87_spk(&row.payout_payload.as_bytes()),
                ));
            }
        }
        Ok(outputs)
    }

    /// Append both monetary legs at the end of this block's current delta.
    /// The budget engine sources its own funds and liability from the post-state
    /// at this position, including other activity earlier in the same block.
    pub fn settle_private_moe2(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        self.settle_private_moe2_funding(config, book, delta, claim, position, release, None)
    }

    /// Accounting entry point for the private block owner after validating the
    /// new-profile claim, its unique EXEC slot and its award in this delta.
    /// The worker section is taken from the same block's coinbase. The owning
    /// block validates its other outputs and authenticates the recipient plan.
    pub fn settle_private_moe2_exec_slot(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
        slot: &Moe2ExecSlot,
        payees: Moe2ExecWorkerPayees<'_>,
        coinbase_worker_outputs: &[TransactionOutput],
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        if self.private_moe2_exec_worker_outputs(config, position.point.daa_score, release, payees)? != coinbase_worker_outputs {
            return Err(Moe2SettlementError::WorkerOutputsMismatch);
        }
        self.settle_private_moe2_funding(config, book, delta, claim, position, release, Some(slot))
    }

    /// Remove an EXEC settlement from this block before disconnecting its
    /// ordinary native delta. Unwind multiple settlements newest first.
    /// A journal from another branch, or a later write in this block, must
    /// leave both the native state and the accounting book untouched.
    pub fn revert_private_moe2_exec_slot(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        slot: &Moe2ExecSlot,
        undo: &Moe2SettlementUndo,
    ) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::UndoMismatch;
        if config.private_moe2_registration_from().is_none_or(|from| delta.point.daa_score < from)
            || self.last_point() != Some(&delta.point)
            || delta.point != undo.point()
        {
            return Err(UndoMismatch);
        }
        let reward = *book.entitlement(&undo.claim()).ok_or(UndoMismatch)?;
        let model = slot.quote.model;
        let before = undo.market_before();
        let after = reward.allocation.market.map(|quote| quote.after);
        if self.model_market(&model) != after.as_ref() {
            return Err(UndoMismatch);
        }
        let mut suffix = vec![PalwDeltaEntryV2::from(undo.exec_debit(slot)?)];
        if before != after {
            suffix.push(PalwDeltaEntryV2::ModelMarket { key: model, old: before, new: after });
        }
        let record = Moe2NativeExecSettlement::from_settlement(slot, book, undo, reward);
        if self.private_moe2_exec_settlement(&undo.claim()) != Some(&record) {
            return Err(UndoMismatch);
        }
        suffix.push(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record: Box::new(record) });
        if !delta.entries.ends_with(&suffix) {
            return Err(UndoMismatch);
        }

        let budget = self.bond_budget.as_mut().ok_or(UndoMismatch)?;
        book.revert_exec_settlement(slot, undo, budget, &mut Vec::new())?;
        if let Some(market) = before {
            self.model_markets.insert(model, market);
        }
        self.private_moe2_exec_settlements.remove(&undo.claim());
        delta.entries.truncate(delta.entries.len() - suffix.len());
        Ok(())
    }

    fn settle_private_moe2_funding(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
        slot: Option<&Moe2ExecSlot>,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let from = config.private_moe2_registration_from().ok_or(UnknownClaimContext)?;
        let PalwConsensusMode::ConsensusV2(bundle) = &config.palw_consensus_mode else { return Err(UnknownClaimContext) };
        if position.point != delta.point
            || self.last_point() != Some(&delta.point)
            || !bundle.state.bond_budget_active_at(delta.point.daa_score)
        {
            return Err(UnknownClaimContext);
        }
        if book.entitlement(&claim).is_some() || (slot.is_some() && self.private_moe2_exec_settlement(&claim).is_some()) {
            return Err(AlreadySettled);
        }
        let terms = *book.admitted_terms(&claim).ok_or(UnknownClaimContext)?;
        let model = *book.model(&terms.model).ok_or(UnknownRegistration)?;
        let class = self.class(&terms.model).ok_or(UnknownRegistration)?;
        if terms.valid_from < from
            || (matches!(model.allocation, Moe2ModelAllocation::Additional(_)) && class.registered_daa < from)
            || (class.artifact_root, class.registered_daa, class.registrant_bond)
                != (terms.artifact_root, model.registration_daa, model.owner)
        {
            return Err(UnknownRegistration);
        }
        let (before, market_export) = match model.allocation {
            Moe2ModelAllocation::Core => (None, 0),
            Moe2ModelAllocation::Additional(market) => {
                let split = Moe2RewardSplit::additional(release.reward);
                (Some(market), split.purchase + split.liquidity)
            }
        };
        if self.model_market(&terms.model) != before.as_ref() {
            return Err(UnknownRegistration);
        }
        let budget = self.bond_budget().ok_or(ReservationMismatch)?;
        let policy = &bundle.state.bond_budget().ok_or(ReservationMismatch)?.policy;
        if budget.header.policy_digest != policy.digest() {
            return Err(ReservationMismatch);
        }
        let row = budget
            .claim_row(&slot.map_or_else(|| terms.reservation_key(), Moe2ExecSlot::reservation_key))
            .ok_or(ReservationMismatch)?;
        let (bond_collateral, other_live_liability, bond_locked_until) =
            self.bond_liability_terms_v1(&terms.funding_bond, row.liability_sompi);
        let paid_now =
            market_export.checked_add(release.miner_now).and_then(|v| v.checked_add(release.panel_now)).ok_or(ReservationMismatch)?;
        let funding = Moe2SettlementFunding {
            claim,
            position,
            liability: PalwSettlementTermsSourcedV1 {
                reserved_reward: Ok(row.reserved.reward_sompi),
                consumed_reward: Ok(row.consumed.reward_sompi),
                draw: release.reward,
                payment: release.reward,
                paid_now,
                exported_before: Ok(row.consumed.reward_sompi),
                export_cap: Ok(palw_bond_budget_export_cap_v1(policy, row.liability_sompi)),
                liability_sompi: Ok(row.liability_sompi),
                bond_collateral,
                other_live_liability,
                bond_locked_until,
                liability_until: Ok(release.liability_until),
                signed_hard_deadline: Ok(release.signed_hard_deadline),
                material_ttl_until: Ok(release.material_ttl_until),
                vest_until: release.vest_until,
                retire_not_before: Ok(release.retire_not_before),
                now: delta.point.daa_score,
            },
            miner_now: release.miner_now,
            panel_now: release.panel_now,
        };
        let mut writes = Vec::new();
        let budget = self.bond_budget.as_mut().ok_or(ReservationMismatch)?;
        let (reward, undo) =
            book.settle_from_reservation(claim, funding.position, terms.quote_anchor, &funding, budget, &mut writes, slot)?;
        delta.entries.extend(writes.into_iter().map(PalwDeltaEntryV2::from));
        if let Some(q) = reward.allocation.market {
            if before != Some(q.after) {
                self.model_markets.insert(terms.model, q.after);
                delta.entries.push(PalwDeltaEntryV2::ModelMarket { key: terms.model, old: before, new: Some(q.after) });
            }
        }
        if let Some(slot) = slot {
            let record = Moe2NativeExecSettlement::from_settlement(slot, book, &undo, reward);
            self.private_moe2_exec_settlements.insert(claim, record.clone());
            delta.entries.push(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record: Box::new(record) });
        }
        Ok((reward, undo))
    }
}
