//! **Class-derived verification deadlines** (ADR-0152 §4-quater; `Params::palw_class_verify_deadline`).
//!
//! A claim's compute-bearing verification deadline `D(c)` is derived from its class, in the right
//! units, instead of stretching one global number (the 42-DAA court turn, the 3,000-DAA court
//! window and the 600-DAA receipt window stay global — each is a *response* once the panel loop
//! never waits on compute). This module holds the pure, state-free half: the constants, the
//! measured-row type a later flag day installs, and the two derivations (the registry's reference
//! rate, and a measured row's `T_R(N)`). The state-reading half — the claim's `D`, its receipt
//! window `W_r(c)`, its verification horizon `H(c)` and the one Final floor — lives beside the
//! deadline index in `palw_state_v2` (`PalwStateParamsV2::claim_verify_daa_v1`,
//! `palw_claim_final_floor_v1`, `palw_class_verify_deadline_v1`).
//!
//! # The units (the bug this fixes, V3)
//!
//! The registry prices a span as 600 s of reference work (`PALW_SPAN_MS_V1`, 4·10⁹ MAC-eq/s) and
//! derives a class's `verification_window_spans` in THOSE spans. ADR-0133 §11.3 multiplied it by the
//! lane's schedule span, which is 5 DAA on testnet-11 (right: 5 × 120 s = 600 s) and 1 DAA on
//! testnet-12 (wrong: a fifth of the window the derivation meant — the 2M row got 2,799 DAA instead
//! of 13,995). Past the fence a span is counted as `PALW_CLASS_VERIFY_REF_SPAN_DAA_V1` =
//! ⌈600,000 ms / 120,000 ms⌉ = 5 DAA on every network, whatever the lane's span.
//!
//! # Not in this change — owed before the flag day that opens a long-D class (ADR-0153)
//!
//! No testnet-12 genesis class is long-D except the 2M row, which V2 keeps closed, so none of these
//! binds at launch; each binds the first time a class with `D` past 120 takes a claim (a post-genesis
//! registration, or the 2M row's measured row):
//!
//! * **Retention vs the court (the review's L5).** The producer's trace retention is pinned at
//!   admission to the global `bind + receipt + challenge + court` = A + 5,400, but a class with `D` in
//!   (120, 600] can be redrawn, bound late and Final at `H`, and a court opened just before that Final
//!   runs to about A + 5,402 — two DAA past what the producer was obliged to keep. §4-quater's `R_eff`
//!   (the DA and court gates, the held dissection) is the fix; it is dormant here.
//! * **V6 invalidates receipts already signed.** Advancing `panels[c].bound_daa` by a DA pause makes
//!   every receipt signed before the shift fail the `signed_daa ≥ bound_daa` check, so each seat must
//!   re-sign. A seat must not have to REPLAY again for that: node N-2 must keep its replay result per
//!   (claim, job), not per duty key (which the new `bound_daa` changes).
//! * **V2c at registration** (paging classes, keyed on `artifact_bytes`, see
//!   `PalwFoldReadV1::check_class_verify_admits_v1`'s TODO), and **SR-1b's `daa ≥ H`** on its
//!   supplementary path.
//! * **S-6's share and SW-9's `ready_eff` read C7, not K-1's hold.** `bond_class_share_v1` sizes
//!   `c_class` by C7 (`palw_rcore_class_is_c7_v1`) and `palw_panel_room_ready_eff_terms_v1` asks the
//!   window rule, where K-1's hold is `palw_panel_holds_to_final_v1` (C7 ∪ long-D). A long-D class
//!   outside C7 would be held to Final by the room yet priced by the rate for its per-bond share and
//!   counted by `ready_eff`. Must be fixed before any long-D class is accepted (TODOs at both sites).
//!
//! # The pruning depth (P-1), in force from genesis
//!
//! Past this fence the pruning depth is derived from the D_cap claim lattice
//! (`config::params::palw_v2_claim_lattice_daa_v1`: the receipt window at D_cap and the DA term at
//! `R_eff`'s span) — 74,920 on testnet-12, against 12,002 before — because a pruning point can never
//! move backward (K37), so the horizon a 2M flag day will need can only be chosen at genesis. For the
//! regenesis owner: the storage and IBD cost of a ~6× horizon is unmeasured (M12 runs after launch),
//! the pruning point starts ~74,920 DAA (≈ 104 days at 120 s) after genesis instead of ~12,002
//! (≈ 17 days), the header/block caches are sized by it (under their byte budgets), and a node fewer
//! than 74,920 DAA behind now syncs by headers rather than by a pruning proof.

