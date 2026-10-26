//! **ADR-0176 amendment — ユーザー判断 2026-10-15: bond式の再設計** — fence `palw_bond_formula_v2` (**DORMANT**: `None` on every
//! preset, Some-only in both fingerprints, collapsed from `Some(never())`, refused when armed). The formula live on testnet-12 is
//! unchanged: below the fence (keyed on each claim's acceptance DAA, A-2 uniform) every reader answers exactly as before.
//!
//! **What the user verified (pre `4fb381a87`, the 2026-10-10 audit JSON; price queries on a synthetic bond, not live measurement).**
//! Admission is `L_existing + L_new ≤ u·C`, `u = 0.5`. An own-work Attempt reserves `L = L_weight + m_c`,
//! `L_weight = ⌈PWU_exposure · S · A / ρ⌉`, `m_c` the escrow term (not multiplied by `A`): at audit DAA 10,000 a Qwen A16 8k claim is
//! `0.02471601 + 3,200.84650080 = 3,200.87121681` BILI (≈ 99.9992 % escrow), A16 2M `59.74294207 + 3,200.84650080`. The escrow
//! discount `⌈E/ρ⌉` applies to the Floor class only (`palw_capacity_class_creditable_v1`, deliberately: model-class audit capacity is
//! unmeasured), so `ρ` does not shrink a model claim. A licence releases the escrow reservation and the issuance slot BEFORE `Final`,
//! so a colluding Valid rotates the same slots. Raising `u` to 100 % turns 2 claims into 4 on 13,000 BILI; raising `ρ` shrinks only
//! the 0.0247 term. (The back-calculations in the decision doc — ≈192,052 / ≈1,344,366 BILI, ≤ ~216.67 / ~30.95 BILI per claim — are
//! NOT proposed values.)
//!
//! **The new formula (past the fence, per claim of type `t`, mode `M`, bond capital `C`, budget policy `P` (V3), formula policy `Φ`):**
//!
//! ```text
//! (a) concurrent collateral   Σ_open L + L_existing_locks + accusation_reserves + outstanding_liability + L_new ≤ u · C,
//!                             u = Φ.concurrent_ratio_permille / 1000 (PROVISIONALLY 500, the existing V2 exposure ceiling's ratio)
//! (b) period accounting       Q/B/R/F over the common hold W_M (Panel=1 1-day, Panel=0 7-day; per-day equalized) — the ADR-0176
//!                             budget engine's long / day / burst views (palw_bond_budget_v1::hold)
//! (c) capacity recovery       earliest_recovery = d + W_M, FIXED AT ACCEPTANCE; no licence, Valid, Final, void or court result
//!                             brings it forward; collateral release is the liability clock's, separately
//!
//! per-claim rights (D2)       r_c = ⌊r_day / (q_day · ρ)⌋, f_c = ⌊f_day / (q_day · ρ)⌋        (the period total split over its claims)
//!                             R_c = min(R_ask, r_c) if the reduction is allowed, else R_ask    (allowed: Floor class or a pinned
//!                             F_c = min(F_ask, f_c)                                             measured audit-capacity record)
//! decomposition               L_weight  = max(⌈f_c · v_F⌉, ⌈PWU·S·A · λ_pwu / 1000⌉)           — NEVER divided by ρ
//!                             L_right   = R_c                (0 for kernel / native claims: the user's escrow is not producer collateral)
//!                             gain      = R_c + ⌈f_c · v_F⌉                                     (the claim's maximum attributable rights)
//!                             L_coll    = max(loss_floor_t, ⌈gain · κ_t / 1000⌉)                (ρ-independent floor; κ_t ≥ 1000)
//!                             L_penalty = L_coll − (L_right + ⌈f_c·v_F⌉)  if positive, else 0   (all of L_coll for kernel / native)
//!                             L_other   = court_cost_t + da_retention_cost_t
//!                             L         = L_weight + L_right + L_penalty + L_other ≥ L_coll ≥ gain
//! component holds             weight:  until the claim's Final weight leaves its fraud window (terminal + H_L)
//!                             right:   until the reward is paid or forfeited and its liability ends (terminal + H_L)
//!                             penalty: with the right
//!                             other:   until max(d + retention_hold_t, every court/DA session of the claim closed)
//!                             capacity: d + W_M (fixed, independent of all four)
//! ```
//!
//! On the V2 Attempt ledger the decomposition is read through the existing ONE-expression sites: the weight part is `claim.reserved`
//! (`palw_claim_weight_reservation_of_v1`), the rest is the escrow term (`PalwStateParamsV2::claim_escrow_term_v2`), so admission,
//! the fold's reserve/release and the load re-derivation keep agreeing; a licence no longer flips `escrow_released`
//! (`palw_rcore_supplementary_flips_v1`) and an issuance slot is held until `d + W_M` (`palw_issuance_holds_slot_v1`).
//!
//! **Every value is POLICY and UNAPPROVED** (`PalwBondFormulaPolicyV2::unapproved_probe_v2` is not a proposal). The values come from
//! the measurement procedure (`docs/design/palw/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md`).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_bond_budget_v1::{
    PALW_BOND_BUDGET_POLICY_VERSION_V3, PalwBondBudgetPolicyV1, PalwBudgetVectorV1, PalwHoldModeV1, palw_bond_budget_claim_ceilings_v1,
};
use crate::palw_mode_v2::PalwModeV2Error;

