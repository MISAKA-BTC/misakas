//! **V+ — pre-issued per-model block rights** (RFC-0016 §8; the adopted physical-layer design, 2026-10-15). Pure arithmetic; the fold
//! reads it through `palw_state_v2::palw_rfc0016_fold_v1`.
//!
//! ```text
//! epoch e (the PALW epoch, `epoch_length` DAA)    N_exp = epoch_length · attempt_share / 1000   (the W step's own expectation)
//! entrant reserve                                  Ent   = ⌊N_exp · ε⌋, deducted FIRST from the fixed total
//! issue                                            Rt_m  = ⌊(N_exp − Ent) · A_m / ΣA⌋  (+ the entrant grant, linear, capped)
//! consume                                          one right per accepted model-class Attempt (the state's epoch counter)
//! admit                                            own right (used_m < Rt_m)                                   → Own
//!                                                  past φ, used_m < Rt_m·(1+σ) and the others' protected pace
//!                                                  leaves room in the epoch total                               → Reissued
//!                                                  never past Σ Rt (the hard epoch total)
//! expire                                           whatever is not consumed by the epoch's end is never minted; the floor and the
//!                                                  heartbeat fill that cadence under their own unchanged rules
//! ```
//!
//! Consumption is not a new ledger: it IS the state's per-class epoch production counter (`epoch_counters[class].produced_blocks`,
//! written once per accepted Attempt), mapped class → model. So the ledger is exactly as reorg-, restart- and IBD-consistent as the
//! counter the work target already reads, and needs no new state, delta, aux table or object.

use std::collections::BTreeMap;

use crate::Hash64;
use crate::palw_bond_budget_v1::{PalwAllocationWeightV1, PalwBondBudgetStateV1, palw_model_available_v1};
use crate::palw_state_v2::PalwEpochCounterV2;

use super::values::PalwRfc0016ValuesV1;

fn mul_div_u64(a: u64, b: u64, d: u64) -> u64 {
    if d == 0 {
        return 0;
    }
    ((a as u128).saturating_mul(b as u128) / d as u128).min(u64::MAX as u128) as u64
}

/// **`N_exp`**: the epoch's expected model blocks — the attempt lane's slice of the cadence, a DAA count and never a clock (the same
/// expression the work target's step reads).
pub fn palw_rights_n_exp_v1(epoch_length: u64, attempt_share_permille: u16) -> u64 {
    mul_div_u64(epoch_length, attempt_share_permille as u64, 1_000)
}

/// **One epoch's issued rights**, fixed at the epoch's start (the allocation is constant within it: validation requires the allocation
/// epoch to be a whole number of rights epochs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwRightsEpochV1 {
    pub epoch_index: u64,
    pub start_daa: u64,
    pub length: u64,
    pub n_exp: u64,
    /// The entrant reserve deducted from the fixed total (granted or not: an ungranted remainder is never minted).
    pub entrant_reserve: u64,
    /// `Rt_m` per model, entrant grants included. `Σ issued ≤ n_exp`.
    pub issued: BTreeMap<Hash64, u64>,
}

impl PalwRightsEpochV1 {
    pub fn issued_of(&self, model: &Hash64) -> u64 {
        self.issued.get(model).copied().unwrap_or(0)
    }

    /// The hard epoch total: no admission ever takes the epoch's consumption past it.
    pub fn total_issued(&self) -> u64 {
        self.issued.values().fold(0u64, |a, b| a.saturating_add(*b))
    }

    pub fn end_daa(&self) -> u64 {
        self.start_daa.saturating_add(self.length)
    }
}

/// **The narrow interface to ADR-0177's allocation** (`palw_model_bond_allocation_v1`, rewritten by the ADR lane to the 30-day
/// Miner + Verifier `S̄_m` and `α = 1.5`): the current allocation epoch's per-model weights `A_m` and `ΣA`. RFC-0016 consumes `P_m`
/// only through this trait; it does not compute `S_m` or `A_m` itself.
pub trait PalwRightsWeightsSourceV1 {
    /// `(A_m per model, ΣA)` of the allocation epoch in force, or `None` where no allocation epoch exists (then nothing is issued).
    fn rights_weights_v1(&self) -> Option<(BTreeMap<Hash64, PalwAllocationWeightV1>, PalwAllocationWeightV1)>;
    /// A model's seasoned locked capital `S_m` (the entrant split is linear in it), `0` if unknown.
    fn rights_capital_v1(&self, model: &Hash64) -> u128;
}