use crate::Hash64;
use crate::palw_model_registry_v1::{PALW_REGISTRY_GLOBALS_V1, PalwModelWorkV1, palw_verification_window_spans_v1};
use crate::palw_verification_profile_v1::PALW_SPAN_MS_V1;

/// τ: the chain's target block time, the unit a wall-clock deadline is converted to DAA in (U-D4:
/// the target value, no MTP floor).
pub const PALW_CLASS_VERIFY_TAU_MS_V1: u64 = crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;

/// One registry span in DAA: ⌈600,000 ms / τ⌉ = 5. The unit fix (V3).
pub const PALW_CLASS_VERIFY_REF_SPAN_DAA_V1: u64 = PALW_SPAN_MS_V1.div_ceil(PALW_CLASS_VERIFY_TAU_MS_V1);

/// **D_cap** (U-D3): no claim's compute deadline exceeds 16,000 DAA. A measured row's `D` is clamped
/// to `[1, D_cap]`; a free-prompt claim whose measured `D` exceeds it is refused
/// (`FreePromptDeadlineOverCap`). The pruning depth that makes a 16,000-DAA claim lattice fit is
/// derived from it wherever this fence is armed (P-1, `config::params::palw_v2_claim_lattice_daa_v1`).
pub const PALW_CLASS_VERIFY_CAP_DAA_V1: u64 = 16_000;

/// m: the margin a measured row's replay time is multiplied by (U-D4).
pub const PALW_CLASS_VERIFY_MARGIN_V1: u64 = 2;

/// The held context past which a class needs a measured row (U-D2 (ii)): the 8k row's `n_ctx`.
pub const PALW_CLASS_VERIFY_HELD_N_CTX_MAX_V1: u32 = 8_192;

/// **Long-D**: a deadline past the short challenge window (`PALW_SHORT_CHALLENGE_WINDOW_DAA_V1`,
/// 120). Only a long-D claim can outlive its licence's own Final floor, so only a long-D class owes
/// the panel room until Final (K-1) and only a long-D claim's DA pause moves `H` (V6). No testnet-12
/// genesis class is long-D except the 2M row, which is closed.
pub const PALW_CLASS_VERIFY_LONG_D_DAA_V1: u64 = crate::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1;

/// **The shape of the job a seat replays to verify a claim** — the canonical job (an attempt), or a
/// free-prompt run of `work_leaves` leaves. The derived branch prices a free-prompt claim at its
/// class's largest run (the class's `n_ctx`, [`palw_fp_class_verify_ccu_v1`]); a measured row prices
/// it at its own size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimVerifyShapeV1 {
    Attempt,
    FreePrompt { work_leaves: u64 },
}

impl PalwClaimVerifyShapeV1 {
    /// A claim's shape, read off its record: its lane and, for a free-prompt claim, its stored
    /// `work_leaves` (which the chain re-derives at commitment, K25).
    pub fn of_claim(claim: &crate::palw_state_v2::PalwClaimStateV2) -> Self {
        match claim.source {
            crate::palw_state_v2::PalwClaimSourceV2::Attempt => Self::Attempt,
            crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. } => Self::FreePrompt { work_leaves: claim.work_leaves },
        }
    }
}