/// The formula policy's version.
pub const PALW_BOND_FORMULA_POLICY_VERSION_V2: u16 = 2;

/// **PROVISIONAL (user, 2026-10-15): the concurrent-collateral ratio `u`**, permille — the existing V2 exposure ceiling's 500 ‰ kept as
/// the starting point ("do not tune it first").
pub const PALW_BOND_FORMULA_CONCURRENT_RATIO_PROVISIONAL_PERMILLE_V2: u16 = 500;

/// **INTERIM, UNAPPROVED: `λ_pwu`**, permille of the UNSCALED PWU slash exposure (`PWU·S·A`, never divided by ρ) kept as the weight
/// part's floor. 1000 = the class's whole slash value per PWU.
pub const PALW_BOND_FORMULA_PWU_LOSS_INTERIM_PERMILLE_V2: u32 = 1_000;

/// **INTERIM, UNAPPROVED: `κ`**, permille — the collectable loss is at least the claim's maximum attributable rights times this
/// (1000 = the gain itself; a deterrence multiple above it is ECON's).
pub const PALW_BOND_FORMULA_LOSS_MULTIPLIER_INTERIM_PERMILLE_V2: u32 = 1_000;

/// **The measured audit-capacity records this binary pins** (item 5): a model class may take the reduced (rights-split) reservation
/// only with a record here. EMPTY — no model class's verification/audit capacity has been measured, so only the Floor class reduces.
pub const PALW_BOND_FORMULA_MEASURED_CAPACITY_PINNED_V2: &[PalwMeasuredAuditCapacityRecordV2] = &[];

/// **A measured verification/audit capacity record** of one class (item 5): the class, the sustained audits per day-equivalent the
/// measurement supported, and the evidence and independent-review digests. A pinned record is the only door to the reduced
/// reservation for a model class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwMeasuredAuditCapacityRecordV2 {
    pub class_id: [u8; 64],
    pub audits_per_day: u64,
    pub evidence_digest: [u8; 64],
    pub review_digest: [u8; 64],
}

/// **Is the reduced (rights-split) reservation allowed for a class?** The Floor class (its audit is the CPU replay every operator
/// runs), or a class with a pinned measured-capacity record with positive capacity.
pub fn palw_bond_formula_reduction_allowed_v2(class_id: &Hash64, base_class: &Hash64) -> bool {
    class_id == base_class
        || PALW_BOND_FORMULA_MEASURED_CAPACITY_PINNED_V2.iter().any(|r| {
            r.audits_per_day > 0
                && class_id.as_byte_slice() == &r.class_id[..]
                && r.evidence_digest != [0; 64]
                && r.review_digest != [0; 64]
        })
}

/// **The pricing type of a budget row's origin** (the close path's reading: which type's court/DA/retention span holds its liability).
pub fn palw_claim_type_of_origin_v2(origin: crate::palw_bond_budget_v1::PalwBudgetOriginV1) -> PalwClaimTypeV2 {
    use crate::palw_bond_budget_v1::PalwBudgetOriginV1 as O;
    match origin {
        O::FreePrompt => PalwClaimTypeV2::FreePrompt,
        O::KernelRoute => PalwClaimTypeV2::KernelPanelZero,
        _ => PalwClaimTypeV2::Attempt,
    }
}

