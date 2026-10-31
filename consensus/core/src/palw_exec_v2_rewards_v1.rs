//! EXEC's additional reward legs retain their funding role, root and fixed payee through the collectible window.
//! Native root/Panel legs remain in the native vesting row, extended through that same window.

use super::*;
use crate::palw_work_slice_v2::{
    PalwExecV2RewardActionV1 as Action, PalwExecV2RewardFundV1 as Fund, PalwExecV2RewardRowV1 as Row, PalwWorkRootPhaseV2,
};

/// A native consumer's material service cannot silently extend the immutable
/// EXEC terms authorized by the root declaration and each worker's job nonce.
/// A longer proposed root expiry must be signed before admission instead.
pub(super) fn require_paid_exec_material_window_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    root: &Hash64,
    expiry: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(terms) = state.paid_consumer_terms_v1(root) else { return Ok(()) };
    let authorized = match &state.kernel_route {
        Some(kernel) => {
            super::palw_kernel_route_fold_v1::exec_liability_floor_v1(expiry, params.window_court(), &kernel.header.policy)?
        }
        None => expiry.checked_add(params.window_court()).ok_or(PalwStateV2Error::Overflow("paid EXEC material horizon"))?,
    };
    if terms.material_until > authorized {
        return Err(PalwStateV2Error::ExecV2Refused(format!(
            "paid consumer requires material through {}, beyond the signed EXEC horizon {authorized}",
            terms.material_until
        )));
    }
    Ok(())
}

/// The bound base floor and every retained dependency horizon. No dependency may disappear during its proof window.
pub(super) fn horizon(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    root_id: &Hash64,
) -> Result<Option<u64>, PalwStateV2Error> {
    let Some(root) = state.exec_v2.roots.get(root_id) else { return Ok(None) };
    let mut until = match &state.kernel_route {
        Some(kernel) => {
            super::palw_kernel_route_fold_v1::exec_liability_floor_v1(root.expiry_daa, params.window_court(), &kernel.header.policy)?
        }
        None => root.expiry_daa.checked_add(params.window_court()).ok_or(PalwStateV2Error::Overflow("EXEC liability horizon"))?,
    };
    if let Some(kernel) = &state.kernel_route {
        let claims = std::iter::once(root.prefix_claim)
            .chain(state.exec_v2.slices.range((*root_id, 0)..=(*root_id, u32::MAX)).map(|(_, row)| row.evidence_root));
        for claim in claims {
            let row: misaka_palw_kernel::ledger::ClaimRowV1 = kernel
                .rows
                .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&claim.as_bytes()).expect("a digest serializes")))
                .and_then(|bytes| borsh::from_slice(bytes).ok())
                .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("an EXEC dependency horizon is missing".into()))?;
            until =
                until
                    .max(row.liability_until.ok_or_else(|| {
                        PalwStateV2Error::CarriageInconsistent("an EXEC dependency has no liability horizon".into())
                    })?);
        }
    }
    Ok(Some(until))
}

/// Native disputes cannot unilaterally extend the consumer-frozen kernel obligation.
/// Use the shortest dependency, not the reward reader's maximum: one longer-lived
/// dependency cannot supply another executor's collateral or public material.
pub(super) fn require_native_session_window(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    root_id: &Hash64,
    required_until: u64,
) -> Result<(), PalwStateV2Error> {
    crate::palw_panel_lifetime_v2::require_panel_session_window_v2(state, params, root_id, required_until)?;
    state.require_paid_consumer_backstop_v1(root_id, required_until)?;
    let root = state.exec_v2.roots.get(root_id);
    let mut bound = match root {
        Some(root) => match &state.kernel_route {
            Some(kernel) => super::palw_kernel_route_fold_v1::exec_liability_floor_v1(
                root.expiry_daa,
                params.window_court(),
                &kernel.header.policy,
            )?,
            None => {
                root.expiry_daa.checked_add(params.window_court()).ok_or(PalwStateV2Error::Overflow("EXEC native session horizon"))?
            }
        },
        None => match state.exec_v2_rewards.get(root_id) {
            Some(receipt) => receipt.liability_until,
            None => return Ok(()),
        },
    };
    if let (Some(root), Some(kernel)) = (root, &state.kernel_route) {
        let claims = std::iter::once(root.prefix_claim)
            .chain(state.exec_v2.slices.range((*root_id, 0)..=(*root_id, u32::MAX)).map(|(_, row)| row.evidence_root));
        for claim in claims {
            let row: misaka_palw_kernel::ledger::ClaimRowV1 = kernel
                .rows
                .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&claim.as_bytes()).expect("a digest serializes")))
                .and_then(|bytes| borsh::from_slice(bytes).ok())
                .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("an EXEC session dependency is missing".into()))?;
            bound = bound.min(row.liability_until.ok_or_else(|| {
                PalwStateV2Error::CarriageInconsistent("an EXEC session dependency has no liability horizon".into())
            })?);
        }
    }
    if required_until > bound {
        return Err(PalwStateV2Error::ExecV2Refused(format!(
            "native session requires liability through {required_until}, beyond the EXEC bound {bound}"
        )));
    }
    Ok(())
}

