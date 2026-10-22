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
    /// How long after its eligibility a model is an entrant (the averaging window's DAA, 7 day-equivalents since 2026-10-16, is the
    /// design value; measured in DAA).
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

/// **V5: the calibration epoch, in DAA** — 720 DAA = ONE epoch (the nominal day at the 120 s clock slot). It is a DAA count: its
/// wall-clock span is MEASURED and reported (the report's epoch clock, the harness's drift), never labelled "24 h".
pub const PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1: u64 = 720;
/// **V5: the S̄_m average's nominal window, in DAA** — 7 calibration epochs (5,040 DAA; user decision 2026-10-16, was 30 / 21,600); a DAA
/// count, not a wall-clock week.
pub const PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1: u64 = 7 * PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1;

/// **V9: the verifier share's calibration baseline** — 20 % (200 ‰), inside ADR-0132 C's 10–30 % clamp which the slope never leaves.
/// A split of the SAME claim pay: no new issuance pays verifiers.
pub const PALW_RFC0016_VERIFIER_SHARE_BASE_CALIBRATION_PERMILLE_V1: u32 = 200;

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
    // V5: an entrant is a model younger than the averaging window (7 epochs = 5,040 DAA; user decision 2026-10-16, re-confirmed
    // 2026-10-22; was 30 / 21,600).
    entrant_age_daa: PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1,
    // PROBES (measured, not decided):
    capacity_safety_permille: 1_000,
    c7_measured_concurrency: 0,
    queue_len_per_class: 8,
    queue_ttl_daa: 720,
    quote_margin_h: 2,
    verifier_share_base_permille: PALW_RFC0016_VERIFIER_SHARE_BASE_CALIBRATION_PERMILLE_V1,
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

/// **V11 (user decision 2026-10-15): the coinbase-receiving mining route is NOT migrated off the Panel this round.** The V2 Attempt
/// route keeps its existing Panel (Panel=1 hold, coinbase-derived escrow); the Kernel/OPV route's Panel=0 is a different, user-paid
/// route and never makes the coinbase route Panel-free (D-P5). A migration needs real-node tests of verification, rotation,
/// liability and Final first, then a separate decision — flipping this is that decision, never a value.
pub const PALW_RFC0016_COINBASE_PANEL_MIGRATION_V1: bool = false;

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

// ---- The §14 arm-readiness record gate (lane rfcgap-b, 2026-10-22) ------------------------------------------------------------------
//
// Before this gate the two validators ended in a fixed "implemented dormant, not adopted for activation" refusal AFTER the
// measured-value gate, so no evidence could ever arm either fence. RFC-0016's design (V+) is adopted (D-V1); what remains open is
// §14's AND contract. The gate below names each remaining §14 conjunct and admits an armed height only when a record for exactly
// (fence, network, height) carrying one evidence digest per conjunct and a review digest is pinned. The list is EMPTY, so every armed
// height is still refused (same outcome as before, now naming what is missing). The measured-value gate above it is unchanged.

/// The two RFC-0016 fences the §14 record gate governs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwRfc0016GatedFenceV1 {
    /// `palw_model_block_rights_v1` (V+).
    ModelBlockRightsV1,
    /// `palw_elastic_final_capacity_v1`.
    ElasticFinalCapacityV1,
}

/// RFC-0016 §14's conjuncts shared by both fences (the values conjunct is the measured-value gate, checked before this one).
const RFC0016_C1_C5: &str = "§13 C1–C5 complete (mode-specific W_hold, S_m = 7-day-average miner + verifier capital, α = 1.5, escrow never clipped, R shortage refused before acceptance)";
const RFC0016_S12: &str = "§12 tests (T1–T7, N1–N7, P4a/P4b, M1–M7, K1a, H1–H4, A1–A12) PASS on the physical-block and payout columns with independent review";
const RFC0016_E2E: &str = "real-node E2E PASS: V2 Panel route, Kernel route, Panel=0/Panel=1 mode switch, reorg / restart / IBD";
const RFC0016_ADR0177: &str = "ADR-0177 two-stage evaluation recomputed with the measured values";
const RFC0016_COORDINATED: &str = "the single coordinated all-RFC activation release names this network and height";

