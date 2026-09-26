//! **ADR-0160 — the reference formulas of claim-capacity separation** (lane shadow, day 0–1).
//!
//! ADR-0160 splits what one claim number carries today into four quantities — raw PWU `C` (kept),
//! the withheld reward `E`, the fork-choice weight `W`, and the pre-licence obligation `m` — and
//! re-prices the reservation, the weight, the aggregate liability and the seat capital from them.
//! This module is every one of those formulas, pure and integer-only, in one place:
//!
//! * §4.1 the reservation: [`palw_capacity_consensus_reservation_v1`] (`min(w_c, R_budget − held)`)
//!   and [`palw_capacity_weight_budget_sompi_v1`] (`R_budget`);
//! * §4.2 staged weight and the per-bond cap (J-1): [`PalwCapacityStageV1`],
//!   [`palw_capacity_stage_of_claim_v1`], [`palw_capacity_weight_full_v1`],
//!   [`palw_capacity_staged_weight_v1`], [`palw_capacity_weight_cap_v1`] (`W_cap`) and
//!   [`palw_capacity_bond_weight_term_v1`] (`min(X_b, W_cap)`);
//! * §4.5 the monetary pre-licence risk: [`palw_capacity_m_star_v1`] (`m*(q)`),
//!   [`palw_capacity_m_ramp_v1`] (`⌈E/ρ⌉`), [`palw_capacity_m_c_v1`] (`m_c`),
//!   [`palw_capacity_conviction_l_v1`] (`L = 3G`), [`palw_capacity_q_needed_permille_v1`] and the
//!   campaign expectation [`palw_capacity_campaign_ev_sompi_v1`] (`EV(K)`);
//! * §4.7 the seat side: [`palw_capacity_seat_duty_v1`] (AS-1 `duty′`),
//!   [`palw_capacity_seat_lock_v1`] (AS-2 `lock′`) and its credit test
//!   [`palw_capacity_seat_credit_applies_v1`] (`q ≥ q_seat`);
//! * §5 the numeric targets: [`palw_capacity_n_instant_v1`] (concurrent claims per bond),
//!   [`palw_capacity_per_80_daa_milli_v1`] (per-bond throughput), the §5.4 seat-capital estimate
//!   [`palw_capacity_seat_capital_per_claim_v1`] / [`palw_capacity_claims_per_daa_milli_v1`],
//!   licence carriage [`palw_capacity_carriers_per_block_v1`] and the 2M compute arithmetic
//!   [`palw_capacity_gpu_equivalents_milli_v1`].
//!
//! **Who reads it.** Lane shadow (`palw_capacity_shadow_v1`, node-only) today. The consensus lanes
//! (weight, escrow, liab, verify) call these functions behind their dormant fences once they merge.
//! **A formula is frozen once a consensus lane calls it; a change is a new `_v2` function** (ADR-0160
//! §7). Until then nothing in consensus calls this module: `palw_capacity_shadow_is_node_only`
//! scans `consensus/src` and `consensus/core/src` for call sites and allows only the shadow module,
//! the read API and tests — widened, by name, when a consensus lane starts calling a formula.
//!
//! **Arithmetic.** Sompi and weight units are `u128`, probabilities are permille (`u16`), and every
//! division states its rounding: ceilings where the ADR writes `⌈⌉` (the obligation and the ramp
//! term round against the producer), floors elsewhere. Nothing panics on any input: products
//! saturate, and a zero divisor is read as the degenerate case the doc names.

use crate::palw_state_v2::{PalwClaimPhaseV2, PalwClaimStateV2};

// ---------------------------------------------------------------------------------------------
// §6.1 constants. Not fence values: changing one is a new fence.
// ---------------------------------------------------------------------------------------------

/// **One floor-claim weight (FCW)** in raw fork-choice units: `β·pwu` of a genesis-target floor claim,
/// `⌊0.1 × 279 × 21,657,728⌋` (ADR-0160 Appendix A). The unit `W_cap` is counted in, so a bond's
/// provisional weight is class-neutral (§10 D-2).
pub const PALW_CAPACITY_FCW_V1: u128 = 604_250_611;

/// **The collateral that buys one FCW of provisional weight**: 6,500 MSK, the 500‰ ceiling of the
/// 13,000 MSK producer floor divided by today's 2 instant floor claims (§10 D-1: `⌊C/6,500⌋`).
pub const PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1: u64 = 650_000_000_000;

/// **The weight reservation of one FCW, in sompi** (`w_FCW`, §4.1): a floor claim's `w`, 0.1075266
/// MSK. `R_budget(b) = ⌊C_b / 6,500 MSK⌋ × w_FCW`.
pub const PALW_CAPACITY_W_FCW_SOMPI_V1: u128 = 10_752_660;

/// **Anchored stage**: a bound, not yet licensed claim weighs 10‰ of its full weight (§4.2).
pub const PALW_CAPACITY_ANCHORED_PERMILLE_V1: u16 = 10;

/// **S2 partial licence** (`basis_k = 1`): 250‰ of the full weight (§4.2).
pub const PALW_CAPACITY_S2_PERMILLE_V1: u16 = 250;

/// A counted licence (`basis_k ≥ 2`, the full-replay door): the whole weight.
pub const PALW_CAPACITY_FULL_PERMILLE_V1: u16 = 1_000;

/// **C7's Final weight ceiling** (§4.2, §10 D-3): the raw weight of the heaviest attributable class,
/// 8k — 138,892,697,241 units = 229.86 FCW — until ADR-0153 gives 2M a conviction route.
pub const PALW_CAPACITY_C7_WEIGHT_CEILING_V1: u128 = 138_892_697_241;

/// **P\***, the licence probability of one fraudulent claim the obligation must deter (§4.5): 500‰.
pub const PALW_CAPACITY_P_STAR_PERMILLE_V1: u16 = 500;

/// **L = 3·G**: what one producer conviction definitely collects is the action-tier cap, three times
/// the claim's fraud gain `G` (§4.5). Whole-bond forfeiture is margin on top.
pub const PALW_CAPACITY_L_MULTIPLE_OF_G_V1: u128 = 3;

/// **§5.4's duty hold**: DAA a seat's duty is held from bind to Final on the floor (licence latency
/// plus the 120-DAA challenge window, measured 122 in the capacity harness). An estimate input, not
/// a rule.
pub const PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1: u64 = 122;

/// **A five-receipt coverage licence carrier's transient mass** (capacity map §6.4: 23,951 B × 4
/// plus the carrier's own signature and key). With the 500,000 block limit, three fit a block.
pub const PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1: u64 = 125_768;

/// Sompi per MSK, for the reference tables.
pub const PALW_CAPACITY_SOMPI_PER_MSK_V1: u128 = 100_000_000;

// ---------------------------------------------------------------------------------------------
// The ramp step (F-L's value carries a schedule of these; lane liab owns the fence).
// ---------------------------------------------------------------------------------------------

