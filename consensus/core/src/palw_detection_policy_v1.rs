//! **G14 — the per-class detection policy** (`docs/design/palw/g14-detection-policy-2026-10-17.md`; external audit 2026-10-17 §5,
//! §7 P1). **DORMANT**: the fence `palw_detection_policy_v1` is `None` on every preset, hashed Some-only in both fingerprints (each
//! policy's id beside the height), collapsed from `Some(never())`, and refused when armed ([`crate::config::params::Params::
//! validate_palw_detection_policy_v1`]) — the per-class costs, replay units and bytes are design values without a measured record.
//! Below the fence nothing here is read: the legacy draw (`palw_fp_interval_draw_v1`, `k = 4`, its domain and golden vector) is
//! unchanged.
//!
//! ```text
//! policy(class, kernel_version)  = inspection method, N rule, n_max, fault model (D_min), target ε, k_max, rounds, shadow rate,
//!                                  economics (cost, coverage, collection, margin), resources (bytes, replay units, retention,
//!                                  court/DA openings, throughput), deadlines, localisation — one consensus value, one id
//! N                              = the class's interval count from CHAIN counts that re-price to the claim's priced work
//!                                  (never a count the producer or the served material reports)
//! D_min(N)                       = max(d_min_abs, ⌈N · d_min_ppm / 10^6⌉) ∧ N   — the detectably-bad interval count ε is promised for
//! miss(N, D, K)                  = C(N−D, K) / C(N, K)   (0 when K > N−D)       — one honest checker, K distinct uniform intervals
//! K_needed                       = the smallest K with  q · (1 − miss(N, D_min, K)) ≥ 1 − ε   (integer fixed point, rounded so K is
//!                                  never below the exact minimum)
//! S_required (gain ∝ skipped)    = margin · N · c / (q_eff · q_collect)        — binding at D = N (see below), for EVERY D ≥ 1
//! S_required (fixed gain G)      = margin · G · N / (K_tot · q_eff · q_collect) — binding at D = 1 (a chosen output)
//! ```
//!
//! **Why the proportional bound binds at D = N, not D = 1.** With gain `D·c` for `D` skipped intervals and penalty `S` collected with
//! probability `p(D) = q·(1 − miss(N, D, K))`, forging is unprofitable in expectation for every `D` iff `S ≥ max_D D·c / p(D)`. The
//! increments `p(D+1) − p(D) = q·miss(D)·K/(N−D)` are non-increasing in `D` (their ratio is `(N−D−K)/(N−D−1) ≤ 1`), so `p` is concave
//! with `p(0) = 0`, `p(D)/D` is non-increasing, and the maximum of `D/p(D)` is at `D = N`, where `p = q`. Hence `S ≥ N·c/q` — the
//! claim's whole honest cost over the coverage — deters every skip, independent of `K` (a test brute-forces this for every small
//! `N, K`). `K` matters for the DETECTION target ε, not for this bound. A fraud whose gain does not scale with the skipped work (a
//! chosen output) is deterred by sampling only when `S ≥ G·N/K_tot`; the policy states whether it claims that cover
//! ([`PalwDetectionEconomicsV1::fixed_gain_sompi`]); otherwise the cover is deterministic full re-execution by any party
//! (`misaka_palw_kernel::seg_detect`, `kaspad::palw_kernel_verifier`, the legacy `--palw-challenge` watchdog) plus the court.
//!
//! **No cryptographic proof of computation** (CLAUDE.md, PRINCIPLES §8): everything here is sampling, deterministic recomputation and
//! economics; hashes and the existing challenge streams (`misaka_palw_challenge::seed`, rejection sampling, Floyd's distinct draw)
//! are used only for randomness and identity.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;
use misaka_palw_challenge::seed::{ChallengeStreamV1, StreamKindV1, StreamLabelV1};

use crate::config::params::ForkActivation;

/// The policy object's version.
pub const PALW_DETECTION_POLICY_VERSION_V1: u16 = 1;
/// The policy id's domain.
pub const PALW_DETECTION_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/detection/policy/v1";
/// **The new draw's domain** — distinct from `PALW_FP_INTERVAL_DRAW_DOMAIN_V1`, so no pre-fence draw or golden vector moves.
pub const PALW_DETECTION_DRAW_DOMAIN_V2: &[u8] = b"misaka-palw/detection/interval-draw/v2";
/// The checker-scope domain (the stream's `scope_id`).
pub const PALW_DETECTION_CHECKER_DOMAIN_V2: &[u8] = b"misaka-palw/detection/checker/v2";
/// The shadow-selection domain.
pub const PALW_DETECTION_SHADOW_DOMAIN_V2: &[u8] = b"misaka-palw/detection/shadow/v2";

/// One in a million.
pub const PPM: u64 = 1_000_000;
/// The largest ε denominator: keeps every comparison inside `u128` (see [`palw_detection_miss_bounds_v1`]).
pub const PALW_DETECTION_EPSILON_DEN_MAX_V1: u64 = 1_000_000_000;
/// The fixed point the miss probability is carried in (`10^21`): `scale · factor < 2^102`, `scale · den' < 2^120`.
const MISS_SCALE: u128 = 1_000_000_000_000_000_000_000;

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> [u8; 64] {
    state.finalize().as_bytes().try_into().expect("64 bytes")
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The value
// ---------------------------------------------------------------------------------------------------------------------------------

/// An exact ratio `num / den`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwRatioV1 {
    pub num: u64,
    pub den: u64,
}

impl PalwRatioV1 {
    pub const fn new(num: u64, den: u64) -> Self {
        Self { num, den }
    }
}

/// The route a class is adjudicated on (mirrors `palw_genesis8_catalog_v1::PalwGenesisRouteV1`, plus the kernel route).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwDetectionRouteV1 {
    Base0A16 = 1,
    TirV1 = 2,
    PalwQ361 = 3,
    /// K2-TIR kernel route claims (RFC-0014/0015).
    Kernel = 4,
}

/// **How an honest checker inspects one claim of the class.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwInspectionMethodV1 {
    /// The checker replays `K` distinct checkpoint intervals and compares every row and every recomputed state root exactly (the
    /// seat's interval arm, ADR-0077 Decision 8 / ADR-0082 Decision 9).
    IntervalReplay = 1,
    /// The checker re-executes the whole claim and compares roots (`misaka_palw_kernel::seg_detect`; the legacy watchdog): certain for
    /// every committed value of a checked claim — its rate is the share of claims checked.
    FullReexecution = 2,
}

/// **Where `N` comes from** — chain counts only. Each rule is the class's own checkpoint geometry spelled once on the consensus side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwIntervalCountRuleV1 {
    /// `N = max(1, ⌈(decode − 1) / checkpoint_interval⌉)` — base0's call geometry (`Base0FpIntervalGeometryV1::from_chain_facts_v1`).
    DecodeCalls { checkpoint_interval: u32 },
    /// `N = max(1, ⌈(prompt + decode − 1) / width⌉)` — the held geometry (`from_chain_facts_held_v1`, ADR-0103 Decision 2).
    HeldPositions { width: u32 },
    /// `N = ⌈work_leaves / leaves_per_interval⌉` — a route whose unit is the priced leaf itself (the kernel route's positions).
    PricedLeaves { leaves_per_interval: u64 },
}

/// **The fault set the target is promised for**: claims whose detectably-bad interval count is at least `D_min(N)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwFaultModelV1 {
    pub d_min_abs: u32,
    pub d_min_ppm: u32,
}

/// **The deterrence inputs.** `cost_per_interval_sompi` and the coverage floors are DESIGN values until measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwDetectionEconomicsV1 {
    /// An honest checkpoint interval's compute cost, sompi.
    pub cost_per_interval_sompi: u64,
    /// The class's registered reservation per interval (what an honest claim of `N` intervals holds as `reserved`), sompi — the arm
    /// check proves the class's own collateral rule meets the bound at every `N`.
    pub collateral_per_interval_sompi: u64,
    /// Floor (ppm) on the chance that an honest checker inspects a given claim (`10^6` under the G14 premise that an honest party
    /// checks every claim of the class).
    pub check_coverage_ppm: u32,
    /// Floor (ppm) on the collection of `S` given a conviction.
    pub collection_ppm: u32,
    /// Safety margin, permille (≥ 1000).
    pub margin_permille: u32,
    /// `Some(G)`: the policy claims sampled cover of a fixed-gain (chosen-output) fraud of gain up to `G` sompi, which requires
    /// `S ≥ margin · G · N / (K_tot · q_eff · q_collect)`. `None`: no such claim — full re-execution and the court are that cover.
    pub fixed_gain_sompi: Option<u64>,
}

