//! One committed snapshot of native and additional EXEC funds. Paging never changes whole-query money/hold totals.
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwPayoutV2, PalwStateParamsV2};
use crate::palw_vesting_read_v1::{
    PalwVestingPayeeV1, PalwVestingQueryV1, PalwVestingReadV1, PalwVestingReaderV1, palw_vesting_read_v1,
};
use crate::palw_work_slice_v2::PalwExecV2RewardFundV1 as Fund;
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

pub type PalwExecRewardCursorV2 = (u64, Hash64, PalwBondKeyV2, Fund);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecRewardReadV2 {
    pub claim_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub fund: Fund,
    pub payout: PalwPayoutV2,
    pub final_daa: u64,
    pub earliest_daa: u64,
    pub native_pending: bool,
    pub native_clock_live: bool,
    pub native_expiry_daa: Option<u64>,
    pub native_second_clock_bound_daa: Option<u64>,
    pub lock_live: bool,
    pub ready: bool,
    pub in_next_block: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRewardVestingReadV2 {
    /// Native component, with its original row cursor and attribution. Its global counters include all funds.
    pub native: PalwVestingReadV1,
    pub state_root: Hash64,
    pub outstanding_sompi: u128,
    pub exec_slice_sompi: u128,
    pub exec_kernel_fee_sompi: u128,
    pub query_outstanding_sompi: u128,
    pub query_exec_pending_sompi: u128,
    pub query_exec_ready_sompi: u128,
    pub locked_sources: usize,
    pub last_lock_daa: Option<u64>,
    pub exec_rows: Vec<PalwExecRewardReadV2>,
    pub exec_rows_total: usize,
    pub exec_next_after: Option<PalwExecRewardCursorV2>,
}

pub fn palw_reward_vesting_read_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    next_daa: u64,
    raw_depth: Option<u64>,
    query: &PalwVestingQueryV1,
    limit: usize,
    after: Option<(u64, Hash64)>,
    exec_after: Option<PalwExecRewardCursorV2>,
) -> PalwRewardVestingReadV2 {
    let native = palw_vesting_read_v1(state, params, next_daa, raw_depth, query, limit, after);
    let reader = PalwVestingReaderV1::new(state, params, next_daa, raw_depth);
    let planned: BTreeSet<_> =
        state.exec_v2_reward_release_moves_v1(next_daa, true).into_iter().map(|(root, who, _)| (root, who)).collect();
    let limit = if limit == 0 { 500 } else { limit.min(500) };
    let mut matched = Vec::new();
    let mut slice = 0u128;
    let mut fee = 0u128;
    let mut pending = 0u128;
    let mut ready = 0u128;
    let mut holds = BTreeSet::new();
    let mut last_lock_daa = native.lock_live_last_expiry_daa;
    for (root, row) in state.exec_v2_reward_vesting_iter_v1() {
        let native_row = state.vesting_row(root);
        let native_clock_live = native_row.is_some_and(|row| reader.lock_live(row));
        let native_bound = native_row.and_then(|row| reader.maturity(row).second_clock_bound_daa);
        for ((bond, fund), payout) in &row.legs {
            match fund {
                Fund::NativeSlice => slice += u128::from(payout.amount),
                Fund::KernelFee => fee += u128::from(payout.amount),
            }
            let ours = match query {
                PalwVestingQueryV1::Chain => true,
                PalwVestingQueryV1::Claim(id) => id == root,
                PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(who)) => who == bond,
                PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Payload(payload)) => *payload == payout.payload,
            };
            if !ours {
                continue;
            }
            let earliest = state.exec_v2_reward_hold_after_v1(root, bond).expect("an iterated leg has its clock");
            let lock_live =
                state.exec_v2_reward_clock_live_v1(params, root, bond, next_daa, raw_depth).expect("an iterated leg has its clock");
            // The liability horizon is inclusive, even if its successor saturates at u64::MAX.
            let is_ready = next_daa > row.liability_until && next_daa >= earliest && native_row.is_none();
            if is_ready {
                ready += u128::from(payout.amount);
            } else {
                pending += u128::from(payout.amount);
            }
            if lock_live {
                holds.insert((*root, *bond));
                last_lock_daa = last_lock_daa.max(Some(earliest.max(native_row.map_or(0, |row| row.expiry_daa))));
            }
            matched.push(PalwExecRewardReadV2 {
                claim_id: *root,
                bond: *bond,
                fund: *fund,
                payout: *payout,
                final_daa: row.final_daa,
                earliest_daa: earliest,
                native_pending: native_row.is_some(),
                native_clock_live,
                native_expiry_daa: native_row.map(|row| row.expiry_daa),
                native_second_clock_bound_daa: native_bound,
                lock_live,
                ready: is_ready,
                in_next_block: planned.contains(&(*root, (*bond, *fund))),
            });
        }
    }
    let key = |r: &PalwExecRewardReadV2| (r.final_daa, r.claim_id, r.bond, r.fund);
    matched.sort_by_key(key);
    let exec_rows_total = matched.len();
    let mut page: Vec<_> = matched.into_iter().filter(|r| exec_after.is_none_or(|after| key(r) > after)).take(limit + 1).collect();
    let exec_next_after = (page.len() > limit).then(|| key(&page[limit - 1]));
    page.truncate(limit);
    PalwRewardVestingReadV2 {
        state_root: state.state_root(),
        outstanding_sompi: native.live_sompi + slice + fee,
        exec_slice_sompi: slice,
        exec_kernel_fee_sompi: fee,
        query_outstanding_sompi: native.maturing_sompi + native.latched_sompi_of_query + pending + ready,
        query_exec_pending_sompi: pending,
        query_exec_ready_sompi: ready,
        locked_sources: native.lock_live_rows + holds.len(),
        last_lock_daa,
        exec_rows: page,
        exec_rows_total,
        exec_next_after,
        native,
    }
}
