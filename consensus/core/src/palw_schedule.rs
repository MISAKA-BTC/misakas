//! PALW challenge scheduling v1 — the ADR-0028 fabric at Stage 0 (shadow).
//!
//! Normative source: ADR-0028 (Accepted 2026-08-16). Three things live here, and all three are
//! **computed and logged only** — no consensus validation, fork choice, acceptance or credit
//! path reads any of it:
//!
//! * **§2 assignment** — [`select_replay_panel_v1`], the domain-keyed twin of
//!   `vlt::select_verifiers`: deterministic, class-scoped, executor-excluded. The ADR's
//!   eligibility rule (`bonded ∧ registered class ∧ not frozen ∧ ≠ executor`) is enforced HERE,
//!   not left to callers, because a duty must be derivable identically by every observer for
//!   no-show to ever become an objective offense.
//! * **§3 windows** — [`PalwScheduleParamsV1`] with the Stage-1 placeholder defaults for both
//!   PALW networks, and [`PalwScheduleParamsV1::validate`], which enforces the ADR's inequality
//!   set against the *real* network constants. The test suite pins the exact failure this
//!   review caught (a 48 h challenge window exceeds both pruning horizons) so the mistake
//!   cannot be re-made silently.
//! * **§6 Stage-0 telemetry** — [`PalwShadowLedgerV1`], the shadow ledger that classifies duty
//!   observations against a job's schedule and aggregates the §12 gate artifacts: observed
//!   check rate (the honest name for measured-not-assumed `P_check`), no-show and mismatch
//!   counts, attestation/refutation inclusion latency, and replay-cost percentiles.
//!
//! Numbers in reports are **counts and nearest-rank percentiles** — no floats, no averages —
//! so two observers of the same events publish byte-identical artifacts.

use crate::config::params::BlockrateParams;
use kaspa_hashes::Hash64;
use std::collections::HashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b domain of the assignment ticket (ADR-0028 §2). Must never equal
/// `vlt::VERIFIER_SORTITION_KEY`: the two lotteries are twins by construction, and one shared
/// key would make a VLT verifier draw predict a PALW panel draw.
pub const PALW_SCHEDULE_DOMAIN_ASSIGNMENT_TICKET: &[u8] = b"misaka-palw/v2-replay-assignment-ticket/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_SCHEDULE_ALL_DOMAINS: &[&[u8]] = &[PALW_SCHEDULE_DOMAIN_ASSIGNMENT_TICKET];

/// The degraded-ladder round budget the challenge window must fit (ADR-0028 §3, ADR-0027 §1:
/// ≈ log₂ of the step count at the credited ceiling).
pub const PALW_SCHEDULE_LADDER_ROUNDS: u64 = 20;

/// `κ` — every response window must be at least this multiple of the class's measured
/// `p99_cold_replay` (ADR-0028 §3).
pub const PALW_SCHEDULE_REPLAY_KAPPA: u64 = 3;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwScheduleError {
    #[error("unsupported palw-schedule object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("window parameters are not canonical: {reason}")]
    ParamsNotCanonical { reason: &'static str },
    #[error("window inequality violated: {rule}")]
    WindowInequalityViolated { rule: &'static str },
    #[error("DAA arithmetic overflow computing {what}")]
    DaaOverflow { what: &'static str },
}

// ---------------------------------------------------------------------------------------------
// §2 — the assignment ticket
// ---------------------------------------------------------------------------------------------

/// One candidate for a replay panel. The flags carry the ADR-0028 §2 eligibility facts so the
/// rule lives in the function; the CALLER is responsible for the flags being the chain's truth
/// at the anchor (that lookup is Stage-1 wiring, not this module's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelCandidateV1 {
    pub validator_id: Hash64,
    pub runtime_class_id: Hash64,
    pub bonded: bool,
    pub frozen: bool,
}

/// The ADR-0028 §2 duty lottery: `ticket(v) = H(domain ‖ C ‖ executor_id ‖ anchor ‖ v)`, the
/// `q` lowest tickets over the eligible set, `validator_id` as the tie-break — exactly
/// `vlt::select_verifiers`' construction under this module's own domain key.
///
/// Deterministic in every input and invariant under candidate order; a class with fewer than
/// `q` eligible members yields a smaller panel (whether that panel may credit is §1's gate,
/// not this function's). Nothing here relies on the anchor being unpredictable — see the ADR:
/// a known panel is safe because replays are full and refutation is permissionless.
pub fn select_replay_panel_v1(
    commitment_root: &Hash64,
    executor_id: &Hash64,
    anchor: &Hash64,
    runtime_class_id: &Hash64,
    candidates: &[PalwPanelCandidateV1],
    q: usize,
) -> Vec<Hash64> {
    if q == 0 {
        return Vec::new();
    }
    let mut ticketed: Vec<(Hash64, Hash64)> = candidates
        .iter()
        .filter(|c| c.bonded && !c.frozen && c.runtime_class_id == *runtime_class_id && c.validator_id != *executor_id)
        .map(|c| {
            let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_SCHEDULE_DOMAIN_ASSIGNMENT_TICKET).to_state();
            hasher.update(commitment_root.as_byte_slice());
            hasher.update(executor_id.as_byte_slice());
            hasher.update(anchor.as_byte_slice());
            hasher.update(c.validator_id.as_byte_slice());
            let mut out = [0u8; 64];
            out.copy_from_slice(hasher.finalize().as_bytes());
            (Hash64::from_bytes(out), c.validator_id)
        })
        .collect();
    // Ticket first, `validator_id` as the tie-break: candidates whose tickets collide must
    // still order identically on every node.
    ticketed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    ticketed.truncate(q);
    ticketed.into_iter().map(|(_, id)| id).collect()
}

// ---------------------------------------------------------------------------------------------
// §3 — windows
// ---------------------------------------------------------------------------------------------

pub const PALW_SCHEDULE_PARAMS_VERSION_V1: u16 = 1;

/// The per-class window parameters, DAA-denominated. Registered at class registration in later
/// stages; the constructors below are the ADR-0028 §3 **Stage-1 placeholders**, and
/// [`Self::validate`] is the rule they are one solution of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwScheduleParamsV1 {
    /// = [`PALW_SCHEDULE_PARAMS_VERSION_V1`].
    pub version: u16,
    /// Anchor offset: a settling offset, not a finality bound (ADR-0028 §2).
    pub delta_bind: u64,
    /// Assigned duty: attest or refute within this many DAA of the anchor.
    pub w_replay: u64,
    /// Opening-call answer window.
    pub w_answer: u64,
    /// Bisection rung response window.
    pub w_round: u64,
    /// Permissionless refutation window from `daa(C)`; `credit(C)` evaluates at its close.
    pub w_challenge: u64,
    /// Slack between challenge close and the pruning horizon for prosecution to land.
    pub prosecution_slack: u64,
    /// Funded panel size.
    pub q: u16,
}

