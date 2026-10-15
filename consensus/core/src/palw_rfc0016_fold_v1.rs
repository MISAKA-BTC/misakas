//! **RFC-0016 in the fold** — a child module of `palw_state_v2` (so it reads the state's epoch counters, the readiness predicate and the
//! panel demand terms the way the fold does). Dormant: every entry point returns "no change" unless the V2 mirror of
//! `palw_model_block_rights_v1` / `palw_elastic_final_capacity_v1` is in force, and validation refuses both armed.
//!
//! * [`PalwFoldReadV1::rfc0016_rights_admit_v1`] — V+: a model-class Attempt is admitted only with a block right of its model (asked by
//!   `check_class_admits_claim`, so the producer's pre-check `palw_class_admits_claim_v1` answers it BEFORE the forward). Past φ the
//!   epoch's `A_m`-weighted reissue snapshot (aux 132) decides; where `A_m` is unavailable nothing is reissued (V2, user decision
//!   2026-10-15: never a first-come grant).
//! * [`PalwFoldReadV1::rfc0016_elastic_room_v1`] — the per-verifier water-filled room; capacity 0 admits nothing; a claim the
//!   capacity cannot take now is admitted into the class's bounded pending queue (aux 133/134) while it has room.
//! * [`on_attempt_accepted_rfc0016_v1`] — the accepted claim is queued when its class's demand exceeds capacity (no bind deadline
//!   while queued; its window starts when it leaves), and its verifier share takes the M\*-49 slope.
//! * [`tick_rfc0016_v1`] — per block: the reissue snapshot at φ, the queue's TTL voids and FIFO dequeues as capacity frees, and the
//!   heartbeat-dependent DAA counter (aux 135).
//! * [`palw_rfc0016_quote_v1`] / [`palw_rfc0016_model_report_v1`] — the pre-lottery quote and the report rows.
//!
//! Interfaces assumed from other lanes (not edited here): `P_m` through `PalwRightsWeightsSourceV1`, eligibility through
//! [`palw_rfc0016_eligibility_v1`] (the one swap point: RFC-0017's eligibility, merged).

use super::*;
use crate::palw_rfc0016_v1::capacity::{
    PalwAdmissionQuoteV1, PalwVerifierSupplyV1, palw_admission_quote_v1, palw_c7_derived_cap_v1, palw_capacity_fits_v1,
    palw_opv_capacity_admits_v1, palw_queue_bond_share_cap_v1, palw_verifier_share_permille_v1, palw_waterfill_capacity_v1,
};
use crate::palw_rfc0016_v1::ledger::*;
use crate::palw_rfc0016_v1::metrics::{PalwRfc0016ObservationV1, palw_rfc0016_observe_v1, palw_rfc0016_observed_v1};
use crate::palw_rfc0016_v1::report::{PalwMinerRevenueRowV1, PalwModelReportRowV1};
use crate::palw_rfc0016_v1::rights::{
    PalwModelEligibilitySourceV1, PalwRightsAllowanceV1, PalwRightsEpochV1, PalwRightsWeightsSourceV1,
    palw_rights_admit_with_snapshot_v1, palw_rights_consumed_v1, palw_rights_issue_v1, palw_rights_n_exp_v1,
    palw_rights_past_checkpoint_v1, palw_rights_quote_v1, palw_rights_reissue_snapshot_v1,
};
use crate::palw_rfc0016_v1::values::{PALW_RFC0016_VALUES_V1, PalwRfc0016ValuesV1};

/// Telemetry rows older than this many epochs are dropped (bounded state).
pub const PALW_RFC0016_TELEMETRY_EPOCHS_KEPT_V1: u64 = 8;

/// **RFC-0017's eligibility, as V+ reads it** (merged from `integ/rfc0017-model-eligibility`): a model is issued normal V+ rights
/// when any of its classes carries a reward-bearing block right outside probation (or is not governed by RFC-0017: the old rule);
/// a `Probation` class draws on RFC-0017's own probation budget instead (V+ lets its Attempts through, never issuing it rights from
/// `P_m` too); a `Registered` / `Lapsed` class is refused by RFC-0017's claim gate. The entrant clock is the DAA a class was
/// promoted to `Eligible`.
pub struct PalwRfc0017EligibilityV1<'a> {
    state: &'a PalwChainStateV2,
    params: &'a PalwStateParamsV2,
    ownership: bool,
}

impl PalwRfc0017EligibilityV1<'_> {
    fn classes_of(&self, model: &Hash64) -> Vec<Hash64> {
        self.state
            .model_lifecycles
            .keys()
            .filter(|class| palw_rfc0016_model_of_v1(self.state, self.ownership, class) == *model)
            .copied()
            .collect()
    }
}

