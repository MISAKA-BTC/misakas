//! ADR-0040 §16′ dynamic replica premium `π`: a bounded, chain-derived controller for the A/B reward
//! split.
//!
//! # Security model
//!
//! The split ratio is invariant under collusion economics. In a self-collusion attack (A and every
//! sybil B are the same party) the attacker collects the leaf's total value, so moving A:B does not
//! change forgery EV by one bit. The three security walls are all orthogonal to the split:
//!
//! * the reroll wall `β^m > m/(m+1)`,
//! * the escrow anchor on `c_A`,
//! * the audit wall `q·S > V`.
//!
//! The controller therefore changes supply incentives without changing the collusion payoff.
//!
//! # Manipulation resistance
//!
//! Signal forgery is constrained by using signals that carry an economic cost and by bounding each
//! adjustment. Latency within the deadline is not a signal because a
//! verifier cartel can slow-walk to just inside the deadline for free, manufacturing fake scarcity at
//! zero cost. The two signals used here both cost the forger:
//!
//! * `r` (shortfall rate) can only be raised by *actually* not delivering — which forfeits the
//!   forger's own B revenue and triggers the objective no-show penalty (scaled by `ζ` against the
//!   premium itself, so pumping the dial is always EV-negative).
//! * `I` (allocation intensity) can only be lowered by injecting bond — but beacon assignment is
//!   `∝ bond`, so the injector gets drawn as B and must either complete the work or no-show into `r`.
//!
//! Demand-side forgery (job flooding) is not forgery at all: challenge-in-context + escrow + replica
//! cost mean jobs can only be manufactured by doing real work. That *is* demand, and the controller
//! should respond to it.
//!
//! The `π_min` and `π_max` bounds keep both roles above their participation thresholds.
//!
//! # Neutral point
//!
//! At `π = 1`, each of the `1 + m` roles receives `1/(1+m)` of the split. For `m = 1`, the integer
//! arithmetic matches `a = base/2; b = base − a`; see `neutral_pi_is_byte_identical_to_half`. Provider
//! B is beacon-assigned in proportion to bond and cannot self-select.
//!
//! # Determinism discipline
//!
//! Windows are cut on DAA score (pruning-invariant and viewpoint-independent), not on
//! selected-chain index, wall clock, or header-declared time. Every quantity here is integer
//! basis-point arithmetic with round-to-nearest-**even** (`div_rne`), matching the integer discipline
//! ADR-0040 §3.3 imposes on the model itself. Nothing is written to a header; every node re-derives the
//! same `π` from the same finalized inputs, exactly like difficulty.

use borsh::{BorshDeserialize, BorshSerialize};

/// One unit of `π` and of every ratio here: 10 000 bps = 1.0.
pub const PALW_PREMIUM_BPS_ONE: u32 = 10_000;

/// ADR-0040 §16′ — the GOVERNED range for the replica premium π, in basis points.
///
/// These bound what the controller may ever propose. They are exposed as constants (rather than
/// living only inside [`PalwPremiumParams::genesis_defaults`]) because the consensus seam that derives
/// π for a reward class has to clamp against them, and that seam has no `PalwPremiumParams` in scope —
/// π is not a `Params` field, it is epoch state.
///
/// `PALW_PREMIUM_BPS_ONE` must lie inside the range, or the neutral point itself would be clamped and
/// the "inert by default" property would silently stop holding. That is pinned by
/// `premium_range_contains_the_neutral_point`.
pub const PALW_PREMIUM_PI_MIN_BPS: u32 = 5_000; // 0.5 ⇒ σ_A = 0.667 at m = 1
pub const PALW_PREMIUM_PI_MAX_BPS: u32 = 30_000; // 3.0 ⇒ σ_A = 0.25  at m = 1

/// Hard cap on `maturation_windows`, so the rate-limiter ring stays bounded (and borsh-bounded).
pub const PALW_PREMIUM_MAX_MATURATION_WINDOWS: u32 = 64;

/// Round-to-nearest, ties-to-even integer division. Used for every ratio so the controller cannot
/// accumulate a directional rounding drift (round-half-up would bias `π` upward over many windows).
///
/// Panics on `den == 0` — callers guard, and a zero denominator is a caller bug, not an input.
pub fn div_rne(num: u128, den: u128) -> u128 {
    assert!(den != 0, "div_rne: zero denominator");
    let q = num / den;
    let r = num % den;
    let twice = r * 2;
    if twice > den || (twice == den && q % 2 == 1) { q + 1 } else { q }
}

/// The controller's frozen parameters (ADR-0040 §16′ parameter table). Calibrated in the S phases;
/// `genesis_defaults()` are the pre-calibration values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPremiumParams {
    /// Control window length in DAA score (≈ 1 day: BPS × 86 400).
    pub window_daa: u64,
    /// EMA smoothing factor in bps. Derived OFF-CHAIN from the half-life `H` as
    /// `α = 1 − 2^(−1/H)` and frozen here, because deriving a fractional power on-chain would
    /// reintroduce exactly the floating-point non-determinism this design forbids.
    pub ema_alpha_bps: u32,
    /// Up/down step per window, in bps of the *current* `π`. Asymmetric on purpose: respond fast to
    /// scarcity, slowly to glut.
    pub kappa_up_bps: u32,
    pub kappa_down_bps: u32,
    /// Consecutive qualifying windows required before any move (debounces one-off shocks such as a
    /// single large batch).
    pub consecutive_windows: u32,
    /// Rate limiter: cumulative |Δπ| over the last `maturation_windows` must not exceed `delta_cap_bps`.
    /// Stability rule: supply cannot respond faster than bond maturation `k`, so the
    /// controller must not be able to travel further than that dead time allows, or it will overshoot.
    pub delta_cap_bps: u32,
    pub maturation_windows: u32,
    /// Floors/ceilings on `π` (participation floors for both roles).
    pub pi_min_bps: u32,
    pub pi_max_bps: u32,
    /// Shortfall band. Above `r_hi` ⇒ overload ⇒ raise; below `r_lo` (and `I` low) ⇒ glut ⇒ lower.
    /// The band itself is the deadband — no separate ε is needed.
    pub r_lo_bps: u32,
    pub r_hi_bps: u32,
    /// Allocation-intensity floor: only treat "low shortfall" as glut when work per bond is also thin.
    pub i_lo_bps: u32,
    /// Thin-market guard: below this many ordered slot-CU in a window, hold (the sample cannot
    /// distinguish signal from Poisson noise).
    pub n_stat_slot_cu: u128,
    /// Bootstrap: force `hold` for this many windows from genesis, while there is no data.
    pub bootstrap_hold_windows: u32,
    /// **`P₀`, the flat no-show floor, in bps of `V̄_leaf`** (ADR-0040 §16′-3).
    ///
    /// Must sit inside the two-sided window derived by [`Self::p0_window_bps`]. It is not a nuisance
    /// fee; it is the load-bearing term that makes a rotating-sybil pump EV-negative.
    pub p0_bps_of_leaf: u32,
    /// **Eclipse tolerance**: the largest `P₀` (bps of `V̄_leaf`) a single no-show may cost.
    ///
    /// An eclipse VICTIM is not at fault and is indistinguishable from a deliberate no-show, so one
    /// event must be survivable. This is the hard ceiling regardless of what the pump math wants.
    pub p0_max_bps_eclipse: u32,
    /// The no-load shortfall baseline `r₀` in bps (an S2 measurement; `r_lo`/`r_hi` are multiples of it).
    /// Used to price what an HONEST provider expects to pay through no fault of its own.
    pub r0_baseline_bps: u32,
    /// The largest expected no-show cost an honest provider may carry at `r₀`, in bps of `V̄_leaf`.
    /// Harmlessness: honest operation must not be quietly taxed.
    pub honest_penalty_harmless_bps: u32,
}

