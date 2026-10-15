//! RFC-0016's named fenced constants, the measured-value record gate, the V2 mirror and the two fences' refusals.
//!
//! **Every value here is a PLACEHOLDER** (user decision 2026-10-15: σ and k "set to appropriate values BY MEASUREMENT"). A value is
//! fixed only by the three-stage derivation of RFC-0016 §3.6 / §8.6 (measure the honest minimal capacity → sweep → adversarial and
//! boundary tests), recorded as a [`PalwRfc0016MeasuredRecordV1`] with its evidence and independent-review digests and pinned in
//! [`PALW_RFC0016_MEASURED_RECORDS_V1`]. Until then validation refuses either fence armed, naming the missing record.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::PalwStateParamsV2;

/// **RFC-0016's values, as one fenced set.** Permille values are `x / 1000`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwRfc0016ValuesV1 {
    /// **σ** — the cap on mid-epoch reissue: past the checkpoint a model may consume reissued rights only while its epoch total stays
    /// within `issued_m · (1 + σ)`. `0` reissues nothing (strict `P_m`).
    pub sigma_permille: u32,
    /// **k** — the bound of the OPTIONAL auxiliary per-model ticket tilt `τ_m ∈ [1/k, k]` (RFC-0016 §8.5): `1000` (k = 1) disables it.
    /// Not wired into the lottery in this implementation ([`super::rights::palw_rights_tilt_permille_v1`] is the pure function).
    pub tilt_k_permille: u32,
    /// **φ** — the epoch fraction past which unused rights may be reissued.
    pub reissue_checkpoint_permille: u32,
    /// The pace margin protecting an under-demand model's projected remaining use past φ (`need = consumed · remaining/elapsed · m`).
    pub pace_margin_permille: u32,
    /// **ε** — the entrant reserve, deducted from the fixed epoch total before the `α` allocation (never extra issuance).
    pub entrant_reserve_permille: u32,
    /// One entrant's ceiling, permille of the epoch total.
    pub entrant_max_per_model_permille: u32,
    /// `S_entry`: the seasoned locked capital an entrant needs.
    pub entrant_min_capital_sompi: u64,
    /// How long after its eligibility a model is an entrant (the 30-day-equivalent DAA is the design value; measured in DAA).
    pub entrant_age_daa: u64,
    /// `sigma_cap` — the safety factor applied to measured verifier capacity.
    pub capacity_safety_permille: u32,
    /// The measured long-window (C7, the 2M row) concurrency ceiling; `0` = unmeasured (the derived cap stands alone).
    pub c7_measured_concurrency: u32,
    /// `L_queue` — the pending Final queue's length per class.
    pub queue_len_per_class: u32,
    /// `T_queue` — how long a queued claim may wait before it is voided without slash.
    pub queue_ttl_daa: u64,
    /// `h` — the quote's margin for concurrent winners (parallel blocks).
    pub quote_margin_h: u32,
    /// The verifier share's base (within ADR-0132 C's 10–30 %).
    pub verifier_share_base_permille: u32,
    /// `γ` — the verifier share's slope in utilization.
    pub verifier_slope_gamma_permille: u32,
    /// `u*` — the utilization at which the share is its base.
    pub verifier_util_target_permille: u32,
    /// The OPV route's checks per claim (the per-model check budget scales with the claim rate).
    pub opv_checks_per_claim_permille: u32,
    /// The checks one ready verifier of a model performs per epoch on the OPV route (measured): the model's check supply.
    pub opv_checks_per_verifier_epoch: u32,
}

impl PalwRfc0016ValuesV1 {
    /// **A placeholder** has any term a rule cannot run without at zero (σ = 0 is a legitimate MEASURED choice, so it alone is not a
    /// placeholder mark; the record gate is what pins it).
    pub fn is_placeholder(&self) -> bool {
        self.tilt_k_permille < 1_000
            || self.reissue_checkpoint_permille == 0
            || self.reissue_checkpoint_permille > 1_000
            || self.pace_margin_permille == 0
            || self.entrant_reserve_permille > 1_000
            || self.entrant_age_daa == 0
            || self.capacity_safety_permille == 0
            || self.capacity_safety_permille > 1_000
            || self.queue_len_per_class == 0
            || self.queue_ttl_daa == 0
            || self.opv_checks_per_verifier_epoch == 0
            || self.verifier_share_base_permille < PALW_RFC0016_VERIFIER_SHARE_MIN_PERMILLE_V1
            || self.verifier_share_base_permille > PALW_RFC0016_VERIFIER_SHARE_MAX_PERMILLE_V1
    }
}