impl PalwModelEligibilitySourceV1 for PalwRfc0017EligibilityV1<'_> {
    fn block_rights_eligible_v1(&self, model: &Hash64, daa: u64) -> bool {
        let classes = self.classes_of(model);
        classes.is_empty()
            || classes.iter().any(|class| {
                match super::palw_model_eligibility_fold_v1::eligibility_of_v1(self.state, self.params, class, daa) {
                    None => true,
                    Some((_, rights)) => rights.block_right && !rights.probation_bounded,
                }
            })
    }

    fn eligible_since_daa_v1(&self, model: &Hash64) -> Option<u64> {
        self.classes_of(model)
            .iter()
            .filter_map(|class| super::palw_model_eligibility_fold_v1::eligibility_class_view_v1(self.state, class)?.row)
            .filter(|row| row.stage == crate::palw_model_eligibility_v1::PalwEligibilityStageV1::Eligible)
            .map(|row| row.stage_since_daa)
            .max()
    }
}

/// **The one swap point for eligibility** — RFC-0017's (merged), replacing the earlier all-eligible stand-in.
pub fn palw_rfc0016_eligibility_v1<'a>(
    state: &'a PalwChainStateV2,
    params: &'a PalwStateParamsV2,
    ownership: bool,
) -> PalwRfc0017EligibilityV1<'a> {
    PalwRfc0017EligibilityV1 { state, params, ownership }
}

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
            if crate::palw_bond_budget_v1::PalwBondBudgetStateV1::is_allocation_aux_row_v1(&budget.models[model]) {
                continue;
            }
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

/// The aux rows of `table` whose key starts with `prefix`, in key order.
fn rfc0016_rows_v1<'a>(state: &'a PalwChainStateV2, table: u8, prefix: &[u8]) -> Vec<(&'a Vec<u8>, &'a Vec<u8>)> {
    let Some(route) = state.kernel_route.as_ref() else { return Vec::new() };
    route
        .aux
        .range((table, prefix.to_vec())..)
        .take_while(|((t, k), _)| *t == table && k.starts_with(prefix))
        .map(|((_, k), v)| (k, v))
        .collect()
}

impl PalwChainStateV2 {
    /// Is `claim_id` waiting in an RFC-0016 pending Final queue? (No bind deadline while it is; the panel bind is inert.)
    pub fn rfc0016_claim_queued_v1(&self, claim_id: &Hash64) -> bool {
        self.kernel_route
            .as_ref()
            .is_some_and(|route| route.aux.contains_key(&(PALW_RFC0016_QUEUE_INDEX_TABLE_V1, queue_index_key_v1(claim_id))))
    }

    /// A class's queue, FIFO: `(claim, bond, queued_daa)`.
    pub fn rfc0016_queue_v1(&self, class_id: &Hash64) -> Vec<(Hash64, PalwBondKeyV2, u64)> {
        rfc0016_rows_v1(self, PALW_RFC0016_QUEUE_TABLE_V1, &queue_class_prefix_v1(class_id))
            .into_iter()
            .filter_map(|(k, v)| {
                let claim = Hash64::try_from_slice(k.get(72..136)?).ok()?;
                let (bond, queued, _reserved): (PalwBondKeyV2, u64, u64) = borsh::from_slice(v).ok()?;
                Some((claim, bond, queued))
            })
            .collect()
    }

    /// The epoch's reissue snapshot, if it was taken (the marker row present).
    pub fn rfc0016_snapshot_v1(&self, epoch: u64) -> Option<BTreeMap<Hash64, PalwRightsAllowanceV1>> {
        let rows = rfc0016_rows_v1(self, PALW_RFC0016_RIGHTS_TABLE_V1, &rights_epoch_prefix_v1(epoch));
        let mut out = BTreeMap::new();
        let mut marked = false;
        for (k, v) in rows {
            let Some(model) = k.get(8..72).and_then(|b| Hash64::try_from_slice(b).ok()) else { continue };
            let Ok((allowance, reissued)) = borsh::from_slice::<(u64, u64)>(v) else { continue };
            if model == PALW_RFC0016_SNAPSHOT_MARKER_V1 {
                marked = true;
            } else {
                out.insert(model, PalwRightsAllowanceV1 { allowance, reissued });
            }
        }
        marked.then_some(out)
    }

    fn rfc0016_telemetry_u64s_v1<T: borsh::BorshDeserialize + Default>(&self, key: Vec<u8>) -> T {
        self.kernel_route.as_ref().and_then(|route| route.aux_row(PALW_RFC0016_TELEMETRY_TABLE_V1, &key)).unwrap_or_default()
    }
}

