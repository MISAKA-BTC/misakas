//! **RFC-0016 in the fold** — a child module of `palw_state_v2` (so it reads the state's epoch counters, the readiness predicate and the
//! panel demand terms the way the fold does). Dormant: every entry point returns "no change" unless the V2 mirror of
//! `palw_model_block_rights_v1` / `palw_elastic_final_capacity_v1` is in force, and validation refuses both armed.
//!
//! * [`PalwFoldReadV1::rfc0016_rights_admit_v1`] — V+: a model-class Attempt is admitted only with a block right of its model (asked by
//!   `check_class_admits_claim`, so the producer's pre-check `palw_class_admits_claim_v1` answers it BEFORE the forward).
//! * [`PalwFoldReadV1::rfc0016_elastic_room_v1`] — the per-verifier water-filled room replacing the cross-class rate room, capacity 0
//!   admits nothing, and the C7 concurrency derived from capacity.
//! * [`palw_rfc0016_quote_v1`] — the combined pre-lottery quote for a producer or an RPC reader.
//! * [`palw_rfc0016_model_report_v1`] — the per-model and per-miner report rows (targets apart from outcomes).
//!
//! Interfaces assumed from other lanes (not edited here): `P_m` through `PalwRightsWeightsSourceV1` (implemented for the budget
//! engine's current allocation epoch; the ADR-0177 lane's 30-day Miner + Verifier `S̄_m` lands behind the same rows), eligibility
//! through `PalwModelEligibilitySourceV1` (the stand-in `PalwAllEligibleV1` until RFC-0017's gates are consumable).

use super::*;
use crate::palw_rfc0016_v1::capacity::{
    PalwAdmissionQuoteV1, PalwVerifierSupplyV1, palw_admission_quote_v1, palw_c7_derived_cap_v1, palw_capacity_fits_v1,
    palw_waterfill_capacity_v1,
};
use crate::palw_rfc0016_v1::report::{PalwMinerRevenueRowV1, PalwModelReportRowV1};
use crate::palw_rfc0016_v1::rights::{
    PalwAllEligibleV1, PalwModelEligibilitySourceV1, PalwRightsEpochV1, PalwRightsWeightsSourceV1, palw_rights_admit_v1,
    palw_rights_consumed_v1, palw_rights_issue_v1, palw_rights_n_exp_v1, palw_rights_quote_v1,
};
use crate::palw_rfc0016_v1::values::{PALW_RFC0016_VALUES_V1, PalwRfc0016ValuesV1};

/// A class's model (its artifact line, as the budget engine keys models; the class itself where it has none).
pub fn palw_rfc0016_model_of_v1(state: &PalwChainStateV2, ownership_active: bool, class_id: &Hash64) -> Hash64 {
    state
        .class(class_id)
        .and_then(|row| state.artifact_line_of_root(class_id, &row.artifact_root, ownership_active))
        .unwrap_or(*class_id)
}

/// **The epoch's issued rights at `now_daa`** — a pure function of the epoch, the allocation snapshot (constant within the epoch) and
/// the eligibility answers AS OF THE EPOCH'S START.
pub fn palw_rfc0016_rights_epoch_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    eligibility: &dyn PalwModelEligibilitySourceV1,
    now_daa: u64,
    values: &PalwRfc0016ValuesV1,
) -> PalwRightsEpochV1 {
    let length = params.epoch_length().max(1);
    let epoch_index = now_daa / length;
    let start = epoch_index.saturating_mul(length);
    let n_exp = palw_rights_n_exp_v1(length, params.fp_attempt_share_permille());
    let budget = state.bond_budget.as_ref();
    let weights = budget.and_then(|b| b.rights_weights_v1());
    let mut entrants: BTreeMap<Hash64, u128> = BTreeMap::new();
    if let Some(budget) = budget
        && values.entrant_age_daa > 0
    {
        for model in budget.models.keys() {
            let young =
                eligibility.eligible_since_daa_v1(model).is_some_and(|since| start.saturating_sub(since) < values.entrant_age_daa);
            if young {
                entrants.insert(*model, budget.rights_capital_v1(model));
            }
        }
    }
    palw_rights_issue_v1(epoch_index, start, length, n_exp, values, weights.as_ref(), &entrants, |m| {
        eligibility.block_rights_eligible_v1(m, start)
    })
}

/// **The rights consumed so far in `epoch_index`**: the state's per-class epoch production counters, per model.
pub fn palw_rfc0016_rights_consumed_now_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ownership_active: bool,
    epoch_index: u64,
) -> BTreeMap<Hash64, u64> {
    let base = params.base_class_id();
    palw_rights_consumed_v1(&state.epoch_counters, epoch_index, &base, |class| {
        palw_rfc0016_model_of_v1(state, ownership_active, class)
    })
}