/// **One step of the capacity ramp** (ADR-0160 §6.1, F-L's value `PalwCapacityLiabilityV1 { activation,
/// steps }`): from `from_daa` on, the ramp factor `rho` (the obligation's floor is `⌈E/ρ⌉`) and the
/// credited attribution rate `q_credit_permille` (§4.5, `≤ ½ ×` the measured rate, §10 D-8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCapacityStepV1 {
    pub from_daa: u64,
    /// The ramp factor. `0` is read as `1` everywhere (no division by zero, no reduction).
    pub rho: u32,
    pub q_credit_permille: u16,
}

impl PalwCapacityStepV1 {
    /// `rho` with `0` read as `1`.
    pub fn rho_or_one(&self) -> u128 {
        u128::from(self.rho.max(1))
    }
}

/// **The ramp the ADR's tables are written for** (§5.2, §9): ρ ∈ {10, 25, 50, 100, 1000} with
/// `q_credit = 143‰`, the smallest credit at which `m* = 0` for every ρ at `L = 3G` — so each row's
/// obligation is the ramp term `⌈E/ρ⌉` (the ADR's "attributable" column). `from_daa = 0`: a shadow
/// step, not a schedule. What the shadow reports when a caller names no steps.
pub const PALW_CAPACITY_REFERENCE_STEPS_V1: [PalwCapacityStepV1; 5] = [
    PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 143 },
    PalwCapacityStepV1 { from_daa: 0, rho: 25, q_credit_permille: 143 },
    PalwCapacityStepV1 { from_daa: 0, rho: 50, q_credit_permille: 143 },
    PalwCapacityStepV1 { from_daa: 0, rho: 100, q_credit_permille: 143 },
    PalwCapacityStepV1 { from_daa: 0, rho: 1000, q_credit_permille: 143 },
];

/// **The step in force at `daa`** in a schedule sorted by `from_daa`: the last whose `from_daa ≤ daa`,
/// `None` before the first (or for an empty schedule). A schedule that is not sorted is read in
/// the order given, which `validate_palw_v2` (lane liab) refuses at startup.
pub fn palw_capacity_step_at_v1(steps: &[PalwCapacityStepV1], daa: u64) -> Option<PalwCapacityStepV1> {
    steps.iter().take_while(|step| step.from_daa <= daa).last().copied()
}

// ---------------------------------------------------------------------------------------------
// §4.2 W — staged weight and the per-bond cap (J-1)
// ---------------------------------------------------------------------------------------------

/// **A claim's weight stage** (ADR-0160 §4.2's table). Lane weight's `PalwWeightStageV1` is this
/// table; the shadow reads it to report what each live claim would weigh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwCapacityStageV1 {
    /// `Provisional`: 0.
    Created,
    /// `PanelBound`, `DefaultDisputed`: `⌊W_full × 10‰⌋`.
    Anchored,
    /// `ReceiptLicensed`: `⌊W_full × permille‰⌋` — 1000 on a counted licence, 250 on S2 (a future
    /// sampled-coverage door writes its covered permille).
    Licensed { permille: u16 },
    /// `Final`: 0 in the immature set; `W_full` enters `safe_weight`.
    Final,
    /// `Voided` (and retired): 0.
    Terminal,
}

impl PalwCapacityStageV1 {
    /// A short name for logs and the RPC.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Anchored => "anchored",
            Self::Licensed { permille } if *permille >= PALW_CAPACITY_FULL_PERMILLE_V1 => "licensed",
            Self::Licensed { .. } => "licensed-partial",
            Self::Final => "final",
            Self::Terminal => "terminal",
        }
    }

    /// Does the stage count in the bond's provisional sum `X_b` (non-terminal, non-Final)?
    pub fn is_provisional(&self) -> bool {
        matches!(self, Self::Created | Self::Anchored | Self::Licensed { .. })
    }
}

/// **The stage a claim's phase puts it in** (§4.2). `ReceiptLicensed` is a counted licence
/// (`palw_rcore_counts_licensed_v1`: no door recorded, or `basis_k ≥ 2`) at 1000‰, else S2 at 250‰.
pub fn palw_capacity_stage_of_claim_v1(claim: &PalwClaimStateV2) -> PalwCapacityStageV1 {
    match &claim.phase {
        PalwClaimPhaseV2::Provisional => PalwCapacityStageV1::Created,
        PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. } => PalwCapacityStageV1::Anchored,
        PalwClaimPhaseV2::ReceiptLicensed { .. } => {
            if crate::palw_state_v2::palw_rcore_counts_licensed_v1(claim) {
                PalwCapacityStageV1::Licensed { permille: PALW_CAPACITY_FULL_PERMILLE_V1 }
            } else {
                PalwCapacityStageV1::Licensed { permille: PALW_CAPACITY_S2_PERMILLE_V1 }
            }
        }
        PalwClaimPhaseV2::Final { .. } => PalwCapacityStageV1::Final,
        PalwClaimPhaseV2::Voided { .. } => PalwCapacityStageV1::Terminal,
    }
}

/// **`W_full(c) = min(raw_c, ceiling_class(c))`** (§4.2): `raw_c` is the stored
/// `immature_contribution` (already D7-gated); the ceiling is [`PALW_CAPACITY_C7_WEIGHT_CEILING_V1`]
/// for a C7 class and unbounded otherwise.
pub fn palw_capacity_weight_full_v1(raw: u128, c7: bool) -> u128 {
    if c7 { raw.min(PALW_CAPACITY_C7_WEIGHT_CEILING_V1) } else { raw }
}

/// **`x_c`, the staged weight** (§4.2's table): floors, as `immature_contribution_v2` does.
pub fn palw_capacity_staged_weight_v1(stage: PalwCapacityStageV1, w_full: u128) -> u128 {
    let permille = match stage {
        PalwCapacityStageV1::Created | PalwCapacityStageV1::Final | PalwCapacityStageV1::Terminal => return 0,
        PalwCapacityStageV1::Anchored => PALW_CAPACITY_ANCHORED_PERMILLE_V1,
        PalwCapacityStageV1::Licensed { permille } => permille.min(PALW_CAPACITY_FULL_PERMILLE_V1),
    };
    w_full.saturating_mul(u128::from(permille)) / 1_000
}

/// How many FCW (and weight-reservation units) a bond's posted collateral buys: `⌊C / 6,500 MSK⌋`.
pub fn palw_capacity_fcw_units_v1(collateral_sompi: u64) -> u128 {
    u128::from(collateral_sompi / PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1)
}

/// **`W_cap(b) = ⌊C_b / 6,500 MSK⌋ × FCW`** (§4.2, J-1): 2 FCW at 13k, 15 at 100k, 144 for a
/// genesis card (939,063 MSK), 153 at 1M.
pub fn palw_capacity_weight_cap_v1(collateral_sompi: u64) -> u128 {
    palw_capacity_fcw_units_v1(collateral_sompi).saturating_mul(PALW_CAPACITY_FCW_V1)
}

