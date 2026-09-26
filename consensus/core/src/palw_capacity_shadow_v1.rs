//! **ADR-0160 §6.3 / §7.5 — shadow accounting: what the capacity formulas would say about the live
//! chain, next to what the chain says today.** Node-only; no consensus rule reads it.
//!
//! [`palw_capacity_shadow_v1`] takes one committed PALW state and a list of ramp steps
//! ([`PalwCapacityStepV1`]) and reports, as if every live claim had been accepted past ADR-0160's
//! fences at that step:
//!
//! * **per claim** — the raw and the staged weight (§4.2), today's weight reservation `w` and the
//!   capped one `min(w, R_budget − held)` (§4.1), today's commitment and the new one per step
//!   (`m_c + reserved`, E-3's release, E-4's void hold);
//! * **per bond** — collateral, `W_cap`, `X_b`, `min(X_b, W_cap)`, the A-1 commitment today and per
//!   step (its own claims, its seat duties and locks repriced by AS-1/AS-2, the rest unchanged), the
//!   instantaneous floor-claim capacity `N_instant` today and per step (E-T3's number), and whether
//!   AG-3 would have frozen it;
//! * **network** — `bounded_immature` today vs `Σ_b min(X_b, W_cap)`, seat duty and lock totals and
//!   the §5.4 seat-capacity estimate per step, the licence queue and the carriage it needs, and
//!   attribution counters per class (convictions by kind, DA sessions by non-panel filers, the
//!   conviction-latency histogram, and — for bonds a caller names as adversarial, the O-3 runs of
//!   ADR-0160 §9 Stage 0 — the measured attribution rate `q` with the A8 alarm).
//!
//! **Where each "new" value comes from.** The formulas are [`crate::palw_capacity_formulas_v1`]'s;
//! the reading of the state is this module's and it is stated where it is an approximation:
//!
//! * The as-if rule applies to every attempt claim; free-prompt claims keep today's accounting
//!   (E-6: `E = 0`, `rights_reserved` per claim), and so does their reservation.
//! * The weight budget is spent in acceptance order (`accepted_daa`, then claim id) over the bond's
//!   claims that still hold a reservation under the new rule: non-terminal, or voided within `h_obl
//!   = window_receipt` (E-4, any reason, inclusive like the abandon hold).
//! * A seat duty row is repriced from its stored `seat_exposure` `d` as `min(⌈d/ρ⌉, commitment′ /
//!   seats)`. That is AS-1 exactly when `d` was not capped by `commitment / seats` at bind (floor and
//!   8k: the λ-term or `lock_2` bound it); a capped row (2M) reports a lower bound, and the count of
//!   such rows is reported.
//! * AS-2's lock credit uses `L_seat =` [`PALW_CAPACITY_SEAT_L_REFERENCE_SOMPI_V1`] (130,000 MSK, the
//!   seat floor: §10 D-5's recommended whole-seat-bond forfeiture on a located false Valid).
//! * AG-3's freeze reads the chain's conviction records: `DaDefault` and `CourtConviction` are the
//!   intent class (final); `ExecutorRefuted` and `PanelFalseValidV2` may be either (their
//!   contradiction kind is not in the record), so they freeze for `window_court` and are reported
//!   as undetermined; the rest are tier-capped. Lane liab's `palw_offence_is_intent_class_v1`
//!   replaces this reading once it merges.
//!
//! **Cost** (S-I3): one pass over the claims, the bonds, their locks, the duty rows, the DA sessions
//! and the conviction records — `O(claims + bonds + locks + rows)` per call, with a sort of each
//! bond's live claims. kaspad calls it every [`PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1`] DAA.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_hashes::Hash64;

use crate::palw_capacity_formulas_v1::{
    PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1, PALW_CAPACITY_REFERENCE_STEPS_V1, PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1,
    PALW_CAPACITY_SOMPI_PER_MSK_V1, PALW_CAPACITY_W_FCW_SOMPI_V1, PalwCapacitySeatCapitalInputsV1, PalwCapacityStageV1,
    PalwCapacityStepV1, palw_capacity_bond_weight_term_v1, palw_capacity_carriers_per_block_v1, palw_capacity_claims_per_daa_milli_v1,
    palw_capacity_consensus_reservation_v1, palw_capacity_conviction_l_v1, palw_capacity_m_c_v1, palw_capacity_n_instant_v1,
    palw_capacity_q_needed_permille_v1, palw_capacity_seat_capital_per_claim_v1, palw_capacity_seat_credit_applies_v1,
    palw_capacity_seat_duty_v1, palw_capacity_seat_lock_v1, palw_capacity_stage_of_claim_v1, palw_capacity_staged_weight_v1,
    palw_capacity_weight_budget_sompi_v1, palw_capacity_weight_cap_v1, palw_capacity_weight_full_v1,
};
use crate::palw_offence_v1::PalwOffenceKindV1;
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwRcoreGateV1,
    PalwStateParamsV2, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_bond_committed_raw_v1, palw_claim_commitment_v1,
    palw_claim_g_v1, palw_rcore_class_is_c7_v1, palw_rcore_gate_room_of_v1, palw_rcore_lock_v1, palw_second_clock_depth_v1,
};

/// kaspad recomputes the shadow every this many DAA of the tip (ADR-0160 §7.5).
pub const PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1: u64 = 10;

/// The reference bond of the log line's `N13k[ρ]`: the 13,000 MSK producer floor.
pub const PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1: u64 = 13_000 * 100_000_000;

/// **`L_seat` for AS-2's credit test** (§4.7, §10 D-5): the 130,000 MSK seat floor, what a located
/// `PanelFalseValidV2` forfeits at least under D-5's recommendation (q_seat ≈ 0.024).
pub const PALW_CAPACITY_SEAT_L_REFERENCE_SOMPI_V1: u128 = 130_000 * PALW_CAPACITY_SOMPI_PER_MSK_V1;

/// The seats of a floor panel when the state holds none to count (testnet-12's panel).
pub const PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1: u32 = 5;

/// The conviction-latency histogram's bucket upper bounds (DAA from the claim's acceptance to the
/// conviction, exclusive); a last bucket takes everything at or past 3,000 (`window_court`).
pub const PALW_CAPACITY_LATENCY_BUCKETS_V1: [u64; 7] = [10, 50, 100, 300, 600, 1_200, 3_000];

/// Licences counted as "recent" (the carriage's observed rate) within this many DAA of `now`.
pub const PALW_CAPACITY_SHADOW_RECENT_DAA_V1: u64 = PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1;

/// What one shadow computation reads besides the state and its params.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacityShadowOptionsV1 {
    /// The ramp steps to price. Empty: [`PALW_CAPACITY_REFERENCE_STEPS_V1`].
    pub steps: Vec<PalwCapacityStepV1>,
    /// The second clock's RAW depth at `now_daa` (the processor's `palw_settled_anchor_depth_at`);
    /// `None` is the DAA-only lock rule. Only lock liveness reads it.
    pub raw_depth: Option<u64>,
    /// Bonds whose claims are known to be adversarial (an O-3 run): their claims measure `q`.
    pub adversary_bonds: Vec<PalwBondKeyV2>,
    /// The block transient-mass limit the carriage estimate divides (`Params::max_block_mass`);
    /// `0` reads 500,000, testnet-12's.
    pub block_mass_limit: u64,
}

/// **One claim, today and under the new rules.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityClaimShadowV1 {
    pub claim_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub class_id: Hash64,
    /// The phase's name (`provisional`, `panel-bound`, `licensed`, `final`, `voided`, `disputed`).
    pub phase: &'static str,
    pub stage: PalwCapacityStageV1,
    pub accepted_daa: u64,
    pub free_prompt: bool,
    /// A C7 class (no conviction route: `m_c = E`, the Final weight ceiling).
    pub c7: bool,
    /// The stored `immature_contribution` — today's weight while not Final.
    pub raw_w: u128,
    pub w_full: u128,
    pub staged_w: u128,
    /// Today's weight reservation (`claim.reserved`) and the capped one.
    pub reserved_today: u128,
    pub reserved_new: u128,
    /// Today's escrow term on the bond (`claim_escrow_reservation_v1`).
    pub escrow_term_today: u128,
    /// SR-1 has released the escrow term (E-3 releases `m_c` on the same conditions).
    pub escrow_released: bool,
    /// Today's commitment (`palw_claim_commitment_v1`).
    pub commitment_today: u128,
    /// `L = 3G` for this claim.
    pub l_sompi: u128,
    /// `m_c` per step (0 for a free-prompt claim).
    pub m_new: Vec<u128>,
    /// The commitment per step.
    pub commitment_new: Vec<u128>,
}