impl PalwFoldReadV1<'_> {
    /// **V+ admission** (RFC-0016 §8.1 step 2): past `palw_model_block_rights_v1`, a model-class Attempt finds one of its model's block
    /// rights (own, or a σ-capped reissued one past φ, never past the epoch total), or it is refused before acceptance. `Ok` below the
    /// fence and for the floor class (the floor and the heartbeat keep their own unchanged rules).
    pub(super) fn rfc0016_rights_admit_v1(&self, class_id: &Hash64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.params.rfc0016_rights_active_at(now_daa) || *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(mirror) = self.params.rfc0016_v1() else { return Ok(()) };
        let ownership = self.extras.artifact_root_ownership_active;
        let epoch = palw_rfc0016_rights_epoch_v1(self.state, self.params, &PalwAllEligibleV1, now_daa, &mirror.values);
        let consumed = palw_rfc0016_rights_consumed_now_v1(self.state, self.params, ownership, epoch.epoch_index);
        let model = palw_rfc0016_model_of_v1(self.state, ownership, class_id);
        palw_rights_admit_v1(&epoch, &consumed, &model, now_daa, &mirror.values).map(|_| ()).map_err(|refusal| {
            PalwStateV2Error::ClassNotRewardable { class: *class_id, code: refusal.code(), why: refusal.why().to_string() }
        })
    }

    /// **The water-filled capacity read** of `class_id` (scaled like the demand terms, `× PALW_PANEL_DEMAND_SCALE_V1`): every ready
    /// verifier's per-span replay split over the classes it is ready for, by demand; its demand with `extra` pooled in; and the number
    /// of ready verifiers holding it. O(bonds × classes) readiness reads — bounded by the registry's tables; dormant.
    pub(super) fn rfc0016_capacity_read_v1(
        &self,
        class_id: &Hash64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
        extra: Option<(Hash64, PalwGatedClaimV1)>,
    ) -> (u128, u128, u32) {
        let g = &fold.globals;
        let seat = palw_panel_shipped_per_span_v1(1, g).saturating_mul(crate::palw_work_target_v1::PALW_PANEL_DEMAND_SCALE_V1);
        let base = self.params.base_class_id();
        let classes: Vec<Hash64> = self.state.model_lifecycles.keys().filter(|c| **c != base).copied().collect();
        let mut verifiers: Vec<PalwVerifierSupplyV1> = Vec::new();
        let mut ready_here: u32 = 0;
        for (bond_key, bond) in self.state.bonds.iter() {
            let ready: BTreeSet<Hash64> =
                classes.iter().filter(|c| self.model_registry_seat_is_ready(bond_key, bond, c, now_daa, fold)).copied().collect();
            if ready.contains(class_id) {
                ready_here = ready_here.saturating_add(1);
            }
            if !ready.is_empty() {
                verifiers.push(PalwVerifierSupplyV1 { per_span: seat, classes: ready });
            }
        }
        let terms = self.with_inflight_index(|index| {
            let owed = palw_panel_owed_v1(self.state, self.params, index, self.own_attempt_class, extra);
            palw_panel_terms_v1(self.state, g.seat_count as u128, &owed)
        });
        let capacity = palw_waterfill_capacity_v1(&verifiers, &terms);
        (capacity.get(class_id).copied().unwrap_or(0), terms.get(class_id).copied().unwrap_or(0), ready_here)
    }

    /// **The elastic room** (RFC-0016 §10): `None` below `palw_elastic_final_capacity_v1` (the shipped room decides); past it the class's
    /// demand with this claim must fit its water-filled capacity (capacity 0 admits nothing), and a C7 (long-window) class is held to
    /// the concurrency derived from that capacity instead of its static `max_inflight_claims`.
    pub(super) fn rfc0016_elastic_room_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
        whole: u64,
        incoming: PalwGatedClaimV1,
    ) -> Option<Result<(), PalwStateV2Error>> {
        if !self.params.rfc0016_capacity_active_at(now_daa) {
            return None;
        }
        let mirror = self.params.rfc0016_v1()?;
        let (capacity, demand, _ready) = self.rfc0016_capacity_read_v1(class_id, fold, now_daa, Some((*class_id, incoming)));
        let window = row.profile.verification_window_spans as u64;
        if !palw_capacity_fits_v1(capacity, demand) {
            return Some(Err(PalwStateV2Error::PanelRoomExhausted {
                class: *class_id,
                inflight_replay: demand,
                budget: capacity,
                horizon_spans: window,
            }));
        }
        if palw_panel_holds_to_final_v1(self.params, self.state, class_id) {
            let claim_replay = row.work.economic_ccu_per_claim.saturating_mul(fold.globals.seat_count as u128);
            let unscaled = capacity / crate::palw_work_target_v1::PALW_PANEL_DEMAND_SCALE_V1;
            let cap = palw_c7_derived_cap_v1(unscaled, window, claim_replay, &mirror.values);
            let inflight = self.model_registry_inflight(class_id);
            if (inflight as u64).saturating_add(whole) > cap as u64 {
                return Some(Err(PalwStateV2Error::ClassInflightCapped { class: *class_id, inflight, cap }));
            }
        }
        Some(Ok(()))
    }
}