/// **The resources a check consumes**, coupled to `K` — a `K` they cannot carry refuses the class instead of dropping an opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwDetectionResourcesV1 {
    /// The class's ceiling on one opening's bytes (`palw_fp_interval_opening_ceiling_v1` at `n_max`).
    pub opening_bytes: u64,
    /// The fetched-bytes ceiling for one claim (all rounds).
    pub max_claim_fetch_bytes: u64,
    /// One interval's replay cost, in the checker's units.
    pub replay_units_per_interval: u64,
    /// The replay ceiling for one claim (all rounds).
    pub max_claim_replay_units: u64,
    /// The declared honest checker's throughput, units per DAA.
    pub checker_units_per_daa: u64,
    /// The fetch latency allowance per round, DAA.
    pub fetch_daa: u64,
    /// Claims one checker holds in flight.
    pub claims_in_flight: u32,
    /// The node-side opening retention, `(claim, interval)` pairs (the seat pool, 512 before this policy).
    pub retention_pairs_cap: u32,
    /// Openings the DA/court budget can compel for ONE claim (each drawn interval must be compellable).
    pub court_openings_per_claim: u32,
    /// DA bytes the budget can carry for ONE claim's round.
    pub da_bytes_per_claim: u64,
}

/// The policy's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwDetectionDeadlinesV1 {
    /// From the panel's bind: every round must have been inspected by here (≤ the receipt window).
    pub inspection_deadline_daa: u64,
    /// Round `r`'s anchor slot is round 0's plus `r · round_spacing_daa`.
    pub round_spacing_daa: u64,
    /// Rounds of distinct draws per checker and claim (≥ 1).
    pub rounds: u32,
}

/// **How a detected mismatch is localised** to what the court adjudicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwLocalisationV1 {
    /// The seat's `Fault { interval, leaf }` / `FaultInRange` (or the `(0, 0)` state-root fault) → the leaf court's bisection.
    IntervalLeafCourt = 1,
    /// Root comparison then `seg_detect`'s descent (≤ ⌈log₂ S⌉ probes) to the first divergent position → the element court.
    RootDescentElementCourt = 2,
}

/// **One class's detection policy** (consensus value; [`Self::id`] is what fingerprints and draws bind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PalwDetectionPolicyV1 {
    pub version: u16,
    pub class_id: Hash64,
    /// The class's kernel / TIR version the policy is bound to (0: the class's own registered binding).
    pub kernel_version: u16,
    pub route: PalwDetectionRouteV1,
    pub method: PalwInspectionMethodV1,
    pub n_rule: PalwIntervalCountRuleV1,
    /// The largest `N` a claim of the class may have; above it the claim is refused (the checks below are proved up to here).
    pub n_max: u32,
    /// **The attempt lane's `N`**: an attempt claim answers the class's canonical job (it prices no leaves of its own), so its
    /// interval count is a class constant. `0`: this policy checks no attempt claim, and past the fence the class's attempts are
    /// refused.
    pub attempt_intervals: u32,
    pub fault_model: PalwFaultModelV1,
    /// The miss the policy allows on its fault set (the user's 1–10%).
    pub epsilon: PalwRatioV1,
    /// The per-round ceiling on drawn intervals.
    pub k_max: u32,
    /// Policy-driven random full re-execution, ppm of claims ([`palw_detection_is_shadowed_v2`]).
    pub shadow_rate_ppm: u32,
    pub economics: PalwDetectionEconomicsV1,
    pub resources: PalwDetectionResourcesV1,
    pub deadlines: PalwDetectionDeadlinesV1,
    pub localisation: PalwLocalisationV1,
}

/// **What one claim presents to the policy** — all of it chain facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDetectionClaimFactsV1 {
    /// An attempt claim (the class's canonical job, `N = attempt_intervals`); `false` for a free-prompt or kernel claim.
    pub attempt_lane: bool,
    /// The claim's priced leaves (`0`: an attempt claim, priced by the class).
    pub work_leaves: u64,
    /// What the class prices the context carrying the two counts at (`fp_context_work_leaves_v1`); must equal `work_leaves` when the
    /// claim prices its work.
    pub context_priced_leaves: Option<u64>,
    pub prompt_tokens: u32,
    pub decode_tokens_executed: u32,
    /// `claim.reserved` — what a conviction slashes.
    pub collateral_sompi: u128,
}

/// **Why a policy refuses a claim or a class.** Every one is a refusal, never a verdict about the producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwDetectionRefusalV1 {
    #[error("the policy's shape is invalid: {0}")]
    Malformed(&'static str),
    #[error("past the fence the class has no detection policy")]
    NoPolicyForClass,
    #[error("the context prices at {priced:?} leaves and the claim at {claim}: N would be the accused's choice")]
    NotAtTheClaimsPrice { priced: Option<u64>, claim: u64 },
    #[error("the chain counts yield no interval")]
    NoIntervals,
    #[error("the claim has {n} intervals, above the class's n_max {n_max}")]
    IntervalCountAboveCeiling { n: u32, n_max: u32 },
    #[error("the backend's geometry counts {backend} intervals and the policy's rule {policy}")]
    GeometryMismatch { backend: u32, policy: u32 },
    #[error("the coverage floor {coverage_ppm} ppm cannot reach the target 1 − ε")]
    CoverageBelowTarget { coverage_ppm: u64 },
    #[error("the target needs {needed} distinct intervals; the policy allows {allowed} ({rounds} round(s) × k_max)")]
    DetectionTargetUnreachable { needed: u32, allowed: u64, rounds: u32 },
    #[error("collateral {held} sompi is below the deterrence bound {required} sompi")]
    CollateralBelowDeterrence { required: u128, held: u128 },
    #[error("the deterrence bound overflows")]
    BoundOverflow,
    #[error("one claim fetches {need} bytes; the ceiling is {cap}")]
    FetchBytesOverBudget { need: u128, cap: u64 },
    #[error("one claim replays {need} units; the ceiling is {cap}")]
    ReplayUnitsOverBudget { need: u128, cap: u64 },
    #[error("retention needs {need} (claim, interval) pairs; the node holds {cap}")]
    RetentionBelowDraw { need: u64, cap: u32 },
    #[error("the court/DA budget compels {cap} openings or {cap_bytes} bytes per claim; the draw needs {need} / {need_bytes}")]
    CourtBudgetBelowDraw { need: u32, cap: u32, need_bytes: u128, cap_bytes: u64 },
    #[error("a round needs {need} DAA; the spacing is {spacing}, the deadline {deadline}")]
    DeadlineUnreachable { need: u64, spacing: u64, deadline: u64 },
}

/// **The policy's answer for one claim**: every number the checker, the court budget and the metrics read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDetectionPlanV1 {
    pub n: u32,
    pub d_min: u32,
    /// Distinct intervals the target needs (`n` for full re-execution).
    pub k_needed: u32,
    /// Drawn per round.
    pub k_round: u32,
    pub rounds: u32,
    /// Distinct intervals inspected over all rounds (`min(n, k_round · rounds)`).
    pub k_total: u32,
    /// Upper bound on the miss at `D_min` for one honest checker of a checked claim, ppm (rounded up).
    pub miss_at_d_min_ppm_ceil: u64,
    /// `false` when the fixed point could not decide the boundary and `k_needed` was raised by the conservative rule.
    pub k_exact: bool,
    pub collateral_required_sompi: u128,
    /// The single-interval (D = 1) figure `margin · c · N / (K_tot · q_eff · q_collect)` — for the record; the proportional bound
    /// binds at `D = N`.
    pub d1_proportional_sompi: u128,
    pub fetch_bytes: u128,
    pub replay_units: u128,
    pub retention_pairs: u64,
    pub round_daa: u64,
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Exact sampling arithmetic
// ---------------------------------------------------------------------------------------------------------------------------------