/// ADR-0132 C's panel-share clamp, which the verifier-share slope moves within (never outside).
pub const PALW_RFC0016_VERIFIER_SHARE_MIN_PERMILLE_V1: u32 = 100;
pub const PALW_RFC0016_VERIFIER_SHARE_MAX_PERMILLE_V1: u32 = 300;

/// **The binary's RFC-0016 values — PLACEHOLDER (unmeasured).** `k = 1` (tilt disabled); every other term zero. Validation refuses
/// either fence armed while this set has no pinned measured record.
pub const PALW_RFC0016_VALUES_V1: PalwRfc0016ValuesV1 = PalwRfc0016ValuesV1 {
    sigma_permille: 0,
    tilt_k_permille: 1_000,
    reissue_checkpoint_permille: 0,
    pace_margin_permille: 0,
    entrant_reserve_permille: 0,
    entrant_max_per_model_permille: 0,
    entrant_min_capital_sompi: 0,
    entrant_age_daa: 0,
    capacity_safety_permille: 0,
    c7_measured_concurrency: 0,
    queue_len_per_class: 0,
    queue_ttl_daa: 0,
    quote_margin_h: 0,
    verifier_share_base_permille: 0,
    verifier_slope_gamma_permille: 0,
    verifier_util_target_permille: 0,
    opv_checks_per_claim_permille: 0,
    opv_checks_per_verifier_epoch: 0,
};

// ---- User decisions 2026-10-15 (RFC-0016 §14.1): CALIBRATION STARTING POINTS — never production values; production approval is
// ---- withheld and the measured-value gate below still refuses every non-calibration network. ----------------------------------

/// **V2: σ's calibration starting point** (0.25). The sweep still measures every §8.6 candidate.
pub const PALW_RFC0016_SIGMA_CALIBRATION_PERMILLE_V1: u32 = 250;
/// **V2: φ's calibration starting point** (0.5).
pub const PALW_RFC0016_REISSUE_CHECKPOINT_CALIBRATION_PERMILLE_V1: u32 = 500;
/// **V2: the pace margin's calibration starting point** (1.2).
pub const PALW_RFC0016_PACE_MARGIN_CALIBRATION_PERMILLE_V1: u32 = 1_200;
/// **V2: k = 1** — the auxiliary ticket tilt stays OFF (it is not wired into the lottery; a calibration build may not arm another k).
pub const PALW_RFC0016_TILT_K_CALIBRATION_PERMILLE_V1: u32 = 1_000;

/// **V3: the entrant reserve's calibration cap** — 20 ‰ of the network's total block opportunity, as ONE pool shared with RFC-0017's
/// Probation budget (explicit netting in `palw_rights_shared_pool_left_v1`; never reserved twice).
pub const PALW_RFC0016_ENTRANT_RESERVE_MAX_PERMILLE_V1: u32 = 20;
/// **V3: one entrant's (and one Probation model's) calibration cap** — 2 ‰ (RFC-0017's `probation_model_permille` design value).
pub const PALW_RFC0016_ENTRANT_MAX_PER_MODEL_CALIBRATION_PERMILLE_V1: u32 = 2;