/// **The bond's term in `bounded_immature`**: `min(X_b, W_cap(b))` (§4.2).
pub fn palw_capacity_bond_weight_term_v1(x_b: u128, w_cap: u128) -> u128 {
    x_b.min(w_cap)
}

// ---------------------------------------------------------------------------------------------
// §4.1 / §4.3 the reservation's consensus term
// ---------------------------------------------------------------------------------------------

/// **`R_budget(b) = ⌊C_b / 6,500 MSK⌋ × w_FCW`** (§4.1): the most weight reservation a bond's
/// new-rule claims hold at once — 0.215 MSK at 13k, 1.61 at 100k, 16.45 at 1M.
pub fn palw_capacity_weight_budget_sompi_v1(collateral_sompi: u64) -> u128 {
    palw_capacity_fcw_units_v1(collateral_sompi).saturating_mul(PALW_CAPACITY_W_FCW_SOMPI_V1)
}

/// **The consensus reservation** `effective_weight × price_per_weight = min(w_c, R_budget − held)`
/// (§4.1; price is the uniform `p0`, so the product is the stored `w_c` capped by the remaining
/// budget). `held` is `Σ reserved` of the bond's live new-rule claims before this one.
pub fn palw_capacity_consensus_reservation_v1(w_c: u128, budget: u128, held: u128) -> u128 {
    w_c.min(budget.saturating_sub(held))
}

// ---------------------------------------------------------------------------------------------
// §4.5 m — the monetary pre-licence risk
// ---------------------------------------------------------------------------------------------

/// **`m*(q) = max(0, ⌈(P*·E − q·L/(1−q)) / (1−P*)⌉)`** (§4.5) at `P* =`
/// [`PALW_CAPACITY_P_STAR_PERMILLE_V1`]: the obligation that makes one fraudulent claim's
/// expectation non-positive when a conviction (probability `q`) collects `L`. `m*(0) = E`; `q ≥
/// 1000‰` is certain conviction (`0`).
pub fn palw_capacity_m_star_v1(e_sompi: u128, q_permille: u16, l_sompi: u128) -> u128 {
    palw_capacity_m_star_at_v1(e_sompi, q_permille, l_sompi, PALW_CAPACITY_P_STAR_PERMILLE_V1)
}

/// [`palw_capacity_m_star_v1`] at an explicit `P*` (permille). `P* ≥ 1000‰` (a fraud that always
/// licenses) cannot be deterred by any finite obligation and returns `E` unchanged.
pub fn palw_capacity_m_star_at_v1(e_sompi: u128, q_permille: u16, l_sompi: u128, p_star_permille: u16) -> u128 {
    if q_permille >= 1_000 {
        return 0;
    }
    if p_star_permille >= 1_000 {
        return e_sompi;
    }
    let p = i128::from(p_star_permille);
    let q = i128::from(q_permille);
    let e = i128::try_from(e_sompi).unwrap_or(i128::MAX);
    let l = i128::try_from(l_sompi).unwrap_or(i128::MAX);
    // (p/1000)·E − (q/(1000−q))·L, over (1000−p)/1000:
    //   [p·E·(1000−q) − 1000·q·L] / [(1000−q)·(1000−p)].
    let gain = p.saturating_mul(e).saturating_mul(1_000 - q);
    let loss = 1_000i128.saturating_mul(q).saturating_mul(l);
    let num = gain.saturating_sub(loss);
    if num <= 0 {
        return 0;
    }
    let den = (1_000 - q) * (1_000 - p);
    // Positive over positive: ceiling.
    (num as u128).div_ceil(den as u128)
}

/// **The ramp term `⌈E/ρ⌉`** (§4.5; `ρ = 0` is read as `1`): the obligation's floor at a step.
pub fn palw_capacity_m_ramp_v1(e_sompi: u128, rho: u32) -> u128 {
    e_sompi.div_ceil(u128::from(rho.max(1)))
}

/// **`m_c`** (§4.5): `E` when there is no step (today, or below F-L) or when the class has no
/// conviction route (C7, `q ≡ 0`); otherwise `max(m*(q_credit), ⌈E/ρ⌉)`. `L` is the claim's
/// [`palw_capacity_conviction_l_v1`]. `m_c ≥ ⌈E/ρ⌉ ≥ 1` sompi whenever `E ≥ 1` (X-I6), and
/// `m_c = E` at `q_credit = 0`.
pub fn palw_capacity_m_c_v1(e_sompi: u128, step: Option<&PalwCapacityStepV1>, attributable: bool, l_sompi: u128) -> u128 {
    match step {
        None => e_sompi,
        Some(_) if !attributable => e_sompi,
        Some(step) => {
            palw_capacity_m_star_v1(e_sompi, step.q_credit_permille, l_sompi).max(palw_capacity_m_ramp_v1(e_sompi, step.rho))
        }
    }
}

/// **`L = 3·G`** (§4.5): the smallest amount every producer conviction route collects, from the
/// claim's fraud gain `G` (`palw_claim_g_v1(..).g()`: `g_res + E`).
pub fn palw_capacity_conviction_l_v1(g_sompi: u128) -> u128 {
    g_sompi.saturating_mul(PALW_CAPACITY_L_MULTIPLE_OF_G_V1)
}