impl PalwScheduleParamsV1 {
    /// ADR-0028 §3 defaults on the 0.1-bps network (10 s blocks): 20 min / 1 h / 1 h / 1 h /
    /// 24 h, slack 1 h, `q = 2`.
    pub fn stage1_defaults_deci_bps() -> Self {
        Self {
            version: PALW_SCHEDULE_PARAMS_VERSION_V1,
            delta_bind: 120,
            w_replay: 360,
            w_answer: 360,
            w_round: 360,
            w_challenge: 8_640,
            prosecution_slack: 360,
            q: 2,
        }
    }

    /// The same wall-clock intent on the 120 s public PALW testnet.
    pub fn stage1_defaults_two_minute_bps() -> Self {
        Self {
            version: PALW_SCHEDULE_PARAMS_VERSION_V1,
            delta_bind: 10,
            w_replay: 30,
            w_answer: 30,
            w_round: 30,
            w_challenge: 720,
            prosecution_slack: 30,
            q: 2,
        }
    }

    /// DAA the ladder leaves unused inside the challenge window.
    pub fn ladder_margin(&self) -> u64 {
        self.w_challenge.saturating_sub(self.w_replay.saturating_add(PALW_SCHEDULE_LADDER_ROUNDS.saturating_mul(self.w_round)))
    }

    /// The ADR-0028 §3 inequality set, checked against the network the class registers on.
    /// This is the check whose absence let the first draft ship a 48 h challenge window that
    /// no PALW network's pruning horizon can hold.
    pub fn validate(&self, blockrate: &BlockrateParams) -> Result<(), PalwScheduleError> {
        if self.version != PALW_SCHEDULE_PARAMS_VERSION_V1 {
            return Err(PalwScheduleError::UnsupportedVersion { got: self.version, expected: PALW_SCHEDULE_PARAMS_VERSION_V1 });
        }
        if self.q == 0 {
            return Err(PalwScheduleError::ParamsNotCanonical { reason: "q is zero — nobody is funded to replay" });
        }
        if self.delta_bind == 0 || self.w_replay == 0 || self.w_answer == 0 || self.w_round == 0 || self.w_challenge == 0 {
            return Err(PalwScheduleError::ParamsNotCanonical { reason: "a window is zero" });
        }
        let ladder = self
            .w_replay
            .checked_add(
                PALW_SCHEDULE_LADDER_ROUNDS
                    .checked_mul(self.w_round)
                    .ok_or(PalwScheduleError::DaaOverflow { what: "ladder budget" })?,
            )
            .ok_or(PalwScheduleError::DaaOverflow { what: "ladder budget" })?;
        if self.w_challenge < ladder {
            return Err(PalwScheduleError::WindowInequalityViolated { rule: "w_challenge ≥ w_replay + LADDER_ROUNDS · w_round" });
        }
        // Duties must resolve before the credit gate evaluates.
        let duty_close = self.delta_bind.checked_add(self.w_replay).ok_or(PalwScheduleError::DaaOverflow { what: "duty close" })?;
        if duty_close >= self.w_challenge {
            return Err(PalwScheduleError::WindowInequalityViolated { rule: "delta_bind + w_replay < w_challenge" });
        }
        // Anchor-referencing offenses prosecute only after the anchor is final.
        if blockrate.finality_depth >= self.w_challenge {
            return Err(PalwScheduleError::WindowInequalityViolated { rule: "finality_depth < w_challenge" });
        }
        // The binding constraint: the whole dispute must close inside the pruning horizon.
        let challenge_plus_slack = self
            .w_challenge
            .checked_add(self.prosecution_slack)
            .ok_or(PalwScheduleError::DaaOverflow { what: "challenge + prosecution slack" })?;
        if challenge_plus_slack >= blockrate.pruning_depth {
            return Err(PalwScheduleError::WindowInequalityViolated { rule: "w_challenge + prosecution_slack < pruning_depth" });
        }
        Ok(())
    }
}

/// The absolute deadlines of one job commitment (ADR-0028 §3 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwJobScheduleV1 {
    pub commit_daa: u64,
    /// `commit + delta_bind`: the anchor block's DAA score; panel derivable from here.
    pub anchor_daa: u64,
    /// `anchor + w_replay`: assigned attestations/refutations due.
    pub replay_deadline_daa: u64,
    /// `commit + w_challenge`: `credit(C)` evaluates here.
    pub challenge_close_daa: u64,
}

pub fn job_schedule_v1(params: &PalwScheduleParamsV1, commit_daa: u64) -> Result<PalwJobScheduleV1, PalwScheduleError> {
    let anchor_daa = commit_daa.checked_add(params.delta_bind).ok_or(PalwScheduleError::DaaOverflow { what: "anchor" })?;
    let replay_deadline_daa =
        anchor_daa.checked_add(params.w_replay).ok_or(PalwScheduleError::DaaOverflow { what: "replay deadline" })?;
    let challenge_close_daa =
        commit_daa.checked_add(params.w_challenge).ok_or(PalwScheduleError::DaaOverflow { what: "challenge close" })?;
    Ok(PalwJobScheduleV1 { commit_daa, anchor_daa, replay_deadline_daa, challenge_close_daa })
}

/// Deadline of an opening-call answer posted at `call_daa` (ADR-0028 §5).
pub fn answer_deadline_v1(params: &PalwScheduleParamsV1, call_daa: u64) -> Result<u64, PalwScheduleError> {
    call_daa.checked_add(params.w_answer).ok_or(PalwScheduleError::DaaOverflow { what: "answer deadline" })
}

/// The §3 fit check a class's measured replay cost must pass:
/// `κ · p99_cold_replay ≤ w_replay` (all in milliseconds via the network's block time).
pub fn replay_p99_fits_v1(p99_cold_replay_ms: u64, params: &PalwScheduleParamsV1, target_time_per_block_ms: u64) -> bool {
    let window_ms = params.w_replay.saturating_mul(target_time_per_block_ms);
    PALW_SCHEDULE_REPLAY_KAPPA.saturating_mul(p99_cold_replay_ms) <= window_ms
}

// ---------------------------------------------------------------------------------------------
// §3 credited-ceiling re-derivation — the registration value from a measurement (B13)
// ---------------------------------------------------------------------------------------------