/// **One bond, today and under the new rules.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityBondShadowV1 {
    pub bond: PalwBondKeyV2,
    /// Posted collateral (`record.collateral`) and its 500‰ ceiling.
    pub collateral: u64,
    pub ceiling: u128,
    /// A seat: registered, active, capable of at least one class.
    pub seat: bool,
    /// Claims with a commitment under either rule, and of them the unlicensed ones.
    pub live_claims: u64,
    pub unlicensed_claims: u64,
    /// Today's immature weight of its non-terminal claims (`Σ raw_w`).
    pub raw_immature_today: u128,
    pub w_cap: u128,
    /// `X_b = Σ` staged weight of its provisional claims, and `min(X_b, W_cap)`.
    pub x_b: u128,
    pub capped: u128,
    pub r_budget: u128,
    pub reserved_new_total: u128,
    /// A-1 today (`palw_bond_committed_raw_v1`), and of it the bond's own claims' commitments.
    pub committed_today: u128,
    pub own_claims_today: u128,
    pub committed_new: Vec<u128>,
    /// Floor claims its collateral holds at once with its other commitments as they stand
    /// (`N_instant`), and how many more it can open now.
    pub n_instant_today: u64,
    pub n_instant_new: Vec<u64>,
    pub n_more_today: u64,
    pub n_more_new: Vec<u64>,
    /// AG-3 would hold it frozen now; `freeze_final` if an intent-class conviction did it.
    pub frozen_would_be: bool,
    pub freeze_final: bool,
    pub freeze_undetermined: bool,
    pub first_conviction_daa: Option<u64>,
    pub convictions: u64,
}

/// **The network under one step.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityStepShadowV1 {
    pub step: PalwCapacityStepV1,
    /// `m_c` of the reference floor claim (attributable) and the `q` its ramp term needs at `L = 3G`.
    pub m_floor: u128,
    pub q_needed_permille: u16,
    /// `q_credit ≥ q_needed`: the ramp term `⌈E/ρ⌉` binds, not `m*`.
    pub ramp_binds: bool,
    /// AS-2's seat credit applies (`q_credit ≥ q_seat`), so locks shrink by ρ.
    pub seat_credit: bool,
    pub claims_commitment_total: u128,
    pub committed_total: u128,
    pub seat_duty_total: u128,
    pub seat_lock_total: u128,
    /// §5.4: floor claims per DAA the seats' capital sustains, in thousandths.
    pub seat_capacity_milli_per_daa: u64,
    /// `N_instant` of a fresh 13,000 MSK bond on floor claims.
    pub n_instant_13k: u64,
    /// A8: some class's measured `q` (adversary claims) is below `2 × q_needed`.
    pub q_alarm: bool,
}

/// **Attribution counters of one class** (`class_id` zero: records whose claim the state no longer
/// holds, or that name none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacityClassAttributionV1 {
    pub class_id: Hash64,
    pub claims_live: u64,
    pub claims_final: u64,
    pub claims_voided: u64,
    /// Voided claims by reason (`Debug` names), attributed or not.
    pub voids_by_reason: Vec<(String, u64)>,
    /// Voids whose reason is a conviction (`CourtFraud`, `ProducerWithholding`, `CourtDefault`,
    /// `CourtHeldVerdict`).
    pub voids_attributed: u64,
    /// Conviction records by `PalwOffenceKindV1` discriminant.
    pub convictions_by_kind: Vec<(u8, u64)>,
    /// Open DA sessions by filers that are not a seat of the claim's panel, and by seats.
    pub da_open_non_seat: u64,
    pub da_open_seat: u64,
    /// Sessions ever opened by non-seats on the claims the state still holds.
    pub da_opened_non_seat_total: u64,
    /// DAA from acceptance to conviction, bucketed by [`PALW_CAPACITY_LATENCY_BUCKETS_V1`].
    pub conviction_latency_histogram: [u64; 8],
    /// Adversary claims of this class the state holds, and of them the attributed ones.
    pub adversary_claims: u64,
    pub adversary_attributed: u64,
    /// `adversary_attributed / adversary_claims` in permille (`None` without adversary claims).
    pub q_measured_permille: Option<u16>,
}

/// **Everything one shadow computation reports.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityShadowV1 {
    pub now_daa: u64,
    /// The committed tip's DAA (the state's last point).
    pub tip_daa: u64,
    pub steps: Vec<PalwCapacityStepShadowV1>,
    pub claims: Vec<PalwCapacityClaimShadowV1>,
    pub bonds: Vec<PalwCapacityBondShadowV1>,
    // Weight (J-1).
    pub bounded_immature_today: u128,
    pub bounded_immature_new: u128,
    pub safe_weight: u128,
    pub w_cap_total: u128,
    // The reference floor claim the network figures price.
    pub reference_escrow: u128,
    pub reference_w_floor: u128,
    pub reference_l: u128,
    pub reference_seats: u32,
    pub reference_duty: u128,
    pub reference_lock: u128,
    // Money.
    pub claims_commitment_today: u128,
    pub committed_today_total: u128,
    // Seats.
    pub seats: u64,
    pub seat_usable_capital: u128,
    pub seat_duty_total_today: u128,
    pub seat_lock_total_today: u128,
    pub seat_capacity_today_milli_per_daa: u64,
    pub duty_rows: u64,
    /// Duty rows capped by `commitment / seats` at bind: their repriced duty is a lower bound.
    pub duty_rows_capped: u64,
    // Carriage.
    pub licence_queue: u64,
    pub licence_queue_oldest_bound_daa: Option<u64>,
    pub licensed_recent: u64,
    pub carriers_per_block: u64,
    pub carriage_blocks_to_drain: u64,
    // Attribution.
    pub attribution: Vec<PalwCapacityClassAttributionV1>,
    pub convictions_total: u64,
}

impl PalwCapacityShadowV1 {
    /// The step row for `rho`, if priced.
    pub fn step_for_rho(&self, rho: u32) -> Option<&PalwCapacityStepShadowV1> {
        self.steps.iter().find(|s| s.step.rho == rho)
    }

    /// **The one compact log line** (ADR-0160 §7.5): `capacity-shadow: daa=… immature today/new=…/…
    /// bonds=… claims=… N13k[ρ]=… seatcap[ρ]=… queue=… convictions=…`. Weights in FCW, seat
    /// capacity in floor claims per DAA.
    pub fn summary(&self) -> String {
        let fcw = |w: u128| w / crate::palw_capacity_formulas_v1::PALW_CAPACITY_FCW_V1;
        let n13k: Vec<String> = self.steps.iter().map(|s| format!("{}:{}", s.step.rho, s.n_instant_13k)).collect();
        let seatcap: Vec<String> = self
            .steps
            .iter()
            .map(|s| {
                format!("{}:{}.{:02}", s.step.rho, s.seat_capacity_milli_per_daa / 1_000, (s.seat_capacity_milli_per_daa % 1_000) / 10)
            })
            .collect();
        let alarm = if self.steps.iter().any(|s| s.q_alarm) { " q-ALARM" } else { "" };
        format!(
            "capacity-shadow: daa={} immature today/new={}/{} FCW bonds={} claims={} N13k[ρ]={} seatcap[ρ]={} (today {}.{:02}/DAA) queue={} convictions={}{}",
            self.now_daa,
            fcw(self.bounded_immature_today),
            fcw(self.bounded_immature_new),
            self.bonds.len(),
            self.claims.len(),
            n13k.join(","),
            seatcap.join(","),
            self.seat_capacity_today_milli_per_daa / 1_000,
            (self.seat_capacity_today_milli_per_daa % 1_000) / 10,
            self.licence_queue,
            self.convictions_total,
            alarm
        )
    }
}

/// **The shadow at `now_daa` over `steps`** (ADR-0160 §7.5's signature): the DAA-only lock rule, no
/// adversary set, testnet-12's block mass. [`palw_capacity_shadow_with_v1`] takes the rest.
pub fn palw_capacity_shadow_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now_daa: u64,
    steps: &[PalwCapacityStepV1],
) -> PalwCapacityShadowV1 {
    palw_capacity_shadow_with_v1(state, params, now_daa, &PalwCapacityShadowOptionsV1 { steps: steps.to_vec(), ..Default::default() })
}

/// The phase's short name.
fn phase_name(phase: &PalwClaimPhaseV2) -> &'static str {
    match phase {
        PalwClaimPhaseV2::Provisional => "provisional",
        PalwClaimPhaseV2::PanelBound { .. } => "panel-bound",
        PalwClaimPhaseV2::ReceiptLicensed { .. } => "licensed",
        PalwClaimPhaseV2::Final { .. } => "final",
        PalwClaimPhaseV2::Voided { .. } => "voided",
        PalwClaimPhaseV2::DefaultDisputed { .. } => "disputed",
    }
}

/// A void whose reason is a conviction of the producer.
fn void_is_attributed(reason: PalwVoidReasonV2) -> bool {
    matches!(
        reason,
        PalwVoidReasonV2::CourtFraud
            | PalwVoidReasonV2::ProducerWithholding
            | PalwVoidReasonV2::CourtDefault
            | PalwVoidReasonV2::CourtHeldVerdict
    )
}

/// **The freeze class AG-3 would give a conviction of `kind`** (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCapacityFreezeClassV1 {
    /// Never lifted (AG-2's forfeiture).
    Intent,
    /// Lifted at `since + window_court` without a further conviction.
    Tier,
    /// Either, by a contradiction kind the record does not carry: held for `window_court`.
    Undetermined,
}

/// The shadow's reading of AG-2's intent class from a conviction record.
pub fn palw_capacity_freeze_class_v1(kind: PalwOffenceKindV1) -> PalwCapacityFreezeClassV1 {
    match kind {
        PalwOffenceKindV1::DaDefault | PalwOffenceKindV1::CourtConviction => PalwCapacityFreezeClassV1::Intent,
        PalwOffenceKindV1::ExecutorRefuted | PalwOffenceKindV1::PanelFalseValidV2 => PalwCapacityFreezeClassV1::Undetermined,
        _ => PalwCapacityFreezeClassV1::Tier,
    }
}