impl PalwPremiumParams {
    /// ADR-0040 §16′ parameter table — genesis defaults, to be replaced by S-phase measurements.
    /// `window_daa` is caller-supplied because it depends on the net's BPS.
    pub const fn genesis_defaults(window_daa: u64) -> Self {
        Self {
            window_daa,
            // half-life H = 5 windows ⇒ α = 1 − 2^(−1/5) ≈ 0.1294.
            ema_alpha_bps: 1_294,
            kappa_up_bps: 150,  // 1.5 %/window
            kappa_down_bps: 75, // 0.75 %/window
            consecutive_windows: 2,
            delta_cap_bps: 1_000, // 10 %
            maturation_windows: 14,
            pi_min_bps: PALW_PREMIUM_PI_MIN_BPS,
            pi_max_bps: PALW_PREMIUM_PI_MAX_BPS,
            // r₀ (no-load shortfall baseline) is an S2 measurement; 3 % is the placeholder.
            r_lo_bps: 450,       // 1.5 × r₀
            r_hi_bps: 900,       // 3   × r₀
            i_lo_bps: 5_000,     // 0.5 × target draws/bond
            n_stat_slot_cu: 530, // ≈ 16/r₀ slots at r₀ = 3 % ⇒ σ(r) ≤ r₀/4
            bootstrap_hold_windows: 30,
            // ADR-0040 §16′-3: 3.5 % of a leaf ⇒ 1.47× margin over the pump's gain ceiling. The bare
            // requirement is 2.38 %; the margin absorbs calibration error in Δσ_B and V̄_leaf.
            p0_bps_of_leaf: 350,
            p0_max_bps_eclipse: 1_000,       // one no-show costs at most 10 % of a leaf
            r0_baseline_bps: 300,            // r₀ = 3 % (S2 placeholder; r_lo/r_hi above are 1.5×/3×)
            honest_penalty_harmless_bps: 15, // honest expectation ≤ 0.15 % of a leaf
        }
    }

    /// **ADR-0040 §16′-3 — the two-sided `P₀` window, derived from the CURRENT parameters.**
    ///
    /// Returns `(exclusive_lower, inclusive_upper)` in bps of `V̄_leaf`. Nothing here is hardcoded: change
    /// `Δ_cap`, `κ_up`, `M_mat`, `r₀` or either tolerance and the window moves with them, so a future
    /// parameter change that breaks the equilibrium fails CI instead of silently weakening it.
    ///
    /// * **Lower** (pump defeat): `Δσ_B · (M − W) / W`, `W = ⌈Δ_cap/κ_up⌉` — see
    ///   [`Self::flat_model_pump_is_ev_negative`]. Below this a rotating-sybil pump is profitable.
    /// * **Upper** is the tighter of two independent ceilings:
    ///   - *eclipse tolerance* — one no-show must not ruin a blameless victim;
    ///   - *harmlessness* — at the honest baseline `r₀`, expected cost stays within
    ///     `honest_penalty_harmless_bps`, i.e. `P₀ ≤ harmless · 10000 / r₀`.
    ///
    /// The two pressures are independent, so they can conflict. When they do, the right outcome is a
    /// stopped build, not an arbitrary pick — see [`Self::p0_window_is_non_empty`].
    pub fn p0_window_bps(&self) -> (u32, u32) {
        let one = PALW_PREMIUM_BPS_ONE as u128;
        let w = self.delta_cap_bps.div_ceil(self.kappa_up_bps) as u128;
        let lower = if w == 0 || self.maturation_windows as u128 <= w {
            0 // the pump cannot outlast its own cost window; no floor needed
        } else {
            let pi_pumped = one + self.delta_cap_bps as u128;
            let sigma_b_pumped = pi_pumped * one / (one + pi_pumped);
            let delta_sigma = sigma_b_pumped.saturating_sub(one / 2);
            (delta_sigma * (self.maturation_windows as u128 - w) / w) as u32
        };
        let harmless_ceiling = if self.r0_baseline_bps == 0 {
            u32::MAX
        } else {
            (self.honest_penalty_harmless_bps as u128 * one / self.r0_baseline_bps as u128) as u32
        };
        (lower, self.p0_max_bps_eclipse.min(harmless_ceiling))
    }

    /// **Is the `P₀` window non-empty?** i.e. does a valid `P₀` exist at all for these parameters?
    ///
    /// This is the meta-property worth having: it asserts that the parameter SPACE is inhabited, not
    /// merely that the current point happens to be legal. If a future calibration pushes the pump floor
    /// above the eclipse/harmlessness ceiling, there is no safe `P₀` — and the correct response is to
    /// stop and rebalance `Δ_cap`/`κ`/`M`, never to quietly pick a side.
    pub fn p0_window_is_non_empty(&self) -> bool {
        let (lo, hi) = self.p0_window_bps();
        lo < hi
    }

    /// Does the configured `P₀` sit inside its window?
    pub fn p0_is_calibrated(&self) -> bool {
        let (lo, hi) = self.p0_window_bps();
        self.p0_bps_of_leaf > lo && self.p0_bps_of_leaf <= hi
    }

    /// Returns whether a premium pump is EV-negative under the flat cost model.
    ///
    /// # Flat-cost requirement
    ///
    /// `esc(k)` escalates per credential, so a rotating sybil cartel with many minimum-bond credentials
    /// can spread no-shows so every event is the first for its credential and pays `k = 0`. Minting
    /// credentials costs only the minimum-bond granularity,
    /// which is negligible for a cartel that already holds β. The equilibrium therefore requires
    /// `ζ` and `P₀` alone to make the pump unprofitable; `esc` is additional protection against a
    /// non-rotating attacker.
    ///
    /// # The condition
    ///
    /// A pump must sustain shortfall for `W = ⌈Δ_cap / κ_up⌉` windows, then enjoys the raised `σ_B` for
    /// the remaining `M − W` windows of the maturation horizon. The cartel's assignment volume cancels
    /// (it no-shows its own slots, then earns on its own slots), leaving a per-slot comparison:
    ///
    /// ```text
    /// cost = P₀ · W              gain = Δσ_B · V̄_leaf · (M − W)
    /// require  P₀ > Δσ_B · (M − W) / W          [all in bps of V̄_leaf]
    /// ```
    ///
    /// `ζ·(π−1)⁺` is excluded from `cost`: during the pump `π` is still near neutral,
    /// so that term is ≈ 0 exactly when it is needed. It only starts paying after the dial has already
    /// moved — which is why `P₀`, not `ζ`, is what has to carry this.
    pub fn flat_model_pump_is_ev_negative(&self) -> bool {
        let one = PALW_PREMIUM_BPS_ONE as u128;
        let w = self.delta_cap_bps.div_ceil(self.kappa_up_bps) as u128;
        if w == 0 || self.maturation_windows as u128 <= w {
            return true; // the pump cannot outlast its own cost window
        }
        let pi_pumped = one + self.delta_cap_bps as u128;
        let sigma_b_pumped = pi_pumped * one / (one + pi_pumped);
        let delta_sigma = sigma_b_pumped.saturating_sub(one / 2);
        let required_p0_bps = delta_sigma * (self.maturation_windows as u128 - w) / w;
        self.p0_bps_of_leaf as u128 > required_p0_bps
    }

    /// Does `κ_up · M_mat ≤ Δ_cap` hold — i.e. is the per-window step bound *alone* already enough to
    /// keep travel over the maturation horizon inside the cap?
    ///
    /// This is the design's stated stability condition, but it is **not** required for safety, and the
    /// genesis defaults deliberately do not satisfy it (1.5 % × 14 = 21 % > 10 %). Two regimes are both
    /// sound, and they differ only in which mechanism binds:
    ///
    /// * **structural regime** (`κ·M ≤ Δ_cap`): steps are small enough that the ring-buffer limiter is
    ///   pure belt-and-braces and never fires;
    /// * **limiter regime** (`κ·M > Δ_cap`, the genesis defaults): the limiter actively binds after
    ///   ⌈Δ_cap/κ⌉ ≈ 7 windows, which is *tighter* control, not looser.
    ///
    /// Total travel over any maturation horizon is bounded by `Δ_cap` in BOTH regimes, because the ring
    /// buffer enforces it directly. Requiring the structural regime would have forced either a 21 % cap
    /// or a 0.71 %/window step — i.e. it would have loosened the real bound in order to satisfy a
    /// sufficient-but-unnecessary condition.
    pub fn step_bound_is_structural(&self) -> bool {
        self.kappa_up_bps.saturating_mul(self.maturation_windows) <= self.delta_cap_bps
    }