/// The fixed part of a cold replay: model load plus process spin-up, independent of decode
/// depth. Subtracted before dividing the residual by the per-token cost, so the ceiling is
/// not depressed by a large constant on a short measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReplayCostMeasurementV1 {
    /// The class's SLOWEST host, cold: the fixed overhead (ms) — the load-only p99.
    pub fixed_overhead_ms: u64,
    /// The class's slowest host: measured milliseconds per decode token at depth (the
    /// marginal cost — `(p99(D) − fixed) / D` from the bench, taken at the largest measured D
    /// so the marginal estimate is not overwhelmed by the fixed part).
    pub ms_per_decode_token: u64,
    /// The format ceiling — the largest decode-token count the v2 wire can express. The
    /// credited ceiling can never exceed it regardless of how fast the class is.
    pub format_ceiling_tokens: u32,
}

/// Re-derives the **credited-job ceiling** — the largest decode-token count a job may claim
/// and still be creditable — from a class's measured replay cost against its registered
/// windows (ADR-0028 §3's rule, made a function so registration cannot ship a guessed value).
///
/// The window a cold replay of `D` decode tokens must fit is `w_replay` (κ folded in): the
/// derivation solves `κ · (fixed + D · per_token) ≤ w_replay · block_ms` for the largest `D`,
/// then floors at the format ceiling. Returns 0 when even a zero-decode job cannot fit
/// (a class too slow to register at all — an honest, if unwelcome, answer).
pub fn credited_ceiling_tokens_v1(
    measurement: &PalwReplayCostMeasurementV1,
    params: &PalwScheduleParamsV1,
    target_time_per_block_ms: u64,
) -> u32 {
    let window_ms = params.w_replay.saturating_mul(target_time_per_block_ms);
    let budget_ms = window_ms / PALW_SCHEDULE_REPLAY_KAPPA; // κ·(fixed + D·per) ≤ window
    let Some(residual) = budget_ms.checked_sub(measurement.fixed_overhead_ms) else {
        return 0; // the fixed overhead alone overruns κ⁻¹ of the window
    };
    if measurement.ms_per_decode_token == 0 {
        return measurement.format_ceiling_tokens; // a class with no marginal cost is format-bound
    }
    let derived = residual / measurement.ms_per_decode_token;
    derived.min(measurement.format_ceiling_tokens as u64) as u32
}

// ---------------------------------------------------------------------------------------------
// §4e leverage remedy — the aggregate inequality, encoded (B15 amendment, 2026-08-16)
// ---------------------------------------------------------------------------------------------

/// `λ` in tenths (2.0) — the §4e economic-security factor: refutation must put at least
/// `λ ·` the mintable gain at risk. Named apart from `vlt::lambda_vlt_per_kas`, which is a
/// different concept (a collateral ceiling per KAS, not a security multiple).
pub const PALW_LEVERAGE_LAMBDA_X10: u64 = 20;

/// ADR-0028 §4e (2026-08-16 amendment), encoded. The AGGREGATE reading of `max_leverage ≤ 1`
/// governs: credit mintable by one validator within one unbonding period must not exceed
/// `S_eff / λ` — nothing else stops cheating job after job against one bond. The amendment's
/// two credible remedies are the same inequality solved for different variables, so ONE
/// encoding carries both levers:
///
/// * the per-validator credited-job **rate cap** — `min_credit_interval_daa` with the full
///   subsidy (`base_subsidy_permille = 1000`);
/// * the **fractional `base(C)`** — a small `base_subsidy_permille` at a chosen credit rate.
///
/// The registration chooses the pair; [`max_leverage_holds_v1`] is the one check both must
/// pass. Integer per-mille follows the registry's `rho_v_permille` convention: no floats in
/// a hashed preimage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwLeverageRemedyV1 {
    /// Minimum DAA distance between two credited jobs of ONE validator. `1` = every block
    /// (no rate lever — the fraction must carry the whole inequality). `0` is not a remedy.
    pub min_credit_interval_daa: u64,
    /// `base(C)` as a per-mille fraction of the block subsidy (`1000` = the whole subsidy).
    pub base_subsidy_permille: u32,
}

/// The chain facts the §4e inequality reads — deliberately NOT part of any hashed
/// registration preimage: subsidy schedule, bond size and unbonding period are network
/// facts, not claims a registrant may assert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicFactsV1 {
    /// The block subsidy at the crediting height (sompi).
    pub block_subsidy_sompi: u64,
    /// `S_eff` — the slashable bond reachable by refutation (sompi).
    pub s_eff_sompi: u64,
    /// The validator unbonding period (blocks).
    pub unbonding_period_blocks: u64,
}

/// ONE credited job's full mint in sompi: `base(C)` plus its `q` attester shares, each
/// `ρ_v · base(C)`.
///
/// THE single definition of that amount. Three rules need it and must not be able to
/// disagree: the per-block crediting ceiling that pays it out, the §4e inequality that
/// decides whether the bond covers it, and the tests that pin the admissible remedies. It
/// lives beside the remedy rather than on the registration because `max_leverage_holds_v1`
/// must be able to reach it without a registration in hand;
/// [`crate::palw_registry::PalwClassRegistrationV1::one_job_payout_sompi`] and
/// [`crate::palw_credit::PalwCreditParamsV1::one_job_ceiling_sompi`] both delegate here.
///
/// Every step saturates. `q` is `u16` and `ρ_v` is per-mille, so the product is bounded in
/// practice — but a saturated payout must read as "enormous", which fails the inequality,
/// and never wrap to "small", which would pass it.
pub fn one_job_payout_sompi_v1(remedy: &PalwLeverageRemedyV1, rho_v_permille: u32, q: u16, block_subsidy_sompi: u64) -> u64 {
    let base = ((block_subsidy_sompi as u128) * (remedy.base_subsidy_permille as u128) / 1000) as u64;
    let share = ((base as u128) * (rho_v_permille as u128) / 1000) as u64;
    base.saturating_add(share.saturating_mul(q as u64))
}