/// **The smallest credited `q` (permille) at which the ramp term binds** — `m*(q) ≤ ⌈E/ρ⌉` (§4.5's
/// "q needed for each ramp step", `q/(1−q) ≥ (E − E/ρ)/(2L)` at `P* = ½`). Exact over the integer
/// `m*`, so it is the ADR's figure rounded UP to a permille: 131 / 138 / 142 / 143 at ρ = 10 / 25 /
/// 100 / 1000 and `L = 3G` (the ADR prints 0.130 for 0.1304). `1000` when nothing short of certain
/// conviction suffices (`E/ρ` below any `m*` — never at a finite `L > 0`).
pub fn palw_capacity_q_needed_permille_v1(e_sompi: u128, rho: u32, l_sompi: u128) -> u16 {
    let target = palw_capacity_m_ramp_v1(e_sompi, rho);
    // `m*` is non-increasing in q: the first q that meets the target, by bisection over [0, 1000].
    let (mut lo, mut hi) = (0u16, 1_000u16);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if palw_capacity_m_star_v1(e_sompi, mid, l_sompi) <= target {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

/// **`EV(K)` of a campaign of `K` fraudulent claims inside one maturity window** (§4.5), in sompi
/// (truncated toward zero):
///
/// ```text
/// EV(K) = (1−q)^K · K · [P·E − (1−P)·m] − (1 − (1−q)^K) · L
/// ```
///
/// Freeze-on-first-conviction (AG-3) makes the campaign earn only if none is convicted, and
/// `EV(1) ≤ 0 ⇒ EV(K) ≤ 0` for every `K` (Bernoulli) — lane liab's L-T6 golden and the ADR's
/// Appendix A (q 0.11, P 0.5, m 32, L 13,000: −20, −193, −1,317, −4,006, −12,999, −13,000 MSK).
/// Fixed point at 10¹⁸; saturating.
pub fn palw_capacity_campaign_ev_sompi_v1(
    k: u32,
    q_permille: u16,
    p_permille: u16,
    e_sompi: u128,
    m_sompi: u128,
    l_sompi: u128,
) -> i128 {
    const ONE: u128 = 1_000_000_000_000_000_000;
    let q = u128::from(q_permille.min(1_000));
    let p = i128::from(p_permille.min(1_000));
    // a = 1 − q at 10¹⁸, then a^K by squaring (each product rounded down at 10¹⁸).
    let a = (1_000 - q) * (ONE / 1_000);
    let mut base = a;
    let mut exp = k;
    let mut a_k = ONE;
    while exp > 0 {
        if exp & 1 == 1 {
            a_k = mul_fixed(a_k, base, ONE);
        }
        base = mul_fixed(base, base, ONE);
        exp >>= 1;
    }
    let e = i128::try_from(e_sompi).unwrap_or(i128::MAX);
    let m = i128::try_from(m_sompi).unwrap_or(i128::MAX);
    let l = i128::try_from(l_sompi).unwrap_or(i128::MAX);
    // Per-claim edge in sompi × 1000: P·E − (1−P)·m.
    let edge_milli = p.saturating_mul(e).saturating_sub((1_000 - p).saturating_mul(m));
    let a_k_i = a_k as i128;
    let one_i = ONE as i128;
    // a^K · K · edge / 1000, at 10¹⁸.
    let win = (a_k_i / 1_000_000).saturating_mul(i128::from(k)).saturating_mul(edge_milli) / 1_000 / 1_000_000_000_000;
    let lose = ((one_i - a_k_i) / 1_000_000).saturating_mul(l) / 1_000_000_000_000;
    win.saturating_sub(lose)
}

fn mul_fixed(a: u128, b: u128, one: u128) -> u128 {
    // a, b ≤ one = 10¹⁸: split one operand to keep the product inside u128.
    let (hi, lo) = (a / 1_000_000_000, a % 1_000_000_000);
    (hi.saturating_mul(b) / (one / 1_000_000_000)).saturating_add(lo.saturating_mul(b) / one)
}

// ---------------------------------------------------------------------------------------------
// §4.7 the seat side (lane liab AS-1 / AS-2)
// ---------------------------------------------------------------------------------------------

/// **AS-1: `duty′ = palw_rcore_duty_bind_v1(⌈λ/ρ⌉, ⌈lock_2/ρ⌉, commitment′_at_bind, seats)`**, with
/// `commitment′ = m_c + reserved`, so `seats × duty′ ≤ commitment′` (the ≤ 1 amplification bound,
/// F14) still holds. `rho = 1` is today's duty.
pub fn palw_capacity_seat_duty_v1(lambda_term: u128, lock_2: u128, commitment_new: u128, seat_count: usize, rho: u32) -> u128 {
    let rho = u128::from(rho.max(1));
    crate::palw_state_v2::palw_rcore_duty_bind_v1(lambda_term.div_ceil(rho), lock_2.div_ceil(rho), commitment_new, seat_count)
}

/// **The seat credit test of AS-2: `q_credit ≥ q_seat = E / (E + L_seat)`**, exactly (no rounding):
/// `q_credit · (E + L_seat) ≥ 1000 · E`. `L_seat` is 3G (q_seat 0.25) or the whole seat bond for a
/// located `PanelFalseValidV2` (q_seat ≈ 0.024, §10 D-5).
pub fn palw_capacity_seat_credit_applies_v1(q_credit_permille: u16, e_sompi: u128, l_seat_sompi: u128) -> bool {
    u128::from(q_credit_permille).saturating_mul(e_sompi.saturating_add(l_seat_sompi)) >= e_sompi.saturating_mul(1_000)
}

/// `q_seat` in permille, rounded up (the display of the test above; `⌈1000·E/(E+L)⌉`).
pub fn palw_capacity_q_seat_permille_v1(e_sompi: u128, l_seat_sompi: u128) -> u16 {
    let den = e_sompi.saturating_add(l_seat_sompi);
    if den == 0 {
        return 0;
    }
    e_sompi.saturating_mul(1_000).div_ceil(den).min(1_000) as u16
}

/// **AS-2: `lock′ = max(1 sompi, ⌈lock_k/ρ⌉)`** when the step's credit passes the seat test
/// ([`palw_capacity_seat_credit_applies_v1`] at `e_sompi` and `l_seat_sompi`), else `lock_k`
/// unchanged. The lock stays `> 0` so B-3's exit gate still pins the seat to its horizon. `None`
/// (no step) is today's lock.
pub fn palw_capacity_seat_lock_v1(lock_k: u128, step: Option<&PalwCapacityStepV1>, e_sompi: u128, l_seat_sompi: u128) -> u128 {
    match step {
        Some(step) if palw_capacity_seat_credit_applies_v1(step.q_credit_permille, e_sompi, l_seat_sompi) => {
            lock_k.div_ceil(step.rho_or_one()).max(1)
        }
        _ => lock_k,
    }
}

// ---------------------------------------------------------------------------------------------
// §5 the numeric targets
// ---------------------------------------------------------------------------------------------

/// **Concurrent claims a bond's room holds** (§5.2, E-T3): the largest `N` with
/// `N·m + min(N·w, budget_left) ≤ room` — `N` identical claims each committing the obligation `m`
/// and the weight reservation `min(w, R_budget − held)`, whose sum over `N` claims is exactly
/// `min(N·w, budget_left)`. Today's rule is `m = w + E`, `w = 0`: `⌊room / (w + E)⌋`.
/// `u64::MAX` when nothing is committed per claim (unbounded by collateral).
pub fn palw_capacity_n_instant_v1(room_sompi: u128, m_sompi: u128, w_sompi: u128, budget_left_sompi: u128) -> u64 {
    let cost = |n: u128| n.saturating_mul(m_sompi).saturating_add(n.saturating_mul(w_sompi).min(budget_left_sompi));
    let hi = if m_sompi > 0 {
        room_sompi / m_sompi
    } else if w_sompi == 0 || budget_left_sompi <= room_sompi {
        return u64::MAX;
    } else {
        room_sompi / w_sompi
    };
    // `cost` is non-decreasing; `cost(0) = 0 ≤ room`. Largest n in [0, hi] with cost(n) ≤ room.
    let (mut lo, mut hi) = (0u128, hi);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if cost(mid) <= room_sompi {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    u64::try_from(lo).unwrap_or(u64::MAX)
}

/// **Per-bond throughput** (§5.3): `N` slots turning over at the licence latency `H_L` give
/// `N / H_L` claims per DAA, reported per 80 DAA in thousandths (`⌊N × 80 × 1000 / H_L⌋`).
pub fn palw_capacity_per_80_daa_milli_v1(n: u64, licence_latency_daa: u64) -> u64 {
    if licence_latency_daa == 0 {
        return u64::MAX;
    }
    u64::try_from(u128::from(n) * 80_000 / u128::from(licence_latency_daa)).unwrap_or(u64::MAX)
}

/// The §5.4 seat-capital estimate's inputs for one claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacitySeatCapitalInputsV1 {
    /// Seats that carry the duty (the panel's seat count).
    pub duty_seats: u32,
    /// The per-seat duty, bind → Final (sompi).
    pub duty_sompi: u128,
    /// How long the duty is held (DAA; [`PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1`]).
    pub duty_hold_daa: u64,
    /// Seats that lock after the licence (every Valid seat; five at the coverage door).
    pub lock_seats: u32,
    /// The per-seat lock (sompi).
    pub lock_sompi: u128,
    /// How long the lock is held past Final (DAA; `window_court`, 3,000 shipped, 1,000 with the
    /// lock-life lane).
    pub lock_hold_daa: u64,
}

/// **Seat capital one claim ties up** (§5.4, Appendix A): `duty_seats·duty·duty_hold +
/// lock_seats·lock·lock_hold`, in sompi·DAA. Today's floor: 3,992,004 MSK·DAA.
pub fn palw_capacity_seat_capital_per_claim_v1(inputs: &PalwCapacitySeatCapitalInputsV1) -> u128 {
    u128::from(inputs.duty_seats).saturating_mul(inputs.duty_sompi).saturating_mul(u128::from(inputs.duty_hold_daa)).saturating_add(
        u128::from(inputs.lock_seats).saturating_mul(inputs.lock_sompi).saturating_mul(u128::from(inputs.lock_hold_daa)),
    )
}

/// **Network claims per DAA the seat capital sustains** (§5.4), in thousandths:
/// `⌊1000 · usable / per_claim⌋` — usable seat capital (sompi) over seat capital per claim
/// (sompi·DAA). 8 genesis seats × 469,531.6 MSK against 3,992,004 MSK·DAA: 941 (0.94/DAA).
/// `u64::MAX` for a claim that ties up nothing.
pub fn palw_capacity_claims_per_daa_milli_v1(usable_sompi: u128, per_claim_sompi_daa: u128) -> u64 {
    if per_claim_sompi_daa == 0 {
        return u64::MAX;
    }
    u64::try_from(usable_sompi.saturating_mul(1_000) / per_claim_sompi_daa).unwrap_or(u64::MAX)
}

/// **Licence carriers one block holds** (§5.4, capacity map §6.4): `⌊block_mass / carrier_mass⌋` —
/// 3 five-receipt coverage carriers against 500,000.
pub fn palw_capacity_carriers_per_block_v1(block_mass_limit: u64, carrier_mass: u64) -> u64 {
    block_mass_limit.checked_div(carrier_mass).unwrap_or(u64::MAX)
}

/// **Reference-speed GPU equivalents for ×`multiplier` of today's 2M rate** (§5.4, Appendix A), in
/// thousandths: `multiplier / period × r_v × vccu / ref_ccu_per_span` — today's rate is one claim per
/// `period_daa` (≈ 2,900), each replayed `r_v` times at `vccu / ref` reference seat-spans (≈ 1,398.9
/// for 2M at 2.4·10¹² CCU per span). ×10 / ×100 / ×1000 at `r_v = 2`: 9.6 / 96.5 / 964.7.
pub fn palw_capacity_gpu_equivalents_milli_v1(multiplier: u64, r_v: u64, vccu: u128, ref_ccu_per_span: u128, period_daa: u64) -> u64 {
    let den = ref_ccu_per_span.saturating_mul(u128::from(period_daa));
    if den == 0 {
        return u64::MAX;
    }
    let num = u128::from(multiplier).saturating_mul(u128::from(r_v)).saturating_mul(vccu).saturating_mul(1_000);
    u64::try_from(num / den).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    //! **S-T1 (formula half): the golden tables of ADR-0160 §4.5, §5.2, §5.3, §5.4 and Appendix A**,
    //! re-derived from the functions above.
    use super::*;

    const MSK: u128 = PALW_CAPACITY_SOMPI_PER_MSK_V1;
    /// Appendix A: E, and w for floor / 8k / 2M (sompi).
    const E: u128 = 320_084_650_080;
    const W_FLOOR: u128 = 10_752_660;
    const W_8K: u128 = 2_471_600_230;
    const W_2M: u128 = 5_974_294_206_820;
    /// Raw immature weight `β·pwu` (β = 100‰) of the three classes.
    const RAW_FLOOR: u128 = 604_250_611;
    const RAW_8K: u128 = 138_892_697_241;
    const RAW_2M: u128 = 335_728_175_722_137;
    const BONDS_MSK: [u64; 3] = [13_000, 100_000, 1_000_000];

    fn msk(n: u64) -> u64 {
        n * 100_000_000
    }
    fn l_3g_floor() -> u128 {
        palw_capacity_conviction_l_v1(W_FLOOR + E)
    }
    /// Round sompi to hundredths of an MSK (the ADR prints two decimals).
    fn centi(sompi: u128) -> u128 {
        (sompi + MSK / 200) / (MSK / 100)
    }
    fn ceiling(collateral_msk: u64) -> u128 {
        u128::from(msk(collateral_msk)) * 500 / 1_000
    }

    #[test]
    fn constants_match_appendix_a() {
        assert_eq!(PALW_CAPACITY_FCW_V1, 279 * 21_657_728 / 10, "FCW = ⌊0.1 × 279 × 21,657,728⌋");
        assert_eq!(PALW_CAPACITY_W_FCW_SOMPI_V1, W_FLOOR, "w_FCW is the floor claim's w");
        assert_eq!(RAW_FLOOR, PALW_CAPACITY_FCW_V1);
        assert_eq!(PALW_CAPACITY_C7_WEIGHT_CEILING_V1, RAW_8K, "the C7 ceiling is 8k's raw weight");
        // p0 is uniform: w_c / raw_c agrees to 7 digits across the three classes.
        let p0_e12 = |w: u128, raw: u128| w * 1_000_000_000_000 / raw;
        let p0 = p0_e12(W_FLOOR, RAW_FLOOR);
        assert_eq!(p0 / 10_000_000, 1_779, "p0 = 0.0177950 sompi per unit");
        for (w, raw) in [(W_8K, RAW_8K), (W_2M, RAW_2M)] {
            assert!(p0.abs_diff(p0_e12(w, raw)) * 10_000_000 / p0 == 0, "p0 uniform to 7 digits");
        }
        // raw / FCW: 229.86 (8k) and 555,610.8 (2M).
        assert_eq!(RAW_8K * 100 / PALW_CAPACITY_FCW_V1, 22_985);
        assert_eq!(RAW_2M * 10 / PALW_CAPACITY_FCW_V1, 5_556_108);
        // Today's reserves (§2): 3,200.95 / 3,225.56 / 62,943.79 MSK.
        assert_eq!([centi(W_FLOOR + E), centi(W_8K + E), centi(W_2M + E)], [320_095, 322_556, 6_294_379]);
    }

    #[test]
    fn weight_cap_and_budget_per_bond() {
        let fcw: Vec<u128> = BONDS_MSK.iter().map(|c| palw_capacity_weight_cap_v1(msk(*c)) / PALW_CAPACITY_FCW_V1).collect();
        assert_eq!(fcw, vec![2, 15, 153], "W_cap: 2 / 15 / 153 FCW");
        assert_eq!(palw_capacity_weight_cap_v1(93_906_321_000_000) / PALW_CAPACITY_FCW_V1, 144, "a genesis card: 144 FCW");
        let budget: Vec<u128> = BONDS_MSK.iter().map(|c| palw_capacity_weight_budget_sompi_v1(msk(*c))).collect();
        // R_budget: 0.215 / 1.61 / 16.45 MSK.
        assert_eq!(budget, vec![21_505_320, 161_289_900, 1_645_156_980]);
        // A 2M claim's consensus reservation falls from 59,742.94 MSK to ≤ 0.215 at 13k (§4.3).
        assert_eq!(palw_capacity_consensus_reservation_v1(W_2M, budget[0], 0), 21_505_320);
        assert_eq!(palw_capacity_consensus_reservation_v1(W_2M, budget[0], 21_505_320), 0, "the budget spent, nothing more");
        assert_eq!(palw_capacity_consensus_reservation_v1(W_FLOOR, budget[0], W_FLOOR), W_FLOOR, "the second floor claim fits");
        assert_eq!(palw_capacity_consensus_reservation_v1(W_FLOOR, budget[0], 2 * W_FLOOR), 0, "the third holds no weight");
        assert_eq!(
            palw_capacity_bond_weight_term_v1(555_610 * PALW_CAPACITY_FCW_V1, 2 * PALW_CAPACITY_FCW_V1),
            2 * PALW_CAPACITY_FCW_V1
        );
        assert_eq!(palw_capacity_weight_cap_v1(msk(6_499)), 0, "below 6,500 MSK a bond buys no provisional weight");
    }

    #[test]
    fn staged_weight_table() {
        let full = palw_capacity_weight_full_v1(RAW_2M, false);
        assert_eq!(full, RAW_2M, "outside C7 the full weight is the raw weight");
        assert_eq!(palw_capacity_weight_full_v1(RAW_2M, true), RAW_8K, "C7's Final weight stops at 8k's raw weight");
        assert_eq!(palw_capacity_weight_full_v1(RAW_FLOOR, true), RAW_FLOOR, "a ceiling, not a floor");
        let x = |stage| palw_capacity_staged_weight_v1(stage, RAW_8K);
        assert_eq!(x(PalwCapacityStageV1::Created), 0);
        assert_eq!(x(PalwCapacityStageV1::Anchored), RAW_8K * 10 / 1_000);
        assert_eq!(x(PalwCapacityStageV1::Licensed { permille: 1_000 }), RAW_8K);
        assert_eq!(x(PalwCapacityStageV1::Licensed { permille: 250 }), RAW_8K / 4);
        assert_eq!(x(PalwCapacityStageV1::Licensed { permille: 4_000 }), RAW_8K, "no licence weighs more than full");
        assert_eq!(x(PalwCapacityStageV1::Final), 0, "Final leaves the immature set");
        assert_eq!(x(PalwCapacityStageV1::Terminal), 0);
    }

    /// §4.5's table: `m*` at L = 3G and at L = 13,000 MSK.
    #[test]
    fn m_star_golden_table() {
        let l13k = 13_000 * MSK;
        let rows: [(u16, u128, u128); 6] = [
            (0, 320_085, 320_085),
            (50, 219_002, 183_243),
            (80, 153_078, 93_998),
            (100, 106_688, 31_196),
            (110, 82_711, 0),
            (143, 0, 0),
        ];
        for (q, at_3g, at_13k) in rows {
            assert_eq!(centi(palw_capacity_m_star_v1(E, q, l_3g_floor())), at_3g, "m*(q = {q}‰, L = 3G)");
            assert_eq!(centi(palw_capacity_m_star_v1(E, q, l13k)), at_13k, "m*(q = {q}‰, L = 13k)");
        }
        assert_eq!(palw_capacity_m_star_v1(E, 0, l_3g_floor()), E, "m*(0) is exactly E");
        assert!(palw_capacity_m_star_v1(E, 142, l_3g_floor()) > 0, "0.142 is not yet enough at L = 3G");
        assert_eq!(palw_capacity_m_star_v1(E, 1_000, 0), 0, "certain conviction");
        assert_eq!(palw_capacity_m_star_at_v1(E, 0, l13k, 1_000), E, "P* = 1 cannot be deterred");
    }

    /// §4.5's q needed per ramp step at L = 3G (0.130 / 0.138 / 0.142 / 0.143, rounded up) and at
    /// L = 13k (0.0997 / 0.1057 / 0.1086 / 0.1095).
    #[test]
    fn q_needed_per_ramp_step() {
        let at = |rho, l| palw_capacity_q_needed_permille_v1(E, rho, l);
        assert_eq!([10, 25, 100, 1000].map(|rho| at(rho, l_3g_floor())), [131, 138, 142, 143]);
        assert_eq!([10, 25, 100, 1000].map(|rho| at(rho, 13_000 * MSK)), [100, 106, 109, 110]);
        // The value found is the first that works.
        for rho in [10u32, 25, 50, 100, 1000] {
            let q = at(rho, l_3g_floor());
            assert!(palw_capacity_m_star_v1(E, q, l_3g_floor()) <= palw_capacity_m_ramp_v1(E, rho));
            assert!(palw_capacity_m_star_v1(E, q - 1, l_3g_floor()) > palw_capacity_m_ramp_v1(E, rho));
        }
    }

    #[test]
    fn m_c_rules() {
        let step = |rho, q| PalwCapacityStepV1 { from_daa: 0, rho, q_credit_permille: q };
        let l = l_3g_floor();
        assert_eq!(palw_capacity_m_c_v1(E, None, true, l), E, "no step: today's E");
        assert_eq!(palw_capacity_m_c_v1(E, Some(&step(100, 500)), false, l), E, "C7: m = E whatever the step");
        assert_eq!(palw_capacity_m_c_v1(E, Some(&step(100, 0)), true, l), E, "q_credit 0: m = E (X-I6)");
        // The ramp rows of §5.2: 320.08 / 128.03 / 64.02 / 32.01 / 3.20.
        let ramp: Vec<u128> =
            [10u32, 25, 50, 100, 1000].iter().map(|rho| centi(palw_capacity_m_c_v1(E, Some(&step(*rho, 143)), true, l))).collect();
        assert_eq!(ramp, vec![32_008, 12_803, 6_402, 3_201, 320]);
        // An under-credited q only raises m above the ramp term.
        assert_eq!(palw_capacity_m_c_v1(E, Some(&step(100, 100)), true, l), palw_capacity_m_star_v1(E, 100, l));
        assert!(palw_capacity_m_c_v1(1, Some(&step(1_000_000, 143)), true, l) >= 1, "⌈E/ρ⌉ ≥ 1 sompi");
    }

    /// §5.2: concurrent claims per 13k / 100k / 1M bond, floor/8k by collateral and 2M under C7,
    /// with the J-1 weight budget (E-T3's figures: 13k → 20 / 50 / 101 / 203 / 2,030). Exact, so
    /// the weight term counts: ρ = 1000 at 1M is 156,203, where the ADR's table (which omits the
    /// ≤ 16.45 MSK weight budget and rounds `E/ρ` down) prints 156,208.
    #[test]
    fn claims_per_bond_golden() {
        let l = l_3g_floor();
        let today: Vec<u64> = BONDS_MSK.iter().map(|c| palw_capacity_n_instant_v1(ceiling(*c), W_FLOOR + E, 0, 0)).collect();
        assert_eq!(today, vec![2, 15, 156], "today");
        let today_8k: Vec<u64> = BONDS_MSK.iter().map(|c| palw_capacity_n_instant_v1(ceiling(*c), W_8K + E, 0, 0)).collect();
        assert_eq!(today_8k[0], 2, "8k by collateral alone (the w hold makes it 1 sustained)");
        assert_eq!(
            BONDS_MSK.map(|c| palw_capacity_n_instant_v1(ceiling(c), W_2M + E, 0, 0)),
            [0, 0, 7],
            "2M today (7 at 1M by exposure alone)"
        );
        let expect: [(u32, [u64; 3]); 5] = [
            (10, [20, 156, 1_562]),
            (25, [50, 390, 3_905]),
            (50, [101, 781, 7_810]),
            (100, [203, 1_562, 15_620]),
            (1000, [2_030, 15_620, 156_203]),
        ];
        for (rho, row) in expect {
            let step = PalwCapacityStepV1 { from_daa: 0, rho, q_credit_permille: 143 };
            let m = palw_capacity_m_c_v1(E, Some(&step), true, l);
            for (i, c) in BONDS_MSK.iter().enumerate() {
                let budget = palw_capacity_weight_budget_sompi_v1(msk(*c));
                assert_eq!(palw_capacity_n_instant_v1(ceiling(*c), m, W_FLOOR, budget), row[i], "floor ρ = {rho}, {c} MSK");
                assert_eq!(palw_capacity_n_instant_v1(ceiling(*c), m, W_8K, budget), row[i], "8k by collateral ρ = {rho}, {c} MSK");
                // 2M (C7): m = E whatever ρ → 2 / 15 / 156, the weight term ≤ R_budget.
                let m_2m = palw_capacity_m_c_v1(E, Some(&step), false, l);
                assert_eq!(palw_capacity_n_instant_v1(ceiling(*c), m_2m, W_2M, budget), [2, 15, 156][i], "2M ρ = {rho}, {c} MSK");
            }
        }
        // G-B: 13k × 100 needs ≤ 65 MSK per claim — ρ = 50 is the first row that meets it.
        let m50 = palw_capacity_m_c_v1(E, Some(&PalwCapacityStepV1 { from_daa: 0, rho: 50, q_credit_permille: 143 }), true, l);
        assert!(m50 + W_FLOOR <= 65 * MSK);
        let m25 = palw_capacity_m_c_v1(E, Some(&PalwCapacityStepV1 { from_daa: 0, rho: 25, q_credit_permille: 143 }), true, l);
        assert!(m25 > 65 * MSK);
    }

    /// §5.3: per-bond floor throughput at H_L = 21 DAA (claims per 80 DAA).
    #[test]
    fn per_bond_throughput_golden() {
        assert_eq!(palw_capacity_per_80_daa_milli_v1(2, 21) / 100, 76, "today 13k: 7.6");
        assert_eq!(palw_capacity_per_80_daa_milli_v1(20, 21) / 1_000, 76, "ρ = 10, 13k: 76");
        assert_eq!(palw_capacity_per_80_daa_milli_v1(156, 21) / 1_000, 594, "ρ = 10, 100k: 594");
        assert_eq!(palw_capacity_per_80_daa_milli_v1(203, 21) / 1_000, 773, "ρ = 100, 13k: 773");
        assert_eq!(palw_capacity_per_80_daa_milli_v1(15_620, 21) / 1_000, 59_504, "ρ = 100 1M / ρ = 1000 100k: 59,505");
        assert_eq!(palw_capacity_per_80_daa_milli_v1(2_030, 21) / 1_000, 7_733, "ρ = 1000, 13k: 7,733");
    }

    /// §5.4 / Appendix A: network floor claims per DAA from seat capital, shipped / +V02 / +V02 and
    /// lock life F+1,000, at each ρ (duty and lock both ÷ρ).
    #[test]
    fn seat_capital_golden() {
        let duty = 64_017_000_000u128; // 640.17 MSK
        let lock = 24_010_000_000u128; // 240.1 MSK
        let per_claim = |rho: u32, lock_hold: u64| {
            palw_capacity_seat_capital_per_claim_v1(&PalwCapacitySeatCapitalInputsV1 {
                duty_seats: 5,
                duty_sompi: duty.div_ceil(u128::from(rho)),
                duty_hold_daa: PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1,
                lock_seats: 5,
                lock_sompi: lock.div_ceil(u128::from(rho)),
                lock_hold_daa: lock_hold,
            })
        };
        assert_eq!(per_claim(1, 3_000) / MSK, 3_992_003, "3,992,004 MSK·DAA per floor claim");
        assert_eq!(per_claim(1, 1_000) / MSK, 1_591_003, "1,591,004 with lock life F+1,000");
        let shipped = 8 * 46_953_160_000_000u128; // 8 × 469,531.6 MSK
        let v02 = 8 * 88_706_300_000_000u128; // 8 × 887,063 MSK
        let rows: [(u32, [u64; 3]); 5] = [
            (1, [940, 1_777, 4_460]),
            (10, [9_409, 17_776, 44_603]),
            (25, [23_523, 44_441, 111_509]),
            (100, [94_094, 177_767, 446_039]),
            (1000, [940_944, 1_777_679, 4_460_394]),
        ];
        for (rho, [a, b, c]) in rows {
            assert_eq!(palw_capacity_claims_per_daa_milli_v1(shipped, per_claim(rho, 3_000)), a, "shipped ρ = {rho}");
            assert_eq!(palw_capacity_claims_per_daa_milli_v1(v02, per_claim(rho, 3_000)), b, "V02 ρ = {rho}");
            assert_eq!(palw_capacity_claims_per_daa_milli_v1(v02, per_claim(rho, 1_000)), c, "V02 + lock life ρ = {rho}");
        }
        assert_eq!(
            palw_capacity_carriers_per_block_v1(500_000, PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1),
            3,
            "3 coverage carriers per block"
        );
    }

    #[test]
    fn seat_duty_and_lock_repricing() {
        // Today's cases (ADR-0152's unit test) at ρ = 1.
        assert_eq!(palw_capacity_seat_duty_v1(256, 160, 3_201, 5, 1), 256);
        assert_eq!(palw_capacity_seat_duty_v1(100, 33_379, 62_943, 5, 1), 12_588);
        // AS-1 at ρ = 100: a floor duty of 640.17 falls to ≈ 6.40 and the ≤ 1 bound holds.
        let floor_duty = palw_capacity_seat_duty_v1(64_016_930_016, 24_010_000_000, 3_211_537_161, 5, 100);
        assert_eq!(floor_duty, 640_169_301);
        assert!(floor_duty * 5 <= 3_211_537_161);
        for rho in [1u32, 10, 100, 1000] {
            for (lambda, lock2, commit, n) in
                [(64_016_930_016u128, 24_010_000_000u128, 320_095_402_740u128, 5usize), (100, 3_337_900_000_000, 6_294_378_856_900, 5)]
            {
                let commit_new = commit.div_ceil(u128::from(rho));
                assert!(palw_capacity_seat_duty_v1(lambda, lock2, commit_new, n, rho) * n as u128 <= commit_new, "F14 at ρ = {rho}");
            }
        }
        // AS-2: q_seat = E/(E + 3G) = 0.25; with the whole seat bond (130k) ≈ 0.024.
        let l3g = l_3g_floor();
        assert_eq!(palw_capacity_q_seat_permille_v1(E, l3g), 250);
        assert_eq!(palw_capacity_q_seat_permille_v1(E, 130_000 * MSK), 25, "0.0240 rounded up");
        assert!(palw_capacity_seat_credit_applies_v1(250, E, l3g));
        assert!(!palw_capacity_seat_credit_applies_v1(249, E, l3g));
        assert!(palw_capacity_seat_credit_applies_v1(25, E, 130_000 * MSK));
        let lock = 24_010_000_000u128;
        let credited = PalwCapacityStepV1 { from_daa: 0, rho: 100, q_credit_permille: 250 };
        let short = PalwCapacityStepV1 { from_daa: 0, rho: 100, q_credit_permille: 143 };
        assert_eq!(palw_capacity_seat_lock_v1(lock, Some(&credited), E, l3g), 240_100_000);
        assert_eq!(palw_capacity_seat_lock_v1(lock, Some(&short), E, l3g), lock, "q below q_seat: the lock stays");
        assert_eq!(palw_capacity_seat_lock_v1(lock, None, E, l3g), lock);
        assert_eq!(palw_capacity_seat_lock_v1(1, Some(&PalwCapacityStepV1 { rho: 1_000, ..credited }), E, l3g), 1, "lock′ ≥ 1 sompi");
    }

    /// Appendix A: EV(K) at q 0.11, P 0.5, m 32, L 13,000 — K = 1 binds.
    #[test]
    fn campaign_ev_golden() {
        let ev = |k| {
            let v = palw_capacity_campaign_ev_sompi_v1(k, 110, 500, E, 32 * MSK, 13_000 * MSK);
            // Round to whole MSK.
            (v + v.signum() * (MSK as i128 / 2)) / MSK as i128
        };
        assert_eq!([1, 2, 5, 10, 100, 1000].map(ev), [-20, -193, -1_317, -4_006, -12_999, -13_000]);
        // EV(1) ≤ 0 ⇒ EV(K) ≤ 0 across a grid (Bernoulli).
        for q in [0u16, 50, 110, 143, 300] {
            let l = l_3g_floor();
            let m = palw_capacity_m_star_v1(E, q, l);
            for k in [1u32, 2, 10, 100, 1000] {
                assert!(palw_capacity_campaign_ev_sompi_v1(k, q, 500, E, m, l) <= 1, "q = {q}‰, K = {k}");
            }
        }
    }

    /// §5.4: 2M reference-GPU equivalents at r_v = 2 (9.6 / 96.5 / 964.7).
    #[test]
    fn two_m_gpu_golden() {
        let vccu = 3_357_281_757_221_376u128;
        let at = |mult| palw_capacity_gpu_equivalents_milli_v1(mult, 2, vccu, 2_400_000_000_000, 2_900) / 100;
        assert_eq!([10, 100, 1000].map(at), [96, 964, 9_647]);
    }

    #[test]
    fn step_schedule_and_degenerate_inputs() {
        let s = |from, rho| PalwCapacityStepV1 { from_daa: from, rho, q_credit_permille: 143 };
        let steps = [s(100, 10), s(500, 25)];
        assert_eq!(palw_capacity_step_at_v1(&steps, 99), None);
        assert_eq!(palw_capacity_step_at_v1(&steps, 100).map(|x| x.rho), Some(10));
        assert_eq!(palw_capacity_step_at_v1(&steps, 499).map(|x| x.rho), Some(10));
        assert_eq!(palw_capacity_step_at_v1(&steps, u64::MAX).map(|x| x.rho), Some(25));
        assert_eq!(palw_capacity_step_at_v1(&[], 7), None);
        assert_eq!(palw_capacity_m_ramp_v1(E, 0), E, "ρ = 0 is read as 1");
        assert_eq!(palw_capacity_n_instant_v1(100, 0, 0, 0), u64::MAX);
        assert_eq!(palw_capacity_n_instant_v1(100, 0, 10, 1_000), 10);
        assert_eq!(palw_capacity_n_instant_v1(0, 5, 1, 1), 0);
        assert_eq!(palw_capacity_n_instant_v1(u128::MAX, 1, 1, u128::MAX), u64::MAX);
        assert_eq!(palw_capacity_claims_per_daa_milli_v1(1, 0), u64::MAX);
        assert_eq!(palw_capacity_per_80_daa_milli_v1(1, 0), u64::MAX);
        assert_eq!(palw_capacity_carriers_per_block_v1(1, 0), u64::MAX);
        assert_eq!(palw_capacity_q_seat_permille_v1(0, 0), 0);
        let _ = palw_capacity_campaign_ev_sompi_v1(u32::MAX, 1, 999, u128::MAX, u128::MAX, u128::MAX);
        assert_eq!(palw_capacity_m_star_v1(u128::MAX, 1, u128::MAX), 0, "saturating, never a panic");
    }
}
