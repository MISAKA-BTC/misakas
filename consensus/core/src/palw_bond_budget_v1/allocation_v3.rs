//! **ADR-0177 / RFC-0015 §8.5.5 — the model allocation as the user decided it (2026-10-14; DORMANT, no height).**
//!
//! ```text
//! S_m(k)  = C_miner_claim(m, k) + C_verifier_claim(m, k)        deduplicated per bond (the same funds counted once), Active bonds
//! S̄_m(d)  = (1 / n) · Σ_{k = e−n+1}^{e} S_m(k)                  n = 7 day-equivalents (2026-10-16; 30 before) / epoch length, e the current epoch
//! P_m     = S̄_m^1.5 / Σ_j S̄_j^1.5                               (0 for every model when the denominator is 0)
//! block opportunity   a model's reward-bearing block units this epoch ≤ ⌊accrued_blocks · A_m / Σ A⌋ (accrued: one block per DAA)
//! coinbase            R_m = ⌊R_PALW · A_m / Σ A⌋ (the existing reward share; applied ONCE — never multiplied per block again)
//! ```
//!
//! *Verifier capital* is the capital a bond has actually locked as a Panel seat behind a live claim of the model (its seat duty on that
//! claim, `PalwPanelDutyRowV1::seat_exposure`) — never a waiting bond, a registration fee, an AMM / Position or an unpaid reward. It is
//! added to `S_m` only, never to the miner's own Q/B/R/F caps. A bond's miner assignment and its verifier locks are clipped together
//! to its capital (`palw_assignment_pro_rata_v1` over their per-model sums), so no funds are counted twice.
//!
//! *Storage.* The epoch snapshots and the per-epoch block reservations ride the budget's `models` table (table 5) as rows under
//! domain-separated synthetic keys, distinguished by a marker in `reserved_sompi` (`u64::MAX` history, `u64::MAX − 1` blocks; a live
//! model row's reservation is at most one epoch's carve, far below either) and carrying the model id in `weight` — so they are rooted,
//! carried, reverted and replayed exactly as every other row, and no table id, encoding or version changes.
//!
//! **Every number is POLICY; `α = 1.5`, the 7-day average (2026-10-16) and the per-model block opportunity are the user's decision,
//! the epoch length and seasoning are not, and the fence stays refused.** Required before activation (user): new models are not
//! starved under `α = 1.5`; verifier participation, liability, hold and reward rules (including bond-without-checking) are defined.

use super::*;

/// The allocation policy version that carries [`PalwAllocationAverageV1`] (the 2026-10-14 decision).
pub const PALW_MODEL_ALLOCATION_POLICY_VERSION_V3: u16 = 3;
/// **User decision (2026-10-14): `α = 1.5`** in half steps (RFC-0015 §8.5.5).
pub const PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1: u8 = 3;
/// **User decision (2026-10-16, replacing 2026-10-14's 30 days): the averaging period, in day-equivalents** (7-day-equivalent DAA).
pub const PALW_ALLOCATION_AVERAGE_DAYS_V1: u32 = 7;
/// The most epochs one average may span (bounds the history rows a model keeps).
pub const PALW_ALLOCATION_AVERAGE_EPOCHS_MAX_V1: u64 = 1_440;
/// Marker of an epoch-snapshot history row in the `models` table.
pub const PALW_ALLOCATION_HISTORY_MARKER_V1: u64 = u64::MAX;
/// Marker of a per-epoch model block-reservation row in the `models` table.
pub const PALW_ALLOCATION_BLOCKS_MARKER_V1: u64 = u64::MAX - 1;

const PALW_ALLOCATION_HISTORY_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/model-capital-history/v1";
const PALW_ALLOCATION_BLOCKS_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/model-block-opportunity/v1";

/// **The 2026-10-14 averaging and application terms** of a V3 allocation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAllocationAverageV1 {
    /// 30 (user decision).
    pub average_days: u32,
    /// DAA per day-equivalent ([`PALW_BOND_HOLD_DAA_PER_DAY_V1`]).
    pub daa_per_day: u64,
    /// Count claim-bound verifier capital in `S_m` (user decision: miner + verifier).
    pub verifier_capital: bool,
    /// Apply `P_m` to each model's reward-bearing block opportunity (user decision).
    pub block_opportunity: bool,
}