/// The dormant engine as it stands on this branch: the model rows of the current allocation epoch and its `ΣA`.
impl PalwRightsWeightsSourceV1 for PalwBondBudgetStateV1 {
    fn rights_weights_v1(&self) -> Option<(BTreeMap<Hash64, PalwAllocationWeightV1>, PalwAllocationWeightV1)> {
        let epoch = self.header.allocation?;
        // The ADR-0177 V3 engine keeps synthetic history / block-reservation rows in the same table: never a model's weight.
        let weights = self
            .models
            .iter()
            .filter(|(_, row)| row.epoch == epoch.index && !PalwBondBudgetStateV1::is_allocation_aux_row_v1(row))
            .map(|(m, row)| (*m, row.weight))
            .collect();
        Some((weights, epoch.sum_a))
    }

    fn rights_capital_v1(&self, model: &Hash64) -> u128 {
        let index = self.header.allocation.map(|e| e.index);
        self.models
            .get(model)
            .filter(|row| Some(row.epoch) == index && !PalwBondBudgetStateV1::is_allocation_aux_row_v1(row))
            .map(|row| row.capital)
            .unwrap_or(0)
    }
}

/// **The narrow interface to RFC-0017's model eligibility** (Registered / Probation / Eligible / Lapsed, added by the RFC-0017 lane at
/// admission, block-right issuance, coinbase allocation and Final-weight sites). Until that lane's module lands, [`PalwAllEligibleV1`]
/// stands in (every model eligible, no entrant clock), which issues no entrant grant.
pub trait PalwModelEligibilitySourceV1 {
    /// May `model` be issued block rights at `daa`? An ineligible model's `P_m` share is not issued and not redistributed (it expires).
    fn block_rights_eligible_v1(&self, model: &Hash64, daa: u64) -> bool;
    /// The DAA the model became eligible, if known (an entrant is one younger than the entrant age).
    fn eligible_since_daa_v1(&self, model: &Hash64) -> Option<u64>;
}

/// The stand-in until RFC-0017's eligibility is consumable here: every model eligible, no model an entrant.
#[derive(Clone, Copy, Debug, Default)]
pub struct PalwAllEligibleV1;

impl PalwModelEligibilitySourceV1 for PalwAllEligibleV1 {
    fn block_rights_eligible_v1(&self, _model: &Hash64, _daa: u64) -> bool {
        true
    }
    fn eligible_since_daa_v1(&self, _model: &Hash64) -> Option<u64> {
        None
    }
}

/// **Issue one epoch's rights** (MUST be the same on every node: a pure function of the epoch, `N_exp`, the allocation snapshot and the
/// eligibility answers). The entrant reserve is deducted first; the remainder is split `⌊base · A_m / ΣA⌋` over the ELIGIBLE weights
/// (an ineligible model's share expires, it is not re-split); entrants then take a linear, per-model-capped share of the reserve.
/// `Σ issued ≤ n_exp` by the floors.
#[allow(clippy::too_many_arguments)]
pub fn palw_rights_issue_v1(
    epoch_index: u64,
    start_daa: u64,
    length: u64,
    n_exp: u64,
    values: &PalwRfc0016ValuesV1,
    weights: Option<&(BTreeMap<Hash64, PalwAllocationWeightV1>, PalwAllocationWeightV1)>,
    entrants: &BTreeMap<Hash64, u128>,
    eligible: impl Fn(&Hash64) -> bool,
) -> PalwRightsEpochV1 {
    let entrant_reserve = mul_div_u64(n_exp, values.entrant_reserve_permille.min(1_000) as u64, 1_000);
    let base = n_exp.saturating_sub(entrant_reserve);
    let mut issued: BTreeMap<Hash64, u64> = BTreeMap::new();
    if let Some((weights, sum_a)) = weights {
        for (model, a_m) in weights {
            if !eligible(model) {
                continue;
            }
            let r = palw_model_available_v1(base, a_m, sum_a, 0);
            if r > 0 {
                issued.insert(*model, r);
            }
        }
    }
    // Entrants: linear in seasoned capital (split-neutral), each capped, only above `S_entry`.
    let cap = mul_div_u64(n_exp, values.entrant_max_per_model_permille as u64, 1_000);
    let floor = values.entrant_min_capital_sompi as u128;
    let qualifying: Vec<(Hash64, u64)> = entrants
        .iter()
        .filter(|(m, c)| **c >= floor && **c > 0 && eligible(m))
        .map(|(m, c)| (*m, (*c).min(u64::MAX as u128) as u64))
        .collect();
    let sum_c: u128 = qualifying.iter().fold(0u128, |a, (_, c)| a.saturating_add(*c as u128));
    if sum_c > 0 && entrant_reserve > 0 {
        let mut granted: u64 = 0;
        for (model, c) in qualifying {
            let share = ((entrant_reserve as u128).saturating_mul(c as u128) / sum_c).min(u64::MAX as u128) as u64;
            let g = share.min(cap).min(entrant_reserve.saturating_sub(granted));
            if g > 0 {
                granted += g;
                let slot = issued.entry(model).or_insert(0);
                *slot = slot.saturating_add(g);
            }
        }
    }
    PalwRightsEpochV1 { epoch_index, start_daa, length, n_exp, entrant_reserve, issued }
}