/// **The combined pre-lottery admission quote** for one more Attempt of `class_id` at `now_daa` (`None` where neither RFC-0016 fence is
/// armed). The pending queue is not wired into the claim lifecycle in this implementation, so its length reads as 0.
pub fn palw_rfc0016_quote_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    class_id: &Hash64,
    now_daa: u64,
) -> Option<PalwAdmissionQuoteV1> {
    let mirror = params.rfc0016_v1()?;
    let ownership = extras.artifact_root_ownership_active;
    let rights = params.rfc0016_rights_active_at(now_daa).then(|| {
        let epoch = palw_rfc0016_rights_epoch_v1(state, params, &PalwAllEligibleV1, now_daa, &mirror.values);
        let consumed = palw_rfc0016_rights_consumed_now_v1(state, params, ownership, epoch.epoch_index);
        let model = palw_rfc0016_model_of_v1(state, ownership, class_id);
        palw_rights_quote_v1(&epoch, &consumed, &model, now_daa, &mirror.values)
    });
    let capacity = if params.rfc0016_capacity_active_at(now_daa) {
        let read = PalwFoldReadV1::outside(state, params, extras);
        extras
            .model_registry
            .as_ref()
            .map(|fold| read.rfc0016_capacity_read_v1(class_id, fold, now_daa, Some((*class_id, PalwGatedClaimV1::Attempt))))
    } else {
        None
    };
    Some(palw_admission_quote_v1(rights, capacity, 0, &mirror.values))
}

/// **The per-model and per-miner report** at `now_daa` (RFC-0016 §11): consensus-derived where the state holds it, `None` (UNKNOWN)
/// where it does not. Readable below the fences too (with the placeholder values) so the shadow can be measured; it changes nothing.
pub fn palw_rfc0016_model_report_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    now_daa: u64,
) -> (Vec<PalwModelReportRowV1>, Vec<PalwMinerRevenueRowV1>) {
    let values = params.rfc0016_v1().map(|m| m.values).unwrap_or(PALW_RFC0016_VALUES_V1);
    let ownership = extras.artifact_root_ownership_active;
    let epoch = palw_rfc0016_rights_epoch_v1(state, params, &PalwAllEligibleV1, now_daa, &values);
    let consumed = palw_rfc0016_rights_consumed_now_v1(state, params, ownership, epoch.epoch_index);
    let weights = state.bond_budget.as_ref().and_then(|b| b.rights_weights_v1());
    let mut models: BTreeSet<Hash64> = epoch.issued.keys().chain(consumed.keys()).copied().collect();
    if let Some((w, _)) = weights.as_ref() {
        models.extend(w.keys().copied());
    }
    // Finals per model (all epochs).
    let mut finals: BTreeMap<Hash64, u64> = BTreeMap::new();
    for claim in state.claims.values() {
        if matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
            let slot = finals.entry(palw_rfc0016_model_of_v1(state, ownership, &claim.class_id)).or_insert(0);
            *slot = slot.saturating_add(1);
        }
    }
    // Coinbase-derived payout per model and per (bond, model), from the budget engine's consumed reward.
    let mut payout: BTreeMap<Hash64, u128> = BTreeMap::new();
    let mut per_miner: BTreeMap<(PalwBondKeyV2, Hash64), (u128, u64)> = BTreeMap::new();
    if let Some(budget) = state.bond_budget.as_ref() {
        for row in budget.claims.values() {
            let Some((model, _)) = row.model else { continue };
            let paid = row.consumed.reward_sompi as u128;
            let slot = payout.entry(model).or_insert(0);
            *slot = slot.saturating_add(paid);
            let miner = per_miner.entry((row.bond, model)).or_insert((0, 0));
            miner.0 = miner.0.saturating_add(paid);
            miner.1 = miner.1.saturating_add(1);
        }
    }
    let rows = models
        .into_iter()
        .map(|model| {
            let issued = epoch.issued_of(&model);
            let used = consumed.get(&model).copied().unwrap_or(0);
            let target_p_ppm = weights
                .as_ref()
                .and_then(|(w, sum)| w.get(&model).map(|a| crate::palw_bond_budget_v1::palw_model_available_v1(1_000_000, a, sum, 0)));
            PalwModelReportRowV1 {
                model,
                epoch_index: epoch.epoch_index,
                target_p_ppm,
                rights_issued: issued,
                physical_blocks: used,
                rights_consumed: if params.rfc0016_rights_active_at(now_daa) { used } else { 0 },
                rights_unused: if now_daa >= epoch.end_daa() { issued.saturating_sub(used) } else { 0 },
                finals: Some(finals.get(&model).copied().unwrap_or(0)),
                payout_coinbase_sompi: state.bond_budget.as_ref().map(|_| payout.get(&model).copied().unwrap_or(0)),
                ..Default::default()
            }
        })
        .collect();
    let miners = per_miner
        .into_iter()
        .map(|((bond, model), (paid, claims))| PalwMinerRevenueRowV1 { bond, model, payout_coinbase_sompi: paid, claims })
        .collect();
    (rows, miners)
}
