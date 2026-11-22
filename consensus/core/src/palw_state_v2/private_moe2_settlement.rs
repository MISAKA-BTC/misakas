//! Private post-block accounting. The caller owns the block delta, the branch's
//! settlement book and the authenticated claim release. No public object dispatch.
use super::*;
use crate::block::Block;
use crate::config::params::Params;
use crate::palw_bond_budget_v1::{
    PALW_BUDGET_TABLE_CLAIMS_V1, PalwBudgetClaimRowV1, PalwBudgetOriginV1, palw_bond_budget_export_cap_v1,
};
use crate::palw_mode_v2::PalwConsensusMode;
use crate::palw_model_market_v1::moe2::{
    Moe2ModelAllocation, Moe2QuantityLimits, Moe2RegisteredModel, Moe2RewardSplit, quote_additional_reward,
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

// The ordinary clock can release window capacity independently of the
// outstanding liability. Every other funding field must survive until debit.
fn same_exec_reservation(original: &PalwBudgetClaimRowV1, current: &PalwBudgetClaimRowV1) -> bool {
    (original.in_window || !current.in_window) && *original == (PalwBudgetClaimRowV1 { in_window: original.in_window, ..*current })
}

fn exec_reservation_from_parent(prefix: &[PalwDeltaEntryV2], reservation: Hash64, current: &PalwBudgetClaimRowV1) -> bool {
    for entry in prefix {
        let PalwDeltaEntryV2::BondBudgetRow { table: PALW_BUDGET_TABLE_CLAIMS_V1, key, old, .. } = entry else { continue };
        if key.as_slice() == reservation.as_byte_slice() {
            return old
                .as_deref()
                .and_then(|bytes| borsh::from_slice::<PalwBudgetClaimRowV1>(bytes).ok())
                .is_some_and(|original| same_exec_reservation(&original, current));
        }
    }
    // No earlier write in the block changed this row.
    true
}

/// The private fold journals the one-shot EXEC debit, any market change,
/// and the receipt together. Replay must see that exact debit in this block.
pub(super) fn check_exec_settlement_debit_v1(
    entry: &PalwDeltaEntryV2,
    point: PalwBlockContextV2,
    prefix: &[PalwDeltaEntryV2],
) -> Result<(), PalwStateV2Error> {
    let (record, keyed) = match entry {
        PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record } => (record, false),
        PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { record, .. } => (record, true),
        _ => return Ok(()),
    };
    let mismatch = || PalwStateV2Error::DeltaMismatch("private MOE2 EXEC settlement lacks its same-block debit");
    if record.position.point != point {
        return Err(mismatch());
    }
    if !record.has_matching_reservation_key() {
        return Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC slot does not bind the consumed reservation"));
    }
    let allocation = &record.reward.allocation;
    let total = record.total_reward().ok_or_else(mismatch)?;
    let (through_debit, before_market) = match allocation.market {
        Some(quote) if quote.split.purchase != 0 || quote.split.liquidity != 0 => {
            let Some((PalwDeltaEntryV2::ModelMarket { key, old: Some(old), new: Some(new) }, preceding)) = prefix.split_last() else {
                return Err(mismatch());
            };
            if *key != record.model || *new != quote.after {
                return Err(mismatch());
            }
            (preceding, Some(*old))
        }
        market => (prefix, market.map(|quote| quote.after)),
    };
    if let Some((market, quote)) = before_market.zip(allocation.market) {
        let bounds = Moe2QuantityLimits {
            max_reward: total,
            max_purchase_sompi: quote.split.purchase,
            min_out: quote.purchased_units,
            max_purchase_units: quote.purchased_units,
            max_lp_sompi: quote.split.liquidity,
            max_lp_units: quote.liquidity_units,
        };
        if quote_additional_reward(&market, total, bounds) != Ok(quote)
            || record.permanent_liquidity.is_some_and(|lp| lp.before != market)
        {
            return Err(mismatch());
        }
    }
    let Some((debit, before_debit)) = through_debit.split_last() else { return Err(mismatch()) };
    let PalwDeltaEntryV2::BondBudgetRow { table: PALW_BUDGET_TABLE_CLAIMS_V1, key, old: Some(old), new: Some(new) } = debit else {
        return Err(mismatch());
    };
    if key.as_slice() != record.reservation.as_byte_slice() {
        return Err(mismatch());
    }
    let before = borsh::from_slice::<PalwBudgetClaimRowV1>(old).map_err(|_| mismatch())?;
    let after = borsh::from_slice::<PalwBudgetClaimRowV1>(new).map_err(|_| mismatch())?;
    let expected = before.after_exec_slot_reward(total).ok();
    if before.model.map(|(model, _)| model) != Some(record.model)
        || matches!(before.origin, PalwBudgetOriginV1::Moe2Exec { expires_at } if expires_at <= point.daa_score)
        || expected != Some(after)
    {
        return Err(mismatch());
    }
    if keyed && !exec_reservation_from_parent(before_debit, record.reservation, &before) {
        return Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC reservation differs from selected parent"));
    }
    Ok(())
}