/// **The rights consumed so far in `epoch_index`**: each non-floor class's epoch production counter, summed per model. A counter of
/// another epoch is zero for this one (the fold starts a new epoch's count at its first Attempt).
pub fn palw_rights_consumed_v1(
    counters: &BTreeMap<Hash64, PalwEpochCounterV2>,
    epoch_index: u64,
    base_class: &Hash64,
    model_of: impl Fn(&Hash64) -> Hash64,
) -> BTreeMap<Hash64, u64> {
    let mut out: BTreeMap<Hash64, u64> = BTreeMap::new();
    for (class, counter) in counters {
        if class == base_class || counter.epoch_index != epoch_index || counter.produced_blocks == 0 {
            continue;
        }
        let slot = out.entry(model_of(class)).or_insert(0);
        *slot = slot.saturating_add(counter.produced_blocks);
    }
    out
}

/// **An under-demand model's protected remaining rights past φ**: before φ all its unused rights; past it `min(unused, ⌈used ·
/// remaining / elapsed · margin⌉)` — the rights its own pace still needs. A model that has used nothing by φ protects nothing.
pub fn palw_rights_protected_v1(issued: u64, used: u64, elapsed: u64, length: u64, values: &PalwRfc0016ValuesV1) -> u64 {
    let unused = issued.saturating_sub(used);
    if !palw_rights_past_checkpoint_v1(elapsed, length, values) {
        return unused;
    }
    let remaining = length.saturating_sub(elapsed) as u128;
    let numerator = (used as u128).saturating_mul(remaining).saturating_mul(values.pace_margin_permille as u128);
    let denominator = (elapsed.max(1) as u128).saturating_mul(1_000);
    let need = numerator.div_ceil(denominator).min(u64::MAX as u128) as u64;
    unused.min(need)
}

/// Is `elapsed` (DAA since the epoch's start) at or past the checkpoint φ?
pub fn palw_rights_past_checkpoint_v1(elapsed: u64, length: u64, values: &PalwRfc0016ValuesV1) -> bool {
    (elapsed as u128).saturating_mul(1_000) >= (length as u128).saturating_mul(values.reissue_checkpoint_permille as u128)
}

/// How a right was found for an Attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRightsUseV1 {
    /// One of the model's own issued rights.
    Own,
    /// A right released by an under-demand model past φ, within the model's σ cap.
    Reissued,
}

/// Why an Attempt finds no right (the refusal is before acceptance; the pre-Attempt quote shows it first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRightsRefusalV1 {
    /// The model was issued nothing this epoch (no allocation weight, ineligible, or `ΣA = 0`).
    NoRightsIssued,
    /// Its own rights are used and the checkpoint φ is not reached.
    OwnRightsExhausted,
    /// Its σ cap is reached.
    SigmaCapReached,
    /// No released right is left beside the others' protected pace.
    NoReleasedRights,
    /// The epoch's hard total is consumed.
    EpochTotalReached,
}

impl PalwRightsRefusalV1 {
    /// The refusal code the fold reports (`ClassNotRewardable { code, .. }`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoRightsIssued => "RFC0016_NO_RIGHTS_ISSUED",
            Self::OwnRightsExhausted => "RFC0016_OWN_RIGHTS_EXHAUSTED",
            Self::SigmaCapReached => "RFC0016_SIGMA_CAP_REACHED",
            Self::NoReleasedRights => "RFC0016_NO_RELEASED_RIGHTS",
            Self::EpochTotalReached => "RFC0016_EPOCH_TOTAL_REACHED",
        }
    }

    pub fn why(&self) -> &'static str {
        match self {
            Self::NoRightsIssued => "the model holds no block right this epoch (RFC-0016 V+)",
            Self::OwnRightsExhausted => {
                "the model's own block rights are used and the reissue checkpoint is not reached (RFC-0016 V+)"
            }
            Self::SigmaCapReached => "the model reached its sigma cap on reissued block rights (RFC-0016 V+)",
            Self::NoReleasedRights => "no released block right is left beside the other models' protected pace (RFC-0016 V+)",
            Self::EpochTotalReached => "the epoch's block rights are all consumed (RFC-0016 V+)",
        }
    }
}