impl PalwFoldReadV1<'_> {
    /// **V+ admission** (RFC-0016 §8.1 step 2): past `palw_model_block_rights_v1`, a model-class Attempt finds one of its model's block
    /// rights — own, or (past φ) within the epoch's `A_m`-weighted reissue snapshot (none where no snapshot could be taken: V2, user
    /// decision 2026-10-15, no first-come fallback) — never past the epoch total; otherwise it is refused before acceptance (`ModelBlockRightsRefused`, skippable for the
    /// block's own attempt). `Ok` below the fence and for the floor class (the floor and the heartbeat keep their own rules).
    pub(super) fn rfc0016_rights_admit_v1(&self, class_id: &Hash64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.params.rfc0016_rights_active_at(now_daa) || *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(mirror) = self.params.rfc0016_v1() else { return Ok(()) };
        // RFC-0017: a Probation class's right is RFC-0017's probation grant (not V+'s), and a Registered / Lapsed class is refused
        // by RFC-0017's own claim gate — one gate, one refusal.
        if let Some((_, rights)) = super::palw_model_eligibility_fold_v1::eligibility_of_v1(self.state, self.params, class_id, now_daa)
            && (rights.probation_bounded || !rights.block_right)
        {
            return Ok(());
        }
        let ownership = self.extras.artifact_root_ownership_active;
        let eligibility = palw_rfc0016_eligibility_v1(self.state, self.params, ownership);
        let epoch = palw_rfc0016_rights_epoch_v1(self.state, self.params, &eligibility, now_daa, &mirror.values);
        let consumed = palw_rfc0016_rights_consumed_now_v1(self.state, self.params, ownership, epoch.epoch_index);
        let model = palw_rfc0016_model_of_v1(self.state, ownership, class_id);
        let snapshot = self.state.rfc0016_snapshot_v1(epoch.epoch_index);
        palw_rights_admit_with_snapshot_v1(&epoch, &consumed, &model, now_daa, &mirror.values, snapshot.as_ref()).map(|_| ()).map_err(
            |refusal| {
                palw_rfc0016_observe_v1(class_id, PalwRfc0016ObservationV1::RefusedForRights);
                PalwStateV2Error::ModelBlockRightsRefused { class: *class_id, code: refusal.code(), why: refusal.why().to_string() }
            },
        )
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

    /// **The elastic room** (RFC-0016 §10): `None` below `palw_elastic_final_capacity_v1` (the shipped room decides). Past it:
    /// capacity 0 admits nothing; demand within capacity admits; otherwise the claim is admitted into the class's bounded pending queue
    /// while it has room (it is queued at acceptance), or refused (queue full). A C7 (long-window) class is held to the concurrency
    /// derived from its capacity instead of its static `max_inflight_claims`.
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
        let refuse = || PalwStateV2Error::PanelRoomExhausted {
            class: *class_id,
            inflight_replay: demand,
            budget: capacity,
            horizon_spans: window,
        };
        if capacity == 0 {
            palw_rfc0016_observe_v1(class_id, PalwRfc0016ObservationV1::RefusedForCapacity);
            return Some(Err(refuse()));
        }
        if !palw_capacity_fits_v1(capacity, demand) {
            // The queue takes Attempts only (a free-prompt or evaluation claim has no bind window to defer), and only with the kernel
            // route's rows to hold it.
            let queued = self.state.rfc0016_queue_v1(class_id).len() as u64;
            if incoming == PalwGatedClaimV1::Attempt
                && self.state.kernel_route.is_some()
                && queued < mirror.values.queue_len_per_class as u64
            {
                return Some(Ok(()));
            }
            palw_rfc0016_observe_v1(class_id, PalwRfc0016ObservationV1::QueueFull);
            return Some(Err(refuse()));
        }
        if palw_panel_holds_to_final_v1(self.params, self.state, class_id) {
            let claim_replay = row.work.economic_ccu_per_claim.saturating_mul(fold.globals.seat_count as u128);
            let unscaled = capacity / crate::palw_work_target_v1::PALW_PANEL_DEMAND_SCALE_V1;
            let cap = palw_c7_derived_cap_v1(unscaled, window, claim_replay, &mirror.values);
            let inflight = self.model_registry_inflight(class_id);
            if (inflight as u64).saturating_add(whole) > cap as u64 {
                palw_rfc0016_observe_v1(class_id, PalwRfc0016ObservationV1::RefusedForCapacity);
                return Some(Err(PalwStateV2Error::ClassInflightCapped { class: *class_id, inflight, cap }));
            }
        }
        Some(Ok(()))
    }

    /// **The queue's per-bond share** (RFC-0016 §10, A5): past the capacity fence, an Attempt that would be queued is refused when its
    /// bond already holds its capital share of the class's queue (`⌈L · C_b / ΣC⌉` over the bonds in the queue, the asking one
    /// included). `Ok` when the claim would not be queued.
    pub(super) fn rfc0016_queue_bond_share_v1(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.rfc0016_capacity_active_at(now_daa) || *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(mirror) = self.params.rfc0016_v1() else { return Ok(()) };
        let Some(fold) = self.extras.model_registry.as_ref() else { return Ok(()) };
        let (capacity, demand, _) =
            self.rfc0016_capacity_read_v1(class_id, fold, now_daa, Some((*class_id, PalwGatedClaimV1::Attempt)));
        if palw_capacity_fits_v1(capacity, demand) {
            return Ok(());
        }
        let queue = self.state.rfc0016_queue_v1(class_id);
        let capital = |b: &PalwBondKeyV2| self.state.bonds.get(b).map(|r| r.collateral as u128).unwrap_or(0);
        let mut active: BTreeSet<PalwBondKeyV2> = queue.iter().map(|(_, b, _)| *b).collect();
        active.insert(*bond);
        let total: u128 = active.iter().map(capital).fold(0u128, u128::saturating_add);
        let cap = palw_queue_bond_share_cap_v1(capital(bond), total, &mirror.values);
        let held = queue.iter().filter(|(_, b, _)| b == bond).count() as u64;
        if held >= cap as u64 {
            palw_rfc0016_observe_v1(class_id, PalwRfc0016ObservationV1::QueueFull);
            return Err(PalwStateV2Error::FinalQueueRefused {
                class: *class_id,
                code: "RFC0016_QUEUE_BOND_SHARE",
                why: format!("the bond holds {held} of its {cap} queued claims of the class"),
            });
        }
        Ok(())
    }
}