/// **The claim types priced separately** (item 6: the ~3,200 BILI own-work Attempt quote is not a common price).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwClaimTypeV2 {
    /// Own-work V2 Attempt (and riders).
    Attempt = 0,
    /// Free Prompt commitment.
    FreePrompt = 1,
    /// Kernel-route claim licensed by a Panel (Panel=1).
    KernelPanelOne = 2,
    /// Kernel-route OPV claim (Panel=0).
    KernelPanelZero = 3,
    /// Native funded (Panel-free) work.
    NativeFunded = 4,
}

impl PalwClaimTypeV2 {
    /// The verification mode (and so the common hold) of the type.
    pub fn mode(self) -> PalwHoldModeV1 {
        match self {
            Self::Attempt | Self::FreePrompt | Self::KernelPanelOne => PalwHoldModeV1::PanelOne,
            Self::KernelPanelZero | Self::NativeFunded => PalwHoldModeV1::PanelZero,
        }
    }

    /// Whether the reward right sits in a user-paid escrow rather than the producer's bond (kernel / native): the producer's liability
    /// is then reserved in full beside it (item 6: user prepayment does NOT remove the producer's liability reservation).
    pub fn reward_in_user_escrow(self) -> bool {
        matches!(self, Self::KernelPanelOne | Self::KernelPanelZero | Self::NativeFunded)
    }
}

/// **One claim type's pricing** — POLICY, unapproved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimTypePricingV2 {
    /// The collectable-loss floor, base units (ρ-independent).
    pub loss_floor_sompi: u64,
    /// `κ_t`, permille (≥ 1000).
    pub loss_multiplier_permille: u32,
    /// The court reserve (does not scale with ρ).
    pub court_cost_sompi: u64,
    /// The DA / data-retention reserve (does not scale with ρ).
    pub da_retention_cost_sompi: u64,
    /// How long after acceptance the court/DA/retention part is held at least, DAA.
    pub retention_hold_daa: u64,
}

impl PalwClaimTypePricingV2 {
    pub const fn interim_unapproved_v2() -> Self {
        Self {
            loss_floor_sompi: 1,
            loss_multiplier_permille: PALW_BOND_FORMULA_LOSS_MULTIPLIER_INTERIM_PERMILLE_V2,
            court_cost_sompi: 1,
            da_retention_cost_sompi: 1,
            retention_hold_daa: 1,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.loss_floor_sompi == 0 {
            return Err("a claim type's collectable-loss floor must be positive");
        }
        if self.loss_multiplier_permille < 1_000 {
            return Err("the collectable loss is at least the claim's maximum attributable rights (kappa >= 1000 permille)");
        }
        if self.retention_hold_daa == 0 {
            return Err("the court/DA/retention part must be held for a positive span");
        }
        Ok(())
    }
}

/// **`palw_bond_formula_v2`'s policy** — every value POLICY, none approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBondFormulaPolicyV2 {
    pub version: u16,
    /// `u`, permille (provisionally 500).
    pub concurrent_ratio_permille: u16,
    /// `v_F`: base units per unit of Final weight (the weight right's value).
    pub weight_value_sompi_per_weight: u64,
    /// `λ_pwu`, permille of the unscaled PWU slash exposure.
    pub pwu_loss_permille: u32,
    pub attempt: PalwClaimTypePricingV2,
    pub free_prompt: PalwClaimTypePricingV2,
    pub kernel_panel_one: PalwClaimTypePricingV2,
    pub kernel_panel_zero: PalwClaimTypePricingV2,
    pub native_funded: PalwClaimTypePricingV2,
}