impl PalwAllocationAverageV1 {
    pub const fn decided_v1() -> Self {
        Self {
            average_days: PALW_ALLOCATION_AVERAGE_DAYS_V1,
            daa_per_day: PALW_BOND_HOLD_DAA_PER_DAY_V1,
            verifier_capital: true,
            block_opportunity: true,
        }
    }

    /// The averaging span in DAA.
    pub fn span_daa(&self) -> u64 {
        (self.average_days as u64).saturating_mul(self.daa_per_day)
    }
}

impl PalwModelAllocationPolicyV1 {
    /// **NOT A PROPOSAL**: the decided `α`, average and application with a 1-day epoch and one epoch of seasoning (both POLICY).
    pub fn decided_unapproved_v3() -> Self {
        Self {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V3,
            epoch_daa: PALW_BOND_HOLD_DAA_PER_DAY_V1,
            seasoning_epochs: 1,
            curve: PalwAllocationCurveV1 { alpha_halves: PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1 },
            max_models_per_bond: 16,
            average: Some(PalwAllocationAverageV1::decided_v1()),
        }
    }

    /// `n`, the epochs one average spans (`None` for a V2 policy).
    pub fn average_epochs(&self) -> Option<u64> {
        let average = self.average.as_ref()?;
        Some((average.span_daa() / self.epoch_daa.max(1)).max(1))
    }

    /// The V3 form's refusals (called by `validate`).
    pub(super) fn validate_v3(&self) -> Result<(), &'static str> {
        let Some(average) = self.average.as_ref() else { return Err("a V3 allocation policy carries its averaging terms") };
        if self.curve.alpha_halves != PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1 {
            return Err("the allocation curve is alpha = 1.5 (user decision 2026-10-14)");
        }
        if average.average_days != PALW_ALLOCATION_AVERAGE_DAYS_V1 || average.daa_per_day != PALW_BOND_HOLD_DAA_PER_DAY_V1 {
            return Err("the allocation averages over 7 day-equivalents of DAA (user decision 2026-10-16)");
        }
        if !average.verifier_capital || !average.block_opportunity {
            return Err("S_m counts miner + claim-bound verifier capital, and P_m applies to block opportunity and coinbase (2026-10-14)");
        }
        // A zero epoch is refused here, before the remainder below (`validate`'s own zero check runs after this one).
        if self.epoch_daa == 0 {
            return Err("the allocation epoch must be positive");
        }
        if average.span_daa() % self.epoch_daa != 0 {
            return Err("the allocation epoch must divide the averaging span");
        }
        if average.span_daa() / self.epoch_daa > PALW_ALLOCATION_AVERAGE_EPOCHS_MAX_V1 {
            return Err("the averaging span holds too many epochs");
        }
        Ok(())
    }
}

/// The history row key of `(model, epoch)`.
pub fn palw_allocation_history_key_v1(model: &Hash64, epoch: u64) -> Hash64 {
    keyed64(PALW_ALLOCATION_HISTORY_DOMAIN_V1, &[model.as_byte_slice(), &epoch.to_le_bytes()])
}

/// The block-reservation row key of `(model, epoch)`.
pub fn palw_allocation_blocks_key_v1(model: &Hash64, epoch: u64) -> Hash64 {
    keyed64(PALW_ALLOCATION_BLOCKS_DOMAIN_V1, &[model.as_byte_slice(), &epoch.to_le_bytes()])
}

fn model_as_weight(model: &Hash64) -> PalwAllocationWeightV1 {
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(model.as_byte_slice());
    PalwAllocationWeightV1::from_le_bytes(bytes)
}

fn weight_as_model(weight: &PalwAllocationWeightV1) -> Hash64 {
    Hash64::from_bytes(weight.to_le_bytes())
}