fn rfc0016_unqueue_v1(builder: &mut TransitionBuilder<'_>, class_id: &Hash64, queued_daa: u64, claim_id: &Hash64) {
    builder.write_kernel_row(PALW_RFC0016_QUEUE_TABLE_V1, queue_key_v1(class_id, queued_daa, claim_id), None);
    builder.write_kernel_row(PALW_RFC0016_QUEUE_INDEX_TABLE_V1, queue_index_key_v1(claim_id), None);
}

fn rfc0016_write_v1<T: borsh::BorshSerialize>(builder: &mut TransitionBuilder<'_>, table: u8, key: Vec<u8>, value: &T) {
    builder.write_kernel_row(table, key, Some(borsh::to_vec(value).expect("an RFC-0016 row serializes")));
}

/// **A claim has just been accepted** (RFC-0016 §10; infallible: nothing written above it is stranded). Past the capacity fence, with
/// the kernel route's rows present: its verifier share takes the M\*-49 slope of its class's utilization, and if its class's demand
/// (this claim included) exceeds capacity it joins the class's pending queue — one reservation (its block right, already consumed),
/// no bind deadline while queued (`palw_provisional_bind_deadline_v1`), its window from the DAA it leaves.
pub(super) fn on_attempt_accepted_rfc0016_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, claim_id: Hash64) {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.rfc0016_v1() else { return };
    if !builder.params.rfc0016_capacity_active_at(now) || builder.state.kernel_route.is_none() {
        return;
    }
    let Some(claim) = builder.state.claims.get(&claim_id).cloned() else { return };
    if claim.class_id == builder.params.base_class_id() {
        return;
    }
    let read = {
        let read = builder.read();
        read.extras.model_registry.as_ref().map(|fold| read.rfc0016_capacity_read_v1(&claim.class_id, fold, now, None))
    };
    let Some((capacity, demand, _)) = read else { return };
    if capacity > 0 {
        let utilization = (demand.saturating_mul(1_000) / capacity).min(u32::MAX as u128) as u32;
        if let Some(mut economics) = builder.state.claim_economics.get(&claim_id).cloned() {
            economics.panel_share_permille = palw_verifier_share_permille_v1(utilization, &mirror.values) as u16;
            builder.write_claim_economics(claim_id, Some(economics));
        }
    }
    if demand > capacity {
        rfc0016_write_v1(
            builder,
            PALW_RFC0016_QUEUE_TABLE_V1,
            queue_key_v1(&claim.class_id, now, &claim_id),
            &(claim.bond, now, 1u64),
        );
        rfc0016_write_v1(builder, PALW_RFC0016_QUEUE_INDEX_TABLE_V1, queue_index_key_v1(&claim_id), &(claim.class_id, now));
        let _ = builder.rearm_claim_deadline_dl1_v1(claim_id, now);
        palw_rfc0016_observe_v1(&claim.class_id, PalwRfc0016ObservationV1::Queued);
    }
}