pub(super) fn require_native_da_window(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    root_id: &Hash64,
    deadline: u64,
    opened_daa: u64,
) -> Result<(), PalwStateV2Error> {
    if state.paid_consumer_contract_v1(root_id).is_some() {
        let through = deadline
            .checked_add(params.window_challenge_at(opened_daa))
            .ok_or(PalwStateV2Error::Overflow("paid consumer DA whole path"))?;
        state.require_paid_consumer_dispute_v1(root_id, opened_daa, through)?;
    }
    // Preserve ordinary native admission, including its saturation behavior.
    if !state.exec_v2.roots.contains_key(root_id) && !state.exec_v2_rewards.contains_key(root_id) {
        return Ok(());
    }
    let until = deadline
        .checked_add(params.window_challenge_at(opened_daa))
        .ok_or(PalwStateV2Error::Overflow("EXEC native DA liability horizon"))?;
    require_native_session_window(state, params, root_id, until)
}

impl PalwChainStateV2 {
    /// A retired root's unpaid receipt still occupies its bounded execution slot.
    pub(super) fn exec_v2_retained_slots_v1(&self) -> usize {
        self.exec_v2.roots.len() + self.exec_v2_rewards.keys().filter(|id| !self.exec_v2.roots.contains_key(*id)).count()
    }
    pub fn exec_v2_liability_until_v1(&self, params: &PalwStateParamsV2, root: &Hash64) -> Result<Option<u64>, PalwStateV2Error> {
        let until = horizon(self, params, root)?;
        Ok(until.max(self.exec_v2_rewards.get(root).map(|row| row.liability_until)))
    }
    pub fn exec_v2_reward_vesting_v1(&self, root: &Hash64) -> Option<&Row> {
        self.exec_v2_rewards.get(root)
    }
    pub fn exec_v2_reward_vesting_iter_v1(&self) -> impl Iterator<Item = (&Hash64, &Row)> {
        self.exec_v2_rewards.iter()
    }

    /// This executor's earliest DAA release (shared native clocks can still delay it).
    pub fn exec_v2_reward_hold_after_v1(&self, root: &Hash64, bond: &PalwBondKeyV2) -> Option<u64> {
        let row = self.exec_v2_rewards.get(root)?;
        row.legs
            .keys()
            .any(|(payee, _)| payee == bond)
            .then(|| row.liability_until.saturating_add(1).max(self.exec_v2_reward_holds.get(&(*root, *bond)).copied().unwrap_or(0)))
    }

    pub fn exec_v2_reward_clock_live_v1(
        &self,
        params: &PalwStateParamsV2,
        root: &Hash64,
        bond: &PalwBondKeyV2,
        daa: u64,
        raw: Option<u64>,
    ) -> Option<bool> {
        self.exec_v2_reward_hold_after_v1(root, bond).map(|_| actor_unmatured(self, params, root, bond, daa, raw))
    }

    /// The exact additional moves the fold will make, optionally simulating its first queue drain.
    pub fn exec_v2_reward_release_moves_v1(&self, daa: u64, after_drain: bool) -> Vec<(Hash64, (PalwBondKeyV2, Fund), PalwPayoutV2)> {
        release_plan(self, daa, after_drain).0
    }

    /// Additional new keys ahead of native vesting, after this block's drain (or its next-block simulation).
    pub fn exec_v2_reward_release_keys_v1(&self, daa: u64, after_drain: bool) -> usize {
        release_plan(self, daa, after_drain).1
    }
}

/// Maintain the derived actor index on the fold and on delta replay.
pub(super) fn index_replace(state: &mut PalwChainStateV2, root: Hash64, old: Option<&Row>, new: Option<&Row>) {
    if let Some(old) = old {
        for (bond, _) in old.legs.keys() {
            if let Some(roots) = state.exec_v2_reward_payees.get_mut(bond) {
                roots.remove(&root);
                if roots.is_empty() {
                    state.exec_v2_reward_payees.remove(bond);
                }
            }
        }
    }
    if let Some(new) = new {
        for (bond, _) in new.legs.keys() {
            state.exec_v2_reward_payees.entry(*bond).or_default().insert(root);
        }
    }
}