impl PalwBondFormulaPolicyV2 {
    /// **NOT A PROPOSAL**: the provisional `u`, the interim multipliers, every price one base unit.
    pub const fn unapproved_probe_v2() -> Self {
        Self {
            version: PALW_BOND_FORMULA_POLICY_VERSION_V2,
            concurrent_ratio_permille: PALW_BOND_FORMULA_CONCURRENT_RATIO_PROVISIONAL_PERMILLE_V2,
            weight_value_sompi_per_weight: 1,
            pwu_loss_permille: PALW_BOND_FORMULA_PWU_LOSS_INTERIM_PERMILLE_V2,
            attempt: PalwClaimTypePricingV2::interim_unapproved_v2(),
            free_prompt: PalwClaimTypePricingV2::interim_unapproved_v2(),
            kernel_panel_one: PalwClaimTypePricingV2::interim_unapproved_v2(),
            kernel_panel_zero: PalwClaimTypePricingV2::interim_unapproved_v2(),
            native_funded: PalwClaimTypePricingV2::interim_unapproved_v2(),
        }
    }

    pub fn pricing(&self, t: PalwClaimTypeV2) -> &PalwClaimTypePricingV2 {
        match t {
            PalwClaimTypeV2::Attempt => &self.attempt,
            PalwClaimTypeV2::FreePrompt => &self.free_prompt,
            PalwClaimTypeV2::KernelPanelOne => &self.kernel_panel_one,
            PalwClaimTypeV2::KernelPanelZero => &self.kernel_panel_zero,
            PalwClaimTypeV2::NativeFunded => &self.native_funded,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PALW_BOND_FORMULA_POLICY_VERSION_V2 {
            return Err("the bond formula policy's version is not one this binary reads");
        }
        if self.concurrent_ratio_permille == 0 || self.concurrent_ratio_permille > 1_000 {
            return Err("u is a positive permille of capital, at most 1000");
        }
        if self.weight_value_sompi_per_weight == 0 {
            return Err("the weight right's value v_F must be positive");
        }
        if self.pwu_loss_permille == 0 {
            return Err("lambda_pwu must be positive (the weight part never drops the PWU slash exposure)");
        }
        for t in [
            PalwClaimTypeV2::Attempt,
            PalwClaimTypeV2::FreePrompt,
            PalwClaimTypeV2::KernelPanelOne,
            PalwClaimTypeV2::KernelPanelZero,
            PalwClaimTypeV2::NativeFunded,
        ] {
            self.pricing(t).validate()?;
        }
        Ok(())
    }

    pub fn digest(&self) -> Hash64 {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/bond-formula/policy/v2").to_state();
        s.update(&borsh::to_vec(self).expect("a policy serializes"));
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

/// **`Params::palw_bond_formula_v2`'s value** — Some-only everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBondFormulaFenceV2 {
    pub activation: ForkActivation,
    pub policy: PalwBondFormulaPolicyV2,
}

impl PalwBondFormulaFenceV2 {
    /// A bare height with the unapproved probe policy (what a fence probe builds from a height alone).
    pub fn at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwBondFormulaPolicyV2::unapproved_probe_v2() }
    }

    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(borsh::to_vec(&self.policy).expect("the policy serializes"));
    }
}

/// **The fence's mirror on the V2 bundle's state params** (not Borsh, not hashed — the fence is what the ids name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBondFormulaMirrorV2 {
    pub from_daa: u64,
    pub policy: PalwBondFormulaPolicyV2,
}

/// **How long one component of a claim's reservation is held** (item 3) — the rule fixed at acceptance and its earliest end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwComponentHoldV2 {
    /// Until the claim is terminal (Final / void / conviction) and then `liability_hold_daa` more (its fraud window).
    TerminalPlusLiability { liability_hold_daa: u64 } = 0,
    /// Until `max(accepted + retention_hold, every court / DA session of the claim closed)`.
    RetentionAndCourtClosed { not_before_daa: u64 } = 1,
}

/// **A claim's reservation, decomposed** (item 3) — what RPC / audit output shows beside the total.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReservationSplitV2 {
    pub claim_type: PalwClaimTypeV2,
    pub mode: PalwHoldModeV1,
    /// The claim's maximum attributable rights (Q = 1, B, R_c, F_c).
    pub rights: PalwBudgetVectorV1,
    /// Whether the rights-split reduction was allowed (Floor class or a measured-capacity record).
    pub reduced: bool,
    pub weight_sompi: u128,
    pub right_sompi: u128,
    pub penalty_sompi: u128,
    pub other_sompi: u128,
    /// `L_coll` — what must be collectable on conviction.
    pub collectable_loss_sompi: u128,
    /// The claim's maximum attributable gain (`R_c + ⌈f_c·v_F⌉`).
    pub gain_sompi: u128,
    pub weight_hold: PalwComponentHoldV2,
    pub right_hold: PalwComponentHoldV2,
    pub other_hold: PalwComponentHoldV2,
    /// `d + W_M` — the earliest capacity recovery, fixed at acceptance.
    pub capacity_recovery_daa: u64,
}