/// **The calibration starting set** (user decisions 2026-10-15). The decided terms are the named constants above (and, as their
/// decisions land, V3 / V5 / V8 / V9's); the terms the user left to measurement (`L_queue`, `T_queue`, `h`, `γ`, `u*`, the OPV check
/// supply) are PROBES the harness sweeps — never decided values. An isolated calibration network runs this set unless its build
/// carries `MISAKA_RFC0016_CALIBRATION_VALUES`. Not armable anywhere else (no measured record).
pub const PALW_RFC0016_CALIBRATION_START_V1: PalwRfc0016ValuesV1 = PalwRfc0016ValuesV1 {
    sigma_permille: PALW_RFC0016_SIGMA_CALIBRATION_PERMILLE_V1,
    tilt_k_permille: PALW_RFC0016_TILT_K_CALIBRATION_PERMILLE_V1,
    reissue_checkpoint_permille: PALW_RFC0016_REISSUE_CHECKPOINT_CALIBRATION_PERMILLE_V1,
    pace_margin_permille: PALW_RFC0016_PACE_MARGIN_CALIBRATION_PERMILLE_V1,
    entrant_reserve_permille: PALW_RFC0016_ENTRANT_RESERVE_MAX_PERMILLE_V1,
    entrant_max_per_model_permille: PALW_RFC0016_ENTRANT_MAX_PER_MODEL_CALIBRATION_PERMILLE_V1,
    // V3: `S_entry` is the network's eligible minimum producer bond (the fold takes `max(this, min_collateral_sompi)`); 0 adds nothing.
    entrant_min_capital_sompi: 0,
    // PROBES (measured, not decided):
    entrant_age_daa: 21_600,
    capacity_safety_permille: 1_000,
    c7_measured_concurrency: 0,
    queue_len_per_class: 8,
    queue_ttl_daa: 720,
    quote_margin_h: 2,
    verifier_share_base_permille: 200,
    verifier_slope_gamma_permille: 100,
    verifier_util_target_permille: 700,
    opv_checks_per_claim_permille: 500,
    opv_checks_per_verifier_epoch: 10,
};

/// **The decided calibration terms a calibration build must keep** (user decisions 2026-10-15): `k = 1` (V2: the tilt is not wired,
/// so no build may pretend to measure another `k`). `Err` names the first term that strays; the swept terms (σ, φ, pace) are free.
pub fn palw_rfc0016_calibration_conforms_v1(values: &PalwRfc0016ValuesV1) -> Result<(), &'static str> {
    if values.tilt_k_permille != PALW_RFC0016_TILT_K_CALIBRATION_PERMILLE_V1 {
        return Err("RFC-0016 V2: k = 1 (tilt_k_permille = 1000) — the auxiliary tilt is not wired into the lottery");
    }
    if values.entrant_reserve_permille > PALW_RFC0016_ENTRANT_RESERVE_MAX_PERMILLE_V1 {
        return Err("RFC-0016 V3: the entrant reserve (shared with RFC-0017's Probation budget) is at most 20 permille");
    }
    if values.entrant_max_per_model_permille > values.entrant_reserve_permille {
        return Err("RFC-0016 V3: one entrant's cap cannot exceed the shared pool");
    }
    Ok(())
}

/// σ, named (placeholder until measured).
pub const PALW_RFC0016_SIGMA_PERMILLE_V1: u32 = PALW_RFC0016_VALUES_V1.sigma_permille;
/// k, named (placeholder until measured; 1000 = the tilt is off).
pub const PALW_RFC0016_TILT_K_PERMILLE_V1: u32 = PALW_RFC0016_VALUES_V1.tilt_k_permille;

/// **A measured-value record**: the values the three-stage derivation selected, with the digests of the evidence (harness commit,
/// params, hardware, network conditions, raw data, derivation) and of its independent review. Pinned only by a reviewed change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0016MeasuredRecordV1 {
    pub values: PalwRfc0016ValuesV1,
    pub evidence_digest: [u8; 32],
    pub review_digest: [u8; 32],
    /// The harness and scenario set the record came from (`tools/rfc0016/…`, RFC-0016 §8.6).
    pub harness: &'static str,
}

/// **The pinned measured records — EMPTY.** No value is measured; arming either fence is refused.
pub const PALW_RFC0016_MEASURED_RECORDS_V1: &[PalwRfc0016MeasuredRecordV1] = &[];