/// B-3 uses the obligation clocks, not DA/halt/payout-queue presence. A spent native latch is irreversible.
fn native_unmatured(state: &PalwChainStateV2, params: &PalwStateParamsV2, root: &Hash64, daa: u64, raw: Option<u64>) -> bool {
    let Some(row) = state.vesting.get(root) else { return false };
    row.matured_at.is_none()
        && crate::palw_vesting_v1::palw_vesting_lock_is_live_v1(
            row.expiry_daa,
            row.settled_at_final,
            daa,
            state.settled_attempt_finals,
            palw_second_clock_depth_v1(raw, &state.recent_anchor_daas, daa, params.window_court()),
            params.window_court(),
        )
}

fn actor_unmatured(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    root: &Hash64,
    bond: &PalwBondKeyV2,
    daa: u64,
    raw: Option<u64>,
) -> bool {
    let Some(row) = state.exec_v2_rewards.get(root) else { return true }; // An index without its receipt must hold safely.
    daa <= row.liability_until
        || state.exec_v2_reward_holds.get(&(*root, *bond)).is_some_and(|after| daa < *after)
        || native_unmatured(state, params, root, daa, raw)
}

/// With R-core+ off, also retain the native payees of actual forced EXEC vesting (including empty receipts).
pub(super) fn bond_unmatured(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    daa: u64,
    raw: Option<u64>,
    include_native: bool,
) -> bool {
    if state.exec_v2_rewards.is_empty() {
        return false;
    }
    state
        .exec_v2_reward_payees
        .get(bond)
        .is_some_and(|roots| roots.iter().any(|root| actor_unmatured(state, params, root, bond, daa, raw)))
        || (include_native
            && state
                .vesting_unlatched_of_payee(bond)
                .any(|(_, root)| state.exec_v2_rewards.contains_key(&root) && native_unmatured(state, params, &root, daa, raw)))
}

/// Tier-freeze only still-unmatured executor funds. Other executors on this root keep their own clocks.
pub(super) fn freeze_bond(
    builder: &mut TransitionBuilder<'_>,
    bond: PalwBondKeyV2,
    daa: u64,
    lift: u64,
) -> Result<(), PalwStateV2Error> {
    let raw = palw_settled_anchor_depth_v1(builder.extras);
    let roots: Vec<_> = builder.state.exec_v2_reward_payees.get(&bond).into_iter().flatten().copied().collect();
    for root in roots {
        let row = &builder.state.exec_v2_rewards[&root];
        let after =
            row.liability_until.saturating_add(1).max(builder.state.exec_v2_reward_holds.get(&(root, bond)).copied().unwrap_or(0));
        if lift > after && actor_unmatured(&builder.state, builder.params, &root, &bond, daa, raw) {
            builder.write_exec_reward_hold((root, bond), Some(lift))?;
        }
    }
    Ok(())
}

fn burn_actor(
    builder: &mut TransitionBuilder<'_>,
    root: Hash64,
    bond: PalwBondKeyV2,
    action: Action,
) -> Result<u64, PalwStateV2Error> {
    let Some(mut row) = builder.state.exec_v2_rewards.get(&root).cloned() else { return Ok(0) };
    let issuer = builder
        .state
        .bond_budget
        .as_ref()
        .and_then(|b| b.claim_row(&super::palw_bond_budget_fold_v1::native_income_id_v1(root, &bond, 0)))
        .is_some();
    builder.cancel_native_recipient_v1(root, bond, if issuer { 0 } else { 2 })?;
    let own: Vec<_> = row.legs.iter().filter(|((payee, _), _)| *payee == bond).map(|(who, leg)| (*who, *leg)).collect();
    let mut taken = 0u64;
    let mut c = builder.state.vesting_counters;
    for (who, payout) in own {
        taken = taken.checked_add(payout.amount).ok_or(PalwStateV2Error::Overflow("EXEC forfeiture"))?;
        c.burned = c.burned.checked_add(u128::from(payout.amount)).ok_or(PalwStateV2Error::Overflow("EXEC vesting burned"))?;
        row.legs.remove(&who);
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
            claim_id: root,
            bond,
            fund: who.1,
            action,
            payout,
        })?;
    }
    // During Final the kernel fee is credited before the native row exists. Preserve the allocation receipt
    // so a forfeited fee cannot erase the later slice/native accounting in this same atomic settlement.
    let final_in_progress = builder.state.claims.get(&root).is_some_and(|claim| !claim.phase.is_terminal());
    builder.write_exec_reward(
        root,
        if row.legs.is_empty() && !builder.state.vesting.contains_key(&root) && !final_in_progress { None } else { Some(row) },
    )?;
    builder.write_vesting_counters(c)?;
    Ok(taken)
}