    /// Structural consistency. Note what is deliberately absent: `κ_up · M_mat ≤ Δ_cap` is NOT required
    /// (see [`Self::step_bound_is_structural`]). What IS required is that the cap leaves room for at
    /// least one step at the smallest premium — otherwise the rate limiter would freeze the controller
    /// permanently at its initial value, which is a silent failure rather than a safe one.
    pub fn is_consistent(&self) -> bool {
        let min_step_bps = div_rne(self.pi_min_bps as u128 * self.kappa_up_bps as u128, PALW_PREMIUM_BPS_ONE as u128) as u32;
        self.delta_cap_bps >= min_step_bps.max(1)
            // ADR-0040 §16′-3: the calibration is machine-checked, not prose — on BOTH sides, plus the
            // meta-condition that a valid P₀ exists at all. A parameter set whose P₀ leaves a
            // rotating-sybil pump profitable, or which taxes honest providers, or for which NO P₀ can
            // satisfy both, is not a valid parameter set.
            && self.p0_window_is_non_empty()
            && self.p0_is_calibrated()
            && self.is_consistent_inner()
    }

    fn is_consistent_inner(&self) -> bool {
        self.window_daa > 0
            && self.ema_alpha_bps > 0
            && self.ema_alpha_bps <= PALW_PREMIUM_BPS_ONE
            && self.kappa_up_bps > 0
            && self.kappa_down_bps > 0
            && self.consecutive_windows > 0
            && self.maturation_windows > 0
            && self.maturation_windows <= PALW_PREMIUM_MAX_MATURATION_WINDOWS
            && self.pi_min_bps > 0
            && self.pi_min_bps <= PALW_PREMIUM_BPS_ONE
            && self.pi_max_bps >= PALW_PREMIUM_BPS_ONE
            && self.r_lo_bps < self.r_hi_bps
            && self.n_stat_slot_cu > 0
    }
}

/// The two per-window measurements (ADR-0040 §16′ §2), both one-sided.
///
/// **Cohort accounting**: each `A_commit` belongs to the window in which it was ACCEPTED, and that
/// cohort is only evaluated at `close(w) + L` (L = B deadline + reveal window + finality depth). This
/// removes the growth-phase bias that a naive per-window demand/supply ratio suffers, where demand
/// lands in window `w` but delivery lands in `w+1`, and it closes the "a slack deadline hides
/// saturation" blind spot — the deadline is the instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwWindowSample {
    /// Total ordered replica-slot CU for the cohort.
    pub ordered_slot_cu: u128,
    /// Slot CU that missed the deadline (no-show / reroll). Numerator of `r`.
    pub failed_slot_cu: u128,
    /// Effective bonded amount in the window's frozen snapshot. Denominator of `I`.
    pub snapshot_bond_sompi: u128,
}

impl PalwWindowSample {
    /// Shortfall rate `r` in bps — the OVERLOAD signal (meaningful only when rising).
    pub fn shortfall_bps(&self) -> u32 {
        if self.ordered_slot_cu == 0 {
            return 0;
        }
        let capped = self.failed_slot_cu.min(self.ordered_slot_cu);
        div_rne(capped * PALW_PREMIUM_BPS_ONE as u128, self.ordered_slot_cu) as u32
    }

    /// Allocation intensity `I` in bps — the GLUT signal (work per unit bond thinning out).
    pub fn intensity_bps(&self) -> u32 {
        if self.snapshot_bond_sompi == 0 {
            return 0;
        }
        div_rne(self.ordered_slot_cu * PALW_PREMIUM_BPS_ONE as u128, self.snapshot_bond_sompi).min(u32::MAX as u128) as u32
    }
}

/// What the controller decided in a window. Surfaced so telemetry can publish the `(r, I, π)` series
/// alongside the bond-concentration series (ADR-0040 §5.3 uses the same "chain tightens itself" frame).
///
/// # Every hold carries a reason, on purpose
///
/// Freezing is the safe behaviour, but **an unobserved freeze is indistinguishable from a fault**. A
/// controller stuck at its initial value because of a misconfigured cap looks exactly like a controller
/// correctly holding inside the deadband, unless the reason is published. So each variant below is a
/// distinct telemetry reason code, and [`PalwPremiumDecision::is_hold`] lets an operator alert on
/// "held for N consecutive windows" without having to infer it from an unchanging `π`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPremiumDecision {
    /// Degraded/health mode owns the levers; the split controller must not fight it (mode separation).
    HoldDegraded,
    /// Fewer than `n_stat_slot_cu` ordered — the sample cannot beat Poisson noise.
    HoldThinMarket,
    /// Still inside `bootstrap_hold_windows`.
    HoldBootstrap,
    /// Inside the `[r_lo, r_hi]` band, or the consecutive-window condition is unmet.
    HoldDeadband,
    /// The rate limiter refused the move (cumulative |Δπ| over the maturation horizon).
    HoldRateLimited,
    Raised {
        from_bps: u32,
        to_bps: u32,
    },
    Lowered {
        from_bps: u32,
        to_bps: u32,
    },
}

impl PalwPremiumDecision {
    /// Did the controller decline to move? Distinguishes a *held* window from a *moved* one without
    /// having to compare `π` before and after (which cannot tell a clamped move from a refused one).
    pub fn is_hold(&self) -> bool {
        !matches!(self, Self::Raised { .. } | Self::Lowered { .. })
    }

    /// Stable, low-cardinality reason code for telemetry. **Publish this every window**, not only on
    /// change: ADR-0040 §16′ requires the `(r, I, π)` series to carry the frozen state with its reason,
    /// because a silent freeze is safe but unobservable, and an unobservable freeze cannot be told apart
    /// from a fault.
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::HoldDegraded => "hold/degraded",
            Self::HoldThinMarket => "hold/thin-market",
            Self::HoldBootstrap => "hold/bootstrap",
            Self::HoldDeadband => "hold/deadband",
            Self::HoldRateLimited => "hold/rate-limited",
            Self::Raised { .. } => "raised",
            Self::Lowered { .. } => "lowered",
        }
    }
}

/// kaspa-pq **ADR-0040 §16′-3** — which `π` the no-show penalty references.
///
/// This is the one place the two `π` readings are easy to confuse, and getting it backwards silently
/// removes the anti-pump property:
///
/// * A **leaf's payout** uses the π frozen at its COMMIT window (`WorkRewardClass::ReplicaPalw
///   .premium_pi_bps`). A reroll does not re-date it — the leaf keeps its original commit window's π,
///   so a producer cannot re-aim the split by forcing a reroll into a more favourable window.
/// * A **no-show penalty** must use the **live π at the event**, because the live value is what the
///   pump is trying to inflate. Charging the frozen value would let a cartel raise π and pay penalties
///   priced at the pre-pump level — i.e. the ring buffer would bound the exploit but never make it
///   negative, and only ζ makes it negative.
///
/// `penalty ≥ ζ · (π_live − 1)⁺ · expected_assignment_revenue`, floored at the flat `P₀` and escalating
/// on repeat offence (both unchanged from the base penalty schedule).
pub fn no_show_penalty_floor_sompi(
    live_pi_bps: u32,
    expected_assignment_revenue_sompi: u128,
    zeta: u32,
    flat_floor_sompi: u128,
) -> u128 {
    let excess_bps = live_pi_bps.saturating_sub(PALW_PREMIUM_BPS_ONE) as u128;
    let scaled = div_rne(zeta as u128 * excess_bps * expected_assignment_revenue_sompi, PALW_PREMIUM_BPS_ONE as u128);
    scaled.max(flat_floor_sompi)
}

/// ADR-0040 §16′-3 — repeat-offence escalation `esc(k) = μ^min(k, k_cap)`.
///
/// `k` is the offender's no-show count over the trailing `M_esc` windows (a decaying ring, so an old
/// offence eventually stops counting — a permanent record would make a single eclipse event a life
/// sentence). Capped at `k_cap` because an uncapped geometric term overflows long before it deters
/// anything, and because past some multiple the bond is exhausted anyway.
pub fn no_show_escalation(mu: u32, k: u32, k_cap: u32) -> u128 {
    (mu as u128).saturating_pow(k.min(k_cap))
}