/// **The admission rule** for one model-class Attempt of `model` at `now_daa` (the rights as consumed BEFORE this Attempt).
pub fn palw_rights_admit_v1(
    epoch: &PalwRightsEpochV1,
    consumed: &BTreeMap<Hash64, u64>,
    model: &Hash64,
    now_daa: u64,
    values: &PalwRfc0016ValuesV1,
) -> Result<PalwRightsUseV1, PalwRightsRefusalV1> {
    let total = epoch.total_issued();
    let total_used = consumed.values().fold(0u64, |a, b| a.saturating_add(*b));
    if total_used >= total {
        return Err(if total == 0 { PalwRightsRefusalV1::NoRightsIssued } else { PalwRightsRefusalV1::EpochTotalReached });
    }
    let issued_m = epoch.issued_of(model);
    let used_m = consumed.get(model).copied().unwrap_or(0);
    if used_m < issued_m {
        return Ok(PalwRightsUseV1::Own);
    }
    if issued_m == 0 {
        return Err(PalwRightsRefusalV1::NoRightsIssued);
    }
    let elapsed = now_daa.saturating_sub(epoch.start_daa).min(epoch.length);
    if !palw_rights_past_checkpoint_v1(elapsed, epoch.length, values) {
        return Err(PalwRightsRefusalV1::OwnRightsExhausted);
    }
    let cap = issued_m.saturating_add(mul_div_u64(issued_m, values.sigma_permille as u64, 1_000));
    if used_m >= cap {
        return Err(PalwRightsRefusalV1::SigmaCapReached);
    }
    let protected_others = epoch
        .issued
        .iter()
        .filter(|(j, _)| *j != model)
        .map(|(j, issued_j)| palw_rights_protected_v1(*issued_j, consumed.get(j).copied().unwrap_or(0), elapsed, epoch.length, values))
        .fold(0u64, u64::saturating_add);
    if total_used.saturating_add(protected_others).saturating_add(1) > total {
        return Err(PalwRightsRefusalV1::NoReleasedRights);
    }
    Ok(PalwRightsUseV1::Reissued)
}

/// **The pre-Attempt rights quote** (RFC-0016 §8.1 step 7): what the producer reads BEFORE spending a forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRightsQuoteV1 {
    pub epoch_index: u64,
    pub epoch_end_daa: u64,
    pub issued: u64,
    pub used: u64,
    pub remaining_own: u64,
    /// The σ cap's remaining room (rights beyond its own it may still take past φ).
    pub reissue_room: u64,
    pub past_checkpoint: bool,
    pub epoch_total: u64,
    pub epoch_used: u64,
    /// The admission verdict for one more Attempt now.
    pub verdict: Result<PalwRightsUseV1, PalwRightsRefusalV1>,
}

pub fn palw_rights_quote_v1(
    epoch: &PalwRightsEpochV1,
    consumed: &BTreeMap<Hash64, u64>,
    model: &Hash64,
    now_daa: u64,
    values: &PalwRfc0016ValuesV1,
) -> PalwRightsQuoteV1 {
    let issued = epoch.issued_of(model);
    let used = consumed.get(model).copied().unwrap_or(0);
    let cap = issued.saturating_add(mul_div_u64(issued, values.sigma_permille as u64, 1_000));
    let elapsed = now_daa.saturating_sub(epoch.start_daa).min(epoch.length);
    PalwRightsQuoteV1 {
        epoch_index: epoch.epoch_index,
        epoch_end_daa: epoch.end_daa(),
        issued,
        used,
        remaining_own: issued.saturating_sub(used),
        reissue_room: cap.saturating_sub(used.max(issued)),
        past_checkpoint: palw_rights_past_checkpoint_v1(elapsed, epoch.length, values),
        epoch_total: epoch.total_issued(),
        epoch_used: consumed.values().fold(0u64, |a, b| a.saturating_add(*b)),
        verdict: palw_rights_admit_v1(epoch, consumed, model, now_daa, values),
    }
}