/// **A measured verification row** (`Params::palw_class_verify_rows`, ADR-0153's flag day): the
/// replay time of the class on `h_min`, the slowest host class certified for it, as
/// `T_R(N) = a_R·N + b_R·N(N−1)/2` over `N` replayed positions, plus the fixed terms (the seat's
/// material wait, naming, checkpoint I/O, propagation). Empty on every network at launch, so every
/// class runs the derived branch and a class that needs a measured row (NM) takes no claim.
///
/// The two quantities the chain does not store are carried by the row: the canonical job's position
/// count `N0` (an attempt's `N`) and the leaves a free-prompt position costs (`N(c) = ⌈work_leaves /
/// λ⌉`, UNVERIFIED for decode positions — pinned by the flag day's golden test).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassVerifyRowV1 {
    pub class_id: Hash64,
    /// The DAA from which the row prices the class's claims — judged at a claim's bind (`B(c)`), and
    /// at the admitting block for admission.
    pub activation_daa: u64,
    /// `a_R`, picoseconds a replayed position costs with no history.
    pub a_r_ps: u64,
    /// `b_R`, picoseconds each (position, history row) pair adds.
    pub b_r_ps: u64,
    /// `t_fixed_R`, milliseconds: `≥ X_ASK·τ + T_name + checkpoint I/O + propagation`.
    pub t_fixed_ms: u64,
    /// `N0`: the positions of the class's canonical job (an attempt's replay).
    pub canonical_positions: u64,
    /// λ: step leaves per replayed position of a free-prompt run.
    pub leaves_per_position: u64,
}

impl PalwClassVerifyRowV1 {
    /// Whether the row prices a claim bound (or admitted) at `daa_score`.
    pub fn is_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.activation_daa
    }

    /// `N` for a claim of this shape: `N0` for an attempt, `⌈work_leaves / λ⌉` (one at least) for a
    /// free-prompt run.
    pub fn positions_of(&self, shape: PalwClaimVerifyShapeV1) -> u64 {
        match shape {
            PalwClaimVerifyShapeV1::Attempt => self.canonical_positions,
            PalwClaimVerifyShapeV1::FreePrompt { work_leaves } => work_leaves.div_ceil(self.leaves_per_position.max(1)).max(1),
        }
    }

    /// `⌈(m·T_R(N) + t_fixed_R) / τ⌉`, unclamped (saturating: a size no u128 holds is over any cap).
    pub fn raw_daa(&self, positions: u64) -> u128 {
        let n = u128::from(positions);
        let pairs = n.saturating_mul(n.saturating_sub(1)) / 2;
        let t_ps = u128::from(self.a_r_ps).saturating_mul(n).saturating_add(u128::from(self.b_r_ps).saturating_mul(pairs));
        let fixed_ps = u128::from(self.t_fixed_ms).saturating_mul(1_000_000_000);
        let tau_ps = u128::from(PALW_CLASS_VERIFY_TAU_MS_V1) * 1_000_000_000;
        t_ps.saturating_mul(u128::from(PALW_CLASS_VERIFY_MARGIN_V1)).saturating_add(fixed_ps).div_ceil(tau_ps)
    }

    /// `D(c) = min(D_cap, ⌈(m·T_R(N) + t_fixed_R) / τ⌉)`, one at least.
    pub fn daa(&self, positions: u64) -> u64 {
        self.raw_daa(positions).clamp(1, u128::from(PALW_CLASS_VERIFY_CAP_DAA_V1)) as u64
    }

    /// What `Params::validate_palw_v2` asks of a row on its own: a job to price (`N0 ≥ 1`, `λ ≥ 1`),
    /// a replay that costs something (`a_R` or `b_R` non-zero), and a canonical deadline inside
    /// `[1, D_cap]` — a row whose own canonical job does not fit the cap is a class that cannot open
    /// on this chain, not one to clamp.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.class_id == Hash64::default() {
            return Err("a palw_class_verify_rows row names the zero class id");
        }
        if self.canonical_positions == 0 || self.leaves_per_position == 0 {
            return Err("a palw_class_verify_rows row prices no job: canonical_positions and leaves_per_position must be non-zero");
        }
        if self.a_r_ps == 0 && self.b_r_ps == 0 {
            return Err("a palw_class_verify_rows row measures a replay that costs nothing: a_r_ps or b_r_ps must be non-zero");
        }
        if self.raw_daa(self.canonical_positions) > u128::from(PALW_CLASS_VERIFY_CAP_DAA_V1) {
            return Err("a palw_class_verify_rows row's canonical deadline exceeds D_cap (PALW_CLASS_VERIFY_CAP_DAA_V1)");
        }
        Ok(())
    }
}