/// AG-2 burns every unpaid fund of this actor, including spent clocks waiting for queue room, exactly once.
pub(super) fn forfeit_bond(
    builder: &mut TransitionBuilder<'_>,
    bond: PalwBondKeyV2,
    offence_id: Hash64,
) -> Result<u64, PalwStateV2Error> {
    let roots: Vec<_> = builder.state.exec_v2_reward_payees.get(&bond).into_iter().flatten().copied().collect();
    let mut taken = 0u64;
    for root in roots {
        taken = taken.saturating_add(burn_actor(builder, root, bond, Action::Forfeited { offence_id })?);
    }
    Ok(taken)
}

pub(super) fn start(builder: &mut TransitionBuilder<'_>, root: Hash64, allocation: u64, daa: u64) -> Result<(), PalwStateV2Error> {
    let until = horizon(&builder.state, builder.params, &root)?
        .ok_or_else(|| PalwStateV2Error::ExecV2Refused("reward has no EXEC root".into()))?;
    builder.write_exec_reward(
        root,
        Some(Row { final_daa: daa, liability_until: until, allocation_sompi: allocation, legs: BTreeMap::new() }),
    )?;
    Ok(())
}

pub(super) fn add(
    builder: &mut TransitionBuilder<'_>,
    root: Hash64,
    bond: PalwBondKeyV2,
    fund: Fund,
    amount: u64,
) -> Result<(), PalwStateV2Error> {
    if amount == 0 {
        return Ok(());
    }
    let payload = builder.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.payout_payload;
    let mut row = builder
        .state
        .exec_v2_rewards
        .get(&root)
        .cloned()
        .ok_or_else(|| PalwStateV2Error::ExecV2Refused("EXEC reward allocation is missing".into()))?;
    let leg = row.legs.entry((bond, fund)).or_insert(PalwPayoutV2 { payload, amount: 0 });
    if leg.payload != payload {
        return Err(PalwStateV2Error::ExecV2Refused("EXEC reward payee changed during Final".into()));
    }
    leg.amount = leg.amount.checked_add(amount).ok_or(PalwStateV2Error::Overflow("EXEC vested reward"))?;
    if row.total_sompi_u128() > u128::from(row.allocation_sompi) {
        return Err(PalwStateV2Error::ExecV2Refused("EXEC vesting exceeds its allocation".into()));
    }
    builder.write_exec_reward(root, Some(row))?;
    let mut c = builder.state.vesting_counters;
    c.created = c.created.checked_add(u128::from(amount)).ok_or(PalwStateV2Error::Overflow("EXEC vesting created"))?;
    builder.write_vesting_counters(c)?;
    builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
        claim_id: root,
        bond,
        fund,
        action: Action::Created,
        payout: PalwPayoutV2 { payload, amount },
    })?;
    if let Some(freeze) = builder.state.bond_freezes.get(&bond).copied() {
        if freeze.final_ {
            let taken = burn_actor(builder, root, bond, Action::Forfeited { offence_id: freeze.offence_key })?;
            builder.write_bond_freeze_v1(
                bond,
                Some(crate::palw_aggregate_liability_v1::PalwBondFreezeV1 {
                    forfeited_sompi: freeze.forfeited_sompi.saturating_add(taken),
                    ..freeze
                }),
            )?;
        } else if let Some(lift) = freeze.lifts_at_v1(builder.params.window_court()) {
            freeze_bond(builder, bond, builder.state.exec_v2_rewards[&root].final_daa, lift)?;
        }
    }
    Ok(())
}

pub(super) fn finish(builder: &mut TransitionBuilder<'_>, root: Hash64) -> Result<(), PalwStateV2Error> {
    if builder.state.exec_v2_rewards.get(&root).is_some_and(|row| row.legs.is_empty()) && !builder.state.vesting.contains_key(&root) {
        builder.write_exec_reward(root, None)?;
    }
    Ok(())
}

pub(super) fn burn(builder: &mut TransitionBuilder<'_>, root: Hash64) -> Result<(), PalwStateV2Error> {
    let Some(row) = builder.state.exec_v2_rewards.get(&root).cloned() else { return Ok(()) };
    for (bond, _) in row.legs.keys() {
        let issuer = builder
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.claim_row(&super::palw_bond_budget_fold_v1::native_income_id_v1(root, bond, 0)))
            .is_some();
        builder.cancel_native_recipient_v1(root, *bond, if issuer { 0 } else { 2 })?;
    }

    let mut c = builder.state.vesting_counters;
    c.burned = c.burned.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("EXEC vesting burned"))?;
    let retained = if builder.state.vesting.contains_key(&root) { Some(Row { legs: BTreeMap::new(), ..row.clone() }) } else { None };
    builder.write_exec_reward(root, retained)?;
    builder.write_vesting_counters(c)?;
    for ((bond, fund), payout) in row.legs {
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
            claim_id: root,
            bond,
            fund,
            action: Action::Burned,
            payout,
        })?;
    }
    Ok(())
}