/// **Calibration builds only: the values a σ/k measurement run arms** — `MISAKA_RFC0016_CALIBRATION_VALUES` at BUILD time, 18
/// comma-separated integers in the field order of [`PalwRfc0016ValuesV1`]. Read only on an ISOLATED calibration network
/// (`Params::has_isolated_native_calibration_v34`); a production binary is built without it, and the measured-value gate still
/// governs every other network. `None` where unset or malformed.
pub fn palw_rfc0016_calibration_values_v1() -> Option<PalwRfc0016ValuesV1> {
    palw_rfc0016_parse_values_v1(option_env!("MISAKA_RFC0016_CALIBRATION_VALUES")?)
}

/// Parse the 18-integer calibration form (also used by the measurement harness to print what a build arms).
pub fn palw_rfc0016_parse_values_v1(text: &str) -> Option<PalwRfc0016ValuesV1> {
    let n: Vec<u64> = text.split(',').map(|t| t.trim().parse::<u64>().ok()).collect::<Option<Vec<u64>>>()?;
    if n.len() != 18 {
        return None;
    }
    let u = |i: usize| u32::try_from(n[i]).ok();
    Some(PalwRfc0016ValuesV1 {
        sigma_permille: u(0)?,
        tilt_k_permille: u(1)?,
        reissue_checkpoint_permille: u(2)?,
        pace_margin_permille: u(3)?,
        entrant_reserve_permille: u(4)?,
        entrant_max_per_model_permille: u(5)?,
        entrant_min_capital_sompi: n[6],
        entrant_age_daa: n[7],
        capacity_safety_permille: u(8)?,
        c7_measured_concurrency: u(9)?,
        queue_len_per_class: u(10)?,
        queue_ttl_daa: n[11],
        quote_margin_h: u(12)?,
        verifier_share_base_permille: u(13)?,
        verifier_slope_gamma_permille: u(14)?,
        verifier_util_target_permille: u(15)?,
        opv_checks_per_claim_permille: u(16)?,
        opv_checks_per_verifier_epoch: u(17)?,
    })
}

/// **The measured-value gate**: `values` is not a placeholder and exactly matches a pinned record whose evidence and review digests
/// are both non-zero and distinct.
pub fn palw_rfc0016_values_measured_v1(values: &PalwRfc0016ValuesV1, records: &[PalwRfc0016MeasuredRecordV1]) -> bool {
    !values.is_placeholder()
        && palw_rfc0016_calibration_conforms_v1(values).is_ok()
        && records.iter().any(|r| {
            r.values == *values
                && r.evidence_digest != [0u8; 32]
                && r.review_digest != [0u8; 32]
                && r.evidence_digest != r.review_digest
        })
}

/// **The fences' mirror on the V2 bundle's state params** (`PalwStateParamsV2::rfc0016_v1`): the two heights and the value set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0016MirrorV1 {
    pub rights_from_daa: Option<u64>,
    pub capacity_from_daa: Option<u64>,
    pub values: PalwRfc0016ValuesV1,
}

impl PalwStateParamsV2 {
    /// Written by `Params::sync_palw_rfc0016_v1` (and fixtures) only.
    pub fn with_rfc0016_v1(mut self, mirror: Option<PalwRfc0016MirrorV1>) -> Self {
        self.rfc0016_v1 = mirror;
        self
    }

    /// The mirror, if either fence is armed.
    pub fn rfc0016_v1(&self) -> Option<PalwRfc0016MirrorV1> {
        self.rfc0016_v1
    }

    /// Is V+ (pre-issued block rights) in force at `daa`? `false` on every shipped preset.
    pub fn rfc0016_rights_active_at(&self, daa: u64) -> bool {
        self.rfc0016_v1.and_then(|m| m.rights_from_daa).is_some_and(|from| daa >= from)
    }

    /// Is the elastic Final capacity in force at `daa`? `false` on every shipped preset.
    pub fn rfc0016_capacity_active_at(&self, daa: u64) -> bool {
        self.rfc0016_v1.and_then(|m| m.capacity_from_daa).is_some_and(|from| daa >= from)
    }
}