/// **The derived branch**: `D = ref_span_daa · palw_verification_window_spans_v1(ccu)` =
/// `5 · (⌈2·ccu / 2.4·10¹²⌉ + 1)` — the registry's own window derivation, in the reference rate's
/// spans, counted as 5 DAA each. For an attempt `ccu` is the row's `verification_ccu`, so this is
/// `5 × verification_window_spans`: floor 10, Qwen3.6 10, the 8k row 15, the 2M row 13,995.
pub fn palw_derived_verify_daa_v1(ccu: u128) -> u64 {
    let work = PalwModelWorkV1 { verification_ccu: ccu, ..Default::default() };
    PALW_CLASS_VERIFY_REF_SPAN_DAA_V1.saturating_mul(u64::from(palw_verification_window_spans_v1(&work, &PALW_REGISTRY_GLOBALS_V1)))
}

/// **The compute of the largest free-prompt run a class's context holds** — every one of its `n_ctx`
/// positions runs the body and the logits (`palw_job_breakdown_from_shape_v1` at one prefill token
/// and `n_ctx` decode tokens), on the same economic cost table the registry's `verification_ccu`
/// is derived from. Any split of a run of at most `n_ctx` positions into prefill and decode costs no
/// more, so this bounds every free-prompt claim of the class; and because the cost table prices the
/// attention over the cache per history row, it grows with `n_ctx²` for held and non-held profiles
/// alike. `None` where the profile has no economic shape (the caller then prices the claim at the
/// canonical job).
///
/// **A deviation from §4-quater.4's formula, stated.** The spec approximates this as
/// `verification_ccu · κ(n_ctx)/κ(N0)` with a knee model (and `(n_ctx/N0)²` for a non-held
/// profile). The chain stores no `N0` for a class — the canonical job is a registration fact the
/// state never kept — so the approximation cannot be evaluated by the fold; the exact function it
/// approximates can, and it is the function the class's `verification_ccu` came from.
pub fn palw_fp_class_verify_ccu_v1(profile: &crate::palw_step::PalwShapeProfileV3) -> Option<u128> {
    use crate::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_economic_shape_v1, palw_job_breakdown_from_shape_v1};
    let shape = palw_economic_shape_v1(profile, &PALW_ECONOMIC_COST_TABLE_V1).ok()?;
    Some(palw_job_breakdown_from_shape_v1(&shape, 1, profile.n_ctx.max(1)).total())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(a: u64, b: u64, fixed_ms: u64, n0: u64) -> PalwClassVerifyRowV1 {
        PalwClassVerifyRowV1 {
            class_id: Hash64::from_le_u64([7, 0, 0, 0, 0, 0, 0, 0]),
            activation_daa: 100,
            a_r_ps: a,
            b_r_ps: b,
            t_fixed_ms: fixed_ms,
            canonical_positions: n0,
            leaves_per_position: 10,
        }
    }

    /// The constants are the spec's: τ 120 s, a span 5 DAA, D_cap 16,000, m 2, the held knee 8,192,
    /// long-D past 120.
    #[test]
    fn the_constants_are_the_spec_s() {
        assert_eq!(PALW_CLASS_VERIFY_TAU_MS_V1, 120_000);
        assert_eq!(PALW_CLASS_VERIFY_REF_SPAN_DAA_V1, 5, "600 s of reference work is five 120-s DAA");
        assert_eq!(PALW_CLASS_VERIFY_CAP_DAA_V1, 16_000, "U-D3");
        assert_eq!(PALW_CLASS_VERIFY_MARGIN_V1, 2, "U-D4");
        assert_eq!(PALW_CLASS_VERIFY_HELD_N_CTX_MAX_V1, 8_192, "U-D2 (ii)");
        assert_eq!(PALW_CLASS_VERIFY_LONG_D_DAA_V1, 120);
    }

    /// T-D1's derived goldens from the registry's CONFIRMED `verification_ccu`s (K4): floor 10,
    /// Qwen3.6 10, the 8k row 15, the 2M row 13,995 — and the 2M row's `2,799` spans, which the
    /// unfixed rule counted as 2,799 DAA on testnet-12.
    #[test]
    fn the_derived_branch_is_five_daa_a_reference_span() {
        assert_eq!(palw_derived_verify_daa_v1(30_504_896), 10, "floor");
        assert_eq!(palw_derived_verify_daa_v1(339_369_880_576), 10, "Qwen3.6");
        assert_eq!(palw_derived_verify_daa_v1(1_390_562_722_816), 15, "8k");
        assert_eq!(palw_derived_verify_daa_v1(3_357_306_292_151_296), 13_995, "2M");
        let work = PalwModelWorkV1 { verification_ccu: 3_357_306_292_151_296, ..Default::default() };
        assert_eq!(palw_verification_window_spans_v1(&work, &PALW_REGISTRY_GLOBALS_V1), 2_799, "the 2M row's spans");
    }

    /// The measured branch: `⌈(m·(a·N + b·N(N−1)/2) + t_fixed) / τ⌉`, clamped to `[1, D_cap]`; `N`
    /// is `N0` for an attempt and `⌈work_leaves / λ⌉` for a free-prompt run.
    #[test]
    fn a_measured_row_prices_its_own_positions() {
        // 1 s a position, no history term, 60 s fixed: an attempt of 1,000 positions is
        // ⌈(2 · 1,000 s + 60 s) / 120 s⌉ = ⌈17.17⌉ = 18.
        let r = row(1_000_000_000_000, 0, 60_000, 1_000);
        assert_eq!(r.positions_of(PalwClaimVerifyShapeV1::Attempt), 1_000);
        assert_eq!(r.daa(1_000), 18);
        assert_eq!(r.positions_of(PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 20_001 }), 2_001, "⌈20,001 / 10⌉");
        assert_eq!(r.positions_of(PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 0 }), 1, "one position at least");
        // The quadratic term: b = 1 µs a pair, N = 4,001 → 8,002,000 pairs → 8.002 s; doubled 16.004 s.
        let q = row(0, 1_000_000, 0, 4_001);
        assert_eq!(q.raw_daa(4_001), 1, "⌈16.004 / 120⌉");
        // The clamp: a huge N is D_cap, never more; a free replay is 1, never 0.
        assert_eq!(r.daa(u64::MAX), PALW_CLASS_VERIFY_CAP_DAA_V1);
        assert!(r.raw_daa(u64::MAX) > u128::from(PALW_CLASS_VERIFY_CAP_DAA_V1));
        assert_eq!(row(1, 0, 0, 1).daa(1), 1);
        assert!(r.is_active_at(100) && !r.is_active_at(99), "active from its activation DAA");
    }

    /// A row is refused on its own when it prices nothing or its canonical job does not fit D_cap.
    #[test]
    fn a_row_is_validated_on_its_own() {
        assert_eq!(row(1_000_000_000_000, 0, 60_000, 1_000).validate(), Ok(()));
        assert!(row(0, 0, 60_000, 1_000).validate().is_err(), "a free replay");
        assert!(row(1, 0, 0, 0).validate().is_err(), "no canonical job");
        let mut zero_class = row(1, 0, 0, 1);
        zero_class.class_id = Hash64::default();
        assert!(zero_class.validate().is_err());
        // 1,000 s a position × 1,000 positions × 2 = 2,000,000 s = 16,667 DAA > D_cap.
        assert!(row(1_000_000_000_000_000, 0, 0, 1_000).validate().is_err(), "the canonical job must fit D_cap");
    }
}