fn key(fund: Fund, payload: &Hash64) -> Hash64 {
    match fund {
        Fund::NativeSlice => palw_panel_payout_key_v1(payload),
        Fund::KernelFee => crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(payload),
    }
}

type Leg = (Hash64, (PalwBondKeyV2, Fund), PalwPayoutV2);
/// Release only AFTER the inclusive liability day, and after the native row's second clock has released it.
/// Old additional legs run before new native moves, so an arriving stream of native rows cannot starve them.
fn release_plan(state: &PalwChainStateV2, daa: u64, after_drain: bool) -> (Vec<Leg>, usize) {
    use crate::palw_vesting_v1::{PALW_V2_VESTING_LEGS_PER_BLOCK, PALW_V2_VESTING_MARKET_RESERVE};
    let queued: BTreeMap<_, _> =
        state.pending_payouts.iter().skip(if after_drain { PALW_V2_MAX_PAYOUTS_PER_BLOCK } else { 0 }).collect();
    let non_market = queued.keys().filter(|key| key.as_byte_slice()[0] != PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX).count();
    let market = queued
        .keys()
        .filter(|key| key.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX)
        .count()
        .min(PALW_V2_VESTING_MARKET_RESERVE);
    let width = PALW_V2_VESTING_LEGS_PER_BLOCK
        .saturating_sub(non_market)
        .saturating_sub(market)
        .min(PALW_V2_MAX_PENDING_PAYOUTS.saturating_sub(queued.len()));
    let mut ready: Vec<_> =
        state.exec_v2_rewards.iter().filter(|(id, row)| daa > row.liability_until && !state.vesting.contains_key(id)).collect();
    ready.sort_by_key(|(id, row)| (row.final_daa, **id));
    let mut fresh = BTreeSet::new();
    let mut out = Vec::new();
    for (id, row) in ready {
        for (who, leg) in &row.legs {
            if state.exec_v2_reward_holds.get(&(*id, who.0)).is_some_and(|after| daa < *after) {
                continue;
            }
            let payout_key = key(who.1, &leg.payload);
            if !queued.contains_key(&payout_key) && !fresh.contains(&payout_key) {
                if fresh.len() >= width {
                    return (out, fresh.len());
                }
                fresh.insert(payout_key);
            }
            out.push((*id, *who, *leg));
        }
    }
    (out, fresh.len())
}

pub(super) fn mature(builder: &mut TransitionBuilder<'_>, daa: u64) -> Result<(), PalwStateV2Error> {
    let empty: Vec<_> = builder
        .state
        .exec_v2_rewards
        .iter()
        .filter(|(id, row)| row.legs.is_empty() && daa > row.liability_until && !builder.state.vesting.contains_key(id))
        .map(|(id, _)| *id)
        .collect();
    for id in empty {
        builder.write_exec_reward(id, None)?;
    }
    for (root, who, leg) in release_plan(&builder.state, daa, false).0 {
        // RFC-0008 once-only settlement (dormant): one root leg `(root, bond, fund)` is paid once across every consumer. A leg
        // another consumer already settled is retired from the row without moving value again.
        let pays = {
            use crate::palw_settlement_once_v1::{
                PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
            };
            let fund = match who.1 {
                Fund::NativeSlice => 0,
                Fund::KernelFee => 1,
            };
            // RFC08-2: a leg held in the root's reward row past the row's own liability horizon — no new R draw.
            let horizon = builder.state.exec_v2_rewards.get(&root).map(|row| row.liability_until);
            let final_daa = builder.state.claims.get(&root).and_then(|c| match c.phase {
                PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
                _ => None,
            });
            builder
                .settle_once_liable_v1(
                    daa,
                    PalwSettlementObligationV1::with_party(
                        PalwSettlementLegV1::ExecLegPayment,
                        root.as_bytes(),
                        crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&who.0),
                        fund,
                    ),
                    PalwSettlementConsumerV1::ExecLegMaturity,
                    leg.amount,
                    PalwSettlementDebitV1::Held { amount: leg.amount },
                    super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
                        claim: root,
                        final_daa,
                        horizon,
                        draw: 0,
                        paid_now: leg.amount,
                        exported_before: None,
                        vest_until: None,
                    },
                )?
                .pays()
        };
        if pays {
            match who.1 {
                Fund::NativeSlice => builder.add_panel_payout(leg.payload, leg.amount)?,
                Fund::KernelFee => builder.add_kernel_payout(leg.payload, leg.amount)?,
            }
        }
        let mut row = builder.state.exec_v2_rewards[&root].clone();
        row.legs.remove(&who);
        let finished = !row.legs.keys().any(|(bond, _)| *bond == who.0);
        let issuer = builder.state.claims.get(&root).is_some_and(|claim| claim.bond == who.0)
            || builder
                .state
                .bond_budget
                .as_ref()
                .and_then(|b| b.claim_row(&super::palw_bond_budget_fold_v1::native_income_id_v1(root, &who.0, 0)))
                .is_some();
        if pays {
            builder.pay_native_recipient_v1(
                root,
                who.0,
                if issuer { 0 } else { 2 },
                daa,
                leg.amount,
                key(who.1, &leg.payload),
                finished,
            )?;
        }
        builder.write_exec_reward(root, if row.legs.is_empty() { None } else { Some(row) })?;
        if !pays {
            continue;
        }
        let mut c = builder.state.vesting_counters;
        c.moved = c.moved.checked_add(u128::from(leg.amount)).ok_or(PalwStateV2Error::Overflow("EXEC vesting moved"))?;
        builder.write_vesting_counters(c)?;
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
            claim_id: root,
            bond: who.0,
            fund: who.1,
            action: Action::Moved,
            payout: leg,
        })?;
    }
    Ok(())
}