/// **`miss(N, D, K) = C(N−D, K) / C(N, K)` as a scaled interval `[lo, hi]`** with `lo ≤ miss · 10^21 ≤ hi` (each factor rounded
/// down for `lo`, up for `hi`). `K > N − D` is an exact zero; `D = 0` an exact one.
pub fn palw_detection_miss_bounds_v1(n: u32, d: u32, k: u32) -> (u128, u128) {
    let (n, d, k) = (u128::from(n), u128::from(d.min(n)), u128::from(k.min(n)));
    if d == 0 {
        return (MISS_SCALE, MISS_SCALE);
    }
    if k > n - d {
        return (0, 0);
    }
    let (mut lo, mut hi) = (MISS_SCALE, MISS_SCALE);
    for i in 0..k {
        let (num, den) = (n - d - i, n - i);
        lo = lo * num / den;
        hi = (hi * num).div_ceil(den);
    }
    (lo, hi)
}

/// `miss ≤ num/den`: `Some(true)` provably, `Some(false)` provably not, `None` the fixed point cannot tell.
fn miss_at_most(bounds: (u128, u128), num: u128, den: u128) -> Option<bool> {
    let (lo, hi) = bounds;
    if hi * den <= num * MISS_SCALE {
        Some(true)
    } else if lo * den > num * MISS_SCALE {
        Some(false)
    } else {
        None
    }
}

/// **The smallest `K` with `miss(N, D, K) ≤ num/den`**, and whether it is exact. An undecidable boundary counts as "not yet", so the
/// answer is never below the exact minimum (at most the one `K` above it). `num/den ≥ 1` is `K = 0`.
pub fn palw_detection_k_min_v1(n: u32, d: u32, num: u128, den: u128) -> (u32, bool) {
    if n == 0 || d == 0 || den == 0 {
        return (n, true);
    }
    if num >= den {
        return (0, true);
    }
    let d = d.min(n);
    // miss is non-increasing in K and exactly 0 at K = N − D + 1 (or N).
    let (mut lo, mut hi) = (0u32, (n - d + 1).min(n));
    let mut exact = true;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match miss_at_most(palw_detection_miss_bounds_v1(n, d, mid), num, den) {
            Some(true) => hi = mid,
            Some(false) => lo = mid + 1,
            None => {
                exact = false;
                lo = mid + 1;
            }
        }
    }
    (lo, exact)
}

/// The detection floor `1 − miss` in ppm (rounded down) for one honest checker of a checked claim.
pub fn palw_detection_ppm_floor_v1(n: u32, d: u32, k: u32) -> u64 {
    let (_, hi) = palw_detection_miss_bounds_v1(n, d, k);
    ((MISS_SCALE - hi.min(MISS_SCALE)) * u128::from(PPM) / MISS_SCALE) as u64
}