impl PalwRfc0016GatedFenceV1 {
    pub const ALL: [Self; 2] = [Self::ModelBlockRightsV1, Self::ElasticFinalCapacityV1];

    pub fn name(self) -> &'static str {
        match self {
            Self::ModelBlockRightsV1 => "palw_model_block_rights_v1",
            Self::ElasticFinalCapacityV1 => "palw_elastic_final_capacity_v1",
        }
    }

    /// The real height this fence is armed at on `params`, if any.
    pub fn armed(self, params: &Params) -> Option<u64> {
        match self {
            Self::ModelBlockRightsV1 => params.palw_model_block_rights_v1_from_daa(),
            Self::ElasticFinalCapacityV1 => params.palw_elastic_final_capacity_v1_from_daa(),
        }
    }

    /// The evidence a record must carry, one digest per entry, in this order (RFC-0016 §14 ACTIVATION_ALLOWED).
    pub fn requirements(self) -> &'static [&'static str] {
        match self {
            Self::ModelBlockRightsV1 => &[
                RFC0016_C1_C5,
                RFC0016_S12,
                RFC0016_E2E,
                RFC0016_ADR0177,
                "§8.6 decision on k and on wiring the tilt into the lottery (V1), recorded with the σ/k measurement",
                RFC0016_COORDINATED,
            ],
            Self::ElasticFinalCapacityV1 => &[
                RFC0016_C1_C5,
                RFC0016_S12,
                RFC0016_E2E,
                RFC0016_ADR0177,
                "V8: capacity attestation form, sigma_cap and the compute bound decided; per-class claim loads measured (PALW_RFC0016_CLAIM_LOADS_V1)",
                RFC0016_COORDINATED,
            ],
        }
    }

    /// The refusal while no record for this fence, network and height is pinned.
    pub fn no_record_refusal(self) -> &'static str {
        match self {
            Self::ModelBlockRightsV1 => {
                "palw_model_block_rights_v1 cannot be armed: no RFC-0016 §14 arm-readiness record is pinned for this network and height \
                 (PALW_RFC0016_ARMING_RECORDS_V1 is empty)"
            }
            Self::ElasticFinalCapacityV1 => {
                "palw_elastic_final_capacity_v1 cannot be armed: no RFC-0016 §14 arm-readiness record is pinned for this network and \
                 height (PALW_RFC0016_ARMING_RECORDS_V1 is empty)"
            }
        }
    }
}

/// The record format's version.
pub const PALW_RFC0016_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **An RFC-0016 §14 arm-readiness record** — same shape and form rules as `crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1`.
/// `values` must equal the value set the binary arms (the one the measured-value gate admitted), so a record cannot outlive a value
/// change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0016ArmingRecordV1 {
    pub version: u16,
    pub fence: PalwRfc0016GatedFenceV1,
    pub genesis: crate::Hash64,
    pub activation_daa: u64,
    pub values: PalwRfc0016ValuesV1,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned §14 records — EMPTY.** Every RFC-0016 fence armed at a real height is refused.
pub const PALW_RFC0016_ARMING_RECORDS_V1: &[PalwRfc0016ArmingRecordV1] = &[];

/// Does `records` authorize arming `fence` at `activation_daa` on the network `genesis` with `values`? The first record naming exactly
/// this (fence, network, height) decides; its form must hold. `Err` carries the refusal text.
pub fn palw_rfc0016_arming_record_admits_v1(
    fence: PalwRfc0016GatedFenceV1,
    genesis: crate::Hash64,
    activation_daa: u64,
    values: &PalwRfc0016ValuesV1,
    records: &[PalwRfc0016ArmingRecordV1],
) -> Result<(), &'static str> {
    let Some(r) = records.iter().find(|r| r.fence == fence && r.genesis == genesis && r.activation_daa == activation_daa) else {
        return Err(fence.no_record_refusal());
    };
    if r.version != PALW_RFC0016_ARMING_RECORD_VERSION_V1 {
        return Err("the RFC-0016 arm-readiness record's version is not this binary's");
    }
    if r.values != *values {
        return Err("the RFC-0016 arm-readiness record was made for another value set than the one this binary arms");
    }
    if r.source_commit == [0; 20] {
        return Err("the RFC-0016 arm-readiness record names no source commit");
    }
    if r.evidence.len() != fence.requirements().len() {
        return Err("the RFC-0016 arm-readiness record does not carry one evidence digest per §14 requirement");
    }
    if r.review == [0; 32] {
        return Err("the RFC-0016 arm-readiness record carries no review digest");
    }
    for (i, d) in r.evidence.iter().enumerate() {
        if *d == [0; 32] || *d == r.review || r.evidence[..i].contains(d) {
            return Err("an RFC-0016 arm-readiness evidence digest is zero, repeated, or equal to the review");
        }
    }
    Ok(())
}

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
    /// **V8's measured per-class claim loads** ([`PALW_RFC0016_CLAIM_LOADS_V1`]; empty: unmeasured). Past the capacity fence an attested
    /// verifier is credited for a class only where the class has a record — an unmeasured class credits NOTHING (fail closed), never
    /// a compute-only figure; the court admission bound of such a class is 0 too.
    pub claim_loads: &'static [PalwRfc0016ClassLoadV1],
}