/// **The optional auxiliary ticket tilt `τ_m`** (RFC-0016 §8.5; the meaning of `k`): `clamp(P_m / c_m, 1/k, k)` in permille, where
/// `target_permille` is the model's `P_m` and `physical_permille` its realized physical block share. `1000` (no tilt) when `k ≤ 1` or
/// either share is zero. NOT wired into the lottery in this implementation (`k = 1` until measured and separately decided).
pub fn palw_rights_tilt_permille_v1(target_permille: u32, physical_permille: u32, k_permille: u32) -> u32 {
    if k_permille <= 1_000 || target_permille == 0 || physical_permille == 0 {
        return 1_000;
    }
    let ratio = (target_permille as u64 * 1_000 / physical_permille as u64).min(u32::MAX as u64) as u32;
    let low = (1_000_000u64 / k_permille as u64) as u32;
    ratio.clamp(low, k_permille)
}

/// **B+ (the considered alternative, kept for comparison): a model's epoch-close payout fraction `ψ`** in permille —
/// `min(1, (min(budget, demand) + spill_in) / demand)`; `1000` with no demand.
pub fn palw_bplus_psi_permille_v1(budget: u64, demand: u64, spill_in: u64) -> u32 {
    if demand == 0 {
        return 1_000;
    }
    let paid = budget.min(demand).saturating_add(spill_in).min(demand);
    (paid as u128 * 1_000 / demand as u128) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(i: u64) -> Hash64 {
        Hash64::from_u64_word(i)
    }

    /// A TEST value set (not a proposal).
    fn v(sigma: u32, phi: u32) -> PalwRfc0016ValuesV1 {
        PalwRfc0016ValuesV1 {
            sigma_permille: sigma,
            tilt_k_permille: 1_000,
            reissue_checkpoint_permille: phi,
            pace_margin_permille: 1_000,
            entrant_reserve_permille: 100,
            entrant_max_per_model_permille: 50,
            entrant_min_capital_sompi: 10,
            entrant_age_daa: 1,
            ..Default::default()
        }
    }

    fn w(x: u64) -> PalwAllocationWeightV1 {
        PalwAllocationWeightV1::from_u64(x)
    }

    fn epoch(issued: &[(u64, u64)]) -> PalwRightsEpochV1 {
        PalwRightsEpochV1 {
            epoch_index: 0,
            start_daa: 0,
            length: 100,
            n_exp: issued.iter().map(|(_, r)| r).sum(),
            entrant_reserve: 0,
            issued: issued.iter().map(|(m, r)| (h(*m), *r)).collect(),
        }
    }

    /// T2 / G1t: more weight, more issued rights; N1: the entrant reserve comes out of the fixed total.
    #[test]
    fn issue_follows_weight_and_deducts_the_entrant_reserve() {
        let weights: BTreeMap<_, _> = [(h(1), w(3)), (h(2), w(1))].into_iter().collect();
        let both = (weights, w(4));
        let e = palw_rights_issue_v1(0, 0, 100, 100, &v(0, 500), Some(&both), &BTreeMap::new(), |_| true);
        assert_eq!(e.entrant_reserve, 10);
        assert_eq!((e.issued_of(&h(1)), e.issued_of(&h(2))), (67, 22), "⌊90 · 3/4⌋, ⌊90 · 1/4⌋");
        assert!(e.total_issued() <= e.n_exp);
        let entrants: BTreeMap<_, _> = [(h(9), 100u128), (h(8), 5u128)].into_iter().collect();
        let e2 = palw_rights_issue_v1(0, 0, 100, 100, &v(0, 500), Some(&both), &entrants, |_| true);
        assert_eq!(e2.issued_of(&h(9)), 5, "linear share of 10, capped at 5 % of N_exp");
        assert_eq!(e2.issued_of(&h(8)), 0, "below S_entry");
        assert!(e2.total_issued() <= e2.n_exp, "the reserve is inside the fixed total, never extra");
    }

    /// No allocation, zero ΣA, or an ineligible model: nothing issued, and an ineligible share is not re-split.
    #[test]
    fn zero_denominator_and_ineligible_issue_nothing_extra() {
        let e = palw_rights_issue_v1(0, 0, 100, 100, &v(0, 500), None, &BTreeMap::new(), |_| true);
        assert_eq!(e.total_issued(), 0);
        let weights: BTreeMap<_, _> = [(h(1), w(1)), (h(2), w(1))].into_iter().collect();
        let both = (weights, w(2));
        let e = palw_rights_issue_v1(0, 0, 100, 100, &v(0, 500), Some(&both), &BTreeMap::new(), |m| *m != h(2));
        assert_eq!(e.issued_of(&h(1)), 45, "model 2's half expires; model 1 is not raised");
        assert_eq!(e.issued_of(&h(2)), 0);
    }

    /// The hard epoch total and the own-right rule before φ.
    #[test]
    fn own_rights_then_refusal_before_the_checkpoint() {
        let e = epoch(&[(1, 2), (2, 2)]);
        let mut used = BTreeMap::new();
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(1), 10, &v(500, 500)), Ok(PalwRightsUseV1::Own));
        used.insert(h(1), 2);
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(1), 10, &v(500, 500)), Err(PalwRightsRefusalV1::OwnRightsExhausted));
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(3), 10, &v(500, 500)), Err(PalwRightsRefusalV1::NoRightsIssued));
    }

    /// Past φ an over-demand model takes released rights, within σ, never past the epoch total; the others' pace is protected.
    #[test]
    fn reissue_is_sigma_capped_and_conserves_the_epoch_total() {
        let e = epoch(&[(1, 10), (2, 10)]);
        let mut used: BTreeMap<Hash64, u64> = [(h(1), 10), (h(2), 0)].into_iter().collect();
        // Model 2 used nothing by φ: it protects nothing; model 1 may take up to 10 · 1.5 = 15.
        for _ in 0..5 {
            assert_eq!(palw_rights_admit_v1(&e, &used, &h(1), 60, &v(500, 500)), Ok(PalwRightsUseV1::Reissued));
            *used.get_mut(&h(1)).unwrap() += 1;
        }
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(1), 60, &v(500, 500)), Err(PalwRightsRefusalV1::SigmaCapReached));
        // Model 2 still has its own rights until the hard total is reached.
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(2), 60, &v(500, 500)), Ok(PalwRightsUseV1::Own));
        let total: u64 = used.values().sum();
        assert!(total <= e.total_issued());
        // σ = 0: strict P_m.
        let used0: BTreeMap<Hash64, u64> = [(h(1), 10)].into_iter().collect();
        assert_eq!(palw_rights_admit_v1(&e, &used0, &h(1), 60, &v(0, 500)), Err(PalwRightsRefusalV1::SigmaCapReached));
    }

    /// A model still on pace past φ keeps its projected need: the over-demand model cannot take it.
    #[test]
    fn an_on_pace_model_is_protected() {
        let e = epoch(&[(1, 10), (2, 10)]);
        // At elapsed 50 of 100, model 2 used 5: its pace needs 5 more (margin 1.0) — all of its unused rights.
        let used: BTreeMap<Hash64, u64> = [(h(1), 10), (h(2), 5)].into_iter().collect();
        assert_eq!(palw_rights_protected_v1(10, 5, 50, 100, &v(500, 500)), 5);
        assert_eq!(palw_rights_admit_v1(&e, &used, &h(1), 50, &v(1_000, 500)), Err(PalwRightsRefusalV1::NoReleasedRights));
    }

    #[test]
    fn the_quote_matches_the_admission() {
        let e = epoch(&[(1, 3)]);
        let used: BTreeMap<Hash64, u64> = [(h(1), 1)].into_iter().collect();
        let q = palw_rights_quote_v1(&e, &used, &h(1), 5, &v(0, 500));
        assert_eq!((q.issued, q.used, q.remaining_own), (3, 1, 2));
        assert_eq!(q.verdict, palw_rights_admit_v1(&e, &used, &h(1), 5, &v(0, 500)));
    }

    #[test]
    fn the_tilt_is_off_at_k_one_and_bounded_otherwise() {
        assert_eq!(palw_rights_tilt_permille_v1(800, 200, 1_000), 1_000);
        assert_eq!(palw_rights_tilt_permille_v1(800, 200, 2_000), 2_000, "4× clamped to k = 2");
        assert_eq!(palw_rights_tilt_permille_v1(200, 800, 2_000), 500, "1/4 clamped to 1/k");
    }

    #[test]
    fn bplus_psi_is_bounded() {
        assert_eq!(palw_bplus_psi_permille_v1(5, 10, 0), 500);
        assert_eq!(palw_bplus_psi_permille_v1(5, 10, 100), 1_000);
        assert_eq!(palw_bplus_psi_permille_v1(5, 0, 0), 1_000);
    }
}