fn miss_ppm_ceil(n: u32, d: u32, k: u32) -> u64 {
    let (_, hi) = palw_detection_miss_bounds_v1(n, d, k);
    (hi * u128::from(PPM)).div_ceil(MISS_SCALE) as u64
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The policy
// ---------------------------------------------------------------------------------------------------------------------------------

impl PalwIntervalCountRuleV1 {
    /// `N` from chain counts. `None` when the rule's parameter is zero or the result overflows `u32`.
    pub fn interval_count_v1(&self, facts: &PalwDetectionClaimFactsV1) -> Option<u32> {
        match *self {
            Self::DecodeCalls { checkpoint_interval } => {
                if checkpoint_interval == 0 {
                    return None;
                }
                Some(facts.decode_tokens_executed.saturating_sub(1).div_ceil(checkpoint_interval).max(1))
            }
            Self::HeldPositions { width } => {
                if width == 0 {
                    return None;
                }
                let steps = u64::from(facts.prompt_tokens) + u64::from(facts.decode_tokens_executed.saturating_sub(1));
                u32::try_from(steps.div_ceil(u64::from(width)).max(1)).ok()
            }
            Self::PricedLeaves { leaves_per_interval } => {
                if leaves_per_interval == 0 || facts.work_leaves == 0 {
                    return None;
                }
                u32::try_from(facts.work_leaves.div_ceil(leaves_per_interval)).ok()
            }
        }
    }
}

impl PalwFaultModelV1 {
    /// `D_min(N) = max(d_min_abs, ⌈N · d_min_ppm / 10^6⌉)`, at most `N`.
    pub fn d_min_v1(&self, n: u32) -> u32 {
        let by_share = (u64::from(n) * u64::from(self.d_min_ppm)).div_ceil(PPM);
        (u64::from(self.d_min_abs).max(by_share).min(u64::from(n))) as u32
    }
}

impl PalwDetectionPolicyV1 {
    /// **The policy id**: every field, domain-separated. Draws and fingerprints bind this.
    pub fn id(&self) -> Hash64 {
        let mut s = keyed(PALW_DETECTION_POLICY_DOMAIN_V1);
        s.update(&self.version.to_le_bytes());
        s.update(self.class_id.as_byte_slice());
        s.update(&self.kernel_version.to_le_bytes());
        s.update(&[self.route as u8, self.method as u8]);
        match self.n_rule {
            PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval } => {
                s.update(&[1]);
                s.update(&u64::from(checkpoint_interval).to_le_bytes());
            }
            PalwIntervalCountRuleV1::HeldPositions { width } => {
                s.update(&[2]);
                s.update(&u64::from(width).to_le_bytes());
            }
            PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval } => {
                s.update(&[3]);
                s.update(&leaves_per_interval.to_le_bytes());
            }
        }
        for v in [
            u64::from(self.n_max),
            u64::from(self.attempt_intervals),
            u64::from(self.fault_model.d_min_abs),
            u64::from(self.fault_model.d_min_ppm),
            self.epsilon.num,
            self.epsilon.den,
            u64::from(self.k_max),
            u64::from(self.shadow_rate_ppm),
        ] {
            s.update(&v.to_le_bytes());
        }
        let e = &self.economics;
        for v in [
            e.cost_per_interval_sompi,
            e.collateral_per_interval_sompi,
            u64::from(e.check_coverage_ppm),
            u64::from(e.collection_ppm),
            u64::from(e.margin_permille),
        ] {
            s.update(&v.to_le_bytes());
        }
        match e.fixed_gain_sompi {
            None => s.update(&[0]),
            Some(g) => {
                s.update(&[1]);
                s.update(&g.to_le_bytes())
            }
        };
        let r = &self.resources;
        for v in [
            r.opening_bytes,
            r.max_claim_fetch_bytes,
            r.replay_units_per_interval,
            r.max_claim_replay_units,
            r.checker_units_per_daa,
            r.fetch_daa,
            u64::from(r.claims_in_flight),
            u64::from(r.retention_pairs_cap),
            u64::from(r.court_openings_per_claim),
            r.da_bytes_per_claim,
            self.deadlines.inspection_deadline_daa,
            self.deadlines.round_spacing_daa,
            u64::from(self.deadlines.rounds),
        ] {
            s.update(&v.to_le_bytes());
        }
        s.update(&[self.localisation as u8]);
        Hash64::from_bytes(finish(s))
    }

    /// **The shape refusals** — what no value of the policy may be.
    pub fn validate_shape_v1(&self) -> Result<(), PalwDetectionRefusalV1> {
        use PalwDetectionRefusalV1::Malformed;
        if self.version != PALW_DETECTION_POLICY_VERSION_V1 {
            return Err(Malformed("unknown policy version"));
        }
        if self.class_id == Hash64::default() {
            return Err(Malformed("the policy names no class"));
        }
        let eps = self.epsilon;
        if eps.num == 0 || eps.num >= eps.den || eps.den > PALW_DETECTION_EPSILON_DEN_MAX_V1 {
            return Err(Malformed("ε must be in (0, 1) with a denominator ≤ 10^9"));
        }
        if self.n_max == 0 || self.k_max == 0 || self.deadlines.rounds == 0 {
            return Err(Malformed("n_max, k_max and rounds must be positive"));
        }
        if self.fault_model.d_min_abs == 0 && self.fault_model.d_min_ppm == 0 {
            return Err(Malformed("the fault model names no fault (d_min_abs = d_min_ppm = 0)"));
        }
        let e = &self.economics;
        let ppm_ok = |v: u32| v as u64 <= PPM;
        if !ppm_ok(self.fault_model.d_min_ppm)
            || !ppm_ok(self.shadow_rate_ppm)
            || !ppm_ok(e.check_coverage_ppm)
            || !ppm_ok(e.collection_ppm)
        {
            return Err(Malformed("a ppm field exceeds 10^6"));
        }
        if e.check_coverage_ppm == 0 || e.collection_ppm == 0 {
            return Err(Malformed("coverage and collection floors must be positive"));
        }
        if e.margin_permille < 1_000 {
            return Err(Malformed("the margin must be at least 1000 permille"));
        }
        if e.cost_per_interval_sompi == 0 {
            return Err(Malformed("an interval with no cost deters nothing: cost_per_interval_sompi must be positive"));
        }
        let r = &self.resources;
        if r.checker_units_per_daa == 0 || r.replay_units_per_interval == 0 || r.opening_bytes == 0 {
            return Err(Malformed("throughput, replay units and opening bytes must be positive"));
        }
        if self.deadlines.rounds > 1 && self.deadlines.round_spacing_daa == 0 {
            return Err(Malformed("several rounds need a positive round spacing"));
        }
        match (self.method, self.localisation) {
            (PalwInspectionMethodV1::FullReexecution, PalwLocalisationV1::IntervalLeafCourt) => {
                return Err(Malformed("full re-execution localises by root descent"));
            }
            (PalwInspectionMethodV1::IntervalReplay, _) | (PalwInspectionMethodV1::FullReexecution, _) => {}
        }
        match self.n_rule {
            PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval: 0 }
            | PalwIntervalCountRuleV1::HeldPositions { width: 0 }
            | PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval: 0 } => {
                return Err(Malformed("the N rule's unit is zero"));
            }
            _ => {}
        }
        Ok(())
    }

    /// `q_eff = q + r − q·r` in ppm (floor): a shadowed claim is fully re-executed, so its detection is certain.
    pub fn effective_coverage_ppm_v1(&self) -> u64 {
        let (q, r) = (u64::from(self.economics.check_coverage_ppm), u64::from(self.shadow_rate_ppm));
        (q + r - q * r / PPM).min(PPM)
    }

    /// **The plan for one claim**, or the refusal. Pure; every refusal is about the policy's ability to check, never about the claim's
    /// truth.
    pub fn plan_v1(&self, facts: &PalwDetectionClaimFactsV1) -> Result<PalwDetectionPlanV1, PalwDetectionRefusalV1> {
        use PalwDetectionRefusalV1 as R;
        self.validate_shape_v1()?;
        // **N from priced work** (ADR-0074 Decision 5): the counts are believed only when they re-price to the claim. An attempt
        // answers the class's canonical job, whose N is the policy's class constant.
        let n = if facts.attempt_lane {
            self.attempt_intervals
        } else {
            if crate::palw_backend::palw_claim_prices_work_v1(facts.work_leaves)
                && facts.context_priced_leaves != Some(facts.work_leaves)
            {
                return Err(R::NotAtTheClaimsPrice { priced: facts.context_priced_leaves, claim: facts.work_leaves });
            }
            self.n_rule.interval_count_v1(facts).ok_or(R::NoIntervals)?
        };
        if n == 0 {
            return Err(R::NoIntervals);
        }
        if n > self.n_max {
            return Err(R::IntervalCountAboveCeiling { n, n_max: self.n_max });
        }
        let d_min = self.fault_model.d_min_v1(n);
        // The target is on the sampled inspection alone (the shadow is not counted toward ε): q · (1 − miss) ≥ 1 − ε
        // ⇔ miss ≤ (q·den − (den − num)·10^6) / (q·den).
        let q = u128::from(self.economics.check_coverage_ppm);
        let (num, den) = (u128::from(self.epsilon.num), u128::from(self.epsilon.den));
        let lhs = q * den;
        let need = (den - num) * u128::from(PPM);
        if lhs <= need {
            return Err(R::CoverageBelowTarget { coverage_ppm: q as u64 });
        }
        let (k_needed, k_exact, rounds, k_round) = match self.method {
            PalwInspectionMethodV1::IntervalReplay => {
                let (k, exact) = palw_detection_k_min_v1(n, d_min, lhs - need, lhs);
                let rounds = self.deadlines.rounds;
                let k_round = k.div_ceil(rounds).min(n);
                if k_round > self.k_max {
                    return Err(R::DetectionTargetUnreachable {
                        needed: k,
                        allowed: u64::from(self.k_max) * u64::from(rounds),
                        rounds,
                    });
                }
                (k, exact, rounds, k_round)
            }
            PalwInspectionMethodV1::FullReexecution => (n, true, 1, n),
        };
        let k_total = (u64::from(k_round) * u64::from(rounds)).min(u64::from(n)) as u32;
        let miss_at_d_min_ppm_ceil = match self.method {
            PalwInspectionMethodV1::IntervalReplay => miss_ppm_ceil(n, d_min, k_total),
            PalwInspectionMethodV1::FullReexecution => 0,
        };
        // **Deterrence** (module doc): S ≥ margin · N · c / (q_eff · q_collect), and for a declared fixed gain G,
        // S ≥ margin · G · N / (K_tot · q_eff · q_collect).
        let e = &self.economics;
        let q_eff = u128::from(self.effective_coverage_ppm_v1().max(1));
        let q_col = u128::from(e.collection_ppm);
        let scale = |base: u128, times: u128, over: u128| -> Result<u128, R> {
            let x = base.checked_mul(u128::from(e.margin_permille)).ok_or(R::BoundOverflow)?.div_ceil(1_000);
            let x = x.checked_mul(times).ok_or(R::BoundOverflow)?.div_ceil(over);
            let x = x.checked_mul(u128::from(PPM)).ok_or(R::BoundOverflow)?.div_ceil(q_eff);
            Ok(x.checked_mul(u128::from(PPM)).ok_or(R::BoundOverflow)?.div_ceil(q_col))
        };
        let cost = u128::from(n) * u128::from(e.cost_per_interval_sompi);
        let proportional = scale(cost, 1, 1)?;
        let d1_proportional_sompi = scale(u128::from(e.cost_per_interval_sompi), u128::from(n), u128::from(k_total.max(1)))?;
        let fixed = match e.fixed_gain_sompi {
            None => 0,
            Some(g) => scale(u128::from(g), u128::from(n), u128::from(k_total.max(1)))?,
        };
        let collateral_required_sompi = proportional.max(fixed);
        if facts.collateral_sompi < collateral_required_sompi {
            return Err(R::CollateralBelowDeterrence { required: collateral_required_sompi, held: facts.collateral_sompi });
        }
        // **Resources, coupled to K.**
        let r = &self.resources;
        let (fetch_bytes, replay_units) = (
            u128::from(k_total) * u128::from(r.opening_bytes),
            u128::from(k_total) * u128::from(r.replay_units_per_interval),
        );
        if self.method == PalwInspectionMethodV1::IntervalReplay && fetch_bytes > u128::from(r.max_claim_fetch_bytes) {
            return Err(R::FetchBytesOverBudget { need: fetch_bytes, cap: r.max_claim_fetch_bytes });
        }
        if replay_units > u128::from(r.max_claim_replay_units) {
            return Err(R::ReplayUnitsOverBudget { need: replay_units, cap: r.max_claim_replay_units });
        }
        // A seat draws every round of its duty at once (`palw_detection_seat_rounds_v2`), so it holds `K_tot` pairs per claim.
        let retention_pairs = match self.method {
            PalwInspectionMethodV1::IntervalReplay => u64::from(k_total) * u64::from(r.claims_in_flight),
            PalwInspectionMethodV1::FullReexecution => 0,
        };
        if retention_pairs > u64::from(r.retention_pairs_cap) {
            return Err(R::RetentionBelowDraw { need: retention_pairs, cap: r.retention_pairs_cap });
        }
        if self.method == PalwInspectionMethodV1::IntervalReplay {
            let need_bytes = u128::from(k_round) * u128::from(r.opening_bytes);
            if k_round > r.court_openings_per_claim || need_bytes > u128::from(r.da_bytes_per_claim) {
                return Err(R::CourtBudgetBelowDraw {
                    need: k_round,
                    cap: r.court_openings_per_claim,
                    need_bytes,
                    cap_bytes: r.da_bytes_per_claim,
                });
            }
        }
        let round_units = u128::from(k_round) * u128::from(r.replay_units_per_interval);
        let round_daa = u64::try_from(round_units.div_ceil(u128::from(r.checker_units_per_daa)))
            .ok()
            .and_then(|d| d.checked_add(r.fetch_daa))
            .ok_or(R::BoundOverflow)?;
        let dl = &self.deadlines;
        let total = u64::from(rounds - 1).checked_mul(dl.round_spacing_daa).and_then(|x| x.checked_add(round_daa));
        let spacing_ok = rounds == 1 || dl.round_spacing_daa >= round_daa;
        if !spacing_ok || total.is_none_or(|t| t > dl.inspection_deadline_daa) {
            return Err(R::DeadlineUnreachable { need: round_daa, spacing: dl.round_spacing_daa, deadline: dl.inspection_deadline_daa });
        }
        Ok(PalwDetectionPlanV1 {
            n,
            d_min,
            k_needed,
            k_round,
            rounds,
            k_total,
            miss_at_d_min_ppm_ceil,
            k_exact,
            collateral_required_sompi,
            d1_proportional_sompi,
            fetch_bytes,
            replay_units,
            retention_pairs,
            round_daa,
        })
    }

    /// **The arm check**: the plan holds, with the class's own collateral rule, at `N = 1`, every power of two below `n_max`, and
    /// `n_max`. `D_min` grows with `N` and the k needed for a share-based `D_min` is bounded in `N`, so the extremes are where a budget
    /// fails; the per-claim [`Self::plan_v1`] is the rule, this only refuses a class that would refuse its own claims.
    pub fn validate_armable_v1(&self) -> Result<(), PalwDetectionRefusalV1> {
        self.validate_shape_v1()?;
        let mut probes: Vec<u32> = (0..32).map(|b| 1u32 << b).take_while(|&n| n < self.n_max).collect();
        probes.push(self.n_max);
        for n in probes {
            let facts = self.synthetic_facts_v1(n).ok_or(PalwDetectionRefusalV1::Malformed("the N rule cannot reach a probe N"))?;
            self.plan_v1(&facts)?;
        }
        if self.attempt_intervals > 0 {
            self.plan_v1(&self.synthetic_attempt_facts_v1())?;
        }
        Ok(())
    }

    /// An attempt claim at the class's own collateral rule (`collateral_per_interval · attempt_intervals`).
    pub fn synthetic_attempt_facts_v1(&self) -> PalwDetectionClaimFactsV1 {
        PalwDetectionClaimFactsV1 {
            attempt_lane: true,
            work_leaves: 0,
            context_priced_leaves: None,
            prompt_tokens: 0,
            decode_tokens_executed: 0,
            collateral_sompi: u128::from(self.attempt_intervals) * u128::from(self.economics.collateral_per_interval_sompi),
        }
    }

    /// Chain counts that give exactly `n` intervals under the rule, with the class's own collateral (`collateral_per_interval · n`).
    pub fn synthetic_facts_v1(&self, n: u32) -> Option<PalwDetectionClaimFactsV1> {
        let collateral = u128::from(n) * u128::from(self.economics.collateral_per_interval_sompi);
        let facts = match self.n_rule {
            PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval } => PalwDetectionClaimFactsV1 {
                attempt_lane: false,
                work_leaves: 0,
                context_priced_leaves: None,
                prompt_tokens: 1,
                decode_tokens_executed: n.checked_mul(checkpoint_interval)?.checked_add(1)?,
                collateral_sompi: collateral,
            },
            PalwIntervalCountRuleV1::HeldPositions { width } => PalwDetectionClaimFactsV1 {
                attempt_lane: false,
                work_leaves: 0,
                context_priced_leaves: None,
                prompt_tokens: n.checked_mul(width)?,
                decode_tokens_executed: 1,
                collateral_sompi: collateral,
            },
            PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval } => {
                let leaves = u64::from(n).checked_mul(leaves_per_interval)?;
                PalwDetectionClaimFactsV1 {
                    attempt_lane: false,
                    work_leaves: leaves,
                    context_priced_leaves: Some(leaves),
                    prompt_tokens: 0,
                    decode_tokens_executed: 0,
                    collateral_sompi: collateral,
                }
            }
        };
        (self.n_rule.interval_count_v1(&facts) == Some(n)).then_some(facts)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The fence and the regime