impl PalwReservationSplitV2 {
    /// `L = L_weight + L_right + L_penalty + L_other`.
    pub fn total(&self) -> u128 {
        self.weight_sompi.saturating_add(self.right_sompi).saturating_add(self.penalty_sompi).saturating_add(self.other_sompi)
    }

    /// The escrow term the V2 ledger reads beside `claim.reserved`: everything but the weight part.
    pub fn escrow_term(&self) -> u128 {
        self.right_sompi.saturating_add(self.penalty_sompi).saturating_add(self.other_sompi)
    }
}

/// The inputs of one claim's pricing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBondFormulaInputV2 {
    pub claim_type: PalwClaimTypeV2,
    pub accepted_daa: u64,
    /// The reward the claim asks (its escrowed reward; a kernel claim's job escrow).
    pub reward_ask_sompi: u64,
    /// The Final weight the claim asks.
    pub final_weight_ask: u128,
    /// Reward-block units the claim asks.
    pub block_units_ask: u64,
    /// `PWU · S · A`, UNSCALED (never divided by ρ).
    pub pwu_exposure_sompi: u128,
    /// Whether the class may take the reduced reservation ([`palw_bond_formula_reduction_allowed_v2`]).
    pub reduction_allowed: bool,
}

fn ceil_mul_div(a: u128, b: u128, d: u128) -> u128 {
    a.checked_mul(b).map(|p| p.div_ceil(d.max(1))).unwrap_or(u128::MAX)
}

/// **The per-claim right ceilings** `(r_c, f_c)` of the budget policy (the period total split over its claims, D2). A V3 budget policy
/// with `slice_rights_by_rho` is required by validation; `None` otherwise.
pub fn palw_bond_formula_right_ceilings_v2(budget: &PalwBondBudgetPolicyV1) -> Option<(u64, u128)> {
    palw_bond_budget_claim_ceilings_v1(budget)
}

/// **The weight part** `L_weight = max(⌈f_c · v_F⌉, ⌈PWU·S·A · λ_pwu / 1000⌉)` — never divided by ρ. The value the V2 ledger records as
/// `claim.reserved` past the fence.
pub fn palw_bond_formula_weight_part_v2(
    policy: &PalwBondFormulaPolicyV2,
    budget: &PalwBondBudgetPolicyV1,
    pwu_exposure: u128,
) -> u128 {
    let f_c = palw_bond_formula_right_ceilings_v2(budget).map_or(0, |(_, f)| f);
    let value = f_c.saturating_mul(policy.weight_value_sompi_per_weight as u128);
    value.max(ceil_mul_div(pwu_exposure, policy.pwu_loss_permille as u128, 1_000))
}