/// ADR-0040 §16′-3 — the full no-show penalty: `max(P₀, ζ·(π_live − 1)⁺·R̂) · esc(k)`.
///
/// # The three components do different jobs
///
/// * **`P₀`** keeps a no-show from being free even at the neutral premium — it prices the reroll
///   externality (re-draw + delay). Kept small enough not to ruin an eclipse VICTIM, who is not at
///   fault and whose only tell is indistinguishable from a deliberate no-show.
/// * **the ζ term** makes pumping EV-negative. The ring buffer bounds how much a cartel can extract by
///   faking scarcity; only ζ makes that extraction *negative*.
/// * **`esc`** carries repeat offenders, so a cartel cannot amortise a pump across many cheap events.
///
/// # `R̂` must be priced at the LIVE premium
///
/// `expected_assignment_revenue_sompi` = `σ_B(π_live, m) · V̄_leaf`. Using the leaf's FROZEN π here
/// would be the one subtle way to break the whole mechanism: the cartel inflates π, then pays penalties
/// priced at the pre-pump level. Every input to this function is a live-at-event quantity.
pub fn no_show_penalty_sompi(
    live_pi_bps: u32,
    expected_assignment_revenue_sompi: u128,
    zeta: u32,
    flat_floor_sompi: u128,
    mu: u32,
    k: u32,
    k_cap: u32,
) -> u128 {
    no_show_penalty_floor_sompi(live_pi_bps, expected_assignment_revenue_sompi, zeta, flat_floor_sompi)
        .saturating_mul(no_show_escalation(mu, k, k_cap))
}

/// ADR-0040 §16′-3 — where a no-show penalty is collected FROM, and where it goes TO.
///
/// # From the provider's BOND, never from escrow
///
/// Escrow is A's asset posted against A's own obligations; it is not collateral for B's fault. Draining
/// escrow for a B no-show would let a malicious B burn A's money.
///
/// # To burn or the audit budget, and NEVER to A
///
/// This is the non-obvious half. Routing the penalty to A would pay A for B's failure — which is a
/// direct subsidy for **eclipse-grinding**: A silences an honest B (or simply reports it silent) and
/// collects, then earns a reroll on top. A's legitimate remedy is the escrow-neutral reroll plus delay
/// compensation, and B's punishment must not be the funding source for A's compensation. Keeping the
/// two flows unlinked is what stops "harm your counterparty" from becoming a strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoShowPenaltyDestination {
    /// Removed from supply.
    Burn,
    /// Credited to the epoch's audit budget (funds the very auditors who detect the next one).
    AuditBudget,
}

/// The outcome of charging a no-show penalty against a provider bond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoShowCharge {
    /// Actually collected (bounded by the bond — a bond cannot go negative).
    pub collected_sompi: u128,
    /// Assessed before the bond bound; `assessed > collected` means the bond was exhausted.
    pub assessed_sompi: u128,
    /// Bond exhausted ⇒ the credential is Suspended (excluded from selection until re-bonded). A
    /// provider with a drained bond is unslashable, and an unslashable provider must not be drawn.
    pub suspended: bool,
    pub destination: NoShowPenaltyDestination,
}

/// ADR-0040 §16′-3 — charge a no-show penalty against `bond_sompi`.
pub fn charge_no_show(bond_sompi: u128, assessed_sompi: u128, destination: NoShowPenaltyDestination) -> NoShowCharge {
    let collected = assessed_sompi.min(bond_sompi);
    NoShowCharge { collected_sompi: collected, assessed_sompi, suspended: collected >= bond_sompi, destination }
}

/// The controller's persisted state: one scalar `π` plus the smoothing/debounce/rate-limit bookkeeping.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPremiumStateV1 {
    pub version: u16,
    /// The replica premium, in bps. Neutral = [`PALW_PREMIUM_BPS_ONE`].
    pub pi_bps: u32,
    pub r_ema_bps: u32,
    pub i_ema_bps: u32,
    pub consecutive_up: u32,
    pub consecutive_down: u32,
    pub windows_elapsed: u32,
    /// Ring of per-window |Δπ| in bps over the maturation horizon (oldest first, len ≤ maturation).
    pub recent_delta_bps: Vec<u32>,
    /// Whether the EMAs have been seeded (first sample seeds rather than blends toward zero).
    pub ema_seeded: bool,
}

impl PalwPremiumStateV1 {
    /// Genesis: neutral premium, no history. Reproduces the pre-controller split exactly.
    pub fn neutral() -> Self {
        Self {
            version: 1,
            pi_bps: PALW_PREMIUM_BPS_ONE,
            r_ema_bps: 0,
            i_ema_bps: 0,
            consecutive_up: 0,
            consecutive_down: 0,
            windows_elapsed: 0,
            recent_delta_bps: Vec::new(),
            ema_seeded: false,
        }
    }

    fn cumulative_delta_bps(&self) -> u32 {
        self.recent_delta_bps.iter().copied().fold(0u32, u32::saturating_add)
    }

    fn push_delta(&mut self, delta_bps: u32, maturation_windows: u32) {
        self.recent_delta_bps.push(delta_bps);
        while self.recent_delta_bps.len() > maturation_windows as usize {
            self.recent_delta_bps.remove(0);
        }
    }
}

/// Integer EMA: `new = old + α·(sample − old)`, with RNE rounding and no directional drift.
fn ema_step(old: u32, sample: u32, alpha_bps: u32) -> u32 {
    let one = PALW_PREMIUM_BPS_ONE as u128;
    let a = alpha_bps as u128;
    div_rne(old as u128 * (one - a) + sample as u128 * a, one) as u32
}

/// ADR-0040 §16′ §3 — the window-boundary update. Pure: same inputs ⇒ same output on every node.
///
/// `degraded` is passed in by the caller because **health mode owns the levers during a crisis**
/// (§5.3 raises `m`/`q`, lowers the mint cap, extends maturation). The split controller freezes then,
/// so the two mechanisms never fight over the same signal.
pub fn update_premium(
    state: &PalwPremiumStateV1,
    sample: &PalwWindowSample,
    params: &PalwPremiumParams,
    degraded: bool,
) -> (PalwPremiumStateV1, PalwPremiumDecision) {
    let mut next = state.clone();
    next.windows_elapsed = state.windows_elapsed.saturating_add(1);

    // Mode separation: never move while the health mechanism is steering.
    if degraded {
        next.consecutive_up = 0;
        next.consecutive_down = 0;
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldDegraded);
    }

    // Thin-market guard BEFORE folding the sample into the EMAs: a sub-resolution window carries no
    // information, so letting it move the average would launder noise into the signal.
    if sample.ordered_slot_cu < params.n_stat_slot_cu {
        next.consecutive_up = 0;
        next.consecutive_down = 0;
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldThinMarket);
    }

    let (r, i) = (sample.shortfall_bps(), sample.intensity_bps());
    if state.ema_seeded {
        next.r_ema_bps = ema_step(state.r_ema_bps, r, params.ema_alpha_bps);
        next.i_ema_bps = ema_step(state.i_ema_bps, i, params.ema_alpha_bps);
    } else {
        next.r_ema_bps = r;
        next.i_ema_bps = i;
        next.ema_seeded = true;
    }

    // Bootstrap: measure, but do not act.
    if next.windows_elapsed <= params.bootstrap_hold_windows {
        next.consecutive_up = 0;
        next.consecutive_down = 0;
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldBootstrap);
    }

    // Debounce: the band [r_lo, r_hi] IS the deadband; `C` consecutive qualifying windows are required.
    let wants_up = next.r_ema_bps > params.r_hi_bps;
    let wants_down = next.r_ema_bps < params.r_lo_bps && next.i_ema_bps < params.i_lo_bps;
    next.consecutive_up = if wants_up { state.consecutive_up.saturating_add(1) } else { 0 };
    next.consecutive_down = if wants_down { state.consecutive_down.saturating_add(1) } else { 0 };

    let direction_up = next.consecutive_up >= params.consecutive_windows;
    let direction_down = next.consecutive_down >= params.consecutive_windows;
    if !direction_up && !direction_down {
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldDeadband);
    }

    // Proposed multiplicative step, clamped to the participation floors.
    let one = PALW_PREMIUM_BPS_ONE as u128;
    let cur = state.pi_bps as u128;
    let proposed = if direction_up {
        div_rne(cur * (one + params.kappa_up_bps as u128), one).min(params.pi_max_bps as u128)
    } else {
        div_rne(cur * (one - params.kappa_down_bps as u128), one).max(params.pi_min_bps as u128)
    } as u32;

    let delta = proposed.abs_diff(state.pi_bps);
    if delta == 0 {
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldDeadband);
    }

    // The rate limiter. Supply cannot respond faster than bond maturation, so the controller must not
    // travel further than that dead time allows — this is what prevents overshoot, not the step size.
    if state.cumulative_delta_bps().saturating_add(delta) > params.delta_cap_bps {
        next.push_delta(0, params.maturation_windows);
        return (next, PalwPremiumDecision::HoldRateLimited);
    }

    next.pi_bps = proposed;
    next.push_delta(delta, params.maturation_windows);
    // A realised move resets its own debounce counter, so a sustained trend steps once per `C` windows
    // rather than every window.
    if direction_up {
        next.consecutive_up = 0;
        (next, PalwPremiumDecision::Raised { from_bps: state.pi_bps, to_bps: proposed })
    } else {
        next.consecutive_down = 0;
        (next, PalwPremiumDecision::Lowered { from_bps: state.pi_bps, to_bps: proposed })
    }
}