// ---------------------------------------------------------------------------------------------------------------------------------

/// **`Params::palw_detection_policy_v1`'s value**: the height and the per-class policies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDetectionPolicyFenceV1 {
    pub activation: ForkActivation,
    pub policies: Vec<PalwDetectionPolicyV1>,
}

impl PalwDetectionPolicyFenceV1 {
    /// **NOT A PROPOSAL**: the fence at `activation` with no policy (the fork-id probe, fixtures).
    pub fn at(activation: ForkActivation) -> Self {
        Self { activation, policies: Vec::new() }
    }

    /// The value's refusals: one policy per `(class, kernel_version)`, each armable.
    pub fn validate(&self) -> Result<(), PalwDetectionRefusalV1> {
        let mut seen = BTreeSet::new();
        for p in &self.policies {
            if !seen.insert((p.class_id, p.kernel_version)) {
                return Err(PalwDetectionRefusalV1::Malformed("two policies for one (class, kernel_version)"));
            }
            p.validate_armable_v1()?;
        }
        Ok(())
    }

    pub fn policy_for_v1(&self, class_id: &Hash64, kernel_version: u16) -> Option<&PalwDetectionPolicyV1> {
        self.policies.iter().find(|p| p.class_id == *class_id && p.kernel_version == kernel_version)
    }

    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write((self.policies.len() as u64).to_le_bytes());
        for p in &self.policies {
            h.write(p.id().as_bytes());
        }
    }
}

/// The seat's opening retention before this policy: `k = 4` intervals for each of 128 claims in flight.
pub const PALW_DETECTION_LEGACY_RETENTION_PAIRS_V1: u32 = 512;

/// **The node's opening-retention ceiling**: the legacy 512 pairs, raised to the largest `retention_pairs_cap` any policy of the
/// fence declares (each policy's plan already proved `k · claims_in_flight` fits its cap). Unchanged (512) with no fence.
pub fn palw_detection_retention_pairs_v1(fence: Option<&PalwDetectionPolicyFenceV1>) -> u32 {
    fence
        .filter(|f| f.activation != ForkActivation::never())
        .map(|f| f.policies.iter().map(|p| p.resources.retention_pairs_cap).max().unwrap_or(0))
        .unwrap_or(0)
        .max(PALW_DETECTION_LEGACY_RETENTION_PAIRS_V1)
}

/// **Which rule a claim is checked under.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDetectionRegimeV1<'a> {
    /// Below the fence (or no fence): the legacy draw, `k = 4`, its domain — unchanged.
    LegacyV1,
    /// Past the fence, under the class's policy.
    Policy(&'a PalwDetectionPolicyV1),
    /// Past the fence, a class with no policy: no declared target exists, so the claim is refused (never a silent `k = 4`).
    NoPolicy,
}

/// The regime of a claim of `(class, kernel_version)` keyed at `daa` (the panel's bind DAA for a seat; the acceptance DAA for
/// admission).
pub fn palw_detection_regime_v1<'a>(
    fence: Option<&'a PalwDetectionPolicyFenceV1>,
    class_id: &Hash64,
    kernel_version: u16,
    daa: u64,
) -> PalwDetectionRegimeV1<'a> {
    match fence {
        Some(f) if f.activation != ForkActivation::never() && f.activation.is_active(daa) => {
            f.policy_for_v1(class_id, kernel_version).map_or(PalwDetectionRegimeV1::NoPolicy, PalwDetectionRegimeV1::Policy)
        }
        _ => PalwDetectionRegimeV1::LegacyV1,
    }
}

/// **Claim admission under the regime**: `Ok(None)` below the fence; the plan past it, or the refusal that keeps the claim out.
pub fn palw_detection_admit_claim_v1(
    regime: PalwDetectionRegimeV1<'_>,
    facts: &PalwDetectionClaimFactsV1,
) -> Result<Option<PalwDetectionPlanV1>, PalwDetectionRefusalV1> {
    match regime {
        PalwDetectionRegimeV1::LegacyV1 => Ok(None),
        PalwDetectionRegimeV1::NoPolicy => Err(PalwDetectionRefusalV1::NoPolicyForClass),
        PalwDetectionRegimeV1::Policy(p) => p.plan_v1(facts).map(Some),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The draw (v2): post-commit, bound, distinct across rounds, re-derivable
// ---------------------------------------------------------------------------------------------------------------------------------

/// Who is checking — the stream's scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwDetectionCheckerV1 {
    /// Seat `i` of the bound panel.
    Seat(u8),
    /// A public bonded verifier, by its bond's rate key.
    PublicBond(Hash64),
}

/// **What a draw is bound to**: network, claim, model (class), job, policy and checker. The round and its anchor ride separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDetectionDrawBindingV2 {
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    /// The job the execution answered (`PalwClaimStateV2::job_identity`; the FP job pin).
    pub job_id: Hash64,
    pub policy_id: Hash64,
    pub checker: PalwDetectionCheckerV1,
}