/// Does the attempt claim hold a reservation under the new rule at `now_daa` (non-terminal, or
/// voided within `h_obl`, E-4)?
fn holds_new_rule(claim: &PalwClaimStateV2, h_obl: u64, now_daa: u64) -> bool {
    match &claim.phase {
        PalwClaimPhaseV2::Final { .. } => false,
        PalwClaimPhaseV2::Voided { voided_daa, .. } => voided_daa.checked_add(h_obl).is_none_or(|release_at| now_daa <= release_at),
        _ => true,
    }
}

fn latency_bucket(latency: u64) -> usize {
    PALW_CAPACITY_LATENCY_BUCKETS_V1.iter().position(|upper| latency < *upper).unwrap_or(PALW_CAPACITY_LATENCY_BUCKETS_V1.len())
}

#[derive(Default)]
struct BondFreeze {
    first: Option<u64>,
    last: u64,
    count: u64,
    intent: bool,
    undetermined: bool,
}

#[derive(Default)]
struct BondAcc {
    own_today: u128,
    own_new: Vec<u128>,
    raw_immature: u128,
    x_b: u128,
    reserved_new_total: u128,
    live: u64,
    unlicensed: u64,
    duties_today: u128,
    duties_new: Vec<u128>,
}

/// **The shadow with every input** (see [`PalwCapacityShadowOptionsV1`]).
pub fn palw_capacity_shadow_with_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now_daa: u64,
    options: &PalwCapacityShadowOptionsV1,
) -> PalwCapacityShadowV1 {
    let steps: Vec<PalwCapacityStepV1> =
        if options.steps.is_empty() { PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec() } else { options.steps.clone() };
    let n_steps = steps.len();
    let h_obl = params.window_receipt();
    let window_court = params.window_court();
    let ratio = params.fp_max_exposure_ratio_permille();
    let adversary: BTreeSet<PalwBondKeyV2> = options.adversary_bonds.iter().copied().collect();

    // ---- the reference floor claim ----------------------------------------------------------
    // E is the same for every class (720‰ of the subsidy): the newest attempt claim's, or the
    // carve of the tip's subsidy when the state holds none.
    let newest_attempt = state
        .claims_iter()
        .filter(|(_, c)| matches!(c.source, PalwClaimSourceV2::Attempt) && c.escrowed_reward > 0)
        .max_by_key(|(id, c)| (c.accepted_daa, **id));
    let reference_escrow: u128 = match newest_attempt {
        Some((_, claim)) => u128::from(claim.escrowed_reward),
        None => {
            u128::from(crate::palw_state_v2::palw_claim_escrow_v1(params, state.last_point().map(|p| p.subsidy).unwrap_or(0), None))
        }
    };
    let reference_w_floor = PALW_CAPACITY_W_FCW_SOMPI_V1;
    let reference_l = palw_capacity_conviction_l_v1(reference_escrow.saturating_add(reference_w_floor));
    let floor_class = params.base_class_id();

    // ---- pass 1: claims, grouped by bond in acceptance order ---------------------------------
    let mut by_bond: BTreeMap<PalwBondKeyV2, Vec<(u64, Hash64)>> = BTreeMap::new();
    let mut convicted_claims: BTreeSet<Hash64> = BTreeSet::new();
    for (_, offence) in state.consumed_offences_iter() {
        if offence.claim_id != Hash64::default() {
            convicted_claims.insert(offence.claim_id);
        }
    }
    let mut attribution: BTreeMap<Hash64, PalwCapacityClassAttributionV1> = BTreeMap::new();
    let mut voids: BTreeMap<Hash64, BTreeMap<String, u64>> = BTreeMap::new();
    let mut licence_queue = 0u64;
    let mut licence_queue_oldest: Option<u64> = None;
    let mut licensed_recent = 0u64;
    for (id, claim) in state.claims_iter() {
        let class = attribution
            .entry(claim.class_id)
            .or_insert_with(|| PalwCapacityClassAttributionV1 { class_id: claim.class_id, ..Default::default() });
        match &claim.phase {
            PalwClaimPhaseV2::Final { .. } => class.claims_final += 1,
            PalwClaimPhaseV2::Voided { reason, .. } => {
                class.claims_voided += 1;
                if void_is_attributed(*reason) {
                    class.voids_attributed += 1;
                }
                *voids.entry(claim.class_id).or_default().entry(format!("{reason:?}")).or_default() += 1;
            }
            PalwClaimPhaseV2::PanelBound { bound_daa } => {
                class.claims_live += 1;
                licence_queue += 1;
                licence_queue_oldest = Some(licence_queue_oldest.map_or(*bound_daa, |d| d.min(*bound_daa)));
            }
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                class.claims_live += 1;
                if now_daa.saturating_sub(*licensed_daa) <= PALW_CAPACITY_SHADOW_RECENT_DAA_V1 {
                    licensed_recent += 1;
                }
            }
            _ => class.claims_live += 1,
        }
        if let Some(da) = state.da_claim(id) {
            class.da_opened_non_seat_total += u64::from(da.opened_non_seat_total);
        }
        if adversary.contains(&claim.bond) {
            class.adversary_claims += 1;
            let attributed = convicted_claims.contains(id)
                || matches!(claim.phase, PalwClaimPhaseV2::Voided { reason, .. } if void_is_attributed(reason));
            if attributed {
                class.adversary_attributed += 1;
            }
        }
        by_bond.entry(claim.bond).or_default().push((claim.accepted_daa, *id));
    }

    // ---- pass 2: per bond, the budget in acceptance order, and every claim row --------------
    let mut claims_out: Vec<PalwCapacityClaimShadowV1> = Vec::new();
    let mut accs: BTreeMap<PalwBondKeyV2, BondAcc> = BTreeMap::new();
    // Per claim: the commitment each step's duty is capped by (bind-time commitment′).
    let mut bind_commitment_new: BTreeMap<Hash64, Vec<u128>> = BTreeMap::new();
    let mut claims_commitment_today = 0u128;
    let mut claims_commitment_new = vec![0u128; n_steps];
    for (bond, mut ids) in by_bond {
        ids.sort_unstable();
        let collateral = state.bond(&bond).map(|b| b.collateral).unwrap_or(0);
        let budget = palw_capacity_weight_budget_sompi_v1(collateral);
        let acc = accs.entry(bond).or_insert_with(|| BondAcc {
            own_new: vec![0; n_steps],
            duties_new: vec![0; n_steps],
            ..Default::default()
        });
        let mut held = 0u128;
        for (_, id) in ids {
            let Some(claim) = state.claim(&id) else { continue };
            let free_prompt = matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. });
            let c7 = palw_rcore_class_is_c7_v1(params, state, &claim.class_id);
            let stage = palw_capacity_stage_of_claim_v1(claim);
            let w_full = palw_capacity_weight_full_v1(claim.immature_contribution, c7);
            let staged_w = palw_capacity_staged_weight_v1(stage, w_full);
            let escrow_term_today = params.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
            let commitment_today = palw_claim_commitment_v1(params, claim, now_daa).unwrap_or(0);
            let l_sompi = palw_capacity_conviction_l_v1(palw_claim_g_v1(state, &id).map(|g| g.g()).unwrap_or(0));
            let holds = holds_new_rule(claim, h_obl, now_daa);
            let reserved_new = if free_prompt {
                claim.reserved
            } else if holds {
                let r = palw_capacity_consensus_reservation_v1(claim.reserved, budget, held);
                held = held.saturating_add(r);
                r
            } else {
                0
            };
            let e = u128::from(claim.escrowed_reward);
            let mut m_new = Vec::with_capacity(n_steps);
            let mut commitment_new = Vec::with_capacity(n_steps);
            let mut bind_new = Vec::with_capacity(n_steps);
            for step in &steps {
                if free_prompt {
                    // E-6: the FP lane is unchanged.
                    m_new.push(0);
                    commitment_new.push(commitment_today);
                    bind_new.push(crate::palw_state_v2::palw_claim_bond_reservation_v1(params, claim).unwrap_or(0));
                    continue;
                }
                let m = palw_capacity_m_c_v1(e, Some(step), !c7, l_sompi);
                let full = m.saturating_add(reserved_new);
                let commitment = match &claim.phase {
                    PalwClaimPhaseV2::Final { .. } => 0,
                    PalwClaimPhaseV2::Voided { .. } => {
                        if holds {
                            full
                        } else {
                            0
                        }
                    }
                    _ => {
                        if claim.rcore.escrow_released {
                            reserved_new
                        } else {
                            full
                        }
                    }
                };
                m_new.push(m);
                commitment_new.push(commitment);
                bind_new.push(full);
            }
            let live = commitment_today > 0 || commitment_new.iter().any(|c| *c > 0) || stage.is_provisional();
            if !claim.phase.is_terminal() {
                acc.raw_immature = acc.raw_immature.saturating_add(claim.immature_contribution);
            }
            if stage.is_provisional() {
                acc.x_b = acc.x_b.saturating_add(staged_w);
            }
            acc.own_today = acc.own_today.saturating_add(commitment_today);
            claims_commitment_today = claims_commitment_today.saturating_add(commitment_today);
            for (i, c) in commitment_new.iter().enumerate() {
                acc.own_new[i] = acc.own_new[i].saturating_add(*c);
                claims_commitment_new[i] = claims_commitment_new[i].saturating_add(*c);
            }
            if !free_prompt {
                acc.reserved_new_total = acc.reserved_new_total.saturating_add(reserved_new);
            }
            bind_commitment_new.insert(id, bind_new);
            if !live {
                continue;
            }
            acc.live += 1;
            if matches!(
                claim.phase,
                PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. }
            ) {
                acc.unlicensed += 1;
            }
            claims_out.push(PalwCapacityClaimShadowV1 {
                claim_id: id,
                bond,
                class_id: claim.class_id,
                phase: phase_name(&claim.phase),
                stage,
                accepted_daa: claim.accepted_daa,
                free_prompt,
                c7,
                raw_w: claim.immature_contribution,
                w_full,
                staged_w,
                reserved_today: claim.reserved,
                reserved_new,
                escrow_term_today,
                escrow_released: claim.rcore.escrow_released,
                commitment_today,
                l_sompi,
                m_new,
                commitment_new,
            });
        }
    }

    // ---- seat duties: every row, repriced per step -------------------------------------------
    let mut duty_rows = 0u64;
    let mut duty_rows_capped = 0u64;
    let mut seat_duty_total_today = 0u128;
    let mut seat_duty_total_new = vec![0u128; n_steps];
    // (seat, claim) → duty per step, for the lock excess.
    let mut duty_of: BTreeMap<(PalwBondKeyV2, Hash64), Vec<u128>> = BTreeMap::new();
    let mut floor_duty_sum = 0u128;
    let mut floor_duty_rows = 0u128;
    let mut floor_seat_sum = 0u128;
    for (claim_id, row) in state.panel_duty_rows_iter() {
        let seats = row.seats.len();
        if seats == 0 {
            continue;
        }
        duty_rows += 1;
        let d = row.seat_exposure;
        let claim = state.claim(claim_id);
        let commitment_today_at_bind =
            claim.and_then(|c| crate::palw_state_v2::palw_claim_bond_reservation_v1(params, c)).unwrap_or(u128::MAX);
        if d >= commitment_today_at_bind / seats as u128 {
            duty_rows_capped += 1;
        }
        if claim.is_some_and(|c| c.class_id == floor_class) {
            floor_duty_sum = floor_duty_sum.saturating_add(d);
            floor_duty_rows += 1;
            floor_seat_sum += seats as u128;
        }
        let bind_new = bind_commitment_new.get(claim_id);
        let per_step: Vec<u128> = steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let commitment = bind_new.map(|v| v[i]).unwrap_or(commitment_today_at_bind);
                palw_capacity_seat_duty_v1(d, 0, commitment, seats, step.rho)
            })
            .collect();
        seat_duty_total_today = seat_duty_total_today.saturating_add(d.saturating_mul(seats as u128));
        for (i, duty) in per_step.iter().enumerate() {
            seat_duty_total_new[i] = seat_duty_total_new[i].saturating_add(duty.saturating_mul(seats as u128));
        }
        for seat in row.seats.keys() {
            let acc = accs.entry(*seat).or_insert_with(|| BondAcc {
                own_new: vec![0; n_steps],
                duties_new: vec![0; n_steps],
                ..Default::default()
            });
            acc.duties_today = acc.duties_today.saturating_add(d);
            for (i, duty) in per_step.iter().enumerate() {
                acc.duties_new[i] = acc.duties_new[i].saturating_add(*duty);
            }
            duty_of.insert((*seat, *claim_id), per_step.clone());
        }
    }

    // ---- per bond: A-1 today and per step, N_instant, the freeze ------------------------------
    let mut freezes: BTreeMap<PalwBondKeyV2, BondFreeze> = BTreeMap::new();
    let mut convictions_total = 0u64;
    let mut kinds: BTreeMap<Hash64, BTreeMap<u8, u64>> = BTreeMap::new();
    for (_, offence) in state.consumed_offences_iter() {
        convictions_total += 1;
        let freeze = freezes.entry(PalwBondKeyV2(offence.accused)).or_default();
        freeze.first = Some(freeze.first.map_or(offence.accepted_daa, |d| d.min(offence.accepted_daa)));
        freeze.last = freeze.last.max(offence.accepted_daa);
        freeze.count += 1;
        match palw_capacity_freeze_class_v1(offence.kind) {
            PalwCapacityFreezeClassV1::Intent => freeze.intent = true,
            PalwCapacityFreezeClassV1::Undetermined => freeze.undetermined = true,
            PalwCapacityFreezeClassV1::Tier => {}
        }
        let claim = (offence.claim_id != Hash64::default()).then(|| state.claim(&offence.claim_id)).flatten();
        let class_id = claim.map(|c| c.class_id).unwrap_or_default();
        *kinds.entry(class_id).or_default().entry(offence.kind as u8).or_default() += 1;
        let class = attribution.entry(class_id).or_insert_with(|| PalwCapacityClassAttributionV1 { class_id, ..Default::default() });
        if let Some(claim) = claim {
            class.conviction_latency_histogram[latency_bucket(offence.accepted_daa.saturating_sub(claim.accepted_daa))] += 1;
        }
    }
    for ((claim_id, _), session) in state.da_sessions_iter() {
        let class_id = state.claim(claim_id).map(|c| c.class_id).unwrap_or_default();
        let class = attribution.entry(class_id).or_insert_with(|| PalwCapacityClassAttributionV1 { class_id, ..Default::default() });
        if session.accuser_is_seat {
            class.da_open_seat += 1;
        } else {
            class.da_open_non_seat += 1;
        }
    }

    let escaped_depth = palw_second_clock_depth_v1(options.raw_depth, state.recent_anchor_daas(), now_daa, window_court);
    let settled_now = state.settled_attempt_finals();
    let mut bonds_out: Vec<PalwCapacityBondShadowV1> = Vec::new();
    let mut seat_lock_total_today = 0u128;
    let mut seat_lock_total_new = vec![0u128; n_steps];
    let mut floor_lock_sum = 0u128;
    let mut floor_lock_count = 0u128;
    let mut seats = 0u64;
    let mut seat_usable_capital = 0u128;
    let mut committed_today_total = 0u128;
    let mut committed_new_total = vec![0u128; n_steps];
    let mut bounded_immature_new = 0u128;
    let mut w_cap_total = 0u128;
    let reference_steps_m: Vec<u128> =
        steps.iter().map(|step| palw_capacity_m_c_v1(reference_escrow, Some(step), true, reference_l)).collect();
    let empty_acc = BondAcc { own_new: vec![0; n_steps], duties_new: vec![0; n_steps], ..Default::default() };
    for (bond, record) in state.bonds_iter() {
        let acc = accs.get(bond).unwrap_or(&empty_acc);
        let collateral = record.collateral;
        let ceiling = u128::from(collateral).saturating_mul(u128::from(ratio)) / 1_000;
        let seat = matches!(record.status, PalwBondStatusV2::Active) && !record.capable_classes.is_empty();
        if seat {
            seats += 1;
            seat_usable_capital = seat_usable_capital.saturating_add(ceiling);
        }
        // Locks: today's excess over the duty, and each step's.
        let mut locks_excess_today = 0u128;
        let mut locks_excess_new = vec![0u128; n_steps];
        for ((_, claim_id), lock) in state.slashable_locks_of(bond) {
            let claim = state.claim(claim_id);
            let live =
                lock.is_live_v3(now_daa, settled_now, escaped_depth, window_court) || claim.is_some_and(|c| !c.phase.is_terminal());
            if !live {
                continue;
            }
            let duty_today =
                state.panel_duty_row_of(claim_id).filter(|r| r.seats.contains_key(bond)).map(|r| r.seat_exposure).unwrap_or(0);
            locks_excess_today = locks_excess_today.saturating_add(lock.amount.saturating_sub(duty_today));
            seat_lock_total_today = seat_lock_total_today.saturating_add(lock.amount);
            if claim.is_some_and(|c| c.class_id == floor_class) {
                floor_lock_sum = floor_lock_sum.saturating_add(lock.amount);
                floor_lock_count += 1;
            }
            let e = claim.map(|c| u128::from(c.escrowed_reward)).unwrap_or(reference_escrow);
            let duties = duty_of.get(&(*bond, *claim_id));
            for (i, step) in steps.iter().enumerate() {
                let lock_new = palw_capacity_seat_lock_v1(lock.amount, Some(step), e, PALW_CAPACITY_SEAT_L_REFERENCE_SOMPI_V1);
                let duty_new = duties.map(|v| v[i]).unwrap_or(0);
                locks_excess_new[i] = locks_excess_new[i].saturating_add(lock_new.saturating_sub(duty_new));
                seat_lock_total_new[i] = seat_lock_total_new[i].saturating_add(lock_new);
            }
        }
        let committed_today = palw_bond_committed_raw_v1(state, params, bond, now_daa, options.raw_depth);
        let registration = state.registration_exposure(bond);
        let other_reserved = state.reserved_exposure(bond).saturating_sub(acc.own_today).saturating_sub(acc.duties_today);
        let accuser = palw_accuser_exposure_v1(state, bond);
        let committed_new: Vec<u128> = (0..n_steps)
            .map(|i| {
                acc.own_new[i]
                    .saturating_add(acc.duties_new[i])
                    .saturating_add(other_reserved)
                    .saturating_add(registration)
                    .saturating_add(locks_excess_new[i])
            })
            .collect();
        let room = |committed: u128| palw_rcore_gate_room_of_v1(collateral, ratio, committed, accuser, PalwRcoreGateV1::Work);
        let per_claim_today = reference_escrow.saturating_add(reference_w_floor);
        let n_instant_today = palw_capacity_n_instant_v1(room(committed_today.saturating_sub(acc.own_today)), per_claim_today, 0, 0);
        let n_more_today = palw_capacity_n_instant_v1(room(committed_today), per_claim_today, 0, 0);
        let r_budget = palw_capacity_weight_budget_sompi_v1(collateral);
        let n_instant_new: Vec<u64> = (0..n_steps)
            .map(|i| {
                palw_capacity_n_instant_v1(
                    room(committed_new[i].saturating_sub(acc.own_new[i])),
                    reference_steps_m[i],
                    reference_w_floor,
                    r_budget,
                )
            })
            .collect();
        let n_more_new: Vec<u64> = (0..n_steps)
            .map(|i| {
                palw_capacity_n_instant_v1(
                    room(committed_new[i]),
                    reference_steps_m[i],
                    reference_w_floor,
                    r_budget.saturating_sub(acc.reserved_new_total),
                )
            })
            .collect();
        let w_cap = palw_capacity_weight_cap_v1(collateral);
        let capped = palw_capacity_bond_weight_term_v1(acc.x_b, w_cap);
        bounded_immature_new = bounded_immature_new.saturating_add(capped);
        w_cap_total = w_cap_total.saturating_add(w_cap);
        committed_today_total = committed_today_total.saturating_add(committed_today);
        for (i, c) in committed_new.iter().enumerate() {
            committed_new_total[i] = committed_new_total[i].saturating_add(*c);
        }
        let freeze = freezes.get(bond);
        let (frozen_would_be, freeze_final, freeze_undetermined, first_conviction_daa, convictions) = match freeze {
            None => (false, false, false, None, 0),
            Some(f) => {
                let within = f.last.checked_add(window_court).is_none_or(|lift| now_daa < lift);
                (f.intent || within, f.intent, f.undetermined, f.first, f.count)
            }
        };
        bonds_out.push(PalwCapacityBondShadowV1 {
            bond: *bond,
            collateral,
            ceiling,
            seat,
            live_claims: acc.live,
            unlicensed_claims: acc.unlicensed,
            raw_immature_today: acc.raw_immature,
            w_cap,
            x_b: acc.x_b,
            capped,
            r_budget,
            reserved_new_total: acc.reserved_new_total,
            committed_today,
            own_claims_today: acc.own_today,
            committed_new,
            n_instant_today,
            n_instant_new,
            n_more_today,
            n_more_new,
            frozen_would_be,
            freeze_final,
            freeze_undetermined,
            first_conviction_daa,
            convictions,
        });
    }
    // Claims whose bond record is gone still weigh (no cap: W_cap of no collateral is 0).
    for (bond, acc) in &accs {
        if state.bond(bond).is_none() {
            bounded_immature_new = bounded_immature_new.saturating_add(palw_capacity_bond_weight_term_v1(acc.x_b, 0));
        }
    }

    // ---- the reference floor claim's seat prices, and the §5.4 estimate per step ------------
    let reference_seats = if floor_duty_rows > 0 {
        u32::try_from(floor_seat_sum / floor_duty_rows).unwrap_or(PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1)
    } else {
        PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1
    }
    .max(1);
    let reference_duty = if floor_duty_rows > 0 {
        floor_duty_sum / floor_duty_rows
    } else {
        // λ binds a floor duty at `E/seats` (the harness's 640.17 MSK).
        reference_escrow / u128::from(reference_seats)
    };
    let reference_lock = if floor_lock_count > 0 {
        floor_lock_sum / floor_lock_count
    } else {
        // L-1 at k′ = 2 on the floor's residual gain `w`.
        palw_rcore_lock_v1(reference_w_floor, u64::try_from(reference_escrow).unwrap_or(u64::MAX), 0, 2)
    };
    let seat_capacity = |duty: u128, lock: u128| {
        palw_capacity_claims_per_daa_milli_v1(
            seat_usable_capital,
            palw_capacity_seat_capital_per_claim_v1(&PalwCapacitySeatCapitalInputsV1 {
                duty_seats: reference_seats,
                duty_sompi: duty,
                duty_hold_daa: PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1,
                lock_seats: reference_seats,
                lock_sompi: lock,
                lock_hold_daa: window_court,
            }),
        )
    };
    let seat_capacity_today_milli_per_daa = seat_capacity(reference_duty, reference_lock);
    let block_mass = if options.block_mass_limit == 0 { 500_000 } else { options.block_mass_limit };
    let carriers_per_block = palw_capacity_carriers_per_block_v1(block_mass, PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1);
    let carriage_blocks_to_drain = if carriers_per_block == 0 { u64::MAX } else { licence_queue.div_ceil(carriers_per_block) };

    // ---- attribution rows ---------------------------------------------------------------------
    let mut attribution_out: Vec<PalwCapacityClassAttributionV1> = attribution
        .into_values()
        .map(|mut row| {
            row.voids_by_reason = voids.remove(&row.class_id).map(|m| m.into_iter().collect()).unwrap_or_default();
            row.convictions_by_kind = kinds.remove(&row.class_id).map(|m| m.into_iter().collect()).unwrap_or_default();
            row.q_measured_permille = (row.adversary_claims > 0).then(|| {
                u16::try_from(u128::from(row.adversary_attributed) * 1_000 / u128::from(row.adversary_claims)).unwrap_or(1_000)
            });
            row
        })
        .collect();
    attribution_out.sort_by_key(|row| row.class_id);

    let step_rows: Vec<PalwCapacityStepShadowV1> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let m_floor = reference_steps_m[i];
            let q_needed = palw_capacity_q_needed_permille_v1(reference_escrow, step.rho, reference_l);
            let duty = palw_capacity_seat_duty_v1(
                reference_duty,
                0,
                m_floor.saturating_add(reference_w_floor),
                reference_seats as usize,
                step.rho,
            );
            let lock =
                palw_capacity_seat_lock_v1(reference_lock, Some(step), reference_escrow, PALW_CAPACITY_SEAT_L_REFERENCE_SOMPI_V1);
            let q_alarm =
                attribution_out.iter().filter_map(|row| row.q_measured_permille).any(|q| u32::from(q) < 2 * u32::from(q_needed));
            PalwCapacityStepShadowV1 {
                step: *step,
                m_floor,
                q_needed_permille: q_needed,
                ramp_binds: step.q_credit_permille >= q_needed,
                seat_credit: palw_capacity_seat_credit_applies_v1(
                    step.q_credit_permille,
                    reference_escrow,
                    PALW_CAPACITY_SEAT_L_REFERENCE_SOMPI_V1,
                ),
                claims_commitment_total: claims_commitment_new[i],
                committed_total: committed_new_total[i],
                seat_duty_total: seat_duty_total_new[i],
                seat_lock_total: seat_lock_total_new[i],
                seat_capacity_milli_per_daa: seat_capacity(duty, lock),
                n_instant_13k: palw_capacity_n_instant_v1(
                    u128::from(PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1) * u128::from(ratio) / 1_000,
                    m_floor,
                    reference_w_floor,
                    palw_capacity_weight_budget_sompi_v1(PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1),
                ),
                q_alarm,
            }
        })
        .collect();

    PalwCapacityShadowV1 {
        now_daa,
        tip_daa: state.last_point().map(|p| p.daa_score).unwrap_or(0),
        steps: step_rows,
        claims: claims_out,
        bonds: bonds_out,
        bounded_immature_today: state.bounded_immature(),
        bounded_immature_new,
        safe_weight: state.safe_weight(),
        w_cap_total,
        reference_escrow,
        reference_w_floor,
        reference_l,
        reference_seats,
        reference_duty,
        reference_lock,
        claims_commitment_today,
        committed_today_total,
        seats,
        seat_usable_capital,
        seat_duty_total_today,
        seat_lock_total_today,
        seat_capacity_today_milli_per_daa,
        duty_rows,
        duty_rows_capped,
        licence_queue,
        licence_queue_oldest_bound_daa: licence_queue_oldest,
        licensed_recent,
        carriers_per_block,
        carriage_blocks_to_drain,
        attribution: attribution_out,
        convictions_total,
    }
}