/// **The decomposition of one claim's reservation** (the formula in the module doc). Pure and deterministic; `Err` where the budget
/// policy is not a rights-slicing V3 policy.
pub fn palw_bond_formula_split_v2(
    policy: &PalwBondFormulaPolicyV2,
    budget: &PalwBondBudgetPolicyV1,
    input: &PalwBondFormulaInputV2,
) -> Result<PalwReservationSplitV2, &'static str> {
    let hold =
        budget.hold.as_ref().filter(|_| budget.version == PALW_BOND_BUDGET_POLICY_VERSION_V3).ok_or("needs a V3 budget policy")?;
    let (r_c, f_c) = palw_bond_formula_right_ceilings_v2(budget).ok_or("needs a budget policy that slices rights by rho (D2)")?;
    let t = input.claim_type;
    let pricing = policy.pricing(t);
    let mode = t.mode();
    // Rights: the period total split over its claims. The weight right is always split; the reward right only where the reduction
    // is allowed (Floor class or a measured audit capacity) — elsewhere the claim keeps its whole reward right and reserves for it.
    let reward = if input.reduction_allowed { input.reward_ask_sompi.min(r_c) } else { input.reward_ask_sompi };
    let weight = input.final_weight_ask.min(f_c);
    let rights = PalwBudgetVectorV1 {
        claims: 1,
        block_units: input.block_units_ask,
        reward_sompi: reward,
        final_weight: weight,
        round_rights: 0,
    };
    let weight_value = f_c.saturating_mul(policy.weight_value_sompi_per_weight as u128);
    let weight_part = palw_bond_formula_weight_part_v2(policy, budget, input.pwu_exposure_sompi);
    let right_part = if t.reward_in_user_escrow() { 0 } else { reward as u128 };
    let gain = (reward as u128).saturating_add(weight_value);
    let collectable = (pricing.loss_floor_sompi as u128).max(ceil_mul_div(gain, pricing.loss_multiplier_permille as u128, 1_000));
    let penalty =
        if t.reward_in_user_escrow() { collectable } else { collectable.saturating_sub(right_part.saturating_add(weight_value)) };
    let other = (pricing.court_cost_sompi as u128).saturating_add(pricing.da_retention_cost_sompi as u128);
    let terminal = PalwComponentHoldV2::TerminalPlusLiability { liability_hold_daa: budget.liability_hold_daa };
    Ok(PalwReservationSplitV2 {
        claim_type: t,
        mode,
        rights,
        reduced: input.reduction_allowed,
        weight_sompi: weight_part,
        right_sompi: right_part,
        penalty_sompi: penalty,
        other_sompi: other,
        collectable_loss_sompi: collectable,
        gain_sompi: gain,
        weight_hold: terminal,
        right_hold: terminal,
        other_hold: PalwComponentHoldV2::RetentionAndCourtClosed {
            not_before_daa: input.accepted_daa.saturating_add(pricing.retention_hold_daa),
        },
        capacity_recovery_daa: input.accepted_daa.saturating_add(hold.window_daa(mode)),
    })
}

/// **(a) The concurrent-collateral check**: `open + existing_locks + accusation_reserves + outstanding + new ≤ u · C`. Every term is
/// actual collateral already reserved; nothing is fictitious or counted twice (the caller passes each ledger once).
pub fn palw_bond_formula_concurrent_admits_v2(
    policy: &PalwBondFormulaPolicyV2,
    capital: u64,
    open_reservations: u128,
    existing_locks: u128,
    accusation_reserves: u128,
    outstanding_liability: u128,
    new_reservation: u128,
) -> bool {
    let room = (capital as u128).saturating_mul(policy.concurrent_ratio_permille as u128) / 1_000;
    open_reservations
        .checked_add(existing_locks)
        .and_then(|v| v.checked_add(accusation_reserves))
        .and_then(|v| v.checked_add(outstanding_liability))
        .and_then(|v| v.checked_add(new_reservation))
        .is_some_and(|total| total <= room)
}

/// **Item 6 — a Panel=0 / kernel claim's three separate accounts**: the user-paid escrow (no coinbase leg, never clipped by a model
/// budget), the producer's liability collateral (the formula's total — the user's prepayment never removes it) and the producer's
/// period `R` (the claim's reward right against the bond's period cap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwKernelAccountingV2 {
    pub user_escrow_sompi: u64,
    pub producer_liability_sompi: u128,
    pub producer_period_reward_sompi: u64,
}

pub fn palw_kernel_accounting_v2(split: &PalwReservationSplitV2, user_escrow_sompi: u64) -> PalwKernelAccountingV2 {
    PalwKernelAccountingV2 {
        user_escrow_sompi,
        producer_liability_sompi: split.total(),
        producer_period_reward_sompi: split.rights.reward_sompi,
    }
}

// ---- the V2 ledger's readings (the ONE-expression hooks) -------------------------------------------------------------------------

impl crate::palw_state_v2::PalwStateParamsV2 {
    /// **Does `palw_bond_formula_v2` price a claim accepted at `accepted_daa`?** `false` on every shipped preset; keyed on acceptance,
    /// so a claim accepted below the fence completes under the formula it was accepted under.
    pub fn bond_formula_v2_applies_at(&self, accepted_daa: u64) -> bool {
        self.bond_formula_v2().is_some_and(|m| accepted_daa >= m.from_daa)
            && self.bond_budget().is_some_and(|b| b.policy.version == PALW_BOND_BUDGET_POLICY_VERSION_V3)
    }