/// **The per-block RFC-0016 tick**, beside the budget's clock (both fold sites call it): the reissue snapshot at φ, the queue's TTL
/// voids and FIFO dequeues, and the heartbeat-dependent DAA counter. A no-op below both fences and without the kernel route's rows.
pub(super) fn tick_rfc0016_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.rfc0016_v1() else { return Ok(()) };
    let rights_on = builder.params.rfc0016_rights_active_at(now);
    let capacity_on = builder.params.rfc0016_capacity_active_at(now);
    if !(rights_on || capacity_on) || builder.state.kernel_route.is_none() {
        return Ok(());
    }
    let values = mirror.values;
    let ownership = builder.extras.artifact_root_ownership_active;
    let length = builder.params.epoch_length().max(1);
    let epoch_index = now / length;
    if rights_on {
        rfc0016_snapshot_tick_v1(builder, now, &values, ownership)?;
    }
    if capacity_on {
        rfc0016_queue_tick_v1(builder, ctx, &values, ownership, epoch_index)?;
    }
    // Heartbeat-dependent DAA: the DAA steps since the last tick while no REAL attempt is flowing (the floor Idle: the floor and the
    // heartbeat carry the clock). Consensus-derived from the rooted floor state.
    let key = telemetry_heartbeat_key_v1(epoch_index);
    let (dependent, last): (u64, u64) = builder.state.rfc0016_telemetry_u64s_v1(key.clone());
    let step = if last == 0 { 0 } else { now.saturating_sub(last) };
    let idle = matches!(builder.state.floor_state_v1().mode, crate::palw_real_share_v1::PalwFloorModeV1::Idle);
    rfc0016_write_v1(
        builder,
        PALW_RFC0016_TELEMETRY_TABLE_V1,
        key,
        &(if idle { dependent.saturating_add(step) } else { dependent }, now),
    );
    // Bounded: drop telemetry rows older than the kept epochs.
    if epoch_index > PALW_RFC0016_TELEMETRY_EPOCHS_KEPT_V1 {
        let floor = epoch_index - PALW_RFC0016_TELEMETRY_EPOCHS_KEPT_V1;
        let stale: Vec<Vec<u8>> = rfc0016_rows_v1(&builder.state, PALW_RFC0016_TELEMETRY_TABLE_V1, &[])
            .into_iter()
            .filter(|(k, _)| k.first() != Some(&PALW_RFC0016_TELEMETRY_CLAIM_MODEL_PREFIX_V1))
            .filter(|(k, _)| k.get(1..9).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).is_some_and(|e| e < floor))
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            builder.write_kernel_row(PALW_RFC0016_TELEMETRY_TABLE_V1, k, None);
        }
    }
    Ok(())
}

fn rfc0016_snapshot_tick_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    values: &PalwRfc0016ValuesV1,
    ownership: bool,
) -> Result<(), PalwStateV2Error> {
    let eligibility = palw_rfc0016_eligibility_v1(&builder.state, builder.params, ownership);
    let epoch = palw_rfc0016_rights_epoch_v1(&builder.state, builder.params, &eligibility, now, values);
    let elapsed = now.saturating_sub(epoch.start_daa);
    if !palw_rights_past_checkpoint_v1(elapsed, epoch.length, values) || builder.state.rfc0016_snapshot_v1(epoch.epoch_index).is_some()
    {
        return Ok(());
    }
    // Earlier epochs' snapshots leave (bounded state).
    let stale: Vec<Vec<u8>> = rfc0016_rows_v1(&builder.state, PALW_RFC0016_RIGHTS_TABLE_V1, &[])
        .into_iter()
        .filter(|(k, _)| k.get(0..8).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).is_some_and(|e| e < epoch.epoch_index))
        .map(|(k, _)| k.clone())
        .collect();
    for k in stale {
        builder.write_kernel_row(PALW_RFC0016_RIGHTS_TABLE_V1, k, None);
    }
    let consumed = palw_rfc0016_rights_consumed_now_v1(&builder.state, builder.params, ownership, epoch.epoch_index);
    let weights = builder.state.bond_budget.as_ref().and_then(|b| b.rights_weights_v1());
    // No weights: no snapshot — the first-come σ rule stands for the rest of the epoch (the fallback).
    let Some(snapshot) = palw_rights_reissue_snapshot_v1(&epoch, &consumed, weights.as_ref(), elapsed, values) else { return Ok(()) };
    for (model, row) in snapshot {
        rfc0016_write_v1(
            builder,
            PALW_RFC0016_RIGHTS_TABLE_V1,
            rights_key_v1(epoch.epoch_index, &model),
            &(row.allowance, row.reissued),
        );
    }
    rfc0016_write_v1(
        builder,
        PALW_RFC0016_RIGHTS_TABLE_V1,
        rights_key_v1(epoch.epoch_index, &PALW_RFC0016_SNAPSHOT_MARKER_V1),
        &(0u64, 0u64),
    );
    Ok(())
}