/// Reverse only the composite root's contribution; the offending executor is charged by the kernel route.
/// No native offence or extra exposure release is fabricated against the root producer or its Panel.
pub(super) fn invalidate_final(
    builder: &mut TransitionBuilder<'_>,
    root: Hash64,
    dependency: Hash64,
    ctx: &PalwBlockContextV2,
    reason: PalwVoidReasonV2,
) -> Result<(), PalwStateV2Error> {
    if !builder.state.claims.get(&root).is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::Final { .. })) {
        return Ok(());
    }
    builder.reverse_convicted_final(ctx, root, reason)?;
    builder.forfeit_minted_round_rights_of_claim(&root)?;
    if let Some(row) = builder.state.vesting.get(&root).cloned() {
        for (bond, _) in &row.seats {
            builder.cancel_native_recipient_v1(root, *bond, 1)?;
        }
        builder.cancel_native_recipient_v1(root, row.producer_bond, 0)?;
        let mut c = builder.state.vesting_counters;
        c.burned = c.burned.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("EXEC native vesting burned"))?;
        builder.write_vesting(root, None)?;
        builder.write_vesting_counters(c)?;
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecDependencyBurned {
            claim_id: root,
            kernel_claim_id: dependency,
            sompi: row.total_sompi(),
            legs: row.legs().collect(),
        })?;
    }
    burn(builder, root)?;
    builder.mark_liability_convicted(root, ctx.daa_score, reason)?;
    Ok(())
}

/// Every finite retirement composes the physical-income material window.
/// The legacy EXEC horizon still extends only the ordinary retirement; operational
/// collateral-release entries are separately preserved by the terminal deadline caller.
pub(super) fn retirement(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    id: &Hash64,
    at: Option<u64>,
    claim: &PalwClaimStateV2,
) -> Result<Option<u64>, PalwStateV2Error> {
    if params.claim_retirement_daa == 0 {
        return Ok(at);
    }
    let ordinary = terminal_daa_of(claim).and_then(|daa| daa.checked_add(params.claim_retirement_daa));
    let monetary =
        state.material_income_hold_until_v1(params, *id).max(state.paid_consumer_terms_v1(id).map(|terms| terms.material_until));
    let held = at.map(|at| monetary.map_or(at, |until| at.max(if until == u64::MAX { u64::MAX } else { until.saturating_sub(1) })));
    if at != ordinary {
        return Ok(held);
    }
    let at = held;
    match horizon(state, params, id)? {
        Some(until) => {
            let after = until.checked_add(1).ok_or(PalwStateV2Error::Overflow("EXEC retirement horizon"))?;
            Ok(at.map(|at| at.max(after)))
        }
        None => Ok(at),
    }
}