    /// The formula's weight part for a claim accepted at `accepted_daa` (`None` below the fence: the caller's old rule).
    pub fn bond_formula_weight_part_v2(&self, accepted_daa: u64, pwu_exposure: u128) -> Option<u128> {
        if !self.bond_formula_v2_applies_at(accepted_daa) {
            return None;
        }
        let formula = self.bond_formula_v2()?;
        Some(palw_bond_formula_weight_part_v2(&formula.policy, &self.bond_budget()?.policy, pwu_exposure))
    }

    /// The formula's escrow term (right + penalty + other) for a V2 claim of `class_id` accepted at `accepted_daa` with
    /// `escrowed_reward` (`None` below the fence). A claim with no escrow is a Free Prompt commitment.
    pub fn bond_formula_escrow_term_v2(&self, accepted_daa: u64, escrowed_reward: u64, class_id: &Hash64) -> Option<u128> {
        if !self.bond_formula_v2_applies_at(accepted_daa) {
            return None;
        }
        let formula = self.bond_formula_v2()?;
        let claim_type = if escrowed_reward > 0 { PalwClaimTypeV2::Attempt } else { PalwClaimTypeV2::FreePrompt };
        let input = PalwBondFormulaInputV2 {
            claim_type,
            accepted_daa,
            reward_ask_sompi: escrowed_reward,
            final_weight_ask: u128::MAX,
            block_units_ask: 0,
            pwu_exposure_sompi: 0,
            reduction_allowed: palw_bond_formula_reduction_allowed_v2(class_id, &self.base_class_id()),
        };
        palw_bond_formula_split_v2(&formula.policy, &self.bond_budget()?.policy, &input).ok().map(|s| s.escrow_term())
    }

    /// The common hold `W_M` of a claim of `mode` past the fence (`None` below it).
    pub fn bond_formula_recovery_daa_v2(&self, accepted_daa: u64, mode: PalwHoldModeV1) -> Option<u64> {
        if !self.bond_formula_v2_applies_at(accepted_daa) {
            return None;
        }
        let hold = self.bond_budget()?.policy.hold?;
        Some(accepted_daa.saturating_add(hold.window_daa(mode)))
    }
}

// ---- the fence on `Params` ------------------------------------------------------------------------------------------------------

impl Params {
    /// Whether the formula is in force at `daa` — never, in this binary.
    pub fn palw_bond_formula_v2_active_at(&self, daa: u64) -> bool {
        self.palw_bond_formula_v2.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
    }

    /// **The fence's refusal**: the policy checked first, then a V3, rights-slicing `palw_bond_budget_v1` required at or below it (the
    /// period accounting (b) and the fixed recovery (c) are the budget's), the budget's concurrent-liability permille no looser than
    /// `u`, then any armed height refused — every value is POLICY the user has not set and the measurement does not exist.
    pub fn validate_palw_bond_formula_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_bond_formula_v2.filter(|f| f.activation != ForkActivation::never()) else { return Ok(()) };
        if fence.policy.validate().is_err() {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_formula_v2's policy is invalid (PalwBondFormulaPolicyV2::validate names the rule that fails)",
            ));
        }
        let budget_ok = self.palw_bond_budget_v1.as_ref().is_some_and(|b| {
            b.activation != ForkActivation::never()
                && b.activation.daa_score() <= fence.activation.daa_score()
                && b.policy.version == PALW_BOND_BUDGET_POLICY_VERSION_V3
                && b.policy.slice_rights_by_rho
                && b.policy.hold.is_some_and(|h| h.concurrent_liability_permille <= fence.policy.concurrent_ratio_permille.max(1))
        });
        if !budget_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_formula_v2 requires a V3, rights-slicing palw_bond_budget_v1 at or below it whose concurrent cap is within u",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_bond_formula_v2 cannot be armed: u, v_F, lambda_pwu, kappa, the per-type prices and the per-day rates are POLICY the user has not set, and the measurement (ADR-0176 plan) does not exist",
        ))
    }

    /// The fence's mirror on the V2 bundle's state params (fixtures that bypass validation call it after arming).
    pub fn sync_palw_bond_formula_v2(&mut self) {
        let mirror = self
            .palw_bond_formula_v2
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| PalwBondFormulaMirrorV2 { from_daa: f.activation.daa_score(), policy: f.policy });
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_bond_formula_v2(mirror);
        }
    }
}