/// The §4e aggregate inequality: `λ · G_max ≤ S_eff`, where `G_max` is the credit ONE
/// validator can mint inside one unbonding period under this remedy.
///
/// `one_job_payout_sompi` is the caller's own per-job mint amount, and the choice of that
/// unit is the whole correctness question. This function used to derive it from the remedy
/// as `base(C)` alone, while the crediting walk's ceiling drained
/// `base(C) + q · ρ_v · base(C)` — so the inequality was validated against strictly less
/// than the code can mint, by a factor of `1 + q · ρ_v / 1000` (2× at illustrative live
/// parameters, and unbounded in `q`). The attester shares belong in `G_max` because the
/// party §4e reasons about is a party that also holds the panel bonds — a Sybil ring pays
/// its own attesters — and because every sompi of that payout enters circulation on the
/// strength of ONE refutable job, backed by the one bond refutation can reach.
///
/// So the unit is now supplied, and there is exactly one function that computes it:
/// [`crate::palw_registry::PalwClassRegistrationV1::one_job_payout_sompi`], which both the
/// ceiling and this check go through. A payout below the remedy's own `base(C)` fraction is
/// refused rather than trusted: that is the shape of the bug this parameter replaced, and a
/// caller that reintroduces it gets a closed gate instead of a silently weak bound.
///
/// Counting is conservative in the attacker's favor: a period of `U` blocks holds
/// `⌊U / interval⌋ + 1` credit opportunities (both ends included), so the continuous
/// `S_eff / (λ · base)` job budget from the amendment (≈ 2.2 jobs at live parameters) rounds
/// DOWN to an interval strictly wider than the continuous bound would suggest — the test
/// suite pins that difference. `base(C)` floors exactly as the mint arithmetic will.
pub fn max_leverage_holds_v1(remedy: &PalwLeverageRemedyV1, facts: &PalwEconomicFactsV1, one_job_payout_sompi: u64) -> bool {
    if remedy.min_credit_interval_daa == 0 || remedy.base_subsidy_permille > 1000 {
        return false; // not a canonical remedy encoding
    }
    let base_sompi = (facts.block_subsidy_sompi as u128) * (remedy.base_subsidy_permille as u128) / 1000;
    if (one_job_payout_sompi as u128) < base_sompi {
        return false; // a unit smaller than the size lever cannot bound the mint it authorizes
    }
    let jobs = (facts.unbonding_period_blocks / remedy.min_credit_interval_daa) as u128 + 1;
    let g_max = (one_job_payout_sompi as u128) * jobs;
    g_max * (PALW_LEVERAGE_LAMBDA_X10 as u128) <= (facts.s_eff_sompi as u128) * 10
}

// ---------------------------------------------------------------------------------------------
// §6 Stage 0 — the shadow ledger
// ---------------------------------------------------------------------------------------------

/// What was OBSERVED of one assigned duty by the time the observer reports (which must be
/// after `replay_deadline_daa` — a `Silent` before the deadline is not yet a fact).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDutyObservationV1 {
    /// The assignee published a bonded attestation.
    Attested { root_matched: bool, attest_daa: u64 },
    /// Nothing was published by the report time.
    Silent,
}

/// How the ledger classifies a duty against the schedule. `Mismatch` is timing-independent —
/// a signed non-matching root is contradiction material whenever it lands (ADR-0027 §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwDutyClassV1 {
    OnTimeMatch,
    LateMatch,
    Mismatch,
    NoShow,
}

/// Classification is a pure function of (observation, schedule) so every observer of the same
/// events reaches the same ledger.
pub fn classify_duty_v1(observation: &PalwDutyObservationV1, schedule: &PalwJobScheduleV1) -> PalwDutyClassV1 {
    match observation {
        PalwDutyObservationV1::Silent => PalwDutyClassV1::NoShow,
        PalwDutyObservationV1::Attested { root_matched: false, .. } => PalwDutyClassV1::Mismatch,
        PalwDutyObservationV1::Attested { root_matched: true, attest_daa } => {
            if *attest_daa <= schedule.replay_deadline_daa {
                PalwDutyClassV1::OnTimeMatch
            } else {
                PalwDutyClassV1::LateMatch
            }
        }
    }
}

/// Everything observed about one job commitment in shadow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShadowJobObservationV1 {
    pub schedule: PalwJobScheduleV1,
    /// One entry per assigned panel member.
    pub duties: Vec<PalwDutyObservationV1>,
    /// DAA score at which the first accepted refutation was included, if any.
    pub refutation_included_daa: Option<u64>,
    /// Measured cold replay costs for this job (worker `model_load + execute`), one per replay
    /// actually performed. Feeds the `p99_cold_replay` gate artifact.
    pub replay_durations_ms: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ClassAccumulator {
    jobs: u64,
    creditable_jobs: u64,
    jobs_with_on_time_match: u64,
    refuted_in_window_jobs: u64,
    refuted_after_close_jobs: u64,
    duties: u64,
    on_time_matches: u64,
    late_matches: u64,
    mismatches: u64,
    no_shows: u64,
    replay_ms: Vec<u64>,
    attest_latency_daa: Vec<u64>,
    refutation_latency_daa: Vec<u64>,
}

/// Nearest-rank percentiles over a sample set. `samples == 0` reports `None`s rather than
/// inventing a zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPercentilesV1 {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}

/// Nearest-rank percentile (deterministic, no interpolation): the value at rank
/// `⌈n · numerator / denominator⌉` (1-based) of the ascending-sorted samples.
pub fn nearest_rank_percentile(sorted_ascending: &[u64], numerator: u64, denominator: u64) -> Option<u64> {
    if sorted_ascending.is_empty() || denominator == 0 || numerator == 0 || numerator > denominator {
        return None;
    }
    let n = sorted_ascending.len() as u64;
    let rank = (n * numerator).div_ceil(denominator).max(1);
    Some(sorted_ascending[(rank - 1) as usize])
}

fn percentiles(mut samples: Vec<u64>) -> PalwPercentilesV1 {
    samples.sort_unstable();
    PalwPercentilesV1 {
        samples: samples.len() as u64,
        p50: nearest_rank_percentile(&samples, 50, 100),
        p95: nearest_rank_percentile(&samples, 95, 100),
        p99: nearest_rank_percentile(&samples, 99, 100),
        max: samples.last().copied(),
    }
}

/// One class's §12 gate artifact. Counts and percentiles only — every consumer derives its own
/// rates, so two observers of the same events publish identical artifacts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShadowClassReportV1 {
    pub runtime_class_id: Hash64,
    pub jobs: u64,
    /// Jobs the §1 gate would credit: ≥ 1 on-time matching attestation and no refutation
    /// included by challenge close.
    pub creditable_jobs: u64,
    /// The observed-`P_check` numerator: jobs with at least one on-time matching attestation.
    /// Named *observed* deliberately — `P_check` is measured, never assumed (ADR-0028 §4e).
    pub jobs_with_on_time_match: u64,
    pub refuted_in_window_jobs: u64,
    /// Refutations landing after challenge close: the job would have credited, and the slash
    /// arrives later (P3). The metric ADR-0028 assumption 2 worries about — watch its tail.
    pub refuted_after_close_jobs: u64,
    pub duties: u64,
    pub on_time_matches: u64,
    pub late_matches: u64,
    /// Signed non-matching roots — each one is `ClassContradictionCertificateV1` material.
    pub mismatches: u64,
    pub no_shows: u64,
    /// Cold replay cost (worker `model_load + execute`), milliseconds.
    pub replay_ms: PalwPercentilesV1,
    /// Attestation inclusion latency, DAA from the anchor (on-time and late alike).
    pub attest_latency_daa: PalwPercentilesV1,
    /// Refutation inclusion latency, DAA from commit.
    pub refutation_latency_daa: PalwPercentilesV1,
}