pub(super) fn consistent(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    let bad = |why: &str| PalwStateV2Error::CarriageInconsistent(format!("EXEC reward vesting: {why}"));
    if state.exec_v2_retained_slots_v1() > crate::palw_work_slice_v2::PALW_EXEC_V2_MAX_OPEN_ROOTS {
        return Err(bad("too many roots"));
    }
    for ((id, _), session) in &state.da_sessions {
        require_native_da_window(state, params, id, session.deadline_daa, session.opened_daa)?;
    }
    for session in state.court_sessions.values() {
        require_native_session_window(state, params, &session.claim, session.deadline_daa)?;
    }
    for id in state.exec_v2.roots.keys().chain(state.exec_v2_rewards.keys()) {
        if let Some(PalwClaimStateV2 { phase: PalwClaimPhaseV2::DefaultDisputed { accused_daa, .. }, .. }) = state.claims.get(id) {
            let deadline = accused_daa
                .checked_add(palw_da_disclose_window_daa_v1(params))
                .ok_or(PalwStateV2Error::Overflow("EXEC native DA deadline"))?;
            require_native_session_window(state, params, id, deadline)?;
        }
    }
    let mut payees: BTreeMap<PalwBondKeyV2, BTreeSet<Hash64>> = BTreeMap::new();
    for (id, row) in &state.exec_v2_rewards {
        for (bond, _) in row.legs.keys() {
            payees.entry(*bond).or_default().insert(*id);
        }
        let native_receipt = row.legs.is_empty() && state.vesting.contains_key(id);
        if (row.legs.is_empty()
            && !state.vesting.contains_key(id)
            && state.last_point.as_ref().is_none_or(|point| point.daa_score <= row.liability_until))
            || row.legs.len() > 2 * (crate::palw_work_slice_v2::PALW_EXEC_V2_MAX_EXTRA_EXECUTORS + 1)
            || row.legs.values().any(|leg| leg.amount == 0)
            || row.total_sompi_u128() > u128::from(row.allocation_sompi)
            || row.final_daa > row.liability_until
        {
            return Err(bad("invalid legs, allocation or horizon"));
        }
        if let Some(root) = state.exec_v2.roots.get(id) {
            if !(matches!(root.phase, PalwWorkRootPhaseV2::Settled { settled_daa } if settled_daa == row.final_daa)
                || (native_receipt && matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. })))
                || horizon(state, params, id)?.is_some_and(|until| row.liability_until < until)
                || row.legs.keys().any(|(bond, _)| *bond != root.root_bond && !root.extra_executors.contains(bond))
            {
                return Err(bad("root, payee or liability disagrees"));
            }
        } else if state.last_point.as_ref().is_none_or(|point| point.daa_score <= row.liability_until) {
            return Err(bad("a collectible dependency root was retired"));
        }
        if state.claims.get(id).is_some_and(|claim| {
            !(matches!(claim.phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == row.final_daa)
                || (native_receipt && matches!(claim.phase, PalwClaimPhaseV2::Voided { .. })))
        }) {
            return Err(bad("claim is not its Final"));
        }
        if let Some(native) = state.vesting.get(id)
            && (native.expiry_daa <= row.liability_until
                || native.total_sompi_u128() + row.total_sompi_u128()
                    > u128::from(native.escrowed_reward.saturating_sub(native.buyback_bound)))
        {
            return Err(bad("native and additional vesting disagree"));
        }
    }
    if payees != state.exec_v2_reward_payees {
        return Err(bad("payee index disagrees"));
    }
    for ((root, bond), after) in &state.exec_v2_reward_holds {
        let Some(row) = state.exec_v2_rewards.get(root) else { return Err(bad("actor hold has no receipt")) };
        if !row.legs.keys().any(|(payee, _)| payee == bond) || *after <= row.liability_until.saturating_add(1) {
            return Err(bad("actor hold has no fund or extension"));
        }
    }
    Ok(())
}

// =================================================================================================================================
// G14 r6 (item 2): the EXEC and provider dependency horizons the clock-v2 dispute extension must not outrun (RFC-0026 §3.3 F07)
// =================================================================================================================================

/// A kernel claim's stored row, or a typed refusal (a dependency that is named but missing is never zero-filled).
fn kernel_dependency_row_v2(
    kernel: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    claim: &Hash64,
) -> Result<misaka_palw_kernel::ledger::ClaimRowV1, PalwStateV2Error> {
    kernel
        .rows
        .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&claim.as_bytes()).expect("a digest serializes")))
        .and_then(|bytes| borsh::from_slice(bytes).ok())
        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("clock v2: an EXEC dependency's kernel claim row is missing".into()))
}