impl Params {
    /// `palw_model_block_rights_v1`'s height, `Some` only for a real one.
    pub fn palw_model_block_rights_v1_from_daa(&self) -> Option<u64> {
        self.palw_model_block_rights_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// `palw_elastic_final_capacity_v1`'s height, `Some` only for a real one.
    pub fn palw_elastic_final_capacity_v1_from_daa(&self) -> Option<u64> {
        self.palw_elastic_final_capacity_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    fn palw_rfc0016_mirror_v1(&self) -> Option<PalwRfc0016MirrorV1> {
        let rights_from_daa = self.palw_model_block_rights_v1_from_daa();
        let capacity_from_daa = self.palw_elastic_final_capacity_v1_from_daa();
        // An isolated calibration network runs the values its build was made for (the σ/k measurement); every other network runs
        // the binary's named constants.
        let values = if self.has_isolated_native_calibration_v34() {
            // User decisions 2026-10-15: without a build-time value set the calibration network runs the calibration starting set.
            palw_rfc0016_calibration_values_v1().unwrap_or(PALW_RFC0016_CALIBRATION_START_V1)
        } else {
            PALW_RFC0016_VALUES_V1
        };
        (rights_from_daa.is_some() || capacity_from_daa.is_some()).then_some(PalwRfc0016MirrorV1 {
            rights_from_daa,
            capacity_from_daa,
            values,
        })
    }

    /// **The two fences' mirror** on the V2 bundle's state params; `None` where neither is armed.
    pub fn sync_palw_rfc0016_v1(&mut self) {
        let mirror = self.palw_rfc0016_mirror_v1();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_rfc0016_v1(mirror);
        }
    }

    /// **The calibration exception** (the σ/k measurement, RFC-0016 §8.6): on an isolated calibration network a fence may be armed
    /// once its build carries non-placeholder calibration values; the structural prerequisites above it still hold.
    fn palw_rfc0016_calibration_allows_v1(&self) -> Option<Result<(), PalwModeV2Error>> {
        if !self.has_isolated_native_calibration_v34() {
            return None;
        }
        let values = palw_rfc0016_calibration_values_v1().unwrap_or(PALW_RFC0016_CALIBRATION_START_V1);
        if values.is_placeholder() {
            return Some(Err(PalwModeV2Error::Invalid(
                "an RFC-0016 calibration run needs non-placeholder values (MISAKA_RFC0016_CALIBRATION_VALUES, or the calibration starting set)",
            )));
        }
        Some(palw_rfc0016_calibration_conforms_v1(&values).map_err(PalwModeV2Error::Invalid))
    }

    fn palw_rfc0016_below_v1(fence: Option<ForkActivation>, at: u64) -> bool {
        fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at)
    }

    fn validate_palw_rfc0016_mirror_v1(&self) -> Result<(), PalwModeV2Error> {
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.rfc0016_v1() != self.palw_rfc0016_mirror_v1()
        {
            return Err(PalwModeV2Error::Invalid(
                "RFC-0016's fences disagree with the V2 bundle's mirror: Params::sync_palw_rfc0016_v1",
            ));
        }
        Ok(())
    }