// ---- audit / RPC output (item 3) ------------------------------------------------------------------------------------------------

impl PalwComponentHoldV2 {
    fn audit_json(&self) -> String {
        match self {
            Self::TerminalPlusLiability { liability_hold_daa } => {
                format!("{{\"until\":\"terminal+liability\",\"liabilityHoldDaa\":{liability_hold_daa}}}")
            }
            Self::RetentionAndCourtClosed { not_before_daa } => {
                format!("{{\"until\":\"retention-and-courts-closed\",\"notBeforeDaa\":{not_before_daa}}}")
            }
        }
    }
}

impl PalwReservationSplitV2 {
    /// **The decomposition as one JSON object** (RPC `getPalwClaims.reservationSplit`, audit output): every amount a decimal string
    /// of base units, each component's hold, the claim's rights and the fixed earliest capacity recovery — never only the total.
    pub fn audit_json(&self) -> String {
        format!(
            "{{\"type\":\"{:?}\",\"mode\":\"{:?}\",\"reduced\":{},\"weight\":\"{}\",\"right\":\"{}\",\"penalty\":\"{}\",\"other\":\"{}\",\"total\":\"{}\",\"collectableLoss\":\"{}\",\"gain\":\"{}\",\"weightHold\":{},\"rightHold\":{},\"penaltyHold\":{},\"otherHold\":{},\"capacityRecoveryDaa\":{},\"rights\":{{\"claims\":{},\"blockUnits\":{},\"reward\":\"{}\",\"finalWeight\":\"{}\"}}}}",
            self.claim_type,
            self.mode,
            self.reduced,
            self.weight_sompi,
            self.right_sompi,
            self.penalty_sompi,
            self.other_sompi,
            self.total(),
            self.collectable_loss_sompi,
            self.gain_sompi,
            self.weight_hold.audit_json(),
            self.right_hold.audit_json(),
            self.right_hold.audit_json(),
            self.other_hold.audit_json(),
            self.capacity_recovery_daa,
            self.rights.claims,
            self.rights.block_units,
            self.rights.reward_sompi,
            self.rights.final_weight,
        )
    }
}

/// **A V2 claim's decomposition as the ledger holds it** (the read behind RPC / audit output): `None` below the fence. The weight part
/// is the one the ledger recorded (`claim.reserved`), the economic right carries a free prompt's priced receipt rights, and the rest
/// is the formula's — so `total()` is exactly what `palw_claim_bond_reservation_v1` sums for the claim.
pub fn palw_bond_formula_claim_split_v2(
    params: &crate::palw_state_v2::PalwStateParamsV2,
    claim: &crate::palw_state_v2::PalwClaimStateV2,
) -> Option<PalwReservationSplitV2> {
    if !params.bond_formula_v2_applies_at(claim.accepted_daa) {
        return None;
    }
    let formula = params.bond_formula_v2()?;
    let budget = params.bond_budget()?;
    let free_prompt = !matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::Attempt);
    let input = PalwBondFormulaInputV2 {
        claim_type: if free_prompt || claim.escrowed_reward == 0 { PalwClaimTypeV2::FreePrompt } else { PalwClaimTypeV2::Attempt },
        accepted_daa: claim.accepted_daa,
        reward_ask_sompi: claim.escrowed_reward,
        final_weight_ask: u128::MAX,
        block_units_ask: if free_prompt { 0 } else { crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1 },
        pwu_exposure_sompi: 0,
        reduction_allowed: palw_bond_formula_reduction_allowed_v2(&claim.class_id, &params.base_class_id()),
    };
    let mut split = palw_bond_formula_split_v2(&formula.policy, &budget.policy, &input).ok()?;
    split.weight_sompi = claim.reserved;
    split.right_sompi = split.right_sompi.saturating_add(claim.rights_reserved);
    Some(split)
}