/// **The provider quorum horizon of a transferred kernel claim**: the last DAA through which at least
/// [`crate::palw_provider_court_v1::PALW_PROVIDER_MIN_PROVIDERS_V1`] distinct non-producer operators still serve it, from the leases
/// live at `now` (each operator counted at its longest lease). `None`: the claim's DA was never transferred to leases (no provider
/// dependency). `Some(0)`: transferred but lapsed, or fewer live operators than the quorum — a dependency that covers nothing.
pub(super) fn provider_quorum_horizon_v2(
    state: &PalwChainStateV2,
    claim: &Hash64,
    producer_operator: Option<Hash64>,
    now: u64,
) -> Option<u64> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_MIN_PROVIDERS_V1, ProviderSubjectV1};
    let route = state.kernel_route.as_ref()?;
    let subject = ProviderSubjectV1::KernelClaim { claim: *claim };
    let transfer = route.provider_subject_v1(&subject).filter(|t| t.transferred_daa.is_some())?;
    if transfer.lapsed_daa.is_some() {
        return Some(0);
    }
    let mut per_operator: BTreeMap<Hash64, u64> = BTreeMap::new();
    for (provider, row) in route.provider_leases_of_v1(&subject) {
        if !row.live_at(now) {
            continue;
        }
        let Some(operator) = state.bond(&provider).map(|b| b.operator_id) else { continue };
        if Some(operator) == producer_operator {
            continue;
        }
        let until = per_operator.entry(operator).or_insert(0);
        *until = (*until).max(row.serve_until_daa);
    }
    Some(crate::palw_court_clock_v2::palw_provider_quorum_horizon_of_v2(per_operator.into_values(), PALW_PROVIDER_MIN_PROVIDERS_V1))
}

/// **Every EXEC and provider dependency of `claim_id`, with its signed horizon** — what the clock-v2 dispute extension
/// (`palw_dispute_record_hard_deadline_v2`) must see covered, beside the claim's own trace and its staged custody:
///
/// * `Exec`: when `claim_id` is an EXEC root — the root's signed liability floor (`expiry + window_court`, or the kernel policy's
///   floor), and EACH kernel claim it rests on (its prefix claim and every slice's evidence root) at its own `liability_until`
///   (F07 is per dependency: one longer-lived dependency never supplies another's material). A retired root's unpaid receipt carries
///   its `liability_until`. A named kernel claim whose row or horizon is missing is a typed refusal.
/// * `Provider`: for each of those kernel claims, and for `claim_id` itself as a kernel subject, whose DA moved to leases (tag 153) —
///   the provider quorum horizon ([`provider_quorum_horizon_v2`]), excluding the producer's own operator.
///
/// Nothing here writes; the caller refuses the extension atomically on the first short horizon.
pub(super) fn court_clock_v2_exec_provider_dependencies_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    now: u64,
) -> Result<Vec<crate::palw_court_clock_v2::PalwCourtDependencyV2>, PalwStateV2Error> {
    use crate::palw_court_clock_v2::{PalwCourtDependencyKindV2 as Kind, PalwCourtDependencyV2 as Dep};
    let mut out = Vec::new();
    let mut kernel_claims: Vec<Hash64> = vec![*claim_id];
    if let Some(root) = state.exec_v2.roots.get(claim_id) {
        let floor = match &state.kernel_route {
            Some(kernel) => super::palw_kernel_route_fold_v1::exec_liability_floor_v1(
                root.expiry_daa,
                params.window_court(),
                &kernel.header.policy,
            )?,
            None => root.expiry_daa.checked_add(params.window_court()).ok_or(PalwStateV2Error::Overflow("EXEC dependency horizon"))?,
        };
        out.push(Dep { kind: Kind::Exec, signed_horizon_daa: floor });
        if let Some(kernel) = &state.kernel_route {
            let claims = std::iter::once(root.prefix_claim)
                .chain(state.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(_, row)| row.evidence_root));
            for claim in claims {
                let row = kernel_dependency_row_v2(kernel, &claim)?;
                let until = row.liability_until.ok_or_else(|| {
                    PalwStateV2Error::CarriageInconsistent("clock v2: an EXEC dependency has no liability horizon".into())
                })?;
                out.push(Dep { kind: Kind::Exec, signed_horizon_daa: until });
                kernel_claims.push(claim);
            }
        }
    } else if let Some(receipt) = state.exec_v2_rewards.get(claim_id) {
        out.push(Dep { kind: Kind::Exec, signed_horizon_daa: receipt.liability_until });
    }
    if let Some(kernel) = &state.kernel_route {
        for claim in kernel_claims {
            // The producer's own operator never counts toward the quorum (as `require_exec_provider_horizon_v1`).
            let producer_operator = kernel
                .kernel_claim_row_v1(&claim)
                .and_then(|row| kernel.bond_key_of(&row.producer))
                .and_then(|bond| state.bond(&bond).map(|b| b.operator_id));
            if let Some(until) = provider_quorum_horizon_v2(state, &claim, producer_operator, now) {
                out.push(Dep { kind: Kind::Provider, signed_horizon_daa: until });
            }
        }
    }
    Ok(out)
}