    /// **`palw_model_block_rights_v1`'s refusals** (RFC-0016 V+): the mirror; a ConsensusV2 rule; the allocation it issues from
    /// (`palw_bond_budget_v1` and `palw_model_bond_allocation_v1` at or below it, the allocation epoch a whole number of rights epochs so
    /// `P_m` is constant within one); the measured-value gate; and — implemented dormant, not adopted for activation — any armed
    /// height is refused until the single full-activation release names one.
    pub fn validate_palw_model_block_rights_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_rfc0016_mirror_v1()?;
        let Some(at) = self.palw_model_block_rights_v1_from_daa() else { return Ok(()) };
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            return Err(PalwModeV2Error::Invalid("palw_model_block_rights_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        };
        let budget =
            self.palw_bond_budget_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score());
        if budget.is_none_or(|b| b > at) {
            return Err(PalwModeV2Error::Invalid("palw_model_block_rights_v1 requires palw_bond_budget_v1 at or below it"));
        }
        let Some(allocation) = self.palw_model_bond_allocation_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1 requires palw_model_bond_allocation_v1 (the P_m it issues from) at or below it",
            ));
        };
        if allocation.activation.daa_score() > at {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1 requires palw_model_bond_allocation_v1 (the P_m it issues from) at or below it",
            ));
        }
        // `P_m` is applied to block opportunity ONCE (RFC-0015 §8.5.5): V+ is RFC-0016's physical-layer gate, so the ADR-0177 V3
        // policy's own reward-bearing block-opportunity reservation must be off where V+ is armed (one gate, never two).
        if allocation.policy.average.is_some_and(|a| a.block_opportunity) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1 and the allocation's block_opportunity would apply P_m to blocks twice: arm one",
            ));
        }
        // The reissue snapshot and the telemetry are kernel-route aux rows (132, 135): the route must exist at or below.
        if !Self::palw_rfc0016_below_v1(self.palw_probabilistic_constraints_v1, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1 requires palw_probabilistic_constraints_v1 (the kernel route whose rows hold its ledger) at or below it",
            ));
        }
        let rights_epoch = bundle.state.epoch_length().max(1);
        // V3 (user decision 2026-10-15): the entrant reserve and RFC-0017's Probation budget are ONE pool netted per epoch, so the
        // two must count the same epoch where both are armed.
        if let Some(eligibility) = bundle.state.model_eligibility_v1()
            && eligibility.policy.probation_epoch_daa != rights_epoch
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1: the shared new-model pool (V3) needs RFC-0017's probation epoch equal to the rights epoch",
            ));
        }
        if allocation.policy.epoch_daa == 0 || allocation.policy.epoch_daa % rights_epoch != 0 {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1: the allocation epoch must be a whole number of rights epochs (P_m constant within one)",
            ));
        }
        if let Some(verdict) = self.palw_rfc0016_calibration_allows_v1() {
            return verdict;
        }
        if !palw_rfc0016_values_measured_v1(&PALW_RFC0016_VALUES_V1, PALW_RFC0016_MEASURED_RECORDS_V1) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_block_rights_v1 cannot be armed: σ, k, φ, the pace margin and the entrant terms have no pinned measured-value record (RFC-0016 §8.6)",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_model_block_rights_v1 cannot be armed: implemented dormant, not adopted for activation (RFC-0016 §14 gate open)",
        ))
    }

    /// **`palw_elastic_final_capacity_v1`'s refusals**: the mirror; a ConsensusV2 rule on a network whose model registry governs; the
    /// measured-value gate (capacity safety, the C7 ceiling, queue terms); and any armed height, as above.
    pub fn validate_palw_elastic_final_capacity_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_rfc0016_mirror_v1()?;
        let Some(at) = self.palw_elastic_final_capacity_v1_from_daa() else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_elastic_final_capacity_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle",
            ));
        }
        if self.palw_model_registry.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_elastic_final_capacity_v1 requires palw_model_registry at or below it: it replaces the registry's room",
            ));
        }
        // The pending queue and its telemetry are kernel-route aux rows (133–135): the route must exist at or below.
        if !Self::palw_rfc0016_below_v1(self.palw_probabilistic_constraints_v1, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_elastic_final_capacity_v1 requires palw_probabilistic_constraints_v1 (the kernel route whose rows hold its queue) at or below it",
            ));
        }
        if let Some(verdict) = self.palw_rfc0016_calibration_allows_v1() {
            return verdict;
        }
        if !palw_rfc0016_values_measured_v1(&PALW_RFC0016_VALUES_V1, PALW_RFC0016_MEASURED_RECORDS_V1) {
            return Err(PalwModeV2Error::Invalid(
                "palw_elastic_final_capacity_v1 cannot be armed: the capacity safety factor, the C7 ceiling and the queue terms have no pinned measured-value record",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_elastic_final_capacity_v1 cannot be armed: implemented dormant, not adopted for activation (RFC-0016 §14 gate open)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> PalwRfc0016ValuesV1 {
        // A TEST value set (not a proposal): every term a rule needs is non-zero.
        PalwRfc0016ValuesV1 {
            sigma_permille: 250,
            tilt_k_permille: 1_000,
            reissue_checkpoint_permille: 500,
            pace_margin_permille: 1_200,
            entrant_reserve_permille: 20,
            entrant_max_per_model_permille: 5,
            entrant_min_capital_sompi: 1,
            entrant_age_daa: 100,
            capacity_safety_permille: 800,
            c7_measured_concurrency: 0,
            queue_len_per_class: 8,
            queue_ttl_daa: 50,
            quote_margin_h: 2,
            verifier_share_base_permille: 150,
            verifier_slope_gamma_permille: 100,
            verifier_util_target_permille: 700,
            opv_checks_per_claim_permille: 500,
            opv_checks_per_verifier_epoch: 10,
        }
    }

    #[test]
    fn the_calibration_form_parses_exactly_eighteen_integers() {
        let v = palw_rfc0016_parse_values_v1("250,1000,500,1200,20,5,1,100,800,0,8,50,2,150,100,700,500,10").unwrap();
        assert_eq!(v, measured());
        assert!(palw_rfc0016_parse_values_v1("1,2,3").is_none());
        assert!(palw_rfc0016_parse_values_v1("x,1000,500,1200,20,5,1,100,800,0,8,50,2,150,100,700,500,10").is_none());
    }

    #[test]
    fn the_shipped_values_are_a_placeholder_and_no_record_is_pinned() {
        assert!(PALW_RFC0016_VALUES_V1.is_placeholder());
        assert!(PALW_RFC0016_MEASURED_RECORDS_V1.is_empty());
        assert!(!palw_rfc0016_values_measured_v1(&PALW_RFC0016_VALUES_V1, PALW_RFC0016_MEASURED_RECORDS_V1));
        assert_eq!(PALW_RFC0016_TILT_K_PERMILLE_V1, 1_000, "k = 1: the auxiliary tilt is off");
    }

    /// User decisions 2026-10-15 (V2): the calibration starting set is σ = 0.25, φ = 0.5, pace margin 1.2, k = 1 — runnable in
    /// calibration, never a measured record (so never armable elsewhere).
    #[test]
    fn v2_the_calibration_starting_set_is_the_decided_one() {
        let v = PALW_RFC0016_CALIBRATION_START_V1;
        assert_eq!(
            (v.sigma_permille, v.reissue_checkpoint_permille, v.pace_margin_permille, v.tilt_k_permille),
            (250, 500, 1_200, 1_000)
        );
        assert!(!v.is_placeholder(), "a calibration run can arm it");
        assert!(palw_rfc0016_calibration_conforms_v1(&v).is_ok());
        assert!(!palw_rfc0016_values_measured_v1(&v, PALW_RFC0016_MEASURED_RECORDS_V1), "not a measured record");
        let mut tilted = v;
        tilted.tilt_k_permille = 1_500;
        assert!(palw_rfc0016_calibration_conforms_v1(&tilted).is_err(), "k stays 1");
    }

    #[test]
    fn the_gate_needs_a_pinned_record_with_real_digests() {
        let v = measured();
        assert!(!v.is_placeholder());
        assert!(!palw_rfc0016_values_measured_v1(&v, &[]), "unpinned");
        let zero = PalwRfc0016MeasuredRecordV1 { values: v, evidence_digest: [0; 32], review_digest: [1; 32], harness: "t" };
        assert!(!palw_rfc0016_values_measured_v1(&v, &[zero]), "no evidence");
        let same = PalwRfc0016MeasuredRecordV1 { values: v, evidence_digest: [2; 32], review_digest: [2; 32], harness: "t" };
        assert!(!palw_rfc0016_values_measured_v1(&v, &[same]), "review is not independent");
        let ok = PalwRfc0016MeasuredRecordV1 { values: v, evidence_digest: [2; 32], review_digest: [3; 32], harness: "t" };
        assert!(palw_rfc0016_values_measured_v1(&v, &[ok]));
        let mut other = v;
        other.sigma_permille += 1;
        assert!(!palw_rfc0016_values_measured_v1(&other, &[ok]), "a different σ is not the measured one");
    }
}