#[cfg(test)]
mod tests {
    //! **S-T1 / S-I4 (shadow half): the shadow on planted states equals ADR-0160's golden tables**
    //! (§4.5, §5.2, E-T3), and each "new" value is the formula applied to the state's own facts.
    use super::*;
    use crate::palw_capacity_formulas_v1::{PALW_CAPACITY_C7_WEIGHT_CEILING_V1, PALW_CAPACITY_FCW_V1, palw_capacity_m_ramp_v1};
    use crate::palw_offence_v1::PalwConsumedOffenceV1;
    use crate::palw_panel_var_v1::PalwSlashableLockV1;
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwBondStateV2, PalwClaimRcoreV1, PalwDeltaEntryV2, PalwPanelDutyRowV1, PalwStateDeltaV2, apply_delta_v2,
    };
    use crate::palw_verification_v2::PalwSegmentMaskV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    const MSK: u64 = 100_000_000;
    /// Appendix A's E and w (sompi), and the raw weights.
    const E: u64 = 320_084_650_080;
    const W_FLOOR: u128 = 10_752_660;
    const W_2M: u128 = 5_974_294_206_820;
    const RAW_2M: u128 = 335_728_175_722_137;
    const NOW: u64 = 5_000;
    /// A genesis card: 939,063.21 MSK.
    const CARD: u64 = 93_906_321_000_000;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond_key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB000 + n), 0))
    }
    fn floor() -> Hash64 {
        h(1)
    }
    fn two_m() -> Hash64 {
        h(3)
    }

    /// testnet-12's shape: β 100‰, window_receipt 600 (= h_obl), window_court 3,000, the 500‰
    /// ceiling, option A's escrow term from genesis, R-core+ from genesis with 2M in C7.
    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 600, 600, 120, 3_000, 1_000, floor(), 4, 1_000, 13_000 * MSK, 800, 600)
            .unwrap()
            .with_fp_exposure_ceiling(500)
            .unwrap()
            .with_escrow_backed_exposure_from_daa(Some(0))
            .with_rcore_plus_mirrors(Some(0), 0, vec![two_m()])
    }

    fn bond(n: u64, collateral: u64, seat: bool) -> PalwBondStateV2 {
        PalwBondStateV2 {
            pubkey: vec![n as u8; 4],
            operator_id: h(0x0B00 + n),
            collateral,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 1,
            payout_payload: h(0x9A00 + n),
            capable_classes: if seat { [floor()].into_iter().collect() } else { Default::default() },
        }
    }

    fn claim(
        producer: u64,
        class_id: Hash64,
        phase: PalwClaimPhaseV2,
        reserved: u128,
        raw: u128,
        accepted_daa: u64,
    ) -> PalwClaimStateV2 {
        PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id,
            bond: bond_key(producer),
            pwu: 1,
            accepted_daa,
            rebound_daa: None,
            accepted_blue_score: accepted_daa,
            accepted_block: h(0xB0),
            trace_root: h(0x71),
            output_root: h(0x72),
            execution_root: h(0xE0),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            reserved,
            immature_contribution: raw,
            escrowed_reward: E,
            work_leaves: 0,
            work_id: None,
            phase,
            rights_reserved: 0,
            job_identity: Hash64::default(),
            rcore: PalwClaimRcoreV1::default(),
        }
    }

    fn floor_claim(producer: u64, phase: PalwClaimPhaseV2, accepted_daa: u64) -> PalwClaimStateV2 {
        claim(producer, floor(), phase, W_FLOOR, PALW_CAPACITY_FCW_V1, accepted_daa)
    }

    /// A planted chain: bonds, claims (with each producer's `reserved_exposure` the sum of its
    /// claims' commitments plus its duties, as the fold keeps it), and `extra` entries.
    struct Plant {
        bonds: Vec<(u64, PalwBondStateV2)>,
        claims: Vec<(Hash64, PalwClaimStateV2)>,
        duties: Vec<(Hash64, PalwPanelDutyRowV1)>,
        extra: Vec<PalwDeltaEntryV2>,
    }

    impl Plant {
        fn new() -> Self {
            Self { bonds: Vec::new(), claims: Vec::new(), duties: Vec::new(), extra: Vec::new() }
        }
        fn bond(mut self, n: u64, collateral: u64, seat: bool) -> Self {
            self.bonds.push((n, bond(n, collateral, seat)));
            self
        }
        fn claim(mut self, id: u64, record: PalwClaimStateV2) -> Self {
            self.claims.push((h(id), record));
            self
        }
        fn state(self) -> PalwChainStateV2 {
            let p = params();
            let mut entries: Vec<PalwDeltaEntryV2> = self
                .bonds
                .iter()
                .map(|(n, b)| PalwDeltaEntryV2::Bond { key: bond_key(*n), old: None, new: Some(b.clone()) })
                .collect();
            let mut exposure: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
            let mut immature = 0u128;
            for (id, c) in &self.claims {
                *exposure.entry(c.bond).or_default() += palw_claim_commitment_v1(&p, c, NOW).unwrap();
                if !c.phase.is_terminal() {
                    immature += c.immature_contribution;
                }
                entries.push(PalwDeltaEntryV2::Claim { key: *id, old: None, new: Some(c.clone()) });
            }
            for (id, row) in &self.duties {
                for seat in row.seats.keys() {
                    *exposure.entry(*seat).or_default() += row.seat_exposure;
                }
                entries.push(PalwDeltaEntryV2::PanelDuties { key: *id, old: None, new: Some(row.clone()) });
            }
            for (key, value) in exposure {
                entries.push(PalwDeltaEntryV2::Exposure { key, old: None, new: Some(value) });
            }
            entries.push(PalwDeltaEntryV2::Weights { old: (0, 0), new: (0, immature) });
            let point = PalwBlockContextV2 { block: h(0xB1), daa_score: NOW, blue_score: NOW, subsidy: 0 };
            entries.push(PalwDeltaEntryV2::LastPoint { old: None, new: Some(point) });
            entries.extend(self.extra);
            apply_delta_v2(&PalwChainStateV2::genesis(), &PalwStateDeltaV2 { point, entries }, &p).expect("the planted state folds")
        }
    }

    fn eight_cards(plant: Plant) -> Plant {
        (0..8u64).fold(plant, |p, i| p.bond(100 + i, CARD, true))
    }

    fn row_of(shadow: &PalwCapacityShadowV1, n: u64) -> &PalwCapacityBondShadowV1 {
        shadow.bonds.iter().find(|b| b.bond == bond_key(n)).expect("the bond's row")
    }

    fn claim_row(shadow: &PalwCapacityShadowV1, id: u64) -> Option<&PalwCapacityClaimShadowV1> {
        shadow.claims.iter().find(|c| c.claim_id == h(id))
    }

    /// **E-T3 and §5.2 on a planted chain**: a fresh 13k / 100k / 1M bond holds 2 / 15 / 156 floor
    /// claims today and 20·50·101·203·2,030 / … at ρ = 10·25·50·100·1000 (q credited at 143‰); the
    /// §4.5 obligations and q-needed; the §5.4 seat capacity of eight genesis cards.
    #[test]
    fn s_t1_fresh_bonds_hold_the_golden_claim_counts() {
        let state = eight_cards(Plant::new())
            .bond(1, 13_000 * MSK, false)
            .bond(2, 100_000 * MSK, false)
            .bond(3, 1_000_000 * MSK, false)
            .bond(9, 1_000_000 * MSK, false)
            .claim(0x900, floor_claim(9, PalwClaimPhaseV2::Provisional, NOW - 3))
            .state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        assert_eq!(shadow.reference_escrow, u128::from(E), "E read off the newest attempt claim");
        assert_eq!(shadow.steps.iter().map(|s| s.step.rho).collect::<Vec<_>>(), vec![10, 25, 50, 100, 1000]);
        let golden: [(u64, u64, [u64; 5]); 3] = [
            (1, 2, [20, 50, 101, 203, 2_030]),
            (2, 15, [156, 390, 781, 1_562, 15_620]),
            (3, 156, [1_562, 3_905, 7_810, 15_620, 156_203]),
        ];
        for (n, today, new) in golden {
            let row = row_of(&shadow, n);
            assert_eq!(row.n_instant_today, today, "bond {n} today");
            assert_eq!(row.n_instant_new, new.to_vec(), "bond {n} per step");
            assert_eq!(row.n_more_new, row.n_instant_new, "a fresh bond can open all of them now");
            assert_eq!(row.committed_today, 0);
        }
        assert_eq!(shadow.steps.iter().map(|s| s.n_instant_13k).collect::<Vec<_>>(), vec![20, 50, 101, 203, 2_030]);
        // §4.5: the obligation is the ramp term, and the q it needs at L = 3G.
        let m: Vec<u128> = shadow.steps.iter().map(|s| s.m_floor).collect();
        assert_eq!(m, [10u32, 25, 50, 100, 1000].map(|rho| palw_capacity_m_ramp_v1(u128::from(E), rho)).to_vec());
        let q: Vec<u16> = shadow.steps.iter().map(|s| s.q_needed_permille).collect();
        assert_eq!((q[0], q[1], q[3], q[4]), (131, 138, 142, 143));
        assert!(shadow.steps.iter().all(|s| s.ramp_binds && s.seat_credit), "143‰ credits the ramp and the seats (L_seat 130k)");
        // §5.4: 8 cards × 469,531.6 MSK against a floor claim's duty and lock — 0.94/DAA today,
        // and ×ρ per step with both ÷ρ (within 0.5% of the ADR's 640.17 / 240.1 MSK inputs).
        assert_eq!(shadow.seats, 8);
        assert_eq!(shadow.seat_usable_capital, 8 * u128::from(CARD) / 2);
        assert_eq!(shadow.reference_duty, u128::from(E) / 5, "λ binds the floor duty at E/5 = 640.17 MSK");
        let lock_msk = shadow.reference_lock / u128::from(MSK);
        assert!((239..=241).contains(&lock_msk), "the floor's L-1 lock at k′ = 2 is ≈ 240.1 MSK, got {lock_msk}");
        let near = |got: u64, want: u64| got.abs_diff(want) * 200 <= want;
        assert!(near(shadow.seat_capacity_today_milli_per_daa, 940), "{}", shadow.seat_capacity_today_milli_per_daa);
        let cap: Vec<u64> = shadow.steps.iter().map(|s| s.seat_capacity_milli_per_daa).collect();
        for (got, want) in cap.iter().zip([9_409u64, 23_523, 47_047, 94_094, 940_944]) {
            assert!(near(*got, want), "seat capacity {got} vs {want}");
        }
        assert_eq!((shadow.carriers_per_block, shadow.licence_queue, shadow.carriage_blocks_to_drain), (3, 0, 0));
        assert!(shadow.summary().contains("N13k[ρ]=10:20,25:50,50:101,100:203,1000:2030"), "{}", shadow.summary());
    }

    /// **W-I1 in the shadow: claims × N is not fork power × N.** A 13k bond's licensed floor claims
    /// weigh 1 FCW each today; under J-1 the bond's term is at most 2 FCW at every N, a 2M claim
    /// included, and the weight reservation spends `R_budget` in acceptance order (W-I4).
    #[test]
    fn s_t1_claims_times_n_is_not_fork_power_times_n() {
        let licensed = |d| PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: d };
        for n in [1u64, 10, 100, 1_000] {
            let mut plant = eight_cards(Plant::new()).bond(1, 13_000 * MSK, false);
            for i in 0..n {
                plant = plant.claim(0x1_0000 + i, floor_claim(1, licensed(NOW - 1), NOW - 30 + (i % 7)));
            }
            let state = plant.state();
            let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
            let row = row_of(&shadow, 1);
            assert_eq!(row.x_b, u128::from(n) * PALW_CAPACITY_FCW_V1, "N = {n}: every licensed claim at full weight");
            assert_eq!(row.capped, u128::from(n.min(2)) * PALW_CAPACITY_FCW_V1, "N = {n}: capped at W_cap = 2 FCW");
            assert_eq!(shadow.bounded_immature_today, u128::from(n) * PALW_CAPACITY_FCW_V1, "today: additive");
            assert_eq!(shadow.bounded_immature_new, row.capped);
            assert!(row.reserved_new_total <= row.r_budget, "W-I4");
            assert_eq!(row.reserved_new_total, W_FLOOR * u128::from(n.min(2)));
            let reserved: Vec<u128> = shadow.claims.iter().map(|c| c.reserved_new).collect();
            assert_eq!(reserved.iter().filter(|r| **r == W_FLOOR).count() as u64, n.min(2), "the first two in acceptance order");
        }
        // A 2M claim (C7 in these params) on the same bond: its reservation falls from 59,742.94 MSK
        // to the budget, its Final weight to 8k's, and the bond's term stays 2 FCW.
        let state = eight_cards(Plant::new())
            .bond(1, 13_000 * MSK, false)
            .claim(0x2000, claim(1, two_m(), licensed(NOW - 1), W_2M, RAW_2M, NOW - 40))
            .claim(0x2001, floor_claim(1, PalwClaimPhaseV2::Provisional, NOW - 2))
            .state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        let big = claim_row(&shadow, 0x2000).unwrap();
        assert!(big.c7);
        assert_eq!(big.w_full, PALW_CAPACITY_C7_WEIGHT_CEILING_V1);
        assert_eq!(big.reserved_new, 2 * W_FLOOR, "R_budget(13k) = 0.215 MSK, all to the first claim");
        assert_eq!(claim_row(&shadow, 0x2001).unwrap().reserved_new, 0, "nothing left for the second");
        assert_eq!(row_of(&shadow, 1).capped, 2 * PALW_CAPACITY_FCW_V1);
        assert_eq!(big.m_new, vec![u128::from(E); 5], "C7: m = E at every step");
        assert_eq!(big.commitment_new, vec![u128::from(E) + 2 * W_FLOOR; 5]);
        assert_eq!(big.commitment_today, u128::from(E) + W_2M);
    }

    /// **The commitment table by phase** (§4.4 E-3/E-4, E-6) and the bond's A-1 recomposed.
    #[test]
    fn s_t1_commitments_by_phase_and_the_void_hold() {
        let mut released = floor_claim(2, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: NOW - 1 }, NOW - 25);
        released.rcore.escrow_released = true;
        let mut fp = floor_claim(2, PalwClaimPhaseV2::Provisional, NOW - 1);
        fp.source = PalwClaimSourceV2::FreePrompt { quanta: 4, spent: Default::default() };
        fp.escrowed_reward = 0;
        fp.rights_reserved = 777;
        let state = eight_cards(Plant::new())
            .bond(2, 100_000 * MSK, false)
            .claim(1, floor_claim(2, PalwClaimPhaseV2::Provisional, NOW - 30))
            .claim(2, floor_claim(2, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 9 }, NOW - 29))
            .claim(3, released)
            .claim(
                4,
                floor_claim(2, PalwClaimPhaseV2::Voided { voided_daa: NOW - 5, reason: PalwVoidReasonV2::BindTimeout }, NOW - 605),
            )
            .claim(
                5,
                floor_claim(
                    2,
                    PalwClaimPhaseV2::Voided { voided_daa: NOW - 601, reason: PalwVoidReasonV2::ReceiptTimeout },
                    NOW - 900,
                ),
            )
            .claim(6, floor_claim(2, PalwClaimPhaseV2::Final { final_daa: NOW - 3 }, NOW - 200))
            .claim(7, fp)
            .state();
        let p = params();
        let steps = [PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 143 }];
        let shadow = palw_capacity_shadow_v1(&state, &p, NOW, &steps);
        let m = palw_capacity_m_ramp_v1(u128::from(E), 10);
        let full_today = u128::from(E) + W_FLOOR;
        let expect: [(u64, u128, u128); 5] = [
            (1, full_today, m + W_FLOOR),
            (2, full_today, m + W_FLOOR),
            (3, W_FLOOR, W_FLOOR),
            // E-4: a void holds m_c + reserved for h_obl = 600, whatever the reason — today it holds nothing.
            (4, 0, m + W_FLOOR),
            (7, W_FLOOR + 777, W_FLOOR + 777),
        ];
        for (id, today, new) in expect {
            let row = claim_row(&shadow, id).unwrap_or_else(|| panic!("claim {id} reported"));
            assert_eq!((row.commitment_today, row.commitment_new[0]), (today, new), "claim {id}");
        }
        assert!(claim_row(&shadow, 5).is_none(), "a void past h_obl holds nothing under either rule");
        assert!(claim_row(&shadow, 6).is_none(), "Final holds nothing");
        assert_eq!(claim_row(&shadow, 7).unwrap().m_new, vec![0], "E-6: the FP lane keeps its accounting");
        assert_eq!(claim_row(&shadow, 1).unwrap().stage, PalwCapacityStageV1::Created);
        assert_eq!(claim_row(&shadow, 2).unwrap().stage, PalwCapacityStageV1::Anchored);
        assert_eq!(claim_row(&shadow, 2).unwrap().staged_w, PALW_CAPACITY_FCW_V1 / 100);
        let row = row_of(&shadow, 2);
        assert_eq!(row.committed_today, state.reserved_exposure(&bond_key(2)), "A-1 = the planted exposure");
        assert_eq!(row.own_claims_today, row.committed_today);
        assert_eq!(row.committed_new, vec![2 * (m + W_FLOOR) + W_FLOOR + (m + W_FLOOR) + W_FLOOR + 777]);
        // The void hold spends budget too: four attempt claims hold, each within R_budget(100k).
        assert_eq!(row.reserved_new_total, 4 * W_FLOOR);
        assert_eq!(row.unlicensed_claims, 3, "provisional, panel-bound and the FP claim");
        assert_eq!(shadow.licence_queue, 1);
        assert_eq!(shadow.licence_queue_oldest_bound_daa, Some(NOW - 9));
        assert_eq!(shadow.licensed_recent, 1);
    }

    /// **Seat duties and locks**: the identity step (ρ 1, q 0) reproduces every seat's A-1; a
    /// credited ρ = 10 step divides the duty (AS-1) and the lock (AS-2) by ten.
    #[test]
    fn s_t1_seat_duties_and_locks_reprice_by_rho() {
        let duty = u128::from(E) / 5;
        let seats: BTreeMap<PalwBondKeyV2, u64> = (100..105u64).map(|n| (bond_key(n), 0)).collect();
        let mut plant = eight_cards(Plant::new())
            .bond(1, 100_000 * MSK, false)
            .claim(0x10, floor_claim(1, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 3 }, NOW - 25))
            .claim(0x11, floor_claim(1, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 2 }, NOW - 24))
            .claim(0x12, floor_claim(1, PalwClaimPhaseV2::Final { final_daa: NOW - 50 }, NOW - 200));
        for id in [0x10u64, 0x11] {
            plant.duties.push((h(id), PalwPanelDutyRowV1 { seats: seats.clone(), seat_exposure: duty }));
        }
        let lock = 24_010_000_000u128;
        plant.extra.push(PalwDeltaEntryV2::SlashableLock {
            key: (bond_key(100), h(0x12)),
            old: None,
            new: Some(PalwSlashableLockV1 {
                claim: h(0x12),
                amount: lock,
                expiry_daa: NOW + 2_950,
                settled_at_final: 0,
                attested: PalwSegmentMaskV2::NONE,
                segments: 0,
            }),
        });
        let state = plant.state();
        let steps = [
            PalwCapacityStepV1 { from_daa: 0, rho: 1, q_credit_permille: 0 },
            PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 143 },
        ];
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &steps);
        for n in 100..108u64 {
            let row = row_of(&shadow, n);
            assert_eq!(row.committed_new[0], row.committed_today, "the identity step is today's A-1 (seat {n})");
        }
        let seat = row_of(&shadow, 100);
        assert_eq!(seat.committed_today, 2 * duty + lock);
        assert_eq!(seat.committed_new[1], 2 * duty.div_ceil(10) + lock.div_ceil(10));
        assert_eq!(shadow.duty_rows, 2);
        assert_eq!(shadow.duty_rows_capped, 0, "the floor duty is λ-bound, below commitment / seats");
        assert_eq!(shadow.seat_duty_total_today, 2 * 5 * duty);
        assert_eq!(shadow.steps[1].seat_duty_total, 2 * 5 * duty.div_ceil(10));
        assert_eq!((shadow.seat_lock_total_today, shadow.steps[1].seat_lock_total), (lock, lock.div_ceil(10)));
        assert_eq!(shadow.steps[0].seat_lock_total, lock, "q 0: no seat credit, the lock stays");
        assert_eq!(shadow.reference_duty, duty, "the floor rows' mean duty");
        assert_eq!(shadow.reference_lock, lock, "the floor locks' mean");
    }

    /// **AG-3 read off the conviction records, and the attribution counters** (convictions by
    /// kind, DA by non-seats, the latency histogram, the adversary q and the A8 alarm).
    #[test]
    fn s_t1_freeze_and_attribution_counters() {
        let offence = |kind, accused: u64, daa: u64, claim_id: Hash64| PalwConsumedOffenceV1 {
            kind,
            accused: bond_key(accused).0,
            amount: 1,
            accepted_daa: daa,
            execution_root: Hash64::default(),
            collected: 1,
            claim_id,
        };
        let voided = |daa, reason| PalwClaimPhaseV2::Voided { voided_daa: daa, reason };
        let mut plant = eight_cards(Plant::new())
            .bond(10, 13_000 * MSK, false)
            .bond(11, 13_000 * MSK, false)
            .bond(12, 13_000 * MSK, false)
            .bond(13, 13_000 * MSK, false)
            .bond(14, 13_000 * MSK, false)
            .claim(0x50, floor_claim(10, voided(NOW - 4_000, PalwVoidReasonV2::ProducerWithholding), NOW - 4_050))
            .claim(0x60, floor_claim(14, voided(NOW - 10, PalwVoidReasonV2::CourtFraud), NOW - 700))
            .claim(0x61, floor_claim(14, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 4 }, NOW - 30))
            .claim(0x62, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 3));
        let kinds = [
            (0xA0, PalwOffenceKindV1::DaDefault, 10, NOW - 4_000, h(0x50)),
            (0xA1, PalwOffenceKindV1::ExecutorEquivocation, 11, NOW - 10, Hash64::default()),
            (0xA2, PalwOffenceKindV1::ExecutorEquivocation, 12, NOW - 3_001, Hash64::default()),
            (0xA3, PalwOffenceKindV1::ExecutorRefuted, 13, NOW - 100, Hash64::default()),
        ];
        for (key, kind, accused, daa, claim_id) in kinds {
            plant.extra.push(PalwDeltaEntryV2::ConsumedOffence {
                key: h(key),
                old: None,
                new: Some(offence(kind, accused, daa, claim_id)),
            });
        }
        let session = |seat| crate::palw_da_rcore_v1::PalwDaSessionV1 {
            opened_daa: NOW - 2,
            deadline_daa: NOW + 20,
            accuser_is_seat: seat,
            exposure: 0,
            units: Vec::new(),
            stage: crate::palw_da_rcore_v1::PalwDaStageV1::Live,
        };
        plant.extra.push(PalwDeltaEntryV2::DaSession { key: (h(0x61), bond_key(100)), old: None, new: Some(session(true)) });
        plant.extra.push(PalwDeltaEntryV2::DaSession { key: (h(0x61), bond_key(101)), old: None, new: Some(session(false)) });
        plant.extra.push(PalwDeltaEntryV2::DaClaim {
            key: h(0x61),
            old: None,
            new: Some(crate::palw_da_rcore_v1::PalwDaClaimV1 {
                open_seat_sessions: 1,
                open_other_sessions: 1,
                opened_non_seat_total: 3,
                ..Default::default()
            }),
        });
        let state = plant.state();
        let p = params();
        let options = PalwCapacityShadowOptionsV1 { adversary_bonds: vec![bond_key(10), bond_key(14)], ..Default::default() };
        let shadow = palw_capacity_shadow_with_v1(&state, &p, NOW, &options);
        let frozen = |n| {
            let r = row_of(&shadow, n);
            (r.frozen_would_be, r.freeze_final, r.freeze_undetermined, r.convictions)
        };
        assert_eq!(frozen(10), (true, true, false, 1), "DA default: intent class, final");
        assert_eq!(frozen(11), (true, false, false, 1), "a tier conviction 10 DAA ago: frozen");
        assert_eq!(frozen(12), (false, false, false, 1), "3,001 DAA ago: lifted at since + window_court");
        assert_eq!(frozen(13), (true, false, true, 1), "ExecutorRefuted: sub-kind undetermined, held for window_court");
        assert_eq!(shadow.convictions_total, 4);
        let floor_row = shadow.attribution.iter().find(|a| a.class_id == floor()).unwrap();
        assert_eq!(floor_row.convictions_by_kind, vec![(PalwOffenceKindV1::DaDefault as u8, 1)]);
        assert_eq!(floor_row.conviction_latency_histogram, [0, 0, 1, 0, 0, 0, 0, 0], "50 DAA from acceptance: the [50, 100) bucket");
        assert_eq!((floor_row.da_open_non_seat, floor_row.da_open_seat, floor_row.da_opened_non_seat_total), (1, 1, 3));
        assert_eq!((floor_row.claims_voided, floor_row.voids_attributed, floor_row.claims_live), (2, 2, 2));
        assert_eq!(floor_row.voids_by_reason, vec![("CourtFraud".to_string(), 1), ("ProducerWithholding".to_string(), 1)]);
        // Adversary bonds 10 and 14 hold four claims, two attributed: q = 500‰ ≥ 2 × 143 — no alarm.
        assert_eq!((floor_row.adversary_claims, floor_row.adversary_attributed, floor_row.q_measured_permille), (4, 2, Some(500)));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm));
        let keyless = shadow.attribution.iter().find(|a| a.class_id == Hash64::default()).unwrap();
        assert_eq!(keyless.convictions_by_kind.iter().map(|(_, n)| n).sum::<u64>(), 3, "records naming no claim");
        // A8: with only bond 14 named, one of three attributed — 333‰ — still ≥ 2 × 143 = 286.
        let one = PalwCapacityShadowOptionsV1 { adversary_bonds: vec![bond_key(14)], ..Default::default() };
        let shadow = palw_capacity_shadow_with_v1(&state, &p, NOW, &one);
        assert!(shadow.steps.iter().all(|s| !s.q_alarm));
        // The measured rate falls under twice what the step needs: the alarm fires (ρ = 10 needs
        // 131, so 250‰ < 262 alarms; the 1000 step needs 143 → 286).
        let state = eight_cards(Plant::new())
            .bond(14, 13_000 * MSK, false)
            .claim(0x60, floor_claim(14, voided(NOW - 10, PalwVoidReasonV2::CourtFraud), NOW - 700))
            .claim(0x61, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 5))
            .claim(0x62, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 4))
            .claim(0x63, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 3))
            .state();
        let shadow = palw_capacity_shadow_with_v1(&state, &p, NOW, &one);
        assert_eq!(shadow.attribution[0].q_measured_permille, Some(250));
        assert!(shadow.steps.iter().all(|s| s.q_alarm), "250‰ < 2 × q_needed at every reference step");
        assert!(shadow.summary().ends_with("q-ALARM"), "{}", shadow.summary());
    }

    /// **S-I3's shape**: every claim and bond is visited once; a large planted chain reports every
    /// live claim, and the per-bond sums equal the per-claim rows.
    #[test]
    fn s_i3_one_pass_over_claims_and_bonds() {
        let mut plant = eight_cards(Plant::new());
        for b in 0..50u64 {
            plant = plant.bond(1_000 + b, (13_000 + 1_000 * b) * MSK, false);
            for i in 0..40u64 {
                plant = plant.claim(0x10_0000 + b * 100 + i, floor_claim(1_000 + b, PalwClaimPhaseV2::Provisional, NOW - 40 + i));
            }
        }
        let state = plant.state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        assert_eq!(shadow.claims.len(), 2_000);
        assert_eq!(shadow.bonds.len(), 58);
        for row in shadow.bonds.iter().filter(|r| !r.seat) {
            let own: Vec<u128> =
                (0..5).map(|i| shadow.claims.iter().filter(|c| c.bond == row.bond).map(|c| c.commitment_new[i]).sum()).collect();
            assert_eq!(row.committed_new, own, "a producer's commitment is its claims'");
            assert_eq!(row.live_claims, 40);
        }
        assert_eq!(shadow.claims_commitment_today, 2_000 * (u128::from(E) + W_FLOOR));
    }
}