fn rfc0016_queue_tick_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    values: &PalwRfc0016ValuesV1,
    ownership: bool,
    epoch_index: u64,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let classes: BTreeSet<Hash64> = rfc0016_rows_v1(&builder.state, PALW_RFC0016_QUEUE_INDEX_TABLE_V1, &[])
        .into_iter()
        .filter_map(|(_, v)| borsh::from_slice::<(Hash64, u64)>(v).ok().map(|(class, _)| class))
        .collect();
    for class_id in classes {
        // TTL voids (no slash: `BindTimeout` is never charged) and rows of claims that left by another route.
        let mut live: Vec<(Hash64, u64, PalwClaimStateV2)> = Vec::new();
        for (claim_id, _bond, queued) in builder.state.rfc0016_queue_v1(&class_id) {
            match builder.state.claims.get(&claim_id).cloned() {
                Some(claim) if matches!(claim.phase, PalwClaimPhaseV2::Provisional) => {
                    if queued.saturating_add(values.queue_ttl_daa) < now {
                        rfc0016_unqueue_v1(builder, &class_id, queued, &claim_id);
                        builder.disarm_deadline(claim_id);
                        builder.void_claim(claim_id, &claim, now, PalwVoidReasonV2::BindTimeout)?;
                    } else {
                        live.push((claim_id, queued, claim));
                    }
                }
                _ => rfc0016_unqueue_v1(builder, &class_id, queued, &claim_id),
            }
        }
        if live.is_empty() {
            continue;
        }
        // How many fit now: the class's unqueued demand plus `n` more within its capacity.
        let fit = {
            let read = builder.read();
            match (read.extras.model_registry.as_ref(), read.state.model_lifecycles.get(&class_id)) {
                (Some(fold), Some(row)) => {
                    let (capacity, _, _) = read.rfc0016_capacity_read_v1(&class_id, fold, now, None);
                    let owed = read.with_inflight_index(|index| palw_panel_owed_v1(read.state, read.params, index, None, None));
                    let unqueued = owed.get(&class_id).copied().unwrap_or(0).saturating_sub(live.len() as u128);
                    let claim_replay = row.work.economic_ccu_per_claim.saturating_mul(fold.globals.seat_count as u128);
                    let window = row.profile.verification_window_spans as u64;
                    let mut n = 0usize;
                    while n < live.len()
                        && crate::palw_work_target_v1::palw_panel_demand_term_v1(unqueued + n as u128 + 1, claim_replay, window)
                            <= capacity
                    {
                        n += 1;
                    }
                    n
                }
                _ => 0,
            }
        };
        let model = palw_rfc0016_model_of_v1(&builder.state, ownership, &class_id);
        for (claim_id, queued, claim) in live.into_iter().take(fit) {
            rfc0016_unqueue_v1(builder, &class_id, queued, &claim_id);
            // The window starts when the claim leaves the queue: its bind base (and anchor slot) is now.
            let mut released = claim;
            released.rebound_daa = Some(now);
            builder.write_claim(claim_id, Some(released));
            builder.rearm_claim_deadline_dl1_v1(claim_id, now)?;
            // Telemetry: the queue wait, consensus-derived.
            let key = telemetry_model_key_v1(PALW_RFC0016_TELEMETRY_QUEUE_WAIT_PREFIX_V1, epoch_index, &model);
            let (count, sum, max): (u64, u64, u64) = builder.state.rfc0016_telemetry_u64s_v1(key.clone());
            let wait = now.saturating_sub(queued);
            rfc0016_write_v1(builder, PALW_RFC0016_TELEMETRY_TABLE_V1, key, &(count + 1, sum.saturating_add(wait), max.max(wait)));
        }
    }
    Ok(())
}