/// One round's anchor: a chain block that did not exist when the claim (and every earlier round) was committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDetectionRoundAnchorV1 {
    pub block: Hash64,
    pub daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwDetectionDrawRefusalV2 {
    #[error("round {round}'s anchor (DAA {anchor_daa}) does not postdate what it must follow (DAA {after_daa})")]
    AnchorNotPostCommit { round: u32, anchor_daa: u64, after_daa: u64 },
    #[error("the challenge stream refused: {0}")]
    Stream(&'static str),
}

fn checker_scope_v2(checker: &PalwDetectionCheckerV1) -> [u8; 64] {
    let mut s = keyed(PALW_DETECTION_CHECKER_DOMAIN_V2);
    match checker {
        PalwDetectionCheckerV1::Seat(i) => {
            s.update(&[1, *i]);
        }
        PalwDetectionCheckerV1::PublicBond(b) => {
            s.update(&[2]);
            s.update(b.as_byte_slice());
        }
    }
    finish(s)
}

/// **The round's challenge seed**: `H(domain ‖ network ‖ anchor ‖ claim ‖ class ‖ job ‖ policy ‖ round)`. The anchor is the
/// post-commit randomness (the panel anchor for round 0, a later slot's block after); no new beacon is introduced.
pub fn palw_detection_round_seed_v2(binding: &PalwDetectionDrawBindingV2, round: u32, anchor: &Hash64) -> [u8; 64] {
    let mut s = keyed(PALW_DETECTION_DRAW_DOMAIN_V2);
    s.update(binding.network_domain.as_byte_slice());
    s.update(anchor.as_byte_slice());
    s.update(binding.claim_id.as_byte_slice());
    s.update(binding.class_id.as_byte_slice());
    s.update(binding.job_id.as_byte_slice());
    s.update(binding.policy_id.as_byte_slice());
    s.update(&round.to_le_bytes());
    finish(s)
}

/// **One round's draw**: `k` intervals of `0..n` none of which is in `inspected_before`, uniform without replacement over the
/// uninspected ones (the shared challenge stream's Floyd draw, rejection-sampled), returned ascending. Fewer than `k` remain → all of
/// them.
pub fn palw_detection_draw_round_v2(
    binding: &PalwDetectionDrawBindingV2,
    round: u32,
    anchor: &Hash64,
    k: u32,
    n: u32,
    inspected_before: &BTreeSet<u32>,
) -> Result<Vec<u32>, PalwDetectionDrawRefusalV2> {
    let prior = inspected_before.iter().filter(|i| **i < n).count() as u64;
    let remaining = u64::from(n) - prior;
    let take = u64::from(k).min(remaining);
    if take == 0 {
        return Ok(Vec::new());
    }
    let seed = palw_detection_round_seed_v2(binding, round, anchor);
    let label = StreamLabelV1 { kind: StreamKindV1::Segment, scope_id: checker_scope_v2(&binding.checker), relation: round, repetition: 0 };
    let ranks = ChallengeStreamV1::new(&seed, &label)
        .distinct_indices(remaining, take)
        .map_err(|_| PalwDetectionDrawRefusalV2::Stream("the stream exhausted or the range was empty"))?;
    // Rank j among the uninspected intervals, ascending — one walk.
    let mut out = Vec::with_capacity(ranks.len());
    let mut ranks = ranks.into_iter().peekable();
    let mut rank = 0u64;
    for index in 0..n {
        if inspected_before.contains(&index) {
            continue;
        }
        if ranks.peek() == Some(&rank) {
            out.push(index);
            ranks.next();
            if ranks.peek().is_none() {
                break;
            }
        }
        rank += 1;
    }
    Ok(out)
}

/// **Every round, re-derived from chain anchors alone**: round `r` excludes everything rounds `< r` drew for this checker, so a
/// restart, a reorg or IBD recomputes the same sets from the same anchors, and no interval is ever counted twice. Each anchor must
/// postdate the claim's acceptance and the previous round's anchor (the staged-round rule of `misaka_palw_challenge::seed`).
pub fn palw_detection_draw_rounds_v2(
    binding: &PalwDetectionDrawBindingV2,
    claim_accepted_daa: u64,
    anchors: &[PalwDetectionRoundAnchorV1],
    k_round: u32,
    n: u32,
) -> Result<(Vec<Vec<u32>>, BTreeSet<u32>), PalwDetectionDrawRefusalV2> {
    let mut inspected = BTreeSet::new();
    let mut rounds = Vec::with_capacity(anchors.len());
    let mut after = claim_accepted_daa;
    for (r, anchor) in anchors.iter().enumerate() {
        let round = r as u32;
        if anchor.daa <= after {
            return Err(PalwDetectionDrawRefusalV2::AnchorNotPostCommit { round, anchor_daa: anchor.daa, after_daa: after });
        }
        after = anchor.daa;
        let drawn = palw_detection_draw_round_v2(binding, round, &anchor.block, k_round, n, &inspected)?;
        inspected.extend(drawn.iter().copied());
        rounds.push(drawn);
    }
    Ok((rounds, inspected))
}

/// **Is this claim shadowed** — fully re-executed by policy, at `shadow_rate_ppm`? `H(domain ‖ network ‖ anchor ‖ claim ‖ policy)`
/// below the rate. The anchor is round 0's, so the producer cannot know at commitment.
pub fn palw_detection_is_shadowed_v2(policy: &PalwDetectionPolicyV1, network_domain: &Hash64, anchor: &Hash64, claim_id: &Hash64) -> bool {
    if policy.shadow_rate_ppm == 0 {
        return false;
    }
    let mut s = keyed(PALW_DETECTION_SHADOW_DOMAIN_V2);
    s.update(network_domain.as_byte_slice());
    s.update(anchor.as_byte_slice());
    s.update(claim_id.as_byte_slice());
    s.update(policy.id().as_byte_slice());
    let seed = finish(s);
    let label = StreamLabelV1 { kind: StreamKindV1::Query, scope_id: [0u8; 64], relation: 0, repetition: 0 };
    ChallengeStreamV1::new(&seed, &label).index_below(PPM).is_ok_and(|x| x < u64::from(policy.shadow_rate_ppm))
}

/// The seat-round anchor domain.
pub const PALW_DETECTION_SEAT_ROUND_DOMAIN_V2: &[u8] = b"misaka-palw/detection/seat-round-anchor/v2";

/// **A seat's round-`r` anchor**: round 0 is the panel anchor itself (the block at the claim's anchor slot, ADR-0044 F4/F5); round
/// `r ≥ 1` is `H(domain ‖ panel_anchor ‖ r)`. Every round is therefore post-commit — the claim's commitment is fixed before the panel
/// anchor exists, and the producer speaks no further message a later round could be adapted to — and no new beacon is introduced.
/// (A party that re-checks LATER, after the producer has served or withheld — a redrawn panel, a public verifier — passes real later
/// chain anchors to [`palw_detection_draw_rounds_v2`] instead.)
pub fn palw_detection_seat_round_anchor_v2(panel_anchor: &Hash64, round: u32) -> Hash64 {
    if round == 0 {
        return *panel_anchor;
    }
    let mut s = keyed(PALW_DETECTION_SEAT_ROUND_DOMAIN_V2);
    s.update(panel_anchor.as_byte_slice());
    s.update(&round.to_le_bytes());
    Hash64::from_bytes(finish(s))
}

/// **A seat's whole duty under a multi-round policy**: rounds `0..rounds` of `k_round` each, every round distinct from the earlier
/// ones (the v2 exclusion), from the seat-round anchors. Re-derivable from the panel anchor alone.
pub fn palw_detection_seat_rounds_v2(
    binding: &PalwDetectionDrawBindingV2,
    panel_anchor: &Hash64,
    k_round: u32,
    rounds: u32,
    n: u32,
) -> Result<(Vec<Vec<u32>>, BTreeSet<u32>), PalwDetectionDrawRefusalV2> {
    let mut inspected = BTreeSet::new();
    let mut out = Vec::with_capacity(rounds as usize);
    for r in 0..rounds {
        let drawn = palw_detection_draw_round_v2(binding, r, &palw_detection_seat_round_anchor_v2(panel_anchor, r), k_round, n, &inspected)?;
        inspected.extend(drawn.iter().copied());
        out.push(drawn);
    }
    Ok((out, inspected))
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The claim gate (state transition) and the mirror
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The detection policy at the claim gate** (`palw_detection_policy_v1`, keyed on the claim's acceptance DAA): `None` below the
/// fence or when the class's policy admits the claim; else `Some((code, why))`, refused exactly as G-01 option (b) refuses — no claim,
/// no reward, weight or consensus credit; a refused own attempt is skipped, a free-prompt carrier pays its ordinary fee. Refused: a
/// class with no policy, a claim whose collectable collateral is below the deterrence bound, a claim the policy cannot check within
/// its resources. The kernel version is the class's own registered binding (0) on the V2 lanes.
pub fn palw_detection_claim_refusal_v1(
    params: &crate::palw_state_v2::PalwStateParamsV2,
    class_id: &Hash64,
    accepted_daa: u64,
    facts: &PalwDetectionClaimFactsV1,
) -> Option<(&'static str, String)> {
    let regime = palw_detection_regime_v1(params.detection_policy_v1(), class_id, 0, accepted_daa);
    match palw_detection_admit_claim_v1(regime, facts) {
        Ok(_) => None,
        Err(e) => Some((e.code(), format!("palw_detection_policy_v1: {e} — no reward, weight or consensus credit"))),
    }
}

/// What a detected claim became on chain, for the funnel's later stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDetectionChainOutcomeV1 {
    InFlight,
    /// A data-availability dispute is open over it (accepted, undecided).
    Disputed,
    /// Voided on a proven fraud (court verdict / default charged as fraud / a work slice proven false).
    Convicted,
    /// Voided on the producer's confirmed withholding (or a defaulted work slice).
    Defaulted,
    /// It reached `Final` — after a detection, a deadline loss.
    Final,
    /// Voided because no panel was ever bound or capable — unassigned.
    Unassigned,
    /// Voided for another reason (timeouts, quorum): no reward, but no conviction either.
    OtherVoid,
}

/// The chain outcome of a claim phase.
pub fn palw_detection_chain_outcome_v1(phase: &crate::palw_state_v2::PalwClaimPhaseV2) -> PalwDetectionChainOutcomeV1 {
    use crate::palw_state_v2::{PalwClaimPhaseV2 as P, PalwVoidReasonV2 as V};
    use PalwDetectionChainOutcomeV1 as O;
    match phase {
        P::Voided { reason, .. } => match reason {
            V::CourtFraud | V::CourtDefault | V::CourtHeldVerdict | V::WorkSliceProvenFalse => O::Convicted,
            V::ProducerWithholding | V::WorkSliceDefaulted => O::Defaulted,
            V::BindTimeout | V::NoCapablePanel | V::PermissionlessNoCapablePanel => O::Unassigned,
            _ => O::OtherVoid,
        },
        P::DefaultDisputed { .. } => O::Disputed,
        P::Final { .. } => O::Final,
        _ => O::InFlight,
    }
}

/// **The stages (and loss) a detected claim's chain outcome implies**: a dispute or a conviction implies the fault was localised and
/// filed by someone and the filing accepted; a conviction or default invalidates the claim's reward and rights; `collected` needs the
/// producer bond's `slashed` to have risen since the detection. A detected claim that reached `Final` is a deadline loss; one that was
/// never assigned, an unassigned loss.
pub fn palw_detection_follow_v1(
    outcome: PalwDetectionChainOutcomeV1,
    slashed_since_detection: bool,
) -> (Vec<PalwDetectionStageV1>, Option<PalwDetectionLossV1>) {
    use PalwDetectionChainOutcomeV1 as O;
    use PalwDetectionStageV1::*;
    let decided = |verdict: PalwDetectionStageV1| {
        let mut v = vec![Localized, Filed, Accepted, verdict, Invalidated];
        if slashed_since_detection {
            v.push(Collected);
        }
        v
    };
    match outcome {
        O::InFlight | O::OtherVoid => (Vec::new(), None),
        O::Disputed => (vec![Localized, Filed, Accepted], None),
        O::Convicted => (decided(Convicted), None),
        O::Defaulted => (decided(Defaulted), None),
        O::Final => (Vec::new(), Some(PalwDetectionLossV1::DeadlineExpiry)),
        O::Unassigned => (Vec::new(), Some(PalwDetectionLossV1::Unassigned)),
    }
}

impl crate::config::params::Params {
    /// The fence's mirror for the V2 bundle's state params (`never()` collapses to `None`).
    pub fn palw_detection_policy_v1_mirror(&self) -> Option<PalwDetectionPolicyFenceV1> {
        self.palw_detection_policy_v1.clone().filter(|f| f.activation != ForkActivation::never())
    }

    /// Writes the mirror onto the V2 bundle's state params (the claim gates read it there).
    pub fn sync_palw_detection_policy_v1(&mut self) {
        let mirror = self.palw_detection_policy_v1_mirror();
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_detection_policy_v1(mirror);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The inspection (v2): the seat's replay over the policy's draw
// ---------------------------------------------------------------------------------------------------------------------------------

/// What one checker concluded about one round's draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwDetectionOutcomeV2 {
    /// Every drawn interval answered and replayed clean.
    Certified,
    /// A mismatch, localised to the leaf court's address. Not a conviction.
    Fault { interval_index: u32, first_leaf_index: u64, leaf_count: u64 },
    /// Drawn intervals with no opening that binds to the claim: not certified, never an accusation (the DA/default path).
    NotVerified { unanswered: Vec<u32> },
}

/// **Replay the drawn intervals with the class's own kernels** — the same contract as the seat's v1 loop
/// (`kaspad::palw_fp_seat::palw_fp_seat_verify_openings_v1`): an interval is answered if ANY candidate replays clean, a fault
/// short-circuits, a candidate that does not bind is skipped.
pub fn palw_detection_inspect_v2(
    backend: &dyn crate::palw_backend::PalwExecutionBackendV1,
    roots: crate::palw_backend::PalwClaimRootsV1,
    prompt_token_ids: &[u32],
    work_leaves: u64,
    drawn: &[u32],
    openings: &dyn Fn(u32) -> Vec<Vec<u8>>,
    opening_ceiling: usize,
) -> PalwDetectionOutcomeV2 {
    use crate::palw_backend::PalwFpIntervalVerdictV1 as V;
    let mut unanswered = Vec::new();
    for index in drawn {
        let mut answered = false;
        for candidate in openings(*index) {
            if candidate.len() > opening_ceiling {
                continue;
            }
            match backend.verify_fp_interval_opening(&candidate, roots, *index, prompt_token_ids, work_leaves) {
                V::Valid => {
                    answered = true;
                    break;
                }
                V::Fault { leaf_index } => {
                    return PalwDetectionOutcomeV2::Fault { interval_index: *index, first_leaf_index: leaf_index, leaf_count: 1 };
                }
                V::FaultInRange { first_leaf_index, leaf_count } => {
                    return PalwDetectionOutcomeV2::Fault { interval_index: *index, first_leaf_index, leaf_count };
                }
                V::Mismatch | V::Unverifiable => {}
            }
        }
        if !answered {
            unanswered.push(*index);
        }
    }
    if unanswered.is_empty() { PalwDetectionOutcomeV2::Certified } else { PalwDetectionOutcomeV2::NotVerified { unanswered } }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The effective-rate funnel (node-side counters; nothing consensus reads)
// ---------------------------------------------------------------------------------------------------------------------------------

/// The funnel's stages, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwDetectionStageV1 {
    Eligible = 0,
    Checked = 1,
    Detected = 2,
    Localized = 3,
    Filed = 4,
    Accepted = 5,
    Convicted = 6,
    Defaulted = 7,
    Collected = 8,
    Invalidated = 9,
}

/// Losses recorded apart from the stages — never folded into the sampling miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwDetectionLossV1 {
    CapacityRefusal = 0,
    FeeShortfall = 1,
    MissingMaterial = 2,
    DeadlineExpiry = 3,
    Unassigned = 4,
    PolicyRefused = 5,
    GeometryMismatch = 6,
}

pub const PALW_DETECTION_STAGE_NAMES_V1: [&str; 10] =
    ["eligible", "checked", "detected", "localized", "filed", "accepted", "convicted", "defaulted", "collected", "invalidated"];
pub const PALW_DETECTION_LOSS_NAMES_V1: [&str; 7] =
    ["capacity_refusal", "fee_shortfall", "missing_material", "deadline_expiry", "unassigned", "policy_refused", "geometry_mismatch"];

/// **eligible → checked → detected → localized → filed → accepted → convicted/defaulted → collected → invalidated**, plus the losses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwDetectionFunnelV1 {
    pub stages: [u64; 10],
    pub losses: [u64; 7],
}

impl PalwDetectionFunnelV1 {
    pub fn record(&mut self, stage: PalwDetectionStageV1) {
        let s = &mut self.stages[stage as usize];
        *s = s.saturating_add(1);
    }

    pub fn record_loss(&mut self, loss: PalwDetectionLossV1) {
        let s = &mut self.losses[loss as usize];
        *s = s.saturating_add(1);
    }

    pub fn stage(&self, stage: PalwDetectionStageV1) -> u64 {
        self.stages[stage as usize]
    }

    pub fn loss(&self, loss: PalwDetectionLossV1) -> u64 {
        self.losses[loss as usize]
    }

    pub fn merge(&mut self, other: &Self) {
        for (a, b) in self.stages.iter_mut().zip(other.stages) {
            *a = a.saturating_add(b);
        }
        for (a, b) in self.losses.iter_mut().zip(other.losses) {
            *a = a.saturating_add(b);
        }
    }

    /// The funnel's own invariant: each stage at most its predecessor (convicted + defaulted at most accepted, and collected at most
    /// their sum). A violation is a recorder bug, not a measurement.
    pub fn is_consistent(&self) -> bool {
        use PalwDetectionStageV1::*;
        let g = |s| self.stage(s);
        g(Checked) <= g(Eligible)
            && g(Detected) <= g(Checked)
            && g(Localized) <= g(Detected)
            && g(Filed) <= g(Localized)
            && g(Accepted) <= g(Filed)
            && g(Convicted).saturating_add(g(Defaulted)) <= g(Accepted)
            && g(Collected) <= g(Convicted).saturating_add(g(Defaulted))
            && g(Invalidated) <= g(Convicted).saturating_add(g(Defaulted))
    }

    /// The end-to-end rate `invalidated / eligible`, ppm (floor) — the number the audit's §5.3 asks for, measured, not assumed.
    pub fn effective_rate_ppm(&self) -> u64 {
        let e = self.stage(PalwDetectionStageV1::Eligible);
        if e == 0 { 0 } else { self.stage(PalwDetectionStageV1::Invalidated).saturating_mul(PPM) / e }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Design values for the genesis families (NOT ARMED; unmeasured)
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The genesis design table** — a policy template per route, with a caller-supplied class id (roots are not invented). Every cost,
/// unit and byte value is a DESIGN value until measured on the real artifact (`docs/design/palw/g14-detection-policy-2026-10-17.md`
/// §7); the fence refuses arming regardless.
pub fn palw_detection_genesis_design_v1(class_id: Hash64, route: PalwDetectionRouteV1, n_rule: PalwIntervalCountRuleV1) -> PalwDetectionPolicyV1 {
    let interval = matches!(route, PalwDetectionRouteV1::Base0A16 | PalwDetectionRouteV1::PalwQ361);
    PalwDetectionPolicyV1 {
        version: PALW_DETECTION_POLICY_VERSION_V1,
        class_id,
        kernel_version: 0,
        route,
        method: if interval { PalwInspectionMethodV1::IntervalReplay } else { PalwInspectionMethodV1::FullReexecution },
        n_rule,
        n_max: 32_768,
        // DESIGN: the canonical job's interval count is the class's (unmeasured here); 64 is a shape value.
        attempt_intervals: 64,
        // 99% on faults that make at least 10% of the intervals detectably wrong; below that, the deterrence bound and full
        // re-execution are the cover (stated, not claimed by sampling).
        fault_model: PalwFaultModelV1 { d_min_abs: 1, d_min_ppm: 100_000 },
        epsilon: PalwRatioV1::new(1, 100),
        k_max: 64,
        shadow_rate_ppm: 10_000,
        economics: PalwDetectionEconomicsV1 {
            cost_per_interval_sompi: 1_000,
            collateral_per_interval_sompi: 1_200,
            check_coverage_ppm: 1_000_000,
            collection_ppm: 1_000_000,
            margin_permille: 1_100,
            fixed_gain_sompi: None,
        },
        resources: PalwDetectionResourcesV1 {
            opening_bytes: 1 << 20,
            max_claim_fetch_bytes: 64 << 20,
            replay_units_per_interval: 1,
            max_claim_replay_units: if interval { 64 } else { 32_768 },
            checker_units_per_daa: if interval { 8 } else { 64 },
            fetch_daa: 10,
            claims_in_flight: 32,
            retention_pairs_cap: 2_048,
            court_openings_per_claim: 64,
            da_bytes_per_claim: 64 << 20,
        },
        deadlines: PalwDetectionDeadlinesV1 { inspection_deadline_daa: 600, round_spacing_daa: 0, rounds: 1 },
        localisation: if interval { PalwLocalisationV1::IntervalLeafCourt } else { PalwLocalisationV1::RootDescentElementCourt },
    }
}

impl PalwDetectionRefusalV1 {
    /// A stable name per refusal (for `PalwModeV2Error::Invalid` and the metrics' log lines).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed(why) => *why,
            Self::NoPolicyForClass => "DETECTION_NO_POLICY_FOR_CLASS",
            Self::NotAtTheClaimsPrice { .. } => "DETECTION_NOT_AT_THE_CLAIMS_PRICE",
            Self::NoIntervals => "DETECTION_NO_INTERVALS",
            Self::IntervalCountAboveCeiling { .. } => "DETECTION_N_ABOVE_N_MAX",
            Self::GeometryMismatch { .. } => "DETECTION_GEOMETRY_MISMATCH",
            Self::CoverageBelowTarget { .. } => "DETECTION_COVERAGE_BELOW_TARGET",
            Self::DetectionTargetUnreachable { .. } => "DETECTION_TARGET_UNREACHABLE_WITHIN_K_MAX",
            Self::CollateralBelowDeterrence { .. } => "DETECTION_COLLATERAL_BELOW_DETERRENCE_BOUND",
            Self::BoundOverflow => "DETECTION_BOUND_OVERFLOW",
            Self::FetchBytesOverBudget { .. } => "DETECTION_FETCH_BYTES_OVER_BUDGET",
            Self::ReplayUnitsOverBudget { .. } => "DETECTION_REPLAY_UNITS_OVER_BUDGET",
            Self::RetentionBelowDraw { .. } => "DETECTION_RETENTION_BELOW_DRAW",
            Self::CourtBudgetBelowDraw { .. } => "DETECTION_COURT_BUDGET_BELOW_DRAW",
            Self::DeadlineUnreachable { .. } => "DETECTION_DEADLINE_UNREACHABLE",
        }
    }
}

impl crate::config::params::Params {
    /// **The fence's refusals**: a malformed or un-armable policy is named (one per `(class, kernel_version)`, each passing its own
    /// plan at `N = 1`, the powers of two and `n_max` with the class's own collateral rule); then any armed height is refused — the
    /// per-class costs, replay units, bytes and throughputs are design values without a measured record on the real artifacts.
    pub fn validate_palw_detection_policy_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.detection_policy_v1().cloned() != self.palw_detection_policy_v1_mirror()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_detection_policy_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_detection_policy_v1",
            ));
        }
        let Some(fence) = self.palw_detection_policy_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        fence.validate().map_err(|e| PalwModeV2Error::Invalid(e.code()))?;
        Err(PalwModeV2Error::Invalid(
            "palw_detection_policy_v1 cannot be armed: the per-class cost, replay-unit, byte and throughput values are design values without a measured record on the real genesis artifacts (g14-detection-policy-2026-10-17 §7)",
        ))
    }
}
