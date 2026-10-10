//! EXEC's additional reward legs retain their funding role, root and fixed payee through the collectible window.
//! Native root/Panel legs remain in the native vesting row, extended through that same window.

use super::*;
use crate::palw_work_slice_v2::{
    PalwExecV2RewardActionV1 as Action, PalwExecV2RewardFundV1 as Fund, PalwExecV2RewardRowV1 as Row, PalwWorkRootPhaseV2,
};

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

    /// Additional new keys ahead of native vesting, after this block's drain (or its next-block simulation).
    pub fn exec_v2_reward_release_keys_v1(&self, daa: u64, after_drain: bool) -> usize {
        release_plan(self, daa, after_drain).1
    }
}

pub(super) fn start(builder: &mut TransitionBuilder<'_>, root: Hash64, allocation: u64, daa: u64) -> Result<(), PalwStateV2Error> {
    let until = horizon(&builder.state, builder.params, &root)?
        .ok_or_else(|| PalwStateV2Error::ExecV2Refused("reward has no EXEC root".into()))?;
    builder.write_exec_reward(
        root,
        Some(Row { final_daa: daa, liability_until: until, allocation_sompi: allocation, legs: BTreeMap::new() }),
    );
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
    builder.write_exec_reward(root, Some(row));
    let mut c = builder.state.vesting_counters;
    c.created = c.created.checked_add(u128::from(amount)).ok_or(PalwStateV2Error::Overflow("EXEC vesting created"))?;
    builder.write_vesting_counters(c);
    builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
        claim_id: root,
        bond,
        fund,
        action: Action::Created,
        payout: PalwPayoutV2 { payload, amount },
    });
    Ok(())
}

pub(super) fn finish(builder: &mut TransitionBuilder<'_>, root: Hash64) {
    if builder.state.exec_v2_rewards.get(&root).is_some_and(|row| row.legs.is_empty()) && !builder.state.vesting.contains_key(&root) {
        builder.write_exec_reward(root, None);
    }
}

pub(super) fn burn(builder: &mut TransitionBuilder<'_>, root: Hash64) -> Result<(), PalwStateV2Error> {
    let Some(row) = builder.state.exec_v2_rewards.get(&root).cloned() else { return Ok(()) };
    let mut c = builder.state.vesting_counters;
    c.burned = c.burned.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("EXEC vesting burned"))?;
    let retained = if builder.state.vesting.contains_key(&root) { Some(Row { legs: BTreeMap::new(), ..row.clone() }) } else { None };
    builder.write_exec_reward(root, retained);
    builder.write_vesting_counters(c);
    for ((bond, fund), payout) in row.legs {
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
            claim_id: root,
            bond,
            fund,
            action: Action::Burned,
            payout,
        });
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
        builder.write_exec_reward(id, None);
    }
    for (root, who, leg) in release_plan(&builder.state, daa, false).0 {
        match who.1 {
            Fund::NativeSlice => builder.add_panel_payout(leg.payload, leg.amount)?,
            Fund::KernelFee => builder.add_kernel_payout(leg.payload, leg.amount)?,
        }
        let mut row = builder.state.exec_v2_rewards[&root].clone();
        row.legs.remove(&who);
        builder.write_exec_reward(root, if row.legs.is_empty() { None } else { Some(row) });
        let mut c = builder.state.vesting_counters;
        c.moved = c.moved.checked_add(u128::from(leg.amount)).ok_or(PalwStateV2Error::Overflow("EXEC vesting moved"))?;
        builder.write_vesting_counters(c);
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecReward {
            claim_id: root,
            bond: who.0,
            fund: who.1,
            action: Action::Moved,
            payout: leg,
        });
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
    builder.forfeit_minted_round_rights_of_claim(&root);
    if let Some(row) = builder.state.vesting.get(&root).cloned() {
        let mut c = builder.state.vesting_counters;
        c.burned = c.burned.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("EXEC native vesting burned"))?;
        builder.write_vesting(root, None);
        builder.write_vesting_counters(c);
        builder.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::ExecDependencyBurned {
            claim_id: root,
            kernel_claim_id: dependency,
            sompi: row.total_sompi(),
            legs: row.legs().collect(),
        });
    }
    burn(builder, root)?;
    builder.mark_liability_convicted(root, ctx.daa_score, reason);
    Ok(())
}

/// Retirement is strictly after the dependency proof window; release/obligation deadlines are unchanged.
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
    if at != ordinary {
        return Ok(at);
    }
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
    for (id, row) in &state.exec_v2_rewards {
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
        if let Some(native) = state.vesting.get(id) {
            if native.expiry_daa <= row.liability_until
                || native.total_sompi_u128() + row.total_sompi_u128()
                    > u128::from(native.escrowed_reward.saturating_sub(native.buyback_bound))
            {
                return Err(bad("native and additional vesting disagree"));
            }
        }
    }
    Ok(())
}