/// **V8: one class's measured per-claim DA and court load on one verifier** (audit 16-A, user decision 2026-10-17: measured per
/// class/model with the release binary as part of the measurement records). Compute and memory are the model's own registry facts
/// (`economic_ccu_per_claim`, `working_set_bytes`); these two have no chain fact, so they are measured values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0016ClassLoadV1 {
    pub class_id: [u8; 64],
    /// DA bytes one verifier fetches and retains for one claim over its verification window.
    pub da_bytes_per_claim: u64,
    /// Court units one verifier carries for one claim (the unit the attestations' court figure is per span in).
    pub court_units_per_claim: u64,
}

/// **The binary's V8 per-class loads — UNMEASURED (empty)**: until the release-phase measurement records fix them, past the capacity
/// fence no attested verifier is credited and no court-demanding claim fits the derived court bound; validation refuses the fence anyway.
pub const PALW_RFC0016_CLAIM_LOADS_V1: &[PalwRfc0016ClassLoadV1] = &[];

impl PalwRfc0016MirrorV1 {
    /// A fixture's measured loads (the binary's are [`PALW_RFC0016_CLAIM_LOADS_V1`]).
    pub fn with_claim_loads(mut self, claim_loads: &'static [PalwRfc0016ClassLoadV1]) -> Self {
        self.claim_loads = claim_loads;
        self
    }