/// **One epoch's deduplicated snapshot** `S_m(k)`: per Active bond, its seasoned miner amounts and its claim-bound verifier locks,
/// summed per model and clipped together to the bond's capital (pro-rata), then summed over bonds.
pub fn palw_allocation_snapshot_v3(
    miner: &BTreeMap<PalwBondKeyV2, Vec<(Hash64, u64)>>,
    verifier: &BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>>,
    capital_of: impl Fn(&PalwBondKeyV2) -> (u64, bool),
) -> BTreeMap<Hash64, u128> {
    let bonds: BTreeSet<PalwBondKeyV2> = miner.keys().chain(verifier.keys()).copied().collect();
    let mut s: BTreeMap<Hash64, u128> = BTreeMap::new();
    for bond in bonds {
        let (capital, active) = capital_of(&bond);
        if !active || capital == 0 {
            continue;
        }
        let mut per_model: BTreeMap<Hash64, u64> = BTreeMap::new();
        for (m, a) in miner.get(&bond).into_iter().flatten() {
            let e = per_model.entry(*m).or_default();
            *e = e.saturating_add(*a);
        }
        for (m, a) in verifier.get(&bond).into_iter().flatten() {
            let e = per_model.entry(*m).or_default();
            *e = e.saturating_add(*a);
        }
        let amounts: Vec<(Hash64, u64)> = per_model.into_iter().collect();
        for (m, a) in palw_assignment_pro_rata_v1(&amounts, capital) {
            *s.entry(m).or_default() += a as u128;
        }
    }
    s
}

impl PalwBondBudgetStateV1 {
    /// Whether a `models` row is a V3 history or block-reservation row (never a live model row).
    pub fn is_allocation_aux_row_v1(row: &PalwModelBudgetRowV1) -> bool {
        matches!(row.reserved_sompi, PALW_ALLOCATION_HISTORY_MARKER_V1 | PALW_ALLOCATION_BLOCKS_MARKER_V1)
    }

    /// **Open allocation epoch `index` under a V3 policy** (the 2026-10-14 decision): promote seasoned assignments, take the
    /// deduplicated miner + verifier snapshot `S_m(index)`, record it, drop history older than the averaging span and last epochs'
    /// block reservations, then write every model's live row from the average `S̄_m` and `A_m = ⌊S̄_m^α⌋`.
    ///
    /// `eligible_capital(model, capital)` is RFC-0017's hook (GAP-E1, `palw_eligibility_allocation_capital_v1`): the capital a model's
    /// eligibility stage admits — 0 for a Registered / Lapsed model past `palw_model_eligibility_v1`, the capital unchanged below it.
    /// It is applied to each epoch's snapshot (parked capital is never recorded) and to the average (a model that lapsed now leaves
    /// the denominator now), so such a model has no live row: no coinbase share, no block opportunity, and no dilution of others.
    #[allow(clippy::too_many_arguments)]
    pub fn roll_epoch_v3(
        &mut self,
        policy: &PalwModelAllocationPolicyV1,
        index: u64,
        start_daa: u64,
        capital_of: impl Fn(&PalwBondKeyV2) -> (u64, bool),
        verifier: &BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>>,
        eligible_capital: impl Fn(&Hash64, u128) -> u128,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) {
        self.roll_epoch_v3_tiered(
            policy,
            index,
            start_daa,
            capital_of,
            verifier,
            eligible_capital,
            |_| (false, crate::palw_model_tier_v1::PALW_MODEL_TIER_APPROVED_PERMILLE_V1),
            None,
            j,
        )
    }