/// **The OPV route's capacity gate and claim → model note** (RFC-0016 §10, at the kernel route's `ClaimCommitted`, before any write):
/// past the capacity fence, an OPV claim of a model whose open OPV claims would exceed the checks its ready verifiers can perform is
/// refused before acceptance. Writes the kernel claim's model for the escrow-payout telemetry when admitted.
pub(super) fn rfc0016_opv_claim_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    kernel_claim: &[u8; 64],
    native_class: Option<Hash64>,
    open_opv_claims_of_class: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = builder.params.rfc0016_v1() else { return Ok(()) };
    if !builder.params.rfc0016_capacity_active_at(now) {
        return Ok(());
    }
    let Some(class_id) = native_class else { return Ok(()) };
    let ready = {
        let read = builder.read();
        match read.extras.model_registry.as_ref() {
            Some(fold) => read.rfc0016_capacity_read_v1(&class_id, fold, now, None).2,
            None => 0,
        }
    };
    let checks = (ready as u64).saturating_mul(mirror.values.opv_checks_per_verifier_epoch as u64);
    if !palw_opv_capacity_admits_v1(open_opv_claims_of_class, checks, &mirror.values) {
        palw_rfc0016_observe_v1(&class_id, PalwRfc0016ObservationV1::RefusedForCapacity);
        return Err(PalwStateV2Error::FinalQueueRefused {
            class: class_id,
            code: "RFC0016_OPV_CHECKS_SHORT",
            why: format!("{open_opv_claims_of_class} open OPV claims need more checks than {ready} ready verifiers give ({checks})"),
        });
    }
    let model = palw_rfc0016_model_of_v1(&builder.state, builder.extras.artifact_root_ownership_active, &class_id);
    rfc0016_write_v1(builder, PALW_RFC0016_TELEMETRY_TABLE_V1, telemetry_claim_model_key_v1(kernel_claim), &model);
    Ok(())
}

/// **The escrow-payout telemetry** (consensus-derived): a kernel claim's Final reward, paid from its user's escrow, is added to its
/// model's epoch row (never summed with coinbase). A no-op where the claim's model was not noted (below the fence).
pub(super) fn rfc0016_note_escrow_payout_v1(builder: &mut TransitionBuilder<'_>, now: u64, kernel_claim: &[u8; 64], amount: u64) {
    if builder.params.rfc0016_v1().is_none() || builder.state.kernel_route.is_none() {
        return;
    }
    let note = telemetry_claim_model_key_v1(kernel_claim);
    let Some(model) =
        builder.state.kernel_route.as_ref().and_then(|route| route.aux_row::<Hash64>(PALW_RFC0016_TELEMETRY_TABLE_V1, &note))
    else {
        return;
    };
    let epoch = now / builder.params.epoch_length().max(1);
    let key = telemetry_model_key_v1(PALW_RFC0016_TELEMETRY_ESCROW_PREFIX_V1, epoch, &model);
    let paid: u64 = builder.state.rfc0016_telemetry_u64s_v1(key.clone());
    rfc0016_write_v1(builder, PALW_RFC0016_TELEMETRY_TABLE_V1, key, &paid.saturating_add(amount));
    builder.write_kernel_row(PALW_RFC0016_TELEMETRY_TABLE_V1, note, None);
}

/// **The combined pre-lottery admission quote** for one more Attempt of `class_id` at `now_daa` (`None` where neither RFC-0016 fence is
/// armed), with the class's real queue length.
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
        let eligibility = palw_rfc0016_eligibility_v1(state, params, ownership);
        let epoch = palw_rfc0016_rights_epoch_v1(state, params, &eligibility, now_daa, &mirror.values);
        let consumed = palw_rfc0016_rights_consumed_now_v1(state, params, ownership, epoch.epoch_index);
        let model = palw_rfc0016_model_of_v1(state, ownership, class_id);
        let mut quote = palw_rights_quote_v1(&epoch, &consumed, &model, now_daa, &mirror.values);
        let snapshot = state.rfc0016_snapshot_v1(epoch.epoch_index);
        quote.verdict = palw_rights_admit_with_snapshot_v1(&epoch, &consumed, &model, now_daa, &mirror.values, snapshot.as_ref());
        if let Some(row) = snapshot.as_ref().and_then(|s| s.get(&model)) {
            quote.reissue_room = row.allowance.saturating_sub(quote.used.max(quote.issued));
        }
        quote
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
    let queue_len = state.rfc0016_queue_v1(class_id).len().min(u32::MAX as usize) as u32;
    Some(palw_admission_quote_v1(rights, capacity, queue_len, &mirror.values))
}