    /// The measured load of `class`, if it has a record.
    pub fn claim_load_of(&self, class: &crate::Hash64) -> Option<PalwRfc0016ClassLoadV1> {
        self.claim_loads.iter().find(|l| l.class_id == class.as_bytes()).copied()
    }
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
            claim_loads: PALW_RFC0016_CLAIM_LOADS_V1,
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
    /// `P_m` is constant within one); the measured-value gate; and the §14 arm-readiness record for exactly this network and height
    /// ([`PALW_RFC0016_ARMING_RECORDS_V1`], empty: every armed height is refused until the single full-activation release pins one).
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
        // V5 (user decision 2026-10-15; window 7 epochs since 2026-10-16, re-confirmed 2026-10-22): a calibration run measures ONE
        // epoch = 720 DAA and the 7-day average window = 5,040 DAA.
        if self.has_isolated_native_calibration_v34() {
            let span = allocation.policy.average.map(|a| a.span_daa());
            if rights_epoch != PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1
                || allocation.policy.epoch_daa != PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1
                || span.is_some_and(|span| span != PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1)
            {
                return Err(PalwModeV2Error::Invalid(
                    "an RFC-0016 calibration run uses one epoch = 720 DAA (rights and allocation) and the 5,040-DAA 7-day average window (V5)",
                ));
            }
        }
        if let Some(verdict) = self.palw_rfc0016_calibration_allows_v1() {
            return verdict;
        }
        self.palw_rfc0016_value_and_arm_gate_v1(
            PalwRfc0016GatedFenceV1::ModelBlockRightsV1,
            &PALW_RFC0016_VALUES_V1,
            PALW_RFC0016_MEASURED_RECORDS_V1,
            PALW_RFC0016_ARMING_RECORDS_V1,
        )
    }

    /// **The last two gates of either fence** (after its structural refusals and the calibration exception): the measured-value gate
    /// (`values` must match a pinned measured record), then the §14 arm-readiness record for exactly (fence, network, height). The node
    /// passes the binary's constants; tests and arm-readiness tooling pass their own lists. An unarmed fence passes.
    pub fn palw_rfc0016_value_and_arm_gate_v1(
        &self,
        fence: PalwRfc0016GatedFenceV1,
        values: &PalwRfc0016ValuesV1,
        measured: &[PalwRfc0016MeasuredRecordV1],
        arming: &[PalwRfc0016ArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = fence.armed(self) else { return Ok(()) };
        if !palw_rfc0016_values_measured_v1(values, measured) {
            return Err(PalwModeV2Error::Invalid(match fence {
                PalwRfc0016GatedFenceV1::ModelBlockRightsV1 => {
                    "palw_model_block_rights_v1 cannot be armed: σ, k, φ, the pace margin and the entrant terms have no pinned measured-value record (RFC-0016 §8.6)"
                }
                PalwRfc0016GatedFenceV1::ElasticFinalCapacityV1 => {
                    "palw_elastic_final_capacity_v1 cannot be armed: the capacity safety factor, the C7 ceiling and the queue terms have no pinned measured-value record"
                }
            }));
        }
        palw_rfc0016_arming_record_admits_v1(fence, self.genesis.hash, at, values, arming).map_err(PalwModeV2Error::Invalid)
    }

    /// **`palw_elastic_final_capacity_v1`'s refusals**: the mirror; a ConsensusV2 rule on a network whose model registry governs; the
    /// measured-value gate (capacity safety, the C7 ceiling, queue terms); and the §14 arm-readiness record, as above.
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
        self.palw_rfc0016_value_and_arm_gate_v1(
            PalwRfc0016GatedFenceV1::ElasticFinalCapacityV1,
            &PALW_RFC0016_VALUES_V1,
            PALW_RFC0016_MEASURED_RECORDS_V1,
            PALW_RFC0016_ARMING_RECORDS_V1,
        )
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

    /// **rfcgap-b (RFC-0016 §14):** the fixed "not adopted for activation" refusal is replaced by an arm-readiness record gate for
    /// exactly (fence, network, height); the pinned list is empty, so every armed height is still refused — now by name. A record
    /// admits only with the value set it was made for, one non-zero distinct evidence digest per §14 requirement and a review digest.
    #[test]
    fn rfcgap_b_rfc0016_arm_record_gate_is_exact_and_the_pinned_list_is_empty() {
        use crate::config::params::palw_t12_shipped_params;
        assert!(PALW_RFC0016_ARMING_RECORDS_V1.is_empty());
        static EVIDENCE: [[u8; 32]; 6] = [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32]];
        static DUPLICATED: [[u8; 32]; 6] = [[1; 32], [1; 32], [3; 32], [4; 32], [5; 32], [6; 32]];
        let values = measured();
        let measured_record = PalwRfc0016MeasuredRecordV1 { values, evidence_digest: [7; 32], review_digest: [8; 32], harness: "test" };
        for fence in PalwRfc0016GatedFenceV1::ALL {
            assert_eq!(fence.requirements().len(), 6, "{}: one digest per §14 conjunct", fence.name());
            let shipped = palw_t12_shipped_params();
            assert_eq!(fence.armed(&shipped), None, "dormant on the shipped preset");
            shipped.palw_rfc0016_value_and_arm_gate_v1(fence, &values, &[], &[]).unwrap();
            let mut p = palw_t12_shipped_params();
            match fence {
                PalwRfc0016GatedFenceV1::ModelBlockRightsV1 => p.palw_model_block_rights_v1 = Some(ForkActivation::new(16_000)),
                PalwRfc0016GatedFenceV1::ElasticFinalCapacityV1 => p.palw_elastic_final_capacity_v1 = Some(ForkActivation::new(16_000)),
            }
            assert_eq!(fence.armed(&p), Some(16_000));
            let record = PalwRfc0016ArmingRecordV1 {
                version: PALW_RFC0016_ARMING_RECORD_VERSION_V1,
                fence,
                genesis: p.genesis.hash,
                activation_daa: 16_000,
                values,
                source_commit: [9; 20],
                evidence: &EVIDENCE,
                review: [10; 32],
            };
            let gate = |measured: &[PalwRfc0016MeasuredRecordV1], arming: &[PalwRfc0016ArmingRecordV1]| {
                p.palw_rfc0016_value_and_arm_gate_v1(fence, &values, measured, arming).map_err(|e| format!("{e:?}"))
            };
            // The measured-value gate stays first.
            assert!(gate(&[], &[record]).unwrap_err().contains("measured-value record"));
            // With the binary's (empty) list: refused by name.
            let why = gate(&[measured_record], PALW_RFC0016_ARMING_RECORDS_V1).unwrap_err();
            assert!(why.contains(fence.name()) && why.contains("PALW_RFC0016_ARMING_RECORDS_V1"), "{why}");
            assert!(gate(&[measured_record], &[record]).is_ok(), "{}: an exact well-formed record admits", fence.name());
            let other = |f: &dyn Fn(&mut PalwRfc0016ArmingRecordV1)| {
                let mut r = record;
                f(&mut r);
                gate(&[measured_record], &[r])
            };
            assert!(other(&|r| r.activation_daa = 16_001).unwrap_err().contains("no RFC-0016"), "another height");
            assert!(other(&|r| r.genesis = crate::Hash64::from_u64_word(1)).unwrap_err().contains("no RFC-0016"), "another network");
            assert!(other(&|r| r.values.sigma_permille += 1).unwrap_err().contains("another value set"));
            assert!(other(&|r| r.version = 2).is_err());
            assert!(other(&|r| r.source_commit = [0; 20]).is_err());
            assert!(other(&|r| r.review = [0; 32]).is_err());
            assert!(other(&|r| r.review = [1; 32]).is_err(), "review equal to an evidence digest");
            assert!(other(&|r| r.evidence = &EVIDENCE[..5]).is_err(), "one digest short");
            assert!(other(&|r| r.evidence = &DUPLICATED).is_err(), "repeated digest");
            let other_fence = PalwRfc0016GatedFenceV1::ALL.into_iter().find(|f| *f != fence).unwrap();
            assert!(other(&|r| r.fence = other_fence).is_err(), "a record for the other fence does not admit this one");
        }
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

    /// V5 (user decision 2026-10-15): one calibration epoch is 720 DAA and the average window 5,040 DAA (7 epochs, 10-16) — the same DAA counts the
    /// ADR-0177 allocation's decided policy uses (one day-equivalent epoch, 7 of them per average).
    #[test]
    fn v5_one_epoch_is_720_daa_and_the_window_5_040() {
        assert_eq!(PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1, 720);
        assert_eq!(PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1, 5_040);
        let decided = crate::palw_bond_budget_v1::PalwModelAllocationPolicyV1::decided_unapproved_v3();
        assert_eq!(decided.epoch_daa, PALW_RFC0016_EPOCH_CALIBRATION_DAA_V1);
        assert_eq!(decided.average.unwrap().span_daa(), PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1);
        assert_eq!(PALW_RFC0016_CALIBRATION_START_V1.entrant_age_daa, PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1);
    }

    /// V11 (user decision 2026-10-15): no coinbase Panel migration — the coinbase-derived origins stay on the Panel (Panel=1) route;
    /// only the user-paid kernel route is Panel=0, and it carries no coinbase leg.
    #[test]
    fn v11_the_coinbase_route_keeps_its_panel() {
        use crate::palw_bond_budget_v1::{PalwBudgetOriginV1 as O, PalwHoldModeV1, palw_budget_origin_is_user_escrow_v1};
        assert!(!PALW_RFC0016_COINBASE_PANEL_MIGRATION_V1);
        for coinbase in [O::Attempt, O::Rider, O::FreePrompt] {
            assert_eq!(PalwHoldModeV1::for_origin(coinbase), PalwHoldModeV1::PanelOne, "{coinbase:?} stays on the Panel route");
            assert!(!palw_budget_origin_is_user_escrow_v1(coinbase), "{coinbase:?} is coinbase-derived");
        }
        assert_eq!(PalwHoldModeV1::for_origin(O::KernelRoute), PalwHoldModeV1::PanelZero);
        assert!(palw_budget_origin_is_user_escrow_v1(O::KernelRoute), "Panel=0 is the user-paid route, no coinbase leg");
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