    /// **[`Self::roll_epoch_v3`] with ADR-0179's model tiers** (user decision 2026-10-15, round 4; `palw_model_tier_v1`, dormant):
    /// `tier(model) = (registered, c_permille)`. Each live row's weight is `A_m = c_m · S̄_m^α` — the ONE place the tier is applied;
    /// the resulting `P_m = A_m / ΣA` feeds RFC-0016 V+ block rights and the coinbase allocation alike, and no accepted block's
    /// reward is multiplied by the tier again. With `registered_cap_permille = Some(ρ)` the registered (0.3) models' weights are
    /// scaled so they total at most `ρ` of `ΣA`, and `ΣA` is the sum of the SAME scaled weights (user correction 2026-10-22: the header
    /// once kept the unscaled `ΣA`, which let registered capital dilute the approved tier whatever `ρ` was). N registrations never
    /// multiply the tier's share; the approved tier keeps at least `1 − ρ` of the allocation.
    #[allow(clippy::too_many_arguments)]
    pub fn roll_epoch_v3_tiered(
        &mut self,
        policy: &PalwModelAllocationPolicyV1,
        index: u64,
        start_daa: u64,
        capital_of: impl Fn(&PalwBondKeyV2) -> (u64, bool),
        verifier: &BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>>,
        eligible_capital: impl Fn(&Hash64, u128) -> u128,
        tier: impl Fn(&Hash64) -> (bool, u32),
        registered_cap_permille: Option<u32>,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) {
        let n = policy.average_epochs().unwrap_or(1);
        // 1. Seasoning, as the V2 roll.
        let mut miner: BTreeMap<PalwBondKeyV2, Vec<(Hash64, u64)>> = BTreeMap::new();
        let bonds: Vec<PalwBondKeyV2> = self.assignments.keys().copied().collect();
        for bond in bonds {
            let mut row = self.assignments.get(&bond).cloned().expect("listed above");
            if row.pending.is_some()
                && index >= row.pending_since_epoch.saturating_add(1).saturating_add(policy.seasoning_epochs as u64)
            {
                row.effective = row.pending.take().expect("checked");
            }
            let amounts: Vec<(Hash64, u64)> = match &row.pending {
                None => row.effective.clone(),
                Some(pending) => {
                    let pending: BTreeMap<Hash64, u64> = pending.iter().copied().collect();
                    row.effective.iter().map(|(m, a)| (*m, (*a).min(pending.get(m).copied().unwrap_or(0)))).collect()
                }
            };
            miner.insert(bond, amounts);
            put(&mut self.assignments, PALW_BUDGET_TABLE_ASSIGNMENTS_V1, &bond, Some(row), j);
        }
        // 2. This epoch's snapshot, recorded.
        let snapshot = palw_allocation_snapshot_v3(&miner, verifier, &capital_of);
        for (model, capital) in &snapshot {
            let capital = eligible_capital(model, *capital);
            if capital == 0 {
                continue;
            }
            let row = PalwModelBudgetRowV1 {
                epoch: index,
                capital,
                weight: model_as_weight(model),
                reserved_sompi: PALW_ALLOCATION_HISTORY_MARKER_V1,
            };
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &palw_allocation_history_key_v1(model, index), Some(row), j);
        }
        // 3. Drop history outside the span and every earlier epoch's block reservations.
        let stale_aux: Vec<Hash64> = self
            .models
            .iter()
            .filter(|(_, row)| match row.reserved_sompi {
                PALW_ALLOCATION_HISTORY_MARKER_V1 => row.epoch.saturating_add(n) <= index,
                PALW_ALLOCATION_BLOCKS_MARKER_V1 => row.epoch < index,
                _ => false,
            })
            .map(|(k, _)| *k)
            .collect();
        for key in stale_aux {
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &key, None, j);
        }
        // 4. The averages over the span (a model absent from an epoch counted 0 there).
        let mut sums: BTreeMap<Hash64, u128> = BTreeMap::new();
        for row in self.models.values().filter(|row| row.reserved_sompi == PALW_ALLOCATION_HISTORY_MARKER_V1) {
            *sums.entry(weight_as_model(&row.weight)).or_default() += row.capital;
        }
        let averages: BTreeMap<Hash64, u128> = sums
            .into_iter()
            .map(|(m, total)| {
                let average = eligible_capital(&m, total / n as u128);
                (m, average)
            })
            .filter(|(_, avg)| *avg > 0)
            .collect();
        // 5. Live rows: drop the models that left, write the rest.
        let stale: Vec<Hash64> = self
            .models
            .iter()
            .filter(|(k, row)| !Self::is_allocation_aux_row_v1(row) && !averages.contains_key(*k))
            .map(|(k, _)| *k)
            .collect();
        for model in stale {
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &model, None, j);
        }
        // ADR-0179: `A_m = c_m · S̄_m^α`, once; then the registered tier's aggregate cap.
        let mut sum_a = PalwAllocationWeightV1::ZERO;
        let mut registered_sum = PalwAllocationWeightV1::ZERO;
        let mut rows: Vec<(Hash64, u128, PalwAllocationWeightV1, bool)> = Vec::with_capacity(averages.len());
        for (model, average) in averages {
            let (registered, c_permille) = tier(&model);
            let weight = crate::palw_model_tier_v1::palw_model_tier_weight_v1(policy.curve.weight(average), c_permille);
            sum_a = sum_a.saturating_add(weight);
            if registered {
                registered_sum = registered_sum.saturating_add(weight);
            }
            rows.push((model, average, weight, registered));
        }
        let canonical_sum = sum_a.saturating_sub(registered_sum);
        let factor = registered_cap_permille
            .map(|cap| crate::palw_model_tier_v1::palw_model_tier_cap_factor_ppm_v1(canonical_sum, registered_sum, cap))
            .unwrap_or(1_000_000);
        // ADR-0179 (user correction 2026-10-22): `ΣA` is the sum of the same scaled `A_m` the rows carry. Identical to the sum above
        // whenever the cap does not bind (factor 1,000,000: every weight unchanged), so the untiered roll is byte for byte the old one.
        if factor < 1_000_000 {
            sum_a = PalwAllocationWeightV1::ZERO;
        }
        for (model, average, weight, registered) in rows {
            let weight = if registered { crate::palw_model_tier_v1::palw_model_tier_scale_v1(weight, factor) } else { weight };
            if factor < 1_000_000 {
                sum_a = sum_a.saturating_add(weight);
            }
            put(
                &mut self.models,
                PALW_BUDGET_TABLE_MODELS_V1,
                &model,
                Some(PalwModelBudgetRowV1 { epoch: index, capital: average, weight, reserved_sompi: 0 }),
                j,
            );
        }
        let mut header = self.header.clone();
        header.allocation = Some(PalwAllocationEpochV1 { index, start_daa, sum_a, accrued_sompi: 0 });
        self.put_header(header, j);
    }

    /// **A model's remaining reward-bearing block opportunity at `now`** (block units): `⌊accrued · A_m / Σ A⌋ − reserved_m`, the
    /// accrued opportunity being one block per DAA of the epoch so far. 0 without an allocation, a live row or a denominator.
    pub fn model_blocks_available_v1(&self, model: &Hash64, now: u64) -> u64 {
        let Some(epoch) = self.header.allocation else { return 0 };
        let Some(row) = self.models.get(model).filter(|row| row.epoch == epoch.index && !Self::is_allocation_aux_row_v1(row)) else {
            return 0;
        };
        let accrued = now.saturating_sub(epoch.start_daa).saturating_add(1).saturating_mul(PALW_BUDGET_BLOCK_UNIT_V1);
        let reserved = self
            .models
            .get(&palw_allocation_blocks_key_v1(model, epoch.index))
            .map_or(0, |row| u64::try_from(row.capital).unwrap_or(u64::MAX));
        palw_model_available_v1(accrued, &row.weight, &epoch.sum_a, reserved)
    }

    /// **Reserve `units` of a model's block opportunity this epoch** (the claim's acceptance; never returned early).
    pub fn reserve_model_blocks_v1(&mut self, model: &Hash64, units: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let Some(epoch) = self.header.allocation else { return };
        if units == 0 {
            return;
        }
        let key = palw_allocation_blocks_key_v1(model, epoch.index);
        let prior = self.models.get(&key).map_or(0, |row| row.capital);
        let row = PalwModelBudgetRowV1 {
            epoch: epoch.index,
            capital: prior.saturating_add(units as u128),
            weight: model_as_weight(model),
            reserved_sompi: PALW_ALLOCATION_BLOCKS_MARKER_V1,
        };
        put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &key, Some(row), j);
    }
}