/// **The per-model and per-miner report** at `now_daa` (RFC-0016 §11): consensus-derived where the state holds it (targets, physical
/// blocks, rights, Finals, coinbase payout, per-miner revenue, escrow payout, queue wait, REAL gap, heartbeat-dependent DAA), node-local
/// observations where it cannot (refusals, labelled as such), `None` only below the fences. It changes nothing.
pub fn palw_rfc0016_model_report_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    now_daa: u64,
) -> (Vec<PalwModelReportRowV1>, Vec<PalwMinerRevenueRowV1>) {
    let values = params.rfc0016_v1().map(|m| m.values).unwrap_or(PALW_RFC0016_VALUES_V1);
    let ownership = extras.artifact_root_ownership_active;
    let eligibility = palw_rfc0016_eligibility_v1(state, params, ownership);
    let epoch = palw_rfc0016_rights_epoch_v1(state, params, &eligibility, now_daa, &values);
    let consumed = palw_rfc0016_rights_consumed_now_v1(state, params, ownership, epoch.epoch_index);
    let weights = state.bond_budget.as_ref().and_then(|b| b.rights_weights_v1());
    let mut models: BTreeSet<Hash64> = epoch.issued.keys().chain(consumed.keys()).copied().collect();
    if let Some((w, _)) = weights.as_ref() {
        models.extend(w.keys().copied());
    }
    // Finals per model (all epochs) and the classes of each model (for the node-local observations).
    let mut finals: BTreeMap<Hash64, u64> = BTreeMap::new();
    for claim in state.claims.values() {
        if matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
            let slot = finals.entry(palw_rfc0016_model_of_v1(state, ownership, &claim.class_id)).or_insert(0);
            *slot = slot.saturating_add(1);
        }
    }
    let mut classes_of: BTreeMap<Hash64, Vec<Hash64>> = BTreeMap::new();
    for class_id in state.model_lifecycles.keys() {
        classes_of.entry(palw_rfc0016_model_of_v1(state, ownership, class_id)).or_default().push(*class_id);
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
    let armed = params.rfc0016_v1().is_some();
    let real_gap = match state.floor_state_v1().mode {
        crate::palw_real_share_v1::PalwFloorModeV1::Normal { last_blue } => Some(now_daa.saturating_sub(last_blue)),
        _ => None,
    };
    let (heartbeat_dependent, _): (u64, u64) = state.rfc0016_telemetry_u64s_v1(telemetry_heartbeat_key_v1(epoch.epoch_index));
    let rows = models
        .into_iter()
        .map(|model| {
            let issued = epoch.issued_of(&model);
            let used = consumed.get(&model).copied().unwrap_or(0);
            let target_p_ppm = weights
                .as_ref()
                .and_then(|(w, sum)| w.get(&model).map(|a| crate::palw_bond_budget_v1::palw_model_available_v1(1_000_000, a, sum, 0)));
            let escrow: u64 = state.rfc0016_telemetry_u64s_v1(telemetry_model_key_v1(
                PALW_RFC0016_TELEMETRY_ESCROW_PREFIX_V1,
                epoch.epoch_index,
                &model,
            ));
            let (dequeued, wait_sum, wait_max): (u64, u64, u64) = state.rfc0016_telemetry_u64s_v1(telemetry_model_key_v1(
                PALW_RFC0016_TELEMETRY_QUEUE_WAIT_PREFIX_V1,
                epoch.epoch_index,
                &model,
            ));
            let classes = classes_of.get(&model).cloned().unwrap_or_default();
            let observed = |kind| classes.iter().map(|c| palw_rfc0016_observed_v1(c, kind)).sum::<u64>();
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
                payout_escrow_sompi: armed.then_some(escrow as u128),
                refused_for_capacity: armed.then(|| observed(PalwRfc0016ObservationV1::RefusedForCapacity)),
                refused_for_rights: armed.then(|| observed(PalwRfc0016ObservationV1::RefusedForRights)),
                queue_wait_mean_daa: (armed && dequeued > 0).then(|| wait_sum / dequeued),
                queue_wait_max_daa: (armed && dequeued > 0).then_some(wait_max),
                real_gap_daa: real_gap,
                heartbeat_dependent_daa: armed.then_some(heartbeat_dependent),
                capacity_per_span: None,
                capacity_shortfall_per_span: None,
                queue_len: classes.iter().map(|c| state.rfc0016_queue_v1(c).len() as u64).sum(),
            }
        })
        .collect();
    let miners = per_miner
        .into_iter()
        .map(|((bond, model), (paid, claims))| PalwMinerRevenueRowV1 { bond, model, payout_coinbase_sompi: paid, claims })
        .collect();
    (rows, miners)
}