impl PalwChainStateV2 {
    pub fn private_moe2_exec_settlement(&self, claim: &Hash64) -> Option<&Moe2NativeExecSettlement> {
        self.private_moe2_exec_settlements.get(claim)
    }

    pub fn private_moe2_work_claim(&self, work: &Hash64) -> Option<&Hash64> {
        self.private_moe2_work_uses.get(work)
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
        self.settle_private_moe2_funding(config, book, delta, claim, position, release, None, None)
    }

    /// Legacy private settlement for a claim without a canonical work identity.
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
        block: &Block,
        worker_start: usize,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        self.settle_private_moe2_exec(config, book, delta, claim, position, release, slot, None, payees, block, worker_start)
    }

    /// Settle an independently authenticated award on a private MOE2 genesis.
    /// The block owner derives `work` from the winning claim's authenticated
    /// EXEC input and resolves equivalent profiles to the same canonical key.
    /// It also validates the other coinbase outputs, the worker plan and its
    /// placement before entering this accounting fold.
    pub fn settle_private_moe2_exec_work(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
        slot: &Moe2ExecSlot,
        work: Hash64,
        payees: Moe2ExecWorkerPayees<'_>,
        block: &Block,
        worker_start: usize,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        self.settle_private_moe2_exec(config, book, delta, claim, position, release, slot, Some(work), payees, block, worker_start)
    }

    /// Admit and settle an authenticated award using its selected-parent
    /// reservation. The block owner supplies the authenticated price-anchor
    /// state, signed slot, claim/work binding and worker allocation. The
    /// candidate header and ordinary delta must both name the supplied parent.
    pub fn settle_private_moe2_exec_award(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        parent: &PalwChainStateV2,
        quote_anchor: &PalwChainStateV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
        slot: &Moe2ExecSlot,
        work: Hash64,
        payees: Moe2ExecWorkerPayees<'_>,
        block: &Block,
        worker_start: usize,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let parent_point = parent.last_point().ok_or(UnknownClaimContext)?;
        let anchor_point = quote_anchor.last_point().ok_or(UnknownClaimContext)?;
        if anchor_point.block != slot.quote.quote_anchor
            || anchor_point.blue_score > parent_point.blue_score
            || !(anchor_point.daa_score..=parent_point.daa_score).contains(&slot.close_daa)
            || !(slot.quote.valid_from..=slot.quote.deadline).contains(&anchor_point.daa_score)
            || !delta.entries.iter().any(|entry| {
                matches!(entry, PalwDeltaEntryV2::LastPoint { old: Some(old), new: Some(new) }
                    if old == parent_point && *new == delta.point)
            })
            || crate::palw_state_proof_v1::state_root_of_pinned_header_v1(&block.header, delta.point.block)
                .map_err(|_| UnknownClaimContext)?
                != parent.state_root()
        {
            return Err(UnknownClaimContext);
        }
        if self.private_moe2_work_claim(&work).is_some() {
            return Err(WorkAlreadyConsumed);
        }
        let class = quote_anchor.class(&slot.quote.model).ok_or(UnknownRegistration)?;
        let historical = Moe2RegisteredModel {
            model: slot.quote.model,
            artifact_root: class.artifact_root,
            registration_daa: class.registered_daa,
            owner: class.registrant_bond,
            allocation: slot.quote.allocation_at(config, quote_anchor, anchor_point.daa_score)?,
        };
        if book.model(&slot.quote.model).ok_or(UnknownRegistration)?.allocation == Moe2ModelAllocation::Core
            && historical.allocation != Moe2ModelAllocation::Core
        {
            return Err(UnknownRegistration);
        }
        let budget = parent.bond_budget().ok_or(ReservationMismatch)?;
        let key = slot.reservation_key();
        let original = budget.claim_row(&key).ok_or(ReservationMismatch)?;
        let current = self.bond_budget().and_then(|budget| budget.claim_row(&key)).ok_or(ReservationMismatch)?;
        if !same_exec_reservation(original, current) {
            return Err(ReservationMismatch);
        }
        book.admit_exec_slot(claim, slot, budget, |_, _| Some(historical))?;
        let result =
            self.settle_private_moe2_exec_work(config, book, delta, claim, position, release, slot, work, payees, block, worker_start);
        if result.is_err() {
            book.revert_exec_admission(claim, slot).expect("failed settlement leaves the new admission intact");
        }
        result
    }

    /// Disconnect a settlement whose admission was created in the award block.
    pub fn revert_private_moe2_exec_award(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        slot: &Moe2ExecSlot,
        undo: &Moe2SettlementUndo,
    ) -> Result<(), Moe2SettlementError> {
        if !book.has_exec_binding(undo.claim(), slot) {
            return Err(Moe2SettlementError::UndoMismatch);
        }
        self.revert_private_moe2_exec_slot(config, book, delta, slot, undo)?;
        book.revert_exec_admission(undo.claim(), slot).expect("settlement rollback restores the verified admission");
        Ok(())
    }

    fn settle_private_moe2_exec(
        &mut self,
        config: &Params,
        book: &mut Moe2SettlementBook,
        delta: &mut PalwStateDeltaV2,
        claim: Hash64,
        position: Moe2Position,
        release: Moe2WorkerRelease,
        slot: &Moe2ExecSlot,
        work: Option<Hash64>,
        payees: Moe2ExecWorkerPayees<'_>,
        block: &Block,
        worker_start: usize,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        use Moe2SettlementError::{UnknownClaimContext, WorkerOutputsMismatch};
        if position.point != delta.point
            || self.last_point() != Some(&delta.point)
            || (block.hash(), block.header.daa_score, block.header.blue_score)
                != (delta.point.block, delta.point.daa_score, delta.point.blue_score)
            || crate::hashing::header::hash(&block.header) != block.hash()
        {
            return Err(UnknownClaimContext);
        }
        let expected = self.private_moe2_exec_worker_outputs(config, position.point.daa_score, release, payees)?;
        let coinbase = block.transactions.first().filter(|tx| tx.is_coinbase()).ok_or(WorkerOutputsMismatch)?;
        let worker_end = worker_start.checked_add(expected.len()).ok_or(WorkerOutputsMismatch)?;
        if coinbase.outputs.get(worker_start..worker_end) != Some(expected.as_slice())
            || crate::merkle::calc_hash_merkle_root(block.transactions.iter()) != block.header.hash_merkle_root
        {
            return Err(WorkerOutputsMismatch);
        }
        self.settle_private_moe2_funding(config, book, delta, claim, position, release, Some(slot), work)
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
        let work = match delta.entries.last() {
            Some(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { work, .. }) => {
                if self.private_moe2_work_claim(work) != Some(&undo.claim()) {
                    return Err(UndoMismatch);
                }
                suffix.push(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { work: *work, record: Box::new(record) });
                Some(*work)
            }
            _ => {
                if self.private_moe2_work_uses.values().any(|claim| *claim == undo.claim()) {
                    return Err(UndoMismatch);
                }
                suffix.push(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record: Box::new(record) });
                None
            }
        };
        if !delta.entries.ends_with(&suffix) {
            return Err(UndoMismatch);
        }

        let budget = self.bond_budget.as_mut().ok_or(UndoMismatch)?;
        book.revert_exec_settlement(slot, undo, budget, &mut Vec::new())?;
        if let Some(market) = before {
            self.model_markets.insert(model, market);
        }
        if let Some(work) = work {
            self.private_moe2_work_uses.remove(&work);
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
        work: Option<Hash64>,
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
        if work.is_some_and(|key| self.private_moe2_work_claim(&key).is_some()) {
            return Err(WorkAlreadyConsumed);
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
        let reservation = slot.map_or_else(|| terms.reservation_key(), Moe2ExecSlot::reservation_key);
        let row = budget.claim_row(&reservation).ok_or(ReservationMismatch)?;
        if work.is_some() && !exec_reservation_from_parent(&delta.entries, reservation, row) {
            return Err(ReservationMismatch);
        }
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
            let entry = if let Some(work) = work {
                self.private_moe2_work_uses.insert(work, claim);
                PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { work, record: Box::new(record) }
            } else {
                PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record: Box::new(record) }
            };
            delta.entries.push(entry);
        }
        Ok((reward, undo))
    }
}