/// ADR-0040 §16′ — the split itself: `A` has weight 1 and each of the `m` replicas has weight `π`.
///
/// ```text
/// σ_A = 1/(1 + m·π)      σ_B = π/(1 + m·π)
/// ```
///
/// Returns `(a_sompi, per_b_sompi, remainder_to_last_b)` such that `a + Σb == base` exactly — the
/// remainder goes to the last replica so the sum is conserved, mirroring the previous
/// `b = base − a` convention.
///
/// **At `π = 1` this is exactly `1/(1+m)` each, and for `m = 1` it reproduces the previous
/// `a = base/2; b = base − a` byte for byte.** That is what makes the controller inert-by-default: a
/// net that never leaves the neutral point pays precisely what it paid before.
pub fn premium_split(base_sompi: u64, replica_count: u16, pi_bps: u32) -> (u64, u64, u64) {
    let m = replica_count.max(1) as u128;
    let one = PALW_PREMIUM_BPS_ONE as u128;
    let denom = one + m * pi_bps as u128;
    // Floor for A (not RNE): the remainder is explicitly redistributed below, so flooring here keeps
    // `a + Σb == base` exact without a second correction pass.
    let a = (base_sompi as u128 * one / denom) as u64;
    let b_total = base_sompi - a;
    let per_b = (b_total as u128 / m) as u64;
    let remainder = b_total - per_b * (m as u64);
    (a, per_b, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> PalwPremiumParams {
        PalwPremiumParams::genesis_defaults(864_000)
    }

    /// **ECON-NEUTRALITY CONFORMANCE VECTOR** (ADR-0040 §16′ / named CI conformance).
    ///
    /// A net sitting at the neutral point must pay EXACTLY what the fixed 50/50 split paid. This is not
    /// a one-off migration check — it is a permanent named vector, on the same footing as the model's
    /// canonical vectors, so that any future refactor which breaks the neutral point's byte identity
    /// fails under a name that says what was broken rather than as an opaque arithmetic diff.
    ///
    /// If this test ever needs "updating", that is the signal to stop: the controller is no longer inert
    /// by default and the §16 equal-split promise has silently become a different promise.
    #[test]
    fn econ_neutrality_vector_neutral_pi_is_byte_identical_to_half() {
        for base in [0u64, 1, 2, 3, 7, 100, 999, 1_000_000_007, u64::MAX / 4, u64::MAX / 2] {
            let (a, per_b, rem) = premium_split(base, 1, PALW_PREMIUM_BPS_ONE);
            assert_eq!(a, base / 2, "A must match the pre-controller `base/2` at neutral (base={base})");
            assert_eq!(per_b + rem, base - base / 2, "B must match the pre-controller `base - a`");
            assert_eq!(a + per_b + rem, base, "the split must conserve the base exactly");
        }
    }

    /// Neutral generalises to an equal m+1-way split — §16's principle, not just its m=1 instance.
    #[test]
    fn neutral_pi_is_equal_split_for_any_m() {
        let base = 1_000_000u64;
        for m in 1..=8u16 {
            let (a, per_b, rem) = premium_split(base, m, PALW_PREMIUM_BPS_ONE);
            let expected = base / (1 + m as u64);
            assert_eq!(a, expected, "m={m}: A must get 1/(1+m)");
            // every participant within one sompi of every other
            assert!(a.abs_diff(per_b) <= 1, "m={m}: A={a} per_b={per_b} must be equal up to rounding");
            assert_eq!(a + per_b * m as u64 + rem, base, "m={m}: conservation");
        }
    }

    #[test]
    fn split_conserves_base_and_moves_monotonically_with_pi() {
        let base = 1_000_000u64;
        let mut last_a = u64::MAX;
        for pi in [5_000u32, 7_500, 10_000, 15_000, 20_000, 30_000] {
            let (a, per_b, rem) = premium_split(base, 1, pi);
            assert_eq!(a + per_b + rem, base, "conservation at pi={pi}");
            assert!(a < last_a, "raising the replica premium must strictly lower A's share (pi={pi})");
            last_a = a;
        }
        // the floors bound A's share to the participation band at m = 1
        assert_eq!(premium_split(base, 1, 5_000).0, 666_666); // σ_A ≈ 0.667
        assert_eq!(premium_split(base, 1, 30_000).0, 250_000); // σ_A = 0.25
    }

    #[test]
    fn div_rne_is_ties_to_even_and_drift_free() {
        assert_eq!(div_rne(5, 2), 2, "2.5 → 2 (even)");
        assert_eq!(div_rne(7, 2), 4, "3.5 → 4 (even)");
        assert_eq!(div_rne(4, 2), 2);
        assert_eq!(div_rne(3, 2), 2, "1.5 → 2 (even)");
        assert_eq!(div_rne(1, 2), 0, "0.5 → 0 (even)");
        // Round-half-up would bias upward across many ties; RNE must not.
        let ties_up: u128 = (0..100).map(|k| div_rne(2 * k + 1, 2)).sum();
        let exact: u128 = (0..100).map(|k| 2 * k + 1).sum::<u128>() / 2;
        assert!(ties_up.abs_diff(exact) <= 25, "RNE must not accumulate a directional drift");
    }

    /// The genesis defaults run in the LIMITER regime, and that is deliberate. `κ·M ≤ Δ_cap` is a
    /// sufficient-but-unnecessary condition: demanding it would have forced either a 21 % cap or a
    /// 0.71 %/window step, i.e. it would have LOOSENED the real bound to satisfy a stricter-looking one.
    /// Travel over the maturation horizon is bounded by `Δ_cap` in both regimes — the ring buffer, not
    /// the step size, is what enforces it.
    #[test]
    fn genesis_params_are_consistent_and_run_in_the_limiter_regime() {
        let p = params();
        assert!(p.is_consistent());
        assert!(!p.step_bound_is_structural(), "1.5 % × 14 = 21 % > 10 % ⇒ the limiter binds (intended)");
        // ...and the limiter binds after roughly Δ_cap/κ windows, which must be > 1 (else it is a freeze).
        let windows_to_bind = p.delta_cap_bps / p.kappa_up_bps;
        assert!(windows_to_bind >= 2, "the cap must permit more than a single step before binding");

        // A parameter set that IS structural is equally consistent — both regimes are admissible.
        assert!(PalwPremiumParams { maturation_windows: 6, ..p }.is_consistent());
        assert!(PalwPremiumParams { maturation_windows: 6, ..p }.step_bound_is_structural());

        // The real failure mode the check must catch: a cap so small the controller can never move.
        assert!(!PalwPremiumParams { delta_cap_bps: 0, ..p }.is_consistent(), "a zero cap is a silent freeze");

        // ADR-0040 §16′-3: the P₀ calibration is part of validity. The PREVIOUS default (2 % of a leaf)
        // left a rotating-sybil pump profitable (140k cost vs 167k gain) and must now be rejected —
        // this is the recalibration being enforced rather than merely documented.
        assert!(!PalwPremiumParams { p0_bps_of_leaf: 200, ..p }.is_consistent(), "the old 2 % P₀ must now be invalid");
        assert!(!PalwPremiumParams { p0_bps_of_leaf: 238, ..p }.is_consistent(), "exactly break-even is not a margin");
        assert!(PalwPremiumParams { p0_bps_of_leaf: 300, ..p }.is_consistent(), "3 % clears the bar");
        assert!(!PalwPremiumParams { r_lo_bps: 900, r_hi_bps: 450, ..p }.is_consistent(), "inverted band rejected");
        assert!(!PalwPremiumParams { maturation_windows: 1_000, ..p }.is_consistent(), "unbounded ring rejected");
    }

    fn sample(ordered: u128, failed: u128, bond: u128) -> PalwWindowSample {
        PalwWindowSample { ordered_slot_cu: ordered, failed_slot_cu: failed, snapshot_bond_sompi: bond }
    }

    /// Bootstrap, thin markets and degraded mode all hold — the controller measures but does not act.
    #[test]
    fn holds_during_bootstrap_thin_market_and_degraded_mode() {
        let p = params();
        let overload = sample(10_000, 5_000, 1_000); // r = 50 %, far above r_hi

        // degraded: health mode owns the levers
        let (s, d) = update_premium(&PalwPremiumStateV1::neutral(), &overload, &p, true);
        assert_eq!(d, PalwPremiumDecision::HoldDegraded);
        assert_eq!(s.pi_bps, PALW_PREMIUM_BPS_ONE);

        // thin market: below the Poisson resolution, and it must NOT pollute the EMA
        let thin = sample(10, 10, 1_000);
        let (s, d) = update_premium(&PalwPremiumStateV1::neutral(), &thin, &p, false);
        assert_eq!(d, PalwPremiumDecision::HoldThinMarket);
        assert!(!s.ema_seeded, "a sub-resolution window must not seed the EMA");

        // bootstrap: measures (EMA seeded) but does not move
        let (s, d) = update_premium(&PalwPremiumStateV1::neutral(), &overload, &p, false);
        assert_eq!(d, PalwPremiumDecision::HoldBootstrap);
        assert!(s.ema_seeded);
        assert_eq!(s.pi_bps, PALW_PREMIUM_BPS_ONE);
    }

    /// Sustained overload raises π; the debounce means it cannot move on a single window.
    #[test]
    fn sustained_overload_raises_after_debounce_only() {
        let p = params();
        let mut st = PalwPremiumStateV1::neutral();
        st.windows_elapsed = p.bootstrap_hold_windows; // past bootstrap
        st.ema_seeded = true;
        st.r_ema_bps = p.r_hi_bps + 1; // already hot
        let hot = sample(10_000, 9_000, 1_000);

        let (s1, d1) = update_premium(&st, &hot, &p, false);
        assert_eq!(d1, PalwPremiumDecision::HoldDeadband, "one qualifying window must not move π (C = 2)");
        assert_eq!(s1.pi_bps, PALW_PREMIUM_BPS_ONE);
        assert_eq!(s1.consecutive_up, 1);

        let (s2, d2) = update_premium(&s1, &hot, &p, false);
        assert!(matches!(d2, PalwPremiumDecision::Raised { .. }), "the second consecutive window moves it, got {d2:?}");
        assert!(s2.pi_bps > PALW_PREMIUM_BPS_ONE);
        assert_eq!(s2.consecutive_up, 0, "a realised move resets its own debounce");
    }

    /// Glut lowers π only when BOTH signals agree — low shortfall alone is not evidence of oversupply.
    #[test]
    fn glut_requires_both_signals() {
        let p = params();
        let mut st = PalwPremiumStateV1::neutral();
        st.windows_elapsed = p.bootstrap_hold_windows;
        st.ema_seeded = true;
        st.r_ema_bps = 0;
        st.i_ema_bps = 0;

        // low shortfall but HIGH intensity (plenty of work per bond) ⇒ not a glut
        let busy = sample(1_000_000, 0, 1); // I enormous
        let (s1, d1) = update_premium(&st, &busy, &p, false);
        assert_eq!(d1, PalwPremiumDecision::HoldDeadband, "low r alone must not lower π");
        let (_, d2) = update_premium(&s1, &busy, &p, false);
        assert_eq!(d2, PalwPremiumDecision::HoldDeadband);

        // low shortfall AND thin work per bond ⇒ genuine glut
        let idle = sample(1_000, 0, 1_000_000_000);
        let (s1, _) = update_premium(&st, &idle, &p, false);
        let (s2, d) = update_premium(&s1, &idle, &p, false);
        assert!(matches!(d, PalwPremiumDecision::Lowered { .. }), "both signals agreeing must lower π, got {d:?}");
        assert!(s2.pi_bps < PALW_PREMIUM_BPS_ONE);
    }

    /// The rate limiter is the stability core: a permanent overload cannot walk π further than the
    /// maturation horizon permits, so the controller cannot outrun the supply response it is inducing.
    #[test]
    fn rate_limiter_caps_travel_over_the_maturation_horizon() {
        let p = params();
        let mut st = PalwPremiumStateV1::neutral();
        st.windows_elapsed = p.bootstrap_hold_windows;
        st.ema_seeded = true;
        st.r_ema_bps = p.r_hi_bps * 4;
        let hot = sample(10_000, 9_500, 1_000);

        let start = st.pi_bps;
        let mut limited = false;
        for _ in 0..200 {
            let (n, d) = update_premium(&st, &hot, &p, false);
            if matches!(d, PalwPremiumDecision::HoldRateLimited) {
                limited = true;
            }
            st = n;
        }
        assert!(limited, "a permanent overload must eventually hit the rate limiter");
        // Total travel within any maturation horizon stays inside the cap (plus one step's granularity).
        let travelled_bps = div_rne(st.pi_bps.abs_diff(start) as u128 * PALW_PREMIUM_BPS_ONE as u128, start as u128) as u32;
        assert!(
            travelled_bps <= p.delta_cap_bps * 200 / p.maturation_windows,
            "π travelled {travelled_bps} bps — the limiter must bound it"
        );
        assert!(st.pi_bps <= p.pi_max_bps, "π must never exceed its ceiling");
    }

    /// Floors and ceilings hold under unbounded pressure — the role-abandonment guard.
    #[test]
    fn pi_is_clamped_to_the_participation_band() {
        let mut p = params();
        p.delta_cap_bps = u32::MAX; // disable the limiter to test the clamp itself
        p.maturation_windows = 1;
        let hot = sample(10_000, 10_000, 1_000);
        let idle = sample(10_000, 0, u128::from(u64::MAX));

        let mut st = PalwPremiumStateV1::neutral();
        st.windows_elapsed = p.bootstrap_hold_windows;
        st.ema_seeded = true;
        st.r_ema_bps = p.r_hi_bps * 10;
        for _ in 0..5_000 {
            st = update_premium(&st, &hot, &p, false).0;
        }
        assert_eq!(st.pi_bps, p.pi_max_bps, "unbounded overload must saturate at π_max, not beyond");

        st.r_ema_bps = 0;
        st.i_ema_bps = 0;
        for _ in 0..5_000 {
            st = update_premium(&st, &idle, &p, false).0;
        }
        assert_eq!(st.pi_bps, p.pi_min_bps, "unbounded glut must saturate at π_min, not below");
    }

    /// Latency is deliberately absent from the signal set, so this asserts the shape of what IS
    /// measured: `r` is bounded and saturating, and a cartel cannot express "slow but on time".
    #[test]
    fn shortfall_is_bounded_and_only_delivery_failure_moves_it() {
        assert_eq!(sample(1_000, 0, 1).shortfall_bps(), 0, "on-time delivery, however slow, reads as zero shortfall");
        assert_eq!(sample(1_000, 500, 1).shortfall_bps(), 5_000);
        assert_eq!(sample(1_000, 1_000, 1).shortfall_bps(), PALW_PREMIUM_BPS_ONE);
        assert_eq!(sample(1_000, 9_999, 1).shortfall_bps(), PALW_PREMIUM_BPS_ONE, "r saturates at 100 %");
        assert_eq!(sample(0, 0, 1).shortfall_bps(), 0, "empty window is not infinite shortfall");
        assert_eq!(sample(1_000, 0, 0).intensity_bps(), 0, "zero bond must not divide by zero");
    }

    /// ADR-0040 §16′-3 — the ζ penalty must reference the LIVE π, and the split must reference the
    /// FROZEN one. Getting these the wrong way round is the single easiest mistake in this design, and
    /// it silently removes the anti-pump property: a cartel could raise π and still pay penalties priced
    /// at the pre-pump level, leaving the exploit bounded (by the ring buffer) but never negative.
    #[test]
    fn zeta_penalty_scales_with_the_live_premium_not_the_frozen_one() {
        let revenue = 1_000_000u128;
        let (zeta, floor) = (2u32, 10_000u128);

        // At neutral there is no excess to charge for, so the flat floor governs.
        assert_eq!(no_show_penalty_floor_sompi(PALW_PREMIUM_BPS_ONE, revenue, zeta, floor), floor);
        assert_eq!(
            no_show_penalty_floor_sompi(5_000, revenue, zeta, floor),
            floor,
            "below neutral clamps at the floor, never negative"
        );

        // Above neutral the penalty rises with the LIVE premium — this is what makes pumping EV-negative.
        let at_1_5 = no_show_penalty_floor_sompi(15_000, revenue, zeta, floor);
        let at_2_0 = no_show_penalty_floor_sompi(20_000, revenue, zeta, floor);
        let at_3_0 = no_show_penalty_floor_sompi(30_000, revenue, zeta, floor);
        assert_eq!(at_1_5, 2 * 5_000 * revenue / 10_000, "ζ · (π−1) · revenue");
        assert!(at_1_5 < at_2_0 && at_2_0 < at_3_0, "the penalty must be strictly increasing in the live premium");

        // The pump only pays if the penalty grows SLOWER than the premium it buys. At ζ = 2 the penalty
        // grows twice as fast as the excess, so a no-show that lifts π by one step can never recover its
        // own cost — the property ζ exists to provide.
        let excess_gain = 20_000 - PALW_PREMIUM_BPS_ONE; // the premium excess the pump would win
        let penalty_at_that_premium = at_2_0;
        assert!(
            penalty_at_that_premium > excess_gain as u128 * revenue / PALW_PREMIUM_BPS_ONE as u128,
            "ζ must make the penalty exceed the premium excess it would buy"
        );
    }

    /// Every hold must carry a distinct, stable reason code. A freeze is safe behaviour, but an
    /// UNOBSERVED freeze is indistinguishable from a fault — so the reason is part of the interface,
    /// not a debugging convenience.
    #[test]
    fn every_decision_has_a_distinct_reason_code() {
        use std::collections::HashSet;
        let all = [
            PalwPremiumDecision::HoldDegraded,
            PalwPremiumDecision::HoldThinMarket,
            PalwPremiumDecision::HoldBootstrap,
            PalwPremiumDecision::HoldDeadband,
            PalwPremiumDecision::HoldRateLimited,
            PalwPremiumDecision::Raised { from_bps: 1, to_bps: 2 },
            PalwPremiumDecision::Lowered { from_bps: 2, to_bps: 1 },
        ];
        let codes: HashSet<_> = all.iter().map(|d| d.reason_code()).collect();
        assert_eq!(codes.len(), all.len(), "reason codes must be distinct — an operator alerts on these");
        for d in &all[..5] {
            assert!(d.is_hold(), "{d:?} must classify as a hold");
        }
        assert!(!all[5].is_hold() && !all[6].is_hold());
        // A rate-limited hold must be distinguishable from a deadband hold: one means "the controller
        // wanted to move and was refused", the other means "nothing to do". Conflating them hides
        // exactly the misconfiguration this reason code exists to surface.
        assert_ne!(PalwPremiumDecision::HoldRateLimited.reason_code(), PalwPremiumDecision::HoldDeadband.reason_code());
    }

    /// Verifies `pump_ev_negative` against a rotating-sybil strategy.
    ///
    /// # Adversary model
    ///
    /// `esc(k)` escalates per credential. A cartel holding many minimum-bond credentials can spread its
    /// no-shows so that every event is that credential's first — `k = 0` throughout — evading escalation
    /// almost entirely. Minting credentials costs only the min-bond granularity, which is nothing to a
    /// cartel that already holds β. The flat model must therefore remain EV-negative using `ζ` and
    /// `P₀` alone, with `esc` providing additional protection against non-rotating attackers.
    ///
    /// # Role of `P₀`
    ///
    /// During the pump `π` is still near neutral, so `(π − 1)⁺ ≈ 0` and the `ζ` term contributes almost
    /// nothing — it is smallest exactly when it is needed. `ζ` prices *sustaining* an already-raised
    /// premium; `P₀` prices *raising* it.
    #[test]
    fn pump_ev_negative() {
        const ZETA: u32 = 2;
        const MU: u32 = 2;
        const K_CAP: u32 = 4;
        let v_leaf: u128 = 1_000_000;

        for (regime, p) in [
            ("limiter (genesis defaults)", PalwPremiumParams::genesis_defaults(864_000)),
            ("structural", PalwPremiumParams { maturation_windows: 6, ..PalwPremiumParams::genesis_defaults(864_000) }),
        ] {
            assert!(p.is_consistent(), "{regime}: genesis parameters must satisfy the calibration condition");
            assert!(p.flat_model_pump_is_ev_negative(), "{regime}: the FLAT model must be EV-negative on its own");

            let p0 = div_rne(p.p0_bps_of_leaf as u128 * v_leaf, PALW_PREMIUM_BPS_ONE as u128);
            let pi_pumped = PALW_PREMIUM_BPS_ONE + p.delta_cap_bps;
            assert!(pi_pumped <= p.pi_max_bps, "{regime}: the cap must stay inside the participation band");

            let windows_to_pump = p.delta_cap_bps.div_ceil(p.kappa_up_bps) as u128;
            if p.maturation_windows as u128 <= windows_to_pump {
                continue; // the pump cannot outlast its own cost window; nothing to prove
            }

            // GAIN ceiling: the raised σ_B on every post-pump window, assuming the cartel is assigned on
            // every leaf (generous to the attacker — volume cancels, so this is per slot-stream).
            let sigma_b_pumped = pi_pumped as u128 * PALW_PREMIUM_BPS_ONE as u128 / (PALW_PREMIUM_BPS_ONE + pi_pumped) as u128;
            let delta_sigma = sigma_b_pumped - PALW_PREMIUM_BPS_ONE as u128 / 2;
            let gain = div_rne(delta_sigma * v_leaf, PALW_PREMIUM_BPS_ONE as u128) * (p.maturation_windows as u128 - windows_to_pump);

            // COST floor under the ROTATING SYBIL model: every event is k = 0 (escalation evaded), and
            // π is still neutral while the pump is being paid for (so the ζ term is ≈ 0). This is the
            // cheapest the attack can possibly be.
            let rotating_cost = no_show_penalty_sompi(PALW_PREMIUM_BPS_ONE, v_leaf / 2, ZETA, p0, MU, 0, K_CAP) * windows_to_pump;

            assert!(
                rotating_cost > gain,
                "{regime}: a ROTATING SYBIL pump must be EV-negative on the flat model alone — \
                 cost {rotating_cost} must exceed gain {gain}"
            );

            // `esc` is now surplus, not load-bearing: a cartel that does NOT rotate pays strictly more.
            // If this ever fails, escalation has become a discount, which would be worse than useless.
            let non_rotating_cost = (0..windows_to_pump as u32)
                .map(|k| no_show_penalty_sompi(PALW_PREMIUM_BPS_ONE, v_leaf / 2, ZETA, p0, MU, k, K_CAP))
                .sum::<u128>();
            assert!(
                non_rotating_cost > rotating_cost,
                "{regime}: esc(k) must penalise the lazy attacker MORE than the rotating one, never less"
            );
        }
    }

    /// **ADR-0040 §16′-3 — the `P₀` window is two-sided, derived, and provably inhabited.**
    ///
    /// Three properties, and the third is the one that makes this a finished calibration rather than a
    /// lucky constant:
    ///
    /// 1. the LOWER bound is recomputed from the current `Δ_cap`/`κ_up`/`M_mat` — a future parameter
    ///    change that breaks the pump equilibrium turns CI red instead of silently weakening it;
    /// 2. the UPPER bound is the tighter of eclipse tolerance and honest-baseline harmlessness — two
    ///    independent pressures, so neither can be satisfied by ignoring the other;
    /// 3. **the window is non-empty** — the parameter SPACE is inhabited, not just the current point.
    ///    If the pump floor ever rises above the harm ceiling there is no safe `P₀`, and the build must
    ///    stop rather than quietly pick a side.
    #[test]
    fn p0_window_is_two_sided_derived_and_inhabited() {
        let p = PalwPremiumParams::genesis_defaults(864_000);
        let (lo, hi) = p.p0_window_bps();

        // Derived, not hardcoded: 2.38 % floor, 5 % ceiling (harmlessness binds before eclipse here).
        assert_eq!(lo, 238, "the pump floor must be derived from Δ_cap/κ_up/M_mat");
        assert_eq!(hi, 500, "the harmlessness ceiling (0.15 % / r₀ = 3 %) binds before the 10 % eclipse cap");
        assert!(p.p0_window_is_non_empty() && p.p0_is_calibrated());
        assert!(lo < p.p0_bps_of_leaf && p.p0_bps_of_leaf <= hi, "the default 3.5 % sits inside its own window");

        // (1) The floor MOVES with the parameters it is derived from. A slower controller spends longer
        //     pumping and enjoys the premium for fewer windows, so the required P₀ falls.
        let slower = PalwPremiumParams { kappa_up_bps: 75, ..p };
        assert!(slower.p0_window_bps().0 < lo, "halving κ_up must lower the pump floor, not leave it pinned");
        // A longer maturation horizon means more windows of gain, so the floor RISES.
        let longer = PalwPremiumParams { maturation_windows: 28, ..p };
        assert!(longer.p0_window_bps().0 > lo, "a longer horizon must raise the pump floor");

        // (2) The ceiling moves with the honest baseline: a noisier network (higher r₀) means honest
        //     providers eat more no-shows through no fault, so P₀ must come DOWN to stay harmless.
        let noisy = PalwPremiumParams { r0_baseline_bps: 600, ..p };
        assert!(noisy.p0_window_bps().1 < hi, "a higher honest baseline must tighten the harmlessness ceiling");
        // ...and at r₀ = 6 % the current P₀ is no longer harmless, so the params are rejected.
        assert!(!noisy.is_consistent(), "P₀ must be recalibrated when r₀ doubles — silence would be a hidden tax");

        // (3) **Non-emptiness.** Push the horizon far enough and the pump floor overtakes the harm
        //     ceiling: no P₀ satisfies both, and `is_consistent` must refuse rather than choose.
        let impossible = PalwPremiumParams { maturation_windows: 64, honest_penalty_harmless_bps: 1, ..p };
        let (ilo, ihi) = impossible.p0_window_bps();
        assert!(ilo >= ihi, "fixture sanity: this parameter set must genuinely have no safe P₀ ({ilo} vs {ihi})");
        assert!(!impossible.p0_window_is_non_empty());
        assert!(!impossible.is_consistent(), "an uninhabited window must stop the build, not pick a side");

        // The honest expectation at r₀ is what the ceiling actually prices: ~0.1 % of a leaf.
        let honest_expected_bps = p.r0_baseline_bps as u128 * p.p0_bps_of_leaf as u128 / PALW_PREMIUM_BPS_ONE as u128;
        assert!(honest_expected_bps <= p.honest_penalty_harmless_bps as u128, "honest operation must not be quietly taxed");
        assert_eq!(honest_expected_bps, 10, "0.10 % of a leaf at r₀ = 3 %");
    }

    /// ADR-0040 §16′-3 — the two consistency checks on `P₀`'s upper bound.
    ///
    /// `P₀` must be large enough to kill the pump (above) yet small enough not to ruin an **eclipse
    /// victim**, who is not at fault and whose signature is indistinguishable from a deliberate no-show.
    /// Those two pressures do not actually collide, for two independent reasons — both worth pinning,
    /// because "the guard happens to work" and "the guard is designed to work" look identical until one
    /// of them is refactored away.
    #[test]
    fn p0_upper_bound_is_compatible_with_the_pump_floor() {
        let p = PalwPremiumParams::genesis_defaults(864_000);
        let v_leaf: u128 = 1_000_000;
        let p0 = div_rne(p.p0_bps_of_leaf as u128 * v_leaf, PALW_PREMIUM_BPS_ONE as u128);

        // (1) In a market above N_STAT, a pump needs MANY no-show events, so the per-event P₀ that
        //     defeats it is small in absolute terms — a single unlucky victim pays one such event, not
        //     the whole campaign's cost.
        let windows_to_pump = p.delta_cap_bps.div_ceil(p.kappa_up_bps) as u128;
        assert!(windows_to_pump >= 2, "a pump is inherently multi-event, which is what keeps per-event P₀ small");
        assert!(p0 * 10 < v_leaf, "a single no-show must cost well under one leaf's reward — an eclipse victim survives it");

        // (2) In a THIN market the question does not arise: the N_STAT guard holds π, so there is no
        //     dial to pump. The existing statistical guard is doing safety work, not just noise control.
        let thin =
            PalwWindowSample { ordered_slot_cu: p.n_stat_slot_cu - 1, failed_slot_cu: p.n_stat_slot_cu - 1, snapshot_bond_sompi: 1 };
        let (_, d) = update_premium(&PalwPremiumStateV1::neutral(), &thin, &p, false);
        assert_eq!(d, PalwPremiumDecision::HoldThinMarket, "a thin market must freeze π, making a pump structurally impossible");
    }

    /// ADR-0040 §16′-3 — the collection path. Two properties that are easy to get wrong and expensive
    /// to get wrong: the penalty comes from the offender's BOND (not A's escrow), and it never lands in
    /// A's pocket (which would pay A for B's failure and subsidise eclipse-grinding).
    #[test]
    fn no_show_is_charged_to_the_bond_and_never_paid_to_the_requester() {
        // Partial: bond covers the penalty, credential survives.
        let c = charge_no_show(1_000, 400, NoShowPenaltyDestination::Burn);
        assert_eq!(c.collected_sompi, 400);
        assert!(!c.suspended, "a bond that still has value must not be suspended");

        // Exhausting: collect only what exists, and suspend — an unslashable provider must not be drawn.
        let c = charge_no_show(1_000, 5_000, NoShowPenaltyDestination::AuditBudget);
        assert_eq!(c.collected_sompi, 1_000, "a bond can never go negative");
        assert_eq!(c.assessed_sompi, 5_000, "the shortfall stays visible for the escalation record");
        assert!(c.suspended);

        // The destination is a closed set that structurally EXCLUDES the requester. There is no
        // `ToRequester` variant, so "pay A for B's no-show" is unrepresentable rather than merely
        // discouraged — A's remedy is the escrow-neutral reroll, funded separately.
        for d in [NoShowPenaltyDestination::Burn, NoShowPenaltyDestination::AuditBudget] {
            assert_eq!(charge_no_show(1_000, 100, d).destination, d);
        }
    }

    /// Determinism: the update is a pure function of (state, sample, params, mode).
    #[test]
    fn update_is_deterministic() {
        let p = params();
        let mut st = PalwPremiumStateV1::neutral();
        st.windows_elapsed = p.bootstrap_hold_windows;
        st.ema_seeded = true;
        st.r_ema_bps = p.r_hi_bps + 100;
        let s = sample(50_000, 20_000, 7_777);
        let a = update_premium(&st, &s, &p, false);
        let b = update_premium(&st, &s, &p, false);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }

    /// The neutral point must lie INSIDE the governed range, or the consensus seam's clamp would move
    /// it — and "π = 1 pays byte-identically to the old fixed 50/50" would quietly stop being true.
    #[test]
    fn premium_range_contains_the_neutral_point() {
        assert!(PALW_PREMIUM_PI_MIN_BPS <= PALW_PREMIUM_BPS_ONE && PALW_PREMIUM_BPS_ONE <= PALW_PREMIUM_PI_MAX_BPS);
        assert_eq!(PALW_PREMIUM_BPS_ONE.clamp(PALW_PREMIUM_PI_MIN_BPS, PALW_PREMIUM_PI_MAX_BPS), PALW_PREMIUM_BPS_ONE);
        // And the genesis params agree with the constants the seam clamps against — one source of truth.
        let p = PalwPremiumParams::genesis_defaults(864_000);
        assert_eq!((p.pi_min_bps, p.pi_max_bps), (PALW_PREMIUM_PI_MIN_BPS, PALW_PREMIUM_PI_MAX_BPS));
    }

    /// The split's arithmetic is total over the governed range: no division by zero (denom ≥ 10_000),
    /// no `u128` overflow at a `u64::MAX` base, and `a + Σb == base` exactly at both ends.
    #[test]
    fn premium_split_is_total_and_conserving_across_the_governed_range() {
        for &pi in &[PALW_PREMIUM_PI_MIN_BPS, PALW_PREMIUM_BPS_ONE, PALW_PREMIUM_PI_MAX_BPS] {
            for &m in &[1u16, 2, 8] {
                for &base in &[0u64, 1, 1_000_000, u64::MAX] {
                    let (a, per_b, rem) = premium_split(base, m, pi);
                    let total = a as u128 + per_b as u128 * m as u128 + rem as u128;
                    assert_eq!(total, base as u128, "value must be conserved (base={base} m={m} pi={pi})");
                }
            }
        }
    }
}