/// The Stage-0 shadow ledger: classify, count, and report. Process-local; never serialized by
/// this module (publishing the report is the harness's job, and wire-freezing a measurement
/// format before the measurements exist would be backwards).
#[derive(Clone, Debug, Default)]
pub struct PalwShadowLedgerV1 {
    classes: HashMap<Hash64, ClassAccumulator>,
}

impl PalwShadowLedgerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_job(&mut self, runtime_class_id: Hash64, observation: &PalwShadowJobObservationV1) {
        let acc = self.classes.entry(runtime_class_id).or_default();
        acc.jobs += 1;
        let mut on_time = false;
        for duty in &observation.duties {
            acc.duties += 1;
            match classify_duty_v1(duty, &observation.schedule) {
                PalwDutyClassV1::OnTimeMatch => {
                    acc.on_time_matches += 1;
                    on_time = true;
                }
                PalwDutyClassV1::LateMatch => acc.late_matches += 1,
                PalwDutyClassV1::Mismatch => acc.mismatches += 1,
                PalwDutyClassV1::NoShow => acc.no_shows += 1,
            }
            if let PalwDutyObservationV1::Attested { attest_daa, .. } = duty {
                acc.attest_latency_daa.push(attest_daa.saturating_sub(observation.schedule.anchor_daa));
            }
        }
        if on_time {
            acc.jobs_with_on_time_match += 1;
        }
        let refuted_in_window = match observation.refutation_included_daa {
            Some(included) if included <= observation.schedule.challenge_close_daa => {
                acc.refuted_in_window_jobs += 1;
                acc.refutation_latency_daa.push(included.saturating_sub(observation.schedule.commit_daa));
                true
            }
            Some(included) => {
                acc.refuted_after_close_jobs += 1;
                acc.refutation_latency_daa.push(included.saturating_sub(observation.schedule.commit_daa));
                false
            }
            None => false,
        };
        // The §1 gate, evaluated as shadow: a late refutation does NOT block credit — it
        // arrives as a later slash (P3), which is exactly why refuted_after_close is counted.
        if on_time && !refuted_in_window {
            acc.creditable_jobs += 1;
        }
        acc.replay_ms.extend_from_slice(&observation.replay_durations_ms);
    }

    /// Reports, sorted by class id so the artifact is deterministic.
    pub fn report(&self) -> Vec<PalwShadowClassReportV1> {
        let mut out: Vec<PalwShadowClassReportV1> = self
            .classes
            .iter()
            .map(|(class, acc)| PalwShadowClassReportV1 {
                runtime_class_id: *class,
                jobs: acc.jobs,
                creditable_jobs: acc.creditable_jobs,
                jobs_with_on_time_match: acc.jobs_with_on_time_match,
                refuted_in_window_jobs: acc.refuted_in_window_jobs,
                refuted_after_close_jobs: acc.refuted_after_close_jobs,
                duties: acc.duties,
                on_time_matches: acc.on_time_matches,
                late_matches: acc.late_matches,
                mismatches: acc.mismatches,
                no_shows: acc.no_shows,
                replay_ms: percentiles(acc.replay_ms.clone()),
                attest_latency_daa: percentiles(acc.attest_latency_daa.clone()),
                refutation_latency_daa: percentiles(acc.refutation_latency_daa.clone()),
            })
            .collect();
        out.sort_by(|a, b| a.runtime_class_id.as_byte_slice().cmp(b.runtime_class_id.as_byte_slice()));
        out
    }
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;
    use crate::vlt::VERIFIER_SORTITION_KEY;

    fn h64(seed: u8) -> Hash64 {
        Hash64::from_bytes([seed; 64])
    }

    fn candidates() -> Vec<PalwPanelCandidateV1> {
        (1u8..=8)
            .map(|i| PalwPanelCandidateV1 { validator_id: h64(i), runtime_class_id: h64(0xC1), bonded: true, frozen: false })
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // §2 assignment
    // -----------------------------------------------------------------------------------------

    #[test]
    fn panel_is_deterministic_and_order_invariant() {
        let cands = candidates();
        let panel = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 3);
        assert_eq!(panel.len(), 3);
        let mut shuffled = cands.clone();
        shuffled.reverse();
        shuffled.swap(0, 3);
        assert_eq!(panel, select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &shuffled, 3));
        assert_eq!(panel, select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 3));
    }

    #[test]
    fn eligibility_is_the_adr_rule_not_the_callers_discipline() {
        let mut cands = candidates();
        // The executor itself, an unbonded validator, a frozen one, and a cross-class one —
        // each ineligible for its own reason.
        cands[0].validator_id = h64(0x02); // executor
        cands[1].bonded = false;
        cands[2].frozen = true;
        cands[3].runtime_class_id = h64(0xC2);
        let panel = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 8);
        assert_eq!(panel.len(), 4, "exactly the four eligible remain");
        for excluded in [h64(0x02), cands[1].validator_id, cands[2].validator_id, cands[3].validator_id] {
            assert!(!panel.contains(&excluded));
        }
    }

    #[test]
    fn a_small_class_yields_a_small_panel_and_q_zero_yields_none() {
        let cands = candidates()[..1].to_vec();
        assert_eq!(select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 5).len(), 1);
        assert!(select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 0).is_empty());
    }

    #[test]
    fn every_ticket_input_moves_the_panel() {
        let cands = candidates();
        let base = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 8);
        // Full-panel ORDER is the sensitive object (q = all eligible), so any input change
        // must reorder with overwhelming probability; these are fixed vectors, not chances.
        let other_root = select_replay_panel_v1(&h64(0x11), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 8);
        let other_executor = select_replay_panel_v1(&h64(0x01), &h64(0x12), &h64(0x03), &h64(0xC1), &cands, 8);
        let other_anchor = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x13), &h64(0xC1), &cands, 8);
        assert_ne!(base, other_root, "commitment root does not reach the ticket");
        assert_ne!(base, other_executor, "executor id does not reach the ticket");
        assert_ne!(base, other_anchor, "anchor does not reach the ticket");
    }

    #[test]
    fn the_twin_lotteries_share_a_shape_but_never_a_draw() {
        // Same job, same executor, same beacon, same candidate set — the VLT sortition and the
        // PALW assignment must still order differently, or one lottery would predict the other.
        let cands = candidates();
        let vlt_pairs: Vec<(Hash64, Hash64)> = cands.iter().map(|c| (c.validator_id, c.runtime_class_id)).collect();
        let vlt_panel = crate::vlt::select_verifiers(h64(0x01), h64(0x02), h64(0x03), h64(0xC1), &vlt_pairs, 8);
        let palw_panel = select_replay_panel_v1(&h64(0x01), &h64(0x02), &h64(0x03), &h64(0xC1), &cands, 8);
        assert_eq!(vlt_panel.len(), palw_panel.len());
        assert_ne!(vlt_panel, palw_panel, "domain separation failed: the two lotteries drew identically");
    }

    #[test]
    fn schedule_domains_are_unique_across_all_palw_modules() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend_from_slice(PALW_SCHEDULE_ALL_DOMAINS);
        all.extend_from_slice(PALW_LEGS_ALL_DOMAINS);
        all.extend_from_slice(PALW_V2_ALL_DOMAINS);
        all.extend_from_slice(PALW_S_ALL_DOMAINS);
        all.extend_from_slice(PALW_REFERENCE_ALL_DOMAINS);
        all.push(VERIFIER_SORTITION_KEY);
        let before = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), before, "a domain string is shared across families — a preimage bridge");
        // And the blake2b key-length cap the keyed construction depends on.
        assert!(PALW_SCHEDULE_DOMAIN_ASSIGNMENT_TICKET.len() <= 64);
    }

    // -----------------------------------------------------------------------------------------
    // §3 windows
    // -----------------------------------------------------------------------------------------

    #[test]
    fn the_stage1_defaults_validate_against_the_real_network_parameters() {
        let deci = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        deci.validate(&BlockrateParams::new_deci_bps()).expect("deci-bps defaults hold");
        let two_minute = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        two_minute.validate(&BlockrateParams::new_two_minute_bps()).expect("120 s defaults hold");
        // The ladder margin the ADR states: 24 h − (1 h + 20 · 1 h) = 3 h.
        assert_eq!(deci.ladder_margin(), 1_080, "3 h at 10 s blocks");
        assert_eq!(two_minute.ladder_margin(), 90, "3 h at 120 s blocks");
    }

    /// The regression pin for the bug the ADR review caught: the first draft's 48 h challenge
    /// window exceeds BOTH pruning horizons (30 h at 0.1 bps; 38.1 h on the 120 s net, where
    /// the prunality lower bound of 1 144 blocks binds). This test is why it cannot recur.
    #[test]
    fn the_first_drafts_48h_window_fails_on_both_networks() {
        let mut deci = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        deci.w_challenge = 17_280; // 48 h at 10 s blocks
        assert_eq!(
            deci.validate(&BlockrateParams::new_deci_bps()),
            Err(PalwScheduleError::WindowInequalityViolated { rule: "w_challenge + prosecution_slack < pruning_depth" })
        );
        let mut two_minute = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        two_minute.w_challenge = 1_440; // 48 h at 120 s blocks
        assert_eq!(
            two_minute.validate(&BlockrateParams::new_two_minute_bps()),
            Err(PalwScheduleError::WindowInequalityViolated { rule: "w_challenge + prosecution_slack < pruning_depth" })
        );
    }

    #[test]
    fn every_window_inequality_rejects_on_its_own() {
        let blockrate = BlockrateParams::new_deci_bps();
        let good = PalwScheduleParamsV1::stage1_defaults_deci_bps();

        let mut wrong_version = good;
        wrong_version.version = 2;
        assert!(matches!(wrong_version.validate(&blockrate), Err(PalwScheduleError::UnsupportedVersion { .. })));

        let mut no_panel = good;
        no_panel.q = 0;
        assert!(matches!(no_panel.validate(&blockrate), Err(PalwScheduleError::ParamsNotCanonical { .. })));

        let mut zero_window = good;
        zero_window.w_answer = 0;
        assert!(matches!(zero_window.validate(&blockrate), Err(PalwScheduleError::ParamsNotCanonical { .. })));

        let mut ladder_broken = good;
        ladder_broken.w_round = 1_000; // 20 rounds no longer fit inside 24 h
        assert_eq!(
            ladder_broken.validate(&blockrate),
            Err(PalwScheduleError::WindowInequalityViolated { rule: "w_challenge ≥ w_replay + LADDER_ROUNDS · w_round" })
        );

        let mut duty_after_close = good;
        duty_after_close.delta_bind = 9_000;
        assert_eq!(
            duty_after_close.validate(&blockrate),
            Err(PalwScheduleError::WindowInequalityViolated { rule: "delta_bind + w_replay < w_challenge" })
        );

        let mut inside_finality = good;
        inside_finality.w_challenge = 4_000; // below the 4 320 finality depth
        inside_finality.w_round = 150; // keep the ladder satisfiable so finality is what trips
        assert_eq!(
            inside_finality.validate(&blockrate),
            Err(PalwScheduleError::WindowInequalityViolated { rule: "finality_depth < w_challenge" })
        );
    }

    #[test]
    fn job_schedule_matches_the_adr_table() {
        let params = PalwScheduleParamsV1::stage1_defaults_deci_bps();
        let schedule = job_schedule_v1(&params, 1_000_000).unwrap();
        assert_eq!(schedule.anchor_daa, 1_000_120);
        assert_eq!(schedule.replay_deadline_daa, 1_000_480);
        assert_eq!(schedule.challenge_close_daa, 1_008_640);
        assert_eq!(answer_deadline_v1(&params, 1_000_500).unwrap(), 1_000_860);
        assert!(matches!(job_schedule_v1(&params, u64::MAX - 100), Err(PalwScheduleError::DaaOverflow { .. })));
    }

    #[test]
    fn the_replay_fit_check_is_kappa_p99_against_the_window() {
        let params = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        // w_replay = 30 blocks × 120 000 ms = 3 600 000 ms; κ = 3 ⇒ p99 must be ≤ 20 min.
        assert!(replay_p99_fits_v1(20 * 60 * 1_000, &params, 120_000));
        assert!(!replay_p99_fits_v1(20 * 60 * 1_000 + 1, &params, 120_000));
    }

    #[test]
    fn credited_ceiling_rederives_from_the_measured_fleet_numbers() {
        let params = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        // The slowest measured host (D, 2026-08-16 fleet bench): load p50 ≈ 4.3 s cold, and
        // 165.1 ms/decode-token at D=512. Format ceiling is the v2 wire's 4 095.
        let measured =
            PalwReplayCostMeasurementV1 { fixed_overhead_ms: 4_300, ms_per_decode_token: 165, format_ceiling_tokens: 4_095 };
        // Budget = w_replay(30) × 120 000 / κ(3) = 1 200 000 ms; minus 4 300 fixed = 1 195 700;
        // / 165 = 7 246 tokens — well past the format ceiling, so the class is FORMAT-bound.
        let ceiling = credited_ceiling_tokens_v1(&measured, &params, 120_000);
        assert_eq!(ceiling, 4_095, "the pinned Q4 fleet is format-bound, not window-bound — the ~10x-conservative finding");
        // The fit check agrees: even the format ceiling's replay fits the window.
        assert!(replay_p99_fits_v1(4_300 + 4_095 * 165, &params, 120_000));

        // A hypothetical 10× slower class IS window-bound below the format ceiling.
        let slow = PalwReplayCostMeasurementV1 { fixed_overhead_ms: 20_000, ms_per_decode_token: 1_650, format_ceiling_tokens: 4_095 };
        let slow_ceiling = credited_ceiling_tokens_v1(&slow, &params, 120_000);
        assert!(slow_ceiling > 0 && slow_ceiling < 4_095, "a slow class is window-capped: {slow_ceiling}");
        // Its own ceiling replay must (barely) fit — the derivation's defining property.
        assert!(replay_p99_fits_v1(20_000 + slow_ceiling as u64 * 1_650, &params, 120_000));
        assert!(!replay_p99_fits_v1(20_000 + (slow_ceiling as u64 + 1) * 1_650, &params, 120_000));

        // A class too slow to fit even a zero-decode job credits nothing.
        let hopeless =
            PalwReplayCostMeasurementV1 { fixed_overhead_ms: 2_000_000, ms_per_decode_token: 1, format_ceiling_tokens: 4_095 };
        assert_eq!(credited_ceiling_tokens_v1(&hopeless, &params, 120_000), 0);
    }

    /// The B15 live facts (`docs/palw-economic-parameters-2026-08-16.md`): the 120 s subsidy
    /// rate-preserved from the 10 BPS genesis value, the 20 000 MSK bond, unbonding 10 083.
    fn b15_facts() -> PalwEconomicFactsV1 {
        PalwEconomicFactsV1 {
            block_subsidy_sompi: 370_468_345 * 1_200, // 444 562 014 000 sompi = 4 445.62 MSK
            s_eff_sompi: 20_000 * 100_000_000,        // the 20 000 MSK bond
            unbonding_period_blocks: 10_083,
        }
    }

    /// The live panel shape the payout unit is measured at: `q = 2` from the two-minute
    /// defaults, and `ρ_v = 1 000‰` — a full `base(C)` per attester, which is the registry's
    /// own fixture value and the most expensive admissible panel.
    const B15_Q: u16 = 2;
    const B15_RHO_PERMILLE: u32 = 1_000;

    fn b15_holds(remedy: &PalwLeverageRemedyV1) -> bool {
        let facts = b15_facts();
        let payout = one_job_payout_sompi_v1(remedy, B15_RHO_PERMILLE, B15_Q, facts.block_subsidy_sompi);
        max_leverage_holds_v1(remedy, &facts, payout)
    }

    #[test]
    fn the_live_panel_costs_three_bases_per_job_so_neither_adr_remedy_survives_as_written() {
        let facts = b15_facts();
        assert_eq!(PalwScheduleParamsV1::stage1_defaults_two_minute_bps().q, B15_Q, "the pinned panel size moved");

        // The unit, first. One job pays base(C) + q · ρ_v · base(C) = 3 × base(C) here, and
        // for a long time the §4e check derived base(C) alone — so it validated the bond
        // against a THIRD of what the crediting walk's ceiling drains.
        let full = PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 1_000 };
        let base_only = PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 1_000 };
        assert_eq!(
            one_job_payout_sompi_v1(&full, B15_RHO_PERMILLE, B15_Q, facts.block_subsidy_sompi),
            one_job_payout_sompi_v1(&base_only, 0, 0, facts.block_subsidy_sompi) * 3,
            "one job costs 1 + q · ρ_v = 3 bases at the live panel"
        );

        // The pre-amendment live shape — full subsidy, credit every block — is the B15
        // finding. Even counting only ONE job per block (the amendment's 11 655× uses the
        // physical multi-job-per-block rate), the violation is three orders of magnitude.
        assert!(!b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 1, base_subsidy_permille: 1_000 }));
        let g_max = (facts.block_subsidy_sompi as u128) * (facts.unbonding_period_blocks as u128 + 1);
        assert!(g_max * 2 / (facts.s_eff_sompi as u128) > 4_000, "one-job-per-block leverage is already >4 000× over the bond");

        // ADR remedy 1 — the per-validator rate cap at FULL subsidy — is not merely narrowed
        // by the corrected unit, it is GONE at this panel. `jobs ≥ 1` for every interval, and
        // one full-subsidy job already pays 3 × 4 445.62 = 13 336.86 MSK, so λ · G_max exceeds
        // a 20 000 MSK bond before the rate lever gets a say. Widening the interval cannot
        // rescue it; only a smaller base(C), a smaller ρ_v, or a smaller q can. Pinned across
        // a range that includes intervals far wider than the whole unbonding period.
        for interval in [1u64, 5_042, 10_083, 10_084, 40_000] {
            assert!(
                !b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: interval, base_subsidy_permille: 1_000 }),
                "full subsidy must be inadmissible at every rate once attester shares count (interval {interval})"
            );
        }

        // ADR remedy 2 — fractional base(C) — survives, but not at the pair the amendment
        // printed. (10 blocks, 0.2 %) held only under the base-only unit; at 3 × base the
        // smallest admissible interval for even 0.1 % is 14 blocks. 13 fails. Pinned in both
        // directions so the boundary cannot drift toward the attacker unnoticed.
        assert!(!b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 2 }));
        assert!(b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 14, base_subsidy_permille: 1 }));
        assert!(!b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 13, base_subsidy_permille: 1 }));

        // A cheaper panel restores the amendment's own pair, which is the honest statement of
        // what changed: the remedy space is (interval, base‰, q, ρ_v), not the two levers the
        // ADR described while the shares were invisible to the check.
        let printed = PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 2 };
        let payout_no_shares = one_job_payout_sompi_v1(&printed, 0, B15_Q, facts.block_subsidy_sompi);
        assert!(max_leverage_holds_v1(&printed, &facts, payout_no_shares), "the printed pair holds iff shares cost nothing");
    }

    #[test]
    fn a_degenerate_remedy_encoding_never_holds_and_zero_base_mints_nothing() {
        let facts = b15_facts();
        // A zero interval is not a rate; a fraction above the whole subsidy is not a base.
        assert!(!b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 0, base_subsidy_permille: 2 }));
        assert!(!b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 10, base_subsidy_permille: 1_001 }));
        // A zero fraction mints nothing, so the inequality holds trivially — the same
        // meaning as the §12 zero-credit stage, reached through the arithmetic itself.
        assert!(b15_holds(&PalwLeverageRemedyV1 { min_credit_interval_daa: 1, base_subsidy_permille: 0 }));

        // The unit is now an argument, so a caller could pass one smaller than the remedy's
        // own base(C) — which is exactly the defect this parameter replaced. That is refused
        // rather than believed: an under-stated unit closes the gate instead of widening it.
        let fractional = PalwLeverageRemedyV1 { min_credit_interval_daa: 14, base_subsidy_permille: 1 };
        let honest = one_job_payout_sompi_v1(&fractional, B15_RHO_PERMILLE, B15_Q, facts.block_subsidy_sompi);
        assert!(max_leverage_holds_v1(&fractional, &facts, honest));
        assert!(
            !max_leverage_holds_v1(&fractional, &facts, honest / 3 - 1),
            "a unit below base(C) must fail closed, not license the mint it under-measures"
        );
        // At the boundary — exactly base(C) — the gate still opens; the refusal is for units
        // that are provably too small, not for a class whose panel genuinely costs nothing.
        assert!(max_leverage_holds_v1(&fractional, &facts, honest / 3));
    }

    // -----------------------------------------------------------------------------------------
    // §6 shadow ledger
    // -----------------------------------------------------------------------------------------

    fn schedule() -> PalwJobScheduleV1 {
        job_schedule_v1(&PalwScheduleParamsV1::stage1_defaults_deci_bps(), 10_000).unwrap()
    }

    #[test]
    fn duty_classification_is_the_pure_function_it_claims_to_be() {
        let s = schedule(); // anchor 10 120, replay deadline 10 480
        let on_time = PalwDutyObservationV1::Attested { root_matched: true, attest_daa: 10_480 };
        let late = PalwDutyObservationV1::Attested { root_matched: true, attest_daa: 10_481 };
        let mismatch_late = PalwDutyObservationV1::Attested { root_matched: false, attest_daa: 99_999 };
        assert_eq!(classify_duty_v1(&on_time, &s), PalwDutyClassV1::OnTimeMatch);
        assert_eq!(classify_duty_v1(&late, &s), PalwDutyClassV1::LateMatch);
        assert_eq!(classify_duty_v1(&mismatch_late, &s), PalwDutyClassV1::Mismatch, "mismatch is timing-independent");
        assert_eq!(classify_duty_v1(&PalwDutyObservationV1::Silent, &s), PalwDutyClassV1::NoShow);
    }

    #[test]
    fn the_credit_gate_shadow_counts_late_refutations_as_credited_and_refuted() {
        let mut ledger = PalwShadowLedgerV1::new();
        let s = schedule(); // challenge close 18 640
        let on_time = PalwDutyObservationV1::Attested { root_matched: true, attest_daa: 10_200 };

        // Creditable: one on-time match, no refutation.
        ledger.observe_job(
            h64(0xC1),
            &PalwShadowJobObservationV1 {
                schedule: s,
                duties: vec![on_time, PalwDutyObservationV1::Silent],
                refutation_included_daa: None,
                replay_durations_ms: vec![600_000],
            },
        );
        // Refuted in window: not creditable.
        ledger.observe_job(
            h64(0xC1),
            &PalwShadowJobObservationV1 {
                schedule: s,
                duties: vec![on_time],
                refutation_included_daa: Some(18_640),
                replay_durations_ms: vec![],
            },
        );
        // Refuted AFTER close: creditable AND counted as refuted-after-close — the P3 case the
        // report exists to surface.
        ledger.observe_job(
            h64(0xC1),
            &PalwShadowJobObservationV1 {
                schedule: s,
                duties: vec![on_time],
                refutation_included_daa: Some(18_641),
                replay_durations_ms: vec![],
            },
        );
        // No on-time attestation at all: never creditable.
        ledger.observe_job(
            h64(0xC1),
            &PalwShadowJobObservationV1 {
                schedule: s,
                duties: vec![PalwDutyObservationV1::Silent, PalwDutyObservationV1::Silent],
                refutation_included_daa: None,
                replay_durations_ms: vec![],
            },
        );

        let report = ledger.report();
        assert_eq!(report.len(), 1);
        let r = &report[0];
        assert_eq!((r.jobs, r.creditable_jobs, r.jobs_with_on_time_match), (4, 2, 3));
        assert_eq!((r.refuted_in_window_jobs, r.refuted_after_close_jobs), (1, 1));
        assert_eq!((r.duties, r.on_time_matches, r.no_shows), (6, 3, 3));
        assert_eq!(r.replay_ms.samples, 1);
        assert_eq!(r.attest_latency_daa.p50, Some(80), "10 200 − anchor 10 120");
        assert_eq!(r.refutation_latency_daa.samples, 2);
    }

    #[test]
    fn classes_do_not_mix_and_the_report_orders_deterministically() {
        let mut ledger = PalwShadowLedgerV1::new();
        let s = schedule();
        let observation = PalwShadowJobObservationV1 {
            schedule: s,
            duties: vec![PalwDutyObservationV1::Attested { root_matched: true, attest_daa: 10_200 }],
            refutation_included_daa: None,
            replay_durations_ms: vec![1],
        };
        ledger.observe_job(h64(0xC2), &observation);
        ledger.observe_job(h64(0xC1), &observation);
        ledger.observe_job(h64(0xC2), &observation);
        let report = ledger.report();
        assert_eq!(report.len(), 2);
        assert_eq!(report[0].runtime_class_id, h64(0xC1), "sorted by class id");
        assert_eq!(report[0].jobs, 1);
        assert_eq!(report[1].jobs, 2);
    }

    #[test]
    fn nearest_rank_percentiles_are_the_documented_convention() {
        let samples: Vec<u64> = (1..=10).map(|i| i * 10).collect();
        assert_eq!(nearest_rank_percentile(&samples, 50, 100), Some(50), "rank ⌈5⌉");
        assert_eq!(nearest_rank_percentile(&samples, 95, 100), Some(100), "rank ⌈9.5⌉ = 10");
        assert_eq!(nearest_rank_percentile(&samples, 99, 100), Some(100));
        assert_eq!(nearest_rank_percentile(&samples, 1, 100), Some(10), "rank ⌈0.1⌉ clamps to 1");
        assert_eq!(nearest_rank_percentile(&[], 50, 100), None);
        assert_eq!(nearest_rank_percentile(&samples, 0, 100), None);
        assert_eq!(nearest_rank_percentile(&samples, 101, 100), None);
        let p = percentiles(vec![30, 10, 20]);
        assert_eq!((p.samples, p.p50, p.max), (3, Some(20), Some(30)), "sorts before ranking");
    }
}
